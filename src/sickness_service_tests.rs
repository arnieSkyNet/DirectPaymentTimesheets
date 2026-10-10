use super::*;
use crate::payroll_snapshot_service::{publish_candidate, CandidatePublication};
use crate::payroll_worked_item_repository::PayrollWorkedItemRepository;
use crate::timesheet_delivery as delivery;

fn fixture() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::database::open(dir.path().join("payroll.sqlite")).unwrap();
    crate::database::create_schema(&db).unwrap();
    db.execute_batch("INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Sickness','Test');
        INSERT INTO payroll_schedules(payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES('2026/27',7,'31/08/2026','x','x','x'),('2026/27',8,'28/09/2026','x','x','x');
        INSERT INTO payroll_timesheets(id,personal_assistant_id,payroll_year,cycle_number,created_at,updated_at) VALUES(10,1,'2026/27',7,'original','original'),(11,1,'2026/27',8,'original','original');").unwrap();
    for (id, start) in [(10, "2026-08-31"), (11, "2026-09-28")] {
        let start = crate::date_utils::parse_legacy(start).unwrap();
        for week in 0..4 {
            db.execute("INSERT INTO payroll_timesheet_weeks(id,payroll_timesheet_id,week_number,week_commencing,worked_hours) VALUES(?1,?2,?3,?4,0)",params![id*10+week,id,week+1,crate::date_utils::iso(start+Duration::days(week*7))]).unwrap();
        }
    }
    (dir, db)
}
fn row(a: &str, b: &str) -> SicknessPeriod {
    SicknessPeriod {
        id: 0,
        personal_assistant_id: 1,
        start_date: a.into(),
        end_date: b.into(),
    }
}
fn change(
    db: &Connection,
    id: i64,
    old: Option<&SicknessPeriod>,
    new: Option<&SicknessPeriod>,
    scope: Scope,
) -> Result<Option<SicknessPeriod>> {
    let plan = review(db, id, old, new)?;
    mutate(
        db,
        id,
        old,
        new,
        scope,
        "Reviewed against sickness records",
        &plan.signature,
    )
}
fn publish(dir: &tempfile::TempDir, db: &Connection, id: i64, label: &str) -> delivery::Intent {
    let repo = PayrollWorkedItemRepository::new(
        crate::database::open(dir.path().join("payroll.sqlite")).unwrap(),
    );
    let path = dir.path().join(format!("{id}-{label}.pdf"));
    let evidence = encode(&evidence(db, id).unwrap()).unwrap();
    let correction = correction_note(db, id).unwrap();
    let bytes = format!("PDF {label} {evidence} {correction:?}");
    let historical = historical_hours(db, id).unwrap();
    let items = historical
        .as_ref()
        .map(|h| h.snapshot_items.as_slice())
        .unwrap_or(&[]);
    let totals = historical
        .as_ref()
        .map(|h| h.week_totals_minutes)
        .unwrap_or([0; 4]);
    let carry = historical.as_ref().map_or(0, |h| h.previous_cycle_minutes);
    publish_candidate(
        &repo,
        CandidatePublication {
            payroll_timesheet_id: id,
            items,
            final_pdf_path: &path,
            generated_at: "test",
            previous_cycle_minutes: carry,
            week_ids: &[id * 10, id * 10 + 1, id * 10 + 2, id * 10 + 3],
            week_totals_minutes: &totals,
        },
        |p| {
            std::fs::write(p, bytes)?;
            Ok(())
        },
    )
    .unwrap();
    delivery::capture(db, id).unwrap()
}
fn send(db: &Connection, i: &delivery::Intent) -> Result<bool> {
    let bytes = delivery::bytes(db, i)?;
    delivery::execute(
        db,
        i,
        "Payroll; CC employer; BCC PA",
        &format!("<{}@sickness.test>", i.intent_id),
        &bytes,
        |_| Ok(()),
        || Ok(()),
    )
}
fn protect(dir: &tempfile::TempDir, db: &Connection, settled: bool) -> delivery::Intent {
    let i = publish(dir, db, 10, "original");
    send(db, &i).unwrap();
    if settled {
        db.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',7,'payslip','settled-original')",[]).unwrap();
    }
    i
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn add_edit_delete_invalidate_candidates_and_regeneration_projects_new_dates() {
    let (dir, db) = fixture();
    let i = publish(&dir, &db, 10, "empty");
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-05", "2026-09-08")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    assert!(send(&db, &i).is_err());
    assert_eq!(count(&db, "timesheet_delivery_attempts"), 0);
    let i = publish(&dir, &db, 10, "added");
    let mut revised = p.clone();
    revised.end_date = "2026-09-10".into();
    let p2 = change(&db, 10, Some(&p), Some(&revised), Scope::Editable)
        .unwrap()
        .unwrap();
    assert!(send(&db, &i).is_err());
    let start = crate::date_utils::parse_legacy("2026-08-31").unwrap();
    let projected = projection(
        &evidence(&db, 10).unwrap(),
        &std::array::from_fn(|i| start + Duration::days(i as i64 * 7)),
    )
    .unwrap();
    assert_eq!(projected[0][0][1], "10/09/2026)");
    assert_eq!(projected[0], projected[1]);
    let i = publish(&dir, &db, 10, "edited");
    change(&db, 10, Some(&p2), None, Scope::Editable).unwrap();
    assert!(send(&db, &i).is_err());
    let fresh = publish(&dir, &db, 10, "deleted");
    assert!(!fresh.resend);
    assert!(send(&db, &fresh).unwrap());
    assert_eq!(count(&db, "sickness_changes"), 3);
}
#[test]
fn direct_date_change_is_detected_by_first_send_freshness() {
    let (dir, db) = fixture();
    let i = publish(&dir, &db, 10, "candidate");
    db.execute("INSERT INTO personal_assistant_sickness_periods(personal_assistant_id,start_date,end_date) VALUES(1,'2026-09-03','2026-09-04')",[]).unwrap();
    assert!(send(&db, &i).unwrap_err().to_string().contains("changed"));
    assert_eq!(count(&db, "timesheet_delivery_attempts"), 0);
}
#[test]
fn duplicates_are_rejected_but_overlaps_are_preserved() {
    let (_dir, db) = fixture();
    let p = row("2026-09-01", "2026-09-06");
    change(&db, 10, None, Some(&p), Scope::Editable).unwrap();
    assert!(change(&db, 10, None, Some(&p), Scope::Editable)
        .unwrap_err()
        .to_string()
        .contains("already exists"));
    change(
        &db,
        10,
        None,
        Some(&row("2026-09-05", "2026-09-08")),
        Scope::Editable,
    )
    .unwrap();
    assert_eq!(evidence(&db, 10).unwrap().periods.len(), 2);
    assert_eq!(count(&db, "sickness_changes"), 2);
}
#[test]
fn stale_editor_and_stale_payroll_review_cannot_overwrite() {
    let (dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-03")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    let mut a = p.clone();
    a.end_date = "2026-09-04".into();
    let reviewed = review(&db, 10, Some(&p), Some(&a)).unwrap();
    let second = crate::database::open(dir.path().join("payroll.sqlite")).unwrap();
    change(&second, 10, Some(&p), Some(&a), Scope::Editable).unwrap();
    assert!(mutate(
        &db,
        10,
        Some(&p),
        None,
        Scope::Editable,
        "stale",
        &reviewed.signature
    )
    .is_err());
    let plan = review(&db, 10, Some(&a), None).unwrap();
    protect(&dir, &db, false);
    assert!(mutate(
        &db,
        10,
        Some(&a),
        None,
        Scope::AuthorisedCorrection,
        "stale",
        &plan.signature
    )
    .is_err());
}
#[test]
fn submitted_and_settled_cross_cycle_records_require_authorisation() {
    for settled in [false, true] {
        let (dir, db) = fixture();
        let p = change(
            &db,
            10,
            None,
            Some(&row("2026-09-25", "2026-10-02")),
            Scope::Editable,
        )
        .unwrap()
        .unwrap();
        let original = protect(&dir, &db, settled);
        let bytes = delivery::bytes(&db, &delivery::capture(&db, 10).unwrap()).unwrap();
        let mut revised = p.clone();
        revised.end_date = "2026-10-05".into();
        assert!(change(&db, 11, Some(&p), Some(&revised), Scope::Editable).is_err());
        assert!(change(&db, 11, Some(&p), None, Scope::Editable).is_err());
        change(
            &db,
            11,
            Some(&p),
            Some(&revised),
            Scope::AuthorisedCorrection,
        )
        .unwrap();
        assert_eq!(count(&db, "payroll_submissions"), 1);
        assert_eq!(count(&db, "payroll_corrections"), 0);
        assert_eq!(
            delivery::bytes(&db, &delivery::capture(&db, 10).unwrap()).unwrap(),
            bytes
        );
        let corrected = publish(&dir, &db, 10, "corrected");
        assert!(!corrected.resend);
        assert_ne!(corrected.document, original.document);
        assert!(send(&db, &corrected).unwrap());
        assert_eq!(count(&db, "payroll_submissions"), 2);
        let original_bytes: Vec<u8> = db
            .query_row(
                "SELECT pdf_bytes FROM timesheet_documents WHERE id=?1",
                [original.document],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(original_bytes, bytes);
        let original_disk = std::fs::read(&original.path).unwrap();
        assert_eq!(original_disk, bytes);
        if settled {
            assert_eq!(
                lifecycle::stage(&db, &record(&db, 10).unwrap()).unwrap(),
                lifecycle::Stage::Settled
            );
            assert_eq!(
                db.query_row::<String, _, _>(
                    "SELECT sent_at FROM payroll_timesheet_email_status WHERE email_type='payslip'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
                "settled-original"
            );
        }
        let history = audit(&db, 1).unwrap();
        assert!(history
            .iter()
            .any(|s| s.contains("reconciliation decision") && s.contains("Corrected dates")));
        assert_eq!(count(&db, "sickness_submission_evidence"), 2);
        assert_eq!(count(&db, "sickness_attempt_evidence"), 2);
        let resend = delivery::capture(&db, 10).unwrap();
        assert!(resend.resend);
        send(&db, &resend).unwrap();
        assert_eq!(count(&db, "payroll_submissions"), 2);
        assert_eq!(count(&db, "sickness_attempt_evidence"), 3);
    }
}
#[test]
fn editable_only_scope_preserves_protected_full_dates_and_resend_bytes() {
    let (dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-25", "2026-10-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    protect(&dir, &db, true);
    let old = evidence(&db, 10).unwrap();
    let i = delivery::capture(&db, 10).unwrap();
    let bytes = delivery::bytes(&db, &i).unwrap();
    let mut revised = p.clone();
    revised.end_date = "2026-10-05".into();
    change(&db, 11, Some(&p), Some(&revised), Scope::EditableOnly).unwrap();
    assert_eq!(evidence(&db, 10).unwrap(), old);
    assert_eq!(evidence(&db, 11).unwrap().periods[0].end_date, "2026-10-05");
    assert!(send(&db, &i).unwrap());
    assert_eq!(delivery::bytes(&db, &i).unwrap(), bytes);
    assert_eq!(count(&db, "sickness_corrections"), 0);
}
#[test]
fn uncertain_delivery_blocks_all_scopes_until_audited_resolution() {
    let (dir, mut db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-25", "2026-10-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    let i = publish(&dir, &db, 10, "uncertain");
    let bytes = delivery::bytes(&db, &i).unwrap();
    let attempt =
        match delivery::claim(&db, &i, "Payroll", "<uncertain@test.local>", &bytes).unwrap() {
            delivery::Claim::Claimed(id) => id,
            _ => panic!(),
        };
    for scope in [
        Scope::Editable,
        Scope::EditableOnly,
        Scope::AuthorisedCorrection,
    ] {
        assert!(change(&db, 11, Some(&p), None, scope).is_err());
    }
    let reopened = crate::database::open(dir.path().join("payroll.sqlite")).unwrap();
    assert!(change(&reopened, 11, Some(&p), None, Scope::EditableOnly).is_err());
    delivery::review(
        &mut db,
        attempt,
        "confirmed_not_sent",
        "SMTP log proves no acceptance",
    )
    .unwrap();
    change(&db, 11, Some(&p), None, Scope::Editable).unwrap();
    assert_eq!(count(&db, "timesheet_delivery_reviews"), 1);
}
#[test]
fn dispatch_publication_and_restore_os_lock_blocks_mutation_across_instances() {
    let (dir, db) = fixture();
    let guard = delivery::production_lock(&db).unwrap();
    let other = crate::database::open(dir.path().join("payroll.sqlite")).unwrap();
    let result = change(
        &other,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("active in another instance"));
    assert_eq!(count(&db, "sickness_changes"), 0);
    drop(guard);
    change(
        &other,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap();
}
#[test]
fn publication_callback_cannot_mutate_its_sickness_inputs() {
    let (dir, db) = fixture();
    let repo = PayrollWorkedItemRepository::new(
        crate::database::open(dir.path().join("payroll.sqlite")).unwrap(),
    );
    let path = dir.path().join("during-generation.pdf");
    publish_candidate(
        &repo,
        CandidatePublication {
            payroll_timesheet_id: 10,
            items: &[],
            final_pdf_path: &path,
            generated_at: "test",
            previous_cycle_minutes: 0,
            week_ids: &[100, 101, 102, 103],
            week_totals_minutes: &[0; 4],
        },
        |p| {
            assert!(change(
                &db,
                10,
                None,
                Some(&row("2026-09-01", "2026-09-02")),
                Scope::Editable
            )
            .is_err());
            std::fs::write(p, b"immutable generated input")?;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count(&db, "sickness_changes"), 0);
}
#[test]
fn failed_mutation_rolls_back_dates_audit_and_invalidation() {
    let (dir, db) = fixture();
    let i = publish(&dir, &db, 10, "rollback");
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    db.execute_batch("CREATE TRIGGER reject_sickness_audit BEFORE INSERT ON sickness_changes BEGIN SELECT RAISE(ABORT,'audit unavailable'); END;").unwrap();
    assert!(change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable
    )
    .is_err());
    db.execute_batch("DROP TRIGGER reject_sickness_audit;")
        .unwrap();
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    assert_eq!(delivery::capture(&db, 10).unwrap().document, i.document);
}
#[test]
fn legacy_hours_warn_without_inventing_dates_and_notes_use_dynamic_stars() {
    let (_dir, db) = fixture();
    db.execute(
        "UPDATE payroll_timesheet_weeks SET sick_leave_hours=8 WHERE id=100",
        [],
    )
    .unwrap();
    assert!(legacy_warning(&db, 10).unwrap());
    change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-01")),
        Scope::Editable,
    )
    .unwrap();
    assert!(!legacy_warning(&db, 10).unwrap());
    assert_eq!(
        db.query_row::<f64, _, _>(
            "SELECT sick_leave_hours FROM payroll_timesheet_weeks WHERE id=100",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        8.0
    );
    assert_eq!(
        notes_with_correction("", Some("Sickness Information Correction")),
        "* Sickness Information Correction"
    );
    assert_eq!(
        notes_with_correction("Existing note", Some("Correction")),
        "Existing note\n** Correction"
    );
    assert_eq!(
        notes_with_correction("* One\n** Two", Some("Correction")),
        "* One\n** Two\n*** Correction"
    );
    assert_eq!(notes_with_correction("", None), "");
    assert_eq!(
        notes_with_correction("  Original notes\n ", None),
        "  Original notes\n "
    );
    assert_eq!(notes_with_correction("", Some("  ")), "");
}
#[test]
fn relocated_pdf_retains_sickness_snapshot_identity() {
    let (dir, db) = fixture();
    change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap();
    let i = protect(&dir, &db, false);
    let before: String = db
        .query_row(
            "SELECT evidence FROM sickness_document_evidence WHERE document_id=?1",
            [i.document],
            |r| r.get(0),
        )
        .unwrap();
    let archive = dir.path().join("Archived");
    std::fs::create_dir(&archive).unwrap();
    let moved = archive.join("original.pdf");
    std::fs::rename(&i.path, &moved).unwrap();
    db.execute(
        "UPDATE payroll_timesheet_snapshot_states SET pdf_path=?1 WHERE payroll_timesheet_id=10",
        [moved.to_str()],
    )
    .unwrap();
    let captured = delivery::capture(&db, 10).unwrap();
    assert_eq!(captured.document, i.document);
    assert!(send(&db, &captured).unwrap());
    assert_eq!(
        db.query_row::<String, _, _>(
            "SELECT evidence FROM sickness_document_evidence WHERE document_id=?1",
            [i.document],
            |r| r.get(0)
        )
        .unwrap(),
        before
    );
}

#[test]
fn backup_restore_preserves_all_sickness_evidence_and_reviews_destructive_rollback() {
    use crate::backup_service::{BackupService, RestoreAuthorisation};
    let (dir, db) = fixture();
    let database = dir.path().join("payroll.sqlite");
    let config = dir.path().join("config.toml");
    let backups = dir.path().join("backups");
    std::fs::write(
        &config,
        "[folders]\npdf_output='/never-access-production-business-files'\n",
    )
    .unwrap();
    let original = BackupService::create_verified(&database, &config, &backups).unwrap();
    change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap();
    protect(&dir, &db, true);
    let period = evidence(&db, 10).unwrap().periods[0].clone();
    let mut revised = period.clone();
    revised.end_date = "2026-09-03".into();
    change(
        &db,
        10,
        Some(&period),
        Some(&revised),
        Scope::AuthorisedCorrection,
    )
    .unwrap();
    let corrected = publish(&dir, &db, 10, "corrected");
    send(&db, &corrected).unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    let plan = BackupService::preview_restore(&original, &database, &config, &backups).unwrap();
    for table in [
        "sickness_changes",
        "sickness_corrections",
        "sickness_document_evidence",
        "sickness_submission_evidence",
        "sickness_attempt_evidence",
    ] {
        assert!(
            plan.replaced_rows.iter().any(|(t, n)| t == table && *n > 0),
            "{table}"
        );
    }
    let approval = RestoreAuthorisation {
        plan,
        acknowledged: false,
        reason: "".into(),
    };
    assert!(BackupService::restore(&original, &database, &config, &backups, &approval).is_err());
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    let plan = BackupService::preview_restore(&original, &database, &config, &backups).unwrap();
    let result=BackupService::restore(&original,&database,&config,&backups,&RestoreAuthorisation{plan,acknowledged:true,reason:"Reviewed all lost sickness and accepted email evidence; deliberate isolated rollback".into()}).unwrap();
    let recovery = crate::database::open(result.safety_backup.join("database.sqlite")).unwrap();
    assert_eq!(
        crate::database_recovery::fingerprint(&recovery).unwrap(),
        before
    );
    assert_eq!(count(&recovery, "sickness_submission_evidence"), 2);
    assert_eq!(count(&db, "sickness_changes"), 0);
    assert!(std::path::Path::new(&corrected.path).is_file());
}
#[test]
fn version36_connection_cannot_mutate_version37_records() {
    let (dir, db) = fixture();
    let old = crate::database::open(dir.path().join("payroll.sqlite")).unwrap();
    old.create_scalar_function(
        "dpt_schema_version",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |_| Ok(36i64),
    )
    .unwrap();
    assert!(old.execute("INSERT INTO personal_assistant_sickness_periods(personal_assistant_id,start_date,end_date) VALUES(1,'2026-09-01','2026-09-02')",[]).is_err());
    assert!(old
        .execute("UPDATE schema_version SET version=36", [])
        .is_err());
    assert_eq!(count(&db, "personal_assistant_sickness_periods"), 0);
}
#[test]
fn legacy_candidate_without_sickness_capture_requires_regeneration_but_resend_is_retained() {
    let (dir, db) = fixture();
    let i = publish(&dir, &db, 10, "legacy");
    // Simulate a genuine schema-36 PDF: no sickness capture or historical dates invented.
    db.execute_batch("DROP TRIGGER retain_sickness_document_evidence_DELETE; DELETE FROM sickness_document_evidence;").unwrap();
    assert!(send(&db, &i)
        .unwrap_err()
        .to_string()
        .contains("predates structured sickness"));
    assert_eq!(count(&db, "timesheet_delivery_attempts"), 0);
    let fresh = publish(&dir, &db, 10, "regenerated");
    send(&db, &fresh).unwrap();
    assert_eq!(count(&db, "sickness_submission_evidence"), 1);
}
#[test]
fn reviewed_change_rejects_aba_edits_and_audit_evidence_is_immutable() {
    let (_dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    let planned = review(&db, 10, Some(&p), None).unwrap();
    let mut b = p.clone();
    b.end_date = "2026-09-03".into();
    let b = change(&db, 10, Some(&p), Some(&b), Scope::Editable)
        .unwrap()
        .unwrap();
    change(&db, 10, Some(&b), Some(&p), Scope::Editable).unwrap();
    assert!(mutate(
        &db,
        10,
        Some(&p),
        None,
        Scope::Editable,
        "stale ABA",
        &planned.signature
    )
    .is_err());
    assert!(db.execute("DELETE FROM sickness_changes", []).is_err());
    assert!(db
        .execute("UPDATE sickness_changes SET reason='overwrite'", [])
        .is_err());
}

#[test]
fn editable_portion_deletion_can_be_followed_by_authorised_protected_correction() {
    let (dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-25", "2026-10-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    protect(&dir, &db, true);
    change(&db, 11, Some(&p), None, Scope::EditableOnly).unwrap();
    assert!(evidence(&db, 11).unwrap().periods.is_empty());
    assert_eq!(evidence(&db, 10).unwrap().periods, vec![p.clone()]);
    let mut corrected = p.clone();
    corrected.end_date = "2026-09-26".into();
    change(
        &db,
        10,
        Some(&p),
        Some(&corrected),
        Scope::AuthorisedCorrection,
    )
    .unwrap();
    assert_eq!(evidence(&db, 10).unwrap().periods[0].end_date, "2026-09-26");
    assert!(evidence(&db, 11).unwrap().periods.is_empty());
    let i = publish(&dir, &db, 10, "historical scoped correction");
    send(&db, &i).unwrap();
    assert_eq!(count(&db, "payroll_submissions"), 2);
}
#[test]
fn deleted_sickness_period_ids_are_never_reused() {
    let (_dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    change(&db, 10, Some(&p), None, Scope::Editable).unwrap();
    let next = change(
        &db,
        10,
        None,
        Some(&row("2026-09-03", "2026-09-04")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    assert!(next.id > p.id);
}

fn legacy36(db: &Connection) {
    crate::database::tests::remove_schema_38_fixture(db);
    let triggers = db
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'dpt37_%'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for t in triggers {
        db.execute_batch(&format!("DROP TRIGGER {t}")).unwrap();
    }
    db.execute_batch("DROP TABLE sickness_attempt_evidence; DROP TABLE sickness_submission_evidence; DROP TABLE sickness_document_evidence; DROP TABLE sickness_corrections; DROP TABLE sickness_cycle_overrides; DROP TABLE sickness_changes; UPDATE schema_version SET version=36;").unwrap();
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for t in tables {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            db.execute_batch(&format!("CREATE TRIGGER dpt36_{t}_{op} BEFORE {op} ON {t} BEGIN SELECT CASE WHEN dpt_schema_version()<>36 THEN RAISE(ABORT,'Incompatible application schema') END; END;")).unwrap();
        }
    }
}
#[test]
fn verified_version36_upgrade_preserves_dates_submissions_attempts_and_original_backup() {
    let (dir, db) = fixture();
    change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap();
    let original = protect(&dir, &db, true);
    legacy36(&db);
    let config = dir.path().join("config.toml");
    std::fs::write(
        config,
        "[folders]\npdf_output='/never-access-business-files'\n",
    )
    .unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    crate::database::initialise_database(&dir.path().join("payroll.sqlite")).unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap(),
        crate::database::CURRENT_SCHEMA_VERSION
    );
    assert_eq!(count(&db, "personal_assistant_sickness_periods"), 1);
    assert_eq!(count(&db, "payroll_submissions"), 1);
    assert_eq!(count(&db, "timesheet_delivery_attempts"), 1);
    assert_eq!(count(&db, "sickness_document_evidence"), 0);
    assert_eq!(count(&db, "sickness_changes"), 0);
    let backups =
        crate::backup_service::BackupService::discover(&dir.path().join("backups")).unwrap();
    assert_eq!(backups.len(), 1);
    let copy = crate::database::open(backups[0].path.join("database.sqlite")).unwrap();
    assert_eq!(
        crate::database_recovery::fingerprint(&copy).unwrap(),
        before
    );
    let resend = delivery::capture(&db, 10).unwrap();
    assert_eq!(resend.document, original.document);
    assert!(resend.resend);
    send(&db, &resend).unwrap();
}
#[test]
fn version37_backup_failure_leaves_legacy_database_untouched() {
    let (dir, db) = fixture();
    legacy36(&db);
    std::fs::write(dir.path().join("backups"), "blocked backup destination").unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    assert!(crate::database::initialise_database(&dir.path().join("payroll.sqlite")).is_err());
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
}
#[test]
fn version37_migration_failure_rolls_back_identity_rebuild_and_all_new_tables() {
    let (_dir, db) = fixture();
    legacy36(&db);
    // Use the old connection's schema function only to install a failure fixture.
    db.create_scalar_function(
        "dpt_schema_version",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |_| Ok(36i64),
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER reject37 BEFORE UPDATE ON schema_version WHEN NEW.version=37 BEGIN SELECT RAISE(ABORT,'version37 failure'); END;").unwrap();
    crate::database::register_connection(&db).unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    assert!(migrate(&db).is_err());
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    assert!(db.prepare("SELECT * FROM sickness_changes").is_err());
}

#[test]
fn legacy_submitted_identical_regeneration_does_not_become_an_unsent_document() {
    let (dir, db) = fixture();
    let original = protect(&dir, &db, false);
    legacy36(&db);
    db.create_scalar_function(
        "dpt_schema_version",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |_| Ok(36i64),
    )
    .unwrap();
    db.execute(
        "UPDATE payroll_candidate_checks SET evidence_signature='legacy-no-sickness-signature'",
        [],
    )
    .unwrap();
    crate::database::register_connection(&db).unwrap();
    crate::database::initialise_database(&dir.path().join("payroll.sqlite")).unwrap();
    let regenerated = publish(&dir, &db, 10, "original");
    assert_eq!(regenerated.document, original.document);
    assert!(regenerated.resend);
    assert_eq!(count(&db, "sickness_document_evidence"), 0);
    assert_eq!(count(&db, "payroll_submissions"), 1);
}

#[test]
fn insertion_extension_and_movement_cannot_enter_protected_cycles_without_review() {
    let (dir, db) = fixture();
    protect(&dir, &db, true);
    assert!(change(
        &db,
        11,
        None,
        Some(&row("2026-09-25", "2026-10-02")),
        Scope::Editable
    )
    .is_err());
    let p = change(
        &db,
        11,
        None,
        Some(&row("2026-09-29", "2026-10-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    let mut extended = p.clone();
    extended.start_date = "2026-09-25".into();
    assert!(change(&db, 11, Some(&p), Some(&extended), Scope::Editable).is_err());
    let mut moved = p.clone();
    moved.start_date = "2026-09-02".into();
    moved.end_date = "2026-09-03".into();
    assert!(change(&db, 11, Some(&p), Some(&moved), Scope::Editable).is_err());
    change(
        &db,
        11,
        Some(&p),
        Some(&extended),
        Scope::AuthorisedCorrection,
    )
    .unwrap();
    assert_eq!(evidence(&db, 10).unwrap().periods.len(), 1);
}
#[test]
fn changed_payroll_totals_refuse_sickness_only_delivery_without_smtp_attempt() {
    let (dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    protect(&dir, &db, true);
    let mut revised = p.clone();
    revised.end_date = "2026-09-03".into();
    change(
        &db,
        10,
        Some(&p),
        Some(&revised),
        Scope::AuthorisedCorrection,
    )
    .unwrap();
    let corrected = publish(&dir, &db, 10, "corrected");
    db.execute(
        "UPDATE payroll_timesheet_weeks SET worked_hours=99 WHERE id=100",
        [],
    )
    .unwrap();
    assert!(send(&db, &corrected)
        .unwrap_err()
        .to_string()
        .contains("payroll amounts"));
    assert_eq!(count(&db, "timesheet_delivery_attempts"), 1);
    assert_eq!(count(&db, "payroll_submissions"), 1);
}

#[test]
fn failed_settled_correction_publication_remains_retryable_without_reopening_settlement() {
    let (dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    protect(&dir, &db, true);
    let mut revised = p.clone();
    revised.end_date = "2026-09-03".into();
    change(
        &db,
        10,
        Some(&p),
        Some(&revised),
        Scope::AuthorisedCorrection,
    )
    .unwrap();
    let repo = PayrollWorkedItemRepository::new(
        crate::database::open(dir.path().join("payroll.sqlite")).unwrap(),
    );
    let blocked = dir.path().join("blocked.pdf");
    std::fs::create_dir(&blocked).unwrap();
    assert!(publish_candidate(
        &repo,
        CandidatePublication {
            payroll_timesheet_id: 10,
            items: &[],
            final_pdf_path: &blocked,
            generated_at: "test",
            previous_cycle_minutes: 0,
            week_ids: &[100, 101, 102, 103],
            week_totals_minutes: &[0; 4]
        },
        |p| {
            std::fs::write(p, b"corrected pending PDF")?;
            Ok(())
        }
    )
    .is_err());
    assert!(correction_open(&db, 10).unwrap());
    assert_eq!(
        lifecycle::stage(&db, &record(&db, 10).unwrap()).unwrap(),
        lifecycle::Stage::Settled
    );
    let retry = publish(&dir, &db, 10, "retry correction");
    send(&db, &retry).unwrap();
    assert!(!correction_open(&db, 10).unwrap());
}

fn retained_fixture() -> (tempfile::TempDir, Connection, SicknessPeriod) {
    let (dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-25", "2026-10-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    protect(&dir, &db, true);
    change(&db, 11, Some(&p), None, Scope::EditableOnly).unwrap();
    (dir, db, p)
}

#[test]
fn retained_transfers_persist_dates_and_invalidate_every_destination_candidate() {
    for (a, b) in [
        ("2026-09-25", "2026-10-05"),
        ("2026-10-01", "2026-10-03"),
        ("2026-11-02", "2026-11-03"),
    ] {
        let (dir, db, p) = retained_fixture();
        let destination = publish(&dir, &db, 11, "destination");
        let original = delivery::capture(&db, 10).unwrap();
        let bytes = delivery::bytes(&db, &original).unwrap();
        let mut n = p.clone();
        n.start_date = a.into();
        n.end_date = b.into();
        let plan = review(&db, 10, Some(&p), Some(&n)).unwrap();
        if a != "2026-11-02" {
            assert!(plan.description.contains("period 8"));
        }
        let saved = mutate(
            &db,
            10,
            Some(&p),
            Some(&n),
            Scope::AuthorisedCorrection,
            "Reviewed transfer",
            &plan.signature,
        )
        .unwrap()
        .unwrap();
        assert_ne!(saved.id, p.id);
        assert_eq!(stored(&db, saved.id).unwrap(), Some(saved.clone()));
        for id in [10, 11] {
            let (start, end) = bounds(&db, id).unwrap();
            let overlap = a <= crate::date_utils::iso(end).as_str()
                && b >= crate::date_utils::iso(start).as_str();
            let e = evidence(&db, id).unwrap();
            assert_eq!(e.periods.contains(&saved), overlap);
            let projected = projection(
                &e,
                &std::array::from_fn(|i| start + Duration::days(i as i64 * 7)),
            )
            .unwrap();
            assert_eq!(projected.iter().any(|w| !w.is_empty()), overlap);
        }
        if a != "2026-11-02" {
            assert!(send(&db, &destination).is_err());
        }
        assert_eq!(delivery::bytes(&db, &original).unwrap(), bytes);
        let audit = audit(&db, 1).unwrap().join("\n");
        assert!(audit.contains(a));
        assert!(audit.contains("Reviewed transfer"));
    }
}

#[test]
fn retained_destination_protection_uncertainty_duplicates_and_stale_review_are_atomic() {
    for stage in ["submitted", "settled", "uncertain", "duplicate", "stale"] {
        let (dir, db, p) = retained_fixture();
        let destination = publish(&dir, &db, 11, "destination");
        let mut n = p.clone();
        n.start_date = "2026-10-01".into();
        n.end_date = "2026-10-03".into();
        if stage == "submitted" || stage == "settled" {
            send(&db, &destination).unwrap();
        }
        if stage == "settled" {
            db.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',8,'payslip','original settlement')",[]).unwrap();
        }
        if stage == "uncertain" {
            let bytes = delivery::bytes(&db, &destination).unwrap();
            delivery::claim(&db, &destination, "Payroll", "<uncertain@fixture>", &bytes).unwrap();
        }
        if stage == "duplicate" {
            change(
                &db,
                11,
                None,
                Some(&row("2026-10-01", "2026-10-03")),
                Scope::Editable,
            )
            .unwrap();
        }
        let plan = review(&db, 10, Some(&p), Some(&n)).unwrap();
        assert!(plan.description.contains("period 8"));
        if stage == "stale" {
            change(
                &db,
                11,
                None,
                Some(&row("2026-10-08", "2026-10-09")),
                Scope::Editable,
            )
            .unwrap();
        }
        let before = crate::database_recovery::fingerprint(&db).unwrap();
        let outcome = mutate(
            &db,
            10,
            Some(&p),
            Some(&n),
            Scope::AuthorisedCorrection,
            "Transfer review",
            &plan.signature,
        );
        if matches!(stage, "uncertain" | "duplicate" | "stale") {
            assert!(outcome.is_err());
            assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
        } else {
            let saved = outcome.unwrap().unwrap();
            assert!(evidence(&db, 11).unwrap().periods.contains(&saved));
            assert_eq!(count(&db, "sickness_corrections"), 2);
            assert_eq!(
                lifecycle::stage(&db, &record(&db, 11).unwrap()).unwrap(),
                if stage == "settled" {
                    lifecycle::Stage::Settled
                } else {
                    lifecycle::Stage::Submitted
                }
            );
        }
    }
}

#[test]
fn retained_local_shortening_and_deletion_preserve_other_views_and_conflicting_transfer_is_refused()
{
    for delete in [false, true] {
        let (_dir, db, p) = retained_fixture();
        let mut n = p.clone();
        n.end_date = "2026-09-26".into();
        change(
            &db,
            10,
            Some(&p),
            (!delete).then_some(&n),
            Scope::AuthorisedCorrection,
        )
        .unwrap();
        assert!(evidence(&db, 11).unwrap().periods.is_empty());
        assert_eq!(
            evidence(&db, 10).unwrap().periods,
            if delete { vec![] } else { vec![n] }
        );
        assert!(stored(&db, p.id).unwrap().is_none());
    }
    let (dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-25", "2026-10-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    protect(&dir, &db, true);
    let mut canonical = p.clone();
    canonical.end_date = "2026-10-04".into();
    change(&db, 11, Some(&p), Some(&canonical), Scope::EditableOnly).unwrap();
    let mut moved = p.clone();
    moved.start_date = "2026-10-10".into();
    moved.end_date = "2026-10-11".into();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    assert!(review(&db, 10, Some(&p), Some(&moved))
        .unwrap_err()
        .to_string()
        .contains("conflict"));
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
}

#[test]
fn historical_financial_payload_changes_are_refused_even_when_worked_totals_match() {
    for edit in ["UPDATE payroll_timesheet_worked_item_snapshots SET worked_minutes=91 WHERE payroll_timesheet_id=10",
        "INSERT INTO payroll_correction_applications VALUES(10,99,15)",
        "UPDATE payroll_timesheets SET previous_cycle_hours=2 WHERE id=10",
        "UPDATE payroll_timesheet_weeks SET annual_leave_hours=2 WHERE payroll_timesheet_id=10",
        "UPDATE payroll_timesheet_weeks SET week_commencing='2026-09-02' WHERE id=100"] {
        let (dir,db)=fixture();
        let p=change(&db,10,None,Some(&row("2026-09-01","2026-09-02")),Scope::Editable).unwrap().unwrap();
        let i=protect(&dir,&db,true);
        // Add matching original/current membership, with unchanged weekly totals.
        let sid=lifecycle::latest(&db,10).unwrap().unwrap();
        db.execute("INSERT INTO payroll_timesheet_worked_item_snapshots(payroll_timesheet_id,week_number,source_type,worked_minutes,captured_at,pay_rate_id,pay_rate_effective_date,total_hourly_rate) VALUES(10,1,'manual_adjustment',90,'original',1,'2026-01-01',12)",[]).unwrap();
        db.execute("INSERT INTO payroll_submission_items SELECT ?1,s.* FROM payroll_timesheet_worked_item_snapshots s WHERE payroll_timesheet_id=10",[sid]).unwrap();
        let mut n=p.clone();n.end_date="2026-09-03".into();change(&db,10,Some(&p),Some(&n),Scope::AuthorisedCorrection).unwrap();
        let corrected=publish(&dir,&db,10,"corrected");db.execute_batch(edit).unwrap();
        assert!(send(&db,&corrected).is_err(),"{edit}");assert_eq!(count(&db,"timesheet_delivery_attempts"),1);
        assert!(delivery::bytes(&db,&delivery::capture(&db,10).unwrap()).is_ok());
        assert_ne!(corrected.document,i.document);
    }
}

fn genuine_legacy(version: i64) -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::database::open(dir.path().join("payroll.sqlite")).unwrap();
    crate::database::create_legacy_schema(&db, version).unwrap();
    let ddl: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='personal_assistant_sickness_periods'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!ddl.contains("AUTOINCREMENT"));
    db.execute_batch("INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Legacy','Assistant');
        INSERT INTO employers(name) VALUES('Legacy Employer');
        INSERT INTO payroll_schedules(payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES('2026/27',7,'31/08/2026','x','x','x');
        INSERT INTO payroll_timesheets(id,personal_assistant_id,payroll_year,cycle_number,created_at,updated_at) VALUES(10,1,'2026/27',7,'original','original');
        INSERT INTO personal_assistant_sickness_periods(id,personal_assistant_id,start_date,end_date) VALUES(42,1,'2026-09-01','2026-09-03');
        INSERT INTO payroll_submissions(id,payroll_timesheet_id,submitted_at,pdf_path,pdf_sha256,pdf_bytes,payroll_department_notes) VALUES(100,10,'original acceptance','legacy.pdf','legacy digest',X'010203','Legacy note');
        INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',7,'payslip','original settlement');").unwrap();
    for w in 0..4 {
        db.execute("INSERT INTO payroll_timesheet_weeks(payroll_timesheet_id,week_number,week_commencing,worked_hours,annual_leave_hours,travel_miles) VALUES(10,?1,?2,12.5,2,3)",params![w+1,crate::date_utils::iso(crate::date_utils::parse_legacy("2026-08-31").unwrap()+Duration::days(w*7))]).unwrap();
    }
    db.execute("INSERT INTO payroll_submission_weeks SELECT 100,w.* FROM payroll_timesheet_weeks w WHERE payroll_timesheet_id=10",[]).unwrap();
    if version == 36 {
        db.execute_batch("INSERT INTO timesheet_documents(id,payroll_timesheet_id,pdf_path,pdf_sha256,pdf_bytes,generated_at,legacy,legacy_submission_id) VALUES(50,10,'legacy.pdf','legacy digest',X'010203','original',1,100);
            UPDATE payroll_submissions SET document_id=50 WHERE id=100;
            INSERT INTO timesheet_delivery_attempts(intent_id,payroll_timesheet_id,document_id,submission_id,classification,recipients,pdf_path,message_id,started_at,completed_at,outcome,detail) VALUES('legacy intent',10,50,100,'first_send','Original Payroll','legacy.pdf','<legacy@fixture>','original','accepted timestamp','accepted','Original acceptance evidence');").unwrap();
    }
    std::fs::write(
        dir.path().join("config.toml"),
        "[folders]\npdf_output='/never-access-business-files'\n",
    )
    .unwrap();
    (dir, db)
}

#[test]
fn genuinely_populated_schema35_and36_upgrade_and_restore_preserve_original_data() {
    use crate::backup_service::{BackupService, RestoreAuthorisation};
    for version in [35, 36] {
        let (dir, db) = genuine_legacy(version);
        let path = dir.path().join("payroll.sqlite");
        let config = dir.path().join("config.toml");
        let backups = dir.path().join("backups");
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        let before = crate::database_recovery::fingerprint(&db).unwrap();
        let selected = BackupService::create_verified(&path, &config, &backups).unwrap();
        crate::database::initialise_database(&path).unwrap();
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
                .unwrap(),
            crate::database::CURRENT_SCHEMA_VERSION
        );
        assert_eq!(stored(&db, 42).unwrap().unwrap().end_date, "2026-09-03");
        assert_eq!(count(&db, "payroll_submissions"), 1);
        assert_eq!(count(&db, "sickness_document_evidence"), 0);
        assert_eq!(
            count(&db, "timesheet_delivery_attempts"),
            i64::from(version == 36)
        );
        assert_eq!(
            db.query_row::<String, _, _>(
                "SELECT sent_at FROM payroll_timesheet_email_status WHERE email_type='payslip'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "original settlement"
        );
        let source = crate::database::open(selected.join("database.sqlite")).unwrap();
        assert_eq!(
            crate::database_recovery::fingerprint(&source).unwrap(),
            before
        );
        db.execute(
            "INSERT INTO employers(name) VALUES('Newer preserved recovery evidence')",
            [],
        )
        .unwrap();
        let live = crate::database_recovery::fingerprint(&db).unwrap();
        let plan = BackupService::preview_restore(&selected, &path, &config, &backups).unwrap();
        let restored = BackupService::restore(
            &selected,
            &path,
            &config,
            &backups,
            &RestoreAuthorisation {
                plan,
                acknowledged: true,
                reason: "Explicit isolated regression rollback; preserve newer recovery evidence"
                    .into(),
            },
        )
        .unwrap();
        assert_eq!(
            crate::database_recovery::fingerprint(
                &crate::database::open(restored.safety_backup.join("database.sqlite")).unwrap()
            )
            .unwrap(),
            live
        );
        assert_eq!(
            crate::database_recovery::fingerprint(&source).unwrap(),
            before
        );
        assert_eq!(stored(&db, 42).unwrap().unwrap().start_date, "2026-09-01");
    }
}

#[test]
fn genuine_schema35_failure_after_committed_schema36_keeps_original_and_verified_backup() {
    let (dir, db) = genuine_legacy(35);
    db.execute_batch("CREATE TRIGGER fail37 BEFORE UPDATE ON schema_version WHEN NEW.version=37 BEGIN SELECT RAISE(ABORT,'interrupted final migration'); END;").unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    assert!(crate::database::initialise_database(&dir.path().join("payroll.sqlite")).is_err());
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    assert!(db
        .prepare("SELECT * FROM timesheet_delivery_attempts")
        .is_err());
    let backups =
        crate::backup_service::BackupService::discover(&dir.path().join("backups")).unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        crate::database_recovery::fingerprint(
            &crate::database::open(backups[0].path.join("database.sqlite")).unwrap()
        )
        .unwrap(),
        before
    );
}

#[test]
fn nonzero_original_carry_worked_membership_and_corrections_survive_historical_delivery() {
    let (dir, db) = fixture();
    let p = change(
        &db,
        10,
        None,
        Some(&row("2026-09-01", "2026-09-02")),
        Scope::Editable,
    )
    .unwrap()
    .unwrap();
    let repo = PayrollWorkedItemRepository::new(
        crate::database::open(dir.path().join("payroll.sqlite")).unwrap(),
    );
    let items = vec![
        crate::payroll_worked_item_repository::WorkedItemSnapshot {
            week_number: 1,
            source_type: "legacy_previous_cycle_adjustment".into(),
            worked_minutes: 120,
            timesheet_id: None,
            direct_shift_id: None,
            source_evidence: None,
            work_date: None,
            pay_rate_id: None,
            pay_rate_effective_date: None,
            total_hourly_rate: None,
            reason: Some("Retained carry".into()),
        },
        crate::payroll_worked_item_repository::WorkedItemSnapshot {
            week_number: 1,
            source_type: "carry_correction".into(),
            worked_minutes: 30,
            timesheet_id: None,
            direct_shift_id: None,
            source_evidence: None,
            work_date: Some("2026-08-20".into()),
            pay_rate_id: None,
            pay_rate_effective_date: None,
            total_hourly_rate: None,
            reason: Some("Retained correction".into()),
        },
        crate::payroll_worked_item_repository::WorkedItemSnapshot {
            week_number: 1,
            source_type: "manual_adjustment".into(),
            worked_minutes: 90,
            timesheet_id: None,
            direct_shift_id: None,
            source_evidence: None,
            work_date: None,
            pay_rate_id: Some(1),
            pay_rate_effective_date: Some("2026-01-01".into()),
            total_hourly_rate: Some(12.0),
            reason: Some("Original work".into()),
        },
    ];
    db.execute(
        "INSERT INTO payroll_correction_applications VALUES(10,99,30)",
        [],
    )
    .unwrap();
    // Candidate verification requires a matching available monetary correction.
    db.execute("INSERT INTO payroll_corrections(id,personal_assistant_id,origin_payroll_timesheet_id,evidence_key,minutes,reason,created_at) VALUES(99,1,11,'nonzero retained correction',30,'Original correction','original')",[]).unwrap();
    let path = dir.path().join("original-nonzero.pdf");
    publish_candidate(
        &repo,
        CandidatePublication {
            payroll_timesheet_id: 10,
            items: &items,
            final_pdf_path: &path,
            generated_at: "original",
            previous_cycle_minutes: 150,
            week_ids: &[100, 101, 102, 103],
            week_totals_minutes: &[240, 0, 0, 0],
        },
        |p| {
            std::fs::write(p, b"original nonzero immutable bytes")?;
            Ok(())
        },
    )
    .unwrap();
    let original = delivery::capture(&db, 10).unwrap();
    send(&db, &original).unwrap();
    db.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',7,'payslip','original settlement')",[]).unwrap();
    let original_sid = lifecycle::latest(&db, 10).unwrap().unwrap();
    let original_payload = super::original_payload(&db, original_sid).unwrap();
    let mut n = p.clone();
    n.end_date = "2026-09-03".into();
    change(&db, 10, Some(&p), Some(&n), Scope::AuthorisedCorrection).unwrap();
    let retained = historical_hours(&db, 10).unwrap().unwrap();
    assert_eq!(retained.previous_cycle_minutes, 150);
    assert_eq!(retained.week_totals_minutes, [240, 0, 0, 0]);
    assert_eq!(retained.snapshot_items, items);
    let corrected = publish(&dir, &db, 10, "nonzero historical");
    send(&db, &corrected).unwrap();
    let latest = lifecycle::latest(&db, 10).unwrap().unwrap();
    assert_ne!(latest, original_sid);
    assert_eq!(lifecycle::items(&db, latest).unwrap(), items);
    assert_eq!(
        payload_rows(
            &db,
            "payroll_submission_corrections",
            "submission_id",
            latest
        )
        .unwrap(),
        payload_rows(
            &db,
            "payroll_submission_corrections",
            "submission_id",
            original_sid
        )
        .unwrap()
    );
    assert_eq!(
        super::original_payload(&db, original_sid).unwrap(),
        original_payload
    );
    assert_eq!(
        lifecycle::stage(&db, &record(&db, 10).unwrap()).unwrap(),
        lifecycle::Stage::Settled
    );
    assert_eq!(
        std::fs::read(path).unwrap(),
        b"original nonzero immutable bytes"
    );
    let resend = delivery::capture(&db, 10).unwrap();
    send(&db, &resend).unwrap();
    assert_eq!(count(&db, "payroll_submissions"), 2);
}

#[test]
fn retained_local_edits_and_deletion_cannot_ignore_uncertain_original_range() {
    let (dir, db, p) = retained_fixture();
    let i = publish(&dir, &db, 11, "uncertain destination");
    let bytes = delivery::bytes(&db, &i).unwrap();
    delivery::claim(&db, &i, "Payroll", "<retained-local@fixture>", &bytes).unwrap();
    let mut n = p.clone();
    n.end_date = "2026-09-26".into();
    for proposed in [Some(&n), None] {
        let before = crate::database_recovery::fingerprint(&db).unwrap();
        assert!(
            change(&db, 10, Some(&p), proposed, Scope::AuthorisedCorrection)
                .unwrap_err()
                .to_string()
                .contains("uncertain")
        );
        assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    }
}

#[test]
fn missing_preparation_with_historical_delivery_refuses_sickness_changes() {
    let (_dir, db) = fixture();
    db.execute(
        "DELETE FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=11",
        [],
    )
    .unwrap();
    db.execute("DELETE FROM payroll_timesheets WHERE id=11", [])
        .unwrap();
    db.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',8,'payslip','historic settlement')",[]).unwrap();
    let before = crate::database_recovery::fingerprint(&db).unwrap();
    assert!(
        review(&db, 10, None, Some(&row("2026-09-25", "2026-10-02")))
            .unwrap_err()
            .to_string()
            .contains("without a retained")
    );
    assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
}
