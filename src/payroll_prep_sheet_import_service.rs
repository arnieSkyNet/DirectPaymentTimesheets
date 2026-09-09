use std::error::Error;
use std::path::Path;

use chrono::NaiveDate;

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

        self.import_text(&text)
    }

    fn import_text(&self, text: &str) -> Result<usize, Box<dyn Error>> {
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
        validate_payroll_year(&payroll_year)?;

        let mut schedules = Vec::new();

        for line in text.lines() {
            let dates: Vec<&str> = line
                .split_whitespace()
                .filter(|value| crate::date_utils::parse_input(value).is_ok())
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
                    payslips_sent: false,
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

        validate_schedules(&schedules)?;
        self.repository
            .replace_year_atomically(&payroll_year, &schedules)
    }

    fn import_docx(&self, _path: &Path) -> Result<usize, Box<dyn Error>> {
        Err("Word Payroll Prep Sheet importing is not implemented yet.".into())
    }
}

fn validate_payroll_year(payroll_year: &str) -> Result<(), Box<dyn Error>> {
    let bytes = payroll_year.as_bytes();
    if bytes.len() != 7
        || bytes[4] != b'/'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..].iter().all(u8::is_ascii_digit)
    {
        return Err(format!(
            "Invalid payroll year '{payroll_year}'; expected YYYY/YY, for example 2027/28."
        )
        .into());
    }

    let start_year: i32 = payroll_year[..4].parse()?;
    let ending_year: i32 = payroll_year[5..].parse()?;
    if ending_year != (start_year + 1).rem_euclid(100) {
        return Err(format!(
            "Invalid payroll year '{payroll_year}'; the ending must identify the following calendar year."
        )
        .into());
    }

    Ok(())
}

fn validate_schedules(schedules: &[PayrollSchedule]) -> Result<(), Box<dyn Error>> {
    let mut previous_first_week = None;

    for schedule in schedules {
        let first_week = parse_schedule_date(
            &schedule.first_week_commencing,
            schedule.cycle_number,
            "first week commencing",
        )?;
        let posting_date = parse_schedule_date(
            &schedule.latest_posting_date,
            schedule.cycle_number,
            "latest posting date",
        )?;
        let pay_date = parse_schedule_date(&schedule.pay_date, schedule.cycle_number, "pay date")?;

        if posting_date < first_week || pay_date < first_week {
            return Err(format!(
                "Payroll cycle {} has a posting or pay date before its first week commencing date.",
                schedule.cycle_number
            )
            .into());
        }

        if let Some(previous) = previous_first_week {
            let difference = first_week.signed_duration_since(previous).num_days();
            if difference <= 0 {
                return Err(format!(
                    "Payroll cycle {} first week commencing date is duplicate or out of order.",
                    schedule.cycle_number
                )
                .into());
            }
            if difference != 28 {
                return Err(format!(
                    "Payroll cycle {} must start exactly 28 days after the previous cycle; found {difference} days.",
                    schedule.cycle_number
                )
                .into());
            }
        }

        previous_first_week = Some(first_week);
    }

    Ok(())
}

