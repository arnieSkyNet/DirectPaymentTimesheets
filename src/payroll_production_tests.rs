use super::*;
use rusqlite::params;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

fn fixture() -> (
    crate::private_storage::PrivateTempDir,
    DirectPaymentApp,
    PayrollSchedule,
) {
    let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
    let db = crate::payroll_evidence::open(&app).unwrap();
    db.execute_batch("INSERT INTO employers(id,name,email) VALUES (1,'Test Employer','employer@example.test');
        INSERT INTO payroll_provider(id,name,payroll_department_email) VALUES (1,'Payroll','payroll@example.test');
        INSERT INTO payroll_schedules(id,payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES (1,'2026/27',1,'01/04/2026','20/04/2026','30/04/2026','created');").unwrap();
    for pa in 1..=5 {
        db.execute("INSERT INTO personal_assistants(id,first_name,surname,email,start_date,employment_status) VALUES (?1,?2,'Test',?3,'01/01/2026','Active')",params![pa,format!("PA{pa}"),format!("pa{pa}@example.test")]).unwrap();
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
fn db(app: &DirectPaymentApp) -> crate::database::DatabaseConnection {
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
        app.capture_email_confirmation(&mut batch).unwrap();
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
        app.application.context.config.email.smtp_security =
            Some(crate::config::SmtpSecurity::Plaintext);
        app.application.context.config.email.smtp_host = "127.0.0.1".into();
        app.application.context.config.email.allow_remote_plaintext = true; // Synthetic loopback fixture.
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
    app.capture_email_confirmation(&mut pending).unwrap();
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
fn direct_amendment_retains_original_pdf_work_and_note_through_selected_workflow() {
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
    save_feature2_hours(&app,3.0);
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
    assert!(!pending.selected_ids.contains(&1));
    assert!(!pending.selected_ids.contains(&2));
    assert!(!pending.choices.iter().any(|c| c.id == 3));
    assert_eq!(generate(&mut app, &[1, 2]).completed(), 2);
    let smtp = Smtp::new(&mut app);
    app.begin_retained_timesheet_batch();
    let mut batch=app.pending_email_batch.take().unwrap();
    batch.selected_personal_assistant_ids=vec![1,2];
    batch.stage=EmailBatchNoteStage::ConfirmDispatch;
    app.capture_email_confirmation(&mut batch).unwrap();
    batch.resend_acknowledged=true;
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
    app.capture_email_confirmation(&mut batch).unwrap();
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

fn payslip_selection_fixture() -> (
    crate::private_storage::PrivateTempDir,
    DirectPaymentApp,
    PayrollSchedule,
) {
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
    app.capture_email_confirmation(&mut batch).unwrap();
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
        app.capture_email_confirmation(&mut batch).unwrap();
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
    app.capture_email_confirmation(&mut batch).unwrap();
    assert_eq!(app.dispatch_payslip_selection(&batch).unwrap(), 1);
    assert!(app.dispatch_payslip_selection(&batch).is_err());
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
    batch = payslip_batch(&mut app, &[4]);
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
        app.begin_email_batch(PayrollEmailKind::Payslip);
        let batch=app.pending_email_batch.take().unwrap();
        assert!(app.dispatch_payslip_selection(&batch).is_err());
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
    assert!(app.validate_email_confirmation(&batch).is_err());
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
    assert!(app.validate_email_confirmation(&original).is_err());
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
    assert!(app.validate_email_confirmation(&batch).is_err());
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
    assert!(rendered.contains("To: payroll@example.test"), "{rendered}");
    assert!(rendered.contains("CC: employer@example.test"), "{rendered}");
    assert!(rendered.contains("BCC:"), "{rendered}");
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
    save_feature2_hours(&app, 3.0);
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
    assert_eq!(
        app.application.context.config.folders.pdf_output,
        _dir.path().canonicalize().unwrap().join("pdfs")
    );
    std::fs::create_dir_all(&app.application.context.config.folders.payslip_folder).unwrap();
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
    let attempts_before: Vec<(i64, String, String)> = conn.prepare("SELECT id,pdf_path,outcome FROM timesheet_delivery_attempts WHERE payroll_timesheet_id=1 ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
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
        !messages
            .iter()
            .any(|m| m.contains("failed") || m.contains("pending") || m.contains("refused")),
        "{messages:?}"
    );
    let archived = delivery::capture(&conn, 1).unwrap();
    assert_eq!(archived.document, captured.document);
    let expected = app
        .application
        .context
        .config
        .folders
        .pdf_output
        .canonicalize()
        .unwrap()
        .join("2026 to 2027")
        .join("Archived")
        .join(std::path::Path::new(&captured.path).file_name().unwrap());
    assert_eq!(std::path::Path::new(&archived.path), expected);
    assert_eq!(std::fs::read(&expected).unwrap(), bytes);
    assert_eq!(
        crate::payroll_document_repository::file_digest(&expected).unwrap(),
        captured.digest
    );
    assert_eq!(delivery::bytes(&conn, &archived).unwrap(), bytes);
    assert!(!std::path::Path::new(&captured.path).exists());
    assert_eq!(
        conn.query_row::<i64, _, _>(
            "SELECT count(*) FROM payroll_submissions WHERE payroll_timesheet_id=1 AND pdf_path=?1",
            [captured.path.as_str()],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
    let attempts_after: Vec<(i64, String, String)> = conn.prepare("SELECT id,pdf_path,outcome FROM timesheet_delivery_attempts WHERE payroll_timesheet_id=1 ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(attempts_before, attempts_after);
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

#[test]
fn stage8_timesheet_cc_enabled_disabled_first_corrected_and_resend_exact_approval() {
    for enabled in [true, false] {
        let (_dir, mut app, _schedule) = fixture();
        app.application.context.config.email.timesheet_employer_cc = enabled;
        let mut sent_messages = Vec::new();
        generate(&mut app, &[1]);
        for phase in 0..3 {
            if phase == 2 {
                assert_eq!(generate(&mut app, &[1]).completed(), 1);
            }
            let batch = email_batch(&mut app, &[1]);
            let preview = batch.approved.get(&1).unwrap();
            assert_eq!(preview.to, "payroll@example.test");
            assert_eq!(
                preview.cc.as_deref(),
                enabled.then_some("employer@example.test")
            );
            assert_eq!(preview.bcc.as_deref(), Some("pa1@example.test"));
            assert_eq!(batch.choices[0].intent.as_ref().unwrap().resend, phase == 1);
            sent_messages.push(stage8_accept_timesheet(&app, &batch));
        }
        assert_eq!(sent_messages.len(), 3);
        for (message, envelope) in &sent_messages {
            assert_eq!(message.contains("Cc: employer@example.test"), enabled);
            assert!(message.contains("To: payroll@example.test"));
            assert!(envelope.contains(&"payroll@example.test".to_string()));
            assert!(envelope.contains(&"pa1@example.test".to_string()));
            assert_eq!(
                envelope.contains(&"employer@example.test".to_string()),
                enabled
            );
            assert_eq!(envelope.len(), if enabled { 3 } else { 2 });
        }
        let attempts: Vec<String> = db(&app)
            .prepare("SELECT recipients FROM timesheet_delivery_attempts ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(attempts.len(), 3);
        for recipients in attempts {
            assert_eq!(recipients.contains("CC: employer@example.test"), enabled);
        }
    }
}

#[test]
fn stage8_changed_cc_setting_invalidates_approved_first_send_and_resend() {
    let (_dir, mut app, _schedule) = fixture();
    generate(&mut app, &[1]);
    let batch = email_batch(&mut app, &[1]);
    app.application.context.config.email.timesheet_employer_cc = false;
    assert!(app.validate_email_confirmation(&batch).is_err());
    assert!(app.dispatch_timesheet_selection(&batch).is_err());
    assert_eq!(
        db(&app)
            .query_row(
                "SELECT count(*) FROM timesheet_delivery_attempts",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let batch = email_batch(&mut app, &[1]);
    stage8_accept_timesheet(&app, &batch);
    let resend = email_batch(&mut app, &[1]);
    app.application.context.config.email.timesheet_employer_cc = true;
    assert!(app.dispatch_timesheet_selection(&resend).is_err());
    assert_eq!(
        db(&app)
            .query_row(
                "SELECT count(*) FROM timesheet_delivery_attempts",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn stage8_payslip_cc_setting_never_changes_pa_bcc_routing_and_exact_approval() {
    for enabled in [true, false] {
        let (_dir, mut app, _schedule) = payslip_selection_fixture();
        app.application.context.config.email.timesheet_employer_cc = enabled;
        let batch = payslip_batch(&mut app, &[1]);
        let preview = batch.approved.get(&1).unwrap();
        assert_eq!(preview.to, "pa1@example.test");
        assert!(preview.cc.is_none());
        assert_eq!(preview.bcc.as_deref(), Some("employer@example.test"));
        assert_eq!(batch.approved_bundles[&1].digests.len(), 3);
        let (message, envelope) = stage8_accept_payslip(&app, &batch);
        assert!(!message.contains("Cc:"));
        assert!(!message.contains("payroll@example.test"));
        assert_eq!(
            envelope,
            vec![
                "pa1@example.test".to_string(),
                "employer@example.test".to_string()
            ]
        );
    }
}

#[test]
fn stage8_payslip_final_approval_rejects_changed_addresses_file_settings_and_documents() {
    for mode in 0..5 {
        let (dir, mut app, _schedule) = payslip_selection_fixture();
        let batch = payslip_batch(&mut app, &[1]);
        match mode {
            0 => {
                db(&app)
                    .execute(
                        "UPDATE personal_assistants SET email='other@example.test' WHERE id=1",
                        [],
                    )
                    .unwrap();
            }
            1 => {
                db(&app)
                    .execute("UPDATE employers SET email='other@example.test'", [])
                    .unwrap();
            }
            2 => {
                std::fs::write(dir.path().join("p45-PA1.pdf"), b"%PDF-1.4 changed").unwrap();
            }
            3 => {
                app.application.context.config.email.timesheet_employer_cc = false;
            }
            _ => {
                db(&app).execute("UPDATE imported_payroll_documents SET history_state='external' WHERE personal_assistant_id=1 AND document_type='p60'",[]).unwrap();
            }
        }
        assert!(app.dispatch_payslip_selection(&batch).is_err());
        assert_eq!(
            db(&app)
                .query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db(&app)
                .query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn stage8_other_instance_saved_settings_change_invalidates_confirmation() {
    let (_dir, mut app, _schedule) = fixture();
    generate(&mut app, &[1]);
    let batch = email_batch(&mut app, &[1]);
    let path = app
        .application
        .context
        .environment
        .data_dir
        .join("config.toml");
    let mut config = crate::config::AppConfig::default();
    config.email.timesheet_employer_cc = false;
    config.save(&path).unwrap();
    assert!(app.validate_email_confirmation(&batch).is_err());
    assert!(app.dispatch_timesheet_selection(&batch).is_err());
    assert_eq!(
        db(&app)
            .query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

// Inject typed transport results into the production ledger using the real approved
// MIME/envelope. These deterministic tests need no sockets or external SMTP.
fn stage8_accept_timesheet(
    app: &DirectPaymentApp,
    batch: &PendingEmailBatch,
) -> (String, Vec<String>) {
    app.validate_email_confirmation(batch).unwrap();
    let intent = batch
        .choices
        .iter()
        .find(|c| c.id == 1)
        .unwrap()
        .intent
        .as_ref()
        .unwrap();
    let preview = batch.approved.get(&1).unwrap();
    let db = db(app);
    let bytes = crate::timesheet_delivery::bytes(&db, intent).unwrap();
    let message_id = format!("<{}@test.local>", intent.intent_id);
    let mut result = None;
    crate::timesheet_delivery::execute(
        &db,
        intent,
        &crate::payroll_delivery::recipients(preview),
        &message_id,
        &bytes,
        |_| app.validate_email_confirmation(batch),
        || {
            let message =
                crate::email_service::prepare_timesheet_message(preview, &bytes, &message_id)
                    .unwrap();
            result = Some((
                String::from_utf8(message.formatted()).unwrap(),
                message
                    .envelope()
                    .to()
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ));
            Ok(())
        },
    )
    .unwrap();
    result.unwrap()
}
fn stage8_accept_payslip(
    app: &DirectPaymentApp,
    batch: &PendingEmailBatch,
) -> (String, Vec<String>) {
    app.validate_email_confirmation(batch).unwrap();
    let schedule = app.validated_schedule_for_email_batch(batch).unwrap();
    let mut db = db(app);
    let mut result = None;
    crate::payroll_delivery::dispatch(
        &mut db,
        1,
        schedule.as_ref().map(
            |s| crate::payslip_delivery_service::PayslipDeliveryIdentity {
                personal_assistant_id: 1,
                payroll_year: &s.payroll_year,
                cycle_number: s.cycle_number,
            },
        ),
        &batch.approved_bundles[&1],
        |_| app.payslip_approval(1, schedule.as_ref()),
        |preview, bytes, message_id| {
            let message =
                crate::email_service::prepare_bundle_message(preview, bytes, message_id).unwrap();
            result = Some((
                String::from_utf8(message.formatted()).unwrap(),
                message
                    .envelope()
                    .to()
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ));
            Ok(())
        },
    )
    .unwrap();
    result.unwrap()
}

#[test]
fn stage8_timesheet_pa_bcc_stays_optional_with_employer_cc_enabled_or_disabled() {
    for enabled in [true,false] {
        let (_dir,mut app,_schedule)=fixture();
        app.application.context.config.email.timesheet_employer_cc=enabled;
        db(&app).execute("UPDATE personal_assistants SET email=NULL WHERE id=1",[]).unwrap();
        assert_eq!(generate(&mut app,&[1]).completed(),1);
        let batch=email_batch(&mut app,&[1]);
        assert!(batch.approved[&1].bcc.is_none());
        let (message,envelope)=stage8_accept_timesheet(&app,&batch);
        assert!(message.contains("To: payroll@example.test"));
        assert_eq!(envelope,if enabled {vec!["payroll@example.test".to_string(),"employer@example.test".to_string()]} else {vec!["payroll@example.test".to_string()]});
    }
}

#[test]
fn regional_holiday_generation_approval_rejects_changed_region_under_lock() {
    let (_dir, mut app, _) = fixture();
    app.begin_generation();
    let mut pending = app.pending_generation.take().unwrap();
    pending.selected_ids = vec![1];
    let before = facts(&app, 1);
    app.application.context.config.payroll.public_holiday_region =
        crate::public_holidays::PublicHolidayRegion::Scotland;
    let report = app.generate_selection(&pending).unwrap();
    assert!(matches!(report.entries[0].2, ProductionOutcome::Failed(_)));
    assert_eq!(facts(&app, 1), before);
}

#[test]
fn regional_holiday_dispatch_rejects_changed_dates_without_creating_attempt_or_sending() {
    let (_dir, mut app, _) = fixture();
    let cache = app.application.context.environment.cache_dir.clone();
    crate::public_holidays::tests::seed(&cache, 100);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let batch = email_batch(&mut app, &[1]);
    let before = facts(&app, 1);
    let path = cache.join("bank-holidays-v1.json");
    // Valid replacement with a published date inside the approved period changed.
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let payload = value["payload"]
        .as_str()
        .unwrap()
        .replace("2026-04-06", "2026-04-07");
    value["payload"] = payload.clone().into();
    value["sha256"] = crate::payroll_delivery::digest(payload.as_bytes()).into();
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(app.validate_email_confirmation(&batch).is_err());
    let report = app.dispatch_timesheet_selection(&batch).unwrap();
    assert!(matches!(report.entries[0].2, ProductionOutcome::Failed(_)));
    assert_eq!(facts(&app, 1), before);
}

#[test]
fn regional_holiday_refresh_timestamp_does_not_invalidate_email_approval() {
    let (_dir, mut app, _) = fixture();
    let cache = app.application.context.environment.cache_dir.clone();
    crate::public_holidays::tests::seed(&cache, 100);
    generate(&mut app, &[1]);
    let batch = email_batch(&mut app, &[1]);
    crate::public_holidays::tests::seed(&cache, 200);
    app.validate_email_confirmation(&batch).unwrap();
}

#[test]
fn regional_holiday_configuration_from_another_instance_requires_fresh_review() {
    let (_dir, mut app, schedule) = fixture();
    app.application.save_config().unwrap();
    let start = crate::date_utils::parse_legacy(&schedule.first_week_commencing).unwrap();
    let binding = crate::public_holidays::for_application(&app.application, start).binding;
    let path = app
        .application
        .context
        .environment
        .data_dir
        .join("config.toml");
    let mut other = crate::config::AppConfig::load(&path).unwrap();
    other.payroll.public_holiday_region =
        crate::public_holidays::PublicHolidayRegion::NorthernIreland;
    other.save(&path).unwrap();
    assert!(crate::public_holidays::verify(&app.application, start, &binding).is_err());
}

#[test]
fn regional_holiday_submitted_pdf_and_evidence_survive_region_change_and_resend() {
    let (_dir, mut app, schedule) = fixture();
    let smtp = Smtp::new(&mut app);
    generate(&mut app, &[1]);
    let batch = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&batch)
            .unwrap()
            .completed(),
        1
    );
    let pdf = std::fs::read(path(&app, &schedule, 1)).unwrap();
    let original: Vec<(i64, String, Vec<u8>)> = db(&app).prepare("SELECT id,pdf_sha256,pdf_bytes FROM timesheet_documents WHERE payroll_timesheet_id=1 ORDER BY id").unwrap().query_map([], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    app.application.context.config.payroll.public_holiday_region =
        crate::public_holidays::PublicHolidayRegion::Scotland;
    let fresh = email_batch(&mut app, &[1]);
    assert_eq!(
        app.dispatch_timesheet_selection(&fresh)
            .unwrap()
            .completed(),
        1
    );
    assert_eq!(std::fs::read(path(&app, &schedule, 1)).unwrap(), pdf);
    let retained: Vec<(i64, String, Vec<u8>)> = db(&app).prepare("SELECT id,pdf_sha256,pdf_bytes FROM timesheet_documents WHERE payroll_timesheet_id=1 ORDER BY id").unwrap().query_map([], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(retained, original);
    assert_eq!(smtp.count(), 2);
}

fn save_feature2_hours(app:&DirectPaymentApp, hours:f64) -> i64 {
    let record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",1,1).unwrap().unwrap();
    let mut weeks=app.application.payroll_timesheet_repository.get_weeks(record.id).unwrap();
    weeks[0].worked_hours=hours;
    let adjustments=weeks.iter().map(|w|crate::payroll_worked_item_repository::ManualHoursAdjustment{week_number:w.week_number,adjustment_minutes:0,reason:None}).collect::<Vec<_>>();
    let holidays=app.application.payroll_timesheet_repository.get_public_holidays(record.id).unwrap();
    let leave=app.application.payroll_timesheet_repository.get_annual_leave(record.id).unwrap();
    assert!(app.application.payroll_timesheet_repository.save_preparation_atomically(&record,&weeks,&holidays,&leave,&adjustments,"synthetic amendment").unwrap());
    record.id
}
#[test]
fn feature2_submitted_and_settled_amendments_regenerate_without_sending_or_rewriting_history() {
    use crate::payroll_worked_item_repository::SnapshotState;
    for settled in [false,true] {
        let (_dir,mut app,schedule)=fixture();
        assert_eq!(generate(&mut app,&[1]).completed(),1);
        let file=path(&app,&schedule,1);
        let record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",1,1).unwrap().unwrap();
        crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,record.id,1,"2026/27",1,&file,"fixture",||Ok(())).unwrap();
        let original=std::fs::read(&file).unwrap();
        let conn=db(&app);
        if settled {conn.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',1,'payslip','fixture-settlement')",[]).unwrap();}
        let sid:i64=conn.query_row("SELECT MAX(id) FROM payroll_submissions",[],|r|r.get(0)).unwrap();
        let sent:String=conn.query_row("SELECT sent_at FROM payroll_timesheet_email_status WHERE personal_assistant_id=1 AND email_type='timesheet'",[],|r|r.get(0)).unwrap();
        for hours in [3.0,1.5,4.0] {
            save_feature2_hours(&app,hours);
            assert!(app.application.payroll_worked_item_repository.snapshot_metadata(record.id).unwrap().is_none());
            let report=generate(&mut app,&[1]);assert_eq!(report.completed(),1,"{:?}",report.entries);
            let version=crate::timesheet_amendments::latest_generated(&conn,record.id).unwrap().unwrap();
            assert_eq!(version.totals,[ (hours*60.0) as i64,0,0,0]);
            assert_eq!(app.application.payroll_worked_item_repository.snapshot_metadata(record.id).unwrap().unwrap().state,SnapshotState::Candidate);
            crate::payroll_snapshot_service::verify_preview_or_test_attachment(&app.application.payroll_worked_item_repository,record.id,&file).unwrap();
            crate::payroll_evidence::lifecycle::verify_candidate_evidence(&conn,record.id).unwrap();
            let current=std::fs::read(&file).unwrap();assert_ne!(current,original);assert!(current.starts_with(b"%PDF"));
            use sha2::Digest;
            let digest:String=conn.query_row("SELECT pdf_sha256 FROM timesheet_documents WHERE id=?1",[version.document],|r|r.get(0)).unwrap();
            assert_eq!(digest,format!("{:x}",sha2::Sha256::digest(&current)));
            assert_eq!(conn.query_row("SELECT pdf_bytes FROM timesheet_documents WHERE id=?1",[version.document],|r|r.get::<_,Vec<u8>>(0)).unwrap(),current);
            assert_eq!(conn.query_row("SELECT COUNT(*) FROM payroll_submissions",[],|r|r.get::<_,i64>(0)).unwrap(),1);
            assert_eq!(conn.query_row("SELECT sent_at FROM payroll_timesheet_email_status WHERE personal_assistant_id=1 AND email_type='timesheet'",[],|r|r.get::<_,String>(0)).unwrap(),sent);
            assert_eq!(conn.query_row("SELECT pdf_bytes FROM payroll_submissions WHERE id=?1",[sid],|r|r.get::<_,Vec<u8>>(0)).unwrap(),original);
        }
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM timesheet_amendment_versions WHERE event='save'",[],|r|r.get::<_,i64>(0)).unwrap(),3);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM timesheet_amendment_versions WHERE event='generated'",[],|r|r.get::<_,i64>(0)).unwrap(),3);
        assert!(conn.execute("DELETE FROM timesheet_amendment_versions",[]).is_err());
        assert!(conn.execute("UPDATE timesheet_amendment_versions SET actor='changed'",[]).is_err());
        // A replacement is sent only when the separate send operation is invoked.
        crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,record.id,1,"2026/27",1,&file,"fixture",||Ok(())).unwrap();
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM payroll_submissions",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(conn.query_row("SELECT supersedes_id FROM payroll_submissions ORDER BY id DESC LIMIT 1",[],|r|r.get::<_,i64>(0)).unwrap(),sid);
    }
}

#[test]
fn feature2_accepted_amendment_drives_future_carry_without_double_payment() {
    let (_dir,mut app,schedule)=fixture();assert_eq!(generate(&mut app,&[1]).completed(),1);
    let original_record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",1,1).unwrap().unwrap();
    let file=path(&app,&schedule,1);
    crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,original_record.id,1,"2026/27",1,&file,"fixture",||Ok(())).unwrap();
    save_feature2_hours(&app,1.5);
    assert!(crate::timesheet_amendments::latest_generated(&db(&app),original_record.id).unwrap().is_none(),"saved-only draft is not a regenerated authority");
    assert_eq!(generate(&mut app,&[1]).completed(),1);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository, original_record.id, 1,
        "2026/27", 1, &path(&app, &schedule, 1), "synthetic replacement", || Ok(()),
    ).unwrap();
    let conn=db(&app);
    conn.execute("INSERT INTO payroll_schedules(id,payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES(6,'2026/27',2,'29/04/2026','20/05/2026','30/05/2026','synthetic')",[]).unwrap();
    let future=app.application.payroll_timesheet_repository.insert("2026/27",2,1,None,"synthetic").unwrap();
    app.application.payroll_timesheet_repository.create_missing_weeks(future,&["29/04/2026".into(),"06/05/2026".into(),"13/05/2026".into(),"20/05/2026".into()],&[0.;4]).unwrap();
    let record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",2,1).unwrap().unwrap();
    let dates=["29/04/2026","06/05/2026","13/05/2026","20/05/2026"].map(|d|crate::date_utils::parse_legacy(d).unwrap());
    let unchanged=crate::payroll_evidence::reconciliation::calculate(&app.application,&record,&dates,None).unwrap();
    assert_eq!(unchanged.week_totals_minutes,[0;4],"historical CSV hours must not override the corrected version");
    conn.execute("UPDATE timesheets SET worked_minutes=150,end_time='2026-04-02T11:30:00' WHERE id=1",[]).unwrap();
    for amended in [1.5,1.0] {
        if amended==1.0 {
            save_feature2_hours(&app,amended);assert_eq!(generate(&mut app,&[1]).completed(),1);
            assert_eq!(crate::payroll_evidence::reconciliation::calculate(&app.application,&record,&dates,None).unwrap().week_totals_minutes,[60,0,0,0]);
            crate::payroll_snapshot_service::send_production_candidate(
                &app.application.payroll_worked_item_repository, original_record.id, 1,
                "2026/27", 1, &path(&app, &schedule, 1), "synthetic replacement", || Ok(()),
            ).unwrap();
            assert_eq!(crate::payroll_evidence::reconciliation::calculate(&app.application,&record,&dates,None).unwrap().week_totals_minutes,[0;4]);
            conn.execute("UPDATE timesheets SET worked_minutes=180,end_time='2026-04-02T12:00:00' WHERE id=1",[]).unwrap();
        }
        let difference=if amended==1.5 {60} else {120};
        for _ in 0..2 {
            let hours=crate::payroll_evidence::reconciliation::calculate(&app.application,&record,&dates,None).unwrap();
            assert_eq!(hours.week_totals_minutes.iter().sum::<i64>(),difference);
            assert_eq!(hours.previous_cycle_minutes,difference,"original 2-hour estimate must not override the corrected estimate");
        }
    }
    // A later actual-hours reduction uses the same corrected baseline. Positive
    // and negative remaining amounts net once through the established engine.
    conn.execute("UPDATE timesheets SET worked_minutes=0,end_time='2026-04-02T09:00:00' WHERE id=1",[]).unwrap();
    conn.execute("INSERT INTO timesheets(id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount) VALUES(6,1,'Synthetic','2026-04-30T09:00:00','2026-04-30T12:00:00',0,180,12,36)",[]).unwrap();
    let held = crate::payroll_evidence::reconciliation::calculate(&app.application, &record, &dates, None).unwrap();
    assert_eq!(held.previous_cycle_minutes, 120, "the aggregate deduction must stay held");
    let pending = crate::payroll_evidence::reconciliation::changes(&app.application, &original_record).unwrap();
    assert!(pending.requires_complete);
    assert!(crate::payroll_evidence::reconciliation::carry(
        &app.application, &original_record, &pending.signature, false,
    ).is_err());
    crate::payroll_evidence::reconciliation::carry(
        &app.application, &original_record, &pending.signature, true,
    ).unwrap();
    let payable=crate::payroll_evidence::reconciliation::calculate(&app.application,&record,&dates,None).unwrap();
    assert_eq!(payable.week_totals_minutes.iter().sum::<i64>(),120);
    assert_eq!(payable.previous_cycle_minutes,-60);
    let weeks=app.application.payroll_timesheet_repository.get_weeks(record.id).unwrap();
    let week_ids=std::array::from_fn(|i|weeks[i].id);
    let future_path=app.application.context.environment.data_dir.join("synthetic-future.pdf");
    crate::payroll_snapshot_service::publish_candidate(&app.application.payroll_worked_item_repository,
        crate::payroll_snapshot_service::CandidatePublication{payroll_timesheet_id:record.id,items:&payable.snapshot_items,final_pdf_path:&future_path,generated_at:"synthetic",previous_cycle_minutes:payable.previous_cycle_minutes,week_ids:&week_ids,week_totals_minutes:&payable.week_totals_minutes},|p|{std::fs::write(p,b"%PDF synthetic future")?;Ok(())}).unwrap();
    crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,record.id,1,"2026/27",2,&future_path,"fixture",||Ok(())).unwrap();
    conn.execute("INSERT INTO payroll_schedules(id,payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES(7,'2026/27',3,'27/05/2026','20/06/2026','30/06/2026','synthetic')",[]).unwrap();
    let next_id=app.application.payroll_timesheet_repository.insert("2026/27",3,1,None,"synthetic").unwrap();
    let text=["27/05/2026","03/06/2026","10/06/2026","17/06/2026"].map(str::to_string);
    app.application.payroll_timesheet_repository.create_missing_weeks(next_id,&text,&[0.;4]).unwrap();
    let next=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",3,1).unwrap().unwrap();
    let next_dates=text.map(|d|crate::date_utils::parse_legacy(&d).unwrap());
    assert_eq!(crate::payroll_evidence::reconciliation::calculate(&app.application,&next,&next_dates,None).unwrap().week_totals_minutes,[0;4],"already applied carry and source hours must not be paid again");
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM payroll_submissions WHERE payroll_timesheet_id=?1",[original_record.id],|r|r.get::<_,i64>(0)).unwrap(),3);
}

#[test]
fn feature2_failed_version_publication_never_advances_authority_or_allows_stale_send() {
    let (_dir,mut app,schedule)=fixture();assert_eq!(generate(&mut app,&[1]).completed(),1);
    let original=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",1,1).unwrap().unwrap();
    let file=path(&app,&schedule,1);
    crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,original.id,1,"2026/27",1,&file,"fixture",||Ok(())).unwrap();
    save_feature2_hours(&app,3.);assert_eq!(generate(&mut app,&[1]).completed(),1);
    let conn=db(&app);let previous=crate::timesheet_amendments::latest_generated(&conn,original.id).unwrap().unwrap();
    save_feature2_hours(&app,4.);
    conn.execute_batch("CREATE TRIGGER fail_version BEFORE INSERT ON timesheet_amendment_versions WHEN NEW.event='generated' BEGIN SELECT RAISE(ABORT,'synthetic version failure'); END;").unwrap();
    assert_eq!(generate(&mut app,&[1]).completed(),0);
    assert_eq!(crate::timesheet_amendments::latest_generated(&conn,original.id).unwrap().unwrap().document,previous.document);
    assert!(app.application.payroll_worked_item_repository.snapshot_metadata(original.id).unwrap().is_none());
    assert!(crate::timesheet_delivery::capture(&conn,original.id).is_err());
    conn.execute_batch("DROP TRIGGER fail_version").unwrap();
    assert_eq!(generate(&mut app,&[1]).completed(),1);
    assert_eq!(crate::timesheet_amendments::latest_generated(&conn,original.id).unwrap().unwrap().totals[0],240);
}

#[derive(Clone,Copy,Debug)]
enum Feature2Ingest { Folder, Clipboard, Logger }
fn feature2_source_csv(days:&[&str]) -> String {
    let mut text="Fictional Hours Keeper export\nPA,Start,End,Break,Worked,Rate,Amount,Note\n".to_string();
    for day in days {text.push_str(&format!("PA1 Test,{day} at 09:00:00,{day} at 11:00:00,0h 00m,2h 00m,12,24,source note\n"));}
    text
}
fn feature2_ingest_actuals(app:&mut DirectPaymentApp, mode:Feature2Ingest, days:&[&str], name:&str) {
    match mode {
        Feature2Ingest::Folder=>{
            crate::private_storage::write_new(&app.application.context.config.folders.csv_import.join(name),feature2_source_csv(days).as_bytes()).unwrap();
            app.begin_hours_csv_import();
            let summary=app.last_import.as_ref().unwrap();assert!(!summary.has_failures(),"{:?}",summary.failure_messages);
            assert_eq!(summary.rows_imported,days.len() as i64);
            assert!(app.clipboard_import.is_none(),"current-cycle CSV covers this PA");
        }
        Feature2Ingest::Clipboard=>{
            app.begin_hours_csv_import();
            let session=app.clipboard_import.as_mut().expect("historic CSV files must not hide the next-cycle clipboard prompt");
            assert_eq!(session.outstanding.iter().map(|pa|pa.id).collect::<Vec<_>>(),vec![1]);
            session.accepting=true;
            let ctx=egui::Context::default();
            super::clipboard_hours_ui_tests::click(&ctx,app,"Import from Clipboard");
            super::clipboard_hours_ui_tests::draw(&ctx,app,vec![]);
            super::clipboard_hours_ui_tests::draw(&ctx,app,vec![egui::Event::Paste(feature2_source_csv(days))]);
            super::clipboard_hours_ui_tests::click(&ctx,app,"Import");
            assert!(app.clipboard_import.is_none());
            let summary=app.last_import.as_ref().unwrap();assert!(!summary.has_failures(),"{:?}",summary.failure_messages);
            assert_eq!(summary.rows_imported,days.len() as i64);
        }
        Feature2Ingest::Logger=>{
            for day in days {
                let start=crate::csv_import::parse_supported_timestamp(&format!("{day} at 09:00:00")).unwrap();
                let end=crate::csv_import::parse_supported_timestamp(&format!("{day} at 11:00:00")).unwrap();
                let shift=app.application.direct_shift_repository.clock_in(1,start,"synthetic cross-cycle").unwrap();
                app.application.direct_shift_repository.complete(shift.id,end,0,Some("synthetic actual hours"),"synthetic cross-cycle").unwrap();
            }
        }
    }
}
fn feature2_cycle_schedule(app:&DirectPaymentApp,cycle:i64,first:&str,pay:&str)->PayrollSchedule {
    db(app).execute("INSERT INTO payroll_schedules(payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES('2026/27',?1,?2,?3,?3,'synthetic')",params![cycle,first,pay]).unwrap();
    app.application.payroll_schedule_repository.get_for_year_and_cycle("2026/27",cycle).unwrap().unwrap()
}
fn feature2_assert_next_cycles_accept_actuals(mode:Feature2Ingest) {
    for estimate in [3.0,5.0,1.5] {
        let (_dir,mut app,original_schedule)=fixture();let conn=db(&app);
        conn.execute_batch("UPDATE personal_assistants SET employment_status=CASE id WHEN 1 THEN 'Active' ELSE 'Inactive' END; UPDATE timesheets SET pa_name='PA1 Test',start_time='02/04/2026 at 09:00:00',end_time='02/04/2026 at 11:00:00' WHERE id=1;").unwrap();
        crate::private_storage::ensure_private_dir(&app.application.context.config.folders.csv_import).unwrap();
        // This CSV remains in the import folder across every later cycle.
        crate::private_storage::write_new(&app.application.context.config.folders.csv_import.join("historic.csv"),feature2_source_csv(&["02/04/2026"]).as_bytes()).unwrap();
        let initial=app.application.import_csv().unwrap();assert!(!initial.has_failures());assert_eq!(initial.rows_skipped,1);assert_eq!(initial.rows_imported,0);
        let original_generation=generate(&mut app,&[1]);assert_eq!(original_generation.completed(),1,"{:?}",original_generation.entries);
        let original=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",1,1).unwrap().unwrap();
        let original_pdf=path(&app,&original_schedule,1);
        crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,original.id,1,"2026/27",1,&original_pdf,"synthetic",||Ok(())).unwrap();
        let before:Vec<u8>=conn.query_row("SELECT pdf_bytes FROM payroll_submissions WHERE payroll_timesheet_id=?1 ORDER BY id LIMIT 1",[original.id],|r|r.get(0)).unwrap();
        if estimate==5.0 {conn.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',1,'payslip','synthetic historical settlement')",[]).unwrap();}
        save_feature2_hours(&app,estimate);assert_eq!(generate(&mut app,&[1]).completed(),1);
        crate::payroll_snapshot_service::send_production_candidate(
            &app.application.payroll_worked_item_repository, original.id, 1,
            "2026/27", 1, &path(&app, &original_schedule, 1), "synthetic replacement", || Ok(()),
        ).unwrap();
        let mut authority=crate::timesheet_amendments::latest_generated(&conn,original.id).unwrap().unwrap();
        let next=feature2_cycle_schedule(&app,2,"29/04/2026","30/05/2026");
        app.operational_payroll_period.select(&next,None);
        feature2_ingest_actuals(&mut app,mode,&["05/04/2026","02/05/2026"],"cycle-two.csv");
        // Real acquisition always records source audit events. Those events must
        // not be mistaken for an instruction to suppress financial reconciliation.
        if estimate==5.0 {
            let notice=crate::payroll_evidence::reconciliation::historical_save_notice(&app.application,1,crate::date_utils::parse_legacy("05/04/2026").unwrap()).unwrap().unwrap();
            assert!(!notice.contains("approve"));assert!(notice.contains("does not lock"));
        }
        let source_events:i64=conn.query_row("SELECT COUNT(*) FROM shift_change_events",[],|r|r.get(0)).unwrap();assert!(source_events>=2);
        if estimate == 5.0 {
            use crate::payroll_evidence::reconciliation;
            let pending = reconciliation::changes(&app.application, &original).unwrap();
            assert!(pending.requires_complete);
            reconciliation::carry(&app.application, &original, &pending.signature, true).unwrap();
        }
        let carry=if estimate<2. {120} else {240-(estimate*60.) as i64};
        let ctx=egui::Context::default();let mut preparation=crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
        crate::gui_controls::acceptance::frame(&ctx,vec![],|ui|preparation.show(ui,&app.application,&next,"synthetic next cycle"),"Save PA1 Test");
        let next_record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",2,1).unwrap().unwrap();
        let weeks=app.application.payroll_timesheet_repository.get_weeks(next_record.id).unwrap();
        assert_eq!(weeks.iter().map(|w|(w.worked_hours*60.).round() as i64).sum::<i64>(),120+carry,"{mode:?}, estimate {estimate}: next cycle must include its actual hours plus only the reconciled difference");
        assert_eq!(next_record.previous_cycle_hours.map_or(0,|h|(h*60.).round() as i64),carry);
        assert!(!preparation.has_unsaved_changes());
        // Reopening preparation must not freeze, lose actual hours or apply carry twice.
        let mut reopened=crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
        crate::gui_controls::acceptance::frame(&ctx,vec![],|ui|reopened.show(ui,&app.application,&next,"synthetic next cycle"),"Save PA1 Test");
        assert_eq!(generate(&mut app,&[1]).completed(),1);
        if matches!(mode,Feature2Ingest::Folder) && estimate==3.0 {
            app.operational_payroll_period.select(&original_schedule,None);
            save_feature2_hours(&app,3.5);assert_eq!(generate(&mut app,&[1]).completed(),1);
            crate::payroll_snapshot_service::send_production_candidate(
                &app.application.payroll_worked_item_repository, original.id, 1,
                "2026/27", 1, &path(&app, &original_schedule, 1), "synthetic replacement", || Ok(()),
            ).unwrap();
            authority=crate::timesheet_amendments::latest_generated(&conn,original.id).unwrap().unwrap();
            app.operational_payroll_period.select(&next,None);
            assert!(crate::payroll_evidence::lifecycle::verify_candidate_evidence(&conn,next_record.id).is_err(),"an old next-cycle PDF must not conceal the new authority");
            let mut refreshed=crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
            crate::gui_controls::acceptance::frame(&ctx,vec![],|ui|refreshed.show(ui,&app.application,&next,"updated next cycle"),"Save PA1 Test");
            assert_eq!(app.application.payroll_timesheet_repository.get_weeks(next_record.id).unwrap().iter().map(|w|(w.worked_hours*60.).round() as i64).sum::<i64>(),120,"new parent authority supersedes old carry without freezing the prepared cycle");
            assert_eq!(generate(&mut app,&[1]).completed(),1,"next-cycle regeneration remains available");
        }
        let next_pdf=path(&app,&next,1);
        crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,next_record.id,1,"2026/27",2,&next_pdf,"synthetic",||Ok(())).unwrap();
        let third=feature2_cycle_schedule(&app,3,"27/05/2026","30/06/2026");
        app.operational_payroll_period.select(&third,None);
        feature2_ingest_actuals(&mut app,mode,&["30/05/2026"],"cycle-three.csv");
        let mut final_preparation=crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
        crate::gui_controls::acceptance::frame(&ctx,vec![],|ui|final_preparation.show(ui,&app.application,&third,"synthetic third cycle"),"Save PA1 Test");
        let third_record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",3,1).unwrap().unwrap();
        assert_eq!(app.application.payroll_timesheet_repository.get_weeks(third_record.id).unwrap().iter().map(|w|(w.worked_hours*60.).round() as i64).sum::<i64>(),120,"{mode:?}: third cycle must accept fresh actuals without repaying previous estimates/carry");
        assert_eq!(third_record.previous_cycle_hours.unwrap_or(0.),0.);
        assert_eq!(crate::timesheet_amendments::latest_generated(&conn,original.id).unwrap().unwrap().document,authority.document);
        assert_eq!(conn.query_row("SELECT pdf_bytes FROM payroll_submissions WHERE payroll_timesheet_id=?1",[original.id],|r|r.get::<_,Vec<u8>>(0)).unwrap(),before);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM payroll_submissions WHERE payroll_timesheet_id=?1",[original.id],|r|r.get::<_,i64>(0)).unwrap(),if matches!(mode, Feature2Ingest::Folder) && estimate == 3.0 { 3 } else { 2 });
    }
}
#[test]
fn feature2_future_cycles_accept_folder_csv_actuals_and_reconcile_corrected_estimates() {feature2_assert_next_cycles_accept_actuals(Feature2Ingest::Folder);}
#[test]
fn feature2_future_cycles_accept_clipboard_actuals_despite_historic_csv_files() {feature2_assert_next_cycles_accept_actuals(Feature2Ingest::Clipboard);}
#[test]
fn feature2_future_cycles_accept_logged_actuals_and_reconcile_corrected_estimates() {feature2_assert_next_cycles_accept_actuals(Feature2Ingest::Logger);}

#[test]
fn feature2_paid_original_estimates_reconcile_csv_credits_and_full_deductions() {
    use crate::payroll_evidence::reconciliation;

    let (_dir, mut app, original_schedule) = fixture();
    let conn = db(&app);
    conn.execute_batch("UPDATE personal_assistants SET employment_status=CASE id WHEN 1 THEN 'Active' ELSE 'Inactive' END").unwrap();
    crate::private_storage::ensure_private_dir(&app.application.context.config.folders.csv_import)
        .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let old = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 1, 1)
        .unwrap()
        .unwrap();
    for (week_number, adjustment_minutes) in [(3, 90), (4, 300)] {
        app.application
            .payroll_worked_item_repository
            .set_manual_adjustment(
                old.id,
                &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                    week_number,
                    adjustment_minutes,
                    reason: Some("Manual current-period estimate".into()),
                },
                "synthetic estimate",
            )
            .unwrap();
    }
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let original_pdf = path(&app, &original_schedule, 1);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository,
        old.id,
        1,
        "2026/27",
        1,
        &original_pdf,
        "synthetic",
        || Ok(()),
    )
    .unwrap();
    app.application
        .payroll_timesheet_email_repository
        .mark_sent(1, "2026/27", 1, "payslip", "synthetic settlement")
        .unwrap();

    let original_document: (i64, String, Vec<u8>) = conn.query_row(
        "SELECT document_id,pdf_sha256,pdf_bytes FROM payroll_submissions WHERE payroll_timesheet_id=?1 ORDER BY id LIMIT 1",
        [old.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    let next = feature2_cycle_schedule(&app, 2, "29/04/2026", "30/05/2026");
    app.operational_payroll_period.select(&next, None);
    let csv = feature2_source_csv(&["18/04/2026", "25/04/2026", "02/05/2026"])
        .replace("11:00:00,0h 00m,2h 00m", "10:53:00,0h 00m,1h 53m");
    for name in ["first.csv", "reexport.csv"] {
        crate::private_storage::write_new(
            &app.application.context.config.folders.csv_import.join(name),
            csv.as_bytes(),
        )
        .unwrap();
        assert!(!app.application.import_csv().unwrap().has_failures());
        let ctx = egui::Context::default();
        let mut preparation = crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
        crate::gui_controls::acceptance::frame(
            &ctx,
            vec![],
            |ui| preparation.show(ui, &app.application, &next, "synthetic next cycle"),
            "Save PA1 Test",
        );
        let current = app
            .application
            .payroll_timesheet_repository
            .get_for_cycle_and_pa("2026/27", 2, 1)
            .unwrap()
            .unwrap();
        let plan = reconciliation::plan(&app.application, &current).unwrap();
        assert!(plan.historical_reviews.is_empty());
        assert_eq!(plan.work.len(), 1);
        assert_eq!(current.previous_cycle_hours, Some(0.5));
    }
    let ledger = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(ledger("SELECT COUNT(*) FROM payroll_corrections"), 1);
    assert_eq!(ledger("SELECT SUM(minutes) FROM payroll_corrections"), 30);
    let pending = reconciliation::changes(&app.application, &old).unwrap();
    assert!(pending.requires_complete);
    assert!(reconciliation::carry(&app.application, &old, &pending.signature, false).is_err());
    assert_eq!(ledger("SELECT SUM(minutes) FROM payroll_corrections"), 30);
    reconciliation::carry(&app.application, &old, &pending.signature, true).unwrap();
    assert_eq!(ledger("SELECT SUM(minutes) FROM payroll_corrections"), -150);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    assert_eq!(
        ledger("SELECT SUM(minutes) FROM payroll_correction_applications"),
        -150
    );
    assert_eq!(ledger("SELECT COUNT(*) FROM payroll_submissions"), 1);
    assert_eq!(
        ledger("SELECT COUNT(*) FROM timesheet_amendment_versions"),
        0
    );

    let current = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 2, 1)
        .unwrap()
        .unwrap();
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository,
        current.id,
        1,
        "2026/27",
        2,
        &path(&app, &next, 1),
        "synthetic",
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        ledger("SELECT SUM(minutes) FROM payroll_submission_corrections"),
        -150
    );

    // A revised estimate accounts for the earlier applied credit. The other
    // week's already applied deduction stays in submitted history.
    app.operational_payroll_period
        .select(&original_schedule, None);
    let repo = &app.application.payroll_timesheet_repository;
    let mut weeks = repo.get_weeks(old.id).unwrap();
    weeks[2].worked_hours = 1.0;
    let adjustments = weeks
        .iter()
        .map(
            |week| crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number: week.week_number,
                adjustment_minutes: 0,
                reason: None,
            },
        )
        .collect::<Vec<_>>();
    repo.save_preparation_atomically(
        &old,
        &weeks,
        &repo.get_public_holidays(old.id).unwrap(),
        &repo.get_annual_leave(old.id).unwrap(),
        &adjustments,
        "synthetic revised estimate",
    )
    .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);

    assert_eq!(ledger("SELECT COUNT(*) FROM payroll_corrections"), 2);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository, old.id, 1,
        "2026/27", 1, &path(&app, &original_schedule, 1), "synthetic replacement", || Ok(()),
    ).unwrap();

    let third = feature2_cycle_schedule(&app, 3, "27/05/2026", "30/06/2026");
    app.operational_payroll_period.select(&third, None);
    feature2_ingest_actuals(
        &mut app,
        Feature2Ingest::Folder,
        &["30/05/2026"],
        "third.csv",
    );
    let ctx = egui::Context::default();
    let mut preparation = crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
    crate::gui_controls::acceptance::frame(
        &ctx,
        vec![],
        |ui| preparation.show(ui, &app.application, &third, "synthetic third cycle"),
        "Save PA1 Test",
    );
    let current = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 3, 1)
        .unwrap()
        .unwrap();
    let applied = conn.prepare(
        "SELECT minutes FROM payroll_correction_applications WHERE payroll_timesheet_id=?1 ORDER BY minutes",
    ).unwrap().query_map([current.id], |row| row.get::<_, i64>(0))
        .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(applied, [30]);
    assert_eq!(ledger("SELECT COUNT(*) FROM payroll_corrections"), 3);
    assert_eq!(conn.query_row(
        "SELECT document_id,pdf_sha256,pdf_bytes FROM payroll_submissions WHERE payroll_timesheet_id=?1 ORDER BY id LIMIT 1",
        [old.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap(), original_document);
}

#[test]
fn feature2_amended_mixed_estimates_reconcile_midweek_csv_with_existing_actuals() {
    use crate::payroll_evidence::reconciliation;

    let (_dir, mut app, previous) = fixture();
    let conn = db(&app);
    conn.execute(
        "UPDATE personal_assistants SET employment_status='Inactive' WHERE id<>1",
        [],
    )
    .unwrap();
    let folder = app.application.context.config.folders.csv_import.clone();
    crate::private_storage::ensure_private_dir(&folder).unwrap();
    let import = |app: &mut DirectPaymentApp, name: &str, dates: &[&str], expected_rows| {
        crate::private_storage::write_new(
            &folder.join(name),
            feature2_source_csv(dates).as_bytes(),
        )
        .unwrap();
        let summary = app.application.import_csv().unwrap();
        assert!(!summary.has_failures());
        assert_eq!(summary.rows_imported, expected_rows);
    };
    import(
        &mut app,
        "early-actuals.csv",
        &["15/04/2026", "22/04/2026"],
        2,
    );
    let repo = &app.application.payroll_timesheet_repository;
    let old = repo.get_for_cycle_and_pa("2026/27", 1, 1).unwrap().unwrap();
    let adjustments = (1..=4)
        .map(
            |week_number| crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number,
                adjustment_minutes: [0, 0, 60, 240][(week_number - 1) as usize],
                reason: (week_number >= 3).then(|| "Manual current-period estimate".into()),
            },
        )
        .collect::<Vec<_>>();
    for estimate in &adjustments {
        app.application
            .payroll_worked_item_repository
            .set_manual_adjustment(old.id, estimate, "fictional estimate")
            .unwrap();
    }
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository,
        old.id,
        1,
        "2026/27",
        1,
        &path(&app, &previous, 1),
        "synthetic",
        || Ok(()),
    )
    .unwrap();
    app.application
        .payroll_timesheet_email_repository
        .mark_sent(1, "2026/27", 1, "payslip", "synthetic paid")
        .unwrap();
    let repo = &app.application.payroll_timesheet_repository;
    let mut weeks = repo.get_weeks(old.id).unwrap();
    weeks[2].worked_hours = 3.5;
    weeks[3].worked_hours = 5.;
    repo.save_preparation_atomically(
        &old,
        &weeks,
        &repo.get_public_holidays(old.id).unwrap(),
        &repo.get_annual_leave(old.id).unwrap(),
        &adjustments,
        "fictional amended estimates",
    )
    .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository, old.id, 1,
        "2026/27", 1, &path(&app, &previous, 1), "synthetic replacement", || Ok(()),
    ).unwrap();
    let version = crate::timesheet_amendments::latest_generated(&conn, old.id)
        .unwrap()
        .unwrap();
    assert_eq!(&version.totals[2..], &[210, 300]);

    // A deduction posted by the old amended path must stay in the audit
    // ledger, but its unapplied amount still needs completeness confirmation.
    conn.execute(
        "INSERT INTO payroll_corrections(personal_assistant_id,origin_payroll_timesheet_id,evidence_key,work_date,minutes,reason,created_at)
         VALUES (1,?1,?2,'2026-04-22',-30,'legacy synthetic deduction','test')",
        params![old.id, format!("version:{}:week:4|legacy|0", version.document)],
    ).unwrap();

    let next = feature2_cycle_schedule(&app, 2, "29/04/2026", "30/05/2026");
    app.operational_payroll_period.select(&next, None);
    let dates = &["18/04/2026", "25/04/2026", "02/05/2026"];
    import(&mut app, "midweek.csv", dates, 3);
    import(&mut app, "midweek-repeat.csv", dates, 0);
    let ctx = egui::Context::default();
    let mut screen = crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
    crate::gui_controls::acceptance::frame(
        &ctx,
        vec![],
        |ui| {
            screen.show(
                ui,
                &app.application,
                &next,
                "synthetic midweek reconciliation",
            )
        },
        "Save PA1 Test",
    );
    let current = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 2, 1)
        .unwrap()
        .unwrap();
    let plan = reconciliation::plan(&app.application, &current).unwrap();
    assert!(plan.historical_reviews.is_empty() && plan.notices.is_empty());
    assert_eq!(plan.work.len(), 1);
    assert_eq!(current.previous_cycle_hours, Some(0.5));
    let pending = reconciliation::changes(&app.application, &old).unwrap();
    assert!(pending.requires_complete);
    assert_eq!(
        pending
            .components
            .iter()
            .map(|component| component.minutes)
            .collect::<Vec<_>>(),
        [-60]
    );
    assert!(reconciliation::carry(&app.application, &old, &pending.signature, false).is_err());
    reconciliation::carry(&app.application, &old, &pending.signature, true).unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM payroll_corrections", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        conn.query_row(
            "SELECT SUM(minutes) FROM payroll_correction_applications",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        -30
    );
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository,
        current.id,
        1,
        "2026/27",
        2,
        &path(&app, &next, 1),
        "synthetic",
        || Ok(()),
    )
    .unwrap();
    let third = feature2_cycle_schedule(&app, 3, "27/05/2026", "30/06/2026");
    app.operational_payroll_period.select(&third, None);
    import(&mut app, "third-actuals.csv", &["30/05/2026"], 1);
    let mut screen = crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
    crate::gui_controls::acceptance::frame(
        &egui::Context::default(),
        vec![],
        |ui| screen.show(ui, &app.application, &third, "synthetic later period"),
        "Save PA1 Test",
    );
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let later = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 3, 1)
        .unwrap()
        .unwrap();
    assert!(reconciliation::outstanding(&app.application, &later)
        .unwrap()
        .is_empty());
    assert_eq!(
        crate::timesheet_amendments::latest_generated(&conn, old.id)
            .unwrap()
            .unwrap()
            .document,
        version.document
    );
}

#[test]
fn feature2_seven_historical_reviews_refresh_only_after_accepted_amendment() {
    use crate::payroll_evidence::{self, reconciliation};

    let (_dir, mut app, prior) = fixture();
    let conn = db(&app);
    conn.execute(
        "UPDATE personal_assistants SET employment_status='Inactive' WHERE id<>1",
        [],
    )
    .unwrap();
    let old = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 1, 1)
        .unwrap()
        .unwrap();
    app.application
        .payroll_worked_item_repository
        .set_manual_adjustment(
            old.id,
            &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number: 2,
                adjustment_minutes: 30,
                reason: Some("Fictional manual work".into()),
            },
            "synthetic",
        )
        .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository,
        old.id,
        1,
        "2026/27",
        1,
        &path(&app, &prior, 1),
        "synthetic",
        || Ok(()),
    )
    .unwrap();
    app.application
        .payroll_timesheet_email_repository
        .mark_sent(1, "2026/27", 1, "payslip", "synthetic paid")
        .unwrap();
    let folder = app.application.context.config.folders.csv_import.clone();
    crate::private_storage::ensure_private_dir(&folder).unwrap();
    let csv = feature2_source_csv(&[
        "15/04/2026",
        "16/04/2026",
        "17/04/2026",
        "18/04/2026",
        "19/04/2026",
        "20/04/2026",
        "21/04/2026",
    ]);
    for (name, expected) in [("actuals.csv", 7), ("repeat.csv", 0)] {
        crate::private_storage::write_new(&folder.join(name), csv.as_bytes()).unwrap();
        let summary = app.application.import_csv().unwrap();
        assert!(!summary.has_failures());
        assert_eq!(summary.rows_imported, expected);
    }
    let next = feature2_cycle_schedule(&app, 2, "29/04/2026", "30/05/2026");
    let current_id = app
        .application
        .payroll_timesheet_repository
        .insert("2026/27", 2, 1, None, "synthetic")
        .unwrap();
    let current = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 2, 1)
        .unwrap()
        .unwrap();
    assert_eq!(current.id, current_id);
    let refresh = |app: &DirectPaymentApp,
                   review_ui: &mut payroll_evidence::review_ui::ReviewUi| {
        let db = db(app);
        let current_evidence = payroll_evidence::load_for_pa(&db, 1).unwrap();
        let pre = payroll_evidence::preflight_for_pa(&db, &current_evidence, 1).unwrap();
        review_ui
            .refresh_record(&app.application, &current, false, &pre)
            .unwrap();
    };
    let mut review_ui = payroll_evidence::review_ui::ReviewUi::default();
    refresh(&app, &mut review_ui);
    let reviews = &review_ui
        .preparation_plan(current.id)
        .unwrap()
        .historical_reviews;
    assert_eq!(reviews.len(), 7);
    let stale = reviews[0].clone();
    let edited_id = stale.evidence.id;
    conn.execute(
        "UPDATE timesheets SET worked_minutes=90 WHERE id=?1",
        [edited_id],
    )
    .unwrap();
    refresh(&app, &mut review_ui);
    assert!(reconciliation::historical_decision(&app.application, &stale, crate::payroll_evidence::reconciliation::HistoricalDecision::AlreadyPaid).is_err());
    let fresh = review_ui
        .preparation_plan(current.id)
        .unwrap()
        .historical_reviews
        .iter()
        .find(|row| row.evidence.id == edited_id)
        .unwrap()
        .clone();
    assert_eq!(fresh.evidence.minutes, 90);
    let ctx = egui::Context::default();
    let (_, _) = crate::gui_controls::acceptance::frame(
        &ctx,
        vec![],
        |ui| {
            review_ui.show_pa(ui, &app.application, &current, "PA1 Test", false);
        },
        "Historical payment reviews remaining: 7",
    );
    let label = format!(
        "{}: {} – {}, 1.50 hours",
        fresh.evidence.label(),
        fresh.evidence.start,
        fresh.evidence.end
    );
    crate::gui_controls::acceptance::frame(
        &ctx,
        vec![],
        |ui| {
            review_ui.show_pa(ui, &app.application, &current, "PA1 Test", false);
        },
        &label,
    );

    let repo = &app.application.payroll_timesheet_repository;
    let mut weeks = repo.get_weeks(old.id).unwrap();
    weeks[2].worked_hours = 10.;
    let adjustments = (1..=4)
        .map(
            |week_number| crate::payroll_worked_item_repository::ManualHoursAdjustment {
                week_number,
                adjustment_minutes: 0,
                reason: None,
            },
        )
        .collect::<Vec<_>>();
    repo.save_preparation_atomically(
        &old,
        &weeks,
        &repo.get_public_holidays(old.id).unwrap(),
        &repo.get_annual_leave(old.id).unwrap(),
        &adjustments,
        "synthetic saved draft",
    )
    .unwrap();
    refresh(&app, &mut review_ui);
    assert_eq!(
        review_ui
            .preparation_plan(current.id)
            .unwrap()
            .historical_reviews
            .len(),
        7
    );
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    refresh(&app, &mut review_ui);
    assert_eq!(review_ui.preparation_plan(current.id).unwrap().historical_reviews.len(), 7);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository, old.id, 1,
        "2026/27", 1, &path(&app, &prior, 1), "synthetic replacement", || Ok(()),
    ).unwrap();
    refresh(&app, &mut review_ui);
    assert!(review_ui
        .preparation_plan(current.id)
        .unwrap()
        .historical_reviews
        .is_empty());
    assert!(reconciliation::historical_decision(&app.application, &fresh, crate::payroll_evidence::reconciliation::HistoricalDecision::AlreadyPaid).is_err());

    let more = feature2_source_csv(&["20/04/2026"])
        .replace("09:00:00", "13:00:00")
        .replace("11:00:00", "15:00:00");
    crate::private_storage::write_new(&folder.join("late-actual.csv"), more.as_bytes()).unwrap();
    let summary = app.application.import_csv().unwrap();
    assert_eq!(summary.rows_imported, 1);
    app.operational_payroll_period.select(&next, None);
    refresh(&app, &mut review_ui);
    let credits: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(minutes),0) FROM payroll_corrections",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(credits, 330);
    assert!(review_ui
        .preparation_plan(current.id)
        .unwrap()
        .historical_reviews
        .is_empty());
    refresh(&app, &mut review_ui);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM payroll_corrections", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn feature2_last_two_estimated_weeks_reconcile_new_csv_actuals_in_next_cycle() {
    let (_dir,mut app,original_schedule)=fixture();let conn=db(&app);
    conn.execute_batch("UPDATE personal_assistants SET employment_status=CASE id WHEN 1 THEN 'Active' ELSE 'Inactive' END;").unwrap();
    crate::private_storage::ensure_private_dir(&app.application.context.config.folders.csv_import).unwrap();
    assert_eq!(generate(&mut app,&[1]).completed(),1);
    let record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",1,1).unwrap().unwrap();
    let file=path(&app,&original_schedule,1);
    crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,record.id,1,"2026/27",1,&file,"synthetic",||Ok(())).unwrap();
    let repo=&app.application.payroll_timesheet_repository;
    let mut weeks=repo.get_weeks(record.id).unwrap();weeks[2].worked_hours=3.;weeks[3].worked_hours=4.;
    let adjustments=weeks.iter().map(|w|crate::payroll_worked_item_repository::ManualHoursAdjustment{week_number:w.week_number,adjustment_minutes:0,reason:None}).collect::<Vec<_>>();
    repo.save_preparation_atomically(&record,&weeks,&repo.get_public_holidays(record.id).unwrap(),&repo.get_annual_leave(record.id).unwrap(),&adjustments,"synthetic estimated weeks").unwrap();
    assert_eq!(generate(&mut app,&[1]).completed(),1);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository, record.id, 1,
        "2026/27", 1, &path(&app, &original_schedule, 1), "synthetic replacement", || Ok(()),
    ).unwrap();
    let version=crate::timesheet_amendments::latest_generated(&conn,record.id).unwrap().unwrap();assert_eq!(version.totals,[120,0,180,240]);
    let next=feature2_cycle_schedule(&app,2,"29/04/2026","30/05/2026");app.operational_payroll_period.select(&next,None);
    feature2_ingest_actuals(&mut app,Feature2Ingest::Folder,&["18/04/2026","25/04/2026","02/05/2026"],"estimated-week-actuals.csv");
    use crate::payroll_evidence::reconciliation;
    let partial = reconciliation::changes(&app.application, &record).unwrap();
    assert!(partial.requires_complete);
    reconciliation::sync_settled_corrections(&app.application, 1).unwrap();
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM payroll_corrections", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    reconciliation::carry(&app.application, &record, &partial.signature, true).unwrap();
    let ctx=egui::Context::default();let mut preparation=crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
    crate::gui_controls::acceptance::frame(&ctx,vec![],|ui|preparation.show(ui,&app.application,&next,"synthetic next cycle"),"Save PA1 Test");
    let next_record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",2,1).unwrap().unwrap();
    assert_eq!(next_record.previous_cycle_hours,Some(-3.0),"the full approved deduction applies in Week 1");
    assert_eq!(app.application.payroll_timesheet_repository.get_weeks(next_record.id).unwrap().iter().map(|w|(w.worked_hours*60.).round() as i64).sum::<i64>(),-60,"a larger deduction leaves a signed payable Week 1");
    assert_eq!(generate(&mut app,&[1]).completed(),1,"negative carry must not freeze next-cycle generation");
    let next_pdf=path(&app,&next,1);
    crate::payroll_snapshot_service::send_production_candidate(&app.application.payroll_worked_item_repository,next_record.id,1,"2026/27",2,&next_pdf,"synthetic",||Ok(())).unwrap();
    let third=feature2_cycle_schedule(&app,3,"27/05/2026","30/06/2026");app.operational_payroll_period.select(&third,None);
    feature2_ingest_actuals(&mut app,Feature2Ingest::Folder,&["30/05/2026"],"remaining-carry.csv");
    let mut third_preparation=crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
    crate::gui_controls::acceptance::frame(&ctx,vec![],|ui|third_preparation.show(ui,&app.application,&third,"synthetic third cycle"),"Save PA1 Test");
    let third_record=app.application.payroll_timesheet_repository.get_for_cycle_and_pa("2026/27",3,1).unwrap().unwrap();
    assert_eq!(third_record.previous_cycle_hours,None,"the full deduction was submitted in the previous cycle");
    assert_eq!(app.application.payroll_timesheet_repository.get_weeks(third_record.id).unwrap().iter().map(|w|(w.worked_hours*60.).round() as i64).sum::<i64>(),120);
    assert_eq!(generate(&mut app,&[1]).completed(),1);

    assert_eq!(crate::timesheet_amendments::latest_generated(&conn,record.id).unwrap().unwrap().document,version.document);
}

