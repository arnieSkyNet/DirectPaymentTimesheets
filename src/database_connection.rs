//! Owning connection API used by production repositories and backup operations.
//!
//! This retains existing storage guards, not a new VFS/namespace boundary.
//! Native failed-open cleanup remains inside pinned rusqlite. No fail-stop policy
//! is activated: an unsuccessful destructor close conservatively leaks the live
//! connection and its resources rather than release guards or clean up staging.
//! Uncertainty also latches restart-required admission/UI state. Emergency
//! termination remains inactive; catastrophic recovery uses known-good backups.

use std::{fmt, fs::File, mem::ManuallyDrop, path::Path, sync::Arc};

use rusqlite::{Connection, Error, OpenFlags, Transaction};

use crate::private_storage::PrivateTempDir;

struct Resources {
    // Declaration order releases guards before the last staging/cleanup owner.
    _guards: Vec<File>,
    _staging: Option<Arc<PrivateTempDir>>,
}

pub(crate) struct DatabaseConnection {
    connection: Option<Connection>,
    // Manual ownership also retains these if close itself panics. Do not replace
    // with ordinary fields without reviewing both explicit-close and Drop unwind.
    resources: ManuallyDrop<Resources>,
    close_confirmed: bool,
}

impl fmt::Debug for DatabaseConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseConnection { .. }")
    }
}

#[derive(Debug)]
pub(crate) enum DatabaseOpenError {
    Open(Error),
    // A post-open registration error returns the entire usable owner. Callers
    // must not map this into only the underlying Error and discard ownership.
    Registration {
        owner: DatabaseConnection,
        error: Error,
    },
}

impl fmt::Display for DatabaseOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(_) => formatter.write_str("Could not open guarded database"),
            Self::Registration { .. } => formatter.write_str("Database registration failed"),
        }
    }
}

impl std::error::Error for DatabaseOpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open(error) | Self::Registration { error, .. } => Some(error),
        }
    }
}

impl DatabaseOpenError {
    pub(crate) fn into_error(self) -> Error {
        match self {
            Self::Open(error) => error,
            Self::Registration { owner, error } => {
                // Compatibility callers receive the original registration error.
                // The complete owner performs checked destruction first.
                drop(owner);
                error
            }
        }
    }
}

// Shared access preserves existing borrowed query APIs. There is deliberately
// no DerefMut: mutable transactions remain borrowed through the owner.
impl std::ops::Deref for DatabaseConnection {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.connection()
    }
}

// Compatibility for pre-existing synthetic fixtures only. Production cannot
// construct an owner from an already-open, unguarded raw connection.
#[cfg(test)]
impl From<Connection> for DatabaseConnection {
    fn from(connection: Connection) -> Self {
        Self::from_parts(connection, Vec::new(), None)
    }
}

#[allow(dead_code)]
impl DatabaseConnection {
    pub(super) fn open_registered(
        path: &Path,
        staging: Option<Arc<PrivateTempDir>>,
        register: impl FnOnce(&Connection) -> rusqlite::Result<()>,
    ) -> Result<Self, DatabaseOpenError> {
        Self::ensure_available().map_err(DatabaseOpenError::Open)?;
        let guards = crate::private_storage::writable_database_guard(path).map_err(|error| {
            DatabaseOpenError::Open(Error::ToSqlConversionFailure(Box::new(error)))
        })?;
        let owner = Self::open_native(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            guards,
            staging,
        )
        .map_err(DatabaseOpenError::Open)?;
        // Establish ownership BEFORE fallible application registration.
        if let Err(error) = register(owner.connection()) {
            return Err(DatabaseOpenError::Registration { owner, error });
        }
        Ok(owner)
    }

    pub(crate) fn open_read_only(
        path: &Path,
        staging: Option<Arc<PrivateTempDir>>,
    ) -> Result<Self, DatabaseOpenError> {
        Self::ensure_available().map_err(DatabaseOpenError::Open)?;
        let guards = crate::private_storage::read_only_database_guard(path).map_err(|error| {
            DatabaseOpenError::Open(Error::ToSqlConversionFailure(Box::new(error)))
        })?;
        let owner = Self::open_native(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            guards,
            staging,
        )
        .map_err(DatabaseOpenError::Open)?;
        // Read-only opens deliberately perform no registration or migration.
        Ok(owner)
    }

