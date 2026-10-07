fn editor_frame(
    ctx: &egui::Context,
    screen: &mut PersonalAssistantScreen,
    app: &Application,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Pos2)> {
    fn collect(shape: &egui::Shape, labels: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(text) => labels.push((text.galley.job.text.clone(), text.pos)),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, labels)),
            _ => {}
        }
    }
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 1800.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| screen.show(ui, app));
            });
        },
    );
    let mut labels = Vec::new();
    for shape in output.shapes {
        collect(&shape.shape, &mut labels);
    }
    labels
}

fn editor_click(
    ctx: &egui::Context,
    screen: &mut PersonalAssistantScreen,
    app: &Application,
    label: &str,
) {
    editor_frame(ctx, screen, app, vec![]);
    let labels = editor_frame(ctx, screen, app, vec![]);
    let pos = labels
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("Missing {label}: {labels:?}"))
        .1
        + egui::vec2(3.0, 3.0);
    for pressed in [true, false] {
        editor_frame(
            ctx,
            screen,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

// Use actual checkbox and Save events: repository/service-only tests bypass the editor.
fn status_fixture(
    status: &str,
) -> (
    tempfile::TempDir,
    Application,
    egui::Context,
    PersonalAssistantScreen,
) {
    let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
    rusqlite::Connection::open(&app.context.environment.database_path).unwrap().execute(
        "INSERT INTO personal_assistants(id, first_name, surname, employment_status, date_of_birth, start_date, email, telephone)
         VALUES(1, 'Example', 'Assistant', ?1, '1990-02-01', '2025-04-03', 'example@example.test', '01234567890')",
        [status],
    ).unwrap();
    let ctx = egui::Context::default();
    let mut screen = PersonalAssistantScreen::new();
    editor_click(&ctx, &mut screen, &app, "Example Assistant");
    (dir, app, ctx, screen)
}

fn save_editor(ctx: &egui::Context, screen: &mut PersonalAssistantScreen, app: &Application) {
    editor_click(ctx, screen, app, "Save Personal Assistant");
    assert!(
        screen
            .status_message
            .starts_with("Personal Assistant saved."),
        "{}",
        screen.status_message
    );
    editor_frame(ctx, screen, app, vec![]); // process the post-save list reload
}

fn assert_status_reloaded(app: &Application, expected: &str) -> PersonalAssistant {
    use crate::personal_assistant_repository::PersonalAssistantRepository;
    // Reopen persistent storage and run the same schema initialization used at startup.
    let db = rusqlite::Connection::open(&app.context.environment.database_path).unwrap();
    crate::database::create_schema(&db).unwrap();
    let raw: String = db
        .query_row(
            "SELECT employment_status FROM personal_assistants WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(raw, expected);
    let loaded = PersonalAssistantRepository::get_by_id_on(&db, 1).unwrap();
    assert_eq!(loaded.employment_status.as_deref(), Some(expected));
    let repo = PersonalAssistantRepository::new(db);
    assert_eq!(
        repo.get_all().unwrap()[0].employment_status.as_deref(),
        Some(expected)
    );
    assert_eq!(
        repo.get_active().unwrap().len(),
        usize::from(expected == "Active")
    );
    loaded
}

fn reopen_editor(app: &Application, expected: &str) -> (egui::Context, PersonalAssistantScreen) {
    let ctx = egui::Context::default();
    let mut screen = PersonalAssistantScreen::new();
    let pa = app
        .personal_assistant_repository
        .get_all()
        .unwrap()
        .remove(0);
    editor_click(
        &ctx,
        &mut screen,
        app,
        &format!("{} {}", pa.first_name, pa.surname),
    );
    assert_eq!(
        screen
            .editing_assistant
            .as_ref()
            .unwrap()
            .employment_status
            .as_deref(),
        Some(expected)
    );
    (ctx, screen)
}

#[test]
fn status_fixture_uses_canonical_payroll_roots() {
    let (dir, app, _ctx, _screen) = status_fixture("Active");
    let root = dir.path().canonicalize().unwrap();
    for (folder, name) in [
        (&app.context.config.folders.payslip_folder, "payslips"),
        (&app.context.config.folders.pdf_output, "pdfs"),
        (
            &app.context.config.folders.payroll_information_folder,
            "information",
        ),
    ] {
        assert_eq!(folder, &root.join(name));
        crate::payroll_archive_service::checked_path(folder).unwrap();
    }
}

#[test]
fn editor_active_to_inactive_persists_after_editor_and_database_reopen() {
    let (_dir, app, ctx, mut screen) = status_fixture("Active");
    editor_click(&ctx, &mut screen, &app, "Active");
    assert_eq!(
        screen
            .editing_assistant
            .as_ref()
            .unwrap()
            .employment_status
            .as_deref(),
        Some("Inactive")
    );
    save_editor(&ctx, &mut screen, &app);
    drop(screen);
    assert_status_reloaded(&app, "Inactive");
    let (ctx, mut screen) = reopen_editor(&app, "Inactive");
    // Clicking the reloaded unchecked checkbox must activate, not deactivate.
    editor_click(&ctx, &mut screen, &app, "Active");
    assert_eq!(
        screen
            .editing_assistant
            .as_ref()
            .unwrap()
            .employment_status
            .as_deref(),
        Some("Active")
    );
}

#[test]
fn editor_inactive_to_active_persists_after_editor_and_database_reopen() {
    let (_dir, app, ctx, mut screen) = status_fixture("Inactive");
    editor_click(&ctx, &mut screen, &app, "Active");
    save_editor(&ctx, &mut screen, &app);
    drop(screen);
    assert_status_reloaded(&app, "Active");
    reopen_editor(&app, "Active");
}

#[test]
fn editor_other_field_save_does_not_reactivate_inactive_pa() {
    for rename in [false, true] {
        let (_dir, app, ctx, mut screen) = status_fixture("Inactive");
        let draft = screen.editing_assistant.as_mut().unwrap();
        draft.telephone = Some("09876543210".into());
        if rename {
            draft.surname = "Renamed".into();
        }
        save_editor(&ctx, &mut screen, &app);
        drop(screen);
        let loaded = assert_status_reloaded(&app, "Inactive");
        assert_eq!(loaded.telephone.as_deref(), Some("09876543210"));
        assert_eq!(loaded.surname, if rename { "Renamed" } else { "Assistant" });
        reopen_editor(&app, "Inactive");
    }
}

#[test]
fn editor_inactive_transition_archives_documents_and_reactivation_preserves_history() {
    let (_dir, app, ctx, mut screen) = status_fixture("Active");
    let payslip_dir = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2026 to 2027");
    let timesheet_dir = app.context.config.folders.pdf_output.join("2026 to 2027");
    std::fs::create_dir_all(&payslip_dir).unwrap();
    std::fs::create_dir_all(&timesheet_dir).unwrap();
    let payslip = payslip_dir.join("Payslip for Week 26 for Example Assistant.pdf");
    let supplement = payslip_dir.join("P45 for year 2026-27 for Example Assistant.pdf");
    let timesheet = timesheet_dir.join("Timesheet - Example Assistant - 260907w04.pdf");
    for path in [&payslip, &supplement, &timesheet] {
        std::fs::write(path, b"%PDF-1.4 synthetic status regression fixture").unwrap();
    }
    app.payroll_timesheet_email_repository
        .register_document(1, "p45", &supplement, Some("2026/27"))
        .unwrap();
    let db = &app.payroll_timesheet_email_repository.connection;
    db.execute("UPDATE imported_payroll_documents SET history_state='application', sent_at='2026-09-29T20:23:33+01:00' WHERE personal_assistant_id=1", []).unwrap();
    let before = app
        .payroll_timesheet_email_repository
        .all_documents_for_pa(1)
        .unwrap()
        .remove(0);
    editor_click(&ctx, &mut screen, &app, "Active");
    save_editor(&ctx, &mut screen, &app);
    assert_status_reloaded(&app, "Inactive");
    // Exercise another edit on the saved inactive PA, then reactivation through the checkbox.
    let (ctx, mut screen) = reopen_editor(&app, "Inactive");
    screen.editing_assistant.as_mut().unwrap().telephone = Some("09876543210".into());
    save_editor(&ctx, &mut screen, &app);
    assert_status_reloaded(&app, "Inactive");
    for expected in ["Inactive", "Active"] {
        if expected == "Active" {
            editor_click(&ctx, &mut screen, &app, "Active");
            save_editor(&ctx, &mut screen, &app);
        }
        assert_status_reloaded(&app, expected);
        for source in [&payslip, &supplement, &timesheet] {
            let archived = source
                .parent()
                .unwrap()
                .join("Archived")
                .join(source.file_name().unwrap());
            assert!(!source.exists());
            assert_eq!(
                std::fs::read(archived).unwrap(),
                b"%PDF-1.4 synthetic status regression fixture"
            );
        }
        let after = app
            .payroll_timesheet_email_repository
            .all_documents_for_pa(1)
            .unwrap()
            .remove(0);
        assert_eq!(
            after.path,
            supplement
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .join("Archived")
                .join(supplement.file_name().unwrap())
        );
        assert_eq!(after.id, before.id);
        assert_eq!(after.sha256, before.sha256);
        assert_eq!(after.history_state, before.history_state);
        let sent: String = db
            .query_row(
                "SELECT sent_at FROM imported_payroll_documents WHERE id=?1",
                [before.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sent, "2026-09-29T20:23:33+01:00");
        assert_eq!(after.delivery_state, before.delivery_state);
        let pending: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM payroll_file_moves WHERE personal_assistant_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pending, 0);
    }
}

#[test]
fn editor_archive_conflict_reports_error_but_persists_inactive_and_edits() {
    let (_dir, app, ctx, mut screen) = status_fixture("Active");
    let root = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2026 to 2027");
    let source = root.join("Payslip for Week 26 for Example Assistant.pdf");
    let destination = root.join("Archived").join(source.file_name().unwrap());
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::fs::write(&source, b"%PDF-1.4 synthetic original").unwrap();
    std::fs::write(&destination, b"%PDF-1.4 synthetic conflicting archive").unwrap();
    screen.editing_assistant.as_mut().unwrap().telephone =
        Some("saved despite archive failure".into());
    editor_click(&ctx, &mut screen, &app, "Active");
    editor_click(&ctx, &mut screen, &app, "Save Personal Assistant");
    assert!(
        screen
            .status_message
            .starts_with("Personal Assistant saved."),
        "{}",
        screen.status_message
    );
    assert!(
        screen
            .status_message
            .contains("Archive destination conflict:"),
        "{}",
        screen.status_message
    );
    editor_frame(&ctx, &mut screen, &app, vec![]);
    assert!(screen
        .status_message
        .starts_with("Personal Assistant saved."));
    let loaded = assert_status_reloaded(&app, "Inactive");
    assert_eq!(
        loaded.telephone.as_deref(),
        Some("saved despite archive failure")
    );
    reopen_editor(&app, "Inactive");
    assert_eq!(
        std::fs::read(source).unwrap(),
        b"%PDF-1.4 synthetic original"
    );
    assert_eq!(
        std::fs::read(destination).unwrap(),
        b"%PDF-1.4 synthetic conflicting archive"
    );
}

#[test]
fn editor_missing_history_does_not_block_existing_documents() {
    let (_dir, app, ctx, mut screen) = status_fixture("Active");
    let root = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2026 to 2027");
    std::fs::create_dir_all(&root).unwrap();
    let missing = root.join("Payslip for Week 25 for Example Assistant.pdf");
    app.payroll_timesheet_email_repository.connection.execute(
        "INSERT INTO payslip_revisions(personal_assistant_id,payroll_year,cycle_number,stored_path,sha256,is_current) VALUES(1,'2026/27',6,?1,'0000000000000000000000000000000000000000000000000000000000000000',0)",
        [missing.to_str()],
    ).unwrap();
    let existing = root.join("P60 for Example Assistant.pdf");
    std::fs::write(&existing, b"%PDF-1.4 existing").unwrap();
    editor_click(&ctx, &mut screen, &app, "Active");
    save_editor(&ctx, &mut screen, &app);
    assert_status_reloaded(&app, "Inactive");
    reopen_editor(&app, "Inactive");
    assert!(!missing.exists());
    assert!(!existing.exists());
    assert!(root.join("Archived/P60 for Example Assistant.pdf").exists());
}

#[test]
fn editor_stale_digest_does_not_block_deactivation_filing() {
    let (_dir, app, ctx, mut screen) = status_fixture("Active");
    let root = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2026 to 2027");
    std::fs::create_dir_all(&root).unwrap();
    let changed = root.join("P45 for Example Assistant.pdf");
    std::fs::write(&changed, b"%PDF-1.4 registered").unwrap();
    app.payroll_timesheet_email_repository
        .register_document(1, "p45", &changed, Some("2026/27"))
        .unwrap();
    std::fs::write(&changed, b"%PDF-1.4 changed").unwrap();
    let other = root.join("P60 for Example Assistant.pdf");
    std::fs::write(&other, b"%PDF-1.4 other").unwrap();
    editor_click(&ctx, &mut screen, &app, "Active");
    save_editor(&ctx, &mut screen, &app);
    assert!(
        !screen.status_message.contains("failed"),
        "{}",
        screen.status_message
    );
    assert_status_reloaded(&app, "Inactive");
    reopen_editor(&app, "Inactive");
    assert!(!changed.exists());
    assert_eq!(
        std::fs::read(root.join("Archived/P45 for Example Assistant.pdf")).unwrap(),
        b"%PDF-1.4 changed"
    );
    assert!(!other.exists());
    assert!(root.join("Archived/P60 for Example Assistant.pdf").exists());
}

#[test]
fn editor_deactivation_reports_one_filing_summary_after_reload() {
    for count in [1, 2] {
        let (_dir, app, ctx, mut screen) = status_fixture("Active");
        let root = app
            .context
            .config
            .folders
            .payslip_folder
            .join("2026 to 2027");
        std::fs::create_dir_all(&root).unwrap();
        for week in 1..=count {
            std::fs::write(
                root.join(format!("Payslip for Week {week} for Example Assistant.pdf")),
                b"%PDF-1.4 synthetic filing",
            )
            .unwrap();
        }
        editor_click(&ctx, &mut screen, &app, "Active");
        save_editor(&ctx, &mut screen, &app);
        assert_eq!(screen.status_message,format!("Personal Assistant saved. {count} payroll file(s) filed; document delivery history preserved."));
        assert_status_reloaded(&app, "Inactive");
        assert_eq!(
            std::fs::read_dir(root.join("Archived")).unwrap().count(),
            count
        );
    }
}
