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
    batch.stage = EmailBatchNoteStage::ConfirmDispatch;
    // Unavailable selections still exercise dispatch refusal.
    if batch
        .choices
        .iter()
        .filter(|c| ids.contains(&c.id))
        .all(|c| c.available)
    {
        app.capture_timesheet_confirmation(&mut batch).unwrap();
    } else {
        batch.approved_selection = ids.to_vec();
    }
    batch.resend_acknowledged = true;
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
    lose_acknowledgement: Arc<AtomicBool>,
    messages: Arc<Mutex<Vec<String>>>,
    envelopes: Arc<Mutex<Vec<Vec<String>>>>,
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
        let lose_acknowledgement = Arc::new(AtomicBool::new(false));
        let lose_ack = lose_acknowledgement.clone();
        let messages = Arc::new(Mutex::new(Vec::new()));
        let captured = messages.clone();
        let envelopes = Arc::new(Mutex::new(Vec::new()));
        let captured_envelopes = envelopes.clone();
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
                let mut recipients = Vec::new();
                loop {
                    let mut line = String::new();
                    if !matches!(reader.read_line(&mut line),Ok(n) if n>0) {
                        break;
                    }
                    if line.starts_with("RCPT TO:") {
                        recipients.push(line.trim().to_string());
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
                                let bytes = reader
                                    .read_line(&mut content)
                                    .expect("SMTP DATA must complete before the read timeout");
                                assert!(bytes > 0, "SMTP connection closed before DATA terminator");
                                if content == ".\r\n" {
                                    break;
                                }
                                message.push_str(&content);
                            }
                            captured.lock().unwrap().push(message);
                            captured_envelopes.lock().unwrap().push(recipients.clone());
                            if lose_ack.load(Ordering::SeqCst) {
                                break;
                            }
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
            lose_acknowledgement,
            messages,
            envelopes,
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
    let mut pending = app.pending_generation.take().unwrap();
    assert_eq!(pending.selected_ids, vec![1, 2, 3, 4, 5]);
    pending.selected_ids = vec![2, 3, 4, 5];
    assert!(
        pending
            .choices
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .available
    );
    assert_eq!(app.generate_selection(&pending).unwrap().completed(), 4);
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    let mut pending = app.pending_email_batch.take().unwrap();
    assert_eq!(pending.selected_personal_assistant_ids, vec![2, 3, 4, 5]);
    pending.selected_personal_assistant_ids = vec![2, 3, 4, 5];
    pending.stage = EmailBatchNoteStage::ConfirmDispatch;
    app.capture_timesheet_confirmation(&mut pending).unwrap();
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
    assert_eq!(std::fs::read(path(&app, &schedule, 1)).unwrap(), pdf);
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
fn partial_send_failure_retry_excludes_accepted_messages() {
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
    let replay = app.dispatch_timesheet_selection(&batch).unwrap();
    assert!(matches!(replay.entries[0].2, ProductionOutcome::Skipped(_)));
    assert!(matches!(replay.entries[1].2, ProductionOutcome::Failed(_)));
    assert_eq!(smtp.count(), 1);
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    assert_eq!(
        app.pending_email_batch
            .as_ref()
            .unwrap()
            .selected_personal_assistant_ids,
        vec![2, 3]
    );
    let retry = email_batch(&mut app, &[2, 3]);
    let report = app.dispatch_timesheet_selection(&retry).unwrap();
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
    let batch = email_batch(&mut app, &[1]);
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
    let mut stream =
        std::net::TcpStream::connect(("127.0.0.1", app.application.context.config.email.smtp_port))
            .unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
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
    assert_eq!(
        smtp.messages.lock().unwrap()[0],
        "Subject: test\r\n\r\nbody\r\n"
    );
    stream.write_all(b"QUIT\r\n").unwrap();
    response.clear();
    reader.read_line(&mut response).unwrap();
    assert_eq!(response, "221 bye\r\n");
    response.clear();
    assert_eq!(reader.read_line(&mut response).unwrap(), 0);
}

#[test]
fn submitted_regeneration_uses_corrected_contracted_hours_and_resend_keeps_pdf() {
    let (_dir, mut app, _) = fixture();
    let conn = db(&app);
    conn.execute_batch("UPDATE payroll_schedules SET first_week_commencing='30/08/2026';
        UPDATE timesheets SET start_time='2026-08-31T09:00:00',end_time='2026-08-31T11:00:00';
        UPDATE payroll_timesheet_weeks SET week_commencing=CASE week_number WHEN 1 THEN '30/08/2026' WHEN 2 THEN '06/09/2026' WHEN 3 THEN '13/09/2026' ELSE '20/09/2026' END;
        INSERT INTO personal_assistant_contracted_hours(personal_assistant_id,effective_date,contracted_hours,created_at) VALUES (1,'12/09/2026','8','created');").unwrap();
    let schedule = app
        .application
        .payroll_schedule_repository
        .get_for_year_and_cycle("2026/27", 1)
        .unwrap()
        .unwrap();
    app.operational_payroll_period.select(&schedule, None);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let file = path(&app, &schedule, 1);
    let old_pdf = std::fs::read(&file).unwrap();
    assert!(pdf_extract::extract_text(&file)
        .unwrap()
        .contains("Unavailable"));
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    let saved = facts(&app, 1);
    let mut hours = app
        .application
        .contracted_hours_repository
        .get_all_for_personal_assistant(1)
        .unwrap()
        .remove(0);
    hours.effective_date = "30/08/2026".into();
    app.application
        .contracted_hours_repository
        .update(&hours)
        .unwrap();
    let choices = app.production_choices(&schedule, true).unwrap();
    assert!(choices[0].available && choices[0].detail.contains("Submitted / sent"));
    // Even with corrected source data, resend sends the original attachment.
    let mut batch = email_batch(&mut app, &[1]);
    app.additional_notes_by_personal_assistant
        .insert(1, "Corrected timesheet attached".into());
    app.capture_timesheet_confirmation(&mut batch).unwrap();
    batch.resend_acknowledged = true;
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(std::fs::read(&file).unwrap(), old_pdf);
    assert_eq!(facts(&app, 1), saved);
    assert!(smtp.messages.lock().unwrap()[1].contains("Corrected timesheet attached"));
    let choices = app.production_choices(&schedule, false).unwrap();
    assert!(choices[0].available && choices[0].detail.contains("Submitted / sent"));
    let report = generate(&mut app, &[1]);
    assert_eq!(report.completed(), 1, "{report:?}");
    let corrected_pdf = std::fs::read(&file).unwrap();
    assert_ne!(corrected_pdf, old_pdf);
    let text = pdf_extract::extract_text(&file).unwrap();
    assert!(!text.contains("Unavailable"));
    assert!(text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains("Contracted Weekly Hours: 8"));
    assert!(!text.contains("Corrected timesheet attached"));
    let choices = app.production_choices(&schedule, true).unwrap();
    assert!(choices[0].available && choices[0].detail.contains("Previously submitted / sent"));
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(std::fs::read(&file).unwrap(), corrected_pdf);
    assert_eq!(smtp.count(), 3);
    let messages = smtp.messages.lock().unwrap();
    let attachment = |message: &String| -> String {
        message
            .split("Content-Type: application/pdf")
            .nth(1)
            .unwrap()
            .split("\r\n\r\n")
            .nth(1)
            .unwrap()
            .split("\r\n--")
            .next()
            .unwrap()
            .to_string()
    };
    assert_eq!(attachment(&messages[0]), attachment(&messages[1]));
    assert_ne!(attachment(&messages[1]), attachment(&messages[2]));
    assert!(app.production_choices(&schedule, true).unwrap()[0]
        .detail
        .contains("Submitted / sent"));
}

fn payslip_selection_fixture() -> (tempfile::TempDir, DirectPaymentApp, PayrollSchedule) {
    let (dir, mut app, schedule) = fixture();
    app.application.context.config.folders.payslip_folder = dir.path().join("payslips");
    for id in 1..=5 {
        let path = crate::payroll_file_naming::payslip_path(
            &app.application.context.config.folders.payslip_folder,
            &format!("PA{id} Test"),
            &schedule,
        )
        .unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("%PDF-1.4 payslip {id}")).unwrap();
        for kind in ["p45", "p60"] {
            let path = dir.path().join(format!("{kind}-PA{id}.pdf"));
            std::fs::write(&path, format!("%PDF-1.4 {kind} {id}")).unwrap();
            let document = app
                .application
                .payroll_timesheet_email_repository
                .register_document(id, kind, &path, None)
                .unwrap();
            assert!(app
                .application
                .payroll_timesheet_email_repository
                .reconcile_document(document, true)
                .unwrap());
        }
    }
    (dir, app, schedule)
}

fn payslip_batch(app: &mut DirectPaymentApp, ids: &[i64]) -> PendingEmailBatch {
    app.begin_email_batch(PayrollEmailKind::Payslip);
    let mut batch = app.pending_email_batch.take().unwrap();
    assert_eq!(batch.stage, EmailBatchNoteStage::ChooseAdditionalNote);
    batch.finish_notes();
    assert_eq!(batch.stage, EmailBatchNoteStage::SelectRecipients);
    batch.selected_personal_assistant_ids = ids.to_vec();
    batch.confirm_selection();
    batch
}

#[test]
fn payslip_selection_one_several_all_preserves_notes_supplements_and_unselected_status() {
    for ids in [vec![1], vec![2, 4], vec![1, 2, 3, 4, 5]] {
        let (_dir, mut app, schedule) = payslip_selection_fixture();
        app.application.context.config.payroll.email_subject_format =
            "Timesheet - {Personal Assistant Name} {YYYYMMwWW}".into();
        let smtp = Smtp::new(&mut app);
        app.begin_email_batch(PayrollEmailKind::Payslip);
        let mut batch = app.pending_email_batch.take().unwrap();
        assert_eq!(batch.selected_personal_assistant_ids, vec![1, 2, 3, 4, 5]);
        assert!(batch
            .choices
            .iter()
            .all(|c| c.available && c.detail.contains("PAYSLIP + P45 + P60")));
        batch.stage = EmailBatchNoteStage::EditAdditionalNotes;
        for id in 1..=5 {
            app.additional_notes_by_personal_assistant
                .insert(id, format!("Private batch note PA{id}"));
        }
        batch.finish_notes();
        batch.selected_personal_assistant_ids = ids.clone();
        assert_eq!(smtp.count(), 0);
        batch.confirm_selection();
        assert_eq!(batch.stage, EmailBatchNoteStage::ConfirmDispatch);
        assert_eq!(app.dispatch_payslip_selection(&batch).unwrap(), ids.len());
        assert_eq!(smtp.count(), ids.len());
        let messages = smtp.messages.lock().unwrap().clone();
        let envelopes = smtp.envelopes.lock().unwrap().clone();
        assert_eq!(envelopes.len(), ids.len());
        for (message, envelope) in messages.iter().zip(&envelopes) {
            let pa = ids
                .iter()
                .find(|id| message.contains(&format!("To: pa{id}@example.test")))
                .unwrap();
            assert_eq!(envelope.len(), 2);
            assert!(envelope.contains(&format!("RCPT TO:<pa{pa}@example.test>")));
            assert!(envelope.contains(&"RCPT TO:<employer@example.test>".into()));
            assert!(!message.contains("Cc:"));
            assert!(!message.contains("payroll@example.test"));
            assert!(!message.contains("Timesheet -"));
            assert!(message.contains(&format!(
                "Subject: Payslip for Week {}",
                crate::payroll_file_naming::paye_week(&schedule).unwrap()
            )));
        }
        for id in 1..=5 {
            let selected = ids.contains(&id);
            let status = app
                .application
                .payroll_timesheet_email_repository
                .get_for_pa_and_cycle(id, &schedule.payroll_year, schedule.cycle_number, "payslip")
                .unwrap();
            assert_eq!(
                status.as_ref().is_some_and(|s| s.is_definitively_sent()),
                selected
            );
            if !selected {
                assert!(status.is_none());
            }
            let docs = app
                .application
                .payroll_timesheet_email_repository
                .documents_for_pa(id)
                .unwrap();
            assert_eq!(docs.len(), 2);
            for doc in docs {
                assert_eq!(
                    matches!(
                        doc.delivery_state,
                        crate::payroll_timesheet_email_repository::EmailDeliveryState::Sent { .. }
                    ),
                    selected
                );
                if !selected {
                    assert_eq!(
                        doc.delivery_state,
                        crate::payroll_timesheet_email_repository::EmailDeliveryState::Unsent
                    );
                }
            }
            assert_eq!(
                messages
                    .iter()
                    .filter(|m| m.contains(&format!("Private batch note PA{id}")))
                    .count(),
                usize::from(selected)
            );
            for kind in ["P45", "P60"] {
                assert_eq!(
                    messages
                        .iter()
                        .filter(|m| m.contains(&format!("{kind} for PA{id} Test.pdf")))
                        .count(),
                    usize::from(selected)
                );
            }
        }
        assert_eq!(
            app.application
                .payroll_schedule_repository
                .get_for_year_and_cycle(&schedule.payroll_year, schedule.cycle_number)
                .unwrap()
                .unwrap()
                .payslips_sent,
            ids.len() == 5
        );
    }
}

#[test]
fn payslip_selection_cancel_and_zero_selection_never_dispatch_or_change_status() {
    let (_dir, mut app, _schedule) = payslip_selection_fixture();
    let smtp = Smtp::new(&mut app);
    for stage in [
        EmailBatchNoteStage::ChooseAdditionalNote,
        EmailBatchNoteStage::EditAdditionalNotes,
        EmailBatchNoteStage::SelectRecipients,
        EmailBatchNoteStage::ConfirmDispatch,
    ] {
        app.begin_email_batch(PayrollEmailKind::Payslip);
        app.pending_email_batch.as_mut().unwrap().stage = stage;
        app.additional_notes_by_personal_assistant
            .insert(1, "cancelled note".into());
        // Render the real workflow without input: no stage is a send action.
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.draw_additional_note_prompt(ui));
        });
        app.clear_pending_email_batch(); // shared Cancel action at every stage
        assert!(app.pending_email_batch.is_none());
        assert!(app.additional_notes_by_personal_assistant.is_empty());
        assert!(app.note_enabled_personal_assistant_ids.is_empty());
    }
    let mut empty = payslip_batch(&mut app, &[]);
    assert_eq!(empty.stage, EmailBatchNoteStage::SelectRecipients);
    assert!(app.dispatch_payslip_selection(&empty).is_err());
    empty.stage = EmailBatchNoteStage::ConfirmDispatch;
    assert!(app.dispatch_payslip_selection(&empty).is_err());
    assert_eq!(smtp.count(), 0);
    assert_eq!(
        db(&app)
            .query_row(
                "SELECT COUNT(*) FROM payroll_timesheet_email_status",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        db(&app)
            .query_row(
                "SELECT COUNT(*) FROM imported_payroll_documents WHERE sent_at IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn payslip_selection_revalidates_sent_indeterminate_changed_and_unavailable_recipients() {
    let (dir, mut app, _schedule) = payslip_selection_fixture();
    let smtp = Smtp::new(&mut app);
    app.begin_email_batch(PayrollEmailKind::Payslip);
    app.additional_notes_by_personal_assistant
        .insert(1, "Private batch note must be cleared".into());
    app.note_enabled_personal_assistant_ids.insert(1);
    app.finish_email_notes(false); // exact No Additional Note action
    assert!(app.additional_notes_by_personal_assistant.is_empty());
    assert!(app.note_enabled_personal_assistant_ids.is_empty());
    let mut batch = app.pending_email_batch.take().unwrap();
    assert_eq!(batch.stage, EmailBatchNoteStage::SelectRecipients);
    batch.selected_personal_assistant_ids = vec![1];
    batch.confirm_selection();
    assert_eq!(app.dispatch_payslip_selection(&batch).unwrap(), 1);
    assert_eq!(app.dispatch_payslip_selection(&batch).unwrap(), 0);
    db(&app).execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES (2,'2026/27',1,'payslip','indeterminate:test')", []).unwrap();
    let mut batch = payslip_batch(&mut app, &[3]);
    assert!(!batch.choices.iter().find(|c| c.id == 1).unwrap().available);
    assert!(!batch.choices.iter().find(|c| c.id == 2).unwrap().available);
    for ids in [vec![1], vec![2], vec![999], vec![3, 3], vec![]] {
        batch.selected_personal_assistant_ids = ids;
        assert!(app.dispatch_payslip_selection(&batch).is_err());
    }
    batch.selected_personal_assistant_ids = vec![3];
    app.operational_payroll_period.revision += 1;
    assert!(app.dispatch_payslip_selection(&batch).is_err());
    app.operational_payroll_period.revision -= 1;
    std::fs::write(dir.path().join("p45-PA3.pdf"), b"%PDF-1.4 changed").unwrap();
    assert!(app.dispatch_payslip_selection(&batch).is_err());
    assert_eq!(smtp.count(), 1);
    // A broken, unselected recipient cannot prevent another selected PA's send.
    batch.selected_personal_assistant_ids = vec![4];
    assert_eq!(app.dispatch_payslip_selection(&batch).unwrap(), 1);
    assert_eq!(smtp.count(), 2);
    assert!(smtp
        .messages
        .lock()
        .unwrap()
        .iter()
        .all(|m| !m.contains("Private batch note")));
}

#[test]
fn supplement_history_review_covers_all_pas_and_survives_cancel_without_sending() {
    let (dir, mut app, schedule) = fixture();
    app.application.context.config.folders.payslip_folder = dir.path().join("payslips");
    // Both an externally delivered historical P60 and a newly received P45
    // enter the same unknown state. No date/name heuristic distinguishes them.
    for (pa, kind) in [(1, "p60"), (2, "p45")] {
        let source = dir.path().join(format!("{kind} for PA{pa} Test.pdf"));
        std::fs::write(&source, format!("%PDF-1.4 {kind}")).unwrap();
        let report = app
            .application
            .import_payroll_documents(&source, None)
            .unwrap();
        assert_eq!(report.supplements_imported, 1);
    }
    let smtp = Smtp::new(&mut app);
    app.begin_email_batch(PayrollEmailKind::Payslip);
    assert_eq!(
        app.pending_email_batch.as_ref().unwrap().stage,
        EmailBatchNoteStage::ReconcileSupplements
    );
    let docs = app.unknown_supplements().unwrap();
    assert_eq!(docs.len(), 2);
    assert!(app
        .pending_email_batch
        .as_ref()
        .unwrap()
        .selected_personal_assistant_ids
        .is_empty());
    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.draw_additional_note_prompt(ui));
    });
    assert_eq!(smtp.count(), 0);
    let repo = &app.application.payroll_timesheet_email_repository;
    assert!(repo.reconcile_document(docs[0].1.id, false).unwrap());
    assert!(repo.reconcile_document(docs[1].1.id, true).unwrap());
    app.clear_pending_email_batch();
    assert_eq!(smtp.count(), 0);
    assert_eq!(
        db(&app)
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM imported_payroll_documents WHERE sent_at IS NOT NULL",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        0
    );
    app.begin_email_batch(PayrollEmailKind::Payslip);
    assert_eq!(
        app.pending_email_batch
            .as_ref()
            .unwrap()
            .selected_personal_assistant_ids,
        vec![2]
    );
    assert!(app.unknown_supplements().unwrap().is_empty());
    let batch = payslip_batch(&mut app, &[2]);
    assert_eq!(app.dispatch_payslip_selection(&batch).unwrap(), 1);
    assert_eq!(smtp.count(), 1);
    assert!(smtp.messages.lock().unwrap()[0].contains("Subject: Payroll documents - PA2 Test"));
    assert_eq!(
        app.application
            .payroll_timesheet_email_repository
            .documents_for_pa(1)
            .unwrap()[0]
            .history_state,
        "external"
    );
    assert!(app
        .application
        .payroll_timesheet_email_repository
        .get_for_pa_and_cycle(2, &schedule.payroll_year, schedule.cycle_number, "payslip")
        .unwrap()
        .is_none());
}

