//! OS-backed SMTP secrets. Persist only immutable opaque references, never passwords.
//! Old entries are deliberately retained for old configs/backups and interrupted saves.
use crate::config::EmailConfig;
use std::{io, path::Path};
use zeroize::Zeroizing;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
compile_error!("SMTP credential storage requires Windows Credential Manager, Linux Secret Service or macOS Keychain; no fallback is permitted.");

const SERVICE: &str = "DirectPaymentTimesheets.v1.smtp";
const UNAVAILABLE: &str = "SMTP credential store unavailable, locked, denied or missing. Unlock your OS keyring and retry/restart, or re-enter the password and Save Email Settings. Authenticated SMTP is blocked; there is no plaintext fallback.";

pub(crate) fn runtime_change_tag(email: &EmailConfig) -> u64 {
    use std::hash::BuildHasher;
    // Approval invalidation only, not authentication. A process-random keyed tag
    // avoids persisting an offline password verifier in approval digests.
    static KEY: std::sync::OnceLock<std::collections::hash_map::RandomState> =
        std::sync::OnceLock::new();
    KEY.get_or_init(Default::default)
        .hash_one(&email.smtp_password)
}

trait Store {
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn put(&self, id: &str, value: &str) -> Result<(), String>;
}
struct Native;
fn native_entry(service: &str, id: &str) -> Result<keyring::Entry, String> {
    #[cfg(target_os = "linux")]
    linux_collection_ready()?;
    // Refer to concrete backends: missing platform features must be a compile
    // error, never keyring's otherwise implicit mock-store fallback.
    #[cfg(windows)]
    let credential = keyring::windows::WinCredential::new_with_target(None, service, id);
    #[cfg(target_os = "linux")]
    let credential = keyring::secret_service::SsCredential::new_with_target(None, service, id);
    #[cfg(target_os = "macos")]
    let credential = keyring::macos::MacCredential::new_with_target(None, service, id);
    Ok(keyring::Entry::new_with_credential(Box::new(
        credential.map_err(|_| UNAVAILABLE.to_owned())?,
    )))
}

#[cfg(any(target_os = "linux", test))]
fn admit_linux_collection(
    locked: bool,
    default: &str,
    session: Option<&str>,
) -> Result<(), String> {
    if locked {
        return Err("Linux Secret Service collection is locked. Unlock it in your desktop keyring manager and retry/restart; authenticated SMTP is blocked.".into());
    }
    if default == "/" || default.ends_with("/session") || session == Some(default) {
        return Err("Linux Secret Service default collection is session-only or unverifiable. Configure an unlocked persistent default collection; no ephemeral credential fallback is permitted.".into());
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn linux_collection_ready() -> Result<(), String> {
    use dbus_secret_service::{EncryptionType, Error, SecretService};
    let service = SecretService::connect(EncryptionType::Dh).map_err(|_| UNAVAILABLE.to_owned())?;
    let collection = service
        .get_default_collection()
        .map_err(|_| UNAVAILABLE.to_owned())?;
    let session = match service.get_collection_by_alias("session") {
        Ok(collection) => Some(collection.path.to_string()),
        Err(Error::NoResult) => None,
        Err(_) => return Err(UNAVAILABLE.into()),
    };
    admit_linux_collection(
        collection.is_locked().map_err(|_| UNAVAILABLE.to_owned())?,
        &collection.path.to_string(),
        session.as_deref(),
    )
}
impl Store for Native {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        let entry = native_entry(SERVICE, id)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            // Backend errors may contain raw secret bytes/DBus diagnostics. Never format them.
            Err(_) => Err(UNAVAILABLE.into()),
        }
    }
    fn put(&self, id: &str, value: &str) -> Result<(), String> {
        native_entry(SERVICE, id)?
            .set_password(value)
            .map_err(|_| UNAVAILABLE.into())
    }
}

// Unit tests cannot silently access a developer's real credential store.
#[cfg(test)]
#[derive(Default)]
struct Memory(std::sync::Mutex<std::collections::HashMap<String, String>>);
#[cfg(test)]
impl Store for Memory {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().get(id).cloned())
    }
    fn put(&self, id: &str, value: &str) -> Result<(), String> {
        self.0.lock().unwrap().insert(id.into(), value.into());
        Ok(())
    }
}
fn store() -> &'static dyn Store {
    #[cfg(not(test))]
    {
        &Native
    }
    #[cfg(test)]
    {
        static STORE: std::sync::OnceLock<Memory> = std::sync::OnceLock::new();
        STORE.get_or_init(Memory::default)
    }
}

