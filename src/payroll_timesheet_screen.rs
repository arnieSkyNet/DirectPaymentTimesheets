use chrono::Datelike;
use eframe::egui;
use std::collections::HashMap;

use crate::app::Application;
use crate::config::ApplicationTheme;
use crate::payroll_schedule_repository::PayrollSchedule;
use crate::payroll_timesheet_repository::{
    derive_annual_leave, PayrollTimesheet, PayrollTimesheetAnnualLeave,
    PayrollTimesheetPublicHoliday, PayrollTimesheetWeek,
};
use crate::payroll_worked_item_repository::{ManualHoursAdjustment, SnapshotState};

#[derive(Clone, Debug, PartialEq, Eq)]
struct BoundPayrollPeriod {
    payroll_year: String,
    cycle_number: i64,
    first_week_commencing: String,
    pay_date: String,
}

impl From<&PayrollSchedule> for BoundPayrollPeriod {
    fn from(schedule: &PayrollSchedule) -> Self {
        Self {
            payroll_year: schedule.payroll_year.clone(),
            cycle_number: schedule.cycle_number,
            first_week_commencing: schedule.first_week_commencing.clone(),
            pay_date: schedule.pay_date.clone(),
        }
    }
}

#[derive(Clone)]
struct PreparationBaseline {
    previous_cycle_hours: Option<f64>,
    weeks: Vec<PayrollTimesheetWeek>,
    public_holidays: Vec<PayrollTimesheetPublicHoliday>,
    manual_adjustments: Vec<ManualHoursAdjustment>,
    annual_leave: Vec<PayrollTimesheetAnnualLeave>,
}

