use std::error::Error;
use std::path::{Path, PathBuf};

use chrono::{Datelike, NaiveDate};

use crate::payroll_schedule_repository::PayrollSchedule;

pub fn paye_week(schedule: &PayrollSchedule) -> Result<u32, Box<dyn Error>> {
    let pay_date = crate::date_utils::parse_legacy(&schedule.pay_date)
        .map_err(|_| format!("Invalid payroll pay date: '{}'.", schedule.pay_date))?;
    let april_sixth = NaiveDate::from_ymd_opt(pay_date.year(), 4, 6)
        .ok_or("Could not calculate the PAYE tax-year start.")?;
    let tax_year_start = if pay_date >= april_sixth {
        april_sixth
    } else {
        NaiveDate::from_ymd_opt(pay_date.year() - 1, 4, 6)
            .ok_or("Could not calculate the PAYE tax-year start.")?
    };

    Ok((pay_date.signed_duration_since(tax_year_start).num_days() / 7 + 1) as u32)
}

pub fn payroll_period_code(schedule: &PayrollSchedule) -> Result<String, Box<dyn Error>> {
    let first_week =
        crate::date_utils::parse_legacy(&schedule.first_week_commencing).map_err(|_| {
            format!(
                "Invalid payroll first week commencing date: '{}'.",
                schedule.first_week_commencing
            )
        })?;
    Ok(format!(
        "{}w{:02}",
        first_week.format("%Y%m"),
        paye_week(schedule)?
    ))
}

pub fn timesheet_filename(
    personal_assistant_name: &str,
    schedule: &PayrollSchedule,
) -> Result<String, Box<dyn Error>> {
    Ok(format!(
        "Timesheet - {} - {}.pdf",
        sanitise_filename(personal_assistant_name),
        payroll_period_code(schedule)?
    ))
}

pub fn payslip_filename(
    personal_assistant_name: &str,
    schedule: &PayrollSchedule,
) -> Result<String, Box<dyn Error>> {
    Ok(format!(
        "Payslip for Week {} for {}.pdf",
        paye_week(schedule)?,
        sanitise_filename(personal_assistant_name)
    ))
}

pub fn payroll_year_directory_name(payroll_year: &str) -> Result<String, Box<dyn Error>> {
    let (start, ending) = payroll_year
        .split_once('/')
        .ok_or_else(|| format!("Invalid payroll year: '{payroll_year}'."))?;
    if start.len() != 4
        || ending.len() != 2
        || !start.bytes().all(|value| value.is_ascii_digit())
        || !ending.bytes().all(|value| value.is_ascii_digit())
    {
        return Err(format!("Invalid payroll year: '{payroll_year}'.").into());
    }
    let start_year: i32 = start.parse()?;
    let end_year = start_year + 1;
    if ending.parse::<i32>()? != end_year.rem_euclid(100) {
        return Err(format!("Invalid payroll year: '{payroll_year}'.").into());
    }
    Ok(format!("{start_year} to {end_year}"))
}

pub fn payroll_year_directory(
    root: &Path,
    schedule: &PayrollSchedule,
) -> Result<PathBuf, Box<dyn Error>> {
    Ok(root_without_payroll_year_suffix(root)
        .join(payroll_year_directory_name(&schedule.payroll_year)?))
}

pub fn payslip_path(
    root: &Path,
    personal_assistant_name: &str,
    schedule: &PayrollSchedule,
) -> Result<PathBuf, Box<dyn Error>> {
    Ok(payroll_year_directory(root, schedule)?
        .join(payslip_filename(personal_assistant_name, schedule)?))
}

/// Accept only an explicit consecutive tax-year range, never a week/cycle guess.
pub fn document_year(filename: &str) -> Option<String> {
    let mut years = std::collections::HashSet::new();
    for range in filename.split(|c: char| !c.is_ascii_digit() && c != '-' && c != '/') {
        let range = range.trim_matches(['-', '/']);
        let Some((start, end)) = range.split_once(['-', '/']) else {
            continue;
        };
        if start.len() != 4 {
            continue;
        }
        let Ok(start) = start.parse::<i32>() else {
            continue;
        };
        if !(1900..=9998).contains(&start) {
            continue;
        }
        if (end.len() == 2 && end.parse::<i32>().ok() == Some((start + 1) % 100))
            || (end.len() == 4 && end.parse::<i32>().ok() == Some(start + 1))
        {
            years.insert(format!("{start}/{:02}", (start + 1) % 100));
        }
    }
    if years.len() == 1 {
        years.into_iter().next()
    } else {
        None
    }
}

