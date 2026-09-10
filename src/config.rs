use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: ApplicationTheme,

    #[serde(default)]
    pub date_display_format: crate::date_utils::DateDisplayFormat,

    #[serde(default)]
    pub hours_shift_date_time_spinner: bool,

    pub folders: FolderConfig,

    #[serde(default)]
    pub pdf: PdfConfig,

    #[serde(default)]
    pub payroll: PayrollConfig,

    #[serde(default)]
    pub email: EmailConfig,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationTheme {
    #[default]
    Dark,
    Light,
    System,
    SoftLight,
    SoftDark,
    Blue,
    AccessibleHighContrast,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderConfig {
    #[serde(default = "default_csv_import_folder")]
    pub csv_import: PathBuf,

    #[serde(default = "default_pdf_output_folder")]
    pub pdf_output: PathBuf,

    #[serde(default = "default_email_archive_folder")]
    pub email_archive: PathBuf,

    #[serde(default = "default_payslip_folder")]
    pub payslip_folder: PathBuf,

    #[serde(default = "default_payroll_information_folder")]
    pub payroll_information_folder: PathBuf,
}

fn portable_business_folder(name: &str) -> PathBuf {
    PathBuf::from("~/Documents/DirectPaymentTimesheets").join(name)
}

fn default_csv_import_folder() -> PathBuf {
    portable_business_folder("import")
}

fn default_pdf_output_folder() -> PathBuf {
    portable_business_folder("pdf")
}

fn default_email_archive_folder() -> PathBuf {
    portable_business_folder("emails")
}

fn default_payslip_folder() -> PathBuf {
    portable_business_folder("payslips")
}

fn default_payroll_information_folder() -> PathBuf {
    portable_business_folder("payroll-information")
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

    #[serde(default = "default_timesheet_footer_text")]
    pub timesheet_footer_text: String,

    #[serde(
        default = "default_timesheet_footer_font_size",
        deserialize_with = "deserialize_footer_font_size"
    )]
    pub timesheet_footer_font_size: f64,

    pub overtime_enabled: bool,
}

pub const FOOTER_FONT_SIZES: [u8; 7] = [6, 7, 8, 9, 10, 11, 12];

pub fn default_timesheet_footer_text() -> String {
    "Both the employer and the employee must sign all time sheets before NASS can process them.\nThese time sheets will be retained on file for 6 years and may be required for inspection by the County Treasurer.".into()
}

fn default_timesheet_footer_font_size() -> f64 {
    7.0
}

pub fn normalise_footer_font_size(size: f64) -> f64 {
    if FOOTER_FONT_SIZES
        .iter()
        .any(|allowed| f64::from(*allowed) == size)
    {
        size
    } else {
        7.0
    }
}

