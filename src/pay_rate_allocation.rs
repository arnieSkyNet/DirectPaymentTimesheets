use std::fmt;

use chrono::NaiveDate;

use crate::models::TimesheetEntry;
use crate::pay_rate_repository::PayRateRepository;
use crate::payroll_worked_item_repository::{PayrollWorkedItemRepository, WorkedItemSnapshot};

#[derive(Debug, Clone, PartialEq)]
pub struct PayRatePortion {
    pub worked_minutes: i64,
    pub total_hourly_rate: Option<f64>,
    pub effective_date: Option<NaiveDate>,
    pub(crate) rate_id: Option<i64>,
    pub is_previous_cycle: bool,
    pub is_opaque_legacy: bool,
}

#[derive(Debug, Clone)]
pub struct PreviousCycleContext {
    pub payroll_timesheet_id: i64,
    pub week_three_start: NaiveDate,
    pub legacy_adjustment_minutes: i64,
}

#[derive(Debug, Clone)]
pub struct ReconciledPayrollHours {
    #[allow(dead_code)]
    pub weeks: [Vec<PayRatePortion>; 4],
    pub week_totals_minutes: [i64; 4],
    pub previous_cycle_minutes: i64,
    pub snapshot_items: Vec<WorkedItemSnapshot>,
}

#[derive(Debug)]
pub struct PayRateAllocationError(String);

impl fmt::Display for PayRateAllocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PayRateAllocationError {}

#[cfg(test)]
pub fn allocate_worked_hours_by_rate(
    repository: &PayRateRepository,
    timesheets: &[TimesheetEntry],
    personal_assistant_id: i64,
    week_dates: &[NaiveDate; 4],
) -> Result<[Vec<PayRatePortion>; 4], PayRateAllocationError> {
    let mut weeks: [Vec<PayRatePortion>; 4] = std::array::from_fn(|_| Vec::new());

    for timesheet in timesheets.iter().filter(|timesheet| {
        timesheet.personal_assistant_id == Some(personal_assistant_id)
            && timesheet.worked_minutes > 0
    }) {
        let worked_date = match parse_timesheet_date(&timesheet.start_time) {
            Some(date) => date,
            None => continue,
        };
        let week_index = (0..4).find(|index| {
            worked_date >= week_dates[*index]
                && worked_date <= week_dates[*index] + chrono::Duration::days(6)
        });
        let Some(week_index) = week_index else {
            continue;
        };

        let rate = repository
            .get_for_personal_assistant_as_of(personal_assistant_id, worked_date)
            .map_err(|error| {
                PayRateAllocationError(format!(
                    "Could not look up the pay rate for {}: {error}",
                    worked_date.format("%d/%m/%Y")
                ))
            })?
            .ok_or_else(|| {
                PayRateAllocationError(format!(
                    "No pay rate is effective for worked date {}.",
                    worked_date.format("%d/%m/%Y")
                ))
            })?;
        let effective_date =
            NaiveDate::parse_from_str(&rate.effective_date, "%d/%m/%Y").map_err(|_| {
                PayRateAllocationError(format!(
                    "Pay rate {} has invalid effective date {}.",
                    rate.id, rate.effective_date
                ))
            })?;
        let portions = &mut weeks[week_index];
        if let Some(portion) = portions
            .iter_mut()
            .find(|portion| portion.rate_id == Some(rate.id))
        {
            portion.worked_minutes += timesheet.worked_minutes;
        } else {
            portions.push(PayRatePortion {
                worked_minutes: timesheet.worked_minutes,
                total_hourly_rate: Some(rate.base_hourly_rate + rate.employer_top_up_rate),
                effective_date: Some(effective_date),
                rate_id: Some(rate.id),
                is_previous_cycle: false,
                is_opaque_legacy: false,
            });
        }
    }

    for portions in &mut weeks {
        portions.sort_by_key(|portion| (portion.effective_date, portion.rate_id));
    }

    Ok(weeks)
}

