use std::path::Path;

use rusqlite::{Connection, Result};

pub fn initialise_database(database_path: &Path) -> Result<()> {
    let connection = Connection::open(database_path)?;

    create_schema(&connection)?;

    println!("Database initialised.");

    Ok(())
}

pub fn create_schema(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        )
        ",
        [],
    )?;

    let version_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))?;

    if version_count == 0 {
        connection.execute("INSERT INTO schema_version (version) VALUES (1)", [])?;
    }

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

    Ok(())
}