struct SaveResult {
    changed: bool,
    candidate_invalidated: bool,
    public_holiday_totals: [f64; 4],
    annual_leave_totals: [f64; 4],
    normalised_annual_leave: Vec<PayrollTimesheetAnnualLeave>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaSectionStyle {
    fill: egui::Color32,
    stroke: egui::Stroke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum NumericEditorKey {
    Worked(i64),
    AnnualLeave(i64),
    SickLeave(i64),
    PublicHoliday(i64),
    TravelMiles(i64),
}

pub struct PayrollTimesheetScreen {
    loaded: bool,
    cycle_number: i64,
    schedule: Option<PayrollSchedule>,
    bound_period: Option<BoundPayrollPeriod>,
    period_label: String,
    records: Vec<PayrollTimesheet>,
    weeks: Vec<(PayrollTimesheet, Vec<PayrollTimesheetWeek>, String)>,
    public_holidays: Vec<Vec<PayrollTimesheetPublicHoliday>>,
    annual_leave: HashMap<i64, Vec<PayrollTimesheetAnnualLeave>>,
    worked_hours_baselines: HashMap<(i64, i64), i64>,
    snapshot_states: HashMap<i64, SnapshotState>,
    preparation_baselines: HashMap<i64, PreparationBaseline>,
    numeric_editor_texts: HashMap<NumericEditorKey, String>,
    assistant_feature_flags: HashMap<i64, (bool, bool)>,
    status_message: String,
    save_errors: HashMap<i64, String>,
}

impl PayrollTimesheetScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,
            cycle_number: 0,
            schedule: None,
            bound_period: None,
            period_label: String::new(),
            records: Vec::new(),
            weeks: Vec::new(),
            public_holidays: Vec::new(),
            annual_leave: HashMap::new(),
            worked_hours_baselines: HashMap::new(),
            snapshot_states: HashMap::new(),
            preparation_baselines: HashMap::new(),
            numeric_editor_texts: HashMap::new(),
            assistant_feature_flags: HashMap::new(),
            status_message: "Payroll Timesheets not loaded.".to_string(),
            save_errors: HashMap::new(),
        }
    }

    pub fn reload(&mut self) {
        self.loaded = false;
        self.schedule = None;
        self.bound_period = None;
        self.period_label.clear();
        self.records.clear();
        self.weeks.clear();
        self.public_holidays.clear();
        self.annual_leave.clear();
        self.save_errors.clear();
        self.worked_hours_baselines.clear();
        self.snapshot_states.clear();
        self.preparation_baselines.clear();
        self.numeric_editor_texts.clear();
        self.assistant_feature_flags.clear();
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        application: &Application,
        operational_schedule: &PayrollSchedule,
        period_label: &str,
    ) {
        self.rebind_if_operational_period_changed(operational_schedule);

        if !self.loaded {
            match self.load(application, operational_schedule, period_label) {
                Ok(()) => self.loaded = true,
                Err(error) => {
                    self.status_message = format!("Failed loading Payroll Timesheets: {}", error);
                }
            }
        }

        if let Some(schedule) = &self.schedule {
            let _ = schedule;
            ui.label(format!("Payroll period: {}", self.period_label));

            ui.label(format!(
                "Four-week period: {} to {}",
                schedule.first_week_commencing,
                self.weeks
                    .first()
                    .and_then(|(_, weeks, _)| weeks.last())
                    .map(|week| week.week_commencing.as_str())
                    .unwrap_or("")
            ));
        }

        ui.separator();

        if self.weeks.is_empty() {
            ui.label("No Payroll Timesheet records are available.");
        }

        for record_index in 0..self.weeks.len() {
            let (records, public_holidays, numeric_editor_texts, assistant_feature_flags) = (
                &mut self.weeks,
                &mut self.public_holidays,
                &mut self.numeric_editor_texts,
                &self.assistant_feature_flags,
            );

            let (record, weeks, assistant_name) = &mut records[record_index];
            let holidays = &mut public_holidays[record_index];
            let annual_leave = self.annual_leave.entry(record.id).or_default();
            let leave_baseline = self.preparation_baselines.get(&record.id).cloned();
            let (sick_pay_enabled, mileage_enabled) = assistant_feature_flags
                .get(&record.personal_assistant_id)
                .copied()
                .unwrap_or((false, false));
            let snapshot_state = self.snapshot_states.get(&record.id).copied();
            let read_only = matches!(
                snapshot_state,
                Some(SnapshotState::Submitted | SnapshotState::Indeterminate)
            );

            let section_style =
                pa_section_style(record_index, application.context.config.theme, ui.visuals());
            egui::Frame::new()
                .fill(section_style.fill)
                .stroke(section_style.stroke)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

            ui.heading(&*assistant_name);

            match snapshot_state {
                Some(SnapshotState::Submitted) => {
                    ui.label("Submitted payroll timesheet — read-only");
                }
                Some(SnapshotState::Indeterminate) => {
                    ui.label("Delivery status is indeterminate — read-only");
                }
                Some(SnapshotState::Candidate) => {
                    ui.label("Generated candidate — changes require regeneration before sending");
                }
                None => {}
            }

            ui.horizontal(|ui| {
                ui.label("Previous cycle hours");

                let previous = record
                    .previous_cycle_hours
                    .map(|value| format_decimal_hours(value))
                    .unwrap_or_default();
                ui.label(previous);
            });

            ui.add_enabled_ui(!read_only, |ui| {
                egui::Grid::new(format!("payroll_week_grid_{}", record_index))
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("W/c");
                        ui.label("Worked");
                        ui.label("Annual Leave");
                        ui.label("Sick / SSP");
                        ui.label("Public Holiday");
                        ui.label("Travel Miles");
                        ui.end_row();

                        for week in weeks.iter_mut() {
                            ui.label(&week.week_commencing);

                            edit_number(
                                ui,
                                numeric_editor_texts,
                                NumericEditorKey::Worked(week.id),
                                &mut week.worked_hours,
                            );
                            ui.vertical(|ui| {
                                edit_annual_leave(ui, numeric_editor_texts, week, annual_leave, leave_baseline.as_ref());
                            });
                            if sick_pay_enabled {
                                edit_number(
                                    ui,
                                    numeric_editor_texts,
                                    NumericEditorKey::SickLeave(week.id),
                                    &mut week.sick_leave_hours,
                                );
                            } else {
                                ui.label("");
                            }

                            ui.vertical(|ui| {
                                let week_number = week.week_number;
                                for holiday in holidays
                                    .iter_mut()
                                    .filter(|holiday| holiday.week_number == week_number)
                                {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(&holiday.holiday_date).size(8.0),
                                        );

                                        edit_optional_number(
                                            ui,
                                            numeric_editor_texts,
                                            NumericEditorKey::PublicHoliday(holiday.id),
                                            &mut holiday.hours,
                                        );
                                    });
                                }
                            });

                            if mileage_enabled {
                                edit_number(
                                    ui,
                                    numeric_editor_texts,
                                    NumericEditorKey::TravelMiles(week.id),
                                    &mut week.travel_miles,
                                );
                            } else {
                                ui.label("");
                            }

                            ui.end_row();
                        }
                    });
            });

            if !read_only && ui.button(format!("Save {}", assistant_name)).clicked() {
                commit_record_numeric_editors(numeric_editor_texts, weeks, holidays);
                let result = save_preparation_record(
                    application,
                    self.bound_period.as_ref(),
                    operational_schedule,
                    record,
                    weeks,
                    holidays,
                    annual_leave,
                    &self.worked_hours_baselines,
                    self.preparation_baselines.get(&record.id),
                );

                match result {
                    Ok(result) => {
                        self.save_errors.remove(&record.id);
                        *annual_leave = result.normalised_annual_leave;
                        if result.candidate_invalidated {
                            self.snapshot_states.remove(&record.id);
                        }
                        for (index, week) in weeks.iter_mut().enumerate() {
                            week.public_holiday_hours = result.public_holiday_totals[index];
                            week.annual_leave_hours = result.annual_leave_totals[index];
                        }
                        self.preparation_baselines.insert(
                            record.id,
                            preparation_baseline(application, record, weeks, holidays)
                                .unwrap_or_else(|_| PreparationBaseline {
                                    previous_cycle_hours: record.previous_cycle_hours,
                                    weeks: weeks.clone(),
                                    public_holidays: holidays.clone(),
                                    manual_adjustments: Vec::new(),
                                    annual_leave: annual_leave.clone(),
                                }),
                        );
                        self.status_message = if result.candidate_invalidated {
                            format!(
                                "Payroll Timesheet saved for {}. Regenerate the PDF before production sending.",
                                assistant_name
                            )
                        } else if result.changed {
                            format!("Payroll Timesheet saved for {}.", assistant_name)
                        } else {
                            format!("No changes to save for {}.", assistant_name)
                        };
                    }

                    Err(error) => {
                        self.status_message = preparation_save_error(error.as_ref());
                        self.save_errors.insert(record.id, self.status_message.clone());
                    }
                }
            }

            if let Some(error) = self.save_errors.get(&record.id) {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.separator();
                });
        }

        ui.label(&self.status_message);
    }

    fn rebind_if_operational_period_changed(
        &mut self,
        operational_schedule: &PayrollSchedule,
    ) -> bool {
        let changed = self
            .bound_period
            .as_ref()
            .is_some_and(|bound| *bound != BoundPayrollPeriod::from(operational_schedule));
        if changed {
            self.reload();
        }
        changed
    }

    fn load(
        &mut self,
        application: &Application,
        schedule: &PayrollSchedule,
        period_label: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let payroll_year = schedule.payroll_year.clone();

        self.cycle_number = schedule.cycle_number;
        self.schedule = Some(schedule.clone());
        self.bound_period = Some(BoundPayrollPeriod::from(schedule));
        self.period_label = period_label.to_string();
        let previous_schedule = application
            .payroll_schedule_repository
            .resolve_previous(&schedule)?;

        let assistants = application.personal_assistant_repository.get_all()?;
        let existing_records = application
            .payroll_timesheet_repository
            .get_all_for_cycle(&payroll_year, schedule.cycle_number)?;
        let existing_personal_assistant_ids = existing_records
            .iter()
            .map(|record| record.personal_assistant_id)
            .collect::<std::collections::HashSet<_>>();

        let all_timesheets = application.get_timesheets()?;

        let first_week =
            parse_date(&schedule.first_week_commencing).ok_or("Invalid payroll schedule date.")?;

        let week_dates = [
            first_week,
            first_week + chrono::Duration::days(7),
            first_week + chrono::Duration::days(14),
            first_week + chrono::Duration::days(21),
        ];

        let week_date_strings = [
            format_date(week_dates[0]),
            format_date(week_dates[1]),
            format_date(week_dates[2]),
            format_date(week_dates[3]),
        ];

        // ------------------------------------------------------------
        // Determine the government-published England & Wales bank
        // holidays that fall within this four-week payroll cycle.
        // ------------------------------------------------------------

        let bank_holidays =
            bank_holidays_for_period(first_week, first_week + chrono::Duration::days(27));

        let holiday_definitions: Vec<(i64, String)> = bank_holidays
            .into_iter()
            .filter_map(|holiday_date| {
                for index in 0..4 {
                    let start = week_dates[index];
                    let end = start + chrono::Duration::days(6);

                    if holiday_date >= start && holiday_date <= end {
                        return Some(((index + 1) as i64, format_date(holiday_date)));
                    }
                }

                None
            })
            .collect();

        self.records.clear();
        self.weeks.clear();
        self.public_holidays.clear();
        self.annual_leave.clear();
        self.save_errors.clear();
        self.worked_hours_baselines.clear();
        self.snapshot_states.clear();
        self.preparation_baselines.clear();
        self.numeric_editor_texts.clear();
        self.assistant_feature_flags.clear();

        for assistant in assistants {
            if !assistant.eligible_for_period(
                first_week,
                first_week + chrono::Duration::days(27),
                existing_personal_assistant_ids.contains(&assistant.id),
            )? {
                continue;
            }

            self.assistant_feature_flags.insert(
                assistant.id,
                (assistant.sick_pay_enabled, assistant.mileage_enabled),
            );

            let assistant_name = format!("{} {}", assistant.first_name, assistant.surname);

            let existing = application
                .payroll_timesheet_repository
                .get_for_cycle_and_pa(&payroll_year, schedule.cycle_number, assistant.id)?;

            let record = match existing {
                Some(record) => record,

                None => {
                    let actual_hours =
                        calculate_actual_hours(&all_timesheets, assistant.id, &week_dates);

                    let previous_cycle_hours = if let Some(previous_schedule) = &previous_schedule {
                        calculate_previous_cycle_adjustment(
                            application,
                            previous_schedule,
                            assistant.id,
                            &all_timesheets,
                            first_week,
                        )?
                    } else {
                        None
                    };

                    let mut current_cycle_hours = actual_hours;

                    if let Some(extra_hours) = previous_cycle_hours {
                        current_cycle_hours[0] += extra_hours;
                    }

                    let id = application.payroll_timesheet_repository.insert(
                        &payroll_year,
                        schedule.cycle_number,
                        assistant.id,
                        previous_cycle_hours,
                        &chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    )?;

                    application
                        .payroll_timesheet_repository
                        .create_missing_weeks(id, &week_date_strings, &current_cycle_hours)?;

                    application
                        .payroll_timesheet_repository
                        .get_for_cycle_and_pa(&payroll_year, schedule.cycle_number, assistant.id)?
                        .ok_or("Failed creating payroll timesheet.")?
                }
            };

            let snapshot_state = application
                .payroll_worked_item_repository
                .snapshot_metadata(record.id)?
                .map(|metadata| metadata.state);
            if let Some(state) = snapshot_state {
                self.snapshot_states.insert(record.id, state);
            }

            if matches!(
                snapshot_state,
                Some(SnapshotState::Submitted | SnapshotState::Indeterminate)
            ) {
                let weeks = application
                    .payroll_timesheet_repository
                    .get_weeks(record.id)?;
                let holidays = application
                    .payroll_timesheet_repository
                    .get_public_holidays(record.id)?;
                self.preparation_baselines.insert(
                    record.id,
                    preparation_baseline(application, &record, &weeks, &holidays)?,
                );
                self.annual_leave.insert(
                    record.id,
                    application
                        .payroll_timesheet_repository
                        .get_annual_leave(record.id)?,
                );
                self.records.push(record.clone());
                self.weeks.push((record, weeks, assistant_name));
                self.public_holidays.push(holidays);
                continue;
            }

            let stored_weeks = application
                .payroll_timesheet_repository
                .get_weeks(record.id)?;
            let stored_holidays = application
                .payroll_timesheet_repository
                .get_public_holidays(record.id)?;
            validate_public_holiday_consistency(&stored_weeks, &stored_holidays).map_err(
                |error| {
                    format!(
                        "Public-holiday data for {assistant_name} requires review before editing: {error}"
                    )
                },
            )?;

            // --------------------------------------------------------
            // Recalculate the worked hours from the current
            // Hours Keeper data whenever the payroll screen loads.
            //
            // This is important because the underlying timesheet
            // data may have been corrected since this payroll record
            // was originally created.
            //
            // Only worked hours and the previous-cycle adjustment
            // are recalculated here. Annual leave, sick leave,
            // public holidays and mileage are left untouched.
            // --------------------------------------------------------

            let previous_record = if let Some(previous_schedule) = &previous_schedule {
                application
                    .payroll_timesheet_repository
                    .get_for_cycle_and_pa(
                        &previous_schedule.payroll_year,
                        previous_schedule.cycle_number,
                        assistant.id,
                    )?
            } else {
                None
            };
            let historical_backfill = application
                .payroll_worked_item_repository
                .get_manual_adjustments(record.id)?
                .iter()
                .any(|adjustment| {
                    crate::historical_payroll_backfill::is_backfill_reason(
                        adjustment.reason.as_deref(),
                    )
                });
            let previous_context = if let Some(previous_record) = &previous_record {
                let previous_weeks = application
                    .payroll_timesheet_repository
                    .get_weeks(previous_record.id)?;
                previous_weeks
                    .get(2)
                    .and_then(|week| parse_date(&week.week_commencing))
                    .map(
                        |week_three_start| crate::pay_rate_allocation::PreviousCycleContext {
                            payroll_timesheet_id: previous_record.id,
                            week_three_start,
                            legacy_adjustment_minutes: record
                                .previous_cycle_hours
                                .map(|hours| (hours * 60.0).round() as i64)
                                .unwrap_or(0),
                        },
                    )
            } else if historical_backfill && record.previous_cycle_hours.is_some() {
                Some(crate::pay_rate_allocation::PreviousCycleContext {
                    payroll_timesheet_id: record.id,
                    week_three_start: week_dates[0],
                    legacy_adjustment_minutes: record
                        .previous_cycle_hours
                        .map(|hours| (hours * 60.0).round() as i64)
                        .unwrap_or(0),
                })
            } else {
                None
            };
            let reconciled = crate::pay_rate_allocation::reconcile_payroll_hours(
                &application.pay_rate_repository,
                &application.payroll_worked_item_repository,
                &all_timesheets,
                assistant.id,
                record.id,
                &week_dates,
                previous_context.as_ref(),
            )?;
            let previous_cycle_hours = (reconciled.previous_cycle_minutes > 0)
                .then(|| reconciled.previous_cycle_minutes as f64 / 60.0);
            let current_cycle_hours =
                std::array::from_fn(|index| reconciled.week_totals_minutes[index] as f64 / 60.0);

            let reconciled_changes_persisted = record
                .previous_cycle_hours
                .map(|hours| (hours * 60.0).round() as i64)
                .unwrap_or(0)
                != reconciled.previous_cycle_minutes
                || stored_weeks.len() != 4
                || stored_weeks.iter().any(|week| {
                    !(1..=4).contains(&week.week_number)
                        || (week.worked_hours * 60.0).round() as i64
                            != reconciled.week_totals_minutes[(week.week_number - 1) as usize]
                });
            let candidate_items_changed = if snapshot_state == Some(SnapshotState::Candidate) {
                application
                    .payroll_worked_item_repository
                    .get_snapshot_items(record.id)?
                    != reconciled.snapshot_items
            } else {
                false
            };
            let invalidate_candidate = snapshot_state == Some(SnapshotState::Candidate)
                && (reconciled_changes_persisted || candidate_items_changed);

            let manual_adjustments = application
                .payroll_worked_item_repository
                .get_manual_adjustments(record.id)?;
            for index in 0..4 {
                let adjustment = manual_adjustments
                    .iter()
                    .find(|adjustment| adjustment.week_number == (index + 1) as i64)
                    .map(|adjustment| adjustment.adjustment_minutes)
                    .unwrap_or(0);
                self.worked_hours_baselines.insert(
                    (record.id, (index + 1) as i64),
                    reconciled.week_totals_minutes[index] - adjustment,
                );
            }

            if reconciled_changes_persisted || invalidate_candidate {
                let candidate_invalidated = application
                    .payroll_timesheet_repository
                    .reconcile_editable_hours_atomically(
                        record.id,
                        previous_cycle_hours,
                        &week_date_strings,
                        &current_cycle_hours,
                        &chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        invalidate_candidate,
                    )?;
                if candidate_invalidated {
                    self.snapshot_states.remove(&record.id);
                }
            }

            let record = application
                .payroll_timesheet_repository
                .get_for_cycle_and_pa(&payroll_year, schedule.cycle_number, assistant.id)?
                .ok_or("Failed reloading payroll timesheet.")?;

            let weeks = application
                .payroll_timesheet_repository
                .get_weeks(record.id)?;

            // --------------------------------------------------------
            // Ensure all government bank-holiday dates for this
            // payroll cycle exist for this PA.
            //
            // Existing records are preserved, including their hours.
            // New records are created with zero hours.
            // --------------------------------------------------------

            application
                .payroll_timesheet_repository
                .create_missing_public_holidays(record.id, &holiday_definitions)?;

            let holidays = application
                .payroll_timesheet_repository
                .get_public_holidays(record.id)?;

            self.preparation_baselines.insert(
                record.id,
                preparation_baseline(application, &record, &weeks, &holidays)?,
            );

            self.annual_leave.insert(
                record.id,
                application
                    .payroll_timesheet_repository
                    .get_annual_leave(record.id)?,
            );
            self.records.push(record.clone());

            self.weeks.push((record, weeks, assistant_name));

            self.public_holidays.push(holidays);
        }

        self.status_message = format!("Loaded {} Payroll Timesheets.", self.weeks.len());

        Ok(())
    }
}