#[test]
fn payslip_invalid_address_never_sends_and_timesheet_routing_remains_unchanged() {
    let (_dir, mut app, schedule) = payslip_selection_fixture();
    let smtp = Smtp::new(&mut app);
    for address in [None, Some(""), Some("not-an-email")] {
        db(&app)
            .execute(
                "UPDATE personal_assistants SET email=?1 WHERE id=1",
                [address],
            )
            .unwrap();
        let choices = app.payslip_recipient_choices(Some(&schedule)).unwrap();
        assert!(!choices.iter().find(|c| c.id == 1).unwrap().available);
        assert!(app.email_payslips(Some(&schedule), &[1]).is_err());
        assert_eq!(smtp.count(), 0);
        assert!(app
            .application
            .payroll_timesheet_email_repository
            .get_for_pa_and_cycle(1, &schedule.payroll_year, schedule.cycle_number, "payslip")
            .unwrap()
            .is_none());
        assert!(app
            .application
            .payroll_timesheet_email_repository
            .documents_for_pa(1)
            .unwrap()
            .iter()
            .all(|d| matches!(
                d.delivery_state,
                crate::payroll_timesheet_email_repository::EmailDeliveryState::Unsent
            )));
    }
    db(&app)
        .execute(
            "UPDATE personal_assistants SET email='pa1@example.test' WHERE id=1",
            [],
        )
        .unwrap();
    app.application.context.config.payroll.email_subject_format =
        "Timesheet - {Personal Assistant Name} {YYYYMMwWW}".into();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(smtp.count(), 1);
    let message = smtp.messages.lock().unwrap()[0].clone();
    assert!(message.contains("To: payroll@example.test"));
    assert!(message.contains("Cc: employer@example.test"));
    assert!(message.contains("Subject: Timesheet - PA1 Test"));
    let envelope = smtp.envelopes.lock().unwrap()[0].clone();
    assert_eq!(envelope.len(), 3);
    for address in [
        "payroll@example.test",
        "employer@example.test",
        "pa1@example.test",
    ] {
        assert!(envelope.contains(&format!("RCPT TO:<{address}>")));
    }
}

