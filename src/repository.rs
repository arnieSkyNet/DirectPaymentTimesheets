use rusqlite::{Connection, Result};

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
            "
            INSERT INTO timesheets
            (pa_name, date, start_time, end_time, break_minutes, notes)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            (
                &entry.pa_name,
                &entry.date,
                &entry.start_time,
                &entry.end_time,
                entry.break_minutes,
                &entry.notes,
            ),
        )?;

        Ok(())
    }
}

