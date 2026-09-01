use rusqlite::{params, Connection, Result};

#[derive(Debug, Clone)]
pub struct PayrollTimesheetEmailStatus {
    pub sent_at: Option<String>,
    pub delivery_state: EmailDeliveryState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailDeliveryState {
    Unsent,
    Indeterminate { attempted_at: String },
    Sent { sent_at: String },
}

const INDETERMINATE_PREFIX: &str = "indeterminate:";

impl PayrollTimesheetEmailStatus {
    pub fn is_definitively_sent(&self) -> bool {
        matches!(self.delivery_state, EmailDeliveryState::Sent { .. })
    }
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
        email_type: &str,
    ) -> Result<Option<PayrollTimesheetEmailStatus>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                sent_at
            FROM payroll_timesheet_email_status
            WHERE personal_assistant_id = ?1
              AND payroll_year = ?2
              AND cycle_number = ?3
              AND email_type = ?4
            LIMIT 1
            ",
        )?;

        let mut rows = statement.query(params![
            personal_assistant_id,
            payroll_year,
            cycle_number,
            email_type
        ])?;

        if let Some(row) = rows.next()? {
            let sent_at: Option<String> = row.get(0)?;
            Ok(Some(PayrollTimesheetEmailStatus {
                delivery_state: delivery_state(sent_at.as_deref()),
                sent_at,
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
        email_type: &str,
    ) -> Result<()> {
        self.connection.execute(
            "
            INSERT OR IGNORE INTO payroll_timesheet_email_status (
                personal_assistant_id,
                payroll_year,
                cycle_number,
                email_type,
                sent_at
            )
            VALUES (?1, ?2, ?3, ?4, NULL)
            ",
            params![
                personal_assistant_id,
                payroll_year,
                cycle_number,
                email_type
            ],
        )?;

        Ok(())
    }

    #[allow(dead_code)]
    pub fn mark_sent(
        &self,
        personal_assistant_id: i64,
        payroll_year: &str,
        cycle_number: i64,
        email_type: &str,
        sent_at: &str,
    ) -> Result<()> {
        self.ensure_record(
            personal_assistant_id,
            payroll_year,
            cycle_number,
            email_type,
        )?;

        self.connection.execute(
            "
            UPDATE payroll_timesheet_email_status
            SET sent_at = ?1
            WHERE personal_assistant_id = ?2
              AND payroll_year = ?3
              AND cycle_number = ?4
              AND email_type = ?5
            ",
            params![
                sent_at,
                personal_assistant_id,
                payroll_year,
                cycle_number,
                email_type
            ],
        )?;

        Ok(())
    }

    pub fn protect_payslip_for_send(
        &self,
        personal_assistant_id: i64,
        payroll_year: &str,
        cycle_number: i64,
        attempted_at: &str,
    ) -> Result<bool> {
        self.ensure_record(personal_assistant_id, payroll_year, cycle_number, "payslip")?;
        let marker = indeterminate_marker(attempted_at);
        Ok(self.connection.execute(
            "UPDATE payroll_timesheet_email_status
             SET sent_at = ?1
             WHERE personal_assistant_id = ?2
               AND payroll_year = ?3
               AND cycle_number = ?4
               AND email_type = 'payslip'
               AND sent_at IS NULL",
            params![marker, personal_assistant_id, payroll_year, cycle_number],
        )? == 1)
    }

    pub fn restore_unsent_after_failed_payslip_send(
        &self,
        personal_assistant_id: i64,
        payroll_year: &str,
        cycle_number: i64,
        attempted_at: &str,
    ) -> Result<bool> {
        let marker = indeterminate_marker(attempted_at);
        Ok(self.connection.execute(
            "UPDATE payroll_timesheet_email_status
             SET sent_at = NULL
             WHERE personal_assistant_id = ?1
               AND payroll_year = ?2
               AND cycle_number = ?3
               AND email_type = 'payslip'
               AND sent_at = ?4",
            params![personal_assistant_id, payroll_year, cycle_number, marker],
        )? == 1)
    }

    pub fn mark_payslip_sent_from_indeterminate(
        &self,
        personal_assistant_id: i64,
        payroll_year: &str,
        cycle_number: i64,
        attempted_at: &str,
        sent_at: &str,
    ) -> Result<bool> {
        let marker = indeterminate_marker(attempted_at);
        Ok(self.connection.execute(
            "UPDATE payroll_timesheet_email_status
             SET sent_at = ?1
             WHERE personal_assistant_id = ?2
               AND payroll_year = ?3
               AND cycle_number = ?4
               AND email_type = 'payslip'
               AND sent_at = ?5",
            params![
                sent_at,
                personal_assistant_id,
                payroll_year,
                cycle_number,
                marker
            ],
        )? == 1)
    }
}

fn indeterminate_marker(attempted_at: &str) -> String {
    format!("{INDETERMINATE_PREFIX}{attempted_at}")
}

fn delivery_state(value: Option<&str>) -> EmailDeliveryState {
    match value {
        None => EmailDeliveryState::Unsent,
        Some(value) => match value.strip_prefix(INDETERMINATE_PREFIX) {
            Some(attempted_at) => EmailDeliveryState::Indeterminate {
                attempted_at: attempted_at.to_string(),
            },
            None => EmailDeliveryState::Sent {
                sent_at: value.to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;

    #[test]
    fn tracks_timesheet_and_payslip_sends_independently() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        let repository = PayrollTimesheetEmailRepository::new(connection);

        repository
            .mark_sent(1, "2026/27", 1, "timesheet", "2026-04-01T10:00:00Z")
            .unwrap();

        assert!(repository
            .get_for_pa_and_cycle(1, "2026/27", 1, "timesheet")
            .unwrap()
            .unwrap()
            .sent_at
            .is_some());
        assert!(repository
            .get_for_pa_and_cycle(1, "2026/27", 1, "payslip")
            .unwrap()
            .is_none());
    }
}
