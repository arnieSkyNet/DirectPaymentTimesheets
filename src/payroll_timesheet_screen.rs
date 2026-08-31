use chrono::Datelike;
use eframe::egui;
use std::collections::HashMap;

use crate::app::Application;
use crate::payroll_schedule_repository::PayrollSchedule;
use crate::payroll_timesheet_repository::{
    PayrollTimesheet, PayrollTimesheetPublicHoliday, PayrollTimesheetWeek,
};

pub struct PayrollTimesheetScreen {
    loaded: bool,
    payroll_year: String,
    cycle_number: i64,
    schedule: Option<PayrollSchedule>,
    records: Vec<PayrollTimesheet>,
    weeks: Vec<(PayrollTimesheet, Vec<PayrollTimesheetWeek>, String)>,
    public_holidays: Vec<Vec<PayrollTimesheetPublicHoliday>>,
    worked_hours_baselines: HashMap<(i64, i64), i64>,
    status_message: String,
}

impl PayrollTimesheetScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,
            payroll_year: "2026/27".to_string(),
            cycle_number: 0,
            schedule: None,
            records: Vec::new(),
            weeks: Vec::new(),
            public_holidays: Vec::new(),
            worked_hours_baselines: HashMap::new(),
            status_message: "Payroll Timesheets not loaded.".to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application) {
        if !self.loaded {
            if let Err(error) = self.load(application) {
                self.status_message = format!("Failed loading Payroll Timesheets: {}", error);
            }

            self.loaded = true;
        }

        ui.heading("Payroll Timesheet Preparation");

        ui.separator();

        if let Some(schedule) = &self.schedule {
            ui.label(format!(
                "Payroll year: {}    Cycle: {}",
                schedule.payroll_year, schedule.cycle_number
            ));

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
            let (records, public_holidays) = (&mut self.weeks, &mut self.public_holidays);

            let (record, weeks, assistant_name) = &mut records[record_index];
            let holidays = &mut public_holidays[record_index];

            ui.heading(&*assistant_name);

            ui.horizontal(|ui| {
                ui.label("Previous cycle hours");

                let previous = record
                    .previous_cycle_hours
                    .map(|value| format_decimal_hours(value))
                    .unwrap_or_default();
                ui.label(previous);
            });

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

                        edit_number(ui, &mut week.worked_hours);
                        edit_number(ui, &mut week.annual_leave_hours);
                        edit_number(ui, &mut week.sick_leave_hours);

                        ui.vertical(|ui| {
                            for holiday in holidays
                                .iter_mut()
                                .filter(|holiday| holiday.week_number == week.week_number)
                            {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&holiday.holiday_date).size(8.0));

                                    edit_optional_number(ui, &mut holiday.hours);
                                });
                            }
                        });

                        edit_number(ui, &mut week.travel_miles);

                        ui.end_row();
                    }
                });

            if ui.button(format!("Save {}", assistant_name)).clicked() {
                let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

                let result = application
                    .payroll_timesheet_repository
                    .update_previous_cycle_hours(record.id, record.previous_cycle_hours, &now)
                    .and_then(|_| {
                        for week in weeks.iter() {
                            let final_minutes = (week.worked_hours * 60.0).round() as i64;
                            let baseline_minutes = self
                                .worked_hours_baselines
                                .get(&(record.id, week.week_number))
                                .copied()
                                .unwrap_or(final_minutes);
                            let existing_reason = application
                                .payroll_worked_item_repository
                                .get_manual_adjustments(record.id)?
                                .into_iter()
                                .find(|adjustment| adjustment.week_number == week.week_number)
                                .and_then(|adjustment| adjustment.reason);
                            application
                                .payroll_worked_item_repository
                                .set_manual_adjustment(
                                    record.id,
                                    &crate::payroll_worked_item_repository::ManualHoursAdjustment {
                                        week_number: week.week_number,
                                        adjustment_minutes: final_minutes - baseline_minutes,
                                        reason: existing_reason,
                                    },
                                    &now,
                                )?;
                            application.payroll_timesheet_repository.update_week(week)?;
                        }

                        for holiday in holidays.iter() {
                            application
                                .payroll_timesheet_repository
                                .update_public_holiday(holiday)?;
                        }

                        Ok(())
                    });

                match result {
                    Ok(()) => {
                        self.status_message =
                            format!("Payroll Timesheet saved for {}.", assistant_name);
                    }

                    Err(error) => {
                        self.status_message = format!("Save failed: {}", error);
                    }
                }
            }

            ui.separator();
        }

        ui.label(&self.status_message);
    }

    fn load(&mut self, application: &Application) -> Result<(), Box<dyn std::error::Error>> {
        let schedules = application.get_payroll_schedule(&self.payroll_year)?;

        let today = chrono::Local::now().date_naive();

        // Select the payroll cycle containing today.
        let schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(date, _)| *date)
            .map(|(_, schedule)| schedule)
            .ok_or("No current payroll cycle found.")?;

        self.cycle_number = schedule.cycle_number;
        self.schedule = Some(schedule.clone());

        let assistants = application.personal_assistant_repository.get_all()?;

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
        self.worked_hours_baselines.clear();

        for assistant in assistants {
            let is_active = match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            };

            if !is_active {
                continue;
            }

            let assistant_name = format!("{} {}", assistant.first_name, assistant.surname);

            let existing = application
                .payroll_timesheet_repository
                .get_for_cycle_and_pa(&self.payroll_year, schedule.cycle_number, assistant.id)?;

            let record = match existing {
                Some(record) => record,

                None => {
                    let actual_hours =
                        calculate_actual_hours(&all_timesheets, assistant.id, &week_dates);

                    let previous_cycle_number = schedule.cycle_number - 1;

                    let previous_cycle_hours = if previous_cycle_number > 0 {
                        calculate_previous_cycle_adjustment(
                            application,
                            &self.payroll_year,
                            previous_cycle_number,
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
                        &self.payroll_year,
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
                        .get_for_cycle_and_pa(
                            &self.payroll_year,
                            schedule.cycle_number,
                            assistant.id,
                        )?
                        .ok_or("Failed creating payroll timesheet.")?
                }
            };

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

            let previous_cycle_number = schedule.cycle_number - 1;
            let previous_record = if previous_cycle_number > 0 {
                application
                    .payroll_timesheet_repository
                    .get_for_cycle_and_pa(&self.payroll_year, previous_cycle_number, assistant.id)?
            } else {
                None
            };
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

            application
                .payroll_timesheet_repository
                .update_previous_cycle_hours(
                    record.id,
                    previous_cycle_hours,
                    &chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                )?;

            application
                .payroll_timesheet_repository
                .create_missing_weeks(record.id, &week_date_strings, &current_cycle_hours)?;

            let mut weeks = application
                .payroll_timesheet_repository
                .get_weeks(record.id)?;

            // Update only the four worked-hour values.
            for index in 0..4 {
                if let Some(week) = weeks.get_mut(index) {
                    week.worked_hours = current_cycle_hours[index];

                    application
                        .payroll_timesheet_repository
                        .update_week_worked_hours(week.id, week.worked_hours)?;
                }
            }

            let record = application
                .payroll_timesheet_repository
                .get_for_cycle_and_pa(&self.payroll_year, schedule.cycle_number, assistant.id)?
                .ok_or("Failed reloading payroll timesheet.")?;

            weeks = application
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

            self.records.push(record.clone());

            self.weeks.push((record, weeks, assistant_name));

            self.public_holidays.push(holidays);
        }

        self.status_message = format!("Loaded {} Payroll Timesheets.", self.weeks.len());

        Ok(())
    }
}

fn edit_number(ui: &mut egui::Ui, value: &mut f64) {
    let mut text = if *value == 0.0 {
        "0".to_string()
    } else {
        format_decimal_hours(*value)
    };

    if ui.text_edit_singleline(&mut text).changed() {
        if let Ok(parsed) = text.trim().parse::<f64>() {
            *value = parsed;
        }
    }
}

fn edit_optional_number(ui: &mut egui::Ui, value: &mut f64) {
    let mut text = if *value == 0.0 {
        String::new()
    } else {
        format_decimal_hours(*value)
    };

    if ui.text_edit_singleline(&mut text).changed() {
        if text.trim().is_empty() {
            *value = 0.0;
        } else if let Ok(parsed) = text.trim().parse::<f64>() {
            *value = parsed;
        }
    }
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
    payroll_year: &str,
    previous_cycle_number: i64,
    personal_assistant_id: i64,
    timesheets: &[crate::models::TimesheetEntry],
    current_cycle_first_week: chrono::NaiveDate,
) -> Result<Option<f64>, Box<dyn std::error::Error>> {
    let previous_record = application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa(payroll_year, previous_cycle_number, personal_assistant_id)?;

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