/// Reuse the canonical PAYE-week calculation only with an explicit tax year
/// and the provider's exact "for Week N for" form on every ordinary payslip.
pub fn infer_payslip_schedule<'a>(
    names: &[String],
    schedules: &'a [PayrollSchedule],
) -> Option<&'a PayrollSchedule> {
    let mut selected: Option<&PayrollSchedule> = None;
    for name in names {
        let year = document_year(name)?;
        let week = provider_payslip_week(name)?;
        let mut candidates = schedules
            .iter()
            .filter(|s| s.payroll_year == year && paye_week(s).ok() == Some(week));
        let candidate = candidates.next()?;
        if candidates.next().is_some() || selected.is_some_and(|s| s.id != candidate.id) {
            return None;
        }
        selected = Some(candidate);
    }
    selected
}

pub fn provider_payslip_week(name: &str) -> Option<u32> {
    let tokens: Vec<_> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let weeks: Vec<u32> = tokens
        .windows(4)
        .filter(|w| {
            w[0].eq_ignore_ascii_case("for")
                && w[1].eq_ignore_ascii_case("week")
                && w[3].eq_ignore_ascii_case("for")
        })
        .filter_map(|w| w[2].parse().ok())
        .filter(|w| (1..=53).contains(w))
        .collect();
    match weeks.as_slice() {
        [week] => Some(*week),
        _ => None,
    }
}

/// A yearless document cannot establish a year by matching a week alone.
/// Offer only compatible periods that have started; future recurring weeks do
/// not supply evidence that an undated returned payslip belongs to a future year.
pub fn plausible_payslip_schedules(
    name: &str,
    schedules: &[PayrollSchedule],
    today: NaiveDate,
) -> Vec<PayrollSchedule> {
    let year = document_year(name);
    let week = provider_payslip_week(name);
    schedules
        .iter()
        .filter(|s| {
            year.as_ref().is_none_or(|y| *y == s.payroll_year)
                && week.is_none_or(|w| paye_week(s).ok() == Some(w))
                && (year.is_some()
                    || crate::date_utils::parse_legacy(&s.first_week_commencing)
                        .is_ok_and(|start| start <= today))
        })
        .cloned()
        .collect()
}

/// PA documents without cycle association never use the flat canonical payslip
/// lookup directory. Strip a configured year suffix before applying their own year.
pub fn independent_pa_document_directory(
    root: &Path,
    pa: i64,
    year: Option<&str>,
) -> Result<PathBuf, Box<dyn Error>> {
    let base = root_without_payroll_year_suffix(root);
    Ok(match year {
        Some(year) => base.join(payroll_year_directory_name(year)?),
        None => base.to_path_buf(),
    }
    .join(format!("PA {pa}")))
}

pub fn archival_payslip_filename(filename: &str) -> String {
    // Strip only a provider prefix before an ordinary payslip heading; retain
    // all week/year/PA/revision text. Unrecognised headings keep the full name.
    let lower = filename.to_ascii_lowercase();
    for heading in ["employee payslip", "payslip"] {
        if let Some(index) = lower.match_indices(heading).find_map(|(i, _)| {
            (i == 0 || !lower.as_bytes()[i - 1].is_ascii_alphanumeric()).then_some(i)
        }) {
            return filename[index..].to_string();
        }
    }
    filename.to_string()
}

/// Without explicit year evidence, keep the configured root rather than dating the document.
pub fn document_year_directory(root: &Path, year: Option<&str>) -> Result<PathBuf, Box<dyn Error>> {
    match year {
        Some(year) => {
            Ok(root_without_payroll_year_suffix(root).join(payroll_year_directory_name(year)?))
        }
        None => Ok(root.to_path_buf()),
    }
}

pub fn timesheet_path(
    root: &Path,
    personal_assistant_name: &str,
    schedule: &PayrollSchedule,
) -> Result<PathBuf, Box<dyn Error>> {
    let output_root = if has_payroll_year_suffix(root) {
        payroll_year_directory(root, schedule)?
    } else {
        root.to_path_buf()
    };
    Ok(output_root.join(timesheet_filename(personal_assistant_name, schedule)?))
}

fn root_without_payroll_year_suffix(root: &Path) -> &Path {
    if has_payroll_year_suffix(root) {
        root.parent().unwrap_or_else(|| Path::new(""))
    } else {
        root
    }
}

