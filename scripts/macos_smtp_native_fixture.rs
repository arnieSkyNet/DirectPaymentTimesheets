// Included INSIDE smtp_credentials::tests by the hash-checked CI overlay only.
// Never compiled into the application. Ordinary tests continue using Memory.
#[cfg(target_os = "macos")]
mod macos_ci {
    use super::*;
    const SERVICE: &str = "DirectPaymentTimesheets.Tests.macos.smtp";
    #[derive(Default)]
    struct Keychain(std::sync::Mutex<Vec<String>>);
    impl Store for Keychain {
        fn get(&self, id: &str) -> Result<Option<String>, String> {
            match native_entry(SERVICE, id)?.get_password() {
                Ok(value) => Ok(Some(value)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(UNAVAILABLE.into()),
            }
        }
        fn put(&self, id: &str, value: &str) -> Result<(), String> {
            let mut ids = self.0.lock().unwrap();
            if !ids.iter().any(|saved| saved == id) {
                ids.push(id.into());
            }
            native_entry(SERVICE, id)?
                .set_password(value)
                .map_err(|_| UNAVAILABLE.into())
        }
    }
    impl Drop for Keychain {
        fn drop(&mut self) {
            // No assertions containing secret values or provider diagnostics.
            let mut clean = true;
            for id in self.0.get_mut().unwrap().iter() {
                match native_entry(SERVICE, id) {
                    Ok(entry) => {
                        clean &= matches!(
                            entry.delete_credential(),
                            Ok(()) | Err(keyring::Error::NoEntry)
                        );
                        clean &= matches!(entry.get_password(), Err(keyring::Error::NoEntry));
                    }
                    Err(_) => clean = false,
                }
            }
            if !clean {
                // Fail even while unwinding without a second panic.
                std::process::exit(71);
            }
        }
    }
    fn audit(dir: &Path) {
        for file in std::fs::read_dir(dir).unwrap() {
            let bytes = std::fs::read(file.unwrap().path()).unwrap();
            assert!(!bytes
                .windows(b"SYNTHETIC_SECRET".len())
                .any(|w| w == b"SYNTHETIC_SECRET"));
        }
    }
    #[test]
    #[ignore = "Explicit GitHub-hosted macOS synthetic native Keychain test"]
    fn migration_restart_missing_and_denied() {
        assert_eq!(std::env::var("DPT_MACOS_SYNTHETIC_CI").as_deref(), Ok("1"));
        let dir = crate::private_storage::tempdir().unwrap();
        let path = dir.path().join("synthetic-config.toml");
        let original = legacy_fixture(&path);
        let store = Keychain::default();
        // Explicitly simulated provider outage: failed migration keeps the
        // original recoverable legacy input and clears the in-memory password.
        let mut failed: crate::config::AppConfig =
            toml::from_str(std::str::from_utf8(&original).unwrap()).unwrap();
        load_with(&Broken(false), &mut failed.email, &path, &original);
        assert!(failed.email.smtp_credential_error.is_some());
        assert!(failed.email.smtp_password.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let mut cfg: crate::config::AppConfig =
            toml::from_str(std::str::from_utf8(&original).unwrap()).unwrap();
        load_with(&store, &mut cfg.email, &path, &original);
        assert!(cfg.email.smtp_credential_error.is_none());
        assert!(cfg.email.smtp_password == "SYNTHETIC_SECRET");
        let id = cfg.email.smtp_credential_ref.clone().unwrap();
        assert!(read(&store, &cfg.email, &id).unwrap().as_str() == "SYNTHETIC_SECRET");
        assert_eq!(
            prepare_with(&store, &cfg.email).unwrap().as_ref(),
            Some(&id)
        );
        let saved = std::fs::read(&path).unwrap();
        assert!(String::from_utf8_lossy(&saved).contains("keep-me"));
        crate::private_storage::write_new(&dir.path().join("synthetic-backup.toml"), &saved)
            .unwrap();
        audit(dir.path());
        // Real fresh process: same native backend, no inherited memory store.
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "smtp_credentials::tests::macos_ci::restart_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env("DPT_MACOS_SYNTHETIC_CONFIG", &path)
            .output()
            .unwrap();
        assert!(child.status.success());
        assert!(!child.stdout.windows(16).any(|w| w == b"SYNTHETIC_SECRET"));
        // Denied-provider simulation is explicitly mocked; missing entry below is native.
        assert!(read(&Broken(false), &cfg.email, &id).is_err());
        assert!(prepare_with(&Broken(false), &cfg.email).is_err());
        let missing = format!("v1-{}", uuid::Uuid::new_v4().simple());
        assert!(read(&store, &cfg.email, &missing).is_err());
        native_entry(SERVICE, &id)
            .unwrap()
            .delete_credential()
            .unwrap();
        assert!(read(&store, &cfg.email, &id).is_err());
        assert!(prepare_with(&store, &cfg.email).is_err());
        audit(dir.path());
    }
    #[test]
    #[ignore = "Child of migration_restart_missing_and_denied only"]
    fn restart_child() {
        assert_eq!(std::env::var("DPT_MACOS_SYNTHETIC_CI").as_deref(), Ok("1"));
        let path =
            std::path::PathBuf::from(std::env::var_os("DPT_MACOS_SYNTHETIC_CONFIG").unwrap());
        assert_eq!(path.file_name().unwrap(), "synthetic-config.toml");
        let parent = path.parent().unwrap();
        assert!(parent
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".dpt-private-"));
        assert_eq!(
            parent.parent().unwrap().canonicalize().unwrap(),
            std::env::temp_dir().canonicalize().unwrap()
        );
        let bytes = std::fs::read(&path).unwrap();
        let mut cfg: crate::config::AppConfig =
            toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let store = Keychain::default();
        load_with(&store, &mut cfg.email, &path, &bytes);
        assert!(cfg.email.smtp_credential_error.is_none());
        assert!(cfg.email.smtp_password == "SYNTHETIC_SECRET");
        audit(parent);
    }
}
