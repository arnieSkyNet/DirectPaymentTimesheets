// Included in payroll_timesheet_screen::tests to exercise real preparation paths.
fn stored_note(app: &Application, record: &PayrollTimesheet) -> String {
    app.payroll_timesheet_repository
        .get_for_cycle_and_pa(
            &record.payroll_year,
            record.cycle_number,
            record.personal_assistant_id,
        )
        .unwrap()
        .unwrap()
        .payroll_department_notes
}

#[test]
fn payroll_notes_round_trip_unicode_limit_whitespace_and_reopen() {
    let (_dir, app, schedule, mut screen) = load_active_record();
    let record = screen.weeks[0].0.clone();
    for note in [
        "Please include any hours in lieu in final pay.\nThank you.  ".into(),
        "é".repeat(256),
        " \n\r\n ".into(),
        String::new(),
    ] {
        screen.weeks[0].0.payroll_department_notes = note.clone();
        screen.save_current(&app).unwrap();
        let reopened = crate::payroll_timesheet_repository::PayrollTimesheetRepository::new(
            setup_connection(&app),
        );
        assert_eq!(
            reopened
                .get_for_cycle_and_pa(
                    &record.payroll_year,
                    record.cycle_number,
                    record.personal_assistant_id
                )
                .unwrap()
                .unwrap()
                .payroll_department_notes,
            note
        );
        screen.reload();
        screen.load(&app, &schedule, "period").unwrap();
        assert_eq!(screen.weeks[0].0.payroll_department_notes, note);
        assert!(!screen.has_unsaved_changes());
    }
    create_candidate(&app, &screen);
    screen.weeks[0].0.payroll_department_notes = "é".repeat(257);
    assert!(screen.save_current(&app).is_err());
    assert_eq!(stored_note(&app, &record), "");
    assert!(app
        .payroll_worked_item_repository
        .snapshot_metadata(record.id)
        .unwrap()
        .is_some());
    // Repository validation is independent of UI validation, and SQL also constrains direct writes.
    let repo = &app.payroll_timesheet_repository;
    assert!(repo
        .save_preparation_atomically(
            &screen.weeks[0].0,
            &screen.weeks[0].1,
            &screen.public_holidays[0],
            &[],
            &[],
            "later"
        )
        .is_err());
    assert!(setup_connection(&app)
        .execute(
            "UPDATE payroll_timesheets SET payroll_department_notes=?1 WHERE id=?2",
            params!["é".repeat(257), record.id]
        )
        .is_err());
}

#[test]
fn payroll_notes_dirty_save_discard_cancel_and_pa_isolation() {
    for choice in [
        UnsavedChoice::Save,
        UnsavedChoice::Discard,
        UnsavedChoice::Cancel,
    ] {
        let (_dir, app, _, mut screen) = two_pa_preparation();
        screen.request_pa(Some(1));
        let record = screen.weeks[0].0.clone();
        screen.weeks[0].0.payroll_department_notes = "A\nB".into();
        assert!(screen.has_unsaved_changes());
        screen.request_pa(Some(2));
        let proceed = screen.resolve_unsaved(&app, choice).unwrap();
        screen.finish_pa_switch(proceed);
        match choice {
            UnsavedChoice::Save => {
                assert_eq!(stored_note(&app, &record), "A\nB");
                assert_eq!(screen.selected_pa, Some(2));
            }
            UnsavedChoice::Discard => {
                assert_eq!(stored_note(&app, &record), "");
                assert_eq!(screen.weeks[0].0.payroll_department_notes, "");
            }
            UnsavedChoice::Cancel => {
                assert_eq!(stored_note(&app, &record), "");
                assert_eq!(screen.selected_pa, Some(1));
                assert!(screen.has_unsaved_changes());
            }
        }
        assert_eq!(stored_note(&app, &screen.weeks[1].0), "");
    }
}

