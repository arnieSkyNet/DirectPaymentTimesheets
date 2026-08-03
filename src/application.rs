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

    initialise_database(&context)?;

    run_startup_import(&context)?;

    launch_gui()?;

    Ok(())
}


fn initialise_database(
    context: &crate::context::AppContext,
) -> Result<(), Box<dyn Error>> {

    database::initialise_database(
        &context.environment.database_path
    )?;

    Ok(())
}


fn run_startup_import(
    context: &crate::context::AppContext,
) -> Result<(), Box<dyn Error>> {

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
    println!(
        "Files processed: {}",
        summary.files_processed
    );
    println!(
        "Rows processed: {}",
        summary.rows_processed
    );
    println!(
        "Rows imported: {}",
        summary.rows_imported
    );
    println!(
        "Rows skipped: {}",
        summary.rows_skipped
    );
    println!(
        "Files failed: {}",
        summary.files_failed
    );

    Ok(())
}


fn launch_gui() -> Result<(), Box<dyn Error>> {

    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "DirectPaymentTimesheets",
        options,
        Box::new(|_cc| {
            Ok(Box::new(
                crate::gui::DirectPaymentApp::new()
            ))
        }),
    )
    .map_err(|error| -> Box<dyn Error> {
        Box::new(error)
    })?;

    Ok(())
}