fn has_payroll_year_suffix(root: &Path) -> bool {
    root.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(is_payroll_year_directory_name)
}

fn is_payroll_year_directory_name(value: &str) -> bool {
    let Some((start, end)) = value.split_once(" to ") else {
        return false;
    };
    if start.len() != 4
        || end.len() != 4
        || !start.bytes().all(|value| value.is_ascii_digit())
        || !end.bytes().all(|value| value.is_ascii_digit())
    {
        return false;
    }
    start
        .parse::<i32>()
        .ok()
        .zip(end.parse::<i32>().ok())
        .is_some_and(|(start, end)| end == start + 1)
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
    #[test]
    fn independent_document_year_requires_unambiguous_consecutive_years() {
        for name in [
            "P60 year 2025-26.pdf",
            "P60 year 2025/26.pdf",
            "P60 year 2025-2026.pdf",
        ] {
            assert_eq!(document_year(name).as_deref(), Some("2025/26"));
        }
        for name in [
            "P45 leaving.pdf",
            "Week 50.pdf",
            "Quarter End Memo April 2026.pdf",
            "P60 2025-27.pdf",
            "P60 2025 [26].pdf",
            "P60 2025 26.pdf",
            "P60 2025-26 and 2026-27.pdf",
        ] {
            assert_eq!(document_year(name), None);
        }
        let root = Path::new("payslips/2030 to 2031");
        assert_eq!(
            document_year_directory(root, Some("2025/26")).unwrap(),
            Path::new("payslips/2025 to 2026")
        );
        assert_eq!(document_year_directory(root, None).unwrap(), root);
    }

    #[test]
    fn equivalent_calendar_formats_preserve_machine_names_and_paye_week() {
        let mut item = schedule(
            "2026/27",
            6,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(),
        );
        let original = timesheet_filename("Birch Sample", &item).unwrap();
        let week = paye_week(&item).unwrap();
        for format in crate::date_utils::DateDisplayFormat::ALL {
            item.first_week_commencing =
                format.format(NaiveDate::from_ymd_opt(2026, 8, 10).unwrap());
            item.pay_date = format.format(NaiveDate::from_ymd_opt(2026, 9, 4).unwrap());
            assert_eq!(
                timesheet_filename("Birch Sample", &item).unwrap(),
                original
            );
            assert_eq!(paye_week(&item).unwrap(), week);
        }
    }

    use super::*;

    fn schedule(
        year: &str,
        cycle: i64,
        first_week: NaiveDate,
        pay_date: NaiveDate,
    ) -> PayrollSchedule {
        PayrollSchedule {
            id: cycle,
            payroll_year: year.to_string(),
            cycle_number: cycle,
            first_week_commencing: first_week.format("%d/%m/%Y").to_string(),
            latest_posting_date: (first_week + chrono::Duration::days(18))
                .format("%d/%m/%Y")
                .to_string(),
            pay_date: pay_date.format("%d/%m/%Y").to_string(),
            created_at: String::new(),
            payslips_sent: false,
        }
    }

    fn sequence(year: &str, first_week_number: u32, tax_year_start: NaiveDate) -> Vec<u32> {
        (0..13)
            .map(|cycle| {
                let pay = tax_year_start
                    + chrono::Duration::days(((first_week_number - 1) * 7 + cycle * 28) as i64);
                let item = schedule(
                    year,
                    cycle as i64 + 1,
                    pay - chrono::Duration::days(25),
                    pay,
                );
                paye_week(&item).unwrap()
            })
            .collect()
    }

    #[test]
    fn historical_paye_week_sequences_are_preserved() {
        assert_eq!(
            sequence("2018/19", 4, NaiveDate::from_ymd_opt(2018, 4, 6).unwrap()),
            vec![4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52]
        );
        assert_eq!(
            sequence("2023/24", 3, NaiveDate::from_ymd_opt(2023, 4, 6).unwrap()),
            vec![3, 7, 11, 15, 19, 23, 27, 31, 35, 39, 43, 47, 51]
        );
        assert_eq!(
            sequence("2026/27", 2, NaiveDate::from_ymd_opt(2026, 4, 6).unwrap()),
            vec![2, 6, 10, 14, 18, 22, 26, 30, 34, 38, 42, 46, 50]
        );
    }

    #[test]
    fn january_to_march_uses_previous_april_sixth() {
        let item = schedule(
            "2026/27",
            13,
            NaiveDate::from_ymd_opt(2027, 2, 22).unwrap(),
            NaiveDate::from_ymd_opt(2027, 3, 19).unwrap(),
        );
        assert_eq!(paye_week(&item).unwrap(), 50);
    }

    #[test]
    fn calculation_naturally_allows_week_53() {
        let item = schedule(
            "2020/21",
            13,
            NaiveDate::from_ymd_opt(2021, 3, 11).unwrap(),
            NaiveDate::from_ymd_opt(2021, 4, 5).unwrap(),
        );
        assert_eq!(paye_week(&item).unwrap(), 53);
    }

    #[test]
    fn current_period_code_and_shared_paths_match_established_names() {
        let item = schedule(
            "2026/27",
            6,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(),
        );
        assert_eq!(payroll_period_code(&item).unwrap(), "202608w22");
        assert_eq!(
            payslip_path(Path::new("/payslips"), "Cedar Fixture", &item).unwrap(),
            Path::new("/payslips/2026 to 2027/Payslip for Week 22 for Cedar Fixture.pdf")
        );
    }

    #[test]
    fn base_root_without_year_suffix_appends_schedule_year() {
        let item = schedule(
            "2026/27",
            6,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(),
        );

        assert_eq!(
            payroll_year_directory(Path::new("/Personal Budgets/Payslips"), &item).unwrap(),
            Path::new("/Personal Budgets/Payslips/2026 to 2027")
        );
    }

    #[test]
    fn matching_existing_year_suffix_is_not_duplicated() {
        let item = schedule(
            "2026/27",
            6,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(),
        );

        assert_eq!(
            payslip_path(
                Path::new("/Personal Budgets/Payslips/2026 to 2027"),
                "Cedar Fixture",
                &item,
            )
            .unwrap(),
            Path::new(
                "/Personal Budgets/Payslips/2026 to 2027/Payslip for Week 22 for Cedar Fixture.pdf"
            )
        );
    }

    #[test]
    fn old_year_suffix_is_replaced_for_future_schedule() {
        let item = schedule(
            "2027/28",
            6,
            NaiveDate::from_ymd_opt(2027, 8, 9).unwrap(),
            NaiveDate::from_ymd_opt(2027, 9, 3).unwrap(),
        );

        assert_eq!(
            payroll_year_directory(Path::new("/Personal Budgets/Payslips/2026 to 2027"), &item,)
                .unwrap(),
            Path::new("/Personal Budgets/Payslips/2027 to 2028")
        );
    }

    #[test]
    fn timesheet_root_replaces_existing_year_but_keeps_generic_root_flat() {
        let item = schedule(
            "2027/28",
            6,
            NaiveDate::from_ymd_opt(2027, 8, 9).unwrap(),
            NaiveDate::from_ymd_opt(2027, 9, 3).unwrap(),
        );

        assert_eq!(
            timesheet_path(Path::new("/Timesheets"), "Cedar Fixture", &item).unwrap(),
            Path::new("/Timesheets/Timesheet - Cedar Fixture - 202708w22.pdf")
        );
        assert_eq!(
            timesheet_path(
                Path::new("/Timesheets/2026 to 2027"),
                "Cedar Fixture",
                &item,
            )
            .unwrap(),
            Path::new("/Timesheets/2027 to 2028/Timesheet - Cedar Fixture - 202708w22.pdf")
        );
    }

    #[test]
    fn payslip_filename_sanitises_maintained_name_path_characters() {
        let item = schedule(
            "2026/27",
            6,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(),
        );

        assert_eq!(
            payslip_filename("Alex/Smith:Payroll", &item).unwrap(),
            "Payslip for Week 22 for Alex_Smith_Payroll.pdf"
        );
    }

    #[test]
    fn portable_default_payslip_root_expands_then_remains_year_aware() {
        let item = schedule(
            "2026/27",
            6,
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(),
        );
        let root = crate::paths::expand_path_with_home(
            Path::new("~/Documents/DirectPaymentTimesheets/payslips"),
            Some(Path::new("/srv/arbitrary-user")),
        );

        assert_eq!(
            payslip_path(&root, "Alex Smith", &item).unwrap(),
            Path::new(
                "/srv/arbitrary-user/Documents/DirectPaymentTimesheets/payslips/2026 to 2027/Payslip for Week 22 for Alex Smith.pdf"
            )
        );
    }
}
