use rusqlite::{params, Connection, Result};

use crate::models::TimesheetEntry;

pub struct TimesheetRepository {
    connection: Connection,
}

impl TimesheetRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn insert(&self, entry: &TimesheetEntry) -> Result<()> {
        self.connection.execute(
            "INSERT INTO timesheets (
                pa_name,
                start_time,
                end_time,
                break_minutes,
                worked_minutes,
                hourly_rate,
                amount,
                notes
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &entry.pa_name,
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
                start_time: row.get(2)?,
                end_time: row.get(3)?,
                break_minutes: row.get(4)?,
                worked_minutes: row.get(5)?,
                hourly_rate: row.get(6)?,
                amount: row.get(7)?,
                notes: row.get(8)?,
            })
        })?;

        let mut entries = Vec::new();

        for timesheet in timesheets {
            entries.push(timesheet?);
        }

        Ok(entries)
    }

    pub fn exists(&self, entry: &TimesheetEntry) -> Result<bool> {
        let mut statement = self.connection.prepare(
            "SELECT COUNT(*)
             FROM timesheets
             WHERE pa_name = ?1
             AND start_time = ?2
             AND end_time = ?3",
        )?;

        let count: i64 = statement.query_row(
            params![&entry.pa_name, &entry.start_time, &entry.end_time,],
            |row| row.get(0),
        )?;

        Ok(count > 0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;
    use crate::models::TimesheetEntry;

    fn create_test_repository() -> TimesheetRepository {
        let connection = Connection::open_in_memory().unwrap();

        create_schema(&connection).unwrap();

        TimesheetRepository::new(connection)
    }

    fn test_entry() -> TimesheetEntry {
        TimesheetEntry {
            id: 0,
            pa_name: "Test PA".to_string(),
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

        let entry = test_entry();

        repository.insert(&entry).unwrap();

        let entries = repository.get_all().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pa_name, "Test PA");
    }

    #[test]
    fn duplicate_timesheet_is_detected() {
        let repository = create_test_repository();

        let entry = test_entry();

        repository.insert(&entry).unwrap();

        assert!(repository.exists(&entry).unwrap());
    }

    #[test]
    fn successful_import_is_detected() {
        let repository = create_test_repository();

        repository
            .add_import_audit(
                "2026-08-02 12:00:00",
                "test.csv",
                "archive/test.csv",
                4,
                4,
                0,
                "SUCCESS",
                None,
            )
            .unwrap();

        assert!(repository.has_successful_import("test.csv").unwrap());
    }
}