fn deserialize_footer_font_size<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<f64, D::Error> {
    let value = toml::Value::deserialize(deserializer)?;
    Ok(normalise_footer_font_size(
        value
            .as_float()
            .or_else(|| value.as_integer().map(|n| n as f64))
            .unwrap_or(7.0),
    ))
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
            timesheet_footer_text: default_timesheet_footer_text(),
            timesheet_footer_font_size: default_timesheet_footer_font_size(),
            overtime_enabled: false,
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
        Self {
            theme: ApplicationTheme::default(),
            date_display_format: Default::default(),
            hours_shift_date_time_spinner: false,
            folders: FolderConfig {
                csv_import: default_csv_import_folder(),
                pdf_output: default_pdf_output_folder(),
                email_archive: default_email_archive_folder(),
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

    #[test]
    fn payroll_footer_old_configs_defaults_and_multiline_round_trip() {
        let mut source = toml::Value::try_from(AppConfig::default()).unwrap();
        let payroll = source.get_mut("payroll").unwrap().as_table_mut().unwrap();
        payroll.remove("timesheet_footer_text");
        payroll.remove("timesheet_footer_font_size");
        let mut loaded: AppConfig = toml::from_str(&toml::to_string(&source).unwrap()).unwrap();
        assert_eq!(loaded.payroll.timesheet_footer_text, "Both the employer and the employee must sign all time sheets before NASS can process them.\nThese time sheets will be retained on file for 6 years and may be required for inspection by the County Treasurer.");
        assert_eq!(loaded.payroll.timesheet_footer_font_size, 7.0);
        source.as_table_mut().unwrap().remove("payroll");
        let missing: AppConfig = toml::from_str(&toml::to_string(&source).unwrap()).unwrap();
        assert_eq!(
            missing.payroll.timesheet_footer_text,
            loaded.payroll.timesheet_footer_text
        );
        for text in [
            "First  line\n\nSecond paragraph\n",
            "",
            "First\r\n\r\nSecond",
        ] {
            loaded.payroll.timesheet_footer_text = text.into();
            let roundtrip: AppConfig = toml::from_str(&toml::to_string(&loaded).unwrap()).unwrap();
            assert_eq!(roundtrip.payroll.timesheet_footer_text, text);
        }
    }

    #[test]
    fn payroll_footer_sizes_are_restricted_and_unsupported_config_falls_back() {
        for size in [
            6.0,
            7.0,
            8.0,
            9.0,
            10.0,
            11.0,
            12.0,
            13.0,
            -1.0,
            0.0,
            7.5,
            100.0,
            f64::NAN,
            f64::INFINITY,
        ] {
            let mut config = AppConfig::default();
            config.payroll.timesheet_footer_font_size = size;
            let loaded: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
            let expected = if (6.0..=12.0).contains(&size) && size.fract() == 0.0 {
                size
            } else {
                7.0
            };
            assert_eq!(loaded.payroll.timesheet_footer_font_size, expected);
            assert_eq!(normalise_footer_font_size(size), expected);
        }
        let mut value = toml::Value::try_from(AppConfig::default()).unwrap();
        value["payroll"]["timesheet_footer_font_size"] = toml::Value::String("invalid".into());
        let loaded: AppConfig = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert_eq!(loaded.payroll.timesheet_footer_font_size, 7.0);
    }

    #[test]
    fn calendar_preference_defaults_and_round_trips_all_restricted_choices() {
        use crate::date_utils::DateDisplayFormat;
        let mut value = toml::Value::try_from(AppConfig::default()).unwrap();
        value.as_table_mut().unwrap().remove("date_display_format");
        let old: AppConfig = value.clone().try_into().unwrap();
        assert_eq!(old.date_display_format, DateDisplayFormat::ShortMonth);
        let dir = tempfile::tempdir().unwrap();
        for format in DateDisplayFormat::ALL {
            let mut config = AppConfig::default();
            config.date_display_format = format;
            config.save(&dir.path().join("config.toml")).unwrap();
            assert_eq!(
                AppConfig::load(&dir.path().join("config.toml"))
                    .unwrap()
                    .date_display_format,
                format
            );
        }
        value
            .as_table_mut()
            .unwrap()
            .insert("date_display_format".into(), "arbitrary".into());
        assert!(value.try_into::<AppConfig>().is_err());
    }

    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn first_run_folder_defaults_are_portable_and_provider_neutral() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("config.toml");
        let config = AppConfig::load(&path).unwrap();
        let serialized = fs::read_to_string(path).unwrap();

        assert!(!serialized.contains("/home/example"));
        assert!(!serialized.contains("NCC"));
        assert_eq!(
            config.folders.csv_import,
            Path::new("~/Documents/DirectPaymentTimesheets/import")
        );
        assert_eq!(
            config.folders.pdf_output,
            Path::new("~/Documents/DirectPaymentTimesheets/pdf")
        );
        assert_eq!(
            config.folders.email_archive,
            Path::new("~/Documents/DirectPaymentTimesheets/emails")
        );
        assert_eq!(
            config.folders.payslip_folder,
            Path::new("~/Documents/DirectPaymentTimesheets/payslips")
        );
        assert_eq!(
            config.folders.payroll_information_folder,
            Path::new("~/Documents/DirectPaymentTimesheets/payroll-information")
        );
    }

    #[test]
    fn missing_folder_keys_receive_the_same_portable_defaults() {
        let mut value = toml::Value::try_from(AppConfig::default()).unwrap();
        value
            .get_mut("folders")
            .and_then(toml::Value::as_table_mut)
            .unwrap()
            .clear();

        let config: AppConfig = value.try_into().unwrap();

        assert_eq!(config.folders.csv_import, default_csv_import_folder());
        assert_eq!(config.folders.pdf_output, default_pdf_output_folder());
        assert_eq!(config.folders.email_archive, default_email_archive_folder());
        assert_eq!(config.folders.payslip_folder, default_payslip_folder());
        assert_eq!(
            config.folders.payroll_information_folder,
            default_payroll_information_folder()
        );
    }

    #[test]
    fn explicitly_configured_absolute_business_paths_round_trip_unchanged() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("config.toml");
        let mut config = AppConfig::default();
        config.folders.csv_import = PathBuf::from("/existing/import");
        config.folders.pdf_output = PathBuf::from("/existing/pdf");
        config.folders.email_archive = PathBuf::from("/existing/email-archive");
        config.folders.payslip_folder = PathBuf::from("/home/example/user-selected/payslips");
        config.folders.payroll_information_folder = PathBuf::from("/existing/payroll-information");

        config.save(&path).unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        loaded.save(&path).unwrap();
        let reloaded = AppConfig::load(&path).unwrap();

        assert_eq!(reloaded.folders.csv_import, Path::new("/existing/import"));
        assert_eq!(reloaded.folders.pdf_output, Path::new("/existing/pdf"));
        assert_eq!(
            reloaded.folders.email_archive,
            Path::new("/existing/email-archive")
        );
        assert_eq!(
            reloaded.folders.payslip_folder,
            Path::new("/home/example/user-selected/payslips")
        );
        assert_eq!(
            reloaded.folders.payroll_information_folder,
            Path::new("/existing/payroll-information")
        );
    }

    #[test]
    fn config_without_theme_defaults_to_dark() {
        let mut value = toml::Value::try_from(AppConfig::default()).unwrap();
        value.as_table_mut().unwrap().remove("theme");

        let config: AppConfig = value.try_into().unwrap();

        assert_eq!(config.theme, ApplicationTheme::Dark);
    }

    #[test]
    fn config_without_hours_shift_date_time_spinner_defaults_to_off() {
        let mut value = toml::Value::try_from(AppConfig::default()).unwrap();
        value
            .as_table_mut()
            .unwrap()
            .remove("hours_shift_date_time_spinner");

        let config: AppConfig = value.try_into().unwrap();

        assert!(!config.hours_shift_date_time_spinner);
    }

    #[test]
    fn hours_shift_date_time_spinner_round_trips_through_config_persistence() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("config.toml");
        let mut config = AppConfig::default();
        config.hours_shift_date_time_spinner = true;

        config.save(&path).unwrap();
        let loaded = AppConfig::load(&path).unwrap();

        assert!(loaded.hours_shift_date_time_spinner);
    }

    #[test]
    fn all_themes_round_trip_through_existing_config_persistence() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "direct-payment-timesheets-config-{}-{unique}.toml",
            std::process::id()
        ));
        let themes = [
            ApplicationTheme::System,
            ApplicationTheme::Light,
            ApplicationTheme::SoftLight,
            ApplicationTheme::Dark,
            ApplicationTheme::SoftDark,
            ApplicationTheme::Blue,
            ApplicationTheme::AccessibleHighContrast,
        ];

        for theme in themes {
            let mut config = AppConfig::default();
            config.theme = theme;

            config.save(&path).unwrap();
            let loaded = AppConfig::load(&path).unwrap();

            assert_eq!(loaded.theme, theme);
        }

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn obsolete_public_holiday_enabled_key_is_ignored_and_not_saved() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "direct-payment-timesheets-obsolete-holiday-setting-{}-{unique}.toml",
            std::process::id()
        ));
        let mut value = toml::Value::try_from(AppConfig::default()).unwrap();
        value
            .get_mut("payroll")
            .and_then(toml::Value::as_table_mut)
            .unwrap()
            .insert("public_holiday_enabled".to_string(), true.into());
        std::fs::write(&path, toml::to_string_pretty(&value).unwrap()).unwrap();

        let config = AppConfig::load(&path).unwrap();
        config.save(&path).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();

        assert!(!saved.contains("public_holiday_enabled"));

        std::fs::remove_file(path).unwrap();
    }

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
