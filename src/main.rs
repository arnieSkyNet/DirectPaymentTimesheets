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
        date: String::from("2026-07-31"),
        start_time: String::from("09:00"),
        end_time: String::from("17:00"),
        break_minutes: 60,
        notes: Some(String::from("First database entry")),
    };

    repository.insert(&entry)
        .expect("Failed to insert timesheet");

    println!("Timesheet saved.");
}

