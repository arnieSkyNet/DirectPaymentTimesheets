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

    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS import_audit (
            id INTEGER PRIMARY KEY,
            import_time TEXT NOT NULL,
            original_filename TEXT NOT NULL,
            archive_filename TEXT NOT NULL,
            rows_processed INTEGER NOT NULL,
            rows_imported INTEGER NOT NULL,
            rows_skipped INTEGER NOT NULL,
            status TEXT NOT NULL,
            error_message TEXT
        )
        ",
        [],
    )?;

    let column_exists: bool = connection
        .prepare("PRAGMA table_info(import_audit)")?
        .query_map([], |row| {
            let name: String = row.get(1)?;
            Ok(name == "error_message")
        })?
        .any(|result| result.unwrap_or(false));

    if !column_exists {
        connection.execute("ALTER TABLE import_audit ADD COLUMN error_message TEXT", [])?;
    }

    println!("Database initialised.");

    Ok(())
}