fn valid_id(id: &str) -> bool {
    id.strip_prefix("v1-")
        .is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn binding(email: &EmailConfig) -> String {
    // This is inside the secure store, not in entry labels or logs.
    serde_json::to_string(&(
        &email.smtp_transport,
        &email.smtp_host,
        email.smtp_port,
        &email.smtp_username,
        email.smtp_security,
    ))
    .expect("Primitive SMTP context is serializable")
}
fn read_record(store: &dyn Store, id: &str) -> Result<(String, Zeroizing<String>), String> {
    if !valid_id(id) {
        return Err("Invalid SMTP credential reference; authenticated SMTP blocked.".into());
    }
    let raw = Zeroizing::new(store.get(id)?.ok_or(UNAVAILABLE)?);
    let (context, password): (String, String) = serde_json::from_str(&raw)
        .map_err(|_| "Invalid secure SMTP credential; authenticated SMTP blocked.".to_owned())?;
    let password = Zeroizing::new(password);
    if password.is_empty() {
        return Err(UNAVAILABLE.into());
    }
    Ok((context, password))
}
fn read(store: &dyn Store, email: &EmailConfig, id: &str) -> Result<Zeroizing<String>, String> {
    let (context, password) = read_record(store, id)?;
    if context != binding(email) {
        return Err("SMTP credential does not match these settings. Re-enter the password and save; authenticated SMTP blocked.".into());
    }
    Ok(password)
}
fn write_verified(
    store: &dyn Store,
    email: &EmailConfig,
    password: &str,
) -> Result<String, String> {
    let id = format!("v1-{}", uuid::Uuid::new_v4().simple());
    // Immutable entries: never overwrite one referenced by an earlier config.
    if store.get(&id)?.is_some() {
        return Err("SMTP credential identifier collision; save refused.".into());
    }
    let raw = Zeroizing::new(
        serde_json::to_string(&(binding(email), password))
            .map_err(|_| "SMTP credential encoding failed.")?,
    );
    store.put(&id, &raw)?;
    if read(store, email, &id)?.as_str() != password {
        return Err(
            "SMTP credential write verification failed; existing configuration retained.".into(),
        );
    }
    Ok(id)
}

fn prepare_with(store: &dyn Store, email: &EmailConfig) -> Result<Option<String>, String> {
    if let Some(error) = &email.smtp_credential_error {
        if email.smtp_password.is_empty() {
            return Err(error.clone());
        }
    }
    if email.smtp_password.is_empty() {
        // An explicit clear does not delete the old entry: historical configs may use it.
        return Ok(None);
    }
    if let Some(id) = &email.smtp_credential_ref {
        if email.smtp_password == email.smtp_password_baseline {
            // Also fail closed for stale/missing/locked store, not cached RAM fallback.
            let (context, password) = read_record(store, id)?;
            if password.as_str() != email.smtp_password_baseline {
                return Err(UNAVAILABLE.into());
            }
            return if context == binding(email) {
                Ok(Some(id.clone()))
            } else {
                write_verified(store, email, &password).map(Some)
            };
        }
    }
    write_verified(store, email, &email.smtp_password).map(Some)
}
pub(crate) fn prepare_save(email: &EmailConfig) -> Result<Option<String>, String> {
    prepare_with(store(), email)
}

pub(crate) fn set_reference(value: &mut toml::Value, id: Option<String>) {
    if let Some(table) = value.get_mut("email").and_then(toml::Value::as_table_mut) {
        table.remove("smtp_password");
        table.remove("smtp_credential_ref");
        if let Some(id) = id {
            table.insert("smtp_credential_ref".into(), toml::Value::String(id));
        }
    }
}

fn has_plaintext(value: &toml::Value) -> bool {
    match value {
        toml::Value::Table(t) => t
            .iter()
            .any(|(k, v)| (k == "smtp_password" && v.as_str() != Some("")) || has_plaintext(v)),
        toml::Value::Array(a) => a.iter().any(has_plaintext),
        _ => false,
    }
}

/// Backup/restore callers transform in RAM before any new file is allocated.
/// Original bytes are preserved exactly when no plaintext secret is present.
pub(crate) fn protected_bytes(bytes: &[u8]) -> io::Result<Vec<u8>> {
    protected_with(store(), bytes).map_err(io::Error::other)
}
fn protected_with(store: &dyn Store, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "Configuration is not UTF-8; secret-safe copy refused.")?;
    let mut value: toml::Value =
        toml::from_str(text).map_err(|_| "Malformed configuration; secret-safe copy refused.")?;
    if !has_plaintext(&value) {
        return Ok(bytes.to_vec());
    }
    let email: EmailConfig = value
        .get("email")
        .ok_or("Unexpected legacy password location; copy refused without exposing its contents.")?
        .clone()
        .try_into()
        .map_err(|_| "Invalid legacy SMTP configuration; secret-safe copy refused.")?;
    if email.smtp_password.is_empty() || email.smtp_credential_ref.is_some() {
        return Err("Conflicting or unexpected legacy SMTP credentials; original retained. Re-enter the password and save explicitly.".into());
    }
    set_reference(&mut value, None);
    if has_plaintext(&value) {
        return Err("Unexpected additional plaintext credential; copy refused.".into());
    }
    let id = write_verified(store, &email, &email.smtp_password)?;
    set_reference(&mut value, Some(id));
    if has_plaintext(&value) {
        return Err("Unexpected additional plaintext credential; copy refused.".into());
    }
    toml::to_string_pretty(&value)
        .map(String::into_bytes)
        .map_err(|_| "Secret-safe configuration encoding failed.".into())
}

