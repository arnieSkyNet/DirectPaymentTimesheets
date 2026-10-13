//! Linux-only ownership proof, not a production connection or namespace guard.
//! Windows/macOS terminal semantics are unsupported and unverified.
#![cfg(target_os = "linux")]

use rusqlite::{ffi, Connection, Error, ErrorCode};
use std::{cell::Cell, path::PathBuf, rc::Rc};

struct Sentinel {
    path: PathBuf,
    closed: Rc<Cell<bool>>,
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
}

impl ProofConnection {
    fn open(root: &std::path::Path) -> Self {
        let closed = Rc::new(Cell::new(false));
        let guard = Sentinel {
            path: root.join("guard-drop"),
            closed: closed.clone(),
        };
        let connection = Connection::open(root.join("synthetic.sqlite"))
            .unwrap_or_else(|_| panic!("synthetic SQLite open failed"));
        Self {
            connection: Some(connection),
            closed,
            _guard: guard,
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
    let owner = ProofConnection::open(&root);
    assert!(!root.join("guard-drop").exists());
    match mode.to_str().expect("invalid proof mode") {
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
            assert!(!root.join("guard-drop").exists());
            // SAFETY: the failed close returned the live owner; statement is
            // still owned by that connection and is finalized exactly once.
            unsafe {
                assert_eq!(ffi::sqlite3_step(statement), ffi::SQLITE_ROW);
                assert_eq!(ffi::sqlite3_finalize(statement), ffi::SQLITE_OK);
            }
            assert!(owner.close().is_ok(), "retry close failed");
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
    } else if abort {
        assert!(
            root.path().join("busy-confirmed").exists(),
            "real busy close unconfirmed"
        );
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
