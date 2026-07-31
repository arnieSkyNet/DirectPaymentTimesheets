mod config;
mod models;
mod error;
mod database;
mod repository;

use models::TimesheetEntry;
use repository::TimesheetRepository;
use rusqlite::Connection;

fn main() {
    println!("DirectPaymentTimesheets v0.0.1");
    println!("Application foundation ready.");

    database::initialise_database()
        .expect("Failed to initialise database");

    let connection = Connection::open("direct_payment_timesheets.db")
        .expect("Failed to open database");

    let repository = TimesheetRepository::new(connection);

    let entry = TimesheetEntry {
        id: 0,
        pa_name: String::from("Example PA"),
        start_time: String::from("2026-07-31 09:00:00"),
        end_time: String::from("2026-07-31 15:31:54"),
        break_minutes: 0,
        worked_minutes: 405,
        hourly_rate: 12.21,
        amount: 82.42,
        notes: Some(String::from("First database entry")),
    };

    let mut entries = repository.get_all()
        .expect("Failed to read timesheets");

    if entries.is_empty() {
        repository.insert(&entry)
            .expect("Failed to insert timesheet");

        println!("Timesheet saved.");

        entries = repository.get_all()
            .expect("Failed to read timesheets");
    } else {
        println!("Existing timesheets found. No new entry added.");
    }

    for entry in entries {
        println!("{:?}", entry);
    }
}