pub(crate) fn load(email: &mut EmailConfig, path: &Path, original: &[u8]) {
    load_with(store(), email, path, original)
}

fn load_with(store: &dyn Store, email: &mut EmailConfig, path: &Path, original: &[u8]) {
    let result = (|| -> Result<(), String> {
        let raw: toml::Value = toml::from_str(
            std::str::from_utf8(original).map_err(|_| "Invalid configuration encoding.")?,
        )
        .map_err(|_| "Invalid configuration syntax.")?;
        if has_plaintext(&raw) {
            // Entire read/verify/replace is coordinated with normal Windows publishers.
            #[cfg(windows)]
            let _guard = crate::private_storage::publication_guard(path).map_err(|_| {
                "Credential migration could not establish safe configuration ancestry."
            })?;
            let clean = Zeroizing::new(protected_with(store, original)?);
            if crate::private_storage::open_read(path)
                .and_then(|mut f| {
                    use std::io::Read;
                    let mut now = Zeroizing::new(Vec::new());
                    f.read_to_end(&mut now)?;
                    if now.as_slice() != original {
                        return Err(io::Error::other("changed"));
                    }
                    Ok(())
                })
                .is_err()
            {
                return Err("Configuration changed during credential migration; original retained. Reload and retry.".into());
            }
            crate::config::publish_config(path,&clean,true).map_err(|_| "Credential migration publication failed. Verified secure entry retained; original configuration remains recoverable. Retry after resolving storage access.".to_owned())?;
            let value: toml::Value = toml::from_str(std::str::from_utf8(&clean).unwrap())
                .map_err(|_| "Migrated configuration decoding failed.")?;
            email.smtp_credential_ref = value
                .get("email")
                .and_then(|e| e.get("smtp_credential_ref"))
                .and_then(toml::Value::as_str)
                .map(str::to_owned);
        }
        if let Some(id) = &email.smtp_credential_ref {
            email.smtp_password = read(store, email, id)?.to_string();
        }
        email.smtp_password_baseline = email.smtp_password.clone();
        Ok(())
    })();
    if let Err(error) = result {
        use zeroize::Zeroize;
        email.smtp_password.zeroize();
        email.smtp_password_baseline.zeroize();
        email.smtp_credential_error = Some(error);
    }
}