mod recipient_layout_tests {
    use super::*;

    fn choices(detail: &str) -> Vec<ProductionChoice> {
        vec![
            ProductionChoice { automatic: true, id: 1, name: "Active Example".into(), available: true, detail: detail.into(), intent: None },
            ProductionChoice { automatic: false, id: 2, name: "Unavailable Example".into(), available: false, detail: "No current document".into(), intent: None },
        ]
    }

    fn frame(
        ctx: &egui::Context,
        app: &mut DirectPaymentApp,
        width: f32,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 720.0))),
            events,
            ..Default::default()
        }, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.draw_recipient_selection(ui));
        })
    }

    fn text_shapes<'a>(shape: &'a egui::Shape, found: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => found.push(text),
            egui::Shape::Vec(shapes) => for shape in shapes { text_shapes(shape, found); },
            _ => {}
        }
    }

    fn text<'a>(out: &'a egui::FullOutput, value: &str) -> &'a egui::epaint::TextShape {
        let mut shapes = Vec::new();
        for clipped in &out.shapes { text_shapes(&clipped.shape, &mut shapes); }
        shapes.into_iter().find(|shape| shape.galley.job.text == value)
            .unwrap_or_else(|| panic!("Missing visible UI text: {value}"))
    }

    fn assert_fits(out: &egui::FullOutput, value: &str, width: f32) {
        let shape = text(out, value);
        assert!(!shape.galley.elided, "Full detail must remain available, not ellipsized");
        assert!(shape.pos.x + shape.galley.rect.left() >= -1.0);
        assert!(shape.pos.x + shape.galley.rect.right() <= width + 1.0,
            "Text extends beyond viewport: {:?}, width {width}", shape.visual_bounding_rect());
    }

    #[test]
    fn long_paths_wrap_in_both_recipient_dialogues_without_elision() {
        let details = [
            format!("Candidate ready: /managed/{}/Final timesheet Example.pdf", "long-directory/".repeat(35)),
            format!("Candidate ready: C:\\Managed\\{}\\Example.pdf", "unbroken".repeat(110)),
            format!("Attachments: /managed/Ångström Example/{}/P45 Example.pdf\n/managed/previous/P60 Example.pdf", "nested/".repeat(45)),
        ];
        for kind in [PayrollEmailKind::Timesheet, PayrollEmailKind::Payslip] {
            for width in [280.0, 400.0, 800.0] {
                for detail in &details {
                    let (_dir, mut app, _) = fixture();
                    app.begin_email_batch(kind);
                    let batch = app.pending_email_batch.as_mut().unwrap();
                    batch.stage = EmailBatchNoteStage::SelectRecipients;
                    batch.choices = choices(detail);
                    batch.selected_personal_assistant_ids = vec![1];
                    let ctx = egui::Context::default();
                    ctx.style_mut(|style| style.animation_time = 0.0);
                    let mut out = frame(&ctx, &mut app, width, vec![]);
                    for _ in 0..3 { out = frame(&ctx, &mut app, width, vec![]); }
                    assert_fits(&out, detail, width);
                    assert!(text(&out, detail).galley.rows.len() > 1);
                    assert_fits(&out, "Active Example", width);
                    assert_fits(&out, "Cancel", width);
                    let proceed = if matches!(kind, PayrollEmailKind::Payslip) { "Review selected recipients" } else { "Continue to email notes" };
                    assert_fits(&out, proceed, width);
                    assert!(text(&out, proceed).visual_bounding_rect().bottom() <= 720.0);
                    assert_eq!(app.pending_email_batch.as_ref().unwrap().selected_personal_assistant_ids, vec![1]);
                    assert_eq!(db(&app).query_row("SELECT COUNT(*) FROM timesheet_delivery_attempts", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
                }
            }
        }
    }

    #[test]
    fn recipient_dialogue_rewraps_after_viewport_shrinks() {
        let (_dir, mut app, _) = fixture();
        app.begin_email_batch(PayrollEmailKind::Timesheet);
        let detail = format!("Candidate ready: /managed/{}/Example.pdf", "long-segment/".repeat(50));
        let batch = app.pending_email_batch.as_mut().unwrap();
        batch.choices = choices(&detail);
        batch.selected_personal_assistant_ids = vec![1];
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.0);
        for width in [1000.0, 320.0, 640.0] {
            let mut out = frame(&ctx, &mut app, width, vec![]);
            for _ in 0..4 { out = frame(&ctx, &mut app, width, vec![]); }
            assert_fits(&out, &detail, width);
            assert_fits(&out, "Cancel", width);
            assert_eq!(app.pending_email_batch.as_ref().unwrap().selected_personal_assistant_ids, vec![1]);
        }
    }

    #[test]
    fn wrapped_choices_keep_selection_and_unavailable_recipient_safeguards() {
        let (_dir, mut app, _) = fixture();
        app.begin_email_batch(PayrollEmailKind::Timesheet);
        let batch = app.pending_email_batch.as_mut().unwrap();
        batch.choices = choices("Candidate: /managed/payroll-year/Archived/Full Example timesheet path.pdf");
        batch.selected_personal_assistant_ids = vec![1];
        batch.resend_acknowledged = true;
        batch.approved_selection = vec![1];
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.0);
        let mut out = frame(&ctx, &mut app, 400.0, vec![]);
        for _ in 0..3 { out = frame(&ctx, &mut app, 400.0, vec![]); }
        for (label, expected) in [("Unavailable Example", vec![1]), ("Active Example", vec![])] {
            let label_shape = text(&out, label);
            let pos = label_shape.pos + label_shape.galley.size() / 2.0;
            for pressed in [true, false] {
                out = frame(&ctx, &mut app, 400.0, vec![egui::Event::PointerMoved(pos), egui::Event::PointerButton {
                    pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default(),
                }]);
            }
            assert_eq!(app.pending_email_batch.as_ref().unwrap().selected_personal_assistant_ids, expected);
        }
        let batch = app.pending_email_batch.as_ref().unwrap();
        assert!(!batch.resend_acknowledged && batch.approved_selection.is_empty());
        assert_eq!(batch.stage, EmailBatchNoteStage::SelectRecipients);
        assert_eq!(db(&app).query_row("SELECT COUNT(*) FROM timesheet_delivery_attempts", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
    }
}

#[test]
fn workflow_eligibility_departed_final_document_import_and_individual_send_ignore_departure_age() {
    let (dir,mut app,_)=fixture();
    db(&app).execute_batch("UPDATE personal_assistants SET start_date='01/01/2019',leaving_date='01/01/2020',employment_status='Inactive' WHERE id=1").unwrap();
    app.operational_payroll_period.selected=None;
    let path=dir.path().join("P45 for PA1 Test.pdf");std::fs::write(&path,b"%PDF-1.4 synthetic late final document").unwrap();
    let original_pa=app.application.personal_assistant_repository.get_all().unwrap();
    app.application.import_payroll_documents(&path,None).unwrap();
    assert_eq!(app.unknown_supplements().unwrap().len(),1);
    let docs=app.application.payroll_timesheet_email_repository.documents_for_pa(1).unwrap();assert_eq!(docs.len(),1);
    app.application.payroll_timesheet_email_repository.reconcile_document(docs[0].id,true).unwrap();
    let smtp=Smtp::new(&mut app);
    app.begin_email_batch(PayrollEmailKind::Payslip);
    let pending=app.pending_email_batch.as_ref().unwrap();
    assert!(pending.selected_personal_assistant_ids.is_empty());
    assert!(pending.choices.iter().any(|c|c.id==1&&c.available&&!c.automatic));
    let mut batch=payslip_batch(&mut app,&[1]);
    batch.volume_acknowledged=false;
    assert_eq!(app.dispatch_payslip_selection(&batch).unwrap(),1);
    assert_eq!(smtp.count(),1);
    let envelopes=smtp.envelopes.lock().unwrap();
    assert!(envelopes[0].iter().any(|r|r.contains("pa1@example.test")));
    assert!(!envelopes[0].iter().any(|r|r.contains("payroll@example.test")));
    assert_eq!(app.application.personal_assistant_repository.get_all().unwrap(),original_pa);
    assert!(matches!(app.application.payroll_timesheet_email_repository.documents_for_pa(1).unwrap()[0].delivery_state,crate::payroll_timesheet_email_repository::EmailDeliveryState::Sent{..}));
}

#[test]
fn workflow_eligibility_bulk_actions_never_select_departed_choices() {
    use crate::gui_controls::acceptance::{frame,click};
    let ctx=egui::Context::default();
    let choices=vec![
        ProductionChoice{id:1,name:"Current PA".into(),automatic:true,available:true,detail:"Ready".into(),intent:None},
        ProductionChoice{id:2,name:"Departed PA".into(),automatic:false,available:true,detail:"Final document".into(),intent:None},
    ];
    for unsent in [false,true] {
        let label=if unsent{"Select all unsent"}else{"Select all available"};let mut ids=vec![];
        let(pos,_)=frame(&ctx,vec![],|ui|draw_production_choices(ui,&choices,&mut ids,unsent),label);
        click(&ctx,pos,|ui|draw_production_choices(ui,&choices,&mut ids,unsent),label);assert_eq!(ids,vec![1]);
        let(pos,_)=frame(&ctx,vec![],|ui|draw_production_choices(ui,&choices,&mut ids,unsent),"Departed PA");
        click(&ctx,pos,|ui|draw_production_choices(ui,&choices,&mut ids,unsent),"Departed PA");assert_eq!(ids,vec![1,2]);
    }
}

#[test]
fn workflow_eligibility_status_change_invalidates_active_and_retained_timesheet_dispatch() {
    for retained in [false,true] {
        let (_dir,mut app,_)=fixture();assert_eq!(generate(&mut app,&[1]).completed(),1);
        let smtp=Smtp::new(&mut app);
        if retained {
            db(&app).execute("UPDATE personal_assistants SET employment_status='Inactive' WHERE id=1",[]).unwrap();
            app.begin_retained_timesheet_batch();
        }else{app.begin_email_batch(PayrollEmailKind::Timesheet);}
        let mut batch=app.pending_email_batch.take().unwrap();batch.selected_personal_assistant_ids=vec![1];batch.stage=EmailBatchNoteStage::ConfirmDispatch;
        app.capture_email_confirmation(&mut batch).unwrap();
        db(&app).execute("UPDATE personal_assistants SET leaving_date='01/01/2020' WHERE id=1",[]).unwrap();
        assert!(app.dispatch_timesheet_selection(&batch).unwrap().entries.iter().any(|(_,_,outcome)|matches!(outcome,ProductionOutcome::Failed(_))));
        assert_eq!(smtp.count(),0);
        assert_eq!(db(&app).query_row("SELECT COUNT(*) FROM timesheet_delivery_attempts",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
}

#[test]
fn workflow_eligibility_departed_payslip_status_or_recipient_change_forces_new_approval() {
    for change in ["UPDATE personal_assistants SET leaving_date='01/01/2020' WHERE id=1", "UPDATE personal_assistants SET email='changed@example.test' WHERE id=1"] {
        let (_dir,mut app,_)=payslip_selection_fixture();
        db(&app).execute("UPDATE personal_assistants SET employment_status='Inactive' WHERE id=1",[]).unwrap();
        let smtp=Smtp::new(&mut app);let batch=payslip_batch(&mut app,&[1]);
        db(&app).execute_batch(change).unwrap();assert!(app.dispatch_payslip_selection(&batch).is_err());assert_eq!(smtp.count(),0);
        assert_eq!(db(&app).query_row("SELECT COUNT(*) FROM payroll_bundle_attempts",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
}

#[test]
fn workflow_eligibility_retained_departed_timesheet_resend_needs_acknowledgement() {
    let (_dir,mut app,_)=fixture();assert_eq!(generate(&mut app,&[1]).completed(),1);
    db(&app).execute("UPDATE personal_assistants SET employment_status='Inactive' WHERE id=1",[]).unwrap();
    let smtp=Smtp::new(&mut app);
    for resend in [false,true] {
        app.begin_retained_timesheet_batch();let mut batch=app.pending_email_batch.take().unwrap();
        assert!(batch.selected_personal_assistant_ids.is_empty());
        assert_eq!(batch.choices.iter().find(|c|c.id==1).unwrap().intent.as_ref().unwrap().resend,resend);
        batch.selected_personal_assistant_ids=vec![1];batch.stage=EmailBatchNoteStage::ConfirmDispatch;
        app.capture_email_confirmation(&mut batch).unwrap();
        if resend {
            assert!(app.dispatch_timesheet_selection(&batch).is_err());assert_eq!(smtp.count(),1);
            batch.resend_acknowledged=true;
        }
        assert_eq!(app.dispatch_timesheet_selection(&batch).unwrap().completed(),1);
    }
    assert_eq!(smtp.count(),2);
    let conn=db(&app);
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM payroll_submissions",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM timesheet_delivery_attempts WHERE classification='resend' AND outcome='accepted'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}

#[test]
fn workflow_eligibility_combined_synthetic_fixture_keeps_ordinary_and_retained_scopes_separate() {
    let (dir,mut app,schedule)=fixture();
    assert_eq!(generate(&mut app,&[1,2,3,4,5]).completed(),5);
    db(&app).execute_batch("UPDATE personal_assistants SET employment_status='Inactive',leaving_date='01/01/2020' WHERE id IN (2,3); UPDATE personal_assistants SET leaving_date='01/01/2020' WHERE id=4; UPDATE personal_assistants SET start_date='01/01/2090' WHERE id=5;").unwrap();
    for (id,kind) in [(2,"P45"),(4,"P60")] {
        let source=dir.path().join(format!("{kind} for PA{id} Test.pdf"));
        std::fs::write(&source,format!("%PDF-1.4 synthetic late document for PA{id}")).unwrap();
        app.application.import_payroll_documents(&source,None).unwrap();
    }
    assert_eq!(app.unknown_supplements().unwrap().len(),2);
    let ctx=egui::Context::default();
    let labels=crate::payroll_settings_screen::tab_acceptance::draw(&ctx,1000.0,vec![],|ui|app.draw_timesheet_email_status(ui));
    assert!(labels.contains_key("PA1 Test"));
    assert!(!labels.contains_key("PA2 Test") && !labels.contains_key("PA3 Test") && !labels.contains_key("PA4 Test") && !labels.contains_key("PA5 Test"));
    app.begin_email_batch(PayrollEmailKind::Timesheet);
    assert_eq!(app.pending_email_batch.as_ref().unwrap().choices.iter().map(|c|c.id).collect::<Vec<_>>(),vec![1]);
    app.begin_retained_timesheet_batch();
    assert_eq!(app.pending_email_batch.as_ref().unwrap().choices.len(),5);
    assert!(app.pending_email_batch.as_ref().unwrap().selected_personal_assistant_ids.is_empty());
    assert_eq!(app.payslip_email_assistants().unwrap().iter().map(|p|p.id).collect::<Vec<_>>(),vec![1,2,4]);
    // Optional export for native synthetic GUI checks; the ordinary regression
    // runs entirely within its disposable private directory.
    if let Ok(manifest)=std::env::var("DPT_DASHBOARD_SYNTHETIC_GUI") {
        let manifest=std::path::PathBuf::from(manifest);
        assert!(manifest.parent().unwrap().file_name().unwrap().to_string_lossy().starts_with("dpt-dashboard-policy-"));
        app.application.save_config().unwrap();
        let destination=dir.path().join("database.sqlite");assert!(!destination.exists());
        db(&app).backup(rusqlite::MAIN_DB,&destination,None::<fn(rusqlite::backup::Progress)>).unwrap();
        use std::io::Write;
        let mut output=std::fs::OpenOptions::new().create_new(true).write(true).open(manifest).unwrap();
        write!(output,"{}",serde_json::json!({"profile":dir.path(),"synthetic_only":true,"schedule":schedule.id})).unwrap();
        drop(app);
        std::mem::forget(dir);
    }
}

#[test]
fn acceptance_gui_signature_buttons_follow_each_owners_usable_image() {
    let (dir,mut app,_)=fixture();
    let bytes=crate::signature::rasterize(&[vec![egui::pos2(0.1,0.5),egui::pos2(0.9,0.5)]]).unwrap();
    let valid=dir.path().join("valid.png");std::fs::write(&valid,bytes).unwrap();
    let invalid=dir.path().join("invalid.png");std::fs::write(&invalid,b"not a signature").unwrap();
    let absent=dir.path().join("missing.png");
    for (employer,pa,wanted) in [
        (None,None,vec![crate::signature::Owner::Employer(1),crate::signature::Owner::Pa(1)]),
        (Some(&valid),None,vec![crate::signature::Owner::Pa(1)]),
        (None,Some(&valid),vec![crate::signature::Owner::Employer(1)]),
        (Some(&valid),Some(&valid),vec![]),
        (Some(&invalid),Some(&valid),vec![crate::signature::Owner::Employer(1)]),
        (Some(&valid),Some(&absent),vec![crate::signature::Owner::Pa(1)]),
    ] {
        db(&app).execute("UPDATE employers SET employer_signature=?1 WHERE id=1",[employer.map(|p|p.to_str().unwrap())]).unwrap();
        db(&app).execute("UPDATE personal_assistants SET signature=?1 WHERE id=1",[pa.map(|p|p.to_str().unwrap())]).unwrap();
        app.begin_generation();let pending=app.pending_generation.as_mut().unwrap();pending.selected_ids=vec![1];
        assert_eq!(app.missing_generation_signatures(app.pending_generation.as_ref().unwrap()).unwrap().into_iter().map(|v|v.0).collect::<Vec<_>>(),wanted);
        let ctx=egui::Context::default();let mut labels=vec![];
        fn scan(s:&egui::Shape,labels:&mut Vec<String>){match s{egui::Shape::Text(t)=>labels.push(t.galley.job.text.clone()),egui::Shape::Vec(v)=>for s in v{scan(s,labels)},_=>{}}}
        for _ in 0..3 {
            let output=ctx.run(Default::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|app.draw_generation_selection(ui));});
            labels.clear();for s in output.shapes{scan(&s.shape,&mut labels);}
        }
        assert_eq!(labels.iter().filter(|s|s.starts_with("Draw signature —")).count(),wanted.len());
        assert!(labels.iter().any(|s|s=="Generate selected PAs"));
    }
}

#[test]
fn acceptance_gui_note_choices_keep_both_workflows_and_note_behaviour() {
    use crate::gui_controls::acceptance::{frame,click};
    for kind in [PayrollEmailKind::Timesheet,PayrollEmailKind::Payslip] {
        for label in ["Add Additional Note","No Additional Note","Cancel"] {
            let (_dir,mut app,_)=fixture();app.begin_email_batch(kind);
            app.pending_email_batch.as_mut().unwrap().stage=EmailBatchNoteStage::ChooseAdditionalNote;
            app.additional_notes_by_personal_assistant.insert(1,"retained draft".into());
            let ctx=egui::Context::default();
            let (pos,_)=frame(&ctx,vec![],|ui|app.draw_additional_note_prompt(ui),label);
            click(&ctx,pos,|ui|app.draw_additional_note_prompt(ui),label);
            match label {
                "Add Additional Note"=>{
                    assert_eq!(app.pending_email_batch.as_ref().unwrap().stage,EmailBatchNoteStage::EditAdditionalNotes);
                    assert_eq!(app.additional_notes_by_personal_assistant.get(&1).unwrap(),"retained draft");
                },
                "No Additional Note"=>{
                    assert_ne!(app.pending_email_batch.as_ref().unwrap().stage,EmailBatchNoteStage::ChooseAdditionalNote);
                    assert!(app.additional_notes_by_personal_assistant.is_empty());
                },
                _=>{assert!(app.pending_email_batch.is_none());assert!(app.additional_notes_by_personal_assistant.is_empty());}
            }
            assert_eq!(db(&app).query_row("SELECT COUNT(*) FROM timesheet_delivery_attempts",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        }
    }
}

#[test]
fn acceptance_gui_note_prompt_wraps_inside_an_expanded_dashboard_for_both_workflows() {
    fn scan(shape: &egui::Shape, labels: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(text) => labels.push((text.galley.job.text.clone(), text.galley.rect.translate(text.pos.to_vec2()))),
            egui::Shape::Vec(shapes) => for shape in shapes { scan(shape, labels); },
            _ => {}
        }
    }
    for kind in [PayrollEmailKind::Timesheet, PayrollEmailKind::Payslip] {
        for width in [1000.0, 600.0, 360.0] {
            let (_dir, mut app, _) = fixture();
            app.begin_email_batch(kind);
            app.pending_email_batch.as_mut().unwrap().stage = EmailBatchNoteStage::ChooseAdditionalNote;
            let ctx = egui::Context::default();
            let mut labels = vec![];
            for _ in 0..3 {
                let output = ctx.run(egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.0))),
                    ..Default::default()
                }, |ctx| { egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_min_width(660.0);
                    app.draw_additional_note_prompt(ui);
                }); });
                labels.clear();
                for shape in output.shapes { scan(&shape.shape, &mut labels); }
            }
            for label in ["Add Additional Note", "No Additional Note", "Cancel"] {
                let rect = labels.iter().find(|(text, _)| text == label).unwrap().1;
                assert!(rect.left() >= 0.0 && rect.right() <= width, "{label} at {width}: {rect:?}");
            }
        }
    }
}

#[test]
fn feature2_manual_preparation_generates_current_pdf_with_open_historical_review() {
    use crate::payroll_evidence::{manual_preparation, reconciliation};

    let (_dir, mut app, prior) = fixture();
    let conn = db(&app);
    conn.execute(
        "UPDATE personal_assistants SET employment_status='Inactive' WHERE id<>1",
        [],
    )
    .unwrap();
    app.application
        .payroll_timesheet_email_repository
        .mark_sent(1, "2026/27", 1, "payslip", "fictional settled history")
        .unwrap();
    let next = feature2_cycle_schedule(&app, 2, "29/04/2026", "30/05/2026");
    app.operational_payroll_period.select(&next, None);
    let folder = &app.application.context.config.folders.csv_import;
    crate::private_storage::ensure_private_dir(folder).unwrap();
    crate::private_storage::write_new(
        &folder.join("fictional.csv"),
        feature2_source_csv(&["18/04/2026", "02/05/2026"]).as_bytes(),
    )
    .unwrap();
    assert_eq!(app.application.import_csv().unwrap().rows_imported, 2);
    let mut screen = crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
    crate::gui_controls::acceptance::frame(
        &egui::Context::default(),
        vec![],
        |ui| screen.show(ui, &app.application, &next, "fictional manual preparation"),
        "Save PA1 Test",
    );
    let current = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 2, 1)
        .unwrap()
        .unwrap();
    let start = crate::date_utils::parse_legacy(&next.first_week_commencing).unwrap();
    let dates = std::array::from_fn(|index| start + chrono::Duration::weeks(index as i64));
    let preview = manual_preparation::preview(&app.application, &current, &dates).unwrap();
    assert_eq!(preview.actual, [120, 0, 0, 0]);
    manual_preparation::save(
        &app.application,
        &current,
        &dates,
        &preview,
        [150, 60, 0, 0],
        &[],
    )
    .unwrap();
    let report = generate(&mut app, &[1]);
    assert_eq!(report.completed(), 1, "{:?}", report.entries);
    let pdf = path(&app, &next, 1);
    let extracted = pdf_extract::extract_text(&pdf).unwrap();
    assert!(extracted.contains("29/04/2026 2.5"), "{extracted}");
    assert!(extracted.contains("06/05/2026 1"), "{extracted}");
    assert_eq!(
        reconciliation::plan(&app.application, &current)
            .unwrap()
            .historical_reviews
            .len(),
        2
    );
    assert!(!path(&app, &prior, 1).exists());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM payroll_submissions", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn stage2a_unsent_amendment_keeps_reconciliation_baseline() {
    use crate::payroll_evidence::reconciliation;
    let (_dir, mut app, schedule) = fixture();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let record = app
        .application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa("2026/27", 1, 1)
        .unwrap()
        .unwrap();
    let file = path(&app, &schedule, 1);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository,
        record.id,
        1,
        "2026/27",
        1,
        &file,
        "synthetic",
        || Ok(()),
    )
    .unwrap();
    let conn = db(&app);
    let submission = crate::payroll_evidence::lifecycle::latest(&conn, record.id)
        .unwrap()
        .unwrap();
    let before = reconciliation::changes(&app.application, &record)
        .unwrap()
        .signature;
    save_feature2_hours(&app, 3.25);
    assert_eq!(
        reconciliation::changes(&app.application, &record)
            .unwrap()
            .signature,
        before
    );
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    crate::payroll_snapshot_service::verify_preview_or_test_attachment(
        &app.application.payroll_worked_item_repository,
        record.id,
        &file,
    )
    .unwrap();
    assert_eq!(
        reconciliation::changes(&app.application, &record)
            .unwrap()
            .signature,
        before
    );
    app.application
        .payroll_timesheet_email_repository
        .mark_sent(1, "2026/27", 1, "payslip", "synthetic settlement")
        .unwrap();
    assert_eq!(
        crate::payroll_evidence::lifecycle::latest(&conn, record.id).unwrap(),
        Some(submission)
    );
    assert_eq!(
        reconciliation::changes(&app.application, &record)
            .unwrap()
            .signature,
        before
    );
}

