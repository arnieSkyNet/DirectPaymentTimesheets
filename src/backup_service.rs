use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use chrono::Local;
use rusqlite::{Connection, DatabaseName, OpenFlags};

const DATABASE_BACKUP_NAME: &str = "database.sqlite";
const CONFIG_BACKUP_NAME: &str = "config.toml";
const README_NAME: &str = "README.txt";
const BACKUP_IDENTITY: &str = "DirectPaymentTimesheets backup";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub directory_name: String,
    pub created_at: String,
    pub has_config: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BackupValidation {
    pub schema_version: i64,
}

/// Approval is bound to the exact database, configuration and selected backup.
#[derive(Clone, Debug)]
pub struct RestorePlan {
    pub backup: PathBuf,
    pub schema_version: i64,
    pub replaced_rows: Vec<(String, usize)>,
    live_path: PathBuf,
    staged_directory: std::sync::Arc<tempfile::TempDir>,
    staged_database: String,
    live_database: String,
    backup_database: String,
    live_config: Option<String>,
    backup_config: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RestoreAuthorisation {
    pub plan: RestorePlan,
    pub reason: String,
    pub acknowledged: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RestoreResult {
    pub restored_backup: PathBuf,
    pub safety_backup: PathBuf,
    pub config_restored: bool,
}

#[derive(Debug)]
pub struct BackupError(pub(crate) String);

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BackupError {}

#[derive(Debug)]
pub struct RestoreError {
    error: BackupError,
    restart_required: bool,
}

impl RestoreError {
    pub fn restart_required(&self) -> bool {
        self.restart_required
    }
}

impl From<BackupError> for RestoreError {
    fn from(error: BackupError) -> Self {
        Self {
            error,
            restart_required: false,
        }
    }
}

impl fmt::Display for RestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for RestoreError {}

pub struct BackupService;

impl BackupService {
    pub fn discover(backups_dir: &Path) -> Result<Vec<BackupInfo>, BackupError> {
        let entries = fs::read_dir(backups_dir).map_err(|error| {
            BackupError(format!(
                "Could not read backups directory {}: {error}",
                backups_dir.display()
            ))
        })?;
        let mut backups = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|error| {
                BackupError(format!("Could not read a backup directory entry: {error}"))
            })?;
            let path = entry.path();
            let directory_name = entry.file_name().to_string_lossy().to_string();
            if !entry
                .file_type()
                .map_err(|error| {
                    BackupError(format!(
                        "Could not inspect backup entry {}: {error}",
                        path.display()
                    ))
                })?
                .is_dir()
                || !is_timestamp_name(&directory_name)
            {
                continue;
            }

            let readme = match fs::read_to_string(path.join(README_NAME)) {
                Ok(readme) if has_backup_identity(&readme) => readme,
                _ => continue,
            };
            let created_at = readme
                .lines()
                .find_map(|line| line.strip_prefix("Created: "))
                .unwrap_or(&directory_name)
                .to_string();
            backups.push(BackupInfo {
                has_config: path.join(CONFIG_BACKUP_NAME).is_file(),
                path,
                directory_name,
                created_at,
            });
        }

        backups.sort_by(|left, right| right.directory_name.cmp(&left.directory_name));
        Ok(backups)
    }

    pub fn create(
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
    ) -> Result<PathBuf, BackupError> {
        let created_at = Local::now();
        // Fractional seconds avoid collisions between upgrade and restore recovery copies.
        let name = format!(
            "{}-{}",
            created_at.format("%Y%m%d-%H%M%S"),
            created_at.timestamp_subsec_nanos()
        );
        Self::create_with_metadata(
            database_path,
            config_path,
            backups_dir,
            &name,
            &created_at.to_rfc3339(),
        )
    }

