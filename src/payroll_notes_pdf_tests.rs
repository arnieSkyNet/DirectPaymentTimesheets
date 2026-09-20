fn note_pdf_data<'a>(
    schedule: &'a crate::payroll_schedule_repository::PayrollSchedule,
    note: &'a str,
) -> TimesheetPdfData<'a> {
    TimesheetPdfData {
        schedule,
        employer_name: "Test Employer",
        personal_assistant_name: "Test Assistant",
        national_insurance_number: "AB123456C",
        contracted_weekly_hours: "16",
        week_commencing_dates: ["05/04/2032", "12/04/2032", "19/04/2032", "26/04/2032"],
        hours_worked: ["4", "5", "6", "7"],
        annual_leave_hours: ["0"; 4],
        sickness_periods: std::array::from_fn(|_| Vec::new()),
        public_holidays: std::array::from_fn(|_| Vec::new()),
        travel_miles: ["0"; 4],
        previous_cycle_hours: None,
        payroll_department_notes: note,
        employer_signature_path: None,
        pa_signature_path: None,
    }
}
fn parsed_note_pdf(path: &Path) -> PdfDocument {
    PdfDocument::parse(
        &fs::read(path).unwrap(),
        &printpdf::PdfParseOptions::default(),
        &mut Vec::new(),
    )
    .unwrap()
}
fn note_pdf_positions(document: &PdfDocument) -> Vec<(f32, f32)> {
    document
        .pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter_map(|op| match op {
            Op::SetTextCursor { pos } => Some((pos.x.0, pos.y.0)),
            _ => None,
        })
        .collect()
}

#[test]
fn payroll_notes_blank_pdf_preserves_every_text_position_and_single_page() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blank.pdf");
    let schedule = schedule("05/04/2032", "30/04/2032");
    let config = crate::config::PdfConfig::default();
    let payroll = crate::config::PayrollConfig::default();
    let mut previous = None;
    for note in ["", "   ", "\r\n \n\t"] {
        PdfGenerator::generate_to_path(&path, &note_pdf_data(&schedule, note), &config, &payroll)
            .unwrap();
        let pdf = parsed_note_pdf(&path);
        assert_eq!(pdf.pages.len(), 1);
        let positions = note_pdf_positions(&pdf);
        for expected in [107.0, 97.0, 90.0, 64.0] {
            assert!(
                positions
                    .iter()
                    .any(|(_, y)| (y - Pt::from(Mm(expected)).0).abs() < 0.01),
                "legacy baseline {expected}"
            );
        }
        if let Some(old) = &previous {
            assert_eq!(&positions, old);
        }
        previous = Some(positions);
        let text = pdf_extract::extract_text(&path).unwrap();
        assert!(!text.contains("Notes for Payroll Department"));
        assert!(text.contains("DECLARATION"));
    }
}

#[test]
fn payroll_notes_measured_wrap_preserves_lines_spaces_and_overlong_unicode_tokens() {
    let font = footer_font();
    let bold = load_pdf_font(&crate::config::PdfConfig::default().bold_font).unwrap();
    for text in [
        "Please include any hours in lieu in final pay.".into(),
        "First line\nSecond line".into(),
        "Line one\r\n\r\nLine three".into(),
        "W".repeat(150),
        "é".repeat(256),
        "Please confirm returned payroll hours. ".repeat(6),
    ] {
        let layout = layout_payroll_notes(&text, 8.0, &font, &bold)
            .unwrap()
            .unwrap();
        assert_eq!(layout.size, 8.0);
        assert!(layout.heading_y < 116.0);
        assert!(layout.lines.last().unwrap().baseline > layout.declaration_y);
        assert!(layout.declaration_y > layout.sentence_y);
        assert!(layout.sentence_y > layout.signature_y);
        assert!(layout.signature_y > layout.image_y + 17.0);
        assert!(layout.image_y > layout.date_y);
        assert!(
            layout.date_y
                + text_vertical_bounds(
                    &format!(
                        "Date: {}",
                        crate::date_utils::formal(Local::now().date_naive())
                    ),
                    8.0,
                    &font
                )
                .unwrap()
                .0
                >= 56.0
        );
        for line in &layout.lines {
            let (left, right) = footer_line_metrics(&line.text, 8.0, &font).unwrap();
            assert!((right - left) * 25.4 / 72.0 <= 170.0);
        }
        assert_eq!(
            layout
                .lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<String>(),
            text.replace("\r\n", "\n").replace('\n', "")
        );
    }
    let lines = wrap_payroll_notes("One\n\nThree\n", 8.0, &font).unwrap();
    assert_eq!(lines, ["One", "", "Three", ""]);
    assert!(layout_payroll_notes(&"X".repeat(257), 8.0, &font, &bold).is_err());
}

