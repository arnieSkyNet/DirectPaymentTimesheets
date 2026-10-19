//! Explicit, debug-only testing profile. Never copies or discovers production data.
use std::{
    cell::Cell,
    fs::File,
    io,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

const MARKER: &[u8] = b"DirectPaymentTimesheets isolated Windows development profile v1\n";
static PROFILE: OnceLock<PathBuf> = OnceLock::new();
// Retain ancestry handles for the entire process, including configuration I/O.
static HELD: OnceLock<Vec<File>> = OnceLock::new();
thread_local! { static ROOT_SCOPE: Cell<bool> = const { Cell::new(false) }; }

pub fn root() -> PathBuf {
    // Molly-only development profile. Neither runtime environment variables nor
    // current directory can redirect it to existing production data.
    PathBuf::from(r"C:\Users\Wizdom\AppData\Local\DirectPaymentTimesheets-v107-development")
}

fn confined(root: &Path, path: &Path) -> bool {
    path.is_absolute() && path.starts_with(root) && path != root
        && !path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        // Only the drive colon is permitted; reject NTFS alternate streams.
        && path.as_os_str().to_string_lossy().chars().filter(|c| *c == ':').count() == 1
}

pub struct RootScope(bool);
impl Drop for RootScope {
    fn drop(&mut self) {
        ROOT_SCOPE.with(|v| v.set(self.0));
    }
}

pub fn database_scope(path: &Path) -> io::Result<RootScope> {
    let Some(root) = PROFILE.get() else {
        return Ok(RootScope(false));
    };
    if !confined(root, path) {
        return Err(io::Error::other(
            "Development database path escaped the fixed private profile",
        ));
    }
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let file = Path::new(&name);
        match std::fs::symlink_metadata(file) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Ok(m) if m.is_file() && crate::private_storage::development_is_private(file)? => (),
            Err(e) => return Err(e),
            _ => {
                return Err(io::Error::other(
                    "Development database/sidecar must be a private, unlinked regular file",
                ))
            }
        }
    }
    Ok(RootScope(ROOT_SCOPE.with(|v| v.replace(true))))
}

pub fn root_exception_enabled() -> bool {
    PROFILE.get().is_some() && ROOT_SCOPE.with(Cell::get)
}

pub fn active() -> bool {
    PROFILE.get().is_some()
}

pub fn warning() -> Option<String> {
    PROFILE.get().map(|root| {
        format!(
            "DEVELOPMENT MODE — isolated test data — real SMTP sends only on request\n{}",
            root.display()
        )
    })
}

pub fn validate_config(path: &Path, config: &crate::config::AppConfig) -> io::Result<()> {
    let Some(root) = PROFILE.get() else {
        return Ok(());
    };
    if path != root.join("config.toml") {
        return Err(io::Error::other(
            "Development configuration path is not the isolated configuration",
        ));
    }
    let expected = folders(root);
    let actual = &config.folders;
    if actual.csv_import != expected.csv_import
        || actual.pdf_output != expected.pdf_output
        || actual.email_archive != expected.email_archive
        || actual.payslip_folder != expected.payslip_folder
        || actual.payroll_information_folder != expected.payroll_information_folder
    {
        return Err(io::Error::other(
            "Development business folders must remain inside the fixed profile",
        ));
    }
    for folder in [
        &actual.csv_import,
        &actual.pdf_output,
        &actual.email_archive,
        &actual.payslip_folder,
        &actual.payroll_information_folder,
    ] {
        crate::private_storage::check_path(folder)?;
    }
    Ok(())
}

pub fn validate_config_location(path: &Path) -> io::Result<()> {
    let Some(root) = PROFILE.get() else {
        return Ok(());
    };
    if path != root.join("config.toml") {
        return Err(io::Error::other(
            "Configuration outside development profile refused",
        ));
    }
    crate::private_storage::check_path(path)?;
    if !crate::private_storage::development_is_private(path)? {
        return Err(io::Error::other("Development configuration is not private"));
    }
    Ok(())
}

fn folders(root: &Path) -> crate::config::FolderConfig {
    crate::config::FolderConfig {
        csv_import: root.join("import"),
        pdf_output: root.join("pdf"),
        email_archive: root.join("emails"),
        payslip_folder: root.join("payslips"),
        payroll_information_folder: root.join("payroll-information"),
    }
}

pub fn validate_folder_draft(paths: &[PathBuf; 5]) -> io::Result<()> {
    let Some(root) = PROFILE.get() else {
        return Ok(());
    };
    let f = folders(root);
    if paths
        != &[
            f.csv_import,
            f.pdf_output,
            f.email_archive,
            f.payslip_folder,
            f.payroll_information_folder,
        ]
    {
        return Err(io::Error::other(
            "Development business folders cannot be redirected",
        ));
    }
    Ok(())
}

