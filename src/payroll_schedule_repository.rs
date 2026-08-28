use rusqlite::{params, Connection, Result};

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

    pub fn delete_all_for_year(&self, payroll_year: &str) -> Result<()> {
        self.connection.execute(
            "
            DELETE FROM payroll_schedules
            WHERE payroll_year = ?1
            ",
            params![payroll_year],
        )?;

        Ok(())
    }

    pub fn insert(&self, schedule: &PayrollSchedule) -> Result<()> {
        self.connection.execute(
            "
            INSERT INTO payroll_schedules (
                payroll_year,
                cycle_number,
                first_week_commencing,
                latest_posting_date,
                pay_date,
                created_at,
                payslips_sent
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                schedule.payroll_year,
                schedule.cycle_number,
                schedule.first_week_commencing,
                schedule.latest_posting_date,
                schedule.pay_date,
                schedule.created_at,
                schedule.payslips_sent,
            ],
        )?;

        Ok(())
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
