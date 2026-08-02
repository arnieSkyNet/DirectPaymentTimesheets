use std::error::Error;

use rusqlite::Connection;

use crate::database;
use crate::import_service::ImportService;
use crate::repository::TimesheetRepository;

pub fn run() -> Result<(), Box<dyn Error>> {

let context = crate::context::AppContext::initialise()?;

    println!(
        "DirectPaymentTimesheets v{}",
        context.version
    );
        
    println!("Application foundation ready.");

    println!(
        "Application data directory: {:?}",
        context.environment.data_dir
    );

    database::initialise_database(
        &context.environment.database_path
    )?;

    let connection = Connection::open(
        &context.environment.database_path
    )?;

    let repository = TimesheetRepository::new(connection);

    let import_dir =
        crate::paths::expand_path(
            &context.config.folders.csv_import
        );

    let service = ImportService::new(
        &repository,
        import_dir,
        &context.environment.archive_dir,
    );

    let summary = service.run()?;

    println!("Import complete.");
    println!("Files processed: {}", summary.files_processed);
    println!("Rows processed: {}", summary.rows_processed);
    println!("Rows imported: {}", summary.rows_imported);
    println!("Rows skipped: {}", summary.rows_skipped);
    println!("Files failed: {}", summary.files_failed);

    Ok(())
}
