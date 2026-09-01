use rusqlite::{params, Connection, Result};

use crate::models::TimesheetEntry;

pub struct TimesheetRepository {
    connection: Connection,
}

impl TimesheetRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    #[cfg(test)]
    pub fn insert(&self, entry: &TimesheetEntry) -> Result<()> {
        let personal_assistant_id: Option<i64> = self
            .connection
            .query_row(
                "
                SELECT id
                FROM personal_assistants
                WHERE first_name || ' ' || surname = ?1
                LIMIT 1
                ",
                params![&entry.pa_name],
                |row| row.get(0),
            )
            .ok();

        self.connection.execute(
            "INSERT INTO timesheets (
                pa_name,
                personal_assistant_id,
                start_time,
                end_time,
                break_minutes,
                worked_minutes,
                hourly_rate,
                amount,
                notes
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &entry.pa_name,
                personal_assistant_id,
                &entry.start_time,
                &entry.end_time,
                &entry.break_minutes,
                &entry.worked_minutes,
                &entry.hourly_rate,
                &entry.amount,
                &entry.notes,
            ],
        )?;

        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<TimesheetEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT
                id,
                pa_name,
                personal_assistant_id,
                start_time,
                end_time,
                break_minutes,
                worked_minutes,
                hourly_rate,
                amount,
                notes
            FROM timesheets",
        )?;

        let timesheets = statement.query_map([], |row| {
            Ok(TimesheetEntry {
                id: row.get(0)?,
                pa_name: row.get(1)?,
                personal_assistant_id: row.get(2)?,
                start_time: row.get(3)?,
                end_time: row.get(4)?,
                break_minutes: row.get(5)?,
                worked_minutes: row.get(6)?,
                hourly_rate: row.get(7)?,
                amount: row.get(8)?,
                notes: row.get(9)?,
            })
        })?;

        let mut entries = Vec::new();

        for timesheet in timesheets {
            entries.push(timesheet?);
        }

        Ok(entries)
    }

    pub fn resolve_personal_assistant_ids(&self, imported_name: &str) -> Result<Vec<i64>> {
        let wanted = normalize_person_name(imported_name);
        let mut statement = self
            .connection
            .prepare("SELECT id, first_name, surname FROM personal_assistants ORDER BY id")?;
        let candidates = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut matches = Vec::new();
        for candidate in candidates {
            let (id, first_name, surname) = candidate?;
            if normalize_person_name(&format!("{first_name} {surname}")) == wanted {
                matches.push(id);
            }
        }
        Ok(matches)
    }

    pub fn import_file_atomically(
        &self,
        entries: &[TimesheetEntry],
        import_time: &str,
        original_filename: &str,
        archive_filename: &str,
        rows_processed: i64,
        rows_skipped: i64,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        for entry in entries {
            let personal_assistant_id = entry.personal_assistant_id.ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "CSV import requires a uniquely resolved personal_assistant_id".to_string(),
                )
            })?;
            transaction.execute(
                "INSERT INTO timesheets (
                    pa_name, personal_assistant_id, start_time, end_time,
                    break_minutes, worked_minutes, hourly_rate, amount, notes
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entry.pa_name,
                    personal_assistant_id,
                    entry.start_time,
                    entry.end_time,
                    entry.break_minutes,
                    entry.worked_minutes,
                    entry.hourly_rate,
                    entry.amount,
                    entry.notes,
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO import_audit (
                import_time, original_filename, archive_filename, rows_processed,
                rows_imported, rows_skipped, status, error_message
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'SUCCESS', NULL)",
            params![
                import_time,
                original_filename,
                archive_filename,
                rows_processed,
                entries.len() as i64,
                rows_skipped,
            ],
        )?;
        transaction.commit()
    }

    pub fn add_import_audit(
        &self,
        import_time: &str,
        original_filename: &str,
        archive_filename: &str,
        rows_processed: i64,
        rows_imported: i64,
        rows_skipped: i64,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<()> {
        self.connection.execute(
            "
            INSERT INTO import_audit (
                import_time,
                original_filename,
                archive_filename,
                rows_processed,
                rows_imported,
                rows_skipped,
                status,
                error_message
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                import_time,
                original_filename,
                archive_filename,
                rows_processed,
                rows_imported,
                rows_skipped,
                status,
                error_message,
            ],
        )?;

        Ok(())
    }

    pub fn has_successful_import(&self, filename: &str) -> Result<bool> {
        let mut statement = self.connection.prepare(
            "
            SELECT COUNT(*)
            FROM import_audit
            WHERE original_filename = ?1
            AND status = 'SUCCESS'
            ",
        )?;

        let count: i64 = statement.query_row(params![filename], |row| row.get(0))?;

        Ok(count > 0)
    }
}

fn normalize_person_name(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;

    fn create_test_repository() -> TimesheetRepository {
        let connection = Connection::open_in_memory().unwrap();

        create_schema(&connection).unwrap();

        TimesheetRepository::new(connection)
    }

    fn test_entry() -> TimesheetEntry {
        TimesheetEntry {
            id: 0,
            pa_name: "Test PA".to_string(),
            personal_assistant_id: None,
            start_time: "09:00".to_string(),
            end_time: "17:00".to_string(),
            break_minutes: 30,
            worked_minutes: 450,
            hourly_rate: 15.0,
            amount: 112.5,
            notes: None,
        }
    }

    #[test]
    fn insert_and_get_timesheet() {
        let repository = create_test_repository();

        repository.insert(&test_entry()).unwrap();

        let entries = repository.get_all().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pa_name, "Test PA");
    }

    #[test]
    fn import_audit_is_recorded() {
        let repository = create_test_repository();

        repository
            .add_import_audit(
                "2026-01-01 12:00:00",
                "/test/file.csv",
                "/archive/file.csv",
                1,
                1,
                0,
                "SUCCESS",
                None,
            )
            .unwrap();

        assert!(repository.has_successful_import("/test/file.csv").unwrap());
    }
}
