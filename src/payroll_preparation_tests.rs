// Focused presentation/navigation regressions. Included in the screen test module
// so the fixtures exercise the same load/save path used by the desktop UI.
fn two_pa_preparation() -> (
    TempDir,
    Application,
    PayrollSchedule,
    PayrollTimesheetScreen,
) {
    let (dir, app, schedule, mut screen) = load_active_record();
    insert_pa(&app, 2, "Longest", Some("Active"));
    setup_connection(&app)
        .execute_batch(
            "UPDATE personal_assistants SET start_date='01/01/2025' WHERE id=1;
        UPDATE personal_assistants SET start_date='01/01/2020' WHERE id=2;",
        )
        .unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    (dir, app, schedule, screen)
}

#[test]
fn default_selection_uses_earliest_employment_date_and_normal_order_for_ties() {
    let (_dir, app, _, screen) = two_pa_preparation();
    assert_eq!(screen.selected_pa, Some(2));
    let mut assistants = app.personal_assistant_repository.get_all().unwrap();
    assistants[0].start_date = Some("2020-01-01".into());
    assert_eq!(default_pa(&assistants, &[(1, false), (2, false)]), Some(1));
    assistants[0].start_date = None;
    assert_eq!(default_pa(&assistants, &[(1, false), (2, false)]), Some(2));
    assert_eq!(default_pa(&assistants, &[]), None);
}

#[test]
fn unresolved_priority_uses_normal_pa_order_before_length_of_service() {
    let (_dir, app, _, _) = two_pa_preparation();
    let assistants = app.personal_assistant_repository.get_all().unwrap();
    assert_eq!(default_pa(&assistants, &[(1, true), (2, true)]), Some(1));
    assert_eq!(default_pa(&assistants, &[(1, true), (2, false)]), Some(1));
    assert_eq!(default_pa(&assistants, &[(1, false), (2, true)]), Some(2));
}

#[test]
fn unchanged_pa_switch_is_immediate_and_retains_loaded_records() {
    let (_dir, _app, _, mut screen) = two_pa_preparation();
    let ids = screen.records.iter().map(|r| r.id).collect::<Vec<_>>();
    assert!(!screen.has_unsaved_changes());
    screen.request_pa(Some(1));
    assert_eq!(screen.selected_pa, Some(1));
    assert!(screen.pending_pa.is_none());
    assert_eq!(screen.records.iter().map(|r| r.id).collect::<Vec<_>>(), ids);
}

#[test]
fn save_discard_cancel_pa_switches_use_current_pa_only() {
    for choice in [
        UnsavedChoice::Save,
        UnsavedChoice::Discard,
        UnsavedChoice::Cancel,
    ] {
        let (_dir, app, _, mut screen) = two_pa_preparation();
        screen.request_pa(Some(1));
        let id = screen.weeks[0].0.id;
        screen.weeks[0].1[0].worked_hours = 2.0;
        screen.request_pa(Some(2));
        assert_eq!(screen.selected_pa, Some(1));
        assert_eq!(screen.pending_pa, Some(2));
        let proceed = screen.resolve_unsaved(&app, choice).unwrap();
        screen.finish_pa_switch(proceed);
        let stored = app.payroll_timesheet_repository.get_weeks(id).unwrap();
        match choice {
            UnsavedChoice::Save => {
                assert_eq!(stored[0].worked_hours, 2.0);
                assert_eq!(screen.selected_pa, Some(2));
            }
            UnsavedChoice::Discard => {
                assert_eq!(stored[0].worked_hours, 0.0);
                assert_eq!(screen.weeks[0].1[0].worked_hours, 0.0);
                assert_eq!(screen.selected_pa, Some(2));
            }
            UnsavedChoice::Cancel => {
                assert_eq!(stored[0].worked_hours, 0.0);
                assert_eq!(screen.weeks[0].1[0].worked_hours, 2.0);
                assert_eq!(screen.selected_pa, Some(1));
                assert!(screen.has_unsaved_changes());
            }
        }
        assert_eq!(
            app.payroll_timesheet_repository
                .get_weeks(screen.weeks[1].0.id)
                .unwrap()[0]
                .worked_hours,
            0.0
        );
    }
}

#[test]
fn failed_navigation_save_keeps_current_pa_and_edits() {
    let (_dir, app, _, mut screen) = two_pa_preparation();
    screen.request_pa(Some(1));
    screen.public_holidays[0][0].hours = -1.0;
    screen.request_pa(Some(2));
    assert!(screen.resolve_unsaved(&app, UnsavedChoice::Save).is_err());
    assert_eq!(screen.selected_pa, Some(1));
    assert_eq!(screen.pending_pa, Some(2));
    assert!(screen.has_unsaved_changes());
}

