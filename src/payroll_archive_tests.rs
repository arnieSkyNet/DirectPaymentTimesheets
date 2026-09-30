use super::*;
use crate::payroll_timesheet_email_repository::EmailDeliveryState;

fn fixture() -> (tempfile::TempDir, Application) {
    let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
    let root = temp_root(&dir);
    app.context.config.folders.payslip_folder = root.join("payslips");
    app.context.config.folders.payroll_information_folder = root.join("info");
    app.context.config.folders.pdf_output = root.join("pdf_output");
    app.payroll_timesheet_email_repository.connection.execute_batch("INSERT INTO personal_assistants(id,first_name,surname,employment_status,email) VALUES (4,'Fictional Middletest','Samplepa','Active','pa4@example.test'),(5,'Imaginary Middletwo','Fixturepa','Active','pa5@example.test');").unwrap();
    (dir, app)
}
fn temp_root(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().canonicalize().unwrap()
}
fn append_raw_suffix(root: &Path, suffix: &str) -> PathBuf {
    let mut path = root.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}
fn pa(app: &Application, id: i64) -> PersonalAssistant {
    app.personal_assistant_repository
        .get_all()
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap()
}
fn pdf(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn register(app: &Application, id: i64, kind: &str, year: &str, filename: &str) -> PathBuf {
    let path = naming::independent_pa_document_directory(
        &app.context.config.folders.payslip_folder,
        id,
        Some(year),
    )
    .unwrap()
    .join(filename);
    pdf(
        &path,
        format!("%PDF-1.4 {id} {kind} {year} {filename}").as_bytes(),
    );
    app.payroll_timesheet_email_repository
        .register_document(id, kind, &path, Some(year))
        .unwrap();
    path
}
fn facts(app: &Application, id: i64) -> Vec<(i64, String, String, Option<String>)> {
    app.payroll_timesheet_email_repository.connection.prepare("SELECT id,sha256,history_state,sent_at FROM imported_payroll_documents WHERE personal_assistant_id=?1 ORDER BY id").unwrap().query_map([id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

#[test]
fn exact_p45_and_p60_imports_are_clean_flat_and_do_not_end_employment() {
    for kind in ["P45", "P60"] {
        let (dir, app) = fixture();
        let ordinary = app
            .context
            .config
            .folders
            .payslip_folder
            .join("2026 to 2027/Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
        pdf(&ordinary, b"%PDF-1.4 ordinary");
        let name = if kind == "P45" {
            "Test Employer - Employee Leaving Statement (P45) for year 2026-27 for Fictional Samplepa.pdf"
                .to_string()
        } else {
            "Test Employer - End of Year Statement (P60) for year 2026-27 for Fictional Samplepa.pdf"
                .into()
        };
        let source = temp_root(&dir).join(name);
        pdf(&source, b"%PDF-1.4 supplement");
        let before = format!("{:?}", pa(&app, 4));
        let result = app.import_payroll_documents(&source, None).unwrap();
        assert_eq!(result.supplements_imported, 1, "{:?}", result.failures);
        let doc = app
            .payroll_timesheet_email_repository
            .documents_for_pa(4)
            .unwrap()
            .remove(0);
        assert_eq!(
            doc.path,
            app.context.config.folders.payslip_folder.join(format!(
                "2026 to 2027/{kind} for year 2026-27 for Fictional Middletest Samplepa.pdf"
            ))
        );
        assert_eq!(doc.history_state, "unknown");
        assert_eq!(doc.delivery_state, EmailDeliveryState::Unsent);
        assert_eq!(format!("{:?}", pa(&app, 4)), before);
        assert!(ordinary.exists());
        assert!(!ordinary.parent().unwrap().join("Archived").exists());
        assert!(!ordinary.parent().unwrap().join("PA 4").exists());
    }
}

#[test]
fn inactive_files_all_years_and_two_pas_into_shared_archived_and_reactivation_keeps_history() {
    let (_dir, app) = fixture();
    let mut sources = Vec::new();
    for year in ["2025/26", "2026/27"] {
        for id in [4, 5] {
            let assistant = pa(&app, id);
            let name = format!("{} {}", assistant.first_name, assistant.surname);
            let path = app
                .context
                .config
                .folders
                .payslip_folder
                .join(naming::payroll_year_directory_name(year).unwrap())
                .join(format!("Payslip for Week 26 for {name}.pdf"));
            pdf(&path, format!("%PDF-1.4 ordinary {id} {year}").as_bytes());
            sources.push((id, path));
            register(
                &app,
                id,
                if year == "2025/26" { "p60" } else { "p45" },
                year,
                &format!("P45) for year {} for {name}.pdf", year.replace('/', "-")),
            );
        }
    }
    app.payroll_timesheet_email_repository.connection.execute_batch("UPDATE imported_payroll_documents SET history_state='external' WHERE personal_assistant_id=4 AND document_type='p60'; UPDATE imported_payroll_documents SET history_state='needs_sending',sent_at='indeterminate:unchanged' WHERE personal_assistant_id=4 AND document_type='p45'; UPDATE imported_payroll_documents SET history_state='application',sent_at='2026-09-29T20:23:33+01:00' WHERE personal_assistant_id=5;").unwrap();
    let before4 = facts(&app, 4);
    let before5 = facts(&app, 5);
    let mut assistant = pa(&app, 4);
    assistant.employment_status = Some("Inactive".into());
    apply(&app, &assistant, true).unwrap();
    assert_eq!(facts(&app, 4), before4);
    for (id, path) in &sources {
        assert_eq!(path.exists(), *id == 5);
    }
    let mut other = pa(&app, 5);
    other.employment_status = Some("Inactive".into());
    apply(&app, &other, true).unwrap();
    for (_, path) in &sources {
        assert!(!path.exists());
        assert!(path
            .parent()
            .unwrap()
            .join("Archived")
            .join(path.file_name().unwrap())
            .exists());
    }
    for year in ["2025/26", "2026/27"] {
        let archive = app
            .context
            .config
            .folders
            .payslip_folder
            .join(naming::payroll_year_directory_name(year).unwrap())
            .join("Archived");
        assert_eq!(fs::read_dir(&archive).unwrap().count(), 4);
        assert!(fs::read_dir(&archive)
            .unwrap()
            .all(|p| p.unwrap().file_type().unwrap().is_file()));
    }
    assert_eq!(facts(&app, 5), before5);
    assert!(crate::payslip_delivery_service::select_payroll_documents(
        &app.payroll_timesheet_email_repository,
        4,
        None
    )
    .is_err());
    assert!(apply(&app, &assistant, true).unwrap().is_empty());
    assistant.employment_status = Some("Active".into());
    apply(&app, &assistant, true).unwrap();
    assert_eq!(facts(&app, 4), before4);
    assert!(sources.iter().all(|(_, p)| !p.exists()));
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
}

#[test]
fn registered_legacy_repair_preserves_every_history_state_and_leaves_active_payslips() {
    for (history, sent) in [
        ("unknown", None),
        ("external", None),
        ("needs_sending", None),
        ("application", Some("sent-tonight")),
        ("needs_sending", Some("indeterminate:attempt")),
    ] {
        let (_dir, app) = fixture();
        let source = register(
            &app,
            4,
            "p45",
            "2026/27",
            "P45) for year 2026-27 for Fictional Samplepa for Fictional Middletest Samplepa.pdf",
        );
        let ordinary = source
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
        pdf(&ordinary, b"%PDF-1.4 ordinary");
        app.payroll_timesheet_email_repository
            .connection
            .execute(
                "UPDATE imported_payroll_documents SET history_state=?1,sent_at=?2",
                params![history, sent],
            )
            .unwrap();
        let before = facts(&app, 4);
        apply(&app, &pa(&app, 4), false).unwrap();
        let destination = naming::supplement_path(
            &app.context.config.folders.payslip_folder,
            &pa(&app, 4),
            "p45",
            Some("2026/27"),
        )
        .unwrap();
        assert!(!source.exists());
        assert!(destination.exists());
        assert!(ordinary.exists());
        assert_eq!(facts(&app, 4), before);
        assert_eq!(
            app.payroll_timesheet_email_repository
                .documents_for_pa(4)
                .unwrap()[0]
                .path,
            destination
        );
        assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
        assert!(apply(&app, &pa(&app, 4), false).unwrap()[0].starts_with("0 payroll"));
        let mut assistant = pa(&app, 4);
        assistant.employment_status = Some("Inactive".into());
        apply(&app, &assistant, true).unwrap();
        assert_eq!(facts(&app, 4), before);
        let source_zip_entry = temp_root(&_dir).join(
            "Test Employer - Employee Leaving Statement (P45) for year 2026-27 for Fictional Samplepa.pdf",
        );
        let stored = app
            .payroll_timesheet_email_repository
            .documents_for_pa(4)
            .unwrap()[0]
            .path
            .clone();
        fs::copy(&stored, &source_zip_entry).unwrap();
        let reimport = app
            .import_payroll_documents(&source_zip_entry, None)
            .unwrap();
        assert_eq!(
            reimport.supplements_already_present, 1,
            "{:?}",
            reimport.failures
        );
        assert_eq!(facts(&app, 4), before);
        let docs = app
            .payroll_timesheet_email_repository
            .documents_for_pa(4)
            .unwrap();
        assert_eq!(
            docs[0].path.parent().unwrap().file_name().unwrap(),
            "Archived"
        );
        assert!(docs[0].path.is_file());
        let bundle = crate::payslip_delivery_service::select_payroll_documents(
            &app.payroll_timesheet_email_repository,
            4,
            None,
        );
        if sent.is_some_and(|s| s.starts_with("indeterminate:")) {
            assert!(bundle.is_err());
        } else {
            assert_eq!(
                bundle.unwrap().document_ids.len(),
                usize::from(history == "needs_sending" && sent.is_none())
            );
        }
    }
}

#[test]
fn collisions_are_reported_together_and_no_file_or_status_is_changed() {
    let (_dir, app) = fixture();
    let sources = [
        register(&app, 4, "p45", "2026/27", "P45 for Fictional Samplepa.pdf"),
        {
            // A legacy duplicate predates the explicit replacement workflow.
            let path = app
                .context
                .config
                .folders
                .payslip_folder
                .join("2026 to 2027/PA 4/P45 for Fictional Middletest Samplepa.pdf");
            pdf(&path, b"%PDF-1.4 legacy duplicate");
            app.payroll_timesheet_email_repository.connection.execute("INSERT INTO imported_payroll_documents(personal_assistant_id,document_type,stored_path,sha256,document_year) VALUES(4,'p45',?1,?2,'2026/27')",params![path.to_str(),file_digest(&path).unwrap()]).unwrap();
            path
        },
    ];
    let before = facts(&app, 4);
    let mut assistant = pa(&app, 4);
    assistant.employment_status = Some("Inactive".into());
    let error = apply(&app, &assistant, true).unwrap_err().to_string();
    for source in &sources {
        assert!(error.contains(&source.display().to_string()));
        assert!(source.exists());
    }
    assert_eq!(facts(&app, 4), before);
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
    // A different pre-existing destination is never overwritten.
    let (_dir, app) = fixture();
    let source = register(&app, 4, "p45", "2026/27", "P45 for Fictional Middletest Samplepa.pdf");
    let target = naming::supplement_destination(
        &app.context.config.folders.payslip_folder,
        &pa(&app, 4),
        "p45",
        Some("2026/27"),
        source.file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();
    pdf(&target, b"%PDF-1.4 different");
    assert!(apply(&app, &pa(&app, 4), false).is_err());
    assert_eq!(fs::read(target).unwrap(), b"%PDF-1.4 different");
    assert!(source.exists());
}

#[test]
fn registry_failure_rolls_back_employment_paths_and_publication() {
    let (_dir, app) = fixture();
    let source = register(&app, 4, "p45", "2026/27", "P45 for Fictional Middletest Samplepa.pdf");
    let earlier = register(
        &app,
        4,
        "p60",
        "2025/26",
        "P60 for year 2025-26 for Fictional Samplepa.pdf",
    );
    let ordinary = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2024 to 2025/Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
    pdf(&ordinary, b"%PDF-1.4 older ordinary");
    let before = facts(&app, 4);
    app.payroll_timesheet_email_repository.connection.execute_batch("CREATE TRIGGER refuse_move BEFORE UPDATE OF stored_path ON imported_payroll_documents WHEN OLD.document_type='p45' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    let mut assistant = pa(&app, 4);
    assistant.employment_status = Some("Inactive".into());
    let target = naming::supplement_destination(
        &app.context.config.folders.payslip_folder,
        &assistant,
        "p45",
        Some("2026/27"),
        source.file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();
    assert!(apply(&app, &assistant, true)
        .unwrap_err()
        .to_string()
        .contains("fixture failure"));
    assert!(source.exists());
    assert!(earlier.exists());
    assert!(ordinary.exists());
    let earlier_target = naming::supplement_destination(
        &app.context.config.folders.payslip_folder,
        &assistant,
        "p60",
        Some("2025/26"),
        earlier.file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();
    assert!(!earlier_target.exists());
    assert!(!ordinary
        .parent()
        .unwrap()
        .join("Archived")
        .join(ordinary.file_name().unwrap())
        .exists());
    assert!(!target.exists());
    assert_eq!(facts(&app, 4), before);
    assert_eq!(
        app.payroll_timesheet_email_repository
            .documents_for_pa(4)
            .unwrap()[0]
            .path,
        source
    );
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
    assert_eq!(
        app.payroll_timesheet_email_repository
            .connection
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_file_moves", [], |r| r.get(0))
            .unwrap(),
        0
    );
}

#[test]
fn journal_cleanup_restarts_safely_and_refuses_changed_or_missing_destination() {
    let (_dir, app) = fixture();
    let base = &app.context.config.folders.payslip_folder;
    let source = base.join("original.pdf");
    let destination = base.join("Archived/copy.pdf");
    pdf(&source, b"%PDF-1.4 original");
    let digest = file_digest(&source).unwrap();
    let db = &app.payroll_timesheet_email_repository.connection;
    db.execute(
        "INSERT INTO payroll_file_moves VALUES(?1,?2,4,?3)",
        params![
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            digest
        ],
    )
    .unwrap();
    assert!(cleanup(db, 4, std::slice::from_ref(base)).is_err());
    assert!(source.exists());
    pdf(&destination, b"%PDF-1.4 wrong");
    assert!(cleanup(db, 4, std::slice::from_ref(base)).is_err());
    assert!(source.exists());
    pdf(&destination, b"%PDF-1.4 original");
    let reopened = Connection::open(&app.context.environment.database_path).unwrap();
    cleanup(&reopened, 4, std::slice::from_ref(base)).unwrap();
    assert!(!source.exists());
    assert!(destination.exists());
    cleanup(&reopened, 4, std::slice::from_ref(base)).unwrap();
}

#[test]
fn unregistered_lookalike_and_out_of_root_registered_paths_are_not_repaired() {
    let (dir, app) = fixture();
    let lookalike = app
        .context
        .config
        .folders
        .payslip_folder
        .join("2026 to 2027/PA 4/P45) unrelated.pdf");
    pdf(&lookalike, b"%PDF-1.4 user file");
    apply(&app, &pa(&app, 4), false).unwrap();
    assert!(lookalike.exists());
    let external = temp_root(&dir).join("elsewhere/P45.pdf");
    pdf(&external, b"%PDF-1.4 external");
    app.payroll_timesheet_email_repository
        .register_document(4, "p45", &external, Some("2026/27"))
        .unwrap();
    assert!(apply(&app, &pa(&app, 4), false).is_err());
    assert!(external.exists());
    let root = temp_root(&dir);
    let traversal = append_raw_suffix(
        &root,
        if cfg!(windows) {
            r"\..\escape.pdf"
        } else {
            "/../escape.pdf"
        },
    );
    assert!(traversal
        .components()
        .any(|component| component == Component::ParentDir));
    assert!(checked_path(&traversal).is_err());
}

#[cfg(windows)]
#[test]
fn checked_path_rejects_dot_components_in_verbatim_windows_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = temp_root(&dir);
    assert!(root.to_string_lossy().starts_with(r"\\?\"));
    let parent = append_raw_suffix(&root, r"\..\escape.pdf");
    assert!(parent
        .components()
        .any(|component| component == Component::ParentDir));
    assert!(checked_path(&parent).is_err());

    let current = append_raw_suffix(&root, r"\.\file.pdf");
    assert!(current
        .components()
        .any(|component| component == Component::CurDir));
    assert!(checked_path(&current).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_archive_directory_is_refused() {
    let (dir, app) = fixture();
    let source = register(&app, 4, "p45", "2026/27", "P45 original.pdf");
    let elsewhere = temp_root(&dir).join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(
        &elsewhere,
        source.parent().unwrap().parent().unwrap().join("Archived"),
    )
    .unwrap();
    let mut assistant = pa(&app, 4);
    assistant.employment_status = Some("Inactive".into());
    assert!(apply(&app, &assistant, true).is_err());
    assert!(source.exists());
    assert_eq!(fs::read_dir(elsewhere).unwrap().count(), 0);
}

#[test]
fn schema34_journal_migration_is_atomic_and_preserves_schema33_history() {
    let (_dir, app) = fixture();
    register(&app, 4, "p45", "2026/27", "P45 original.pdf");
    let db = &app.payroll_timesheet_email_repository.connection;
    db.execute_batch("UPDATE imported_payroll_documents SET history_state='application',sent_at='original-sent'; DROP TABLE payslip_revisions; ALTER TABLE imported_payroll_documents DROP COLUMN superseded_by; DROP TABLE payroll_file_moves; UPDATE schema_version SET version=33;").unwrap();
    let before = facts(&app, 4);
    db.execute_batch("CREATE TRIGGER fail_schema BEFORE UPDATE ON schema_version BEGIN SELECT RAISE(ABORT,'failure'); END;").unwrap();
    assert!(crate::database::create_schema(db).is_err());
    assert!(db.prepare("SELECT * FROM payroll_file_moves").is_err());
    assert_eq!(facts(&app, 4), before);
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap(),
        33
    );
    db.execute_batch("DROP TRIGGER fail_schema").unwrap();
    crate::database::create_schema(db).unwrap();
    crate::database::create_schema(db).unwrap();
    assert_eq!(facts(&app, 4), before);
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap(),
        crate::database::CURRENT_SCHEMA_VERSION
    );
}

#[test]
fn case_insensitive_conflict_is_refused_on_any_test_host() {
    let (_dir, app) = fixture();
    let source = register(&app, 4, "p45", "2026/27", "P45 for Fictional Samplepa.pdf");
    let expected = naming::supplement_destination(
        &app.context.config.folders.payslip_folder,
        &pa(&app, 4),
        "p45",
        Some("2026/27"),
        source.file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();
    let case_variant = expected.with_file_name(
        expected
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase(),
    );
    pdf(&case_variant, b"%PDF-1.4 existing");
    assert!(apply(&app, &pa(&app, 4), false).is_err());
    assert!(source.exists());
    assert_eq!(fs::read(case_variant).unwrap(), b"%PDF-1.4 existing");
}

#[test]
fn inactive_ordinary_delivery_finds_archived_but_unassociated_files_stay_ineligible() {
    let (_dir, app) = fixture();
    let schedule = crate::payroll_schedule_repository::PayrollSchedule {
        id: 1,
        payroll_year: "2026/27".into(),
        cycle_number: 7,
        first_week_commencing: "07/09/2026".into(),
        latest_posting_date: "28/09/2026".into(),
        pay_date: "02/10/2026".into(),
        created_at: "fixture".into(),
        payslips_sent: false,
    };
    let name = "Fictional Middletest Samplepa";
    let path =
        naming::payslip_path(&app.context.config.folders.payslip_folder, name, &schedule).unwrap();
    pdf(&path, b"%PDF-1.4 associated");
    let unassociated = naming::independent_pa_document_directory(
        &app.context.config.folders.payslip_folder,
        4,
        Some("2025/26"),
    )
    .unwrap()
    .join("Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
    pdf(&unassociated, b"%PDF-1.4 unassociated");
    let mut assistant = pa(&app, 4);
    assistant.employment_status = Some("Inactive".into());
    apply(&app, &assistant, true).unwrap();
    let archived =
        naming::existing_payslip_path(&app.context.config.folders.payslip_folder, name, &schedule)
            .unwrap();
    assert_eq!(
        archived,
        path.parent()
            .unwrap()
            .join("Archived")
            .join(path.file_name().unwrap())
    );
    assert!(archived.is_file());
    assert!(app
        .context
        .config
        .folders
        .payslip_folder
        .join(
            "2025 to 2026/Archived/Unassociated - Payslip for Week 26 for Fictional Middletest Samplepa.pdf"
        )
        .is_file());
    assert!(!app
        .context
        .config
        .folders
        .payslip_folder
        .join("2025 to 2026/Archived/Payslip for Week 26 for Fictional Middletest Samplepa.pdf")
        .exists());
    let repo = &app.payroll_timesheet_email_repository;
    let bundle = crate::payslip_delivery_service::select_payroll_documents(
        repo,
        4,
        Some((
            crate::payslip_delivery_service::PayslipDeliveryIdentity {
                personal_assistant_id: 4,
                payroll_year: &schedule.payroll_year,
                cycle_number: schedule.cycle_number,
            },
            &archived,
        )),
    )
    .unwrap();
    assert_eq!(bundle.paths, vec![archived]);
}

#[test]
fn pa_with_registered_supplements_cannot_be_deleted() {
    let (_dir, app) = fixture();
    register(&app, 4, "p60", "2025/26", "P60 for Fictional Samplepa.pdf");
    assert_eq!(
        app.personal_assistant_repository
            .delete_if_unreferenced(4)
            .unwrap(),
        crate::personal_assistant_repository::PersonalAssistantDeleteResult::HasDependentRecords
    );
}

fn rename_schedule() -> crate::payroll_schedule_repository::PayrollSchedule {
    crate::payroll_schedule_repository::PayrollSchedule {
        id: 1,
        payroll_year: "2026/27".into(),
        cycle_number: 7,
        first_week_commencing: "07/09/2026".into(),
        latest_posting_date: "28/09/2026".into(),
        pay_date: "02/10/2026".into(),
        created_at: "fixture".into(),
        payslips_sent: false,
    }
}

#[test]
fn maintained_name_edits_normalise_old_documents_and_preserve_delivery_lookup() {
    for inactive in [false, true] {
        for (given, surname) in [
            ("Fictional Middletest", "Khan"),
            ("Sam Middletest", "Samplepa"),
            ("Fictional New Middle", "Samplepa"),
            ("Fictional Middletest", "Van Dyke-Smith"),
        ] {
            let (_dir, app) = fixture();
            let root = &app.context.config.folders.payslip_folder;
            let schedule = rename_schedule();
            let old = naming::payslip_path(root, "Fictional Middletest Samplepa", &schedule).unwrap();
            pdf(&old, b"%PDF-1.4 ordinary old name");
            let other = naming::payslip_path(root, "Imaginary Middletwo Fixturepa", &schedule).unwrap();
            pdf(&other, b"%PDF-1.4 other PA");
            let mut edited = pa(&app, 4);
            edited.first_name = given.into();
            edited.surname = surname.into();
            if inactive {
                edited.employment_status = Some("Inactive".into());
            }
            let new_name = format!("{given} {surname}");
            let mut sources = Vec::new();
            for kind in ["p45", "p60"] {
                // Exercise old and new names in one registered legacy filename.
                sources.push(register(
                    &app,
                    4,
                    kind,
                    "2026/27",
                    &format!(
                        "{}) for year 2026-27 for {}.pdf",
                        kind.to_uppercase(),
                        if kind == "p45" {
                            format!("Fictional Samplepa for {new_name}")
                        } else {
                            "Fictional Middletest Samplepa".into()
                        }
                    ),
                ));
            }
            app.payroll_timesheet_email_repository.connection.execute_batch(
                "UPDATE imported_payroll_documents SET history_state='application',sent_at='original-sent' WHERE document_type='p60'; UPDATE imported_payroll_documents SET history_state='needs_sending' WHERE document_type='p45';"
            ).unwrap();
            let before = facts(&app, 4);
            apply(&app, &edited, true).unwrap();
            assert_eq!(facts(&app, 4), before);
            assert_eq!(pa(&app, 4).first_name, given);
            assert_eq!(pa(&app, 4).surname, surname);
            assert!(!old.exists());
            assert!(sources.iter().all(|p| !p.exists()));
            assert_eq!(fs::read(&other).unwrap(), b"%PDF-1.4 other PA");
            let current = naming::existing_payslip_path(root, &new_name, &schedule).unwrap();
            let directory = root.join(if inactive {
                "2026 to 2027/Archived"
            } else {
                "2026 to 2027"
            });
            assert_eq!(
                current,
                directory.join(format!("Payslip for Week 26 for {new_name}.pdf"))
            );
            assert_eq!(fs::read(&current).unwrap(), b"%PDF-1.4 ordinary old name");
            let docs = app
                .payroll_timesheet_email_repository
                .documents_for_pa(4)
                .unwrap();
            for doc in &docs {
                assert_eq!(
                    doc.path,
                    directory.join(format!(
                        "{} for year 2026-27 for {new_name}.pdf",
                        doc.document_type.to_uppercase()
                    ))
                );
                assert!(doc.path.is_file());
                assert_eq!(file_digest(&doc.path).unwrap(), doc.sha256);
            }
            let bundle = crate::payslip_delivery_service::select_payroll_documents(
                &app.payroll_timesheet_email_repository,
                4,
                Some((
                    crate::payslip_delivery_service::PayslipDeliveryIdentity {
                        personal_assistant_id: 4,
                        payroll_year: &schedule.payroll_year,
                        cycle_number: schedule.cycle_number,
                    },
                    &current,
                )),
            )
            .unwrap();
            assert_eq!(bundle.email_types, vec!["payslip", "p45"]);
            assert_eq!(bundle.paths[0], current);
            assert_eq!(
                bundle.document_ids,
                vec![docs.iter().find(|d| d.document_type == "p45").unwrap().id]
            );
            apply(&app, &edited, true).unwrap();
            apply(&app, &edited, false).unwrap();
            assert_eq!(facts(&app, 4), before);
            assert_eq!(
                fs::read_dir(&directory).unwrap().count(),
                if inactive { 3 } else { 4 }
            );
        }
    }
}

#[test]
fn name_edit_keeps_archived_history_and_unassociated_evidence_in_place() {
    let (_dir, app) = fixture();
    app.payroll_timesheet_email_repository.connection.execute_batch(
        "UPDATE personal_assistants SET first_name='Anne Marie',surname='Van Dyke-Smith' WHERE id=4;"
    ).unwrap();
    let root = &app.context.config.folders.payslip_folder;
    let archived =
        root.join("2025 to 2026/Archived/Payslip for Week 22 for Anne Marie Van Dyke-Smith.pdf");
    let unassociated =
        root.join("2026 to 2027/PA 4/Employee Payslip for Week 26 for Anne Van Dyke-Smith.pdf");
    pdf(&archived, b"%PDF-1.4 historic ordinary");
    pdf(&unassociated, b"%PDF-1.4 unassociated");
    let original = register(
        &app,
        4,
        "p60",
        "2025/26",
        "P60 for year 2025-26 for Anne Van Dyke-Smith.pdf",
    );
    app.payroll_timesheet_email_repository.connection.execute_batch(
        "UPDATE imported_payroll_documents SET history_state='needs_sending',sent_at='indeterminate:retained';"
    ).unwrap();
    let before = facts(&app, 4);
    let mut edited = pa(&app, 4);
    edited.first_name = "Annie Jane".into();
    edited.surname = "De la Cruz".into();
    apply(&app, &edited, true).unwrap();
    assert_eq!(facts(&app, 4), before);
    assert!(!archived.exists() && !unassociated.exists() && !original.exists());
    assert!(root
        .join("2025 to 2026/Archived/Payslip for Week 22 for Annie Jane De la Cruz.pdf")
        .is_file());
    assert!(root
        .join("2026 to 2027/PA 4/Payslip for Week 26 for Annie Jane De la Cruz.pdf")
        .is_file());
    assert!(
        !naming::existing_payslip_path(root, "Annie Jane De la Cruz", &rename_schedule())
            .unwrap()
            .exists()
    );
    assert!(crate::payslip_delivery_service::select_payroll_documents(
        &app.payroll_timesheet_email_repository,
        4,
        None
    )
    .is_err());
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
}

#[test]
fn ambiguous_old_or_new_names_and_name_change_collisions_refuse_without_mutation() {
    for scenario in ["old ambiguous", "new ambiguous", "collision"] {
        let (_dir, app) = fixture();
        let root = &app.context.config.folders.payslip_folder;
        let old = root.join("2026 to 2027/Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
        pdf(&old, b"%PDF-1.4 original");
        let mut edited = pa(&app, 4);
        edited.surname = "Khan".into();
        if scenario == "old ambiguous" {
            app.payroll_timesheet_email_repository.connection.execute_batch("UPDATE personal_assistants SET first_name='Fictional Middletest',surname='Samplepa' WHERE id=5;").unwrap();
        } else if scenario == "new ambiguous" {
            app.payroll_timesheet_email_repository.connection.execute_batch("UPDATE personal_assistants SET first_name='Fictional Middletest',surname='Khan' WHERE id=5;").unwrap();
        } else {
            pdf(
                &root.join("2026 to 2027/Payslip for Week 26 for Fictional Middletest Khan.pdf"),
                b"%PDF-1.4 unrelated existing",
            );
        }
        assert!(apply(&app, &edited, true).is_err(), "{scenario}");
        assert_eq!(pa(&app, 4).surname, "Samplepa");
        assert_eq!(fs::read(&old).unwrap(), b"%PDF-1.4 original");
        if scenario == "collision" {
            assert_eq!(
                fs::read(root.join("2026 to 2027/Payslip for Week 26 for Fictional Middletest Khan.pdf"))
                    .unwrap(),
                b"%PDF-1.4 unrelated existing"
            );
        }
    }
}

#[test]
fn name_change_registry_failure_rolls_back_name_paths_and_files() {
    let (_dir, app) = fixture();
    let root = &app.context.config.folders.payslip_folder;
    let old = root.join("2025 to 2026/Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
    pdf(&old, b"%PDF-1.4 ordinary");
    let supplement = register(&app, 4, "p45", "2026/27", "P45 for Fictional Samplepa.pdf");
    let before = facts(&app, 4);
    app.payroll_timesheet_email_repository.connection.execute_batch("CREATE TRIGGER refuse_name_move BEFORE UPDATE OF stored_path ON imported_payroll_documents BEGIN SELECT RAISE(ABORT,'name move failure'); END;").unwrap();
    let mut edited = pa(&app, 4);
    edited.surname = "Khan".into();
    assert!(apply(&app, &edited, true)
        .unwrap_err()
        .to_string()
        .contains("name move failure"));
    assert_eq!(pa(&app, 4).surname, "Samplepa");
    assert_eq!(facts(&app, 4), before);
    assert!(old.exists() && supplement.exists());
    assert!(!root
        .join("2025 to 2026/Payslip for Week 26 for Fictional Middletest Khan.pdf")
        .exists());
    assert!(!root
        .join("2026 to 2027/P45 for Fictional Middletest Khan.pdf")
        .exists());
}

#[test]
fn active_repair_finishes_committed_ordinary_cleanup_without_new_ordinary_filing() {
    let (_dir, app) = fixture();
    let root = &app.context.config.folders.payslip_folder;
    let obsolete = root.join("2025 to 2026/Payslip for Week 22 for Fictional Middletest Samplepa.pdf");
    let committed =
        root.join("2025 to 2026/Archived/Payslip for Week 22 for Fictional Middletest Samplepa.pdf");
    let current = root.join("2026 to 2027/Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
    pdf(&obsolete, b"%PDF-1.4 committed ordinary");
    pdf(&committed, b"%PDF-1.4 committed ordinary");
    pdf(&current, b"%PDF-1.4 current ordinary");
    app.payroll_timesheet_email_repository
        .connection
        .execute(
            "INSERT INTO payroll_file_moves VALUES(?1,?2,4,?3)",
            params![
                obsolete.to_str().unwrap(),
                committed.to_str().unwrap(),
                file_digest(&obsolete).unwrap()
            ],
        )
        .unwrap();
    apply(&app, &pa(&app, 4), false).unwrap();
    assert!(!obsolete.exists());
    assert_eq!(fs::read(committed).unwrap(), b"%PDF-1.4 committed ordinary");
    assert_eq!(fs::read(current).unwrap(), b"%PDF-1.4 current ordinary");
    assert_eq!(pa(&app, 4).employment_status.as_deref(), Some("Active"));
    assert_eq!(
        app.payroll_timesheet_email_repository
            .connection
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_file_moves", [], |r| r.get(0))
            .unwrap(),
        0
    );
    assert!(!root.join("2026 to 2027/Archived").exists());
    apply(&app, &pa(&app, 4), false).unwrap();
}

#[test]
fn name_change_reuses_only_proven_interrupted_publication_not_new_name_lookalikes() {
    for with_old_source in [false, true] {
        let (_dir, app) = fixture();
        let root = &app.context.config.folders.payslip_folder;
        let old = root.join("2026 to 2027/Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
        let new = root.join("2026 to 2027/Payslip for Week 26 for Fictional Middletest Khan.pdf");
        pdf(&new, b"%PDF-1.4 same content");
        if with_old_source {
            pdf(&old, b"%PDF-1.4 same content");
        }
        let mut edited = pa(&app, 4);
        edited.surname = "Khan".into();
        let result = apply(&app, &edited, true);
        assert_eq!(result.is_ok(), with_old_source);
        assert_eq!(
            pa(&app, 4).surname,
            if with_old_source { "Khan" } else { "Samplepa" }
        );
        assert_eq!(fs::read(&new).unwrap(), b"%PDF-1.4 same content");
        assert!(!old.exists());
    }
}

#[test]
fn name_edit_with_field_whitespace_still_matches_canonical_delivery_lookup() {
    let (_dir, app) = fixture();
    let root = &app.context.config.folders.payslip_folder;
    let schedule = rename_schedule();
    let original = naming::payslip_path(root, "Fictional Middletest Samplepa", &schedule).unwrap();
    pdf(&original, b"%PDF-1.4 ordinary");
    let mut edited = pa(&app, 4);
    edited.first_name = " Fictional Middletest ".into();
    edited.surname = " Khan ".into();
    apply(&app, &edited, true).unwrap();
    let saved = pa(&app, 4);
    let discovered = naming::existing_payslip_path(
        root,
        &format!("{} {}", saved.first_name, saved.surname),
        &schedule,
    )
    .unwrap();
    assert!(discovered.is_file());
    assert!(!original.exists());
}

#[test]
fn name_change_preserves_all_supplement_history_states_and_document_identities() {
    for (history, sent) in [
        ("unknown", None),
        ("external", None),
        ("needs_sending", None),
        ("application", Some("actual-original-send")),
        ("needs_sending", Some("indeterminate:original-attempt")),
    ] {
        let (_dir, app) = fixture();
        let original = register(
            &app,
            4,
            "p45",
            "2026/27",
            "P45 for year 2026-27 for Fictional Middletest Samplepa.pdf",
        );
        app.payroll_timesheet_email_repository
            .connection
            .execute(
                "UPDATE imported_payroll_documents SET history_state=?1,sent_at=?2",
                params![history, sent],
            )
            .unwrap();
        let before = facts(&app, 4);
        let mut edited = pa(&app, 4);
        edited.surname = "Khan".into();
        apply(&app, &edited, true).unwrap();
        assert_eq!(facts(&app, 4), before);
        let docs = app
            .payroll_timesheet_email_repository
            .documents_for_pa(4)
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].path,
            app.context
                .config
                .folders
                .payslip_folder
                .join("2026 to 2027/P45 for year 2026-27 for Fictional Middletest Khan.pdf")
        );
        assert_eq!(docs[0].document_year.as_deref(), Some("2026/27"));
        assert_eq!(docs[0].document_type, "p45");
        assert_eq!(docs[0].personal_assistant_id, 4);
        assert!(!original.exists());
        let selected = crate::payslip_delivery_service::select_payroll_documents(
            &app.payroll_timesheet_email_repository,
            4,
            None,
        );
        if sent.is_some_and(|s| s.starts_with("indeterminate:")) {
            assert!(selected.is_err());
        } else {
            assert_eq!(
                selected.unwrap().document_ids.len(),
                usize::from(history == "needs_sending" && sent.is_none())
            );
        }
        apply(&app, &edited, false).unwrap();
        assert_eq!(facts(&app, 4), before);
    }
}
