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
}

pub fn file_digest(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

// Share the email repository connection so mixed bundles transition atomically.
impl PayrollTimesheetEmailRepository {
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
        let mut statement = self.connection.prepare("SELECT id, personal_assistant_id, document_type, stored_path, sha256, document_year, sent_at
            FROM imported_payroll_documents WHERE personal_assistant_id = ?1 ORDER BY id")?;
        let rows = statement.query_map([pa], |row| {
            let sent_at: Option<String> = row.get(6)?;
            Ok(PayrollDocument {
                id: row.get(0)?,
                personal_assistant_id: row.get(1)?,
                document_type: row.get(2)?,
                path: PathBuf::from(row.get::<_, String>(3)?),
                sha256: row.get(4)?,
                document_year: row.get(5)?,
                delivery_state: delivery_state(sent_at.as_deref()),
            })
        })?;
        rows.collect()
    }

    pub fn transition_payroll_bundle(
        &self,
        pa: i64,
        identity: Option<&crate::payslip_delivery_service::PayslipDeliveryIdentity<'_>>,
        payslip: bool,
        documents: &[i64],
        expected: Option<&str>,
        next: Option<&str>,
    ) -> rusqlite::Result<bool> {
        if !payslip && documents.is_empty() {
            return Ok(false);
        }
        let transaction = self.connection.unchecked_transaction()?;
        if payslip {
            let Some(identity) = identity.filter(|i| i.personal_assistant_id == pa) else {
                return Ok(false);
            };
            transaction.execute("INSERT OR IGNORE INTO payroll_timesheet_email_status
                (personal_assistant_id, payroll_year, cycle_number, email_type, sent_at) VALUES (?1, ?2, ?3, 'payslip', NULL)",
                params![identity.personal_assistant_id, identity.payroll_year, identity.cycle_number])?;
            if transaction.execute("UPDATE payroll_timesheet_email_status SET sent_at = ?1
                WHERE personal_assistant_id = ?2 AND payroll_year = ?3 AND cycle_number = ?4 AND email_type = 'payslip' AND sent_at IS ?5",
                params![next, identity.personal_assistant_id, identity.payroll_year, identity.cycle_number, expected])? != 1 { return Ok(false); }
        }
        let mut seen = std::collections::HashSet::new();
        for id in documents {
            if !seen.insert(id)
                || transaction.execute(
                    "UPDATE imported_payroll_documents SET sent_at = ?1
                WHERE id = ?2 AND personal_assistant_id = ?3 AND sent_at IS ?4",
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
        let connection = Connection::open(&path).unwrap();
        create_schema(&connection).unwrap();
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
            31
        );
        drop(connection);
        let reopened = Connection::open(path).unwrap();
        create_schema(&reopened).unwrap();
        assert_eq!(statuses(&reopened), before);
        assert_eq!(CURRENT_SCHEMA_VERSION, 31);
    }

    #[test]
    fn schema31_migration_is_atomic_on_version_write_failure() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
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
        let connection = Connection::open_in_memory().unwrap();
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
}
