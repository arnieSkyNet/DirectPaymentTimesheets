//! Safe startup diagnostics: messages never contain raw config/parser/SQL errors.
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Environment,
    Configuration,
    Database,
    Backup,
    Migration,
    Lock,
    Repositories,
    Graphics,
}
impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Environment => "Application folders",
            Self::Configuration => "Configuration",
            Self::Database => "Database validation",
            Self::Backup => "Verified recovery backup",
            Self::Migration => "Database upgrade",
            Self::Lock => "Database/dispatch locking",
            Self::Repositories => "Opening application data",
            Self::Graphics => "Opening the application window",
        })
    }
}
#[derive(Debug, Clone)]
pub struct StartupError {
    pub stage: Stage,
    pub path: Option<PathBuf>,
    pub explanation: String,
    pub guidance: String,
}
impl StartupError {
    pub fn new(
        stage: Stage,
        path: Option<&Path>,
        explanation: impl Into<String>,
        guidance: impl Into<String>,
    ) -> Self {
        // Paths can be configured strings; don't expose credentials embedded in URLs/control text.
        let path = path
            .filter(|p| {
                let s = p.to_string_lossy();
                !s.contains("://") && !s.chars().any(char::is_control)
            })
            .map(Path::to_path_buf);
        Self {
            stage,
            path,
            explanation: explanation.into(),
            guidance: guidance.into(),
        }
    }
    pub fn io(stage: Stage, path: &Path, error: &std::io::Error) -> Self {
        Self::new(stage, Some(path), format!("The required location could not be accessed ({:?}).",error.kind()),
            "Check the selected location, permissions, free space and connected drives, then retry. Existing data must not be deleted or reset.")
    }
    pub fn database(path: &Path, error: &(dyn std::error::Error + 'static)) -> Self {
        let message = error.to_string(); // Classification only: never expose the underlying payload.
        let (stage, explanation, guidance) = if message.contains("Upgrade refused") {
            (Stage::Backup,"The verified recovery backup could not be completed. No database upgrade was applied.","Check backup storage and permissions, then retry. Preserve the original database and its WAL files.")
        } else if message.contains("Isolated upgrade/install failed") {
            (Stage::Migration,"The isolated upgrade or installation failed. Uncommitted installation changes were rolled back.","Close all instances and retain the verified original copy in the backups folder beside the database. Seek help before retrying or sending payroll.")
        } else if message.contains("Unsupported database schema")
            || message.contains("newer than supported")
        {
            (Stage::Database,"This database schema is incompatible with this application.","Use the matching or newer application. Do not downgrade, reset or replace the database.")
        } else if message.contains("another instance") || message.contains("database is locked") {
            (Stage::Lock,"Another operation currently holds a required database or dispatch lock.","Wait for sending, publication or recovery to finish, or close other instances, then retry. Never delete the lock file.")
        } else if message.contains("dispatch lock") {
            (Stage::Lock,"The dispatch lock could not be acquired because of a filesystem or locking-support error.","Check permissions and filesystem locking support. Use a location with reliable SQLite and OS locking; do not bypass or delete the lock.")
        } else {
            (Stage::Database,"The database could not be opened or safely validated.","Check permissions and available storage. If the database is damaged or incomplete, preserve it and its WAL files and use a verified recovery copy with assistance; never reset or delete it.")
        };
        Self::new(stage, Some(path), explanation, guidance)
    }
    pub fn after_database(self) -> Self {
        Self { guidance: format!("{} Database initialisation already completed; any successful upgrade remains applied. If an upgrade was needed, its verified original recovery copy is in the backups folder beside the database. This later failure does not undo an upgrade.",self.guidance), ..self }
    }
}
impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DirectPaymentTimesheets error.\nStage: {}\n", self.stage)?;
        if let Some(path) = &self.path {
            writeln!(f, "Location: {}", path.display())?;
        }
        write!(f, "{}\n\n{}", self.explanation, self.guidance)
    }
}
impl std::error::Error for StartupError {}

/// Always retain stderr even when a dialog succeeds; a desktop may hide stderr.
/// Injection makes presentation/fallback tests independent of desktop services.
fn report_with(
    error: &StartupError,
    mut graphical: impl FnMut(&str) -> bool,
    mut stderr: impl FnMut(&str),
) {
    let text = format!("DirectPaymentTimesheets could not start.\n{error}");
    stderr(&text);
    if !graphical(&text) {
        stderr("A graphical error dialog could not be opened. Read the startup explanation above; no main application window was opened.");
    }
}
pub fn report(error: &StartupError) {
    report_with(
        error,
        |text| present_with(text, error.stage, native_dialog, lightweight_dialog),
        |s| eprintln!("{s}"),
    );
}
pub fn exit_code(failed: bool) -> u8 {
    if failed {
        1
    } else {
        0
    }
}