#[test]
fn payroll_notes_invalidate_only_changed_candidate_and_rollback_atomically() {
    let (_dir, app, schedule, mut screen) = two_pa_preparation();
    screen.request_pa(Some(1));
    create_candidate(&app, &screen);
    let record = screen.weeks[0].0.clone();
    let before = app
        .payroll_worked_item_repository
        .snapshot_metadata(record.id)
        .unwrap()
        .unwrap();
    screen.save_current(&app).unwrap();
    assert_eq!(
        app.payroll_worked_item_repository
            .snapshot_metadata(record.id)
            .unwrap()
            .unwrap()
            .pdf_sha256,
        before.pdf_sha256
    );
    screen.weeks[0].0.payroll_department_notes = "New payroll note".into();
    let db = setup_connection(&app);
    db.execute_batch("CREATE TRIGGER refuse_notes BEFORE UPDATE OF payroll_department_notes ON payroll_timesheets BEGIN SELECT RAISE(ABORT, 'test refusal'); END;").unwrap();
    assert!(screen.save_current(&app).is_err());
    assert_eq!(stored_note(&app, &record), "");
    assert!(screen.has_unsaved_changes());
    assert!(app
        .payroll_worked_item_repository
        .snapshot_metadata(record.id)
        .unwrap()
        .is_some());
    db.execute_batch("DROP TRIGGER refuse_notes").unwrap();
    let result = save_current_row(&app, &schedule, &screen).unwrap();
    assert!(result.changed && result.candidate_invalidated);
    assert_eq!(stored_note(&app, &record), "New payroll note");
    assert!(app
        .payroll_worked_item_repository
        .snapshot_metadata(record.id)
        .unwrap()
        .is_none());
    assert!(
        crate::payroll_snapshot_service::verify_preview_or_test_attachment(
            &app.payroll_worked_item_repository,
            record.id,
            std::path::Path::new("/tmp/stale-candidate.pdf")
        )
        .is_err()
    );
    assert_eq!(stored_note(&app, &screen.weeks[1].0), "");
}

#[test]
fn payroll_notes_protected_states_refuse_and_returned_control_stays_available() {
    for state in ["submitted", "indeterminate", "settled"] {
        let (_dir, app, schedule, mut screen) = load_active_record();
        let record = screen.weeks[0].0.clone();
        create_candidate(&app, &screen);
        let db = setup_connection(&app);
        if state == "settled" {
            app.payroll_timesheet_email_repository
                .mark_sent(
                    record.personal_assistant_id,
                    &record.payroll_year,
                    record.cycle_number,
                    "payslip",
                    "sent",
                )
                .unwrap();
        } else {
            db.execute("UPDATE payroll_timesheet_snapshot_states SET state=?1 WHERE payroll_timesheet_id=?2", params![state, record.id]).unwrap();
        }
        db.execute("UPDATE personal_assistants SET employment_status='Inactive',leaving_date='2020-01-01' WHERE id=?1",[record.personal_assistant_id]).unwrap();
        screen.weeks[0].0.payroll_department_notes = "Cannot overwrite submitted note".into();
        assert!(screen.save_current(&app).is_err());
        assert_eq!(stored_note(&app, &record), "");
        screen.reload();
        screen.load(&app, &schedule, "period").unwrap();
        let ctx = egui::Context::default();
        let labels = render_preparation(&ctx, &app, &schedule, &mut screen);
        assert!(labels.iter().any(|s| s == "Returned payroll information"));
        assert!(labels.iter().any(|s| s == "Save returned hours"));
        app.payroll_timesheet_repository
            .save_actual_in_lieu_hours(record.id, Some(8.0))
            .unwrap();
    }
}

// Capture every table, excluding only the two independently maintained result columns.
fn outgoing_database_facts(db: &Connection) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |r| r.get::<_, String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    tables.into_iter().map(|table| {
        let columns = if table == "payroll_timesheets" { "id,personal_assistant_id,payroll_year,cycle_number,previous_cycle_hours,created_at,updated_at,payroll_department_notes" } else { "*" };
        let mut query = db.prepare(&format!("SELECT {columns} FROM {table} ORDER BY rowid")).unwrap();
        let count = query.column_count();
        let rows = query.query_map([], |row| (0..count).map(|i| row.get(i)).collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        (table, rows)
    }).collect()
}

