use super::*;
#[test]
fn stage7_populated_schema39_migration_preserves_legacy_evidence_and_rolls_back_failure() {
    let db = crate::database::open_in_memory().unwrap();
    crate::database::create_legacy_schema(&db, 39).unwrap();
    db.execute_batch("INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Legacy','PA');
        INSERT INTO timesheets(id,pa_name,personal_assistant_id,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount,notes) VALUES(1,'Legacy PA',1,'2 April 2026 at 09:00','2 April 2026 at 10:00',0,60,12,12,'legacy');
        INSERT INTO shift_change_events(source,source_id,before_evidence,after_evidence,reason,review_signature,recorded_at,actor) VALUES('imported',1,'before','after','historical decision','legacy','old','local_employer');
        CREATE TRIGGER fail40 BEFORE UPDATE ON schema_version WHEN NEW.version=40 BEGIN SELECT RAISE(ABORT,'fixture migration failure'); END;").unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    assert!(migrate(&db).is_err());
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    db.execute_batch("DROP TRIGGER fail40").unwrap();
    let before_row = repository::get_raw_on(&db, 1).unwrap();
    migrate(&db).unwrap();
    assert_eq!(repository::get_raw_on(&db, 1).unwrap(), before_row);
    assert_eq!(snapshot(&db, 1).unwrap().source_event, Some(1));
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM imported_hours_inclusion_events",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap(),
        40
    );
    db.create_scalar_function(
        "dpt_schema_version",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        |_| Ok(39_i64),
    )
    .unwrap();
    assert!(db
        .execute("UPDATE timesheets SET worked_minutes=0 WHERE id=1", [])
        .is_err());
    assert!(db.execute("DELETE FROM shift_change_events", []).is_err());
}
#[test]
fn stage7_schema39_verified_upgrade_keeps_recoverable_original_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("database.sqlite");
    let db = crate::database::open(&path).unwrap();
    crate::database::create_legacy_schema(&db, 39).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Migration','Fixture'); CREATE TRIGGER fail40 BEFORE UPDATE ON schema_version WHEN NEW.version=40 BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    assert!(crate::database_recovery::initialise(&path).is_err());
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    let backups =
        crate::backup_service::BackupService::discover(&dir.path().join("backups")).unwrap();
    assert_eq!(backups.len(), 1);
    let saved = crate::database::open(backups[0].path.join("database.sqlite")).unwrap();
    assert_eq!(
        crate::database_recovery::fingerprint(&saved).unwrap(),
        before
    );
    db.execute_batch("DROP TRIGGER fail40").unwrap();
    crate::database_recovery::initialise(&path).unwrap();
    assert_eq!(crate::database_recovery::version(&db).unwrap(), Some(40));
}