fn preparation_baseline(
    application: &Application,
    record: &PayrollTimesheet,
    weeks: &[PayrollTimesheetWeek],
    public_holidays: &[PayrollTimesheetPublicHoliday],
) -> Result<PreparationBaseline, rusqlite::Error> {
    Ok(PreparationBaseline {
        previous_cycle_hours: record.previous_cycle_hours,
        weeks: weeks.to_vec(),
        public_holidays: public_holidays.to_vec(),
        annual_leave: application
            .payroll_timesheet_repository
            .get_annual_leave(record.id)?,
        manual_adjustments: application
            .payroll_worked_item_repository
            .get_manual_adjustments(record.id)?,
    })
}

fn save_preparation_record(
    application: &Application,
    bound_period: Option<&BoundPayrollPeriod>,
    operational_schedule: &PayrollSchedule,
    record: &PayrollTimesheet,
    weeks: &[PayrollTimesheetWeek],
    public_holidays: &[PayrollTimesheetPublicHoliday],
    annual_leave: &[PayrollTimesheetAnnualLeave],
    worked_hours_baselines: &HashMap<(i64, i64), i64>,
    baseline: Option<&PreparationBaseline>,
) -> Result<SaveResult, Box<dyn std::error::Error>> {
    let bound = bound_period.ok_or(
        "Payroll Timesheet Preparation is not bound to a payroll period. Reload the screen.",
    )?;
    let operational_identity = BoundPayrollPeriod::from(operational_schedule);
    if operational_identity.payroll_year != bound.payroll_year
        || operational_identity.cycle_number != bound.cycle_number
    {
        return Err("The operational payroll period changed while this screen was open. Reload Payroll Timesheet Preparation before saving.".into());
    }
    let stored_schedule = application
        .payroll_schedule_repository
        .get_for_year_and_cycle(&bound.payroll_year, bound.cycle_number)?
        .ok_or("The payroll schedule bound to this screen no longer exists. Reload Payroll Timesheet Preparation before saving.")?;
    if stored_schedule.first_week_commencing != bound.first_week_commencing
        || stored_schedule.pay_date != bound.pay_date
        || operational_schedule.first_week_commencing != bound.first_week_commencing
        || operational_schedule.pay_date != bound.pay_date
    {
        return Err("The payroll schedule changed while this screen was open. Reload Payroll Timesheet Preparation before saving.".into());
    }

    let snapshot_state = application
        .payroll_worked_item_repository
        .snapshot_metadata(record.id)?
        .map(|metadata| metadata.state);
    if matches!(
        snapshot_state,
        Some(SnapshotState::Submitted | SnapshotState::Indeterminate)
    ) {
        return Err("This payroll timesheet is submitted or has an indeterminate delivery state and is read-only.".into());
    }

    let mut weeks = weeks.to_vec();
    let previous_leave = application
        .payroll_timesheet_repository
        .get_annual_leave(record.id)?;
    let annual_leave = derive_annual_leave(record.id, &mut weeks, annual_leave, &previous_leave)?;
    let annual_leave_totals = std::array::from_fn(|index| weeks[index].annual_leave_hours);
    derive_public_holiday_aggregates(&mut weeks, public_holidays)?;
    validate_public_holiday_values(&weeks, public_holidays)?;
    let public_holiday_totals = std::array::from_fn(|index| weeks[index].public_holiday_hours);

    let existing_adjustments = application
        .payroll_worked_item_repository
        .get_manual_adjustments(record.id)?;
    let desired_adjustments = weeks
        .iter()
        .map(|week| {
            let final_minutes = (week.worked_hours * 60.0).round() as i64;
            let baseline_minutes = worked_hours_baselines
                .get(&(record.id, week.week_number))
                .copied()
                .unwrap_or(final_minutes);
            let reason = existing_adjustments
                .iter()
                .find(|adjustment| adjustment.week_number == week.week_number)
                .and_then(|adjustment| adjustment.reason.clone());
            ManualHoursAdjustment {
                week_number: week.week_number,
                adjustment_minutes: final_minutes - baseline_minutes,
                reason,
            }
        })
        .collect::<Vec<_>>();
    let desired_persisted_adjustments = desired_adjustments
        .iter()
        .filter(|adjustment| adjustment.adjustment_minutes != 0 || adjustment.reason.is_some())
        .cloned()
        .collect::<Vec<_>>();

    let changed = baseline.is_none_or(|baseline| {
        !optional_hours_equal(baseline.previous_cycle_hours, record.previous_cycle_hours)
            || !weeks_equal(&baseline.weeks, &weeks)
            || !public_holidays_equal(&baseline.public_holidays, public_holidays)
            || !annual_leave_equal(&baseline.annual_leave, &annual_leave)
            || baseline.manual_adjustments != desired_persisted_adjustments
    });
    if !changed {
        return Ok(SaveResult {
            changed: false,
            candidate_invalidated: false,
            public_holiday_totals,
            annual_leave_totals,
            normalised_annual_leave: annual_leave,
        });
    }

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let candidate_invalidated = application
        .payroll_timesheet_repository
        .save_preparation_atomically(
            record,
            &weeks,
            public_holidays,
            &annual_leave,
            &desired_adjustments,
            &now,
        )?;

    Ok(SaveResult {
        changed: true,
        candidate_invalidated,
        public_holiday_totals,
        annual_leave_totals,
        normalised_annual_leave: annual_leave,
    })
}

fn preparation_save_error(error: &(dyn std::error::Error + 'static)) -> String {
    // Repository validation uses this existing error variant; omit its SQL
    // parameter prefix in the user-facing message while preserving other errors.
    if let Some(rusqlite::Error::InvalidParameterName(message)) =
        error.downcast_ref::<rusqlite::Error>()
    {
        format!("Save failed: {message}")
    } else {
        format!("Save failed: {error}")
    }
}

fn annual_leave_equal(
    left: &[PayrollTimesheetAnnualLeave],
    right: &[PayrollTimesheetAnnualLeave],
) -> bool {
    let keys = |rows: &[PayrollTimesheetAnnualLeave]| {
        let mut values = rows
            .iter()
            .map(|row| {
                (
                    row.payroll_timesheet_id,
                    row.week_number,
                    crate::payroll_timesheet_repository::parse_leave_date(&row.leave_date),
                    row.hours.to_bits(),
                )
            })
            .collect::<Vec<_>>();
        values.sort();
        values
    };
    keys(left) == keys(right)
}

fn new_annual_leave(week: &PayrollTimesheetWeek, hours: f64) -> PayrollTimesheetAnnualLeave {
    PayrollTimesheetAnnualLeave {
        id: 0,
        payroll_timesheet_id: week.payroll_timesheet_id,
        week_number: week.week_number,
        leave_date: String::new(),
        hours,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn edit_annual_leave(
    ui: &mut egui::Ui,
    editor_texts: &mut HashMap<NumericEditorKey, String>,
    week: &mut PayrollTimesheetWeek,
    rows: &mut Vec<PayrollTimesheetAnnualLeave>,
    baseline: Option<&PreparationBaseline>,
) {
    let has_rows = rows.iter().any(|row| row.week_number == week.week_number);
    let legacy = baseline.is_some_and(|baseline| {
        baseline
            .weeks
            .iter()
            .any(|old| old.week_number == week.week_number && old.annual_leave_hours > 0.0)
            && !baseline
                .annual_leave
                .iter()
                .any(|row| row.week_number == week.week_number)
    });
    if !has_rows {
        let response = edit_preparation_number(
            ui,
            editor_texts,
            NumericEditorKey::AnnualLeave(week.id),
            &mut week.annual_leave_hours,
        );
        if legacy {
            ui.add(egui::Label::new("Legacy undated leave").wrap_mode(egui::TextWrapMode::Extend));
        } else if week.annual_leave_hours != 0.0 && !response.has_focus() {
            // Let the user finish typing decimals before replacing the editor.
            rows.push(new_annual_leave(week, week.annual_leave_hours));
            editor_texts.remove(&NumericEditorKey::AnnualLeave(week.id));
        }
    }
    if rows.iter().any(|row| row.week_number == week.week_number) {
        editor_texts.remove(&NumericEditorKey::AnnualLeave(week.id));
        let mut remove = None;
        for (index, row) in rows
            .iter_mut()
            .enumerate()
            .filter(|(_, row)| row.week_number == week.week_number)
        {
            ui.push_id((week.id, index), |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut row.leave_date)
                            .desired_width(85.0)
                            .hint_text("DD/MM/YYYY"),
                    );
                    ui.add(egui::DragValue::new(&mut row.hours).speed(0.25));
                    if ui.button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove {
            rows.remove(index);
        }
        if ui.button("Add another date").clicked() {
            rows.push(new_annual_leave(week, 0.0));
        }
        week.annual_leave_hours = rows
            .iter()
            .filter(|row| row.week_number == week.week_number)
            .map(|row| row.hours)
            .sum();
        ui.label(format!(
            "Total: {}",
            format_decimal_hours(week.annual_leave_hours)
        ));
    }
}

fn optional_hours_equal(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => (left - right).abs() <= f64::EPSILON,
        (None, None) => true,
        _ => false,
    }
}

fn weeks_equal(left: &[PayrollTimesheetWeek], right: &[PayrollTimesheetWeek]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.id == right.id
                && left.week_number == right.week_number
                && left.week_commencing == right.week_commencing
                && (left.worked_hours - right.worked_hours).abs() <= f64::EPSILON
                && (left.annual_leave_hours - right.annual_leave_hours).abs() <= f64::EPSILON
                && (left.sick_leave_hours - right.sick_leave_hours).abs() <= f64::EPSILON
                && (left.public_holiday_hours - right.public_holiday_hours).abs() <= f64::EPSILON
                && (left.travel_miles - right.travel_miles).abs() <= f64::EPSILON
        })
}

fn public_holidays_equal(
    left: &[PayrollTimesheetPublicHoliday],
    right: &[PayrollTimesheetPublicHoliday],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.id == right.id
                && left.week_number == right.week_number
                && left.holiday_date == right.holiday_date
                && (left.hours - right.hours).abs() <= f64::EPSILON
        })
}

fn public_holiday_sum_for_week(
    holidays: &[PayrollTimesheetPublicHoliday],
    week_number: i64,
) -> f64 {
    holidays
        .iter()
        .filter(|holiday| holiday.week_number == week_number)
        .map(|holiday| holiday.hours)
        .sum()
}

fn validate_public_holiday_values(
    weeks: &[PayrollTimesheetWeek],
    holidays: &[PayrollTimesheetPublicHoliday],
) -> Result<(), Box<dyn std::error::Error>> {
    for holiday in holidays {
        if !holiday.hours.is_finite() || holiday.hours < 0.0 {
            return Err(format!(
                "Public-holiday hours for {} must be a finite value of zero or more.",
                holiday.holiday_date
            )
            .into());
        }
        if !weeks
            .iter()
            .any(|week| week.week_number == holiday.week_number)
        {
            return Err(format!(
                "Public-holiday date {} refers to invalid payroll week {}.",
                holiday.holiday_date, holiday.week_number
            )
            .into());
        }
    }
    Ok(())
}

