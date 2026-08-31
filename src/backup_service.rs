use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use rusqlite::{Connection, DatabaseName};

const DATABASE_BACKUP_NAME: &str = "database.sqlite";
const CONFIG_BACKUP_NAME: &str = "config.toml";
const README_NAME: &str = "README.txt";

#[derive(Debug)]
pub struct BackupError(String);

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BackupError {}

pub struct BackupService;

impl BackupService {
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

            return Err(error);
        }

        Ok(backup_dir)
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
            "DirectPaymentTimesheets backup\n\nCreated: {created_at}\n\nBacked-up files:\n{file_list}\n"
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
}