#[test]
fn stage2a_replacement_membership_and_partial_carry_follow_accepted_delivery() {
    stage2a_replacement_outcome("rejected");
    stage2a_replacement_outcome("accepted");
    stage2a_replacement_outcome("review-accepted");
}

#[test]
fn stage2a_uncertain_replacement_keeps_baseline_and_reserves_carry() {
    stage2a_replacement_outcome("uncertain");
}

fn stage2a_replacement_outcome(outcome: &str) {
    use crate::payroll_snapshot_service::{publish_candidate, CandidatePublication};
    use crate::{
        payroll_evidence::{lifecycle, reconciliation},
        timesheet_delivery as delivery,
    };
    let (_dir, mut app, schedule) = fixture();
    let mut conn = db(&app);
    let repos = &app.application.payroll_timesheet_repository;
    let record = repos
        .get_for_cycle_and_pa("2026/27", 1, 1)
        .unwrap()
        .unwrap();
    conn.execute_batch("INSERT INTO payroll_schedules(payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES
            ('2026/27',2,'04/03/2026','20/03/2026','30/03/2026','synthetic'),
            ('2026/27',3,'29/04/2026','20/05/2026','30/05/2026','synthetic')").unwrap();
    let origin = repos.insert("2026/27", 2, 1, None, "synthetic").unwrap();
    repos.insert("2026/27", 3, 1, None, "synthetic").unwrap();
    let later = repos
        .get_for_cycle_and_pa("2026/27", 3, 1)
        .unwrap()
        .unwrap();
    conn.execute("INSERT INTO payroll_corrections(id,personal_assistant_id,origin_payroll_timesheet_id,evidence_key,minutes,reason,created_at) VALUES(1,1,?1,'synthetic',-90,'synthetic','synthetic')", [origin]).unwrap();
    conn.execute(
        "UPDATE timesheets SET worked_minutes=30,end_time='2026-04-02T09:30:00' WHERE id=1",
        [],
    )
    .unwrap();
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let file = path(&app, &schedule, 1);
    // Reproduce a previously accepted partial application from the former
    // zero-floor policy, without editing any accepted ledger row.
    let worked = &app.application.payroll_worked_item_repository;
    let mut items = worked.get_snapshot_items(record.id).unwrap();
    for item in items.iter_mut().filter(|item| item.source_type == "carry_correction") {
        item.worked_minutes = -30;
    }
    conn.execute(
        "UPDATE payroll_correction_applications SET minutes=-30 WHERE payroll_timesheet_id=?1",
        [record.id],
    ).unwrap();
    let first_weeks: [i64; 4] = app.application.payroll_timesheet_repository.get_weeks(record.id)
        .unwrap().iter().map(|week| week.id).collect::<Vec<_>>().try_into().unwrap();
    publish_candidate(worked, CandidatePublication {
        payroll_timesheet_id: record.id,
        items: &items,
        final_pdf_path: &file,
        generated_at: "synthetic historical candidate",
        previous_cycle_minutes: -30,
        week_ids: &first_weeks,
        week_totals_minutes: &[0; 4],
    }, |path| { std::fs::write(path, b"%PDF synthetic original partial application")?; Ok(()) }).unwrap();
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository,
        record.id,
        1,
        "2026/27",
        1,
        &file,
        "synthetic",
        || Ok(()),
    )
    .unwrap();
    let original_bytes = std::fs::read(&file).unwrap();
    let first = lifecycle::latest(&conn, record.id).unwrap().unwrap();
    let initial = lifecycle::items(&conn, first).unwrap();
    assert!(initial.iter().any(|item| item.timesheet_id == Some(1)));
    // Represent a partial deduction on the original accepted submission.
    assert_eq!(
        conn.query_row(
            "SELECT minutes FROM payroll_submission_corrections WHERE submission_id=?1",
            [first],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        -30
    );
    let remaining = || {
        reconciliation::outstanding(&app.application, &later)
            .unwrap()
            .iter()
            .map(|c| c.minutes)
            .sum::<i64>()
    };
    assert_eq!(remaining(), -60);
    save_feature2_hours(&app, 0.75);
    conn.execute(
        "INSERT OR REPLACE INTO payroll_correction_applications VALUES(?1,1,-45)",
        [record.id],
    )
    .unwrap();
    let weeks: [i64; 4] = app
        .application
        .payroll_timesheet_repository
        .get_weeks(record.id)
        .unwrap()
        .iter()
        .map(|week| week.id)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    publish_candidate(
        &app.application.payroll_worked_item_repository,
        CandidatePublication {
            payroll_timesheet_id: record.id,
            items: &[],
            final_pdf_path: &file,
            generated_at: "synthetic",
            previous_cycle_minutes: -45,
            week_ids: &weeks,
            week_totals_minutes: &[45, 0, 0, 0],
        },
        |path| {
            std::fs::write(path, b"%PDF synthetic replacement")?;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        remaining(),
        -60,
        "generation preserves the submitted deduction"
    );
    let competing = if outcome == "uncertain" {
        let repo = &app.application.payroll_worked_item_repository;
        conn.execute(
            "INSERT INTO payroll_correction_applications VALUES(?1,1,-60)",
            [later.id],
        )
        .unwrap();
        let path = app
            .application
            .context
            .environment
            .data_dir
            .join("synthetic-competing.pdf");
        publish_candidate(
            repo,
            CandidatePublication {
                payroll_timesheet_id: later.id,
                items: &[],
                final_pdf_path: &path,
                generated_at: "synthetic",
                previous_cycle_minutes: -60,
                week_ids: &[0; 4],
                week_totals_minutes: &[0; 4],
            },
            |path| {
                std::fs::write(path, b"%PDF synthetic competing candidate")?;
                Ok(())
            },
        )
        .unwrap();
        let intent = delivery::capture(&conn, later.id).unwrap();
        let pdf = delivery::bytes(&conn, &intent).unwrap();
        Some((intent, pdf))
    } else {
        None
    };
    let intent = delivery::capture(&conn, record.id).unwrap();
    let bytes = delivery::bytes(&conn, &intent).unwrap();
    let delivery::Claim::Claimed(attempt) = delivery::claim(
        &conn,
        &intent,
        "fictional@example.test",
        "synthetic",
        &bytes,
    )
    .unwrap() else {
        panic!("expected new replacement claim")
    };
    delivery::finish(
        &conn,
        attempt,
        match outcome {
            "review-accepted" => "uncertain",
            "rejected" => "confirmed_not_sent",
            _ => outcome,
        },
        "synthetic transport",
    )
    .unwrap();
    if outcome == "review-accepted" {
        delivery::review(
            &mut conn,
            attempt,
            "accepted",
            "synthetic acceptance evidence",
        )
        .unwrap();
    }
    drop(conn);
    let conn = db(&app); // Reopen the database after transport/review.
    if outcome == "rejected" {
        // A posting left by an older build cannot make an unsent version authoritative.
        let version = crate::timesheet_amendments::latest_generated(&conn, record.id)
            .unwrap()
            .unwrap();
        conn.execute(
            "INSERT INTO payroll_corrections(personal_assistant_id,origin_payroll_timesheet_id,evidence_key,minutes,reason,created_at)
             VALUES(1,?1,?2,11,'synthetic historical draft posting','synthetic')",
            params![record.id, format!("version:{}:week:1|synthetic|0", version.document)],
        ).unwrap();
        assert_eq!(remaining(), -60, "hold a historical posting tied to an unsent document");
    }
    let accepted = matches!(outcome, "accepted" | "review-accepted");
    let latest = lifecycle::latest(&conn, record.id).unwrap().unwrap();
    assert_eq!(latest != first, accepted, "{outcome}");
    let membership = lifecycle::submitted_items(&conn, record.id, &app.application).unwrap();
    assert_eq!(
        membership.iter().any(|item| item.timesheet_id == Some(1)),
        !accepted,
        "{outcome}"
    );
    assert_eq!(
        remaining(),
        match outcome {
            "uncertain" => 0,
            "rejected" => -60,
            _ => -45,
        }
    );
    assert_eq!(
        remaining(),
        match outcome {
            "uncertain" => 0,
            "rejected" => -60,
            _ => -45,
        }
    );
    assert_eq!(
        lifecycle::submitted_correction_minutes(&conn, 1, None).unwrap(),
        if accepted { -45 } else { -30 }
    );
    let accepted_weeks = lifecycle::weeks(&conn, latest).unwrap();
    assert_eq!(
        accepted_weeks[0].worked_hours,
        if accepted { 0.75 } else { 0.0 }
    );
    assert_eq!(
        conn.query_row(
            "SELECT pdf_bytes FROM payroll_submissions WHERE id=?1",
            [first],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .unwrap(),
        original_bytes
    );
    if let Some((competing, pdf)) = competing {
        let error = delivery::claim(
            &conn,
            &competing,
            "fictional@example.test",
            "synthetic competing send",
            &pdf,
        )
        .err()
        .expect("competing correction must be held");
        assert!(error.to_string().contains("another unresolved attempt"));
        let mut reopened = db(&app);
        delivery::review(
            &mut reopened,
            attempt,
            "confirmed_not_sent",
            "synthetic rejection evidence",
        )
        .unwrap();
        assert_eq!(remaining(), -60);
        assert_eq!(
            lifecycle::latest(&reopened, record.id).unwrap(),
            Some(first)
        );
    }
    if accepted {
        assert_eq!(
            conn.query_row(
                "SELECT supersedes_id FROM payroll_submissions WHERE id=?1",
                [latest],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            first
        );
        assert!(matches!(
            delivery::claim(
                &conn,
                &intent,
                "fictional@example.test",
                "synthetic",
                &bytes
            )
            .unwrap(),
            delivery::Claim::AlreadyAccepted
        ));
        let resend = delivery::capture(&conn, record.id).unwrap();
        assert!(resend.resend);
        let delivery::Claim::Claimed(retry) = delivery::claim(
            &conn,
            &resend,
            "fictional@example.test",
            "synthetic resend",
            &bytes,
        )
        .unwrap() else {
            panic!("expected resend claim")
        };
        delivery::finish(&conn, retry, "accepted", "synthetic transport").unwrap();
        assert_eq!(lifecycle::latest(&conn, record.id).unwrap(), Some(latest));
    }
}

#[test]
fn stage2a_business_examples_week_one_and_pdf() {
    use crate::payroll_evidence::{lifecycle, reconciliation};

    let (dir, mut app, previous) = fixture();
    let conn = db(&app);
    app.application.context.config.payroll.rounding_minutes = 15;
    app.application.context.config.payroll.rounding_direction = "Up".into();
    let record = |cycle, pa| {
        conn.query_row(
            "SELECT id FROM payroll_timesheets WHERE cycle_number=?1 AND personal_assistant_id=?2",
            params![cycle, pa],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    for (pa, week, hours) in [(1, 3, 16), (1, 4, 24), (2, 4, 16)] {
        app.application
            .payroll_worked_item_repository
            .set_manual_adjustment(
                record(1, pa),
                &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                    week_number: week,
                    adjustment_minutes: 60 * hours,
                    reason: Some("estimate".into()),
                },
                "synthetic estimate",
            )
            .unwrap();
    }
    assert_eq!(generate(&mut app, &[1, 2]).completed(), 2);
    for pa in [1, 2] {
        crate::payroll_snapshot_service::send_production_candidate(
            &app.application.payroll_worked_item_repository,
            record(1, pa),
            pa,
            "2026/27",
            1,
            &path(&app, &previous, pa),
            "synthetic",
            || Ok(()),
        )
        .unwrap();
    }
    let schedule = feature2_cycle_schedule(&app, 2, "29/04/2026", "30/05/2026");
    app.operational_payroll_period.select(&schedule, None);
    let folder = &app.application.context.config.folders.csv_import;
    crate::private_storage::ensure_private_dir(folder).unwrap();
    let csv = concat!(
        "Fictional Hours Keeper export\nPA,Start,End,Break,Worked,Rate,Amount,Note\n",
        "PA1 Test,15/04/2026 at 00:00:00,15/04/2026 at 20:53:00,0h 00m,20h 53m,12,0,synthetic\n",
        "PA1 Test,22/04/2026 at 00:00:00,22/04/2026 at 22:53:00,0h 00m,22h 53m,12,0,synthetic\n",
        "PA1 Test,29/04/2026 at 00:00:00,29/04/2026 at 13:00:00,0h 00m,13h 00m,12,0,synthetic\n",
        "PA1 Test,30/04/2026 at 00:00:00,30/04/2026 at 13:00:00,0h 00m,13h 00m,12,0,synthetic\n",
        "PA2 Test,29/04/2026 at 00:00:00,29/04/2026 at 12:00:00,0h 00m,12h 00m,12,0,synthetic\n",
    );
    for name in ["actuals.csv", "duplicate-export.csv"] {
        crate::private_storage::write_new(&folder.join(name), csv.as_bytes()).unwrap();
        assert!(!app.application.import_csv().unwrap().has_failures());
    }
    let repo = &app.application.payroll_timesheet_repository;
    let historic1 = repo.get_for_cycle_and_pa("2026/27", 1, 1).unwrap().unwrap();
    let historic2 = repo.get_for_cycle_and_pa("2026/27", 1, 2).unwrap().unwrap();
    let changes = reconciliation::changes(&app.application, &historic1).unwrap();
    assert_eq!(
        changes
            .components
            .iter()
            .map(|part| part.minutes)
            .collect::<Vec<_>>(),
        [300, -60]
    );
    assert!(changes.requires_complete);
    assert!(
        reconciliation::carry(&app.application, &historic1, &changes.signature, false).is_err()
    );
    reconciliation::carry(&app.application, &historic1, &changes.signature, true).unwrap();
    let ctx = egui::Context::default();
    let mut preparation = crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
    crate::gui_controls::acceptance::frame(
        &ctx,
        vec![],
        |ui| preparation.show(ui, &app.application, &schedule, "synthetic next cycle"),
        "Save PA1 Test",
    );
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let positive = path(&app, &schedule, 1);
    let pdf = pdf_extract::extract_text(&positive).unwrap();
    assert!(pdf.contains("29/04/2026 30"), "{pdf}");
    assert!(pdf.contains("(Info only +4.00 hours)"), "{pdf}");
    // Empty historical CSV coverage requires explicit confirmation.
    let changes = reconciliation::changes(&app.application, &historic2).unwrap();
    assert_eq!(
        changes
            .components
            .iter()
            .map(|part| part.minutes)
            .collect::<Vec<_>>(),
        [-960]
    );
    assert!(changes.requires_complete);
    assert!(
        reconciliation::carry(&app.application, &historic2, &changes.signature, false).is_err()
    );
    reconciliation::carry(&app.application, &historic2, &changes.signature, true).unwrap();
    assert!(reconciliation::carry(&app.application, &historic2, &changes.signature, true).is_err());
    assert_eq!(generate(&mut app, &[1, 2]).completed(), 2);
    assert_eq!(generate(&mut app, &[1, 2]).completed(), 2);
    let negative = path(&app, &schedule, 2);
    let pdf = pdf_extract::extract_text(&negative).unwrap();
    assert!(pdf.contains("29/04/2026 -4"), "{pdf}");
    assert!(pdf.contains("(Info only -16.00 hours)"), "{pdf}");
    for (pa, expected_minutes) in [(1, 1800), (2, -240)] {
        let file = path(&app, &schedule, pa);
        let id = record(2, pa);
        crate::payroll_snapshot_service::send_production_candidate(
            &app.application.payroll_worked_item_repository,
            id,
            pa,
            "2026/27",
            2,
            &file,
            "synthetic",
            || Ok(()),
        )
        .unwrap();
        let reopened = db(&app);
        let sid = lifecycle::latest(&reopened, id).unwrap().unwrap();
        assert_eq!(
            lifecycle::weeks(&reopened, sid).unwrap()[0].worked_hours * 60.0,
            expected_minutes as f64
        );
    }
    // A notes amendment exercises generation and replacement of an accepted
    // negative Week 1 through the ordinary delivery ledger.
    use crate::timesheet_delivery as delivery;
    let id = record(2, 2);
    let accepted = lifecycle::latest(&conn, id).unwrap().unwrap();
    let accepted_pdf = std::fs::read(&negative).unwrap();
    let repo = &app.application.payroll_timesheet_repository;
    let mut replacement = repo.get_for_cycle_and_pa("2026/27", 2, 2).unwrap().unwrap();
    replacement.payroll_department_notes = "Synthetic replacement note".into();
    let weeks = repo.get_weeks(id).unwrap();
    let saved = app
        .application
        .payroll_worked_item_repository
        .get_manual_adjustments(id)
        .unwrap();
    let adjustments = weeks
        .iter()
        .map(|week| {
            saved
                .iter()
                .find(|adjustment| adjustment.week_number == week.week_number)
                .cloned()
                .unwrap_or(
                    crate::payroll_worked_item_repository::ManualHoursAdjustment {
                        week_number: week.week_number,
                        adjustment_minutes: 0,
                        reason: None,
                    },
                )
        })
        .collect::<Vec<_>>();
    repo.save_preparation_atomically(
        &replacement,
        &weeks,
        &repo.get_public_holidays(id).unwrap(),
        &repo.get_annual_leave(id).unwrap(),
        &adjustments,
        "synthetic replacement",
    )
    .unwrap();
    assert_eq!(generate(&mut app, &[2]).completed(), 1);
    let replacement_text = pdf_extract::extract_text(&negative).unwrap();
    assert!(replacement_text.contains("29/04/2026 -4"));
    assert!(replacement_text.contains("(Info only -16.00 hours)"));
    for outcome in ["confirmed_not_sent", "uncertain"] {
        let intent = delivery::capture(&conn, id).unwrap();
        let attachment = delivery::bytes(&conn, &intent).unwrap();
        let delivery::Claim::Claimed(attempt) = delivery::claim(
            &conn,
            &intent,
            "fictional@example.test",
            outcome,
            &attachment,
        )
        .unwrap() else {
            panic!("replacement should have a new attempt")
        };
        delivery::finish(&conn, attempt, outcome, "synthetic transport result").unwrap();
        assert_eq!(lifecycle::latest(&conn, id).unwrap(), Some(accepted));
        if outcome == "uncertain" {
            let mut reviewed = db(&app);
            delivery::review(
                &mut reviewed,
                attempt,
                "accepted",
                "synthetic acceptance evidence",
            )
            .unwrap();
            let submitted = lifecycle::latest(&reviewed, id).unwrap().unwrap();
            assert_ne!(submitted, accepted);
            assert_eq!(
                lifecycle::weeks(&reviewed, submitted).unwrap()[0].worked_hours,
                -4.0
            );
            assert_eq!(
                reviewed
                    .query_row(
                        "SELECT supersedes_id FROM payroll_submissions WHERE id=?1",
                        [submitted],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap(),
                accepted
            );
        }
    }
    assert_eq!(
        conn.query_row(
            "SELECT pdf_bytes FROM payroll_submissions WHERE id=?1",
            [accepted],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .unwrap(),
        accepted_pdf
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM payroll_submissions WHERE payroll_timesheet_id=?1",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM timesheet_amendment_versions WHERE payroll_timesheet_id=?1",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );

    // Rebuild the application and its repositories against the same disposable profile.
    drop(app);
    let mut app = DirectPaymentApp::new(
        crate::payroll_timesheet_screen::tests::reopen_test_application(dir.path()),
    );
    let third = feature2_cycle_schedule(&app, 3, "27/05/2026", "30/06/2026");
    app.operational_payroll_period.select(&third, None);
    let ctx = egui::Context::default();
    let mut preparation = crate::payroll_timesheet_screen::PayrollTimesheetScreen::new();
    crate::gui_controls::acceptance::frame(
        &ctx,
        vec![],
        |ui| preparation.show(ui, &app.application, &third, "synthetic next cycle"),
        "Save PA1 Test",
    );
    assert_eq!(generate(&mut app, &[1, 2]).completed(), 2);
    for pa in [1, 2] {
        let result = db(&app).query_row("SELECT COALESCE(SUM(worked_hours),0) FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=?1",
            [record(3, pa)], |r| r.get::<_, f64>(0)).unwrap();
        assert_eq!(result, 0.0);
    }
}

fn historical_verification_fixture() -> (
    crate::private_storage::PrivateTempDir,
    DirectPaymentApp,
    PayrollSchedule,
) {
    let (dir, app, schedule) = fixture();
    // The standard fixture includes an unrelated April shift from an open
    // preparation. Exclude it so only the submitted facts below are in scope.
    db(&app)
        .execute("DELETE FROM timesheets WHERE id=1", [])
        .unwrap();
    (dir, app, schedule)
}

fn historical_verification_period(
    app: &DirectPaymentApp,
    cycle: i64,
    start: &str,
) -> (
    PayrollSchedule,
    crate::payroll_timesheet_repository::PayrollTimesheet,
) {
    let schedule = feature2_cycle_schedule(app, cycle, start, start);
    let repo = &app.application.payroll_timesheet_repository;
    let id = repo.insert("2026/27", cycle, 1, None, "fictional").unwrap();
    let start = crate::date_utils::parse_legacy(start).unwrap();
    let dates = std::array::from_fn(|week| {
        (start + chrono::Duration::weeks(week as i64))
            .format("%d/%m/%Y")
            .to_string()
    });
    repo.create_missing_weeks(id, &dates, &[0.; 4]).unwrap();
    (
        schedule,
        repo.get_for_cycle_and_pa("2026/27", cycle, 1)
            .unwrap()
            .unwrap(),
    )
}

fn historical_verification_import(app: &mut DirectPaymentApp, name: &str, rows: &[(&str, u32)]) {
    let folder = &app.application.context.config.folders.csv_import;
    crate::private_storage::ensure_private_dir(folder).unwrap();
    let mut csv = String::from("Fictional export\nPA,Start,End,Break,Worked,Rate,Amount,Note\n");
    for (day, minutes) in rows {
        assert!(*minutes < 24 * 60);
        csv.push_str(&format!(
            "PA1 Test,{day} at 00:00:00,{day} at {:02}:{:02}:00,0h 00m,{}h {:02}m,12,0,fictional\n",
            minutes / 60,
            minutes % 60,
            minutes / 60,
            minutes % 60,
        ));
    }
    crate::private_storage::write_new(&folder.join(name), csv.as_bytes()).unwrap();
    let result = app.application.import_csv().unwrap();
    assert!(!result.has_failures(), "{:?}", result.failure_messages);
}

fn historical_verification_submit(
    app: &mut DirectPaymentApp,
    schedule: &PayrollSchedule,
    record: &crate::payroll_timesheet_repository::PayrollTimesheet,
    estimates: &[i64],
) -> i64 {
    for (week, minutes) in estimates
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, minutes)| *minutes != 0)
    {
        app.application
            .payroll_worked_item_repository
            .set_manual_adjustment(
                record.id,
                &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                    week_number: week as i64 + 1,
                    adjustment_minutes: minutes,
                    reason: Some("estimate".into()),
                },
                "fictional estimate",
            )
            .unwrap();
    }
    app.operational_payroll_period.select(schedule, None);
    let result = generate(app, &[1]);
    assert_eq!(result.completed(), 1, "{:?}", result.entries);
    crate::payroll_snapshot_service::send_production_candidate(
        &app.application.payroll_worked_item_repository,
        record.id,
        1,
        "2026/27",
        record.cycle_number,
        &path(app, schedule, 1),
        "fictional transport",
        || Ok(()),
    )
    .unwrap();
    crate::payroll_evidence::lifecycle::latest(&db(app), record.id)
        .unwrap()
        .unwrap()
}

fn historical_verification_calculate(
    app: &DirectPaymentApp,
    record: &crate::payroll_timesheet_repository::PayrollTimesheet,
    start: &str,
) -> crate::pay_rate_allocation::ReconciledPayrollHours {
    let start = crate::date_utils::parse_legacy(start).unwrap();
    crate::payroll_evidence::reconciliation::calculate(
        &app.application,
        record,
        &std::array::from_fn(|week| start + chrono::Duration::weeks(week as i64)),
        None,
    )
    .unwrap()
}

#[test]
fn stage2a_historical_all_weeks_multiple_cycles_and_restart() {
    use crate::payroll_evidence::{lifecycle, reconciliation};
    let (dir, mut app, _) = historical_verification_fixture();
    let (early, early_record) = historical_verification_period(&app, 2, "29/04/2026");
    historical_verification_submit(&mut app, &early, &early_record, &[120]);
    for (cycle, start) in [(3, "27/05/2026"), (4, "24/06/2026")] {
        let (schedule, record) = historical_verification_period(&app, cycle, start);
        historical_verification_submit(&mut app, &schedule, &record, &[]);
    }
    let (previous, previous_record) = historical_verification_period(&app, 5, "22/07/2026");
    historical_verification_submit(&mut app, &previous, &previous_record, &[120, 120, 120, 240]);
    let (current, current_record) = historical_verification_period(&app, 6, "19/08/2026");
    for name in ["historical.csv", "repeat.csv"] {
        historical_verification_import(
            &mut app,
            name,
            &[
                ("29/04/2026", 180),
                ("22/07/2026", 173),
                ("29/07/2026", 53),
                ("05/08/2026", 180),
                ("12/08/2026", 60),
                ("19/08/2026", 60),
            ],
        );
    }
    let early_change = reconciliation::changes(&app.application, &early_record).unwrap();
    assert_eq!(
        early_change
            .components
            .iter()
            .map(|c| c.minutes)
            .collect::<Vec<_>>(),
        [60]
    );
    let previous_change = reconciliation::changes(&app.application, &previous_record).unwrap();
    assert_eq!(
        previous_change
            .components
            .iter()
            .map(|c| c.minutes)
            .collect::<Vec<_>>(),
        [60, -60, 60, -180]
    );
    // The two deductions are proposals pending confirmation; the credits can post automatically.
    assert!(previous_change.requires_complete);
    assert!(reconciliation::carry(
        &app.application,
        &previous_record,
        &previous_change.signature,
        false
    )
    .is_err());
    let held = historical_verification_calculate(&app, &current_record, "19/08/2026");
    assert_eq!(held.week_totals_minutes, [240, 0, 0, 0]);
    let previous_change = reconciliation::changes(&app.application, &previous_record).unwrap();
    reconciliation::carry(
        &app.application,
        &previous_record,
        &previous_change.signature,
        true,
    )
    .unwrap();
    assert!(
        !reconciliation::changes(&app.application, &previous_record)
            .unwrap()
            .changed
    );
    let current_hours = historical_verification_calculate(&app, &current_record, "19/08/2026");
    assert_eq!(current_hours.previous_cycle_minutes, -60);
    assert_eq!(current_hours.week_totals_minutes, [0; 4]);
    assert_eq!(
        current_hours
            .snapshot_items
            .iter()
            .filter(|item| item.timesheet_id.is_some())
            .map(|item| item.work_date.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["2026-08-19"]
    );
    app.operational_payroll_period.select(&current, None);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    drop(app);
    let mut app = DirectPaymentApp::new(
        crate::payroll_timesheet_screen::tests::reopen_test_application(dir.path()),
    );
    app.operational_payroll_period.select(&current, None);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let pdf = pdf_extract::extract_text(&path(&app, &current, 1)).unwrap();
    assert!(pdf.contains("(Info only -1.00 hours)"));
    let sid = historical_verification_submit(&mut app, &current, &current_record, &[]);
    assert_eq!(
        lifecycle::weeks(&db(&app), sid).unwrap()[0].worked_hours,
        0.0
    );
    let (_, next) = historical_verification_period(&app, 7, "16/09/2026");
    assert_eq!(
        historical_verification_calculate(&app, &next, "16/09/2026").week_totals_minutes,
        [0; 4]
    );
    assert!(reconciliation::outstanding(&app.application, &next)
        .unwrap()
        .is_empty());
}

#[test]
fn stage2a_historical_december_early_submission_and_january_reconciliation() {
    use crate::payroll_evidence::reconciliation;
    let (_dir, mut app, _) = historical_verification_fixture();
    let (december, record) = historical_verification_period(&app, 10, "07/12/2026");
    // The schedule normally posts on the 18th. This fixture submits on the 11th,
    // retaining actual work already known and estimates for the other three weeks.
    let conn = db(&app);
    conn.execute(
        "UPDATE payroll_schedules SET latest_posting_date='18/12/2026' WHERE id=?1",
        [december.id],
    )
    .unwrap();
    reconciliation::TEST_TODAY
        .with(|clock| clock.set(Some(chrono::NaiveDate::from_ymd_opt(2026, 12, 11).unwrap())));
    historical_verification_import(&mut app, "early-december.csv", &[("08/12/2026", 120)]);
    let sid = historical_verification_submit(&mut app, &december, &record, &[0, 120, 120, 180]);
    // The delivery fixture uses the host timestamp; set the synthetic historical fact.
    conn.execute(
        "UPDATE payroll_submissions SET submitted_at='2026-12-11 12:00:00' WHERE id=?1",
        [sid],
    )
    .unwrap();
    assert!(reconciliation::changes(&app.application, &record)
        .unwrap()
        .components
        .is_empty());
    reconciliation::TEST_TODAY
        .with(|clock| clock.set(Some(chrono::NaiveDate::from_ymd_opt(2027, 1, 8).unwrap())));
    let (january, current) = historical_verification_period(&app, 11, "04/01/2027");
    historical_verification_import(
        &mut app,
        "january-export.csv",
        &[
            ("08/12/2026", 120),
            ("14/12/2026", 173),
            ("21/12/2026", 60),
            ("28/12/2026", 60),
            ("04/01/2027", 60),
        ],
    );
    let changed = reconciliation::changes(&app.application, &record).unwrap();
    assert_eq!(
        changed
            .components
            .iter()
            .map(|c| c.minutes)
            .collect::<Vec<_>>(),
        [60, -60, -120]
    );
    assert!(changed.requires_complete);
    assert!(reconciliation::carry(&app.application, &record, &changed.signature, false).is_err());
    reconciliation::carry(&app.application, &record, &changed.signature, true).unwrap();
    let payable = historical_verification_calculate(&app, &current, "04/01/2027");
    assert_eq!(payable.previous_cycle_minutes, -120);
    assert_eq!(payable.week_totals_minutes, [-60, 0, 0, 0]);
    app.operational_payroll_period.select(&january, None);
    assert_eq!(generate(&mut app, &[1]).completed(), 1);
    let pdf = pdf_extract::extract_text(&path(&app, &january, 1)).unwrap();
    assert!(pdf.contains("04/01/2027 -1"));
    assert!(pdf.contains("(Info only -2.00 hours)"));
    reconciliation::TEST_TODAY.with(|clock| clock.set(None));
}

#[test]
fn stage2a_historical_opaque_legacy_requires_review() {
    use crate::payroll_evidence::reconciliation;
    let (_dir, mut app, _) = historical_verification_fixture();
    let (_, old) = historical_verification_period(&app, 2, "29/04/2026");
    let (_, current) = historical_verification_period(&app, 6, "19/08/2026");
    app.application
        .payroll_timesheet_email_repository
        .mark_sent(1, "2026/27", 2, "payslip", "fictional legacy settlement")
        .unwrap();
    db(&app).execute("UPDATE payroll_timesheet_weeks SET worked_hours=2 WHERE payroll_timesheet_id=?1 AND week_number=1", [old.id]).unwrap();
    historical_verification_import(&mut app, "ambiguous.csv", &[("29/04/2026", 180)]);
    reconciliation::sync_settled_corrections(&app.application, 1).unwrap();
    let plan = reconciliation::plan(&app.application, &current).unwrap();
    assert_eq!(plan.historical_reviews.len(), 1);
    assert!(plan.work.is_empty());
    assert!(reconciliation::outstanding(&app.application, &current)
        .unwrap()
        .is_empty());
}

#[test]
fn stage2a_historical_zero_net_still_submits_both_components() {
    use crate::payroll_evidence::{lifecycle, reconciliation};
    let (_dir, mut app, _) = historical_verification_fixture();
    let (old, old_record) = historical_verification_period(&app, 2, "29/04/2026");
    historical_verification_submit(&mut app, &old, &old_record, &[120, 120]);
    let (current, record) = historical_verification_period(&app, 3, "27/05/2026");
    historical_verification_import(
        &mut app,
        "cancel.csv",
        &[("29/04/2026", 180), ("06/05/2026", 60), ("27/05/2026", 120)],
    );
    let change = reconciliation::changes(&app.application, &old_record).unwrap();
    assert_eq!(
        change
            .components
            .iter()
            .map(|c| c.minutes)
            .collect::<Vec<_>>(),
        [60, -60]
    );
    reconciliation::carry(&app.application, &old_record, &change.signature, true).unwrap();
    let hours = historical_verification_calculate(&app, &record, "27/05/2026");
    assert_eq!(hours.week_totals_minutes, [120, 0, 0, 0]);
    assert_eq!(hours.previous_cycle_minutes, 0);
    let sid = historical_verification_submit(&mut app, &current, &record, &[]);
    let conn = db(&app);
    assert_eq!(lifecycle::weeks(&conn, sid).unwrap()[0].worked_hours, 2.0);
    let mut stmt = conn.prepare(
        "SELECT minutes FROM payroll_submission_corrections WHERE submission_id=?1 ORDER BY minutes",
    ).unwrap();
    let applications = stmt
        .query_map([sid], |row| row.get::<_, i64>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(applications, [-60, 60]);
    let (_, next) = historical_verification_period(&app, 4, "24/06/2026");
    assert!(reconciliation::outstanding(&app.application, &next)
        .unwrap()
        .is_empty());
}

#[test]
fn stage2a_historical_new_csv_in_ordinary_week_uses_payable_minutes() {
    use crate::payroll_evidence::reconciliation;
    let (_dir, mut app, _) = historical_verification_fixture();
    app.application.context.config.payroll.rounding_minutes = 15;
    app.application.context.config.payroll.rounding_direction = "Up".into();
    let (old, old_record) = historical_verification_period(&app, 2, "29/04/2026");
    historical_verification_import(&mut app, "ordinary.csv", &[("29/04/2026", 120)]);
    historical_verification_submit(&mut app, &old, &old_record, &[]);
    let (_, current) = historical_verification_period(&app, 6, "19/08/2026");
    historical_verification_import(
        &mut app,
        "late-ordinary.csv",
        &[("30/04/2026", 53), ("19/08/2026", 60)],
    );
    let changes = reconciliation::changes(&app.application, &old_record).unwrap();
    assert_eq!(changes.components.len(), 1);
    assert_eq!(changes.components[0].date.as_deref(), Some("2026-04-30"));
    assert_eq!(changes.components[0].minutes, 60);
    assert!(!changes.requires_complete);
    reconciliation::carry(&app.application, &old_record, &changes.signature, false).unwrap();
    let hours = historical_verification_calculate(&app, &current, "19/08/2026");
    assert_eq!(hours.previous_cycle_minutes, 60);
    assert_eq!(hours.week_totals_minutes, [120, 0, 0, 0]);
}