fn derive_public_holiday_aggregates(
    weeks: &mut [PayrollTimesheetWeek],
    holidays: &[PayrollTimesheetPublicHoliday],
) -> Result<(), Box<dyn std::error::Error>> {
    validate_public_holiday_values(weeks, holidays)?;
    for week in weeks {
        week.public_holiday_hours = public_holiday_sum_for_week(holidays, week.week_number);
    }
    Ok(())
}

fn validate_public_holiday_consistency(
    weeks: &[PayrollTimesheetWeek],
    holidays: &[PayrollTimesheetPublicHoliday],
) -> Result<(), String> {
    for week in weeks {
        let detail_total = public_holiday_sum_for_week(holidays, week.week_number);
        if (week.public_holiday_hours - detail_total).abs() > 0.000_001 {
            return Err(format!(
                "week {} stores {} aggregate hour(s), but its dated entries total {}. No values were changed.",
                week.week_number,
                format_decimal_hours(week.public_holiday_hours),
                format_decimal_hours(detail_total)
            ));
        }
    }
    Ok(())
}

fn pa_section_style(
    displayed_index: usize,
    theme: ApplicationTheme,
    visuals: &egui::Visuals,
) -> PaSectionStyle {
    if displayed_index % 2 == 0 {
        return PaSectionStyle {
            fill: visuals.panel_fill,
            stroke: egui::Stroke::NONE,
        };
    }

    if theme == ApplicationTheme::AccessibleHighContrast {
        PaSectionStyle {
            fill: visuals.panel_fill,
            stroke: visuals.widgets.noninteractive.bg_stroke,
        }
    } else {
        PaSectionStyle {
            fill: visuals.faint_bg_color,
            stroke: egui::Stroke::NONE,
        }
    }
}

fn edit_number(
    ui: &mut egui::Ui,
    editor_texts: &mut HashMap<NumericEditorKey, String>,
    key: NumericEditorKey,
    value: &mut f64,
) {
    edit_preparation_number(ui, editor_texts, key, value);
}

fn edit_optional_number(
    ui: &mut egui::Ui,
    editor_texts: &mut HashMap<NumericEditorKey, String>,
    key: NumericEditorKey,
    value: &mut f64,
) {
    edit_preparation_number(ui, editor_texts, key, value);
}

fn numeric_editor_text(value: f64) -> String {
    format_decimal_hours(value)
}

fn edit_preparation_number(
    ui: &mut egui::Ui,
    editor_texts: &mut HashMap<NumericEditorKey, String>,
    key: NumericEditorKey,
    value: &mut f64,
) -> egui::Response {
    let text = editor_texts
        .entry(key)
        .or_insert_with(|| numeric_editor_text(*value));
    let response = edit_numeric_text(ui, text);

    if response.changed() {
        update_numeric_value(text, value);
    }
    if response.lost_focus() {
        commit_numeric_text(text, value);
    }
    response
}

fn update_numeric_value(text: &str, value: &mut f64) {
    if let Ok(parsed) = text.trim().parse::<f64>() {
        *value = parsed;
    }
}

fn commit_numeric_text(text: &mut String, value: &mut f64) {
    if text.trim().is_empty() {
        *value = 0.0;
    } else {
        update_numeric_value(text, value);
    }
    *text = format_decimal_hours(*value);
}

fn commit_record_numeric_editors(
    editor_texts: &mut HashMap<NumericEditorKey, String>,
    weeks: &mut [PayrollTimesheetWeek],
    holidays: &mut [PayrollTimesheetPublicHoliday],
) {
    for week in weeks {
        for (key, value) in [
            (NumericEditorKey::Worked(week.id), &mut week.worked_hours),
            (
                NumericEditorKey::AnnualLeave(week.id),
                &mut week.annual_leave_hours,
            ),
            (
                NumericEditorKey::SickLeave(week.id),
                &mut week.sick_leave_hours,
            ),
            (
                NumericEditorKey::TravelMiles(week.id),
                &mut week.travel_miles,
            ),
        ] {
            if let Some(text) = editor_texts.get_mut(&key) {
                commit_numeric_text(text, value);
            }
        }
    }

    for holiday in holidays {
        if let Some(text) = editor_texts.get_mut(&NumericEditorKey::PublicHoliday(holiday.id)) {
            commit_numeric_text(text, &mut holiday.hours);
        }
    }
}

fn edit_numeric_text(ui: &mut egui::Ui, text: &mut String) -> egui::Response {
    let mut output = egui::TextEdit::singleline(text).show(ui);

    if output.response.gained_focus() {
        output
            .state
            .cursor
            .set_char_range(Some(full_text_selection(text)));
        output.state.store(ui.ctx(), output.response.id);
    }

    output.response
}

fn full_text_selection(text: &str) -> egui::text::CCursorRange {
    egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(text.chars().count()),
    )
}

fn calculate_actual_hours(
    timesheets: &[crate::models::TimesheetEntry],
    personal_assistant_id: i64,
    week_dates: &[chrono::NaiveDate; 4],
) -> [f64; 4] {
    let mut totals = [0i64; 4];

    for timesheet in timesheets {
        if timesheet.personal_assistant_id != Some(personal_assistant_id) {
            continue;
        }

        let date = match extract_timesheet_date(&timesheet.start_time) {
            Some(date) => date,
            None => continue,
        };

        for index in 0..4 {
            let start = week_dates[index];
            let end = start + chrono::Duration::days(6);

            if date >= start && date <= end {
                totals[index] += timesheet.worked_minutes;
                break;
            }
        }
    }

    [
        minutes_to_decimal_hours(totals[0]),
        minutes_to_decimal_hours(totals[1]),
        minutes_to_decimal_hours(totals[2]),
        minutes_to_decimal_hours(totals[3]),
    ]
}

fn calculate_previous_cycle_adjustment(
    application: &Application,
    previous_schedule: &PayrollSchedule,
    personal_assistant_id: i64,
    timesheets: &[crate::models::TimesheetEntry],
    current_cycle_first_week: chrono::NaiveDate,
) -> Result<Option<f64>, Box<dyn std::error::Error>> {
    let previous_record = application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa(
            &previous_schedule.payroll_year,
            previous_schedule.cycle_number,
            personal_assistant_id,
        )?;

    let previous_record = match previous_record {
        Some(record) => record,
        None => return Ok(None),
    };

    let previous_weeks = application
        .payroll_timesheet_repository
        .get_weeks(previous_record.id)?;

    if previous_weeks.len() < 4 {
        return Ok(None);
    }

    let previous_week_3 = parse_date(&previous_weeks[2].week_commencing)
        .ok_or("Invalid previous payroll week 3 date.")?;

    let previous_week_4 = parse_date(&previous_weeks[3].week_commencing)
        .ok_or("Invalid previous payroll week 4 date.")?;

    let actual_previous_hours = calculate_hours_between(
        timesheets,
        personal_assistant_id,
        previous_week_3,
        current_cycle_first_week,
    );

    let previously_submitted_hours =
        previous_weeks[2].worked_hours + previous_weeks[3].worked_hours;

    let adjustment = actual_previous_hours - previously_submitted_hours;

    let adjustment = (adjustment * 100.0).round() / 100.0;

    if adjustment > 0.0 {
        Ok(Some(adjustment))
    } else {
        let _ = previous_week_4;
        Ok(None)
    }
}

fn calculate_hours_between(
    timesheets: &[crate::models::TimesheetEntry],
    personal_assistant_id: i64,
    start_date: chrono::NaiveDate,
    end_date_exclusive: chrono::NaiveDate,
) -> f64 {
    let mut total_minutes = 0i64;

    for timesheet in timesheets {
        if timesheet.personal_assistant_id != Some(personal_assistant_id) {
            continue;
        }

        let date = match extract_timesheet_date(&timesheet.start_time) {
            Some(date) => date,
            None => continue,
        };

        if date >= start_date && date < end_date_exclusive {
            total_minutes += timesheet.worked_minutes;
        }
    }

    minutes_to_decimal_hours(total_minutes)
}

// ------------------------------------------------------------
// England & Wales bank holidays
//
// These are the published bank-holiday dates, including
// substitute days. Actual weekend dates that are replaced by
// substitute days are therefore NOT returned separately.
// ------------------------------------------------------------

fn bank_holidays_for_period(
    start_date: chrono::NaiveDate,
    end_date: chrono::NaiveDate,
) -> Vec<chrono::NaiveDate> {
    let mut holidays = Vec::new();

    for year in start_date.year()..=end_date.year() {
        holidays.extend(bank_holidays_for_year(year));
    }

    holidays.retain(|date| *date >= start_date && *date <= end_date);

    holidays.sort();

    holidays.dedup();

    holidays
}

fn bank_holidays_for_year(year: i32) -> Vec<chrono::NaiveDate> {
    use chrono::Weekday;

    let mut holidays = Vec::new();

    // New Year's Day.
    let new_year = chrono::NaiveDate::from_ymd_opt(year, 1, 1).expect("Invalid New Year's Day");

    holidays.push(observed_monday_if_weekend(new_year));

    // Easter-related bank holidays.
    let easter_sunday = calculate_easter_sunday(year);

    holidays.push(easter_sunday - chrono::Duration::days(2)); // Good Friday
    holidays.push(easter_sunday + chrono::Duration::days(1)); // Easter Monday

    // Early May bank holiday - first Monday in May.
    holidays.push(first_monday_of_month(year, 5));

    // Spring bank holiday - last Monday in May.
    holidays.push(last_monday_of_month(year, 5));

    // Summer bank holiday - last Monday in August.
    holidays.push(last_monday_of_month(year, 8));

    // Christmas Day and Boxing Day.
    //
    // Where either falls on a weekend, use the published
    // substitute-day arrangement.
    let christmas = chrono::NaiveDate::from_ymd_opt(year, 12, 25).expect("Invalid Christmas Day");

    let boxing_day = chrono::NaiveDate::from_ymd_opt(year, 12, 26).expect("Invalid Boxing Day");

    match (christmas.weekday(), boxing_day.weekday()) {
        (Weekday::Sat, Weekday::Sun) => {
            holidays.push(
                chrono::NaiveDate::from_ymd_opt(year, 12, 27)
                    .expect("Invalid Christmas substitute day"),
            );

            holidays.push(
                chrono::NaiveDate::from_ymd_opt(year, 12, 28)
                    .expect("Invalid Boxing Day substitute day"),
            );
        }

        (Weekday::Sun, Weekday::Mon) => {
            holidays.push(boxing_day);

            holidays.push(
                chrono::NaiveDate::from_ymd_opt(year, 12, 27)
                    .expect("Invalid Christmas substitute day"),
            );
        }

        (_, Weekday::Sat) => {
            holidays.push(christmas);

            holidays.push(boxing_day + chrono::Duration::days(2));
        }

        _ => {
            holidays.push(christmas);
            holidays.push(boxing_day);
        }
    }

    holidays
}

