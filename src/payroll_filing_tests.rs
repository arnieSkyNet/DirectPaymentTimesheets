use super::super::tests::{fixture, pa, pdf, register};
use super::*;

fn setup() -> (tempfile::TempDir, Application, Move, Vec<PathBuf>) {
    let (dir, app) = fixture();
    fs::create_dir_all(&app.context.config.folders.pdf_output).unwrap();
    let source = register(
        &app,
        4,
        "p45",
        "2026/27",
        "P45 for Fictional Middletest Samplepa.pdf",
    );
    let destination = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2026 to 2027/Archived/P45 for Fictional Middletest Samplepa.pdf");
    let digest = file_digest(&source).unwrap();
    let id = app
        .payroll_timesheet_email_repository
        .documents_for_pa(4)
        .unwrap()[0]
        .id;
    let movement = Move {
        source,
        destination,
        digest,
        document_id: Some(id),
    };
    let bases = managed_bases(&app).unwrap();
    (dir, app, movement, bases)
}
fn db(app: &Application) -> &Connection {
    &app.payroll_timesheet_email_repository.connection
}
fn state(app: &Application) -> String {
    db(app)
        .query_row(
            "SELECT state FROM payroll_filing_intents LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn interrupt(point: Option<&'static str>) {
    INTERRUPT.with(|p| *p.borrow_mut() = point);
}

#[test]
fn every_interruption_boundary_reopens_and_recovers_without_duplicate_evidence() {
    for boundary in [
        "planned",
        "staged",
        "published_before_state",
        "published",
        "registered",
        "deleted_before_state",
        "source_removed",
        "stage_removed",
    ] {
        let (_dir, app, m, bases) = setup();
        let before = db(&app)
            .query_row(
                "SELECT id,sha256,history_state,sent_at FROM imported_payroll_documents",
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .unwrap();
        interrupt(Some(boundary));
        assert!(move_file(db(&app), 4, &bases, &m).is_err(), "{boundary}");
        interrupt(None);
        assert!(m.source.exists() || m.destination.exists(), "{boundary}");
        let reopened = crate::database::open(&app.context.environment.database_path).unwrap();
        recover(&reopened, 1, &bases).unwrap();
        recover(&reopened, 1, &bases).unwrap();
        assert!(!m.source.exists());
        assert_eq!(file_digest(&m.destination).unwrap(), m.digest);
        assert_eq!(state(&app), "complete");
        let after = db(&app)
            .query_row(
                "SELECT id,sha256,history_state,sent_at FROM imported_payroll_documents",
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            db(&app)
                .query_row::<i64, _, _>(
                    "SELECT COUNT(*) FROM imported_payroll_documents",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            1
        );
        assert!(pending_summary(db(&app), None).unwrap().is_empty());
    }
}
#[test]
fn saved_deactivation_request_can_resume_scan_without_toggling_status() {
    let (_dir, app, m, _bases) = setup();
    let old = pa(&app, 4);
    let mut inactive = old.clone();
    inactive.employment_status = Some("Inactive".into());
    let tx = db(&app).unchecked_transaction().unwrap();
    app.personal_assistant_repository.update(&inactive).unwrap();
    request(&tx, &app, &inactive, &old).unwrap();
    tx.commit().unwrap();
    assert!(pending_summary(db(&app), None)
        .unwrap()
        .contains("1 pending PA"));
    resume(&app, 4).unwrap();
    assert!(!m.source.exists());
    assert!(m.destination.exists());
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Inactive"));
    let mut active = pa(&app, 4);
    active.employment_status = Some("Active".into());
    super::super::apply(&app, &active, true).unwrap();
    resume(&app, 4).unwrap();
    assert!(m.destination.exists());
    assert!(!m.source.exists());
}
#[test]
fn existing_identical_or_conflicting_destinations_are_never_adopted() {
    for identical in [true, false] {
        let (_dir, app, m, bases) = setup();
        pdf(
            &m.destination,
            if identical {
                b"%PDF-1.4 unrelated identical"
            } else {
                b"%PDF-1.4 conflict"
            },
        );
        if identical {
            fs::copy(&m.source, &m.destination).unwrap();
        }
        let original = fs::read(&m.destination).unwrap();
        assert!(move_file(db(&app), 4, &bases, &m).is_err());
        assert!(m.source.exists());
        assert_eq!(fs::read(&m.destination).unwrap(), original);
    }
}
#[test]
fn unrelated_identical_destination_after_intent_does_not_prove_publication() {
    let (_dir, app, m, bases) = setup();
    let id = prepare(db(&app), 4, &m).unwrap();
    fs::create_dir_all(m.destination.parent().unwrap()).unwrap();
    fs::copy(&m.source, &m.destination).unwrap();
    assert!(recover(db(&app), id, &bases).is_err());
    assert!(m.source.exists());
    assert_eq!(state(&app), "planned");
}
#[test]
fn changed_source_destination_or_staging_provenance_blocks_deletion() {
    for changed in ["source", "destination", "stage"] {
        let (_dir, app, m, bases) = setup();
        interrupt(Some("published"));
        assert!(move_file(db(&app), 4, &bases, &m).is_err());
        interrupt(None);
        match changed {
            "source" => fs::write(&m.source, b"%PDF-1.4 changed").unwrap(),
            "destination" => {
                fs::remove_file(&m.destination).unwrap();
                pdf(&m.destination, b"%PDF-1.4 changed");
            }
            _ => {
                let stage: String = db(&app)
                    .query_row("SELECT staging_path FROM payroll_filing_intents", [], |r| {
                        r.get(0)
                    })
                    .unwrap();
                fs::remove_file(&stage).unwrap();
                fs::copy(&m.source, &stage).unwrap();
            }
        }
        assert!(recover(db(&app), 1, &bases).is_err());
        assert!(m.source.exists());
        assert_eq!(state(&app), "published");
    }
}
#[test]
fn restored_registration_membership_mismatch_never_reconciles_files() {
    let (_dir, app, m, bases) = setup();
    interrupt(Some("published"));
    assert!(move_file(db(&app), 4, &bases, &m).is_err());
    interrupt(None);
    db(&app)
        .execute(
            "UPDATE imported_payroll_documents SET history_state='external'",
            [],
        )
        .unwrap();
    assert!(recover(db(&app), 1, &bases).is_err());
    assert!(m.source.exists());
    assert!(m.destination.exists());
    assert_eq!(
        app.payroll_timesheet_email_repository
            .documents_for_pa(4)
            .unwrap()[0]
            .path,
        m.source
    );
}
#[test]
fn unavailable_root_remains_pending_while_other_root_files() {
    let (_dir, app, m, _bases) = setup();
    let root = &app.context.config.folders.pdf_output;
    fs::remove_dir(root).unwrap();
    fs::write(root, b"filesystem obstruction").unwrap();
    let mut inactive = pa(&app, 4);
    inactive.employment_status = Some("Inactive".into());
    let messages = super::super::apply(&app, &inactive, true)
        .unwrap()
        .join("\n");
    assert!(messages.contains("pending"));
    assert!(!m.source.exists());
    assert!(m.destination.exists());
    assert!(pending_summary(db(&app), Some(4))
        .unwrap()
        .contains("pending"));
    fs::remove_file(root).unwrap();
    fs::create_dir(root).unwrap();
    resume(&app, 4).unwrap();
    assert!(pending_summary(db(&app), None).unwrap().is_empty());
}
#[test]
fn disconnected_root_does_not_delete_original_or_change_configuration() {
    let (_dir, app, m, _bases) = setup();
    let id = prepare(db(&app), 4, &m).unwrap();
    let root = app.context.config.folders.payslip_folder.clone();
    let offline = root.with_extension("offline");
    fs::rename(&root, &offline).unwrap();
    assert!(recover(db(&app), id, &available_bases(&app).0).is_err());
    assert_eq!(app.context.config.folders.payslip_folder, root);
    assert!(offline.join(m.source.strip_prefix(&root).unwrap()).exists());
    fs::rename(&offline, &root).unwrap();
    resume(&app, 4).unwrap();
    assert!(m.destination.exists());
}
#[test]
fn dispatch_restore_and_multiple_instance_lock_exclude_all_filing_entrypoints() {
    let (_dir, app, m, _bases) = setup();
    let second = crate::database::open(&app.context.environment.database_path).unwrap();
    let guard = crate::timesheet_delivery::production_lock(&second).unwrap();
    let mut inactive = pa(&app, 4);
    inactive.employment_status = Some("Inactive".into());
    assert!(super::super::apply(&app, &inactive, true).is_err());
    assert!(resume(&app, 4).is_err());
    assert!(app.import_payroll_documents(&m.source, None).is_err());
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
    assert!(m.source.exists());
    drop(guard);
    super::super::apply(&app, &inactive, true).unwrap();
    assert!(m.destination.exists());
}
#[test]
fn uncertainty_blocks_filing_until_audited_reconciliation() {
    let (_dir, app, m, _bases) = setup();
    db(&app)
        .execute(
            "UPDATE imported_payroll_documents SET sent_at='indeterminate:fixture'",
            [],
        )
        .unwrap();
    let mut inactive = pa(&app, 4);
    inactive.employment_status = Some("Inactive".into());
    assert!(super::super::apply(&app, &inactive, true).is_err());
    assert!(resume(&app, 4).is_err());
    assert!(m.source.exists());
}
#[test]
fn migration_preserves_populated_38_journal_and_refuses_old_writers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db.sqlite");
    let db = crate::database::open(&path).unwrap();
    crate::database::create_legacy_schema(&db, 38).unwrap();
    db.execute(
        "INSERT INTO payroll_file_moves VALUES('/original','/Archived/copy',4,?1)",
        ["a".repeat(64)],
    )
    .unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    db.execute_batch("CREATE TRIGGER fail39 BEFORE UPDATE ON schema_version WHEN NEW.version=39 BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(migrate(&db).is_err());
    assert_eq!(crate::database_recovery::version(&db).unwrap(), Some(38));
    db.execute_batch("DROP TRIGGER fail39").unwrap();
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    migrate(&db).unwrap();
    assert_eq!(crate::database_recovery::version(&db).unwrap(), Some(39));
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_file_moves", [], |r| r.get(0))
            .unwrap(),
        1
    );
    let old = Connection::open(path).unwrap();
    old.create_scalar_function(
        "dpt_schema_version",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |_| Ok(38),
    )
    .unwrap();
    assert!(old
        .execute("UPDATE schema_version SET version=38", [])
        .is_err());
    assert!(old.execute("DELETE FROM payroll_file_moves", []).is_err());
}

#[test]
fn partial_staging_copy_requires_review_and_never_deletes_original() {
    let (_dir, app, m, bases) = setup();
    interrupt(Some("staging_created"));
    assert!(move_file(db(&app), 4, &bases, &m).is_err());
    interrupt(None);
    assert!(recover(db(&app), 1, &bases).is_err());
    assert!(m.source.exists());
    assert!(!m.destination.exists());
    assert_eq!(state(&app), "planned");
    assert!(pending_summary(db(&app), Some(4))
        .unwrap()
        .contains("planned"));
}
#[test]
fn case_only_collision_after_planning_is_refused() {
    let (_dir, app, m, bases) = setup();
    let id = prepare(db(&app), 4, &m).unwrap();
    let other = m.destination.with_file_name(
        m.destination
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_uppercase(),
    );
    pdf(&other, b"%PDF-1.4 unrelated");
    assert!(recover(db(&app), id, &bases).is_err());
    assert!(m.source.exists());
    assert_eq!(fs::read(&other).unwrap(), b"%PDF-1.4 unrelated");
}
#[test]
fn repair_and_replacement_are_excluded_by_sender_lock() {
    let (_dir, app, m, _bases) = setup();
    let incoming = m
        .source
        .parent()
        .unwrap()
        .join("P45 revised for Fictional Middletest Samplepa.pdf");
    pdf(&incoming, b"%PDF-1.4 revised");
    let target = crate::payroll_replacement::targets(&app, 4)
        .unwrap()
        .remove(0);
    let reviewed = crate::payroll_replacement::review(&app, target, incoming).unwrap();
    let guard = crate::timesheet_delivery::production_lock(db(&app)).unwrap();
    assert!(super::super::apply(&app, &pa(&app, 4), false).is_err());
    assert!(crate::payroll_replacement::replace(&app, &reviewed).is_err());
    assert!(crate::database_recovery::initialise(&app.context.environment.database_path).is_err());
    assert!(m.source.exists());
    drop(guard);
    let new = crate::payroll_replacement::replace(&app, &reviewed).unwrap();
    assert!(new.exists());
    assert!(m.source.exists());
}
#[test]
fn lock_filesystem_failure_precedes_pa_or_document_changes() {
    let (_dir, app, m, _bases) = setup();
    let lock = app
        .context
        .environment
        .database_path
        .canonicalize()
        .unwrap()
        .with_extension("timesheet-delivery.lock");
    if lock.exists() {
        fs::remove_file(&lock).unwrap();
    }
    fs::create_dir(&lock).unwrap();
    let mut inactive = pa(&app, 4);
    inactive.employment_status = Some("Inactive".into());
    assert!(super::super::apply(&app, &inactive, true).is_err());
    assert!(resume(&app, 4).is_err());
    assert!(m.source.exists());
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
}
#[test]
fn changed_configuration_refuses_resume_before_any_file_mutation() {
    let (_dir, mut app, m, _bases) = setup();
    let old = pa(&app, 4);
    request(db(&app), &app, &old, &old).unwrap();
    prepare(db(&app), 4, &m).unwrap();
    app.context.config.folders.pdf_output = app
        .context
        .config
        .folders
        .pdf_output
        .with_extension("changed");
    assert!(resume(&app, 4).is_err());
    assert!(m.source.exists());
    assert!(!m.destination.exists());
}
#[test]
fn registered_destination_change_after_commit_blocks_source_removal() {
    let (_dir, app, m, bases) = setup();
    interrupt(Some("registered"));
    assert!(move_file(db(&app), 4, &bases, &m).is_err());
    interrupt(None);
    db(&app)
        .execute(
            "UPDATE imported_payroll_documents SET stored_path='/restored-older-path'",
            [],
        )
        .unwrap();
    assert!(recover(db(&app), 1, &bases).is_err());
    assert!(m.source.exists());
    assert!(m.destination.exists());
    assert_eq!(state(&app), "registered");
}
#[test]
fn startup_summary_is_nonblocking_and_persists_after_reopening() {
    let (_dir, app, m, _bases) = setup();
    prepare(db(&app), 4, &m).unwrap();
    crate::database_recovery::initialise(&app.context.environment.database_path).unwrap();
    let reopened = crate::database::open(&app.context.environment.database_path).unwrap();
    assert!(pending_summary(&reopened, None)
        .unwrap()
        .contains("1 document operation"));
    assert!(pending_summary(&reopened, Some(4))
        .unwrap()
        .contains(m.source.to_str().unwrap()));
    let gui = crate::gui::DirectPaymentApp::new(app);
    drop(gui);
}

#[test]
fn older_backup_restore_reports_path_mismatch_without_orphan_adoption() {
    use crate::backup_service::{BackupService, RestoreAuthorisation};
    let (_dir, app, m, bases) = setup();
    let root = &app.context.environment.data_dir;
    let config = root.join("config.toml");
    let backups = root.join("backups");
    let backup =
        BackupService::create_verified(&app.context.environment.database_path, &config, &backups)
            .unwrap();
    move_file(db(&app), 4, &bases, &m).unwrap();
    let plan = BackupService::preview_restore(
        &backup,
        &app.context.environment.database_path,
        &config,
        &backups,
    )
    .unwrap();
    assert!(plan
        .replaced_rows
        .iter()
        .any(|(table, _)| table == "payroll_filing_intents"));
    let mut approval = RestoreAuthorisation {
        plan,
        reason: "Fixture-only informed rollback; external archive stays retained".into(),
        acknowledged: false,
    };
    assert!(BackupService::restore(
        &backup,
        &app.context.environment.database_path,
        &config,
        &backups,
        &approval
    )
    .is_err());
    assert_eq!(state(&app), "complete");
    approval.acknowledged = true;
    let result = BackupService::restore(
        &backup,
        &app.context.environment.database_path,
        &config,
        &backups,
        &approval,
    )
    .unwrap();
    assert!(result.safety_backup.join("database.sqlite").exists());
    assert!(m.destination.exists());
    assert!(!m.source.exists());
    let messages = resume(&app, 4).unwrap().join("\n");
    assert!(messages.contains("not automatically adopted"), "{messages}");
    assert_eq!(
        app.payroll_timesheet_email_repository
            .documents_for_pa(4)
            .unwrap()[0]
            .path,
        m.source
    );
    assert!(m.destination.exists());
    assert!(!m.source.exists());
}
#[test]
fn verified_wal_upgrade_failure_keeps_populated_38_and_recovery_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("database.sqlite");
    let db = crate::database::open(&path).unwrap();
    crate::database::create_legacy_schema(&db, 38).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; INSERT INTO personal_assistants(id,first_name,surname) VALUES(4,'Fixture','PA'); CREATE TRIGGER fail39 BEFORE UPDATE ON schema_version WHEN NEW.version=39 BEGIN SELECT RAISE(ABORT,'stage6 fixture'); END;").unwrap();
    db.execute(
        "INSERT INTO payroll_file_moves VALUES('/fixture/original','/fixture/Archived/copy',4,?1)",
        ["b".repeat(64)],
    )
    .unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    assert!(crate::database_recovery::initialise(&path).is_err());
    assert_eq!(crate::database_recovery::version(&db).unwrap(), Some(38));
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    let backups =
        crate::backup_service::BackupService::discover(&dir.path().join("backups")).unwrap();
    assert_eq!(backups.len(), 1);
    let saved = Connection::open(&backups[0].path.join("database.sqlite")).unwrap();
    assert_eq!(
        crate::database_recovery::fingerprint(&saved).unwrap(),
        before
    );
    db.execute_batch("DROP TRIGGER fail39").unwrap();
    crate::database_recovery::initialise(&path).unwrap();
    assert_eq!(crate::database_recovery::version(&db).unwrap(), Some(39));
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_file_moves", [], |r| r.get(0))
            .unwrap(),
        1
    );
}
#[test]
fn reactivated_pending_scan_cannot_archive_new_active_documents() {
    let (_dir, app, m, bases) = setup();
    let old = pa(&app, 4);
    let mut inactive = old.clone();
    inactive.employment_status = Some("Inactive".into());
    let tx = db(&app).unchecked_transaction().unwrap();
    app.personal_assistant_repository.update(&inactive).unwrap();
    request(&tx, &app, &inactive, &old).unwrap();
    tx.commit().unwrap();
    prepare(db(&app), 4, &m).unwrap();
    let mut active = pa(&app, 4);
    active.employment_status = Some("Active".into());
    super::super::apply(&app, &active, true).unwrap();
    let new = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2026 to 2027/Payslip for Week 30 for Fictional Middletest Samplepa.pdf");
    pdf(&new, b"%PDF-1.4 new active work");
    let messages = resume(&app, 4).unwrap().join("\n");
    assert!(messages.contains("unplanned rescan"));
    assert!(new.exists());
    assert!(m.destination.exists());
    assert!(!m.source.exists());
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
    recover(db(&app), 1, &bases).unwrap();
    assert!(new.exists());
}
#[cfg(unix)]
#[test]
fn unsafe_year_entry_does_not_hide_other_years_or_roots() {
    let (_dir, app, m, _bases) = setup();
    let root = &app.context.config.folders.payslip_folder;
    std::os::unix::fs::symlink(
        &app.context.config.folders.pdf_output,
        root.join("2024 to 2025"),
    )
    .unwrap();
    let mut inactive = pa(&app, 4);
    inactive.employment_status = Some("Inactive".into());
    let messages = super::super::apply(&app, &inactive, true)
        .unwrap()
        .join("\n");
    assert!(messages.contains("enumerating"));
    assert!(m.destination.exists());
    assert!(!m.source.exists());
    assert!(pending_summary(db(&app), Some(4))
        .unwrap()
        .contains("pending"));
}

