use super::*;
use crate::imported_hours::{self, Change};
use crate::repository::TimesheetCorrectionProposal;
fn import(app: &Application, dir: &crate::private_storage::PrivateTempDir, rows: &str) {
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
        assert!(crate::timesheet_delivery::capture(&*open(&app).unwrap(), r.id).is_err());
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

fn stage2b_imported_rounding_edit(initial: i64, edited: i64, accepted: i64) -> Vec<i64> {
    let (dir, mut app) = setup();
    app.context.config.payroll.rounding_minutes = 15;
    app.context.config.payroll.rounding_direction = "Up".into();
    let (record, weeks) = period(&app, 1, "2026-04-01", 1);
    import(
        &app, &dir,
        &format!("Example PA,2 April 2026 at 09:00:00,2 April 2026 at 11:00:00,0h 00m,1h {:02}m,£12,£12,fixture", initial - 60),
    );
    submit(&app, &record, &weeks);
    settle(&app, &record);
    let connection = open(&app).unwrap();
    let submission = lifecycle::latest(&connection, record.id).unwrap().unwrap();
    let before = lifecycle::items(&connection, submission).unwrap();
    assert_eq!(before[0].worked_minutes, accepted);
    let edit = Change::Edit(TimesheetCorrectionProposal {
        start_time: "2026-04-02T09:00".into(),
        end_time: "2026-04-02T11:00".into(),
        break_minutes: 0,
        worked_minutes: edited,
        notes: Some("fictional edit".into()),
    });
    let changed = change(&app, 1, &edit, true).unwrap();
    assert_eq!(changed.entry.raw.worked_minutes, initial);
    assert_eq!(changed.entry.effective.worked_minutes, edited);
    reconciliation::sync_settled_corrections(&app, 1).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM payroll_corrections", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(lifecycle::items(&connection, submission).unwrap(), before);
    reconciliation::changes(&app, &record)
        .unwrap()
        .components
        .into_iter()
        .map(|c| c.minutes)
        .collect()
}

#[test]
fn stage2b_retained_62_to_63_same_rounding_bucket_is_zero() {
    assert_eq!(
        stage2b_imported_rounding_edit(62, 63, 75),
        Vec::<i64>::new()
    );
}

#[test]
fn stage2b_retained_62_to_76_crosses_one_rounding_boundary() {
    assert_eq!(stage2b_imported_rounding_edit(62, 76, 75), [15]);
}

#[test]
fn stage2b_amended_source_less_week_reduction_cannot_post_automatically() {
    let (dir, app) = setup();
    let (record, weeks) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    submit(&app, &record, &weeks);
    // A downward saved total takes the accepted amended-week comparison path.
    save_amendment(&app, &record, 0.75);
    submit(&app, &record, &weeks);
    let db = open(&app).unwrap();
    let submission = lifecycle::latest(&db, record.id).unwrap().unwrap();
    let retained = lifecycle::items(&db, submission).unwrap();
    assert_eq!(
        lifecycle::weeks(&db, submission).unwrap()[0].worked_hours,
        0.75
    );
    change(&app, 1, &Change::Exclude, true).unwrap();
    let proposal = reconciliation::changes(&app, &record).unwrap();
    assert_eq!(proposal.components.len(), 1);
    assert_eq!(proposal.components[0].minutes, -45);
    assert!(proposal.components[0].source.is_none());
    reconciliation::sync_settled_corrections(&app, 1).unwrap();
    assert_eq!(lifecycle::items(&db, submission).unwrap(), retained);
    assert_eq!(
        db.query_row("SELECT count(*) FROM payroll_corrections", [], |row| row.get::<_, i64>(0)).unwrap(),
        0,
        "source approval must not silently post a source-less weekly deduction; requires_complete={}",
        proposal.requires_complete
    );
    assert!(proposal.requires_complete);
    assert!(reconciliation::carry(&app, &record, &proposal.signature, false).is_err());
    reconciliation::carry(&app, &record, &proposal.signature, true).unwrap();
    let (next, dates) = period(&app, 2, "2026-04-29", 1);
    let payable = reconciliation::calculate(&app, &next, &dates, None).unwrap();
    assert_eq!(payable.week_totals_minutes, [-45, 0, 0, 0]);
    assert_eq!(payable.previous_cycle_minutes, -45);
    submit(&app, &next, &dates);
    let (later, dates) = period(&app, 3, "2026-05-27", 1);
    assert_eq!(
        reconciliation::calculate(&app, &later, &dates, None).unwrap().week_totals_minutes,
        [0; 4],
    );
}

#[test]
fn stage2b_date_move_at_equal_payable_minutes_preserves_source_identity() {
    let (dir, app) = setup();
    let (record, weeks) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    submit(&app, &record, &weeks);
    settle(&app, &record);
    let db = open(&app).unwrap();
    let submission = lifecycle::latest(&db, record.id).unwrap().unwrap();
    let original = lifecycle::items(&db, submission).unwrap();
    let moved = Change::Edit(TimesheetCorrectionProposal {
        start_time: "2026-04-10T09:00".into(),
        end_time: "2026-04-10T10:00".into(),
        break_minutes: 0,
        worked_minutes: 60,
        notes: Some("fictional date move".into()),
    });
    change(&app, 1, &moved, true).unwrap();
    let proposal = reconciliation::changes(&app, &record).unwrap();
    let (next, dates) = period(&app, 2, "2026-04-29", 1);
    let before_carry = reconciliation::calculate(&app, &next, &dates, None).unwrap();
    assert_eq!(before_carry.week_totals_minutes, [0; 4]);
    if !proposal.components.is_empty() {
        reconciliation::carry(&app, &record, &proposal.signature, proposal.requires_complete).unwrap();
    }
    let result = reconciliation::calculate(&app, &next, &dates, None).unwrap();
    assert_eq!(result.week_totals_minutes, [0; 4], "{}", proposal.description);
    import(&app, &dir, "Example PA,11 April 2026 at 09:00:00,11 April 2026 at 09:53:00,0h 00m,0h 53m,£12,£12,new work");
    stage2b_review_separate_export_rows(&app);
    let proposal = reconciliation::changes(&app, &record).unwrap();
    assert_eq!(proposal.components.len(), 1);
    assert_eq!(proposal.components[0].minutes, 60);
    assert!(!proposal.requires_complete);
    reconciliation::carry(&app, &record, &proposal.signature, false).unwrap();
    let result = reconciliation::calculate(&app, &next, &dates, None).unwrap();
    assert_eq!(result.week_totals_minutes, [60, 0, 0, 0]);
    assert_eq!(lifecycle::items(&db, submission).unwrap(), original);
}

#[test]
fn stage2b_amendment_does_not_reround_unchanged_paid_shift_when_actual_arrives() {
    let (dir, mut app) = setup();
    app.context.config.payroll.rounding_minutes = 15;
    app.context.config.payroll.rounding_direction = "Up".into();
    let (mut record, weeks) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, "Example PA,2 April 2026 at 09:00:00,2 April 2026 at 10:02:00,0h 00m,1h 02m,£12,£12,original");
    submit(&app, &record, &weeks);
    record.payroll_department_notes = "fictional amendment".into();
    save_amendment(&app, &record, 1.25);
    submit(&app, &record, &weeks);
    let db = open(&app).unwrap();
    let sid = lifecycle::latest(&db, record.id).unwrap().unwrap();
    let original = lifecycle::items(&db, sid).unwrap();
    assert_eq!(original[0].worked_minutes, 75);
    app.context.config.payroll.rounding_direction = "Down".into();
    assert!(reconciliation::changes(&app, &record)
        .unwrap()
        .components
        .is_empty());
    // Repeat the original row alongside the new row so import can distinguish
    // an addition from a possible replacement of an omitted row.
    import(&app, &dir, concat!(
        "Example PA,2 April 2026 at 09:00:00,2 April 2026 at 10:02:00,0h 00m,1h 02m,£12,£12,original\n",
        "Example PA,3 April 2026 at 09:00:00,3 April 2026 at 09:31:00,0h 00m,0h 31m,£12,£6,late",
    ));
    assert_eq!(app.get_timesheets().unwrap().len(), 2);
    assert_eq!(lifecycle::items(&db, sid).unwrap(), original);
    let proposed = reconciliation::changes(&app, &record).unwrap();
    assert_eq!(
        proposed
            .components
            .iter()
            .map(|c| c.minutes)
            .collect::<Vec<_>>(),
        [30]
    );
}

