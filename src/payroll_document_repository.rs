use crate::payroll_timesheet_email_repository::{
    delivery_state, EmailDeliveryState, PayrollTimesheetEmailRepository,
};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct PayrollDocument {
    pub id: i64,
    pub personal_assistant_id: i64,
    pub document_type: String,
    pub path: PathBuf,
    pub sha256: String,
    pub document_year: Option<String>,
    pub delivery_state: EmailDeliveryState,
    pub history_state: String,
}

pub fn file_digest(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

// Share the email repository connection so mixed bundles transition atomically.
impl PayrollTimesheetEmailRepository {
    pub fn reconcile_document(&self, id: i64, needs_sending: bool) -> rusqlite::Result<bool> {
        Ok(self.connection.execute("UPDATE imported_payroll_documents SET history_state = ?1 WHERE id = ?2 AND sent_at IS NULL AND history_state = 'unknown' AND superseded_by IS NULL", params![if needs_sending { "needs_sending" } else { "external" }, id])? == 1)
    }

    pub fn register_document(
        &self,
        pa: i64,
        kind: &str,
        path: &Path,
        year: Option<&str>,
    ) -> Result<i64, Box<dyn Error>> {
        crate::archive::validate_payslip_pdf(path)?;
        let canonical = path.canonicalize()?;
        let path = canonical
            .to_str()
            .ok_or("Payroll document path is not UTF-8")?;
        let digest = file_digest(&canonical)?;
        let transaction = self.connection.unchecked_transaction()?;
        let duplicate: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM imported_payroll_documents WHERE personal_assistant_id=?1 AND document_type=?2 AND sha256=?3 AND stored_path<>?4 AND NOT EXISTS(SELECT 1 FROM imported_payroll_documents WHERE stored_path=?4))", params![pa,kind,digest,path], |r| r.get(0))?;
        if duplicate {
            return Err("Identical supplement already registered at another path; keep its existing delivery history and reconcile that document instead.".into());
        }
        let existing_identity: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM imported_payroll_documents WHERE personal_assistant_id=?1 AND document_type=?2 AND (document_year IS ?3 OR document_year IS NULL OR ?3 IS NULL) AND superseded_by IS NULL AND stored_path<>?4)", params![pa,kind,year,path], |r|r.get(0))?;
        let registered_path: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM imported_payroll_documents WHERE stored_path=?1)",
            [path],
            |r| r.get(0),
        )?;
        if existing_identity && !registered_path {
            return Err("A current supplement already exists for this PA, type and year. Use Replace payroll document in Personal Assistant Maintenance to review a correction.".into());
        }
        transaction.execute(
            "INSERT INTO imported_payroll_documents
            (personal_assistant_id, document_type, stored_path, sha256, document_year)
            VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(stored_path) DO NOTHING",
            params![pa, kind, path, digest, year],
        )?;
        let (id, matches): (i64, bool) = transaction.query_row(
            "SELECT id, personal_assistant_id = ?1 AND document_type = ?2 AND sha256 = ?4 AND document_year IS ?5
             FROM imported_payroll_documents WHERE stored_path = ?3", params![pa, kind, path, digest, year],
            |row| Ok((row.get(0)?, row.get(1)?)))?;
        if !matches {
            return Err(
                "Stored payroll document identity/content differs; delivery state was not changed."
                    .into(),
            );
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn documents_for_pa(&self, pa: i64) -> rusqlite::Result<Vec<PayrollDocument>> {
        self.read_documents(pa, true)
    }
    pub fn all_documents_for_pa(&self, pa: i64) -> rusqlite::Result<Vec<PayrollDocument>> {
        self.read_documents(pa, false)
    }
    fn read_documents(
        &self,
        pa: i64,
        current_only: bool,
    ) -> rusqlite::Result<Vec<PayrollDocument>> {
        let mut statement = self.connection.prepare("SELECT id, personal_assistant_id, document_type, stored_path, sha256, document_year, sent_at, history_state
            FROM imported_payroll_documents WHERE personal_assistant_id = ?1 AND (?2=0 OR superseded_by IS NULL) ORDER BY id")?;
        let rows = statement.query_map(params![pa, current_only], |row| {
            let sent_at: Option<String> = row.get(6)?;
            Ok(PayrollDocument {
                id: row.get(0)?,
                personal_assistant_id: row.get(1)?,
                document_type: row.get(2)?,
                path: PathBuf::from(row.get::<_, String>(3)?),
                sha256: row.get(4)?,
                document_year: row.get(5)?,
                delivery_state: delivery_state(sent_at.as_deref()),
                history_state: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    #[cfg(test)]
    pub fn transition_payroll_bundle(
        &self,
        pa: i64,
        identity: Option<&crate::payslip_delivery_service::PayslipDeliveryIdentity<'_>>,
        payslip: bool,
        documents: &[i64],
        expected: Option<&str>,
        next: Option<&str>,
    ) -> rusqlite::Result<bool> {
        self.transition_payroll_bundle_checked(
            pa, identity, payslip, documents, expected, next, None,
        )
    }

    pub fn transition_payroll_bundle_checked(
        &self,
        pa: i64,
        identity: Option<&crate::payslip_delivery_service::PayslipDeliveryIdentity<'_>>,
        payslip: bool,
        documents: &[i64],
        expected: Option<&str>,
        next: Option<&str>,
        attachment: Option<&(PathBuf, String)>,
    ) -> rusqlite::Result<bool> {
        if !payslip && documents.is_empty() {
            return Ok(false);
        }
        let transaction = self.connection.unchecked_transaction()?;
        if payslip {
            let Some(identity) = identity.filter(|i| i.personal_assistant_id == pa) else {
                return Ok(false);
            };
            if let Some((path, digest)) = attachment {
                let current = crate::payroll_replacement::current_payslip(
                    &transaction,
                    pa,
                    identity.payroll_year,
                    identity.cycle_number,
                )
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
                if current
                    .as_ref()
                    .is_some_and(|r| r.0 != *path || r.1 != *digest)
                    || file_digest(path).ok().as_ref() != Some(digest)
                {
                    return Ok(false);
                }
            }
            transaction.execute("INSERT OR IGNORE INTO payroll_timesheet_email_status
                (personal_assistant_id, payroll_year, cycle_number, email_type, sent_at) VALUES (?1, ?2, ?3, 'payslip', NULL)",
                params![identity.personal_assistant_id, identity.payroll_year, identity.cycle_number])?;
            if transaction.execute("UPDATE payroll_timesheet_email_status SET sent_at = ?1
                WHERE personal_assistant_id = ?2 AND payroll_year = ?3 AND cycle_number = ?4 AND email_type = 'payslip' AND sent_at IS ?5",
                params![next, identity.personal_assistant_id, identity.payroll_year, identity.cycle_number, expected])? != 1 { return Ok(false); }
        }
        if payslip {
            let identity = identity.unwrap();
            transaction.execute("UPDATE payslip_revisions SET sent_at=?1 WHERE personal_assistant_id=?2 AND payroll_year=?3 AND cycle_number=?4 AND is_current=1", params![next,identity.personal_assistant_id,identity.payroll_year,identity.cycle_number])?;
        }
        let mut seen = std::collections::HashSet::new();
        for id in documents {
            if !seen.insert(id)
                || transaction.execute(
                    "UPDATE imported_payroll_documents SET sent_at = ?1
                WHERE id = ?2 AND personal_assistant_id = ?3 AND sent_at IS ?4 AND history_state = 'needs_sending' AND superseded_by IS NULL",
                    params![next, id, pa, expected],
                )? != 1
            {
                return Ok(false);
            }
        }
        transaction.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{create_schema, CURRENT_SCHEMA_VERSION};
    use rusqlite::Connection;

    fn statuses(connection: &Connection) -> Vec<(i64, i64, String, i64, String, Option<String>)> {
        connection.prepare("SELECT id, personal_assistant_id, payroll_year, cycle_number, email_type, sent_at FROM payroll_timesheet_email_status ORDER BY id")
            .unwrap().query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
    }

    #[test]
    fn schema30_to31_preserves_all_18_payslip_and_3_timesheet_statuses_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.sqlite");
        let connection = crate::database::open(&path).unwrap();
        create_schema(&connection).unwrap();
        crate::database::tests::remove_schema_32_fixture(&connection);
        connection
            .execute_batch(
                "DROP TABLE imported_payroll_documents; UPDATE schema_version SET version = 30;",
            )
            .unwrap();
        for i in 1..=21 {
            connection.execute("INSERT INTO payroll_timesheet_email_status (id, personal_assistant_id, payroll_year, cycle_number, email_type, sent_at)
                VALUES (?1, ?1, '2025/26', 13, ?2, ?3)", params![i, if i <= 18 { "payslip" } else { "timesheet" }, format!("2026-04-01T10:00:{i:02}Z")]).unwrap();
        }
        let before = statuses(&connection);
        create_schema(&connection).unwrap();
        assert_eq!(statuses(&connection), before);
        assert_eq!(
            connection
                .query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM imported_payroll_documents",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
                .unwrap(),
            CURRENT_SCHEMA_VERSION
        );
        drop(connection);
        let reopened = crate::database::open(path).unwrap();
        create_schema(&reopened).unwrap();
        assert_eq!(statuses(&reopened), before);
        assert_eq!(CURRENT_SCHEMA_VERSION, 39);
    }

    #[test]
    fn schema31_migration_is_atomic_on_version_write_failure() {
        let connection = crate::database::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        crate::database::tests::remove_schema_32_fixture(&connection);
        connection.execute_batch("DROP TABLE imported_payroll_documents; UPDATE schema_version SET version = 30;
            CREATE TRIGGER refuse_schema BEFORE UPDATE ON schema_version BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(create_schema(&connection).is_err());
        assert_eq!(
            connection
                .query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
                .unwrap(),
            30
        );
        assert_eq!(connection.query_row::<i64,_,_>("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('imported_payroll_documents', 'idx_imported_payroll_documents_pa')", [], |r| r.get(0)).unwrap(), 0);
        connection
            .execute_batch("DROP TRIGGER refuse_schema")
            .unwrap();
        create_schema(&connection).unwrap();
    }

    #[test]
    fn document_identity_reimport_preserves_state_and_checks_content_and_pa() {
        let dir = tempfile::tempdir().unwrap();
        let connection = crate::database::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        connection.execute_batch("INSERT INTO personal_assistants(id, first_name, surname) VALUES (1, 'Test', 'One'), (2, 'Test', 'Two');").unwrap();
        let repository = PayrollTimesheetEmailRepository::new(connection);
        let path = dir.path().join("P60.pdf");
        std::fs::write(&path, b"%PDF-1.4 original").unwrap();
        let id = repository
            .register_document(1, "p60", &path, Some("2025/26"))
            .unwrap();
        repository
            .connection
            .execute(
                "UPDATE imported_payroll_documents SET sent_at = 'sent' WHERE id = ?1",
                [id],
            )
            .unwrap();
        assert_eq!(
            repository
                .register_document(1, "p60", &path, Some("2025/26"))
                .unwrap(),
            id
        );
        assert!(matches!(
            repository.documents_for_pa(1).unwrap()[0].delivery_state,
            EmailDeliveryState::Sent { .. }
        ));
        assert!(repository
            .register_document(2, "p60", &path, Some("2025/26"))
            .is_err());
        assert!(repository.register_document(1, "p45", &path, None).is_err());
        assert!(repository.register_document(1, "p30", &path, None).is_err());
        std::fs::write(&path, b"%PDF-1.4 changed").unwrap();
        assert!(repository
            .register_document(1, "p60", &path, Some("2025/26"))
            .is_err());
        assert!(repository.documents_for_pa(2).unwrap().is_empty());
    }
    #[test]
    fn schema33_preserves_evidence_and_reconciliation_persists_without_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("migration.sqlite");
        let db = crate::database::open(&path).unwrap();
        create_schema(&db).unwrap();
        crate::database::tests::remove_schema_36_fixture(&db);
        db.execute_batch("DROP TABLE payslip_revisions; ALTER TABLE imported_payroll_documents DROP COLUMN superseded_by; DROP TABLE payroll_file_moves; ALTER TABLE imported_payroll_documents DROP COLUMN history_state; UPDATE schema_version SET version=32;
            INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Fixture','PA');").unwrap();
        let markers = [
            None,
            Some("2026-09-29T20:23:33+01:00"),
            Some("indeterminate:original-attempt"),
            None,
        ];
        for (index, marker) in markers.iter().enumerate() {
            let pdf = dir.path().join(format!("doc-{index}.pdf"));
            std::fs::write(&pdf, format!("%PDF-1.4 {index}")).unwrap();
            db.execute("INSERT INTO imported_payroll_documents(id,personal_assistant_id,document_type,stored_path,sha256,document_year,sent_at) VALUES(?1,1,'p45',?2,?3,'2026/27',?4)", params![index+1,pdf.to_str().unwrap(),file_digest(&pdf).unwrap(),marker]).unwrap();
        }
        db.execute_batch("CREATE TRIGGER fail_version BEFORE UPDATE ON schema_version BEGIN SELECT RAISE(ABORT,'migration failure'); END;").unwrap();
        assert!(create_schema(&db).is_err());
        assert!(db
            .prepare("SELECT history_state FROM imported_payroll_documents")
            .is_err());
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
                .unwrap(),
            32
        );
        db.execute_batch("DROP TRIGGER fail_version").unwrap();
        create_schema(&db).unwrap();
        let repo = PayrollTimesheetEmailRepository::new(db);
        let docs = repo.documents_for_pa(1).unwrap();
        assert_eq!(
            docs.iter()
                .map(|d| d.history_state.as_str())
                .collect::<Vec<_>>(),
            vec!["unknown", "application", "application", "unknown"]
        );
        for (index, marker) in markers.iter().enumerate() {
            assert_eq!(
                repo.connection
                    .query_row::<Option<String>, _, _>(
                        "SELECT sent_at FROM imported_payroll_documents WHERE id=?1",
                        [index + 1],
                        |r| r.get(0)
                    )
                    .unwrap()
                    .as_deref(),
                *marker
            );
        }
        assert!(!repo.reconcile_document(2, true).unwrap());
        assert!(!repo.reconcile_document(3, false).unwrap());
        assert!(repo.reconcile_document(1, false).unwrap());
        assert!(repo.reconcile_document(4, true).unwrap());
        assert!(!repo.reconcile_document(1, true).unwrap());
        drop(repo);
        let db = crate::database::open(&path).unwrap();
        create_schema(&db).unwrap();
        let repo = PayrollTimesheetEmailRepository::new(db);
        let docs = repo.documents_for_pa(1).unwrap();
        assert_eq!(docs[0].history_state, "external");
        assert_eq!(docs[3].history_state, "needs_sending");
        for (index, marker) in markers.iter().enumerate() {
            assert_eq!(
                repo.connection
                    .query_row::<Option<String>, _, _>(
                        "SELECT sent_at FROM imported_payroll_documents WHERE id=?1",
                        [index + 1],
                        |r| r.get(0)
                    )
                    .unwrap()
                    .as_deref(),
                *marker
            );
        }
    }

    #[test]
    fn unknown_history_is_not_sendable_and_identical_copies_cannot_bypass_history() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::database::open_in_memory().unwrap();
        create_schema(&db).unwrap();
        db.execute_batch(
            "INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Fixture','PA');",
        )
        .unwrap();
        let repo = PayrollTimesheetEmailRepository::new(db);
        for (index, kind) in ["p45", "p60"].iter().enumerate() {
            let path = dir.path().join(format!("{kind}.pdf"));
            std::fs::write(&path, format!("%PDF-1.4 {kind}")).unwrap();
            let id = repo
                .register_document(1, kind, &path, Some("2026/27"))
                .unwrap();
            assert_eq!(
                repo.documents_for_pa(1).unwrap()[index].history_state,
                "unknown"
            );
            assert!(
                crate::payslip_delivery_service::select_payroll_documents(&repo, 1, None)
                    .unwrap()
                    .paths
                    .is_empty()
            );
            assert!(!repo
                .transition_payroll_bundle(1, None, false, &[id], None, Some("indeterminate:test"))
                .unwrap());
            let copy = dir.path().join(format!("copy-{kind}.pdf"));
            std::fs::copy(&path, &copy).unwrap();
            assert!(repo
                .register_document(1, kind, &copy, Some("2026/27"))
                .is_err());
            assert!(repo.reconcile_document(id, index == 0).unwrap());
            if index == 0 {
                let bundle =
                    crate::payslip_delivery_service::select_payroll_documents(&repo, 1, None)
                        .unwrap();
                assert_eq!(bundle.document_ids, vec![id]);
                crate::payslip_delivery_service::send_payroll_bundle(
                    &repo,
                    1,
                    None,
                    &bundle,
                    "fixture",
                    || Ok(()),
                )
                .unwrap();
            }
            let before = repo
                .connection
                .query_row::<Option<String>, _, _>(
                    "SELECT sent_at FROM imported_payroll_documents WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                repo.register_document(1, kind, &path, Some("2026/27"))
                    .unwrap(),
                id
            );
            assert!(repo
                .register_document(1, kind, &copy, Some("2026/27"))
                .is_err());
            assert_eq!(
                repo.connection
                    .query_row::<Option<String>, _, _>(
                        "SELECT sent_at FROM imported_payroll_documents WHERE id=?1",
                        [id],
                        |r| r.get(0)
                    )
                    .unwrap(),
                before
            );
            assert!(
                crate::payslip_delivery_service::select_payroll_documents(&repo, 1, None)
                    .unwrap()
                    .paths
                    .is_empty()
            );
        }
    }
}