#[test]
fn sent_supplements_stay_excluded_after_new_cycle_import_and_same_path_reimport() {
    let (dir, mut app, schedule) = payslip_selection_fixture();
    let smtp = Smtp::new(&mut app);
    let batch = payslip_batch(&mut app, &[1]);
    assert_eq!(app.dispatch_payslip_selection(&batch).unwrap(), 1);
    let before: Vec<(i64,String)> = db(&app).prepare("SELECT id,sent_at FROM imported_payroll_documents WHERE personal_assistant_id=1 ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    for doc in app
        .application
        .payroll_timesheet_email_repository
        .documents_for_pa(1)
        .unwrap()
    {
        assert_eq!(
            app.application
                .payroll_timesheet_email_repository
                .register_document(
                    1,
                    &doc.document_type,
                    &doc.path,
                    doc.document_year.as_deref()
                )
                .unwrap(),
            doc.id
        );
    }
    db(&app).execute_batch("INSERT INTO payroll_schedules(id,payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES(2,'2026/27',2,'01/05/2026','20/05/2026','28/05/2026','created');").unwrap();
    let next = app
        .application
        .payroll_schedule_repository
        .get_for_year_and_cycle(&schedule.payroll_year, 2)
        .unwrap()
        .unwrap();
    let source = dir.path().join("Payslip for PA1 Test.pdf");
    std::fs::write(&source, b"%PDF-1.4 new cycle").unwrap();
    let report = app
        .application
        .import_payroll_documents(&source, Some(&next))
        .unwrap();
    assert_eq!(report.payslips_imported, 1);
    let pa = app
        .application
        .personal_assistant_repository
        .get_all()
        .unwrap()
        .into_iter()
        .find(|p| p.id == 1)
        .unwrap();
    let bundle = app.unsent_payslip_documents(&pa, &next).unwrap();
    assert_eq!(bundle.email_types, vec!["payslip"]);
    assert!(bundle.document_ids.is_empty());
    let after: Vec<(i64,String)> = db(&app).prepare("SELECT id,sent_at FROM imported_payroll_documents WHERE personal_assistant_id=1 ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(before, after);
    assert_eq!(smtp.count(), 1);
}

#[test]
fn duplicate_supplement_import_preserves_original_and_rolls_back_new_copy() {
    let (dir, app, _) = payslip_selection_fixture();
    let repo = &app.application.payroll_timesheet_email_repository;
    let original = repo.documents_for_pa(1).unwrap().remove(0);
    let bundle = crate::payslip_delivery_service::select_payroll_documents(repo, 1, None).unwrap();
    crate::payslip_delivery_service::send_payroll_bundle(repo, 1, None, &bundle, "fixture", || {
        Ok(())
    })
    .unwrap();
    let source = dir.path().join("P45 renamed copy for PA1 Test.pdf");
    std::fs::copy(&original.path, &source).unwrap();
    let before = std::fs::read(&original.path).unwrap();
    let report = app
        .application
        .import_payroll_documents(&source, None)
        .unwrap();
    assert_eq!(report.supplements_imported, 0);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].contains("P45 renamed copy for PA1 Test.pdf"));
    assert!(report.failures[0].contains("Identical supplement already registered"));
    let documents = repo.documents_for_pa(1).unwrap();
    assert_eq!(documents.len(), 2);
    assert!(documents.iter().all(|d| matches!(
        d.delivery_state,
        crate::payroll_timesheet_email_repository::EmailDeliveryState::Sent { .. }
    )));
    assert_eq!(std::fs::read(&original.path).unwrap(), before);
    let destination = app
        .application
        .context
        .config
        .folders
        .payslip_folder
        .join("P45 renamed copy for PA1 Test.pdf");
    assert!(!destination.exists());
    assert!(source.exists());
}