    fn create_with_metadata(
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
        directory_name: &str,
        created_at: &str,
    ) -> Result<PathBuf, BackupError> {
        fs::create_dir_all(backups_dir).map_err(|error| {
            BackupError(format!(
                "Could not create backups directory {}: {error}",
                backups_dir.display()
            ))
        })?;

        let backup_dir = backups_dir.join(directory_name);
        fs::create_dir(&backup_dir).map_err(|error| {
            BackupError(format!(
                "Could not create backup directory {}: {error}",
                backup_dir.display()
            ))
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&backup_dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| BackupError(e.to_string()))?;
        }
        let result = Self::populate_backup(database_path, config_path, &backup_dir, created_at);
        if let Err(error) = result {
            if let Err(cleanup_error) = fs::remove_dir_all(&backup_dir) {
                return Err(BackupError(format!(
                    "{error} The incomplete backup at {} could not be removed: {cleanup_error}",
                    backup_dir.display()
                )));
            }

            return Err(error.into());
        }

        #[cfg(unix)]
        fs::File::open(backups_dir)
            .and_then(|f| f.sync_all())
            .map_err(|e| BackupError(format!("Could not flush backup directory: {e}")))?;
        Ok(backup_dir)
    }

    pub fn validate(
        backup_dir: &Path,
        backups_dir: &Path,
    ) -> Result<BackupValidation, BackupError> {
        validate_backup_location(backup_dir, backups_dir)?;

        let readme_path = backup_dir.join(README_NAME);
        reject_symlink(&readme_path, "backup manifest")?;
        let readme = fs::read_to_string(&readme_path).map_err(|error| {
            BackupError(format!(
                "Could not read backup manifest {}: {error}",
                readme_path.display()
            ))
        })?;
        if !has_backup_identity(&readme) {
            return Err(BackupError(format!(
                "{} is not identified as a DirectPaymentTimesheets backup.",
                backup_dir.display()
            )));
        }
        if !readme.lines().any(|line| line == "- database.sqlite") {
            return Err(BackupError(
                "The backup manifest does not list database.sqlite.".to_string(),
            ));
        }

        let config_path = backup_dir.join(CONFIG_BACKUP_NAME);
        let manifest_lists_config = readme.lines().any(|line| line == "- config.toml");
        if config_path.exists() {
            reject_symlink(&config_path, "backup configuration file")?;
            if !manifest_lists_config {
                return Err(BackupError(
                    "The backup contains config.toml but the manifest does not list it."
                        .to_string(),
                ));
            }
        } else if manifest_lists_config {
            return Err(BackupError(
                "The backup manifest lists config.toml, but that file is missing.".to_string(),
            ));
        }

        let database_path = backup_dir.join(DATABASE_BACKUP_NAME);
        reject_symlink(&database_path, "backup database")?;
        let connection = open_read_only(&database_path)?;
        let integrity_results = connection
            .prepare("PRAGMA integrity_check")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|error| {
                BackupError(format!(
                    "Could not run SQLite integrity_check on {}: {error}",
                    database_path.display()
                ))
            })?;
        if integrity_results.len() != 1 || !integrity_results[0].eq_ignore_ascii_case("ok") {
            return Err(BackupError(format!(
                "SQLite integrity_check failed for {}: {}",
                database_path.display(),
                integrity_results.join("; ")
            )));
        }

        for table in [
            "schema_version",
            "timesheets",
            "employers",
            "personal_assistants",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .map_err(|error| {
                    BackupError(format!("Could not inspect backup schema: {error}"))
                })?;
            if !exists && !matches!(table, "employers" | "personal_assistants") {
                return Err(BackupError(format!(
                    "The backup database is not recognisable as DirectPaymentTimesheets: required table {table} is missing."
                )));
            }
        }