fn observed_monday_if_weekend(date: chrono::NaiveDate) -> chrono::NaiveDate {
    match date.weekday() {
        chrono::Weekday::Sat => date + chrono::Duration::days(2),
        chrono::Weekday::Sun => date + chrono::Duration::days(1),
        _ => date,
    }
}

fn first_monday_of_month(year: i32, month: u32) -> chrono::NaiveDate {
    let first =
        chrono::NaiveDate::from_ymd_opt(year, month, 1).expect("Invalid first day of month");

    first + chrono::Duration::days((7 - first.weekday().num_days_from_monday()) as i64 % 7)
}

fn last_monday_of_month(year: i32, month: u32) -> chrono::NaiveDate {
    let next_month = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("Invalid next year")
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1).expect("Invalid next month")
    };

    let last_day = next_month - chrono::Duration::days(1);

    last_day - chrono::Duration::days(last_day.weekday().num_days_from_monday() as i64)
}

fn calculate_easter_sunday(year: i32) -> chrono::NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;

    let month = (h + l - 7 * m + 114) / 31;
    let day = ((h + l - 7 * m + 114) % 31) + 1;

    chrono::NaiveDate::from_ymd_opt(year, month as u32, day as u32)
        .expect("Invalid calculated Easter Sunday")
}

fn minutes_to_decimal_hours(minutes: i64) -> f64 {
    ((minutes as f64 / 60.0) * 100.0).round() / 100.0
}

