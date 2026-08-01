use std::path::Path;

use rusqlite::{Connection, Result};

pub fn initialise_database(database_path: &Path) -> Result<()> {
    let connection = Connection::open(database_path)?;

    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS timesheets (
            id INTEGER PRIMARY KEY,
            pa_name TEXT NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            break_minutes INTEGER NOT NULL,
            worked_minutes INTEGER NOT NULL,
            hourly_rate REAL NOT NULL,
            amount REAL NOT NULL,
            notes TEXT
        )
        ",
        [],
    )?;

    println!("Database initialised.");

    Ok(())
}
