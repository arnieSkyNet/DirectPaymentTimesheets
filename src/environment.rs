use std::fs;
use std::path::PathBuf;

use crate::error::AppError;

pub struct AppEnvironment {
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub archive_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub templates_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppEnvironment {
    pub fn initialise() -> Result<Self, AppError> {
        let data_dir = match std::env::var("DIRECTPAYMENTTIMESHEETS_HOME") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                let home = std::env::var("HOME")
                    .map_err(|_| AppError::Config("HOME not set".into()))?;

                PathBuf::from(home)
                    .join(".directpaymenttimesheets")
            }
        };

        let database_path = data_dir.join("database.sqlite");

        let archive_dir = data_dir.join("archive");
        let backups_dir = data_dir.join("backups");
        let logs_dir = data_dir.join("logs");
        let templates_dir = data_dir.join("templates");
        let cache_dir = data_dir.join("cache");

        for directory in [
            &data_dir,
            &archive_dir,
            &backups_dir,
            &logs_dir,
            &templates_dir,
            &cache_dir,
        ] {
            fs::create_dir_all(directory)
                .map_err(|e| AppError::Config(e.to_string()))?;
        }

        Ok(Self {
            data_dir,
            database_path,
            archive_dir,
            backups_dir,
            logs_dir,
            templates_dir,
            cache_dir,
        })
    }
}
