use crate::{app::Application, payroll_schedule_repository::PayrollSchedule};
use std::{io::Write, path::Path};

fn fixture() -> (tempfile::TempDir, Application) {
    let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
    app.context.config.folders.payslip_folder = dir.path().join("payslips");
    app.context.config.folders.payroll_information_folder = dir.path().join("information");
    app.payroll_timesheet_email_repository.connection.execute_batch("INSERT INTO personal_assistants(id, first_name, surname, employment_status, leaving_date)
        VALUES (1, 'Alder', 'Example', 'Inactive', '01/01/2025'), (2, 'Birch', 'Sample', 'Active', NULL), (3, 'Cedar', 'Fixture', 'Active', NULL);").unwrap();
    (dir, app)
}

fn zip(path: &Path, entries: &[(String, Vec<u8>)]) {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

fn entry(name: &str) -> (String, Vec<u8>) {
    (name.into(), format!("%PDF-1.4 {name}").into_bytes())
}

fn unrelated_period() -> PayrollSchedule {
    PayrollSchedule {
        id: 77,
        payroll_year: "2030/31".into(),
        cycle_number: 6,
        first_week_commencing: "12/08/2030".into(),
        latest_posting_date: "30/08/2030".into(),
        pay_date: "06/09/2030".into(),
        created_at: String::new(),
        payslips_sent: false,
    }
}

fn standalone(kind: &str) {
    let (dir, app) = fixture();
    let path = dir.path().join("documents.zip");
    let filename =
        format!("Provider - {kind} End of Year Summary for year 2025-26 for Alder Example.pdf");
    zip(&path, &[entry(&filename)]);
    let before = format!("{:?}", app.personal_assistant_repository.get_all().unwrap());
    assert!(!app.payroll_source_requires_period(&path).unwrap());
    let result = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(result.supplements_imported, 1);
    let docs = app
        .payroll_timesheet_email_repository
        .documents_for_pa(1)
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].document_year.as_deref(), Some("2025/26"));
    assert!(docs[0].path.ends_with(format!("payslips/2025 to 2026/PA 1/{kind} End of Year Summary for year 2025-26 for Alder Example.pdf")));
    let again = app
        .import_payroll_documents(&path, Some(&unrelated_period()))
        .unwrap();
    assert_eq!(again.supplements_already_present, 1);
    assert_eq!(
        app.payroll_timesheet_email_repository
            .documents_for_pa(1)
            .unwrap()[0]
            .id,
        docs[0].id
    );
    assert_eq!(
        before,
        format!("{:?}", app.personal_assistant_repository.get_all().unwrap())
    );
    assert!(app
        .payroll_schedule_repository
        .get_all()
        .unwrap()
        .is_empty());
}

#[test]
fn p60_only_zip_needs_no_period_and_ignores_unrelated_dashboard_year() {
    standalone("P60");
}

#[test]
fn p45_only_zip_needs_no_period_and_never_changes_employment() {
    standalone("P45");
}

#[test]
fn information_and_p30_only_zip_needs_no_period_and_preserves_both_variants() {
    let (dir, app) = fixture();
    let path = dir.path().join("info.zip");
    let names = [
        "P30 Employer's Payslip Week 48 to 52.pdf",
        "P30 Employer's Payslip Week 48 to 52 [1].pdf",
        "Bank-transfer slip Alder Example.pdf",
        "AAAA Quarter End Memo April 2026.pdf",
        "General bulletin.pdf",
    ];
    zip(&path, &names.map(entry));
    assert!(!app.payroll_source_requires_period(&path).unwrap());
    assert_eq!(
        app.import_payroll_documents(&path, None)
            .unwrap()
            .information_files_imported,
        5
    );
    for name in names {
        assert!(dir.path().join("information").join(name).is_file());
    }
    let repeated = app
        .import_payroll_documents(&path, Some(&unrelated_period()))
        .unwrap();
    assert_eq!(repeated.information_files_imported, 0);
    assert_eq!(repeated.information_files_already_present, 5);
    assert!(!dir
        .path()
        .join("information/P30 Employer's Payslip Week 48 to 52 [1] (2).pdf")
        .exists());
    assert!(!dir.path().join("information/2030 to 2031").exists());
    assert!(app
        .payroll_timesheet_email_repository
        .documents_for_pa(1)
        .unwrap()
        .is_empty());
}

