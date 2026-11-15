use std::path::{Path, PathBuf};

pub fn expand_path(path: &PathBuf) -> PathBuf {
    expand_path_with_home(path, dirs::home_dir().as_deref())
}

#[cfg(test)]
pub fn ensure_directories(paths: &[&PathBuf]) -> std::io::Result<()> {
    for path in paths {
        std::fs::create_dir_all(expand_path(path))?;
    }

    Ok(())
}

pub fn expand_path_with_home(path: &Path, home: Option<&Path>) -> PathBuf {
    let path_string = path.to_string_lossy();

    if path_string == "~" {
        if let Some(home) = home {
            return home.to_path_buf();
        }
    } else if let Some(remainder) = path_string.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(remainder);
        }
    }

    path.to_path_buf()
}

/// Startup probes are read-only. Workflows recheck their own required inputs and
/// perform late, fallible output creation; an unrelated folder never gates payroll.
fn business_folders(config: &crate::config::AppConfig) -> Vec<(&'static str, PathBuf)> {
    [
        ("CSV import", &config.folders.csv_import),
        ("PDF output", &config.folders.pdf_output),
        ("Email archive", &config.folders.email_archive),
        ("Payslips", &config.folders.payslip_folder),
        (
            "Payroll information",
            &config.folders.payroll_information_folder,
        ),
    ]
    .into_iter()
    .map(|(name, path)| (name, expand_path(path)))
    .collect()
}
#[cfg(test)]
pub fn business_folder_warnings(config: &crate::config::AppConfig) -> Vec<String> {
    probe_folders(business_folders(config))
}
fn probe_folders(folders: Vec<(&str, PathBuf)>) -> Vec<String> {
    use crate::startup::{Stage, StartupError};
    folders.into_iter().filter_map(|(name,expanded)| {
        match std::fs::read_dir(&expanded) {
            Ok(_) => None,
            Err(error) => Some(StartupError::new(Stage::Environment,Some(&expanded),format!("{name} folder is currently unavailable ({:?}).",error.kind()),"The configured path is retained. Reconnect the drive or correct it in Settings. Only operations needing this location may fail; no alternate folder will be substituted.").to_string().replace("DirectPaymentTimesheets error.\n","Folder warning\n")),
        }
    }).collect()
}
/// Folder metadata on disconnected shares can stall in the OS. Never wait for
/// it on the GUI/startup thread; these workers perform no filesystem writes.
#[derive(Default)]
pub struct FolderCheck {
    paths: Vec<(&'static str, PathBuf)>,
    pending: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    pub warnings: Vec<String>,
}
impl FolderCheck {
    pub fn new(config: &crate::config::AppConfig) -> Self {
        let mut check = Self::default();
        check.restart(config);
        check
    }
    pub fn restart(&mut self, config: &crate::config::AppConfig) {
        self.paths = business_folders(config);
        let folders = self.paths.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.warnings=vec!["Checking configured business folders in the background. Settings and unrelated payroll work remain available.".into()];
        match std::thread::Builder::new()
            .name("business-folder-check".into())
            .spawn(move || {
                let _ = sender.send(probe_folders(folders));
            }) {
            Ok(_) => self.pending = Some(receiver),
            Err(_) => {
                self.pending = None;
                self.warnings=vec!["Folder availability could not be checked. Check paths in Settings; workflows will validate their required files when used.".into()];
            }
        }
    }
    pub fn poll(&mut self, config: &crate::config::AppConfig) {
        if self.paths != business_folders(config) {
            self.restart(config);
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(warnings) => {
                    self.warnings = warnings;
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.warnings = vec![
                        "Folder availability check stopped. Recheck folders or review Settings."
                            .into(),
                    ];
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
    }
    pub fn checking(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_defaults_expand_deterministically_for_an_arbitrary_home() {
        let home = Path::new("/srv/example-user");
        assert_eq!(
            expand_path_with_home(
                Path::new("~/Documents/DirectPaymentTimesheets/payslips"),
                Some(home)
            ),
            home.join("Documents/DirectPaymentTimesheets/payslips")
        );
        assert_eq!(expand_path_with_home(Path::new("~"), Some(home)), home);
    }

    #[test]
    fn ensure_directories_creates_missing_configured_directories() {
        let directory = tempfile::TempDir::new().unwrap();
        let first = directory.path().join("one");
        let second = directory.path().join("two").join("nested");

        ensure_directories(&[&first, &second]).unwrap();

        assert!(first.is_dir());
        assert!(second.is_dir());
    }

    #[test]
    fn explicit_paths_and_unexpandable_home_markers_are_preserved() {
        assert_eq!(
            expand_path_with_home(Path::new("/custom/payroll/payslips"), None),
            Path::new("/custom/payroll/payslips")
        );
        assert_eq!(
            expand_path_with_home(Path::new("~/payslips"), None),
            Path::new("~/payslips")
        );
    }
}

#[cfg(test)]
mod stage5_tests {
    use super::*;
    #[test]
    fn folder_warnings_are_read_only_preserve_paths_and_do_not_block_config_save_or_logger() {
        let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
        let missing = dir.path().join("disconnected/share/import");
        app.context.config.folders.csv_import = missing.clone();
        let warnings = business_folder_warnings(&app.context.config);
        assert!(warnings.iter().any(|s| s.contains("CSV import")));
        assert!(!missing.exists());
        app.save_config().unwrap();
        assert!(!missing.exists());
        assert_eq!(app.context.config.folders.csv_import, missing);
        assert!(app.import_csv().is_err());
        let db = crate::database::open(&app.context.environment.database_path).unwrap();
        db.execute(
            "INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Test','PA')",
            [],
        )
        .unwrap();
        let time = crate::direct_shift_repository::parse_shift_time("2026-04-02T09:00").unwrap();
        app.direct_shift_repository
            .clock_in(1, time, "test")
            .unwrap();
        // Recovering the configured root clears that folder's warning without redirecting it.
        std::fs::create_dir_all(&missing).unwrap();
        assert!(!business_folder_warnings(&app.context.config)
            .iter()
            .any(|s| s.contains("CSV import")));
    }
    #[test]
    fn a_file_in_place_of_business_directory_produces_a_safe_warning() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-directory");
        std::fs::write(&file, b"retained").unwrap();
        let mut config = crate::config::AppConfig::default();
        config.folders.pdf_output = file.clone();
        assert!(business_folder_warnings(&config)
            .iter()
            .any(|s| s.contains("PDF output")));
        assert_eq!(std::fs::read(file).unwrap(), b"retained");
    }
}

#[cfg(test)]
mod stage5_background_tests {
    #[test]
    fn stage5_background_folder_check_reports_missing_paths_and_refreshes_changed_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = crate::config::AppConfig::default();
        config.folders.csv_import = dir.path().join("missing");
        config.folders.pdf_output = dir.path().to_path_buf();
        config.folders.email_archive = dir.path().to_path_buf();
        config.folders.payslip_folder = dir.path().to_path_buf();
        config.folders.payroll_information_folder = dir.path().to_path_buf();
        let mut check = super::FolderCheck::new(&config);
        assert!(check.checking());
        let wait = |check: &mut super::FolderCheck, config: &crate::config::AppConfig| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while check.checking() && std::time::Instant::now() < deadline {
                check.poll(config);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(!check.checking());
        };
        wait(&mut check, &config);
        assert!(check.warnings.iter().any(|s| s.contains("CSV import")));
        assert!(!config.folders.csv_import.exists());
        config.folders.csv_import = dir.path().to_path_buf();
        check.poll(&config);
        wait(&mut check, &config);
        assert!(check.warnings.is_empty());
    }
}
