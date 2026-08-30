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

    #[serde(default)]
    pub email: EmailConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderConfig {
    pub csv_import: PathBuf,
    pub pdf_output: PathBuf,
    pub email_archive: PathBuf,

    #[serde(default = "default_payslip_folder")]
    pub payslip_folder: PathBuf,

    #[serde(default = "default_payroll_information_folder")]
    pub payroll_information_folder: PathBuf,
}

fn default_payslip_folder() -> PathBuf {
    PathBuf::from("/home/example/Desktop/launchers/Example Timesheets Payslips/2026 to 2027/")
}

fn default_payroll_information_folder() -> PathBuf {
    PathBuf::from("/home/example/Desktop/launchers/Example Personal Budgets/")
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

    #[serde(default = "default_email_subject_format")]
    pub email_subject_format: String,

    #[serde(default = "default_timesheet_email_body")]
    pub timesheet_email_body: String,

    #[serde(default = "default_payslip_email_body")]
    pub payslip_email_body: String,

    pub overtime_enabled: bool,
    pub public_holiday_enabled: bool,
}

fn default_rounding_direction() -> String {
    "Up".to_string()
}

fn default_workweek() -> String {
    "Monday".to_string()
}

fn default_email_subject_format() -> String {
    "{YYYYMMwWW}".to_string()
}

fn default_timesheet_email_body() -> String {
    "Please find attached the timesheet for the payroll period.".to_string()
}

fn default_payslip_email_body() -> String {
    "Please find attached your payslip for the payroll period.".to_string()
}

impl Default for PayrollConfig {
    fn default() -> Self {
        Self {
            frequency: "Every Four Weeks".to_string(),
            rounding_minutes: 15,
            rounding_direction: "Up".to_string(),
            start_of_workweek: "Monday".to_string(),
            email_subject_format: default_email_subject_format(),
            timesheet_email_body: default_timesheet_email_body(),
            payslip_email_body: default_payslip_email_body(),
            overtime_enabled: false,
            public_holiday_enabled: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmailConfig {
    #[serde(default = "default_smtp_transport")]
    pub smtp_transport: String,

    #[serde(default = "default_smtp_host")]
    pub smtp_host: String,

    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,

    #[serde(default)]
    pub smtp_username: String,

    #[serde(default)]
    pub smtp_password: String,

    #[serde(default)]
    pub payroll_test_email_address: String,

    #[serde(default)]
    pub pa_test_email_address: String,

    // Retained for compatibility with existing config.toml files. The active
    // production body continues to be reconciled from PayrollConfig.
    #[serde(default = "default_email_body")]
    pub timesheet_body: String,
}

fn default_smtp_host() -> String {
    "localhost".to_string()
}

fn default_smtp_transport() -> String {
    "Local SMTP Server".to_string()
}

fn default_smtp_port() -> u16 {
    25
}

fn default_email_body() -> String {
    "Hi all,\n\nand thank you :-)\n\nM.".to_string()
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_transport: default_smtp_transport(),
            smtp_host: default_smtp_host(),
            smtp_port: default_smtp_port(),
            smtp_username: String::new(),
            smtp_password: String::new(),
            payroll_test_email_address: String::new(),
            pa_test_email_address: String::new(),
            timesheet_body: default_email_body(),
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
                payslip_folder: default_payslip_folder(),
                payroll_information_folder: default_payroll_information_folder(),
            },

            pdf: PdfConfig::default(),
            payroll: PayrollConfig::default(),
            email: EmailConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self, AppError> {
        if path.exists() {
            let contents = fs::read_to_string(path).map_err(|e| AppError::Config(e.to_string()))?;
            let source: toml::Value =
                toml::from_str(&contents).map_err(|e| AppError::Config(e.to_string()))?;
            let mut config: Self =
                toml::from_str(&contents).map_err(|e| AppError::Config(e.to_string()))?;

            reconcile_legacy_timesheet_email_body(&mut config, &source);

            Ok(config)
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

fn reconcile_legacy_timesheet_email_body(config: &mut AppConfig, source: &toml::Value) {
    let configured_new_body = source
        .get("payroll")
        .and_then(toml::Value::as_table)
        .is_some_and(|payroll| payroll.contains_key("timesheet_email_body"));

    if configured_new_body {
        return;
    }

    let legacy_body = source
        .get("email")
        .and_then(toml::Value::as_table)
        .and_then(|email| email.get("timesheet_body"))
        .and_then(toml::Value::as_str)
        .filter(|body| !body.trim().is_empty());

    if let Some(legacy_body) = legacy_body {
        config.payroll.timesheet_email_body = legacy_body.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_timesheet_body_fills_missing_authoritative_body_only() {
        let source: toml::Value = r#"
            [email]
            timesheet_body = "Legacy timesheet text"
        "#
        .parse()
        .unwrap();
        let mut config = AppConfig::default();
        config.payroll.payslip_email_body = "Existing payslip text".to_string();

        reconcile_legacy_timesheet_email_body(&mut config, &source);

        assert_eq!(config.payroll.timesheet_email_body, "Legacy timesheet text");
        assert_eq!(config.payroll.payslip_email_body, "Existing payslip text");
    }

    #[test]
    fn authoritative_timesheet_body_takes_precedence_over_legacy_body() {
        let source: toml::Value = r#"
            [payroll]
            timesheet_email_body = "Authoritative timesheet text"

            [email]
            timesheet_body = "Legacy timesheet text"
        "#
        .parse()
        .unwrap();
        let mut config = AppConfig::default();
        config.payroll.timesheet_email_body = "Authoritative timesheet text".to_string();

        reconcile_legacy_timesheet_email_body(&mut config, &source);

        assert_eq!(
            config.payroll.timesheet_email_body,
            "Authoritative timesheet text"
        );
    }
}
