use chrono::NaiveDateTime;
use csv::ReaderBuilder;
use std::error::Error;

use crate::models::TimesheetEntry;

/// Persisted import interpretation. Introductory rows are physical lines before
/// the CSV header; quoted multiline data records are parsed by the CSV reader.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CsvImportConfig {
    pub introductory_rows: usize,
    pub hours_method: HoursMethod,
    /// Aliases by semantic field: PA, start, end, break, worked, rate, amount, note.
    pub headings: [Vec<String>; 8],
    /// Disabled fields retain aliases but are ignored when interpreting new CSV.
    pub enabled: [bool; 8],
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoursMethod {
    #[default]
    WorkedHours,
    WorkedHoursMinusBreak,
    EndMinusStart,
    EndMinusStartMinusBreak,
}
impl Default for CsvImportConfig {
    fn default() -> Self {
        Self {
            enabled: [true; 8],
            introductory_rows: 1,
            hours_method: HoursMethod::WorkedHours,
            headings: [
                vec!["Client Name".into(), "PA".into()],
                vec!["Start Time".into(), "Start".into()],
                vec!["End Time".into(), "End".into()],
                vec!["Break Time".into(), "Break".into()],
                vec!["Worked Hours".into(), "Worked".into()],
                vec!["Rate/h".into(), "Rate".into()],
                vec!["Amount".into()],
                vec!["Note".into()],
            ],
        }
    }
}
fn normal_heading(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('\u{feff}')
        .trim()
        .to_lowercase()
}
impl HoursMethod {
    pub const ALL: [Self; 4] = [
        Self::WorkedHours,
        Self::WorkedHoursMinusBreak,
        Self::EndMinusStart,
        Self::EndMinusStartMinusBreak,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::WorkedHours => "CSV Worked Hours",
            Self::WorkedHoursMinusBreak => "CSV Worked Hours minus Break",
            Self::EndMinusStart => "End minus Start (no break deduction)",
            Self::EndMinusStartMinusBreak => "End minus Start minus Break",
        }
    }
    pub fn uses_worked_hours(self) -> bool {
        matches!(self, Self::WorkedHours | Self::WorkedHoursMinusBreak)
    }
    pub fn deducts_break(self) -> bool {
        matches!(
            self,
            Self::WorkedHoursMinusBreak | Self::EndMinusStartMinusBreak
        )
    }
    pub fn requires(self, field: usize) -> bool {
        field < 3
            || (field == 3 && self.deducts_break())
            || (field == 4 && self.uses_worked_hours())
    }
}
pub const FIELD_NAMES: [&str; 8] = [
    "PA",
    "Start",
    "End",
    "Break",
    "Worked Hours",
    "Rate",
    "Amount",
    "Note",
];
impl CsvImportConfig {
    /// UI method changes enable mandatory fields without discarding any aliases.
    pub fn require_method_fields(&mut self) {
        for field in 0..8 {
            if self.hours_method.requires(field) {
                self.enabled[field] = true;
            }
        }
    }
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        let mut seen = std::collections::HashSet::new();
        for (field, aliases) in self.headings.iter().enumerate() {
            if self.hours_method.requires(field) && (!self.enabled[field] || aliases.is_empty()) {
                return Err(format!(
                    "CSV required field {} must be enabled and have a heading mapping",
                    FIELD_NAMES[field]
                )
                .into());
            }
            if !self.enabled[field] {
                continue;
            }
            for alias in aliases {
                let key = normal_heading(alias);
                if key.is_empty() || !seen.insert(key) {
                    return Err(
                        "CSV heading mappings must be nonempty and unique among enabled fields"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }
}

/// New imports only. The original parser below remains the legacy archive reader.
pub fn import_csv_with_config(
    bytes: &[u8],
    config: &CsvImportConfig,
) -> Result<Vec<ParsedTimesheetRow>, Box<dyn Error>> {
    config.validate()?;
    let text = std::str::from_utf8(bytes).map_err(|e| format!("CSV is not valid UTF-8: {e}"))?;
    let mut body = text;
    for _ in 0..config.introductory_rows {
        let end = body
            .find('\n')
            .ok_or("CSV introductory rows extend past the header")?;
        body = &body[end + 1..];
    }
    let offset = u64::try_from(config.introductory_rows)?;
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(body.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| configured_csv_error(e, body, offset))?
        .clone();
    if headers.is_empty() {
        return Err("CSV header is missing".into());
    }
    let mut columns = [None; 8];
    for (column, heading) in headers.iter().enumerate() {
        let key = normal_heading(heading);
        if let Some(field) = config
            .headings
            .iter()
            .enumerate()
            .position(|(field, aliases)| {
                config.enabled[field] && aliases.iter().any(|a| normal_heading(a) == key)
            })
        {
            if columns[field].replace(column).is_some() {
                return Err(format!("Invalid CSV header: duplicate field {heading:?}").into());
            }
        }
    }
    for field in (0..8).filter(|&f| config.hours_method.requires(f)) {
        if columns[field].is_none() {
            return Err(format!(
                "Invalid CSV header: missing required field {}",
                FIELD_NAMES[field]
            )
            .into());
        }
    }
    let mut rows = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| configured_csv_error(e, body, offset))?;
        let source_row = record
            .position()
            .map_or(0, |p| physical_row(body, p.byte(), offset));
        let value = |field: usize| {
            columns[field]
                .and_then(|c| record.get(c))
                .unwrap_or("")
                .trim()
        };
        let pa_name = value(0).to_string();
        if pa_name.is_empty() {
            return Err(format!("Invalid CSV row {source_row}: missing PA name").into());
        }
        let start = parse_supported_timestamp(value(1))
            .ok_or_else(|| format!("Invalid CSV row {source_row} start timestamp"))?;
        let end = parse_supported_timestamp(value(2))
            .ok_or_else(|| format!("Invalid CSV row {source_row} end timestamp"))?;
        if end <= start {
            return Err(format!(
                "Invalid CSV row {source_row}: end timestamp must be later than start timestamp"
            )
            .into());
        }
        let duration = |field: usize| -> Result<i64, Box<dyn Error>> {
            if value(field).is_empty() {
                return Ok(0);
            }
            parse_duration(value(field))
                .map_err(|e| format!("Invalid CSV row {source_row} duration: {e}").into())
        };
        let money = |field: usize| -> Result<f64, Box<dyn Error>> {
            if value(field).is_empty() {
                return Ok(0.0);
            }
            parse_money(value(field))
                .map_err(|e| format!("Invalid CSV row {source_row} monetary value: {e}").into())
        };
        let elapsed = (end - start).num_minutes();
        let base_minutes = if config.hours_method.uses_worked_hours() {
            if value(4).is_empty() {
                return Err(format!("Invalid CSV row {source_row}: missing Worked Hours").into());
            }
            duration(4)?
        } else {
            elapsed
        };
        if config.hours_method.deducts_break() && value(3).is_empty() {
            return Err(
                format!("Invalid CSV row {source_row}: missing required Break value").into(),
            );
        }
        let break_minutes = duration(3)?;
        let worked_minutes = if config.hours_method.deducts_break() {
            if break_minutes > elapsed || break_minutes > base_minutes {
                return Err(format!("Invalid CSV row {source_row}: Break exceeds shift duration or selected hours; negative calculated durations are not allowed").into());
            }
            base_minutes
                .checked_sub(break_minutes)
                .filter(|v| *v >= 0)
                .ok_or_else(|| {
                    format!("Invalid CSV row {source_row}: negative calculated duration")
                })?
        } else {
            base_minutes
        };
        // No import rounding: preparation applies configured rounding once per shift.
        rows.push(ParsedTimesheetRow {
            source_row,
            start,
            end,
            supplied_fields: std::array::from_fn(|f| !value(f).is_empty()),
            entry: TimesheetEntry {
                id: 0,
                pa_name,
                personal_assistant_id: None,
                start_time: value(1).into(),
                end_time: value(2).into(),
                break_minutes,
                worked_minutes,
                hourly_rate: money(5)?,
                amount: money(6)?,
                notes: (!value(7).is_empty()).then(|| value(7).to_string()),
            },
        });
    }
    Ok(rows)
}

// csv-core can position a CRLF record on the preceding LF. Count physical
// newlines from byte positions rather than treating that separator as data.
fn physical_row(body: &str, byte: u64, offset: u64) -> u64 {
    let bytes = body.as_bytes();
    let mut start = usize::try_from(byte)
        .unwrap_or(bytes.len())
        .min(bytes.len());
    while start < bytes.len() && matches!(bytes[start], b'\r' | b'\n') {
        start += 1;
    }
    offset + 1 + bytes[..start].iter().filter(|b| **b == b'\n').count() as u64
}
fn configured_csv_error(error: csv::Error, body: &str, offset: u64) -> Box<dyn Error> {
    match error.position() {
        Some(p) => format!(
            "Invalid CSV structure at physical row {}: {error}",
            physical_row(body, p.byte(), offset)
        )
        .into(),
        None => format!("Invalid CSV structure: {error}").into(),
    }
}
#[derive(Debug, Clone)]
pub struct ParsedTimesheetRow {
    pub source_row: u64,
    pub supplied_fields: [bool; 8],
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub entry: TimesheetEntry,
}

/// Original eight-field interpretation, used only to reconstruct legacy archives.
pub fn import_csv_bytes(bytes: &[u8]) -> Result<Vec<ParsedTimesheetRow>, Box<dyn Error>> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        format!(
            "CSV is not valid UTF-8 at byte {}: {error}",
            error.valid_up_to()
        )
    })?;
    let first_line_end = text
        .find('\n')
        .ok_or("Invalid CSV structure: expected a preamble line followed by a header line")?;
    if text[..first_line_end]
        .trim_start_matches('\u{feff}')
        .trim()
        .is_empty()
    {
        return Err("Invalid CSV structure: preamble line is empty".into());
    }

    let csv_body = &text[first_line_end + 1..];
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_body.as_bytes());
    let headers = reader.headers().map_err(|error| csv_error(error, 1))?;
    if headers.len() != 8 {
        return Err(format!(
            "Invalid CSV header on physical row 2: expected 8 columns, found {}",
            headers.len()
        )
        .into());
    }
    let names: [&[&str]; 8] = [
        &["client name", "pa"],
        &["start time", "start"],
        &["end time", "end"],
        &["break time", "break"],
        &["worked hours", "worked"],
        &["rate/h", "rate"],
        &["amount"],
        &["note"],
    ];
    let mut columns = [usize::MAX; 8];
    for (index, header) in headers.iter().enumerate() {
        let normal = header
            .trim()
            .trim_start_matches('\u{feff}')
            .trim()
            .to_lowercase();
        let field = names
            .iter()
            .position(|aliases| aliases.contains(&normal.as_str()))
            .ok_or_else(|| {
                format!("Invalid CSV header on physical row 2: unknown field {header:?}")
            })?;
        if columns[field] != usize::MAX {
            return Err(format!(
                "Invalid CSV header on physical row 2: duplicate field {header:?}"
            )
            .into());
        }
        columns[field] = index;
    }
    if columns.contains(&usize::MAX) {
        return Err("Invalid CSV header on physical row 2: missing required field".into());
    }

    let mut entries = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|error| csv_error(error, 1))?;
        let source_row = record.position().map_or(0, |position| position.line() + 1);
        if record.len() != 8 {
            return Err(format!(
                "Invalid CSV row {source_row}: expected 8 columns, found {}",
                record.len()
            )
            .into());
        }

        let pa_name = record[columns[0]].trim().to_string();
        if pa_name.is_empty() {
            return Err(format!("Invalid CSV row {source_row}: missing PA name").into());
        }
        let start_time = record[columns[1]].trim().to_string();
        let end_time = record[columns[2]].trim().to_string();
        let start = parse_supported_timestamp(&start_time).ok_or_else(|| {
            format!("Invalid CSV row {source_row} start timestamp: unsupported timestamp {start_time:?}")
        })?;
        let end = parse_supported_timestamp(&end_time).ok_or_else(|| {
            format!(
                "Invalid CSV row {source_row} end timestamp: unsupported timestamp {end_time:?}"
            )
        })?;
        if end <= start {
            return Err(format!(
                "Invalid CSV row {source_row}: end timestamp must be later than start timestamp"
            )
            .into());
        }

        let break_minutes = parse_duration(&record[columns[3]])
            .map_err(|error| format!("Invalid CSV row {source_row} break duration: {error}"))?;
        let worked_minutes = parse_duration(&record[columns[4]])
            .map_err(|error| format!("Invalid CSV row {source_row} worked duration: {error}"))?;
        let hourly_rate = parse_money(&record[columns[5]])
            .map_err(|error| format!("Invalid CSV row {source_row} hourly rate: {error}"))?;
        let amount = parse_money(&record[columns[6]])
            .map_err(|error| format!("Invalid CSV row {source_row} amount: {error}"))?;

        entries.push(ParsedTimesheetRow {
            source_row,
            supplied_fields: std::array::from_fn(|f| !record[columns[f]].trim().is_empty()),
            start,
            end,
            entry: TimesheetEntry {
                id: 0,
                pa_name,
                personal_assistant_id: None,
                start_time,
                end_time,
                break_minutes,
                worked_minutes,
                hourly_rate,
                amount,
                notes: if record[columns[7]].trim().is_empty() {
                    None
                } else {
                    Some(record[columns[7]].trim().to_string())
                },
            },
        });
    }

    Ok(entries)
}

