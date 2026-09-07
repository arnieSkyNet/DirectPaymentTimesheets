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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
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
