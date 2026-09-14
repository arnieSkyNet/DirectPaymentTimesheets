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
    assert!(result.publication_failure.is_none());
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
        assert!(docs[0]
            .path
            .starts_with(dir.path().join(format!("payslips/2025 to 2026/PA {id}"))));
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
    assert!(app.import_payroll_documents(&source, None).is_err());
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
    assert!(app
        .import_payroll_documents(&source, Some(&unrelated))
        .is_err());
    assert!(!dir.path().join("payslips").exists());
    let result = app
        .import_payroll_documents(&source, Some(&choices[0]))
        .unwrap();
    assert_eq!(
        (result.payslips_imported, result.archival_payslips_imported),
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
