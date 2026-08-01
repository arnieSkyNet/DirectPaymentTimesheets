use crate::environment::AppEnvironment;
use crate::error::AppError;

pub struct AppContext {
    pub environment: AppEnvironment,
    pub version: String,
}

impl AppContext {
    pub fn initialise() -> Result<Self, AppError> {
        let environment = AppEnvironment::initialise()?;

        Ok(Self {
            environment,
            version: "0.0.1".to_string(),
        })
    }
}