pub fn reconcile_payroll_hours(
    pay_rates: &PayRateRepository,
    worked_items: &PayrollWorkedItemRepository,
    timesheets: &[TimesheetEntry],
    personal_assistant_id: i64,
    current_payroll_timesheet_id: i64,
    week_dates: &[NaiveDate; 4],
    previous_cycle: Option<&PreviousCycleContext>,
) -> Result<ReconciledPayrollHours, PayRateAllocationError> {
    let mut weeks: [Vec<PayRatePortion>; 4] = std::array::from_fn(|_| Vec::new());
    let mut snapshot_items = Vec::new();

    for timesheet in timesheets.iter().filter(|entry| {
        entry.personal_assistant_id == Some(personal_assistant_id) && entry.worked_minutes > 0
    }) {
        let Some(work_date) = parse_timesheet_date(&timesheet.start_time) else {
            continue;
        };
        let Some(week_index) = week_index_for_date(work_date, week_dates) else {
            continue;
        };
        add_raw_item(
            pay_rates,
            &mut weeks[week_index],
            &mut snapshot_items,
            personal_assistant_id,
            timesheet,
            work_date,
            week_index,
            "imported_shift",
            false,
        )?;
    }

    let mut previous_cycle_minutes = 0;
    if let Some(previous) = previous_cycle {
        let previous_state = worked_items
            .snapshot_metadata(previous.payroll_timesheet_id)
            .map_err(repository_error)?
            .map(|metadata| metadata.state);
        if previous_state == Some(crate::payroll_worked_item_repository::SnapshotState::Submitted) {
            let submitted_ids = worked_items
                .submitted_snapshot_timesheet_ids(previous.payroll_timesheet_id)
                .map_err(repository_error)?;
            for timesheet in timesheets.iter().filter(|entry| {
                entry.personal_assistant_id == Some(personal_assistant_id)
                    && entry.worked_minutes > 0
                    && !submitted_ids.contains(&entry.id)
            }) {
                let Some(work_date) = parse_timesheet_date(&timesheet.start_time) else {
                    continue;
                };
                if work_date < previous.week_three_start || work_date >= week_dates[0] {
                    continue;
                }
                add_raw_item(
                    pay_rates,
                    &mut weeks[0],
                    &mut snapshot_items,
                    personal_assistant_id,
                    timesheet,
                    work_date,
                    0,
                    "previous_cycle_late_shift",
                    true,
                )?;
                previous_cycle_minutes += timesheet.worked_minutes;
            }
        } else if previous_state.is_none() && previous.legacy_adjustment_minutes > 0 {
            weeks[0].push(PayRatePortion {
                worked_minutes: previous.legacy_adjustment_minutes,
                total_hourly_rate: None,
                effective_date: None,
                rate_id: None,
                is_previous_cycle: true,
                is_opaque_legacy: true,
            });
            snapshot_items.push(WorkedItemSnapshot {
                week_number: 1,
                source_type: "legacy_previous_cycle_adjustment".to_string(),
                timesheet_id: None,
                direct_shift_id: None,
                source_evidence: None,
                work_date: None,
                worked_minutes: previous.legacy_adjustment_minutes,
                pay_rate_id: None,
                pay_rate_effective_date: None,
                total_hourly_rate: None,
                reason: Some(
                    "Preserved version-18 aggregate; historical rate allocation unavailable"
                        .to_string(),
                ),
            });
            previous_cycle_minutes = previous.legacy_adjustment_minutes;
        }
    }

    let mut excludes_previous_cycle_from_week_totals = false;
    for adjustment in worked_items
        .get_manual_adjustments(current_payroll_timesheet_id)
        .map_err(repository_error)?
    {
        let is_historical_backfill =
            crate::historical_payroll_backfill::is_backfill_reason(adjustment.reason.as_deref());
        excludes_previous_cycle_from_week_totals |= is_historical_backfill;
        if adjustment.adjustment_minutes == 0 || !(1..=4).contains(&adjustment.week_number) {
            continue;
        }
        let index = (adjustment.week_number - 1) as usize;
        let rate = select_manual_adjustment_rate(
            pay_rates,
            personal_assistant_id,
            week_dates[index],
            week_dates[index] + chrono::Duration::days(6),
            adjustment.adjustment_minutes,
        )?;
        let Some(rate) = rate else {
            if !is_historical_backfill {
                return Err(no_applicable_rate_for_week(
                    week_dates[index],
                    week_dates[index] + chrono::Duration::days(6),
                ));
            }
            weeks[index].push(PayRatePortion {
                worked_minutes: adjustment.adjustment_minutes,
                total_hourly_rate: None,
                effective_date: None,
                rate_id: None,
                is_previous_cycle: false,
                is_opaque_legacy: true,
            });
            snapshot_items.push(WorkedItemSnapshot {
                week_number: adjustment.week_number,
                source_type: "manual_adjustment".to_string(),
                timesheet_id: None,
                direct_shift_id: None,
                source_evidence: None,
                work_date: None,
                worked_minutes: adjustment.adjustment_minutes,
                pay_rate_id: None,
                pay_rate_effective_date: None,
                total_hourly_rate: None,
                reason: adjustment.reason,
            });
            continue;
        };
        add_portion(
            &mut weeks[index],
            adjustment.adjustment_minutes,
            &rate,
            false,
        )?;
        snapshot_items.push(snapshot_from_rate(
            adjustment.week_number,
            "manual_adjustment",
            None,
            None,
            adjustment.adjustment_minutes,
            &rate,
            adjustment.reason,
        ));
    }

    let week_totals_minutes = current_cycle_week_totals(
        &weeks,
        previous_cycle_minutes,
        excludes_previous_cycle_from_week_totals,
    );
    Ok(ReconciledPayrollHours {
        weeks,
        week_totals_minutes,
        previous_cycle_minutes,
        snapshot_items,
    })
}

fn current_cycle_week_totals(
    weeks: &[Vec<PayRatePortion>; 4],
    previous_cycle_minutes: i64,
    excludes_previous_cycle: bool,
) -> [i64; 4] {
    let mut totals = std::array::from_fn(|index| {
        weeks[index]
            .iter()
            .map(|portion| portion.worked_minutes)
            .sum()
    });
    if excludes_previous_cycle {
        totals[0] -= previous_cycle_minutes;
    }
    totals
}

