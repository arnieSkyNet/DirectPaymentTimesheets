use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Local;
use rusqlite::backup::Backup;
use rusqlite::{Connection, DatabaseName, OpenFlags};

const DATABASE_BACKUP_NAME: &str = "database.sqlite";
const CONFIG_BACKUP_NAME: &str = "config.toml";
const README_NAME: &str = "README.txt";
const BACKUP_IDENTITY: &str = "DirectPaymentTimesheets backup";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub directory_name: String,
    pub created_at: String,
    pub has_config: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BackupValidation {
    pub schema_version: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RestoreResult {
    pub restored_backup: PathBuf,
    pub safety_backup: PathBuf,
    pub config_restored: bool,
}

#[derive(Debug)]
pub struct BackupError(String);

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BackupError {}

#[derive(Debug)]
pub struct RestoreError {
    error: BackupError,
    restart_required: bool,
}

impl RestoreError {
    pub fn restart_required(&self) -> bool {
        self.restart_required
    }
}

impl From<BackupError> for RestoreError {
    fn from(error: BackupError) -> Self {
        Self {
            error,
            restart_required: false,
        }
    }
}

impl fmt::Display for RestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for RestoreError {}

pub struct BackupService;

impl BackupService {
    pub fn discover(backups_dir: &Path) -> Result<Vec<BackupInfo>, BackupError> {
        let entries = fs::read_dir(backups_dir).map_err(|error| {
            BackupError(format!(
                "Could not read backups directory {}: {error}",
                backups_dir.display()
            ))
        })?;
        let mut backups = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|error| {
                BackupError(format!("Could not read a backup directory entry: {error}"))
            })?;
            let path = entry.path();
            let directory_name = entry.file_name().to_string_lossy().to_string();
            if !entry
                .file_type()
                .map_err(|error| {
                    BackupError(format!(
                        "Could not inspect backup entry {}: {error}",
                        path.display()
                    ))
                })?
                .is_dir()
                || !is_timestamp_name(&directory_name)
            {
                continue;
            }

            let readme = match fs::read_to_string(path.join(README_NAME)) {
                Ok(readme) if has_backup_identity(&readme) => readme,
                _ => continue,
            };
            let created_at = readme
                .lines()
                .find_map(|line| line.strip_prefix("Created: "))
                .unwrap_or(&directory_name)
                .to_string();
            backups.push(BackupInfo {
                has_config: path.join(CONFIG_BACKUP_NAME).is_file(),
                path,
                directory_name,
                created_at,
            });
        }

        backups.sort_by(|left, right| right.directory_name.cmp(&left.directory_name));
        Ok(backups)
    }

    pub fn create(
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
    ) -> Result<PathBuf, BackupError> {
        let created_at = Local::now();

        Self::create_with_metadata(
            database_path,
            config_path,
            backups_dir,
            &created_at.format("%Y%m%d-%H%M%S").to_string(),
            &created_at.to_rfc3339(),
        )
    }

    fn create_with_metadata(
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
        directory_name: &str,
        created_at: &str,
    ) -> Result<PathBuf, BackupError> {
        fs::create_dir_all(backups_dir).map_err(|error| {
            BackupError(format!(
                "Could not create backups directory {}: {error}",
                backups_dir.display()
            ))
        })?;

        let backup_dir = backups_dir.join(directory_name);
        fs::create_dir(&backup_dir).map_err(|error| {
            BackupError(format!(
                "Could not create backup directory {}: {error}",
                backup_dir.display()
            ))
        })?;

        let result = Self::populate_backup(database_path, config_path, &backup_dir, created_at);
        if let Err(error) = result {
            if let Err(cleanup_error) = fs::remove_dir_all(&backup_dir) {
                return Err(BackupError(format!(
                    "{error} The incomplete backup at {} could not be removed: {cleanup_error}",
                    backup_dir.display()
                )));
            }

            return Err(error.into());
        }

        Ok(backup_dir)
    }

    pub fn validate(
        backup_dir: &Path,
        backups_dir: &Path,
    ) -> Result<BackupValidation, BackupError> {
        validate_backup_location(backup_dir, backups_dir)?;

        let readme_path = backup_dir.join(README_NAME);
        reject_symlink(&readme_path, "backup manifest")?;
        let readme = fs::read_to_string(&readme_path).map_err(|error| {
            BackupError(format!(
                "Could not read backup manifest {}: {error}",
                readme_path.display()
            ))
        })?;
        if !has_backup_identity(&readme) {
            return Err(BackupError(format!(
                "{} is not identified as a DirectPaymentTimesheets backup.",
                backup_dir.display()
            )));
        }
        if !readme.lines().any(|line| line == "- database.sqlite") {
            return Err(BackupError(
                "The backup manifest does not list database.sqlite.".to_string(),
            ));
        }

        let config_path = backup_dir.join(CONFIG_BACKUP_NAME);
        let manifest_lists_config = readme.lines().any(|line| line == "- config.toml");
        if config_path.exists() {
            reject_symlink(&config_path, "backup configuration file")?;
            if !manifest_lists_config {
                return Err(BackupError(
                    "The backup contains config.toml but the manifest does not list it."
                        .to_string(),
                ));
            }
        } else if manifest_lists_config {
            return Err(BackupError(
                "The backup manifest lists config.toml, but that file is missing.".to_string(),
            ));
        }

        let database_path = backup_dir.join(DATABASE_BACKUP_NAME);
        reject_symlink(&database_path, "backup database")?;
        let connection = open_read_only(&database_path)?;
        let integrity_results = connection
            .prepare("PRAGMA integrity_check")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|error| {
                BackupError(format!(
                    "Could not run SQLite integrity_check on {}: {error}",
                    database_path.display()
                ))
            })?;
        if integrity_results.len() != 1 || !integrity_results[0].eq_ignore_ascii_case("ok") {
            return Err(BackupError(format!(
                "SQLite integrity_check failed for {}: {}",
                database_path.display(),
                integrity_results.join("; ")
            )));
        }

        for table in [
            "schema_version",
            "timesheets",
            "employers",
            "personal_assistants",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    BackupError(format!("Could not inspect backup schema: {error}"))
                })?;
            if !exists {
                return Err(BackupError(format!(
                    "The backup database is not recognisable as DirectPaymentTimesheets: required table {table} is missing."
                )));
            }
        }

        let versions = connection
            .prepare("SELECT version FROM schema_version")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|error| {
                BackupError(format!("Could not read backup schema version: {error}"))
            })?;
        if versions.len() != 1 || versions[0] != crate::database::CURRENT_SCHEMA_VERSION {
            return Err(BackupError(format!(
                "The backup has an unsupported DirectPaymentTimesheets schema version: {:?} (required: {}).",
                versions,
                crate::database::CURRENT_SCHEMA_VERSION
            )));
        }

        Ok(BackupValidation {
            schema_version: versions[0],
        })
    }

    pub fn restore(
        backup_dir: &Path,
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
    ) -> Result<RestoreResult, RestoreError> {
        Self::validate(backup_dir, backups_dir)?;

        let safety_backup =
            Self::create(database_path, config_path, backups_dir).map_err(|error| {
                RestoreError::from(BackupError(format!(
                    "Restore aborted because the required pre-restore safety backup failed: {error}"
                )))
            })?;

        let selected_config = backup_dir.join(CONFIG_BACKUP_NAME);
        let staged_config = if selected_config.is_file() {
            Some(stage_config(&selected_config, config_path).map_err(|error| {
                RestoreError::from(BackupError(format!(
                    "Restore aborted before changing live data because config.toml could not be staged: {error}"
                )))
            })?)
        } else {
            None
        };

        let source_path = backup_dir.join(DATABASE_BACKUP_NAME);
        let source = open_read_only(&source_path)?;
        let mut destination = Connection::open(database_path).map_err(|error| {
            BackupError(format!(
                "Could not open live database {} for restoration: {error}",
                database_path.display()
            ))
        })?;
        let restore_result = {
            let backup = Backup::new(&source, &mut destination).map_err(|error| {
                BackupError(format!("Could not initialise SQLite restoration: {error}"))
            })?;
            backup
                .run_to_completion(100, Duration::from_millis(10), None)
                .map_err(|error| BackupError(format!("SQLite restoration failed: {error}")))
        };
        if let Err(error) = restore_result {
            if let Some(staged_config) = staged_config {
                let _ = fs::remove_file(staged_config);
            }
            return Err(error.into());
        }

        if let Some(staged_config) = staged_config {
            fs::rename(&staged_config, config_path).map_err(|error| RestoreError {
                error: BackupError(format!(
                        "The database was restored, but config.toml could not be installed from {}: {error}. Close and restart the application; the pre-restore safety backup is {}.",
                        backup_dir.display(),
                        safety_backup.display()
                    )),
                restart_required: true,
            })?;
        }

        Ok(RestoreResult {
            restored_backup: backup_dir.to_path_buf(),
            safety_backup,
            config_restored: selected_config.is_file(),
        })
    }

    fn populate_backup(
        database_path: &Path,
        config_path: &Path,
        backup_dir: &Path,
        created_at: &str,
    ) -> Result<(), BackupError> {
        let source = Connection::open(database_path).map_err(|error| {
            BackupError(format!(
                "Could not open database {} for backup: {error}",
                database_path.display()
            ))
        })?;
        let database_backup_path = backup_dir.join(DATABASE_BACKUP_NAME);
        source
            .backup(DatabaseName::Main, &database_backup_path, None)
            .map_err(|error| {
                BackupError(format!(
                    "Could not back up database to {}: {error}",
                    database_backup_path.display()
                ))
            })?;

        let mut backed_up_files = vec![DATABASE_BACKUP_NAME];
        if config_path.exists() {
            let config_backup_path = backup_dir.join(CONFIG_BACKUP_NAME);
            fs::copy(config_path, &config_backup_path).map_err(|error| {
                BackupError(format!(
                    "Could not copy configuration file {} to {}: {error}",
                    config_path.display(),
                    config_backup_path.display()
                ))
            })?;
            backed_up_files.push(CONFIG_BACKUP_NAME);
        }

        let file_list = backed_up_files
            .iter()
            .map(|name| format!("- {name}"))
            .collect::<Vec<_>>()
            .join("\n");
        let readme = format!(
            "{BACKUP_IDENTITY}\n\nCreated: {created_at}\n\nBacked-up files:\n{file_list}\n"
        );
        let readme_path = backup_dir.join(README_NAME);
        fs::write(&readme_path, readme).map_err(|error| {
            BackupError(format!(
                "Could not write backup manifest {}: {error}",
                readme_path.display()
            ))
        })?;

        Ok(())
    }
}

