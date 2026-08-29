use rusqlite::{params, Connection, Result};

#[derive(Debug, Clone)]
pub struct PayrollTimesheetEmailStatus {
    pub id: i64,
    pub personal_assistant_id: i64,
    pub payroll_year: String,
    pub cycle_number: i64,
    pub sent_at: Option<String>,
}

pub struct PayrollTimesheetEmailRepository {
    connection: Connection,
}

impl PayrollTimesheetEmailRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn get_for_pa_and_cycle(
        &self,
        personal_assistant_id: i64,
        payroll_year: &str,
        cycle_number: i64,
    ) -> Result<Option<PayrollTimesheetEmailStatus>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                personal_assistant_id,
                payroll_year,
                cycle_number,
                sent_at
            FROM payroll_timesheet_email_status
            WHERE personal_assistant_id = ?1
              AND payroll_year = ?2
              AND cycle_number = ?3
            LIMIT 1
            ",
        )?;

        let mut rows = statement.query(params![
            personal_assistant_id,
            payroll_year,
            cycle_number
        ])?;

        if let Some(row) = rows.next()? {
            Ok(Some(PayrollTimesheetEmailStatus {
                id: row.get(0)?,
                personal_assistant_id: row.get(1)?,
                payroll_year: row.get(2)?,
                cycle_number: row.get(3)?,
                sent_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn ensure_record(
        &self,
        personal_assistant_id: i64,
        payroll_year: &str,
        cycle_number: i64,
    ) -> Result<()> {
        self.connection.execute(
            "
            INSERT OR IGNORE INTO payroll_timesheet_email_status (
                personal_assistant_id,
                payroll_year,
                cycle_number,
                sent_at
            )
            VALUES (?1, ?2, ?3, NULL)
            ",
            params![personal_assistant_id, payroll_year, cycle_number],
        )?;

        Ok(())
    }

    pub fn mark_sent(
        &self,
        personal_assistant_id: i64,
        payroll_year: &str,
        cycle_number: i64,
        sent_at: &str,
    ) -> Result<()> {
        self.ensure_record(personal_assistant_id, payroll_year, cycle_number)?;

        self.connection.execute(
            "
            UPDATE payroll_timesheet_email_status
            SET sent_at = ?1
            WHERE personal_assistant_id = ?2
              AND payroll_year = ?3
              AND cycle_number = ?4
            ",
            params![
                sent_at,
                personal_assistant_id,
                payroll_year,
                cycle_number
            ],
        )?;

        Ok(())
    }

    pub fn get_all_for_cycle(
        &self,
        payroll_year: &str,
        cycle_number: i64,
    ) -> Result<Vec<PayrollTimesheetEmailStatus>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                personal_assistant_id,
                payroll_year,
                cycle_number,
                sent_at
            FROM payroll_timesheet_email_status
            WHERE payroll_year = ?1
              AND cycle_number = ?2
            ORDER BY personal_assistant_id
            ",
        )?;

        let rows = statement.query_map(params![payroll_year, cycle_number], |row| {
            Ok(PayrollTimesheetEmailStatus {
                id: row.get(0)?,
                personal_assistant_id: row.get(1)?,
                payroll_year: row.get(2)?,
                cycle_number: row.get(3)?,
                sent_at: row.get(4)?,
            })
        })?;

        rows.collect()
    }
}