fn extract_timesheet_date(value: &str) -> Option<chrono::NaiveDate> {
    let value = value.trim();

    if let Some(date) = parse_date(value) {
        return Some(date);
    }

    if let Some(date_part) = value.split(" at ").next() {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part.trim(), "%d %B %Y") {
            return Some(date);
        }

        if let Ok(date) = chrono::NaiveDate::parse_from_str(date_part.trim(), "%d %b %Y") {
            return Some(date);
        }
    }

    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::app::Application;
    use crate::config::AppConfig;
    use crate::context::AppContext;
    use crate::environment::AppEnvironment;
    use rusqlite::{params, Connection};
    use tempfile::TempDir;

    pub(crate) fn test_application() -> (TempDir, Application) {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("test.sqlite");
        let connection = Connection::open(&database_path).unwrap();
        crate::database::create_schema(&connection).unwrap();
        drop(connection);
        let open = || Connection::open(&database_path).unwrap();
        let environment = AppEnvironment {
            data_dir: directory.path().to_path_buf(),
            database_path: database_path.clone(),
            import_dir: directory.path().join("import"),
            archive_dir: directory.path().join("archive"),
            backups_dir: directory.path().join("backups"),
            logs_dir: directory.path().join("logs"),
            templates_dir: directory.path().join("templates"),
            cache_dir: directory.path().join("cache"),
        };
        let application = Application {
            annual_leave_settings_repository:
                crate::annual_leave_settings_repository::AnnualLeaveSettingsRepository::new(
                    Connection::open(&database_path).unwrap(),
                ),
            context: AppContext {
                environment,
                config: AppConfig::default(),
                version: "test".to_string(),
            },
            repository: crate::repository::TimesheetRepository::new(open()),
            employer_repository: crate::employer_repository::EmployerRepository::new(open()),
            personal_assistant_repository:
                crate::personal_assistant_repository::PersonalAssistantRepository::new(open()),
            pay_rate_repository: crate::pay_rate_repository::PayRateRepository::new(open()),
            contracted_hours_repository:
                crate::contracted_hours_repository::ContractedHoursRepository::new(open()),
            direct_shift_repository: crate::direct_shift_repository::DirectShiftRepository::new(
                open(),
            ),
            payroll_provider_repository:
                crate::payroll_provider_repository::PayrollProviderRepository::new(open()),
            payroll_schedule_repository:
                crate::payroll_schedule_repository::PayrollScheduleRepository::new(open()),
            payroll_timesheet_repository:
                crate::payroll_timesheet_repository::PayrollTimesheetRepository::new(open()),
            payroll_worked_item_repository:
                crate::payroll_worked_item_repository::PayrollWorkedItemRepository::new(open()),
            payroll_timesheet_email_repository:
                crate::payroll_timesheet_email_repository::PayrollTimesheetEmailRepository::new(
                    open(),
                ),
        };
        (directory, application)
    }

    fn setup_connection(application: &Application) -> Connection {
        Connection::open(&application.context.environment.database_path).unwrap()
    }

    fn insert_schedule(
        application: &Application,
        year: &str,
        cycle: i64,
        first_week: &str,
        pay_date: &str,
    ) -> PayrollSchedule {
        let connection = setup_connection(application);
        connection
            .execute(
                "INSERT INTO payroll_schedules (
                    payroll_year, cycle_number, first_week_commencing,
                    latest_posting_date, pay_date, created_at, payslips_sent
                 ) VALUES (?1, ?2, ?3, ?3, ?4, 'created', 0)",
                params![year, cycle, first_week, pay_date],
            )
            .unwrap();
        application
            .payroll_schedule_repository
            .get_for_year_and_cycle(year, cycle)
            .unwrap()
            .unwrap()
    }

    fn insert_pa(application: &Application, id: i64, name: &str, status: Option<&str>) {
        let connection = setup_connection(application);
        connection
            .execute(
                "INSERT INTO personal_assistants (id, first_name, surname, employment_status)
                 VALUES (?1, ?2, 'Test', ?3)",
                params![id, name, status],
            )
            .unwrap();
    }

    fn load_active_record() -> (
        TempDir,
        Application,
        PayrollSchedule,
        PayrollTimesheetScreen,
    ) {
        let (directory, application) = test_application();
        let schedule = insert_schedule(&application, "2026/27", 6, "10/08/2026", "04/09/2026");
        insert_pa(&application, 1, "Active", Some("Active"));
        let mut screen = PayrollTimesheetScreen::new();
        screen
            .load(
                &application,
                &schedule,
                "2026/27 · Week 22 · 10/08/2026 to 06/09/2026 · Pay 04/09/2026",
            )
            .unwrap();
        (directory, application, schedule, screen)
    }

    fn create_candidate(application: &Application, screen: &PayrollTimesheetScreen) {
        let record = &screen.weeks[0].0;
        let weeks = &screen.weeks[0].1;
        let week_ids = [weeks[0].id, weeks[1].id, weeks[2].id, weeks[3].id];
        application
            .payroll_worked_item_repository
            .replace_candidate(
                record.id,
                &[],
                "/tmp/stale-candidate.pdf",
                "digest",
                "generated",
                0,
                &week_ids,
                &[0, 0, 0, 0],
            )
            .unwrap();
    }

    fn save_current_row(
        application: &Application,
        schedule: &PayrollSchedule,
        screen: &PayrollTimesheetScreen,
    ) -> Result<SaveResult, Box<dyn std::error::Error>> {
        let (record, weeks, _) = &screen.weeks[0];
        save_preparation_record(
            application,
            screen.bound_period.as_ref(),
            schedule,
            record,
            weeks,
            &screen.public_holidays[0],
            screen
                .annual_leave
                .get(&record.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            &screen.worked_hours_baselines,
            screen.preparation_baselines.get(&record.id),
        )
    }

    fn set_dated_leave(
        screen: &mut PayrollTimesheetScreen,
        week_index: usize,
        offset: i64,
        hours: f64,
    ) {
        let week = &screen.weeks[0].1[week_index];
        let mut row = new_annual_leave(week, hours);
        row.leave_date = format_date(
            parse_date(&week.week_commencing).unwrap() + chrono::Duration::days(offset),
        );
        screen
            .annual_leave
            .entry(week.payroll_timesheet_id)
            .or_default()
            .push(row);
    }

    #[test]
    fn out_of_week_error_identifies_entered_date_and_week() {
        let (_directory, _application, _schedule, screen) = load_active_record();
        let mut weeks = screen.weeks[0].1.clone();
        weeks[0].week_commencing = "14/09/2026".into();
        let mut row = new_annual_leave(&weeks[0], 2.0);
        row.leave_date = "22/09/2026".into();
        let error = derive_annual_leave(weeks[0].payroll_timesheet_id, &mut weeks, &[row], &[])
            .unwrap_err();
        assert_eq!(
            preparation_save_error(&error),
            "Save failed: Annual-leave date 22/09/2026 is outside the week commencing 14/09/2026."
        );
    }

    #[test]
    fn flexible_leave_dates_normalise_for_storage_and_successful_save_display() {
        use crate::payroll_timesheet_repository::parse_leave_date;
        for input in [
            "18/9/26",
            "18/09/2026",
            "18-9-26",
            "18-09-2026",
            "2026-09-18",
        ] {
            assert_eq!(
                parse_leave_date(input)
                    .unwrap()
                    .format("%d/%m/%Y")
                    .to_string(),
                "18/09/2026"
            );
        }
        let (_directory, application, schedule, mut screen) = load_active_record();
        let id = screen.weeks[0].0.id;
        set_dated_leave(&mut screen, 0, 0, 2.0);
        let expected = screen.annual_leave[&id][0].leave_date.clone();
        screen.annual_leave.get_mut(&id).unwrap()[0].leave_date = parse_leave_date(&expected)
            .unwrap()
            .format("%-d/%-m/%y")
            .to_string();
        let result = save_current_row(&application, &schedule, &screen).unwrap();
        assert_eq!(result.normalised_annual_leave[0].leave_date, expected);
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_annual_leave(id)
                .unwrap()[0]
                .leave_date,
            expected
        );
        screen.reload();
        screen.load(&application, &schedule, "period").unwrap();
        screen.annual_leave.get_mut(&id).unwrap()[0].leave_date = parse_leave_date(&expected)
            .unwrap()
            .format("%-d/%-m/%y")
            .to_string();
        let result = save_current_row(&application, &schedule, &screen).unwrap();
        assert!(!result.changed);
        assert_eq!(result.normalised_annual_leave[0].leave_date, expected);
    }

    #[test]
    fn new_dated_detail_carries_hours_but_never_invents_a_date() {
        let (_directory, _application, _schedule, screen) = load_active_record();
        let week = &screen.weeks[0].1[0];
        let row = new_annual_leave(week, 7.25);
        assert_eq!(row.hours, 7.25);
        assert_eq!(row.payroll_timesheet_id, week.payroll_timesheet_id);
        assert_eq!(row.week_number, week.week_number);
        assert!(row.leave_date.is_empty());
    }

    #[test]
    fn dated_leave_sums_loads_in_order_and_last_removal_returns_zero() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        let id = screen.weeks[0].0.id;
        set_dated_leave(&mut screen, 0, 5, 2.5);
        screen.weeks[0].1[0].annual_leave_hours = 999.0; // Dated rows override a conflicting total.
        let result = save_current_row(&application, &schedule, &screen).unwrap();
        assert_eq!(result.annual_leave_totals[0], 2.5);
        set_dated_leave(&mut screen, 1, 0, 1.0);
        set_dated_leave(&mut screen, 0, 1, 3.0);
        save_current_row(&application, &schedule, &screen).unwrap();
        let rows = application
            .payroll_timesheet_repository
            .get_annual_leave(id)
            .unwrap();
        assert_eq!(
            rows.iter().map(|row| row.week_number).collect::<Vec<_>>(),
            vec![1, 1, 2]
        );
        assert!(parse_date(&rows[0].leave_date) < parse_date(&rows[1].leave_date));
        assert!(!rows[0].created_at.is_empty());
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(id)
                .unwrap()[0]
                .annual_leave_hours,
            5.5
        );
        screen.reload();
        screen.load(&application, &schedule, "period").unwrap();
        assert!(
            !save_current_row(&application, &schedule, &screen)
                .unwrap()
                .changed
        );
        screen
            .annual_leave
            .get_mut(&id)
            .unwrap()
            .retain(|row| row.week_number != 1);
        assert!(
            save_current_row(&application, &schedule, &screen)
                .unwrap()
                .changed
        );
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(id)
                .unwrap()[0]
                .annual_leave_hours,
            0.0
        );
    }

    #[test]
    fn dated_leave_validation_rejects_bad_dates_hours_ownership_and_duplicates() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        let id = screen.weeks[0].0.id;
        for offset in [-1, 7] {
            set_dated_leave(&mut screen, 0, offset, 1.0);
            assert!(save_current_row(&application, &schedule, &screen).is_err());
            screen.annual_leave.get_mut(&id).unwrap().clear();
        }
        for hours in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            set_dated_leave(&mut screen, 0, 0, hours);
            assert!(save_current_row(&application, &schedule, &screen).is_err());
            screen.annual_leave.get_mut(&id).unwrap().clear();
        }
        set_dated_leave(&mut screen, 0, 0, 1.0);
        let valid = screen.annual_leave[&id][0].clone();
        for bad_date in ["", "31/02/2026", "not a date"] {
            screen.annual_leave.get_mut(&id).unwrap()[0].leave_date = bad_date.into();
            assert!(save_current_row(&application, &schedule, &screen).is_err());
        }
        screen.annual_leave.insert(id, vec![valid.clone()]);
        screen.annual_leave.get_mut(&id).unwrap()[0].payroll_timesheet_id = -1;
        assert!(save_current_row(&application, &schedule, &screen).is_err());
        screen.annual_leave.insert(id, vec![valid.clone()]);
        screen.annual_leave.get_mut(&id).unwrap()[0].week_number = 5;
        assert!(save_current_row(&application, &schedule, &screen).is_err());
        let mut duplicate = valid.clone();
        duplicate.leave_date = parse_date(&valid.leave_date)
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        screen.annual_leave.insert(id, vec![valid, duplicate]);
        assert!(save_current_row(&application, &schedule, &screen).is_err());
        assert!(application
            .payroll_timesheet_repository
            .get_annual_leave(id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn legacy_undated_leave_survives_load_and_unrelated_save() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        let id = screen.weeks[0].0.id;
        setup_connection(&application).execute("UPDATE payroll_timesheet_weeks SET annual_leave_hours = 7.25 WHERE payroll_timesheet_id = ?1 AND week_number = 1", [id]).unwrap();
        screen.reload();
        screen.load(&application, &schedule, "period").unwrap();
        assert!(
            !save_current_row(&application, &schedule, &screen)
                .unwrap()
                .changed
        );
        screen.weeks[0].1[0].worked_hours = 1.0;
        save_current_row(&application, &schedule, &screen).unwrap();
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(id)
                .unwrap()[0]
                .annual_leave_hours,
            7.25
        );
        assert!(application
            .payroll_timesheet_repository
            .get_annual_leave(id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn dated_date_only_change_invalidates_candidate_and_protected_states_refuse_saves() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        let id = screen.weeks[0].0.id;
        set_dated_leave(&mut screen, 0, 0, 2.0);
        save_current_row(&application, &schedule, &screen).unwrap();
        screen.reload();
        screen.load(&application, &schedule, "period").unwrap();
        create_candidate(&application, &screen);
        let new_date = format_date(
            parse_date(&screen.weeks[0].1[0].week_commencing).unwrap() + chrono::Duration::days(1),
        );
        screen.annual_leave.get_mut(&id).unwrap()[0].leave_date = new_date;
        let result = save_current_row(&application, &schedule, &screen).unwrap();
        assert!(result.changed && result.candidate_invalidated);
        create_candidate(&application, &screen);
        for state in ["submitted", "indeterminate"] {
            setup_connection(&application).execute("UPDATE payroll_timesheet_snapshot_states SET state = ?1 WHERE payroll_timesheet_id = ?2", rusqlite::params![state, id]).unwrap();
            screen.annual_leave.get_mut(&id).unwrap()[0].hours = 9.0;
            assert!(save_current_row(&application, &schedule, &screen).is_err());
            let adjustments = screen.weeks[0]
                .1
                .iter()
                .map(|week| ManualHoursAdjustment {
                    week_number: week.week_number,
                    adjustment_minutes: 0,
                    reason: None,
                })
                .collect::<Vec<_>>();
            assert!(application
                .payroll_timesheet_repository
                .save_preparation_atomically(
                    &screen.weeks[0].0,
                    &screen.weeks[0].1,
                    &screen.public_holidays[0],
                    &screen.annual_leave[&id],
                    &adjustments,
                    "now"
                )
                .is_err());
            assert_eq!(
                application
                    .payroll_timesheet_repository
                    .get_annual_leave(id)
                    .unwrap()[0]
                    .hours,
                2.0
            );
        }
    }

    #[test]
    fn failed_dated_leave_write_rolls_back_weekly_fields_adjustments_and_candidate() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        let id = screen.weeks[0].0.id;
        create_candidate(&application, &screen);
        setup_connection(&application)
            .execute_batch(
                "CREATE TRIGGER refuse_annual_leave BEFORE INSERT ON payroll_timesheet_annual_leave
             BEGIN SELECT RAISE(ABORT, 'dated leave failed'); END;",
            )
            .unwrap();
        set_dated_leave(&mut screen, 0, 0, 2.0);
        screen.weeks[0].1[0].worked_hours = 1.0;
        assert!(save_current_row(&application, &schedule, &screen).is_err());
        let week = &application
            .payroll_timesheet_repository
            .get_weeks(id)
            .unwrap()[0];
        assert_eq!(week.worked_hours, 0.0);
        assert_eq!(week.annual_leave_hours, 0.0);
        assert!(application
            .payroll_timesheet_repository
            .get_annual_leave(id)
            .unwrap()
            .is_empty());
        assert!(application
            .payroll_worked_item_repository
            .get_manual_adjustments(id)
            .unwrap()
            .is_empty());
        assert_eq!(
            application
                .payroll_worked_item_repository
                .snapshot_metadata(id)
                .unwrap()
                .unwrap()
                .state,
            SnapshotState::Candidate
        );
    }

    #[test]
    fn supplied_historical_current_and_future_schedules_are_bound_exactly() {
        let (_directory, application) = test_application();
        insert_pa(&application, 1, "Active", Some("Active"));
        let schedules = [
            insert_schedule(&application, "2025/26", 1, "24/03/2025", "18/04/2025"),
            insert_schedule(&application, "2026/27", 6, "10/08/2026", "04/09/2026"),
            insert_schedule(&application, "2027/28", 1, "22/03/2027", "16/04/2027"),
        ];

        for schedule in schedules {
            let mut screen = PayrollTimesheetScreen::new();
            screen
                .load(&application, &schedule, "Week-labelled period")
                .unwrap();
            assert_eq!(
                screen.bound_period,
                Some(BoundPayrollPeriod::from(&schedule))
            );
            assert_eq!(
                screen.schedule.as_ref().unwrap().payroll_year,
                schedule.payroll_year
            );
        }
    }

    #[test]
    fn operational_period_change_clears_stale_preparation_before_rebinding() {
        let (_directory, application) = test_application();
        insert_pa(&application, 1, "Active", Some("Active"));
        let first = insert_schedule(&application, "2026/27", 6, "10/08/2026", "04/09/2026");
        let second = insert_schedule(&application, "2026/27", 7, "07/09/2026", "02/10/2026");
        let mut screen = PayrollTimesheetScreen::new();
        screen.load(&application, &first, "first period").unwrap();
        screen.loaded = true;
        screen.numeric_editor_texts.insert(
            NumericEditorKey::Worked(screen.weeks[0].1[0].id),
            "stale edit".to_string(),
        );

        assert!(screen.rebind_if_operational_period_changed(&second));
        assert!(!screen.loaded);
        assert!(screen.bound_period.is_none());
        assert!(screen.weeks.is_empty());
        assert!(screen.numeric_editor_texts.is_empty());

        screen.load(&application, &second, "second period").unwrap();
        assert_eq!(screen.bound_period, Some(BoundPayrollPeriod::from(&second)));
        assert_eq!(screen.period_label, "second period");
    }

    #[test]
    fn unchanged_operational_period_preserves_preparation_state() {
        let (_directory, _application, schedule, mut screen) = load_active_record();
        let record_id = screen.weeks[0].0.id;

        assert!(!screen.rebind_if_operational_period_changed(&schedule));
        assert_eq!(screen.weeks[0].0.id, record_id);
        assert_eq!(
            screen.bound_period,
            Some(BoundPayrollPeriod::from(&schedule))
        );
    }

    #[test]
    fn pa_section_alternation_is_deterministic_by_display_order_for_every_theme() {
        let visuals = egui::Visuals::dark();
        let themes = [
            ApplicationTheme::System,
            ApplicationTheme::Light,
            ApplicationTheme::SoftLight,
            ApplicationTheme::Dark,
            ApplicationTheme::SoftDark,
            ApplicationTheme::Blue,
            ApplicationTheme::AccessibleHighContrast,
        ];

        for theme in themes {
            let first = pa_section_style(0, theme, &visuals);
            let second = pa_section_style(1, theme, &visuals);
            let third = pa_section_style(2, theme, &visuals);

            assert_eq!(first, third);
            if theme == ApplicationTheme::AccessibleHighContrast {
                assert_eq!(second.fill, visuals.panel_fill);
                assert_ne!(second.stroke, egui::Stroke::NONE);
            } else {
                assert_eq!(second.fill, visuals.faint_bg_color);
                assert_ne!(first.fill, second.fill);
            }
        }
    }

    #[test]
    fn visible_period_label_uses_week_wording_without_internal_cycle() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        let label = "2026/27 · Week 22 · 10/08/2026 to 06/09/2026 · Pay 04/09/2026";
        screen.reload();
        screen.load(&application, &schedule, label).unwrap();
        assert_eq!(screen.period_label, label);
        assert!(!screen.period_label.contains("Cycle"));
        assert!(!screen.period_label.contains("C6"));
    }

    #[test]
    fn preparation_respects_dates_but_keeps_existing_leaver_record() {
        let (_directory, application) = test_application();
        let schedule = insert_schedule(&application, "2026/27", 6, "10/08/2026", "04/09/2026");
        for id in 1..=4 {
            insert_pa(&application, id, &format!("PA{id}"), Some("Active"));
        }
        let connection = setup_connection(&application);
        connection
            .execute_batch(
                "UPDATE personal_assistants SET leaving_date='09/08/2026' WHERE id IN (1,2);
            UPDATE personal_assistants SET leaving_date='10/08/2026' WHERE id=3;
            UPDATE personal_assistants SET start_date='07/09/2026' WHERE id=4;",
            )
            .unwrap();
        application
            .payroll_timesheet_repository
            .insert("2026/27", 6, 2, None, "created")
            .unwrap();
        let mut screen = PayrollTimesheetScreen::new();
        screen.load(&application, &schedule, "period").unwrap();
        let ids = screen
            .weeks
            .iter()
            .map(|(record, _, _)| record.personal_assistant_id)
            .collect::<Vec<_>>();
        assert!(!ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
        assert!(!ids.contains(&4));
        assert_eq!(
            application.personal_assistant_repository.get_all().unwrap()[0]
                .employment_status
                .as_deref(),
            Some("Active")
        );
    }

    #[test]
    fn pa_union_includes_active_and_inactive_with_record_but_not_unreferenced_inactive() {
        let (_directory, application) = test_application();
        let schedule = insert_schedule(&application, "2026/27", 6, "10/08/2026", "04/09/2026");
        insert_pa(&application, 1, "Active", Some("Active"));
        insert_pa(&application, 2, "Historical", Some("Inactive"));
        insert_pa(&application, 3, "Excluded", Some("Inactive"));
        insert_pa(&application, 4, "LegacyActive", None);
        application
            .payroll_timesheet_repository
            .insert("2026/27", 6, 2, None, "created")
            .unwrap();
        let mut screen = PayrollTimesheetScreen::new();
        screen.load(&application, &schedule, "period").unwrap();
        let names = screen
            .weeks
            .iter()
            .map(|(_, _, name)| name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"Active Test"));
        assert!(names.contains(&"Historical Test"));
        assert!(names.contains(&"LegacyActive Test"));
        assert!(!names.contains(&"Excluded Test"));
    }

    fn assert_protected_load_is_read_only(state: SnapshotState) {
        let (_directory, application, schedule, screen) = load_active_record();
        create_candidate(&application, &screen);
        let record_id = screen.weeks[0].0.id;
        let connection = setup_connection(&application);
        connection
            .execute(
                "UPDATE payroll_timesheet_snapshot_states SET state = ?1 WHERE payroll_timesheet_id = ?2",
                params![match state { SnapshotState::Submitted => "submitted", SnapshotState::Indeterminate => "indeterminate", SnapshotState::Candidate => "candidate" }, record_id],
            )
            .unwrap();
        let before_record = application
            .payroll_timesheet_repository
            .get_for_cycle_and_pa("2026/27", 6, 1)
            .unwrap()
            .unwrap();
        let before_weeks = application
            .payroll_timesheet_repository
            .get_weeks(record_id)
            .unwrap();
        let before_holidays = application
            .payroll_timesheet_repository
            .get_public_holidays(record_id)
            .unwrap();

        let mut reloaded = PayrollTimesheetScreen::new();
        reloaded.load(&application, &schedule, "period").unwrap();

        let after_record = application
            .payroll_timesheet_repository
            .get_for_cycle_and_pa("2026/27", 6, 1)
            .unwrap()
            .unwrap();
        let after_weeks = application
            .payroll_timesheet_repository
            .get_weeks(record_id)
            .unwrap();
        let after_holidays = application
            .payroll_timesheet_repository
            .get_public_holidays(record_id)
            .unwrap();
        assert_eq!(before_record.updated_at, after_record.updated_at);
        assert!(weeks_equal(&before_weeks, &after_weeks));
        assert!(public_holidays_equal(&before_holidays, &after_holidays));
        assert_eq!(reloaded.snapshot_states.get(&record_id), Some(&state));
        assert!(save_current_row(&application, &schedule, &reloaded).is_err());
    }

    #[test]
    fn submitted_load_is_non_mutating_and_save_is_refused() {
        assert_protected_load_is_read_only(SnapshotState::Submitted);
    }

    #[test]
    fn indeterminate_load_is_non_mutating_and_save_is_refused() {
        assert_protected_load_is_read_only(SnapshotState::Indeterminate);
    }

    #[test]
    fn unchanged_candidate_survives_opening_and_remains_editable() {
        let (_directory, application, schedule, screen) = load_active_record();
        create_candidate(&application, &screen);
        let record_id = screen.weeks[0].0.id;
        let mut reloaded = PayrollTimesheetScreen::new();
        reloaded.load(&application, &schedule, "period").unwrap();
        assert_eq!(
            application
                .payroll_worked_item_repository
                .snapshot_metadata(record_id)
                .unwrap()
                .unwrap()
                .state,
            SnapshotState::Candidate
        );
        assert_eq!(
            reloaded.snapshot_states.get(&record_id),
            Some(&SnapshotState::Candidate)
        );
        assert!(
            !save_current_row(&application, &schedule, &reloaded)
                .unwrap()
                .changed
        );
    }

    #[test]
    fn unsent_record_remains_editable() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        screen.weeks[0].1[0].annual_leave_hours = 2.0;
        assert!(
            save_current_row(&application, &schedule, &screen)
                .unwrap()
                .changed
        );
    }

    #[test]
    fn materially_changed_reconciliation_invalidates_candidate_before_persistence() {
        let (_directory, application, schedule, screen) = load_active_record();
        create_candidate(&application, &screen);
        let record_id = screen.weeks[0].0.id;
        let connection = setup_connection(&application);
        connection.execute(
            "INSERT INTO personal_assistant_pay_rates (personal_assistant_id, effective_date, base_hourly_rate, employer_top_up_rate, created_at) VALUES (1, '01/01/2026', 12, 0, 'created')",
            [],
        ).unwrap();
        connection.execute(
            "INSERT INTO timesheets (pa_name, personal_assistant_id, start_time, end_time, break_minutes, worked_minutes, hourly_rate, amount, notes) VALUES ('Active Test', 1, '10/08/2026', '10/08/2026', 0, 60, 0, 0, NULL)",
            [],
        ).unwrap();

        let mut reloaded = PayrollTimesheetScreen::new();
        reloaded.load(&application, &schedule, "period").unwrap();
        assert!(application
            .payroll_worked_item_repository
            .snapshot_metadata(record_id)
            .unwrap()
            .is_none());
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(record_id)
                .unwrap()[0]
                .worked_hours,
            1.0
        );
    }

    #[test]
    fn failed_candidate_invalidation_prevents_load_and_save_mutations() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        create_candidate(&application, &screen);
        let record_id = screen.weeks[0].0.id;
        let connection = setup_connection(&application);
        connection.execute(
            "INSERT INTO personal_assistant_pay_rates (personal_assistant_id, effective_date, base_hourly_rate, employer_top_up_rate, created_at) VALUES (1, '01/01/2026', 12, 0, 'created')",
            [],
        ).unwrap();
        connection.execute(
            "INSERT INTO timesheets (pa_name, personal_assistant_id, start_time, end_time, break_minutes, worked_minutes, hourly_rate, amount, notes) VALUES ('Active Test', 1, '10/08/2026', '10/08/2026', 0, 60, 0, 0, NULL)",
            [],
        ).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER refuse_candidate_delete
             BEFORE DELETE ON payroll_timesheet_snapshot_states
             BEGIN SELECT RAISE(ABORT, 'candidate invalidation failed'); END;",
            )
            .unwrap();

        let mut reloaded = PayrollTimesheetScreen::new();
        assert!(reloaded.load(&application, &schedule, "period").is_err());
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(record_id)
                .unwrap()[0]
                .worked_hours,
            0.0
        );
        assert_eq!(
            application
                .payroll_worked_item_repository
                .snapshot_metadata(record_id)
                .unwrap()
                .unwrap()
                .state,
            SnapshotState::Candidate
        );

        screen.weeks[0].1[0].annual_leave_hours = 2.0;
        assert!(save_current_row(&application, &schedule, &screen).is_err());
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(record_id)
                .unwrap()[0]
                .annual_leave_hours,
            0.0
        );
    }

    #[test]
    fn stale_operational_selection_or_changed_schedule_refuses_save_without_mutation() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        screen.weeks[0].1[0].annual_leave_hours = 3.0;
        let other = PayrollSchedule {
            payroll_year: "2027/28".to_string(),
            cycle_number: 1,
            ..schedule.clone()
        };
        assert!(save_current_row(&application, &other, &screen).is_err());
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(screen.weeks[0].0.id)
                .unwrap()[0]
                .annual_leave_hours,
            0.0
        );

        setup_connection(&application)
            .execute(
                "UPDATE payroll_schedules SET pay_date = '05/09/2026' WHERE id = ?1",
                [schedule.id],
            )
            .unwrap();
        assert!(save_current_row(&application, &schedule, &screen).is_err());
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(screen.weeks[0].0.id)
                .unwrap()[0]
                .annual_leave_hours,
            0.0
        );

        setup_connection(&application)
            .execute("DELETE FROM payroll_schedules WHERE id = ?1", [schedule.id])
            .unwrap();
        assert!(save_current_row(&application, &schedule, &screen).is_err());
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(screen.weeks[0].0.id)
                .unwrap()[0]
                .annual_leave_hours,
            0.0
        );
    }

    macro_rules! candidate_save_invalidation_test {
        ($name:ident, $change:expr) => {
            #[test]
            fn $name() {
                let (_directory, application, schedule, mut screen) = load_active_record();
                create_candidate(&application, &screen);
                let record_id = screen.weeks[0].0.id;
                $change(&mut screen);
                let result = save_current_row(&application, &schedule, &screen).unwrap();
                assert!(result.changed);
                assert!(result.candidate_invalidated);
                assert!(application
                    .payroll_worked_item_repository
                    .snapshot_metadata(record_id)
                    .unwrap()
                    .is_none());
                let error = crate::payroll_snapshot_service::verify_candidate(
                    &application.payroll_worked_item_repository,
                    record_id,
                    std::path::Path::new("/tmp/stale-candidate.pdf"),
                )
                .unwrap_err();
                assert!(error.to_string().contains("No generated candidate"));
            }
        };
    }

    #[test]
    fn worked_hours_save_invalidates_candidate_and_persists_manual_adjustment() {
        let (_directory, application, schedule, mut screen) = load_active_record();
        create_candidate(&application, &screen);
        let record_id = screen.weeks[0].0.id;
        screen.weeks[0].1[0].worked_hours = 1.0;
        let result = save_current_row(&application, &schedule, &screen).unwrap();
        assert!(result.candidate_invalidated);
        assert_eq!(
            application
                .payroll_worked_item_repository
                .get_manual_adjustments(record_id)
                .unwrap()[0]
                .adjustment_minutes,
            60
        );
    }
    candidate_save_invalidation_test!(
        annual_leave_save_invalidates_candidate,
        |screen: &mut PayrollTimesheetScreen| {
            screen.weeks[0].1[0].annual_leave_hours = 1.0;
        }
    );
    candidate_save_invalidation_test!(
        sick_leave_save_invalidates_candidate,
        |screen: &mut PayrollTimesheetScreen| {
            screen.weeks[0].1[0].sick_leave_hours = 1.0;
        }
    );
    candidate_save_invalidation_test!(
        mileage_save_invalidates_candidate,
        |screen: &mut PayrollTimesheetScreen| {
            screen.weeks[0].1[0].travel_miles = 1.0;
        }
    );
    candidate_save_invalidation_test!(
        public_holiday_save_invalidates_candidate,
        |screen: &mut PayrollTimesheetScreen| {
            screen.public_holidays[0][0].hours = 1.0;
        }
    );

    fn load_week_with_eighteen_imported_hours() -> (
        TempDir,
        Application,
        PayrollSchedule,
        PayrollTimesheetScreen,
    ) {
        let (directory, application, schedule, _screen) = load_active_record();
        let connection = setup_connection(&application);
        connection
            .execute(
                "INSERT INTO personal_assistant_pay_rates (
                personal_assistant_id, effective_date, base_hourly_rate,
                employer_top_up_rate, created_at
             ) VALUES (1, '01/01/2026', 12, 0, 'created')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO timesheets (
                pa_name, personal_assistant_id, start_time, end_time,
                break_minutes, worked_minutes, hourly_rate, amount, notes
             ) VALUES ('Active Test', 1, '31/08/2026', '31/08/2026', 0, 1080, 0, 0, NULL)",
                [],
            )
            .unwrap();
        let mut screen = PayrollTimesheetScreen::new();
        screen.load(&application, &schedule, "period").unwrap();
        (directory, application, schedule, screen)
    }

    #[test]
    fn public_holiday_edits_leave_worked_hours_unchanged() {
        let (_directory, application, schedule, mut screen) =
            load_week_with_eighteen_imported_hours();
        let week_index = 3;
        screen.weeks[0].1[week_index].worked_hours = 8.0;
        screen.public_holidays[0][0].hours = 7.0;
        save_current_row(&application, &schedule, &screen).unwrap();

        let record_id = screen.weeks[0].0.id;
        let mut reloaded = PayrollTimesheetScreen::new();
        reloaded.load(&application, &schedule, "period").unwrap();
        reloaded.public_holidays[0][0].hours = 6.0;
        save_current_row(&application, &schedule, &reloaded).unwrap();
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(record_id)
                .unwrap()[week_index]
                .worked_hours,
            8.0
        );

        let mut reloaded = PayrollTimesheetScreen::new();
        reloaded.load(&application, &schedule, "period").unwrap();
        reloaded.public_holidays[0][0].hours = 8.0;
        save_current_row(&application, &schedule, &reloaded).unwrap();
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(record_id)
                .unwrap()[week_index]
                .worked_hours,
            8.0
        );
    }

    #[test]
    fn deliberate_worked_and_public_holiday_values_save_and_reload_independently() {
        let (_directory, application, schedule, mut screen) =
            load_week_with_eighteen_imported_hours();
        let week_index = 3;
        screen.weeks[0].1[week_index].worked_hours = 11.0;
        screen.public_holidays[0][0].hours = 7.0;
        save_current_row(&application, &schedule, &screen).unwrap();

        let record_id = screen.weeks[0].0.id;
        let stored_weeks = application
            .payroll_timesheet_repository
            .get_weeks(record_id)
            .unwrap();
        assert_eq!(stored_weeks[week_index].worked_hours, 11.0);
        assert_eq!(stored_weeks[week_index].public_holiday_hours, 7.0);
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_public_holidays(record_id)
                .unwrap()[0]
                .hours,
            7.0
        );
        assert_eq!(
            application
                .payroll_worked_item_repository
                .get_manual_adjustments(record_id)
                .unwrap()[0]
                .adjustment_minutes,
            -420
        );

        let mut reloaded = PayrollTimesheetScreen::new();
        reloaded.load(&application, &schedule, "period").unwrap();
        assert_eq!(reloaded.weeks[0].1[week_index].worked_hours, 11.0);
        assert_eq!(reloaded.public_holidays[0][0].hours, 7.0);
        assert!(
            !save_current_row(&application, &schedule, &reloaded)
                .unwrap()
                .changed
        );
    }

    #[test]
    fn failed_holiday_save_rolls_back_candidate_and_all_preparation_changes() {
        let (directory, application, schedule, mut screen) = load_active_record();
        let record_id = screen.weeks[0].0.id;
        let candidate_path = directory.path().join("candidate.pdf");
        std::fs::write(&candidate_path, b"candidate").unwrap();
        let digest = crate::payroll_snapshot_service::sha256_file(&candidate_path).unwrap();
        let week_ids = std::array::from_fn(|index| screen.weeks[0].1[index].id);
        application
            .payroll_worked_item_repository
            .replace_candidate(
                record_id,
                &[],
                candidate_path.to_string_lossy().as_ref(),
                &digest,
                "generated",
                0,
                &week_ids,
                &[0; 4],
            )
            .unwrap();

        screen.weeks[0].1[3].worked_hours = 1.0;
        screen.public_holidays[0][0].hours = 1.0;
        screen.public_holidays[0][0].id = -1;
        assert!(save_current_row(&application, &schedule, &screen).is_err());

        assert_eq!(
            application
                .payroll_worked_item_repository
                .snapshot_metadata(record_id)
                .unwrap()
                .unwrap()
                .state,
            SnapshotState::Candidate
        );
        crate::payroll_snapshot_service::verify_candidate(
            &application.payroll_worked_item_repository,
            record_id,
            &candidate_path,
        )
        .unwrap();
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(record_id)
                .unwrap()[3]
                .public_holiday_hours,
            0.0
        );
        assert!(application
            .payroll_worked_item_repository
            .get_manual_adjustments(record_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn invalidated_candidate_file_cannot_be_reused_even_if_it_still_exists() {
        let (directory, application, schedule, mut screen) =
            load_week_with_eighteen_imported_hours();
        let record_id = screen.weeks[0].0.id;
        let candidate_path = directory.path().join("stale-candidate.pdf");
        std::fs::write(&candidate_path, b"candidate").unwrap();
        let digest = crate::payroll_snapshot_service::sha256_file(&candidate_path).unwrap();
        let week_ids = std::array::from_fn(|index| screen.weeks[0].1[index].id);
        application
            .payroll_worked_item_repository
            .replace_candidate(
                record_id,
                &[],
                candidate_path.to_string_lossy().as_ref(),
                &digest,
                "generated",
                0,
                &week_ids,
                &[0, 0, 0, 1080],
            )
            .unwrap();

        screen.public_holidays[0][0].hours = 7.0;
        assert!(
            save_current_row(&application, &schedule, &screen)
                .unwrap()
                .candidate_invalidated
        );
        assert!(candidate_path.is_file());
        let error = crate::payroll_snapshot_service::verify_candidate(
            &application.payroll_worked_item_repository,
            record_id,
            &candidate_path,
        )
        .unwrap_err();
        assert!(error.to_string().contains("No generated candidate"));
    }

    #[test]
    fn two_holiday_entries_remain_independent_of_worked_hours() {
        let (_directory, application, schedule, mut screen) =
            load_week_with_eighteen_imported_hours();
        let record_id = screen.weeks[0].0.id;
        application
            .payroll_timesheet_repository
            .insert_public_holiday(record_id, 4, "01/09/2026")
            .unwrap();
        screen = PayrollTimesheetScreen::new();
        screen.load(&application, &schedule, "period").unwrap();

        screen.weeks[0].1[3].worked_hours = 8.0;
        screen.public_holidays[0][0].hours = 6.0;
        screen.public_holidays[0][1].hours = 4.0;
        save_current_row(&application, &schedule, &screen).unwrap();

        let weeks = application
            .payroll_timesheet_repository
            .get_weeks(record_id)
            .unwrap();
        let holidays = application
            .payroll_timesheet_repository
            .get_public_holidays(record_id)
            .unwrap();
        assert_eq!(weeks[3].worked_hours, 8.0);
        assert_eq!(weeks[3].public_holiday_hours, 10.0);
        assert_eq!(
            holidays
                .iter()
                .map(|holiday| holiday.hours)
                .collect::<Vec<_>>(),
            vec![6.0, 4.0]
        );
    }

    #[test]
    fn negative_and_non_finite_holiday_hours_are_rejected() {
        for invalid in [-1.0, f64::NAN, f64::INFINITY] {
            let (_directory, application, schedule, mut screen) = load_active_record();
            screen.public_holidays[0][0].hours = invalid;
            assert!(save_current_row(&application, &schedule, &screen).is_err());
        }
    }

    #[test]
    fn explicit_zero_holiday_round_trips_visibly_without_changing_worked_hours() {
        let (_directory, application, schedule, mut screen) =
            load_week_with_eighteen_imported_hours();
        let record_id = screen.weeks[0].0.id;
        let week_index = 3;
        screen.weeks[0].1[week_index].worked_hours = 8.0;
        screen.public_holidays[0][0].hours = 1.0;
        save_current_row(&application, &schedule, &screen).unwrap();

        let mut reloaded = PayrollTimesheetScreen::new();
        reloaded.load(&application, &schedule, "period").unwrap();
        reloaded.public_holidays[0][0].hours = 0.0;
        save_current_row(&application, &schedule, &reloaded).unwrap();

        let mut reloaded = PayrollTimesheetScreen::new();
        reloaded.load(&application, &schedule, "period").unwrap();
        assert_eq!(reloaded.weeks[0].1[week_index].worked_hours, 8.0);
        assert_eq!(reloaded.public_holidays[0][0].hours, 0.0);
        assert_eq!(numeric_editor_text(0.0), "0");
        assert_eq!(
            application
                .payroll_timesheet_repository
                .get_weeks(record_id)
                .unwrap()[week_index]
                .public_holiday_hours,
            0.0
        );
    }

    #[test]
    fn numeric_editor_full_selection_covers_the_existing_value() {
        for text in ["0", "7", "8", "26.5"] {
            assert_eq!(
                full_text_selection(text).as_sorted_char_range(),
                0..text.len()
            );
        }
    }

    #[test]
    fn numeric_editor_keeps_temporary_blank_text_until_replacement_or_commit() {
        for (existing, replacement) in [(6.0, "6"), (0.0, "5"), (26.5, "8.25")] {
            let mut value = existing;
            let mut text = format_decimal_hours(value);

            text.clear();
            update_numeric_value(&text, &mut value);
            assert!(text.is_empty());
            assert_eq!(value, existing);

            text.push_str(replacement);
            update_numeric_value(&text, &mut value);
            assert_eq!(value, replacement.parse::<f64>().unwrap());
            assert_eq!(text, replacement);
        }

        let mut value = 26.5;
        let mut text = String::new();
        commit_numeric_text(&mut text, &mut value);
        assert_eq!(value, 0.0);
        assert_eq!(text, "0");
    }
}

fn parse_date(value: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(value.trim(), "%d/%m/%Y")
        .or_else(|_| chrono::NaiveDate::parse_from_str(value.trim(), "%d-%m-%Y"))
        .or_else(|_| chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d"))
        .ok()
}

fn format_date(date: chrono::NaiveDate) -> String {
    date.format("%d/%m/%Y").to_string()
}

fn format_decimal_hours(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{:.0}", value)
    } else {
        format!("{:.2}", value)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}