#[test]
fn period_switch_save_discard_cancel_and_new_default() {
    for choice in [
        UnsavedChoice::Save,
        UnsavedChoice::Discard,
        UnsavedChoice::Cancel,
    ] {
        let (_dir, app, schedule, mut screen) = two_pa_preparation();
        let next = insert_schedule(&app, "2026/27", 7, "07/09/2026", "02/10/2026");
        screen.request_pa(Some(1));
        let id = screen.weeks[0].0.id;
        screen.weeks[0].1[0].travel_miles = 8.0;
        assert!(screen.rebind_if_operational_period_changed(&next));
        assert_eq!(
            screen.bound_period,
            Some(BoundPayrollPeriod::from(&schedule))
        );
        if screen.resolve_unsaved(&app, choice).unwrap() {
            screen.rebind_if_operational_period_changed(&next);
            screen.load(&app, &next, "next").unwrap();
            assert_eq!(screen.bound_period, Some(BoundPayrollPeriod::from(&next)));
            assert_eq!(screen.selected_pa, Some(2));
        } else {
            assert_eq!(
                screen.bound_period,
                Some(BoundPayrollPeriod::from(&schedule))
            );
            assert!(screen.has_unsaved_changes());
        }
        assert_eq!(
            app.payroll_timesheet_repository.get_weeks(id).unwrap()[0].travel_miles,
            if matches!(choice, UnsavedChoice::Save) {
                8.0
            } else {
                0.0
            }
        );
    }
}

#[test]
fn dirty_detection_covers_every_editable_value_and_reverting_to_baseline() {
    let (_dir, _app, _, mut screen) = load_active_record();
    for field in 0..5 {
        match field {
            0 => screen.weeks[0].1[0].worked_hours = 1.0,
            1 => screen.weeks[0].1[0].annual_leave_hours = 1.0,
            2 => screen.weeks[0].1[0].sick_leave_hours = 1.0,
            3 => screen.weeks[0].1[0].travel_miles = 1.0,
            _ => screen.public_holidays[0][0].hours = 1.0,
        }
        assert!(screen.has_unsaved_changes());
        screen.discard_current();
        assert!(!screen.has_unsaved_changes());
    }
    let id = screen.weeks[0].0.id;
    let mut row = new_annual_leave(&screen.weeks[0].1[0], 0.0);
    row.leave_date = "11/08/2026".into();
    screen.annual_leave.insert(id, vec![row]);
    assert!(screen.has_unsaved_changes());
    screen.discard_current();
    screen.weeks[0].1[0].worked_hours = 1.0;
    screen.weeks[0].1[0].worked_hours = 0.0;
    assert!(!screen.has_unsaved_changes());
}

#[test]
fn equivalent_numeric_text_does_not_prompt_but_uncommitted_blank_does() {
    let (_dir, app, _, mut screen) = load_active_record();
    screen.weeks[0].1[0].worked_hours = 2.0;
    screen.save_current(&app).unwrap();
    let key = NumericEditorKey::Worked(screen.weeks[0].1[0].id);
    screen.numeric_editor_texts.insert(key, "2.00".into());
    assert!(!screen.has_unsaved_changes());
    screen.numeric_editor_texts.insert(key, String::new());
    assert!(screen.has_unsaved_changes());
    screen.resolve_unsaved(&app, UnsavedChoice::Cancel).unwrap();
    assert_eq!(screen.numeric_editor_texts[&key], "");
    screen.discard_current();
    assert!(!screen.has_unsaved_changes());
    assert_eq!(screen.weeks[0].1[0].worked_hours, 2.0);
}

fn painted_text(output: egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::Shape, text: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => text.push(t.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, text);
                }
            }
            _ => {}
        }
    }
    let mut text = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut text);
    }
    text
}

fn render_preparation(
    ctx: &egui::Context,
    app: &Application,
    schedule: &PayrollSchedule,
    screen: &mut PayrollTimesheetScreen,
) -> Vec<String> {
    painted_text(ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 2000.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| screen.show(ui, app, schedule, "period"));
        },
    ))
}

#[test]
fn rendered_preparation_keeps_active_pa_first_and_visited_pa_below_without_empty_audit() {
    let (_dir, app, schedule, mut screen) = two_pa_preparation();
    screen.loaded = true;
    let ctx = egui::Context::default();
    render_preparation(&ctx, &app, &schedule, &mut screen);
    let text = render_preparation(&ctx, &app, &schedule, &mut screen).join("\n");
    assert!(text.contains("Save Longest Test"), "{text}");
    assert!(!text.contains("Save Active Test"));
    assert!(!text.contains("Payroll audit/details"));
    screen.request_pa(Some(1));
    let text = render_preparation(&ctx, &app, &schedule, &mut screen).join("\n");
    assert!(text.contains("Save Active Test"));
    assert!(!text.contains("Save Longest Test"));
    assert!(text.contains("Longest Test"));
    assert!(text.contains("Start editing Longest Test"));
    assert!(text.find("Save Active Test").unwrap() < text.find("Longest Test").unwrap());
    assert!(!text.contains("Payroll audit/details"));
}

#[test]
fn preparation_cached_frames_do_not_read_evidence_again() {
    let (_dir, app, schedule, mut screen) = two_pa_preparation();
    screen.loaded = true;
    screen.preflight_done = true;
    let ctx = egui::Context::default();
    screen.request_pa(Some(1));
    assert_eq!(screen.visited_pas, vec![1, 2]);
    // Any accidental evidence reload now fails; cached rendering must still work.
    setup_connection(&app)
        .execute_batch("ALTER TABLE timesheets RENAME TO hidden_evidence;")
        .unwrap();
    let start = std::time::Instant::now();
    for _ in 0..30 {
        let text = render_preparation(&ctx, &app, &schedule, &mut screen).join("\n");
        assert!(!text.contains("Failed loading"));
    }
    eprintln!("30 cached preparation frames: {:?}", start.elapsed());
    assert!(screen.loaded);
}

