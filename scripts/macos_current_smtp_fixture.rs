#[cfg(test)]
mod current_keychain_acceptance {
    use super::*;
    fn disposable() {
        assert_eq!(std::env::consts::OS, "macos");
        assert_eq!(
            std::env::var("DPT_MACOS_CURRENT_ACCEPTANCE").as_deref(),
            Ok("1")
        );
    }
    fn config() -> crate::config::AppConfig {
        let mut c = crate::config::AppConfig::default();
        c.email.smtp_transport = "SMTP Server".into();
        c.email.smtp_host = "smtp.example.invalid".into();
        c.email.smtp_username = "synthetic@example.invalid".into();
        c.email.smtp_security = Some(crate::config::SmtpSecurity::RequiredStarttls);
        c
    }
    #[test]
    fn password_child() {
        disposable();
        let root = std::path::PathBuf::from(std::env::var_os("DPT_SYNTHETIC_CONFIG_ROOT").unwrap());
        let ns = std::env::var("DPT_SYNTHETIC_NAMESPACE").unwrap();
        let store = NativeAcceptanceStore::new(&ns);
        let path = root.join("config.toml");
        let raw = std::fs::read(&path).unwrap();
        let mut c: crate::config::AppConfig =
            toml::from_str(std::str::from_utf8(&raw).unwrap()).unwrap();
        load_with(&store, &mut c.email, &path, &raw);
        assert!(c.email.smtp_credential_error.is_none());
        assert_eq!(c.email.smtp_password, "MACOS_SYNTHETIC_PASSWORD_OLD");
    }
    #[test]
    fn native_password_lifecycle() {
        disposable();
        let ns = uuid::Uuid::new_v4().to_string();
        let store = NativeAcceptanceStore::new(&ns);
        let dir = crate::private_storage::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let c = config();
        let mut value = toml::Value::try_from(&c).unwrap();
        value["email"].as_table_mut().unwrap().insert(
            "smtp_password".into(),
            toml::Value::String("MACOS_SYNTHETIC_PASSWORD_OLD".into()),
        );
        let raw = toml::to_string(&value).unwrap().into_bytes();
        crate::private_storage::write_new(&path, &raw).unwrap();
        // Exercise the actual migration/publication functions with the scoped native store.
        let mut c: crate::config::AppConfig =
            toml::from_str(std::str::from_utf8(&raw).unwrap()).unwrap();
        load_with(&store, &mut c.email, &path, &raw);
        assert!(c.email.smtp_credential_error.is_none());
        let old = c.email.smtp_credential_ref.clone().unwrap();
        assert_eq!(
            read(&store, &c.email, &old).unwrap().as_str(),
            "MACOS_SYNTHETIC_PASSWORD_OLD"
        );
        assert_eq!(prepare_with(&store, &c.email).unwrap(), Some(old.clone()));
        let clean = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&clean);
        assert!(!text.contains("MACOS_SYNTHETIC_PASSWORD_OLD"));
        assert!(!text.contains("smtp_password"));
        let backup = protected_with(&store, &clean).unwrap();
        assert_eq!(backup, clean);
        crate::private_storage::write_new(&dir.path().join("backup-config.toml"), &backup).unwrap();
        let legacy_backup = protected_with(&store, &raw).unwrap();
        assert!(!String::from_utf8_lossy(&legacy_backup).contains("MACOS_SYNTHETIC_PASSWORD_OLD"));
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "smtp_credentials::current_keychain_acceptance::password_child",
                "--test-threads=1",
            ])
            .env("DPT_SYNTHETIC_CONFIG_ROOT", dir.path())
            .env("DPT_SYNTHETIC_NAMESPACE", &ns)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "synthetic password child failed; output withheld"
        );
        c.email.smtp_password = "MACOS_SYNTHETIC_PASSWORD_NEW".into();
        let new = prepare_with(&store, &c.email).unwrap().unwrap();
        assert_ne!(old, new);
        assert_eq!(
            read(&store, &c.email, &new).unwrap().as_str(),
            "MACOS_SYNTHETIC_PASSWORD_NEW"
        );
        assert_eq!(
            read(&store, &c.email, &old).unwrap().as_str(),
            "MACOS_SYNTHETIC_PASSWORD_OLD"
        );
        let mut wrong = c.email.clone();
        wrong.smtp_username = "other@example.invalid".into();
        assert!(read(&store, &wrong, &new).is_err());
        // Scope guard records every write and confirms native deletion/absence.
        drop(store);
        println!("PASS native password migration, backup sanitization, cross-process retrieval, replacement/history binding and scoped cleanup");
    }
}