fn is_timestamp_name(name: &str) -> bool {
    name.len() == 15
        && name.as_bytes()[8] == b'-'
        && name
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 8 || byte.is_ascii_digit())
}

fn has_backup_identity(readme: &str) -> bool {
    readme.lines().next() == Some(BACKUP_IDENTITY)
}

fn validate_backup_location(backup_dir: &Path, backups_dir: &Path) -> Result<(), BackupError> {
    let name = backup_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| BackupError("The selected backup has an invalid directory name.".into()))?;
    if !is_timestamp_name(name) {
        return Err(BackupError(format!(
            "The selected directory {} does not use the DirectPaymentTimesheets backup timestamp format YYYYMMDD-HHMMSS.",
            backup_dir.display()
        )));
    }
    reject_symlink(backup_dir, "backup directory")?;

    let canonical_root = backups_dir.canonicalize().map_err(|error| {
        BackupError(format!(
            "Could not resolve backups directory {}: {error}",
            backups_dir.display()
        ))
    })?;
    let canonical_backup = backup_dir.canonicalize().map_err(|error| {
        BackupError(format!(
            "Could not resolve selected backup directory {}: {error}",
            backup_dir.display()
        ))
    })?;
    if canonical_backup.parent() != Some(canonical_root.as_path()) {
        return Err(BackupError(
            "Only direct child directories of the configured backups directory can be restored."
                .into(),
        ));
    }
    Ok(())
}

