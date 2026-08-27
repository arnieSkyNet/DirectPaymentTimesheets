use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Local};
use printpdf::{
    graphics::{Line, LinePoint, Point, Rect},
    ops::{Op, PdfPage},
    units::{Mm, Pt},
    ParsedFont, PdfDocument, PdfSaveOptions, TextItem, XObjectTransform,
};

pub struct TimesheetPdfData<'a> {
    pub employer_name: &'a str,
    pub personal_assistant_name: &'a str,
    pub national_insurance_number: &'a str,
    pub contracted_weekly_hours: &'a str,
    pub pay_rate: f64,

    pub week_commencing_dates: [&'a str; 4],
    pub hours_worked: [&'a str; 4],

    pub annual_leave_hours: [&'a str; 4],
    pub sick_leave_hours: [&'a str; 4],
    pub public_holiday_hours: [&'a str; 4],
    pub public_holiday_dates: [&'a str; 4],
    pub travel_miles: [&'a str; 4],

    pub previous_cycle_hours: Option<&'a str>,

    pub employer_signature_path: Option<&'a Path>,
    pub pa_signature_path: Option<&'a Path>,
}

pub struct PdfGenerator;

impl PdfGenerator {
    pub fn generate(
        output_dir: &Path,
        data: &TimesheetPdfData<'_>,
        pdf_config: &crate::config::PdfConfig,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        fs::create_dir_all(output_dir)?;

        let filename = format!(
            "Timesheet - {} - {}.pdf",
            sanitise_filename(data.personal_assistant_name),
            payroll_week_filename(data.week_commencing_dates[0])
        );

        let output_path = output_dir.join(filename);

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

        write_text(
            &mut ops,
            &format!(
                "Contracted Weekly Hours: {}     Pay Rate: £{:.2} per hour",
                data.contracted_weekly_hours, data.pay_rate
            ),
            20.0,
            235.0,
            9.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Main four-week table
        // ------------------------------------------------------------

        let table_x = 15.0;
        let table_top = 220.0;

        let header_height = 24.0;
        let week_row_height = 16.0;

        let columns = [
            ("W/c\nDate", 28.0),
            ("Hours\nworked\nPay rate £", 25.0),
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
            &data.public_holiday_hours,
            &data.public_holiday_dates,
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

        Ok(output_path)
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
    public_holiday_hours: &[&str; 4],
    public_holiday_dates: &[&str; 4],
    travel_miles: &[&str; 4],
    previous_cycle_hours: Option<&str>,
    pdf_config: &crate::config::PdfConfig,
    bold_font: &printpdf::FontId,
    regular_font: &printpdf::FontId,
) {
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
            pdf_config.week_commencing_font_size as f32,
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
            x + 28.0 + 1.5,
            row_top - 10.0,
            pdf_config.hours_font_size as f32,
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
                        &format!("(info.only +{} prev)", previous),
                        x + 28.0 + 1.5,
                        row_top - 14.5,
                        pdf_config.information_font_size as f32,
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
                x + 28.0 + 25.0 + 7.0,
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
                x + 28.0 + 50.0 + 7.0,
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

        let public_holiday = display_table_value(public_holiday_hours[index]);

        if !public_holiday.is_empty() {
            let public_holiday_x = x + 28.0 + 75.0 + 1.5;

            // Public Holiday hours.
            write_text(
                ops,
                public_holiday,
                public_holiday_x,
                row_top - 10.0,
                12.0,
                true,
                TextAlignment::Left,
                bold_font,
                regular_font,
            );

            // Public Holiday date uses the configurable
            // Information / Secondary Text size.
            let public_holiday_date = public_holiday_dates[index].trim();

            if !public_holiday_date.is_empty() {
                let hours_width = approximate_text_width(public_holiday, 12.0, true);

                let date_text = public_holiday_date
                    .split_once('(')
                    .and_then(|(_, date)| date.strip_suffix(')'))
                    .map(|date| format!("({})", date))
                    .unwrap_or_else(|| public_holiday_date.to_string());

                write_text(
                    ops,
                    &date_text,
                    public_holiday_x + hours_width,
                    row_top - 10.0,
                    pdf_config.information_font_size as f32,
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
                x + 28.0 + 100.0 + 7.0,
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

fn payroll_week_filename(date: &str) -> String {
    if let Ok(parsed) = chrono::NaiveDate::parse_from_str(date, "%d/%m/%Y") {
        let payroll_start = chrono::NaiveDate::from_ymd_opt(2026, 3, 23)
            .expect("Invalid payroll schedule start date");

        let days_since_start = (parsed - payroll_start).num_days();

        if days_since_start >= 0 && days_since_start % 7 == 0 {
            let week_number = 2 + (days_since_start / 7);

            return format!("{}{:02}w{:02}", parsed.year(), parsed.month(), week_number);
        }
    }

    sanitise_filename(date)
}

fn sanitise_filename(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => character,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn generates_four_week_timesheet_pdf() {
        let output_dir = env::temp_dir().join("direct_payment_timesheets_test");

        let data = TimesheetPdfData {
            employer_name: "Morgan",
            personal_assistant_name: "Birch Sample",
            national_insurance_number: "AB123456C",
            contracted_weekly_hours: "25",
            pay_rate: 12.72,

            week_commencing_dates: ["23/03/2026", "30/03/2026", "06/04/2026", "13/04/2026"],

            hours_worked: ["25", "25", "25", "25"],

            annual_leave_hours: ["0", "0", "0", "0"],

            sick_leave_hours: ["0", "0", "0", "0"],

            public_holiday_hours: ["0", "0", "0", "0"],

            public_holiday_dates: ["", "", "", ""],

            travel_miles: ["0", "0", "0", "0"],

            previous_cycle_hours: Some("5.25"),

            employer_signature_path: None,

            pa_signature_path: None,
        };

        let pdf_config = crate::config::PdfConfig::default();

        let result = PdfGenerator::generate(&output_dir, &data, &pdf_config);

        assert!(result.is_ok());

        let path = result.unwrap();

        assert!(path.exists());

        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn current_payroll_filename_is_correct() {
        assert_eq!(payroll_week_filename("13/07/2026"), "202607w18");
    }
}