#[test]
fn payroll_notes_pdf_short_multiline_maximum_and_wrapping_stay_before_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.pdf");
    let schedule = schedule("05/04/2032", "30/04/2032");
    let config = crate::config::PdfConfig::default();
    for text in [
        "Please include any hours in lieu in final pay.".into(),
        "Line one\nLine two".into(),
        "W".repeat(150),
        "é".repeat(256),
        "Please confirm returned payroll hours. ".repeat(6),
    ] {
        PdfGenerator::generate_to_path(
            &path,
            &note_pdf_data(&schedule, &text),
            &config,
            &crate::config::PayrollConfig::default(),
        )
        .unwrap();
        let pdf = parsed_note_pdf(&path);
        assert_eq!(pdf.pages.len(), 1);
        let regular = load_pdf_font(&config.regular_font).unwrap();
        let bold = load_pdf_font(&config.bold_font).unwrap();
        let layout = layout_payroll_notes(&text, 8.0, &regular, &bold)
            .unwrap()
            .unwrap();
        let positions = note_pdf_positions(&pdf);
        for y in [
            layout.declaration_y,
            layout.sentence_y,
            layout.signature_y,
            layout.date_y,
        ] {
            assert!(positions
                .iter()
                .any(|(_, actual)| (actual - Pt::from(Mm(y)).0).abs() < 0.01));
        }
        let extracted = pdf_extract::extract_text(&path).unwrap();
        let start = extracted.find("Notes for Payroll Department").unwrap();
        let end = extracted.find("DECLARATION").unwrap();
        assert!(start < end);
        let visible: String = extracted[start..end]
            .trim_start_matches("Notes for Payroll Department")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert_eq!(
            visible,
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        );
        assert!(!extracted.contains("Actual in-lieu"));
    }
}

