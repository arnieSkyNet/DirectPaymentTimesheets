use std::error::Error;

use crate::app::Application;
use crate::database;

pub fn run() -> Result<(), Box<dyn Error>> {
    let app = Application::initialise()?;

    println!("DirectPaymentTimesheets v{}", app.context.version);

    println!("Application foundation ready.");

    println!(
        "Application data directory: {:?}",
        app.context.environment.data_dir
    );

    initialise_database(&app)?;

    launch_gui(app)?;

    Ok(())
}

fn initialise_database(_app: &Application) -> Result<(), Box<dyn Error>> {
    database::initialise_database(&_app.context.environment.database_path)?;
    crate::historical_payroll_backfill::apply(&_app.context.environment.database_path)?;

    Ok(())
}

fn launch_gui(app: Application) -> Result<(), Box<dyn Error>> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Direct Payments Timesheets",
        options,
        Box::new(|cc| {
            crate::theme::apply_theme(&cc.egui_ctx, app.context.config.theme);
            Ok(Box::new(crate::gui::DirectPaymentApp::new(app)))
        }),
    )
    .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;

    Ok(())
}
