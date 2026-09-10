//! Read-only guidance from retained preparation and payroll evidence. Never
//! reconcile raw shifts here or persist derived totals. Payroll is definitive.
use std::collections::{BTreeSet, HashSet};

use chrono::{Datelike, Duration, NaiveDate};

use crate::annual_leave_settings_repository::{normalise_boundary, AnnualLeaveSettings};
use crate::app::Application;
use crate::contracted_hours_repository::{ContractedHoursEntry, HoursBasis};
use crate::models::{parse_employment_date, PersonalAssistant};
use crate::payroll_timesheet_repository::{
    PayrollTimesheet, PayrollTimesheetAnnualLeave, PayrollTimesheetWeek,
};
use crate::payroll_worked_item_repository::{SnapshotState, WorkedItemSnapshot};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaveYear(pub i32);
impl LeaveYear {
    pub fn containing(date: NaiveDate) -> Self {
        Self(date.year() - i32::from(date.month() < 4))
    }
    pub fn start(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.0, 4, 1).unwrap()
    }
    pub fn end(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.0 + 1, 4, 1).unwrap() - Duration::days(1)
    }
    pub fn label(self) -> String {
        format!("{}/{:02}", self.0, (self.0 + 1) % 100)
    }
    fn contains(self, date: NaiveDate) -> bool {
        date >= self.start() && date <= self.end()
    }
}

#[derive(Clone)]
struct CycleEvidence {
    record: PayrollTimesheet,
    weeks: Vec<PayrollTimesheetWeek>,
    leave: Vec<PayrollTimesheetAnnualLeave>,
    items: Vec<WorkedItemSnapshot>,
    sent: bool,
    snapshot_state: Option<SnapshotState>,
    period_end: Option<NaiveDate>,
}

struct Evidence {
    assistant: PersonalAssistant,
    settings: AnnualLeaveSettings,
    history: Vec<ContractedHoursEntry>,
    cycles: Vec<CycleEvidence>,
    schedule_dates: Vec<NaiveDate>,
}
impl Evidence {
    fn load(application: &Application, pa_id: i64) -> Result<Self> {
        let assistant = application
            .personal_assistant_repository
            .get_all()?
            .into_iter()
            .find(|pa| pa.id == pa_id)
            .ok_or("Personal Assistant no longer exists.")?;
        let settings = application
            .annual_leave_settings_repository
            .get()?
            .validated()?;
        let history = application
            .contracted_hours_repository
            .get_all_for_personal_assistant(pa_id)?;
        let schedules = application.payroll_schedule_repository.get_all()?;
        let mut cycles = Vec::new();
        for record in application
            .payroll_timesheet_repository
            .get_all_for_personal_assistant(pa_id)?
        {
            let weeks = application
                .payroll_timesheet_repository
                .get_weeks(record.id)?;
            let period_end = schedules
                .iter()
                .find(|schedule| {
                    schedule.payroll_year == record.payroll_year
                        && schedule.cycle_number == record.cycle_number
                })
                .and_then(|schedule| parse_employment_date(&schedule.first_week_commencing))
                .map(|date| date + Duration::days(27));
            let sent = application
                .payroll_timesheet_email_repository
                .get_for_pa_and_cycle(pa_id, &record.payroll_year, record.cycle_number, "payslip")?
                .is_some_and(|status| status.is_definitively_sent());
            cycles.push(CycleEvidence {
                leave: application
                    .payroll_timesheet_repository
                    .get_annual_leave(record.id)?,
                items: application
                    .payroll_worked_item_repository
                    .get_snapshot_items(record.id)?,
                snapshot_state: application
                    .payroll_worked_item_repository
                    .snapshot_metadata(record.id)?
                    .map(|metadata| metadata.state),
                record,
                weeks,
                sent,
                period_end,
            });
        }
        Ok(Self {
            assistant,
            settings,
            history,
            cycles,
            schedule_dates: schedules
                .iter()
                .filter_map(|schedule| parse_employment_date(&schedule.first_week_commencing))
                .collect(),
        })
    }

