use super::*;
use crate::import_service::{ImportService, ImportSummary};
fn csv(dir: &tempfile::TempDir, name: &str, rows: &[&str]) {
    std::fs::create_dir_all(dir.path().join("import")).unwrap();
    std::fs::create_dir_all(dir.path().join("archive")).unwrap();
    std::fs::write(dir.path().join("import").join(name),format!("period\nClient Name,Start Time,End Time,Break Time,Worked Hours,Rate/h,Amount,Note\n{}\n",rows.join("\n"))).unwrap();
}
fn imported(dir: &tempfile::TempDir, app: &Application) -> ImportSummary {
    ImportService::new(
        &app.repository,
        dir.path().join("import"),
        &dir.path().join("archive"),
    )
    .run()
    .unwrap()
}
const ORIGINAL: &str =
    "Example PA,2 April 2026 at 09:00:00,2 April 2026 at 10:00:00,0h 00m,1h 00m,£12,£12,actual";
const CHANGED: &str =
    "Example PA,2 April 2026 at 09:00:00,2 April 2026 at 11:00:00,0h 00m,1h 45m,£12,£21,corrected";
const MOVED: &str =
    "Example PA,3 April 2026 at 09:00:00,3 April 2026 at 10:00:00,0h 00m,1h 00m,£12,£12,actual";