fn add_raw_item(
    pay_rates: &PayRateRepository,
    portions: &mut Vec<PayRatePortion>,
    snapshots: &mut Vec<WorkedItemSnapshot>,
    personal_assistant_id: i64,
    timesheet: &TimesheetEntry,
    work_date: NaiveDate,
    week_index: usize,
    source_type: &str,
    is_previous_cycle: bool,
) -> Result<(), PayRateAllocationError> {
    let rate = pay_rates
        .get_for_personal_assistant_as_of(personal_assistant_id, work_date)
        .map_err(repository_error)?
        .ok_or_else(|| {
            PayRateAllocationError(format!(
                "No pay rate is effective for worked date {}.",
                work_date.format("%d/%m/%Y")
            ))
        })?;
    add_portion(portions, timesheet.worked_minutes, &rate, is_previous_cycle)?;
    snapshots.push(snapshot_from_rate(
        (week_index + 1) as i64,
        source_type,
        Some(timesheet.id),
        Some(work_date.format("%Y-%m-%d").to_string()),
        timesheet.worked_minutes,
        &rate,
        None,
    ));
    Ok(())
}

/// Round payable duration only; source intervals and raw minutes stay untouched.
fn payable_minutes(
    minutes: i64,
    settings: &crate::config::PayrollConfig,
) -> Result<i64, PayRateAllocationError> {
    let increment = settings.rounding_minutes;
    if minutes < 0 || ![1, 5, 10, 15, 30, 60].contains(&increment) {
        return Err(PayRateAllocationError(
            "Invalid payroll duration or rounding increment".into(),
        ));
    }
    let remainder = minutes % increment;
    match settings.rounding_direction.as_str() {
        "Down" => Ok(minutes - remainder),
        "Up" => minutes
            .checked_add(if remainder == 0 {
                0
            } else {
                increment - remainder
            })
            .ok_or_else(|| PayRateAllocationError("Payroll rounding overflow".into())),
        _ => Err(PayRateAllocationError(
            "Invalid payroll rounding direction".into(),
        )),
    }
}

/// Allocates already selected evidence; duplicate and lifecycle policy stays upstream.
pub fn add_evidence(
    rates: &PayRateRepository,
    result: &mut ReconciledPayrollHours,
    evidence: &crate::payroll_evidence::WorkEvidence,
    week: usize,
    late: bool,
    settings: &crate::config::PayrollConfig,
) -> Result<(), PayRateAllocationError> {
    let minutes = payable_minutes(evidence.minutes, settings)?;
    let date = evidence
        .date()
        .map_err(|e| PayRateAllocationError(e.to_string()))?;
    let rate = rates
        .get_for_personal_assistant_as_of(evidence.pa, date)
        .map_err(repository_error)?
        .ok_or_else(|| {
            PayRateAllocationError(format!("No pay rate is effective for worked date {date}."))
        })?;
    add_portion(&mut result.weeks[week], minutes, &rate, late)?;
    let mut item = snapshot_from_rate(
        (week + 1) as i64,
        if evidence.source == "direct" {
            "direct_shift"
        } else if late {
            "previous_cycle_late_shift"
        } else {
            "imported_shift"
        },
        (evidence.source == "imported").then_some(evidence.id),
        Some(date.to_string()),
        minutes,
        &rate,
        None,
    );
    item.direct_shift_id = (evidence.source == "direct").then_some(evidence.id);
    item.source_evidence =
        Some(toml::to_string(evidence).map_err(|e| PayRateAllocationError(e.to_string()))?);
    result.snapshot_items.push(item);
    result.week_totals_minutes[week] = result.week_totals_minutes[week]
        .checked_add(minutes)
        .ok_or_else(|| PayRateAllocationError("Worked minutes overflow".into()))?;
    if late {
        result.previous_cycle_minutes += minutes;
    }
    Ok(())
}

fn add_portion(
    portions: &mut Vec<PayRatePortion>,
    minutes: i64,
    rate: &crate::pay_rate_repository::PersonalAssistantPayRate,
    is_previous_cycle: bool,
) -> Result<(), PayRateAllocationError> {
    let effective_date = NaiveDate::parse_from_str(&rate.effective_date, "%d/%m/%Y")
        .map_err(|_| PayRateAllocationError("Invalid pay-rate effective date.".into()))?;
    if let Some(portion) = portions.iter_mut().find(|portion| {
        portion.rate_id == Some(rate.id) && portion.is_previous_cycle == is_previous_cycle
    }) {
        portion.worked_minutes += minutes;
    } else {
        portions.push(PayRatePortion {
            worked_minutes: minutes,
            total_hourly_rate: Some(rate.base_hourly_rate + rate.employer_top_up_rate),
            effective_date: Some(effective_date),
            rate_id: Some(rate.id),
            is_previous_cycle,
            is_opaque_legacy: false,
        });
    }
    portions.sort_by_key(|portion| {
        (
            portion.is_previous_cycle,
            portion.effective_date,
            portion.rate_id,
        )
    });
    Ok(())
}

fn snapshot_from_rate(
    week_number: i64,
    source_type: &str,
    timesheet_id: Option<i64>,
    work_date: Option<String>,
    worked_minutes: i64,
    rate: &crate::pay_rate_repository::PersonalAssistantPayRate,
    reason: Option<String>,
) -> WorkedItemSnapshot {
    WorkedItemSnapshot {
        week_number,
        source_type: source_type.to_string(),
        timesheet_id,
        direct_shift_id: None,
        source_evidence: None,
        work_date,
        worked_minutes,
        pay_rate_id: Some(rate.id),
        pay_rate_effective_date: Some(rate.effective_date.clone()),
        total_hourly_rate: Some(rate.base_hourly_rate + rate.employer_top_up_rate),
        reason,
    }
}

