//! New private objects only. Existing objects are assessed, never chmod'd.
//! Business directories retain their inherited/shared access policy.
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
#[path = "private_storage_linux.rs"]
mod linux_acl;
#[cfg(target_os = "macos")]
#[path = "private_storage_macos.rs"]
mod macos_acl;
#[cfg(windows)]
#[path = "private_storage_windows.rs"]
mod platform;

#[cfg(all(test, windows))]
#[path = "private_storage_windows_tests.rs"]
mod windows_verification_tests;

pub fn check_path(path: &Path) -> io::Result<()> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut prefix = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(io::Error::other("Parent traversal in managed storage path"));
        }
        prefix.push(component);
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&prefix) {
            Ok(m) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if m.file_attributes() & 0x400 != 0 {
                        return Err(io::Error::other("Reparse point in managed storage path"));
                    }
                }
                #[cfg(target_os = "macos")]
                if m.file_type().is_symlink() && verified_system_alias(&prefix, &m)? {
                    continue;
                }
                if m.file_type().is_symlink() {
                    return Err(io::Error::other("Symlink in managed storage path"));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

// Only the two root-level Apple aliases are eligible. No arbitrary canonicalization.
#[cfg(any(target_os = "macos", test))]
fn system_alias_target(alias: &Path, target: &Path) -> bool {
    if !matches!(alias.to_str(), Some("/var" | "/tmp")) {
        return false;
    }
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        Path::new("/").join(target)
    };
    resolved == Path::new("/private").join(alias.strip_prefix("/").unwrap())
        && !target
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}
#[cfg(target_os = "macos")]
fn verified_system_alias(alias: &Path, link: &fs::Metadata) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    if link.uid() != 0 || !system_alias_target(alias, &fs::read_link(alias)?) {
        return Ok(false);
    }
    let private = fs::symlink_metadata("/private")?;
    if !private.is_dir() || private.uid() != 0 || private.mode() & 0o022 != 0 {
        return Ok(false);
    }
    // Mode bits alone cannot establish that the protected alias target's
    // parent is safe on macOS: an extended ACL could grant directory writes.
    use std::os::unix::fs::OpenOptionsExt;
    let private_handle = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open("/private")?;
    if macos_acl::has_allow_entries(&private_handle)? {
        return Ok(false);
    }
    let expected = Path::new("/private").join(alias.strip_prefix("/").unwrap());
    let target = fs::symlink_metadata(expected)?;
    Ok(target.is_dir()
        && target.uid() == 0
        && (target.mode() & 0o022 == 0
            || (alias == Path::new("/tmp") && target.mode() & 0o1000 != 0)))
}

fn create_dir(path: &Path, private: bool) -> io::Result<()> {
    #[cfg(windows)]
    if private {
        return platform::create_dir(path);
    }
    #[cfg(target_os = "macos")]
    if private {
        return macos_acl::create_dir(path);
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = fs::symlink_metadata(path)?;
        if metadata.permissions().mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(io::Error::other(
                "Filesystem did not enforce private directory modes",
            ));
        }
    }
    Ok(())
}

fn ensure_dir(path: &Path, private: bool) -> io::Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    check_path(path)?;
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(io::Error::other("Storage directory is not a directory")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                ensure_dir(parent, private)?;
            }
            match create_dir(path, private) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    check_path(path)?;
                    if fs::metadata(path)?.is_dir() {
                        Ok(())
                    } else {
                        Err(e)
                    }
                }
                result => result,
            }
        }
        Err(e) => Err(e),
    }
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    ensure_dir(path, true)
}
pub fn ensure_output_dir(path: &Path) -> io::Result<()> {
    ensure_dir(path, false)
}
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    check_path(path)?;
    create_dir(path, true)
}

