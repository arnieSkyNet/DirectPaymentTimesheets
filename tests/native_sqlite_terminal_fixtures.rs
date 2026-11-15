//! Future, explicitly authorized native children only. Never a production API.
#![cfg(any(windows, target_os = "macos"))]

use rusqlite::{ffi, Connection, Error, ErrorCode, OpenFlags};
use std::{fs, fs::File, path::Path, path::PathBuf, time::Duration};

const STATUS: i32 = 86;

fn mark(root: &Path, name: &str) {
    assert!(
        fs::write(root.join(name), b"verified").is_ok(),
        "fixture marker failed"
    );
}

fn directory(path: &Path) -> File {
    let mut options = fs::OpenOptions::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .access_mode(0x20080)
            .share_mode(3)
            .custom_flags(0x02200000);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options
        .open(path)
        .unwrap_or_else(|_| panic!("fixture handle unavailable"));
    assert!(
        file.metadata().unwrap().is_dir(),
        "fixture directory refused"
    );
    file
}

struct Owner {
    db: Option<Connection>,
    root: PathBuf,
    handles: Vec<File>,
    terminal: bool,
}

impl Owner {
    fn open(root: &Path, terminal: bool) -> Self {
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            fs::set_permissions(&stage, fs::Permissions::from_mode(0o700)).unwrap();
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x00200000);
        }
        let path = stage.join("synthetic.sqlite");
        let file = options.open(&path).unwrap();
        #[allow(unused_mut)]
        let mut handles = vec![directory(root), directory(&stage), file];
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            handles.push(
                fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .share_mode(0)
                    .open(root.join("exclusive"))
                    .unwrap(),
            );
        }
        mark(root, "handles_ready");
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap_or_else(|_| panic!("fixture SQLite open failed"));
        let owner = Self {
            db: Some(db),
            root: root.to_owned(),
            handles,
            terminal,
        };
        owner.db().busy_timeout(Duration::from_millis(50)).unwrap();
        owner
    }

    fn db(&self) -> &Connection {
        self.db.as_ref().unwrap()
    }

    fn statement(&self) -> *mut ffi::sqlite3_stmt {
        let mut statement = std::ptr::null_mut();
        // SAFETY: live same-thread connection and static NUL-terminated SQL.
        let result = unsafe {
            ffi::sqlite3_prepare_v2(
                self.db().handle(),
                b"SELECT 1\0".as_ptr().cast(),
                -1,
                &mut statement,
                std::ptr::null_mut(),
            )
        };
        assert!(
            result == ffi::SQLITE_OK && !statement.is_null(),
            "native prepare refused"
        );
        statement
    }

    fn close(mut self) -> Result<(), (Self, Error)> {
        match self.db.take().unwrap().close() {
            Ok(()) => Ok(()),
            Err((db, error)) => {
                self.db = Some(db);
                Err((self, error))
            }
        }
    }

    fn busy(self) -> Self {
        let (owner, error) = match self.close() {
            Err(pair) => pair,
            Ok(()) => panic!("busy close missing"),
        };
        assert!(
            matches!(error, Error::SqliteFailure(code, _) if code.code == ErrorCode::DatabaseBusy),
            "unexpected close error"
        );
        assert!(
            owner.handles.iter().all(|file| file.metadata().is_ok()),
            "handles lost"
        );
        assert!(owner.root.join("stage").is_dir(), "staging lost");
        assert!(
            !owner.root.join("guards_dropped").exists(),
            "premature guard release"
        );
        mark(&owner.root, "busy_confirmed");
        owner
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(db) = self.db.take() {
            if let Err((db, error)) = db.close() {
                self.db = Some(db);
                if self.terminal
                    && self.root.join("busy_confirmed").is_file()
                    && matches!(error, Error::SqliteFailure(code, _) if code.code == ErrorCode::DatabaseBusy)
                {
                    mark(&self.root, "terminal_ready");
                    terminal();
                }
                // Never unwind/release guards on an unexpected close failure.
                // The bounded external supervisor must classify and stop this child.
                mark(&self.root, "unexpected_close_failure");
                loop {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        mark(&self.root, "guards_dropped");
        // Staging is deliberately retained even after success. Parent teardown
        // is permitted only after verified child termination; no pathname cleanup.
        mark(&self.root, "staging_released");
    }
}

fn terminal() -> ! {
    #[cfg(windows)]
    unsafe {
        // ABI: HANDLE pointer, UINT u32, BOOL i32, WINAPI calling convention.
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn TerminateProcess(process: *mut std::ffi::c_void, status: u32) -> i32;
        }
        let _ = TerminateProcess(GetCurrentProcess(), STATUS as u32);
        // API failure must not fall back to abort or allow destructor return.
        loop {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[cfg(target_os = "macos")]
    unsafe {
        libc::_exit(STATUS)
    }
}

fn privacy(root: &Path) {
    let require = |ok: bool| {
        if !ok {
            mark(root, "unsupported_privacy");
            panic!("unsupported_privacy");
        }
    };
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn WerSetFlags(flags: u32) -> i32;
            fn WerGetFlags(process: *mut std::ffi::c_void, flags: *mut u32) -> i32;
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
        }
        // NOHEAP reduces WER collection; it does NOT suppress LocalDumps.
        require(WerSetFlags(1) == 0);
        let mut flags = 0;
        require(WerGetFlags(GetCurrentProcess(), &mut flags) == 0 && flags & 1 != 0);
    }
    #[cfg(target_os = "macos")]
    unsafe {
        let limits = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        require(libc::setrlimit(libc::RLIMIT_CORE, &limits) == 0);
        let mut observed = limits;
        require(
            libc::getrlimit(libc::RLIMIT_CORE, &mut observed) == 0
                && observed.rlim_cur == 0
                && observed.rlim_max == 0,
        );
        // Independent child watchdog; no descendant spawning in this fixture.
        assert!(
            libc::signal(libc::SIGALRM, libc::SIG_DFL) != libc::SIG_ERR,
            "unsupported_cleanup"
        );
        let mut signals = std::mem::zeroed();
        libc::sigemptyset(&mut signals);
        libc::sigaddset(&mut signals, libc::SIGALRM);
        assert!(
            libc::sigprocmask(libc::SIG_UNBLOCK, &signals, std::ptr::null_mut()) == 0,
            "unsupported_cleanup"
        );
        libc::alarm(10);
    }
}

#[test]
#[ignore = "requires future explicit native harness authorization"]
fn native_child() {
    let profile = std::env::var("DPT_NATIVE_SQLITE_PROFILE").unwrap_or_else(|_| "github".into());
    assert!(
        profile == "github" || profile == "molly-local",
        "profile refused"
    );
    let local = profile == "molly-local";
    if local {
        assert!(
            cfg!(all(windows, target_arch = "x86_64")),
            "unsupported_platform"
        );
        assert!(
            std::env::vars_os().all(|(key, _)| !key.to_string_lossy().starts_with("GITHUB_")),
            "local GitHub environment refused"
        );
        assert!(
            std::env::var("DPT_NATIVE_SQLITE_HOST").as_deref() == Ok("IMOLLY"),
            "host refused"
        );
        #[cfg(windows)]
        unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetComputerNameW(buffer: *mut u16, size: *mut u32) -> i32;
                fn GetCurrentProcess() -> *mut std::ffi::c_void;
                fn IsWow64Process2(
                    process: *mut std::ffi::c_void,
                    process_machine: *mut u16,
                    native_machine: *mut u16,
                ) -> i32;
            }
            let (mut process_machine, mut native_machine) = (0u16, 0u16);
            assert!(
                IsWow64Process2(
                    GetCurrentProcess(),
                    &mut process_machine,
                    &mut native_machine
                ) != 0
                    && process_machine == 0
                    && native_machine == 0x8664,
                "native x64 refused"
            );
            let mut buffer = [0u16; 256];
            let mut length = buffer.len() as u32;
            assert!(
                GetComputerNameW(buffer.as_mut_ptr(), &mut length) != 0,
                "host identity unavailable"
            );
            assert!(
                String::from_utf16(&buffer[..length as usize])
                    .unwrap()
                    .eq_ignore_ascii_case("IMOLLY"),
                "host refused"
            );
        }
        assert!(
            std::env::var("DPT_NATIVE_SQLITE_LOCAL_RISK").as_deref()
                == Ok("Molly-disposable-synthetic-terminal-only"),
            "risk refused"
        );
    } else {
        assert!(
            std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
                && std::env::var("GITHUB_REF").as_deref() == Ok("refs/heads/main")
                && std::env::var("DPT_NATIVE_SQLITE_PREFLIGHT").as_deref() == Ok("verified"),
            "unsupported_cleanup"
        );
    }
    assert!(
        std::env::var("DPT_NATIVE_SQLITE_PREFLIGHT").as_deref() == Ok("verified"),
        "preflight refused"
    );
    let sha = std::env::var("EXPECTED_SHA").expect("approved source missing");
    assert!(
        sha.len() == 40
            && sha
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "approved source refused"
    );
    #[cfg(windows)]
    assert!(
        std::env::var("NATIVE_RUNNER").as_deref()
            == Ok(if local { "molly-local" } else { "windows-2022" }),
        "unsupported_platform"
    );
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert!(
        std::env::var("NATIVE_RUNNER").as_deref() == Ok("macos-15"),
        "unsupported_platform"
    );
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    assert!(
        std::env::var("NATIVE_RUNNER").as_deref() == Ok("macos-15-intel"),
        "unsupported_platform"
    );
    assert!(
        std::env::var("DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS").as_deref() == Ok("1"),
        "disabled"
    );
    assert!(
        std::env::var("DPT_SYNTHETIC_REPORT_RISK").as_deref() == Ok("disposable-synthetic-only"),
        "unsupported_privacy"
    );
    assert!(
        std::env::var_os("DPT_WINDOWS_ROOT_DIAGNOSTICS").is_none()
            && std::env::var_os("DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS").is_none(),
        "root diagnostics refused"
    );
    let mode = std::env::var("DPT_NATIVE_SQLITE_CASE").expect("case missing");
    assert!(
        [
            "normal",
            "retry",
            "unwind",
            "terminal",
            "panic_terminal",
            "rollback",
            "wal",
            "stall",
            "recover_rollback",
            "recover_wal"
        ]
        .contains(&mode.as_str()),
        "case refused"
    );
    let root = PathBuf::from(std::env::var_os("DPT_NATIVE_SQLITE_ROOT").expect("fixture missing"));
    std::panic::set_hook(Box::new(|_| {}));
    privacy(&root);
    mark(&root, "privacy_ready");
    // Supervisor verifies containment before releasing this start barrier.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !root.join("go").is_file() {
        assert!(std::time::Instant::now() < deadline, "unsupported_cleanup");
        std::thread::sleep(Duration::from_millis(10));
    }
    if mode.starts_with("recover_") {
        let db = Connection::open_with_flags(
            root.join("stage/synthetic.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        db.busy_timeout(Duration::from_millis(50)).unwrap();
        let integrity: String = db
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        let rows: Vec<i64> = db
            .prepare("SELECT value FROM evidence ORDER BY value")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        let expected = if mode == "recover_wal" {
            vec![1, 2]
        } else {
            vec![1]
        };
        assert!(integrity == "ok" && rows == expected, "recovery refused");
        assert!(db.close().is_ok(), "recovery close refused");
        mark(&root, "recovery_confirmed");
        return;
    }
    let terminal_case = ["terminal", "panic_terminal", "rollback", "wal"].contains(&mode.as_str());
    let owner = Owner::open(&root, terminal_case);
    match mode.as_str() {
        "normal" => assert!(owner.close().is_ok()),
        "unwind" => {
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _owner = owner;
                panic!("synthetic unwind");
            }))
            .is_err());
        }
        "retry" => {
            let statement = owner.statement();
            let owner = owner.busy();
            // SAFETY: failed close retained owner; finalize exactly once.
            unsafe {
                assert!(ffi::sqlite3_finalize(statement) == ffi::SQLITE_OK);
            }
            assert!(owner.close().is_ok());
        }
        "stall" => {
            mark(&root, "stall_ready");
            loop {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        _ => {
            if mode == "rollback" || mode == "wal" {
                let sql = if mode == "rollback" {
                    "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA cache_size=1; CREATE TABLE evidence(value INTEGER, payload BLOB); INSERT INTO evidence VALUES(1,NULL);"
                } else {
                    "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint=0; CREATE TABLE evidence(value INTEGER, payload BLOB); INSERT INTO evidence VALUES(1,NULL);"
                };
                owner.db().execute_batch(sql).unwrap();
                let journal_mode: String = owner
                    .db()
                    .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                    .unwrap();
                assert!(
                    journal_mode == if mode == "rollback" { "delete" } else { "wal" },
                    "journal mode refused"
                );
                if mode == "rollback" {
                    assert!(fs::write(
                        root.join("baseline-image"),
                        fs::read(root.join("stage/synthetic.sqlite")).unwrap()
                    )
                    .is_ok());
                }
                if mode == "wal" {
                    let checkpoint: (i64, i64, i64) = owner
                        .db()
                        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                        })
                        .unwrap();
                    assert!(checkpoint == (0, 0, 0), "checkpoint refused");
                    owner
                        .db()
                        .execute_batch("INSERT INTO evidence VALUES(2,zeroblob(8192));")
                        .unwrap();
                    // Main-file-only copy MUST still contain row 1, proving row 2
                    // depends on the committed WAL, not merely rollback of row 3.
                    let witness = root.join("checkpoint-only.sqlite");
                    fs::copy(root.join("stage/synthetic.sqlite"), &witness).unwrap();
                    let main = Connection::open_with_flags(
                        &witness,
                        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
                    )
                    .unwrap();
                    let values: Vec<i64> = main
                        .prepare("SELECT value FROM evidence ORDER BY value")
                        .unwrap()
                        .query_map([], |row| row.get(0))
                        .unwrap()
                        .map(|row| row.unwrap())
                        .collect();
                    assert!(values == [1], "WAL witness refused");
                    assert!(main.close().is_ok(), "witness close refused");
                    mark(&root, "wal_committed_only");
                }
                owner
                    .db()
                    .execute_batch(if mode == "wal" {
                        "BEGIN IMMEDIATE; INSERT INTO evidence VALUES(3,zeroblob(8192));"
                    } else {
                        "BEGIN IMMEDIATE; INSERT INTO evidence VALUES(2,zeroblob(8192));"
                    })
                    .unwrap();
                // SAFETY: live same-thread connection.
                assert!(
                    unsafe { ffi::sqlite3_db_cacheflush(owner.db().handle()) } == ffi::SQLITE_OK
                );
                mark(&root, "pending_ready");
            }
            let _statement = owner.statement();
            let owner = owner.busy();
            if mode == "panic_terminal" {
                mark(&root, "panic_ready");
                let _owner = owner;
                panic!("synthetic terminal unwind");
            }
            drop(owner);
        }
    }
    mark(&root, "completed");
}
