mod config;
mod models;
mod error;
mod database;
mod repository;
mod csv_import;
mod environment;
mod context;


use repository::TimesheetRepository;
use rusqlite::Connection;

fn main() {
    println!("DirectPaymentTimesheets v0.0.1");
    println!("Application foundation ready.");

    let context = context::AppContext::initialise()
        .expect("Failed to initialise application context");

    println!(
        "Application data directory: {:?}",
        context.environment.data_dir
    );

    database::initialise_database(&context.environment.database_path)
        .expect("Failed to initialise database");

    let connection = Connection::open(&context.environment.database_path)
        .expect("Failed to open database");


    let repository = TimesheetRepository::new(connection);

    let csv_file = context.environment.import_dir.join("sample_timesheets.csv");

    let csv_entries = csv_import::import_csv(&csv_file)
        .expect("Failed to import CSV");


    for entry in csv_entries {
        if repository.exists(&entry)
            .expect("Failed to check existing timesheet")
        {
            println!("Skipping existing entry: {} {}", entry.pa_name, entry.start_time);
        } else {
            repository.insert(&entry)
                .expect("Failed to insert timesheet");

            println!("Imported: {} {}", entry.pa_name, entry.start_time);
        }
    }

    let entries = repository.get_all()
        .expect("Failed to read timesheets");

    println!("Database contains {} entries.", entries.len());

    for entry in entries {
        println!("{:?}", entry);
    }
}

