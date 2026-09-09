use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use printpdf::{
    graphics::{Line, LinePoint, Point, Rect},
    ops::{Op, PdfPage},
    units::{Mm, Pt},
    ParsedFont, PdfDocument, PdfSaveOptions, TextItem, XObjectTransform,
};

pub struct TimesheetPdfData<'a> {
    pub schedule: &'a crate::payroll_schedule_repository::PayrollSchedule,
    pub employer_name: &'a str,
    pub personal_assistant_name: &'a str,
    pub national_insurance_number: &'a str,
    pub contracted_weekly_hours: &'a str,

    pub week_commencing_dates: [&'a str; 4],
    pub hours_worked: [&'a str; 4],

    pub annual_leave_hours: [&'a str; 4],
    pub sick_leave_hours: [&'a str; 4],
    pub public_holidays: [Vec<PublicHolidayPdfEntry>; 4],
    pub travel_miles: [&'a str; 4],

    pub previous_cycle_hours: Option<&'a str>,

    pub employer_signature_path: Option<&'a Path>,
    pub pa_signature_path: Option<&'a Path>,
}

pub struct PublicHolidayPdfEntry {
    pub hours: String,
    pub date: String,
}

pub struct PdfGenerator;

impl PdfGenerator {
    pub fn timesheet_output_path(
        output_dir: &Path,
        personal_assistant_name: &str,
        schedule: &crate::payroll_schedule_repository::PayrollSchedule,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        crate::payroll_file_naming::timesheet_path(output_dir, personal_assistant_name, schedule)
    }

    pub fn output_path(
        output_dir: &Path,
        data: &TimesheetPdfData<'_>,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        Self::timesheet_output_path(output_dir, data.personal_assistant_name, data.schedule)
    }

    pub fn generate(
        output_dir: &Path,
        data: &TimesheetPdfData<'_>,
        pdf_config: &crate::config::PdfConfig,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        fs::create_dir_all(output_dir)?;

        let output_path = Self::output_path(output_dir, data)?;

        Self::generate_to_path(&output_path, data, pdf_config)?;
        Ok(output_path)
    }

    pub fn generate_to_path(
        output_path: &Path,
        data: &TimesheetPdfData<'_>,
        pdf_config: &crate::config::PdfConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not create payroll PDF output directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let mut document = PdfDocument::new("Direct Payment Timesheet");

        let regular_bytes = fs::read(&pdf_config.regular_font)?;
        let bold_bytes = fs::read(&pdf_config.bold_font)?;

        let mut font_warnings = Vec::new();

        let regular_font = ParsedFont::from_bytes(&regular_bytes, 0, &mut font_warnings)
            .ok_or("Could not parse configured regular PDF font")?;

        let bold_font = ParsedFont::from_bytes(&bold_bytes, 0, &mut font_warnings)
            .ok_or("Could not parse configured bold PDF font")?;

        let regular_id = document.add_font(&regular_font);
        let bold_id = document.add_font(&bold_font);

        let mut ops = Vec::new();

        // ------------------------------------------------------------
        // Header
        // ------------------------------------------------------------

        write_text(
            &mut ops,
            "Personal Budget Support Service",
            105.0,
            278.0,
            14.0,
            true,
            TextAlignment::Centre,
            &bold_id,
            &regular_id,
        );

        write_text(
            &mut ops,
            "4 WEEKLY TIME SHEET",
            105.0,
            269.0,
            12.0,
            true,
            TextAlignment::Centre,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Employer / PA information
        // ------------------------------------------------------------

        write_text(
            &mut ops,
            &format!("Employer: {}", data.employer_name),
            20.0,
            253.0,
            9.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        write_text(
            &mut ops,
            &format!(
                "Personal Assistant: {} (NI: {})",
                data.personal_assistant_name, data.national_insurance_number
            ),
            20.0,
            244.0,
            9.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        for (index, line) in wrap_text(
            &format!("Contracted Weekly Hours: {}", data.contracted_weekly_hours),
            170.0,
            9.0,
        )
        .iter()
        .enumerate()
        {
            write_text(
                &mut ops,
                line,
                20.0,
                235.0 - (index as f32 * 4.0),
                9.0,
                false,
                TextAlignment::Left,
                &bold_id,
                &regular_id,
            );
        }

        // ------------------------------------------------------------
        // Main four-week table
        // ------------------------------------------------------------

        let table_x = 15.0;
        let table_top = 220.0;

        let header_height = 24.0;
        let week_row_height = 20.0;

        let columns = [
            ("W/c\nDate", 28.0),
            ("Hours\nworked", 42.0),
            ("Annual\nLeave\nHrs.", 25.0),
            ("Sick\nleave\n/ SSP", 25.0),
            ("Public Hols.\nhours\nworked", 25.0),
            ("Travel\nMiles\nclaimed @\n.40p/mile", 25.0),
        ];

        draw_table(
            &mut ops,
            table_x,
            table_top,
            header_height,
            week_row_height,
            &columns,
            &data.week_commencing_dates,
            &data.hours_worked,
            &data.annual_leave_hours,
            &data.sick_leave_hours,
            &data.public_holidays,
            &data.travel_miles,
            data.previous_cycle_hours,
            pdf_config,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Signatures / declaration
        // ------------------------------------------------------------

        let signature_y = 82.0;

        write_text(
            &mut ops,
            "DECLARATION",
            20.0,
            signature_y + 25.0,
            9.0,
            true,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        write_text(
            &mut ops,
            "I confirm that the hours and information recorded above are correct.",
            20.0,
            signature_y + 15.0,
            8.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Employer signature
        // ------------------------------------------------------------

        if let Some(path) = data.employer_signature_path {
            add_signature(&mut document, &mut ops, path, 20.0, 58.0, 55.25, 17.0)?;
        }

        write_text(
            &mut ops,
            "Employer signature",
            22.0,
            signature_y,
            8.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // PA signature
        // ------------------------------------------------------------

        if let Some(path) = data.pa_signature_path {
            add_signature(&mut document, &mut ops, path, 115.0, 58.0, 55.25, 17.0)?;
        }

        write_text(
            &mut ops,
            "PA signature",
            117.0,
            signature_y,
            8.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Signature dates
        // ------------------------------------------------------------

        let current_date = Local::now().format("%d/%m/%Y").to_string();

        write_text(
            &mut ops,
            &format!("Date: {}", current_date),
            20.0,
            45.0,
            8.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        write_text(
            &mut ops,
            &format!("Date: {}", current_date),
            115.0,
            45.0,
            8.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Required NASS statements
        // ------------------------------------------------------------

        write_text(
            &mut ops,
            "Both the employer and the employee must sign all time sheets before NASS can process them.",
            20.0,
            32.0,
            7.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        write_text(
            &mut ops,
            "These time sheets will be retained on file for 6 years and may be required for inspection by the County Treasurer.",
            20.0,
            24.0,
            7.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Create PDF
        // ------------------------------------------------------------

        let page = PdfPage::new(Mm(210.0), Mm(297.0), ops);

        document.with_pages(vec![page]);

        let mut warnings = Vec::new();

        let bytes = document.save(&PdfSaveOptions::default(), &mut warnings);

        fs::write(&output_path, bytes)?;

        Ok(())
    }
}

#[derive(Clone, Copy)]
enum TextAlignment {
    Left,
    Centre,
}

fn write_text(
    ops: &mut Vec<Op>,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    bold: bool,
    alignment: TextAlignment,
    bold_font: &printpdf::FontId,
    regular_font: &printpdf::FontId,
) {
    let font = if bold {
        bold_font.clone()
    } else {
        regular_font.clone()
    };

    let text_width = approximate_text_width(text, size, bold);

    let adjusted_x = match alignment {
        TextAlignment::Left => x,
        TextAlignment::Centre => x - (text_width / 2.0),
    };

    ops.push(Op::StartTextSection);

    ops.push(Op::SetTextCursor {
        pos: Point {
            x: Mm(adjusted_x).into(),
            y: Mm(y).into(),
        },
    });

    ops.push(Op::SetFontSize {
        size: Pt(size),
        font: font.clone(),
    });

    ops.push(Op::WriteText {
        items: vec![TextItem::Text(text.to_string())],
        font,
    });

    ops.push(Op::EndTextSection);
}

fn approximate_text_width(text: &str, font_size: f32, bold: bool) -> f32 {
    let factor = if bold { 0.29 } else { 0.27 };

    text.chars().count() as f32 * font_size * factor
}

fn draw_box(ops: &mut Vec<Op>, x: f32, y: f32, width: f32, height: f32) {
    let rect = Rect {
        x: Mm(x).into(),
        y: Mm(y + height).into(),
        width: Mm(width).into(),
        height: Mm(height).into(),
    };

    ops.push(Op::DrawLine {
        line: rect.to_line(),
    });
}

fn display_table_value(value: &str) -> &str {
    if value.trim() == "0" {
        ""
    } else {
        value
    }
}

pub fn contracted_hours_summary(
    values: &[String; 4],
    week_commencing_dates: &[String; 4],
) -> String {
    if values.iter().all(|value| value == &values[0]) {
        return values[0].clone();
    }

    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for (value, date) in values.iter().zip(week_commencing_dates) {
        let compact_date = date
            .split_once('/')
            .and_then(|(day, remainder)| {
                remainder
                    .split_once('/')
                    .map(|(month, _)| format!("{day}/{month}"))
            })
            .unwrap_or_else(|| date.clone());
        if let Some((_, dates)) = groups
            .iter_mut()
            .find(|(existing_value, _)| existing_value == value)
        {
            dates.push(compact_date);
        } else {
            groups.push((value.clone(), vec![compact_date]));
        }
    }

    groups
        .into_iter()
        .map(|(value, dates)| format!("{} (w/c {})", value, dates.join(", ")))
        .collect::<Vec<_>>()
        .join("  ")
}

fn wrap_text(text: &str, max_width: f32, font_size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if !current.is_empty() && approximate_text_width(&candidate, font_size, false) > max_width {
            lines.push(current);
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn draw_table(
    ops: &mut Vec<Op>,
    x: f32,
    top: f32,
    header_height: f32,
    week_row_height: f32,
    columns: &[(&str, f32)],
    week_dates: &[&str; 4],
    hours_worked: &[&str; 4],
    annual_leave_hours: &[&str; 4],
    sick_leave_hours: &[&str; 4],
    public_holidays: &[Vec<PublicHolidayPdfEntry>; 4],
    travel_miles: &[&str; 4],
    previous_cycle_hours: Option<&str>,
    pdf_config: &crate::config::PdfConfig,
    bold_font: &printpdf::FontId,
    regular_font: &printpdf::FontId,
) {
    let (week_commencing_font_size, hours_font_size, information_font_size) =
        configured_table_font_sizes(pdf_config);
    let table_width: f32 = columns.iter().map(|(_, width)| *width).sum();

    let table_height = header_height + (week_row_height * 4.0);

    draw_box(ops, x, top - table_height, table_width, table_height);

    // ------------------------------------------------------------
    // Vertical lines
    // ------------------------------------------------------------

    let mut current_x = x;

    for (_, width) in columns {
        current_x += *width;

        if current_x < x + table_width {
            draw_vertical_line(ops, current_x, top - table_height, top);
        }
    }

    // ------------------------------------------------------------
    // Horizontal lines
    // ------------------------------------------------------------

    draw_horizontal_line(ops, x, x + table_width, top - header_height);

    for row in 1..4 {
        let y = top - header_height - (week_row_height * row as f32);

        draw_horizontal_line(ops, x, x + table_width, y);
    }

    // ------------------------------------------------------------
    // Headers
    // ------------------------------------------------------------

    let mut current_x = x;

    for (header, width) in columns {
        let header_lines: Vec<&str> = header.split('\n').collect();

        let line_height = 4.5;

        for (line_index, line) in header_lines.iter().enumerate() {
            write_text(
                ops,
                line,
                current_x + 1.5,
                top - 5.0 - (line_index as f32 * line_height),
                6.0,
                true,
                TextAlignment::Left,
                bold_font,
                regular_font,
            );
        }

        current_x += *width;
    }

    // ------------------------------------------------------------
    // Four payroll weeks
    // ------------------------------------------------------------

    for index in 0..4 {
        let row_top = top - header_height - (week_row_height * index as f32);

        // --------------------------------------------------------
        // W/c date
        // --------------------------------------------------------

        write_text(
            ops,
            week_dates[index],
            x + 1.5,
            row_top - 10.0,
            week_commencing_font_size,
            false,
            TextAlignment::Left,
            bold_font,
            regular_font,
        );

        // --------------------------------------------------------
        // Worked hours
        // --------------------------------------------------------

        write_text(
            ops,
            hours_worked[index],
            x + columns[0].1 + 1.5,
            row_top - 10.0,
            hours_font_size,
            true,
            TextAlignment::Left,
            bold_font,
            regular_font,
        );

        // --------------------------------------------------------
        // Previous-cycle information
        // --------------------------------------------------------

        if index == 0 {
            if let Some(previous) = previous_cycle_hours {
                if !previous.trim().is_empty() {
                    write_text(
                        ops,
                        &format!(
                            "(Info only {:+.2} hours)",
                            previous.parse::<f64>().unwrap_or(0.0)
                        ),
                        x + columns[0].1 + 1.5,
                        row_top - 14.5,
                        information_font_size,
                        false,
                        TextAlignment::Left,
                        bold_font,
                        regular_font,
                    );
                }
            }
        }

        // --------------------------------------------------------
        // Annual Leave
        // --------------------------------------------------------

        let annual_leave = display_table_value(annual_leave_hours[index]);

        if !annual_leave.is_empty() {
            write_text(
                ops,
                annual_leave,
                x + columns[0].1 + columns[1].1 + 7.0,
                row_top - 10.0,
                12.0,
                true,
                TextAlignment::Left,
                bold_font,
                regular_font,
            );
        }

        // --------------------------------------------------------
        // Sick / SSP
        // --------------------------------------------------------

        let sick_leave = display_table_value(sick_leave_hours[index]);

        if !sick_leave.is_empty() {
            write_text(
                ops,
                sick_leave,
                x + columns[0].1 + columns[1].1 + columns[2].1 + 7.0,
                row_top - 10.0,
                12.0,
                true,
                TextAlignment::Left,
                bold_font,
                regular_font,
            );
        }

        // --------------------------------------------------------
        // Public Holiday
        // --------------------------------------------------------

        let public_holiday_x = x + columns[0].1 + columns[1].1 + columns[2].1 + columns[3].1 + 1.5;
        let line_spacing = (hours_font_size * 0.42).max(4.5);
        for (line_index, entry) in public_holidays[index].iter().enumerate() {
            let public_holiday = display_table_value(&entry.hours);
            if public_holiday.is_empty() {
                continue;
            }
            let line_y = row_top - 7.0 - line_index as f32 * line_spacing;
            write_text(
                ops,
                public_holiday,
                public_holiday_x,
                line_y,
                hours_font_size,
                true,
                TextAlignment::Left,
                bold_font,
                regular_font,
            );

            if !entry.date.trim().is_empty() {
                let hours_width = approximate_text_width(public_holiday, hours_font_size, true);
                write_text(
                    ops,
                    &format!("({})", entry.date.trim()),
                    public_holiday_x + hours_width,
                    line_y,
                    information_font_size,
                    false,
                    TextAlignment::Left,
                    bold_font,
                    regular_font,
                );
            }
        }

        // --------------------------------------------------------
        // Travel miles
        // --------------------------------------------------------

        let travel = display_table_value(travel_miles[index]);

        if !travel.is_empty() {
            write_text(
                ops,
                travel,
                x + columns[0].1 + columns[1].1 + columns[2].1 + columns[3].1 + columns[4].1 + 7.0,
                row_top - 10.0,
                12.0,
                true,
                TextAlignment::Left,
                bold_font,
                regular_font,
            );
        }
    }
}

fn configured_table_font_sizes(pdf_config: &crate::config::PdfConfig) -> (f32, f32, f32) {
    (
        pdf_config.week_commencing_font_size as f32,
        pdf_config.hours_font_size as f32,
        pdf_config.information_font_size as f32,
    )
}

fn add_signature(
    document: &mut PdfDocument,
    ops: &mut Vec<Op>,
    path: &Path,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;

    let mut warnings = Vec::new();

    let image =
        printpdf::image::RawImage::decode_from_bytes(&bytes, &mut warnings).map_err(|error| {
            format!(
                "Could not decode signature image {}: {}",
                path.display(),
                error
            )
        })?;

    let image_width_px = image.width as f32;
    let image_height_px = image.height as f32;

    if image_width_px <= 0.0 || image_height_px <= 0.0 {
        return Err(format!("Signature image has invalid dimensions: {}", path.display()).into());
    }

    let image_aspect = image_width_px / image_height_px;
    let box_aspect = width / height;

    let (rendered_width, rendered_height) = if image_aspect > box_aspect {
        let rendered_width = width;
        let rendered_height = width / image_aspect;

        (rendered_width, rendered_height)
    } else {
        let rendered_height = height;
        let rendered_width = height * image_aspect;

        (rendered_width, rendered_height)
    };

    let placed_x = x + ((width - rendered_width) / 2.0);
    let placed_y = y + ((height - rendered_height) / 2.0);

    let image_id = document.add_image(&image);

    let scale_x = rendered_width / (image_width_px / 96.0 * 25.4);
    let scale_y = rendered_height / (image_height_px / 96.0 * 25.4);

    ops.push(Op::UseXobject {
        id: image_id,
        transform: XObjectTransform {
            translate_x: Some(Mm(placed_x).into()),
            translate_y: Some(Mm(placed_y).into()),
            scale_x: Some(scale_x),
            scale_y: Some(scale_y),
            dpi: Some(96.0),
            ..Default::default()
        },
    });

    Ok(())
}

fn draw_vertical_line(ops: &mut Vec<Op>, x: f32, bottom: f32, top: f32) {
    ops.push(Op::DrawLine {
        line: Line {
            points: vec![
                LinePoint {
                    p: Point {
                        x: Mm(x).into(),
                        y: Mm(bottom).into(),
                    },
                    bezier: false,
                },
                LinePoint {
                    p: Point {
                        x: Mm(x).into(),
                        y: Mm(top).into(),
                    },
                    bezier: false,
                },
            ],
            is_closed: false,
        },
    });
}

fn draw_horizontal_line(ops: &mut Vec<Op>, left: f32, right: f32, y: f32) {
    ops.push(Op::DrawLine {
        line: Line {
            points: vec![
                LinePoint {
                    p: Point {
                        x: Mm(left).into(),
                        y: Mm(y).into(),
                    },
                    bezier: false,
                },
                LinePoint {
                    p: Point {
                        x: Mm(right).into(),
                        y: Mm(y).into(),
                    },
                    bezier: false,
                },
            ],
            is_closed: false,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn schedule(
        first_week: &str,
        pay_date: &str,
    ) -> crate::payroll_schedule_repository::PayrollSchedule {
        crate::payroll_schedule_repository::PayrollSchedule {
            id: 1,
            payroll_year: "2026/27".to_string(),
            cycle_number: 1,
            first_week_commencing: first_week.to_string(),
            latest_posting_date: String::new(),
            pay_date: pay_date.to_string(),
            created_at: String::new(),
            payslips_sent: false,
        }
    }

    #[test]
    fn generates_four_week_timesheet_pdf() {
        let output_dir = env::temp_dir().join("direct_payment_timesheets_test");
        let schedule = schedule("23/03/2026", "17/04/2026");

        let mut data = TimesheetPdfData {
            schedule: &schedule,
            employer_name: "Morgan",
            personal_assistant_name: "Birch Sample",
            national_insurance_number: "AB123456C",
            contracted_weekly_hours: "25",
            week_commencing_dates: ["23/03/2026", "30/03/2026", "06/04/2026", "13/04/2026"],

            hours_worked: ["26.5", "21", "6.25", "0"],

            annual_leave_hours: ["0", "0", "0", "0"],

            sick_leave_hours: ["0", "0", "0", "0"],

            public_holidays: [
                vec![],
                vec![PublicHolidayPdfEntry {
                    hours: "7.5".to_string(),
                    date: "03/04/2026".to_string(),
                }],
                vec![],
                vec![],
            ],

            travel_miles: ["0", "0", "0", "0"],

            previous_cycle_hours: Some("1.25"),

            employer_signature_path: None,

            pa_signature_path: None,
        };

        let pdf_config = crate::config::PdfConfig::default();

        let result = PdfGenerator::generate(&output_dir, &data, &pdf_config);

        assert!(result.is_ok());

        let path = result.unwrap();

        assert!(path.exists());

        let extracted = pdf_extract::extract_text(&path).unwrap();
        let normalised = extracted.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(!normalised.contains("Total"));
        assert!(normalised.contains("26.5"));
        assert!(normalised.contains("21"));
        assert!(normalised.contains("6.25"));
        assert!(normalised.contains("(Info only +1.25 hours)"));
        assert!(normalised.contains("(03/04/2026)"));
        assert!(!normalised.contains('£'));
        assert!(!normalised.contains("from 01/04/2026"));
        assert!(!normalised.contains("see note"));
        assert!(!normalised.contains("historical work date"));
        assert!(normalised.contains("Contracted Weekly Hours: 25"));

        data.hours_worked = ["15.75", "0", "0", "0"];
        data.previous_cycle_hours = Some("-3.00");
        PdfGenerator::generate(&output_dir, &data, &pdf_config).unwrap();
        let text = pdf_extract::extract_text(&path)
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("15.75"));
        assert!(text.contains("(Info only -3.00 hours)"));
        assert!(!text.contains("12.75")); // Info only must not be deducted again.
        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn public_holidays_render_as_separate_dated_lines_without_combined_total() {
        let output_dir = env::temp_dir().join("direct_payment_timesheets_holiday_test");
        let schedule = schedule("21/12/2026", "15/01/2027");
        let data = TimesheetPdfData {
            schedule: &schedule,
            employer_name: "Test Employer",
            personal_assistant_name: "Holiday Test",
            national_insurance_number: "AB123456C",
            contracted_weekly_hours: "20",
            week_commencing_dates: ["21/12/2026", "28/12/2026", "04/01/2027", "11/01/2027"],
            hours_worked: ["8", "0", "0", "0"],
            annual_leave_hours: ["0", "0", "0", "0"],
            sick_leave_hours: ["0", "0", "0", "0"],
            public_holidays: [
                vec![
                    PublicHolidayPdfEntry {
                        hours: "6".to_string(),
                        date: "25/12/2026".to_string(),
                    },
                    PublicHolidayPdfEntry {
                        hours: "4".to_string(),
                        date: "26/12/2026".to_string(),
                    },
                ],
                vec![PublicHolidayPdfEntry {
                    hours: "0".to_string(),
                    date: "28/12/2026".to_string(),
                }],
                vec![],
                vec![],
            ],
            travel_miles: ["0", "0", "0", "0"],
            previous_cycle_hours: None,
            employer_signature_path: None,
            pa_signature_path: None,
        };

        let path = PdfGenerator::generate(&output_dir, &data, &crate::config::PdfConfig::default())
            .unwrap();
        let extracted = pdf_extract::extract_text(&path).unwrap();
        let tokens = extracted.split_whitespace().collect::<Vec<_>>();
        assert!(tokens.contains(&"8"));
        assert!(tokens.contains(&"6"));
        assert!(tokens.contains(&"4"));
        assert!(tokens.contains(&"(25/12/2026)"));
        assert!(tokens.contains(&"(26/12/2026)"));
        assert!(!tokens.contains(&"10"));
        assert!(!tokens.contains(&"18"));
        assert!(!tokens.contains(&"(28/12/2026)"));

        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn contracted_hours_summary_groups_week_dates_across_a_boundary() {
        let values = [
            "16".to_string(),
            "16".to_string(),
            "20".to_string(),
            "20".to_string(),
        ];
        let dates = [
            "10/08/2026".to_string(),
            "17/08/2026".to_string(),
            "24/08/2026".to_string(),
            "31/08/2026".to_string(),
        ];

        assert_eq!(
            contracted_hours_summary(&values, &dates),
            "16 (w/c 10/08, 17/08)  20 (w/c 24/08, 31/08)"
        );
        assert_eq!(
            contracted_hours_summary(&std::array::from_fn(|_| "25".to_string()), &dates),
            "25"
        );
    }

    #[test]
    fn four_week_pdf_displays_boundary_summary_without_altering_worked_hours() {
        let output_dir = env::temp_dir().join("direct_payment_timesheets_boundary_test");
        let summary = contracted_hours_summary(
            &[
                "16".to_string(),
                "16".to_string(),
                "20".to_string(),
                "20".to_string(),
            ],
            &[
                "10/08/2026".to_string(),
                "17/08/2026".to_string(),
                "24/08/2026".to_string(),
                "31/08/2026".to_string(),
            ],
        );
        let schedule = schedule("10/08/2026", "04/09/2026");
        let data = TimesheetPdfData {
            schedule: &schedule,
            employer_name: "Morgan",
            personal_assistant_name: "Boundary Test",
            national_insurance_number: "AB123456C",
            contracted_weekly_hours: &summary,
            week_commencing_dates: ["10/08/2026", "17/08/2026", "24/08/2026", "31/08/2026"],
            hours_worked: ["26.5", "21", "28.5", "6.25"],
            annual_leave_hours: ["0", "0", "0", "0"],
            sick_leave_hours: ["0", "0", "0", "0"],
            public_holidays: std::array::from_fn(|_| Vec::new()),
            travel_miles: ["0", "0", "0", "0"],
            previous_cycle_hours: None,
            employer_signature_path: None,
            pa_signature_path: None,
        };

        let path = PdfGenerator::generate(&output_dir, &data, &crate::config::PdfConfig::default())
            .unwrap();
        let extracted = pdf_extract::extract_text(&path).unwrap();
        let normalised = extracted.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(normalised.contains("16 (w/c 10/08, 17/08)"));
        assert!(normalised.contains("20 (w/c 24/08, 31/08)"));
        assert!(normalised.contains("26.5"));
        assert!(normalised.contains("21"));
        assert!(normalised.contains("28.5"));
        assert!(normalised.contains("6.25"));

        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn configured_table_font_sizes_keep_established_roles() {
        let mut config = crate::config::PdfConfig::default();
        config.week_commencing_font_size = 8.25;
        config.hours_font_size = 14.0;
        config.information_font_size = 5.25;

        assert_eq!(configured_table_font_sizes(&config), (8.25, 14.0, 5.25));
    }
}