pub fn create_new(path: &Path) -> io::Result<File> {
    check_path(path)?;
    #[cfg(windows)]
    {
        platform::create_file(path)
    }
    #[cfg(target_os = "macos")]
    {
        macos_acl::create_file(path)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata = file.metadata()?;
            if metadata.permissions().mode() & 0o077 != 0
                || metadata.uid() != unsafe { libc::geteuid() }
            {
                return Err(io::Error::other(
                    "Filesystem did not enforce private file modes",
                ));
            }
        }
        Ok(file)
    }
}

pub fn open_read(path: &Path) -> io::Result<File> {
    open_regular(path, false)
}

/// Retain the lock inode: create privately if absent, otherwise open without
/// truncation, replacement or permission repair.
pub fn open_or_create_lock(path: &Path) -> io::Result<File> {
    match create_new(path) {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => open_regular(path, true),
        Err(error) => Err(error),
    }
}

fn open_regular(path: &Path, writable: bool) -> io::Result<File> {
    check_path(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true).write(writable);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if file.metadata()?.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("Reparse-point input refused"));
        }
    }
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Storage input is not a regular file"));
    }
    Ok(file)
}

pub fn copy_new(source: &Path, destination: &Path) -> io::Result<()> {
    let mut input = open_read(source)?;
    let mut output = create_new(destination)?;
    io::copy(&mut input, &mut output)?;
    output.sync_all()
}

pub fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub fn tempfile_in(parent: &Path) -> io::Result<tempfile::NamedTempFile> {
    check_path(parent)?;
    tempfile::Builder::new()
        .prefix(".dpt-private-")
        .make_in(parent, create_new)
}

#[derive(Debug)]
pub struct PrivateTempDir {
    path: PathBuf,
    #[cfg(target_os = "linux")]
    cleanup: Option<linux_acl::DirectoryCleanup>,
}
impl PrivateTempDir {
    pub fn path(&self) -> &Path {
        &self.path
    }
    #[cfg(test)]
    pub(crate) fn cleanup_enabled(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.cleanup.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
}
impl Drop for PrivateTempDir {
    fn drop(&mut self) {
        // Never fall back to deleting by an untrusted stored pathname. Other
        // platforms retain private staging until native cleanup is verified.
        #[cfg(target_os = "linux")]
        if let Some(cleanup) = &self.cleanup {
            let _ = cleanup.remove();
        }
    }
}

pub fn tempdir_in(parent: &Path) -> io::Result<PrivateTempDir> {
    check_path(parent)?;
    // Builder supplies randomized names, not a cryptographic guarantee. Exclusive
    // creation prevents collisions from opening or truncating existing objects.
    let temporary = tempfile::Builder::new()
        .prefix(".dpt-private-")
        .make_in(parent, create_private_dir)?;
    let (_, path) = temporary.into_parts();
    let path = path.keep()?;
    #[cfg(target_os = "linux")]
    let cleanup = linux_acl::DirectoryCleanup::capture(&path).ok();
    Ok(PrivateTempDir {
        path,
        #[cfg(target_os = "linux")]
        cleanup,
    })
}

pub fn tempdir() -> io::Result<PrivateTempDir> {
    // Resolve the OS-provided temporary root (macOS commonly exposes /var as a symlink).
    tempdir_in(&std::env::temp_dir().canonicalize()?)
}

fn operation_error(operation: &str, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!(
            "{operation}: code={:?}; class={:?}",
            error.raw_os_error(),
            error.kind()
        ),
    )
}

/// Keep our privately created file without tempfile's Windows attribute reset.
pub fn keep_tempfile(
    pending: tempfile::NamedTempFile,
) -> Result<(File, PathBuf), tempfile::PersistError> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let metadata = match pending.as_file().metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                return Err(tempfile::PersistError {
                    error,
                    file: pending,
                })
            }
        };
        if metadata.file_attributes() & 0x100 != 0 {
            return Err(tempfile::PersistError {
                error: io::Error::other(
                    "windows.keep_private: code=none; class=temporary_attribute_unsupported",
                ),
                file: pending,
            });
        }
        let (file, mut path) = pending.into_parts();
        let kept = path.to_path_buf();
        path.disable_cleanup(true);
        Ok((file, kept))
    }
    #[cfg(not(windows))]
    pending.keep()
}

