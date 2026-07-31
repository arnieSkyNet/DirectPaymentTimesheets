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

pub fn get_all(&self) -> Result<Vec<TimesheetEntry>> {
    let mut statement = self.connection.prepare(
        "
        SELECT id, pa_name, date, start_time, end_time, break_minutes, notes
        FROM timesheets
        "
    )?;

    let entries = statement.query_map([], |row| {
        Ok(TimesheetEntry {
            id: row.get(0)?,
            pa_name: row.get(1)?,
            date: row.get(2)?,
            start_time: row.get(3)?,
            end_time: row.get(4)?,
            break_minutes: row.get(5)?,
            notes: row.get(6)?,
        })
    })?;

    let mut results = Vec::new();

    for entry in entries {
        results.push(entry?);
    }

    Ok(results)
}

}

