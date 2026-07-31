use csv::ReaderBuilder;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::models::TimesheetEntry;

pub fn import_csv(path: &str) -> Result<Vec<TimesheetEntry>, Box<dyn Error>> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();

    // Skip the report date range line
    lines.next();

    let mut csv_data = String::new();

    for line in lines {
        csv_data.push_str(&line?);
        csv_data.push('\n');
    }

    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_data.as_bytes());

    let mut entries = Vec::new();

    for result in reader.records() {
        let record = result?;

        let entry = TimesheetEntry {
            id: 0,
            pa_name: record[0].to_string(),
            start_time: record[1].to_string(),
            end_time: record[2].to_string(),
            break_minutes: parse_duration(&record[3]),
            worked_minutes: parse_duration(&record[4]),
            hourly_rate: parse_money(&record[5]),
            amount: parse_money(&record[6]),
            notes: if record[7].is_empty() {
                None
            } else {
                Some(record[7].to_string())
            },
        };

        entries.push(entry);
    }

    Ok(entries)
}

fn parse_duration(value: &str) -> i64 {
    let cleaned = value
        .replace('h', "")
        .replace('m', "");

    let parts: Vec<&str> = cleaned
        .split_whitespace()
        .collect();

    if parts.len() == 2 {
        let hours: i64 = parts[0].parse().unwrap_or(0);
        let minutes: i64 = parts[1].parse().unwrap_or(0);

        (hours * 60) + minutes
    } else {
        0
    }
}


fn parse_money(value: &str) -> f64 {
    value
        .replace('£', "")
        .parse()
        .unwrap_or(0.0)
}

