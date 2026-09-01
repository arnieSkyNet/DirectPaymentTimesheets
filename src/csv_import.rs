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
    if headers.iter().any(|header| header.trim().is_empty()) {
        return Err("Invalid CSV header on physical row 2: column names must not be empty".into());
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

        let pa_name = record[0].trim().to_string();
        if pa_name.is_empty() {
            return Err(format!("Invalid CSV row {source_row}: missing PA name").into());
        }
        let start_time = record[1].trim().to_string();
        let end_time = record[2].trim().to_string();
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

        let break_minutes = parse_duration(&record[3])
            .map_err(|error| format!("Invalid CSV row {source_row} break duration: {error}"))?;
        let worked_minutes = parse_duration(&record[4])
            .map_err(|error| format!("Invalid CSV row {source_row} worked duration: {error}"))?;
        let hourly_rate = parse_money(&record[5])
            .map_err(|error| format!("Invalid CSV row {source_row} hourly rate: {error}"))?;
        let amount = parse_money(&record[6])
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
                notes: if record[7].trim().is_empty() {
                    None
                } else {
                    Some(record[7].trim().to_string())
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