fn csv_error(error: csv::Error, physical_line_offset: u64) -> Box<dyn Error> {
    let row = error
        .position()
        .map(|position| position.line() + physical_line_offset);
    match row {
        Some(row) => format!("Invalid CSV structure at physical row {row}: {error}").into(),
        None => format!("Invalid CSV structure: {error}").into(),
    }
}

pub(crate) fn parse_supported_timestamp(value: &str) -> Option<NaiveDateTime> {
    const FORMATS: [&str; 8] = [
        "%e %B %Y at %H:%M:%S",
        "%e %b %Y at %H:%M:%S",
        "%e %B %Y at %H:%M",
        "%e %b %Y at %H:%M",
        "%d/%m/%Y at %H:%M:%S",
        "%d/%m/%Y at %H:%M",
        "%Y-%m-%d at %H:%M:%S",
        "%Y-%m-%d at %H:%M",
    ];
    let value = value.trim();
    FORMATS
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
}

fn parse_duration(value: &str) -> Result<i64, Box<dyn Error>> {
    let mut parts = value.split_whitespace();
    let hours_text = parts.next().ok_or("missing hours")?;
    let minutes_text = parts.next().ok_or("missing minutes")?;
    if parts.next().is_some() || !hours_text.ends_with('h') || !minutes_text.ends_with('m') {
        return Err(format!("expected '<hours>h <minutes>m', found {value:?}").into());
    }
    let hours: i64 = hours_text[..hours_text.len() - 1]
        .parse()
        .map_err(|_| format!("invalid hours in {value:?}"))?;
    let minutes: i64 = minutes_text[..minutes_text.len() - 1]
        .parse()
        .map_err(|_| format!("invalid minutes in {value:?}"))?;
    if hours < 0 || !(0..=59).contains(&minutes) {
        return Err(
            format!("duration must be non-negative with minutes from 0 to 59: {value:?}").into(),
        );
    }
    hours
        .checked_mul(60)
        .and_then(|total| total.checked_add(minutes))
        .ok_or_else(|| format!("duration is too large: {value:?}").into())
}