#[test]
fn actual_in_lieu_save_edit_clear_preserves_all_outgoing_facts_in_every_state() {
    for state in ["candidate", "submitted", "settled", "indeterminate"] {
        let (dir, app, _, mut screen) = load_active_record();
        let record = screen.weeks[0].0.clone();
        screen.weeks[0].0.payroll_department_notes = "Payroll, please confirm lieu hours".into();
        screen.weeks[0].1[0].worked_hours = 4.0;
        screen.save_current(&app).unwrap();
        create_candidate(&app, &screen);
        let db = setup_connection(&app);
        let pdf = dir.path().join("candidate.pdf");
        std::fs::write(&pdf, b"unchanged retained PDF bytes").unwrap();
        db.execute("UPDATE payroll_timesheet_snapshot_states SET pdf_path=?1,pdf_sha256=?2 WHERE payroll_timesheet_id=?3", params![pdf.to_str().unwrap(),crate::payroll_snapshot_service::sha256_file(&pdf).unwrap(),record.id]).unwrap();
        db.execute("UPDATE payroll_timesheet_weeks SET worked_hours=4,annual_leave_hours=2,travel_miles=3,sick_leave_hours=1 WHERE payroll_timesheet_id=?1", [record.id]).unwrap();
        db.execute("INSERT INTO payroll_timesheet_annual_leave (payroll_timesheet_id,week_number,leave_date,hours,created_at,updated_at) VALUES (?1,1,'2026-09-01',2,'created','updated')", [record.id]).unwrap();
        db.execute("INSERT INTO personal_assistant_sickness_periods(personal_assistant_id,start_date,end_date) VALUES (?1,'2026-09-02','2026-09-03')", [record.personal_assistant_id]).unwrap();
        db.execute("INSERT INTO payroll_corrections(personal_assistant_id,origin_payroll_timesheet_id,evidence_key,minutes,reason,created_at) VALUES (?1,?2,'retained',60,'existing correction','created')",params![record.personal_assistant_id,record.id]).unwrap();
        if state != "candidate" {
            crate::payroll_evidence::lifecycle::archive_submission(&db, record.id, "submitted")
                .unwrap();
            db.execute("UPDATE payroll_timesheet_snapshot_states SET state=?1 WHERE payroll_timesheet_id=?2",params![if state == "settled" { "submitted" } else { state },record.id]).unwrap();
            app.payroll_timesheet_email_repository
                .mark_sent(
                    record.personal_assistant_id,
                    &record.payroll_year,
                    record.cycle_number,
                    if state == "settled" {
                        "payslip"
                    } else {
                        "timesheet"
                    },
                    "retained delivery",
                )
                .unwrap();
        }
        let before = outgoing_database_facts(&db);
        for value in [Some(0.0), Some(8.0), Some(8.125), None] {
            app.payroll_timesheet_repository
                .save_actual_in_lieu_hours(record.id, value)
                .unwrap();
            let reopened = crate::payroll_timesheet_repository::PayrollTimesheetRepository::new(
                setup_connection(&app),
            );
            let stored = reopened
                .get_for_cycle_and_pa(
                    &record.payroll_year,
                    record.cycle_number,
                    record.personal_assistant_id,
                )
                .unwrap()
                .unwrap();
            assert_eq!(stored.actual_in_lieu_hours, value);
            assert!(stored.actual_in_lieu_updated_at.is_some());
            assert_eq!(outgoing_database_facts(&db), before, "{state}");
            assert_eq!(
                std::fs::read(&pdf).unwrap(),
                b"unchanged retained PDF bytes"
            );
        }
    }
}

#[test]
fn actual_in_lieu_validation_isolation_and_stale_preparation_save() {
    let (_dir, app, schedule, mut screen) = two_pa_preparation();
    screen.request_pa(Some(1));
    let record = screen.weeks[0].0.clone();
    let repo = &app.payroll_timesheet_repository;
    assert_eq!(record.actual_in_lieu_hours, None);
    for value in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(repo
            .save_actual_in_lieu_hours(record.id, Some(value))
            .is_err());
    }
    assert!(repo.save_actual_in_lieu_hours(-1, Some(1.0)).is_err());
    for text in ["NaN", "inf", "-0.1", "not a number"] {
        assert!(parse_actual_in_lieu(text).is_err());
    }
    assert_eq!(parse_actual_in_lieu(" ").unwrap(), None);
    assert_eq!(parse_actual_in_lieu("0.00").unwrap(), Some(0.0));
    assert_eq!(parse_actual_in_lieu("8.00").unwrap(), Some(8.0));
    repo.save_actual_in_lieu_hours(record.id, Some(8.0))
        .unwrap();
    screen.weeks[0].0.payroll_department_notes = "Saved from an older preparation draft".into();
    screen.save_current(&app).unwrap();
    let stored = repo
        .get_for_cycle_and_pa(
            &record.payroll_year,
            record.cycle_number,
            record.personal_assistant_id,
        )
        .unwrap()
        .unwrap();
    assert_eq!(stored.actual_in_lieu_hours, Some(8.0));
    assert_eq!(
        repo.get_for_cycle_and_pa(&record.payroll_year, record.cycle_number, 2)
            .unwrap()
            .unwrap()
            .actual_in_lieu_hours,
        None
    );
    let other = insert_schedule(
        &app,
        &record.payroll_year,
        schedule.cycle_number + 1,
        "05/10/2026",
        "30/10/2026",
    );
    screen.reload();
    screen.load(&app, &other, "other").unwrap();
    assert!(
        screen
            .weeks
            .iter()
            .all(|(r, _, _)| r.actual_in_lieu_hours.is_none()
                && r.payroll_department_notes.is_empty())
    );
}

