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
    pub sickness_periods: [Vec<[String; 2]>; 4],
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

/// Read-only PDF projection: retain complete dates in every overlapping week.
pub fn sickness_periods_for_weeks(
    repository: &crate::sickness_period_repository::SicknessPeriodRepository,
    pa_id: i64,
    weeks: &[chrono::NaiveDate; 4],
) -> Result<[Vec<[String; 2]>; 4], Box<dyn std::error::Error>> {
    let mut result = std::array::from_fn(|_| Vec::new());
    for (index, start) in weeks.iter().enumerate() {
        let end = *start + chrono::Duration::days(6);
        for period in repository.get_overlapping_for_pa(
            pa_id,
            &crate::date_utils::iso(*start),
            &crate::date_utils::iso(end),
        )? {
            result[index].push([
                format!("({} to", crate::date_utils::compact(&period.start_date)?),
                format!("{})", crate::date_utils::compact(&period.end_date)?),
            ]);
        }
    }
    Ok(result)
}

// Use the existing information font, with a small extra gap between periods.
fn sickness_line_offset(block: usize, line: usize, size: f32) -> f32 {
    let leading = (size * 1.2 * 25.4 / 72.0).max(2.4);
    3.0 + block as f32 * (2.0 * leading + 0.8) + line as f32 * leading
}

fn validate_sickness_layout(
    weeks: &[Vec<[String; 2]>; 4],
    size: f32,
    font: &ParsedFont,
) -> Result<(), String> {
    for (week, periods) in weeks.iter().enumerate() {
        for (block, lines) in periods.iter().enumerate() {
            for (line, text) in lines.iter().enumerate() {
                let (left, right) = footer_line_metrics(text, size, font)?;
                if !size.is_finite()
                    || size <= 0.0
                    || left < 0.0
                    || right * 25.4 / 72.0 > 22.0
                    || size * 25.4 / 72.0 > 3.0
                    || sickness_line_offset(block, line, size) > 18.5
                {
                    return Err(format!("Sickness dates for week {} do not fit in the Sick / SSP cell at the configured information font size. No dates have been omitted; PDF generation stopped.", week + 1));
                }
            }
        }
    }
    Ok(())
}

pub struct PdfGenerator;