#[test]
fn mixed_cross_year_package_classification_and_routing_match_real_filename_model() {
    let (dir, app) = fixture();
    let path = dir.path().join("mixed.zip");
    let mut entries = vec![
        (
            "AAAA Payroll prep sheet 2 - 2026-27.pdf".into(),
            crate::payroll_prep_sheet_import_service::tests::pdf_sheet(),
        ),
        entry("AAAA Quarter End Memo April 2026.pdf"),
        entry("Robin Placeholder - P30 Employer's Payslip for Week 48 to 52.pdf"),
        entry("Robin Placeholder - P30 Employer's Payslip for Week 48 to 52 [1].pdf"),
    ];
    for name in ["Alder Example", "Birch Sample", "Cedar Fixture"] {
        entries.push(entry(&format!(
            "Robin Placeholder - Employee Payslip for Week 50 for {name}.pdf"
        )));
        entries.push(entry(&format!(
            "Robin Placeholder - P60 End of Year Summary for year 2025-26 for {name}.pdf"
        )));
    }
    zip(&path, &entries);
    // Only the new year's early cycle is stored before this cross-year package.
    seed_current_cycle(&app);
    assert!(!app.payroll_source_requires_period(&path).unwrap());
    let result = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(result.archival_payslips_imported, 3);
    assert_eq!(
        (
            result.payslips_imported,
            result.supplements_imported,
            result.information_files_imported,
            result.schedule_entries_imported
        ),
        (0, 3, 4, 13)
    );
    assert!(result.prep_sheet_failures.is_empty());
    assert!(result.failures.is_empty());
    let schedules = app
        .payroll_schedule_repository
        .get_all_for_year("2026/27")
        .unwrap();
    assert_eq!(schedules.len(), 13);
    assert_eq!(schedules[0].first_week_commencing, "23/03/2026");
    assert!(dir
        .path()
        .join("information/2026 to 2027/AAAA Payroll prep sheet 2 - 2026-27.pdf")
        .is_file());
    assert!(dir
        .path()
        .join("information/AAAA Quarter End Memo April 2026.pdf")
        .is_file());
    for (id, name) in [
        (1, "Alder Example"),
        (2, "Birch Sample"),
        (3, "Cedar Fixture"),
    ] {
        let docs = app
            .payroll_timesheet_email_repository
            .documents_for_pa(id)
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].document_type, "p60");
        assert_eq!(docs[0].document_year.as_deref(), Some("2025/26"));
        let expected_directory = dir
            .path()
            .join(format!("payslips/2025 to 2026/PA {id}"))
            .canonicalize()
            .unwrap();
        assert!(docs[0].path.starts_with(expected_directory));
        // A P60 in the same package does not prove this payslip's year.
        assert!(dir
            .path()
            .join(format!(
                "payslips/PA {id}/Employee Payslip for Week 50 for {name}.pdf"
            ))
            .is_file());
        assert!(!dir
            .path()
            .join(format!(
                "payslips/2026 to 2027/Payslip for Week 50 for {name}.pdf"
            ))
            .exists());
    }
}

#[test]
fn individual_documents_and_unknown_year_use_configured_root_without_cycle() {
    let (dir, app) = fixture();
    let path = dir
        .path()
        .join("Provider - P45 Leaving details for Alder Example.pdf");
    std::fs::write(&path, b"%PDF-1.4 leaving details").unwrap();
    assert!(!app.payroll_source_requires_period(&path).unwrap());
    app.import_payroll_documents(&path, None).unwrap();
    let doc = app
        .payroll_timesheet_email_repository
        .documents_for_pa(1)
        .unwrap()
        .remove(0);
    assert_eq!(doc.document_year, None);
    assert!(doc
        .path
        .ends_with("payslips/PA 1/P45 Leaving details for Alder Example.pdf"));
    let ordinary = dir
        .path()
        .join("Employee Payslip for Week 50 for Alder Example.pdf");
    std::fs::write(&ordinary, b"%PDF-1.4 payslip").unwrap();
    assert!(!app.payroll_source_requires_period(&ordinary).unwrap());
    assert_eq!(
        app.import_payroll_documents(&ordinary, None)
            .unwrap()
            .archival_payslips_imported,
        1
    );
    assert_eq!(
        app.import_payroll_documents(&ordinary, Some(&unrelated_period()))
            .unwrap()
            .archival_payslips_already_present,
        1
    );
}

#[test]
fn supplement_filename_variant_is_not_assumed_to_be_a_duplicate() {
    let (dir, app) = fixture();
    let path = dir.path().join("variants.zip");
    zip(
        &path,
        &[
            entry("P60 for Alder Example.pdf"),
            entry("P60 for Alder Example [1].pdf"),
        ],
    );
    assert_eq!(
        app.import_payroll_documents(&path, None)
            .unwrap()
            .supplements_imported,
        2
    );
    let docs = app
        .payroll_timesheet_email_repository
        .documents_for_pa(1)
        .unwrap();
    assert_eq!(docs.len(), 2);
    assert_ne!(docs[0].id, docs[1].id);
    assert_ne!(docs[0].sha256, docs[1].sha256);
}

