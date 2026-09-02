#![allow(dead_code)]

use rusqlite::{params, Connection, OptionalExtension, Result, Transaction, TransactionBehavior};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayrollRevisionState {
    Candidate,
    Submitted,
    Indeterminate,
}

impl PayrollRevisionState {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "candidate" => Ok(Self::Candidate),
            "submitted" => Ok(Self::Submitted),
            "indeterminate" => Ok(Self::Indeterminate),
            _ => Err(rusqlite::Error::InvalidColumnType(
                3,
                "state".to_string(),
                rusqlite::types::Type::Text,
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayrollTimesheetRevision {
    pub id: i64,
    pub payroll_timesheet_id: i64,
    pub revision_number: i64,
    pub state: PayrollRevisionState,
    pub pdf_path: String,
    pub pdf_sha256: String,
    pub generated_at: String,
    pub send_attempted_at: Option<String>,
    pub submitted_at: Option<String>,
    pub indeterminate_at: Option<String>,
    pub legacy_backfilled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RevisionWorkedItem {
    pub week_number: i64,
    pub source_type: String,
    pub timesheet_id: Option<i64>,
    pub timesheet_correction_event_id: Option<i64>,
    pub direct_shift_id: Option<i64>,
    pub direct_shift_audit_id: Option<i64>,
    pub effective_start_time: Option<String>,
    pub effective_end_time: Option<String>,
    pub effective_break_minutes: Option<i64>,
    pub effective_notes: Option<String>,
    pub work_date: Option<String>,
    pub worked_minutes: i64,
    pub pay_rate_id: Option<i64>,
    pub pay_rate_effective_date: Option<String>,
    pub total_hourly_rate: Option<f64>,
    pub reason: Option<String>,
    pub captured_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RevisionWeek {
    pub week_number: i64,
    pub week_commencing: String,
    pub worked_hours: f64,
    pub annual_leave_hours: f64,
    pub sick_leave_hours: f64,
    pub public_holiday_hours: f64,
    pub travel_miles: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RevisionPublicHoliday {
    pub week_number: i64,
    pub holiday_date: String,
    pub hours: f64,
}

pub struct CandidateRevision<'a> {
    pub payroll_timesheet_id: i64,
    pub pdf_path: &'a str,
    pub pdf_sha256: &'a str,
    pub generated_at: &'a str,
    pub worked_items: &'a [RevisionWorkedItem],
    pub weeks: &'a [RevisionWeek],
    pub public_holidays: &'a [RevisionPublicHoliday],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionDeliveryAttempt {
    pub id: i64,
    pub payroll_timesheet_revision_id: i64,
    pub outcome: String,
    pub attempted_at: String,
    pub completed_at: Option<String>,
    pub recipient_to: Option<String>,
    pub recipient_cc: Option<String>,
    pub recipient_bcc: Option<String>,
    pub subject: Option<String>,
    pub attachment_path: String,
    pub attachment_sha256: String,
    pub transport_error: Option<String>,
}

pub struct NewRevisionDeliveryAttempt<'a> {
    pub revision_id: i64,
    pub outcome: &'a str,
    pub attempted_at: &'a str,
    pub completed_at: Option<&'a str>,
    pub recipient_to: Option<&'a str>,
    pub recipient_cc: Option<&'a str>,
    pub recipient_bcc: Option<&'a str>,
    pub subject: Option<&'a str>,
    pub attachment_path: &'a str,
    pub attachment_sha256: &'a str,
    pub transport_error: Option<&'a str>,
}

pub struct PayrollRevisionRepository {
    connection: Connection,
}

impl PayrollRevisionRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn history(&self, payroll_timesheet_id: i64) -> Result<Vec<PayrollTimesheetRevision>> {
        let mut statement = self.connection.prepare(
            "SELECT id, payroll_timesheet_id, revision_number, state, pdf_path,
                    pdf_sha256, generated_at, send_attempted_at, submitted_at,
                    indeterminate_at, legacy_backfilled
             FROM payroll_timesheet_revisions
             WHERE payroll_timesheet_id = ?1 ORDER BY revision_number",
        )?;
        let revisions = statement
            .query_map([payroll_timesheet_id], revision_from_row)?
            .collect();
        revisions
    }

    pub fn latest(&self, payroll_timesheet_id: i64) -> Result<Option<PayrollTimesheetRevision>> {
        self.connection
            .query_row(
                "SELECT id, payroll_timesheet_id, revision_number, state, pdf_path,
                        pdf_sha256, generated_at, send_attempted_at, submitted_at,
                        indeterminate_at, legacy_backfilled
                 FROM payroll_timesheet_revisions
                 WHERE payroll_timesheet_id = ?1
                 ORDER BY revision_number DESC LIMIT 1",
                [payroll_timesheet_id],
                revision_from_row,
            )
            .optional()
    }

    pub fn current_candidate(
        &self,
        payroll_timesheet_id: i64,
    ) -> Result<Option<PayrollTimesheetRevision>> {
        self.connection
            .query_row(
                "SELECT id, payroll_timesheet_id, revision_number, state, pdf_path,
                        pdf_sha256, generated_at, send_attempted_at, submitted_at,
                        indeterminate_at, legacy_backfilled
                 FROM payroll_timesheet_revisions
                 WHERE payroll_timesheet_id = ?1 AND state = 'candidate'",
                [payroll_timesheet_id],
                revision_from_row,
            )
            .optional()
    }

    pub fn worked_items(&self, revision_id: i64) -> Result<Vec<RevisionWorkedItem>> {
        let mut statement = self.connection.prepare(
            "SELECT week_number, source_type, timesheet_id, timesheet_correction_event_id,
                    direct_shift_id, direct_shift_audit_id, effective_start_time,
                    effective_end_time, effective_break_minutes, effective_notes,
                    work_date, worked_minutes, pay_rate_id, pay_rate_effective_date,
                    total_hourly_rate, reason, captured_at
             FROM payroll_timesheet_revision_worked_items
             WHERE payroll_timesheet_revision_id = ?1 ORDER BY id",
        )?;
        let items = statement
            .query_map([revision_id], |row| {
                Ok(RevisionWorkedItem {
                    week_number: row.get(0)?,
                    source_type: row.get(1)?,
                    timesheet_id: row.get(2)?,
                    timesheet_correction_event_id: row.get(3)?,
                    direct_shift_id: row.get(4)?,
                    direct_shift_audit_id: row.get(5)?,
                    effective_start_time: row.get(6)?,
                    effective_end_time: row.get(7)?,
                    effective_break_minutes: row.get(8)?,
                    effective_notes: row.get(9)?,
                    work_date: row.get(10)?,
                    worked_minutes: row.get(11)?,
                    pay_rate_id: row.get(12)?,
                    pay_rate_effective_date: row.get(13)?,
                    total_hourly_rate: row.get(14)?,
                    reason: row.get(15)?,
                    captured_at: row.get(16)?,
                })
            })?
            .collect();
        items
    }

    pub fn weeks(&self, revision_id: i64) -> Result<Vec<RevisionWeek>> {
        let mut statement = self.connection.prepare(
            "SELECT week_number, week_commencing, worked_hours, annual_leave_hours,
                    sick_leave_hours, public_holiday_hours, travel_miles
             FROM payroll_timesheet_revision_weeks
             WHERE payroll_timesheet_revision_id = ?1 ORDER BY week_number",
        )?;
        let weeks = statement
            .query_map([revision_id], |row| {
                Ok(RevisionWeek {
                    week_number: row.get(0)?,
                    week_commencing: row.get(1)?,
                    worked_hours: row.get(2)?,
                    annual_leave_hours: row.get(3)?,
                    sick_leave_hours: row.get(4)?,
                    public_holiday_hours: row.get(5)?,
                    travel_miles: row.get(6)?,
                })
            })?
            .collect();
        weeks
    }

    pub fn public_holidays(&self, revision_id: i64) -> Result<Vec<RevisionPublicHoliday>> {
        let mut statement = self.connection.prepare(
            "SELECT week_number, holiday_date, hours
             FROM payroll_timesheet_revision_public_holidays
             WHERE payroll_timesheet_revision_id = ?1 ORDER BY week_number, holiday_date",
        )?;
        let holidays = statement
            .query_map([revision_id], |row| {
                Ok(RevisionPublicHoliday {
                    week_number: row.get(0)?,
                    holiday_date: row.get(1)?,
                    hours: row.get(2)?,
                })
            })?
            .collect();
        holidays
    }

    pub fn delivery_attempts(&self, revision_id: i64) -> Result<Vec<RevisionDeliveryAttempt>> {
        let mut statement = self.connection.prepare(
            "SELECT id, payroll_timesheet_revision_id, outcome, attempted_at,
                    completed_at, recipient_to, recipient_cc, recipient_bcc, subject,
                    attachment_path, attachment_sha256, transport_error
             FROM payroll_timesheet_revision_delivery_attempts
             WHERE payroll_timesheet_revision_id = ?1 ORDER BY id",
        )?;
        let attempts = statement
            .query_map([revision_id], |row| {
                Ok(RevisionDeliveryAttempt {
                    id: row.get(0)?,
                    payroll_timesheet_revision_id: row.get(1)?,
                    outcome: row.get(2)?,
                    attempted_at: row.get(3)?,
                    completed_at: row.get(4)?,
                    recipient_to: row.get(5)?,
                    recipient_cc: row.get(6)?,
                    recipient_bcc: row.get(7)?,
                    subject: row.get(8)?,
                    attachment_path: row.get(9)?,
                    attachment_sha256: row.get(10)?,
                    transport_error: row.get(11)?,
                })
            })?
            .collect();
        attempts
    }

    pub fn append_delivery_attempt(&self, attempt: NewRevisionDeliveryAttempt<'_>) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO payroll_timesheet_revision_delivery_attempts (
                payroll_timesheet_revision_id, outcome, attempted_at, completed_at,
                recipient_to, recipient_cc, recipient_bcc, subject, attachment_path,
                attachment_sha256, transport_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                attempt.revision_id,
                attempt.outcome,
                attempt.attempted_at,
                attempt.completed_at,
                attempt.recipient_to,
                attempt.recipient_cc,
                attempt.recipient_bcc,
                attempt.subject,
                attempt.attachment_path,
                attempt.attachment_sha256,
                attempt.transport_error
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn create_or_replace_candidate(
        &self,
        candidate: CandidateRevision<'_>,
    ) -> Result<PayrollTimesheetRevision> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        if !transaction.query_row(
            "SELECT EXISTS (SELECT 1 FROM payroll_timesheets WHERE id = ?1)",
            [candidate.payroll_timesheet_id],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(rusqlite::Error::InvalidParameterName(
                "Candidate revision requires an existing payroll timesheet.".to_string(),
            ));
        }
        let existing: Option<(i64, i64)> = transaction
            .query_row(
                "SELECT id, revision_number FROM payroll_timesheet_revisions
                 WHERE payroll_timesheet_id = ?1 AND state = 'candidate'",
                [candidate.payroll_timesheet_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let existing_candidate_id = existing.map(|(revision_id, _)| revision_id);
        if transaction.query_row(
            "SELECT EXISTS (
                SELECT 1 FROM payroll_timesheet_revisions
                WHERE pdf_path = ?1 AND id <> COALESCE(?2, -1)
             )",
            params![candidate.pdf_path, existing_candidate_id],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(rusqlite::Error::InvalidParameterName(
                "Candidate PDF path is already owned by another payroll revision.".to_string(),
            ));
        }
        let revision_id = if let Some((revision_id, _)) = existing {
            if transaction.execute(
                "UPDATE payroll_timesheet_revisions
                 SET pdf_path = ?1, pdf_sha256 = ?2, generated_at = ?3,
                     send_attempted_at = NULL, submitted_at = NULL,
                     indeterminate_at = NULL, legacy_backfilled = 0
                 WHERE id = ?4 AND payroll_timesheet_id = ?5 AND state = 'candidate'",
                params![
                    candidate.pdf_path,
                    candidate.pdf_sha256,
                    candidate.generated_at,
                    revision_id,
                    candidate.payroll_timesheet_id
                ],
            )? != 1
            {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            transaction.execute(
                "DELETE FROM payroll_timesheet_revision_worked_items
                 WHERE payroll_timesheet_revision_id = ?1",
                [revision_id],
            )?;
            transaction.execute(
                "DELETE FROM payroll_timesheet_revision_weeks
                 WHERE payroll_timesheet_revision_id = ?1",
                [revision_id],
            )?;
            transaction.execute(
                "DELETE FROM payroll_timesheet_revision_public_holidays
                 WHERE payroll_timesheet_revision_id = ?1",
                [revision_id],
            )?;
            revision_id
        } else {
            let revision_number: i64 = transaction.query_row(
                "SELECT COALESCE(MAX(revision_number), 0) + 1
                 FROM payroll_timesheet_revisions WHERE payroll_timesheet_id = ?1",
                [candidate.payroll_timesheet_id],
                |row| row.get(0),
            )?;
            transaction.execute(
                "INSERT INTO payroll_timesheet_revisions (
                    payroll_timesheet_id, revision_number, state, pdf_path,
                    pdf_sha256, generated_at, legacy_backfilled
                 ) VALUES (?1, ?2, 'candidate', ?3, ?4, ?5, 0)",
                params![
                    candidate.payroll_timesheet_id,
                    revision_number,
                    candidate.pdf_path,
                    candidate.pdf_sha256,
                    candidate.generated_at
                ],
            )?;
            transaction.last_insert_rowid()
        };

        insert_candidate_evidence(&transaction, revision_id, &candidate)?;
        let revision = transaction.query_row(
            "SELECT id, payroll_timesheet_id, revision_number, state, pdf_path,
                    pdf_sha256, generated_at, send_attempted_at, submitted_at,
                    indeterminate_at, legacy_backfilled
             FROM payroll_timesheet_revisions WHERE id = ?1",
            [revision_id],
            revision_from_row,
        )?;
        transaction.commit()?;
        Ok(revision)
    }
}

fn insert_candidate_evidence(
    connection: &Connection,
    revision_id: i64,
    candidate: &CandidateRevision<'_>,
) -> Result<()> {
    for item in candidate.worked_items {
        connection.execute(
            "INSERT INTO payroll_timesheet_revision_worked_items (
                payroll_timesheet_revision_id, week_number, source_type, timesheet_id,
                timesheet_correction_event_id, direct_shift_id, direct_shift_audit_id,
                effective_start_time, effective_end_time, effective_break_minutes,
                effective_notes, work_date, worked_minutes, pay_rate_id,
                pay_rate_effective_date, total_hourly_rate, reason, captured_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                revision_id,
                item.week_number,
                item.source_type,
                item.timesheet_id,
                item.timesheet_correction_event_id,
                item.direct_shift_id,
                item.direct_shift_audit_id,
                item.effective_start_time,
                item.effective_end_time,
                item.effective_break_minutes,
                item.effective_notes,
                item.work_date,
                item.worked_minutes,
                item.pay_rate_id,
                item.pay_rate_effective_date,
                item.total_hourly_rate,
                item.reason,
                item.captured_at
            ],
        )?;
    }
    for week in candidate.weeks {
        connection.execute(
            "INSERT INTO payroll_timesheet_revision_weeks (
                payroll_timesheet_revision_id, week_number, week_commencing,
                worked_hours, annual_leave_hours, sick_leave_hours,
                public_holiday_hours, travel_miles
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                revision_id,
                week.week_number,
                week.week_commencing,
                week.worked_hours,
                week.annual_leave_hours,
                week.sick_leave_hours,
                week.public_holiday_hours,
                week.travel_miles
            ],
        )?;
    }
    for holiday in candidate.public_holidays {
        connection.execute(
            "INSERT INTO payroll_timesheet_revision_public_holidays (
                payroll_timesheet_revision_id, week_number, holiday_date, hours
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                revision_id,
                holiday.week_number,
                holiday.holiday_date,
                holiday.hours
            ],
        )?;
    }
    Ok(())
}

fn revision_from_row(row: &rusqlite::Row<'_>) -> Result<PayrollTimesheetRevision> {
    let state: String = row.get(3)?;
    Ok(PayrollTimesheetRevision {
        id: row.get(0)?,
        payroll_timesheet_id: row.get(1)?,
        revision_number: row.get(2)?,
        state: PayrollRevisionState::parse(&state)?,
        pdf_path: row.get(4)?,
        pdf_sha256: row.get(5)?,
        generated_at: row.get(6)?,
        send_attempted_at: row.get(7)?,
        submitted_at: row.get(8)?,
        indeterminate_at: row.get(9)?,
        legacy_backfilled: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> PayrollRevisionRepository {
        let connection = Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO payroll_timesheets (
                    id, personal_assistant_id, payroll_year, cycle_number,
                    created_at, updated_at
                 ) VALUES (40, 7, '2026/27', 6, 'created', 'updated')",
                [],
            )
            .unwrap();
        PayrollRevisionRepository::new(connection)
    }

    fn worked_item(minutes: i64) -> RevisionWorkedItem {
        RevisionWorkedItem {
            week_number: 1,
            source_type: "imported_shift".to_string(),
            timesheet_id: Some(91),
            timesheet_correction_event_id: Some(12),
            direct_shift_id: None,
            direct_shift_audit_id: None,
            effective_start_time: Some("2026-08-10T09:00".to_string()),
            effective_end_time: Some("2026-08-10T10:00".to_string()),
            effective_break_minutes: Some(0),
            effective_notes: Some("copied evidence".to_string()),
            work_date: Some("2026-08-10".to_string()),
            worked_minutes: minutes,
            pay_rate_id: Some(3),
            pay_rate_effective_date: Some("01/04/2026".to_string()),
            total_hourly_rate: Some(14.5),
            reason: None,
            captured_at: "captured".to_string(),
        }
    }

    fn week(hours: f64) -> RevisionWeek {
        RevisionWeek {
            week_number: 1,
            week_commencing: "10/08/2026".to_string(),
            worked_hours: hours,
            annual_leave_hours: 1.0,
            sick_leave_hours: 2.0,
            public_holiday_hours: 3.0,
            travel_miles: 4.0,
        }
    }

    fn holiday(hours: f64) -> RevisionPublicHoliday {
        RevisionPublicHoliday {
            week_number: 1,
            holiday_date: "10/08/2026".to_string(),
            hours,
        }
    }

    #[test]
    fn allocates_revision_numbers_and_replaces_only_the_current_candidate() {
        let repository = repository();
        repository
            .connection
            .execute_batch(
                "INSERT INTO payroll_timesheet_revisions (
                    payroll_timesheet_id, revision_number, state, pdf_path,
                    pdf_sha256, generated_at, submitted_at, legacy_backfilled
                 ) VALUES (40, 1, 'submitted', '/r1.pdf', 'old-digest',
                           'generated-1', 'submitted-1', 1);
                 INSERT INTO payroll_timesheet_revisions (
                    payroll_timesheet_id, revision_number, state, pdf_path,
                    pdf_sha256, generated_at, send_attempted_at, indeterminate_at,
                    legacy_backfilled
                 ) VALUES (40, 2, 'indeterminate', '/r2.pdf', 'uncertain-digest',
                           'generated-2', 'attempted-2', 'attempted-2', 1);",
            )
            .unwrap();
        let first_items = [worked_item(60)];
        let first_weeks = [week(1.0)];
        let first_holidays = [holiday(3.0)];
        assert!(repository
            .create_or_replace_candidate(CandidateRevision {
                payroll_timesheet_id: 40,
                pdf_path: "/r1.pdf",
                pdf_sha256: "must-not-reuse",
                generated_at: "must-not-create",
                worked_items: &first_items,
                weeks: &first_weeks,
                public_holidays: &first_holidays,
            })
            .is_err());
        assert_eq!(repository.history(40).unwrap().len(), 2);
        let created = repository
            .create_or_replace_candidate(CandidateRevision {
                payroll_timesheet_id: 40,
                pdf_path: "/r3.pdf",
                pdf_sha256: "digest-3",
                generated_at: "generated-3",
                worked_items: &first_items,
                weeks: &first_weeks,
                public_holidays: &first_holidays,
            })
            .unwrap();
        assert_eq!(created.revision_number, 3);

        let replacement_items = [worked_item(75)];
        let replacement_weeks = [week(1.25)];
        let replacement_holidays = [holiday(4.0)];
        let replaced = repository
            .create_or_replace_candidate(CandidateRevision {
                payroll_timesheet_id: 40,
                pdf_path: "/r3-replaced.pdf",
                pdf_sha256: "digest-3b",
                generated_at: "generated-3b",
                worked_items: &replacement_items,
                weeks: &replacement_weeks,
                public_holidays: &replacement_holidays,
            })
            .unwrap();

        assert_eq!(replaced.id, created.id);
        assert_eq!(replaced.revision_number, 3);
        assert_eq!(
            repository.worked_items(replaced.id).unwrap(),
            replacement_items
        );
        assert_eq!(repository.weeks(replaced.id).unwrap(), replacement_weeks);
        assert_eq!(
            repository.public_holidays(replaced.id).unwrap(),
            replacement_holidays
        );
        let history = repository.history(40).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].state, PayrollRevisionState::Submitted);
        assert_eq!(history[0].pdf_path, "/r1.pdf");
        assert_eq!(history[1].state, PayrollRevisionState::Indeterminate);
        assert_eq!(history[1].pdf_path, "/r2.pdf");
        assert_eq!(history[1].indeterminate_at.as_deref(), Some("attempted-2"));
        assert_eq!(repository.latest(40).unwrap(), Some(replaced.clone()));
        assert_eq!(repository.current_candidate(40).unwrap(), Some(replaced));