fn select_manual_adjustment_rate(
    pay_rates: &PayRateRepository,
    personal_assistant_id: i64,
    week_start: NaiveDate,
    week_end: NaiveDate,
    adjustment_minutes: i64,
) -> Result<Option<crate::pay_rate_repository::PersonalAssistantPayRate>, PayRateAllocationError> {
    let rates = pay_rates
        .get_applicable_during_period(personal_assistant_id, week_start, week_end)
        .map_err(repository_error)?;
    let comparison = |rate: &&crate::pay_rate_repository::PersonalAssistantPayRate| {
        let pennies = ((rate.base_hourly_rate + rate.employer_top_up_rate) * 100.0).round() as i64;
        (pennies, rate.id)
    };
    let selected = if adjustment_minutes > 0 {
        rates.iter().max_by_key(comparison)
    } else {
        rates.iter().min_by_key(comparison)
    };
    Ok(selected.cloned())
}

fn no_applicable_rate_for_week(
    week_start: NaiveDate,
    week_end: NaiveDate,
) -> PayRateAllocationError {
    PayRateAllocationError(format!(
        "No pay rate is applicable during payroll week {} to {}.",
        week_start.format("%d/%m/%Y"),
        week_end.format("%d/%m/%Y")
    ))
}

fn week_index_for_date(date: NaiveDate, week_dates: &[NaiveDate; 4]) -> Option<usize> {
    (0..4).find(|index| {
        date >= week_dates[*index] && date <= week_dates[*index] + chrono::Duration::days(6)
    })
}

fn repository_error(error: rusqlite::Error) -> PayRateAllocationError {
    PayRateAllocationError(error.to_string())
}

