use std::error::Error;

use crate::app::Application;
use crate::database;

pub fn run() -> Result<(), Box<dyn Error>> {

    let app = Application::initialise()?;

    println!(
        "DirectPaymentTimesheets v{}",
        app.context.version
    );

    println!("Application foundation ready.");

    println!(
        "Application data directory: {:?}",
        app.context.environment.data_dir
    );

    initialise_database(&app)?;

    run_startup_import(&app)?;

    launch_gui(app)?;

    Ok(())
}


fn initialise_database(
    _app: &Application,
) -> Result<(), Box<dyn Error>> {

    database::initialise_database(
        &_app.context.environment.database_path
    )?;

    Ok(())
}


fn run_startup_import(
    app: &Application,
) -> Result<(), Box<dyn Error>> {

    let service = app.create_import_service();

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

fn launch_gui(
    app: Application,
    ) -> Result<(), Box<dyn Error>> {
    
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "DirectPaymentTimesheets",
        options,
        Box::new(|_cc| {
            Ok(Box::new(
                crate::gui::DirectPaymentApp::new(app)
            ))
        }),
    )
    .map_err(|error| -> Box<dyn Error> {
        Box::new(error)
    })?;

    Ok(())
}
