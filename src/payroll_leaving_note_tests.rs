// Included in preparation tests: disposable databases and the real load/save paths.
fn leaving_setup(
    date: Option<&str>,
    note: &str,
) -> (
    TempDir,
    Application,
    PayrollSchedule,
    PayrollTimesheetScreen,
) {
    let (dir, app, schedule, mut screen) = load_active_record();
    let db = setup_connection(&app);
    db.execute(
        "UPDATE personal_assistants SET leaving_date=?1 WHERE id=1",
        [date],
    )
    .unwrap();
    db.execute(
        "UPDATE payroll_timesheets SET payroll_department_notes=?1",
        [note],
    )
    .unwrap();
    screen.reload();
    screen.load(&app, &schedule, "period").unwrap();
    (dir, app, schedule, screen)
}
fn leaving_text(date: &str) -> String {
    format!("Leaving date {date}. Please include any hours in lieu in final pay.")
}

#[test]
fn leaving_note_inclusive_boundaries_uk_format_and_blank_variants_are_drafts() {
    for (date, uk) in [
        ("10/08/2026", "10/08/2026"),
        ("06/09/2026", "06/09/2026"),
        ("2026-08-23", "23/08/2026"),
    ] {
        for blank in ["", " \n\t "] {
            let (_dir, app, _, screen) = leaving_setup(Some(date), blank);
            let record = &screen.weeks[0].0;
            assert_eq!(record.payroll_department_notes, leaving_text(uk));
            assert!(screen.has_unsaved_changes());
            assert_eq!(stored_note(&app, record), blank);
            assert_eq!(
                screen.preparation_baselines[&record.id].payroll_department_notes,
                blank
            );
        }
    }
}

#[test]
fn leaving_note_outside_period_absent_or_nonblank_is_unchanged() {
    for date in [None, Some("09/08/2026"), Some("07/09/2026")] {
        let (_dir, app, _, screen) = leaving_setup(date, "");
        assert_eq!(screen.weeks[0].0.payroll_department_notes, "");
        assert_eq!(stored_note(&app, &screen.weeks[0].0), "");
        assert!(!screen.has_unsaved_changes());
    }
    let saved = "  Employer's own note\nKeep exactly.  ";
    let (_dir, app, schedule, mut screen) = leaving_setup(Some("20/08/2026"), saved);
    assert_eq!(screen.weeks[0].0.payroll_department_notes, saved);
    assert!(!screen.has_unsaved_changes());
    setup_connection(&app)
        .execute(
            "UPDATE personal_assistants SET leaving_date='21/08/2026' WHERE id=1",
            [],
        )
        .unwrap();
    screen.reload();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.weeks[0].0.payroll_department_notes, saved);
    assert!(!screen.has_unsaved_changes());
}

#[test]
fn leaving_note_can_be_edited_deleted_and_saved_without_reappearing() {
    for edited in ["", "Employer replacement\nText"] {
        let (_dir, app, schedule, mut screen) = leaving_setup(Some("20/08/2026"), "");
        screen.weeks[0].0.payroll_department_notes = edited.into();
        screen.activate_pa(Some(1));
        assert_eq!(screen.weeks[0].0.payroll_department_notes, edited);
        screen.save_current(&app).unwrap();
        assert_eq!(stored_note(&app, &screen.weeks[0].0), edited);
        assert!(!screen.has_unsaved_changes());
        screen.reload();
        screen.load(&app, &schedule, "period").unwrap();
        assert_eq!(
            screen.weeks[0].0.payroll_department_notes,
            if edited.is_empty() {
                leaving_text("20/08/2026")
            } else {
                edited.into()
            }
        );
    }
}