#[test]
fn exact_explicit_year_and_provider_week_reuse_schedule_but_missing_or_ambiguous_data_prompt() {
    let (dir, app) = fixture();
    let db = &app.payroll_timesheet_email_repository.connection;
    db.execute_batch("INSERT INTO payroll_schedules(id, payroll_year, cycle_number, first_week_commencing, latest_posting_date, pay_date, created_at)
        VALUES (10, '2025/26', 13, '23/02/2026', '13/03/2026', '20/03/2026', 'test');").unwrap();
    let path = dir.path().join("documents.zip");
    let exact = entry("Employee Payslip for Week 50 for Alder Example 2025-26.pdf");
    zip(&path, &[exact.clone()]);
    assert!(!app.payroll_source_requires_period(&path).unwrap());
    assert_eq!(
        app.import_payroll_documents(&path, None)
            .unwrap()
            .payslips_imported,
        1
    );
    assert!(dir
        .path()
        .join("payslips/2025 to 2026/Payslip for Week 50 for Alder Example.pdf")
        .is_file());
    zip(
        &path,
        &[entry("Employee Payslip for Week 50 for Alder Example.pdf")],
    );
    assert!(app.payroll_source_requires_period(&path).unwrap());
    zip(
        &path,
        &[entry(
            "Employee Payslip for Week 49 for Alder Example 2025-26.pdf",
        )],
    );
    assert!(!app.payroll_source_requires_period(&path).unwrap());
    zip(&path, &[exact]);
    db.execute_batch("INSERT INTO payroll_schedules(id, payroll_year, cycle_number, first_week_commencing, latest_posting_date, pay_date, created_at)
        VALUES (11, '2025/26', 12, '23/02/2026', '13/03/2026', '20/03/2026', 'test');").unwrap();
    assert!(app.payroll_source_requires_period(&path).unwrap());
}

fn seed_current_cycle(app: &Application) {
    app.payroll_timesheet_email_repository.connection.execute_batch("INSERT INTO payroll_schedules(id, payroll_year, cycle_number, first_week_commencing, latest_posting_date, pay_date, created_at)
        VALUES (20, '2026/27', 1, '23/03/2026', '10/04/2026', '17/04/2026', 'test');").unwrap();
}

#[test]
fn historical_year_known_archival_evidence_never_creates_email_or_settlement_state() {
    let (dir, app) = fixture();
    seed_current_cycle(&app);
    let current = app.payroll_schedule_repository.get_all().unwrap().remove(0);
    let before = format!("{:?}", app.payroll_schedule_repository.get_all().unwrap());
    let source = dir.path().join("old.zip");
    let name = "Provider - Employee Payslip for Week 50 for Alder Example 2025-26.pdf";
    zip(&source, &[entry(name)]);
    assert!(!app.payroll_source_requires_period(&source).unwrap());
    // Even a supplied unrelated Dashboard schedule cannot associate this file.
    let result = app
        .import_payroll_documents(&source, Some(&current))
        .unwrap();
    assert_eq!(
        (result.payslips_imported, result.archival_payslips_imported),
        (0, 1)
    );
    let archived = dir.path().join(
        "payslips/2025 to 2026/PA 1/Employee Payslip for Week 50 for Alder Example 2025-26.pdf",
    );
    assert!(archived.is_file());
    assert_eq!(
        before,
        format!("{:?}", app.payroll_schedule_repository.get_all().unwrap())
    );
    assert!(app
        .payroll_timesheet_email_repository
        .documents_for_pa(1)
        .unwrap()
        .is_empty());
    assert_eq!(
        app.payroll_timesheet_email_repository
            .connection
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM payroll_timesheet_email_status",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        0
    );
    let canonical = crate::payroll_file_naming::payslip_path(
        &dir.path().join("payslips"),
        "Alder Example",
        &current,
    )
    .unwrap();
    assert!(!canonical.exists());
    assert!(crate::payslip_delivery_service::select_payroll_documents(
        &app.payroll_timesheet_email_repository,
        1,
        Some((
            crate::payslip_delivery_service::PayslipDeliveryIdentity {
                personal_assistant_id: 1,
                payroll_year: &current.payroll_year,
                cycle_number: current.cycle_number
            },
            &canonical
        ))
    )
    .unwrap()
    .paths
    .is_empty());
    assert_eq!(
        app.import_payroll_documents(&source, None)
            .unwrap()
            .archival_payslips_already_present,
        1
    );
    let original = std::fs::read(&archived).unwrap();
    zip(
        &source,
        &[(name.into(), b"%PDF-1.4 different content".to_vec())],
    );
    assert_eq!(
        app.import_payroll_documents(&source, None)
            .unwrap()
            .failures
            .len(),
        1
    );
    assert_eq!(std::fs::read(archived).unwrap(), original);
}

#[test]
fn yearless_pa_documents_escape_configured_year_suffix_and_keep_known_years() {
    let (dir, mut app) = fixture();
    app.context.config.folders.payslip_folder = dir.path().join("payslips/2026 to 2027");
    let source = dir.path().join("unknown.zip");
    zip(
        &source,
        &[
            entry("Provider - Employee Payslip for Week 50 for Alder Example.pdf"),
            entry("Provider - P60 for Alder Example.pdf"),
            entry("Provider - P45 Leaving details for Alder Example.pdf"),
            entry("Provider - P60 2025-26 for Alder Example.pdf"),
            entry("Provider - P45 2025-26 for Alder Example.pdf"),
        ],
    );
    assert!(!app.payroll_source_requires_period(&source).unwrap());
    let before = format!("{:?}", app.personal_assistant_repository.get_all().unwrap());
    let result = app.import_payroll_documents(&source, None).unwrap();
    assert_eq!(
        (
            result.archival_payslips_imported,
            result.supplements_imported
        ),
        (1, 4)
    );
    for name in [
        "Employee Payslip for Week 50 for Alder Example.pdf",
        "P60 for Alder Example.pdf",
        "P45 Leaving details for Alder Example.pdf",
    ] {
        assert!(dir.path().join("payslips/PA 1").join(name).exists());
    }
    for kind in ["P60", "P45"] {
        assert!(dir
            .path()
            .join(format!(
                "payslips/2025 to 2026/PA 1/{kind} 2025-26 for Alder Example.pdf"
            ))
            .exists());
    }
    assert!(!dir.path().join("payslips/2026 to 2027").exists());
    assert_eq!(
        before,
        format!("{:?}", app.personal_assistant_repository.get_all().unwrap())
    );
    let bundle = crate::payslip_delivery_service::select_payroll_documents(
        &app.payroll_timesheet_email_repository,
        1,
        None,
    )
    .unwrap();
    assert_eq!(bundle.paths.len(), 4);
    assert!(!bundle.email_types.contains(&"payslip"));
}

#[test]
fn cycle_choice_contains_only_plausible_periods_and_cannot_contaminate_archival_files() {
    let (dir, app) = fixture();
    seed_current_cycle(&app);
    let db = &app.payroll_timesheet_email_repository.connection;
    db.execute_batch("INSERT INTO payroll_schedules(id, payroll_year, cycle_number, first_week_commencing, latest_posting_date, pay_date, created_at)
        VALUES (10, '2025/26', 13, '23/02/2026', '13/03/2026', '20/03/2026', 'test');").unwrap();
    let source = dir.path().join("two.zip");
    zip(
        &source,
        &[
            entry("Employee Payslip for Week 50 for Alder Example.pdf"),
            entry("Employee Payslip for Week 50 for Birch Sample 2024-25.pdf"),
        ],
    );
    assert!(app.payroll_source_requires_period(&source).unwrap());
    let choices = app.payroll_source_period_candidates(&source).unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].id, 10);
    let unrelated = app
        .payroll_schedule_repository
        .get_all()
        .unwrap()
        .into_iter()
        .find(|s| s.id == 20)
        .unwrap();
    let partial = app
        .import_payroll_documents(&source, Some(&unrelated))
        .unwrap();
    assert_eq!(partial.failures.len(), 1);
    assert_eq!(partial.archival_payslips_imported, 1);
    assert_eq!(partial.payslips_imported, 0);
    let result = app
        .import_payroll_documents(&source, Some(&choices[0]))
        .unwrap();
    assert_eq!(
        (
            result.payslips_imported,
            result.archival_payslips_already_present
        ),
        (1, 1)
    );
    assert!(dir
        .path()
        .join("payslips/2025 to 2026/Payslip for Week 50 for Alder Example.pdf")
        .exists());
    assert!(dir
        .path()
        .join(
            "payslips/2024 to 2025/PA 2/Employee Payslip for Week 50 for Birch Sample 2024-25.pdf"
        )
        .exists());
}

#[test]
fn future_recurring_week_does_not_supply_a_year_for_a_historical_return() {
    let schedules = vec![PayrollSchedule {
        id: 20,
        payroll_year: "2026/27".into(),
        cycle_number: 13,
        first_week_commencing: "22/02/2027".into(),
        latest_posting_date: "12/03/2027".into(),
        pay_date: "19/03/2027".into(),
        created_at: String::new(),
        payslips_sent: false,
    }];
    assert_eq!(
        crate::payroll_file_naming::paye_week(&schedules[0]).unwrap(),
        50
    );
    let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
    assert!(crate::payroll_file_naming::plausible_payslip_schedules(
        "Employee Payslip for Week 50 for Alder Example.pdf",
        &schedules,
        today
    )
    .is_empty());
    assert!(crate::payroll_file_naming::infer_payslip_schedule(
        &["Employee Payslip for Week 50 for Alder Example 2026-27.pdf".into()],
        &schedules
    )
    .is_some());
}

fn information_reimport_is_unchanged(name: &str) {
    let (dir, app) = fixture();
    let source = dir.path().join("info.zip");
    zip(&source, &[entry(name)]);
    let first = app.import_payroll_documents(&source, None).unwrap();
    assert_eq!(
        (
            first.information_files_imported,
            first.information_files_already_present
        ),
        (1, 0)
    );
    let again = app.import_payroll_documents(&source, None).unwrap();
    assert_eq!(
        (
            again.information_files_imported,
            again.information_files_already_present
        ),
        (0, 1)
    );
    assert!(again.published_paths.is_empty());
    assert!(again
        .details
        .iter()
        .any(|detail| detail.contains("already present and unchanged") && detail.contains(name)));
    let files: Vec<_> = std::fs::read_dir(dir.path().join("information"))
        .unwrap()
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        std::fs::read(dir.path().join("information").join(name)).unwrap(),
        entry(name).1
    );
}

#[test]
fn identical_p30_reimport_keeps_one_physical_file() {
    information_reimport_is_unchanged(
        "Robin Placeholder - P30 Employer's Payslip for Week 48 to 52.pdf",
    );
}

#[test]
fn identical_memo_and_other_information_reimports_keep_one_physical_file() {
    for name in [
        "AAAA Quarter End Memo April 2026.pdf",
        "Bank-transfer slip.pdf",
        "General information.txt",
    ] {
        information_reimport_is_unchanged(name);
    }
}

#[test]
fn different_information_bytes_keep_second_copy_and_its_reimport_does_not_create_third() {
    for name in ["Memo.pdf", "P30 Employer's Payslip [1].pdf"] {
        let (dir, app) = fixture();
        let source = dir.path().join("info.zip");
        let first_bytes = b"%PDF-1.4 content AAA".to_vec();
        let second_bytes = b"%PDF-1.4 content BBB".to_vec();
        zip(&source, &[(name.into(), first_bytes.clone())]);
        app.import_payroll_documents(&source, None).unwrap();
        zip(&source, &[(name.into(), second_bytes.clone())]);
        let second = app.import_payroll_documents(&source, None).unwrap();
        assert_eq!(
            (
                second.information_files_imported,
                second.information_files_already_present
            ),
            (1, 0)
        );
        let renamed = dir
            .path()
            .join("information")
            .join(name.replace(".pdf", " (2).pdf"));
        assert_eq!(std::fs::read(&renamed).unwrap(), second_bytes);
        let original = dir.path().join("information").join(name);
        assert_eq!(std::fs::read(&original).unwrap(), first_bytes);
        let again = app.import_payroll_documents(&source, None).unwrap();
        assert_eq!(
            (
                again.information_files_imported,
                again.information_files_already_present
            ),
            (0, 1)
        );
        assert!(again
            .details
            .iter()
            .any(|detail| detail.contains("(2).pdf")));
        assert_eq!(
            std::fs::read_dir(dir.path().join("information"))
                .unwrap()
                .count(),
            2
        );
        // Only a temporary fixture is changed: a gap must not hide an identical later candidate.
        std::fs::remove_file(&original).unwrap();
        assert_eq!(
            app.import_payroll_documents(&source, None)
                .unwrap()
                .information_files_already_present,
            1
        );
        assert!(!original.exists());
        assert!(renamed.exists());
    }
}

#[test]
fn provider_bracket_variant_is_a_distinct_filename_even_with_identical_bytes() {
    let (dir, app) = fixture();
    let source = dir.path().join("info.zip");
    let names = [
        "P30 Employer's Payslip.pdf",
        "P30 Employer's Payslip [1].pdf",
    ];
    zip(
        &source,
        &names.map(|name| (name.into(), b"%PDF-1.4 same content".to_vec())),
    );
    assert_eq!(
        app.import_payroll_documents(&source, None)
            .unwrap()
            .information_files_imported,
        2
    );
    let repeated = app.import_payroll_documents(&source, None).unwrap();
    assert_eq!(
        (
            repeated.information_files_imported,
            repeated.information_files_already_present
        ),
        (0, 2)
    );
    assert_eq!(
        std::fs::read_dir(dir.path().join("information"))
            .unwrap()
            .count(),
        2
    );
    for name in names {
        assert!(dir.path().join("information").join(name).exists());
    }
}

#[test]
fn identical_prep_sheet_reprocesses_schedule_without_another_physical_pdf() {
    for name in [
        "AAAA Payroll prep sheet 2 - 2026-27.pdf",
        "Provider attachment.pdf",
    ] {
        let (dir, app) = fixture();
        let source = dir.path().join("prep.zip");
        zip(
            &source,
            &[(
                name.into(),
                crate::payroll_prep_sheet_import_service::tests::pdf_sheet(),
            )],
        );
        let first = app.import_payroll_documents(&source, None).unwrap();
        assert_eq!(
            (
                first.information_files_imported,
                first.schedule_entries_imported
            ),
            (1, 13)
        );
        let again = app.import_payroll_documents(&source, None).unwrap();
        assert_eq!(
            (
                again.information_files_imported,
                again.information_files_already_present,
                again.schedule_entries_imported
            ),
            (0, 1, 13)
        );
        assert!(again.published_paths.is_empty());
        assert_eq!(again.prep_sheet_paths, first.prep_sheet_paths);
        let stored = &first.prep_sheet_paths[0];
        assert!(stored.exists());
        assert_eq!(
            std::fs::read_dir(stored.parent().unwrap()).unwrap().count(),
            1
        );
        assert!(again.prep_sheet_failures.is_empty());
    }
}

#[test]
fn partial_zip_reports_every_failure_and_keeps_database_and_files_consistent() {
    let (dir, app) = fixture();
    let db = &app.payroll_timesheet_email_repository.connection;
    db.execute_batch("UPDATE personal_assistants SET first_name = 'Alder Middle' WHERE id = 1;
        UPDATE personal_assistants SET first_name = 'Cedar James' WHERE id = 3;
        INSERT INTO personal_assistants(id, first_name, surname) VALUES (4, 'Cedar John', 'Fixture');
        CREATE TRIGGER fail_p45 BEFORE INSERT ON imported_payroll_documents
        WHEN NEW.document_type = 'p45' AND NEW.personal_assistant_id = 2
        BEGIN SELECT RAISE(ABORT, 'registration fixture failure'); END;").unwrap();
    let path = dir.path().join("mixed.ZIP");
    let bad = [
        "Payslip Nobody Here.pdf",
        "P45 for Cedar Fixture.pdf",
        "P45 for Birch Sample.pdf",
        "Payslip Birch Sample.pdf",
    ];
    zip(&path, &[
        entry(bad[0]),
        entry("Mark Worsdall - Employee Leaving Statement (P45) for year 2026-27 for Alder Example.pdf"),
        entry(bad[1]), entry(bad[2]),
        (bad[3].into(), b"invalid PDF".to_vec()),
        entry("Payslip Alder Example.pdf"),
        entry("P60 for Cedar James Fixture.pdf"),
        entry("P30 Employer's Payslip.pdf"), entry("Quarter End Memo.pdf"),
    ]);
    let original = std::fs::read(&path).unwrap();
    assert!(!app.payroll_source_requires_period(&path).unwrap());
    let result = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(result.imported_count(), 5);
    assert_eq!(result.failures.len(), 4, "{:?}", result.failures);
    for name in bad {
        assert!(result.failures.iter().any(|failure| failure.contains(name)));
    }
    assert_eq!(result.open_zip_path(), Some(path.as_path()));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(result.published_paths.len(), 5);
    for path in &result.published_paths {
        assert!(path.is_file());
    }
    let docs = app
        .payroll_timesheet_email_repository
        .documents_for_pa(1)
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert!(docs[0].path.is_file());
    assert_eq!(
        app.payroll_timesheet_email_repository
            .documents_for_pa(3)
            .unwrap()
            .len(),
        1
    );
    for id in [2, 4] {
        assert!(app
            .payroll_timesheet_email_repository
            .documents_for_pa(id)
            .unwrap()
            .is_empty());
    }
    assert!(!dir
        .path()
        .join("payslips/PA 2/P45 for Birch Sample.pdf")
        .exists());
    assert!(!dir
        .path()
        .join("payslips/PA 3/P45 for Cedar Fixture.pdf")
        .exists());
    assert!(!dir.path().join("payslips/PA 4").exists());
    fn no_temporary_files(path: &Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                no_temporary_files(&path);
            } else {
                assert!(!path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".tmp"));
            }
        }
    }
    no_temporary_files(dir.path());
    // Retry neither duplicates successful registrations nor counts them as new imports.
    let again = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(again.imported_count(), 0);
    assert_eq!(again.failures.len(), 4);
    assert_eq!(again.supplements_already_present, 2);
}