pub fn initialise() -> io::Result<PathBuf> {
    if !std::env::args_os().any(|a| a == "--isolated-windows-development")
        || std::env::var_os("DIRECTPAYMENTTIMESHEETS_HOME").is_some()
    {
        return Err(io::Error::other(
            "Explicit isolated launch required; no home override allowed",
        ));
    }
    let root = root();
    if !root.is_absolute() || !root.starts_with(r"C:\") {
        return Err(io::Error::other(
            "Development profile must be on the verified C: volume",
        ));
    }
    crate::private_storage::check_path(&root)?;
    let fresh = match std::fs::symlink_metadata(&root) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // Parent must exist; never create an arbitrary ancestor tree.
            crate::private_storage::create_private_dir(&root)?;
            true
        }
        Ok(m) if m.is_dir() => false,
        _ => return Err(io::Error::other("Invalid development profile directory")),
    };
    PROFILE
        .set(root.clone())
        .map_err(|_| io::Error::other("Development profile already initialised"))?;
    // Full production ancestry/parent/inheritance/identity checks, including the
    // authorized production volume-root policy. No DB/config writes yet.
    let held = crate::private_storage::development_ancestry(&root.join("database.sqlite"))?;
    HELD.set(held)
        .map_err(|_| io::Error::other("Development ancestry already held"))?;
    let marker = root.join("development-profile.txt");
    let config_path = root.join("config.toml");
    if fresh {
        use std::io::Write;
        crate::private_storage::create_new(&marker)?.write_all(MARKER)?;
        let config = initial_config(&root);
        // Create exclusively: no existing profile/configuration is overwritten.
        let bytes = toml::to_string_pretty(&config).map_err(io::Error::other)?;
        crate::private_storage::create_new(&config_path)?.write_all(bytes.as_bytes())?;
    } else {
        crate::private_storage::check_path(&marker)?;
        if !crate::private_storage::development_is_private(&marker)?
            || std::fs::read(&marker)? != MARKER
            || !config_path.is_file()
            || !root.join("database.sqlite").is_file()
        {
            return Err(io::Error::other(
                "Unrecognised or incomplete development profile; refusing to repair or adopt it",
            ));
        }
    }
    crate::private_storage::check_path(&config_path)?;
    let config = crate::config::AppConfig::load(&config_path).map_err(io::Error::other)?;
    validate_config(&config_path, &config)?;
    for folder in [
        &config.folders.csv_import,
        &config.folders.pdf_output,
        &config.folders.email_archive,
        &config.folders.payslip_folder,
        &config.folders.payroll_information_folder,
    ] {
        crate::private_storage::ensure_private_dir(folder)?;
        if !crate::private_storage::development_is_private(folder)? {
            return Err(io::Error::other(
                "Development business directory is not private",
            ));
        }
    }
    eprintln!("{}", warning().unwrap());
    Ok(root)
}

