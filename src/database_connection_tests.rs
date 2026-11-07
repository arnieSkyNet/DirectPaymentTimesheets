//! Nonterminal synthetic tests only: no abort, watchdog or native failure injection.
use super::*;

fn stage() -> Arc<PrivateTempDir> {
    Arc::new(crate::private_storage::tempdir().unwrap())
}

fn owned(stage: Arc<PrivateTempDir>) -> DatabaseConnection {
    let path = stage.path().join("owned.sqlite");
    crate::database::open_owned(&path, Some(stage)).unwrap()
}

struct NativeStatement(*mut rusqlite::ffi::sqlite3_stmt);
impl NativeStatement {
    fn new(owner: &DatabaseConnection) -> Self {
        let mut statement = std::ptr::null_mut();
        // SAFETY: a same-thread live connection and valid static SQL. This test
        // explicitly finalizes before successful close, including during unwind.
        let result = unsafe {
            rusqlite::ffi::sqlite3_prepare_v2(
                owner.connection().handle(),
                b"SELECT 1\0".as_ptr().cast(),
                -1,
                &mut statement,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(result, rusqlite::ffi::SQLITE_OK);
        assert!(!statement.is_null());
        Self(statement)
    }
}
impl Drop for NativeStatement {
    fn drop(&mut self) {
        // SAFETY: this test owns exactly one valid prepared statement.
        unsafe { rusqlite::ffi::sqlite3_finalize(self.0) };
    }
}

#[test]
fn writable_open_registers_and_supports_borrowed_statements_and_transactions() {
    let mut owner = owned(stage());
    assert_eq!(
        owner
            .connection()
            .query_row("SELECT dpt_schema_version()", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        crate::database::CURRENT_SCHEMA_VERSION
    );
    owner
        .connection()
        .execute_batch("CREATE TABLE evidence(value INTEGER)")
        .unwrap();
    {
        let tx = owner.transaction().unwrap();
        tx.execute("INSERT INTO evidence VALUES(1)", []).unwrap();
        tx.commit().unwrap();
    }
    {
        let mut statement = owner
            .connection()
            .prepare("SELECT value FROM evidence")
            .unwrap();
        assert_eq!(
            statement.query_row([], |row| row.get::<_, i64>(0)).unwrap(),
            1
        );
    }
    owner.close().unwrap();
}

#[test]
fn read_only_open_does_not_register_or_migrate() {
    let staging = stage();
    let path = staging.path().join("owned.sqlite");
    owned(staging.clone()).close().unwrap();
    let before = std::fs::read(&path).unwrap();
    let owner = crate::database::open_read_only_owned(&path, Some(staging.clone())).unwrap();
    assert!(owner
        .connection()
        .is_readonly(rusqlite::DatabaseName::Main)
        .unwrap());
    assert!(owner
        .connection()
        .execute_batch("CREATE TABLE refused(value)")
        .is_err());
    assert!(owner
        .connection()
        .query_row("SELECT dpt_schema_version()", [], |_| Ok(()))
        .is_err());
    owner.close().unwrap();
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn read_only_missing_database_does_not_create_it() {
    let staging = stage();
    let path = staging.path().join("missing.sqlite");
    assert!(matches!(
        crate::database::open_read_only_owned(&path, Some(staging.clone())),
        Err(DatabaseOpenError::Open(_))
    ));
    assert!(!path.exists());
}

#[test]
fn registration_failure_returns_live_owner_and_staging() {
    let staging = stage();
    let weak = Arc::downgrade(&staging);
    let result = DatabaseConnection::open_registered(
        &staging.path().join("registration.sqlite"),
        Some(staging.clone()),
        |connection| {
            crate::database::register_connection(connection)?;
            connection.execute_batch("INVALID SYNTHETIC REGISTRATION")
        },
    );
    drop(staging);
    let Err(DatabaseOpenError::Registration { owner, error }) = result else {
        panic!("expected post-open registration error");
    };
    // Bundled modern SQLite reports SQL parser failures as SqlInputError;
    // configurations without error-offset support use SqliteFailure instead.
    assert!(matches!(
        error,
        Error::SqlInputError { .. } | Error::SqliteFailure(..)
    ));
    assert!(weak.upgrade().is_some());
    assert_eq!(
        owner
            .connection()
            .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    owner.close().unwrap();
    assert!(weak.upgrade().is_none());
}

#[test]
fn repeated_busy_close_returns_complete_owner_until_retry_succeeds() {
    let staging = stage();
    let weak = Arc::downgrade(&staging);
    let mut owner = owned(staging.clone());
    // Retain a real synthetic handle in the same resource bundle on every OS.
    // This is an ownership assertion, not native ACL/sharing acceptance.
    owner
        .resources
        ._guards
        .push(crate::private_storage::open_read(&staging.path().join("owned.sqlite")).unwrap());
    drop(staging);
    let statement = NativeStatement::new(&owner);
    for _ in 0..2 {
        let (returned, error) = owner.close().unwrap_err();
        assert!(
            matches!(error, Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::DatabaseBusy)
        );
        assert!(weak.upgrade().is_some());
        assert!(returned
            .resources
            ._guards
            .iter()
            .all(|file| file.metadata().is_ok()));
        assert!(!returned.close_confirmed);
        owner = returned;
    }
    drop(statement);
    owner.close().unwrap();
    assert!(weak.upgrade().is_none());
}

#[test]
fn shared_staging_survives_another_connection_closing() {
    let staging = stage();
    let weak = Arc::downgrade(&staging);
    let first = owned(staging.clone());
    let second = owned(staging.clone());
    drop(staging);
    first.close().unwrap();
    assert!(weak.upgrade().is_some());
    assert_eq!(
        second
            .connection()
            .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    second.close().unwrap();
    assert!(weak.upgrade().is_none());
}

#[test]
fn borrowed_backup_includes_committed_wal_and_retains_both_stages() {
    let source_stage = stage();
    let destination_stage = stage();
    let source_weak = Arc::downgrade(&source_stage);
    let destination_weak = Arc::downgrade(&destination_stage);
    let source = owned(source_stage.clone());
    let mut destination = owned(destination_stage.clone());
    source.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE evidence(value INTEGER); INSERT INTO evidence VALUES(1); INSERT INTO evidence VALUES(2);").unwrap();
    assert!(source_stage.path().join("owned.sqlite-wal").is_file());
    drop(source_stage);
    drop(destination_stage);
    {
        let snapshot = source.unchecked_transaction().unwrap();
        let expected = crate::database_recovery::fingerprint(&snapshot).unwrap();
        source.copy_into(&mut destination).unwrap();
        assert_eq!(
            crate::database_recovery::fingerprint(&destination).unwrap(),
            expected
        );
    }
    {
        let backup = source.backup_into(&mut destination).unwrap();
        assert!(source_weak.upgrade().is_some());
        assert!(destination_weak.upgrade().is_some());
        let mut result = rusqlite::backup::StepResult::More;
        while result == rusqlite::backup::StepResult::More {
            result = backup.step(100).unwrap();
        }
        assert_eq!(result, rusqlite::backup::StepResult::Done);
    }
    assert_eq!(
        destination
            .query_row("SELECT sum(value) FROM evidence", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
    source.close_or_retain().unwrap();
    assert!(source_weak.upgrade().is_none());
    assert!(destination_weak.upgrade().is_some());
    destination.close_or_retain().unwrap();
    assert!(destination_weak.upgrade().is_none());
}

#[test]
fn ordinary_unwind_rolls_back_and_closes_before_last_staging_release() {
    let staging = stage();
    let path = staging.path().join("owned.sqlite");
    let weak = Arc::downgrade(&staging);
    let mut owner = owned(staging.clone());
    owner
        .connection()
        .execute_batch("CREATE TABLE evidence(value INTEGER)")
        .unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let tx = owner.transaction().unwrap();
        tx.execute("INSERT INTO evidence VALUES(1)", []).unwrap();
        panic!("synthetic caller unwind");
    }));
    assert!(panic.is_err());
    assert_eq!(Arc::strong_count(&staging), 1);
    let reader = crate::database::open_read_only_owned(&path, Some(staging.clone())).unwrap();
    assert_eq!(
        reader
            .connection()
            .query_row("SELECT count(*) FROM evidence", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    reader.close().unwrap();
    drop(staging);
    assert!(weak.upgrade().is_none());
}