#[cfg(test)]
pub fn format_week_rate_portions(portions: &[PayRatePortion], cycle_start: NaiveDate) -> String {
    if portions.is_empty() {
        return "0".to_string();
    }

    let target_hundredths =
        minutes_to_hundredths(portions.iter().map(|portion| portion.worked_minutes).sum());
    let mut allocated_hundredths = 0;
    portions
        .iter()
        .enumerate()
        .map(|(index, portion)| {
            let hundredths = if index + 1 == portions.len() {
                target_hundredths - allocated_hundredths
            } else {
                minutes_to_hundredths(portion.worked_minutes)
            };
            allocated_hundredths += hundredths;
            let hours = format_hundredths(hundredths);
            if portion.is_opaque_legacy {
                return format!(
                    "{} (previous-cycle adjustment; historical rate allocation unavailable)",
                    hours
                );
            }
            let previous = if portion.is_previous_cycle {
                "; previous cycle"
            } else {
                ""
            };
            let effective_date = portion
                .effective_date
                .expect("allocated pay-rate portion has an effective date");
            let total_hourly_rate = portion
                .total_hourly_rate
                .expect("allocated pay-rate portion has an hourly rate");
            if effective_date >= cycle_start {
                format!(
                    "{} (£{:.2} from {}{})",
                    hours,
                    total_hourly_rate,
                    effective_date.format("%d/%m/%Y"),
                    previous
                )
            } else {
                format!("{} (£{:.2}{})", hours, total_hourly_rate, previous)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_total_minutes(minutes: i64) -> String {
    format_hundredths(minutes_to_hundredths(minutes))
}

fn minutes_to_hundredths(minutes: i64) -> i64 {
    (minutes as f64 * 100.0 / 60.0).round() as i64
}

fn format_hundredths(hundredths: i64) -> String {
    let sign = if hundredths < 0 { "-" } else { "" };
    let absolute = hundredths.abs();
    if absolute % 100 == 0 {
        format!("{sign}{}", absolute / 100)
    } else {
        format!("{sign}{}.{:02}", absolute / 100, absolute % 100)
            .trim_end_matches('0')
            .to_string()
    }
}

pub(crate) fn parse_timesheet_date(value: &str) -> Option<NaiveDate> {
    let value = value.trim();
    NaiveDate::parse_from_str(value, "%d/%m/%Y")
        .or_else(|_| NaiveDate::parse_from_str(value, "%d-%m-%Y"))
        .or_else(|_| NaiveDate::parse_from_str(value, "%Y-%m-%d"))
        .ok()
        .or_else(|| {
            let date = value.split(" at ").next()?.trim();
            NaiveDate::parse_from_str(date, "%d %B %Y")
                .or_else(|_| NaiveDate::parse_from_str(date, "%d %b %Y"))
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    use crate::database::create_schema;
    use crate::pay_rate_repository::PersonalAssistantPayRate;
    use crate::payroll_worked_item_repository::{
        ManualHoursAdjustment, PayrollWorkedItemRepository,
    };

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn repository_with_rates(rates: &[(&str, f64, f64)]) -> PayRateRepository {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        let repository = PayRateRepository::new(connection);
        for (effective_date, base, top_up) in rates {
            repository
                .insert(&PersonalAssistantPayRate {
                    id: 0,
                    personal_assistant_id: 1,
                    effective_date: effective_date.to_string(),
                    base_hourly_rate: *base,
                    employer_top_up_rate: *top_up,
                    created_at: "2026-01-01".to_string(),
                })
                .unwrap();
        }
        repository
    }

    fn shift_with_id(id: i64, date: &str, worked_minutes: i64) -> TimesheetEntry {
        TimesheetEntry {
            id,
            pa_name: "Test PA".to_string(),
            personal_assistant_id: Some(1),
            start_time: format!("{date} at 09:00"),
            end_time: format!("{date} at 17:00"),
            break_minutes: 0,
            worked_minutes,
            hourly_rate: 999.0,
            amount: 999.0,
            notes: None,
        }
    }

    fn shift(date: &str, worked_minutes: i64) -> TimesheetEntry {
        shift_with_id(0, date, worked_minutes)
    }

    fn shared_repositories(
        rates: &[(&str, f64, f64)],
    ) -> (
        NamedTempFile,
        PayRateRepository,
        PayrollWorkedItemRepository,
    ) {
        let file = NamedTempFile::new().unwrap();
        let setup = Connection::open(file.path()).unwrap();
        create_schema(&setup).unwrap();
        drop(setup);
        let pay_rates = PayRateRepository::new(Connection::open(file.path()).unwrap());
        for (effective_date, base, top_up) in rates {
            pay_rates
                .insert(&PersonalAssistantPayRate {
                    id: 0,
                    personal_assistant_id: 1,
                    effective_date: effective_date.to_string(),
                    base_hourly_rate: *base,
                    employer_top_up_rate: *top_up,
                    created_at: "2026-01-01".to_string(),
                })
                .unwrap();
        }
        let worked_items = PayrollWorkedItemRepository::new(Connection::open(file.path()).unwrap());
        (file, pay_rates, worked_items)
    }

    fn persist_submitted_snapshot(
        repository: &PayrollWorkedItemRepository,
        payroll_timesheet_id: i64,
        items: &[WorkedItemSnapshot],
    ) {
        repository
            .replace_candidate(
                payroll_timesheet_id,
                items,
                "/tmp/test.pdf",
                "digest",
                "2027-03-27",
                0,
                &[0; 4],
                &[0; 4],
            )
            .unwrap();
        assert!(repository
            .protect_for_send(payroll_timesheet_id, "2027-03-27")
            .unwrap());
        repository
            .mark_submitted_and_email_sent(
                payroll_timesheet_id,
                1,
                "2026/27",
                payroll_timesheet_id,
                "2027-03-27",
            )
            .unwrap();
    }

    #[test]
    fn cycle_crossing_rate_change_groups_different_weeks_at_authoritative_rates() {
        let repository =
            repository_with_rates(&[("01/04/2026", 12.0, 0.21), ("05/04/2027", 12.5, 0.21)]);
        let weeks = [
            date("2027-03-15"),
            date("2027-03-22"),
            date("2027-03-29"),
            date("2027-04-05"),
        ];
        let timesheets = vec![shift("24 March 2027", 600), shift("07 April 2027", 360)];

        let portions = allocate_worked_hours_by_rate(&repository, &timesheets, 1, &weeks).unwrap();

        assert_eq!(
            format_week_rate_portions(&portions[1], weeks[0]),
            "10 (£12.21)"
        );
        assert_eq!(
            format_week_rate_portions(&portions[3], weeks[0]),
            "6 (£12.71 from 05/04/2027)"
        );
    }

    #[test]
    fn midweek_rate_change_splits_hours_by_actual_shift_date() {
        let repository =
            repository_with_rates(&[("01/04/2026", 12.0, 0.21), ("31/03/2027", 12.5, 0.21)]);
        let weeks = [
            date("2027-03-29"),
            date("2027-04-05"),
            date("2027-04-12"),
            date("2027-04-19"),
        ];
        let timesheets = vec![shift("30 March 2027", 600), shift("31 March 2027", 360)];

        let portions = allocate_worked_hours_by_rate(&repository, &timesheets, 1, &weeks).unwrap();

        assert_eq!(
            format_week_rate_portions(&portions[0], weeks[0]),
            "10 (£12.21)\n6 (£12.71 from 31/03/2027)"
        );
    }

    #[test]
    fn worked_hours_before_first_rate_are_refused_but_unused_earlier_dates_are_not() {
        let repository = repository_with_rates(&[("31/03/2027", 12.5, 0.21)]);
        let weeks = [
            date("2027-03-29"),
            date("2027-04-05"),
            date("2027-04-12"),
            date("2027-04-19"),
        ];

        let error =
            allocate_worked_hours_by_rate(&repository, &[shift("30 March 2027", 60)], 1, &weeks)
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "No pay rate is effective for worked date 30/03/2027."
        );

        let portions =
            allocate_worked_hours_by_rate(&repository, &[shift("31 March 2027", 60)], 1, &weeks)
                .unwrap();
        assert!(portions[0]
            .iter()
            .any(|portion| portion.worked_minutes == 60));
    }

    #[test]
    fn snapshots_keep_distinct_same_day_shift_membership_and_copied_facts() {
        let (_file, pay_rates, worked_items) = shared_repositories(&[("01/01/2027", 12.0, 0.21)]);
        let weeks = [
            date("2027-03-01"),
            date("2027-03-08"),
            date("2027-03-15"),
            date("2027-03-22"),
        ];
        let shifts = vec![
            shift_with_id(101, "02 March 2027", 75),
            shift_with_id(102, "02 March 2027", 105),
        ];

        let result =
            reconcile_payroll_hours(&pay_rates, &worked_items, &shifts, 1, 44, &weeks, None)
                .unwrap();
        persist_submitted_snapshot(&worked_items, 44, &result.snapshot_items);

        assert_eq!(result.week_totals_minutes, [180, 0, 0, 0]);
        assert_eq!(
            worked_items
                .submitted_snapshot_timesheet_ids(44)
                .unwrap()
                .len(),
            2
        );
        let snapshot = worked_items.get_snapshot_items(44).unwrap();
        assert_eq!(snapshot[0].timesheet_id, Some(101));
        assert_eq!(snapshot[1].timesheet_id, Some(102));
        assert!(snapshot
            .iter()
            .all(|item| item.work_date.as_deref() == Some("2027-03-02")));
        assert_eq!(
            snapshot.iter().map(|item| item.worked_minutes).sum::<i64>(),
            180
        );
    }

    #[test]
    fn late_previous_cycle_shifts_are_found_by_id_and_keep_historical_rates() {
        let (_file, pay_rates, worked_items) =
            shared_repositories(&[("01/01/2027", 12.0, 0.21), ("15/03/2027", 12.5, 0.21)]);
        persist_submitted_snapshot(
            &worked_items,
            70,
            &[WorkedItemSnapshot {
                week_number: 3,
                source_type: "imported_shift".to_string(),
                timesheet_id: Some(1),
                direct_shift_id: None,
                source_evidence: None,
                work_date: Some("2027-03-10".to_string()),
                worked_minutes: 60,
                pay_rate_id: Some(1),
                pay_rate_effective_date: Some("01/01/2027".to_string()),
                total_hourly_rate: Some(12.21),
                reason: None,
            }],
        );
        let weeks = [
            date("2027-03-29"),
            date("2027-04-05"),
            date("2027-04-12"),
            date("2027-04-19"),
        ];
        let shifts = vec![
            shift_with_id(1, "10 March 2027", 60),
            shift_with_id(2, "12 March 2027", 90),
            shift_with_id(3, "16 March 2027", 120),
        ];
        let previous = PreviousCycleContext {
            payroll_timesheet_id: 70,
            week_three_start: date("2027-03-08"),
            legacy_adjustment_minutes: 999,
        };

        let result = reconcile_payroll_hours(
            &pay_rates,
            &worked_items,
            &shifts,
            1,
            71,
            &weeks,
            Some(&previous),
        )
        .unwrap();

        assert_eq!(result.previous_cycle_minutes, 210);
        assert_eq!(result.week_totals_minutes[0], 210);
        assert_eq!(result.weeks[0].len(), 2);
        assert!(result.weeks[0]
            .iter()
            .all(|portion| portion.is_previous_cycle));
        assert_eq!(result.snapshot_items.len(), 2);
        assert_eq!(result.snapshot_items[0].timesheet_id, Some(2));
        assert_eq!(result.snapshot_items[1].timesheet_id, Some(3));
        assert_eq!(result.snapshot_items[0].total_hourly_rate, Some(12.21));
        assert_eq!(result.snapshot_items[1].total_hourly_rate, Some(12.71));
    }

    #[test]
    fn manual_adjustments_choose_favourable_rate_within_week() {
        let (_file, pay_rates, worked_items) =
            shared_repositories(&[("01/01/2027", 12.0, 0.21), ("03/03/2027", 12.5, 0.21)]);
        worked_items
            .set_manual_adjustment(
                80,
                &ManualHoursAdjustment {
                    week_number: 1,
                    adjustment_minutes: 60,
                    reason: None,
                },
                "2027-03-30",
            )
            .unwrap();
        worked_items
            .set_manual_adjustment(
                80,
                &ManualHoursAdjustment {
                    week_number: 2,
                    adjustment_minutes: -30,
                    reason: Some("Employer correction".to_string()),
                },
                "2027-03-30",
            )
            .unwrap();
        let weeks = [
            date("2027-03-01"),
            date("2027-02-28"),
            date("2027-03-15"),
            date("2027-03-22"),
        ];

        let result =
            reconcile_payroll_hours(&pay_rates, &worked_items, &[], 1, 80, &weeks, None).unwrap();

        assert_eq!(result.weeks[0][0].total_hourly_rate, Some(12.71));
        assert_eq!(result.weeks[1][0].total_hourly_rate, Some(12.21));
        assert_eq!(
            result.snapshot_items[1].reason.as_deref(),
            Some("Employer correction")
        );
    }

    #[test]
    fn single_rate_manual_adjustments_use_that_rate_for_both_signs() {
        let (_file, pay_rates, worked_items) = shared_repositories(&[("01/01/2027", 12.0, 0.21)]);
        for (week_number, minutes) in [(1, 45), (2, -30)] {
            worked_items
                .set_manual_adjustment(
                    81,
                    &ManualHoursAdjustment {
                        week_number,
                        adjustment_minutes: minutes,
                        reason: None,
                    },
                    "2027-03-30",
                )
                .unwrap();
        }
        let weeks = [
            date("2027-03-01"),
            date("2027-03-08"),
            date("2027-03-15"),
            date("2027-03-22"),
        ];

        let result =
            reconcile_payroll_hours(&pay_rates, &worked_items, &[], 1, 81, &weeks, None).unwrap();

        assert_eq!(result.weeks[0][0].total_hourly_rate, Some(12.21));
        assert_eq!(result.weeks[1][0].total_hourly_rate, Some(12.21));
    }

    #[test]
    fn verified_historical_previous_cycle_hours_remain_information_only() {
        let (_file, pay_rates, worked_items) = shared_repositories(&[]);
        worked_items
            .set_manual_adjustment(
                82,
                &ManualHoursAdjustment {
                    week_number: 1,
                    adjustment_minutes: 60,
                    reason: Some("Verified historical payroll backfill (2026-09-04)".to_string()),
                },
                "2026-09-04",
            )
            .unwrap();
        let weeks = [
            date("2026-04-20"),
            date("2026-04-27"),
            date("2026-05-04"),
            date("2026-05-11"),
        ];
        let previous = PreviousCycleContext {
            payroll_timesheet_id: 81,
            week_three_start: date("2026-04-06"),
            legacy_adjustment_minutes: 120,
        };

        let result = reconcile_payroll_hours(
            &pay_rates,
            &worked_items,
            &[],
            1,
            82,
            &weeks,
            Some(&previous),
        )
        .unwrap();

        assert_eq!(result.week_totals_minutes, [60, 0, 0, 0]);
        assert_eq!(result.previous_cycle_minutes, 120);
        assert_eq!(result.weeks[0][1].total_hourly_rate, None);
        assert_eq!(result.snapshot_items[1].total_hourly_rate, None);
    }

    #[test]
    fn ordinary_manual_adjustments_still_require_an_applicable_pay_rate() {
        let (_file, pay_rates, worked_items) = shared_repositories(&[]);
        worked_items
            .set_manual_adjustment(
                83,
                &ManualHoursAdjustment {
                    week_number: 1,
                    adjustment_minutes: 60,
                    reason: Some("Ordinary correction".to_string()),
                },
                "2026-09-04",
            )
            .unwrap();
        let weeks = [
            date("2026-03-23"),
            date("2026-03-30"),
            date("2026-04-06"),
            date("2026-04-13"),
        ];

        let error = reconcile_payroll_hours(&pay_rates, &worked_items, &[], 1, 83, &weeks, None)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "No pay rate is applicable during payroll week 23/03/2026 to 29/03/2026."
        );
    }

    #[test]
    fn verified_week_two_six_and_ten_figures_reconcile_without_historical_rates() {
        let (_file, pay_rates, worked_items) = shared_repositories(&[]);
        let cases = [
            (201, date("2026-03-23"), [165, 225, 0, 315]),
            (202, date("2026-04-20"), [75, 255, 0, 0]),
            (203, date("2026-05-18"), [0, 135, 285, 345]),
            (204, date("2026-03-23"), [615, 735, 525, 645]),
            (205, date("2026-04-20"), [705, 555, 825, 465]),
            (206, date("2026-05-18"), [585, 765, 675, 435]),
            (207, date("2026-03-23"), [195, 330, 270, 210]),
            (208, date("2026-04-20"), [360, 180, 240, 300]),
            (209, date("2026-05-18"), [405, 150, 390, 120]),
        ];

        for (timesheet_id, first_week, expected) in cases {
            for (index, minutes) in expected.iter().enumerate() {
                worked_items
                    .set_manual_adjustment(
                        timesheet_id,
                        &ManualHoursAdjustment {
                            week_number: index as i64 + 1,
                            adjustment_minutes: *minutes,
                            reason: Some(
                                "Verified historical payroll backfill (2026-09-04)".to_string(),
                            ),
                        },
                        "2026-09-04",
                    )
                    .unwrap();
            }
            let weeks =
                std::array::from_fn(|index| first_week + chrono::Duration::days(index as i64 * 7));
            let result = reconcile_payroll_hours(
                &pay_rates,
                &worked_items,
                &[],
                1,
                timesheet_id,
                &weeks,
                None,
            )
            .unwrap();
            assert_eq!(result.week_totals_minutes, expected);
        }
    }

    #[test]
    fn verified_historical_adjustments_still_use_a_recorded_rate_when_available() {
        let (_file, pay_rates, worked_items) = shared_repositories(&[("01/04/2026", 12.71, 0.0)]);
        worked_items
            .set_manual_adjustment(
                210,
                &ManualHoursAdjustment {
                    week_number: 1,
                    adjustment_minutes: 60,
                    reason: Some("Verified historical payroll backfill (2026-09-04)".to_string()),
                },
                "2026-09-04",
            )
            .unwrap();
        let weeks = [
            date("2026-06-15"),
            date("2026-06-22"),
            date("2026-06-29"),
            date("2026-07-06"),
        ];

        let result =
            reconcile_payroll_hours(&pay_rates, &worked_items, &[], 1, 210, &weeks, None).unwrap();

        assert_eq!(result.week_totals_minutes, [60, 0, 0, 0]);
        assert_eq!(result.weeks[0][0].total_hourly_rate, Some(12.71));
    }

    #[test]
    fn formatted_portions_reconcile_exactly_to_the_displayed_rounded_total() {
        let portions = vec![
            PayRatePortion {
                worked_minutes: 20,
                total_hourly_rate: Some(12.21),
                effective_date: Some(date("2027-01-01")),
                rate_id: Some(1),
                is_previous_cycle: false,
                is_opaque_legacy: false,
            },
            PayRatePortion {
                worked_minutes: 20,
                total_hourly_rate: Some(12.71),
                effective_date: Some(date("2027-03-03")),
                rate_id: Some(2),
                is_previous_cycle: false,
                is_opaque_legacy: false,
            },
        ];

        assert_eq!(format_total_minutes(40), "0.67");
        assert_eq!(
            format_week_rate_portions(&portions, date("2027-03-01")),
            "0.33 (£12.21)\n0.34 (£12.71 from 03/03/2027)"
        );
    }

    #[test]
    fn unsubmitted_candidate_is_not_late_shift_membership_but_submitted_snapshot_is() {
        let (_file, pay_rates, worked_items) = shared_repositories(&[("01/01/2027", 12.0, 0.21)]);
        let submitted_item = WorkedItemSnapshot {
            week_number: 3,
            source_type: "imported_shift".to_string(),
            timesheet_id: Some(1),
            direct_shift_id: None,
            source_evidence: None,
            work_date: Some("2027-03-10".to_string()),
            worked_minutes: 60,
            pay_rate_id: Some(1),
            pay_rate_effective_date: Some("01/01/2027".to_string()),
            total_hourly_rate: Some(12.21),
            reason: None,
        };
        worked_items
            .replace_candidate(
                90,
                std::slice::from_ref(&submitted_item),
                "/tmp/test.pdf",
                "digest",
                "2027-03-27",
                0,
                &[0; 4],
                &[0; 4],
            )
            .unwrap();
        let weeks = [
            date("2027-03-29"),
            date("2027-04-05"),
            date("2027-04-12"),
            date("2027-04-19"),
        ];
        let shifts = vec![
            shift_with_id(1, "10 March 2027", 60),
            shift_with_id(2, "16 March 2027", 120),
        ];
        let previous = PreviousCycleContext {
            payroll_timesheet_id: 90,
            week_three_start: date("2027-03-08"),
            legacy_adjustment_minutes: 300,
        };

        let while_candidate = reconcile_payroll_hours(
            &pay_rates,
            &worked_items,
            &shifts,
            1,
            91,
            &weeks,
            Some(&previous),
        )
        .unwrap();
        assert_eq!(while_candidate.previous_cycle_minutes, 0);

        assert!(worked_items.protect_for_send(90, "2027-03-28").unwrap());
        worked_items
            .mark_submitted_and_email_sent(90, 1, "2026/27", 6, "2027-03-28")
            .unwrap();
        let after_submission = reconcile_payroll_hours(
            &pay_rates,
            &worked_items,
            &shifts,
            1,
            91,
            &weeks,
            Some(&previous),
        )
        .unwrap();
        assert_eq!(after_submission.previous_cycle_minutes, 120);
        assert_eq!(after_submission.snapshot_items[0].timesheet_id, Some(2));
    }

    #[test]
    fn legacy_aggregate_remains_opaque_and_renders_without_a_fabricated_rate() {
        let (_file, pay_rates, worked_items) =
            shared_repositories(&[("01/01/2027", 12.0, 0.21), ("15/03/2027", 12.5, 0.21)]);
        let weeks = [
            date("2027-03-29"),
            date("2027-04-05"),
            date("2027-04-12"),
            date("2027-04-19"),
        ];
        let previous = PreviousCycleContext {
            payroll_timesheet_id: 99,
            week_three_start: date("2027-03-08"),
            legacy_adjustment_minutes: 300,
        };

        let result = reconcile_payroll_hours(
            &pay_rates,
            &worked_items,
            &[],
            1,
            100,
            &weeks,
            Some(&previous),
        )
        .unwrap();

        assert_eq!(result.previous_cycle_minutes, 300);
        assert_eq!(result.week_totals_minutes[0], 300);
        let legacy = &result.snapshot_items[0];
        assert_eq!(legacy.timesheet_id, None);
        assert_eq!(legacy.work_date, None);
        assert_eq!(legacy.pay_rate_id, None);
        assert_eq!(legacy.pay_rate_effective_date, None);
        assert_eq!(legacy.total_hourly_rate, None);
        worked_items
            .replace_candidate(
                100,
                &result.snapshot_items,
                "/tmp/legacy.pdf",
                "digest",
                "2027-03-29",
                300,
                &[0; 4],
                &result.week_totals_minutes,
            )
            .unwrap();
        let persisted = worked_items.get_snapshot_items(100).unwrap();
        assert_eq!(persisted[0].pay_rate_id, None);
        assert_eq!(persisted[0].pay_rate_effective_date, None);
        assert_eq!(persisted[0].total_hourly_rate, None);
        let rendered = format_week_rate_portions(&result.weeks[0], weeks[0]);
        assert_eq!(
            rendered,
            "5 (previous-cycle adjustment; historical rate allocation unavailable)"
        );
        assert!(!rendered.contains('£'));
    }
}

#[cfg(test)]
mod payroll_rounding_tests {
    use super::*;

    #[test]
    fn payroll_rounding_preserves_exact_multiples_zero_and_rejects_invalid_settings() {
        let mut settings = crate::config::PayrollConfig::default();
        for direction in ["Up", "Down"] {
            settings.rounding_direction = direction.into();
            assert_eq!(payable_minutes(0, &settings).unwrap(), 0);
            assert_eq!(payable_minutes(30, &settings).unwrap(), 30);
        }
        assert_eq!(payable_minutes(14, &settings).unwrap(), 0);
        assert!(payable_minutes(-1, &settings).is_err());
        settings.rounding_minutes = 0;
        assert!(payable_minutes(34, &settings).is_err());
        settings.rounding_minutes = 15;
        settings.rounding_direction = "invalid".into();
        assert!(payable_minutes(34, &settings).is_err());
        settings.rounding_direction = "Up".into();
        assert!(payable_minutes(i64::MAX, &settings).is_err());
    }
}