#[test]
fn stage1_defaults_manual_acknowledgement_and_corrected_first_send() {
    let (_dir, mut app, _) = fixture();
    generate(&mut app, &[1, 2]);
    let smtp = Smtp::new(&mut app);
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    assert_eq!(
        app.pending_email_batch
            .as_ref()
            .unwrap()
            .selected_personal_assistant_ids,
        vec![1, 2]
    );
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    let pending = app.pending_email_batch.as_ref().unwrap();
    assert_eq!(pending.selected_personal_assistant_ids, vec![2]);
    assert!(
        pending
            .choices
            .iter()
            .find(|c| c.id == 1)
            .unwrap()
            .available
    );
    let mut resend = email_batch(&mut app, &[1]);
    resend.resend_acknowledged = false;
    assert!(app
        .dispatch_timesheet_selection(&resend)
        .unwrap_err()
        .to_string()
        .contains("acknowledgement"));
    resend.resend_acknowledged = true;
    assert_eq!(
        app.dispatch_timesheet_selection(&resend)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(
        db(&app)
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM payroll_submissions WHERE payroll_timesheet_id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    assert_eq!(
        app.pending_email_batch
            .as_ref()
            .unwrap()
            .selected_personal_assistant_ids,
        vec![1, 2]
    );
    let corrected = email_batch(&mut app, &[1]);
    assert!(!corrected.choices[0].intent.as_ref().unwrap().resend);
    assert_eq!(
        app.dispatch_timesheet_selection(&corrected)
            .unwrap()
            .completed(),
        1
    );
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    assert_eq!(
        app.pending_email_batch
            .as_ref()
            .unwrap()
            .selected_personal_assistant_ids,
        vec![2]
    );
    assert_eq!(smtp.count(), 3);
    assert_eq!(
        db(&app)
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM payroll_submissions WHERE payroll_timesheet_id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        2
    );
}

#[test]
fn stage1_stale_selection_routing_and_classification_refuse_without_sending() {
    let (_dir, mut app, _) = fixture();
    generate(&mut app, &[1, 2]);
    let smtp = Smtp::new(&mut app);
    let mut batch = email_batch(&mut app, &[1]);
    batch.selected_personal_assistant_ids.push(2);
    assert!(app.dispatch_timesheet_selection(&batch).is_err());
    let batch = email_batch(&mut app, &[1]);
    db(&app)
        .execute(
            "UPDATE personal_assistants SET email='changed@example.test' WHERE id=1",
            [],
        )
        .unwrap();
    assert!(app.validate_timesheet_confirmation(&batch).is_err());
    assert!(matches!(
        app.dispatch_timesheet_selection(&batch).unwrap().entries[0].2,
        ProductionOutcome::Failed(_)
    ));
    let original = email_batch(&mut app, &[1]);
    let other = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&other)
            .unwrap()
            .completed(),
        1
    );
    assert!(app.validate_timesheet_confirmation(&original).is_err());
    assert!(matches!(
        app.dispatch_timesheet_selection(&original).unwrap().entries[0].2,
        ProductionOutcome::Failed(_)
    ));
    assert_eq!(smtp.count(), 1);
}

