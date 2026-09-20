use super::*;
use rusqlite::{params, Connection};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

fn fixture() -> (tempfile::TempDir, DirectPaymentApp, PayrollSchedule) {
    let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
    app.context.config.folders.pdf_output = dir.path().join("pdfs");
    let db = crate::payroll_evidence::open(&app).unwrap();
    db.execute_batch("INSERT INTO employers(id,name,email) VALUES (1,'Test Employer','employer@example.test');
        INSERT INTO payroll_provider(id,name,payroll_department_email) VALUES (1,'Payroll','payroll@example.test');
        INSERT INTO payroll_schedules(id,payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES (1,'2026/27',1,'01/04/2026','20/04/2026','30/04/2026','created');").unwrap();
    for pa in 1..=5 {
        db.execute("INSERT INTO personal_assistants(id,first_name,surname,email,start_date) VALUES (?1,?2,'Test',?3,'01/01/2026')",params![pa,format!("PA{pa}"),format!("pa{pa}@example.test")]).unwrap();
        db.execute("INSERT INTO personal_assistant_pay_rates(personal_assistant_id,effective_date,base_hourly_rate,employer_top_up_rate,created_at) VALUES (?1,'01/01/2026',12,0,'created')",[pa]).unwrap();
        db.execute("INSERT INTO timesheets(id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount,notes) VALUES (?1,?1,'Test','2026-04-02T09:00:00','2026-04-02T11:00:00',0,120,12,24,'source note')",[pa]).unwrap();
        let record = app
            .payroll_timesheet_repository
            .insert("2026/27", 1, pa, None, "created")
            .unwrap();
        app.payroll_timesheet_repository
            .create_missing_weeks(
                record,
                &[
                    "01/04/2026".into(),
                    "08/04/2026".into(),
                    "15/04/2026".into(),
                    "22/04/2026".into(),
                ],
                &[2.0, 0.0, 0.0, 0.0],
            )
            .unwrap();
        db.execute(
            "UPDATE payroll_timesheets SET payroll_department_notes=?1 WHERE id=?2",
            params![format!("Payroll note for PA{pa}\nPlease confirm."), record],
        )
        .unwrap();
        app.payroll_timesheet_repository
            .save_actual_in_lieu_hours(record, Some(pa as f64 + 0.125))
            .unwrap();
    }
    let schedule = app
        .payroll_schedule_repository
        .get_for_year_and_cycle("2026/27", 1)
        .unwrap()
        .unwrap();
    let mut gui = DirectPaymentApp::new(app);
    gui.operational_payroll_period.select(&schedule, None);
    (dir, gui, schedule)
}
fn db(app: &DirectPaymentApp) -> Connection {
    crate::payroll_evidence::open(&app.application).unwrap()
}
fn generate(app: &mut DirectPaymentApp, ids: &[i64]) -> ProductionReport {
    app.begin_generation();
    let mut pending = app.pending_generation.take().unwrap();
    pending.selected_ids = ids.to_vec();
    app.generate_selection(&pending).unwrap()
}
fn email_batch(app: &mut DirectPaymentApp, ids: &[i64]) -> PendingEmailBatch {
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    let mut batch = app.pending_email_batch.take().unwrap();
    batch.selected_personal_assistant_ids = ids.to_vec();
    batch
}
fn path(app: &DirectPaymentApp, schedule: &PayrollSchedule, pa: i64) -> std::path::PathBuf {
    PdfGenerator::timesheet_output_path(
        &app.application.context.config.folders.pdf_output,
        &format!("PA{pa} Test"),
        schedule,
    )
    .unwrap()
}
fn facts(app: &DirectPaymentApp, pa: i64) -> String {
    let db = db(app);
    let mut tables = Vec::new();
    for sql in [
        "SELECT * FROM payroll_timesheets WHERE personal_assistant_id=?1",
        "SELECT * FROM payroll_timesheet_weeks WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
        "SELECT * FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
        "SELECT * FROM payroll_candidate_checks WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
        "SELECT * FROM payroll_timesheet_worked_item_snapshots WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
        "SELECT * FROM payroll_timesheet_email_status WHERE personal_assistant_id=?1",
        "SELECT * FROM payroll_submissions WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
        "SELECT * FROM payroll_corrections WHERE personal_assistant_id=?1",
        "SELECT * FROM payroll_correction_applications WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
        "SELECT * FROM payroll_timesheet_manual_adjustments WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
        "SELECT * FROM payroll_timesheet_annual_leave WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
        "SELECT * FROM payroll_timesheet_public_holidays WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)",
    ] {
        let mut stmt = db.prepare(sql).unwrap(); let count = stmt.column_count();
        let rows = stmt.query_map([pa],|r|(0..count).map(|i|r.get::<_,rusqlite::types::Value>(i)).collect::<rusqlite::Result<Vec<_>>>()).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        tables.push(rows);
    }
    format!("{tables:?}")
}
fn returned(app: &DirectPaymentApp) -> Vec<(i64, Option<f64>, Option<String>)> {
    db(app).prepare("SELECT id,actual_in_lieu_hours,actual_in_lieu_updated_at FROM payroll_timesheets ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

// Real SMTP transport against localhost only; production dispatch and state code are unmocked.
struct Smtp {
    stop: Arc<AtomicBool>,
    fail_attempt: Arc<AtomicUsize>,
    messages: Arc<Mutex<Vec<String>>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Smtp {
    fn new(app: &mut DirectPaymentApp) -> Self {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        app.application.context.config.email.smtp_transport = "SMTP Server".into();
        app.application.context.config.email.smtp_host = "127.0.0.1".into();
        app.application.context.config.email.smtp_port = listener.local_addr().unwrap().port();
        app.application.context.config.email.smtp_username.clear();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let fail_attempt = Arc::new(AtomicUsize::new(0));
        let fail = fail_attempt.clone();
        let messages = Arc::new(Mutex::new(Vec::new()));
        let captured = messages.clone();
        let worker = std::thread::spawn(move || {
            let mut attempts = 0;
            while !done.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                };
                // Exercise the inherited-nonblocking case on Linux too, so
                // removing the normalisation below breaks local regression tests.
                stream.set_nonblocking(true).unwrap();
                // Accepted sockets can inherit the listener's nonblocking mode
                // on Windows/macOS (Linux does not). This protocol loop uses
                // blocking reads with a timeout, never readiness polling.
                stream.set_nonblocking(false).unwrap();
                attempts += 1;
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                stream.write_all(b"220 localhost test\r\n").unwrap();
                loop {
                    let mut line = String::new();
                    if !matches!(reader.read_line(&mut line),Ok(n) if n>0) {
                        break;
                    }
                    let response: &[u8] = if line.starts_with("EHLO") || line.starts_with("HELO") {
                        b"250 localhost\r\n"
                    } else if line.starts_with("DATA") {
                        if fail.load(Ordering::SeqCst) == attempts {
                            b"451 test SMTP failure\r\n"
                        } else {
                            stream.write_all(b"354 send message\r\n").unwrap();
                            let mut message = String::new();
                            loop {
                                let mut content = String::new();
                                let bytes = reader.read_line(&mut content)
                                    .expect("SMTP DATA must complete before the read timeout");
                                assert!(bytes > 0, "SMTP connection closed before DATA terminator");
                                if content == ".\r\n" {
                                    break;
                                }
                                message.push_str(&content);
                            }
                            captured.lock().unwrap().push(message);
                            b"250 queued\r\n"
                        }
                    } else if line.starts_with("QUIT") {
                        let _ = stream.write_all(b"221 bye\r\n");
                        break;
                    } else {
                        b"250 ok\r\n"
                    };
                    if stream.write_all(response).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            stop,
            fail_attempt,
            messages,
            worker: Some(worker),
        }
    }
    fn count(&self) -> usize {
        self.messages.lock().unwrap().len()
    }
}
impl Drop for Smtp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn one_of_five_then_remaining_batch_preserves_early_submission_notes_and_results() {
    let (_dir, mut app, schedule) = fixture();
    let before: Vec<_> = (2..=5).map(|pa| facts(&app, pa)).collect();
    let awards = returned(&app);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    for pa in 2..=5 {
        assert_eq!(facts(&app, pa), before[(pa - 2) as usize]);
        assert!(!path(&app, &schedule, pa).exists());
    }
    let pdf = std::fs::read(path(&app, &schedule, 1)).unwrap();
    assert!(pdf_extract::extract_text(path(&app, &schedule, 1))
        .unwrap()
        .contains("Payroll note for PA1"));
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(smtp.count(), 1);
    for pa in 2..=5 {
        assert_eq!(facts(&app, pa), before[(pa - 2) as usize]);
    }
    let early = facts(&app, 1);
    app.begin_generation();
    let pending = app.pending_generation.take().unwrap();
    assert_eq!(pending.selected_ids, vec![2, 3, 4, 5]);
    assert!(
        !pending
            .choices
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .available
    );
    assert_eq!(app.generate_selection(&pending).unwrap().completed(), 4);
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    let pending = app.pending_email_batch.take().unwrap();
    assert_eq!(pending.selected_personal_assistant_ids, vec![2, 3, 4, 5]);
    assert_eq!(
        app.dispatch_timesheet_selection(&pending)
            .unwrap()
            .completed(),
        4
    );
    assert_eq!(smtp.count(), 5);
    assert_eq!(facts(&app, 1), early);
    assert_eq!(std::fs::read(path(&app, &schedule, 1)).unwrap(), pdf);
    assert_eq!(returned(&app), awards);
    let count:i64=db(&app).query_row("SELECT count(*) FROM payroll_submissions WHERE payroll_department_notes LIKE 'Payroll note for PA%'",[],|r|r.get(0)).unwrap();
    assert_eq!(count, 5);
    let retry = app.dispatch_timesheet_selection(&batch).unwrap();
    assert_eq!(retry.completed(), 0);
    assert_eq!(smtp.count(), 5);
}

#[test]
fn one_some_all_empty_selection_and_captured_context() {
    let (_dir, mut app, _) = fixture();
    app.begin_generation();
    let mut pending = app.pending_generation.take().unwrap();
    assert_eq!(pending.selected_ids, vec![1, 2, 3, 4, 5]);
    pending.selected_ids.clear();
    assert!(app.generate_selection(&pending).is_err());
    pending.selected_ids = vec![2, 4];
    assert_eq!(app.generate_selection(&pending).unwrap().completed(), 2);
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    let mut batch = app.pending_email_batch.take().unwrap();
    assert_eq!(batch.selected_personal_assistant_ids, vec![2, 4]);
    batch.selected_personal_assistant_ids.clear();
    assert!(app.dispatch_timesheet_selection(&batch).is_err());
    batch.selected_personal_assistant_ids = vec![2, 4];
    app.operational_payroll_period.revision += 1;
    assert!(app.generate_selection(&pending).is_err());
    assert!(app.dispatch_timesheet_selection(&batch).is_err());
    app.operational_payroll_period.revision -= 1;
    db(&app)
        .execute("UPDATE payroll_schedules SET pay_date='01/05/2026'", [])
        .unwrap();
    assert!(app.generate_selection(&pending).is_err());
    assert!(app.dispatch_timesheet_selection(&batch).is_err());
}

#[test]
fn unselected_broken_evidence_missing_preparation_and_stale_decisions_are_isolated() {
    let (_dir, mut app, _) = fixture();
    let conn = db(&app);
    // Resolve PA2's overlapping sources, then make its decision stale and evidence invalid.
    conn.execute_batch("INSERT INTO direct_shifts(id,personal_assistant_id,start_time,end_time,break_minutes,source_type,created_at,updated_at) VALUES (20,2,'2026-04-02T09:00:00','2026-04-02T11:00:00',0,'direct','created','updated');").unwrap();
    let evidence = crate::payroll_evidence::load_for_pa(&conn, 2).unwrap();
    let groups = crate::payroll_evidence::groups(&evidence).unwrap();
    crate::payroll_evidence::resolve(
        &conn,
        &evidence,
        &[(groups[0].fingerprint.clone(), "imported:2".into())],
    )
    .unwrap();
    conn.execute_batch(
        "UPDATE direct_shifts SET break_minutes=999 WHERE id=20;
        DELETE FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=3;
        DELETE FROM payroll_timesheets WHERE personal_assistant_id=4;",
    )
    .unwrap();
    let before: Vec<_> = (2..=5).map(|pa| facts(&app, pa)).collect();
    assert!(crate::payroll_evidence::load_connection(&conn).is_err());
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(smtp.count(), 1);
    assert_eq!(
        conn.query_row::<i64, _, _>(
            "SELECT count(*) FROM payroll_duplicate_decisions WHERE invalidated_at IS NULL",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        1
    );
    for pa in 2..=5 {
        assert_eq!(facts(&app, pa), before[(pa - 2) as usize]);
    }
    // Selected broken evidence still fails, rather than losing protections.
    let report = generate(&mut app, &[2]);
    assert!(matches!(&report.entries[0].2, ProductionOutcome::Failed(_)));
}

#[test]
fn partial_send_failure_reports_completed_failed_unattempted_and_retry_skips_success() {
    let (_dir, mut app, _) = fixture();
    assert_eq!(generate(&mut app, &[1, 2, 3]).completed(), 3);
    let smtp = Smtp::new(&mut app);
    smtp.fail_attempt.store(2, Ordering::SeqCst);
    let batch = email_batch(&mut app, &[1, 2, 3]);
    let report = app.dispatch_timesheet_selection(&batch).unwrap();
    assert_eq!(report.entries[0].2, ProductionOutcome::Completed);
    assert!(matches!(report.entries[1].2, ProductionOutcome::Failed(_)));
    assert_eq!(report.entries[2].2, ProductionOutcome::NotAttempted);
    assert_eq!(report.entries[2].1, "PA3 Test");
    assert_eq!(
        app.application
            .payroll_worked_item_repository
            .snapshot_metadata(2)
            .unwrap()
            .unwrap()
            .state,
        crate::payroll_worked_item_repository::SnapshotState::Candidate
    );
    smtp.fail_attempt.store(0, Ordering::SeqCst);
    let report = app.dispatch_timesheet_selection(&batch).unwrap();
    assert!(matches!(report.entries[0].2, ProductionOutcome::Skipped(_)));
    assert_eq!(report.completed(), 2);
    assert_eq!(smtp.count(), 3);
}

#[test]
fn selected_indeterminate_finalisation_is_protected_and_never_retried() {
    let (_dir, mut app, _) = fixture();
    generate(&mut app, &[1, 2]);
    let smtp = Smtp::new(&mut app);
    let conn = db(&app);
    conn.execute_batch("CREATE TRIGGER fail_submission BEFORE INSERT ON payroll_submissions BEGIN SELECT RAISE(ABORT,'test finalisation failure'); END;").unwrap();
    let batch = email_batch(&mut app, &[1, 2]);
    let report = app.dispatch_timesheet_selection(&batch).unwrap();
    assert!(matches!(report.entries[0].2, ProductionOutcome::Failed(_)));
    assert_eq!(report.entries[1].2, ProductionOutcome::NotAttempted);
    assert_eq!(
        app.application
            .payroll_worked_item_repository
            .snapshot_metadata(1)
            .unwrap()
            .unwrap()
            .state,
        crate::payroll_worked_item_repository::SnapshotState::Indeterminate
    );
    conn.execute_batch("DROP TRIGGER fail_submission").unwrap();
    let retry = app.dispatch_timesheet_selection(&batch).unwrap();
    assert_eq!(retry.completed(), 0);
    assert_eq!(smtp.count(), 1);
    assert!(matches!(
        generate(&mut app, &[1]).entries[0].2,
        ProductionOutcome::Failed(_)
    ));
}

fn change_note(app: &DirectPaymentApp, pa: i64, note: &str) {
    let repo = &app.application.payroll_timesheet_repository;
    let mut record = repo
        .get_for_cycle_and_pa("2026/27", 1, pa)
        .unwrap()
        .unwrap();
    record.payroll_department_notes = note.into();
    let weeks = repo.get_weeks(record.id).unwrap();
    let adjustments = weeks
        .iter()
        .map(
            |w| crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number: w.week_number,
                adjustment_minutes: 0,
                reason: None,
            },
        )
        .collect::<Vec<_>>();
    repo.save_preparation_atomically(&record, &weeks, &[], &[], &adjustments, "changed")
        .unwrap();
    assert_eq!(
        repo.get_for_cycle_and_pa("2026/27", 1, pa)
            .unwrap()
            .unwrap()
            .payroll_department_notes,
        note
    );
}

#[test]
fn selected_notes_missing_modified_candidates_and_source_notes_preserve_safety() {
    let (_dir, mut app, schedule) = fixture();
    assert_eq!(generate(&mut app, &[1, 2]).completed(), 2);
    let other = facts(&app, 2);
    let awards = returned(&app);
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1]);
    change_note(
        &app,
        1,
        "Please include any hours in lieu in final pay.\nThank you.",
    );
    assert!(matches!(
        app.dispatch_timesheet_selection(&batch).unwrap().entries[0].2,
        ProductionOutcome::Failed(_)
    ));
    assert_eq!(smtp.count(), 0);
    assert_eq!(facts(&app, 2), other);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let file = path(&app, &schedule, 1);
    let original = std::fs::read(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    assert!(matches!(
        app.dispatch_timesheet_selection(&batch).unwrap().entries[0].2,
        ProductionOutcome::Failed(_)
    ));
    std::fs::write(&file, b"modified PDF").unwrap();
    assert!(matches!(
        app.dispatch_timesheet_selection(&batch).unwrap().entries[0].2,
        ProductionOutcome::Failed(_)
    ));
    assert_eq!(smtp.count(), 0);
    std::fs::write(&file, original).unwrap();
    db(&app)
        .execute(
            "UPDATE timesheets SET notes='source notes still do not invalidate' WHERE id=1",
            [],
        )
        .unwrap();
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(smtp.count(), 1);
    assert_eq!(facts(&app, 2), other);
    assert_eq!(returned(&app), awards);
}

#[test]
fn selected_duplicate_protection_scopes_obsolete_decisions_and_rejects_stale_candidate() {
    let (_dir, mut app, _) = fixture();
    let conn = db(&app);
    for pa in [1, 2] {
        conn.execute("INSERT INTO direct_shifts(id,personal_assistant_id,start_time,end_time,break_minutes,source_type,created_at,updated_at) VALUES (?1,?1,'2026-04-02T09:00:00','2026-04-02T11:00:00',0,'direct','created','updated')", [pa]).unwrap();
        let evidence = crate::payroll_evidence::load_for_pa(&conn, pa).unwrap();
        if pa == 1 {
            assert!(matches!(
                generate(&mut app, &[pa]).entries[0].2,
                ProductionOutcome::Failed(_)
            ));
        }
        let groups = crate::payroll_evidence::groups(&evidence).unwrap();
        crate::payroll_evidence::resolve(
            &conn,
            &evidence,
            &[(groups[0].fingerprint.clone(), format!("imported:{pa}"))],
        )
        .unwrap();
    }
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    conn.execute_batch("UPDATE direct_shifts SET break_minutes=15")
        .unwrap();
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1]);
    assert!(matches!(
        app.dispatch_timesheet_selection(&batch).unwrap().entries[0].2,
        ProductionOutcome::Failed(_)
    ));
    assert_eq!(smtp.count(), 0);
    assert!(matches!(
        generate(&mut app, &[1]).entries[0].2,
        ProductionOutcome::Failed(_)
    ));
    let states: Vec<(i64, Option<String>)> = conn
        .prepare("SELECT winner_id,invalidated_at FROM payroll_duplicate_decisions ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(states[0].1.is_some());
    assert!(states[1].1.is_none());
    // The original global preflight still invalidates obsolete decisions across all PAs.
    let all = crate::payroll_evidence::load_connection(&conn).unwrap();
    assert!(crate::payroll_evidence::preflight_for_pa(&conn, &all, 1).is_err());
    crate::payroll_evidence::preflight(&conn, &all).unwrap();
    assert_eq!(
        conn.query_row::<i64, _, _>(
            "SELECT count(*) FROM payroll_duplicate_decisions WHERE invalidated_at IS NULL",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn partial_generation_preserves_completed_and_unattempted_files_and_state() {
    let (_dir, mut app, schedule) = fixture();
    db(&app)
        .execute(
            "DELETE FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=2",
            [],
        )
        .unwrap();
    let third = facts(&app, 3);
    let report = generate(&mut app, &[1, 2, 3]);
    assert_eq!(report.entries[0].2, ProductionOutcome::Completed);
    assert!(matches!(report.entries[1].2, ProductionOutcome::Failed(_)));
    assert_eq!(report.entries[2].2, ProductionOutcome::NotAttempted);
    assert!(path(&app, &schedule, 1).exists());
    assert!(!path(&app, &schedule, 2).exists());
    assert!(!path(&app, &schedule, 3).exists());
    assert_eq!(facts(&app, 3), third);
}

#[test]
fn authorised_replacement_retains_original_pdf_work_and_note_through_selected_workflow() {
    let (_dir, mut app, schedule) = fixture();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let original = std::fs::read(path(&app, &schedule, 1)).unwrap();
    let awards = returned(&app);
    let others: Vec<_> = (2..=5).map(|pa| facts(&app, pa)).collect();
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    let conn = db(&app);
    conn.execute(
        "UPDATE timesheets SET worked_minutes=180,end_time='2026-04-02T12:00:00' WHERE id=1",
        [],
    )
    .unwrap();
    let record = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 1, 1)
        .unwrap()
        .unwrap();
    let changes =
        crate::payroll_evidence::reconciliation::changes(&app.application, &record).unwrap();
    assert!(changes.changed);
    crate::payroll_evidence::lifecycle::authorize_resubmission(
        &app.application,
        &record,
        &changes.signature,
    )
    .unwrap();
    change_note(&app, 1, "Replacement payroll note");
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let replacement = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&replacement)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(smtp.count(), 2);
    let history:Vec<(i64,Option<i64>,Vec<u8>,String)> = conn.prepare("SELECT id,supersedes_id,pdf_bytes,payroll_department_notes FROM payroll_submissions ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].2, original);
    assert_eq!(history[0].3, "Payroll note for PA1\nPlease confirm.");
    assert_eq!(history[1].1, Some(history[0].0));
    assert_eq!(history[1].3, "Replacement payroll note");
    let minutes:Vec<i64> = conn.prepare("SELECT SUM(worked_minutes) FROM payroll_submission_items GROUP BY submission_id ORDER BY submission_id").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(minutes, vec![120, 180]);
    assert_eq!(returned(&app), awards);
    for pa in 2..=5 {
        assert_eq!(facts(&app, pa), others[(pa - 2) as usize]);
    }
}

#[test]
fn historical_departed_and_retained_preparation_eligibility_use_existing_rules() {
    let (_dir, mut app, _) = fixture();
    db(&app).execute_batch("UPDATE personal_assistants SET leaving_date='15/04/2026',employment_status='Inactive' WHERE id=1;
        UPDATE personal_assistants SET start_date='01/01/2027' WHERE id IN (2,3);
        DELETE FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=3;
        DELETE FROM payroll_timesheets WHERE id=3;").unwrap();
    app.begin_generation();
    let pending = app.pending_generation.take().unwrap();
    assert!(pending.selected_ids.contains(&1));
    assert!(pending.selected_ids.contains(&2));
    assert!(!pending.choices.iter().any(|c| c.id == 3));
    assert_eq!(generate(&mut app, &[1, 2]).completed(), 2);
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1, 2]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        2
    );
    assert_eq!(smtp.count(), 2);
}

#[test]
fn smtp_fixture_completes_data_and_quit_from_nonblocking_accepted_socket() {
    use std::io::{BufRead, BufReader, Write};
    let (_dir, mut app, _schedule) = fixture();
    let smtp = Smtp::new(&mut app);
    let mut stream = std::net::TcpStream::connect((
        "127.0.0.1",
        app.application.context.config.email.smtp_port,
    )).unwrap();
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    assert_eq!(response, "220 localhost test\r\n");
    for (command, expected) in [
        ("EHLO localhost\r\n", "250 localhost\r\n"),
        ("MAIL FROM:<sender@example.test>\r\n", "250 ok\r\n"),
        ("RCPT TO:<payroll@example.test>\r\n", "250 ok\r\n"),
        ("DATA\r\n", "354 send message\r\n"),
    ] {
        stream.write_all(command.as_bytes()).unwrap();
        response.clear();
        reader.read_line(&mut response).unwrap();
        assert_eq!(response, expected);
    }
    stream.write_all(b"Subject: test\r\n\r\nbody\r\n").unwrap();
    assert_eq!(smtp.count(), 0);
    stream.write_all(b".\r\n").unwrap();
    response.clear();
    reader.read_line(&mut response).unwrap();
    assert_eq!(response, "250 queued\r\n");
    assert_eq!(smtp.count(), 1);
    assert_eq!(smtp.messages.lock().unwrap()[0], "Subject: test\r\n\r\nbody\r\n");
    stream.write_all(b"QUIT\r\n").unwrap();
    response.clear();
    reader.read_line(&mut response).unwrap();
    assert_eq!(response, "221 bye\r\n");
    response.clear();
    assert_eq!(reader.read_line(&mut response).unwrap(), 0);
}