        let versions = connection
            .prepare("SELECT version FROM schema_version")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|error| {
                BackupError(format!("Could not read backup schema version: {error}"))
            })?;
        if versions.len() != 1
            || !(1..=crate::database::CURRENT_SCHEMA_VERSION).contains(&versions[0])
        {
            return Err(BackupError(format!(
                "The backup has an unsupported DirectPaymentTimesheets schema version: {:?} (supported: 1 through {}).",
                versions,
                crate::database::CURRENT_SCHEMA_VERSION
            )));
        }

        if versions[0] >= 2 {
            for table in ["employers", "personal_assistants"] {
                let exists: bool = connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)",
                        [table],
                        |r| r.get(0),
                    )
                    .map_err(|e| BackupError(e.to_string()))?;
                if !exists {
                    return Err(BackupError(format!("Backup schema is missing {table}")));
                }
            }
        }
        Ok(BackupValidation {
            schema_version: versions[0],
        })
    }

    pub fn create_verified(
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
    ) -> Result<PathBuf, BackupError> {
        let backup = Self::create(database_path, config_path, backups_dir)?;
        Self::validate(&backup, backups_dir)?;
        Ok(backup)
    }

    pub fn preview_restore(
        backup_dir: &Path,
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
    ) -> Result<RestorePlan, BackupError> {
        let validation = Self::validate(backup_dir, backups_dir)?;
        let result = (|| -> crate::database_recovery::Result<RestorePlan> {
            let live = crate::database::open(database_path)?;
            let _dispatch = crate::timesheet_delivery::production_lock(&live)?;
            let tx = live.unchecked_transaction()?;
            if crate::database_recovery::unresolved(&tx)? {
                return Err("Restore blocked: resolve all uncertain email outcomes with evidence before reviewing a rollback.".into());
            }
            let source = open_read_only(&backup_dir.join(DATABASE_BACKUP_NAME))?;
            let source_fingerprint = crate::database_recovery::fingerprint(&source)?;
            let config_fingerprint =
                crate::database_recovery::file_fingerprint(&backup_dir.join(CONFIG_BACKUP_NAME))?;
            let (temporary, staged) = crate::database_recovery::staged_backup(backup_dir)?;
            if crate::database_recovery::fingerprint(&source)? != source_fingerprint {
                return Err("Selected backup changed during preparation".into());
            }
            if let Some(expected) = &config_fingerprint {
                fs::copy(
                    backup_dir.join(CONFIG_BACKUP_NAME),
                    temporary.path().join(CONFIG_BACKUP_NAME),
                )?;
                if crate::database_recovery::file_fingerprint(
                    &temporary.path().join(CONFIG_BACKUP_NAME),
                )?
                .as_ref()
                    != Some(expected)
                {
                    return Err("Selected configuration changed during preparation".into());
                }
            }
            Ok(RestorePlan {
                backup: backup_dir.canonicalize()?,
                schema_version: validation.schema_version,
                replaced_rows: crate::database_recovery::losses(&tx, &staged)?,
                live_path: database_path.canonicalize()?,
                staged_database: crate::database_recovery::fingerprint(&staged)?,
                staged_directory: std::sync::Arc::new(temporary),
                live_database: crate::database_recovery::fingerprint(&tx)?,
                backup_database: source_fingerprint,
                live_config: crate::database_recovery::file_fingerprint(config_path)?,
                backup_config: config_fingerprint,
            })
        })();
        result.map_err(|e| BackupError(e.to_string()))
    }

    pub fn restore(
        backup_dir: &Path,
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
        approval: &RestoreAuthorisation,
    ) -> Result<RestoreResult, RestoreError> {
        // Validate before opening live data; no safety snapshot is made for invalid input.
        Self::validate(backup_dir, backups_dir)?;
        if !approval.acknowledged || approval.reason.trim().is_empty() {
            return Err(BackupError(
                "Restore requires explicit rollback acknowledgement and a documented reason."
                    .into(),
            )
            .into());
        }
        let result = (|| -> crate::database_recovery::Result<RestoreResult> {
            let mut live = crate::database::open(database_path)?;
            let _dispatch = crate::timesheet_delivery::production_lock(&live)?;
            live.pragma_update(None, "foreign_keys", false)?;
            let tx = live.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if crate::database_recovery::unresolved(&tx)? {
                return Err("Restore blocked: uncertain delivery evidence must be resolved first; rollback cannot release an uncertain attempt.".into());
            }
            let source = open_read_only(&backup_dir.join(DATABASE_BACKUP_NAME))?;
            if approval.plan.live_path != database_path.canonicalize()?
                || approval.plan.backup != backup_dir.canonicalize()?
                || approval.plan.live_database != crate::database_recovery::fingerprint(&tx)?
                || approval.plan.backup_database != crate::database_recovery::fingerprint(&source)?
                || approval.plan.live_config
                    != crate::database_recovery::file_fingerprint(config_path)?
                || approval.plan.backup_config
                    != crate::database_recovery::file_fingerprint(
                        &backup_dir.join(CONFIG_BACKUP_NAME),
                    )?
            {
                return Err("Restore approval is stale: live data, configuration or selected backup changed. Review and authorise again.".into());
            }
            let staged = crate::database::open(
                approval.plan.staged_directory.path().join("restore.sqlite"),
            )?;
            if crate::database_recovery::fingerprint(&staged)? != approval.plan.staged_database {
                return Err("Reviewed isolated restore copy changed; restore refused.".into());
            }
            // Recheck the captured source after staging (selected backups are never changed).
            if approval.plan.backup_database
                != crate::database_recovery::fingerprint(&open_read_only(
                    &backup_dir.join(DATABASE_BACKUP_NAME),
                )?)?
            {
                return Err("Selected backup changed during preparation; restore refused.".into());
            }
            let safety_backup = Self::create_verified(database_path, config_path, backups_dir)?;
            use std::io::Write;
            let mut manifest = OpenOptions::new()
                .append(true)
                .open(safety_backup.join(README_NAME))?;
            writeln!(manifest,"\nExplicit destructive restore authorised at {}\nActor: local employer\nSelected backup: {}\nReason: {}\nCurrent database fingerprint: {}\nSelected database fingerprint: {}\nCurrent rows absent/different in selected data: {:?}\nRestoring old email evidence can cause duplicate payroll processing; consult this recovery copy and Payroll before sending.",Local::now().to_rfc3339(),backup_dir.display(),approval.reason.trim(),approval.plan.live_database,approval.plan.backup_database,approval.plan.replaced_rows)?;
            manifest.sync_all()?;
            let selected_config = approval
                .plan
                .staged_directory
                .path()
                .join(CONFIG_BACKUP_NAME);
            if crate::database_recovery::file_fingerprint(&selected_config)?
                != approval.plan.backup_config
            {
                return Err("Reviewed isolated configuration changed; restore refused".into());
            }
            let staged_config = if selected_config.is_file() {
                Some(stage_config(&selected_config, config_path)?)
            } else {
                None
            };
            if let Err(e) = crate::database_recovery::install(&tx, &staged)
                .and_then(|_| tx.commit().map_err(Into::into))
            {
                if let Some(path) = staged_config {
                    let _ = fs::remove_file(path);
                }
                return Err(format!("Restore failed; live database transaction rolled back. Verified recovery: {}. {e}",safety_backup.display()).into());
            }
            if let Some(path) = staged_config {
                if let Err(e) = fs::rename(&path, config_path) {
                    return Err(format!("DATABASE RESTORED; RESTART REQUIRED: configuration could not be restored: {e}. Verified recovery: {}",safety_backup.display()).into());
                }
            }
            Ok(RestoreResult {
                restored_backup: backup_dir.to_path_buf(),
                safety_backup,
                config_restored: selected_config.is_file(),
            })
        })();
        result.map_err(|e| {
            let message = e.to_string();
            RestoreError {
                restart_required: message.contains("RESTART REQUIRED"),
                error: BackupError(message),
            }
        })
    }

    fn populate_backup(
        database_path: &Path,
        config_path: &Path,
        backup_dir: &Path,
        created_at: &str,
    ) -> Result<(), BackupError> {
        let source = open_read_only(database_path).map_err(|error| {
            BackupError(format!(
                "Could not open database {} for backup: {error}",
                database_path.display()
            ))
        })?;
        let read_snapshot = source
            .unchecked_transaction()
            .map_err(|e| BackupError(e.to_string()))?;
        let expected = crate::database_recovery::fingerprint(&read_snapshot)
            .map_err(|e| BackupError(e.to_string()))?;
        let database_backup_path = backup_dir.join(DATABASE_BACKUP_NAME);
        source
            .backup(DatabaseName::Main, &database_backup_path, None)
            .map_err(|error| {
                BackupError(format!(
                    "Could not back up database to {}: {error}",
                    database_backup_path.display()
                ))
            })?;

        let verified = open_read_only(&database_backup_path)?;
        crate::database_recovery::integrity(&verified).map_err(|e| BackupError(e.to_string()))?;
        if crate::database_recovery::fingerprint(&verified)
            .map_err(|e| BackupError(e.to_string()))?
            != expected
        {
            return Err(BackupError(
                "Backup content verification failed; original database was not modified.".into(),
            ));
        }
        sync_file(&database_backup_path)
            .map_err(|e| BackupError(format!("Could not flush database backup: {e}")))?;
        let mut backed_up_files = vec![DATABASE_BACKUP_NAME];
        if config_path.exists() {
            let config_backup_path = backup_dir.join(CONFIG_BACKUP_NAME);
            fs::copy(config_path, &config_backup_path).map_err(|error| {
                BackupError(format!(
                    "Could not copy configuration file {} to {}: {error}",
                    config_path.display(),
                    config_backup_path.display()
                ))
            })?;
            if fs::read(config_path).map_err(|e| BackupError(e.to_string()))?
                != fs::read(&config_backup_path).map_err(|e| BackupError(e.to_string()))?
            {
                return Err(BackupError(
                    "Configuration backup verification failed".into(),
                ));
            }
            sync_file(&config_backup_path).map_err(|e| BackupError(e.to_string()))?;
            backed_up_files.push(CONFIG_BACKUP_NAME);
        }

        let file_list = backed_up_files
            .iter()
            .map(|name| format!("- {name}"))
            .collect::<Vec<_>>()
            .join("\n");
        let inventory = recovery_inventory(&source, config_path)?;
        let readme = format!(
            "{BACKUP_IDENTITY}\n\nCreated: {created_at}\n\nBacked-up files:\n{file_list}\n\nVerified consistent SQLite snapshot (including committed WAL data): {expected}\nThis is DATABASE AND CONFIGURATION recovery, not a complete application-data backup.\nExternal PDFs, payslips, signatures, imported/archive files and fonts are NOT copied or restored.\nBefore an upgrade/rollback, close every application instance and separately protect the full application root and these business files/folders. Do not copy a live SQLite file without its WAL; prefer this verified snapshot.\nUse 1.0.6 Restore Backup to validate and migrate legacy data on an isolated copy. A rollback needs fresh explicit approval; unresolved email uncertainty blocks it. Keep old binaries away from upgraded data: their restore tools can overwrite newer schemas.\nRecovery inventory (paths only; no external files copied):\n{inventory}\n"
        );
        let readme_path = backup_dir.join(README_NAME);
        fs::write(&readme_path, readme).map_err(|error| {
            BackupError(format!(
                "Could not write backup manifest {}: {error}",
                readme_path.display()
            ))
        })?;

        sync_file(&readme_path).map_err(|e| BackupError(e.to_string()))?;
        #[cfg(unix)]
        fs::File::open(backup_dir)
            .and_then(|f| f.sync_all())
            .map_err(|e| BackupError(e.to_string()))?;
        Ok(())
    }
}