#[test]
fn stage1_confirmation_reuses_verified_bytes_and_records_actual_message_id() {
    let (_dir, mut app, _) = fixture();
    generate(&mut app, &[1]);
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1]);
    let intent = batch.choices[0].intent.as_ref().unwrap();
    let approved = &batch.approved[&1];
    let bytes = crate::timesheet_delivery::bytes(&db(&app), intent).unwrap();
    let id = format!("<{}@direct-payment-timesheets.local>", intent.intent_id);
    let prepared = crate::email_service::prepare_timesheet_message(approved, &bytes, &id).unwrap();
    let original = prepared.formatted();
    std::fs::write(&intent.path, b"external mutation").unwrap();
    assert_eq!(prepared.formatted(), original);
    assert!(app.validate_timesheet_confirmation(&batch).is_err());
    assert_eq!(smtp.count(), 0);
    std::fs::write(&intent.path, &bytes).unwrap();
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    assert!(smtp.messages.lock().unwrap()[0].contains(&format!("Message-ID: {id}")));
    let history = crate::timesheet_delivery::history(&db(&app)).unwrap();
    assert_eq!(history[0].message_id, id);
    assert_eq!(
        history[0].recipients,
        format!(
            "To: {}; CC: {}; BCC: {}",
            approved.to,
            approved.cc.as_deref().unwrap_or(""),
            approved.bcc.as_deref().unwrap_or("")
        )
    );
}

