use std::fs;
use std::path::{Path, PathBuf};

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
        let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let data_dir = resolve_data_dir(
            configured_home.as_deref(),
            user_home.as_deref(),
            local_app_data.as_deref(),
            cfg!(windows),
        )?;

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
    local_app_data: Option<&Path>,
    windows: bool,
) -> Result<PathBuf, AppError> {
    match configured_home {
        Some(path) => Ok(PathBuf::from(path)),
        None if windows => local_app_data
            .map(|local| local.join("DirectPaymentTimesheets"))
            .ok_or_else(|| AppError::Config("LOCALAPPDATA not set".into())),
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
            resolve_data_dir(None, Some("/srv/arbitrary-user"), None, false).unwrap(),
            PathBuf::from("/srv/arbitrary-user/.directpaymenttimesheets")
        );
        assert_eq!(
            resolve_data_dir(
                Some("/var/lib/direct-payment-test"),
                Some("/srv/arbitrary-user"),
                None,
                false,
            )
            .unwrap(),
            PathBuf::from("/var/lib/direct-payment-test")
        );
        assert!(resolve_data_dir(None, None, None, false).is_err());
    }

    #[test]
    fn windows_data_root_uses_local_app_data_without_requiring_home() {
        let local = Path::new("C:/Users/Synthetic User/AppData/Local");
        for home in [None, Some("/git-bash/home")] {
            assert_eq!(
                resolve_data_dir(None, home, Some(local), true).unwrap(),
                local.join("DirectPaymentTimesheets")
            );
        }
        assert!(resolve_data_dir(None, Some("/git-bash/home"), None, true).is_err());
        assert!(resolve_data_dir(None, None, Some(local), false).is_err());
        assert_eq!(
            resolve_data_dir(None, Some("/srv/arbitrary-user"), Some(local), false).unwrap(),
            PathBuf::from("/srv/arbitrary-user/.directpaymenttimesheets")
        );
    }

    #[test]
    fn application_home_override_is_verbatim_and_has_priority_on_both_platforms() {
        for windows in [false, true] {
            for configured in ["", "relative/../chosen", "~/chosen", "D:/Chosen Data/Zoë"] {
                for local in [None, Some(Path::new("C:/Users/Synthetic/AppData/Local"))] {
                    assert_eq!(
                        resolve_data_dir(Some(configured), Some("/unused/home"), local, windows)
                            .unwrap(),
                        PathBuf::from(configured)
                    );
                }
                assert_eq!(
                    resolve_data_dir(Some(configured), None, None, windows).unwrap(),
                    PathBuf::from(configured)
                );
            }
        }
    }
}
