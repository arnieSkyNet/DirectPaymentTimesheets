//! Test-only owning prototype. No production module includes this file.
//! Verified Linux fixture identities are not a SQLite VFS confinement boundary.
use rusqlite::{ffi, Connection, Error, OpenFlags};
use std::fs::File;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

#[allow(dead_code)]
#[path = "../../src/private_storage_linux.rs"]
mod linux_cleanup;

struct NamespaceGuard {
    held: Vec<(PathBuf, File)>,
    private_directories: [PathBuf; 2],
}
impl NamespaceGuard {
    fn capture(root: &Path, stage: &Path) -> std::io::Result<Self> {
        if !root.is_absolute()
            || stage.parent() != Some(root)
            || stage
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(std::io::Error::other("synthetic namespace layout refused"));
        }
        let mut held = Vec::new();
        let mut prefix = PathBuf::new();
        for component in stage.components() {
            prefix.push(component);
            let metadata = std::fs::symlink_metadata(&prefix)?;
            let user = unsafe { libc::geteuid() };
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || (metadata.uid() != 0 && metadata.uid() != user)
                || (metadata.mode() & 0o022 != 0 && metadata.mode() & 0o1000 == 0)
                || ((prefix == root || prefix == stage)
                    && (metadata.uid() != user || metadata.mode() & 0o077 != 0))
            {
                return Err(std::io::Error::other(
                    "synthetic namespace admission refused",
                ));
            }
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&prefix)?;
            held.push((prefix.clone(), file));
        }
        let mut guard = Self {
            held,
            private_directories: [root.to_owned(), stage.to_owned()],
        };
        guard.verify()?;
        let path = stage.join("synthetic.sqlite");
        let database = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)?;
        let metadata = database.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(std::io::Error::other(
                "synthetic database admission refused",
            ));
        }
        guard.held.push((path, database));
        guard.verify()?;
        Ok(guard)
    }

    fn verify(&self) -> std::io::Result<()> {
        for (path, file) in &self.held {
            let held = file.metadata()?;
            let named = std::fs::symlink_metadata(path)?;
            let user = unsafe { libc::geteuid() };
            if named.file_type().is_symlink()
                || held.dev() != named.dev()
                || held.ino() != named.ino()
                || held.mode() != named.mode()
                || held.uid() != named.uid()
                || (held.uid() != 0 && held.uid() != user)
                || (held.is_dir() && held.mode() & 0o022 != 0 && held.mode() & 0o1000 == 0)
                || ((held.is_file() || self.private_directories.contains(path))
                    && (held.uid() != user || held.mode() & 0o077 != 0))
                || (held.is_file() && held.nlink() != 1)
            {
                return Err(std::io::Error::other(
                    "synthetic namespace identity changed",
                ));
            }
        }
        Ok(())
    }
}

pub(crate) struct Staging {
    pub(crate) path: PathBuf,
    cleanup: linux_cleanup::DirectoryCleanup,
    marker: PathBuf,
    pub(crate) require_closed_sidecars: Cell<bool>,
}
impl Staging {
    pub(crate) fn new(root: &std::path::Path) -> Rc<Self> {
        let temporary = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir_in(root)
            .expect("staging creation failed");
        let cleanup = linux_cleanup::DirectoryCleanup::capture(temporary.path())
            .expect("anchored cleanup acquisition failed");
        Rc::new(Self {
            path: temporary.keep(),
            cleanup,
            marker: root.join("staging-drop"),
            require_closed_sidecars: Cell::new(false),
        })
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        if self.require_closed_sidecars.get() {
            // Observe SQLite's final-close removal BEFORE anchored cleanup.
            assert!(!self.path.join("synthetic.sqlite-wal").exists());
            assert!(!self.path.join("synthetic.sqlite-shm").exists());
        }
        let classification = if self.cleanup.remove().is_ok() {
            "removed"
        } else {
            "refused"
        };
        assert!(
            std::fs::write(&self.marker, classification).is_ok(),
            "cleanup marker failed"
        );
    }
}

struct Sentinel {
    path: PathBuf,
    closed: Rc<Cell<bool>>,
    namespace: NamespaceGuard,
}

impl Drop for Sentinel {
    fn drop(&mut self) {
        let value = if self.closed.get() {
            "closed"
        } else {
            "premature"
        };
        assert!(
            std::fs::write(&self.path, value).is_ok(),
            "sentinel write failed"
        );
    }
}

