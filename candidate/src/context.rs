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
        Self::from_environment(environment)
    }

    pub(crate) fn from_environment(environment: AppEnvironment) -> Result<Self, AppError> {
        // Upgrade safety runs before configuration/default folders can be changed.
        crate::database::initialise_database(&environment.database_path).map_err(|error| {
            crate::startup::StartupError::database(&environment.database_path, error.as_ref())
        })?;
        let config_path = environment.data_dir.join("config.toml");

        let config =
            AppConfig::load(&config_path).map_err(crate::startup::StartupError::after_database)?;

        Ok(Self {
            environment,
            config,
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
}
