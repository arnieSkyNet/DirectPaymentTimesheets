use std::path::{Path, PathBuf};

pub fn expand_path(path: &PathBuf) -> PathBuf {
    expand_path_with_home(path, dirs::home_dir().as_deref())
}

pub fn ensure_directories(paths: &[&PathBuf]) -> std::io::Result<()> {
    for path in paths {
        std::fs::create_dir_all(expand_path(path))?;
    }

    Ok(())
}

pub fn expand_path_with_home(path: &Path, home: Option<&Path>) -> PathBuf {
    let path_string = path.to_string_lossy();

    if path_string == "~" {
        if let Some(home) = home {
            return home.to_path_buf();
        }
    } else if let Some(remainder) = path_string.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(remainder);
        }
    }

    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_defaults_expand_deterministically_for_an_arbitrary_home() {
        let home = Path::new("/srv/example-user");
        assert_eq!(
            expand_path_with_home(
                Path::new("~/Documents/DirectPaymentTimesheets/payslips"),
                Some(home)
            ),
            home.join("Documents/DirectPaymentTimesheets/payslips")
        );
        assert_eq!(expand_path_with_home(Path::new("~"), Some(home)), home);
    }

    #[test]
    fn ensure_directories_creates_missing_configured_directories() {
        let directory = tempfile::TempDir::new().unwrap();
        let first = directory.path().join("one");
        let second = directory.path().join("two").join("nested");

        ensure_directories(&[&first, &second]).unwrap();

        assert!(first.is_dir());
        assert!(second.is_dir());
    }

    #[test]
    fn explicit_paths_and_unexpandable_home_markers_are_preserved() {
        assert_eq!(
            expand_path_with_home(Path::new("/custom/payroll/payslips"), None),
            Path::new("/custom/payroll/payslips")
        );
        assert_eq!(
            expand_path_with_home(Path::new("~/payslips"), None),
            Path::new("~/payslips")
        );
    }
}