fn parse_money(value: &str) -> Result<f64, Box<dyn Error>> {
    let cleaned = value.trim().strip_prefix('£').unwrap_or(value.trim());
    let amount: f64 = cleaned
        .parse()
        .map_err(|_| format!("invalid monetary value {value:?}"))?;
    if !amount.is_finite() || amount < 0.0 {
        return Err(format!("monetary value must be finite and non-negative: {value:?}").into());
    }
    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv(row: &str) -> Vec<u8> {
        format!("period\nPA,Start,End,Break,Worked,Rate,Amount,Note\n{row}\n").into_bytes()
    }

    #[test]
    fn parses_supported_cross_midnight_row_and_keeps_authoritative_worked_minutes() {
        let rows = import_csv_bytes(&csv("Alex Smith,27 July 2026 at 23:00:00,28 July 2026 at 01:00:00,0h 00m,1h 45m,£12.21,£21.37,")) .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_row, 3);
        assert_eq!(rows[0].start.date().to_string(), "2026-07-27");
        assert_eq!(
            parse_supported_timestamp(&rows[0].entry.end_time)
                .unwrap()
                .date()
                .to_string(),
            "2026-07-28"
        );
        assert_eq!(rows[0].entry.worked_minutes, 105);
    }

    #[test]
    fn zero_worked_minutes_remains_supported() {
        let rows = import_csv_bytes(&csv("Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,0h 00m,£12.21,£0.00,")) .unwrap();
        assert_eq!(rows[0].entry.worked_minutes, 0);
    }

    #[test]
    fn rejects_bad_timestamps_durations_money_and_structure_with_rows() {
        for row in [
            "Alex Smith,bad,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.21,£12.21,",
            "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 60m,1h 00m,£12.21,£12.21,",
            "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,-1h 00m,£12.21,£12.21,",
            "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,999999999999999999h 00m,£12.21,£12.21,",
            "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,NaN,£12.21,",
            "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,inf,£12.21,",
            "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.21,-£1.00,",
        ] {
            let error = import_csv_bytes(&csv(row)).unwrap_err().to_string();
            assert!(error.contains("row 3"), "{error}");
        }
        let error = import_csv_bytes(b"period\nA,B\n1,2\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("physical row 2"));
    }

    #[test]
    fn rejects_invalid_utf8() {
        let error = import_csv_bytes(&[0xff, b'\n']).unwrap_err().to_string();
        assert!(error.contains("UTF-8"));
    }
}

