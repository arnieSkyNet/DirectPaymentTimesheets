use crate::config::AppConfig;
use crate::environment::AppEnvironment;
use crate::error::AppError;

pub struct AppContext {
    pub environment: AppEnvironment,
    pub config: AppConfig,
    pub version: String,
}

impl AppContext {
    pub fn initialise() -> Result<Self, AppError> {
        let environment = AppEnvironment::initialise()?;

        let config_path = environment.data_dir.join("config.toml");

        let config = AppConfig::load(&config_path)?;

        crate::paths::ensure_directories(&[
            &config.folders.csv_import,
            &config.folders.pdf_output,
            &config.folders.email_archive,
            &config.folders.payslip_folder,
            &config.folders.payroll_information_folder,
        ])
        .map_err(|error| AppError::Config(error.to_string()))?;

        Ok(Self {
            environment,
            config,
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
}
