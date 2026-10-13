//! Linux-only ownership proof, not a production connection or namespace guard.
//! Windows/macOS terminal semantics are unsupported and unverified.
#![cfg(target_os = "linux")]

use rusqlite::{ffi, Connection, Error, ErrorCode, OpenFlags};
use std::fs::File;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{cell::Cell, path::PathBuf, rc::Rc};

// Compile the existing helper into this integration-test crate only.
#[allow(dead_code)]
#[path = "../src/private_storage_linux.rs"]
mod linux_cleanup;

struct Staging {
    path: PathBuf,
    cleanup: linux_cleanup::DirectoryCleanup,
    marker: PathBuf,
    require_closed_sidecars: Cell<bool>,
}
impl Staging {
    fn new(root: &std::path::Path) -> Rc<Self> {
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
    _handles: Vec<File>,
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

struct ProofConnection {
    connection: Option<Connection>,
    closed: Rc<Cell<bool>>,
    _guard: Sentinel,
    staging: Rc<Staging>,
}

impl ProofConnection {
    fn open(root: &std::path::Path) -> Self {
        let staging = Staging::new(root);
        Self::open_staged(root, staging, false)
            .unwrap_or_else(|_| panic!("synthetic SQLite open failed"))
    }

    fn open_staged(
        root: &std::path::Path,
        staging: Rc<Staging>,
        invalid_flags: bool,
    ) -> Result<Self, Error> {
        // Real synthetic handles; NOT a Unix pathname confinement mechanism.
        let directory = |path: &std::path::Path| {
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)
                .expect("synthetic directory handle failed")
        };
        let path = staging.path.join("synthetic.sqlite");
        let database = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
            .expect("synthetic database handle failed");
        // Closed=true means no live SQLite yet. Guard precedes SQLite open.
        let closed = Rc::new(Cell::new(true));
        let guard = Sentinel {
            path: root.join("guard-drop"),
            closed: closed.clone(),
            _handles: vec![directory(root), directory(&staging.path), database],
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

    fn db(&self) -> &Connection {
        self.connection.as_ref().unwrap()
    }
    fn db_mut(&mut self) -> &mut Connection {
        self.connection.as_mut().unwrap()
    }

    fn registration(self) -> Result<Self, (Self, Error)> {
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

    fn close(mut self) -> Result<(), (Self, Error)> {
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
    fn outstanding_statement(&self) -> *mut ffi::sqlite3_stmt {
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

fn suppress_dumps() {
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: child-local Linux settings, valid rlimit pointers, no parent changes.
    unsafe {
        assert_eq!(
            libc::setrlimit(libc::RLIMIT_CORE, &limit),
            0,
            "core limit setup failed"
        );
        assert_eq!(
            libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0),
            0,
            "dump suppression failed"
        );
        assert_eq!(
            libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0),
            0,
            "dump suppression unverified"
        );
        let mut observed = std::mem::zeroed::<libc::rlimit>();
        assert_eq!(libc::getrlimit(libc::RLIMIT_CORE, &mut observed), 0);
        assert_eq!((observed.rlim_cur, observed.rlim_max), (0, 0));
    }
}

#[test]
fn child_entry() {
    let Some(mode) = std::env::var_os("DPT_CLOSE_PROOF_MODE") else {
        return;
    };
    suppress_dumps(); // No abort case is reachable before verification succeeds.
    std::panic::set_hook(Box::new(|_| {})); // Synthetic panic, no paths/backtrace.
    let root = PathBuf::from(std::env::var_os("DPT_CLOSE_PROOF_ROOT").expect("fixture missing"));
    assert!(std::fs::write(root.join("ready"), "dump-suppression-verified").is_ok());
    let mode = mode.to_str().expect("invalid proof mode");
    if mode == "open_failure" {
        let staging = Staging::new(&root);
        let path = staging.path.clone();
        let error = match ProofConnection::open_staged(&root, staging, true) {
            Err(error) => error,
            Ok(_) => panic!("invalid open flags accepted"),
        };
        // Pinned rusqlite rejects these flags before sqlite3_open_v2: no live
        // SQLite exists to close. This is NOT proof of arbitrary VFS failures.
        assert!(
            matches!(error, Error::SqliteFailure(code, _) if code.code == ErrorCode::ApiMisuse)
        );
        assert!(!path.exists());
        assert!(std::fs::write(root.join("returned"), "returned").is_ok());
        return;
    }
    let owner = ProofConnection::open(&root);
    assert!(!root.join("guard-drop").exists());
    match mode {
        "close" => assert!(owner.close().is_ok(), "ordinary close failed"),
        "retry" => {
            let statement = owner.outstanding_statement();
            let (owner, error) = match owner.close() {
                Err(pair) => pair,
                Ok(()) => panic!("outstanding statement did not prevent close"),
            };
            assert!(
                matches!(error, Error::SqliteFailure(code, _) if code.code == ErrorCode::DatabaseBusy)
            );
            assert!(!owner.closed.get());
            assert!(owner.staging.path.is_dir());
            assert!(!root.join("staging-drop").exists());
            assert!(!root.join("guard-drop").exists());
            // SAFETY: the failed close returned the live owner; statement is
            // still owned by that connection and is finalized exactly once.
            unsafe {
                assert_eq!(ffi::sqlite3_step(statement), ffi::SQLITE_ROW);
                assert_eq!(ffi::sqlite3_finalize(statement), ffi::SQLITE_OK);
            }
            assert!(owner.close().is_ok(), "retry close failed");
        }
        "registration" => {
            let (owner, _) = match owner.registration() {
                Err(pair) => pair,
                Ok(_) => panic!("registration unexpectedly succeeded"),
            };
            assert!(!root.join("guard-drop").exists());
            assert!(!root.join("staging-drop").exists());
            assert_eq!(
                owner
                    .db()
                    .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert!(owner.close().is_ok());
        }
        "rollback" | "rollback_unwind" => {
            let path = owner.staging.path.join("synthetic.sqlite-journal");
            owner
                .db()
                .execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE evidence(value INTEGER);")
                .unwrap();
            let mut owner = owner;
            if mode == "rollback" {
                let transaction = owner.db_mut().transaction().unwrap();
                transaction
                    .execute("INSERT INTO evidence VALUES(1)", [])
                    .unwrap();
                assert!(path.is_file());
                transaction.rollback().unwrap();
            } else {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let transaction = owner.db_mut().transaction().unwrap();
                    transaction
                        .execute("INSERT INTO evidence VALUES(1)", [])
                        .unwrap();
                    assert!(path.is_file());
                    panic!("synthetic transaction unwind");
                }));
                assert!(result.is_err());
            }
            assert!(!path.exists());
            assert_eq!(
                owner
                    .db()
                    .query_row("SELECT count(*) FROM evidence", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                0
            );
            assert!(!root.join("staging-drop").exists());
            assert!(owner.close().is_ok());
        }
        "wal" => {
            owner.staging.require_closed_sidecars.set(true);
            owner.db().execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE evidence(value INTEGER); INSERT INTO evidence VALUES(1);").unwrap();
            let stage = owner.staging.path.clone();
            let second = ProofConnection::open_staged(&root, owner.staging.clone(), false)
                .unwrap_or_else(|_| panic!("second open failed"));
            assert_eq!(
                second
                    .db()
                    .query_row("SELECT count(*) FROM evidence", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert!(stage.join("synthetic.sqlite-wal").is_file());
            assert!(stage.join("synthetic.sqlite-shm").is_file());
            let checkpoint: (i64, i64, i64) = owner
                .db()
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .unwrap();
            assert_eq!(checkpoint.0, 0);
            assert!(owner.close().is_ok());
            assert!(stage.is_dir());
            assert!(!root.join("staging-drop").exists());
            assert!(second
                ._guard
                ._handles
                .iter()
                .all(|file| file.metadata().is_ok()));
            assert_eq!(
                second
                    .db()
                    .query_row("SELECT count(*) FROM evidence", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert!(second.close().is_ok());
            assert!(!stage.exists());
        }
        "backup" => {
            owner
                .db()
                .execute_batch(
                    "CREATE TABLE evidence(value INTEGER); INSERT INTO evidence VALUES(1);",
                )
                .unwrap();
            let destination_root = tempfile::tempdir_in(&root).unwrap();
            let mut destination = ProofConnection::open(destination_root.path());
            {
                let backup =
                    rusqlite::backup::Backup::new(owner.db(), destination.db_mut()).unwrap();
                assert_eq!(backup.step(-1).unwrap(), rusqlite::backup::StepResult::Done);
                assert!(!root.join("guard-drop").exists());
                assert!(!destination_root.path().join("guard-drop").exists());
            }
            assert_eq!(
                destination
                    .db()
                    .query_row("SELECT count(*) FROM evidence", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert!(destination.close().is_ok());
            assert!(owner.close().is_ok());
        }
        "substitution" => {
            let original = owner.staging.path.clone();
            let retained = root.join("original-staging");
            std::fs::rename(&original, &retained).unwrap();
            std::fs::create_dir(&original).unwrap();
            std::fs::write(original.join("replacement"), "keep").unwrap();
            assert!(owner.close().is_ok());
            assert!(retained.join("synthetic.sqlite").is_file());
            assert_eq!(
                std::fs::read(original.join("replacement")).unwrap(),
                b"keep"
            );
            assert_eq!(
                std::fs::read(root.join("staging-drop")).unwrap(),
                b"refused"
            );
        }
        "drop" => drop(owner),
        "panic" => {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _owner = owner;
                panic!("synthetic unwind");
            }));
            assert!(outcome.is_err());
        }
        "abort" => {
            let _statement = owner.outstanding_statement();
            let owner = confirm_busy(owner, &root);
            drop(owner);
        }
        "panic_abort" => {
            let _statement = owner.outstanding_statement();
            let owner = confirm_busy(owner, &root);
            let _owner = owner;
            panic!("synthetic unwind with busy SQLite");
        }
        "watchdog" => loop {
            // SAFETY: synthetic child only; SIGALRM independently bounds this wait.
            unsafe { libc::pause() };
        },
        _ => panic!("unknown proof mode"),
    }
    assert!(std::fs::write(root.join("returned"), "returned").is_ok());
}

fn confirm_busy(owner: ProofConnection, root: &std::path::Path) -> ProofConnection {
    let (owner, error) = match owner.close() {
        Err(pair) => pair,
        Ok(()) => panic!("terminal fixture did not reach busy close"),
    };
    assert!(matches!(error, Error::SqliteFailure(code, _) if code.code == ErrorCode::DatabaseBusy));
    assert!(!owner.closed.get());
    assert!(!root.join("guard-drop").exists());
    assert!(std::fs::write(root.join("busy-confirmed"), "busy").is_ok());
    owner
}

fn parent_dump_state() -> (libc::rlim_t, libc::rlim_t, libc::c_int) {
    unsafe {
        let mut limit = std::mem::zeroed::<libc::rlimit>();
        assert_eq!(libc::getrlimit(libc::RLIMIT_CORE, &mut limit), 0);
        (
            limit.rlim_cur,
            limit.rlim_max,
            libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0),
        )
    }
}

fn run_child(mode: &str, abort: bool) {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    let original = parent_dump_state();
    let root = tempfile::tempdir().expect("synthetic fixture creation failed");
    let executable = std::env::current_exe().expect("test executable unavailable");
    let mut command = std::process::Command::new(executable);
    command
        .args(["--exact", "child_entry", "--test-threads=1"])
        .env_clear()
        .env("DPT_CLOSE_PROOF_MODE", mode)
        .env("DPT_CLOSE_PROOF_ROOT", root.path())
        .current_dir(root.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let parent = unsafe { libc::getpid() };
    // SAFETY: only async-signal-safe system calls in the post-fork child. If the
    // parent dies, this child has no descendants and cannot remain running.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
            }
            // Exec preserves the alarm and resets caught signal dispositions.
            // Default SIGALRM terminates without a core dump; child has no forks.
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = libc::SIG_DFL;
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(libc::SIGALRM, &action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut signals = std::mem::zeroed();
            libc::sigemptyset(&mut signals);
            libc::sigaddset(&mut signals, libc::SIGALRM);
            if libc::sigprocmask(libc::SIG_UNBLOCK, &signals, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            libc::alarm(5);
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .unwrap_or_else(|_| panic!("proof spawn failed"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(_) => {
                terminate_and_reap(&mut child);
                panic!("proof wait failed");
            }
        }
        if std::time::Instant::now() >= deadline {
            terminate_and_reap(&mut child);
            panic!("proof child exceeded ten-second budget");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(
        parent_dump_state(),
        original,
        "parent dump settings changed"
    );
    assert!(
        root.path().join("ready").exists(),
        "child dump suppression not verified"
    );
    assert!(!status.core_dumped(), "unexpected child core dump");
    if mode == "watchdog" {
        assert_eq!(status.signal(), Some(libc::SIGALRM));
        assert!(!root.path().join("guard-drop").exists());
        assert!(!root.path().join("returned").exists());
        assert!(!root.path().join("staging-drop").exists());
    } else if abort {
        assert!(
            root.path().join("busy-confirmed").exists(),
            "real busy close unconfirmed"
        );
        assert!(!root.path().join("staging-drop").exists());
        assert_eq!(
            status.signal(),
            Some(libc::SIGABRT),
            "expected non-unwinding abort"
        );
        assert!(
            !root.path().join("guard-drop").exists(),
            "abort dropped namespace guard"
        );
        assert!(
            !root.path().join("returned").exists(),
            "abort unexpectedly returned"
        );
    } else {
        assert!(status.success(), "proof child failed");
        assert_eq!(
            std::fs::read_to_string(root.path().join("guard-drop"))
                .expect("guard sentinel missing"),
            "closed"
        );
        assert!(root.path().join("returned").exists());
        assert_eq!(
            std::fs::read(root.path().join("staging-drop")).unwrap(),
            if mode == "substitution" {
                b"refused".as_slice()
            } else {
                b"removed".as_slice()
            }
        );
    }
}

fn terminate_and_reap(child: &mut std::process::Child) {
    assert!(child.kill().is_ok(), "proof termination unconfirmed");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(_) => panic!("proof reap failed"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "proof cleanup unconfirmed"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn child_watchdog_terminates_and_is_reaped() {
    run_child("watchdog", false);
}

#[test]
fn ordinary_close() {
    run_child("close", false);
}
#[test]
fn busy_close_returns_owner_for_retry() {
    run_child("retry", false);
}
#[test]
fn successful_destructor_close() {
    run_child("drop", false);
}
#[test]
fn normal_panic_unwinds() {
    run_child("panic", false);
}
#[test]
fn busy_destructor_aborts_before_guard_drop() {
    run_child("abort", true);
}
#[test]
fn busy_destructor_aborts_during_unwind() {
    run_child("panic_abort", true);
}

#[test]
fn real_rollback_journal() {
    run_child("rollback", false);
}
#[test]
fn rollback_during_unwind() {
    run_child("rollback_unwind", false);
}
#[test]
fn wal_multiple_connections_checkpoint_and_final_close() {
    run_child("wal", false);
}
#[test]
fn guarded_backup_connections() {
    run_child("backup", false);
}
#[test]
fn registration_failure_retains_owner() {
    run_child("registration", false);
}
#[test]
fn open_failure_releases_unopened_resources() {
    run_child("open_failure", false);
}
#[test]
fn substituted_namespace_cleanup_is_refused() {
    run_child("substitution", false);
}