#[test]
fn pa_archive_and_reactivation_preserve_stage4_sources_reviews_and_archived_csv_bytes() {
    let (dir, app) = setup();
    let (record, weeks) = period(&app, 1, "2026-04-01", 1);
    csv(&dir, "hours.csv", &[ORIGINAL]);
    let result = imported(&dir, &app);
    let original_bytes = std::fs::read(&result.archived_paths[0]).unwrap();
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let db = open(&app).unwrap();
    let evidence = load(&app).unwrap();
    let group = preflight(&db, &evidence).unwrap().unresolved.remove(0);
    approve_group(&db, &evidence, &[(group.fingerprint, "direct:1".into())]).unwrap();
    let evidence_before = load(&app).unwrap();
    let signature_before = lifecycle::candidate_signature(&db, record.id).unwrap();
    let audit_count: i64 = db
        .query_row("SELECT COUNT(*) FROM shift_change_events", [], |r| r.get(0))
        .unwrap();
    let mut pa = app
        .personal_assistant_repository
        .get_all()
        .unwrap()
        .into_iter()
        .find(|p| p.id == 1)
        .unwrap();
    for status in ["Inactive", "Active"] {
        pa.employment_status = Some(status.into());
        crate::payroll_archive_service::apply(&app, &pa, true).unwrap();
        assert_eq!(load(&app).unwrap(), evidence_before);
        assert_eq!(
            lifecycle::candidate_signature(&db, record.id).unwrap(),
            signature_before
        );
        assert_eq!(
            reconciliation::calculate(&app, &record, &weeks, None)
                .unwrap()
                .week_totals_minutes,
            [60, 0, 0, 0]
        );
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT COUNT(*) FROM shift_change_events", [], |r| r.get(0))
                .unwrap(),
            audit_count
        );
        assert_eq!(
            std::fs::read(&result.archived_paths[0]).unwrap(),
            original_bytes
        );
    }
    assert_eq!(imported(&dir, &app).files_already_imported, 1);
}
#[test]
fn unchanged_same_path_and_renamed_exports_are_idempotent_and_archived() {
    let (dir, app) = setup();
    csv(&dir, "hours.csv", &[ORIGINAL]);
    assert_eq!(imported(&dir, &app).rows_imported, 1);
    assert_eq!(imported(&dir, &app).files_already_imported, 1);
    csv(&dir, "copy.csv", &[ORIGINAL]);
    let result = imported(&dir, &app);
    assert_eq!(result.rows_imported, 0);
    assert_eq!(result.rows_skipped, 1);
    assert_eq!(app.repository.get_all_raw().unwrap().len(), 1);
    assert_eq!(
        std::fs::read(result.archived_paths[0].clone()).unwrap(),
        std::fs::read(dir.path().join("import/copy.csv")).unwrap()
    );
}
#[test]
fn same_path_change_retains_original_and_requires_review_before_payroll() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    csv(&dir, "hours.csv", &[ORIGINAL]);
    let old = imported(&dir, &app);
    let bytes = std::fs::read(&old.archived_paths[0]).unwrap();
    csv(&dir, "hours.csv", &[CHANGED]);
    assert_eq!(imported(&dir, &app).rows_imported, 1);
    assert_eq!(app.repository.get_all_raw().unwrap().len(), 2);
    assert_eq!(std::fs::read(&old.archived_paths[0]).unwrap(), bytes);
    assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
    let db = open(&app).unwrap();
    let all = load(&app).unwrap();
    let pre = preflight(&db, &all).unwrap();
    assert_eq!(pre.unresolved.len(), 1);
    approve_group(
        &db,
        &all,
        &[(pre.unresolved[0].fingerprint.clone(), "imported:2".into())],
    )
    .unwrap();
    // Authoritative 105 minutes rounded using the established default Up rule.
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes[0],
        105
    );
    assert_eq!(app.repository.get_all_raw().unwrap()[0].worked_minutes, 60);
}
#[test]
fn moved_export_deferral_blocks_affected_payroll_but_not_other_pa() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let (other, ow) = period(&app, 1, "2026-04-01", 2);
    csv(&dir, "hours.csv", &[ORIGINAL]);
    imported(&dir, &app);
    csv(&dir, "hours.csv", &[MOVED]);
    imported(&dir, &app);
    let db = open(&app).unwrap();
    let all = load(&app).unwrap();
    let groups = preflight(&db, &all).unwrap().unresolved;
    assert_eq!(groups.len(), 1);
    crate::shift_changes::defer(&db, &groups).unwrap();
    assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
    assert_eq!(
        reconciliation::calculate(&app, &other, &ow, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM shift_review_deferrals", [], |r| r
            .get(0))
            .unwrap(),
        1
    );
    approve_group(
        &db,
        &all,
        &[(groups[0].fingerprint.clone(), "imported:2".into())],
    )
    .unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [60, 0, 0, 0]
    );
}
#[test]
fn cross_source_exact_duplicates_pay_once_in_either_recording_order() {
    for csv_first in [true, false] {
        let (dir, app) = setup();
        let (r, w) = period(&app, 1, "2026-04-01", 1);
        if csv_first {
            csv(&dir, "hours.csv", &[ORIGINAL]);
            imported(&dir, &app);
        }
        direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
        if !csv_first {
            csv(&dir, "hours.csv", &[ORIGINAL]);
            imported(&dir, &app);
        }
        let db = open(&app).unwrap();
        let all = load(&app).unwrap();
        let g = preflight(&db, &all).unwrap().unresolved.remove(0);
        assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
        approve_group(&db, &all, &[(g.fingerprint, "imported:1".into())]).unwrap();
        assert_eq!(
            reconciliation::calculate(&app, &r, &w, None)
                .unwrap()
                .week_totals_minutes,
            [60, 0, 0, 0]
        );
        assert_eq!(all.len(), 2);
    }
}
#[test]
fn touching_same_day_separate_shifts_do_not_need_duplicate_review() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    direct(&app, "2026-04-02T10:00", "2026-04-02T11:00");
    assert!(preflight(&open(&app).unwrap(), &load(&app).unwrap())
        .unwrap()
        .unresolved
        .is_empty());
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [120, 0, 0, 0]
    );
}
#[test]
fn cross_midnight_overlap_is_reviewed_and_explicit_separate_choice_is_audited() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    direct(&app, "2026-04-02T23:00", "2026-04-03T01:00");
    direct(&app, "2026-04-03T00:30", "2026-04-03T01:30");
    let db = open(&app).unwrap();
    let all = load(&app).unwrap();
    let group = preflight(&db, &all).unwrap().unresolved.remove(0);
    assert_eq!(group.candidates.len(), 2);
    approve_group(&db, &all, &[(group.fingerprint, "separate".into())]).unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [180, 0, 0, 0]
    );
    assert_eq!(
        db.query_row::<String, _, _>(
            "SELECT winner_source FROM payroll_duplicate_decisions",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        "separate"
    );
}
#[test]
fn omitted_rows_never_delete_or_create_negative_corrections() {
    let (dir, app) = setup();
    csv(&dir, "hours.csv", &[ORIGINAL, MOVED]);
    imported(&dir, &app);
    csv(&dir, "hours.csv", &[MOVED]);
    imported(&dir, &app);
    let db = open(&app).unwrap();
    assert_eq!(app.repository.get_all_raw().unwrap().len(), 2);
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_corrections", [], |r| r.get(0))
            .unwrap(),
        0
    );
    assert!(preflight(&db, &load(&app).unwrap())
        .unwrap()
        .unresolved
        .is_empty());
}
#[test]
fn prospective_settled_change_needs_source_approval_and_explicit_financial_carry() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    submit(&app, &r, &w);
    settle(&app, &r);
    let db = open(&app).unwrap();
    let sid = lifecycle::latest(&db, r.id).unwrap().unwrap();
    let original = lifecycle::items(&db, sid).unwrap();
    assert!(app
        .direct_shift_repository
        .edit_completed(
            id,
            parse_clock("2026-04-02T09:00").unwrap(),
            parse_clock("2026-04-02T11:00").unwrap(),
            0,
            None,
            "new"
        )
        .is_err());
    reviewed_edit(
        &app,
        id,
        parse_clock("2026-04-02T09:00").unwrap(),
        parse_clock("2026-04-02T11:00").unwrap(),
        0,
        None,
        "new",
    )
    .unwrap();
    reconciliation::sync_settled_corrections(&app, 1).unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_corrections", [], |r| r.get(0))
            .unwrap(),
        0
    );
    let change = reconciliation::changes(&app, &r).unwrap();
    reconciliation::carry(&app, &r, &change.signature, false).unwrap();
    assert_eq!(lifecycle::stage(&db, &r).unwrap(), Stage::Settled);
    assert!(lifecycle::payroll_items_equal(
        &original,
        &lifecycle::items(&db, sid).unwrap()
    ));
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT minutes FROM payroll_corrections", [], |r| r.get(0))
            .unwrap(),
        60
    );
}
#[test]
fn edits_validate_original_and_destination_cycles_and_reject_uncertain_destinations() {
    let (_dir, app) = setup();
    let (_open, _) = period(&app, 1, "2026-04-01", 1);
    let (dest, dw) = period(&app, 2, "2026-04-29", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    submit(&app, &dest, &dw);
    let before = app
        .direct_shift_repository
        .get_including_deleted(id)
        .unwrap()
        .unwrap();
    let moved = parse_clock("2026-04-30T09:00").unwrap();
    let finish = parse_clock("2026-04-30T10:00").unwrap();
    assert!(app
        .direct_shift_repository
        .edit_completed(id, moved, finish, 0, None, "move")
        .is_err());
    let db = open(&app).unwrap();
    db.execute("UPDATE payroll_timesheet_snapshot_states SET state='indeterminate' WHERE payroll_timesheet_id=?1",[dest.id]).unwrap();
    assert!(crate::shift_changes::review(
        &db,
        1,
        &[
            (before.start().unwrap(), before.end().unwrap().unwrap()),
            (moved, finish)
        ]
    )
    .is_err());
    assert_eq!(
        app.direct_shift_repository
            .get_including_deleted(id)
            .unwrap()
            .unwrap(),
        before
    );
}
#[test]
fn stale_editor_and_reviewed_duplicate_choice_are_rejected() {
    let (_dir, app) = setup();
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let before = app
        .direct_shift_repository
        .get_including_deleted(id)
        .unwrap()
        .unwrap();
    let end = parse_clock("2026-04-02T11:00").unwrap();
    let db = open(&app).unwrap();
    let review =
        crate::shift_changes::direct_review(&db, &before, before.start().unwrap(), end, 0, None)
            .unwrap();
    app.direct_shift_repository
        .edit_completed(id, before.start().unwrap(), end, 5, None, "concurrent")
        .unwrap();
    assert!(app
        .direct_shift_repository
        .edit_completed_reviewed(
            &before,
            before.start().unwrap(),
            end,
            0,
            None,
            "stale",
            "reason",
            &review.signature
        )
        .is_err());
    direct(&app, "2026-04-02T09:30", "2026-04-02T10:30");
    let old = load(&app).unwrap();
    let g = preflight(&db, &old).unwrap().unresolved.remove(0);
    direct(&app, "2026-04-02T10:00", "2026-04-02T12:00");
    assert!(approve_group(&db, &old, &[(g.fingerprint, old[0].key())]).is_err());
}
#[test]
fn production_lock_excludes_imports_and_logger_writes_across_connections() {
    let (dir, app) = setup();
    csv(&dir, "hours.csv", &[ORIGINAL]);
    let db = open(&app).unwrap();
    let guard = crate::timesheet_delivery::production_lock(&db).unwrap();
    let archive = dir.path().join("archive");
    let service = ImportService::new(&app.repository, dir.path().join("import"), &archive);
    assert!(service.run().is_err());
    assert!(app
        .direct_shift_repository
        .clock_in(1, parse_clock("2026-04-02T09:00").unwrap(), "blocked")
        .is_err());
    drop(guard);
    assert_eq!(service.run().unwrap().rows_imported, 1);
}
#[test]
fn changed_internal_and_imported_evidence_make_pdf_candidate_stale_without_changing_bytes() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let pdf = candidate(&app, &r, &w);
    let bytes = std::fs::read(&pdf).unwrap();
    let db = open(&app).unwrap();
    let signature = lifecycle::candidate_signature(&db, r.id).unwrap();
    app.direct_shift_repository
        .edit_completed(
            id,
            parse_clock("2026-04-02T09:00").unwrap(),
            parse_clock("2026-04-02T11:00").unwrap(),
            0,
            None,
            "edit",
        )
        .unwrap();
    assert_ne!(
        signature,
        lifecycle::candidate_signature(&db, r.id).unwrap()
    );
    assert!(lifecycle::verify_candidate_evidence(&db, r.id).is_err());
    csv(&dir, "hours.csv", &[CHANGED]);
    imported(&dir, &app);
    assert!(lifecycle::verify_candidate_evidence(&db, r.id).is_err());
    assert_eq!(std::fs::read(&pdf).unwrap(), bytes);
}