fn load_pdf_font(path: &Path) -> Result<ParsedFont, Box<dyn std::error::Error>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let defaults = crate::config::PdfConfig::default();
            if error.kind() == std::io::ErrorKind::NotFound && path == defaults.regular_font {
                include_bytes!("../assets/fonts/DejaVuSans.ttf").to_vec()
            } else if error.kind() == std::io::ErrorKind::NotFound && path == defaults.bold_font {
                include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf").to_vec()
            } else {
                return Err(std::io::Error::new(
                    error.kind(),
                    format!(
                        "Could not read configured PDF font {}: {error}",
                        path.display()
                    ),
                )
                .into());
            }
        }
    };
    ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
        .ok_or_else(|| format!("Could not parse configured PDF font {}", path.display()).into())
}

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
        payroll_config: &crate::config::PayrollConfig,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        fs::create_dir_all(output_dir)?;

        let output_path = Self::output_path(output_dir, data)?;

        Self::generate_to_path(&output_path, data, pdf_config, payroll_config)?;
        Ok(output_path)
    }

    pub fn generate_to_path(
        output_path: &Path,
        data: &TimesheetPdfData<'_>,
        pdf_config: &crate::config::PdfConfig,
        payroll_config: &crate::config::PayrollConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for value in data.week_commencing_dates {
            crate::date_utils::parse_legacy(value)?;
        }
        for entry in data.public_holidays.iter().flatten() {
            if !entry.date.trim().is_empty() {
                crate::date_utils::parse_legacy(&entry.date)?;
            }
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not create payroll PDF output directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let mut document = PdfDocument::new("Direct Payment Timesheet");

        let regular_font = load_pdf_font(&pdf_config.regular_font)?;
        let bold_font = load_pdf_font(&pdf_config.bold_font)?;

        validate_sickness_layout(
            &data.sickness_periods,
            pdf_config.information_font_size as f32,
            &regular_font,
        )?;
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
            &data.sickness_periods,
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

        let signature_y = 90.0;

        write_text(
            &mut ops,
            "DECLARATION",
            20.0,
            107.0,
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
            97.0,
            8.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Employer signature
        // ------------------------------------------------------------

        let employer_signature_bottom = if let Some(path) = data.employer_signature_path {
            add_signature(&mut document, &mut ops, path, 20.0, 68.0, 55.25, 17.0)?
        } else {
            68.0
        };

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

        let pa_signature_bottom = if let Some(path) = data.pa_signature_path {
            add_signature(&mut document, &mut ops, path, 115.0, 68.0, 55.25, 17.0)?
        } else {
            68.0
        };

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

        let current_date = crate::date_utils::formal(Local::now().date_naive());

        write_text(
            &mut ops,
            &format!("Date: {}", current_date),
            20.0,
            employer_signature_bottom - 4.0,
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
            pa_signature_bottom - 4.0,
            8.0,
            false,
            TextAlignment::Left,
            &bold_id,
            &regular_id,
        );

        // ------------------------------------------------------------
        // Configurable organisation-specific footer; generic declaration stays above.
        write_footer(
            &mut ops,
            &regular_font,
            &regular_id,
            &bold_id,
            payroll_config,
        )?;

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

fn write_footer(
    ops: &mut Vec<Op>,
    regular_font: &ParsedFont,
    regular_id: &printpdf::FontId,
    bold_id: &printpdf::FontId,
    payroll_config: &crate::config::PayrollConfig,
) -> Result<(), String> {
    let footer_size =
        crate::config::normalise_footer_font_size(payroll_config.timesheet_footer_font_size) as f32;
    let footer_lines = layout_footer(
        &payroll_config.timesheet_footer_text,
        footer_size,
        regular_font,
    )?;
    for line in footer_lines.iter().filter(|line| !line.text.is_empty()) {
        write_text(
            ops,
            &line.text,
            20.0 - footer_line_metrics(&line.text, footer_size, regular_font)?.0 * 25.4 / 72.0,
            line.baseline,
            footer_size,
            false,
            TextAlignment::Left,
            bold_id,
            regular_id,
        );
    }

    Ok(())
}

// Match printpdf's unkerned, integer-normalized hmtx advances, including spaces
// and glyphs with no outlines. Retain ink overhangs in the measured bounds.
fn footer_line_metrics(text: &str, size: f32, font: &ParsedFont) -> Result<(f32, f32), String> {
    let scale = size / font.font_metrics.units_per_em as f32;
    let mut cursor = 0.0_f32;
    let mut left = 0.0_f32;
    let mut right = 0.0_f32;
    let metrics_count = font
        .hhea_table
        .as_ref()
        .map(|table| table.num_h_metrics as usize)
        .filter(|count| *count > 0)
        .ok_or("Payroll-timesheet footer: configured regular font has no horizontal metrics.")?;
    for ch in text.chars() {
        let glyph = font.lookup_glyph_index(ch as u32).ok_or_else(|| format!("Payroll-timesheet footer contains a character the configured regular font cannot render: {ch:?}. Choose supported text."))?;
        let offset = (glyph as usize).min(metrics_count - 1) * 4;
        let bytes = font
            .hmtx_data
            .get(offset..offset + 2)
            .ok_or("Payroll-timesheet footer: incomplete regular font metrics.")?;
        let advance = u16::from_be_bytes([bytes[0], bytes[1]]) as f32;
        if let Some(record) = font.glyph_records_decoded.get(&glyph) {
            left = left.min(cursor + record.bounding_box.min_x as f32 * scale);
            right = right.max(cursor + record.bounding_box.max_x as f32 * scale);
        }
        cursor +=
            (advance * 1000.0 / font.font_metrics.units_per_em as f32).floor() * size / 1000.0;
    }
    Ok((left, right.max(cursor)))
}

#[derive(Debug)]
struct FooterLine {
    text: String,
    baseline: f32,
}

fn footer_line_spacing(size: f32, font: &ParsedFont) -> f32 {
    (size * 1.25).max(
        font.font_metrics.get_ascender(size) - font.font_metrics.get_descender(size)
            + font.font_metrics.get_line_gap(size),
    ) * 25.4
        / 72.0
}

// Footer: x=20..190 mm, y=10..54 mm, first baseline 48 mm. Signature dates
// are >=64 mm (4 mm below the actual image), leaving a safe gap above the footer.
// Normal lines use font-aware leading (at least 1.25 em); each explicit empty
// line adds 1.5 normal advances. Blank paragraphs therefore have visibly larger
// spacing, and multiple/trailing blank lines consume proportional height.
fn layout_footer(text: &str, size: f32, font: &ParsedFont) -> Result<Vec<FooterLine>, String> {
    let overflow = || {
        format!("Payroll-timesheet footer text does not fit at {size} pt. Shorten the text or select a smaller font size.")
    };
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let max_width = 170.0 * 72.0 / 25.4;
    let width =
        |text: &str| footer_line_metrics(text, size, font).map(|(left, right)| right - left);
    let mut lines = Vec::new();
    let leading = footer_line_spacing(size, font);
    let mut baseline = 48.0;
    let top = baseline + font.font_metrics.get_y_max(size) * 25.4 / 72.0;
    if top > 54.0 {
        return Err(overflow());
    }
    let mut push_line = |text: &str, advance: f32| -> Result<(), String> {
        // Blank lines reserve their entire advance, including trailing blanks.
        let bottom = if text.is_empty() {
            baseline - advance
        } else {
            baseline + font.font_metrics.get_y_min(size) * 25.4 / 72.0
        };
        if bottom < 10.0 {
            return Err(overflow());
        }
        lines.push(FooterLine {
            text: text.into(),
            baseline,
        });
        baseline -= advance;
        Ok(())
    };
    // split, not lines(): retain empty paragraphs and a trailing blank line.
    for explicit_line in text.split('\n') {
        let explicit_line = explicit_line.strip_suffix('\r').unwrap_or(explicit_line);
        if explicit_line.is_empty() {
            push_line("", leading * 1.5)?;
            continue;
        }
        let mut remaining = explicit_line;
        loop {
            if width(remaining)? <= max_width {
                push_line(remaining, leading)?;
                break;
            }
            // Wrap at an existing space without discarding or collapsing it.
            let mut last_break = None;
            for (offset, ch) in remaining.char_indices() {
                if ch == ' ' {
                    let end = offset + ch.len_utf8();
                    if width(&remaining[..end])? <= max_width {
                        last_break = Some(end);
                    } else {
                        break;
                    }
                }
            }
            let end = last_break.ok_or_else(overflow)?;
            push_line(&remaining[..end], leading)?;
            remaining = &remaining[end..];
        }
    }
    Ok(lines)
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
        let compact_date = crate::date_utils::parse_legacy(date)
            .map(|date| date.format("%d/%m").to_string())
            .unwrap_or_else(|_| "Invalid date".into());
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
    sickness_periods: &[Vec<[String; 2]>; 4],
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
            &crate::date_utils::compact(week_dates[index])
                .unwrap_or_else(|_| "Invalid date".into()),
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

        let sickness_x = x + columns[0].1 + columns[1].1 + columns[2].1 + 1.5;
        for (block, lines) in sickness_periods[index].iter().enumerate() {
            for (line, text) in lines.iter().enumerate() {
                write_text(
                    ops,
                    text,
                    sickness_x,
                    row_top - sickness_line_offset(block, line, information_font_size),
                    information_font_size,
                    false,
                    TextAlignment::Left,
                    bold_font,
                    regular_font,
                );
            }
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
                    &format!(
                        "({})",
                        crate::date_utils::compact(&entry.date)
                            .unwrap_or_else(|_| "Invalid date".into())
                    ),
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
) -> Result<f32, Box<dyn std::error::Error>> {
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

    Ok(placed_y)
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

    fn footer_font() -> ParsedFont {
        load_pdf_font(&crate::config::PdfConfig::default().regular_font).unwrap()
    }

    #[test]
    fn footer_preserves_explicit_lines_spaces_and_blank_lines() {
        let font = footer_font();
        assert_eq!(
            layout_footer("First  paragraph\n\nSecond paragraph", 7.0, &font)
                .unwrap()
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["First  paragraph", "", "Second paragraph"]
        );
        assert_eq!(
            layout_footer("First\n", 7.0, &font)
                .unwrap()
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["First", ""]
        );
        assert_eq!(
            layout_footer("First\r\n\r\nSecond", 7.0, &font)
                .unwrap()
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["First", "", "Second"]
        );
        assert!(layout_footer("", 10.0, &font).unwrap().is_empty());
        assert!(layout_footer(&"\n".repeat(30), 7.0, &font).is_err());
    }

    #[test]
    fn footer_paragraph_gaps_are_larger_than_single_and_wrapped_line_breaks() {
        let font = footer_font();
        for size in [7.0, 10.0, 11.0, 12.0] {
            let single = layout_footer("One\nTwo", size, &font).unwrap();
            let paragraph = layout_footer("One\n\nTwo", size, &font).unwrap();
            let multiple = layout_footer("One\n\n\nTwo", size, &font).unwrap();
            let wrapped = layout_footer(&"Payroll instructions ".repeat(12), size, &font).unwrap();
            let normal = single[0].baseline - single[1].baseline;
            assert!((normal - (wrapped[0].baseline - wrapped[1].baseline)).abs() < 0.001);
            assert!((paragraph[0].baseline - paragraph[2].baseline - 2.5 * normal).abs() < 0.001);
            assert!((multiple[0].baseline - multiple[3].baseline - 4.0 * normal).abs() < 0.001);
            assert_eq!(
                single
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>(),
                ["One", "Two"]
            );
        }
    }

    #[test]
    fn footer_wraps_measured_text_and_rejects_overflow_without_shrinking() {
        let font = footer_font();
        let text = "Payroll instructions ".repeat(12);
        let lines = layout_footer(&text, 7.0, &font).unwrap();
        assert!(lines.len() > 1);
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<String>(),
            text
        );
        let size_sensitive_word = "W".repeat(60);
        assert!(layout_footer(&size_sensitive_word, 6.0, &font).is_ok());
        assert!(layout_footer(&size_sensitive_word, 10.0, &font).is_err());
        for text in [
            "W".repeat(500),
            "line\n".repeat(30),
            "Payroll instructions ".repeat(500),
        ] {
            let error = layout_footer(&text, 10.0, &font).unwrap_err();
            assert!(error.contains("does not fit at 10 pt"));
            assert!(error.contains("Shorten the text or select a smaller font size"));
        }
    }

    #[test]
    fn footer_operations_keep_legacy_wording_regular_font_and_exact_selected_size() {
        let font = footer_font();
        let mut document = PdfDocument::new("test");
        let regular = document.add_font(&font);
        let bold = printpdf::FontId("unused-bold".into());
        let mut config = crate::config::PayrollConfig::default();
        let legacy: Vec<_> = config
            .timesheet_footer_text
            .split('\n')
            .map(str::to_string)
            .collect();
        for size in [6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 99.0] {
            config.timesheet_footer_font_size = size;
            let mut ops = Vec::new();
            write_footer(&mut ops, &font, &regular, &bold, &config).unwrap();
            let rendered: Vec<_> = ops
                .iter()
                .filter_map(|op| match op {
                    Op::WriteText { items, font } => {
                        assert_eq!(font, &regular);
                        Some(
                            items
                                .iter()
                                .filter_map(|item| match item {
                                    TextItem::Text(text) => Some(text.as_str()),
                                    _ => None,
                                })
                                .collect::<String>(),
                        )
                    }
                    _ => None,
                })
                .collect();
            if size == 7.0 || size == 99.0 {
                assert_eq!(rendered, legacy);
                let ys: Vec<_> = ops
                    .iter()
                    .filter_map(|op| match op {
                        Op::SetTextCursor { pos } => Some(pos.y.0),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    ys,
                    [
                        Pt::from(Mm(48.0)).0,
                        Pt::from(Mm(48.0 - footer_line_spacing(7.0, &font))).0
                    ]
                );
            }
            for op in &ops {
                if let Op::SetFontSize { size: actual, .. } = op {
                    assert_eq!(
                        actual.0,
                        crate::config::normalise_footer_font_size(size) as f32
                    );
                }
            }
        }
        config.timesheet_footer_text = "W".repeat(500);
        let mut rejected_ops = Vec::new();
        assert!(write_footer(&mut rejected_ops, &font, &regular, &bold, &config).is_err());
        assert!(rejected_ops.is_empty());
        config.timesheet_footer_text.clear();
        let mut ops = Vec::new();
        write_footer(&mut ops, &font, &regular, &bold, &config).unwrap();
        assert!(ops.is_empty());
    }

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
    fn sickness_projection_is_read_only_and_preserves_full_cross_week_dates() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        connection.execute_batch("INSERT INTO personal_assistants (id, first_name, surname) VALUES (1, 'Test', 'PA'), (2, 'Other', 'PA');").unwrap();
        let repo = crate::sickness_period_repository::SicknessPeriodRepository::new(connection);
        let start = chrono::NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        let weeks = std::array::from_fn(|i| start + chrono::Duration::days(i as i64 * 7));
        assert!(sickness_periods_for_weeks(&repo, 1, &weeks)
            .unwrap()
            .iter()
            .all(Vec::is_empty));
        repo.insert(1, "2026-09-01", "2026-09-02").unwrap();
        let one = sickness_periods_for_weeks(&repo, 1, &weeks).unwrap();
        assert_eq!(
            one[0],
            vec![["(01/09/2026 to".to_string(), "02/09/2026)".to_string()]]
        );
        repo.insert(1, "2026-09-05", "2026-09-08").unwrap();
        repo.insert(2, "2026-09-01", "2026-09-20").unwrap();
        let before = repo.get_for_pa(1).unwrap();
        let blocks = sickness_periods_for_weeks(&repo, 1, &weeks).unwrap();
        assert_eq!(blocks[0].len(), 2);
        assert_eq!(blocks[0][1], ["(05/09/2026 to", "08/09/2026)"]);
        assert_eq!(blocks[1], vec![blocks[0][1].clone()]);
        assert!(blocks[2].is_empty() && blocks[3].is_empty());
        assert_eq!(repo.get_for_pa(1).unwrap(), before);
    }

    #[test]
    fn sickness_pdf_writes_separate_lines_only_in_populated_cells() {
        let config = crate::config::PdfConfig::default();
        let mut document = PdfDocument::new("test");
        let regular = document.add_font(&footer_font());
        let bold = regular.clone();
        let blocks = [
            vec![
                ["(01/09/2026 to".into(), "02/09/2026)".into()],
                ["(05/09/2026 to".into(), "08/09/2026)".into()],
            ],
            vec![["(05/09/2026 to".into(), "08/09/2026)".into()]],
            vec![],
            vec![],
        ];
        validate_sickness_layout(&blocks, config.information_font_size as f32, &footer_font())
            .unwrap();
        let mut ops = Vec::new();
        draw_table(
            &mut ops,
            15.0,
            220.0,
            24.0,
            20.0,
            &[
                ("W/c", 28.0),
                ("Worked", 42.0),
                ("Leave", 25.0),
                ("Sick", 25.0),
                ("Holiday", 25.0),
                ("Miles", 25.0),
            ],
            &["31/08/2026", "07/09/2026", "14/09/2026", "21/09/2026"],
            &[""; 4],
            &[""; 4],
            &blocks,
            &std::array::from_fn(|_| Vec::new()),
            &[""; 4],
            None,
            &config,
            &bold,
            &regular,
        );
        let mut cursor = None;
        let mut size = None;
        let mut sickness = Vec::new();
        for op in &ops {
            match op {
                Op::SetTextCursor { pos } => cursor = Some(*pos),
                Op::SetFontSize { size: current, .. } => size = Some(current.0),
                Op::WriteText { items, .. } => {
                    for item in items {
                        if let TextItem::Text(text) = item {
                            let pos = cursor.unwrap();
                            // Sick column's fixed left inset, excluding its header.
                            if (pos.x.0 - Pt::from(Mm(111.5)).0).abs() < 0.01
                                && pos.y.0 < Pt::from(Mm(196.0)).0
                            {
                                sickness.push((text.clone(), pos.y.0, size.unwrap()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        assert_eq!(
            sickness.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
            [
                "(01/09/2026 to",
                "02/09/2026)",
                "(05/09/2026 to",
                "08/09/2026)",
                "(05/09/2026 to",
                "08/09/2026)"
            ]
        );
        assert!(sickness
            .iter()
            .all(|v| v.2 == config.information_font_size as f32));
        assert!(sickness.windows(2).all(|pair| pair[0].1 > pair[1].1));
        assert!(sickness.iter().all(|v| v.1 > Pt::from(Mm(156.0)).0));
        document.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sickness.pdf");
        fs::write(
            &path,
            document.save(&PdfSaveOptions::default(), &mut Vec::new()),
        )
        .unwrap();
        let text = pdf_extract::extract_text(&path).unwrap();
        assert_eq!(text.matches("(01/09/2026 to").count(), 1);
        assert_eq!(text.matches("(05/09/2026 to").count(), 2);
        assert_eq!(text.matches("08/09/2026)").count(), 2);
    }

    #[test]
    fn sickness_overflow_is_rejected_without_changing_font_or_layout() {
        let font = footer_font();
        let block = ["(01/09/2026 to".to_string(), "02/09/2026)".to_string()];
        let mut weeks = std::array::from_fn(|_| Vec::new());
        weeks[0] = vec![block.clone(); 3];
        assert!(validate_sickness_layout(&weeks, 5.5, &font).is_ok());
        weeks[0] = vec![block; 20];
        assert!(validate_sickness_layout(&weeks, 5.5, &font)
            .unwrap_err()
            .contains("week 1"));
    }

    #[test]
    fn generates_four_week_timesheet_pdf() {
        let output_dir = env::temp_dir().join("direct_payment_timesheets_test");
        let schedule = schedule("05/04/2032", "30/04/2032");

        let mut data = TimesheetPdfData {
            schedule: &schedule,
            employer_name: "Robin Placeholder",
            personal_assistant_name: "Birch Sample",
            national_insurance_number: "AB123456C",
            contracted_weekly_hours: "16",
            week_commencing_dates: ["2032-04-05", "12 Apr 2032", "19/04/2032", "26/04/2032"],

            hours_worked: ["14.75", "18.5", "9.25", "0"],

            annual_leave_hours: ["0", "0", "0", "0"],

            sickness_periods: std::array::from_fn(|_| Vec::new()),

            public_holidays: [
                vec![],
                vec![PublicHolidayPdfEntry {
                    hours: "4.25".to_string(),
                    date: "2032-04-16".to_string(),
                }],
                vec![],
                vec![],
            ],

            travel_miles: ["0", "0", "0", "0"],

            previous_cycle_hours: Some("2.75"),

            employer_signature_path: None,

            pa_signature_path: None,
        };

        let pdf_config = crate::config::PdfConfig::default();

        let result = PdfGenerator::generate(
            &output_dir,
            &data,
            &pdf_config,
            &crate::config::PayrollConfig::default(),
        );

        assert!(result.is_ok());

        let path = result.unwrap();

        assert!(path.exists());

        let extracted = pdf_extract::extract_text(&path).unwrap();
        let normalised = extracted.split_whitespace().collect::<Vec<_>>().join(" ");
        for sentence in crate::config::default_timesheet_footer_text().split('\n') {
            assert!(normalised.contains(sentence));
        }
        assert!(normalised.contains("DECLARATION"));
        assert!(normalised
            .contains("I confirm that the hours and information recorded above are correct."));
        let original_pdf = fs::read(&path).unwrap();
        let mut overflowing = crate::config::PayrollConfig::default();
        overflowing.timesheet_footer_text = "Too many lines\n".repeat(20);
        let error =
            PdfGenerator::generate_to_path(&path, &data, &pdf_config, &overflowing).unwrap_err();
        assert!(error
            .to_string()
            .contains("footer text does not fit at 7 pt"));
        assert_eq!(fs::read(&path).unwrap(), original_pdf);

        assert!(!normalised.contains("Total"));
        assert!(normalised.contains("05/04/2032"));
        assert!(normalised.contains("12/04/2032"));
        assert!(normalised.contains(&format!(
            "Date: {}",
            crate::date_utils::formal(Local::now().date_naive())
        )));
        assert!(normalised.contains("14.75"));
        assert!(normalised.contains("18.5"));
        assert!(normalised.contains("9.25"));
        assert!(normalised.contains("(Info only +2.75 hours)"));
        assert!(normalised.contains("(16/04/2032)"));
        assert!(!normalised.contains('£'));
        assert!(!normalised.contains("from 01/04/2032"));
        assert!(!normalised.contains("see note"));
        assert!(!normalised.contains("historical work date"));
        assert!(normalised.contains("Contracted Weekly Hours: 16"));

        data.hours_worked = ["15.75", "0", "0", "0"];
        data.previous_cycle_hours = Some("-3.00");
        PdfGenerator::generate(
            &output_dir,
            &data,
            &pdf_config,
            &crate::config::PayrollConfig::default(),
        )
        .unwrap();
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
            sickness_periods: std::array::from_fn(|_| Vec::new()),
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

        let path = PdfGenerator::generate(
            &output_dir,
            &data,
            &crate::config::PdfConfig::default(),
            &crate::config::PayrollConfig::default(),
        )
        .unwrap();
        let extracted = pdf_extract::extract_text(&path).unwrap();
        // Signature dates now contain a standalone day number, which may
        // legitimately equal the forbidden combined payroll totals (10/18).
        let (table_text, _) = extracted
            .split_once("DECLARATION")
            .expect("declaration follows payroll table");
        let tokens = table_text.split_whitespace().collect::<Vec<_>>();
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
            employer_name: "Robin Placeholder",
            personal_assistant_name: "Boundary Test",
            national_insurance_number: "AB123456C",
            contracted_weekly_hours: &summary,
            week_commencing_dates: ["10/08/2026", "17/08/2026", "24/08/2026", "31/08/2026"],
            hours_worked: ["11.75", "19.5", "13.25", "8.75"],
            annual_leave_hours: ["0", "0", "0", "0"],
            sickness_periods: std::array::from_fn(|_| Vec::new()),
            public_holidays: std::array::from_fn(|_| Vec::new()),
            travel_miles: ["0", "0", "0", "0"],
            previous_cycle_hours: None,
            employer_signature_path: None,
            pa_signature_path: None,
        };

        let path = PdfGenerator::generate(
            &output_dir,
            &data,
            &crate::config::PdfConfig::default(),
            &crate::config::PayrollConfig::default(),
        )
        .unwrap();
        let extracted = pdf_extract::extract_text(&path).unwrap();
        let normalised = extracted.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(normalised.contains("16 (w/c 10/08, 17/08)"));
        assert!(normalised.contains("20 (w/c 24/08, 31/08)"));
        assert!(normalised.contains("11.75"));
        assert!(normalised.contains("19.5"));
        assert!(normalised.contains("13.25"));
        assert!(normalised.contains("8.75"));

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
