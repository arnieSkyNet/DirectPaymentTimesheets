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

    pub fn save_preparation_atomically(
        &self,
        record: &PayrollTimesheet,
        weeks: &[PayrollTimesheetWeek],
        holidays: &[PayrollTimesheetPublicHoliday],
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