#[cfg(test)]
mod stage4_header_tests {
    use super::*;
    const ROW:&str="Example PA,2 April 2026 at 09:00:00,2 April 2026 at 10:00:00,0h 00m,0h 45m,£12.00,£9.00,note";
    #[test]
    fn real_and_synthetic_headers_preserve_authoritative_duration() {
        for header in [
            "Client Name,Start Time,End Time,Break Time,Worked Hours,Rate/h,Amount,Note",
            "PA,Start,End,Break,Worked,Rate,Amount,Note",
        ] {
            let rows =
                import_csv_bytes(format!("\u{feff}period\n{header}\n{ROW}\n").as_bytes()).unwrap();
            assert_eq!(rows[0].entry.worked_minutes, 45);
        }
    }
    #[test]
    fn reordered_quoted_bom_case_and_whitespace_headers_map_semantically() {
        let rows=import_csv_bytes("period\n\u{feff}\" nOtE \",Amount,Rate/h,Worked Hours,Break Time,End Time,Start Time,Client Name\n\"a,b\",£9.00,£12.00,0h 45m,0h 00m,2 April 2026 at 10:00:00,2 April 2026 at 09:00:00,Example PA\n".as_bytes()).unwrap();
        assert_eq!(rows[0].entry.pa_name, "Example PA");
        assert_eq!(rows[0].entry.worked_minutes, 45);
        assert_eq!(rows[0].entry.notes.as_deref(), Some("a,b"));
    }
    #[test]
    fn missing_unknown_and_duplicate_semantic_headers_refuse_before_rows() {
        for header in [
            "PA,Start,End,Break,Worked,Rate,Amount",
            "PA,Start,End,Break,Worked,Rate,Amount,Unknown",
            "PA,Client Name,End,Break,Worked,Rate,Amount,Note",
        ] {
            let err = import_csv_bytes(format!("period\n{header}\n{ROW}").as_bytes())
                .unwrap_err()
                .to_string();
            assert!(err.contains("header"));
        }
    }
}