pub fn publish_tempfile(
    pending: tempfile::NamedTempFile,
    destination: &Path,
    overwrite: bool,
) -> Result<File, tempfile::PersistError> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let metadata = match pending.as_file().metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                return Err(tempfile::PersistError {
                    error,
                    file: pending,
                })
            }
        };
        if metadata.file_attributes() & 0x100 != 0 {
            return Err(tempfile::PersistError {
                error: io::Error::other(
                    "windows.publish_private: code=none; class=temporary_attribute_unsupported",
                ),
                file: pending,
            });
        }
        if let Err(error) = check_path(pending.path())
            .and_then(|_| check_path(destination))
            .and_then(|_| platform::publish(pending.path(), destination, overwrite))
        {
            return Err(tempfile::PersistError {
                error,
                file: pending,
            });
        }
        let (file, mut path) = pending.into_parts();
        path.disable_cleanup(true);
        Ok(file)
    }
    #[cfg(not(windows))]
    if overwrite {
        pending.persist(destination)
    } else {
        pending.persist_noclobber(destination)
    }
}

/// Retain an existing destination's access policy on a NEW replacement inode.
/// Never alter the existing object, and refuse ownership changes on Unix.
pub fn preserve_access(temporary: &Path, destination: &Path) -> io::Result<()> {
    check_path(temporary)?;
    check_path(destination)?;
    match fs::symlink_metadata(destination) {
        Ok(m) if m.is_file() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let existing = open_read(destination)?;
                let replacement = open_read(temporary)?;
                let m = existing.metadata()?;
                let new = replacement.metadata()?;
                if m.uid() != new.uid() || m.gid() != new.gid() {
                    return Err(io::Error::other(
                        "Replacement would change ownership; review storage separately",
                    ));
                }
                #[cfg(target_os = "linux")]
                linux_acl::preserve(&replacement, &existing)?;
                #[cfg(target_os = "macos")]
                macos_acl::preserve(&replacement, &existing)?;
                replacement.set_permissions(m.permissions())?;
            }
            #[cfg(windows)]
            platform::preserve_access(temporary, destination)?;
            Ok(())
        }
        Ok(_) => Err(io::Error::other(
            "Replacement destination is not a regular file",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    check_path(path)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let original = match open_read(path) {
        Ok(file) => Some(
            same_file::Handle::from_file(file)
                .map_err(|e| operation_error("storage.write_atomic.original_identity", e))?,
        ),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(operation_error("storage.write_atomic.open_original", e)),
    };
    let mut pending =
        tempfile_in(parent).map_err(|e| operation_error("storage.write_atomic.create_stage", e))?;
    pending
        .write_all(bytes)
        .map_err(|e| operation_error("storage.write_atomic.write_stage", e))?;
    pending
        .as_file()
        .sync_all()
        .map_err(|e| operation_error("storage.write_atomic.sync_stage", e))?;
    preserve_access(pending.path(), path)
        .map_err(|e| operation_error("storage.write_atomic.preserve", e))?;
    if let Some(original) = original {
        check_path(path)?;
        if original
            != same_file::Handle::from_path(path)
                .map_err(|e| operation_error("storage.write_atomic.identity", e))?
        {
            return Err(io::Error::other(
                "Publication destination changed; original retained",
            ));
        }
        publish_tempfile(pending, path, true)
            .map_err(|e| operation_error("storage.write_atomic.publish", e.error))?;
    } else {
        publish_tempfile(pending, path, false)
            .map_err(|e| operation_error("storage.write_atomic.publish_new", e.error))?;
    }
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn prepare_database(path: &Path) -> io::Result<()> {
    check_path(path)?;
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        check_path(Path::new(&sidecar))?;
    }
    #[cfg(windows)]
    let _ancestors = platform::database_ancestor_guard(path)?;
    #[cfg(windows)]
    {
        // SQLite creates sidecars using the parent DACL, not the database DACL.
        // Refuse unsafe legacy parents rather than silently repairing their ACLs.
        if !platform::is_private(path.parent().unwrap_or(Path::new(".")))? {
            return Err(io::Error::other("Database parent ACL is not private; assess permissions and review a separate repair before opening SQLite"));
        }
    }
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => (),
        Ok(_) => return Err(io::Error::other("Database is not a regular file")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            create_new(path)?;
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

/// Bounded, metadata-only assessment. No file contents or permissions are changed.
pub fn assess(root: &Path, shared: bool) -> Vec<String> {
    let mut messages = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0;
    while let Some(path) = pending.pop() {
        count += 1;
        if count > 10_000 {
            messages.push(
                "Assessment stopped at 10,000 entries; review remaining storage separately.".into(),
            );
            break;
        }
        let result = (|| -> io::Result<()> {
            check_path(&path)?;
            let m = fs::symlink_metadata(&path)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                if m.uid() != unsafe { libc::geteuid() } {
                    messages.push(format!("{}: owner differs from the current account; review separately, no automatic repair.", path.display()));
                }
                if m.permissions().mode() & 0o077 != 0 {
                    messages.push(format!("{}: access extends beyond the owner{}; review separately, no automatic repair.", path.display(), if shared {" (may be intentional sharing)"} else {""}));
                }
            }
            #[cfg(windows)]
            if !platform::is_private(&path)? {
                messages.push(format!("{}: ACL allows access beyond the owner/System/administrators{}; review separately, no automatic repair.",path.display(),if shared {" (may be intentional sharing)"} else {""}));
            }
            #[cfg(target_os = "linux")]
            if m.is_file() && linux_acl::access_acl(&open_read(&path)?)?.is_some() {
                messages.push(format!(
                    "{}: named POSIX access ACL present; review intended sharing separately.",
                    path.display()
                ));
            }
            #[cfg(target_os = "macos")]
            if macos_acl::has_allow_entries(&File::open(&path)?)? {
                messages.push(format!(
                    "{}: extended ACL allow entries present; review intended sharing separately.",
                    path.display()
                ));
            }
            if m.is_dir() {
                for entry in fs::read_dir(&path)? {
                    if count + pending.len() >= 10_000 {
                        messages.push("Directory enumeration limited to 10,000 entries; review remaining storage separately.".into());
                        break;
                    }
                    pending.push(entry?.path());
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            messages.push(format!(
                "{}: assessment unavailable ({:?}); no change made.",
                path.display(),
                e.kind()
            ));
        }
    }
    if messages.is_empty() {
        messages.push("No broader access found in the inspected metadata. Ancestor ownership, privileged access and encryption require separate review.".into());
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_tempdir_cleanup_refuses_substituted_directory() {
        let root = tempdir().unwrap();
        let stage = tempdir_in(root.path()).unwrap();
        let original_path = stage.path().to_path_buf();
        let retained_stage = root.path().join("retained-stage");
        write_new(&original_path.join("reviewed"), b"synthetic reviewed bytes").unwrap();
        fs::rename(&original_path, &retained_stage).unwrap();
        create_private_dir(&original_path).unwrap();
        write_new(
            &original_path.join("unrelated"),
            b"synthetic unrelated bytes",
        )
        .unwrap();
        drop(stage);
        assert_eq!(
            fs::read(retained_stage.join("reviewed")).unwrap(),
            b"synthetic reviewed bytes"
        );
        assert!(
            original_path.join("unrelated").is_file(),
            "Cleanup must not delete a replacement directory at the old staging path"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn phase1_tempdir_cleanup_removes_original_in_private_parent() {
        let root = tempdir().unwrap();
        let stage = tempdir_in(root.path()).unwrap();
        assert!(stage.cleanup_enabled());
        let path = stage.path().to_path_buf();
        write_new(&path.join("synthetic"), b"synthetic staging bytes").unwrap();
        drop(stage);
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn phase1_tempdir_cleanup_refuses_substituted_symlink() {
        let root = tempdir().unwrap();
        let stage = tempdir_in(root.path()).unwrap();
        let path = stage.path().to_path_buf();
        let retained = root.path().join("retained-stage");
        fs::rename(&path, &retained).unwrap();
        let target = root.path().join("unrelated-target");
        create_private_dir(&target).unwrap();
        write_new(&target.join("retained"), b"synthetic unrelated bytes").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        drop(stage);
        assert!(fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read(target.join("retained")).unwrap(),
            b"synthetic unrelated bytes"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn phase1_tempdir_cleanup_anchors_original_parent_after_ancestor_substitution() {
        let root = tempdir().unwrap();
        let parent = root.path().join("parent");
        ensure_private_dir(&parent).unwrap();
        let stage = tempdir_in(&parent).unwrap();
        let path = stage.path().to_path_buf();
        let name = path.file_name().unwrap().to_os_string();
        let retained_parent = root.path().join("retained-parent");
        fs::rename(&parent, &retained_parent).unwrap();
        ensure_private_dir(&parent).unwrap();
        ensure_private_dir(&path).unwrap();
        write_new(&path.join("unrelated"), b"synthetic unrelated bytes").unwrap();
        drop(stage);
        assert!(!retained_parent.join(name).exists());
        assert_eq!(
            fs::read(path.join("unrelated")).unwrap(),
            b"synthetic unrelated bytes"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn phase1_tempdir_cleanup_retains_staging_in_unprotected_shared_parent() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir().unwrap();
        let shared = root.path().join("synthetic-shared");
        create_private_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).unwrap();
        let stage = tempdir_in(&shared).unwrap();
        let path = stage.path().to_path_buf();
        write_new(&path.join("retained"), b"synthetic recovery bytes").unwrap();
        drop(stage);
        assert_eq!(
            fs::read(path.join("retained")).unwrap(),
            b"synthetic recovery bytes"
        );
        assert_eq!(
            fs::metadata(&shared).unwrap().permissions().mode() & 0o777,
            0o777
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn phase1_tempdir_cleanup_refuses_parent_permissions_broadened_after_creation() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir().unwrap();
        let parent = root.path().join("private-parent");
        create_private_dir(&parent).unwrap();
        let stage = tempdir_in(&parent).unwrap();
        assert!(stage.cleanup_enabled());
        let path = stage.path().to_path_buf();
        write_new(&path.join("retained"), b"synthetic recovery bytes").unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o777)).unwrap();
        drop(stage);
        assert_eq!(
            fs::read(path.join("retained")).unwrap(),
            b"synthetic recovery bytes"
        );
        assert_eq!(
            fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
            0o777
        );
    }

    #[test]
    fn phase1_private_copies_and_publication_do_not_overwrite_inputs() {
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("copy");
        fs::write(&source, b"synthetic bytes").unwrap();
        copy_new(&source, &destination).unwrap();
        assert!(copy_new(&source, &destination).is_err());
        assert_eq!(fs::read(&source).unwrap(), fs::read(&destination).unwrap());
        let temporary = tempfile_in(root.path()).unwrap();
        assert!(temporary.persist_noclobber(&destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"synthetic bytes");
    }

    #[test]
    fn phase1_existing_objects_and_assessment_are_non_destructive() {
        let root = tempdir().unwrap();
        let directory = root.path().join("shared");
        fs::create_dir(&directory).unwrap();
        let file = directory.join("existing");
        fs::write(&file, b"original").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o770)).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        }
        let before_dir = fs::metadata(&directory).unwrap().permissions();
        let before = fs::metadata(&file).unwrap().permissions();
        ensure_private_dir(&directory).unwrap();
        prepare_database(&file).unwrap();
        drop(open_or_create_lock(&file).unwrap());
        let _ = assess(&directory, true);
        assert_eq!(before_dir, fs::metadata(&directory).unwrap().permissions());
        assert_eq!(before, fs::metadata(&file).unwrap().permissions());
        assert_eq!(fs::read(&file).unwrap(), b"original");
        write_atomic(&file, b"replacement").unwrap();
        assert_eq!(before, fs::metadata(&file).unwrap().permissions());
        assert_eq!(fs::read(&file).unwrap(), b"replacement");
    }

    #[cfg(unix)]
    #[test]
    fn phase1_symlinks_are_refused_without_touching_targets() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        fs::write(&source, b"retained").unwrap();
        let link = root.path().join("link");
        symlink(&source, &link).unwrap();
        assert!(write_atomic(&link, b"changed").is_err());
        assert!(prepare_database(&link).is_err());
        assert!(open_or_create_lock(&link).is_err());
        assert!(copy_new(&link, &root.path().join("copy")).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"retained");
        let dirlink = root.path().join("directory-link");
        symlink(root.path(), &dirlink).unwrap();
        assert!(ensure_private_dir(&dirlink.join("child")).is_err());
        let database = root.path().join("live.sqlite");
        create_new(&database).unwrap();
        symlink(&source, root.path().join("live.sqlite-wal")).unwrap();
        assert!(prepare_database(&database).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn phase1_permissive_umask_child() {
        // Process isolation: never alter the global umask of parallel test workers.
        if std::env::var_os("DPT_PHASE1_UMASK_CHILD").is_none() {
            return;
        }
        unsafe {
            libc::umask(0);
        }
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let default = tempfile::tempdir().unwrap();
        assert_eq!(
            mode(default.path()),
            0o777,
            "verify locked tempfile directory behavior"
        );
        let root = tempdir().unwrap();
        assert_eq!(mode(root.path()), 0o700);
        let nested = root.path().join("private/nested");
        ensure_private_dir(&nested).unwrap();
        assert_eq!(mode(&nested), 0o700);
        assert_eq!(mode(nested.parent().unwrap()), 0o700);
        let source = root.path().join("source");
        fs::write(&source, b"synthetic").unwrap();
        let copy = root.path().join("copy");
        copy_new(&source, &copy).unwrap();
        assert_eq!(mode(&copy), 0o600);
        assert_eq!(mode(&source), 0o666);
        let output = root.path().join("new.pdf");
        write_atomic(&output, b"synthetic PDF").unwrap();
        assert_eq!(mode(&output), 0o600);
        let csv = crate::archive::archive_csv_bytes(
            Path::new("synthetic.csv"),
            b"synthetic CSV",
            &root.path().join("archive"),
        )
        .unwrap();
        assert_eq!(mode(&csv), 0o600);
        let path = root.path().join("live.sqlite");
        let db = crate::database::open(&path).unwrap();
        assert_eq!(mode(&path), 0o600);
        let _lock = crate::timesheet_delivery::production_lock(&db).unwrap();
        assert_eq!(
            mode(&root.path().join("live.timesheet-delivery.lock")),
            0o600
        );
        db.execute_batch("CREATE TABLE fixture(value TEXT); BEGIN IMMEDIATE; INSERT INTO fixture VALUES('synthetic');").unwrap();
        assert_eq!(mode(&root.path().join("live.sqlite-journal")), 0o600);
        db.execute_batch(
            "COMMIT; PRAGMA journal_mode=WAL; INSERT INTO fixture VALUES('synthetic WAL');",
        )
        .unwrap();
        assert_eq!(mode(&root.path().join("live.sqlite-wal")), 0o600);
        assert_eq!(mode(&root.path().join("live.sqlite-shm")), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn phase1_permissive_umask() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "private_storage::tests::phase1_permissive_umask_child",
                "--nocapture",
            ])
            .env("DPT_PHASE1_UMASK_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_acl_tests {
    use super::*;
    use std::os::fd::AsRawFd;
    #[test]
    fn phase1_named_access_acl_survives_replacement_but_is_not_copied_into_backups() {
        let root = tempdir().unwrap();
        let path = root.path().join("shared-output");
        write_new(&path, b"original").unwrap();
        // Synthetic ACL: owner rw, named current test uid r, owning group none,
        // mask r, other none. Use a mapped uid for user-namespace compatibility.
        // No account lookup or real payroll data is used.
        let mut bytes = 2u32.to_le_bytes().to_vec();
        for (tag, permission, id) in [
            (1u16, 6u16, u32::MAX),
            (2, 4, unsafe { libc::geteuid() }),
            (4, 0, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            bytes.extend(tag.to_le_bytes());
            bytes.extend(permission.to_le_bytes());
            bytes.extend(id.to_le_bytes());
        }
        let file = open_read(&path).unwrap();
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    file.as_raw_fd(),
                    c"system.posix_acl_access".as_ptr(),
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    0,
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
        let before = linux_acl::access_acl(&file).unwrap();
        write_atomic(&path, b"replacement").unwrap();
        assert_eq!(
            before,
            linux_acl::access_acl(&open_read(&path).unwrap()).unwrap()
        );
        let backup = root.path().join("private-copy");
        copy_new(&path, &backup).unwrap();
        assert!(linux_acl::access_acl(&open_read(&backup).unwrap())
            .unwrap()
            .is_none());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(assess(root.path(), true)
            .iter()
            .any(|s| s.contains("named POSIX")));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_acl_tests {
    use super::*;
    #[test]
    fn phase1_macos_assessment_detects_comma_separated_inherited_allow() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir().unwrap();
        let shared = root.path().join("synthetic-shared");
        ensure_output_dir(&shared).unwrap();
        assert!(std::process::Command::new("/bin/chmod")
            .args(["+a", "everyone allow read,file_inherit,directory_inherit"])
            .arg(&shared)
            .status()
            .unwrap()
            .success());
        let inherited = shared.join("synthetic-inherited");
        fs::write(&inherited, b"synthetic inherited bytes").unwrap();
        fs::set_permissions(&inherited, fs::Permissions::from_mode(0o600)).unwrap();
        // The OS supplied inherited ACL is authoritative. A 0600 mode alone
        // must not cause assessment to overlook its comma-suffixed allow tag.
        let messages = assess(&inherited, true);
        assert!(
            messages.iter().any(|m| m.contains("extended ACL allow")),
            "Known gap: inherited allow ACL must be reported independently of Unix modes"
        );
        assert_eq!(fs::read(inherited).unwrap(), b"synthetic inherited bytes");
    }

    #[test]
    fn phase1_macos_private_creation_under_inheritable_acl() {
        let root = tempdir().unwrap();
        let shared = root.path().join("synthetic-shared");
        ensure_output_dir(&shared).unwrap();
        assert!(std::process::Command::new("/bin/chmod")
            .args(["+a", "everyone allow read,file_inherit,directory_inherit"])
            .arg(&shared)
            .status()
            .unwrap()
            .success());
        let private = shared.join("private");
        ensure_private_dir(&private).unwrap();
        let file = shared.join("private-file");
        write_new(&file, b"synthetic private bytes").unwrap();
        // Independently ask the OS for the ACL text; private objects have no
        // entries, so no parsing of allow or inheritance tokens is required.
        for path in [&private, &file] {
            let output = std::process::Command::new("/bin/ls")
                .arg("-lde")
                .arg(path)
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().lines().count(),
                1,
                "Creation must suppress inherited ACL entries"
            );
        }
    }
    #[test]
    fn phase1_macos_creation_excludes_inherited_acl_and_retains_existing_sharing() {
        let root = tempdir().unwrap();
        let shared = root.path().join("shared");
        ensure_output_dir(&shared).unwrap();
        let result = std::process::Command::new("/bin/chmod")
            .args(["+a","everyone allow read,readattr,readextattr,readsecurity,file_inherit,directory_inherit"])
            .arg(&shared).status().unwrap();
        assert!(result.success());
        assert!(macos_acl::has_allow_entries(&File::open(&shared).unwrap()).unwrap());
        let directory = shared.join("new-private");
        ensure_private_dir(&directory).unwrap();
        assert!(!macos_acl::has_allow_entries(&File::open(&directory).unwrap()).unwrap());
        let file = shared.join("new-private-file");
        write_new(&file, b"synthetic").unwrap();
        assert!(!macos_acl::has_allow_entries(&open_read(&file).unwrap()).unwrap());
        let existing = shared.join("existing-shared-file");
        fs::write(&existing, b"original synthetic").unwrap();
        assert!(macos_acl::has_allow_entries(&open_read(&existing).unwrap()).unwrap());
        write_atomic(&existing, b"replacement synthetic").unwrap();
        assert!(macos_acl::has_allow_entries(&open_read(&existing).unwrap()).unwrap());
        assert!(macos_acl::has_allow_entries(&File::open(&shared).unwrap()).unwrap());
    }
}

#[cfg(test)]
mod system_alias_tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn phase1_macos_verified_system_aliases_are_accepted() {
        // Inspect OS aliases only; do not create, modify or remove system paths.
        for alias in ["/var", "/tmp"] {
            let path = Path::new(alias);
            let metadata = fs::symlink_metadata(path).unwrap();
            assert!(metadata.file_type().is_symlink());
            assert!(verified_system_alias(path, &metadata).unwrap());
            check_path(path).unwrap();
        }
    }
    #[test]
    fn phase1_system_alias_requires_exact_absolute_or_relative_target() {
        for name in ["var", "tmp"] {
            let alias = PathBuf::from(format!("/{name}"));
            assert!(system_alias_target(
                &alias,
                Path::new(&format!("/private/{name}"))
            ));
            assert!(system_alias_target(
                &alias,
                Path::new(&format!("private/{name}"))
            ));
            assert!(!system_alias_target(
                &alias,
                Path::new(&format!("private/../private/{name}"))
            ));
            assert!(!system_alias_target(&alias, Path::new("/attacker")));
        }
        assert!(!system_alias_target(
            Path::new("/other"),
            Path::new("/private/other")
        ));
    }
}

#[cfg(test)]
mod publication_policy_tests {
    use super::*;
    #[test]
    fn phase1_private_publication_preserves_policy_and_refuses_overwrite() {
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        write_new(&source, b"synthetic original").unwrap();
        let mut pending = tempfile_in(root.path()).unwrap();
        pending.write_all(b"synthetic retained").unwrap();
        let before = pending.as_file().metadata().unwrap().permissions();
        let (file, kept) = keep_tempfile(pending).unwrap();
        assert_eq!(before, file.metadata().unwrap().permissions());
        assert_eq!(fs::read(&kept).unwrap(), b"synthetic retained");
        let mut pending = tempfile_in(root.path()).unwrap();
        pending.write_all(b"synthetic refused").unwrap();
        let error = publish_tempfile(pending, &source, false).unwrap_err();
        assert_eq!(fs::read(&source).unwrap(), b"synthetic original");
        assert_eq!(fs::read(error.file.path()).unwrap(), b"synthetic refused");
        let target = root.path().join("published");
        let mut pending = tempfile_in(root.path()).unwrap();
        pending.write_all(b"synthetic published").unwrap();
        let before = pending.as_file().metadata().unwrap().permissions();
        let published = publish_tempfile(pending, &target, false).unwrap();
        assert_eq!(before, published.metadata().unwrap().permissions());
        assert_eq!(fs::read(target).unwrap(), b"synthetic published");
    }
    #[test]
    fn phase1_operation_diagnostic_does_not_render_error_contents() {
        let error = operation_error(
            "storage.synthetic",
            io::Error::other("synthetic private detail"),
        );
        assert!(!error.to_string().contains("synthetic private detail"));
        assert!(error.to_string().contains("storage.synthetic"));
    }
}
