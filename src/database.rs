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

    apply_migrations(connection)?;

    Ok(())
}

fn apply_migrations(connection: &Connection) -> Result<()> {
    let current_version: i64 =
        connection.query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get(0)
        })?;

    if current_version < 2 {
        migrate_to_version_2(connection)?;
    }

    Ok(())
}

fn migrate_to_version_2(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS employers (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            address TEXT,
            postcode TEXT,
            telephone TEXT,
            email TEXT,
            payroll_provider TEXT,
            payroll_provider_address TEXT,
            payroll_provider_phone TEXT,
            employer_signature TEXT,
            default_pdf_template TEXT
        )
        ",
        [],
    )?;

    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS personal_assistants (
            id INTEGER PRIMARY KEY,
            first_name TEXT NOT NULL,
            surname TEXT NOT NULL,
            date_of_birth TEXT,
            national_insurance_number TEXT,
            address TEXT,
            postcode TEXT,
            telephone TEXT,
            email TEXT,
            employment_status TEXT
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 2", [])?;

    Ok(())
}