#[test]
fn reviewed_payload_cannot_change_breaks_notes_or_payroll_state_after_approval() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let before = app
        .direct_shift_repository
        .get_including_deleted(id)
        .unwrap()
        .unwrap();
    let db = open(&app).unwrap();
    let end = parse_clock("2026-04-02T11:00").unwrap();
    let approved = crate::shift_changes::direct_review(
        &db,
        &before,
        before.start().unwrap(),
        end,
        0,
        Some("approved note"),
    )
    .unwrap();
    assert!(app
        .direct_shift_repository
        .edit_completed_reviewed(
            &before,
            before.start().unwrap(),
            end,
            10,
            Some("approved note"),
            "save",
            "reason",
            &approved.signature
        )
        .is_err());
    assert!(app
        .direct_shift_repository
        .edit_completed_reviewed(
            &before,
            before.start().unwrap(),
            end,
            0,
            Some("changed note"),
            "save",
            "reason",
            &approved.signature
        )
        .is_err());
    submit(&app, &r, &w);
    assert!(app
        .direct_shift_repository
        .edit_completed_reviewed(
            &before,
            before.start().unwrap(),
            end,
            0,
            Some("approved note"),
            "save",
            "reason",
            &approved.signature
        )
        .is_err());
    assert_eq!(
        app.direct_shift_repository
            .get_including_deleted(id)
            .unwrap()
            .unwrap(),
        before
    );
}
#[test]
fn moving_selected_internal_duplicate_keeps_csv_counterpart_under_review() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    csv(&dir, "hours.csv", &[ORIGINAL]);
    imported(&dir, &app);
    let db = open(&app).unwrap();
    let all = load(&app).unwrap();
    let g = preflight(&db, &all).unwrap().unresolved.remove(0);
    approve_group(&db, &all, &[(g.fingerprint, format!("direct:{id}"))]).unwrap();
    app.direct_shift_repository
        .edit_completed(
            id,
            parse_clock("2026-04-03T09:00").unwrap(),
            parse_clock("2026-04-03T10:00").unwrap(),
            0,
            Some("moved"),
            "new edit",
        )
        .unwrap();
    let pre = preflight(&db, &load(&app).unwrap()).unwrap();
    assert_eq!(pre.unresolved.len(), 1);
    assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
    assert_eq!(app.repository.get_all_raw().unwrap().len(), 1);
}
#[test]
fn concurrent_import_instances_insert_one_copy_and_one_success_audit() {
    let (dir, app) = setup();
    csv(&dir, "hours.csv", &[ORIGINAL]);
    let path = app.context.environment.database_path.clone();
    let input = dir.path().join("import");
    let archive = dir.path().join("archive");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let path = path.clone();
        let input = input.clone();
        let archive = archive.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let repo =
                crate::repository::TimesheetRepository::new(crate::database::open(path).unwrap());
            let service = ImportService::new(&repo, input, &archive);
            barrier.wait();
            for _ in 0..100 {
                match service.run() {
                    Ok(r) => return r.rows_imported,
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
            panic!("Import lock never became available");
        }));
    }
    let inserted: i64 = threads.into_iter().map(|t| t.join().unwrap()).sum();
    assert_eq!(inserted, 1);
    let db = open(&app).unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM timesheets", [], |r| r.get(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM import_audit WHERE status='SUCCESS'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn first_changed_import_after_legacy_upgrade_uses_original_archive_without_reclassifying_old_rows()
{
    let (dir, app) = setup();
    csv(&dir, "hours.csv", &[ORIGINAL]);
    let initial = imported(&dir, &app);
    let db = open(&app).unwrap();
    crate::database::tests::remove_schema_38_fixture(&db);
    crate::shift_changes::migrate(&db).unwrap();
    assert!(!crate::shift_changes::prospective(&db, "imported", 1).unwrap());
    csv(&dir, "hours.csv", &[MOVED]);
    assert_eq!(imported(&dir, &app).rows_imported, 1);
    assert!(!crate::shift_changes::prospective(&db, "imported", 1).unwrap());
    let pre = preflight(&db, &load(&app).unwrap()).unwrap();
    assert_eq!(pre.unresolved.len(), 1);
    assert!(initial.archived_paths[0].is_file());
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM import_audit WHERE status='SUCCESS'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        2
    );
}

