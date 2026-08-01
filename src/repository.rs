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
             AND end_time = ?3"
        )?;

        let count: i64 = statement.query_row(
            params![
                &entry.pa_name,
                &entry.start_time,
                &entry.end_time,
            ],
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
                status
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                import_time,
                original_filename,
                archive_filename,
                rows_processed,
                rows_imported,
                rows_skipped,
                status,
            ],
        )?;

        Ok(())
    }
}