fn duplicate_fixture(app: &Application, pa: i64, imported: i64, direct: i64) {
    setup_connection(app).execute("INSERT INTO personal_assistant_pay_rates(personal_assistant_id,effective_date,base_hourly_rate,employer_top_up_rate,created_at) VALUES (?1,'01/01/2020',12,0,'test')", [pa]).unwrap();
    setup_connection(app).execute("INSERT INTO timesheets(id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount) VALUES (?1,?2,'Fixture','2026-08-11T20:00','2026-08-11T20:34',0,34,12,6.8)", params![imported, pa]).unwrap();
    setup_connection(app).execute("INSERT INTO direct_shifts(id,personal_assistant_id,start_time,end_time,created_at,updated_at) VALUES (?1,?2,'2026-08-11T20:00','2026-08-11T20:34','test','test')", params![direct, pa]).unwrap();
}

#[test]
fn unresolved_duplicate_selects_pa_without_blocking_other_pas_or_rendering_their_sections() {
    let (_dir, app, schedule, mut screen) = two_pa_preparation();
    duplicate_fixture(&app, 1, 149, 6);
    screen.duplicate_ui.refresh(&app, None).unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.selected_pa, Some(1));
    assert!(screen.blocked(screen.weeks[0].0.id, 1));
    screen.loaded = true;
    screen.preflight_done = true;
    let ctx = egui::Context::default();
    render_preparation(&ctx, &app, &schedule, &mut screen);
    let text = render_preparation(&ctx, &app, &schedule, &mut screen).join("\n");
    assert!(text.contains("Resolve possible duplicate shifts"));
    assert!(!text.contains("Save Active Test"));
    screen.request_pa(Some(2));
    let text = render_preparation(&ctx, &app, &schedule, &mut screen).join("\n");
    assert!(!text.contains("Resolve possible duplicate shifts"));
    assert!(text.contains("Save Longest Test"));
}

#[test]
fn resolved_catherine_and_invalidated_laura_audits_preserve_history_and_payable_one_point_five() {
    let (_dir, mut app, schedule, mut screen) = two_pa_preparation();
    setup_connection(&app)
        .execute_batch(
            "UPDATE personal_assistants SET first_name='Birch' WHERE id=1;
        UPDATE personal_assistants SET first_name='Robin',start_date='01/01/2024' WHERE id=2;",
        )
        .unwrap();
    insert_pa(&app, 3, "Longest", Some("Active"));
    setup_connection(&app)
        .execute(
            "UPDATE personal_assistants SET start_date='01/01/2010' WHERE id=3",
            [],
        )
        .unwrap();
    app.context.config.payroll.rounding_minutes = 15;
    app.context.config.payroll.rounding_direction = "Up".into();
    duplicate_fixture(&app, 1, 148, 5);
    duplicate_fixture(&app, 2, 149, 6);
    let evidence = crate::payroll_evidence::load(&app).unwrap();
    let groups = crate::payroll_evidence::groups(&evidence).unwrap();
    let choices = groups
        .iter()
        .map(|g| {
            (
                g.fingerprint.clone(),
                g.candidates
                    .iter()
                    .find(|e| e.source == "imported")
                    .unwrap()
                    .key(),
            )
        })
        .collect::<Vec<_>>();
    crate::payroll_evidence::resolve(&setup_connection(&app), &evidence, &choices).unwrap();
    setup_connection(&app).execute("UPDATE direct_shifts SET start_time='2026-08-11T21:00',end_time='2026-08-11T21:34' WHERE id=6", []).unwrap();
    screen.duplicate_ui.refresh(&app, None).unwrap();
    assert!(screen.duplicate_ui.pending.is_empty());
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.selected_pa, Some(3));
    assert_eq!(screen.dropdown_pas, vec![3, 2, 1]);
    let laura = screen
        .weeks
        .iter()
        .find(|(r, _, _)| r.personal_assistant_id == 2)
        .unwrap();
    assert_eq!(laura.1[0].worked_hours, 1.50);
    let before = crate::payroll_evidence::reconciliation::audit_lines(&app, &laura.0).unwrap();
    assert!(before
        .iter()
        .any(|l| l.contains("winner imported:149") && l.contains("direct:6")));
    let raw_before = crate::payroll_evidence::load(&app).unwrap();
    screen.loaded = true;
    screen.preflight_done = true;
    screen.request_pa(Some(2));
    let ctx = egui::Context::default();
    render_preparation(&ctx, &app, &schedule, &mut screen);
    let text = render_preparation(&ctx, &app, &schedule, &mut screen).join("\n");
    assert!(
        text.contains("Payroll audit/details — Robin Placeholder"),
        "{text}"
    );
    assert!(!text.contains("Payroll audit/details — PA"));
    assert!(!text.contains("Birch"));
    screen.request_pa(Some(1));
    screen.request_pa(Some(2));
    let text = render_preparation(&ctx, &app, &schedule, &mut screen).join("\n");
    assert!(text.contains("Payroll audit/details — Birch Sample"));
    assert!(
        text.find("Payroll audit/details — Robin Placeholder").unwrap()
            < text.find("Payroll audit/details — Birch Sample").unwrap()
    );
    assert!(!text.contains("Save Birch Sample"));

    screen.save_current(&app).unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    let laura = screen
        .weeks
        .iter()
        .find(|(r, _, _)| r.personal_assistant_id == 2)
        .unwrap();
    assert_eq!(laura.1[0].worked_hours, 1.50);
    assert_eq!(
        crate::payroll_evidence::reconciliation::audit_lines(&app, &laura.0).unwrap(),
        before
    );
    assert_eq!(
        raw_before
            .iter()
            .map(|e| (e.key(), e.fingerprint()))
            .collect::<Vec<_>>(),
        crate::payroll_evidence::load(&app)
            .unwrap()
            .iter()
            .map(|e| (e.key(), e.fingerprint()))
            .collect::<Vec<_>>()
    );
    let db = setup_connection(&app);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM payroll_duplicate_decisions WHERE invalidated_at IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(db.query_row("SELECT COUNT(*) FROM payroll_duplicate_decisions WHERE winner_id=149 AND invalidated_at IS NOT NULL", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
}

pub(crate) fn dirty_preparation_fixture() -> (TempDir, Application, PayrollTimesheetScreen) {
    let (dir, app, _, mut screen) = two_pa_preparation();
    screen.request_pa(Some(1));
    screen.weeks[0].1[0].worked_hours = 2.0;
    (dir, app, screen)
}

#[test]
fn historical_review_has_priority_but_completed_review_returns_to_longest_serving() {
    let (_dir, app, schedule, mut screen) = two_pa_preparation();
    let old = insert_schedule(&app, "2026/27", 1, "01/04/2026", "24/04/2026");
    let mut historical = PayrollTimesheetScreen::new();
    historical.load(&app, &old, "old").unwrap();
    app.payroll_timesheet_email_repository
        .mark_sent(1, "2026/27", 1, "payslip", "2026-04-30T10:00:00Z")
        .unwrap();
    setup_connection(&app).execute_batch("INSERT INTO timesheets(id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount) VALUES (1,1,'Active Test','2026-04-02T09:00','2026-04-02T10:00',0,60,12,12);").unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.selected_pa, Some(1));
    let record = screen.weeks[0].0.id;
    assert!(screen.review_ui.blocks_preparation(record));
    let review = screen
        .review_ui
        .preparation_plan(record)
        .unwrap()
        .historical_reviews[0]
        .clone();
    crate::payroll_evidence::reconciliation::historical_decision(&app, &review, true).unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.selected_pa, Some(2));
    assert!(!screen.review_ui.unresolved(record));
    assert_eq!(
        app.payroll_timesheet_repository
            .get_weeks(review.record.id)
            .unwrap()[0]
            .worked_hours,
        0.0
    );
    assert!(
        crate::payroll_evidence::reconciliation::audit_lines(&app, &review.record)
            .unwrap()
            .iter()
            .any(|l| l.contains("already_paid"))
    );
}

