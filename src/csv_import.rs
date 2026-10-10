use chrono::NaiveDateTime;
use csv::ReaderBuilder;
use std::error::Error;

use crate::models::TimesheetEntry;

#[derive(Debug)]
pub struct ParsedTimesheetRow {
    pub source_row: u64,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub entry: TimesheetEntry,
}

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