#[test]
fn payroll_notes_schema32_upgrade_is_atomic_preserves_history_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("upgrade.sqlite");
    let db = Connection::open(&path).unwrap();
    crate::database::create_schema(&db).unwrap();
    crate::database::tests::remove_schema_32_fixture(&db);
    db.execute_batch("UPDATE schema_version SET version=31;
        INSERT INTO payroll_timesheets(id,personal_assistant_id,payroll_year,cycle_number,previous_cycle_hours,created_at,updated_at) VALUES (1,1,'2026/27',1,3,'created','updated');
        INSERT INTO payroll_submissions(id,payroll_timesheet_id,submitted_at,pdf_path,pdf_sha256,pdf_bytes) VALUES (1,1,'submitted','historical.pdf','digest',X'1234');
        CREATE TRIGGER refuse_version BEFORE UPDATE ON schema_version BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
    assert!(crate::database::create_schema(&db).is_err());
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap(),
        31
    );
    assert!(db
        .prepare("SELECT payroll_department_notes FROM payroll_timesheets")
        .is_err());
    assert!(db
        .prepare("SELECT payroll_department_notes FROM payroll_submissions")
        .is_err());
    db.execute_batch("DROP TRIGGER refuse_version").unwrap();
    crate::database::create_schema(&db).unwrap();
    crate::database::create_schema(&db).unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap(),
        32
    );
    let values: (String,Option<f64>,Option<String>,f64,String,String) = db.query_row("SELECT payroll_department_notes,actual_in_lieu_hours,actual_in_lieu_updated_at,previous_cycle_hours,created_at,updated_at FROM payroll_timesheets",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert_eq!(
        values,
        (
            String::new(),
            None,
            None,
            3.0,
            "created".into(),
            "updated".into()
        )
    );
    let historical: (Option<String>, Vec<u8>, String) = db
        .query_row(
            "SELECT payroll_department_notes,pdf_bytes,submitted_at FROM payroll_submissions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(historical, (None, vec![0x12, 0x34], "submitted".into()));
    drop(db);
    let reopened = Connection::open(path).unwrap();
    crate::database::create_schema(&reopened).unwrap();
    assert_eq!(
        reopened
            .query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap(),
        32
    );
}

#[test]
fn payroll_notes_ui_placement_limit_and_returned_display_are_independent() {
    let (_dir, app, schedule, mut screen) = load_active_record();
    let record = screen.weeks[0].0.clone();
    app.payroll_timesheet_repository
        .save_actual_in_lieu_hours(record.id, Some(8.0))
        .unwrap();
    screen.reload();
    screen.load(&app, &schedule, "period").unwrap();
    screen.loaded = true;
    screen.weeks[0].0.payroll_department_notes = "x".repeat(257);
    let ctx = egui::Context::default();
    let mut labels = Vec::new();
    for _ in 0..2 {
        labels = painted_labels(&ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 2000.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| screen.show(ui, &app, &schedule, "period"));
            },
        ));
    }
    let y = |text: &str| labels.iter().find(|(t, _)| t == text).unwrap().1.y;
    assert!(y("Worked") < y("Notes for Payroll Department"));
    assert!(y("Notes for Payroll Department") < y(&format!("Save {}", screen.weeks[0].2)));
    assert!(labels.iter().any(|(t, _)| t == "257 / 256 characters"));
    assert!(labels
        .iter()
        .any(|(t, _)| t.contains("Input has not been truncated")));
    assert_eq!(screen.returned_hours_text.get(&record.id).unwrap(), "8.00");
    assert!(screen.save_current(&app).is_err());
    assert_eq!(stored_note(&app, &record), "");
    assert_eq!(
        screen.weeks[0].0.payroll_department_notes.chars().count(),
        257
    );
}

#[test]
fn payroll_notes_unchanged_nonblank_note_keeps_both_pa_candidates() {
    let (_dir, app, _, mut screen) = two_pa_preparation();
    screen.request_pa(Some(1));
    screen.weeks[0].0.payroll_department_notes = "Saved note".into();
    screen.save_current(&app).unwrap();
    create_candidate(&app, &screen);
    screen.weeks.swap(0, 1);
    create_candidate(&app, &screen);
    screen.weeks.swap(0, 1);
    let metadata = |id| {
        app.payroll_worked_item_repository
            .snapshot_metadata(id)
            .unwrap()
    };
    let first = metadata(screen.weeks[0].0.id);
    let second = metadata(screen.weeks[1].0.id);
    screen.save_current(&app).unwrap();
    assert_eq!(metadata(screen.weeks[0].0.id), first);
    assert_eq!(metadata(screen.weeks[1].0.id), second);
    screen.weeks[0].0.payroll_department_notes = "Saved note\nNew line".into();
    screen.save_current(&app).unwrap();
    assert!(metadata(screen.weeks[0].0.id).is_none());
    assert_eq!(metadata(screen.weeks[1].0.id), second);
}
