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

    initialise_database(&app.context.environment.database_path)?;

    launch_gui(app)?;

    Ok(())
}

fn initialise_database(database_path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    database::initialise_database(database_path)?;

    Ok(())
}

fn launch_gui(app: Application) -> Result<(), Box<dyn Error>> {
    let application_icon = eframe::icon_data::from_png_bytes(include_bytes!(
        "../assets/direct-payment-timesheets.png"
    ))?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(application_icon)
            .with_inner_size([1000.0, 800.0])
            .with_clamp_size_to_monitor_size(true),
        ..Default::default()
    };

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

// egui exposes monitor dimensions, not the desktop work area. Leave room for
// decorations and desktop panels; apply only once before normal interaction.
pub(crate) fn initial_window_size(monitor: Option<egui::Vec2>) -> Option<egui::Vec2> {
    let monitor = monitor?;
    if !monitor.is_finite() || monitor.x <= 0.0 || monitor.y <= 0.0 {
        return None;
    }
    Some(egui::vec2(
        1000.0_f32.min(monitor.x * 0.9),
        monitor.y * 0.85,
    ))
}

#[cfg(test)]
mod window_tests {
    #[test]
    fn initial_size_leaves_monitor_margin_and_handles_missing_dimensions() {
        assert_eq!(super::initial_window_size(None), None);
        assert_eq!(
            super::initial_window_size(Some(egui::vec2(0.0, 768.0))),
            None
        );
        for monitor in [egui::vec2(1024.0, 768.0), egui::vec2(1920.0, 1080.0)] {
            let size = super::initial_window_size(Some(monitor)).unwrap();
            assert!(size.x < monitor.x && size.y < monitor.y);
            assert_eq!(size.y, monitor.y * 0.85);
        }
    }
}

#[cfg(test)]
mod database_tests {
    use super::initialise_database;
    use rusqlite::Connection;

    #[test]
    fn startup_initialises_a_fresh_database_without_historical_source_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("database.sqlite");
        initialise_database(&path).unwrap();
        let connection = Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, crate::database::CURRENT_SCHEMA_VERSION);
        for table in [
            "personal_assistants",
            "payroll_schedules",
            "payroll_timesheets",
            "payroll_timesheet_manual_adjustments",
            "payroll_timesheet_public_holidays",
        ] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn repeated_startup_preserves_a_lone_legacy_adjustment_without_repair_guards() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("database.sqlite");
        initialise_database(&path).unwrap();
        let connection = Connection::open(&path).unwrap();
        connection.execute(
            "INSERT INTO personal_assistants (id, first_name, surname) VALUES (1, 'Test', 'Assistant')",
            [],
        ).unwrap();
        connection.execute(
            "INSERT INTO payroll_timesheets (id, personal_assistant_id, payroll_year, cycle_number, created_at, updated_at)
             VALUES (1, 1, '2030/31', 1, 'test', 'test')",
            [],
        ).unwrap();
        // A single retained row used to trigger the retired partial-repair guard.
        connection
            .execute(
                "INSERT INTO payroll_timesheet_manual_adjustments
             (payroll_timesheet_id, week_number, adjustment_minutes, reason, updated_at)
             VALUES (1, 1, 17, 'Verified historical payroll backfill (2026-09-04)', 'test')",
                [],
            )
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        initialise_database(&path).unwrap();
        initialise_database(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
