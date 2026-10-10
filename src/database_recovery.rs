//! Verified recovery snapshots and isolated upgrades. Never reads business file contents.
use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::backup_service::BackupService;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(crate) fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub(crate) fn version(db: &Connection) -> Result<Option<i64>> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='schema_version')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let versions = db
        .prepare("SELECT version FROM schema_version")?
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if versions.len() != 1 || !(1..=crate::database::CURRENT_SCHEMA_VERSION).contains(&versions[0])
    {
        return Err(format!("Unsupported database schema {versions:?}; use the matching application. No upgrade was performed.").into());
    }
    Ok(Some(versions[0]))
}

pub(crate) fn integrity(db: &Connection) -> Result<()> {
    let rows = db
        .prepare("PRAGMA integrity_check")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows != ["ok"] {
        return Err(format!("SQLite integrity_check failed: {rows:?}").into());
    }
    Ok(())
}

// Hash every schema definition and every typed row, preserving duplicate row counts.
// Sorting row hashes makes comparison independent of physical SQLite page layout/WAL.
fn table_rows(db: &Connection, table: &str) -> Result<Vec<Vec<u8>>> {
    let mut statement = db.prepare(&format!("SELECT * FROM {}", quote(table)))?;
    let columns = statement.column_count();
    let mut rows = statement.query([])?;
    let mut hashes = Vec::new();
    while let Some(row) = rows.next()? {
        let mut hash = Sha256::new();
        for column in 0..columns {
            use rusqlite::types::ValueRef;
            match row.get_ref(column)? {
                ValueRef::Null => hash.update([0]),
                ValueRef::Integer(v) => {
                    hash.update([1]);
                    hash.update(v.to_le_bytes());
                }
                ValueRef::Real(v) => {
                    hash.update([2]);
                    hash.update(v.to_bits().to_le_bytes());
                }
                ValueRef::Text(v) | ValueRef::Blob(v) => {
                    hash.update([if matches!(row.get_ref(column)?, ValueRef::Text(_)) {
                        3
                    } else {
                        4
                    }]);
                    hash.update((v.len() as u64).to_le_bytes());
                    hash.update(v);
                }
            }
        }
        hashes.push(hash.finalize().to_vec());
    }
    hashes.sort();
    Ok(hashes)
}