#[cfg(test)]
mod configurable_tests {
    use super::*;
    fn minimal(worked: &str) -> String {
        format!("period\nPA,Start,End,Worked,Unknown\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,{worked},ignored\n")
    }
    #[test]
    fn optional_columns_and_unknown_columns_keep_unrounded_worked_hours() {
        let rows =
            import_csv_with_config(minimal("0h 17m").as_bytes(), &Default::default()).unwrap();
        let row = &rows[0];
        assert_eq!(row.entry.worked_minutes, 17);
        assert_eq!(
            (
                row.entry.break_minutes,
                row.entry.hourly_rate,
                row.entry.amount
            ),
            (0, 0.0, 0.0)
        );
        assert_eq!(row.entry.notes, None);
        assert_eq!(
            row.supplied_fields,
            [true, true, true, false, true, false, false, false]
        );
    }
    #[test]
    fn end_minus_start_ignores_worked_hours_and_does_not_deduct_break() {
        let config = CsvImportConfig {
            hours_method: HoursMethod::EndMinusStart,
            ..Default::default()
        };
        let csv = "period\nPA,Start,End,Break,Worked\nExample PA,2 April 2026 at 23:50,3 April 2026 at 00:24,0h 10m,not used\n";
        let row = import_csv_with_config(csv.as_bytes(), &config)
            .unwrap()
            .remove(0);
        assert_eq!(row.entry.worked_minutes, 34);
        assert_eq!(row.entry.break_minutes, 10);
        assert!(import_csv_with_config(
            csv.replace(",Worked", "")
                .replace(",not used", "")
                .as_bytes(),
            &config
        )
        .is_ok());
    }
    #[test]
    fn custom_headings_skip_physical_lines_and_parse_multiline_notes() {
        let mut config = CsvImportConfig {
            introductory_rows: 2,
            ..Default::default()
        };
        config.headings[0] = vec!["Employee".into()];
        let csv = "intro\r\n\r\nNote,End,Worked,Employee,Start,extra\r\n\"first\r\nsecond\",2 April 2026 at 10:00,0h 45m,Example PA,2 April 2026 at 09:00,x\r\n";
        let row = import_csv_with_config(csv.as_bytes(), &config)
            .unwrap()
            .remove(0);
        assert_eq!(row.source_row, 4);
        assert!(row.entry.notes.unwrap().contains("second"));
        config.introductory_rows = 0;
        assert_eq!(
            import_csv_with_config(
                minimal("0h 00m").split_once('\n').unwrap().1.as_bytes(),
                &config
            )
            .unwrap_err()
            .to_string()
            .contains("missing required"),
            true
        );
        config.headings[0] = vec!["PA".into()];
        assert_eq!(
            import_csv_with_config(
                minimal("0h 00m").split_once('\n').unwrap().1.as_bytes(),
                &config
            )
            .unwrap()[0]
                .entry
                .worked_minutes,
            0
        );
    }
    #[test]
    fn missing_required_invalid_values_and_ambiguous_mappings_refuse() {
        let config = CsvImportConfig::default();
        for csv in [
            minimal(""),
            minimal("-1h 00m"),
            minimal("0h 60m"),
            minimal("0h 17m").replace("09:34", "08:34"),
            minimal("0h 17m").replace("Worked,Unknown", "Worked,PA"),
            minimal("0h 17m").replace("Start,", "unmapped,"),
        ] {
            assert!(
                import_csv_with_config(csv.as_bytes(), &config).is_err(),
                "{csv}"
            );
        }
        let mut bad = config.clone();
        bad.headings[0].push("START".into());
        assert!(bad.validate().is_err());
        bad = config.clone();
        bad.headings[0].clear();
        assert!(bad.validate().is_err());
        let bad_row = minimal("0h 17m")
            .replace(",Unknown", ",Rate")
            .replace(",ignored", ",NaN");
        assert!(import_csv_with_config(bad_row.as_bytes(), &config).is_err());
        let truncated = CsvImportConfig {
            introductory_rows: 99,
            ..config
        };
        assert!(import_csv_with_config(b"one line", &truncated).is_err());
    }
    #[test]
    fn errors_keep_physical_line_numbers_after_skip() {
        let config = CsvImportConfig {
            introductory_rows: 2,
            ..Default::default()
        };
        let csv = format!("extra\n{}", minimal("bad"));
        assert!(import_csv_with_config(csv.as_bytes(), &config)
            .unwrap_err()
            .to_string()
            .contains("row 4"));
        assert!(import_csv_with_config(&[255], &config).is_err());
    }
    #[test]
    fn legacy_full_exports_remain_identical_and_legacy_reader_stays_strict() {
        let csv=b"period\nPA,Start,End,Break,Worked,Rate,Amount,Note\nExample PA,2 April 2026 at 09:00,2 April 2026 at 10:00,0h 10m,0h 34m,12,6.8,note\n";
        let old = import_csv_bytes(csv).unwrap().remove(0).entry;
        let new = import_csv_with_config(csv, &Default::default())
            .unwrap()
            .remove(0)
            .entry;
        assert_eq!(format!("{old:?}"), format!("{new:?}"));
        assert!(import_csv_bytes(minimal("0h 34m").as_bytes()).is_err());
    }
    #[test]
    fn config_defaults_and_custom_mapping_round_trip_without_profile_io() {
        let legacy = toml::to_string(&crate::config::AppConfig::default()).unwrap();
        let mut table: toml::Value = toml::from_str(&legacy).unwrap();
        table.as_table_mut().unwrap().remove("csv_import");
        let mut restored: crate::config::AppConfig = table.try_into().unwrap();
        assert_eq!(restored.csv_import, CsvImportConfig::default());
        restored.csv_import.hours_method = HoursMethod::EndMinusStart;
        restored.csv_import.introductory_rows = 0;
        restored.csv_import.headings[0] = vec!["Employee".into()];
        let again: crate::config::AppConfig =
            toml::from_str(&toml::to_string(&restored).unwrap()).unwrap();
        assert_eq!(again.csv_import, restored.csv_import);
    }
    #[test]
    fn optional_blank_fields_and_invalid_supplied_values_are_distinct() {
        let config = CsvImportConfig::default();
        let csv="period\nPA,Start,End,Worked,Break,Rate,Amount,Note\nExample PA,2 April 2026 at 09:00,2 April 2026 at 10:00,0h 17m,,,,\n";
        let row = import_csv_with_config(csv.as_bytes(), &config)
            .unwrap()
            .remove(0);
        assert_eq!(
            row.supplied_fields,
            [true, true, true, false, true, false, false, false]
        );
        for tail in ["0h 60m,,,", ",-1,,", ",,inf,"] {
            let invalid = csv.replace("0h 17m,,,,", &format!("0h 17m,{tail}"));
            assert!(import_csv_with_config(invalid.as_bytes(), &config).is_err());
        }
    }
}