fn initial_config(root: &Path) -> crate::config::AppConfig {
    let mut config = crate::config::AppConfig::default();
    config.folders = folders(root);
    config.email.smtp_host.clear();
    config.email.smtp_username.clear();
    config.email.smtp_password.clear();
    config.email.payroll_test_email_address.clear();
    config.email.pa_test_email_address.clear();
    config.email.test_sender_address.clear();
    config.email.select_transport("SMTP Server".into());
    config.email.smtp_security = Some(crate::config::SmtpSecurity::RequiredStarttls);
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn development_path_isolation_rejects_escape_and_streams() {
        let root = root();
        assert!(confined(&root, &root.join("database.sqlite")));
        for path in [
            PathBuf::from("database.sqlite"),
            root.clone(),
            root.join("../database.sqlite"),
            root.with_file_name("molly-v107-development-other")
                .join("database.sqlite"),
            root.join("database.sqlite:stream"),
            PathBuf::from(r"C:\Users\Public\database.sqlite"),
        ] {
            assert!(!confined(&root, &path), "{}", path.display());
        }
        assert!(!root_exception_enabled());
    }
    #[test]
    fn development_folders_and_credentials_start_isolated() {
        let root = root();
        let config = initial_config(&root);
        assert!(
            config.email.smtp_host.is_empty()
                && config.email.smtp_username.is_empty()
                && config.email.smtp_password.is_empty()
                && config.email.payroll_test_email_address.is_empty()
                && config.email.pa_test_email_address.is_empty()
                && config.email.test_sender_address.is_empty()
        );
        assert_eq!(
            config.email.smtp_security,
            Some(crate::config::SmtpSecurity::RequiredStarttls)
        );
        let f = config.folders;
        for p in [
            &f.csv_import,
            &f.pdf_output,
            &f.email_archive,
            &f.payslip_folder,
            &f.payroll_information_folder,
        ] {
            assert!(confined(&root, p));
        }
    }

    // Subprocess keeps opt-in state out of every other unit test. All ACL and
    // junction changes below concern new synthetic objects only.
    #[test]
    fn development_native_isolation_persistence_and_protections() {
        const CHILD: &str = "DPT_DEVELOPMENT_SYNTHETIC_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "development::tests::development_native_isolation_persistence_and_protections",
                    "--exact",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        // Molly's general Temp directory has an unrelated modify ACE. Keep that
        // legitimate refusal: use a new private fixture under the reviewed local
        // profile parent, never change Temp's ACL or bypass an intermediate.
        let fixture = crate::private_storage::tempdir_in(root().parent().unwrap()).unwrap();
        let root = fixture.path().to_path_buf();
        let database = root.join("database.sqlite");
        // The separately approved production root policy also admits this safe
        // ancestry without development activation; no other owner is relaxed.
        drop(crate::private_storage::development_ancestry(&database).unwrap());
        PROFILE.set(root.clone()).unwrap();
        let held = crate::private_storage::development_ancestry(&database).unwrap();
        assert!(!root_exception_enabled()); // Scope must end, even on success.
        assert!(
            crate::private_storage::development_ancestry(&root.join("../escape.sqlite")).is_err()
        );
        assert!(crate::private_storage::development_ancestry(
            &root.with_extension("outside").join("db.sqlite")
        )
        .is_err());

        crate::database::initialise_database(&database).unwrap();
        let db = crate::database::open(&database).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM payroll_timesheets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        db.execute_batch("CREATE TABLE development_fixture(value TEXT); BEGIN IMMEDIATE; INSERT INTO development_fixture VALUES('synthetic');").unwrap();
        assert!(crate::private_storage::development_is_private(
            &root.join("database.sqlite-journal")
        )
        .unwrap());
        db.execute_batch(
            "COMMIT; PRAGMA journal_mode=WAL; INSERT INTO development_fixture VALUES('WAL');",
        )
        .unwrap();
        for name in [
            "database.sqlite",
            "database.sqlite-wal",
            "database.sqlite-shm",
        ] {
            assert!(crate::private_storage::development_is_private(&root.join(name)).unwrap());
        }

        let config_path = root.join("config.toml");
        let mut config = crate::config::AppConfig::default();
        config.folders = folders(&root);
        crate::private_storage::write_new(
            &config_path,
            toml::to_string_pretty(&config).unwrap().as_bytes(),
        )
        .unwrap();
        config.email.select_transport("SMTP Server".into());
        config.email.smtp_host = "smtp.synthetic.invalid".into();
        config.email.test_sender_address = "sender@example.invalid".into();
        config.email.payroll_test_email_address = "recipient@example.invalid".into();
        config.email.smtp_username = "synthetic-user".into();
        config.email.smtp_password = "synthetic-password".into();
        config.save(&config_path).unwrap();
        let reloaded = crate::config::AppConfig::load(&config_path).unwrap();
        assert_eq!(reloaded.email.smtp_host, "smtp.synthetic.invalid");
        assert_eq!(reloaded.email.smtp_password, "synthetic-password");
        assert!(crate::email_service::prepare_smtp_test_message(
            &reloaded.email,
            crate::email_settings::TestRecipient::Payroll
        )
        .is_ok());
        let original = std::fs::read(&config_path).unwrap();
        config.folders.pdf_output = PathBuf::from(r"C:\Users\Public\forbidden");
        assert!(config.save(&config_path).is_err());
        assert_eq!(std::fs::read(&config_path).unwrap(), original);
        assert!(crate::config::AppConfig::load(&root.join("missing-config.toml")).is_err());
        assert!(!root.join("missing-config.toml").exists());
        let retained_config = root.join("retained-config.toml");
        std::fs::rename(&config_path, &retained_config).unwrap();
        assert!(crate::config::AppConfig::load(&config_path).is_err());
        assert!(!config_path.exists());
        std::fs::rename(&retained_config, &config_path).unwrap();
        let config_alias = root.join("config-alias.toml");
        std::fs::hard_link(&config_path, &config_alias).unwrap();
        assert!(crate::config::AppConfig::load(&config_path).is_err());
        std::fs::remove_file(&config_alias).unwrap(); // New synthetic alias only.
        assert_eq!(std::fs::read(&config_path).unwrap(), original);
        let linked = root.join("hardlinked.sqlite");
        let original_db = root.join("original.sqlite");
        crate::private_storage::write_new(&original_db, b"synthetic retained bytes").unwrap();
        std::fs::hard_link(&original_db, &linked).unwrap();
        assert!(crate::private_storage::prepare_database(&linked).is_err());
        assert_eq!(
            std::fs::read(&original_db).unwrap(),
            b"synthetic retained bytes"
        );
        let broad_sidecar_db = root.join("broad-sidecar.sqlite");
        crate::private_storage::prepare_database(&broad_sidecar_db).unwrap();
        let sidecar = root.join("broad-sidecar.sqlite-wal");
        crate::private_storage::write_new(&sidecar, b"synthetic sidecar").unwrap();
        fixture_acl(&sidecar, false);
        assert!(crate::private_storage::prepare_database(&broad_sidecar_db).is_err());
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"synthetic sidecar");

        let intermediate = root.join("intermediate");
        let parent = intermediate.join("private-parent");
        crate::private_storage::create_private_dir(&intermediate).unwrap();
        crate::private_storage::create_private_dir(&parent).unwrap();
        let guarded =
            crate::private_storage::development_ancestry(&parent.join("positive.sqlite")).unwrap();
        let original_identity =
            same_file::Handle::from_file(guarded.last().unwrap().try_clone().unwrap()).unwrap();
        assert!(original_identity == same_file::Handle::from_path(&parent).unwrap());
        let renamed = intermediate.join("renamed");
        match std::fs::rename(&parent, &renamed) {
            Ok(()) => {
                // Metadata handles are identity anchors, not a promise that the
                // current trusted user cannot rename directories. Exercise the
                // exact equality comparison used by the production guard.
                crate::private_storage::create_private_dir(&parent).unwrap();
                assert!(original_identity != same_file::Handle::from_path(&parent).unwrap());
                std::fs::remove_dir(&parent).unwrap(); // Empty synthetic replacement only.
                std::fs::rename(&renamed, &parent).unwrap();
                assert!(original_identity == same_file::Handle::from_path(&parent).unwrap());
            }
            Err(error) => assert!(matches!(error.raw_os_error(), Some(5 | 32 | 33))),
        }
        drop(guarded);
        fixture_acl(&intermediate, false);
        let refused = parent.join("refused.sqlite");
        let error = crate::private_storage::prepare_database(&refused)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("reason=untrusted_allow_ace")
                && error.contains("role=existing_ancestor")
        );
        assert!(!refused.exists());
        let broad_parent = root.join("broad-parent");
        crate::private_storage::create_private_dir(&broad_parent).unwrap();
        fixture_acl(&broad_parent, true);
        let refused = broad_parent.join("refused.sqlite");
        let error = crate::private_storage::prepare_database(&refused)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsafe_sidecar_inheritance"));
        assert!(!refused.exists());

        let target = root.join("junction-target");
        crate::private_storage::create_private_dir(&target).unwrap();
        let link = root.join("junction");
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:DPT_LINK -Target $env:DPT_TARGET | Out-Null"])
            .env("DPT_LINK", &link).env("DPT_TARGET", &target).output().unwrap();
        assert!(output.status.success(), "junction fixture failed");
        assert!(crate::private_storage::prepare_database(&link.join("refused.sqlite")).is_err());
        assert!(!target.join("refused.sqlite").exists());
        // Remove only the new junction itself; never recurse into its target.
        std::fs::remove_dir(&link).unwrap();
        assert!(!root_exception_enabled());
        drop(db);
        drop(held);
    }

    fn fixture_acl(path: &Path, inherit_only: bool) {
        // Construct and set only a new DACL. PowerShell Set-Acl on a fetched
        // descriptor tries to set audit security and requires SeSecurityPrivilege
        // on Molly; tests must not enable privileges or change owner/SACL.
        let script = r#"
            $ErrorActionPreference='Stop'
            $directory=Test-Path -LiteralPath $env:DPT_FIXTURE -PathType Container
            $acl=if($directory){[Security.AccessControl.DirectorySecurity]::new()}else{[Security.AccessControl.FileSecurity]::new()}
            $current=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
            $flags=if($directory){'OICI'}else{''}
            $broadFlags=if($env:DPT_INHERIT_ONLY -eq '1'){'OICIIO'}else{$flags}
            $sddl='D:P(A;'+$flags+';FA;;;'+$current+')(A;'+$flags+';FA;;;SY)(A;'+$broadFlags+';FA;;;WD)'
            $acl.SetSecurityDescriptorSddlForm($sddl,[Security.AccessControl.AccessControlSections]::Access)
            if($directory){[IO.Directory]::SetAccessControl($env:DPT_FIXTURE,$acl)}else{[IO.File]::SetAccessControl($env:DPT_FIXTURE,$acl)}
        "#;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("DPT_FIXTURE", path)
            .env("DPT_INHERIT_ONLY", if inherit_only { "1" } else { "0" })
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "synthetic ACL fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