#[test]
fn submitted_discrepancy_is_prioritised_and_remains_read_only() {
    let (_dir, app, schedule, mut screen) = two_pa_preparation();
    app.payroll_timesheet_email_repository
        .mark_sent(1, "2026/27", 6, "timesheet", "2026-09-04T10:00:00Z")
        .unwrap();
    setup_connection(&app).execute_batch("INSERT INTO timesheets(id,personal_assistant_id,pa_name,start_time,end_time,break_minutes,worked_minutes,hourly_rate,amount) VALUES (1,1,'Active Test','2026-08-11T09:00','2026-08-11T10:00',0,60,12,12);").unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.selected_pa, Some(1));
    assert!(screen.review_ui.unresolved(screen.weeks[0].0.id));
    assert_eq!(
        screen.snapshot_states[&screen.weeks[0].0.id],
        SnapshotState::Submitted
    );
    assert_eq!(screen.weeks[0].1[0].worked_hours, 0.0);
}

#[test]
fn unsaved_dialog_renders_exactly_the_three_requested_choices() {
    let (_dir, app, _, mut screen) = load_active_record();
    screen.weeks[0].1[0].worked_hours = 1.0;
    let ctx = egui::Context::default();
    let mut text = Vec::new();
    for _ in 0..2 {
        text = painted_text(ctx.run(Default::default(), |ctx| {
            assert!(screen.unsaved_dialog(ctx, &app).is_none());
        }));
    }
    for label in ["Save changes", "Discard changes", "Cancel"] {
        assert_eq!(
            text.iter().filter(|t| t.as_str() == label).count(),
            1,
            "{text:?}"
        );
    }
    assert!(!text
        .iter()
        .any(|t| t.contains("Loaded") || t.contains("and continue")));
    assert!(screen.has_unsaved_changes());
}

#[test]
fn numeric_display_precision_is_not_an_edit_but_invalid_draft_text_is() {
    let (_dir, _app, _, mut screen) = load_active_record();
    let id = screen.weeks[0].0.id;
    let key = NumericEditorKey::TravelMiles(screen.weeks[0].1[0].id);
    screen.weeks[0].1[0].travel_miles = 1.0 / 3.0;
    screen.preparation_baselines.get_mut(&id).unwrap().weeks[0].travel_miles = 1.0 / 3.0;
    screen.numeric_editor_texts.insert(key, "0.33".into());
    assert!(!screen.has_unsaved_changes());
    screen.numeric_editor_texts.insert(key, ".".into());
    assert!(screen.has_unsaved_changes());
}

