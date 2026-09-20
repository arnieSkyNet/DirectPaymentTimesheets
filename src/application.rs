use std::error::Error;

use crate::app::Application;
use crate::database;

// Both package formats use the same executable entry point. Keep this policy
// pure so tests never mutate the process environment while running in parallel.
pub(crate) fn use_packaged_arm_software_rendering(
    os: &str,
    arch: &str,
    installation_kind: Option<&str>,
    explicitly_configured: bool,
) -> bool {
    os == "linux"
        && matches!(arch, "arm" | "aarch64")
        && matches!(installation_kind, Some("deb" | "appimage"))
        && !explicitly_configured
}

#[cfg(test)]
mod rendering_tests {
    #[test]
    fn software_default_is_limited_to_both_arm_linux_package_kinds() {
        for os in ["linux", "windows", "macos"] {
            for arch in ["arm", "aarch64", "x86_64", "x86"] {
                for kind in [
                    None,
                    Some("source"),
                    Some("deb"),
                    Some("appimage"),
                    Some("windows"),
                    Some("macos"),
                    Some("unknown"),
                ] {
                    let expected = os == "linux"
                        && matches!(arch, "arm" | "aarch64")
                        && matches!(kind, Some("deb" | "appimage"));
                    assert_eq!(
                        super::use_packaged_arm_software_rendering(os, arch, kind, false),
                        expected
                    );
                    assert!(!super::use_packaged_arm_software_rendering(
                        os, arch, kind, true
                    ));
                }
            }
        }
    }
}

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
            .with_inner_size(DEFAULT_WINDOW_SIZE)
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

const DEFAULT_WINDOW_SIZE: egui::Vec2 = egui::vec2(1000.0, 800.0);

// egui exposes monitor dimensions, not the desktop work area. Leave room for
// decorations and panels, but never resize to transient startup geometry. This
// is a startup-policy floor, not a minimum size imposed on later user resizing.
pub(crate) fn initial_window_size(monitor: Option<egui::Vec2>) -> egui::Vec2 {
    let Some(monitor) = monitor else {
        return DEFAULT_WINDOW_SIZE;
    };
    if !monitor.is_finite() || monitor.x <= 0.0 || monitor.y <= 0.0 {
        return DEFAULT_WINDOW_SIZE;
    }
    let size = egui::vec2(1000.0_f32.min(monitor.x * 0.9), monitor.y * 0.85);
    if size.x < 320.0 || size.y < 240.0 {
        return DEFAULT_WINDOW_SIZE;
    }
    size
}

#[cfg(test)]
mod window_tests {
    #[test]
    fn startup_unavailable_invalid_or_tiny_geometry_uses_sensible_default() {
        assert_eq!(super::initial_window_size(None), super::DEFAULT_WINDOW_SIZE);
        for monitor in [
            egui::vec2(0.0, 768.0),
            egui::vec2(1024.0, 0.0),
            egui::vec2(-1.0, 768.0),
            egui::vec2(f32::NAN, 768.0),
            egui::vec2(1024.0, f32::INFINITY),
            egui::vec2(1.0, 1.0),
            egui::vec2(0.5, 0.5),
            egui::vec2(1.0, 768.0),
            egui::vec2(1024.0, 1.0),
            egui::vec2(320.0, 240.0),
        ] {
            assert_eq!(
                super::initial_window_size(Some(monitor)),
                super::DEFAULT_WINDOW_SIZE
            );
        }
    }

    #[test]
    fn usable_geometry_preserves_existing_monitor_relative_sizing() {
        for monitor in [
            egui::vec2(640.0, 480.0),
            egui::vec2(800.0, 600.0),
            egui::vec2(1024.0, 768.0),
            egui::vec2(1920.0, 1080.0),
        ] {
            let size = super::initial_window_size(Some(monitor));
            assert_eq!(
                size,
                egui::vec2(1000.0_f32.min(monitor.x * 0.9), monitor.y * 0.85)
            );
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