fn definitions(db: &Connection) -> Result<Vec<(String, String, String)>> {
    Ok(db
        .prepare(
            "SELECT type,name,sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type,name",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub(crate) fn fingerprint(db: &Connection) -> Result<String> {
    let mut hash = Sha256::new();
    for (kind, name, sql) in definitions(db)? {
        for value in [&kind, &name, &sql] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        if kind == "table" {
            let rows = table_rows(db, &name)?;
            hash.update((rows.len() as u64).to_le_bytes());
            for row in rows {
                hash.update(row);
            }
        }
    }
    for pragma in ["user_version", "application_id"] {
        let value: i64 = db.query_row(&format!("PRAGMA {pragma}"), [], |r| r.get(0))?;
        hash.update(value.to_le_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn file_fingerprint(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(format!("{:x}", Sha256::digest(std::fs::read(path)?))))
}

/// Holds the live writer reservation throughout backup, verification and staging.
/// No live schema changes occur until the staged chain is completely verified.
pub fn initialise(path: &Path) -> Result<()> {
    let mut live = crate::database::open(path)?;
    let _dispatch = crate::timesheet_delivery::production_lock(&live)?;
    // Preserve existing logical/orphaned legacy references during whole-schema transfer.
    // Exact typed-row verification prevents accidental omission; normal connections are unaffected.
    live.pragma_update(None, "foreign_keys", false)?;
    let tx = live.transaction_with_behavior(TransactionBehavior::Immediate)?;
    match version(&tx)? {
        Some(crate::database::CURRENT_SCHEMA_VERSION) => {
            tx.commit()?;
            return Ok(());
        }
        None => {
            let count: i64 = tx.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )?;
            if count != 0 {
                return Err("Unrecognised database; startup refused without changing it.".into());
            }
            // Fresh empty databases have no original payroll data to back up.
            tx.commit()?;
            crate::database::create_schema(&live)?;
            return Ok(());
        }
        Some(_) => {}
    }
    let parent = path.parent().ok_or("Database path has no parent")?;
    let recovery = BackupService::create_verified(path, &parent.join("config.toml"), &parent.join("backups"))
        .map_err(|e| format!("Upgrade refused before changing the database: {e}. Free backup storage/check permissions and retry; never delete the original database."))?;
    let upgrade = (|| -> Result<()> {
        let staged = tempfile::tempdir_in(parent)?;
        let staged_path = staged.path().join("upgrade.sqlite");
        std::fs::copy(recovery.join("database.sqlite"), &staged_path)?;
        let migrated = crate::database::open(&staged_path)?;
        crate::database::create_schema(&migrated)?;
        integrity(&migrated)?;
        // The existing writer reservation excludes concurrent edits during installation.
        install(&tx, &migrated)?;
        tx.commit()?;
        Ok(())
    })();
    upgrade.map_err(|e| format!("Isolated upgrade/install failed: {e}. Uncommitted installation changes are rolled back; verified original recovery is {}. Close all instances, retain this recovery copy, and seek help before retrying or sending payroll.",recovery.display()))?;
    println!("Database upgraded to schema {}. Verified database/config recovery: {}. External business files are not copied; see its recovery manifest.",crate::database::CURRENT_SCHEMA_VERSION,recovery.display());
    Ok(())
}

/// Recreate the isolated database inside one live SQLite transaction. The backup
/// API cannot install into a connection holding a writer reservation; SQL transfer
/// keeps other writers excluded and rolls back schema/data together on failure.
pub(crate) fn install(live: &Connection, staged: &Connection) -> Result<()> {
    let current = definitions(live)?;
    for kind in ["trigger", "view", "index", "table"] {
        for (ty, name, _) in &current {
            if ty == kind && !name.starts_with("sqlite_") {
                live.execute_batch(&format!("DROP {} {}", kind.to_uppercase(), quote(name)))?;
            }
        }
    }
    let source = definitions(staged)?;
    for (kind, name, sql) in &source {
        if kind != "table" || name.starts_with("sqlite_") {
            continue;
        }
        live.execute_batch(sql)?;
        let mut select = staged.prepare(&format!("SELECT * FROM {}", quote(name)))?;
        let count = select.column_count();
        let placeholders = vec!["?"; count].join(",");
        let mut insert = live.prepare(&format!(
            "INSERT INTO {} VALUES ({placeholders})",
            quote(name)
        ))?;
        let mut rows = select.query([])?;
        while let Some(row) = rows.next()? {
            let values = (0..count)
                .map(|n| row.get::<_, rusqlite::types::Value>(n))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            insert.execute(rusqlite::params_from_iter(values))?;
        }
    }
    // AUTOINCREMENT high-water marks may exceed the highest surviving row ID.
    if source.iter().any(|(_, name, _)| name == "sqlite_sequence") {
        live.execute("DELETE FROM sqlite_sequence", [])?;
        let mut stmt = staged.prepare("SELECT name,seq FROM sqlite_sequence")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (name, seq) = row?;
            live.execute(
                "INSERT INTO sqlite_sequence VALUES (?1,?2)",
                rusqlite::params![name, seq],
            )?;
        }
    }
    for (kind, _, sql) in &source {
        if kind != "table" {
            live.execute_batch(sql)?;
        }
    }
    for pragma in ["user_version", "application_id"] {
        let value: i64 = staged.query_row(&format!("PRAGMA {pragma}"), [], |r| r.get(0))?;
        live.execute_batch(&format!("PRAGMA {pragma}={value}"))?;
    }
    integrity(live)?;
    if fingerprint(live)? != fingerprint(staged)? {
        return Err("Installed database verification failed; transaction rolled back".into());
    }
    Ok(())
}

pub(crate) fn unresolved(db: &Connection) -> Result<bool> {
    for (table, predicate) in [
        (
            "timesheet_delivery_attempts",
            "outcome='uncertain' AND resolved_at IS NULL",
        ),
        (
            "payroll_timesheet_email_status",
            "sent_at LIKE 'indeterminate:%'",
        ),
        (
            "imported_payroll_documents",
            "sent_at LIKE 'indeterminate:%'",
        ),
    ] {
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)",
            [table],
            |r| r.get(0),
        )?;
        if exists
            && db.query_row(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM {} WHERE {predicate})",
                    quote(table)
                ),
                [],
                |r| r.get::<_, bool>(0),
            )?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn losses(live: &Connection, staged: &Connection) -> Result<Vec<(String, usize)>> {
    let target = definitions(staged)?
        .into_iter()
        .filter(|(kind, _, _)| kind == "table")
        .map(|(_, name, _)| (name, ()))
        .collect::<BTreeMap<_, _>>();
    let mut lost = Vec::new();
    for (kind, name, _) in definitions(live)? {
        if kind != "table" || name.starts_with("sqlite_") {
            continue;
        }
        let mut available = BTreeMap::<Vec<u8>, usize>::new();
        if target.contains_key(&name) {
            for row in table_rows(staged, &name)? {
                *available.entry(row).or_default() += 1;
            }
        }
        let mut count = 0;
        for row in table_rows(live, &name)? {
            match available.get_mut(&row) {
                Some(n) if *n > 0 => *n -= 1,
                _ => count += 1,
            }
        }
        if count > 0 {
            lost.push((name, count));
        }
    }
    Ok(lost)
}

pub(crate) fn staged_backup(backup: &Path) -> Result<(tempfile::TempDir, Connection)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("restore.sqlite");
    let source = Connection::open_with_flags(
        backup.join("database.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    source.backup(rusqlite::DatabaseName::Main, &path, None)?;
    let db = crate::database::open(path)?;
    crate::database::create_schema(&db)?;
    integrity(&db)?;
    Ok((dir, db))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup_service::{RestoreAuthorisation, RestorePlan};
    use crate::payroll_snapshot_service::{publish_candidate, CandidatePublication};
    use crate::payroll_worked_item_repository::PayrollWorkedItemRepository;

    fn fixture() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::database::open(dir.path().join("database.sqlite")).unwrap();
        crate::database::create_schema(&db).unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "[folders]\npdf_output='/never-access-business-files'\n",
        )
        .unwrap();
        (dir, db)
    }
    fn legacy(db: &Connection, version: i64) {
        crate::database::tests::remove_schema_36_fixture(db);
        if version == 34 {
            db.execute_batch("DROP TABLE payslip_revisions; ALTER TABLE imported_payroll_documents DROP COLUMN superseded_by;").unwrap();
        }
        db.execute("UPDATE schema_version SET version=?1", [version])
            .unwrap();
    }
    fn backup(dir: &Path) -> std::path::PathBuf {
        BackupService::create_verified(
            &dir.join("database.sqlite"),
            &dir.join("config.toml"),
            &dir.join("backups"),
        )
        .unwrap()
    }
    fn review(dir: &Path, backup: &Path) -> RestorePlan {
        BackupService::preview_restore(
            backup,
            &dir.join("database.sqlite"),
            &dir.join("config.toml"),
            &dir.join("backups"),
        )
        .unwrap()
    }
    fn restore(
        dir: &Path,
        plan: RestorePlan,
        acknowledged: bool,
        reason: &str,
    ) -> std::result::Result<
        crate::backup_service::RestoreResult,
        crate::backup_service::RestoreError,
    > {
        BackupService::restore(
            &plan.backup.clone(),
            &dir.join("database.sqlite"),
            &dir.join("config.toml"),
            &dir.join("backups"),
            &RestoreAuthorisation {
                plan,
                reason: reason.into(),
                acknowledged,
            },
        )
    }
    fn candidate(dir: &Path, db: &Connection) -> (crate::timesheet_delivery::Intent, Vec<u8>) {
        db.execute(
            "INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Synthetic','Test')",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO payroll_timesheets(id,personal_assistant_id,payroll_year,cycle_number,created_at,updated_at) VALUES(1,1,'2026/27',1,'fixture','fixture')",[]).unwrap();
        let repo = PayrollWorkedItemRepository::new(
            crate::database::open(dir.join("database.sqlite")).unwrap(),
        );
        publish_candidate(
            &repo,
            CandidatePublication {
                payroll_timesheet_id: 1,
                items: &[],
                final_pdf_path: &dir.join("test.pdf"),
                generated_at: "fixture",
                previous_cycle_minutes: 0,
                week_ids: &[0; 4],
                week_totals_minutes: &[0; 4],
            },
            |path| {
                std::fs::write(path, b"%PDF-1.4 synthetic")?;
                Ok(())
            },
        )
        .unwrap();
        let intent = crate::timesheet_delivery::capture(db, 1).unwrap();
        let bytes = crate::timesheet_delivery::bytes(db, &intent).unwrap();
        (intent, bytes)
    }
    fn accepted(db: &Connection, intent: &crate::timesheet_delivery::Intent, bytes: &[u8]) {
        crate::timesheet_delivery::execute(
            db,
            intent,
            "Payroll; employer CC",
            "<recovery-test@test.local>",
            bytes,
            |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
    }

    #[test]
    fn backup_failure_precedes_any_live_schema_change() {
        let (dir, db) = fixture();
        legacy(&db, 35);
        let before = fingerprint(&db).unwrap();
        let raw = std::fs::read(dir.path().join("database.sqlite")).unwrap();
        std::fs::write(dir.path().join("backups"), "blocks backup creation").unwrap();
        let error = initialise(&dir.path().join("database.sqlite"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Upgrade refused before changing"));
        assert_eq!(version(&db).unwrap(), Some(35));
        assert_eq!(fingerprint(&db).unwrap(), before);
        assert_eq!(
            std::fs::read(dir.path().join("database.sqlite")).unwrap(),
            raw
        );
    }
    #[test]
    fn configuration_copy_failure_aborts_upgrade_and_keeps_original() {
        let (dir, db) = fixture();
        legacy(&db, 35);
        std::fs::remove_file(dir.path().join("config.toml")).unwrap();
        std::fs::create_dir(dir.path().join("config.toml")).unwrap();
        let before = fingerprint(&db).unwrap();
        assert!(initialise(&dir.path().join("database.sqlite")).is_err());
        assert_eq!(fingerprint(&db).unwrap(), before);
        assert_eq!(
            std::fs::read_dir(dir.path().join("backups"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn verified_backup_includes_wal_and_does_not_checkpoint_original() {
        let (dir, db) = fixture();
        legacy(&db, 35);
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE wal_evidence(value TEXT); INSERT INTO wal_evidence VALUES('committed WAL evidence');").unwrap();
        let wal = dir.path().join("database.sqlite-wal");
        let before = std::fs::read(&wal).unwrap();
        assert!(!before.is_empty());
        let copy = backup(dir.path());
        let saved = Connection::open(copy.join("database.sqlite")).unwrap();
        assert_eq!(fingerprint(&saved).unwrap(), fingerprint(&db).unwrap());
        assert_eq!(std::fs::read(wal).unwrap(), before);
        let manifest = std::fs::read_to_string(copy.join("README.txt")).unwrap();
        assert!(manifest.contains("/never-access-business-files"));
        assert!(manifest.contains("NOT copied"));
        initialise(&dir.path().join("database.sqlite")).unwrap();
        assert_eq!(
            db.query_row::<String, _, _>("SELECT value FROM wal_evidence", [], |r| r.get(0))
                .unwrap(),
            "committed WAL evidence"
        );
    }
    #[test]
    fn late_migration_failure_preserves_original_chain_and_verified_recovery() {
        let (dir, db) = fixture();
        legacy(&db, 34);
        db.execute_batch("CREATE TRIGGER fail36 BEFORE UPDATE ON schema_version WHEN NEW.version=36 BEGIN SELECT RAISE(ABORT,'late chain failure'); END;").unwrap();
        let before = fingerprint(&db).unwrap();
        let error = initialise(&dir.path().join("database.sqlite"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("verified original recovery"));
        assert_eq!(fingerprint(&db).unwrap(), before);
        assert_eq!(version(&db).unwrap(), Some(34));
        assert!(db.prepare("SELECT * FROM payslip_revisions").is_err());
        let saved = BackupService::discover(&dir.path().join("backups")).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(
            BackupService::validate(&saved[0].path, &dir.path().join("backups"))
                .unwrap()
                .schema_version,
            34
        );
        assert_eq!(
            fingerprint(&Connection::open(saved[0].path.join("database.sqlite")).unwrap()).unwrap(),
            before
        );
        db.execute_batch("DROP TRIGGER fail36").unwrap();
        initialise(&dir.path().join("database.sqlite")).unwrap();
        assert_eq!(
            version(&db).unwrap(),
            Some(crate::database::CURRENT_SCHEMA_VERSION)
        );
    }
    #[test]
    fn interrupted_isolated_chain_is_not_used_as_live_data_on_restart() {
        let (dir, db) = fixture();
        legacy(&db, 34);
        let copy = backup(dir.path());
        let abandoned = dir.path().join("interrupted-upgrade.sqlite");
        std::fs::copy(copy.join("database.sqlite"), &abandoned).unwrap();
        let stage = crate::database::open(&abandoned).unwrap();
        stage.execute_batch("CREATE TABLE partial_chain_fixture(value TEXT); UPDATE schema_version SET version=35;").unwrap();
        // Simulate interruption after one separately committed migration on a work copy.
        drop(stage);
        initialise(&dir.path().join("database.sqlite")).unwrap();
        assert_eq!(
            version(&db).unwrap(),
            Some(crate::database::CURRENT_SCHEMA_VERSION)
        );
        assert!(db.prepare("SELECT * FROM partial_chain_fixture").is_err());
        assert!(abandoned.is_file());
    }
    #[test]
    fn schema35_restore_is_isolated_and_original_backup_unchanged() {
        let (dir, db) = fixture();
        legacy(&db, 35);
        let selected = backup(dir.path());
        let original =
            fingerprint(&Connection::open(selected.join("database.sqlite")).unwrap()).unwrap();
        initialise(&dir.path().join("database.sqlite")).unwrap();
        db.execute(
            "INSERT INTO employers(name) VALUES('new current employer')",
            [],
        )
        .unwrap();
        let before = fingerprint(&db).unwrap();
        let plan = review(dir.path(), &selected);
        assert_eq!(plan.schema_version, 35);
        assert!(plan
            .replaced_rows
            .iter()
            .any(|(t, n)| t == "employers" && *n == 1));
        let result = restore(
            dir.path(),
            plan,
            true,
            "Verified employer rollback; separately protected business files",
        )
        .unwrap();
        assert_eq!(
            version(&db).unwrap(),
            Some(crate::database::CURRENT_SCHEMA_VERSION)
        );
        assert_eq!(
            fingerprint(&Connection::open(result.safety_backup.join("database.sqlite")).unwrap())
                .unwrap(),
            before
        );
        assert_eq!(
            fingerprint(&Connection::open(selected.join("database.sqlite")).unwrap()).unwrap(),
            original
        );
        assert_eq!(
            BackupService::validate(&result.safety_backup, &dir.path().join("backups"))
                .unwrap()
                .schema_version,
            crate::database::CURRENT_SCHEMA_VERSION
        );
    }
    #[test]
    fn rollback_needs_documented_approval_and_preserves_newer_email_evidence() {
        let (dir, db) = fixture();
        let selected = backup(dir.path());
        let (intent, bytes) = candidate(dir.path(), &db);
        accepted(&db, &intent, &bytes);
        let before = fingerprint(&db).unwrap();
        let plan = review(dir.path(), &selected);
        assert!(plan
            .replaced_rows
            .iter()
            .any(|(t, n)| t == "timesheet_delivery_attempts" && *n == 1));
        assert!(restore(dir.path(), plan.clone(), false, "reason").is_err());
        assert!(restore(dir.path(), plan.clone(), true, "  ").is_err());
        assert_eq!(fingerprint(&db).unwrap(), before);
        let result = restore(
            dir.path(),
            plan,
            true,
            "Payroll confirmed rollback; protect prior accepted delivery evidence",
        )
        .unwrap();
        let saved = crate::database::open(result.safety_backup.join("database.sqlite")).unwrap();
        assert_eq!(
            saved
                .query_row::<i64, _, _>(
                    "SELECT count(*) FROM timesheet_delivery_attempts WHERE outcome='accepted'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            saved
                .query_row::<i64, _, _>("SELECT count(*) FROM payroll_submissions", [], |r| r
                    .get(0))
                .unwrap(),
            1
        );
        assert!(
            std::fs::read_to_string(result.safety_backup.join("README.txt"))
                .unwrap()
                .contains("Payroll confirmed rollback")
        );
    }
    #[test]
    fn new_email_evidence_invalidates_stale_restore_confirmation() {
        let (dir, db) = fixture();
        let selected = backup(dir.path());
        let plan = review(dir.path(), &selected);
        let (intent, bytes) = candidate(dir.path(), &db);
        accepted(&db, &intent, &bytes);
        let before = fingerprint(&db).unwrap();
        assert!(restore(dir.path(), plan, true, "approved before email")
            .unwrap_err()
            .to_string()
            .contains("stale"));
        assert_eq!(fingerprint(&db).unwrap(), before);
    }
    #[test]
    fn uncertain_attempts_cannot_be_released_by_restore_and_survive_restart() {
        let (dir, db) = fixture();
        let selected = backup(dir.path());
        let plan = review(dir.path(), &selected);
        let (intent, bytes) = candidate(dir.path(), &db);
        crate::timesheet_delivery::claim(
            &db,
            &intent,
            "Payroll",
            "<uncertain-recovery@test.local>",
            &bytes,
        )
        .unwrap();
        let before = fingerprint(&db).unwrap();
        assert!(restore(dir.path(), plan, true, "rollback")
            .unwrap_err()
            .to_string()
            .contains("uncertain"));
        assert!(BackupService::preview_restore(
            &selected,
            &dir.path().join("database.sqlite"),
            &dir.path().join("config.toml"),
            &dir.path().join("backups")
        )
        .is_err());
        initialise(&dir.path().join("database.sqlite")).unwrap();
        assert_eq!(fingerprint(&db).unwrap(), before);
        assert!(crate::timesheet_delivery::blocked(&db, 1).unwrap());
    }
    #[test]
    fn sender_lock_excludes_restore_and_migration_across_connections() {
        let (dir, db) = fixture();
        let selected = backup(dir.path());
        let plan = review(dir.path(), &selected);
        let second = crate::database::open(dir.path().join("database.sqlite")).unwrap();
        let guard = crate::timesheet_delivery::production_lock(&second).unwrap();
        assert!(restore(dir.path(), plan.clone(), true, "rollback")
            .unwrap_err()
            .to_string()
            .contains("active"));
        assert!(initialise(&dir.path().join("database.sqlite"))
            .unwrap_err()
            .to_string()
            .contains("active"));
        drop(guard);
        restore(dir.path(), plan, true, "after sender exits").unwrap();
        assert_eq!(
            version(&db).unwrap(),
            Some(crate::database::CURRENT_SCHEMA_VERSION)
        );
    }
    #[test]
    fn writer_reservation_prevents_restore_installation_and_partial_edits() {
        let (dir, db) = fixture();
        let selected = backup(dir.path());
        let plan = review(dir.path(), &selected);
        let tx = db.unchecked_transaction().unwrap();
        tx.execute("UPDATE schema_version SET version=version", [])
            .unwrap();
        let before = fingerprint(&tx).unwrap();
        assert!(restore(dir.path(), plan, true, "rollback").is_err());
        assert_eq!(fingerprint(&tx).unwrap(), before);
        tx.rollback().unwrap();
    }
    #[test]
    fn install_failure_rolls_back_schema_data_and_high_water_marks() {
        let (_dir, mut db) = fixture();
        let before = fingerprint(&db).unwrap();
        // Stage a recognised current application schema, including its SQLite
        // AUTOINCREMENT sequence table, rather than an unrelated bare database.
        let stage = crate::database::open_in_memory().unwrap();
        crate::database::create_schema(&stage).unwrap();
        stage
            .execute_batch("CREATE TABLE bad_fixture(value); INSERT INTO bad_fixture VALUES(1);")
            .unwrap();
        db.pragma_update(None, "foreign_keys", false).unwrap();
        let tx = db.transaction().unwrap();
        install(&tx, &stage).unwrap();
        // Simulated interruption before the atomic install commits.
        drop(tx);
        assert_eq!(fingerprint(&db).unwrap(), before);
    }
    #[test]
    fn future_schema_startup_is_rejected_without_backup_or_downgrade() {
        let (dir, db) = fixture();
        db.execute("UPDATE schema_version SET version=38", [])
            .unwrap();
        let before = fingerprint(&db).unwrap();
        assert!(initialise(&dir.path().join("database.sqlite")).is_err());
        assert_eq!(fingerprint(&db).unwrap(), before);
        assert!(!dir.path().join("backups").exists());
    }
    #[test]
    fn backup_validation_failure_aborts_before_migrations() {
        let (dir, db) = fixture();
        legacy(&db, 35);
        db.pragma_update(None, "foreign_keys", false).unwrap();
        db.execute_batch("DROP TABLE employers").unwrap();
        let before = fingerprint(&db).unwrap();
        let error = initialise(&dir.path().join("database.sqlite"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Upgrade refused before changing"));
        assert!(error.contains("missing employers"));
        assert_eq!(fingerprint(&db).unwrap(), before);
        assert_eq!(version(&db).unwrap(), Some(35));
    }

    #[test]
    fn current_config_and_selected_backup_changes_invalidate_approval() {
        let (dir, db) = fixture();
        let selected = backup(dir.path());
        let plan = review(dir.path(), &selected);
        std::fs::write(dir.path().join("config.toml"), "changed current settings").unwrap();
        assert!(restore(dir.path(), plan, true, "rollback")
            .unwrap_err()
            .to_string()
            .contains("stale"));
        let plan = review(dir.path(), &selected);
        let modified = crate::database::open(selected.join("database.sqlite")).unwrap();
        modified
            .execute("INSERT INTO employers(name) VALUES('changed backup')", [])
            .unwrap();
        assert!(restore(dir.path(), plan, true, "rollback")
            .unwrap_err()
            .to_string()
            .contains("stale"));
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT count(*) FROM employers", [], |r| r.get(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn external_business_files_are_only_inventoried_and_never_restored() {
        let (dir, db) = fixture();
        let external = dir.path().join("external");
        std::fs::create_dir(&external).unwrap();
        let pdf = external.join("retained.pdf");
        let signature = external.join("signature.png");
        std::fs::write(&pdf, "original business PDF").unwrap();
        std::fs::write(&signature, "signature bytes").unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            format!("[folders]\npdf_output={:?}\n", external.to_string_lossy()),
        )
        .unwrap();
        db.execute("INSERT INTO personal_assistants(id,first_name,surname,signature) VALUES(1,'Synthetic','Test',?1)",[signature.to_string_lossy().as_ref()]).unwrap();
        let selected = backup(dir.path());
        let manifest = std::fs::read_to_string(selected.join("README.txt")).unwrap();
        assert!(manifest.contains(signature.to_str().unwrap()));
        std::fs::write(&pdf, "newer business PDF").unwrap();
        let plan = review(dir.path(), &selected);
        restore(dir.path(), plan, true, "Protect external files separately").unwrap();
        assert_eq!(std::fs::read_to_string(&pdf).unwrap(), "newer business PDF");
        assert_eq!(
            std::fs::read_to_string(&signature).unwrap(),
            "signature bytes"
        );
    }

    // Child entry point is inert in an ordinary test run. Only the parent supplies
    // temporary paths directly to Command; no global environment or production data.
    #[test]
    fn recovery_process_fixture() {
        let Some(root) = std::env::var_os("DPT_RECOVERY_TEST_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let mut db = crate::database::open(root.join("database.sqlite")).unwrap();
        let _guard = crate::timesheet_delivery::production_lock(&db).unwrap();
        let _tx = if std::env::var("DPT_RECOVERY_TEST_PHASE").unwrap() == "install" {
            db.pragma_update(None, "foreign_keys", false).unwrap();
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            tx.execute_batch("CREATE TABLE interrupted_live_install(value TEXT); INSERT INTO interrupted_live_install VALUES('uncommitted');").unwrap();
            Some(tx)
        } else {
            None
        };
        std::fs::write(root.join("child-ready"), "ready").unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    struct ChildGuard(std::process::Child);
    impl std::ops::Deref for ChildGuard {
        type Target = std::process::Child;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl std::ops::DerefMut for ChildGuard {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn child(root: &Path, phase: &str) -> ChildGuard {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "database_recovery::tests::recovery_process_fixture",
                "--nocapture",
            ])
            .env("DPT_RECOVERY_TEST_ROOT", root)
            .env("DPT_RECOVERY_TEST_PHASE", phase)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let child = ChildGuard(child);
        for _ in 0..250 {
            if root.join("child-ready").is_file() {
                return child;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("Temporary recovery subprocess did not become ready");
    }
    #[test]
    fn process_exit_releases_sender_lock_but_does_not_release_uncertainty() {
        let (dir, db) = fixture();
        let selected = backup(dir.path());
        let plan = review(dir.path(), &selected);
        let (intent, bytes) = candidate(dir.path(), &db);
        crate::timesheet_delivery::claim(
            &db,
            &intent,
            "Payroll",
            "<interrupted-sender@test.local>",
            &bytes,
        )
        .unwrap();
        let mut sender = child(dir.path(), "sender");
        assert!(initialise(&dir.path().join("database.sqlite"))
            .unwrap_err()
            .to_string()
            .contains("active"));
        sender.kill().unwrap();
        sender.wait().unwrap();
        initialise(&dir.path().join("database.sqlite")).unwrap();
        assert!(crate::timesheet_delivery::blocked(&db, 1).unwrap());
        assert!(restore(dir.path(), plan, true, "cannot bypass uncertainty")
            .unwrap_err()
            .to_string()
            .contains("uncertain"));
    }
    #[test]
    fn killed_live_install_rolls_back_and_restart_keeps_original_database() {
        let (dir, db) = fixture();
        let before = fingerprint(&db).unwrap();
        let mut installer = child(dir.path(), "install");
        installer.kill().unwrap();
        installer.wait().unwrap();
        initialise(&dir.path().join("database.sqlite")).unwrap();
        assert_eq!(fingerprint(&db).unwrap(), before);
        assert!(db
            .prepare("SELECT * FROM interrupted_live_install")
            .is_err());
        integrity(&db).unwrap();
    }

    #[test]
    fn original_schema1_backup_restores_through_full_supported_chain() {
        let (dir, db) = fixture();
        let old_path = dir.path().join("original-schema1.sqlite");
        let old = Connection::open(&old_path).unwrap();
        old.execute_batch("CREATE TABLE schema_version(version INTEGER NOT NULL); INSERT INTO schema_version VALUES(1);
            CREATE TABLE timesheets(id INTEGER PRIMARY KEY,pa_name TEXT NOT NULL,start_time TEXT NOT NULL,end_time TEXT NOT NULL,break_minutes INTEGER NOT NULL,worked_minutes INTEGER NOT NULL,hourly_rate REAL NOT NULL,amount REAL NOT NULL,notes TEXT);").unwrap();
        let selected = BackupService::create_verified(
            &old_path,
            &dir.path().join("config.toml"),
            &dir.path().join("backups"),
        )
        .unwrap();
        let original = fingerprint(&old).unwrap();
        let plan = review(dir.path(), &selected);
        assert_eq!(plan.schema_version, 1);
        restore(
            dir.path(),
            plan,
            true,
            "Verified original schema1 recovery rehearsal",
        )
        .unwrap();
        assert_eq!(
            version(&db).unwrap(),
            Some(crate::database::CURRENT_SCHEMA_VERSION)
        );
        assert_eq!(fingerprint(&old).unwrap(), original);
        assert_eq!(
            db.query_row::<i64, _, _>(
                "SELECT count(*) FROM timesheet_delivery_attempts",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn sql_install_preserves_autoincrement_high_water_mark_and_custom_schema() {
        let (dir, db) = fixture();
        legacy(&db, 35);
        db.execute_batch("CREATE TABLE extra_evidence(id INTEGER PRIMARY KEY AUTOINCREMENT,value TEXT); INSERT INTO extra_evidence VALUES(999,'old retained sequence'); DELETE FROM extra_evidence; CREATE INDEX extra_value ON extra_evidence(value); CREATE VIEW extra_view AS SELECT * FROM extra_evidence;").unwrap();
        initialise(&dir.path().join("database.sqlite")).unwrap();
        db.execute(
            "INSERT INTO extra_evidence(value) VALUES('after upgrade')",
            [],
        )
        .unwrap();
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT id FROM extra_view", [], |r| r.get(0))
                .unwrap(),
            1000
        );
    }
}