fn three_pa_preparation() -> (
    TempDir,
    Application,
    PayrollSchedule,
    PayrollTimesheetScreen,
) {
    let (dir, app, schedule, mut screen) = two_pa_preparation();
    insert_pa(&app, 3, "Middle", Some("Active"));
    setup_connection(&app)
        .execute(
            "UPDATE personal_assistants SET start_date='01/01/2022' WHERE id=3",
            [],
        )
        .unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    (dir, app, schedule, screen)
}

#[test]
fn visits_are_retained_in_most_recent_order_and_reactivation_does_not_duplicate_them() {
    let (_dir, _app, _, mut screen) = three_pa_preparation();
    assert_eq!(screen.dropdown_pas, vec![2, 3, 1]);
    assert_eq!(screen.visited_pas, vec![2]);
    for (next, expected) in [
        (1, vec![1, 2]),
        (3, vec![3, 1, 2]),
        (2, vec![2, 3, 1]),
        (2, vec![2, 3, 1]),
    ] {
        screen.request_pa(Some(next));
        assert_eq!(screen.selected_pa, Some(next));
        assert_eq!(screen.visited_pas, expected);
        assert_eq!(screen.dropdown_pas, vec![2, 3, 1]);
    }
}

#[test]
fn dirty_switch_does_not_reorder_visits_until_save_or_discard_and_cancel_retains_order() {
    for choice in [
        UnsavedChoice::Save,
        UnsavedChoice::Discard,
        UnsavedChoice::Cancel,
    ] {
        let (_dir, app, _, mut screen) = three_pa_preparation();
        screen.request_pa(Some(1));
        screen.weeks[0].1[0].worked_hours = 2.0;
        screen.request_pa(Some(3));
        assert_eq!(screen.visited_pas, vec![1, 2]);
        let proceed = screen.resolve_unsaved(&app, choice).unwrap();
        screen.finish_pa_switch(proceed);
        assert_eq!(
            screen.visited_pas,
            if proceed { vec![3, 1, 2] } else { vec![1, 2] }
        );
        assert_eq!(screen.has_unsaved_changes(), !proceed);
        assert!(weeks_equal(
            &screen.weeks[1].1,
            &screen.preparation_baselines[&screen.weeks[1].0.id].weeks
        ));
        assert!(weeks_equal(
            &screen.weeks[2].1,
            &screen.preparation_baselines[&screen.weeks[2].0.id].weeks
        ));
    }
}

#[test]
fn failed_save_leaves_visit_order_and_active_pa_unchanged() {
    let (_dir, app, _, mut screen) = three_pa_preparation();
    screen.request_pa(Some(1));
    screen.public_holidays[0][0].hours = -1.0;
    screen.request_pa(Some(3));
    assert!(screen.resolve_unsaved(&app, UnsavedChoice::Save).is_err());
    assert_eq!(screen.selected_pa, Some(1));
    assert_eq!(screen.visited_pas, vec![1, 2]);
    assert_eq!(screen.pending_pa, Some(3));
}

#[test]
fn dropdown_orders_unresolved_in_normal_order_then_remaining_pas_by_start_date() {
    let (_dir, app, schedule, mut screen) = three_pa_preparation();
    insert_pa(&app, 4, "Another unresolved", Some("Active"));
    insert_pa(&app, 5, "Not yet employed", Some("Active"));
    setup_connection(&app)
        .execute_batch(
            "UPDATE personal_assistants SET start_date='01/01/2010' WHERE id=4;
        UPDATE personal_assistants SET start_date='01/01/2027' WHERE id=5;",
        )
        .unwrap();
    duplicate_fixture(&app, 1, 149, 6);
    duplicate_fixture(&app, 4, 150, 7);
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.dropdown_pas, vec![1, 4, 2, 3]);
    assert_eq!(screen.selected_pa, Some(1));
    assert_eq!(screen.visited_pas, vec![1]);
    let all = crate::payroll_evidence::load(&app).unwrap();
    let choices = crate::payroll_evidence::groups(&all)
        .unwrap()
        .into_iter()
        .map(|g| {
            let key = g
                .candidates
                .iter()
                .find(|e| e.source == "imported")
                .unwrap()
                .key();
            (g.fingerprint, key)
        })
        .collect::<Vec<_>>();
    crate::payroll_evidence::resolve(&setup_connection(&app), &all, &choices).unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.dropdown_pas, vec![4, 2, 3, 1]);
}

#[test]
fn period_switch_resets_visits_only_after_save_or_discard() {
    for choice in [
        UnsavedChoice::Save,
        UnsavedChoice::Discard,
        UnsavedChoice::Cancel,
    ] {
        let (_dir, app, schedule, mut screen) = three_pa_preparation();
        let next = insert_schedule(&app, "2026/27", 7, "07/09/2026", "02/10/2026");
        screen.request_pa(Some(1));
        screen.request_pa(Some(3));
        screen.weeks[2].1[0].travel_miles = 9.0;
        screen.rebind_if_operational_period_changed(&next);
        assert_eq!(screen.visited_pas, vec![3, 1, 2]);
        if screen.resolve_unsaved(&app, choice).unwrap() {
            screen.rebind_if_operational_period_changed(&next);
            assert!(screen.visited_pas.is_empty());
            screen.load(&app, &next, "next").unwrap();
            assert_eq!(screen.visited_pas, vec![2]);
            assert_eq!(screen.dropdown_pas, vec![2, 3, 1]);
        } else {
            assert_eq!(screen.visited_pas, vec![3, 1, 2]);
            assert_eq!(
                screen.bound_period,
                Some(BoundPayrollPeriod::from(&schedule))
            );
        }
    }
}

