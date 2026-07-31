use rusqlite::{Connection, Result};

pub fn initialise_database() -> Result<()> {
    let connection = Connection::open("direct_payment_timesheets.db")?;

    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS timesheets (
            id INTEGER PRIMARY KEY,
            pa_name TEXT NOT NULL,
            date TEXT NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            break_minutes INTEGER NOT NULL,
            notes TEXT
        )
        ",
        [],
    )?;

    println!("Database initialised.");

    Ok(())
}