fn sync_file(path: &Path) -> std::io::Result<()> {
    // Windows FlushFileBuffers requires a writable handle.
    OpenOptions::new().write(true).open(path)?.sync_all()
}

fn recovery_inventory(db: &Connection, config: &Path) -> Result<String, BackupError> {
    let mut paths = std::collections::BTreeSet::new();
    if let Some(root) = config.parent() {
        paths.insert(format!(
            "Application root: {} (including internal import/archive folders)",
            root.display()
        ));
    }
    if config.is_file() {
        // Read raw TOML only: loading AppConfig would create business directories.
        if let Ok(value) = fs::read_to_string(config)
            .unwrap_or_default()
            .parse::<toml::Value>()
        {
            for section in ["folders", "pdf"] {
                if let Some(values) = value.get(section).and_then(toml::Value::as_table) {
                    for (key, value) in values {
                        if let Some(path) = value.as_str() {
                            paths.insert(format!("{section}.{key}: {path}"));
                        }
                    }
                }
            }
        }
    }
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|e| BackupError(e.to_string()))?;
    for table in tables {
        let columns = db
            .prepare(&format!(
                "PRAGMA table_info({})",
                crate::database_recovery::quote(&table)
            ))
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|e| BackupError(e.to_string()))?;
        for column in columns.iter().filter(|c| {
            matches!(
                c.as_str(),
                "signature"
                    | "employer_signature"
                    | "stored_path"
                    | "pdf_path"
                    | "source_path"
                    | "destination_path"
            )
        }) {
            let sql = format!(
                "SELECT DISTINCT {} FROM {} WHERE {} IS NOT NULL",
                crate::database_recovery::quote(column),
                crate::database_recovery::quote(&table),
                crate::database_recovery::quote(column)
            );
            let values = db
                .prepare(&sql)
                .and_then(|mut s| {
                    s.query_map([], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .map_err(|e| BackupError(e.to_string()))?;
            for path in values {
                if !path.trim().is_empty() {
                    paths.insert(format!("{table}.{column}: {path}"));
                }
            }
        }
    }
    Ok(paths.into_iter().collect::<Vec<_>>().join("\n"))
}

fn is_timestamp_name(name: &str) -> bool {
    let base = name.get(..15).unwrap_or("");
    base.len() == 15
        && base.as_bytes()[8] == b'-'
        && base
            .bytes()
            .enumerate()
            .all(|(n, b)| n == 8 || b.is_ascii_digit())
        && (name.len() == 15
            || name.get(15..).is_some_and(|s| {
                s.starts_with('-') && s.len() > 1 && s[1..].bytes().all(|b| b.is_ascii_digit())
            }))
}

fn has_backup_identity(readme: &str) -> bool {
    readme.lines().next() == Some(BACKUP_IDENTITY)
}

fn validate_backup_location(backup_dir: &Path, backups_dir: &Path) -> Result<(), BackupError> {
    let name = backup_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| BackupError("The selected backup has an invalid directory name.".into()))?;
    if !is_timestamp_name(name) {
        return Err(BackupError(format!(
            "The selected directory {} does not use the DirectPaymentTimesheets backup timestamp format YYYYMMDD-HHMMSS.",
            backup_dir.display()
        )));
    }
    reject_symlink(backup_dir, "backup directory")?;

    let canonical_root = backups_dir.canonicalize().map_err(|error| {
        BackupError(format!(
            "Could not resolve backups directory {}: {error}",
            backups_dir.display()
        ))
    })?;
    let canonical_backup = backup_dir.canonicalize().map_err(|error| {
        BackupError(format!(
            "Could not resolve selected backup directory {}: {error}",
            backup_dir.display()
        ))
    })?;
    if canonical_backup.parent() != Some(canonical_root.as_path()) {
        return Err(BackupError(
            "Only direct child directories of the configured backups directory can be restored."
                .into(),
        ));
    }
    Ok(())
}