#[test]
fn evidence_refresh_retains_session_visits_but_leaving_screen_starts_a_fresh_session() {
    let (_dir, app, schedule, mut screen) = three_pa_preparation();
    screen.request_pa(Some(1));
    screen.request_pa(Some(3));
    screen.reload_evidence_preserving_visits();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.visited_pas, vec![3, 1, 2]);
    assert_eq!(screen.selected_pa, Some(3));
    screen.reload();
    screen.load(&app, &schedule, "period").unwrap();
    assert_eq!(screen.visited_pas, vec![2]);
}

#[test]
fn lower_sections_do_not_create_editors_or_mutate_persisted_annual_leave() {
    let (_dir, app, schedule, mut screen) = three_pa_preparation();
    // Retained undated annual leave must not acquire a draft date row simply
    // because its inactive section is visible while another PA is being edited.
    let record = screen.weeks[0].0.id;
    setup_connection(&app).execute("UPDATE payroll_timesheet_weeks SET annual_leave_hours=1.5 WHERE payroll_timesheet_id=?1 AND week_number=1", [record]).unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    screen.request_pa(Some(1));
    screen.request_pa(Some(3));
    screen.loaded = true;
    screen.preflight_done = true;
    screen.weeks[2].1[0].worked_hours = 2.0;
    let ctx = egui::Context::default();
    for _ in 0..3 {
        render_preparation(&ctx, &app, &schedule, &mut screen);
    }
    assert!(screen.has_unsaved_changes());
    assert!(screen.annual_leave[&record].is_empty());
    for index in [0, 1] {
        assert!(weeks_equal(
            &screen.weeks[index].1,
            &screen.preparation_baselines[&screen.weeks[index].0.id].weeks
        ));
        for week in &screen.weeks[index].1 {
            for key in [
                NumericEditorKey::Worked(week.id),
                NumericEditorKey::AnnualLeave(week.id),
                NumericEditorKey::SickLeave(week.id),
                NumericEditorKey::TravelMiles(week.id),
            ] {
                assert!(!screen.numeric_editor_texts.contains_key(&key));
            }
        }
    }
}

fn painted_labels(output: &egui::FullOutput) -> Vec<(String, egui::Pos2)> {
    fn collect(shape: &egui::Shape, labels: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(t) => labels.push((t.galley.job.text.clone(), t.pos)),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, labels);
                }
            }
            _ => {}
        }
    }
    let mut labels = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut labels);
    }
    labels
}

#[test]
fn compact_modal_places_buttons_on_one_row_and_shows_only_save_errors() {
    let (_dir, app, _, mut screen) = load_active_record();
    screen.status_message = "Loaded 5 Payroll Timesheets.".into();
    let ctx = egui::Context::default();
    for error in [
        None,
        Some("Save failed: invalid annual leave date".to_string()),
    ] {
        screen.unsaved_error = error.clone();
        let mut labels = Vec::new();
        for _ in 0..2 {
            labels = painted_labels(&ctx.run(Default::default(), |ctx| {
                screen.unsaved_dialog(ctx, &app);
            }));
        }
        let buttons = ["Save changes", "Discard changes", "Cancel"]
            .map(|name| labels.iter().find(|(text, _)| text == name).unwrap().1);
        assert!((buttons[0].y - buttons[1].y).abs() < 1.0);
        assert!((buttons[0].y - buttons[2].y).abs() < 1.0);
        assert!(buttons[0].x < buttons[1].x && buttons[1].x < buttons[2].x);
        assert!(!labels
            .iter()
            .any(|(text, _)| text.contains("Loaded") || text.contains("and continue")));
        if let Some(error) = error {
            assert!(labels.iter().any(|(text, _)| text == &error));
        }
    }
}

#[test]
fn start_editing_button_uses_the_existing_guard_before_moving_a_visited_section() {
    for choice in [
        None,
        Some(UnsavedChoice::Save),
        Some(UnsavedChoice::Discard),
        Some(UnsavedChoice::Cancel),
    ] {
        let (_dir, app, schedule, mut screen) = two_pa_preparation();
        screen.loaded = true;
        screen.preflight_done = true;
        screen.request_pa(Some(1));
        if choice.is_some() {
            screen.weeks[0].1[0].worked_hours = 2.0;
        }
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
        let pos = labels
            .iter()
            .find(|(label, _)| label == "Start editing Longest Test")
            .unwrap()
            .1
            + egui::vec2(8.0, 6.0);
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 2000.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| screen.show(ui, &app, &schedule, "period"));
                },
            );
        }
        if let Some(choice) = choice {
            assert_eq!(screen.selected_pa, Some(1));
            assert_eq!(screen.visited_pas, vec![1, 2]);
            assert_eq!(screen.pending_pa, Some(2));
            let proceed = screen.resolve_unsaved(&app, choice).unwrap();
            screen.finish_pa_switch(proceed);
            assert_eq!(
                screen.visited_pas,
                if proceed { vec![2, 1] } else { vec![1, 2] }
            );
        } else {
            assert_eq!(screen.selected_pa, Some(2));
            assert_eq!(screen.visited_pas, vec![2, 1]);
            assert!(screen.pending_pa.is_none());
        }
    }
}