#[test]
fn possible_one_day_date_changes_across_sources_are_reviewed_in_both_orders() {
    for csv_first in [false, true] {
        let (dir, app) = setup();
        let (r, w) = period(&app, 1, "2026-04-01", 1);
        if csv_first {
            csv(&dir, "hours.csv", &[MOVED]);
            imported(&dir, &app);
        }
        direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
        if !csv_first {
            csv(&dir, "hours.csv", &[MOVED]);
            imported(&dir, &app);
        }
        let db = open(&app).unwrap();
        let all = load(&app).unwrap();
        let g = preflight(&db, &all).unwrap().unresolved.remove(0);
        assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
        approve_group(&db, &all, &[(g.fingerprint, "direct:1".into())]).unwrap();
        let calculated = reconciliation::calculate(&app, &r, &w, None).unwrap();
        assert_eq!(calculated.week_totals_minutes, [60, 0, 0, 0]);
        assert_eq!(calculated.snapshot_items.len(), 1);
        assert_eq!(
            crate::models::parse_employment_date(
                calculated.snapshot_items[0].work_date.as_deref().unwrap()
            )
            .unwrap()
            .to_string(),
            "2026-04-02"
        );
    }
}
#[test]
fn co_present_daily_csv_records_are_not_mistaken_for_moved_revisions() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    csv(&dir, "hours.csv", &[ORIGINAL, MOVED]);
    assert_eq!(imported(&dir, &app).rows_imported, 2);
    assert!(preflight(&open(&app).unwrap(), &load(&app).unwrap())
        .unwrap()
        .unresolved
        .is_empty());
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [120, 0, 0, 0]
    );
}
#[test]
fn legacy_cross_midnight_groups_are_not_reclassified_by_migration() {
    let (_dir, app) = setup();
    direct(&app, "2026-04-02T23:00", "2026-04-03T01:00");
    direct(&app, "2026-04-03T00:30", "2026-04-03T01:30");
    let db = open(&app).unwrap();
    crate::database::tests::remove_schema_38_fixture(&db);
    crate::shift_changes::migrate(&db).unwrap();
    assert!(preflight(&db, &load(&app).unwrap())
        .unwrap()
        .unresolved
        .is_empty());
    // A prospective edit reopens review rather than silently changing the legacy result.
    app.direct_shift_repository
        .edit_completed(
            2,
            parse_clock("2026-04-03T00:30").unwrap(),
            parse_clock("2026-04-03T02:00").unwrap(),
            0,
            Some("changed"),
            "new",
        )
        .unwrap();
    assert_eq!(
        preflight(&db, &load(&app).unwrap())
            .unwrap()
            .unresolved
            .len(),
        1
    );
}

#[test]
fn legacy_date_only_imports_do_not_block_new_logger_completion_or_invent_clock_matches() {
    let (_dir, app) = setup();
    let db = open(&app).unwrap();
    db.execute_batch("INSERT INTO timesheets(personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount,notes) VALUES(1,'Example PA','2026-04-01','2026-04-01',0,60,12,12,'actual');").unwrap();
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    assert_eq!(load(&app).unwrap().len(), 2);
    assert!(preflight(&db, &load(&app).unwrap())
        .unwrap()
        .unresolved
        .is_empty());
}
