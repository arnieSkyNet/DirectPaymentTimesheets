mod config;
mod models;
mod error;

use models::TimesheetEntry;

fn main() {
    println!("DirectPaymentTimesheets v0.0.1");
    println!("Application foundation ready.");

    let entry = TimesheetEntry {
        id: 1,
        pa_name: String::from("Example PA"),
        date: String::from("2026-07-31"),
        start_time: String::from("09:00"),
        end_time: String::from("17:00"),
        break_minutes: 60,
        notes: Some(String::from("Example entry")),
    };

    println!("{:?}", entry);
}

