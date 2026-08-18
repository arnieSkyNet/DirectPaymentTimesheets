use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use printpdf::{
    graphics::{Line, LinePoint, Point},
    ops::{Op, PdfPage},
    units::{Mm, Pt},
    ParsedFont, PdfDocument, PdfSaveOptions, TextItem,
};

pub struct TimesheetPdfData<'a> {
    pub employer_name: &'a str,
    pub personal_assistant_name: &'a str,
    pub national_insurance_number: &'a str,
    pub contracted_weekly_hours: &'a str,
    pub pay_rate: f64,
    pub week_commencing_dates: [&'a str; 4],
    pub hours_worked: [&'a str; 4],
    pub previous_cycle_hours: Option<&'a str>,
}

pub struct PdfGenerator;

impl PdfGenerator {
    pub fn generate(
        output_dir: &Path,
        data: &TimesheetPdfData<'_>,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        fs::create_dir_all(output_dir)?;

        let filename = format!(
            "{}_timesheet_{}.pdf",
            sanitise_filename(data.personal_assistant_name),
            sanitise_filename(data.week_commencing_dates[0])
        );

        let output_path = output_dir.join(filename);

        let mut document = PdfDocument::new("Direct Payment Timesheet");

        // ------------------------------------------------------------
        // Load DejaVu Sans so the PDF can display the £ symbol correctly.
        // ------------------------------------------------------------

        let regular_bytes = fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")?;

        let bold_bytes = fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")?;

        let mut font_warnings = Vec::new();

        let regular_font = ParsedFont::from_bytes(&regular_bytes, 0, &mut font_warnings)
            .ok_or("Could not parse DejaVu Sans regular font")?;

        let bold_font = ParsedFont::from_bytes(&bold_bytes, 0, &mut font_warnings)
            .ok_or("Could not parse DejaVu Sans bold font")?;

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

        // Header is deliberately taller than the weekly rows so that
        // the four-line Travel Miles heading fits comfortably.
        let header_height = 24.0;

        // Weekly rows are taller than before to leave room for future
        // timesheet information.
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
            data.previous_cycle_hours,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Declaration / signatures
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

        // No boxes around signatures.
        // The actual signature images will occupy these areas later.

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
            55.0,
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
            55.0,
            8.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Create PDF page
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

    let text_width = approximate_text_width(text, size);

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

fn approximate_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.45
}

fn draw_box(ops: &mut Vec<Op>, x: f32, y: f32, width: f32, height: f32) {
    use printpdf::graphics::Rect;

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

fn draw_table(
    ops: &mut Vec<Op>,
    x: f32,
    top: f32,
    header_height: f32,
    week_row_height: f32,
    columns: &[(&str, f32)],
    week_dates: &[&str; 4],
    hours_worked: &[&str; 4],
    previous_cycle_hours: Option<&str>,
    bold_font: &printpdf::FontId,
    regular_font: &printpdf::FontId,
) {
    let table_width: f32 = columns.iter().map(|(_, width)| *width).sum();

    let table_height = header_height + (week_row_height * 4.0);

    // ------------------------------------------------------------
    // Outer table
    // ------------------------------------------------------------

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

    // Line immediately below the header.
    draw_horizontal_line(ops, x, x + table_width, top - header_height);

    // Lines between the four payroll weeks.
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

        // Keep all header text safely inside the header row.
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

    for (index, date) in week_dates.iter().enumerate() {
        // IMPORTANT:
        // The first weekly row begins BELOW the header line.
        let row_top = top - header_height - (week_row_height * index as f32);

        let y = row_top - 10.0;

        // W/c Date
        write_text(
            ops,
            date,
            x + 1.5,
            y,
            6.5,
            false,
            TextAlignment::Left,
            bold_font,
            regular_font,
        );

        // Hours Worked
        let hours_text = match (index, previous_cycle_hours) {
            (0, Some(previous)) if !previous.is_empty() => {
                format!("{} +{} prev", hours_worked[index], previous)
            }

            _ => hours_worked[index].to_string(),
        };

        // Column 2 begins after the first column.
        write_text(
            ops,
            &hours_text,
            x + 28.0 + 1.5,
            y,
            6.5,
            false,
            TextAlignment::Left,
            bold_font,
            regular_font,
        );

        // Remaining columns intentionally blank for now.
    }
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
            personal_assistant_name: "Andy Pandy",
            national_insurance_number: "AB123456C",
            contracted_weekly_hours: "25",
            pay_rate: 12.72,
            week_commencing_dates: ["23/03/2026", "20/04/2026", "18/05/2026", "15/06/2026"],
            hours_worked: ["25", "25", "25", "25"],
            previous_cycle_hours: Some("5.25"),
        };

        let result = PdfGenerator::generate(&output_dir, &data);

        assert!(result.is_ok());

        let path = result.unwrap();

        assert!(path.exists());

        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(output_dir);
    }
}