#[test]
fn period_change_during_pending_evidence_refresh_does_not_restore_old_visits() {
    let (_dir, app, _, mut screen) = three_pa_preparation();
    let next = insert_schedule(&app, "2026/27", 7, "07/09/2026", "02/10/2026");
    screen.request_pa(Some(1));
    screen.request_pa(Some(3));
    screen.reload_evidence_preserving_visits();
    screen.load(&app, &next, "next").unwrap();
    assert_eq!(screen.visited_pas, vec![2]);
    assert_eq!(screen.selected_pa, Some(2));
}

#[test]
fn active_panel_uses_a_subtle_distinct_fill_and_border_in_every_theme() {
    for theme in [
        ApplicationTheme::System,
        ApplicationTheme::Light,
        ApplicationTheme::SoftLight,
        ApplicationTheme::Dark,
        ApplicationTheme::SoftDark,
        ApplicationTheme::Blue,
        ApplicationTheme::AccessibleHighContrast,
    ] {
        let ctx = egui::Context::default();
        crate::theme::apply_theme(&ctx, theme);
        let visuals = &ctx.style().visuals;
        let panel = active_panel_style(visuals);
        assert_ne!(panel.fill, visuals.panel_fill, "{theme:?}");
        assert_ne!(panel.fill, visuals.text_color(), "{theme:?}");
        assert!(panel.stroke.width > 0.0);
        assert_ne!(panel.stroke.color, panel.fill);
        let shadow = active_panel_shadow(visuals);
        assert_eq!(shadow.offset, [0, 2]);
        assert_eq!(shadow.blur, 4);
        assert_eq!(shadow.spread, 0);
        assert_eq!(shadow.color, visuals.window_shadow.color);
        // The tint stays close to the appearance's own background palette.
        for (fill, base) in panel.fill.to_array()[..3]
            .iter()
            .zip(&visuals.panel_fill.to_array()[..3])
        {
            assert!((*fill as i16 - *base as i16).abs() <= 26);
        }
    }
}

#[test]
fn whole_active_panel_contains_heading_audit_and_save_and_moves_with_selection() {
    let (_dir, app, schedule, mut screen) = two_pa_preparation();
    duplicate_fixture(&app, 1, 149, 6);
    let evidence = crate::payroll_evidence::load(&app).unwrap();
    let group = crate::payroll_evidence::groups(&evidence)
        .unwrap()
        .remove(0);
    crate::payroll_evidence::resolve(
        &setup_connection(&app),
        &evidence,
        &[(group.fingerprint, "imported:149".into())],
    )
    .unwrap();
    screen.load(&app, &schedule, "period").unwrap();
    screen.loaded = true;
    screen.preflight_done = true;
    let ctx = egui::Context::default();
    fn rectangles(shape: &egui::Shape, fill: egui::Color32, found: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Rect(rect) if rect.fill == fill => found.push(rect.rect),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    rectangles(shape, fill, found);
                }
            }
            _ => {}
        }
    }
    for (pa, active, inactive) in [
        (1, "Active Test", "Longest Test"),
        (2, "Longest Test", "Active Test"),
    ] {
        screen.request_pa(Some(pa));
        for _ in 0..2 {
            let output = ctx.run(
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
            );
            let mut panels = Vec::new();
            for shape in &output.shapes {
                rectangles(
                    &shape.shape,
                    active_panel_style(&ctx.style().visuals).fill,
                    &mut panels,
                );
            }
            assert_eq!(panels.len(), 1);
            let panel = panels[0];
            let labels = painted_labels(&output);
            let position = |text: &str| {
                labels
                    .iter()
                    .rev()
                    .find(|(label, _)| label == text)
                    .unwrap()
                    .1
            };
            assert!(!panel.contains(position("Personal Assistant")));
            let dropdown = labels.iter().find(|(label, _)| label == active).unwrap().1;
            assert!(!panel.contains(dropdown));
            assert!(dropdown.y < panel.top());
            for prefix in ["Payroll period:", "Four-week period:"] {
                let context = labels
                    .iter()
                    .find(|(label, _)| label.starts_with(prefix))
                    .unwrap()
                    .1;
                assert!(panel.contains(context));
                assert!(context.y < position(active).y);
            }
            assert!(panel.contains(position(active)));
            assert!(panel.contains(position(&format!("Save {active}"))));
            assert!(!panel.contains(position(inactive)));
            assert!(!panel.contains(position(&format!("Start editing {inactive}"))));
            assert_eq!(
                panel.contains(position("Payroll audit/details — Active Test")),
                pa == 1
            );
            assert!(!labels.iter().any(|(label, _)| label == "Make active"));
        }
    }
}