    fn open_native(
        path: &Path,
        flags: OpenFlags,
        guards: Vec<File>,
        staging: Option<Arc<PrivateTempDir>>,
    ) -> rusqlite::Result<Self> {
        // Pinned rusqlite cannot return a partial native handle when opening
        // fails. Retain the bundle even on error/unwind instead of deleting
        // staging or releasing guards without confirmed native closure.
        let resources = ManuallyDrop::new(Resources {
            _guards: guards,
            _staging: staging,
        });
        let mut attempt = crate::database_failure::OwnershipAttempt::new(
            crate::database_failure::Failure::NativeOpenUnconfirmed,
        );
        let result = Connection::open_with_flags(path, flags);
        attempt.completed();
        let connection = result.map_err(|error| {
            crate::database_failure::record(
                crate::database_failure::Failure::NativeOpenUnconfirmed,
            );
            error
        })?;
        Ok(Self {
            connection: Some(connection),
            resources,
            close_confirmed: false,
        })
    }

    fn from_parts(
        connection: Connection,
        guards: Vec<File>,
        staging: Option<Arc<PrivateTempDir>>,
    ) -> Self {
        Self {
            connection: Some(connection),
            resources: ManuallyDrop::new(Resources {
                _guards: guards,
                _staging: staging,
            }),
            close_confirmed: false,
        }
    }

    pub(crate) fn connection(&self) -> &Connection {
        self.connection.as_ref().expect("owned connection missing")
    }

    fn ensure_available() -> rusqlite::Result<()> {
        match crate::database_failure::current() {
            None => Ok(()),
            Some(failure) => Err(Error::InvalidParameterName(failure.message().into())),
        }
    }

    pub(crate) fn transaction(&mut self) -> rusqlite::Result<Transaction<'_>> {
        self.connection
            .as_mut()
            .expect("owned connection missing")
            .transaction()
    }

    pub(crate) fn transaction_with_behavior(
        &mut self,
        behavior: rusqlite::TransactionBehavior,
    ) -> rusqlite::Result<Transaction<'_>> {
        self.connection
            .as_mut()
            .expect("owned connection missing")
            .transaction_with_behavior(behavior)
    }

    pub(crate) fn close(mut self) -> Result<(), (Self, Error)> {
        let mut attempt = crate::database_failure::OwnershipAttempt::new(
            crate::database_failure::Failure::CloseUnwound,
        );
        let result = self
            .connection
            .take()
            .expect("owned connection missing")
            .close();
        attempt.completed();
        match result {
            Ok(()) => {
                self.close_confirmed = true;
                Ok(())
            }
            Err((connection, error)) => {
                self.connection = Some(connection);
                Err((self, error))
            }
        }
    }

    pub(crate) fn backup_into<'a, 'b>(
        &'a self,
        destination: &'b mut Self,
    ) -> rusqlite::Result<rusqlite::backup::Backup<'a, 'b>> {
        rusqlite::backup::Backup::new(
            self.connection(),
            destination
                .connection
                .as_mut()
                .expect("owned connection missing"),
        )
    }

    /// Same 100-page steps and immediate BUSY/LOCKED refusal as the old pathname
    /// helper, without hidden connections, sleeping or automatic retry.
    pub(crate) fn copy_into(&self, destination: &mut Self) -> rusqlite::Result<()> {
        use rusqlite::backup::StepResult;
        let backup = self.backup_into(destination)?;
        loop {
            match backup.step(100)? {
                StepResult::Done => return Ok(()),
                StepResult::More => (),
                StepResult::Busy => {
                    return Err(Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                        None,
                    ))
                }
                StepResult::Locked => {
                    return Err(Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                        None,
                    ))
                }
                _ => return Err(Error::InvalidQuery),
            }
        }
    }

    /// Report failure while retaining the complete owner. Error conversion must
    /// not implicitly retry close or release a potentially live staging bundle.
    pub(crate) fn close_or_retain(self) -> rusqlite::Result<()> {
        self.close().map_err(|(owner, error)| {
            crate::database_failure::record(
                crate::database_failure::Failure::BoundaryCloseUnconfirmed,
            );
            std::mem::forget(owner);
            error
        })
    }
}

impl Drop for DatabaseConnection {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let mut attempt = crate::database_failure::OwnershipAttempt::new(
                crate::database_failure::Failure::CloseUnwound,
            );
            let result = connection.close();
            attempt.completed();
            match result {
                Ok(()) => self.close_confirmed = true,
                Err((connection, _)) => {
                    crate::database_failure::record(
                        crate::database_failure::Failure::DestructorCloseUnconfirmed,
                    );
                    // No termination, retry, unwind or staging cleanup. The latch
                    // stops new admissions/UI work while normal closure waits
                    // for existing workers. Emergency termination is inactive.
                    std::mem::forget(connection);
                }
            }
        }
        if self.close_confirmed {
            // SAFETY: only this destructor releases resources, once, after a
            // successful close. On a close panic/error they remain retained.
            unsafe { ManuallyDrop::drop(&mut self.resources) };
        }
    }
}

#[cfg(test)]
#[path = "database_connection_tests.rs"]
mod tests;
