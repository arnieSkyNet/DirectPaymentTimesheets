//! Prospective shift safeguards. Migration never reinterprets existing corrections.
use crate::payroll_evidence::lifecycle::{self, Stage};
use crate::payroll_evidence::{self, WorkEvidence};
use crate::payroll_timesheet_repository::PayrollTimesheet;
use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
pub type Result<T> = payroll_evidence::Result<T>;

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    // Replace exact-version guards within this transaction before changing schema.
    let guards = tx
        .prepare("SELECT name,sql FROM sqlite_master WHERE type='trigger' AND name LIKE 'dpt37_%'")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (name, sql) in guards {
        tx.execute_batch(&format!(
            "DROP TRIGGER {}; {}",
            crate::database_recovery::quote(&name),
            sql.replace("dpt_schema_version()<>37", "dpt_schema_version()<37")
        ))?;
    }
    tx.execute_batch("CREATE TABLE csv_import_contents(sha256 TEXT NOT NULL,import_audit_id INTEGER NOT NULL REFERENCES import_audit(id),original_filename TEXT NOT NULL,PRIMARY KEY(sha256,original_filename));
        CREATE TABLE csv_import_rows(sha256 TEXT NOT NULL,source_id INTEGER NOT NULL,PRIMARY KEY(sha256,source_id));
        CREATE TABLE shift_change_links(old_source TEXT NOT NULL,old_id INTEGER NOT NULL,new_source TEXT NOT NULL,new_id INTEGER NOT NULL,created_at TEXT NOT NULL,PRIMARY KEY(old_source,old_id,new_source,new_id));
        CREATE TABLE shift_review_deferrals(id INTEGER PRIMARY KEY,personal_assistant_id INTEGER NOT NULL,group_fingerprint TEXT NOT NULL,evidence TEXT NOT NULL,deferred_at TEXT NOT NULL,actor TEXT NOT NULL);
        CREATE TRIGGER retain_shift_review_deferrals_update BEFORE UPDATE ON shift_review_deferrals BEGIN SELECT RAISE(ABORT,'Immutable deferred review'); END;
        CREATE TRIGGER retain_shift_review_deferrals_delete BEFORE DELETE ON shift_review_deferrals BEGIN SELECT RAISE(ABORT,'Retained deferred review'); END;
        CREATE TABLE shift_change_events(id INTEGER PRIMARY KEY,source TEXT NOT NULL,source_id INTEGER NOT NULL,before_evidence TEXT NOT NULL,after_evidence TEXT NOT NULL,reason TEXT NOT NULL,review_signature TEXT NOT NULL,recorded_at TEXT NOT NULL,actor TEXT NOT NULL);
        CREATE TRIGGER retain_shift_change_events_update BEFORE UPDATE ON shift_change_events BEGIN SELECT RAISE(ABORT,'Immutable shift change audit'); END;
        CREATE TRIGGER retain_shift_change_events_delete BEFORE DELETE ON shift_change_events BEGIN SELECT RAISE(ABORT,'Retained shift change audit'); END;
        UPDATE schema_version SET version=38;")?;
    let tables = tx
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for table in tables {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER dpt38_{table}_{op} BEFORE {op} ON {} BEGIN SELECT CASE WHEN dpt_schema_version()<38 THEN RAISE(ABORT,'Incompatible application schema') END; END;",crate::database_recovery::quote(&table)))?;
        }
    }
    tx.commit()
}
pub fn content_seen(db: &Connection, hash: &str, path: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM csv_import_contents WHERE sha256=?1 AND original_filename=?2)",
        params![hash, path],
        |r| r.get(0),
    )?)
}
pub fn record_import(
    db: &Connection,
    hash: &str,
    path: &str,
    rows: &[crate::models::TimesheetEntry],
    prior_max: i64,
) -> Result<()> {
    let prior: Option<String> = db.query_row("SELECT sha256 FROM csv_import_contents WHERE original_filename=?1 ORDER BY import_audit_id DESC LIMIT 1",[path],|r|r.get(0)).optional()?;
    let audit: i64 = db.query_row(
        "SELECT MAX(id) FROM import_audit WHERE original_filename=?1 AND status='SUCCESS'",
        [path],
        |r| r.get(0),
    )?;
    db.execute(
        "INSERT OR IGNORE INTO csv_import_contents VALUES(?1,?2,?3)",
        params![hash, audit, path],
    )?;
    let ids = db
        .prepare("SELECT id FROM timesheets ORDER BY id")?
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let evidence = ids
        .into_iter()
        .map(|id| crate::repository::get_raw_on(db, id)?.ok_or_else(|| "Missing raw shift".into()))
        .collect::<Result<Vec<_>>>()?;
    let mut present = Vec::new();
    for row in rows {
        let start = crate::csv_import::parse_supported_timestamp(&row.start_time)
            .ok_or("Invalid imported start")?;
        let end = crate::csv_import::parse_supported_timestamp(&row.end_time)
            .ok_or("Invalid imported end")?;
        // Match all retained raw copies: duplicates within one export remain reviewable.
        let incoming = crate::csv_import::ParsedTimesheetRow {
            source_row: 0,
            start,
            end,
            entry: row.clone(),
        };
        for raw in &evidence {
            if !crate::import_service::existing_row_is_materially_identical(&raw, &incoming) {
                continue;
            }
            let id = raw.id;
            db.execute(
                "INSERT OR IGNORE INTO csv_import_rows VALUES(?1,?2)",
                params![hash, id],
            )?;
            present.push(id);
            if id > prior_max && !prospective(db, "imported", id)? {
                event(
                    db,
                    "imported",
                    id,
                    "",
                    &format!("{raw:?}"),
                    "New CSV evidence; protected financial changes require payroll review",
                    hash,
                )?;
            }
        }
    }
    for id in present.iter().copied().filter(|id| *id > prior_max) {
        link_possible_counterparts(db, "imported", id, &present)?;
    }
    if let Some(prior) = prior {
        let previous = db
            .prepare("SELECT source_id FROM csv_import_rows WHERE sha256=?1")?
            .query_map([prior], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for old in evidence
            .iter()
            .filter(|e| previous.contains(&e.id) && !present.contains(&e.id))
        {
            for new in evidence.iter().filter(|e| {
                present.contains(&e.id)
                    && !previous.contains(&e.id)
                    && e.personal_assistant_id == old.personal_assistant_id
            }) {
                // A same-path revision with an omitted old row and an added row is
                // a possible correction, never an automatic replacement.
                db.execute(
                    "INSERT OR IGNORE INTO shift_change_links VALUES(?1,?2,?3,?4,?5)",
                    params![
                        "imported",
                        old.id,
                        "imported",
                        new.id,
                        payroll_evidence::now()
                    ],
                )?;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Review {
    pub signature: String,
    pub protected: bool,
    pub description: String,
}
pub fn review(
    db: &Connection,
    pa: i64,
    ranges: &[(NaiveDateTime, NaiveDateTime)],
) -> Result<Review> {
    let mut stmt=db.prepare("SELECT p.id,p.personal_assistant_id,p.payroll_year,p.cycle_number,p.previous_cycle_hours,p.created_at,p.updated_at,p.payroll_department_notes,p.actual_in_lieu_hours,p.actual_in_lieu_updated_at,s.first_week_commencing FROM payroll_timesheets p JOIN payroll_schedules s ON s.payroll_year=p.payroll_year AND s.cycle_number=p.cycle_number WHERE p.personal_assistant_id=?1 ORDER BY p.id")?;
    let records = stmt
        .query_map([pa], |r| {
            Ok((
                PayrollTimesheet {
                    id: r.get(0)?,
                    personal_assistant_id: r.get(1)?,
                    payroll_year: r.get(2)?,
                    cycle_number: r.get(3)?,
                    previous_cycle_hours: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                    payroll_department_notes: r.get(7)?,
                    actual_in_lieu_hours: r.get(8)?,
                    actual_in_lieu_updated_at: r.get(9)?,
                },
                r.get::<_, String>(10)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut states = Vec::new();
    let mut protected = false;
    for (record, first) in records {
        let start =
            crate::models::parse_employment_date(&first).ok_or("Invalid payroll schedule date")?;
        let end = start + chrono::Duration::days(27);
        if !ranges
            .iter()
            .any(|(a, b)| a.date() <= end && b.date() >= start)
        {
            continue;
        }
        let stage = lifecycle::stage(db, &record)?;
        if stage == Stage::Indeterminate {
            return Err(format!("Payroll {} cycle {} has uncertain delivery; audited resolution is required before changing shifts",record.payroll_year,record.cycle_number).into());
        }
        let sid = lifecycle::latest(db, record.id)?;
        protected |= stage != Stage::Editable || sid.is_some();
        states.push(format!(
            "{}:{}:{stage:?}:{sid:?}:{}",
            record.payroll_year, record.cycle_number, record.updated_at
        ));
    }
    let description = states.join("\n");
    Ok(Review {
        signature: digest(format!("{pa}:{ranges:?}:{description}").as_bytes()),
        protected,
        description,
    })
}

fn retained_ranges(
    db: &Connection,
    source: &str,
    id: i64,
) -> Result<Vec<(NaiveDateTime, NaiveDateTime)>> {
    let dates=db.prepare("SELECT DISTINCT q.first_week_commencing FROM payroll_submission_items i JOIN payroll_submissions s ON s.id=i.submission_id JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id LEFT JOIN payroll_schedules q ON q.payroll_year=p.payroll_year AND q.cycle_number=p.cycle_number WHERE (?1='direct' AND i.direct_shift_id=?2) OR (?1='imported' AND i.timesheet_id=?2)")?.query_map(params![source,id],|r|r.get::<_,Option<String>>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    dates
        .into_iter()
        .map(|s| {
            let d =
                crate::models::parse_employment_date(s.as_deref().ok_or("Missing payroll schedule for retained submission; repair or restore it before changing shifts")?).ok_or("Invalid retained payroll date")?;
            Ok((
                d.and_hms_opt(0, 0, 0).unwrap(),
                (d + chrono::Duration::days(27))
                    .and_hms_opt(23, 59, 59)
                    .unwrap(),
            ))
        })
        .collect()
}
pub fn direct_review(
    db: &Connection,
    before: &crate::direct_shift_repository::DirectShift,
    start: NaiveDateTime,
    end: NaiveDateTime,
    break_minutes: i64,
    notes: Option<&str>,
) -> Result<Review> {
    let mut ranges = vec![
        (
            before.start()?,
            before.end()?.ok_or("Completed shift required")?,
        ),
        (start, end),
    ];
    ranges.extend(retained_ranges(db, "direct", before.id)?);
    let mut review = review(db, before.personal_assistant_id, &ranges)?;
    let notes = notes.map(str::trim).filter(|s| !s.is_empty());
    review.signature = digest(
        format!(
            "{}:{before:?}:{start}:{end}:{break_minutes}:{notes:?}",
            review.signature
        )
        .as_bytes(),
    );
    Ok(review)
}
pub fn group_review(db: &Connection, group: &payroll_evidence::DuplicateGroup) -> Result<Review> {
    let mut ranges = Vec::new();
    let mut payload = String::new();
    for e in &group.candidates {
        ranges.push((e.start_time()?, e.end_time()?));
        ranges.extend(retained_ranges(db, &e.source, e.id)?);
        payload.push_str(&toml::to_string(e)?);
        if e.source == "imported" {
            if let Some(raw) = crate::repository::get_raw_on(db, e.id)? {
                payload.push_str(&format!("{raw:?}"));
            }
        }
    }
    let mut review = review(db, group.candidates[0].pa, &ranges)?;
    review.signature = digest(format!("{}:{payload}", review.signature).as_bytes());
    Ok(review)
}
pub fn event(
    db: &Connection,
    source: &str,
    id: i64,
    before: &str,
    after: &str,
    reason: &str,
    signature: &str,
) -> Result<()> {
    if !before.is_empty() {
        let peers=db.prepare("SELECT DISTINCT m.source,m.source_id FROM payroll_duplicate_members m JOIN payroll_duplicate_decisions d ON d.id=m.decision_id WHERE d.invalidated_at IS NULL AND d.winner_source<>'separate' AND d.id IN (SELECT decision_id FROM payroll_duplicate_members WHERE source=?1 AND source_id=?2)")?.query_map(params![source,id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for (peer, pid) in peers {
            if peer != source || pid != id {
                db.execute(
                    "INSERT OR IGNORE INTO shift_change_links VALUES(?1,?2,?3,?4,?5)",
                    params![peer, pid, source, id, payroll_evidence::now()],
                )?;
            }
        }
    }
    db.execute("INSERT INTO shift_change_events(source,source_id,before_evidence,after_evidence,reason,review_signature,recorded_at,actor) VALUES(?1,?2,?3,?4,?5,?6,?7,'local_employer')",params![source,id,before,after,reason,signature,payroll_evidence::now()])?;
    Ok(())
}
pub fn prospective(db: &Connection, source: &str, id: i64) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM shift_change_events WHERE source=?1 AND source_id=?2)",
        params![source, id],
        |r| r.get(0),
    )?)
}
pub fn linked_groups(
    db: &Connection,
    evidence: &[WorkEvidence],
) -> Result<Vec<payroll_evidence::DuplicateGroup>> {
    let mut groups = payroll_evidence::groups(evidence)?;
    let mut preserved = Vec::new();
    for group in groups {
        let mut changed = false;
        for e in &group.candidates {
            changed |= prospective(db, &e.source, e.id)?;
        }
        if changed {
            preserved.push(group);
            continue;
        }
        // Legacy overlap decisions keep their original same-start-date grouping
        // until new source evidence is introduced or reviewed.
        let mut days = std::collections::BTreeMap::new();
        for e in group.candidates {
            days.entry(e.date()?).or_insert_with(Vec::new).push(e);
        }
        for (_, members) in days {
            preserved.extend(payroll_evidence::groups(&members)?);
        }
    }
    groups = preserved;

    let links = db
        .prepare("SELECT old_source,old_id,new_source,new_id FROM shift_change_links")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (a, ai, b, bi) in links {
        let pair = evidence
            .iter()
            .filter(|e| {
                !e.deleted && ((e.source == a && e.id == ai) || (e.source == b && e.id == bi))
            })
            .cloned()
            .collect::<Vec<_>>();
        if pair.len() != 2 {
            continue;
        }
        let mut members = pair;
        let mut index = 0;
        while index < groups.len() {
            if groups[index]
                .candidates
                .iter()
                .any(|e| members.iter().any(|m| m.key() == e.key()))
            {
                members.extend(groups.remove(index).candidates);
                index = 0;
            } else {
                index += 1;
            }
        }
        members.sort_by_key(WorkEvidence::key);
        members.dedup_by_key(|e| e.key());
        let fingerprint = digest(
            &members
                .iter()
                .map(WorkEvidence::fingerprint)
                .collect::<Vec<_>>()
                .join("|")
                .as_bytes(),
        );
        groups.push(payroll_evidence::DuplicateGroup {
            fingerprint,
            candidates: members,
        });
    }
    Ok(groups)
}

pub fn defer(db: &Connection, groups: &[payroll_evidence::DuplicateGroup]) -> Result<()> {
    let _guard = crate::timesheet_delivery::production_lock(db)?;
    let tx = db.unchecked_transaction()?;
    tx.execute("UPDATE schema_version SET version=version", [])?;
    let fresh = linked_groups(&tx, &payroll_evidence::load_connection(&tx)?)?;
    for g in groups {
        if !fresh.iter().any(|f| f.fingerprint == g.fingerprint) {
            return Err("Evidence changed; refresh before deferring".into());
        }
        let evidence = g
            .candidates
            .iter()
            .map(toml::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n");
        tx.execute("INSERT INTO shift_review_deferrals(personal_assistant_id,group_fingerprint,evidence,deferred_at,actor) VALUES(?1,?2,?3,?4,'local_employer')",params![g.candidates[0].pa,g.fingerprint,evidence,payroll_evidence::now()])?;
    }
    tx.commit()?;
    Ok(())
}

// Called only by a user-requested import, never at startup/migration. Register
// verified archive identity against its real existing audit; invent no attempts.
pub fn seed_legacy_path(db: &Connection, path: &str) -> Result<()> {
    let already: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM csv_import_contents WHERE original_filename=?1)",
        [path],
        |r| r.get(0),
    )?;
    if already {
        return Ok(());
    }
    let legacy:Option<(i64,String)>=db.query_row("SELECT id,archive_filename FROM import_audit WHERE original_filename=?1 AND status='SUCCESS' ORDER BY id DESC LIMIT 1",[path],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((audit, archive)) = legacy else {
        return Ok(());
    };
    let bytes=std::fs::read(&archive).map_err(|e|format!("Cannot verify original CSV archive {archive}: {e}; restore the archive before reviewing this changed export"))?;
    let hash = digest(&bytes);
    let rows = crate::csv_import::import_csv_bytes(&bytes)?;
    db.execute(
        "INSERT INTO csv_import_contents VALUES(?1,?2,?3)",
        params![hash, audit, path],
    )?;
    let ids = db
        .prepare("SELECT id FROM timesheets")?
        .query_map([], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids {
        let raw = crate::repository::get_raw_on(db, id)?.ok_or("Missing original shift")?;
        for row in &rows {
            let mut incoming = crate::csv_import::ParsedTimesheetRow {
                source_row: row.source_row,
                start: row.start,
                end: row.end,
                entry: row.entry.clone(),
            };
            incoming.entry.personal_assistant_id = raw.personal_assistant_id;
            let name = |s: &str| {
                s.split_whitespace()
                    .map(str::to_lowercase)
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            if name(&raw.pa_name) == name(&incoming.entry.pa_name)
                && crate::import_service::existing_row_is_materially_identical(&raw, &incoming)
            {
                db.execute(
                    "INSERT OR IGNORE INTO csv_import_rows VALUES(?1,?2)",
                    params![hash, id],
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod stage4_migration_tests {
    use super::*;
    fn legacy() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::database::open(dir.path().join("payroll.sqlite")).unwrap();
        crate::database::create_legacy_schema(&db, 37).unwrap();
        db.execute_batch("INSERT INTO personal_assistants(id,first_name,surname,employment_status) VALUES(1,'Legacy','Example','Active');
            INSERT INTO payroll_schedules(payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at,payslips_sent) VALUES('2026/27',1,'01/04/2026','01/04/2026','01/04/2026','legacy',1);
            INSERT INTO payroll_timesheets(id,personal_assistant_id,payroll_year,cycle_number,created_at,updated_at) VALUES(10,1,'2026/27',1,'legacy','legacy');
            INSERT INTO timesheets(id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount,notes) VALUES(1,1,'Legacy Example','2 April 2026 at 09:00:00','2 April 2026 at 10:00:00',0,60,12,12,'original');
            INSERT INTO timesheet_correction_events(timesheet_id,actor_id,action_type,action_at,reason,before_start_time,before_end_time,before_break_minutes,before_worked_minutes,after_start_time,after_end_time,after_break_minutes,after_worked_minutes) VALUES(1,'local_employer','edit','legacy','Historical authorised edit','2026-04-02T09:00','2026-04-02T10:00',0,60,'2026-04-02T09:00','2026-04-02T11:00',0,120);
            INSERT INTO payroll_submissions(id,payroll_timesheet_id,submitted_at,pdf_path,pdf_sha256,pdf_bytes) VALUES(3,10,'legacy','original.pdf','original-digest',X'25504446');
            INSERT INTO payroll_reconciliation_decisions(id,payroll_timesheet_id,submission_id,kind,evidence_key,evidence,decided_at,actor) VALUES(8,10,3,'carry','legacy-approved','Original historical decision','legacy','local_employer');
            INSERT INTO payroll_corrections(id,personal_assistant_id,origin_payroll_timesheet_id,submission_id,decision_id,evidence_key,source,source_id,work_date,minutes,reason,created_at) VALUES(7,1,10,3,8,'legacy-applied','imported',1,'2026-04-02',60,'Approved financial correction','legacy'),(9,1,10,3,8,'legacy-outstanding','imported',1,'2026-04-02',-30,'Outstanding original obligation','legacy');
            INSERT INTO payroll_correction_applications VALUES(10,9,-15);
            INSERT INTO payroll_submission_corrections VALUES(3,7,60);
            INSERT INTO direct_shifts(id,personal_assistant_id,start_time,end_time,break_minutes,notes,source_type,created_at,updated_at) VALUES(1,1,'2026-04-03T09:00','2026-04-03T11:00',10,'legacy edited','direct','legacy','legacy');
            INSERT INTO direct_shift_audit(direct_shift_id,actor_id,action_type,action_at,before_start_time,before_end_time,before_break_minutes,after_start_time,after_end_time,after_break_minutes) VALUES(1,'local_employer','edit','legacy','2026-04-03T09:00','2026-04-03T10:00',0,'2026-04-03T09:00','2026-04-03T11:00',10);").unwrap();
        (dir, db)
    }
    fn rows(db: &Connection, table: &str) -> Vec<Vec<rusqlite::types::Value>> {
        let mut stmt = db.prepare(&format!("SELECT * FROM {table}")).unwrap();
        let n = stmt.column_count();
        stmt.query_map([], |row| (0..n).map(|i| row.get(i)).collect())
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }
    #[test]
    fn verified_upgrade_preserves_populated_schema37_legacy_obligations_and_audits() {
        let (dir, db) = legacy();
        let tables = [
            "timesheets",
            "timesheet_correction_events",
            "direct_shifts",
            "direct_shift_audit",
            "payroll_timesheets",
            "payroll_submissions",
            "payroll_reconciliation_decisions",
            "payroll_corrections",
            "payroll_correction_applications",
            "payroll_submission_corrections",
        ];
        let before = tables.map(|t| rows(&db, t));
        let original = crate::database_recovery::fingerprint(&db).unwrap();
        crate::database::initialise_database(&dir.path().join("payroll.sqlite")).unwrap();
        for (table, saved) in tables.into_iter().zip(before) {
            assert_eq!(rows(&db, table), saved, "{table}");
        }
        assert!(rows(&db, "shift_change_events").is_empty());
        assert!(rows(&db, "csv_import_contents").is_empty());
        let backup = std::fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let saved = Connection::open(backup.join("database.sqlite")).unwrap();
        assert_eq!(
            crate::database_recovery::fingerprint(&saved).unwrap(),
            original
        );
        assert_eq!(
            crate::database_recovery::version(&db).unwrap(),
            Some(crate::database::CURRENT_SCHEMA_VERSION)
        );
    }
    #[test]
    fn migration_failure_rolls_back_guards_tables_and_legacy_obligations() {
        let (_dir, db) = legacy();
        db.execute_batch("CREATE TRIGGER fail38 BEFORE UPDATE ON schema_version WHEN NEW.version=38 BEGIN SELECT RAISE(ABORT,'simulated migration failure'); END;").unwrap();
        let before = crate::database_recovery::fingerprint(&db).unwrap();
        assert!(migrate(&db).is_err());
        assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    }
    #[test]
    fn older_schema37_connection_cannot_write_schema38_business_or_version_rows() {
        let (_dir, db) = legacy();
        migrate(&db).unwrap();
        db.create_scalar_function(
            "dpt_schema_version",
            0,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |_| Ok(37i64),
        )
        .unwrap();
        assert!(db
            .execute("UPDATE timesheets SET worked_minutes=999", [])
            .is_err());
        assert!(db
            .execute("UPDATE schema_version SET version=37", [])
            .is_err());
        assert!(db.execute("DELETE FROM payroll_corrections", []).is_err());
        assert_eq!(rows(&db, "payroll_corrections").len(), 2);
    }
}

/// Without an upstream entry ID these are suggestions requiring review, not
/// replacement claims. Co-present CSV rows remain genuinely separate evidence.
pub fn link_possible_counterparts(
    db: &Connection,
    source: &str,
    id: i64,
    co_present: &[i64],
) -> Result<()> {
    let pa: Option<i64> = if source == "imported" {
        db.query_row(
            "SELECT personal_assistant_id FROM timesheets WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .flatten()
    } else {
        db.query_row(
            "SELECT personal_assistant_id FROM direct_shifts WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
    };
    let Some(pa) = pa else {
        return Ok(());
    };
    let all = payroll_evidence::load_for_pa(db, pa)?;
    let Some(new) = all
        .iter()
        .find(|e| e.source == source && e.id == id && !e.deleted)
    else {
        return Ok(());
    };
    for old in all.iter().filter(|e| {
        !e.deleted
            && e.key() != new.key()
            && !(e.source == "imported" && co_present.contains(&e.id))
            && !(source == "direct" && e.source == "direct")
    }) {
        let Some((old_start, old_end)) = old.start_time().ok().zip(old.end_time().ok()) else {
            continue;
        };
        let new_start = new.start_time()?;
        let new_end = new.end_time()?;
        let gap = (old_start.date() - new_start.date()).num_days().abs();
        if gap != 1
            || old_start.time() != new_start.time()
            || old_end.time() != new_end.time()
            || old.break_minutes != new.break_minutes
            || old.minutes != new.minutes
            || old.notes.trim().is_empty()
            || old.notes.trim() != new.notes.trim()
        {
            continue;
        }
        if old.source == "imported" && new.source == "imported" {
            let a = crate::repository::get_raw_on(db, old.id)?.ok_or("Missing old import")?;
            let b = crate::repository::get_raw_on(db, new.id)?.ok_or("Missing new import")?;
            if a.hourly_rate.to_bits() != b.hourly_rate.to_bits()
                || a.amount.to_bits() != b.amount.to_bits()
            {
                continue;
            }
        }
        db.execute(
            "INSERT OR IGNORE INTO shift_change_links VALUES(?1,?2,?3,?4,?5)",
            params![
                old.source,
                old.id,
                new.source,
                new.id,
                payroll_evidence::now()
            ],
        )?;
    }
    Ok(())
}
