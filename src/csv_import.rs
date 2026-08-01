use csv::ReaderBuilder;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::models::TimesheetEntry;

pub fn import_csv(path: &std::path::Path) -> Result<Vec<TimesheetEntry>, Box<dyn Error>> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();

    lines.next();

    let mut csv_data = String::new();

    for line in lines {
        csv_data.push_str(&line?);
        csv_data.push('\n');
    }

    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_data.as_bytes());

    let headers = reader.headers()?;

    if headers.len() != 8 {
        return Err(
            format!("Invalid CSV format: expected 8 columns, found {}", headers.len()).into()
        );
    }

    let mut entries = Vec::new();

    for (line_number, result) in reader.records().enumerate() {
        let record = result?;

        if record.len() != 8 {
            return Err(
                format!(
                    "Invalid CSV row {}: expected 8 columns, found {}",
                    line_number + 2,
                    record.len()
                )
                .into(),
            );
        }

        let entry = TimesheetEntry {
            id: 0,
            pa_name: record[0].trim().to_string(),
            start_time: record[1].trim().to_string(),
            end_time: record[2].trim().to_string(),
            break_minutes: parse_duration(&record[3])?,
            worked_minutes: parse_duration(&record[4])?,
            hourly_rate: parse_money(&record[5])?,
            amount: parse_money(&record[6])?,
            notes: if record[7].trim().is_empty() {
                None
            } else {
                Some(record[7].trim().to_string())
            },
        };

        if entry.pa_name.is_empty() {
            return Err(
                format!("Invalid CSV row {}: missing PA name", line_number + 2).into()
            );
        }

        entries.push(entry);
    }

    Ok(entries)
}

fn parse_duration(value: &str) -> Result<i64, Box<dyn Error>> {
    let cleaned = value
        .replace('h', "")
        .replace('m', "");

    let parts: Vec<&str> = cleaned
        .split_whitespace()
        .collect();

    if parts.len() != 2 {
        return Err(
            format!("Invalid duration value: {}", value).into()
        );
    }

    let hours: i64 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid hours value: {}", value))?;

    let minutes: i64 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid minutes value: {}", value))?;

    Ok((hours * 60) + minutes)
}

fn parse_money(value: &str) -> Result<f64, Box<dyn Error>> {
    let cleaned = value.replace('£', "");

    let amount: f64 = cleaned
        .parse()
        .map_err(|_| format!("Invalid money value: {}", value))?;

    Ok(amount)
}