fn present_with(
    text: &str,
    stage: Stage,
    mut native: impl FnMut(&str) -> bool,
    mut lightweight: impl FnMut(&str) -> bool,
) -> bool {
    native(text) || (stage != Stage::Graphics && lightweight(text))
}
fn lightweight_dialog(text: &str) -> bool {
    struct ErrorWindow(String);
    impl eframe::App for ErrorWindow {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Startup stopped safely");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(&self.0);
                });
                if ui.button("Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
    }
    let text = text.to_string();
    eframe::run_native(
        "DirectPaymentTimesheets startup error",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 420.0]),
            ..Default::default()
        },
        Box::new(move |_| Ok(Box::new(ErrorWindow(text)))),
    )
    .is_ok()
}

#[cfg(target_os = "windows")]
fn native_dialog(text: &str) -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            hwnd: *mut std::ffi::c_void,
            text: *const u16,
            title: *const u16,
            kind: u32,
        ) -> i32;
    }
    let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let title: Vec<u16> = "DirectPaymentTimesheets startup error"
        .encode_utf16()
        .chain([0])
        .collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10) != 0 }
}
#[cfg(target_os = "macos")]
fn native_dialog(text: &str) -> bool {
    std::process::Command::new("osascript").args(["-e","on run argv\ndisplay alert \"DirectPaymentTimesheets startup error\" message (item 1 of argv) as critical buttons {\"OK\"}\nend run",text]).output().is_ok_and(|r|r.status.success())
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn native_dialog(text: &str) -> bool {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return false;
    }
    for (command, args) in [
        (
            "zenity",
            vec![
                "--error",
                "--title=DirectPaymentTimesheets startup error",
                "--no-markup",
                "--text",
                text,
            ],
        ),
        (
            "kdialog",
            vec![
                "--title",
                "DirectPaymentTimesheets startup error",
                "--error",
                text,
            ],
        ),
    ] {
        if std::process::Command::new(command)
            .args(args)
            .output()
            .is_ok_and(|r| r.status.success())
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod stage5_tests {
    use super::*;
    #[test]
    fn graphical_and_fallback_diagnostics_and_nonzero_status() {
        let e = StartupError::new(
            Stage::Graphics,
            None,
            "The window could not open.",
            "Check desktop graphics support.",
        );
        for success in [true, false] {
            let mut calls = 0;
            let mut messages = Vec::new();
            report_with(
                &e,
                |text| {
                    calls += 1;
                    assert!(text.contains("Stage:"));
                    success
                },
                |s| messages.push(s.to_string()),
            );
            assert_eq!(calls, 1);
            assert_eq!(messages.len(), if success { 1 } else { 2 });
        }
        assert_eq!(exit_code(true), 1);
        assert_eq!(exit_code(false), 0);
    }
    #[test]
    fn diagnostics_do_not_expose_underlying_secrets_or_url_paths() {
        let e = std::io::Error::other("smtp_password='SECRET' token=TOKEN");
        let message = StartupError::database(Path::new("database.sqlite"), &e).to_string();
        assert!(!message.contains("SECRET"));
        assert!(!message.contains("TOKEN"));
        assert!(StartupError::io(
            Stage::Configuration,
            Path::new("https://name:SECRET@host"),
            &e
        )
        .path
        .is_none());
    }
    #[test]
    fn later_failures_identify_completed_database_initialisation() {
        let message =
            StartupError::new(Stage::Configuration, None, "Invalid config.", "Repair it.")
                .after_database()
                .to_string();
        assert!(message.contains("already completed"));
        assert!(message.contains("remains applied"));
    }
}

#[cfg(test)]
mod stage5_process_tests {
    #[test]
    fn stage5_isolated_failure_process_fixture() {
        if std::env::var_os("DPT_STAGE5_FAILURE_CHILD").is_none() {
            return;
        }
        let result = crate::app::Application::initialise();
        let error = result
            .err()
            .expect("malformed isolated configuration must fail");
        eprintln!("{error}");
        std::process::exit(super::exit_code(true).into());
    }
    #[test]
    fn stage5_real_startup_failure_exits_nonzero_without_main_gui_or_sensitive_output() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            b"password='PRIVATE_SECRET'\nbroken=[",
        )
        .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "startup::stage5_process_tests::stage5_isolated_failure_process_fixture",
                "--nocapture",
            ])
            .env("DIRECTPAYMENTTIMESHEETS_HOME", dir.path())
            .env("DPT_STAGE5_FAILURE_CHILD", "yes")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(text.contains("Configuration"));
        assert!(!text.contains("PRIVATE_SECRET"));
        assert!(dir.path().join("database.sqlite").exists());
    }
}

#[cfg(test)]
mod stage5_presentation_tests {
    #[test]
    fn stage5_native_failure_uses_lightweight_window_except_after_graphics_failure() {
        let mut fallback = 0;
        assert!(super::present_with(
            "safe diagnostic",
            super::Stage::Configuration,
            |_| false,
            |_| {
                fallback += 1;
                true
            }
        ));
        assert_eq!(fallback, 1);
        assert!(super::present_with(
            "safe diagnostic",
            super::Stage::Database,
            |_| true,
            |_| {
                fallback += 1;
                true
            }
        ));
        assert_eq!(fallback, 1);
        assert!(!super::present_with(
            "safe diagnostic",
            super::Stage::Graphics,
            |_| false,
            |_| {
                fallback += 1;
                true
            }
        ));
        assert_eq!(fallback, 1);
    }
}