fn parse_schedule_date(
    value: &str,
    cycle_number: i64,
    field_name: &str,
) -> Result<NaiveDate, Box<dyn Error>> {
    crate::date_utils::parse_input(value).map_err(|_| {
        format!("Payroll cycle {cycle_number} has an invalid {field_name}: '{value}'.").into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use rusqlite::Connection;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn repository() -> PayrollScheduleRepository {
        let connection = Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        PayrollScheduleRepository::new(connection)
    }

    fn sheet(payroll_year: &str, first_week: NaiveDate) -> String {
        let mut text =
            format!("PAYROLL PREPARATION SHEET {payroll_year}\nPAY DAY EVERY FOUR (4) WEEKS\n");
        for cycle in 0..13 {
            let first = first_week + Duration::days(cycle * 28);
            let posting = first + Duration::days(18);
            let pay = first + Duration::days(25);
            text.push_str(&format!(
                "{} {} {}\n",
                first.format("%d/%m/%Y"),
                posting.format("%d/%m/%Y"),
                pay.format("%d/%m/%Y")
            ));
        }
        text
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn temporary_database_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "direct-payment-timesheets-{label}-{}-{unique}.sqlite",
            std::process::id()
        ))
    }

    #[test]
    fn imports_valid_first_payroll_year() {
        let repository = repository();
        let service = PayrollPrepSheetImportService::new(&repository);

        assert_eq!(
            service
                .import_text(&sheet("2026/27", date(2026, 3, 23)))
                .unwrap(),
            13
        );
        assert_eq!(repository.get_all_for_year("2026/27").unwrap().len(), 13);
    }

    #[test]
    fn importing_future_year_preserves_existing_year() {
        let repository = repository();
        let service = PayrollPrepSheetImportService::new(&repository);

        service
            .import_text(&sheet("2026/27", date(2026, 3, 23)))
            .unwrap();
        service
            .import_text(&sheet("2027/28", date(2027, 3, 22)))
            .unwrap();

        assert_eq!(repository.get_all_for_year("2026/27").unwrap().len(), 13);
        assert_eq!(repository.get_all_for_year("2027/28").unwrap().len(), 13);
    }

    #[test]
    fn reimport_replaces_only_the_intended_year() {
        let repository = repository();
        let service = PayrollPrepSheetImportService::new(&repository);
        service
            .import_text(&sheet("2026/27", date(2026, 3, 23)))
            .unwrap();
        service
            .import_text(&sheet("2027/28", date(2027, 3, 22)))
            .unwrap();

        service
            .import_text(&sheet("2027/28", date(2027, 3, 29)))
            .unwrap();

        assert_eq!(
            repository.get_all_for_year("2026/27").unwrap()[0].first_week_commencing,
            "23/03/2026"
        );
        assert_eq!(
            repository.get_all_for_year("2027/28").unwrap()[0].first_week_commencing,
            "29/03/2027"
        );
    }

    #[test]
    fn rejects_malformed_or_inconsistent_payroll_year() {
        let repository = repository();
        let service = PayrollPrepSheetImportService::new(&repository);

        assert!(service
            .import_text(&sheet("2027-28", date(2027, 3, 22)))
            .is_err());
        assert!(service
            .import_text(&sheet("2027/29", date(2027, 3, 22)))
            .is_err());
    }

    #[test]
    fn rejects_duplicate_unordered_and_non_four_week_cycles() {
        let repository = repository();
        let service = PayrollPrepSheetImportService::new(&repository);
        let valid = sheet("2027/28", date(2027, 3, 22));
        let second = "19/04/2027 07/05/2027 14/05/2027";

        let duplicate = valid.replace(second, "22/03/2027 09/04/2027 16/04/2027");
        assert!(service.import_text(&duplicate).is_err());

        let unordered = valid.replace(second, "15/03/2027 02/04/2027 09/04/2027");
        assert!(service.import_text(&unordered).is_err());

        let wrong_spacing = valid.replace(second, "26/04/2027 14/05/2027 21/05/2027");
        assert!(service.import_text(&wrong_spacing).is_err());
    }

    #[test]
    fn rejects_posting_or_pay_dates_before_the_cycle_start() {
        let repository = repository();
        let service = PayrollPrepSheetImportService::new(&repository);
        let valid = sheet("2027/28", date(2027, 3, 22));

        let early_posting = valid.replace(
            "22/03/2027 09/04/2027 16/04/2027",
            "22/03/2027 21/03/2027 16/04/2027",
        );
        assert!(service.import_text(&early_posting).is_err());

        let early_pay = valid.replace(
            "22/03/2027 09/04/2027 16/04/2027",
            "22/03/2027 09/04/2027 21/03/2027",
        );
        assert!(service.import_text(&early_pay).is_err());
    }

    #[test]
    fn validation_failure_leaves_existing_schedule_untouched() {
        let repository = repository();
        let service = PayrollPrepSheetImportService::new(&repository);
        service
            .import_text(&sheet("2027/28", date(2027, 3, 22)))
            .unwrap();

        let invalid = sheet("2027/28", date(2027, 3, 22)).replace(
            "19/04/2027 07/05/2027 14/05/2027",
            "26/04/2027 14/05/2027 21/05/2027",
        );
        assert!(service.import_text(&invalid).is_err());

        let stored = repository.get_all_for_year("2027/28").unwrap();
        assert_eq!(stored.len(), 13);
        assert_eq!(stored[1].first_week_commencing, "19/04/2027");
    }

    #[test]
    fn safe_reimport_preserves_payslips_sent_state() {
        let repository = repository();
        let service = PayrollPrepSheetImportService::new(&repository);
        let input = sheet("2026/27", date(2026, 3, 23));
        service.import_text(&input).unwrap();
        let first = repository.get_all_for_year("2026/27").unwrap()[0].clone();
        repository.mark_payslips_sent(first.id).unwrap();

        service.import_text(&input).unwrap();

        assert!(repository.get_all_for_year("2026/27").unwrap()[0].payslips_sent);
    }

    #[test]
    fn insertion_failure_rolls_back_the_complete_previous_schedule() {
        let path = temporary_database_path("atomic-schedule");
        let connection = Connection::open(&path).unwrap();
        crate::database::create_schema(&connection).unwrap();
        drop(connection);
        let repository = PayrollScheduleRepository::new(Connection::open(&path).unwrap());
        let service = PayrollPrepSheetImportService::new(&repository);
        service
            .import_text(&sheet("2027/28", date(2027, 3, 22)))
            .unwrap();
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER reject_schedule_insert
                 BEFORE INSERT ON payroll_schedules
                 BEGIN SELECT RAISE(ABORT, 'simulated insertion failure'); END;",
            )
            .unwrap();

        assert!(service
            .import_text(&sheet("2027/28", date(2027, 3, 29)))
            .is_err());
        let stored = repository.get_all_for_year("2027/28").unwrap();
        assert_eq!(stored.len(), 13);
        assert_eq!(stored[0].first_week_commencing, "22/03/2027");

        drop(service);
        drop(repository);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn refuses_changed_schedule_when_prepared_payroll_history_exists() {
        let path = temporary_database_path("schedule-history");
        let connection = Connection::open(&path).unwrap();
        crate::database::create_schema(&connection).unwrap();
        drop(connection);
        let repository = PayrollScheduleRepository::new(Connection::open(&path).unwrap());
        let service = PayrollPrepSheetImportService::new(&repository);
        service
            .import_text(&sheet("2026/27", date(2026, 3, 23)))
            .unwrap();
        Connection::open(&path)
            .unwrap()
            .execute(
                "INSERT INTO payroll_timesheets (
                    personal_assistant_id, payroll_year, cycle_number,
                    previous_cycle_hours, created_at, updated_at
                 ) VALUES (1, '2026/27', 1, NULL, 'created', 'updated')",
                [],
            )
            .unwrap();

        let error = service
            .import_text(&sheet("2026/27", date(2026, 3, 30)))
            .unwrap_err()
            .to_string();
        assert!(error.contains("prepared payroll record"));
        assert_eq!(
            repository.get_all_for_year("2026/27").unwrap()[0].first_week_commencing,
            "23/03/2026"
        );

        drop(service);
        drop(repository);
        std::fs::remove_file(path).unwrap();
    }
}