#[path = "csv_import_evidence.rs"]
pub(crate) mod evidence;

#[cfg(test)]
mod four_method_tests {
    use super::*;
    fn csv(break_value: &str, worked: &str) -> String {
        format!("period\nPA,Start,End,Break,Worked,Rate,Amount,Note\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,{break_value},{worked},bad rate,bad amount,source note\n")
    }
    fn config(method: HoursMethod) -> CsvImportConfig {
        let mut c = CsvImportConfig {
            hours_method: method,
            ..Default::default()
        };
        c.enabled[5] = false;
        c.enabled[6] = false;
        c
    }
    #[test]
    fn all_four_methods_are_unrounded_and_disabled_evidence_is_ignored() {
        for (method, expected) in [
            (HoursMethod::WorkedHours, 27),
            (HoursMethod::WorkedHoursMinusBreak, 17),
            (HoursMethod::EndMinusStart, 34),
            (HoursMethod::EndMinusStartMinusBreak, 24),
        ] {
            let c = config(method);
            let row = import_csv_with_config(csv("0h 10m", "0h 27m").as_bytes(), &c)
                .unwrap()
                .remove(0);
            assert_eq!(row.entry.worked_minutes, expected);
            assert_eq!(row.entry.break_minutes, 10);
            assert_eq!(
                row.supplied_fields,
                [true, true, true, true, true, false, false, true]
            );
            assert_eq!((row.entry.hourly_rate, row.entry.amount), (0.0, 0.0));
            assert_eq!(row.entry.notes.as_deref(), Some("source note"));
            let persisted: CsvImportConfig = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
            assert_eq!(persisted, c);
        }
    }
    #[test]
    fn deducting_methods_require_enabled_break_heading_and_every_valid_value() {
        for method in [
            HoursMethod::WorkedHoursMinusBreak,
            HoursMethod::EndMinusStartMinusBreak,
        ] {
            let mut c = config(method);
            c.enabled[3] = false;
            assert!(c.validate().is_err());
            c.enabled[3] = true;
            c.headings[3].clear();
            assert!(c.validate().is_err());
            c.headings[3] = vec!["Break".into()];
            let missing_heading = csv("0h 10m", "0h 27m")
                .replace(",Break", "")
                .replace(",0h 10m", "");
            assert!(import_csv_with_config(missing_heading.as_bytes(), &c)
                .unwrap_err()
                .to_string()
                .contains("missing required field Break"));
            for value in [
                "",
                "bad",
                "-1h 00m",
                "0h 60m",
                "0h 35m",
                "9223372036854775807h 00m",
            ] {
                let err = import_csv_with_config(csv(value, "0h 27m").as_bytes(), &c)
                    .unwrap_err()
                    .to_string();
                assert!(err.contains("row 3"), "{value}: {err}");
            }
            for value in ["0h 00m", "0h 27m"] {
                assert!(import_csv_with_config(csv(value, "0h 27m").as_bytes(), &c).is_ok());
            }
            if method == HoursMethod::WorkedHoursMinusBreak {
                assert!(import_csv_with_config(csv("0h 28m", "0h 27m").as_bytes(), &c).is_err());
                assert!(import_csv_with_config(csv("0h 10m", "").as_bytes(), &c).is_err());
            } else {
                let row = import_csv_with_config(csv("0h 34m", "unused").as_bytes(), &c)
                    .unwrap()
                    .remove(0);
                assert_eq!(row.entry.worked_minutes, 0);
            }
            // A later invalid record refuses the whole parse, not just that row.
            let two=format!("{}Example PA,2 April 2026 at 10:00,2 April 2026 at 10:34,,0h 27m,ignored,ignored,\n",csv("0h 10m","0h 27m"));
            assert!(import_csv_with_config(two.as_bytes(), &c)
                .unwrap_err()
                .to_string()
                .contains("row 4"));
        }
    }
    #[test]
    fn method_dependent_fields_required_and_disabled_aliases_survive() {
        for method in HoursMethod::ALL {
            let mut c = config(method);
            for field in 0..8 {
                let original = c.clone();
                c.enabled[field] = false;
                assert_eq!(
                    c.validate().is_err(),
                    method.requires(field),
                    "{method:?} {field}"
                );
                c = original;
            }
            if method.uses_worked_hours() {
                assert!(import_csv_with_config(csv("0h 00m", "").as_bytes(), &c).is_err());
                assert!(import_csv_with_config(csv("0h 00m", "-1h 00m").as_bytes(), &c).is_err());
            }
        }
        let mut c = config(HoursMethod::EndMinusStart);
        c.headings[4] = vec!["PA".into()];
        c.enabled[4] = false;
        assert!(c.validate().is_ok());
        let aliases = c.headings.clone();
        c.hours_method = HoursMethod::WorkedHoursMinusBreak;
        c.require_method_fields();
        assert!(c.enabled[3] && c.enabled[4]);
        assert_eq!(c.headings, aliases);
        assert!(c.validate().is_err());
        // Old Stage 1 settings/audits without enabled deserialize as all-enabled.
        let old = toml::to_string(&CsvImportConfig::default())
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with("enabled ="))
            .collect::<Vec<_>>()
            .join("\n");
        let restored: CsvImportConfig = toml::from_str(&old).unwrap();
        assert_eq!(restored, CsvImportConfig::default());
    }
    #[test]
    fn disabled_fields_ignore_duplicate_bad_columns_in_any_order() {
        let mut c = config(HoursMethod::EndMinusStart);
        c.enabled[3] = false;
        c.enabled[4] = false;
        c.enabled[7] = false;
        let text="period\nWorked,End,PA,Rate,Start,Break,Rate,Note\ninvalid,3 April 2026 at 00:24,Example PA,bad,2 April 2026 at 23:50,invalid,also bad,ignored\n";
        let row = import_csv_with_config(text.as_bytes(), &c)
            .unwrap()
            .remove(0);
        assert_eq!(
            (
                row.entry.worked_minutes,
                row.entry.break_minutes,
                row.entry.notes
            ),
            (34, 0, None)
        );
        assert_eq!(
            row.supplied_fields,
            [true, true, true, false, false, false, false, false]
        );
    }
}