pub(crate) struct ProofConnection {
    connection: Option<Connection>,
    closed: Rc<Cell<bool>>,
    _guard: Sentinel,
    staging: Rc<Staging>,
}

impl ProofConnection {
    pub(crate) fn open(root: &std::path::Path) -> Self {
        let staging = Staging::new(root);
        Self::open_staged(root, staging, false)
            .unwrap_or_else(|_| panic!("synthetic SQLite open failed"))
    }

    pub(crate) fn open_staged(
        root: &std::path::Path,
        staging: Rc<Staging>,
        invalid_flags: bool,
    ) -> Result<Self, Error> {
        let namespace = NamespaceGuard::capture(root, &staging.path)
            .map_err(|error| Error::ToSqlConversionFailure(Box::new(error)))?;
        let path = staging.path.join("synthetic.sqlite");
        // true here means no SQLite connection has been returned, not proof of
        // native failed-open cleanup. The invalid-flags fixture never calls VFS.
        let closed = Rc::new(Cell::new(true));
        let guard = Sentinel {
            path: root.join("guard-drop"),
            closed: closed.clone(),
            namespace,
        };
        let flags = if invalid_flags {
            OpenFlags::empty()
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW
        };
        let connection = Connection::open_with_flags(&path, flags)?;
        closed.set(false);
        Ok(Self {
            connection: Some(connection),
            closed,
            _guard: guard,
            staging,
        })
    }

    pub(crate) fn db(&self) -> &Connection {
        self.connection.as_ref().unwrap()
    }
    // No &mut Connection, into_inner, Clone or raw-Connection constructor.
    pub(crate) fn transaction(&mut self) -> rusqlite::Result<rusqlite::Transaction<'_>> {
        self.connection.as_mut().unwrap().transaction()
    }

    pub(crate) fn backup_into<'a, 'b>(
        &'a self,
        destination: &'b mut Self,
    ) -> rusqlite::Result<rusqlite::backup::Backup<'a, 'b>> {
        rusqlite::backup::Backup::new(self.db(), destination.connection.as_mut().unwrap())
    }

    pub(crate) fn staging(&self) -> &Rc<Staging> {
        &self.staging
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.get()
    }

    pub(crate) fn verify_namespace(&self) -> std::io::Result<()> {
        self._guard.namespace.verify()
    }

    pub(crate) fn guard_handles_live(&self) -> bool {
        self._guard
            .namespace
            .held
            .iter()
            .all(|(_, file)| file.metadata().is_ok())
    }

    pub(crate) fn registration(self) -> Result<Self, (Self, Error)> {
        // A real SQL failure after successful synthetic registration/open.
        let result = self
            .db()
            .busy_timeout(std::time::Duration::from_millis(50))
            .and_then(|_| self.db().execute_batch("INVALID SYNTHETIC REGISTRATION"));
        match result {
            Ok(()) => Ok(self),
            Err(error) => Err((self, error)),
        }
    }

    pub(crate) fn close(mut self) -> Result<(), (Self, Error)> {
        let connection = self.connection.take().expect("proof connection missing");
        match connection.close() {
            Ok(()) => {
                self.closed.set(true);
                Ok(())
            }
            Err((connection, error)) => {
                self.connection = Some(connection);
                Err((self, error))
            }
        }
    }

    // Deliberately use a real FFI statement, as in rusqlite 0.31.0 test_close_retry.
    // A normal borrowed Statement would prevent moving its Connection into close.
    pub(crate) fn outstanding_statement(&self) -> *mut ffi::sqlite3_stmt {
        let mut statement = std::ptr::null_mut();
        // SAFETY: same-thread owned, live connection; valid static NUL-terminated
        // SQL; writable out pointer. The statement is finalized before successful
        // close, or retained until the aborting child's process terminates.
        let code = unsafe {
            ffi::sqlite3_prepare_v2(
                self.connection.as_ref().unwrap().handle(),
                b"SELECT 1\0".as_ptr().cast(),
                -1,
                &mut statement,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(code, ffi::SQLITE_OK, "synthetic prepare failed");
        assert!(!statement.is_null());
        statement
    }
}

impl Drop for ProofConnection {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            match connection.close() {
                Ok(()) => self.closed.set(true),
                Err((connection, _)) => {
                    self.connection = Some(connection);
                    // No retry, guard drop, unwind or staging deletion on failure.
                    std::process::abort();
                }
            }
        }
    }
}
