//! Linux-only ownership proof, not a production connection or namespace guard.
//! Windows/macOS terminal semantics are unsupported and unverified.
#![cfg(target_os = "linux")]

use rusqlite::{ffi, Connection, Error, ErrorCode, OpenFlags};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[path = "support/sqlite_owner_prototype.rs"]
mod prototype;
use prototype::{ProofConnection, Staging};

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
    if mode == "recover_pending" {
        let stage = retained_stage(&root);
        let database = stage.join("synthetic.sqlite");
        // Normal SQLite recovery, not the production recovery service. No CREATE.
        let recovered = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap_or_else(|_| panic!("synthetic recovery open failed"));
        recovered
            .busy_timeout(std::time::Duration::from_millis(50))
            .unwrap();
        let integrity: String = recovered
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap_or_else(|_| panic!("synthetic recovery integrity query failed"));
        assert!(integrity == "ok", "synthetic recovery integrity refused");
        let rows: Vec<i64> = recovered
            .prepare("SELECT value FROM evidence ORDER BY value")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            rows == [1],
            "committed baseline or rollback recovery mismatch"
        );
        assert!(recovered.close().is_ok(), "recovered close failed");
        assert!(
            !stage.join("synthetic.sqlite-journal").exists(),
            "recovery journal retained unexpectedly"
        );
        assert!(std::fs::write(root.join("recovery-confirmed"), "recovered").is_ok());
        return;
    }
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
    assert!(owner.verify_namespace().is_ok());
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
            assert!(!owner.is_closed());
            assert!(owner.staging().path.is_dir());
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
        "repeated_retry" | "registration_retry" | "shared_retry" => {
            let retained_stage = std::rc::Rc::downgrade(&owner.staging());
            let path = owner.staging().path.clone();
            let second = if mode == "shared_retry" {
                Some(ProofConnection::open_staged(&root, owner.staging().clone(), false).unwrap())
            } else {
                None
            };
            let statement = owner.outstanding_statement();
            let owner = if mode == "registration_retry" {
                owner
                    .registration()
                    .err()
                    .expect("registration failure missing")
                    .0
            } else {
                owner
            };
            let (owner, first_error) = owner.close().err().expect("busy close missing");
            let (owner, second_error) = owner.close().err().expect("second busy close missing");
            // Finalize before assertions so unexpected assertion unwind cannot
            // accidentally enter the historical terminal destructor fixture.
            unsafe {
                assert_eq!(ffi::sqlite3_finalize(statement), ffi::SQLITE_OK);
            }
            for error in [first_error, second_error] {
                assert!(
                    matches!(error, Error::SqliteFailure(code, _) if code.code == ErrorCode::DatabaseBusy)
                );
            }
            assert!(owner.verify_namespace().is_ok());
            assert!(owner.guard_handles_live());
            assert!(retained_stage.upgrade().is_some());
            assert!(!root.join("guard-drop").exists());
            assert!(!root.join("staging-drop").exists());
            if let Some(second) = second {
                assert!(second.close().is_ok());
                assert!(path.exists());
                assert!(owner.verify_namespace().is_ok());
                assert!(!root.join("staging-drop").exists());
            }
            assert!(owner.close().is_ok());
            assert!(retained_stage.upgrade().is_none());
            assert!(!path.exists());
        }
        "rollback" | "rollback_unwind" => {
            let path = owner.staging().path.join("synthetic.sqlite-journal");
            owner
                .db()
                .execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE evidence(value INTEGER);")
                .unwrap();
            let mut owner = owner;
            if mode == "rollback" {
                let transaction = owner.transaction().unwrap();
                transaction
                    .execute("INSERT INTO evidence VALUES(1)", [])
                    .unwrap();
                assert!(path.is_file());
                transaction.rollback().unwrap();
            } else {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let transaction = owner.transaction().unwrap();
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
            owner.staging().require_closed_sidecars.set(true);
            owner.db().execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE evidence(value INTEGER); INSERT INTO evidence VALUES(1);").unwrap();
            let stage = owner.staging().path.clone();
            let second = ProofConnection::open_staged(&root, owner.staging().clone(), false)
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
            assert!(second.guard_handles_live());
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
        "backup" | "backup_retry" => {
            owner
                .db()
                .execute_batch(
                    "CREATE TABLE evidence(value INTEGER); INSERT INTO evidence VALUES(1);",
                )
                .unwrap();
            let destination_root = tempfile::Builder::new()
                .permissions(std::fs::Permissions::from_mode(0o700))
                .tempdir_in(&root)
                .unwrap();
            let mut destination = ProofConnection::open(destination_root.path());
            {
                let backup = owner.backup_into(&mut destination).unwrap();
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
            let (owner, destination) = if mode == "backup_retry" {
                let source_statement = owner.outstanding_statement();
                let destination_statement = destination.outstanding_statement();
                let (owner, source_error) = owner.close().err().expect("source busy close missing");
                let (destination, destination_error) = destination
                    .close()
                    .err()
                    .expect("destination busy close missing");
                unsafe {
                    assert_eq!(ffi::sqlite3_finalize(source_statement), ffi::SQLITE_OK);
                    assert_eq!(ffi::sqlite3_finalize(destination_statement), ffi::SQLITE_OK);
                }
                for error in [source_error, destination_error] {
                    assert!(
                        matches!(error, Error::SqliteFailure(code, _) if code.code == ErrorCode::DatabaseBusy)
                    );
                }
                assert!(owner.verify_namespace().is_ok());
                assert!(destination.verify_namespace().is_ok());
                assert!(!root.join("staging-drop").exists());
                assert!(!destination_root.path().join("staging-drop").exists());
                (owner, destination)
            } else {
                (owner, destination)
            };
            assert!(destination.close().is_ok());
            assert!(owner.verify_namespace().is_ok());
            assert!(owner.staging().path.exists());
            assert!(owner.close().is_ok());
        }
        "database_substitution" => {
            let path = owner.staging().path.join("synthetic.sqlite");
            std::fs::rename(&path, owner.staging().path.join("retained.sqlite")).unwrap();
            std::fs::write(&path, b"synthetic replacement").unwrap();
            assert!(owner.verify_namespace().is_err());
            assert!(owner.guard_handles_live());
            // Observe identity only; never issue SQL against a substituted name.
            assert!(owner.close().is_ok());
        }
        "substitution" => {
            let original = owner.staging().path.clone();
            let retained = root.join("original-staging");
            std::fs::rename(&original, &retained).unwrap();
            std::fs::create_dir(&original).unwrap();
            std::fs::write(original.join("replacement"), "keep").unwrap();
            assert!(owner.verify_namespace().is_err());
            assert!(owner.guard_handles_live());
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
        "pending_abort" => {
            owner.db().execute_batch("PRAGMA page_size=1024; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA cache_size=1; CREATE TABLE evidence(value INTEGER, payload BLOB); INSERT INTO evidence VALUES(1, NULL);")
                .unwrap_or_else(|_| panic!("rollback baseline setup failed"));
            let journal_mode: String = owner
                .db()
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert!(journal_mode == "delete", "rollback mode unconfirmed");
            let database = owner.staging().path.join("synthetic.sqlite");
            assert!(std::fs::write(
                root.join("baseline-image"),
                std::fs::read(&database).unwrap()
            )
            .is_ok());
            owner
                .db()
                .execute_batch("BEGIN IMMEDIATE; INSERT INTO evidence VALUES(2, zeroblob(8192));")
                .unwrap_or_else(|_| panic!("pending write failed"));
            // SAFETY: live, same-thread connection. Force dirty pages to disk so
            // reopen must recover a hot journal, rather than discard cache-only work.
            let flushed = unsafe { ffi::sqlite3_db_cacheflush(owner.db().handle()) };
            assert!(flushed == ffi::SQLITE_OK, "pending page flush failed");
            let _statement = owner.outstanding_statement();
            let owner = confirm_busy(owner, &root);
            assert!(
                !owner.db().is_autocommit(),
                "busy close lost pending transaction"
            );
            assert!(owner.staging().path.is_dir(), "busy close lost staging");
            assert!(owner.guard_handles_live(), "busy close lost handles");
            assert!(
                !root.join("staging-drop").exists(),
                "busy close cleaned staging"
            );
            assert!(std::fs::write(root.join("pending-abort-ready"), "pending").is_ok());
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
    assert!(!owner.is_closed());
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
    let root = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .expect("synthetic fixture creation failed");
    run_child_at(mode, abort, root.path());
}

fn retained_stage(root: &std::path::Path) -> PathBuf {
    let stages: Vec<_> = std::fs::read_dir(root)
        .expect("retained fixture inspection failed")
        .map(|entry| entry.expect("retained fixture entry failed"))
        .filter(|entry| {
            entry
                .file_type()
                .expect("retained fixture metadata failed")
                .is_dir()
        })
        .map(|entry| entry.path())
        .collect();
    assert!(stages.len() == 1, "retained staging inventory mismatch");
    stages.into_iter().next().unwrap()
}

fn run_child_at(mode: &str, abort: bool, fixture_root: &std::path::Path) {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    let original = parent_dump_state();
    // Borrowing the root lets a second bounded child recover the SAME fixture.
    let root = fixture_root;
    let executable = std::env::current_exe().expect("test executable unavailable");
    let mut command = std::process::Command::new(executable);
    command
        .args(["--exact", "child_entry", "--test-threads=1"])
        .env_clear()
        .env("DPT_CLOSE_PROOF_MODE", mode)
        .env("DPT_CLOSE_PROOF_ROOT", root)
        .current_dir(root)
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
        root.join("ready").exists(),
        "child dump suppression not verified"
    );
    assert!(!status.core_dumped(), "unexpected child core dump");
    if mode == "recover_pending" {
        assert!(status.success(), "bounded recovery child failed");
        assert!(
            root.join("recovery-confirmed").exists(),
            "recovery evidence missing"
        );
        assert!(!root.join("guard-drop").exists());
        assert!(!root.join("staging-drop").exists());
    } else if mode == "watchdog" {
        assert_eq!(status.signal(), Some(libc::SIGALRM));
        assert!(!root.join("guard-drop").exists());
        assert!(!root.join("returned").exists());
        assert!(!root.join("staging-drop").exists());
    } else if abort {
        assert!(
            root.join("busy-confirmed").exists(),
            "real busy close unconfirmed"
        );
        assert!(!root.join("staging-drop").exists());
        assert_eq!(
            status.signal(),
            Some(libc::SIGABRT),
            "expected non-unwinding abort"
        );
        assert!(
            !root.join("guard-drop").exists(),
            "abort dropped namespace guard"
        );
        assert!(
            !root.join("returned").exists(),
            "abort unexpectedly returned"
        );
    } else {
        assert!(status.success(), "proof child failed");
        assert_eq!(
            std::fs::read_to_string(root.join("guard-drop")).expect("guard sentinel missing"),
            "closed"
        );
        assert!(root.join("returned").exists());
        assert_eq!(
            std::fs::read(root.join("staging-drop")).unwrap(),
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
fn repeated_busy_close_retains_verified_guards_and_last_staging_owner() {
    run_child("repeated_retry", false);
}
#[test]
fn registration_failure_then_busy_close_returns_complete_owner() {
    run_child("registration_retry", false);
}
#[test]
fn another_connection_closing_cannot_clean_a_failed_close_owner() {
    run_child("shared_retry", false);
}
#[test]
fn backup_source_and_destination_retain_owners_on_busy_close() {
    run_child("backup_retry", false);
}
#[test]
fn prototype_refuses_broad_private_staging_before_sqlite_open() {
    let root = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let staging = Staging::new(root.path());
    let path = staging.path.clone();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(ProofConnection::open_staged(root.path(), staging, false).is_err());
    assert!(!path.join("synthetic.sqlite").exists());
    assert_eq!(
        std::fs::read(root.path().join("staging-drop")).unwrap(),
        b"refused"
    );
}
#[test]
fn prototype_refuses_symlink_database_without_touching_target() {
    let root = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let staging = Staging::new(root.path());
    let target = root.path().join("target");
    std::fs::write(&target, b"synthetic retained target").unwrap();
    std::os::unix::fs::symlink(&target, staging.path.join("synthetic.sqlite")).unwrap();
    assert!(ProofConnection::open_staged(root.path(), staging, false).is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"synthetic retained target");
}
#[test]
fn database_identity_substitution_is_detected_while_original_handle_survives() {
    run_child("database_substitution", false);
}
#[test]
fn open_failure_releases_unopened_resources() {
    run_child("open_failure", false);
}
#[test]
fn substituted_namespace_cleanup_is_refused() {
    run_child("substitution", false);
}

#[test]
fn pending_write_abort_preserves_journal_and_recovers() {
    let root = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .expect("synthetic recovery fixture failed");
    run_child_at("pending_abort", true, root.path());
    assert!(
        root.path().join("pending-abort-ready").exists(),
        "pending abort phase unconfirmed"
    );
    let stage = retained_stage(root.path());
    let database = stage.join("synthetic.sqlite");
    let journal = stage.join("synthetic.sqlite-journal");
    assert!(
        database.is_file(),
        "original database missing before recovery"
    );
    let journal_bytes = std::fs::read(journal).expect("rollback journal missing before recovery");
    assert!(journal_bytes.len() > 512, "rollback journal incomplete");
    assert!(
        journal_bytes[8..12] != [0, 0, 0, 0],
        "rollback journal has no recorded pages"
    );
    assert!(
        journal_bytes.starts_with(&[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]),
        "hot rollback journal header unconfirmed"
    );
    assert!(
        std::fs::read(database).unwrap()
            != std::fs::read(root.path().join("baseline-image")).unwrap(),
        "pending write did not reach database file"
    );
    assert!(!root.path().join("guard-drop").exists());
    assert!(!root.path().join("staging-drop").exists());
    run_child_at("recover_pending", false, root.path());
    assert!(stage.is_dir(), "recovery unexpectedly removed staging");
}