/// Resolve before opening a socket. Cached passwords never override retrieval failure.
pub(crate) fn for_send(email: &EmailConfig) -> Result<Zeroizing<String>, String> {
    if let Some(error) = &email.smtp_credential_error {
        return Err(error.clone());
    }
    if email.smtp_password.is_empty() {
        return Err("SMTP password missing; authenticated SMTP blocked.".into());
    }
    if let Some(id) = &email.smtp_credential_ref {
        let stored = read(store(), email, id)?;
        if email.smtp_password == email.smtp_password_baseline {
            return Ok(stored);
        }
    }
    // Explicit unsaved password edits retain the existing test-email workflow,
    // but must be written/read-verified in the OS store BEFORE connecting.
    let id = write_verified(store(), email, &email.smtp_password)?;
    read(store(), email, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn email() -> EmailConfig {
        let mut e = EmailConfig::default();
        e.smtp_password = "SYNTHETIC_SECRET".into();
        e.smtp_username = "fixture".into();
        e
    }
    #[test]
    fn linux_locked_and_ephemeral_collections_are_explicitly_refused() {
        assert!(
            admit_linux_collection(false, "/org/freedesktop/secrets/collection/login", None)
                .is_ok()
        );
        assert!(
            admit_linux_collection(true, "/org/freedesktop/secrets/collection/login", None)
                .unwrap_err()
                .contains("locked")
        );
        assert!(
            admit_linux_collection(false, "/org/freedesktop/secrets/collection/session", None)
                .is_err()
        );
        assert!(
            admit_linux_collection(false, "/custom/ephemeral", Some("/custom/ephemeral")).is_err()
        );
        assert!(admit_linux_collection(false, "/", None).is_err());
    }
    #[test]
    fn serializers_and_debug_never_include_password_or_baseline() {
        let mut e = email();
        e.smtp_password_baseline = "BASELINE_SECRET".into();
        for text in [
            toml::to_string(&e).unwrap(),
            serde_json::to_string(&e).unwrap(),
            format!("{e:?}"),
        ] {
            assert!(!text.contains("SYNTHETIC_SECRET"));
            assert!(!text.contains("BASELINE_SECRET"));
            assert!(!text.contains("smtp_password"));
        }
    }
    #[test]
    fn immutable_references_preserve_old_password_and_reject_wrong_context() {
        let store = Memory::default();
        let mut e = email();
        let id = write_verified(&store, &e, &e.smtp_password).unwrap();
        e.smtp_password = "NEW_SECRET".into();
        let new = write_verified(&store, &e, &e.smtp_password).unwrap();
        assert_ne!(id, new);
        assert_eq!(read(&store, &e, &id).unwrap().as_str(), "SYNTHETIC_SECRET");
        e.smtp_host = "different.invalid".into();
        assert!(read(&store, &e, &id).is_err());
        assert!(read(&store, &e, "../../../other").is_err());
    }
    struct Broken(bool);
    impl Store for Broken {
        fn get(&self, _: &str) -> Result<Option<String>, String> {
            if self.0 {
                Ok(None)
            } else {
                Err(UNAVAILABLE.into())
            }
        }
        fn put(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
    }
    #[test]
    fn locked_missing_and_unverified_writes_never_fall_back() {
        let mut e = email();
        for b in [Broken(true), Broken(false)] {
            assert!(write_verified(&b, &e, &e.smtp_password).is_err());
        }
        e.smtp_credential_ref = Some(format!("v1-{}", "a".repeat(32)));
        e.smtp_password_baseline = e.smtp_password.clone();
        assert!(prepare_with(&Broken(true), &e).is_err());
        assert!(prepare_with(&Broken(false), &e).is_err());
    }
    #[test]
    fn legacy_backup_migration_is_secret_free_and_failure_retains_input() {
        let input = b"[email]\nsmtp_password='SYNTHETIC_SECRET'\nsmtp_username='fixture'\n";
        let clean = protected_with(&Memory::default(), input).unwrap();
        let text = String::from_utf8(clean).unwrap();
        assert!(!text.contains("SYNTHETIC_SECRET"));
        assert!(!text.contains("smtp_password"));
        assert!(text.contains("smtp_credential_ref"));
        assert!(protected_with(&Broken(false), input).is_err());
        assert!(std::str::from_utf8(input)
            .unwrap()
            .contains("SYNTHETIC_SECRET"));
        assert!(protected_with(&Memory::default(), b"smtp_password='SYNTHETIC_SECRET'").is_err());
    }
    fn legacy_fixture(path: &Path) -> Vec<u8> {
        let mut cfg = crate::config::AppConfig::default();
        cfg.email = email();
        let mut value = toml::Value::try_from(&cfg).unwrap();
        value.as_table_mut().unwrap().insert(
            "future_metadata".into(),
            toml::Value::String("keep-me".into()),
        );
        value["email"].as_table_mut().unwrap().insert(
            "smtp_password".into(),
            toml::Value::String("SYNTHETIC_SECRET".into()),
        );
        let bytes = toml::to_string_pretty(&value).unwrap().into_bytes();
        crate::private_storage::write_new(path, &bytes).unwrap();
        bytes
    }
    #[test]
    fn config_migration_verifies_store_and_preserves_unknown_settings() {
        let dir = crate::private_storage::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        legacy_fixture(&path);
        let cfg = crate::config::AppConfig::load(&path).unwrap();
        assert!(cfg.email.smtp_credential_error.is_none());
        assert_eq!(cfg.email.smtp_password, "SYNTHETIC_SECRET");
        let disk = std::fs::read_to_string(&path).unwrap();
        assert!(!disk.contains("SYNTHETIC_SECRET"));
        assert!(!disk.contains("smtp_password"));
        assert!(disk.contains("keep-me"));
        assert_eq!(for_send(&cfg.email).unwrap().as_str(), "SYNTHETIC_SECRET");
    }
    #[cfg(windows)]
    #[test]
    fn failed_atomic_migration_preserves_plaintext_original_and_blocks_auth() {
        let dir = crate::private_storage::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let before = legacy_fixture(&path);
        let mut p = std::fs::metadata(&path).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(&path, p).unwrap();
        let cfg = crate::config::AppConfig::load(&path).unwrap();
        assert!(cfg.email.smtp_credential_error.is_some());
        assert!(cfg.email.smtp_password.is_empty());
        assert!(for_send(&cfg.email).is_err());
        assert!(cfg.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let mut p = std::fs::metadata(&path).unwrap().permissions();
        p.set_readonly(false);
        std::fs::set_permissions(&path, p).unwrap();
        let recovered = crate::config::AppConfig::load(&path).unwrap();
        assert!(recovered.email.smtp_credential_error.is_none());
        assert_eq!(recovered.email.smtp_password, "SYNTHETIC_SECRET");
    }
    #[test]
    fn missing_references_block_cached_password_but_not_relay_configuration() {
        let mut e = email();
        e.smtp_credential_ref = Some(format!("v1-{}", uuid::Uuid::new_v4().simple()));
        e.smtp_password_baseline = e.smtp_password.clone();
        assert!(for_send(&e).is_err());
        assert!(prepare_save(&e).is_err());
        e.smtp_credential_error = Some(UNAVAILABLE.into());
        assert!(for_send(&e).is_err());
        e.smtp_username.clear();
        e.smtp_password.clear();
        e.smtp_credential_error = None;
        assert_eq!(prepare_save(&e).unwrap(), None);
    }
    #[test]
    fn retrieval_failure_opens_no_smtp_connection() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut e = email();
        e.smtp_transport = "SMTP Server".into();
        e.smtp_host = "127.0.0.1".into();
        e.smtp_port = listener.local_addr().unwrap().port();
        e.test_sender_address = "sender@example.invalid".into();
        e.payroll_test_email_address = "target@example.invalid".into();
        e.smtp_credential_ref = Some(format!("v1-{}", uuid::Uuid::new_v4().simple()));
        e.smtp_password_baseline = e.smtp_password.clone();
        let message = crate::email_service::prepare_smtp_test_message(
            &e,
            crate::email_settings::TestRecipient::Payroll,
        )
        .unwrap();
        assert!(crate::email_service::send_prepared_timesheet(&e, &message).is_err());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    #[cfg(windows)]
    #[test]
    fn failed_password_rotation_preserves_old_reference_and_no_stage_leaks() {
        let dir = crate::private_storage::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = crate::config::AppConfig::default();
        cfg.email = email();
        cfg.save(&path).unwrap();
        let old = crate::config::AppConfig::load(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut updated = crate::config::AppConfig::load(&path).unwrap();
        updated.email.smtp_password = "NEW_ROTATED_SECRET".into();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        assert!(updated.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(for_send(&old.email).unwrap().as_str(), "SYNTHETIC_SECRET");
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains("SYNTHETIC_SECRET"));
            assert!(!text.contains("NEW_ROTATED_SECRET"));
        }
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        updated.save(&path).unwrap();
        let new = crate::config::AppConfig::load(&path).unwrap();
        assert_eq!(for_send(&new.email).unwrap().as_str(), "NEW_ROTATED_SECRET");
        assert_eq!(for_send(&old.email).unwrap().as_str(), "SYNTHETIC_SECRET");
    }
    #[test]
    #[ignore = "Explicit native synthetic credential write/read/delete test; run separately, never uses production entries"]
    fn native_store_synthetic_roundtrip() {
        let id = format!("v1-{}", uuid::Uuid::new_v4().simple());
        let entry = native_entry("DirectPaymentTimesheets.Tests.smtp", &id).unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
        entry.set_password("SYNTHETIC_NATIVE_SECRET").unwrap();
        assert_eq!(entry.get_password().unwrap(), "SYNTHETIC_NATIVE_SECRET");
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
    }

    #[cfg(windows)]
    struct SyntheticNative(std::sync::Mutex<Vec<String>>);
    #[cfg(windows)]
    impl Store for SyntheticNative {
        fn get(&self, id: &str) -> Result<Option<String>, String> {
            match native_entry("DirectPaymentTimesheets.Tests.smtp", id)?.get_password() {
                Ok(value) => Ok(Some(value)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(UNAVAILABLE.into()),
            }
        }
        fn put(&self, id: &str, value: &str) -> Result<(), String> {
            // Record before writing, so unwinding also cleans partially successful writes.
            self.0.lock().unwrap().push(id.into());
            native_entry("DirectPaymentTimesheets.Tests.smtp", id)?
                .set_password(value).map_err(|_| UNAVAILABLE.into())
        }
    }
    #[cfg(windows)]
    impl Drop for SyntheticNative {
        fn drop(&mut self) {
            for id in self.0.get_mut().unwrap().iter() {
                let entry = native_entry("DirectPaymentTimesheets.Tests.smtp", id).unwrap();
                entry.delete_credential().unwrap();
                assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
            }
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Child harness; invoked by native_synthetic_migration_recovery_and_restart"]
    fn native_synthetic_restart_child() {
        let path = std::path::PathBuf::from(std::env::var_os("DPT_SYNTHETIC_SMTP_CONFIG").unwrap());
        let parent = path.parent().unwrap();
        assert!(parent.file_name().unwrap().to_string_lossy().starts_with(".dpt-private-"));
        assert_eq!(parent.parent().unwrap().canonicalize().unwrap(), std::env::temp_dir().canonicalize().unwrap());
        assert_eq!(path.file_name().unwrap(), "synthetic-config.toml");
        use std::io::Read;
        let mut bytes = Vec::new();
        crate::private_storage::open_read(&path).unwrap().read_to_end(&mut bytes).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("SYNTHETIC_SECRET"));
        let mut cfg: crate::config::AppConfig = toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let store = SyntheticNative(Default::default());
        load_with(&store, &mut cfg.email, &path, &bytes);
        assert!(cfg.email.smtp_credential_error.is_none());
        assert_eq!(cfg.email.smtp_password, "SYNTHETIC_SECRET");
        // Simulate a denied provider despite a usable cached password: no fallback.
        assert!(read(&Broken(false), &cfg.email, cfg.email.smtp_credential_ref.as_ref().unwrap()).is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Explicit native synthetic migration/recovery/process-restart test; no production namespace"]
    fn native_synthetic_migration_recovery_and_restart() {
        let dir = crate::private_storage::tempdir().unwrap();
        let path = dir.path().join("synthetic-config.toml");
        let original = legacy_fixture(&path);
        let store = SyntheticNative(Default::default());
        let mut cfg: crate::config::AppConfig = toml::from_str(std::str::from_utf8(&original).unwrap()).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        load_with(&store, &mut cfg.email, &path, &original);
        assert!(cfg.email.smtp_credential_error.is_some());
        assert!(cfg.email.smtp_password.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&path, permissions).unwrap();
        let mut cfg: crate::config::AppConfig = toml::from_str(std::str::from_utf8(&original).unwrap()).unwrap();
        load_with(&store, &mut cfg.email, &path, &original);
        assert!(cfg.email.smtp_credential_error.is_none());
        assert_eq!(cfg.email.smtp_password, "SYNTHETIC_SECRET");
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            assert!(!String::from_utf8_lossy(&std::fs::read(entry.unwrap().path()).unwrap()).contains("SYNTHETIC_SECRET"));
        }
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "smtp_credentials::tests::native_synthetic_restart_child", "--ignored", "--test-threads=1"])
            .env("DPT_SYNTHETIC_SMTP_CONFIG", &path).output().unwrap();
        assert!(child.status.success(), "Synthetic restart child failed: {}", String::from_utf8_lossy(&child.stdout));
    }
}