#[test]
fn durable_request_failure_rolls_back_pa_deactivation_before_any_publication() {
    let (_dir, app, m, _bases) = setup();
    db(&app).execute_batch("CREATE TRIGGER fail_request BEFORE INSERT ON payroll_filing_requests BEGIN SELECT RAISE(ABORT,'fixture request failure'); END;").unwrap();
    let mut inactive = pa(&app, 4);
    inactive.employment_status = Some("Inactive".into());
    assert!(super::super::apply(&app, &inactive, true).is_err());
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
    assert!(m.source.exists());
    assert!(!m.destination.exists());
    assert!(pending_summary(db(&app), None).unwrap().is_empty());
}
#[test]
fn recreated_original_after_removal_is_retained_for_review() {
    let (_dir, app, m, bases) = setup();
    interrupt(Some("source_removed"));
    assert!(move_file(db(&app), 4, &bases, &m).is_err());
    interrupt(None);
    fs::copy(&m.destination, &m.source).unwrap();
    assert!(recover(db(&app), 1, &bases).is_err());
    assert!(m.source.exists());
    assert!(m.destination.exists());
    assert_eq!(state(&app), "source_removed");
}
#[test]
fn concurrent_connections_share_one_durable_intent_and_cannot_publish_over_sender() {
    let (_dir, app, m, _bases) = setup();
    let mut threads = Vec::new();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    for _ in 0..2 {
        let barrier = barrier.clone();
        let path = app.context.environment.database_path.clone();
        let source = m.source.clone();
        let destination = m.destination.clone();
        let digest = m.digest.clone();
        let document_id = m.document_id;
        threads.push(std::thread::spawn(move || {
            let db = crate::database::open(path).unwrap();
            barrier.wait();
            prepare(
                &db,
                4,
                &Move {
                    source,
                    destination,
                    digest,
                    document_id,
                },
            )
            .unwrap()
        }));
    }
    assert_eq!(
        threads.remove(0).join().unwrap(),
        threads.remove(0).join().unwrap()
    );
    assert_eq!(
        db(&app)
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_filing_intents", [], |r| r
                .get(0))
            .unwrap(),
        1
    );
    let guard = crate::timesheet_delivery::production_lock(db(&app)).unwrap();
    assert!(resume(&app, 4).is_err());
    assert!(m.source.exists());
    drop(guard);
    resume(&app, 4).unwrap();
    assert!(m.destination.exists());
    assert!(!m.source.exists());
}