fn reject_symlink(path: &Path, description: &str) -> Result<(), BackupError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        BackupError(format!(
            "Could not inspect {description} {}: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(BackupError(format!(
            "Refusing to use {description} {} because it is a symbolic link.",
            path.display()
        )));
    }
    Ok(())
}

fn open_read_only(path: &Path) -> Result<Connection, BackupError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|error| {
        BackupError(format!(
            "Could not open backup database {} read-only: {error}",
            path.display()
        ))
    })
}

fn stage_config(source: &Path, destination: &Path) -> Result<PathBuf, BackupError> {
    let parent = destination.parent().ok_or_else(|| {
        BackupError(format!(
            "Configuration path {} has no parent directory.",
            destination.display()
        ))
    })?;
    let staged = parent.join(format!(".config.toml.restore-{}", std::process::id()));
    let contents = fs::read(source).map_err(|error| {
        BackupError(format!(
            "Could not read backup configuration {}: {error}",
            source.display()
        ))
    })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .map_err(|error| {
            BackupError(format!(
                "Could not create staged configuration {}: {error}",
                staged.display()
            ))
        })?;
    use std::io::Write;
    file.write_all(&contents).map_err(|error| {
        BackupError(format!(
            "Could not write staged configuration {}: {error}",
            staged.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        BackupError(format!(
            "Could not flush staged configuration {}: {error}",
            staged.display()
        ))
    })?;
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_database(path: &Path) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute("CREATE TABLE example (value TEXT NOT NULL)", [])
            .unwrap();
        connection
            .execute("INSERT INTO example (value) VALUES ('expected data')", [])
            .unwrap();
    }

    fn create_application_database(path: &Path, marker: &str) {
        let connection = Connection::open(path).unwrap();
        crate::database::create_schema(&connection).unwrap();
        connection
            .execute("CREATE TABLE restore_test_marker (value TEXT NOT NULL)", [])
            .unwrap();
        connection
            .execute(
                "INSERT INTO restore_test_marker (value) VALUES (?1)",
                [marker],
            )
            .unwrap();
    }

    fn read_marker(path: &Path) -> String {
        Connection::open(path)
            .unwrap()
            .query_row("SELECT value FROM restore_test_marker", [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    fn create_named_backup(
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
        name: &str,
    ) -> PathBuf {
        BackupService::create_with_metadata(
            database_path,
            config_path,
            backups_dir,
            name,
            "2026-08-31T14:25:30+01:00",
        )
        .unwrap()
    }

    #[test]
    fn creates_openable_database_backup_with_expected_data_and_copies_config() {
        let temp_dir = TempDir::new().unwrap();
        let database_path = temp_dir.path().join("source.sqlite");
        let config_path = temp_dir.path().join("source.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_test_database(&database_path);
        fs::write(&config_path, "theme = \"dark\"\n").unwrap();

        let backup_dir = BackupService::create_with_metadata(
            &database_path,
            &config_path,
            &backups_dir,
            "20260831-142530",
            "2026-08-31T14:25:30+01:00",
        )
        .unwrap();

        assert_eq!(backup_dir, backups_dir.join("20260831-142530"));
        assert!(backup_dir.is_dir());
        let backup_connection = Connection::open(backup_dir.join(DATABASE_BACKUP_NAME)).unwrap();
        let value: String = backup_connection
            .query_row("SELECT value FROM example", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "expected data");
        assert_eq!(
            fs::read_to_string(backup_dir.join(CONFIG_BACKUP_NAME)).unwrap(),
            "theme = \"dark\"\n"
        );

        let readme = fs::read_to_string(backup_dir.join(README_NAME)).unwrap();
        assert!(readme.contains("DirectPaymentTimesheets backup"));
        assert!(readme.contains("Created: 2026-08-31T14:25:30+01:00"));
        assert!(readme.contains("- database.sqlite"));
        assert!(readme.contains("- config.toml"));
    }

    #[test]
    fn succeeds_without_a_config_file_and_omits_it_from_the_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let database_path = temp_dir.path().join("source.sqlite");
        let missing_config_path = temp_dir.path().join("missing.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_test_database(&database_path);

        let backup_dir = BackupService::create_with_metadata(
            &database_path,
            &missing_config_path,
            &backups_dir,
            "20260831-142531",
            "2026-08-31T14:25:31+01:00",
        )
        .unwrap();

        assert!(backup_dir.join(DATABASE_BACKUP_NAME).is_file());
        assert!(!backup_dir.join(CONFIG_BACKUP_NAME).exists());
        let readme = fs::read_to_string(backup_dir.join(README_NAME)).unwrap();
        assert!(readme.contains("- database.sqlite"));
        assert!(!readme.contains("- config.toml"));
    }

    #[test]
    fn validates_and_restores_application_database_config_and_creates_safety_backup() {
        let temp_dir = TempDir::new().unwrap();
        let live_database = temp_dir.path().join("database.sqlite");
        let selected_database = temp_dir.path().join("selected.sqlite");
        let live_config = temp_dir.path().join("config.toml");
        let selected_config = temp_dir.path().join("selected.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_application_database(&live_database, "current live data");
        create_application_database(&selected_database, "selected backup data");
        fs::write(&live_config, "current config").unwrap();
        fs::write(&selected_config, "selected config").unwrap();
        let selected_backup = create_named_backup(
            &selected_database,
            &selected_config,
            &backups_dir,
            "20260101-010101",
        );

        let validation = BackupService::validate(&selected_backup, &backups_dir).unwrap();
        assert_eq!(
            validation.schema_version,
            crate::database::CURRENT_SCHEMA_VERSION
        );

        let result =
            BackupService::restore(&selected_backup, &live_database, &live_config, &backups_dir)
                .unwrap();

        assert_eq!(result.restored_backup, selected_backup);
        assert!(result.safety_backup.is_dir());
        assert_ne!(result.safety_backup, result.restored_backup);
        assert_eq!(
            read_marker(&result.safety_backup.join(DATABASE_BACKUP_NAME)),
            "current live data"
        );
        assert_eq!(read_marker(&live_database), "selected backup data");
        assert_eq!(fs::read_to_string(&live_config).unwrap(), "selected config");
        assert_eq!(
            fs::read_to_string(result.safety_backup.join(CONFIG_BACKUP_NAME)).unwrap(),
            "current config"
        );
        assert!(selected_backup.is_dir());
    }

    #[test]
    fn rejects_corrupt_backup_without_altering_live_data_or_creating_safety_backup() {
        let temp_dir = TempDir::new().unwrap();
        let live_database = temp_dir.path().join("database.sqlite");
        let live_config = temp_dir.path().join("config.toml");
        let backups_dir = temp_dir.path().join("backups");
        let corrupt_backup = backups_dir.join("20260101-010101");
        create_application_database(&live_database, "current live data");
        fs::write(&live_config, "current config").unwrap();
        fs::create_dir_all(&corrupt_backup).unwrap();
        fs::write(corrupt_backup.join(DATABASE_BACKUP_NAME), "not sqlite").unwrap();
        fs::write(
            corrupt_backup.join(README_NAME),
            format!("{BACKUP_IDENTITY}\n\nCreated: test\n\nBacked-up files:\n- database.sqlite\n"),
        )
        .unwrap();

        let error =
            BackupService::restore(&corrupt_backup, &live_database, &live_config, &backups_dir)
                .unwrap_err();

        assert!(error.to_string().contains("integrity_check"));
        assert_eq!(read_marker(&live_database), "current live data");
        assert_eq!(fs::read_to_string(&live_config).unwrap(), "current config");
        assert_eq!(fs::read_dir(&backups_dir).unwrap().count(), 1);
    }

    #[test]
    fn rejects_sqlite_database_that_is_not_direct_payment_timesheets() {
        let temp_dir = TempDir::new().unwrap();
        let live_database = temp_dir.path().join("database.sqlite");
        let unrelated_database = temp_dir.path().join("unrelated.sqlite");
        let live_config = temp_dir.path().join("config.toml");
        let missing_config = temp_dir.path().join("missing.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_application_database(&live_database, "current live data");
        create_test_database(&unrelated_database);
        fs::write(&live_config, "current config").unwrap();
        let unrelated_backup = create_named_backup(
            &unrelated_database,
            &missing_config,
            &backups_dir,
            "20260101-010101",
        );

        let error = BackupService::restore(
            &unrelated_backup,
            &live_database,
            &live_config,
            &backups_dir,
        )
        .unwrap_err();

        assert!(error.to_string().contains("not recognisable"));
        assert_eq!(read_marker(&live_database), "current live data");
        assert_eq!(fs::read_to_string(&live_config).unwrap(), "current config");
        assert_eq!(fs::read_dir(&backups_dir).unwrap().count(), 1);
    }

    #[test]
    fn restores_database_but_preserves_current_config_when_backup_has_none() {
        let temp_dir = TempDir::new().unwrap();
        let live_database = temp_dir.path().join("database.sqlite");
        let selected_database = temp_dir.path().join("selected.sqlite");
        let live_config = temp_dir.path().join("config.toml");
        let missing_config = temp_dir.path().join("missing.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_application_database(&live_database, "current live data");
        create_application_database(&selected_database, "selected backup data");
        fs::write(&live_config, "current config").unwrap();
        let selected_backup = create_named_backup(
            &selected_database,
            &missing_config,
            &backups_dir,
            "20260101-010101",
        );

        let result =
            BackupService::restore(&selected_backup, &live_database, &live_config, &backups_dir)
                .unwrap();

        assert!(!result.config_restored);
        assert_eq!(read_marker(&live_database), "selected backup data");
        assert_eq!(fs::read_to_string(&live_config).unwrap(), "current config");
        assert!(result.safety_backup.join(CONFIG_BACKUP_NAME).is_file());
    }
}
