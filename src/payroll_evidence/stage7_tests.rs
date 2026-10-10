use super::*;
use crate::imported_hours::{self, Change};
use crate::repository::TimesheetCorrectionProposal;
fn import(app: &Application, dir: &tempfile::TempDir, rows: &str) {
    std::fs::create_dir_all(dir.path().join("import")).unwrap();
    std::fs::create_dir_all(dir.path().join("archive")).unwrap();
    std::fs::write(
        dir.path().join("import/hours.csv"),
        format!("period\nPA,Start,End,Break,Worked,Rate,Amount,Note\n{rows}\n"),
    )
    .unwrap();
    crate::import_service::ImportService::new(
        &app.repository,
        dir.path().join("import"),
        &dir.path().join("archive"),
    )
    .run()
    .unwrap();
}
const ROW: &str =
    "Example PA,2 April 2026 at 09:00:00,2 April 2026 at 10:00:00,0h 00m,1h 00m,£12,£12,actual";
fn change(
    app: &Application,
    id: i64,
    c: &Change,
    authorised: bool,
) -> crate::imported_hours::Result<crate::imported_hours::Snapshot> {
    let db = app.repository.connection();
    let s = imported_hours::snapshot(db, id)?;
    let r = imported_hours::review(db, &s, c)?;
    imported_hours::apply(
        db,
        &s,
        c,
        "Reviewed regression correction",
        &r.signature,
        authorised,
    )
}
fn edit(start: &str, end: &str, minutes: i64) -> Change {
    Change::Edit(TimesheetCorrectionProposal {
        start_time: start.into(),
        end_time: end.into(),
        break_minutes: 5,
        worked_minutes: minutes,
        notes: Some("reviewed".into()),
    })
}
#[test]
fn stage7_imported_edit_and_revert_preserve_raw_and_append_before_after_history() {
    let (dir, app) = setup();
    period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    let original = app.get_timesheets().unwrap();
    let c = edit("2026-04-02T09:00", "2026-04-02T11:00", 97);
    let changed = change(&app, 1, &c, false).unwrap();
    assert_eq!(changed.entry.effective.worked_minutes, 97);
    assert_eq!(changed.entry.raw, original[0]);
    assert!(imported_hours::consistency_warning(&changed).is_some());
    let back = change(&app, 1, &Change::Revert, false).unwrap();
    assert_eq!(back.entry.effective.worked_minutes, 60);
    let history = app.repository.correction_history(1).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].action_type, "revert");
    assert_eq!(history[1].before.worked_minutes, 97);
    assert_eq!(app.get_timesheets().unwrap(), original);
}
#[test]
fn stage7_stale_edit_and_exclusion_versions_reject_without_audit_or_overwrite() {
    let (dir, app) = setup();
    period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    let db = app.repository.connection();
    let old = imported_hours::snapshot(db, 1).unwrap();
    let c = edit("2026-04-02T09:00", "2026-04-02T11:00", 90);
    let r = imported_hours::review(db, &old, &c).unwrap();
    change(&app, 1, &Change::Exclude, false).unwrap();
    assert!(imported_hours::apply(db, &old, &c, "stale", &r.signature, false).is_err());
    let s = change(&app, 1, &Change::Restore, false).unwrap();
    let r = imported_hours::review(db, &s, &c).unwrap();
    change(&app, 1, &c, false).unwrap();
    assert!(imported_hours::apply(db, &s, &c, "stale", &r.signature, false).is_err());
    assert_eq!(app.repository.correction_history(1).unwrap().len(), 1);
}
#[test]
fn stage7_exclusion_restore_audit_and_duplicate_choices_require_new_review() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let db = open(&app).unwrap();
    let all = load(&app).unwrap();
    let g = preflight(&db, &all).unwrap().unresolved.remove(0);
    approve_group(&db, &all, &[(g.fingerprint, "imported:1".into())]).unwrap();
    let excluded = change(&app, 1, &Change::Exclude, false).unwrap();
    assert!(excluded.excluded);
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [60, 0, 0, 0]
    );
    let restored = change(&app, 1, &Change::Restore, false).unwrap();
    assert!(!restored.excluded);
    assert!(!preflight(&db, &load(&app).unwrap())
        .unwrap()
        .unresolved
        .is_empty());
    assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM imported_hours_inclusion_events",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        2
    );
    assert!(db
        .execute("DELETE FROM imported_hours_inclusion_events", [])
        .is_err());
    assert!(db
        .execute(
            "UPDATE imported_hours_inclusion_events SET reason='changed'",
            []
        )
        .is_err());
}
#[test]
fn stage7_settled_exclusion_requires_approval_preserves_submission_and_explicit_carry() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    submit(&app, &r, &w);
    settle(&app, &r);
    let db = open(&app).unwrap();
    let sid = lifecycle::latest(&db, r.id).unwrap().unwrap();
    let old = lifecycle::items(&db, sid).unwrap();
    assert!(change(&app, 1, &Change::Exclude, false).is_err());
    change(&app, 1, &Change::Exclude, true).unwrap();
    assert_eq!(lifecycle::stage(&db, &r).unwrap(), Stage::Settled);
    assert!(lifecycle::payroll_items_equal(
        &old,
        &lifecycle::items(&db, sid).unwrap()
    ));
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_corrections", [], |r| r.get(0))
            .unwrap(),
        0
    );
    let delta = reconciliation::changes(&app, &r).unwrap();
    reconciliation::carry(&app, &r, &delta.signature, false).unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT minutes FROM payroll_corrections", [], |r| r.get(0))
            .unwrap(),
        -60
    );
    change(&app, 1, &Change::Restore, true).unwrap();
    assert_eq!(lifecycle::stage(&db, &r).unwrap(), Stage::Settled);
}
#[test]
fn stage7_submitted_edit_and_reversion_do_not_overwrite_original_pdf_or_submission() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    submit(&app, &r, &w);
    let db = open(&app).unwrap();
    let sid = lifecycle::latest(&db, r.id).unwrap().unwrap();
    let before = lifecycle::items(&db, sid).unwrap();
    let bytes: Vec<u8> = db
        .query_row(
            "SELECT pdf_bytes FROM payroll_submissions WHERE id=?1",
            [sid],
            |r| r.get(0),
        )
        .unwrap();
    let c = edit("2026-04-02T09:00", "2026-04-02T11:00", 110);
    assert!(change(&app, 1, &c, false).is_err());
    change(&app, 1, &c, true).unwrap();
    assert_eq!(lifecycle::stage(&db, &r).unwrap(), Stage::Submitted);
    let discrepancy = reconciliation::changes(&app, &r).unwrap();
    assert!(discrepancy.changed);
    assert_eq!(
        db.query_row::<Vec<u8>, _, _>(
            "SELECT pdf_bytes FROM payroll_submissions WHERE id=?1",
            [sid],
            |r| r.get(0)
        )
        .unwrap(),
        bytes
    );
    assert!(lifecycle::payroll_items_equal(
        &before,
        &lifecycle::items(&db, sid).unwrap()
    ));
    change(&app, 1, &Change::Revert, true).unwrap();
    assert_eq!(app.repository.correction_history(1).unwrap().len(), 2);
}
#[test]
fn stage7_uncertain_original_or_destination_and_stale_authorisation_block_changes() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let (dest, _) = period(&app, 2, "2026-04-29", 1);
    import(&app, &dir, ROW);
    let db = open(&app).unwrap();
    let s = imported_hours::snapshot(&db, 1).unwrap();
    let c = edit("2026-04-30T09:00", "2026-04-30T10:00", 55);
    let review = imported_hours::review(&db, &s, &c).unwrap();
    submit(&app, &r, &w);
    assert!(imported_hours::apply(&db, &s, &c, "old approval", &review.signature, true).is_err());
    db.execute("INSERT INTO payroll_timesheet_snapshot_states(payroll_timesheet_id,state,pdf_path,pdf_sha256,generated_at) VALUES(?1,'indeterminate','missing','digest','fixture')",[dest.id]).unwrap();
    assert!(change(&app, 1, &c, true).is_err());
    db.execute("UPDATE payroll_timesheet_snapshot_states SET state='indeterminate' WHERE payroll_timesheet_id=?1",[r.id]).unwrap();
    assert!(change(&app, 1, &Change::Exclude, true).is_err());
    assert!(change(&app, 1, &Change::Restore, true).is_err());
    assert!(app.repository.correction_history(1).unwrap().is_empty());
}
#[test]
fn stage7_edit_exclusion_and_restore_invalidate_candidates_but_preserve_pdf_bytes() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    for c in [
        edit("2026-04-02T09:00", "2026-04-02T11:00", 105),
        Change::Exclude,
        Change::Restore,
    ] {
        let pdf = candidate(&app, &r, &w);
        let bytes = std::fs::read(&pdf).unwrap();
        change(&app, 1, &c, false).unwrap();
        assert!(crate::timesheet_delivery::capture(&open(&app).unwrap(), r.id).is_err());
        assert_eq!(std::fs::read(pdf).unwrap(), bytes);
        assert!(app
            .payroll_worked_item_repository
            .snapshot_metadata(r.id)
            .unwrap()
            .is_none());
    }
}
#[test]
fn stage7_deleted_moved_shift_still_validates_retained_protected_cycle() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    period(&app, 2, "2026-04-29", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    submit(&app, &r, &w);
    reviewed_edit(
        &app,
        id,
        parse_clock("2026-04-30T09:00").unwrap(),
        parse_clock("2026-04-30T10:00").unwrap(),
        0,
        None,
        "move",
    )
    .unwrap();
    let current = app
        .direct_shift_repository
        .get_including_deleted(id)
        .unwrap()
        .unwrap();
    assert!(app
        .direct_shift_repository
        .soft_delete_completed_expected(&current, "delete")
        .is_err());
    let db = open(&app).unwrap();
    db.execute("UPDATE payroll_timesheet_snapshot_states SET state='indeterminate' WHERE payroll_timesheet_id=?1",[r.id]).unwrap();
    assert!(app
        .direct_shift_repository
        .soft_delete_completed_expected(&current, "delete uncertain")
        .is_err());
    assert!(app
        .direct_shift_repository
        .get_including_deleted(id)
        .unwrap()
        .unwrap()
        .deleted_at
        .is_none());
}
#[test]
fn stage7_stale_deletion_after_other_connection_edit_preserves_new_values() {
    let (_dir, app) = setup();
    period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let expected = app
        .direct_shift_repository
        .get_including_deleted(id)
        .unwrap()
        .unwrap();
    let other = crate::direct_shift_repository::DirectShiftRepository::new(open(&app).unwrap());
    other
        .edit_completed(
            id,
            parse_clock("2026-04-02T09:00").unwrap(),
            parse_clock("2026-04-02T11:00").unwrap(),
            0,
            None,
            "other",
        )
        .unwrap();
    assert!(app
        .direct_shift_repository
        .soft_delete_completed_expected(&expected, "confirm old")
        .is_err());
    let fresh = other.get_including_deleted(id).unwrap().unwrap();
    assert!(fresh.deleted_at.is_none());
    assert_eq!(fresh.worked_minutes().unwrap(), Some(120));
}
#[test]
fn stage7_searchable_bounded_history_reaches_older_shifts() {
    let (_dir, app) = setup();
    for day in 1..=28 {
        direct(
            &app,
            &format!("2026-04-{day:02}T09:00"),
            &format!("2026-04-{day:02}T10:00"),
        );
    }
    let recent = app
        .direct_shift_repository
        .history_for_pa(1, "", 0, 12)
        .unwrap();
    let older = app
        .direct_shift_repository
        .history_for_pa(1, "", 12, 12)
        .unwrap();
    assert_eq!(recent.len(), 12);
    assert_eq!(older.len(), 12);
    assert!(recent.iter().all(|r| older.iter().all(|o| o.id != r.id)));
    let found = app
        .direct_shift_repository
        .history_for_pa(1, "2026-04-02", 0, 12)
        .unwrap();
    assert_eq!(found.len(), 1);
    let s = &found[0];
    app.direct_shift_repository
        .soft_delete_completed_expected(s, "eligible historical deletion")
        .unwrap();
    assert!(app
        .direct_shift_repository
        .history_for_pa(1, "2026-04-02", 0, 12)
        .unwrap()
        .is_empty());
}
#[test]
fn stage7_concurrent_imported_edits_and_dispatch_lock_do_not_partially_mutate() {
    let (dir, app) = setup();
    period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    let db = open(&app).unwrap();
    let expected = imported_hours::snapshot(&db, 1).unwrap();
    let c = edit("2026-04-02T09:00", "2026-04-02T11:00", 100);
    let review = imported_hours::review(&db, &expected, &c).unwrap();
    {
        let _lock = crate::timesheet_delivery::production_lock(&db).unwrap();
        assert!(change(&app, 1, &c, false).is_err());
    }
    let other = open(&app).unwrap();
    imported_hours::apply(
        &other,
        &expected,
        &c,
        "first instance",
        &review.signature,
        false,
    )
    .unwrap();
    assert!(imported_hours::apply(
        &db,
        &expected,
        &Change::Exclude,
        "second instance",
        &review.signature,
        false
    )
    .is_err());
    assert_eq!(app.repository.correction_history(1).unwrap().len(), 1);
    assert!(!imported_hours::snapshot(&db, 1).unwrap().excluded);
}
#[test]
fn stage7_midnight_week_and_year_moves_keep_start_date_allocation_and_original_dates() {
    let (dir, mut app) = setup();
    app.context.config.payroll.rounding_minutes = 1;
    let (r, w) = period(&app, 1, "2026-12-28", 1);
    let (dest, dw) = period(&app, 2, "2027-01-25", 1);
    import(&app,&dir,"Example PA,31 December 2026 at 23:00:00,1 January 2027 at 01:00:00,0h 05m,1h 23m,£12,£16.60,overnight");
    let original = app.get_timesheets().unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [83, 0, 0, 0]
    );
    change(
        &app,
        1,
        &edit("2027-01-05T23:00", "2027-01-06T01:00", 78),
        false,
    )
    .unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [0, 78, 0, 0]
    );
    change(
        &app,
        1,
        &edit("2027-01-25T23:00", "2027-01-26T01:00", 71),
        false,
    )
    .unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &dest, &dw, None)
            .unwrap()
            .week_totals_minutes,
        [71, 0, 0, 0]
    );
    assert_eq!(app.get_timesheets().unwrap(), original);
}
#[test]
fn stage7_existing_holiday_subtraction_is_retained_once_after_imported_correction() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    import(&app,&dir,"Example PA,3 April 2026 at 08:00:00,3 April 2026 at 18:00:00,0h 00m,10h 00m,£12,£120,holiday work");
    let db = open(&app).unwrap();
    app.payroll_timesheet_repository
        .insert_public_holiday(r.id, 1, "03/04/2026")
        .unwrap();
    db.execute(
        "UPDATE payroll_timesheet_public_holidays SET hours=2 WHERE payroll_timesheet_id=?1",
        [r.id],
    )
    .unwrap();
    app.payroll_worked_item_repository
        .set_manual_adjustment(
            r.id,
            &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number: 1,
                adjustment_minutes: -120,
                reason: Some("Existing deliberate holiday subtraction".into()),
            },
            "test",
        )
        .unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [480, 0, 0, 0]
    );
    change(
        &app,
        1,
        &edit("2026-04-03T08:00", "2026-04-03T20:00", 720),
        false,
    )
    .unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [600, 0, 0, 0]
    );
    assert_eq!(
        app.payroll_timesheet_repository
            .get_public_holidays(r.id)
            .unwrap()[0]
            .hours,
        2.0
    );
    change(&app, 1, &Change::Revert, false).unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [480, 0, 0, 0]
    );
}
#[test]
fn stage7_invalid_edit_and_audit_failure_roll_back_all_mutation_and_history() {
    let (dir, app) = setup();
    period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    let db = app.repository.connection();
    let original = imported_hours::snapshot(db, 1).unwrap();
    for c in [
        edit("2026-04-02T11:00", "2026-04-02T10:00", 60),
        edit("invalid", "2026-04-02T10:00", 60),
        edit("2026-04-02T09:00", "2026-04-02T10:00", -1),
    ] {
        assert!(change(&app, 1, &c, false).is_err());
        assert_eq!(imported_hours::snapshot(db, 1).unwrap(), original);
    }
    let review = imported_hours::review(db, &original, &Change::Exclude).unwrap();
    db.execute_batch("CREATE TRIGGER fail_stage7_audit BEFORE INSERT ON shift_change_events BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(imported_hours::apply(
        db,
        &original,
        &Change::Exclude,
        "failure",
        &review.signature,
        false
    )
    .is_err());
    assert_eq!(imported_hours::snapshot(db, 1).unwrap(), original);
    assert_eq!(
        db.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM imported_hours_inclusion_events",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn stage7_restoring_changed_excluded_source_reviews_new_overlap_and_protected_counterpart() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    period(&app, 2, "2026-04-29", 1);
    import(&app, &dir, ROW);
    change(&app, 1, &Change::Exclude, false).unwrap();
    change(
        &app,
        1,
        &edit("2026-04-30T09:00", "2026-04-30T10:00", 55),
        false,
    )
    .unwrap();
    direct(&app, "2026-04-30T09:30", "2026-04-30T10:30");
    let db = open(&app).unwrap();
    let s = imported_hours::snapshot(&db, 1).unwrap();
    let review = imported_hours::review(&db, &s, &Change::Restore).unwrap();
    // Submission of a different interval changes the original cycle state and invalidates this review.
    submit(&app, &r, &w);
    assert!(imported_hours::apply(
        &db,
        &s,
        &Change::Restore,
        "stale payroll",
        &review.signature,
        false
    )
    .is_err());
    change(&app, 1, &Change::Restore, true).unwrap();
    assert_eq!(
        preflight(&db, &load(&app).unwrap())
            .unwrap()
            .unresolved
            .len(),
        1
    );
}
#[test]
fn stage7_new_financial_obligation_invalidates_displayed_source_authorisation() {
    let (dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    submit(&app, &r, &w);
    let db = open(&app).unwrap();
    let s = imported_hours::snapshot(&db, 1).unwrap();
    let c = edit("2026-04-02T09:00", "2026-04-02T11:00", 110);
    let review = imported_hours::review(&db, &s, &c).unwrap();
    assert!(review.protected);
    db.execute("INSERT INTO payroll_corrections(personal_assistant_id,origin_payroll_timesheet_id,submission_id,evidence_key,minutes,reason,created_at) VALUES(1,?1,?2,'retained obligation',30,'Previous financial review','fixture')",params![r.id,lifecycle::latest(&db,r.id).unwrap()]).unwrap();
    assert!(imported_hours::apply(
        &db,
        &s,
        &c,
        "stale financial state",
        &review.signature,
        true
    )
    .is_err());
    assert_eq!(imported_hours::snapshot(&db, 1).unwrap(), s);
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT minutes FROM payroll_corrections", [], |r| r.get(0))
            .unwrap(),
        30
    );
}