#[test]
fn byte_identical_external_destination_replacement_after_registration_keeps_original() {
    let (_dir, app, m, bases) = setup();
    interrupt(Some("registered"));
    assert!(move_file(db(&app), 4, &bases, &m).is_err());
    interrupt(None);
    fs::remove_file(&m.destination).unwrap();
    fs::copy(&m.source, &m.destination).unwrap();
    assert_eq!(file_digest(&m.destination).unwrap(), m.digest);
    assert!(recover(db(&app), 1, &bases).is_err());
    assert!(m.source.exists());
    assert!(m.destination.exists());
    assert_eq!(state(&app), "registered");
}

#[test]
fn ordinary_pa_contact_edits_remain_available_while_document_delivery_is_uncertain() {
    let (_dir, app, m, _bases) = setup();
    db(&app)
        .execute(
            "UPDATE imported_payroll_documents SET sent_at='indeterminate:fixture'",
            [],
        )
        .unwrap();
    let mut edited = pa(&app, 4);
    edited.email = Some("new-contact@example.test".into());
    let messages = super::super::apply(&app, &edited, true).unwrap().join("\n");
    assert!(messages.contains("cleanup deferred"));
    assert_eq!(pa(&app, 4).email, edited.email);
    assert!(m.source.exists());
    assert!(!m.destination.exists());
    assert_eq!(
        db(&app)
            .query_row::<String, _, _>("SELECT sent_at FROM imported_payroll_documents", [], |r| r
                .get(0))
            .unwrap(),
        "indeterminate:fixture"
    );
}

