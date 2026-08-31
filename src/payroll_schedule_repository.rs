use rusqlite::{params, Connection, Result};
use std::error::Error;

#[derive(Debug, Clone)]
pub struct PayrollSchedule {
    pub id: i64,
    pub payroll_year: String,
    pub cycle_number: i64,
    pub first_week_commencing: String,
    pub latest_posting_date: String,
    pub pay_date: String,
    pub created_at: String,
    pub payslips_sent: bool,
}

pub struct PayrollScheduleRepository {
    connection: Connection,
}

impl PayrollScheduleRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn get_all_for_year(&self, payroll_year: &str) -> Result<Vec<PayrollSchedule>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                payroll_year,
                cycle_number,
                first_week_commencing,
                latest_posting_date,
                pay_date,
                created_at,
                payslips_sent
            FROM payroll_schedules
            WHERE payroll_year = ?1
            ORDER BY cycle_number
            ",
        )?;

        let entries = statement.query_map(params![payroll_year], |row| {
            Ok(PayrollSchedule {
                id: row.get(0)?,
                payroll_year: row.get(1)?,
                cycle_number: row.get(2)?,
                first_week_commencing: row.get(3)?,
                latest_posting_date: row.get(4)?,
                pay_date: row.get(5)?,
                created_at: row.get(6)?,
                payslips_sent: row.get::<_, i64>(7)? != 0,
            })
        })?;

        entries.collect()
    }

    pub fn replace_year_atomically(
        &self,
        payroll_year: &str,
        schedules: &[PayrollSchedule],
    ) -> std::result::Result<usize, Box<dyn Error>> {
        let transaction = self.connection.unchecked_transaction()?;
        let existing = {
            let mut statement = transaction.prepare(
                "SELECT id, payroll_year, cycle_number, first_week_commencing,
                        latest_posting_date, pay_date, created_at, payslips_sent
                 FROM payroll_schedules
                 WHERE payroll_year = ?1
                 ORDER BY cycle_number",
            )?;
            let rows = statement.query_map(params![payroll_year], |row| {
                Ok(PayrollSchedule {
                    id: row.get(0)?,
                    payroll_year: row.get(1)?,
                    cycle_number: row.get(2)?,
                    first_week_commencing: row.get(3)?,
                    latest_posting_date: row.get(4)?,
                    pay_date: row.get(5)?,
                    created_at: row.get(6)?,
                    payslips_sent: row.get::<_, i64>(7)? != 0,
                })
            })?;
            rows.collect::<Result<Vec<_>>>()?
        };

        let dates_changed = existing.len() != schedules.len()
            || schedules.iter().any(|schedule| {
                existing
                    .iter()
                    .find(|current| current.cycle_number == schedule.cycle_number)
                    .is_none_or(|current| !same_schedule_dates(current, schedule))
            });

        if dates_changed {
            let prepared_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM payroll_timesheets WHERE payroll_year = ?1",
                params![payroll_year],
                |row| row.get(0),
            )?;
            if prepared_count > 0 {
                return Err(format!(
                    "Cannot replace the {payroll_year} payroll schedule because its dates differ and {prepared_count} prepared payroll record(s) already exist."
                )
                .into());
            }
        }

        transaction.execute(
            "DELETE FROM payroll_schedules WHERE payroll_year = ?1",
            params![payroll_year],
        )?;

        for schedule in schedules {
            let unchanged = existing.iter().find(|current| {
                current.cycle_number == schedule.cycle_number
                    && same_schedule_dates(current, schedule)
            });
            let payslips_sent = unchanged
                .map(|current| current.payslips_sent)
                .unwrap_or(false);
            let created_at = unchanged
                .map(|current| current.created_at.as_str())
                .unwrap_or(&schedule.created_at);

            transaction.execute(
                "INSERT INTO payroll_schedules (
                    payroll_year, cycle_number, first_week_commencing,
                    latest_posting_date, pay_date, created_at, payslips_sent
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    schedule.payroll_year,
                    schedule.cycle_number,
                    schedule.first_week_commencing,
                    schedule.latest_posting_date,
                    schedule.pay_date,
                    created_at,
                    payslips_sent,
                ],
            )?;
        }

        transaction.commit()?;
        Ok(schedules.len())
    }

    pub fn mark_payslips_sent(&self, id: i64) -> Result<()> {
        self.connection.execute(
            "
            UPDATE payroll_schedules
            SET payslips_sent = 1
            WHERE id = ?1
            ",
            params![id],
        )?;

        Ok(())
    }
}

fn same_schedule_dates(left: &PayrollSchedule, right: &PayrollSchedule) -> bool {
    left.first_week_commencing == right.first_week_commencing
        && left.latest_posting_date == right.latest_posting_date
        && left.pay_date == right.pay_date
}
