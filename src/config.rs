use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub folders: FolderConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderConfig {
    pub csv_import: PathBuf,
    pub pdf_output: PathBuf,
    pub email_archive: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir()
            .expect("Could not determine home directory");

        Self {
            folders: FolderConfig {
                csv_import: home.join("Documents/DirectPaymentTimesheets/import"),
                pdf_output: home.join("Documents/DirectPaymentTimesheets/pdf"),
                email_archive: home.join("Documents/DirectPaymentTimesheets/emails"),
            },
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self, AppError> {
        if path.exists() {
            let contents = fs::read_to_string(path)
                .map_err(|e| AppError::Config(e.to_string()))?;

            toml::from_str(&contents)
                .map_err(|e| AppError::Config(e.to_string()))
        } else {
            let config = Self::default();

            let contents = toml::to_string_pretty(&config)
                .map_err(|e| AppError::Config(e.to_string()))?;

            fs::write(path, contents)
                .map_err(|e| AppError::Config(e.to_string()))?;

            Ok(config)
        }
    }
}
