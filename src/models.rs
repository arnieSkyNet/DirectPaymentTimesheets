#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct TimesheetEntry {
    pub id: i64,
    pub pa_name: String,
    pub personal_assistant_id: Option<i64>,
    pub start_time: String,
    pub end_time: String,
    pub break_minutes: i64,
    pub worked_minutes: i64,
    pub hourly_rate: f64,
    pub amount: f64,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Employer {
    pub id: i64,
    pub name: String,

    pub date_of_birth: Option<String>,
    pub national_insurance_number: Option<String>,
    pub reference_account_number: Option<String>,

    pub address: Option<String>,
    pub telephone: Option<String>,
    pub email: Option<String>,

    pub employer_signature: Option<String>,
    pub email_signature: Option<String>,
    pub default_pdf_template: Option<String>,
    pub sick_pay_enabled: bool,
    pub mileage_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct PersonalAssistant {
    pub id: i64,
    pub first_name: String,
    pub surname: String,
    pub date_of_birth: Option<String>,
    pub national_insurance_number: Option<String>,
    pub address: Option<String>,
    pub postcode: Option<String>,
    pub telephone: Option<String>,
    pub email: Option<String>,
    pub employment_status: Option<String>,
    pub sick_pay_enabled: bool,
    pub mileage_enabled: bool,
    pub start_date: Option<String>,
    pub leaving_date: Option<String>,
    pub signature: Option<String>,
}

impl PersonalAssistant {
    pub fn eligible_for_period(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
        has_record: bool,
    ) -> rusqlite::Result<bool> {
        // Stored payroll is historical evidence, not a request to create a new
        // preparation. Keep it accessible even after employment dates change.
        if has_record {
            return Ok(true);
        }
        self.employment_overlaps(start, end)
    }

    /// Inclusive employment overlap for creating new period preparations.
    /// Current status is deliberately irrelevant to historical employment.
    /// Legacy missing/blank dates retain their existing unbounded semantics.
    pub fn employment_overlaps(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> rusqlite::Result<bool> {
        let parse = |value: &Option<String>| -> rusqlite::Result<Option<chrono::NaiveDate>> {
            value
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| {
                    parse_employment_date(value).ok_or_else(|| {
                        rusqlite::Error::InvalidParameterName(format!(
                            "Invalid employment date for {} {}: {value}",
                            self.first_name, self.surname
                        ))
                    })
                })
                .transpose()
        };
        Ok(parse(&self.start_date)?.is_none_or(|date| date <= end)
            && parse(&self.leaving_date)?.is_none_or(|date| date >= start))
    }
}

pub fn parse_employment_date(value: &str) -> Option<chrono::NaiveDate> {
    [
        "%d/%m/%y", "%d-%m-%y", "%d/%m/%Y", "%d-%m-%Y", "%Y-%m-%d", "%d %B %Y", "%d %b %Y",
    ]
    .iter()
    .find_map(|format| chrono::NaiveDate::parse_from_str(value.trim(), format).ok())
}

#[cfg(test)]
mod employment_date_tests {
    use super::parse_employment_date;

    #[test]
    fn flexible_employment_dates_still_require_real_calendar_dates() {
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 9, 30);
        for input in [
            "30/09/2026",
            "30/9/26",
            "2026-09-30",
            "30 September 2026",
            "30 Sep 2026",
        ] {
            assert_eq!(parse_employment_date(input), expected);
        }
        for input in [
            "31 September 2026",
            "29 February 2025",
            "31/09/2026",
            "not a date",
        ] {
            assert!(parse_employment_date(input).is_none());
        }
    }
}
