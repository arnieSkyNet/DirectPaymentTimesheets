use std::error::Error;
use std::path::Path;

use crate::payroll_schedule_repository::{PayrollSchedule, PayrollScheduleRepository};

pub struct PayrollPrepSheetImportService<'a> {
    repository: &'a PayrollScheduleRepository,
}

impl<'a> PayrollPrepSheetImportService<'a> {
    pub fn new(repository: &'a PayrollScheduleRepository) -> Self {
        Self { repository }
    }

    pub fn import(&self, path: &Path) -> Result<usize, Box<dyn Error>> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase());

        match extension.as_deref() {
            Some("pdf") => self.import_pdf(path),
            Some("docx") => self.import_docx(path),
            _ => Err("Unsupported Payroll Prep Sheet file type.".into()),
        }
    }

    fn import_pdf(&self, path: &Path) -> Result<usize, Box<dyn Error>> {
        let text = pdf_extract::extract_text(path)?;

        if !text.contains("PAYROLL PREPARATION SHEET") {
            return Err(
                "The selected PDF does not appear to be a Payroll Preparation Sheet.".into(),
            );
        }

        if !text.contains("PAY DAY EVERY FOUR (4) WEEKS") {
            return Err(
                "The selected PDF does not contain the expected four-weekly payroll information."
                    .into(),
            );
        }
        let payroll_year = text
            .lines()
            .find_map(|line| {
                let marker = "PAYROLL PREPARATION SHEET ";

                line.find(marker).map(|position| {
                    line[position + marker.len()..]
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_string()
                })
            })
            .filter(|year| !year.is_empty())
            .ok_or("Could not find the payroll year in the Payroll Prep Sheet.")?;

        let mut schedules = Vec::new();

        for line in text.lines() {
            let dates: Vec<&str> = line
                .split_whitespace()
                .filter(|value| chrono::NaiveDate::parse_from_str(value, "%d/%m/%Y").is_ok())
                .collect();

            if dates.len() == 3 {
                let cycle_number = (schedules.len() + 1) as i64;

                schedules.push(PayrollSchedule {
                    id: 0,
                    payroll_year: payroll_year.clone(),
                    cycle_number,
                    first_week_commencing: dates[0].to_string(),
                    latest_posting_date: dates[1].to_string(),
                    pay_date: dates[2].to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                });
            }
        }

        if schedules.len() != 13 {
            return Err(format!(
                "Expected 13 payroll schedule entries, but found {}.",
                schedules.len()
            )
            .into());
        }

        self.replace_schedule(&payroll_year, &schedules)
    }

    fn import_docx(&self, _path: &Path) -> Result<usize, Box<dyn Error>> {
        Err("Word Payroll Prep Sheet importing is not implemented yet.".into())
    }

    fn replace_schedule(
        &self,
        payroll_year: &str,
        schedules: &[PayrollSchedule],
    ) -> Result<usize, Box<dyn Error>> {
        self.repository.delete_all_for_year(payroll_year)?;

        for schedule in schedules {
            self.repository.insert(schedule)?;
        }

        Ok(schedules.len())
    }
}