#[test]
fn stage1_smtp_lost_acceptance_acknowledgement_never_retries_on_restart() {
    let (dir, mut app, _) = fixture();
    generate(&mut app, &[1]);
    let smtp = Smtp::new(&mut app);
    smtp.lose_acknowledgement.store(true, Ordering::SeqCst);
    let batch = email_batch(&mut app, &[1]);
    let report = app.dispatch_timesheet_selection(&batch).unwrap();
    assert!(matches!(&report.entries[0].2,ProductionOutcome::Failed(e) if e.contains("uncertain")));
    assert_eq!(smtp.count(), 1);
    let database_path = app.application.context.environment.database_path.clone();
    let restarted = crate::database::open(database_path).unwrap();
    crate::database::create_schema(&restarted).unwrap();
    assert!(crate::timesheet_delivery::blocked(&restarted, 1).unwrap());
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    assert!(app
        .pending_email_batch
        .as_ref()
        .unwrap()
        .selected_personal_assistant_ids
        .is_empty());
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        0
    );
    assert_eq!(smtp.count(), 1);
    assert_eq!(
        crate::timesheet_delivery::history(&restarted).unwrap()[0].outcome,
        "uncertain"
    );
    drop(dir);
}

#[test]
fn stage1_ui_separates_first_sends_and_resends_and_resets_acknowledgement() {
    let (_dir, mut app, _) = fixture();
    generate(&mut app, &[1, 2]);
    let smtp = Smtp::new(&mut app);
    let first = email_batch(&mut app, &[1]);
    app.dispatch_timesheet_selection(&first).unwrap();
    let mut batch = email_batch(&mut app, &[1, 2]);
    batch.resend_acknowledged = false;
    app.pending_email_batch = Some(batch);
    let ctx = egui::Context::default();
    fn text(shape: &egui::Shape, labels: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => labels.push(t.galley.job.text.clone()),
            egui::Shape::Vec(v) => {
                for s in v {
                    text(s, labels)
                }
            }
            _ => {}
        }
    }
    let mut labels = Vec::new();
    for _ in 0..2 {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1800.0, 1800.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.draw_additional_note_prompt(ui));
            },
        );
        for shape in output.shapes {
            text(&shape.shape, &mut labels);
        }
    }
    let rendered = labels.join("\n");
    assert!(rendered.contains("1 first sends; 1 resends"), "{rendered}");
    assert!(rendered.contains("I intentionally authorise"));
    assert!(rendered.contains("To Payroll:"));
    assert!(rendered.contains("Payroll period:"));
    assert_eq!(smtp.count(), 1);
    app.pending_email_batch
        .as_mut()
        .unwrap()
        .resend_acknowledged = true;
    db(&app)
        .execute(
            "UPDATE personal_assistants SET email='different@example.test' WHERE id=1",
            [],
        )
        .unwrap();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.draw_additional_note_prompt(ui));
    });
    assert!(
        !app.pending_email_batch
            .as_ref()
            .unwrap()
            .resend_acknowledged
    );
    assert!(app
        .pending_email_batch
        .as_ref()
        .unwrap()
        .approved
        .is_empty());
    assert_eq!(smtp.count(), 1);
}