#[test]
fn stage2b_partial_csv_estimate_deduction_stays_held_until_confirmation() {
    let (dir, app) = setup();
    let (record, weeks) = period(&app, 1, "2026-04-01", 1);
    app.payroll_worked_item_repository
        .set_manual_adjustment(
            record.id,
            &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number: 1,
                adjustment_minutes: 180,
                reason: Some("estimate".into()),
            },
            "fictional estimate",
        )
        .unwrap();
    submit(&app, &record, &weeks);
    import(&app, &dir, ROW);
    reconciliation::sync_settled_corrections(&app, 1).unwrap();
    let db = open(&app).unwrap();
    let proposal = reconciliation::changes(&app, &record).unwrap();
    assert!(proposal.requires_complete);
    assert_eq!(
        proposal
            .components
            .iter()
            .map(|c| c.minutes)
            .collect::<Vec<_>>(),
        [-120]
    );
    assert!(reconciliation::carry(&app, &record, &proposal.signature, false).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM payroll_corrections", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let (next, dates) = period(&app, 2, "2026-04-29", 1);
    assert_eq!(
        reconciliation::calculate(&app, &next, &dates, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
    reconciliation::carry(&app, &record, &proposal.signature, true).unwrap();
    let hours = reconciliation::calculate(&app, &next, &dates, None).unwrap();
    assert_eq!(hours.week_totals_minutes, [-120, 0, 0, 0]);
    assert_eq!(hours.previous_cycle_minutes, -120);
}

#[test]
fn stage2b_changed_csv_assignee_keeps_separate_sources_and_accepted_owner() {
    let (dir, app) = setup();
    let (original_record, dates) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    submit(&app, &original_record, &dates);
    let db = open(&app).unwrap();
    let submission = lifecycle::latest(&db, original_record.id).unwrap().unwrap();
    let accepted = lifecycle::items(&db, submission).unwrap();
    import(&app, &dir, &ROW.replace("Example PA", "Other PA"));
    let current = load(&app).unwrap();
    assert_eq!(
        current.iter().map(|e| (e.pa, e.id)).collect::<Vec<_>>(),
        [(1, 1), (2, 2)]
    );
    assert!(groups(&current).unwrap().is_empty());
    assert_eq!(lifecycle::items(&db, submission).unwrap(), accepted);
    assert!(reconciliation::changes(&app, &original_record)
        .unwrap()
        .components
        .is_empty());
    let (other, _) = period(&app, 1, "2026-04-01", 2);
    let result = reconciliation::calculate(&app, &other, &dates, None).unwrap();
    assert_eq!(result.snapshot_items.len(), 1);
    assert_eq!(result.snapshot_items[0].timesheet_id, Some(2));
}

#[test]
fn stage2b_manual_totals_and_signed_carry_are_already_payable() {
    let (dir, mut app) = setup();
    app.context.config.payroll.rounding_direction = "Down".into();
    let (old, _) = period(&app, 1, "2026-04-01", 1);
    settle(&app, &old);
    let (next, dates) = period(&app, 2, "2026-04-29", 1);
    let db = open(&app).unwrap();
    db.execute("INSERT INTO payroll_corrections(id,personal_assistant_id,origin_payroll_timesheet_id,evidence_key,minutes,reason,created_at) VALUES(1,1,?1,'fixture',-7,'fictional correction','fixture')", [old.id]).unwrap();
    import(&app, &dir, &format!("{ROW}\nExample PA,30 April 2026 at 09:00:00,30 April 2026 at 09:34:00,0h 00m,0h 34m,£12,£12,fictional"));
    let preview = super::manual_preparation::preview(&app, &next, &dates).unwrap();
    assert_eq!(preview.actual, [30, 0, 0, 0]);
    assert_eq!(
        preview.totals([67, 0, 0, 0], &[1]).unwrap(),
        ([60, 0, 0, 0], -7)
    );
    super::manual_preparation::save(&app, &next, &dates, &preview, [67, 0, 0, 0], &[1]).unwrap();
    let hours = reconciliation::calculate_for_generation(&app, &next, &dates, None).unwrap();
    assert_eq!(hours.week_totals_minutes, [60, 0, 0, 0]);
    assert_eq!(hours.previous_cycle_minutes, -7);
    assert_eq!(
        hours
            .snapshot_items
            .iter()
            .map(|item| item.worked_minutes)
            .collect::<Vec<_>>(),
        [30, 37, -7]
    );
}


fn stage2b_rounding_matrix(step: i64, direction: &str) {
    for sign in [-1, 1] {
        let (dir, mut app) = setup();
        app.context.config.payroll.rounding_minutes = step;
        app.context.config.payroll.rounding_direction = direction.into();
        let (record, weeks) = period(&app, 1, "2026-04-01", 1);
        let remainder = if direction == "Up" { 1 } else { step - 1 };
        let raw = 2 * step + remainder;
        let paid = if direction == "Up" && step > 1 { 3 * step } else { raw / step * step };
        let csv_duration = format!("{}h {:02}m", raw / 60, raw % 60);
        import(&app, &dir, &format!(
            "Example PA,2 April 2026 at 09:00:00,2 April 2026 at 13:00:00,0h 00m,{csv_duration},£12,£12,first
Example PA,3 April 2026 at 09:00:00,3 April 2026 at 13:00:00,0h 00m,{csv_duration},£12,£12,second",
        ));
        submit(&app, &record, &weeks);
        settle(&app, &record);
        let db = open(&app).unwrap();
        let submission = lifecycle::latest(&db, record.id).unwrap().unwrap();
        let original = lifecycle::items(&db, submission).unwrap();
        assert_eq!(original.len(), 2);
        assert!(original.iter().all(|item| item.worked_minutes == paid));
        assert_eq!(
            lifecycle::weeks(&db, submission).unwrap()[0].worked_hours,
            (2 * paid) as f64 / 60.0,
        );

        app.context.config.payroll.rounding_direction =
            if direction == "Up" { "Down" } else { "Up" }.into();
        assert!(reconciliation::changes(&app, &record).unwrap().components.is_empty());
        app.context.config.payroll.rounding_direction = direction.into();

        let revise = |minutes| edit("2026-04-02T09:00", "2026-04-02T13:00", minutes);
        let same_bucket = match (direction, step) {
            (_, 1) => raw,
            ("Up", _) => raw + 1,
            _ => raw - 1,
        };
        change(&app, 1, &revise(same_bucket), true).unwrap();
        assert!(reconciliation::changes(&app, &record).unwrap().components.is_empty());
        change(&app, 1, &revise(raw + sign * step), true).unwrap();
        let proposed = reconciliation::changes(&app, &record).unwrap();
        assert_eq!(proposed.components.len(), 1);
        assert_eq!(proposed.components[0].minutes, sign * step);
        assert!(!proposed.requires_complete);
        reconciliation::carry(&app, &record, &proposed.signature, false).unwrap();
        let (next, next_weeks) = period(&app, 2, "2026-04-29", 1);
        let calculate_next = || reconciliation::calculate(&app, &next, &next_weeks, None).unwrap();
        assert_eq!(calculate_next().week_totals_minutes, [sign * step, 0, 0, 0]);
        assert_eq!(calculate_next().week_totals_minutes, [sign * step, 0, 0, 0]);
        submit(&app, &next, &next_weeks);
        app.context.config.payroll.rounding_direction =
            if direction == "Up" { "Down" } else { "Up" }.into();
        assert!(reconciliation::changes(&app, &record).unwrap().components.is_empty());
        let (later, later_weeks) = period(&app, 3, "2026-05-27", 1);
        assert_eq!(
            reconciliation::calculate(&app, &later, &later_weeks, None).unwrap().week_totals_minutes,
            [0; 4],
        );
        assert!(reconciliation::changes(&app, &record).unwrap().components.is_empty());
        assert_eq!(lifecycle::items(&db, submission).unwrap(), original);
    }
}

macro_rules! rounding_matrix_case {
    ($name:ident, $step:literal, $direction:literal) => {
        #[test]
        fn $name() {
            stage2b_rounding_matrix($step, $direction);
        }
    };
}

rounding_matrix_case!(stage2b_round_01_up, 1, "Up");
rounding_matrix_case!(stage2b_round_01_down, 1, "Down");
rounding_matrix_case!(stage2b_round_05_up, 5, "Up");
rounding_matrix_case!(stage2b_round_05_down, 5, "Down");
rounding_matrix_case!(stage2b_round_10_up, 10, "Up");
rounding_matrix_case!(stage2b_round_10_down, 10, "Down");
rounding_matrix_case!(stage2b_round_15_up, 15, "Up");
rounding_matrix_case!(stage2b_round_15_down, 15, "Down");
rounding_matrix_case!(stage2b_round_30_up, 30, "Up");
rounding_matrix_case!(stage2b_round_30_down, 30, "Down");
rounding_matrix_case!(stage2b_round_60_up, 60, "Up");
rounding_matrix_case!(stage2b_round_60_down, 60, "Down");


#[test]
fn stage2b_moved_shift_does_not_undo_accepted_downward_amendment() {
    let (dir, app) = setup();
    let (record, weeks) = period(&app, 1, "2026-04-01", 1);
    import(&app, &dir, ROW);
    submit(&app, &record, &weeks);
    save_amendment(&app, &record, 0.75);
    submit(&app, &record, &weeks);

    change(&app, 1, &edit("2026-04-10T09:00", "2026-04-10T10:00", 60), true).unwrap();
    assert!(reconciliation::changes(&app, &record).unwrap().components.is_empty());
    import(&app, &dir, "Example PA,11 April 2026 at 09:00:00,11 April 2026 at 09:53:00,0h 00m,0h 53m,£12,£12,new work");
    stage2b_review_separate_export_rows(&app);
    let (next, dates) = period(&app, 2, "2026-04-29", 1);
    assert_eq!(
        reconciliation::calculate(&app, &next, &dates, None).unwrap().week_totals_minutes,
        [60, 0, 0, 0],
    );
}

fn stage2b_review_separate_export_rows(app: &Application) {
    // The fixture reuses its export filename. The existing resolver must
    // explicitly confirm that both linked rows describe separate shifts.
    let db = open(app).unwrap();
    let evidence = load(app).unwrap();
    let unresolved = preflight(&db, &evidence).unwrap().unresolved;
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].candidates.len(), 2);
    approve_group(&db, &evidence, &[(unresolved[0].fingerprint.clone(), "separate".into())]).unwrap();
}