    fn years(&self, today: NaiveDate) -> Vec<LeaveYear> {
        let current = LeaveYear::containing(today);
        let mut years = BTreeSet::from([current.0]);
        let mut add_date = |date: NaiveDate| {
            let year = LeaveYear::containing(date).0;
            if year <= current.0 {
                years.insert(year);
            }
        };
        for entry in &self.history {
            if let Some(date) = parse_employment_date(&entry.effective_date) {
                add_date(date);
            }
        }
        for cycle in &self.cycles {
            for week in &cycle.weeks {
                if let Some(date) = parse_employment_date(&week.week_commencing) {
                    add_date(date);
                    add_date(date + Duration::days(6));
                }
            }
            for row in &cycle.leave {
                if let Some(date) = parse_employment_date(&row.leave_date) {
                    add_date(date);
                }
            }
            for item in &cycle.items {
                if let Some(date) = item.work_date.as_deref().and_then(parse_employment_date) {
                    add_date(date);
                }
            }
        }
        // Employment is positive evidence for intervening years; no arbitrary
        // fixed lookback. Without a Start date, only actual evidence adds years.
        let start = self
            .assistant
            .start_date
            .as_deref()
            .and_then(parse_employment_date);
        let end = self
            .assistant
            .leaving_date
            .as_deref()
            .and_then(parse_employment_date)
            .unwrap_or(today)
            .min(today);
        if let Some(start) = start {
            if start <= end {
                for year in LeaveYear::containing(start).0..=LeaveYear::containing(end).0 {
                    years.insert(year);
                }
            }
        } else {
            for date in &self.schedule_dates {
                if *date <= end {
                    years.insert(LeaveYear::containing(*date).0);
                }
            }
        }
        years.into_iter().rev().map(LeaveYear).collect()
    }
}

#[derive(Clone, Debug)]
struct ContractedPart {
    start: NaiveDate,
    end: NaiveDate,
    history_id: i64,
    weekly_hours: f64,
    entitlement: f64,
}
#[derive(Default, Debug)]
struct Summary {
    entitlement: f64,
    taken: f64,
    contracted: bool,
    variable: bool,
    calculated_to: Option<NaiveDate>,
    contracted_parts: Vec<ContractedPart>,
    details: Vec<String>,
    incomplete: BTreeSet<String>,
}
impl Summary {
    fn label(&self) -> &'static str {
        match (self.contracted, self.variable) {
            (true, true) => "Annual entitlement/accrued",
            (false, true) => "Annual leave accrued",
            _ => "Annual entitlement",
        }
    }
    fn remaining(&self) -> f64 {
        self.entitlement - self.taken
    }
}

fn checked_date(value: &str, label: &str) -> Result<NaiveDate> {
    parse_employment_date(value).ok_or_else(|| format!("Invalid {label}: {value}").into())
}
fn optional_date(value: &Option<String>, label: &str) -> Result<Option<NaiveDate>> {
    value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| checked_date(value, label))
        .transpose()
}
fn display(date: NaiveDate) -> String {
    crate::date_utils::uk(date)
}

// These settings are recurring rule effective dates, never the leave-year
// boundary or a PA's basis-change date. The singleton stores no older values.
fn rule_effective_on(boundary: &str, date: NaiveDate) -> Result<NaiveDate> {
    let boundary = normalise_boundary(boundary)?;
    let occurrence = checked_date(
        &format!("{boundary}/{}", date.year()),
        "rule effective date",
    )?;
    Ok(if occurrence > date {
        occurrence.with_year(date.year() - 1).unwrap()
    } else {
        occurrence
    })
}

fn round_variable(hours: f64, percentage: f64) -> f64 {
    (hours * percentage / 100.0).round()
}