#[test]
fn payroll_notes_overflow_refuses_without_overwriting_existing_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.pdf");
    let schedule = schedule("05/04/2032", "30/04/2032");
    let config = crate::config::PdfConfig::default();
    let payroll = crate::config::PayrollConfig::default();
    PdfGenerator::generate_to_path(
        &path,
        &note_pdf_data(&schedule, "A short note"),
        &config,
        &payroll,
    )
    .unwrap();
    let before = fs::read(&path).unwrap();
    let note = "x\n".repeat(100);
    let error =
        PdfGenerator::generate_to_path(&path, &note_pdf_data(&schedule, &note), &config, &payroll)
            .unwrap_err()
            .to_string();
    assert!(error.contains("Reduce line breaks or shorten the note"));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn payroll_notes_failed_publication_preserves_candidate_identity_and_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("published.pdf");
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::database::create_schema(&connection).unwrap();
    let repo = crate::payroll_worked_item_repository::PayrollWorkedItemRepository::new(connection);
    let schedule = schedule("05/04/2032", "30/04/2032");
    let config = crate::config::PdfConfig::default();
    let payroll = crate::config::PayrollConfig::default();
    let publish = |note: &str| {
        crate::payroll_snapshot_service::publish_candidate(
            &repo,
            crate::payroll_snapshot_service::CandidatePublication {
                payroll_timesheet_id: 10,
                items: &[],
                final_pdf_path: &path,
                generated_at: "generated",
                previous_cycle_minutes: 0,
                week_ids: &[0; 4],
                week_totals_minutes: &[0; 4],
            },
            |temporary| {
                PdfGenerator::generate_to_path(
                    temporary,
                    &note_pdf_data(&schedule, note),
                    &config,
                    &payroll,
                )
            },
        )
    };
    publish("Short note").unwrap();
    let before = fs::read(&path).unwrap();
    let metadata = repo.snapshot_metadata(10).unwrap();
    assert!(publish(&"line\n".repeat(40)).is_err());
    assert_eq!(repo.snapshot_metadata(10).unwrap(), metadata);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn payroll_notes_section_gaps_use_measured_ink_bounds_and_font_line_spacing() {
    let regular = footer_font();
    let bold = load_pdf_font(&crate::config::PdfConfig::default().bold_font).unwrap();
    let body_line = footer_line_spacing(8.0, &regular);
    for text in [
        "Leaving date 20/08/2026. Please include any hours in lieu in final pay.",
        "First line\nFinal line with descenders: gyjp",
        "Line one\n\nLine three",
    ] {
        let layout = layout_payroll_notes(text, 8.0, &regular, &bold)
            .unwrap()
            .unwrap();
        let bounds: Vec<_> = layout
            .lines
            .iter()
            .map(|l| text_vertical_bounds(&l.text, 8.0, &regular).unwrap())
            .collect();
        let top = bounds.iter().map(|b| b.1).fold(0.0_f32, f32::max);
        let bottom = bounds.iter().map(|b| b.0).fold(0.0_f32, f32::min);
        let note_line = body_line.max(top - bottom + 0.5);
        let (declaration_bottom, declaration_top) =
            text_vertical_bounds("DECLARATION", 9.0, &bold).unwrap();
        let (sentence_bottom, sentence_top) = text_vertical_bounds(
            "I confirm that the hours and information recorded above are correct.",
            8.0,
            &regular,
        )
        .unwrap();
        let (label_bottom, label_top) =
            text_vertical_bounds("Employer signature PA signature", 8.0, &regular).unwrap();
        let note_gap = layout.lines.last().unwrap().baseline + bottom
            - (layout.declaration_y + declaration_top);
        let sentence_gap =
            layout.declaration_y + declaration_bottom - (layout.sentence_y + sentence_top);
        let signature_gap = layout.sentence_y + sentence_bottom - (layout.signature_y + label_top);
        let (_, heading_top) =
            text_vertical_bounds("Notes for Payroll Department", 9.0, &bold).unwrap();
        let table_bottom = 220.0 - 24.0 - 4.0 * 20.0;
        let table_to_heading_gap = table_bottom - (layout.heading_y + heading_top);
        assert!((table_to_heading_gap - (2.0 + 0.5 * note_line)).abs() < 0.001);
        assert!((note_gap - 1.5 * note_line).abs() < 0.001);
        assert!((sentence_gap - body_line).abs() < 0.001);
        assert!((signature_gap - 1.25 * body_line).abs() < 0.001);
        assert!(sentence_gap > 3.0); // Clearly larger than the old 1 mm gap.
        assert!((layout.signature_y + label_bottom - (layout.image_y + 17.0) - 1.0).abs() < 0.001);
        let (date_bottom, date_top) = text_vertical_bounds(
            &format!(
                "Date: {}",
                crate::date_utils::formal(Local::now().date_naive())
            ),
            8.0,
            &regular,
        )
        .unwrap();
        assert!((layout.image_y - (layout.date_y + date_top) - 1.0).abs() < 0.001);
        assert!(layout.date_y + date_bottom >= 56.0);
    }
}

#[test]
fn payroll_notes_increased_spacing_rejects_wide_maximum_note_without_replacing_pdf() {
    let regular = footer_font();
    for length in [200, 256] {
        let text = "W".repeat(length);
        let lines = wrap_payroll_notes(&text, 8.0, &regular).unwrap();
        assert!(lines.len() > 1);
        assert_eq!(lines.concat(), text);
        for line in lines {
            let (left, right) = footer_line_metrics(&line, 8.0, &regular).unwrap();
            assert!((right - left) * 25.4 / 72.0 <= 170.0);
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("candidate.pdf");
        let schedule = schedule("05/04/2032", "30/04/2032");
        let config = crate::config::PdfConfig::default();
        let payroll = crate::config::PayrollConfig::default();
        PdfGenerator::generate_to_path(
            &path,
            &note_pdf_data(&schedule, "A short note"),
            &config,
            &payroll,
        )
        .unwrap();
        let before = fs::read(&path).unwrap();
        let error = PdfGenerator::generate_to_path(
            &path,
            &note_pdf_data(&schedule, &text),
            &config,
            &payroll,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("Reduce line breaks or shorten the note"));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