fn reject_symlink(path: &Path, description: &str) -> Result<(), BackupError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        BackupError(format!(
            "Could not inspect {description} {}: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(BackupError(format!(
            "Refusing to use {description} {} because it is a symbolic link.",
            path.display()
        )));
    }
    Ok(())
}

fn open_read_only(path: &Path) -> Result<Connection, BackupError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|error| {
        BackupError(format!(
            "Could not open backup database {} read-only: {error}",
            path.display()
        ))
    })
}

fn stage_config(source: &Path, destination: &Path) -> Result<PathBuf, BackupError> {
    let parent = destination.parent().ok_or_else(|| {
        BackupError(format!(
            "Configuration path {} has no parent directory.",
            destination.display()
        ))
    })?;
    let staged = parent.join(format!(".config.toml.restore-{}", std::process::id()));
    let contents = fs::read(source).map_err(|error| {
        BackupError(format!(
            "Could not read backup configuration {}: {error}",
            source.display()
        ))
    })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .map_err(|error| {
            BackupError(format!(
                "Could not create staged configuration {}: {error}",
                staged.display()
            ))
        })?;
    use std::io::Write;
    file.write_all(&contents).map_err(|error| {
        BackupError(format!(
            "Could not write staged configuration {}: {error}",
            staged.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        BackupError(format!(
            "Could not flush staged configuration {}: {error}",
            staged.display()
        ))
    })?;
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn restore_test(
        backup: &Path,
        live: &Path,
        config: &Path,
        root: &Path,
    ) -> Result<RestoreResult, RestoreError> {
        let plan = BackupService::preview_restore(backup, live, config, root)?;
        BackupService::restore(
            backup,
            live,
            config,
            root,
            &RestoreAuthorisation {
                plan,
                reason: "Verified test rollback".into(),
                acknowledged: true,
            },
        )
    }

    fn create_test_database(path: &Path) {
        let connection = crate::database::open(path).unwrap();
        connection
            .execute("CREATE TABLE example (value TEXT NOT NULL)", [])
            .unwrap();
        connection
            .execute("INSERT INTO example (value) VALUES ('expected data')", [])
            .unwrap();
    }

    fn create_application_database(path: &Path, marker: &str) {
        let connection = crate::database::open(path).unwrap();
        crate::database::create_schema(&connection).unwrap();
        connection
            .execute("CREATE TABLE restore_test_marker (value TEXT NOT NULL)", [])
            .unwrap();
        connection
            .execute(
                "INSERT INTO restore_test_marker (value) VALUES (?1)",
                [marker],
            )
            .unwrap();
    }

    fn read_marker(path: &Path) -> String {
        crate::database::open(path)
            .unwrap()
            .query_row("SELECT value FROM restore_test_marker", [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    fn create_named_backup(
        database_path: &Path,
        config_path: &Path,
        backups_dir: &Path,
        name: &str,
    ) -> PathBuf {
        BackupService::create_with_metadata(
            database_path,
            config_path,
            backups_dir,
            name,
            "2026-08-31T14:25:30+01:00",
        )
        .unwrap()
    }

    #[test]
    fn creates_openable_database_backup_with_expected_data_and_copies_config() {
        let temp_dir = TempDir::new().unwrap();
        let database_path = temp_dir.path().join("source.sqlite");
        let config_path = temp_dir.path().join("source.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_test_database(&database_path);
        fs::write(&config_path, "theme = \"dark\"\n").unwrap();

        let backup_dir = BackupService::create_with_metadata(
            &database_path,
            &config_path,
            &backups_dir,
            "20260831-142530",
            "2026-08-31T14:25:30+01:00",
        )
        .unwrap();

        assert_eq!(backup_dir, backups_dir.join("20260831-142530"));
        assert!(backup_dir.is_dir());
        let backup_connection =
            crate::database::open(backup_dir.join(DATABASE_BACKUP_NAME)).unwrap();
        let value: String = backup_connection
            .query_row("SELECT value FROM example", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "expected data");
        assert_eq!(
            fs::read_to_string(backup_dir.join(CONFIG_BACKUP_NAME)).unwrap(),
            "theme = \"dark\"\n"
        );

        let readme = fs::read_to_string(backup_dir.join(README_NAME)).unwrap();
        assert!(readme.contains("DirectPaymentTimesheets backup"));
        assert!(readme.contains("Created: 2026-08-31T14:25:30+01:00"));
        assert!(readme.contains("- database.sqlite"));
        assert!(readme.contains("- config.toml"));
    }

    #[test]
    fn succeeds_without_a_config_file_and_omits_it_from_the_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let database_path = temp_dir.path().join("source.sqlite");
        let missing_config_path = temp_dir.path().join("missing.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_test_database(&database_path);

        let backup_dir = BackupService::create_with_metadata(
            &database_path,
            &missing_config_path,
            &backups_dir,
            "20260831-142531",
            "2026-08-31T14:25:31+01:00",
        )
        .unwrap();

        assert!(backup_dir.join(DATABASE_BACKUP_NAME).is_file());
        assert!(!backup_dir.join(CONFIG_BACKUP_NAME).exists());
        let readme = fs::read_to_string(backup_dir.join(README_NAME)).unwrap();
        assert!(readme.contains("- database.sqlite"));
        assert!(!readme.contains("- config.toml"));
    }

    #[test]
    fn validates_and_restores_application_database_config_and_creates_safety_backup() {
        let temp_dir = TempDir::new().unwrap();
        let live_database = temp_dir.path().join("database.sqlite");
        let selected_database = temp_dir.path().join("selected.sqlite");
        let live_config = temp_dir.path().join("config.toml");
        let selected_config = temp_dir.path().join("selected.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_application_database(&live_database, "current live data");
        create_application_database(&selected_database, "selected backup data");
        fs::write(&live_config, "current config").unwrap();
        fs::write(&selected_config, "selected config").unwrap();
        let selected_backup = create_named_backup(
            &selected_database,
            &selected_config,
            &backups_dir,
            "20260101-010101",
        );

        let validation = BackupService::validate(&selected_backup, &backups_dir).unwrap();
        assert_eq!(
            validation.schema_version,
            crate::database::CURRENT_SCHEMA_VERSION
        );

        let result =
            restore_test(&selected_backup, &live_database, &live_config, &backups_dir).unwrap();

        assert_eq!(result.restored_backup, selected_backup);
        assert!(result.safety_backup.is_dir());
        assert_ne!(result.safety_backup, result.restored_backup);
        assert_eq!(
            read_marker(&result.safety_backup.join(DATABASE_BACKUP_NAME)),
            "current live data"
        );
        assert_eq!(read_marker(&live_database), "selected backup data");
        assert_eq!(fs::read_to_string(&live_config).unwrap(), "selected config");
        assert_eq!(
            fs::read_to_string(result.safety_backup.join(CONFIG_BACKUP_NAME)).unwrap(),
            "current config"
        );
        assert!(selected_backup.is_dir());
    }

    #[test]
    fn rejects_corrupt_backup_without_altering_live_data_or_creating_safety_backup() {
        let temp_dir = TempDir::new().unwrap();
        let live_database = temp_dir.path().join("database.sqlite");
        let live_config = temp_dir.path().join("config.toml");
        let backups_dir = temp_dir.path().join("backups");
        let corrupt_backup = backups_dir.join("20260101-010101");
        create_application_database(&live_database, "current live data");
        fs::write(&live_config, "current config").unwrap();
        fs::create_dir_all(&corrupt_backup).unwrap();
        fs::write(corrupt_backup.join(DATABASE_BACKUP_NAME), "not sqlite").unwrap();
        fs::write(
            corrupt_backup.join(README_NAME),
            format!("{BACKUP_IDENTITY}\n\nCreated: test\n\nBacked-up files:\n- database.sqlite\n"),
        )
        .unwrap();

        let error =
            restore_test(&corrupt_backup, &live_database, &live_config, &backups_dir).unwrap_err();

        assert!(error.to_string().contains("integrity_check"));
        assert_eq!(read_marker(&live_database), "current live data");
        assert_eq!(fs::read_to_string(&live_config).unwrap(), "current config");
        assert_eq!(fs::read_dir(&backups_dir).unwrap().count(), 1);
    }

    #[test]
    fn rejects_sqlite_database_that_is_not_direct_payment_timesheets() {
        let temp_dir = TempDir::new().unwrap();
        let live_database = temp_dir.path().join("database.sqlite");
        let unrelated_database = temp_dir.path().join("unrelated.sqlite");
        let live_config = temp_dir.path().join("config.toml");
        let missing_config = temp_dir.path().join("missing.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_application_database(&live_database, "current live data");
        create_test_database(&unrelated_database);
        fs::write(&live_config, "current config").unwrap();
        let unrelated_backup = create_named_backup(
            &unrelated_database,
            &missing_config,
            &backups_dir,
            "20260101-010101",
        );

        let error = restore_test(
            &unrelated_backup,
            &live_database,
            &live_config,
            &backups_dir,
        )
        .unwrap_err();

        assert!(error.to_string().contains("not recognisable"));
        assert_eq!(read_marker(&live_database), "current live data");
        assert_eq!(fs::read_to_string(&live_config).unwrap(), "current config");
        assert_eq!(fs::read_dir(&backups_dir).unwrap().count(), 1);
    }

    #[test]
    fn restores_database_but_preserves_current_config_when_backup_has_none() {
        let temp_dir = TempDir::new().unwrap();
        let live_database = temp_dir.path().join("database.sqlite");
        let selected_database = temp_dir.path().join("selected.sqlite");
        let live_config = temp_dir.path().join("config.toml");
        let missing_config = temp_dir.path().join("missing.toml");
        let backups_dir = temp_dir.path().join("backups");
        create_application_database(&live_database, "current live data");
        create_application_database(&selected_database, "selected backup data");
        fs::write(&live_config, "current config").unwrap();
        let selected_backup = create_named_backup(
            &selected_database,
            &missing_config,
            &backups_dir,
            "20260101-010101",
        );

        let result =
            restore_test(&selected_backup, &live_database, &live_config, &backups_dir).unwrap();

        assert!(!result.config_restored);
        assert_eq!(read_marker(&live_database), "selected backup data");
        assert_eq!(fs::read_to_string(&live_config).unwrap(), "current config");
        assert!(result.safety_backup.join(CONFIG_BACKUP_NAME).is_file());
    }
}
