use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub folders: FolderConfig,

    #[serde(default)]
    pub pdf: PdfConfig,

    #[serde(default)]
    pub payroll: PayrollConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderConfig {
    pub csv_import: PathBuf,
    pub pdf_output: PathBuf,
    pub email_archive: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PdfConfig {
    #[serde(default = "default_regular_font")]
    pub regular_font: PathBuf,

    #[serde(default = "default_bold_font")]
    pub bold_font: PathBuf,

    #[serde(default = "default_week_commencing_font_size")]
    pub week_commencing_font_size: f64,

    #[serde(default = "default_hours_font_size")]
    pub hours_font_size: f64,

    #[serde(default = "default_information_font_size")]
    pub information_font_size: f64,
}

fn default_regular_font() -> PathBuf {
    PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
}

fn default_bold_font() -> PathBuf {
    PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")
}

fn default_week_commencing_font_size() -> f64 {
    8.5
}

fn default_hours_font_size() -> f64 {
    14.0
}

fn default_information_font_size() -> f64 {
    5.5
}

impl Default for PdfConfig {
    fn default() -> Self {
        Self {
            regular_font: default_regular_font(),
            bold_font: default_bold_font(),
            week_commencing_font_size: default_week_commencing_font_size(),
            hours_font_size: default_hours_font_size(),
            information_font_size: default_information_font_size(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PayrollConfig {
    pub frequency: String,
    pub rounding_minutes: i64,

    #[serde(default = "default_rounding_direction")]
    pub rounding_direction: String,

    #[serde(default = "default_workweek")]
    pub start_of_workweek: String,

    pub email_subject_format: String,

    pub overtime_enabled: bool,
    pub public_holiday_enabled: bool,
}

fn default_rounding_direction() -> String {
    "Up".to_string()
}

fn default_workweek() -> String {
    "Monday".to_string()
}

impl Default for PayrollConfig {
    fn default() -> Self {
        Self {
            frequency: "Every Four Weeks".to_string(),

            rounding_minutes: 15,
            rounding_direction: "Up".to_string(),

            start_of_workweek: "Monday".to_string(),

            email_subject_format: "YYYYMMwWW".to_string(),

            overtime_enabled: false,
            public_holiday_enabled: false,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().expect("Could not determine home directory");

        Self {
            folders: FolderConfig {
                csv_import: home.join("Documents/DirectPaymentTimesheets/import"),
                pdf_output: home.join("Documents/DirectPaymentTimesheets/pdf"),
                email_archive: home.join("Documents/DirectPaymentTimesheets/emails"),
            },

            pdf: PdfConfig::default(),

            payroll: PayrollConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self, AppError> {
        if path.exists() {
            let contents = fs::read_to_string(path).map_err(|e| AppError::Config(e.to_string()))?;

            toml::from_str(&contents).map_err(|e| AppError::Config(e.to_string()))
        } else {
            let config = Self::default();

            let contents =
                toml::to_string_pretty(&config).map_err(|e| AppError::Config(e.to_string()))?;

            fs::write(path, contents).map_err(|e| AppError::Config(e.to_string()))?;

            Ok(config)
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        let contents = toml::to_string_pretty(self).map_err(|e| AppError::Config(e.to_string()))?;

        fs::write(path, contents).map_err(|e| AppError::Config(e.to_string()))?;

        Ok(())
    }
}
