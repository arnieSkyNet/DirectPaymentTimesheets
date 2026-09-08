use rusqlite::{params, Connection, Result};

use crate::payroll_worked_item_repository::ManualHoursAdjustment;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PayrollTimesheet {
    pub id: i64,
    pub personal_assistant_id: i64,
    pub payroll_year: String,
    pub cycle_number: i64,
    pub previous_cycle_hours: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PayrollTimesheetWeek {
    pub id: i64,
    pub payroll_timesheet_id: i64,
    pub week_number: i64,
    pub week_commencing: String,
    pub worked_hours: f64,
    pub annual_leave_hours: f64,
    pub sick_leave_hours: f64,
    pub public_holiday_hours: f64,
    pub travel_miles: f64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PayrollTimesheetPublicHoliday {
    pub id: i64,
    pub payroll_timesheet_id: i64,
    pub week_number: i64,
    pub holiday_date: String,
    pub hours: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PayrollTimesheetAnnualLeave {
    pub id: i64,
    pub payroll_timesheet_id: i64,
    pub week_number: i64,
    pub leave_date: String,
    pub hours: f64,
    pub created_at: String,
    pub updated_at: String,
}

pub fn parse_leave_date(value: &str) -> Option<chrono::NaiveDate> {
    ["%d/%m/%y", "%d-%m-%y", "%d/%m/%Y", "%d-%m-%Y", "%Y-%m-%d"]
        .iter()
        .find_map(|format| chrono::NaiveDate::parse_from_str(value.trim(), format).ok())
}

// Canonical UK dates match the existing preparation/public-holiday convention.
// Existing detail rows also mark a week as dated when its last row is removed.
pub fn derive_annual_leave(
    record_id: i64,
    weeks: &mut [PayrollTimesheetWeek],
    rows: &[PayrollTimesheetAnnualLeave],
    previous: &[PayrollTimesheetAnnualLeave],
) -> Result<Vec<PayrollTimesheetAnnualLeave>> {
    let invalid = |message: &str| rusqlite::Error::InvalidParameterName(message.into());
    let mut seen = std::collections::HashSet::new();
    let mut normalised = rows.to_vec();
    for row in &mut normalised {
        if row.payroll_timesheet_id != record_id {
            return Err(invalid(
                "Annual leave belongs to another payroll timesheet.",
            ));
        }
        let week = weeks
            .iter()
            .find(|week| {
                week.week_number == row.week_number && week.payroll_timesheet_id == record_id
            })
            .ok_or_else(|| invalid("Annual leave has an invalid payroll week."))?;
        if !(1..=4).contains(&row.week_number) {
            return Err(invalid("Annual leave week must be 1 to 4."));
        }
        let start = parse_leave_date(&week.week_commencing)
            .ok_or_else(|| invalid("Invalid payroll week date."))?;
        let date = parse_leave_date(&row.leave_date)
            .ok_or_else(|| invalid(&format!("Enter a valid annual-leave date (DD/MM/YYYY); received '{}' for week commencing {}.", row.leave_date.trim(), start.format("%d/%m/%Y"))))?;
        if date < start || date.signed_duration_since(start).num_days() >= 7 {
            return Err(invalid(&format!(
                "Annual-leave date {} is outside the week commencing {}.",
                row.leave_date.trim(),
                start.format("%d/%m/%Y")
            )));
        }
        if !row.hours.is_finite() || row.hours < 0.0 {
            return Err(invalid(
                "Annual-leave hours must be finite and non-negative.",
            ));
        }
        if !seen.insert((row.week_number, date)) {
            return Err(invalid("Duplicate annual-leave date in the same week."));
        }
        row.leave_date = date.format("%d/%m/%Y").to_string();
    }
    for week in weeks {
        if rows
            .iter()
            .chain(previous)
            .any(|row| row.week_number == week.week_number)
        {
            let total: f64 = rows
                .iter()
                .filter(|row| row.week_number == week.week_number)
                .map(|row| row.hours)
                .sum();
            if !total.is_finite() {
                return Err(invalid("Annual-leave total is not finite."));
            }
            week.annual_leave_hours = total;
        }
    }
    normalised.sort_by_key(|row| (row.week_number, parse_leave_date(&row.leave_date)));
    Ok(normalised)
}

pub struct PayrollTimesheetRepository {
    connection: Connection,
}

impl PayrollTimesheetRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn get_for_cycle_and_pa(
        &self,
        payroll_year: &str,
        cycle_number: i64,
        personal_assistant_id: i64,
    ) -> Result<Option<PayrollTimesheet>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                personal_assistant_id,
                payroll_year,
                cycle_number,
                previous_cycle_hours,
                created_at,
                updated_at
            FROM payroll_timesheets
            WHERE payroll_year = ?1
              AND cycle_number = ?2
              AND personal_assistant_id = ?3
            LIMIT 1
            ",
        )?;

        let mut rows =
            statement.query(params![payroll_year, cycle_number, personal_assistant_id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(PayrollTimesheet {
                id: row.get(0)?,
                personal_assistant_id: row.get(1)?,
                payroll_year: row.get(2)?,
                cycle_number: row.get(3)?,
                previous_cycle_hours: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_for_personal_assistant(
        &self,
        personal_assistant_id: i64,
    ) -> Result<Vec<PayrollTimesheet>> {
        let mut statement = self.connection.prepare(
            "SELECT id, personal_assistant_id, payroll_year, cycle_number, previous_cycle_hours, created_at, updated_at
             FROM payroll_timesheets WHERE personal_assistant_id = ?1 ORDER BY payroll_year, cycle_number, id"
        )?;
        let rows = statement
            .query_map([personal_assistant_id], |row| {
                Ok(PayrollTimesheet {
                    id: row.get(0)?,
                    personal_assistant_id: row.get(1)?,
                    payroll_year: row.get(2)?,
                    cycle_number: row.get(3)?,
                    previous_cycle_hours: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect();
        rows
    }

    pub fn get_all_for_cycle(
        &self,
        payroll_year: &str,
        cycle_number: i64,
    ) -> Result<Vec<PayrollTimesheet>> {
        let mut statement = self.connection.prepare(
            "SELECT id, personal_assistant_id, payroll_year, cycle_number,
                    previous_cycle_hours, created_at, updated_at
             FROM payroll_timesheets
             WHERE payroll_year = ?1 AND cycle_number = ?2
             ORDER BY personal_assistant_id",
        )?;
        let records = statement
            .query_map(params![payroll_year, cycle_number], |row| {
                Ok(PayrollTimesheet {
                    id: row.get(0)?,
                    personal_assistant_id: row.get(1)?,
                    payroll_year: row.get(2)?,
                    cycle_number: row.get(3)?,
                    previous_cycle_hours: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect();
        records
    }

    pub fn insert(
        &self,
        payroll_year: &str,
        cycle_number: i64,
        personal_assistant_id: i64,
        previous_cycle_hours: Option<f64>,
        created_at: &str,
    ) -> Result<i64> {
        self.connection.execute(
            "
            INSERT INTO payroll_timesheets (
                personal_assistant_id,
                payroll_year,
                cycle_number,
                previous_cycle_hours,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            ",
            params![
                personal_assistant_id,
                payroll_year,
                cycle_number,
                previous_cycle_hours,
                created_at
            ],
        )?;

        Ok(self.connection.last_insert_rowid())
    }

    pub fn get_weeks(&self, payroll_timesheet_id: i64) -> Result<Vec<PayrollTimesheetWeek>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                payroll_timesheet_id,
                week_number,
                week_commencing,
                worked_hours,
                annual_leave_hours,
                sick_leave_hours,
                public_holiday_hours,
                travel_miles
            FROM payroll_timesheet_weeks
            WHERE payroll_timesheet_id = ?1
            ORDER BY week_number
            ",
        )?;

        let rows = statement.query_map(params![payroll_timesheet_id], |row| {
            Ok(PayrollTimesheetWeek {
                id: row.get(0)?,
                payroll_timesheet_id: row.get(1)?,
                week_number: row.get(2)?,
                week_commencing: row.get(3)?,
                worked_hours: row.get(4)?,
                annual_leave_hours: row.get(5)?,
                sick_leave_hours: row.get(6)?,
                public_holiday_hours: row.get(7)?,
                travel_miles: row.get(8)?,
            })
        })?;

        rows.collect()
    }

    pub fn insert_week(
        &self,
        payroll_timesheet_id: i64,
        week_number: i64,
        week_commencing: &str,
        worked_hours: f64,
    ) -> Result<i64> {
        self.connection.execute(
            "
            INSERT INTO payroll_timesheet_weeks (
                payroll_timesheet_id,
                week_number,
                week_commencing,
                worked_hours,
                annual_leave_hours,
                sick_leave_hours,
                public_holiday_hours,
                travel_miles
            )
            VALUES (?1, ?2, ?3, ?4, 0, 0, 0, 0)
            ",
            params![
                payroll_timesheet_id,
                week_number,
                week_commencing,
                worked_hours
            ],
        )?;

        Ok(self.connection.last_insert_rowid())
    }

    pub fn create_missing_weeks(
        &self,
        payroll_timesheet_id: i64,
        week_dates: &[String; 4],
        actual_hours: &[f64; 4],
    ) -> Result<()> {
        let existing = self.get_weeks(payroll_timesheet_id)?;

        for index in 0..4 {
            let week_number = (index + 1) as i64;

            if !existing.iter().any(|week| week.week_number == week_number) {
                self.insert_week(
                    payroll_timesheet_id,
                    week_number,
                    &week_dates[index],
                    actual_hours[index],
                )?;
            }
        }

        Ok(())
    }

    pub fn reconcile_editable_hours_atomically(
        &self,
        payroll_timesheet_id: i64,
        previous_cycle_hours: Option<f64>,
        week_dates: &[String; 4],
        worked_hours: &[f64; 4],
        updated_at: &str,
        invalidate_candidate: bool,
    ) -> Result<bool> {
        use rusqlite::OptionalExtension;

        let transaction = self.connection.unchecked_transaction()?;
        let state: Option<String> = transaction
            .query_row(
                "SELECT state FROM payroll_timesheet_snapshot_states
                 WHERE payroll_timesheet_id = ?1",
                [payroll_timesheet_id],
                |row| row.get(0),
            )
            .optional()?;
        if matches!(state.as_deref(), Some("submitted" | "indeterminate")) {
            return Err(rusqlite::Error::InvalidParameterName(
                "Submitted or indeterminate payroll cannot be reconciled.".to_string(),
            ));
        }
        let candidate_invalidated = invalidate_candidate && state.as_deref() == Some("candidate");
        if candidate_invalidated {
            transaction.execute(
                "DELETE FROM payroll_timesheet_snapshot_states
                 WHERE payroll_timesheet_id = ?1 AND state = 'candidate'",
                [payroll_timesheet_id],
            )?;
            transaction.execute(
                "DELETE FROM payroll_timesheet_worked_item_snapshots
                 WHERE payroll_timesheet_id = ?1",
                [payroll_timesheet_id],
            )?;
        }
        transaction.execute(
            "UPDATE payroll_timesheets
             SET previous_cycle_hours = ?1, updated_at = ?2 WHERE id = ?3",
            params![previous_cycle_hours, updated_at, payroll_timesheet_id],
        )?;
        for index in 0..4 {
            transaction.execute(
                "INSERT INTO payroll_timesheet_weeks (
                    payroll_timesheet_id, week_number, week_commencing, worked_hours,
                    annual_leave_hours, sick_leave_hours, public_holiday_hours, travel_miles
                 ) VALUES (?1, ?2, ?3, ?4, 0, 0, 0, 0)
                 ON CONFLICT(payroll_timesheet_id, week_number) DO UPDATE SET
                    worked_hours = excluded.worked_hours",
                params![
                    payroll_timesheet_id,
                    (index + 1) as i64,
                    week_dates[index],
                    worked_hours[index]
                ],
            )?;
        }
        transaction.commit()?;
        Ok(candidate_invalidated)
    }

    pub fn get_public_holidays(
        &self,
        payroll_timesheet_id: i64,
    ) -> Result<Vec<PayrollTimesheetPublicHoliday>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                payroll_timesheet_id,
                week_number,
                holiday_date,
                hours
            FROM payroll_timesheet_public_holidays
            WHERE payroll_timesheet_id = ?1
            ORDER BY week_number, holiday_date
            ",
        )?;

        let rows = statement.query_map(params![payroll_timesheet_id], |row| {
            Ok(PayrollTimesheetPublicHoliday {
                id: row.get(0)?,
                payroll_timesheet_id: row.get(1)?,
                week_number: row.get(2)?,
                holiday_date: row.get(3)?,
                hours: row.get(4)?,
            })
        })?;

        rows.collect()
    }

    pub fn insert_public_holiday(
        &self,
        payroll_timesheet_id: i64,
        week_number: i64,
        holiday_date: &str,
    ) -> Result<i64> {
        self.connection.execute(
            "
            INSERT INTO payroll_timesheet_public_holidays (
                payroll_timesheet_id,
                week_number,
                holiday_date,
                hours
            )
            VALUES (?1, ?2, ?3, 0)
            ",
            params![payroll_timesheet_id, week_number, holiday_date],
        )?;

        Ok(self.connection.last_insert_rowid())
    }

    pub fn get_annual_leave(
        &self,
        payroll_timesheet_id: i64,
    ) -> Result<Vec<PayrollTimesheetAnnualLeave>> {
        let mut statement = self.connection.prepare(
            "SELECT id, payroll_timesheet_id, week_number, leave_date, hours, created_at, updated_at
             FROM payroll_timesheet_annual_leave WHERE payroll_timesheet_id = ?1
             ORDER BY week_number, substr(leave_date, 7, 4), substr(leave_date, 4, 2), substr(leave_date, 1, 2), id"
        )?;
        let rows = statement
            .query_map([payroll_timesheet_id], |row| {
                Ok(PayrollTimesheetAnnualLeave {
                    id: row.get(0)?,
                    payroll_timesheet_id: row.get(1)?,
                    week_number: row.get(2)?,
                    leave_date: row.get(3)?,
                    hours: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect();
        rows
    }

    pub fn save_preparation_atomically(
        &self,
        record: &PayrollTimesheet,
        weeks: &[PayrollTimesheetWeek],
        holidays: &[PayrollTimesheetPublicHoliday],
        annual_leave: &[PayrollTimesheetAnnualLeave],
        adjustments: &[ManualHoursAdjustment],
        updated_at: &str,
    ) -> Result<bool> {
        use rusqlite::OptionalExtension;

        if weeks.len() != 4 || adjustments.len() != weeks.len() {
            return Err(rusqlite::Error::InvalidParameterName(
                "Preparation save requires exactly four matched payroll weeks and adjustments."
                    .to_string(),
            ));
        }

        let transaction = self.connection.unchecked_transaction()?;
        let state: Option<String> = transaction
            .query_row(
                "SELECT state FROM payroll_timesheet_snapshot_states
                 WHERE payroll_timesheet_id = ?1",
                [record.id],
                |row| row.get(0),
            )
            .optional()?;
        if matches!(state.as_deref(), Some("submitted" | "indeterminate")) {
            return Err(rusqlite::Error::InvalidParameterName(
                "This payroll timesheet is submitted or indeterminate and is read-only."
                    .to_string(),
            ));
        }

        let stored_weeks = self.get_weeks(record.id)?;
        let previous_leave = self.get_annual_leave(record.id)?;
        if stored_weeks.len() != 4
            || weeks.iter().any(|week| {
                !stored_weeks.iter().any(|stored| {
                    stored.id == week.id
                        && stored.week_number == week.week_number
                        && stored.week_commencing == week.week_commencing
                })
            })
            || weeks
                .iter()
                .map(|week| week.week_number)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != 4
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "Preparation weeks do not match stored payroll weeks.".into(),
            ));
        }
        let mut weeks = weeks.to_vec();
        let annual_leave =
            derive_annual_leave(record.id, &mut weeks, annual_leave, &previous_leave)?;

        let candidate_invalidated = state.as_deref() == Some("candidate");
        if candidate_invalidated {
            transaction.execute(
                "DELETE FROM payroll_timesheet_snapshot_states
                 WHERE payroll_timesheet_id = ?1 AND state = 'candidate'",
                [record.id],
            )?;
            transaction.execute(
                "DELETE FROM payroll_timesheet_worked_item_snapshots
                 WHERE payroll_timesheet_id = ?1",
                [record.id],
            )?;
        }

        transaction.execute(
            "UPDATE payroll_timesheets
             SET previous_cycle_hours = ?1, updated_at = ?2
             WHERE id = ?3",
            params![record.previous_cycle_hours, updated_at, record.id],
        )?;

        for (week, adjustment) in weeks.iter().zip(adjustments) {
            if week.payroll_timesheet_id != record.id || adjustment.week_number != week.week_number
            {
                return Err(rusqlite::Error::InvalidParameterName(
                    "Preparation week does not belong to the selected payroll timesheet."
                        .to_string(),
                ));
            }
            if adjustment.adjustment_minutes == 0 && adjustment.reason.is_none() {
                transaction.execute(
                    "DELETE FROM payroll_timesheet_manual_adjustments
                     WHERE payroll_timesheet_id = ?1 AND week_number = ?2",
                    params![record.id, week.week_number],
                )?;
            } else {
                transaction.execute(
                    "INSERT INTO payroll_timesheet_manual_adjustments (
                        payroll_timesheet_id, week_number, adjustment_minutes, reason, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(payroll_timesheet_id, week_number) DO UPDATE SET
                        adjustment_minutes = excluded.adjustment_minutes,
                        reason = excluded.reason,
                        updated_at = excluded.updated_at",
                    params![
                        record.id,
                        adjustment.week_number,
                        adjustment.adjustment_minutes,
                        adjustment.reason,
                        updated_at
                    ],
                )?;
            }
            let changed = transaction.execute(
                "UPDATE payroll_timesheet_weeks SET
                    worked_hours = ?1,
                    annual_leave_hours = ?2,
                    sick_leave_hours = ?3,
                    public_holiday_hours = ?4,
                    travel_miles = ?5
                 WHERE id = ?6 AND payroll_timesheet_id = ?7",
                params![
                    week.worked_hours,
                    week.annual_leave_hours,
                    week.sick_leave_hours,
                    week.public_holiday_hours,
                    week.travel_miles,
                    week.id,
                    record.id
                ],
            )?;
            if changed != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }

        for holiday in holidays {
            if holiday.payroll_timesheet_id != record.id {
                return Err(rusqlite::Error::InvalidParameterName(
                    "Public-holiday row does not belong to the selected payroll timesheet."
                        .to_string(),
                ));
            }
            let changed = transaction.execute(
                "UPDATE payroll_timesheet_public_holidays SET hours = ?1
                 WHERE id = ?2 AND payroll_timesheet_id = ?3",
                params![holiday.hours, holiday.id, record.id],
            )?;
            if changed != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }

        // Detail persistence shares the weekly update and invalidation transaction.
        for old in &previous_leave {
            if !annual_leave
                .iter()
                .any(|row| row.week_number == old.week_number && row.leave_date == old.leave_date)
            {
                transaction.execute(
                    "DELETE FROM payroll_timesheet_annual_leave WHERE id = ?1",
                    [old.id],
                )?;
            }
        }
        for row in &annual_leave {
            transaction.execute(
                "INSERT INTO payroll_timesheet_annual_leave
                 (payroll_timesheet_id, week_number, leave_date, hours, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(payroll_timesheet_id, week_number, leave_date) DO UPDATE SET
                    hours = excluded.hours, updated_at = excluded.updated_at",
                params![
                    record.id,
                    row.week_number,
                    row.leave_date,
                    row.hours,
                    updated_at
                ],
            )?;
        }

        transaction.commit()?;
        Ok(candidate_invalidated)
    }

    pub fn create_missing_public_holidays(
        &self,
        payroll_timesheet_id: i64,
        holidays: &[(i64, String)],
    ) -> Result<()> {
        let existing = self.get_public_holidays(payroll_timesheet_id)?;

        for (week_number, holiday_date) in holidays {
            let exists = existing.iter().any(|holiday| {
                holiday.week_number == *week_number && holiday.holiday_date == *holiday_date
            });

            if !exists {
                self.insert_public_holiday(payroll_timesheet_id, *week_number, holiday_date)?;
            }
        }

        Ok(())
    }
}
