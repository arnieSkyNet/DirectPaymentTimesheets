use super::lifecycle::Stage;
use super::*;
use crate::payroll_timesheet_repository::PayrollTimesheet;
use crate::payroll_worked_item_repository::SnapshotState;

fn e(source: &str, id: i64, start: &str, end: &str) -> WorkEvidence {
    WorkEvidence {
        source: source.into(),
        id,
        pa: 1,
        pa_name: "Test PA".into(),
        start: format!("2026-04-02T{start}:00"),
        end: format!("2026-04-02T{end}:00"),
        break_minutes: 0,
        minutes: 60,
        notes: String::new(),
        deleted: false,
    }
}
fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    crate::database::create_schema(&db).unwrap();
    db
}
#[test]
fn groups_cover_both_sources_same_source_touching_pas_and_transitivity() {
    for sources in [
        ["imported", "direct"],
        ["imported", "imported"],
        ["direct", "direct"],
    ] {
        let rows = [
            e(sources[0], 1, "08:50", "09:45"),
            e(sources[1], 2, "09:00", "09:50"),
        ];
        assert_eq!(groups(&rows).unwrap()[0].candidates.len(), 2);
    }
    let mut rows = vec![
        e("imported", 1, "08:00", "09:00"),
        e("direct", 2, "09:00", "10:00"),
    ];
    assert!(groups(&rows).unwrap().is_empty());
    rows[1].start = "2026-04-02T08:30:00".into();
    rows[1].pa = 2;
    assert!(groups(&rows).unwrap().is_empty());
    let rows = [
        e("imported", 1, "08:00", "09:00"),
        e("direct", 2, "08:30", "10:00"),
        e("direct", 3, "09:30", "11:00"),
    ];
    assert_eq!(groups(&rows).unwrap()[0].candidates.len(), 3);
    let rows = [
        e("imported", 1, "08:50", "09:45"),
        e("direct", 2, "09:00", "09:50"),
        e("imported", 3, "09:00", "09:45"),
        e("direct", 4, "08:50", "09:50"),
    ];
    assert_eq!(groups(&rows).unwrap()[0].candidates.len(), 4);
}
#[test]
fn resolution_is_one_winner_retains_candidates_and_invalidates_material_edits() {
    let db = db();
    let mut rows = vec![
        e("imported", 1, "09:00", "10:00"),
        e("direct", 2, "09:00", "10:00"),
    ];
    let g = preflight(&db, &rows).unwrap().unresolved.remove(0);
    assert!(resolve(&db, &rows, &[(g.fingerprint.clone(), "missing".into())]).is_err());
    resolve(&db, &rows, &[(g.fingerprint, rows[0].key())]).unwrap();
    for _ in 0..2 {
        let p = preflight(&db, &rows).unwrap();
        assert!(p.unresolved.is_empty());
        assert_eq!(p.eligible.len(), 1);
    }
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM payroll_duplicate_members", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        2
    );
    rows[1].notes = "Descriptive correction".into();
    assert!(preflight(&db, &rows).unwrap().unresolved.is_empty());
    rows[1].start = "2026-04-02T11:00:00".into();
    rows[1].end = "2026-04-02T12:00:00".into();
    let p = preflight(&db, &rows).unwrap();
    assert!(p.unresolved.is_empty());
    assert_eq!(p.eligible.len(), 2);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM payroll_duplicate_decisions WHERE invalidated_at IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    rows.push(e("direct", 3, "09:30", "10:30"));
    assert_eq!(preflight(&db, &rows).unwrap().unresolved.len(), 1);
}
fn setup() -> (tempfile::TempDir, Application) {
    let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
    let db = open(&app).unwrap();
    db.execute_batch("INSERT INTO personal_assistants(id,first_name,surname,employment_status) VALUES (1,'Example','PA','Active'),(2,'Other','PA','Active');
        INSERT INTO personal_assistant_pay_rates(personal_assistant_id,effective_date,base_hourly_rate,employer_top_up_rate,created_at) VALUES (1,'01/01/2025',12,0,'test'),(2,'01/01/2025',13,0,'test');").unwrap();
    (dir, app)
}
fn period(app: &Application, n: i64, start: &str, pa: i64) -> (PayrollTimesheet, [NaiveDate; 4]) {
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d").unwrap();
    let db = open(app).unwrap();
    db.execute("INSERT OR IGNORE INTO payroll_schedules(payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at,payslips_sent) VALUES ('2026/27',?1,?2,?2,?2,'test',0)",params![n,start.format("%d/%m/%Y").to_string()]).unwrap();
    let id = app
        .payroll_timesheet_repository
        .insert("2026/27", n, pa, None, "test")
        .unwrap();
    let dates = std::array::from_fn(|i| start + chrono::Duration::days(i as i64 * 7));
    app.payroll_timesheet_repository
        .create_missing_weeks(
            id,
            &dates.map(|d| d.format("%d/%m/%Y").to_string()),
            &[0.0; 4],
        )
        .unwrap();
    (
        app.payroll_timesheet_repository
            .get_for_cycle_and_pa("2026/27", n, pa)
            .unwrap()
            .unwrap(),
        dates,
    )
}
fn direct(app: &Application, start: &str, end: &str) -> i64 {
    let start = crate::direct_shift_repository::parse_shift_time(start).unwrap();
    let end = crate::direct_shift_repository::parse_shift_time(end).unwrap();
    let shift = app
        .direct_shift_repository
        .clock_in(1, start, "test")
        .unwrap();
    app.direct_shift_repository
        .complete(shift.id, end, 0, Some("actual"), "test")
        .unwrap();
    shift.id
}
fn candidate(
    app: &Application,
    r: &PayrollTimesheet,
    weeks: &[NaiveDate; 4],
) -> std::path::PathBuf {
    let hours = reconciliation::calculate(app, r, weeks, None).unwrap();
    let path = app
        .context
        .environment
        .data_dir
        .join(format!("test-{}.pdf", r.id));
    std::fs::write(&path, b"%PDF-test").unwrap();
    let ids = app
        .payroll_timesheet_repository
        .get_weeks(r.id)
        .unwrap()
        .iter()
        .map(|w| w.id)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    app.payroll_worked_item_repository
        .replace_candidate(
            r.id,
            &hours.snapshot_items,
            path.to_str().unwrap(),
            &crate::payroll_snapshot_service::sha256_file(&path).unwrap(),
            "test",
            hours.previous_cycle_minutes,
            &ids,
            &hours.week_totals_minutes,
        )
        .unwrap();
    path
}
fn submit(app: &Application, r: &PayrollTimesheet, weeks: &[NaiveDate; 4]) {
    let path = candidate(app, r, weeks);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.payroll_worked_item_repository,
        r.id,
        r.personal_assistant_id,
        &r.payroll_year,
        r.cycle_number,
        &path,
        "2026-04-15T10:00:00Z",
        || Ok(()),
    )
    .unwrap();
}
fn settle(app: &Application, r: &PayrollTimesheet) {
    app.payroll_timesheet_email_repository
        .mark_sent(
            r.personal_assistant_id,
            &r.payroll_year,
            r.cycle_number,
            "payslip",
            "2026-04-30T10:00:00Z",
        )
        .unwrap();
}
#[test]
fn completed_direct_flows_into_preparation_running_deleted_and_other_pas_do_not() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let deleted = direct(&app, "2026-04-03T09:00", "2026-04-03T10:00");
    app.direct_shift_repository
        .soft_delete_completed(deleted, "test")
        .unwrap();
    app.direct_shift_repository
        .clock_in(
            1,
            crate::direct_shift_repository::parse_shift_time("2026-04-04T09:00").unwrap(),
            "test",
        )
        .unwrap();
    let result = reconciliation::calculate(&app, &r, &w, None).unwrap();
    assert_eq!(result.week_totals_minutes, [60, 0, 0, 0]);
    assert_eq!(result.snapshot_items[0].direct_shift_id, Some(id));
    assert_eq!(result.snapshot_items[0].timesheet_id, None);
    assert_eq!(app.repository.get_all_raw().unwrap().len(), 0);
}
#[test]
fn reopening_stored_evidence_sees_new_internal_work_without_import() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [60, 0, 0, 0]
    );
    direct(&app, "2026-04-02T09:30", "2026-04-02T10:30");
    assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
    assert_eq!(
        open(&app)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM import_audit", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn submitted_then_explicit_resubmission_keeps_original_pdf_and_items() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    open(&app)
        .unwrap()
        .execute(
            "UPDATE payroll_timesheets SET payroll_department_notes=?1 WHERE id=?2",
            params!["Original payroll note\nPreserved", r.id],
        )
        .unwrap();
    submit(&app, &r, &w);
    let db = open(&app).unwrap();
    let first = lifecycle::latest(&db, r.id).unwrap().unwrap();
    app.direct_shift_repository
        .edit_completed(
            id,
            crate::direct_shift_repository::parse_shift_time("2026-04-02T09:00").unwrap(),
            crate::direct_shift_repository::parse_shift_time("2026-04-02T11:00").unwrap(),
            0,
            None,
            "changed",
        )
        .unwrap();
    assert_eq!(lifecycle::stage(&db, &r).unwrap(), Stage::Submitted);
    assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
    assert_eq!(lifecycle::items(&db, first).unwrap()[0].worked_minutes, 60);
    let change = reconciliation::changes(&app, &r).unwrap();
    assert!(change.changed);
    lifecycle::authorize_resubmission(&app, &r, &change.signature).unwrap();
    db.execute(
        "UPDATE payroll_timesheets SET payroll_department_notes=?1 WHERE id=?2",
        params!["Replacement payroll note", r.id],
    )
    .unwrap();
    submit(&app, &r, &w);
    let second = lifecycle::latest(&db, r.id).unwrap().unwrap();
    assert_ne!(first, second);
    let note = |id| {
        db.query_row::<String, _, _>(
            "SELECT payroll_department_notes FROM payroll_submissions WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(note(first), "Original payroll note\nPreserved");
    assert_eq!(note(second), "Replacement payroll note");
    assert_eq!(lifecycle::items(&db, first).unwrap()[0].worked_minutes, 60);
    assert_eq!(
        lifecycle::items(&db, second).unwrap()[0].worked_minutes,
        120
    );
    assert_eq!(
        db.query_row(
            "SELECT supersedes_id FROM payroll_submissions WHERE id=?1",
            [second],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        first
    );
    assert_eq!(
        db.query_row(
            "SELECT pdf_bytes FROM payroll_submissions WHERE id=?1",
            [first],
            |r| r.get::<_, Vec<u8>>(0)
        )
        .unwrap(),
        b"%PDF-test"
    );
}
#[test]
fn carry_is_explicit_preserves_submission_and_positive_persists_until_submitted() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    submit(&app, &r, &w);
    direct(&app, "2026-04-02T11:00", "2026-04-02T12:00");
    let (next, nw) = period(&app, 2, "2026-04-29", 1);
    assert_eq!(
        reconciliation::calculate(&app, &next, &nw, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
    let change = reconciliation::changes(&app, &r).unwrap();
    reconciliation::carry(&app, &r, &change.signature, false).unwrap();
    for _ in 0..2 {
        assert_eq!(
            reconciliation::calculate(&app, &next, &nw, None)
                .unwrap()
                .week_totals_minutes,
            [60, 0, 0, 0]
        );
    }
    assert_eq!(
        app.payroll_worked_item_repository
            .get_snapshot_items(r.id)
            .unwrap()[0]
            .worked_minutes,
        60
    );
    submit(&app, &next, &nw);
    settle(&app, &next);
    let (third, tw) = period(&app, 3, "2026-05-27", 1);
    assert_eq!(
        reconciliation::calculate(&app, &third, &tw, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
}
#[test]
fn settled_aggregate_historical_shift_requires_paid_unpaid_review() {
    for paid in [true, false] {
        let (_dir, app) = setup();
        let (old, _) = period(&app, 1, "2026-04-01", 1);
        settle(&app, &old);
        direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
        let (next, w) = period(&app, 5, "2026-07-22", 1);
        assert!(reconciliation::historical_save_notice(
            &app,
            1,
            NaiveDate::from_ymd_opt(2026, 4, 2).unwrap()
        )
        .unwrap()
        .is_some());
        let p = reconciliation::plan(&app, &next).unwrap();
        assert_eq!(p.historical_reviews.len(), 1);
        assert!(p.work.is_empty());
        reconciliation::historical_decision(&app, &p.historical_reviews[0], paid).unwrap();
        let result = reconciliation::calculate(&app, &next, &w, None).unwrap();
        assert_eq!(result.week_totals_minutes[0], if paid { 0 } else { 60 });
        if !paid {
            assert_eq!(
                result.snapshot_items[0].work_date.as_deref(),
                Some("2026-04-02")
            );
            assert_eq!(result.snapshot_items[0].total_hourly_rate, Some(12.0));
        }
        let (third, tw) = period(&app, 6, "2026-08-19", 1);
        assert_eq!(
            reconciliation::calculate(&app, &third, &tw, None)
                .unwrap()
                .week_totals_minutes[0],
            if paid { 0 } else { 60 }
        );
    }
}
#[test]
fn payslip_settlement_is_per_pa_and_paid_evidence_never_reappears() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let (other, _) = period(&app, 1, "2026-04-01", 2);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    submit(&app, &r, &w);
    settle(&app, &r);
    let db = open(&app).unwrap();
    assert_eq!(lifecycle::stage(&db, &r).unwrap(), Stage::Settled);
    assert_eq!(lifecycle::stage(&db, &other).unwrap(), Stage::Editable);
    let (next, nw) = period(&app, 6, "2026-08-19", 1);
    assert_eq!(
        reconciliation::calculate(&app, &next, &nw, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
    assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
}
#[test]
fn aggregate_negative_requires_audited_completeness_and_zero_floor_remainder_persists() {
    let (_dir, app) = setup();
    let (old, _) = period(&app, 1, "2026-04-01", 1);
    settle(&app, &old);
    open(&app).unwrap().execute("UPDATE payroll_timesheet_weeks SET worked_hours=3 WHERE payroll_timesheet_id=?1 AND week_number=1",[old.id]).unwrap();
    let change = reconciliation::changes(&app, &old).unwrap();
    assert!(change.requires_complete);
    assert!(reconciliation::carry(&app, &old, &change.signature, false).is_err());
    reconciliation::carry(&app, &old, &change.signature, true).unwrap();
    let (next, nw) = period(&app, 2, "2026-04-29", 1);
    direct(&app, "2026-04-30T09:00", "2026-04-30T11:00");
    let result = reconciliation::calculate(&app, &next, &nw, None).unwrap();
    assert_eq!(result.week_totals_minutes, [0; 4]);
    assert_eq!(result.previous_cycle_minutes, -120);
    submit(&app, &next, &nw);
    settle(&app, &next);
    let (third, tw) = period(&app, 3, "2026-05-27", 1);
    let outstanding = reconciliation::outstanding(&app, &third).unwrap();
    assert_eq!(outstanding[0].minutes, -60);
    assert_eq!(
        reconciliation::calculate(&app, &third, &tw, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
    assert_eq!(
        reconciliation::outstanding(&app, &third).unwrap()[0].minutes,
        -60
    );
    assert_eq!(open(&app).unwrap().query_row("SELECT COUNT(*) FROM payroll_reconciliation_decisions WHERE kind='complete_actuals'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}
#[test]
fn concrete_settled_edit_creates_difference_without_blanket_completeness() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T12:00");
    submit(&app, &r, &w);
    settle(&app, &r);
    app.direct_shift_repository
        .edit_completed(
            id,
            crate::direct_shift_repository::parse_shift_time("2026-04-02T09:00").unwrap(),
            crate::direct_shift_repository::parse_shift_time("2026-04-02T10:00").unwrap(),
            0,
            None,
            "changed",
        )
        .unwrap();
    let (next, nw) = period(&app, 2, "2026-04-29", 1);
    reconciliation::calculate(&app, &next, &nw, None).unwrap();
    assert_eq!(
        reconciliation::outstanding(&app, &next).unwrap()[0].minutes,
        -120
    );
    assert_eq!(open(&app).unwrap().query_row("SELECT COUNT(*) FROM payroll_reconciliation_decisions WHERE kind='complete_actuals'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}
#[test]
fn separate_corrections_consolidate_without_losing_components() {
    let cs = [120, -180, -60].map(|minutes| reconciliation::CorrectionApplication {
        id: minutes,
        minutes,
        date: None,
        rate_id: None,
        rate_date: None,
        rate: None,
        reason: "test".into(),
    });
    let mut totals = [60, 0, 0, 0];
    let applied = reconciliation::apply_corrections(&mut totals, &cs).unwrap();
    assert_eq!(totals, [0; 4]);
    assert_eq!(applied, vec![(120, 120), (-180, -180)]);
    assert_eq!(cs[2].minutes, -60);
}
#[test]
fn notes_only_does_not_invalidate_candidate_but_new_shift_does() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let path = candidate(&app, &r, &w);
    app.direct_shift_repository
        .edit_completed(
            id,
            crate::direct_shift_repository::parse_shift_time("2026-04-02T09:00").unwrap(),
            crate::direct_shift_repository::parse_shift_time("2026-04-02T10:00").unwrap(),
            0,
            Some("notes only"),
            "changed",
        )
        .unwrap();
    crate::payroll_snapshot_service::verify_candidate(
        &app.payroll_worked_item_repository,
        r.id,
        &path,
    )
    .unwrap();
    let recalculated = reconciliation::calculate(&app, &r, &w, None).unwrap();
    assert!(lifecycle::payroll_items_equal(
        &app.payroll_worked_item_repository
            .get_snapshot_items(r.id)
            .unwrap(),
        &recalculated.snapshot_items
    ));
    direct(&app, "2026-04-03T09:00", "2026-04-03T10:00");
    assert!(crate::payroll_snapshot_service::verify_candidate(
        &app.payroll_worked_item_repository,
        r.id,
        &path
    )
    .is_err());
    assert_eq!(
        app.payroll_worked_item_repository
            .snapshot_metadata(r.id)
            .unwrap()
            .unwrap()
            .state,
        SnapshotState::Candidate
    );
}

#[test]
fn migration_28_preserves_legacy_submission_facts_and_rolls_back_on_failure() {
    let db = db();
    crate::database::tests::remove_schema_28_fixture(&db);
    db.execute_batch("INSERT INTO payroll_timesheets(id,personal_assistant_id,payroll_year,cycle_number,created_at,updated_at) VALUES (1,1,'2026/27',1,'old','old');
        INSERT INTO payroll_timesheet_snapshot_states(payroll_timesheet_id,state,pdf_path,pdf_sha256,generated_at,submitted_at) VALUES (1,'submitted','retained.pdf','retained-digest','generated','submitted');
        INSERT INTO payroll_timesheet_worked_item_snapshots(id,payroll_timesheet_id,week_number,source_type,timesheet_id,work_date,worked_minutes,pay_rate_id,pay_rate_effective_date,total_hourly_rate,captured_at) VALUES (9,1,1,'imported_shift',42,'2026-04-02',90,1,'01/01/2026',12,'captured');
        CREATE TABLE payroll_duplicate_decisions(block INTEGER);").unwrap();
    assert!(migrate(&db).is_err());
    assert_eq!(
        db.query_row("SELECT version FROM schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        27
    );
    assert_eq!(
        db.query_row(
            "SELECT worked_minutes FROM payroll_timesheet_worked_item_snapshots WHERE id=9",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        90
    );
    db.execute("DROP TABLE payroll_duplicate_decisions", [])
        .unwrap();
    migrate(&db).unwrap();
    crate::database::create_schema(&db).unwrap();
    assert_eq!(
        db.query_row("SELECT version FROM schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        crate::database::CURRENT_SCHEMA_VERSION
    );
    let sid = lifecycle::latest(&db, 1).unwrap().unwrap();
    let items = lifecycle::items(&db, sid).unwrap();
    assert_eq!(items[0].timesheet_id, Some(42));
    assert_eq!(items[0].source_evidence, None);
    assert_eq!(items[0].direct_shift_id, None);
    assert_eq!(
        db.query_row("SELECT submitted_at FROM payroll_submissions", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "submitted"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'payroll_timesheet_revision%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn paid_duplicate_replacement_reconciles_only_the_difference() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    submit(&app, &r, &w);
    settle(&app, &r);
    let replacement = direct(&app, "2026-04-02T09:00", "2026-04-02T11:00");
    let all = load(&app).unwrap();
    let g = preflight(&open(&app).unwrap(), &all)
        .unwrap()
        .unresolved
        .remove(0);
    resolve(
        &open(&app).unwrap(),
        &all,
        &[(g.fingerprint, format!("direct:{replacement}"))],
    )
    .unwrap();
    let (next, nw) = period(&app, 2, "2026-04-29", 1);
    let result = reconciliation::calculate(&app, &next, &nw, None).unwrap();
    assert_eq!(result.week_totals_minutes, [60, 0, 0, 0]);
    assert_eq!(reconciliation::outstanding(&app, &next).unwrap().len(), 2);
}
#[test]
fn mixed_imported_and_direct_selected_evidence_is_paid_once_with_historical_rates() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    app.repository
        .insert(&crate::models::TimesheetEntry {
            id: 0,
            pa_name: "Example PA".into(),
            personal_assistant_id: Some(1),
            start_time: "02 April 2026 at 09:00:00".into(),
            end_time: "02 April 2026 at 10:00:00".into(),
            break_minutes: 0,
            worked_minutes: 60,
            hourly_rate: 99.0,
            amount: 99.0,
            notes: Some("CSV evidence".into()),
        })
        .unwrap();
    let initial = reconciliation::calculate(&app, &r, &w, None).unwrap();
    assert_eq!(initial.week_totals_minutes, [60, 0, 0, 0]);
    assert_eq!(initial.snapshot_items[0].total_hourly_rate, Some(12.0));
    direct(&app, "2026-04-03T09:00", "2026-04-03T10:00");
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [120, 0, 0, 0]
    );
    direct(&app, "2026-04-02T09:30", "2026-04-02T10:30");
    let all = load(&app).unwrap();
    let group = preflight(&open(&app).unwrap(), &all)
        .unwrap()
        .unresolved
        .remove(0);
    resolve(
        &open(&app).unwrap(),
        &all,
        &[(
            group.fingerprint,
            format!("imported:{}", app.repository.get_all_raw().unwrap()[0].id),
        )],
    )
    .unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &r, &w, None)
            .unwrap()
            .week_totals_minutes,
        [120, 0, 0, 0]
    );
    assert_eq!(load(&app).unwrap().len(), 3);
}
#[test]
fn missing_historical_rate_is_not_fabricated() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    open(&app)
        .unwrap()
        .execute("DELETE FROM personal_assistant_pay_rates", [])
        .unwrap();
    assert!(reconciliation::calculate(&app, &r, &w, None)
        .unwrap_err()
        .to_string()
        .contains("No pay rate"));
}
#[test]
fn aggregate_carry_retains_actual_membership_without_adding_full_shift_again() {
    let (_dir, app) = setup();
    let (old, _) = period(&app, 1, "2026-04-01", 1);
    settle(&app, &old);
    open(&app).unwrap().execute("UPDATE payroll_timesheet_weeks SET worked_hours=1 WHERE payroll_timesheet_id=?1 AND week_number=1",[old.id]).unwrap();
    direct(&app, "2026-04-02T09:00", "2026-04-02T11:00");
    let change = reconciliation::changes(&app, &old).unwrap();
    reconciliation::carry(&app, &old, &change.signature, false).unwrap();
    let (next, nw) = period(&app, 2, "2026-04-29", 1);
    let result = reconciliation::calculate(&app, &next, &nw, None).unwrap();
    assert_eq!(result.week_totals_minutes, [60, 0, 0, 0]);
    assert!(reconciliation::plan(&app, &next)
        .unwrap()
        .historical_reviews
        .is_empty());
}
#[test]
fn two_open_candidates_cannot_submit_the_same_historical_shift_twice() {
    let (_dir, app) = setup();
    let (first, fw) = period(&app, 2, "2026-04-29", 1);
    let (second, sw) = period(&app, 3, "2026-05-27", 1);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let stale = candidate(&app, &second, &sw);
    submit(&app, &first, &fw);
    assert!(crate::payroll_snapshot_service::verify_candidate(
        &app.payroll_worked_item_repository,
        second.id,
        &stale
    )
    .is_err());
}

#[test]
fn direct_preparation_manual_adjustment_and_carry_still_reconcile_final_hours() {
    let (_dir, app) = setup();
    let (old, _) = period(&app, 1, "2026-04-01", 1);
    settle(&app, &old);
    direct(&app, "2026-04-02T09:00", "2026-04-02T12:00");
    let change = reconciliation::changes(&app, &old).unwrap();
    reconciliation::carry(&app, &old, &change.signature, false).unwrap();
    let (next, nw) = period(&app, 2, "2026-04-29", 1);
    direct(&app, "2026-04-30T09:00", "2026-04-30T11:00");
    app.payroll_worked_item_repository
        .set_manual_adjustment(
            next.id,
            &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number: 1,
                adjustment_minutes: -240,
                reason: Some("Explicit preparation correction".into()),
            },
            "test",
        )
        .unwrap();
    let result = reconciliation::calculate(&app, &next, &nw, None).unwrap();
    assert_eq!(result.week_totals_minutes, [60, 0, 0, 0]);
    for week in 1..=4 {
        assert_eq!(
            result
                .snapshot_items
                .iter()
                .filter(|i| i.week_number == week)
                .map(|i| i.worked_minutes)
                .sum::<i64>(),
            result.week_totals_minutes[(week - 1) as usize]
        );
    }
}
#[test]
fn pending_replacement_keeps_original_membership_reserved_elsewhere() {
    let (_dir, app) = setup();
    let (old, ow) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    submit(&app, &old, &ow);
    app.direct_shift_repository
        .edit_completed(
            id,
            crate::direct_shift_repository::parse_shift_time("2026-04-02T09:00").unwrap(),
            crate::direct_shift_repository::parse_shift_time("2026-04-02T11:00").unwrap(),
            0,
            None,
            "test",
        )
        .unwrap();
    let change = reconciliation::changes(&app, &old).unwrap();
    lifecycle::authorize_resubmission(&app, &old, &change.signature).unwrap();
    let (next, nw) = period(&app, 2, "2026-04-29", 1);
    assert_eq!(
        reconciliation::calculate(&app, &next, &nw, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
}
#[test]
fn indeterminate_payslip_is_not_settled_and_refuses_reconciliation() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    app.payroll_timesheet_email_repository
        .protect_payslip_for_send(1, "2026/27", 1, "attempt")
        .unwrap();
    assert_eq!(
        lifecycle::stage(&open(&app).unwrap(), &r).unwrap(),
        Stage::Indeterminate
    );
    assert!(reconciliation::calculate(&app, &r, &w, None).is_err());
}

#[test]
fn individual_then_aggregate_negative_differences_are_not_counted_twice() {
    let (_dir, app) = setup();
    let (r, w) = period(&app, 1, "2026-04-01", 1);
    let id = direct(&app, "2026-04-02T09:00", "2026-04-02T11:00");
    app.payroll_worked_item_repository
        .set_manual_adjustment(
            r.id,
            &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number: 1,
                adjustment_minutes: 180,
                reason: Some("Estimated portion".into()),
            },
            "test",
        )
        .unwrap();
    submit(&app, &r, &w);
    settle(&app, &r);
    app.direct_shift_repository
        .edit_completed(
            id,
            crate::direct_shift_repository::parse_shift_time("2026-04-02T09:00").unwrap(),
            crate::direct_shift_repository::parse_shift_time("2026-04-02T10:00").unwrap(),
            0,
            None,
            "changed",
        )
        .unwrap();
    reconciliation::sync_settled_corrections(&app, 1).unwrap();
    let change = reconciliation::changes(&app, &r).unwrap();
    assert_eq!(
        change
            .components
            .iter()
            .filter(|c| c.aggregate)
            .map(|c| c.minutes)
            .sum::<i64>(),
        -180
    );
    reconciliation::carry(&app, &r, &change.signature, true).unwrap();
    let (next, _) = period(&app, 2, "2026-04-29", 1);
    assert_eq!(
        reconciliation::outstanding(&app, &next)
            .unwrap()
            .iter()
            .map(|c| c.minutes)
            .sum::<i64>(),
        -240
    );
}

#[test]
fn retained_legacy_import_membership_uses_matching_original_intervals_without_fabrication() {
    let (_dir, app) = setup();
    let (old, w) = period(&app, 1, "2026-04-01", 1);
    app.repository
        .insert(&crate::models::TimesheetEntry {
            id: 0,
            pa_name: "Example PA".into(),
            personal_assistant_id: Some(1),
            start_time: "02 April 2026 at 09:00:00".into(),
            end_time: "02 April 2026 at 10:00:00".into(),
            break_minutes: 0,
            worked_minutes: 60,
            hourly_rate: 12.0,
            amount: 12.0,
            notes: None,
        })
        .unwrap();
    submit(&app, &old, &w);
    settle(&app, &old);
    let db = open(&app).unwrap();
    db.execute(
        "UPDATE payroll_submission_items SET source_evidence=NULL",
        [],
    )
    .unwrap();
    db.execute(
        "UPDATE payroll_timesheet_worked_item_snapshots SET source_evidence=NULL",
        [],
    )
    .unwrap();
    direct(&app, "2026-04-03T09:00", "2026-04-03T10:00");
    let (next, nw) = period(&app, 2, "2026-04-29", 1);
    assert!(reconciliation::plan(&app, &next)
        .unwrap()
        .historical_reviews
        .is_empty());
    assert_eq!(
        reconciliation::calculate(&app, &next, &nw, None)
            .unwrap()
            .week_totals_minutes,
        [60, 0, 0, 0]
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM payroll_submission_items WHERE source_evidence IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

fn restore_schema_28(db: &Connection) {
    crate::database::tests::remove_schema_32_fixture(db);
    db.execute_batch("DROP TABLE payroll_legacy_cutover; DROP TABLE payroll_legacy_evidence; DROP TABLE payroll_legacy_settlements; UPDATE schema_version SET version=28;").unwrap();
}
fn write_cutover(
    app: &Application,
    evidence: Vec<WorkEvidence>,
    settled: Vec<legacy_baseline::Settlement>,
) {
    let inventory = legacy_baseline::Inventory {
        provenance: "Operator-verified pre-cutover inventory; no migration timestamp inferred"
            .into(),
        evidence,
        settled,
    };
    std::fs::write(
        legacy_baseline::inventory_path(&open(app).unwrap())
            .unwrap()
            .unwrap(),
        toml::to_string(&inventory).unwrap(),
    )
    .unwrap();
}
#[test]
fn migration_29_repairs_live_shape_preserves_duplicate_and_excludes_post_cutover_six() {
    let (_dir, app) = setup();
    let (old, _) = period(&app, 1, "2026-04-01", 1);
    settle(&app, &old);
    let (next, _) = period(&app, 5, "2026-07-22", 1);
    for day in 2..=7 {
        direct(
            &app,
            &format!("2026-04-{day:02}T09:00"),
            &format!("2026-04-{day:02}T10:00"),
        );
    }
    let db = open(&app).unwrap();
    // Both ordinary and duplicate candidates remain original evidence.
    db.execute("UPDATE direct_shifts SET start_time='2026-04-02T09:15',end_time='2026-04-02T10:15' WHERE id=2",[]).unwrap();
    let all = load(&app).unwrap();
    let group = groups(&all).unwrap().remove(0);
    resolve(&db, &all, &[(group.fingerprint.clone(), all[0].key())]).unwrap();
    let decisions: String = db
        .query_row(
            "SELECT decided_at FROM payroll_duplicate_decisions",
            [],
            |r| r.get(0),
        )
        .unwrap();
    restore_schema_28(&db);
    write_cutover(
        &app,
        all.into_iter().filter(|e| e.id <= 5).collect(),
        vec![legacy_baseline::Settlement {
            record: old.id,
            sent_at: "2026-04-30T10:00:00Z".into(),
        }],
    );
    crate::database::create_schema(&db).unwrap();
    drop(db);
    // Restart/idempotent migration does not absorb later evidence or reset choices.
    let db = open(&app).unwrap();
    crate::database::create_schema(&db).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM payroll_legacy_evidence", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        db.query_row(
            "SELECT decided_at FROM payroll_duplicate_decisions WHERE invalidated_at IS NULL",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        decisions
    );
    assert!(preflight(&db, &load(&app).unwrap())
        .unwrap()
        .unresolved
        .is_empty());
    let plan = reconciliation::plan(&app, &next).unwrap();
    assert!(plan.work.is_empty());
    assert_eq!(plan.historical_reviews.len(), 1);
    assert_eq!(plan.historical_reviews[0].evidence.id, 6);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM payroll_reconciliation_decisions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM payroll_corrections", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM payroll_submission_items", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    // Baseline does not disable duplicate review; new overlapping evidence reopens it.
    direct(&app, "2026-04-02T09:30", "2026-04-02T10:30");
    assert_eq!(
        preflight(&db, &load(&app).unwrap())
            .unwrap()
            .unresolved
            .len(),
        1
    );
}
#[test]
fn migration_29_pre_cutover_unsettled_evidence_is_not_paid_and_later_settlement_is_not_baselined() {
    let (_dir, app) = setup();
    let (old, _) = period(&app, 1, "2026-04-01", 1);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let (next, _) = period(&app, 5, "2026-07-22", 1);
    let db = open(&app).unwrap();
    restore_schema_28(&db);
    write_cutover(&app, load(&app).unwrap(), vec![]);
    crate::database::create_schema(&db).unwrap();
    assert_eq!(reconciliation::plan(&app, &next).unwrap().work.len(), 1);
    settle(&app, &old);
    assert_eq!(
        reconciliation::plan(&app, &next)
            .unwrap()
            .historical_reviews
            .len(),
        1
    );
}
#[test]
fn migration_29_invalid_inventory_rolls_back_and_unknown_28_never_guesses() {
    let (_dir, app) = setup();
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let db = open(&app).unwrap();
    restore_schema_28(&db);
    let mut evidence = load(&app).unwrap();
    evidence[0].minutes += 1;
    write_cutover(&app, evidence, vec![]);
    assert!(crate::database::create_schema(&db).is_err());
    assert_eq!(
        db.query_row("SELECT version FROM schema_version", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        28
    );
    std::fs::remove_file(legacy_baseline::inventory_path(&db).unwrap().unwrap()).unwrap();
    crate::database::create_schema(&db).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM payroll_legacy_evidence", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn migration_29_pre_28_upgrade_captures_inventory_once_and_material_edits_reassess() {
    let (_dir, app) = setup();
    let (old, _) = period(&app, 1, "2026-04-01", 1);
    settle(&app, &old);
    direct(&app, "2026-04-02T09:00", "2026-04-02T10:00");
    let (next, _) = period(&app, 5, "2026-07-22", 1);
    let db = open(&app).unwrap();
    crate::database::tests::remove_schema_28_fixture(&db);
    crate::database::create_schema(&db).unwrap();
    assert!(reconciliation::plan(&app, &next)
        .unwrap()
        .historical_reviews
        .is_empty());
    db.execute(
        "UPDATE direct_shifts SET notes='descriptive only' WHERE id=1",
        [],
    )
    .unwrap();
    assert!(reconciliation::plan(&app, &next)
        .unwrap()
        .historical_reviews
        .is_empty());
    db.execute(
        "UPDATE direct_shifts SET end_time='2026-04-02T11:00' WHERE id=1",
        [],
    )
    .unwrap();
    assert_eq!(
        reconciliation::plan(&app, &next)
            .unwrap()
            .historical_reviews
            .len(),
        1
    );
}

#[test]
fn migration_29_imported_baseline_preserves_totals_but_later_import_requires_review() {
    let (_dir, app) = setup();
    let (old, _) = period(&app, 1, "2026-04-01", 1);
    settle(&app, &old);
    let (next, _) = period(&app, 5, "2026-07-22", 1);
    let db = open(&app).unwrap();
    db.execute(
        "UPDATE payroll_timesheet_weeks SET worked_hours=17.5 WHERE payroll_timesheet_id=?1",
        [old.id],
    )
    .unwrap();
    db.execute_batch("INSERT INTO timesheets (id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount,notes) VALUES (1,1,'Example PA','2026-04-02T09:00','2026-04-02T10:00',0,60,12,12,'original');").unwrap();
    restore_schema_28(&db);
    write_cutover(
        &app,
        load(&app).unwrap(),
        vec![legacy_baseline::Settlement {
            record: old.id,
            sent_at: "2026-04-30T10:00:00Z".into(),
        }],
    );
    crate::database::create_schema(&db).unwrap();
    let plan = reconciliation::plan(&app, &next).unwrap();
    assert!(plan.historical_reviews.is_empty());
    assert!(plan.work.is_empty());
    reconciliation::sync_settled_corrections(&app, 1).unwrap();
    assert!(reconciliation::changes(&app, &old)
        .unwrap()
        .components
        .is_empty());
    assert_eq!(
        db.query_row(
            "SELECT SUM(worked_hours) FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=?1",
            [old.id],
            |r| r.get::<_, f64>(0)
        )
        .unwrap(),
        70.0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM payroll_corrections", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("INSERT INTO timesheets (id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount) VALUES (2,1,'Example PA','2026-04-03T09:00','2026-04-03T10:00',0,60,12,12);").unwrap();
    crate::database::create_schema(&db).unwrap();
    let plan = reconciliation::plan(&app, &next).unwrap();
    assert_eq!(plan.historical_reviews.len(), 1);
    assert_eq!(plan.historical_reviews[0].evidence.key(), "imported:2");
}

fn rounding_sources(app: &Application) {
    direct(app, "2026-04-02T19:26", "2026-04-02T20:00");
    open(app).unwrap().execute_batch("INSERT INTO timesheets (id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount) VALUES (1,1,'Example PA','2026-04-02T20:00','2026-04-02T20:34',0,34,12,6.8);").unwrap();
}
#[test]
fn shared_rounding_preserves_both_raw_sources_and_exact_duplicate_intervals() {
    for (increment, direction, expected) in [
        (15, "Up", 45),
        (15, "Down", 30),
        (10, "Up", 40),
        (10, "Down", 30),
        (1, "Up", 34),
    ] {
        for late in [false, true] {
            let (_dir, mut app) = setup();
            app.context.config.payroll.rounding_minutes = increment;
            app.context.config.payroll.rounding_direction = direction.into();
            rounding_sources(&app);
            let raw = load(&app).unwrap();
            assert!(raw.iter().all(|e| e.minutes == 34));
            // Rounding the first duration to 45 minutes must not create an overlap.
            assert!(groups(&raw).unwrap().is_empty());
            let (record, weeks) = period(
                &app,
                if late { 5 } else { 1 },
                if late { "2026-07-22" } else { "2026-04-01" },
                1,
            );
            let result = reconciliation::calculate(&app, &record, &weeks, None).unwrap();
            assert_eq!(result.week_totals_minutes, [2 * expected, 0, 0, 0]);
            assert_eq!(
                result.previous_cycle_minutes,
                if late { 2 * expected } else { 0 }
            );
            assert_eq!(result.snapshot_items.len(), 2);
            for item in result.snapshot_items {
                assert_eq!(item.worked_minutes, expected);
                let source: WorkEvidence =
                    toml::from_str(item.source_evidence.as_deref().unwrap()).unwrap();
                assert_eq!(source.minutes, 34);
                assert!(raw.contains(&source));
            }
            assert_eq!(load(&app).unwrap(), raw);
            // A genuine one-minute overlap is still a duplicate even with Down rounding.
            let mut overlapping = raw.clone();
            overlapping[0].start = "2026-04-02T19:59:00".into();
            assert_eq!(groups(&overlapping).unwrap().len(), 1);
        }
    }
}
#[test]
fn rounding_changes_do_not_recalculate_submitted_settled_or_correction_history() {
    let (_dir, mut app) = setup();
    rounding_sources(&app);
    let (old, weeks) = period(&app, 1, "2026-04-01", 1);
    app.context.config.payroll.rounding_minutes = 15;
    app.context.config.payroll.rounding_direction = "Up".into();
    submit(&app, &old, &weeks);
    let db = open(&app).unwrap();
    let sid = lifecycle::latest(&db, old.id).unwrap().unwrap();
    let submitted = lifecycle::items(&db, sid).unwrap();
    assert!(submitted.iter().all(|i| i.worked_minutes == 45));
    assert!(reconciliation::changes(&app, &old)
        .unwrap()
        .components
        .is_empty());
    app.context.config.payroll.rounding_direction = "Down".into();
    assert!(reconciliation::calculate(&app, &old, &weeks, None).is_err());
    assert!(reconciliation::changes(&app, &old)
        .unwrap()
        .components
        .is_empty());
    settle(&app, &old);
    assert!(reconciliation::calculate(&app, &old, &weeks, None).is_err());
    let (next, nw) = period(&app, 5, "2026-07-22", 1);
    assert_eq!(
        reconciliation::calculate(&app, &next, &nw, None)
            .unwrap()
            .week_totals_minutes,
        [0; 4]
    );
    assert_eq!(lifecycle::items(&db, sid).unwrap(), submitted);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM payroll_corrections", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // Existing corrections are already payable values, not new raw shifts.
    db.execute("INSERT INTO payroll_corrections(personal_assistant_id,origin_payroll_timesheet_id,evidence_key,minutes,reason,created_at) VALUES (1,?1,'retained-correction',7,'retained','original')",[old.id]).unwrap();
    assert_eq!(
        reconciliation::calculate(&app, &next, &nw, None)
            .unwrap()
            .week_totals_minutes,
        [7, 0, 0, 0]
    );
    assert_eq!(
        db.query_row("SELECT minutes FROM payroll_corrections", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        7
    );
}
