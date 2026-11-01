mod config;
mod models;
mod error;
mod database;
mod repository;
mod csv_import;
mod environment;
mod context;
mod paths;
mod archive;
mod import_service;

use repository::TimesheetRepository;
use import_service::ImportService;
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

    let import_dir = paths::expand_path(&context.config.folders.csv_import);

    let import_service = ImportService::new(
        &repository,
        import_dir,
        &context.environment.archive_dir,
    );

    match import_service.run() {
        Ok(summary) => {
            println!("Import complete.");
            println!("Files processed: {}", summary.files_processed);
            println!("Rows processed: {}", summary.rows_processed);
            println!("Rows imported: {}", summary.rows_imported);
            println!("Rows skipped: {}", summary.rows_skipped);
            println!("Files failed: {}", summary.files_failed);
        }

        Err(error) => {
            println!("Import service failed: {}", error);
        }
    }

    let entries = repository.get_all()
        .expect("Failed to read timesheets");

    println!("Database contains {} entries.", entries.len());

    for entry in entries {
        println!("{:?}", entry);
    }
}