        assert!(repository
            .connection
            .execute(
                "INSERT INTO payroll_timesheet_revisions (
                    payroll_timesheet_id, revision_number, state, pdf_path,
                    pdf_sha256, generated_at
                 ) VALUES (40, 3, 'candidate', '/duplicate-number.pdf', 'x', 'x')",
                [],
            )
            .is_err());
        assert!(repository
            .connection
            .execute(
                "INSERT INTO payroll_timesheet_revisions (
                    payroll_timesheet_id, revision_number, state, pdf_path,
                    pdf_sha256, generated_at, submitted_at
                 ) VALUES (40, 4, 'submitted', '/r1.pdf', 'x', 'x', 'x')",
                [],
            )
            .is_err());
        assert!(repository
            .connection
            .execute(
                "INSERT INTO payroll_timesheet_revision_worked_items (
                    payroll_timesheet_revision_id, week_number, source_type,
                    timesheet_correction_event_id, worked_minutes, captured_at
                 ) VALUES (?1, 1, 'future_imported', 88, 60, 'captured')",
                [created.id],
            )
            .is_err());
        assert!(repository
            .connection
            .execute(
                "INSERT INTO payroll_timesheet_revision_worked_items (
                    payroll_timesheet_revision_id, week_number, source_type,
                    direct_shift_audit_id, worked_minutes, captured_at
                 ) VALUES (?1, 1, 'future_direct', 99, 60, 'captured')",
                [created.id],
            )
            .is_err());
    }

    #[test]
    fn evidence_failure_rolls_back_new_candidate_and_all_children() {
        let repository = repository();
        repository
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_revision_week
                 BEFORE INSERT ON payroll_timesheet_revision_weeks
                 BEGIN SELECT RAISE(FAIL, 'injected revision failure'); END;",
            )
            .unwrap();
        let items = [worked_item(60)];
        let weeks = [week(1.0)];
        let holidays = [holiday(3.0)];
        assert!(repository
            .create_or_replace_candidate(CandidateRevision {
                payroll_timesheet_id: 40,
                pdf_path: "/candidate.pdf",
                pdf_sha256: "digest",
                generated_at: "generated",
                worked_items: &items,
                weeks: &weeks,
                public_holidays: &holidays,
            })
            .is_err());
        assert!(repository.history(40).unwrap().is_empty());
        let item_count: i64 = repository
            .connection
            .query_row(
                "SELECT COUNT(*) FROM payroll_timesheet_revision_worked_items",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(item_count, 0);
    }

    #[test]
    fn delivery_attempts_are_append_only_through_the_repository() {
        let repository = repository();
        let no_items = [];
        let no_weeks = [];
        let no_holidays = [];
        let revision = repository
            .create_or_replace_candidate(CandidateRevision {
                payroll_timesheet_id: 40,
                pdf_path: "/candidate.pdf",
                pdf_sha256: "digest",
                generated_at: "generated",
                worked_items: &no_items,
                weeks: &no_weeks,
                public_holidays: &no_holidays,
            })
            .unwrap();
        repository
            .append_delivery_attempt(NewRevisionDeliveryAttempt {
                revision_id: revision.id,
                outcome: "protected",
                attempted_at: "attempted",
                completed_at: None,
                recipient_to: Some("payroll@example.test"),
                recipient_cc: None,
                recipient_bcc: None,
                subject: Some("Timesheet revision 1"),
                attachment_path: "/candidate.pdf",
                attachment_sha256: "digest",
                transport_error: None,
            })
            .unwrap();
        assert_eq!(repository.delivery_attempts(revision.id).unwrap().len(), 1);
    }
}
