use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualHoursAdjustment {
    pub week_number: i64,
    pub adjustment_minutes: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkedItemSnapshot {
    pub week_number: i64,
    pub source_type: String,
    pub timesheet_id: Option<i64>,
    pub direct_shift_id: Option<i64>,
    pub source_evidence: Option<String>,
    pub work_date: Option<String>,
    pub worked_minutes: i64,
    pub pay_rate_id: Option<i64>,
    pub pay_rate_effective_date: Option<String>,
    pub total_hourly_rate: Option<f64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotState {
    Candidate,
    Submitted,
    Indeterminate,
}

impl SnapshotState {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "candidate" => Some(Self::Candidate),
            "submitted" => Some(Self::Submitted),
            "indeterminate" => Some(Self::Indeterminate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotMetadata {
    pub state: SnapshotState,
    pub pdf_path: String,
    pub pdf_sha256: String,
}

pub struct PayrollWorkedItemRepository {
    connection: Connection,
}

impl PayrollWorkedItemRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn get_manual_adjustments(
        &self,
        payroll_timesheet_id: i64,
    ) -> Result<Vec<ManualHoursAdjustment>> {
        let mut statement = self.connection.prepare(
            "SELECT week_number, adjustment_minutes, reason
             FROM payroll_timesheet_manual_adjustments
             WHERE payroll_timesheet_id = ?1
             ORDER BY week_number",
        )?;
        let adjustments = statement
            .query_map([payroll_timesheet_id], |row| {
                Ok(ManualHoursAdjustment {
                    week_number: row.get(0)?,
                    adjustment_minutes: row.get(1)?,
                    reason: row.get(2)?,
                })
            })?
            .collect();
        adjustments
    }

    #[cfg(test)]
    pub fn set_manual_adjustment(
        &self,
        payroll_timesheet_id: i64,
        adjustment: &ManualHoursAdjustment,
        updated_at: &str,
    ) -> Result<()> {
        if adjustment.adjustment_minutes == 0 && adjustment.reason.is_none() {
            self.connection.execute(
                "DELETE FROM payroll_timesheet_manual_adjustments
                 WHERE payroll_timesheet_id = ?1 AND week_number = ?2",
                params![payroll_timesheet_id, adjustment.week_number],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO payroll_timesheet_manual_adjustments (
                    payroll_timesheet_id, week_number, adjustment_minutes, reason, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(payroll_timesheet_id, week_number) DO UPDATE SET
                    adjustment_minutes = excluded.adjustment_minutes,
                    reason = excluded.reason,
                    updated_at = excluded.updated_at",
                params![
                    payroll_timesheet_id,
                    adjustment.week_number,
                    adjustment.adjustment_minutes,
                    adjustment.reason,
                    updated_at
                ],
            )?;
        }
        Ok(())
    }

    pub fn submitted_snapshot_timesheet_ids(
        &self,
        payroll_timesheet_id: i64,
    ) -> Result<HashSet<i64>> {
        let mut statement = self.connection.prepare(
            "SELECT timesheet_id
             FROM payroll_timesheet_worked_item_snapshots
             WHERE payroll_timesheet_id = ?1
               AND timesheet_id IS NOT NULL
               AND EXISTS (
                   SELECT 1 FROM payroll_timesheet_snapshot_states
                   WHERE payroll_timesheet_id = ?1 AND state = 'submitted'
               )",
        )?;
        let ids = statement
            .query_map([payroll_timesheet_id], |row| row.get(0))?
            .collect::<Result<HashSet<_>>>()?;
        Ok(ids)
    }

    pub fn snapshot_metadata(&self, payroll_timesheet_id: i64) -> Result<Option<SnapshotMetadata>> {
        use rusqlite::OptionalExtension;
        self.connection
            .query_row(
                "SELECT state, pdf_path, pdf_sha256
                 FROM payroll_timesheet_snapshot_states
                 WHERE payroll_timesheet_id = ?1",
                [payroll_timesheet_id],
                |row| {
                    let state: String = row.get(0)?;
                    Ok(SnapshotMetadata {
                        state: SnapshotState::parse(&state).ok_or_else(|| {
                            rusqlite::Error::InvalidColumnType(
                                0,
                                "state".to_string(),
                                rusqlite::types::Type::Text,
                            )
                        })?,
                        pdf_path: row.get(1)?,
                        pdf_sha256: row.get(2)?,
                    })
                },
            )
            .optional()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn replace_candidate(
        &self,
        payroll_timesheet_id: i64,
        items: &[WorkedItemSnapshot],
        pdf_path: &str,
        pdf_sha256: &str,
        generated_at: &str,
        previous_cycle_minutes: i64,
        week_ids: &[i64; 4],
        week_totals_minutes: &[i64; 4],
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        crate::payroll_evidence::lifecycle::ensure_editable(&transaction, payroll_timesheet_id)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        let signature = crate::payroll_evidence::lifecycle::candidate_signature(
            &transaction,
            payroll_timesheet_id,
        )
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        transaction.execute("INSERT INTO payroll_candidate_checks VALUES (?1,?2) ON CONFLICT(payroll_timesheet_id) DO UPDATE SET evidence_signature=excluded.evidence_signature",params![payroll_timesheet_id,signature])?;
        let existing_state: Option<String> = transaction
            .query_row(
                "SELECT state FROM payroll_timesheet_snapshot_states
                 WHERE payroll_timesheet_id = ?1",
                [payroll_timesheet_id],
                |row| row.get(0),
            )
            .optional()?;
        if matches!(
            existing_state.as_deref(),
            Some("submitted" | "indeterminate")
        ) {
            return Err(rusqlite::Error::InvalidParameterName(
                "A submitted or indeterminate payroll snapshot cannot be replaced.".to_string(),
            ));
        }
        transaction.execute(
            "DELETE FROM payroll_timesheet_worked_item_snapshots
             WHERE payroll_timesheet_id = ?1",
            [payroll_timesheet_id],
        )?;
        for item in items {
            transaction.execute(
                "INSERT INTO payroll_timesheet_worked_item_snapshots (
                    payroll_timesheet_id, week_number, source_type, timesheet_id,
                    work_date, worked_minutes, pay_rate_id, pay_rate_effective_date,
                    total_hourly_rate, reason, captured_at, direct_shift_id, source_evidence
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    payroll_timesheet_id,
                    item.week_number,
                    item.source_type,
                    item.timesheet_id,
                    item.work_date,
                    item.worked_minutes,
                    item.pay_rate_id,
                    item.pay_rate_effective_date,
                    item.total_hourly_rate,
                    item.reason,
                    generated_at,
                    item.direct_shift_id,
                    item.source_evidence
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO payroll_timesheet_snapshot_states (
                payroll_timesheet_id, state, pdf_path, pdf_sha256, generated_at,
                submitted_at, indeterminate_at
             ) VALUES (?1, 'candidate', ?2, ?3, ?4, NULL, NULL)
             ON CONFLICT(payroll_timesheet_id) DO UPDATE SET
                state = 'candidate', pdf_path = excluded.pdf_path,
                pdf_sha256 = excluded.pdf_sha256, generated_at = excluded.generated_at,
                submitted_at = NULL, indeterminate_at = NULL",
            params![payroll_timesheet_id, pdf_path, pdf_sha256, generated_at],
        )?;
        transaction.execute(
            "UPDATE payroll_timesheets
             SET previous_cycle_hours = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                (previous_cycle_minutes != 0).then(|| previous_cycle_minutes as f64 / 60.0),
                generated_at,
                payroll_timesheet_id
            ],
        )?;
        for index in 0..4 {
            transaction.execute(
                "UPDATE payroll_timesheet_weeks SET worked_hours = ?1 WHERE id = ?2",
                params![week_totals_minutes[index] as f64 / 60.0, week_ids[index]],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn discard_candidate(&self, payroll_timesheet_id: i64) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let changed = transaction.execute(
            "DELETE FROM payroll_timesheet_snapshot_states
             WHERE payroll_timesheet_id = ?1 AND state = 'candidate'",
            [payroll_timesheet_id],
        )?;
        if changed > 0 {
            transaction.execute(
                "DELETE FROM payroll_timesheet_worked_item_snapshots
                 WHERE payroll_timesheet_id = ?1",
                [payroll_timesheet_id],
            )?;
        }
        transaction.commit()
    }

    pub fn verify_current_evidence(
        &self,
        payroll_timesheet_id: i64,
    ) -> crate::payroll_evidence::Result<()> {
        crate::payroll_evidence::lifecycle::verify_candidate_evidence(
            &self.connection,
            payroll_timesheet_id,
        )
    }

    pub fn protect_for_send(&self, payroll_timesheet_id: i64, at: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE payroll_timesheet_snapshot_states
             SET state = 'indeterminate', indeterminate_at = ?1
             WHERE payroll_timesheet_id = ?2 AND state = 'candidate'",
            params![at, payroll_timesheet_id],
        )? == 1)
    }

    pub fn restore_candidate_after_failed_send(&self, payroll_timesheet_id: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE payroll_timesheet_snapshot_states
             SET state = 'candidate', indeterminate_at = NULL
             WHERE payroll_timesheet_id = ?1 AND state = 'indeterminate'",
            [payroll_timesheet_id],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mark_submitted_and_email_sent(
        &self,
        payroll_timesheet_id: i64,
        personal_assistant_id: i64,
        payroll_year: &str,
        cycle_number: i64,
        sent_at: &str,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let changed = transaction.execute(
            "UPDATE payroll_timesheet_snapshot_states
             SET state = 'submitted', submitted_at = ?1, indeterminate_at = NULL
             WHERE payroll_timesheet_id = ?2 AND state = 'indeterminate'",
            params![sent_at, payroll_timesheet_id],
        )?;
        if changed != 1 {
            return Err(rusqlite::Error::InvalidParameterName(
                "Payroll snapshot is not protected for submission.".to_string(),
            ));
        }
        transaction.execute(
            "INSERT INTO payroll_timesheet_email_status (
                personal_assistant_id, payroll_year, cycle_number, email_type, sent_at
             ) VALUES (?1, ?2, ?3, 'timesheet', ?4)
             ON CONFLICT(personal_assistant_id, payroll_year, cycle_number, email_type)
             DO UPDATE SET sent_at = excluded.sent_at",
            params![personal_assistant_id, payroll_year, cycle_number, sent_at],
        )?;
        crate::payroll_evidence::lifecycle::archive_submission(
            &transaction,
            payroll_timesheet_id,
            sent_at,
        )
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        transaction.commit()
    }

    pub fn get_snapshot_items(&self, payroll_timesheet_id: i64) -> Result<Vec<WorkedItemSnapshot>> {
        let mut statement = self.connection.prepare(
            "SELECT week_number, source_type, timesheet_id, work_date, worked_minutes,
                    pay_rate_id, pay_rate_effective_date, total_hourly_rate, reason, direct_shift_id, source_evidence
             FROM payroll_timesheet_worked_item_snapshots
             WHERE payroll_timesheet_id = ?1 ORDER BY id",
        )?;
        let items = statement
            .query_map([payroll_timesheet_id], |row| {
                Ok(WorkedItemSnapshot {
                    week_number: row.get(0)?,
                    source_type: row.get(1)?,
                    timesheet_id: row.get(2)?,
                    direct_shift_id: row.get(9)?,
                    source_evidence: row.get(10)?,
                    work_date: row.get(3)?,
                    worked_minutes: row.get(4)?,
                    pay_rate_id: row.get(5)?,
                    pay_rate_effective_date: row.get(6)?,
                    total_hourly_rate: row.get(7)?,
                    reason: row.get(8)?,
                })
            })?
            .collect();
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;
    use tempfile::NamedTempFile;

    #[test]
    fn manual_adjustment_persists_and_reloads_with_reason() {
        let file = NamedTempFile::new().unwrap();
        let setup = Connection::open(file.path()).unwrap();
        create_schema(&setup).unwrap();
        drop(setup);

        let repository = PayrollWorkedItemRepository::new(Connection::open(file.path()).unwrap());
        repository
            .set_manual_adjustment(
                19,
                &ManualHoursAdjustment {
                    week_number: 3,
                    adjustment_minutes: -45,
                    reason: Some("Corrected external record".to_string()),
                },
                "2027-04-01T10:00:00",
            )
            .unwrap();
        drop(repository);

        let reloaded = PayrollWorkedItemRepository::new(Connection::open(file.path()).unwrap());
        assert_eq!(
            reloaded.get_manual_adjustments(19).unwrap(),
            vec![ManualHoursAdjustment {
                week_number: 3,
                adjustment_minutes: -45,
                reason: Some("Corrected external record".to_string()),
            }]
        );
    }
}
