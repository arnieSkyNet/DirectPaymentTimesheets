use std::fs;
use std::path::PathBuf;

use crate::error::AppError;

#[allow(dead_code)]
pub struct AppEnvironment {
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub import_dir: PathBuf,
    pub archive_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub templates_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppEnvironment {
    pub fn initialise() -> Result<Self, AppError> {
        let configured_home = std::env::var("DIRECTPAYMENTTIMESHEETS_HOME").ok();
        let user_home = std::env::var("HOME").ok();
        let data_dir = resolve_data_dir(configured_home.as_deref(), user_home.as_deref())?;

        let database_path = data_dir.join("database.sqlite");
        let import_dir = data_dir.join("import");
        let archive_dir = data_dir.join("archive");
        let backups_dir = data_dir.join("backups");
        let logs_dir = data_dir.join("logs");
        let templates_dir = data_dir.join("templates");
        let cache_dir = data_dir.join("cache");

        for directory in [
            &data_dir,
            &archive_dir,
            &import_dir,
            &backups_dir,
            &logs_dir,
            &templates_dir,
            &cache_dir,
        ] {
            fs::create_dir_all(directory).map_err(|e| AppError::Config(e.to_string()))?;
        }

        Ok(Self {
            data_dir,
            database_path,
            import_dir,
            archive_dir,
            backups_dir,
            logs_dir,
            templates_dir,
            cache_dir,
        })
    }
}

fn resolve_data_dir(
    configured_home: Option<&str>,
    user_home: Option<&str>,
) -> Result<PathBuf, AppError> {
    match configured_home {
        Some(path) => Ok(PathBuf::from(path)),
        None => user_home
            .map(PathBuf::from)
            .map(|home| home.join(".directpaymenttimesheets"))
            .ok_or_else(|| AppError::Config("HOME not set".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_root_is_deterministic_for_arbitrary_user_and_application_homes() {
        assert_eq!(
            resolve_data_dir(None, Some("/srv/arbitrary-user")).unwrap(),
            PathBuf::from("/srv/arbitrary-user/.directpaymenttimesheets")
        );
        assert_eq!(
            resolve_data_dir(
                Some("/var/lib/direct-payment-test"),
                Some("/srv/arbitrary-user")
            )
            .unwrap(),
            PathBuf::from("/var/lib/direct-payment-test")
        );
        assert!(resolve_data_dir(None, None).is_err());
    }
}