pub(crate) fn employment_period_fixture() -> (TempDir, Application, PayrollSchedule, PayrollSchedule)
{
    let (dir, mut app) = test_application();
    app.context.config.folders.pdf_output = dir.path().join("pdf");
    let historical = insert_schedule(&app, "2026/27", 6, "10/08/2026", "04/09/2026");
    let current = insert_schedule(&app, "2026/27", 7, "07/09/2026", "02/10/2026");
    let db = setup_connection(&app);
    db.execute(
        "INSERT INTO employers(name) VALUES ('Eligibility Employer')",
        [],
    )
    .unwrap();
    for (id, status, start, leaving) in [
        (1, "Inactive", "01/01/2020", Some("15/08/2026")),
        (2, "Active", "07/09/2026", None),
        (3, "Inactive", "06/09/2026", Some("06/09/2026")),
        (4, "Inactive", "07/09/2026", Some("14/09/2026")),
        (5, "Active", "20 August 2026", None),
        (6, "Inactive", "01/07/2026", Some("09/08/2026")),
        (7, "Inactive", "01/01/2027", Some("01/02/2027")),
        (8, "Active", "01/01/2020", Some("10/08/2026")),
    ] {
        insert_pa(&app, id, &format!("Employee{id}"), Some(status));
        db.execute(
            "UPDATE personal_assistants SET start_date=?1,leaving_date=?2 WHERE id=?3",
            params![start, leaving, id],
        )
        .unwrap();
    }
    (dir, app, historical, current)
}

pub(crate) fn load_period_for_eligibility_test(
    app: &Application,
    schedule: &PayrollSchedule,
) -> PayrollTimesheetScreen {
    let mut screen = PayrollTimesheetScreen::new();
    screen.load(app, schedule, "period").unwrap();
    screen
}

#[test]
fn preparation_dropdown_uses_inclusive_employment_for_historical_and_current_periods() {
    let (_dir, app, historical, current) = employment_period_fixture();
    for (schedule, expected) in [(&historical, vec![1, 3, 5, 8]), (&current, vec![2, 4, 5])] {
        let screen = load_period_for_eligibility_test(&app, schedule);
        let mut dropdown = screen.dropdown_pas.clone();
        dropdown.sort();
        assert_eq!(dropdown, expected);
        let mut preflight_scope =
            crate::payroll_evidence::duplicate_ui::preparation_pa_scope(&app, schedule)
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>();
        preflight_scope.sort();
        assert_eq!(preflight_scope, expected);
        let stored = app
            .payroll_timesheet_repository
            .get_all_for_cycle(&schedule.payroll_year, schedule.cycle_number)
            .unwrap();
        assert_eq!(
            stored
                .iter()
                .map(|r| r.personal_assistant_id)
                .collect::<std::collections::HashSet<_>>(),
            expected.into_iter().collect()
        );
    }
}

#[test]
fn existing_records_for_future_or_departed_pas_remain_visible_without_creating_earlier_records() {
    let (_dir, app, historical, current) = employment_period_fixture();
    let initial = load_period_for_eligibility_test(&app, &current);
    let ids = initial.records.iter().map(|r| r.id).collect::<Vec<_>>();
    setup_connection(&app).execute("UPDATE personal_assistants SET employment_status='Inactive',start_date='01/01/2027',leaving_date='01/02/2027' WHERE id=2", []).unwrap();
    let reloaded = load_period_for_eligibility_test(&app, &current);
    assert!(reloaded.dropdown_pas.contains(&2));
    assert_eq!(
        reloaded.records.iter().map(|r| r.id).collect::<Vec<_>>(),
        ids
    );
    let earlier = load_period_for_eligibility_test(&app, &historical);
    assert!(!earlier.dropdown_pas.contains(&2));
    assert!(!earlier.dropdown_pas.contains(&6));
    assert!(!earlier.dropdown_pas.contains(&7));
}

#[test]
fn sent_and_settled_preparations_are_not_rewritten_when_employment_no_longer_overlaps() {
    for kind in ["timesheet", "payslip"] {
        let (_dir, app, historical, _) = employment_period_fixture();
        let original = load_period_for_eligibility_test(&app, &historical);
        let record = original
            .records
            .iter()
            .find(|r| r.personal_assistant_id == 1)
            .unwrap();
        let db = setup_connection(&app);
        db.execute("UPDATE payroll_timesheet_weeks SET worked_hours=7.5,travel_miles=3.0 WHERE payroll_timesheet_id=?1 AND week_number=1", [record.id]).unwrap();
        app.payroll_timesheet_email_repository
            .mark_sent(
                1,
                &historical.payroll_year,
                historical.cycle_number,
                kind,
                "2026-09-04T10:00:00Z",
            )
            .unwrap();
        let before = app
            .payroll_timesheet_repository
            .get_weeks(record.id)
            .unwrap();
        let audit = crate::payroll_evidence::reconciliation::audit_lines(&app, record).unwrap();
        db.execute("UPDATE personal_assistants SET employment_status='Inactive',start_date='01/01/2027',leaving_date='01/02/2027' WHERE id=1", []).unwrap();
        let reloaded = load_period_for_eligibility_test(&app, &historical);
        assert!(reloaded.dropdown_pas.contains(&1));
        assert_eq!(
            reloaded.snapshot_states[&record.id],
            SnapshotState::Submitted
        );
        assert!(weeks_equal(
            &before,
            &app.payroll_timesheet_repository
                .get_weeks(record.id)
                .unwrap()
        ));
        assert_eq!(
            audit,
            crate::payroll_evidence::reconciliation::audit_lines(&app, record).unwrap()
        );
        assert_eq!(reloaded.records.len(), original.records.len());
    }
}