#[test]
fn leaving_note_navigation_save_discard_cancel_preserve_pa_isolation() {
    for choice in [
        UnsavedChoice::Save,
        UnsavedChoice::Discard,
        UnsavedChoice::Cancel,
    ] {
        let (_dir, app, schedule, mut screen) = leaving_setup(Some("20/08/2026"), "");
        insert_pa(&app, 2, "Second", Some("Active"));
        setup_connection(&app)
            .execute(
                "UPDATE personal_assistants SET leaving_date='20/08/2026' WHERE id=2",
                [],
            )
            .unwrap();
        screen.reload();
        screen.load(&app, &schedule, "period").unwrap();
        screen.request_pa(Some(1));
        assert_eq!(screen.selected_pa, Some(1));
        assert_eq!(
            screen
                .weeks
                .iter()
                .find(|(r, _, _)| r.personal_assistant_id == 2)
                .unwrap()
                .0
                .payroll_department_notes,
            ""
        );
        assert!(screen.has_unsaved_changes());
        let record = screen
            .weeks
            .iter()
            .find(|(r, _, _)| r.personal_assistant_id == 1)
            .unwrap()
            .0
            .clone();
        screen.request_pa(Some(2));
        assert_eq!(screen.selected_pa, Some(1));
        assert_eq!(screen.pending_pa, Some(2));
        let proceed = screen.resolve_unsaved(&app, choice).unwrap();
        screen.finish_pa_switch(proceed);
        match choice {
            UnsavedChoice::Save => {
                assert_eq!(stored_note(&app, &record), leaving_text("20/08/2026"));
                assert_eq!(screen.selected_pa, Some(2));
            }
            UnsavedChoice::Discard => {
                assert_eq!(stored_note(&app, &record), "");
                assert_eq!(screen.selected_pa, Some(2));
                screen.reload();
                screen.load(&app, &schedule, "period").unwrap();
                screen.request_pa(Some(1));
                assert!(screen.has_unsaved_changes());
            }
            UnsavedChoice::Cancel => {
                assert_eq!(stored_note(&app, &record), "");
                assert_eq!(screen.selected_pa, Some(1));
                assert!(screen.has_unsaved_changes());
            }
        }
        assert_eq!(
            app.payroll_timesheet_repository
                .get_for_cycle_and_pa(&schedule.payroll_year, schedule.cycle_number, 2)
                .unwrap()
                .unwrap()
                .payroll_department_notes,
            ""
        );
    }
}

#[test]
fn leaving_note_save_invalidates_candidate_and_survives_reopen_without_touching_returned_hours() {
    let (_dir, app, schedule, mut screen) = load_active_record();
    create_candidate(&app, &screen);
    let id = screen.weeks[0].0.id;
    app.payroll_timesheet_repository
        .save_actual_in_lieu_hours(id, Some(8.125))
        .unwrap();
    setup_connection(&app)
        .execute(
            "UPDATE personal_assistants SET leaving_date='2026-08-20' WHERE id=1",
            [],
        )
        .unwrap();
    screen.reload();
    screen.load(&app, &schedule, "period").unwrap();
    assert!(screen.has_unsaved_changes());
    assert!(app
        .payroll_worked_item_repository
        .snapshot_metadata(id)
        .unwrap()
        .is_some());
    assert_eq!(stored_note(&app, &screen.weeks[0].0), "");
    screen.save_current(&app).unwrap();
    assert!(app
        .payroll_worked_item_repository
        .snapshot_metadata(id)
        .unwrap()
        .is_none());
    let reopened = crate::payroll_timesheet_repository::PayrollTimesheetRepository::new(
        setup_connection(&app),
    );
    let saved = reopened
        .get_for_cycle_and_pa(&schedule.payroll_year, schedule.cycle_number, 1)
        .unwrap()
        .unwrap();
    assert_eq!(saved.payroll_department_notes, leaving_text("20/08/2026"));
    assert_eq!(saved.actual_in_lieu_hours, Some(8.125));
    screen.reload();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(
        screen.weeks[0].0.payroll_department_notes,
        saved.payroll_department_notes
    );
    assert!(!screen.has_unsaved_changes());
}

#[test]
fn leaving_note_other_period_and_protected_preparations_remain_unchanged() {
    let (_dir, app, _, mut screen) = leaving_setup(Some("20/08/2026"), "");
    screen
        .resolve_unsaved(&app, UnsavedChoice::Discard)
        .unwrap();
    let second = insert_schedule(&app, "2026/27", 7, "07/09/2026", "02/10/2026");
    // Retained preparation remains eligible even though employment no longer overlaps.
    setup_connection(&app).execute("INSERT INTO payroll_timesheets (personal_assistant_id,payroll_year,cycle_number,created_at,updated_at) VALUES (1,'2026/27',7,'created','created')", []).unwrap();
    screen.reload();
    screen.load(&app, &second, "second").unwrap();
    assert_eq!(screen.weeks[0].0.payroll_department_notes, "");
    assert!(!screen.has_unsaved_changes());
    for state in ["submitted", "indeterminate"] {
        let (_dir, app, schedule, mut screen) = load_active_record();
        create_candidate(&app, &screen);
        let conn = setup_connection(&app);
        conn.execute(
            "UPDATE payroll_timesheet_snapshot_states SET state=?1",
            [state],
        )
        .unwrap();
        conn.execute(
            "UPDATE personal_assistants SET leaving_date='20/08/2026' WHERE id=1",
            [],
        )
        .unwrap();
        screen.reload();
        screen.load(&app, &schedule, "period").unwrap();
        assert_eq!(screen.weeks[0].0.payroll_department_notes, "");
        assert!(!screen.has_unsaved_changes());
    }
}