#[test]
fn unchanged_uncertain_import_uses_registered_canonical_path_and_preserves_history() {
    let (dir, mut app) = fixture();
    let incoming = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("P45 for year 2026-27 for Fictional Middletest Samplepa.pdf");
    pdf(&incoming, b"%PDF-1.4 immutable incoming");
    let first = app.import_payroll_documents(&incoming, None).unwrap();
    assert_eq!(first.supplements_imported, 1, "{:?}", first.failures);
    db(&app).execute("UPDATE imported_payroll_documents SET history_state='needs_sending',sent_at='indeterminate:fixture'",[]).unwrap();
    app.context.config.folders.payslip_folder = app.context.config.folders.payslip_folder.join(".");
    let second = app.import_payroll_documents(&incoming, None).unwrap();
    assert_eq!(
        second.supplements_already_present, 1,
        "{:?}",
        second.failures
    );
    assert!(second.failures.is_empty());
    assert_eq!(
        db(&app)
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM imported_payroll_documents", [], |r| r
                .get(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db(&app)
            .query_row::<String, _, _>("SELECT sent_at FROM imported_payroll_documents", [], |r| r
                .get(0))
            .unwrap(),
        "indeterminate:fixture"
    );
}

#[test]
fn recreated_unregistered_source_cannot_reuse_completed_intent_or_claim_success() {
    let (_dir, app) = fixture();
    fs::create_dir_all(&app.context.config.folders.pdf_output).unwrap();
    let source = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2026 to 2027/Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
    pdf(&source, b"%PDF-1.4 retained ordinary");
    let destination = source
        .parent()
        .unwrap()
        .join("Archived")
        .join(source.file_name().unwrap());
    let movement = Move {
        digest: file_digest(&source).unwrap(),
        source,
        destination,
        document_id: None,
    };
    let bases = managed_bases(&app).unwrap();
    move_file(db(&app), 4, &bases, &movement).unwrap();
    fs::copy(&movement.destination, &movement.source).unwrap();
    fs::remove_file(&movement.destination).unwrap();
    let mut inactive = pa(&app, 4);
    inactive.employment_status = Some("Inactive".into());
    let messages = super::super::apply(&app, &inactive, true)
        .unwrap()
        .join("\n");
    assert!(
        messages.contains("not counted as newly filed"),
        "{messages}"
    );
    assert!(!messages.contains("1 payroll file(s) filed"));
    assert!(movement.source.exists());
    assert!(!movement.destination.exists());
    assert_eq!(state(&app), "complete");
    assert!(pending_summary(db(&app), Some(4))
        .unwrap()
        .contains("1 pending PA"));
}