#[test]
fn all_invalid_zip_reports_zero_and_every_filename_without_registering_or_writing() {
    let (dir, app) = fixture();
    let path = dir.path().join("invalid.zip");
    let names = [
        "P45 for Nobody Here.pdf",
        "Payslip Nobody Here.pdf",
        "../unsafe.txt",
    ];
    zip(&path, &names.map(entry));
    assert!(!app.payroll_source_requires_period(&path).unwrap());
    let result = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(result.imported_count(), 0);
    assert_eq!(result.failures.len(), names.len());
    for name in names {
        assert!(result.failures.iter().any(|failure| failure.contains(name)));
    }
    assert!(result.published_paths.is_empty());
    assert_eq!(result.open_zip_path(), Some(path.as_path()));
    assert!(!dir.path().join("payslips").exists());
    assert!(!dir.path().join("information").exists());
    assert_eq!(
        app.payroll_timesheet_email_repository
            .connection
            .query_row(
                "SELECT COUNT(*) FROM imported_payroll_documents",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn existing_registration_conflict_preserves_file_and_continues_with_later_documents() {
    let (dir, app) = fixture();
    let path = dir.path().join("documents.zip");
    let name = "P60 for Alder Example.pdf";
    zip(&path, &[entry(name)]);
    let first = app.import_payroll_documents(&path, None).unwrap();
    let stored = first.published_paths[0].clone();
    let original = std::fs::read(&stored).unwrap();
    app.payroll_timesheet_email_repository
        .connection
        .execute(
            "UPDATE imported_payroll_documents SET sha256 = '0000000000000000000000000000000000000000000000000000000000000000'",
            [],
        )
        .unwrap();
    zip(
        &path,
        &[
            entry(name),
            entry("P45 for Birch Sample.pdf"),
            entry("Quarter End Memo.pdf"),
        ],
    );
    let result = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(result.failures.len(), 1);
    assert!(result.failures[0].contains(name));
    assert_eq!(result.imported_count(), 2);
    assert_eq!(result.supplements_already_present, 0);
    assert_eq!(std::fs::read(stored).unwrap(), original);
    assert_eq!(
        app.payroll_timesheet_email_repository
            .documents_for_pa(1)
            .unwrap()[0]
            .sha256,
        "0000000000000000000000000000000000000000000000000000000000000000"
    );
    assert_eq!(
        app.payroll_timesheet_email_repository
            .documents_for_pa(2)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn conflicting_name_tokens_never_write_or_register_but_complete_names_import() {
    let (dir, app) = fixture();
    app.payroll_timesheet_email_repository.connection.execute_batch(
        "UPDATE personal_assistants SET first_name = 'John Michael', surname = 'Smith' WHERE id = 1;
         UPDATE personal_assistants SET first_name = 'Anne Marie', surname = 'Van Dyke' WHERE id = 2;"
    ).unwrap();
    let path = dir.path().join("names.zip");
    let bad = [
        "Payslip John John Smith.pdf",
        "Payslip Anne Van Dyke-Smith.pdf",
        "P45 for John John Smith.pdf",
    ];
    zip(
        &path,
        &[
            entry(bad[0]),
            entry(bad[1]),
            entry(bad[2]),
            entry("Payslip John Smith.pdf"),
            entry("P45 for Anne Van Dyke.pdf"),
        ],
    );
    let result = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(result.failures.len(), 3);
    for name in bad {
        assert!(result.failures.iter().any(|f| f.contains(name)));
    }
    assert_eq!(result.archival_payslips_imported, 1);
    assert_eq!(result.supplements_imported, 1);
    assert_eq!(result.published_paths.len(), 2);
    assert!(result.published_paths.iter().all(|p| p.is_file()));
    assert!(app
        .payroll_timesheet_email_repository
        .documents_for_pa(1)
        .unwrap()
        .is_empty());
    let docs = app
        .payroll_timesheet_email_repository
        .documents_for_pa(2)
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert!(docs[0].path.is_file());
    assert!(!dir
        .path()
        .join("payslips/PA 1/Payslip John John Smith.pdf")
        .exists());
    assert!(!dir
        .path()
        .join("payslips/PA 2/Payslip Anne Van Dyke-Smith.pdf")
        .exists());
}

#[test]
fn case_only_pa_destinations_are_all_rejected_in_either_zip_order() {
    for reverse in [false, true] {
        let (dir, app) = fixture();
        let path = dir.path().join("conflicts.zip");
        let names = ["P45 for Alder Example.pdf", "p45 for Alder Example.pdf"];
        let mut entries = names.map(entry).to_vec();
        if reverse {
            entries.reverse();
        }
        entries.push(entry("Quarter End Memo.pdf"));
        zip(&path, &entries);
        let result = app.import_payroll_documents(&path, None).unwrap();
        assert_eq!(result.failures.len(), 2);
        for name in names {
            assert!(result.failures.iter().any(|f| f.starts_with(name)));
        }
        assert_eq!(result.information_files_imported, 1);
        assert_eq!(result.supplements_imported, 0);
        assert!(!dir.path().join("payslips").exists());
        assert!(app
            .payroll_timesheet_email_repository
            .documents_for_pa(1)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn archival_success_details_follow_successful_publication_only() {
    let (dir, app) = fixture();
    let path = dir.path().join("archival.zip");
    let name = "Payslip Alder Example.pdf";
    zip(&path, &[(name.into(), b"not a PDF".to_vec())]);
    let failed = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(failed.imported_count(), 0);
    assert_eq!(failed.failures.len(), 1);
    assert!(failed.failures[0].contains(name));
    assert!(failed.published_paths.is_empty());
    assert!(!failed
        .details
        .iter()
        .any(|d| d.contains("Archived ordinary payslip")));
    zip(&path, &[entry(name)]);
    let success = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(success.archival_payslips_imported, 1);
    assert!(success
        .details
        .iter()
        .any(|d| d.contains("Archived ordinary payslip")));
    let original = std::fs::read(&success.published_paths[0]).unwrap();
    zip(&path, &[(name.into(), b"%PDF-1.4 changed".to_vec())]);
    let conflict = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(conflict.failures.len(), 1);
    assert!(conflict.failures[0].contains(name));
    assert_eq!(conflict.imported_count(), 0);
    assert!(!conflict
        .details
        .iter()
        .any(|d| d.contains("Archived ordinary payslip")));
    assert_eq!(
        std::fs::read(&success.published_paths[0]).unwrap(),
        original
    );
}

#[test]
fn recovery_zip_verification_rejects_replacement_even_with_same_size_and_mtime() {
    let (dir, app) = fixture();
    let path = dir.path().join("original.zip");
    let unknown = "Payslip Unknown Person.pdf";
    let write_stored_zip = |bytes: &[u8]| {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        writer
            .start_file(
                unknown,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
        writer.finish().unwrap();
    };
    write_stored_zip(&entry(unknown).1);
    let result = app.import_payroll_documents(&path, None).unwrap();
    assert_eq!(result.verified_zip_path().unwrap(), path.as_path());
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let length = std::fs::metadata(&path).unwrap().len();
    // Replace content in place to retain filesystem identity and restore mtime;
    // only the digest can distinguish this other, still-valid ZIP.
    write_stored_zip(&vec![b'x'; entry(unknown).1.len()]);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), length);
    assert!(zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).is_ok());
    // Prove this reaches digest verification rather than a cheap metadata refusal.
    assert_eq!(result.open_zip_path(), Some(path.as_path()));
    assert!(result
        .verified_zip_path()
        .unwrap_err()
        .to_string()
        .contains("changed"));
    std::fs::remove_file(&path).unwrap();
    assert!(result.open_zip_path().is_none());
    assert!(result.verified_zip_path().is_err());
    zip(&path, &[entry(unknown)]);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new().set_modified(modified + std::time::Duration::from_secs(5)),
        )
        .unwrap();
    assert!(result.open_zip_path().is_none());
    assert!(result.verified_zip_path().is_err());
    let pdf = dir.path().join(unknown);
    std::fs::write(&pdf, b"%PDF-1.4").unwrap();
    let single = app.import_payroll_documents(&pdf, None).unwrap();
    assert!(single.source_zip.is_none());
    assert!(single.open_zip_path().is_none());
}