fn calculate(evidence: &Evidence, year: LeaveYear, today: NaiveDate) -> Result<Summary> {
    let mut result = Summary::default();
    let start = optional_date(&evidence.assistant.start_date, "Start date")?;
    let end = optional_date(&evidence.assistant.leaving_date, "Leaving date")?;
    if start.zip(end).is_some_and(|(start, end)| end < start) {
        return Err("Leaving date precedes Start date.".into());
    }
    let employment_start = start.unwrap_or(year.start());
    let employment_end = end.unwrap_or(year.end());
    let mut history = evidence
        .history
        .iter()
        .map(|entry| {
            Ok((
                checked_date(&entry.effective_date, "Hours Basis effective date")?,
                entry,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    history.sort_by_key(|(date, entry)| (*date, entry.id));
    let basis_at = |date: NaiveDate| {
        history
            .iter()
            .rev()
            .find(|(effective, _)| *effective <= date)
            .map(|(_, entry)| *entry)
    };
    let employed = |date: NaiveDate| date >= employment_start && date <= employment_end;
    let qualifying = |date: NaiveDate| {
        year.contains(date)
            && employed(date)
            && basis_at(date).is_some_and(|entry| entry.hours_basis == HoursBasis::Variable)
    };
    let days = (year.end() - year.start()).num_days() + 1;
    for offset in 0..days {
        let date = year.start() + Duration::days(offset);
        if !employed(date) {
            continue;
        }
        let Some(entry) = basis_at(date) else {
            result
                .incomplete
                .insert("Hours Basis history is missing for part of the employment window.".into());
            continue;
        };
        match entry.hours_basis {
            HoursBasis::Variable => result.variable = true,
            HoursBasis::Contracted => {
                result.contracted = true;
                let hours: f64 = match entry.contracted_hours.trim().parse::<f64>() {
                    Ok(hours) if hours.is_finite() && hours >= 0.0 => hours,
                    _ => {
                        result.incomplete.insert(format!(
                            "Invalid contracted weekly hours at {}: {}",
                            entry.effective_date, entry.contracted_hours
                        ));
                        continue;
                    }
                };
                let amount = hours * evidence.settings.statutory_weeks / days as f64;
                result.entitlement += amount;
                if let Some(part) = result.contracted_parts.last_mut().filter(|part| {
                    part.history_id == entry.id && part.end + Duration::days(1) == date
                }) {
                    part.end = date;
                    part.entitlement += amount;
                } else {
                    result.contracted_parts.push(ContractedPart {
                        start: date,
                        end: date,
                        history_id: entry.id,
                        weekly_hours: hours,
                        entitlement: amount,
                    });
                }
            }
        }
    }
    let mut seen_shifts = HashSet::new();
    for cycle in &evidence.cycles {
        let cycle_label = format!(
            "{} cycle {}",
            cycle.record.payroll_year, cycle.record.cycle_number
        );
        // Dated leave overrides the compatibility weekly total, even if zero.
        for week in &cycle.weeks {
            let start = checked_date(&week.week_commencing, "week commencing")?;
            let rows: Vec<_> = cycle
                .leave
                .iter()
                .filter(|row| row.week_number == week.week_number)
                .collect();
            if rows.is_empty() {
                if year.contains(start) && week.annual_leave_hours != 0.0 {
                    if !week.annual_leave_hours.is_finite() || week.annual_leave_hours < 0.0 {
                        return Err("Invalid legacy annual-leave hours.".into());
                    }
                    result.taken += week.annual_leave_hours;
                    result.details.push(format!(
                        "Taken: {cycle_label}, legacy undated w/c {}: {:.4} h",
                        display(start),
                        week.annual_leave_hours
                    ));
                }
            } else {
                for row in rows {
                    let date = checked_date(&row.leave_date, "annual-leave date")?;
                    if year.contains(date) {
                        if !row.hours.is_finite() || row.hours < 0.0 {
                            return Err("Invalid dated annual-leave hours.".into());
                        }
                        result.taken += row.hours;
                        result.details.push(format!(
                            "Taken: {cycle_label}, {}: {:.4} h",
                            display(date),
                            row.hours
                        ));
                    }
                }
            }
            if week.sick_leave_hours > 0.0
                && (0..7).any(|day| qualifying(start + Duration::days(day)))
            {
                result.incomplete.insert(format!("Variable-basis Sick/SSP in {cycle_label}, w/c {}: additional sickness accrual calculation required.", display(start)));
            }
        }
        if !cycle.sent {
            continue;
        }
        let relevant_week = cycle.weeks.iter().any(|week| {
            parse_employment_date(&week.week_commencing)
                .is_some_and(|start| (0..7).any(|day| qualifying(start + Duration::days(day))))
        });
        let mut minutes: i64 = 0;
        let mut contributes = false;
        let mut historical_hours = 0.0;
        if cycle.items.is_empty() {
            // Undated preparation totals can only be attributed as whole weeks.
            // An excluded week must not discard safe hours elsewhere in the cycle.
            let mut boundary_exclusions = Vec::new();
            let window_start = year.start().max(employment_start);
            let window_end = year.end().min(employment_end).min(today);
            for week in &cycle.weeks {
                let week_start = checked_date(&week.week_commencing, "week commencing")?;
                let week_end = week_start + Duration::days(6);
                if window_start > window_end
                    || week_end < window_start
                    || week_start > window_end
                    || week.worked_hours == 0.0
                {
                    continue;
                }
                let unambiguous_basis = |date, basis| {
                    basis_at(date).is_some_and(|active| {
                        active.hours_basis == basis
                            && history
                                .iter()
                                .filter(|(effective, _)| {
                                    Some(*effective)
                                        == parse_employment_date(&active.effective_date)
                                })
                                .all(|(_, entry)| entry.hours_basis == basis)
                    })
                };
                // A wholly Contracted portion is outside Variable accrual.
                if (0..7)
                    .map(|day| week_start + Duration::days(day))
                    .filter(|date| *date >= window_start && *date <= window_end)
                    .all(|date| unambiguous_basis(date, HoursBasis::Contracted))
                {
                    continue;
                }
                let safe = week_start >= window_start
                    && week_end <= window_end
                    && (0..7).all(|day| {
                        unambiguous_basis(week_start + Duration::days(day), HoursBasis::Variable)
                    })
                    && week.worked_hours.is_finite()
                    && week.worked_hours >= 0.0
                    && (historical_hours + week.worked_hours).is_finite();
                if safe {
                    historical_hours += week.worked_hours;
                    contributes = true;
                    result.details.push(format!("Variable historical fallback: {cycle_label}, w/c {}: {:.4} h from retained Payroll Timesheet Preparation worked_hours; whole week has unambiguous Variable Hours Basis coverage. Individual work dates are unavailable.", display(week_start), week.worked_hours));
                } else {
                    let warning = format!("{cycle_label}, w/c {}: payslip Sent, but retained submitted work evidence is unavailable; this week's aggregate preparation worked_hours cannot be safely attributed across Hours Basis/year/employment/date boundaries or invalid totals; excluded, variable accrual needs review.", display(week_start));
                    // Presentation only: known historical boundaries can be audited
                    // without marking an otherwise calculated period incomplete.
                    // Missing/conflicting basis evidence, invalid totals and future
                    // weeks still require review, even when other weeks contribute.
                    let historical_boundary_only = week_end <= today
                        && week.worked_hours.is_finite()
                        && week.worked_hours >= 0.0
                        && (historical_hours + week.worked_hours).is_finite()
                        && (0..7)
                            .map(|day| week_start + Duration::days(day))
                            .filter(|date| *date >= window_start && *date <= window_end)
                            .all(|date| {
                                unambiguous_basis(date, HoursBasis::Variable)
                                    || unambiguous_basis(date, HoursBasis::Contracted)
                            });
                    if historical_boundary_only {
                        boundary_exclusions.push((week_start, week.worked_hours, warning));
                    } else {
                        result.incomplete.insert(warning);
                    }
                }
            }
            for (week_start, hours, warning) in boundary_exclusions {
                if contributes && (historical_hours + hours).is_finite() {
                    result.details.push(format!("Historical boundary exclusion: {cycle_label}, w/c {}: {hours:.4} h excluded because individual work dates could not be established across a leave-year/employment/Hours Basis boundary. Accrual uses only safely attributable qualifying weeks; these excluded hours have not been allocated.", display(week_start)));
                } else {
                    result.incomplete.insert(warning);
                }
            }
        } else if cycle.snapshot_state == Some(SnapshotState::Submitted) {
            for item in &cycle.items {
                if item.worked_minutes == 0 {
                    continue;
                }
                match item.source_type.as_str() {
                    "imported_shift" | "previous_cycle_late_shift" | "direct_shift" => {
                        let Some(date) = item.work_date.as_deref().and_then(parse_employment_date)
                        else {
                            if relevant_week {
                                result.incomplete.insert(format!(
                                    "{cycle_label}: a retained shift has no valid work date."
                                ));
                            }
                            continue;
                        };
                        if !qualifying(date) || date > today {
                            continue;
                        }
                        if let Some(key) = item
                            .timesheet_id
                            .map(|id| ("imported", id))
                            .or_else(|| item.direct_shift_id.map(|id| ("direct", id)))
                        {
                            let id = key.1;
                            if !seen_shifts.insert(key) {
                                result.incomplete.insert(format!(
                                    "Duplicate retained shift {id}; counted once."
                                ));
                                continue;
                            }
                        }
                        minutes = minutes
                            .checked_add(item.worked_minutes)
                            .ok_or("Worked minutes overflow.")?;
                        contributes = true;
                        result.details.push(format!(
                            "Variable work: {cycle_label}, {} ({}): {:.4} h",
                            display(date),
                            item.source_type,
                            item.worked_minutes as f64 / 60.0
                        ));
                    }
                    "manual_adjustment" => {
                        let Some(week) = cycle
                            .weeks
                            .iter()
                            .find(|week| week.week_number == item.week_number)
                        else {
                            result
                                .incomplete
                                .insert(format!("{cycle_label}: adjustment has no matching week."));
                            continue;
                        };
                        let start = checked_date(&week.week_commencing, "adjustment week")?;
                        if !(0..7).any(|day| qualifying(start + Duration::days(day))) {
                            continue;
                        }
                        if (0..7).all(|day| {
                            let date = start + Duration::days(day);
                            qualifying(date) && date <= today
                        }) {
                            minutes = minutes
                                .checked_add(item.worked_minutes)
                                .ok_or("Worked minutes overflow.")?;
                            contributes = true;
                            result.details.push(format!("Variable undated manual adjustment: {cycle_label}, w/c {}: {:.4} h (whole week qualifies)", display(start), item.worked_minutes as f64 / 60.0));
                        } else {
                            result.incomplete.insert(format!("{cycle_label}, w/c {}: undated worked-hours adjustment crosses a year/employment/basis boundary; cannot allocate safely.", display(start)));
                        }
                    }
                    "legacy_previous_cycle_adjustment" => {
                        if relevant_week {
                            result.incomplete.insert(format!("{cycle_label}: legacy previous-cycle hours have no actual dates; not accrued automatically."));
                        }
                    }
                    _ => {
                        if relevant_week {
                            result.incomplete.insert(format!(
                                "{cycle_label}: unsupported worked evidence type '{}'; excluded.",
                                item.source_type
                            ));
                        }
                    }
                }
            }
        } else {
            // No trustworthy submitted membership: do not reinterpret raw shifts
            // or treat a current editable preparation total as settled evidence.
            if cycle.weeks.iter().any(|week| {
                week.worked_hours > 0.0
                    && parse_employment_date(&week.week_commencing).is_some_and(|start| {
                        (0..7).any(|day| qualifying(start + Duration::days(day)))
                    })
            }) {
                result.incomplete.insert(format!("{cycle_label}: payslip Sent, but retained submitted work evidence is unavailable; variable accrual needs review."));
            }
        }
        if contributes {
            if minutes < 0 {
                result.incomplete.insert(format!("{cycle_label}: net qualifying worked hours are negative; variable accrual needs review."));
                continue;
            }
            let hours = minutes as f64 / 60.0 + historical_hours;
            let accrued = round_variable(hours, evidence.settings.accrual_percentage);
            result.entitlement += accrued;
            result.details.push(format!("Variable accrual: {cycle_label}: {hours:.4} h × {}% = {:.6} h → {accrued:.0} h (rounded once for this pay period)", evidence.settings.accrual_percentage, hours * evidence.settings.accrual_percentage / 100.0));
            if let Some(end) = cycle.period_end {
                result.calculated_to = Some(result.calculated_to.map_or(end, |old| old.max(end)));
            } else {
                result.incomplete.insert(format!(
                    "{cycle_label}: payroll schedule unavailable for Calculated to."
                ));
            }
        }
    }
    result.details.insert(0, format!("Fixed leave year: {} to {} inclusive ({} days). Current saved/default settings are used; historical rule values are not retained.", display(year.start()), display(year.end()), days));
    result.details.insert(1, format!("Recurring rule dates (not basis or year boundaries): contracted {}, variable {}. Applicable recurrences at year start: {} and {}.", evidence.settings.contracted_effective_from, evidence.settings.variable_effective_from,
        display(rule_effective_on(&evidence.settings.contracted_effective_from, year.start())?), display(rule_effective_on(&evidence.settings.variable_effective_from, year.start())?)));
    Ok(result)
}

// Presentation only: each figure is rounded independently from its precise total.
fn quarter_hour_presentation(hours: f64) -> (String, Option<String>) {
    let rounded = (hours * 4.0).ceil() / 4.0;
    (
        format!("{rounded:.2}"),
        (rounded != hours).then(|| format!("{hours:.2}")),
    )
}

fn summary_bold_family() -> egui::FontFamily {
    egui::FontFamily::Name("annual_leave_summary_bold".into())
}

fn summary_hours_job(ui: &egui::Ui, label: &str, hours: f64) -> egui::text::LayoutJob {
    let (rounded, original) = quarter_hour_presentation(hours);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let normal = egui::TextFormat {
        font_id: font.clone(),
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    job.append(&format!("{label} "), 0.0, normal.clone());
    job.append(
        &rounded,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(font.size, summary_bold_family()),
            ..normal.clone()
        },
    );
    if let Some(original) = original {
        job.append(&format!(" ({original})"), 0.0, normal.clone());
    }
    job.append(" hours", 0.0, normal);
    job
}

fn summary_hours(ui: &mut egui::Ui, label: &str, hours: f64) -> egui::Response {
    ui.label(summary_hours_job(ui, label, hours))
}

#[derive(Default)]
pub struct AnnualLeaveSummaryUi {
    loaded_for: Option<(i64, NaiveDate)>,
    last_frame: Option<u64>,
    selected_pa: Option<i64>,
    evidence: Option<Evidence>,
    selected_year: Option<i32>, // None follows the current year automatically.
    details_open: bool,
    error: Option<String>,
}
impl AnnualLeaveSummaryUi {
    pub fn invalidate(&mut self) {
        self.loaded_for = None;
    }
    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application, pa_id: i64) {
        if pa_id == 0 {
            return;
        }
        // egui's `strong()` changes colour only. Use a dedicated embedded bold
        // face for the summary numbers without changing any existing font family.
        if !ui.fonts(|fonts| fonts.families().contains(&summary_bold_family())) {
            ui.ctx().add_font(egui::epaint::text::FontInsert::new(
                "annual_leave_summary_bold",
                egui::FontData::from_static(include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf")),
                vec![egui::epaint::text::InsertFontFamily {
                    family: summary_bold_family(),
                    priority: egui::epaint::text::FontPriority::Highest,
                }],
            ));
            ui.ctx()
                .request_discard("Load annual leave summary bold font");
            return;
        }
        let frame = ui.ctx().cumulative_frame_nr();
        if self.last_frame.is_some_and(|previous| frame > previous + 1) {
            self.invalidate();
        }
        self.last_frame = Some(frame);
        let today = chrono::Local::now().date_naive();
        if self.selected_pa != Some(pa_id) {
            self.selected_year = None;
            self.details_open = false;
            self.selected_pa = Some(pa_id);
        }
        ui.separator();
        if self.loaded_for != Some((pa_id, today)) {
            self.loaded_for = Some((pa_id, today));
            match Evidence::load(application, pa_id) {
                Ok(evidence) => {
                    self.evidence = Some(evidence);
                    self.error = None;
                }
                Err(error) => {
                    self.evidence = None;
                    self.error = Some(error.to_string());
                }
            }
        }
        let current = LeaveYear::containing(today);
        let years = self
            .evidence
            .as_ref()
            .map(|evidence| evidence.years(today))
            .unwrap_or_default();
        if self
            .selected_year
            .is_some_and(|year| !years.contains(&LeaveYear(year)))
        {
            self.selected_year = None;
        }
        let year = LeaveYear(self.selected_year.unwrap_or(current.0));
        let calculation = self
            .evidence
            .as_ref()
            .map(|evidence| calculate(evidence, year, today));
        let summary = calculation.as_ref().and_then(|result| result.as_ref().ok());
        ui.horizontal_wrapped(|ui| {
            ui.heading("Annual Leave");
            if ui
                .add_enabled(summary.is_some(), egui::Button::new("Details"))
                .clicked()
            {
                self.details_open = true;
            }
            if let Some(summary) = summary {
                summary_hours(ui, "Taken", summary.taken);
                summary_hours(ui, "Remaining", summary.remaining());
                summary_hours(ui, "Entitlement", summary.entitlement)
                    .on_hover_text(summary.label());
            } else {
                ui.label("Summary unavailable");
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Refresh annual leave").clicked() {
                // The summary is above these controls: reload on the next frame.
                self.loaded_for = None;
                ui.ctx().request_repaint();
            }
            if !years.is_empty() {
                let previous_year = self.selected_year;
                crate::gui_controls::combo_box(("annual_leave_year", pa_id))
                    .selected_text(year.label())
                    .show_ui(ui, |ui| {
                        for year in years {
                            crate::gui_controls::combo_value(
                                ui,
                                &mut self.selected_year,
                                if year == current { None } else { Some(year.0) },
                                year.label(),
                            );
                        }
                    });
                if self.selected_year != previous_year {
                    ui.ctx().request_repaint();
                }
            }
            ui.label("Guidance/reference only — Payroll remains definitive. Uses saved PA data.");
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(Err(error)) = &calculation {
            ui.colored_label(ui.visuals().error_fg_color, error.to_string());
        }
        let (Some(summary), Some(evidence)) = (summary, &self.evidence) else {
            return;
        };
        if summary.variable {
            ui.label(format!(
                "Calculated to: {}",
                summary
                    .calculated_to
                    .map(|d| application.context.config.date_display_format.format(d))
                    .unwrap_or_else(|| "no qualifying Sent payroll period".into())
            ));
        }
        if !summary.incomplete.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Incomplete — additional calculation/review required. See Details.",
            );
            for warning in &summary.incomplete {
                if warning.contains("Sick/SSP") {
                    ui.label(warning);
                }
            }
        }
        egui::Window::new(format!("Annual Leave Details — {}", year.label()))
            .id(egui::Id::new(("annual_leave_details",pa_id))).open(&mut self.details_open)
            .default_width(650.0).vscroll(true).show(ui.ctx(), |ui| {
                ui.label("Guidance only. Payroll remains definitive. Values below retain additional precision.");
                for warning in &summary.incomplete { ui.colored_label(ui.visuals().warn_fg_color, crate::date_utils::calendar_text(application.context.config.date_display_format, warning)); }
                for part in &summary.contracted_parts {
                    ui.label(format!("Contracted {}–{} inclusive: {:.4} weekly h × {} weeks × {} / {} days = {:.6} h", application.context.config.date_display_format.format(part.start), application.context.config.date_display_format.format(part.end), part.weekly_hours, evidence.settings.statutory_weeks, (part.end-part.start).num_days()+1, (year.end()-year.start()).num_days()+1, part.entitlement));
                }
                for detail in &summary.details { ui.label(crate::date_utils::calendar_text(application.context.config.date_display_format, detail)); }
            });
    }
}

#[cfg(test)]
mod tests;