#[test]
fn stage2_settled_sickness_correction_uses_real_pdf_and_safeguarded_first_send() {
    use crate::sickness_service as sick;
    let (_dir, mut app, schedule) = fixture();
    let conn = db(&app);
    let draft = crate::sickness_period_repository::SicknessPeriod {
        id: 0,
        personal_assistant_id: 1,
        start_date: "2026-04-03".into(),
        end_date: "2026-04-09".into(),
    };
    let review = sick::review(&conn, 1, None, Some(&draft)).unwrap();
    let original_dates = sick::mutate(
        &conn,
        1,
        None,
        Some(&draft),
        sick::Scope::Editable,
        "Original sickness",
        &review.signature,
    )
    .unwrap()
    .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let original_path = path(&app, &schedule, 1);
    let original_pdf = std::fs::read(&original_path).unwrap();
    let smtp = Smtp::new(&mut app);
    let first = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&first)
            .unwrap()
            .completed(),
        1
    );
    conn.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',1,'payslip','settled timestamp')",[]).unwrap();
    let totals:Vec<f64>=conn.prepare("SELECT worked_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=1 ORDER BY week_number").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let updated_at: String = conn
        .query_row(
            "SELECT updated_at FROM payroll_timesheets WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut corrected = original_dates.clone();
    corrected.end_date = "2026-04-10".into();
    let review = sick::review(&conn, 1, Some(&original_dates), Some(&corrected)).unwrap();
    sick::mutate(
        &conn,
        1,
        Some(&original_dates),
        Some(&corrected),
        sick::Scope::AuthorisedCorrection,
        "Date confirmed with PA",
        &review.signature,
    )
    .unwrap();
    let report = generate(&mut app, &[1]);
    assert_eq!(report.completed(), 1, "{:?}", report.entries);
    let corrected_path: String = conn
        .query_row(
            "SELECT pdf_path FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(corrected_path.contains("Sickness-Correction-"));
    assert_ne!(std::path::Path::new(&corrected_path), original_path);
    let text = pdf_extract::extract_text(&corrected_path).unwrap();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(text.contains("10/04/2026)"));
    assert!(
        text.contains("** Sickness Information Correction"),
        "{text}"
    );
    assert!(text.contains("assess financial impact"));
    assert_eq!(std::fs::read(original_path).unwrap(), original_pdf);
    assert_eq!(
        conn.query_row::<String, _, _>(
            "SELECT updated_at FROM payroll_timesheets WHERE id=1",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        updated_at
    );
    let after:Vec<f64>=conn.prepare("SELECT worked_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=1 ORDER BY week_number").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(totals, after);
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    assert!(app
        .pending_email_batch
        .as_ref()
        .unwrap()
        .selected_personal_assistant_ids
        .contains(&1));
    let corrected_batch = email_batch(&mut app, &[1]);
    assert!(
        !corrected_batch
            .choices
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .intent
            .as_ref()
            .unwrap()
            .resend
    );
    assert_eq!(
        app.dispatch_timesheet_selection(&corrected_batch)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(smtp.count(), 2);
    assert_eq!(conn.query_row::<String,_,_>("SELECT sent_at FROM payroll_timesheet_email_status WHERE personal_assistant_id=1 AND email_type='payslip'",[],|r|r.get(0)).unwrap(),"settled timestamp");
    assert_eq!(
        conn.query_row::<i64, _, _>(
            "SELECT count(*) FROM payroll_corrections WHERE personal_assistant_id=1",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row::<i64, _, _>(
            "SELECT count(*) FROM payroll_submissions WHERE payroll_timesheet_id=1",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        2
    );
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    assert!(!app
        .pending_email_batch
        .as_ref()
        .unwrap()
        .selected_personal_assistant_ids
        .contains(&1));
}

#[test]
fn stage2_sickness_authority_can_be_superseded_by_existing_worked_correction() {
    use crate::sickness_service as sick;
    use crate::timesheet_delivery as delivery;
    let (_dir, mut app, _schedule) = fixture();
    let conn = db(&app);
    let draft = crate::sickness_period_repository::SicknessPeriod {
        id: 0,
        personal_assistant_id: 1,
        start_date: "2026-04-03".into(),
        end_date: "2026-04-04".into(),
    };
    let reviewed = sick::review(&conn, 1, None, Some(&draft)).unwrap();
    let p = sick::mutate(
        &conn,
        1,
        None,
        Some(&draft),
        sick::Scope::Editable,
        "Original",
        &reviewed.signature,
    )
    .unwrap()
    .unwrap();
    generate(&mut app, &[1]);
    let submit = || {
        let i = delivery::capture(&conn, 1).unwrap();
        let bytes = delivery::bytes(&conn, &i).unwrap();
        delivery::execute(
            &conn,
            &i,
            "Payroll; CC employer",
            &format!("<{}@test.local>", i.intent_id),
            &bytes,
            |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
    };
    submit();
    let mut corrected = p.clone();
    corrected.end_date = "2026-04-05".into();
    let reviewed = sick::review(&conn, 1, Some(&p), Some(&corrected)).unwrap();
    sick::mutate(
        &conn,
        1,
        Some(&p),
        Some(&corrected),
        sick::Scope::AuthorisedCorrection,
        "Sickness dates reviewed",
        &reviewed.signature,
    )
    .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    submit();
    conn.execute(
        "UPDATE timesheets SET worked_minutes=180,end_time='2026-04-02T12:00:00' WHERE id=1",
        [],
    )
    .unwrap();
    let record = sick::record(&conn, 1).unwrap();
    let change =
        crate::payroll_evidence::reconciliation::changes(&app.application, &record).unwrap();
    assert!(change.changed);
    crate::payroll_evidence::lifecycle::authorize_resubmission(
        &app.application,
        &record,
        &change.signature,
    )
    .unwrap();
    assert!(sick::correction(&conn, 1).unwrap().is_none());
    let result = generate(&mut app, &[1]);
    assert_eq!(result.completed(), 1, "{:?}", result.entries);
    assert_eq!(conn.query_row::<f64,_,_>("SELECT worked_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=1 AND week_number=1",[],|r|r.get(0)).unwrap(),3.0);
    assert_eq!(
        conn.query_row::<i64, _, _>(
            "SELECT count(*) FROM sickness_corrections WHERE retired_at IS NOT NULL",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn stage2_full_pa_archive_reactivation_preserves_correction_snapshots_and_delivery() {
    use crate::sickness_service as sick;
    use crate::timesheet_delivery as delivery;
    let (_dir, mut app, _schedule) = fixture();
    let conn = db(&app);
    conn.execute(
        "UPDATE personal_assistants SET employment_status='Active' WHERE id=1",
        [],
    )
    .unwrap();
    let p = crate::sickness_period_repository::SicknessPeriod {
        id: 0,
        personal_assistant_id: 1,
        start_date: "2026-04-03".into(),
        end_date: "2026-04-04".into(),
    };
    let plan = sick::review(&conn, 1, None, Some(&p)).unwrap();
    let p = sick::mutate(
        &conn,
        1,
        None,
        Some(&p),
        sick::Scope::Editable,
        "Original sickness",
        &plan.signature,
    )
    .unwrap()
    .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let submit = || {
        let i = delivery::capture(&conn, 1).unwrap();
        let bytes = delivery::bytes(&conn, &i).unwrap();
        delivery::execute(
            &conn,
            &i,
            "Payroll; CC employer",
            &format!("<{}@archive.test>", i.intent_id),
            &bytes,
            |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
    };
    submit();
    conn.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',1,'payslip','original settled')",[]).unwrap();
    let mut revised = p.clone();
    revised.end_date = "2026-04-05".into();
    let plan = sick::review(&conn, 1, Some(&p), Some(&revised)).unwrap();
    sick::mutate(
        &conn,
        1,
        Some(&p),
        Some(&revised),
        sick::Scope::AuthorisedCorrection,
        "Verified sickness dates",
        &plan.signature,
    )
    .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    submit();
    let captured = delivery::capture(&conn, 1).unwrap();
    let bytes = delivery::bytes(&conn, &captured).unwrap();
    let before_documents=conn.prepare("SELECT id,pdf_sha256,pdf_bytes FROM timesheet_documents WHERE payroll_timesheet_id=1 ORDER BY id").unwrap().query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,Vec<u8>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    let snapshots=conn.prepare("SELECT document_id,evidence,correction_id FROM sickness_document_evidence WHERE payroll_timesheet_id=1 ORDER BY document_id").unwrap().query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<i64>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    let mut pa = app
        .application
        .personal_assistant_repository
        .get_all()
        .unwrap()
        .into_iter()
        .find(|p| p.id == 1)
        .unwrap();
    pa.employment_status = Some("Inactive".into());
    let messages = crate::payroll_archive_service::apply(&app.application, &pa, true).unwrap();
    assert!(
        !messages.iter().any(|m| m.contains("failed")),
        "{messages:?}"
    );
    let archived = delivery::capture(&conn, 1).unwrap();
    assert_eq!(archived.document, captured.document);
    assert!(archived.path.contains("Archived"));
    assert_eq!(delivery::bytes(&conn, &archived).unwrap(), bytes);
    assert!(!std::path::Path::new(&captured.path).exists());
    pa.employment_status = Some("Active".into());
    crate::payroll_archive_service::apply(&app.application, &pa, true).unwrap();
    let reactivated = delivery::capture(&conn, 1).unwrap();
    assert_eq!(reactivated.path, archived.path);
    assert!(reactivated.resend);
    let after_documents=conn.prepare("SELECT id,pdf_sha256,pdf_bytes FROM timesheet_documents WHERE payroll_timesheet_id=1 ORDER BY id").unwrap().query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,Vec<u8>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(before_documents, after_documents);
    let after_snapshots=conn.prepare("SELECT document_id,evidence,correction_id FROM sickness_document_evidence WHERE payroll_timesheet_id=1 ORDER BY document_id").unwrap().query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<i64>>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(snapshots, after_snapshots);
    assert_eq!(
        crate::payroll_evidence::lifecycle::stage(&conn, &sick::record(&conn, 1).unwrap()).unwrap(),
        crate::payroll_evidence::lifecycle::Stage::Settled
    );
    submit();
    assert_eq!(
        conn.query_row::<i64, _, _>(
            "SELECT count(*) FROM payroll_submissions WHERE payroll_timesheet_id=1",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row::<i64, _, _>("SELECT count(*) FROM sickness_attempt_evidence", [], |r| r
            .get(0))
            .unwrap(),
        3
    );
}

fn signature_images(path: &std::path::Path) -> usize {
    // printpdf's importer treats raw Flate pixels as encoded images and omits
    // them. Inspect the actual PDF image streams, excluding alpha-mask objects.
    let document = lopdf::Document::load(path).unwrap();
    let masks: HashSet<_> = document
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter_map(|s| s.dict.get(b"SMask").ok()?.as_reference().ok())
        .collect();
    document
        .objects
        .iter()
        .filter(|(id, o)| {
            !masks.contains(id)
                && o.as_stream().ok().is_some_and(|s| {
                    s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()) == Some(b"Image")
                })
        })
        .count()
}
#[test]
fn signature_drawn_immediate_future_use_correct_owner_and_immutable_resend() {
    let (dir, mut app, schedule) = fixture();
    let smtp = Smtp::new(&mut app);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let original = std::fs::read(path(&app, &schedule, 1)).unwrap();
    assert_eq!(signature_images(&path(&app, &schedule, 1)), 0);
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    let historical:Vec<u8>=db(&app).query_row("SELECT pdf_bytes FROM payroll_submissions WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=1)",[],|r|r.get(0)).unwrap();
    assert_eq!(historical, original);
    let bytes =
        crate::signature::rasterize(&[vec![egui::pos2(0.1, 0.5), egui::pos2(0.9, 0.5)]]).unwrap();
    let mut connection = db(&app);
    let pa_path = crate::signature::save_drawn(
        &mut connection,
        dir.path(),
        crate::signature::Owner::Pa(2),
        None,
        &bytes,
        true,
    )
    .unwrap();
    let employer_path = crate::signature::save_drawn(
        &mut connection,
        dir.path(),
        crate::signature::Owner::Employer(1),
        None,
        &bytes,
        true,
    )
    .unwrap();
    assert_ne!(pa_path, employer_path);
    assert_eq!(std::path::Path::new(&pa_path).file_name().unwrap(),"PA2 Test.png");
    assert_eq!(std::path::Path::new(&employer_path).file_name().unwrap(),"Test Employer.png");
    assert_eq!(generate(&mut app, &[2, 3]).completed(), 2);
    assert_eq!(signature_images(&path(&app, &schedule, 2)), 2);
    assert_eq!(signature_images(&path(&app, &schedule, 3)), 1);
    assert_eq!(generate(&mut app, &[2]).completed(), 1);
    assert_eq!(signature_images(&path(&app, &schedule, 2)), 2);
    assert_eq!(std::fs::read(path(&app, &schedule, 1)).unwrap(), original);
    let resend = email_batch(&mut app, &[1]);
    assert!(
        resend
            .choices
            .iter()
            .find(|c| c.id == 1)
            .unwrap()
            .intent
            .as_ref()
            .unwrap()
            .resend
    );
    assert_eq!(
        app.dispatch_timesheet_selection(&resend)
            .unwrap()
            .completed(),
        1
    );
    let preserved:Vec<u8>=connection.query_row("SELECT pdf_bytes FROM payroll_submissions WHERE payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=1)",[],|r|r.get(0)).unwrap();
    assert_eq!(preserved, original);
    let attempts: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM timesheet_delivery_attempts",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(attempts, 2);
    assert_eq!(smtp.messages.lock().unwrap().len(), 2);
}
#[test]
fn signature_invalid_configuration_offers_unsigned_without_losing_paths() {
    let (_dir, mut app, schedule) = fixture();
    let conn = db(&app);
    conn.execute_batch("UPDATE employers SET employer_signature='absent-employer.png'; UPDATE personal_assistants SET signature='absent-pa.png' WHERE id=1;").unwrap();
    app.begin_generation();
    let mut pending = app.pending_generation.take().unwrap();
    pending.selected_ids = vec![1];
    let issues = app.signature_issues(&pending).unwrap();
    assert_eq!(issues.len(), 2);
    pending
        .unsigned_signatures
        .insert(crate::signature::Owner::Employer(1));
    pending
        .unsigned_signatures
        .insert(crate::signature::Owner::Pa(1));
    assert!(app.signature_issues(&pending).unwrap().is_empty());
    assert_eq!(app.generate_selection(&pending).unwrap().completed(), 1);
    assert_eq!(signature_images(&path(&app, &schedule, 1)), 0);
    assert_eq!(
        crate::signature::Owner::Employer(1)
            .path(&conn)
            .unwrap()
            .as_deref(),
        Some("absent-employer.png")
    );
    assert_eq!(
        crate::signature::Owner::Pa(1)
            .path(&conn)
            .unwrap()
            .as_deref(),
        Some("absent-pa.png")
    );
    let smtp = Smtp::new(&mut app);
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(smtp.messages.lock().unwrap().len(), 1);
}
#[test]
fn signature_decode_failure_preserves_registered_candidate_and_pdf() {
    let (dir, mut app, schedule) = fixture();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let before = std::fs::read(path(&app, &schedule, 1)).unwrap();
    let evidence = facts(&app, 1);
    let invalid = dir.path().join("invalid-signature.jpg");
    std::fs::write(&invalid, b"damaged").unwrap();
    db(&app)
        .execute(
            "UPDATE personal_assistants SET signature=?1 WHERE id=1",
            [invalid.to_str().unwrap()],
        )
        .unwrap();
    assert!(matches!(
        generate(&mut app, &[1]).entries[0].2,
        ProductionOutcome::Failed(_)
    ));
    assert_eq!(std::fs::read(path(&app, &schedule, 1)).unwrap(), before);
    assert_eq!(facts(&app, 1), evidence);
}
