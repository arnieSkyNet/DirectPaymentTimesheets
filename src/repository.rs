use std::fmt;

use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::models::TimesheetEntry;

#[allow(dead_code)]
pub const LOCAL_CORRECTION_ACTOR_ID: &str = "local_employer";
const CORRECTION_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M";

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimesheetCorrectionAction {
    Edit,
    Revert,
}

impl TimesheetCorrectionAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Revert => "revert",
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveTimesheetValues {
    pub start_time: String,
    pub end_time: String,
    pub break_minutes: i64,
    pub worked_minutes: i64,
    pub notes: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveTimesheetEntry {
    pub raw: TimesheetEntry,
    pub effective: EffectiveTimesheetValues,
    pub latest_correction_event_id: Option<i64>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimesheetCorrectionEvent {
    pub id: i64,
    pub timesheet_id: i64,
    pub actor_id: String,
    pub action_type: String,
    pub action_at: String,
    pub reason: Option<String>,
    pub before: EffectiveTimesheetValues,
    pub after: EffectiveTimesheetValues,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimesheetCorrectionProposal {
    pub start_time: String,
    pub end_time: String,
    pub break_minutes: i64,
    pub worked_minutes: i64,
    pub notes: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendCorrectionResult {
    pub event_id: Option<i64>,
    pub effective: EffectiveTimesheetValues,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum TimesheetCorrectionError {
    Database(rusqlite::Error),
    NotFound(i64),
    InvalidTimestamp(String),
    EndNotAfterStart,
    NegativeBreak,
    NegativeWorked,
    MissingActor,
    MissingActionTime,
}

impl fmt::Display for TimesheetCorrectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::NotFound(id) => write!(formatter, "imported timesheet row {id} was not found"),
            Self::InvalidTimestamp(value) => write!(formatter, "invalid timestamp: {value}"),
            Self::EndNotAfterStart => write!(formatter, "end time must be later than start time"),
            Self::NegativeBreak => write!(formatter, "break minutes cannot be negative"),
            Self::NegativeWorked => write!(formatter, "worked minutes cannot be negative"),
            Self::MissingActor => write!(formatter, "correction actor identity is required"),
            Self::MissingActionTime => write!(formatter, "correction action time is required"),
        }
    }
}

impl std::error::Error for TimesheetCorrectionError {}

impl From<rusqlite::Error> for TimesheetCorrectionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

pub struct TimesheetRepository {
    connection: Connection,
}

impl TimesheetRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    #[cfg(test)]
    pub fn insert(&self, entry: &TimesheetEntry) -> Result<()> {
        let personal_assistant_id: Option<i64> = self
            .connection
            .query_row(
                "
                SELECT id
                FROM personal_assistants
                WHERE first_name || ' ' || surname = ?1
                LIMIT 1
                ",
                params![&entry.pa_name],
                |row| row.get(0),
            )
            .ok();

        self.connection.execute(
            "INSERT INTO timesheets (
                pa_name,
                personal_assistant_id,
                start_time,
                end_time,
                break_minutes,
                worked_minutes,
                hourly_rate,
                amount,
                notes
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &entry.pa_name,
                personal_assistant_id,
                &entry.start_time,
                &entry.end_time,
                &entry.break_minutes,
                &entry.worked_minutes,
                &entry.hourly_rate,
                &entry.amount,
                &entry.notes,
            ],
        )?;

        Ok(())
    }

    pub fn get_all_raw(&self) -> Result<Vec<TimesheetEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT
                id,
                pa_name,
                personal_assistant_id,
                start_time,
                end_time,
                break_minutes,
                worked_minutes,
                hourly_rate,
                amount,
                notes
            FROM timesheets",
        )?;

        let timesheets = statement.query_map([], timesheet_from_row)?;

        let mut entries = Vec::new();

        for timesheet in timesheets {
            entries.push(timesheet?);
        }

        Ok(entries)
    }

    #[allow(dead_code)]
    pub fn get_all_effective(
        &self,
    ) -> std::result::Result<Vec<EffectiveTimesheetEntry>, TimesheetCorrectionError> {
        self.get_all_raw()?
            .into_iter()
            .map(|raw| effective_entry_on(&self.connection, raw))
            .collect()
    }

    #[allow(dead_code)]
    pub fn get_raw_by_id(&self, id: i64) -> Result<Option<TimesheetEntry>> {
        get_raw_on(&self.connection, id)
    }

    #[allow(dead_code)]
    pub fn append_correction(
        &self,
        timesheet_id: i64,
        proposal: &TimesheetCorrectionProposal,
        actor_id: &str,
        action_at: &str,
        reason: Option<&str>,
    ) -> std::result::Result<AppendCorrectionResult, TimesheetCorrectionError> {
        self.append_correction_with_action(
            timesheet_id,
            TimesheetCorrectionAction::Edit,
            Some(proposal),
            actor_id,
            action_at,
            reason,
        )
    }

    fn append_correction_with_action(
        &self,
        timesheet_id: i64,
        action: TimesheetCorrectionAction,
        proposal: Option<&TimesheetCorrectionProposal>,
        actor_id: &str,
        action_at: &str,
        reason: Option<&str>,
    ) -> std::result::Result<AppendCorrectionResult, TimesheetCorrectionError> {
        if actor_id.trim().is_empty() {
            return Err(TimesheetCorrectionError::MissingActor);
        }
        if action_at.trim().is_empty() {
            return Err(TimesheetCorrectionError::MissingActionTime);
        }
        let transaction = self.connection.unchecked_transaction()?;
        let raw = get_raw_on(&transaction, timesheet_id)?
            .ok_or(TimesheetCorrectionError::NotFound(timesheet_id))?;
        let after = match proposal {
            Some(proposal) => validate_and_normalize_proposal(proposal)?,
            None => validate_and_normalize_proposal(&TimesheetCorrectionProposal {
                start_time: raw.start_time.clone(),
                end_time: raw.end_time.clone(),
                break_minutes: raw.break_minutes,
                worked_minutes: raw.worked_minutes,
                notes: raw.notes.clone(),
            })?,
        };
        let current = effective_entry_on(&transaction, raw)?;
        if current.effective == after {
            transaction.commit()?;
            return Ok(AppendCorrectionResult {
                event_id: None,
                effective: after,
            });
        }
        let reason = normalized_optional_text(reason);
        transaction.execute(
            "INSERT INTO timesheet_correction_events (
                timesheet_id, actor_id, action_type, action_at, reason,
                before_start_time, before_end_time, before_break_minutes,
                before_worked_minutes, before_notes,
                after_start_time, after_end_time, after_break_minutes,
                after_worked_minutes, after_notes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                timesheet_id,
                actor_id.trim(),
                action.as_str(),
                action_at.trim(),
                reason,
                current.effective.start_time,
                current.effective.end_time,
                current.effective.break_minutes,
                current.effective.worked_minutes,
                current.effective.notes,
                after.start_time,
                after.end_time,
                after.break_minutes,
                after.worked_minutes,
                after.notes,
            ],
        )?;
        let event_id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(AppendCorrectionResult {
            event_id: Some(event_id),
            effective: after,
        })
    }

    #[allow(dead_code)]
    pub fn revert_correction_to_raw(
        &self,
        timesheet_id: i64,
        actor_id: &str,
        action_at: &str,
        reason: Option<&str>,
    ) -> std::result::Result<AppendCorrectionResult, TimesheetCorrectionError> {
        self.append_correction_with_action(
            timesheet_id,
            TimesheetCorrectionAction::Revert,
            None,
            actor_id,
            action_at,
            reason,
        )
    }

    #[allow(dead_code)]
    pub fn correction_history(
        &self,
        timesheet_id: i64,
    ) -> std::result::Result<Vec<TimesheetCorrectionEvent>, TimesheetCorrectionError> {
        let mut statement = self.connection.prepare(
            "SELECT id, timesheet_id, actor_id, action_type, action_at, reason,
                    before_start_time, before_end_time, before_break_minutes,
                    before_worked_minutes, before_notes,
                    after_start_time, after_end_time, after_break_minutes,
                    after_worked_minutes, after_notes
             FROM timesheet_correction_events
             WHERE timesheet_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map([timesheet_id], correction_event_from_row)?;
        Ok(rows.collect::<Result<Vec<_>>>()?)
    }

    pub fn resolve_personal_assistant_ids(&self, imported_name: &str) -> Result<Vec<i64>> {
        let wanted = normalize_person_name(imported_name);
        let mut statement = self
            .connection
            .prepare("SELECT id, first_name, surname FROM personal_assistants ORDER BY id")?;
        let candidates = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut matches = Vec::new();
        for candidate in candidates {
            let (id, first_name, surname) = candidate?;
            if normalize_person_name(&format!("{first_name} {surname}")) == wanted {
                matches.push(id);
            }
        }
        Ok(matches)
    }

    pub fn import_file_atomically(
        &self,
        entries: &[TimesheetEntry],
        import_time: &str,
        original_filename: &str,
        archive_filename: &str,
        rows_processed: i64,
        rows_skipped: i64,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        for entry in entries {
            let personal_assistant_id = entry.personal_assistant_id.ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "CSV import requires a uniquely resolved personal_assistant_id".to_string(),
                )
            })?;
            transaction.execute(
                "INSERT INTO timesheets (
                    pa_name, personal_assistant_id, start_time, end_time,
                    break_minutes, worked_minutes, hourly_rate, amount, notes
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entry.pa_name,
                    personal_assistant_id,
                    entry.start_time,
                    entry.end_time,
                    entry.break_minutes,
                    entry.worked_minutes,
                    entry.hourly_rate,
                    entry.amount,
                    entry.notes,
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO import_audit (
                import_time, original_filename, archive_filename, rows_processed,
                rows_imported, rows_skipped, status, error_message
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'SUCCESS', NULL)",
            params![
                import_time,
                original_filename,
                archive_filename,
                rows_processed,
                entries.len() as i64,
                rows_skipped,
            ],
        )?;
        transaction.commit()
    }

    pub fn add_import_audit(
        &self,
        import_time: &str,
        original_filename: &str,
        archive_filename: &str,
        rows_processed: i64,
        rows_imported: i64,
        rows_skipped: i64,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<()> {
        self.connection.execute(
            "
            INSERT INTO import_audit (
                import_time,
                original_filename,
                archive_filename,
                rows_processed,
                rows_imported,
                rows_skipped,
                status,
                error_message
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                import_time,
                original_filename,
                archive_filename,
                rows_processed,
                rows_imported,
                rows_skipped,
                status,
                error_message,
            ],
        )?;

        Ok(())
    }

    pub fn has_successful_import(&self, filename: &str) -> Result<bool> {
        let mut statement = self.connection.prepare(
            "
            SELECT COUNT(*)
            FROM import_audit
            WHERE original_filename = ?1
            AND status = 'SUCCESS'
            ",
        )?;

        let count: i64 = statement.query_row(params![filename], |row| row.get(0))?;

        Ok(count > 0)
    }
}

fn timesheet_from_row(row: &rusqlite::Row<'_>) -> Result<TimesheetEntry> {
    Ok(TimesheetEntry {
        id: row.get(0)?,
        pa_name: row.get(1)?,
        personal_assistant_id: row.get(2)?,
        start_time: row.get(3)?,
        end_time: row.get(4)?,
        break_minutes: row.get(5)?,
        worked_minutes: row.get(6)?,
        hourly_rate: row.get(7)?,
        amount: row.get(8)?,
        notes: row.get(9)?,
    })
}

fn get_raw_on(connection: &Connection, id: i64) -> Result<Option<TimesheetEntry>> {
    connection
        .query_row(
            "SELECT id, pa_name, personal_assistant_id, start_time, end_time,
                    break_minutes, worked_minutes, hourly_rate, amount, notes
             FROM timesheets WHERE id = ?1",
            [id],
            timesheet_from_row,
        )
        .optional()
}

fn effective_entry_on(
    connection: &Connection,
    raw: TimesheetEntry,
) -> std::result::Result<EffectiveTimesheetEntry, TimesheetCorrectionError> {
    let latest = connection
        .query_row(
            "SELECT id, after_start_time, after_end_time, after_break_minutes,
                    after_worked_minutes, after_notes
             FROM timesheet_correction_events
             WHERE timesheet_id = ?1 ORDER BY id DESC LIMIT 1",
            [raw.id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    EffectiveTimesheetValues {
                        start_time: row.get(1)?,
                        end_time: row.get(2)?,
                        break_minutes: row.get(3)?,
                        worked_minutes: row.get(4)?,
                        notes: row.get(5)?,
                    },
                ))
            },
        )
        .optional()?;
    let (latest_correction_event_id, effective) = match latest {
        Some((id, values)) => (Some(id), values),
        None => (
            None,
            validate_and_normalize_proposal(&TimesheetCorrectionProposal {
                start_time: raw.start_time.clone(),
                end_time: raw.end_time.clone(),
                break_minutes: raw.break_minutes,
                worked_minutes: raw.worked_minutes,
                notes: raw.notes.clone(),
            })?,
        ),
    };
    Ok(EffectiveTimesheetEntry {
        raw,
        effective,
        latest_correction_event_id,
    })
}

fn validate_and_normalize_proposal(
    proposal: &TimesheetCorrectionProposal,
) -> std::result::Result<EffectiveTimesheetValues, TimesheetCorrectionError> {
    let start = parse_correction_timestamp(&proposal.start_time)?;
    let end = parse_correction_timestamp(&proposal.end_time)?;
    if end <= start {
        return Err(TimesheetCorrectionError::EndNotAfterStart);
    }
    if proposal.break_minutes < 0 {
        return Err(TimesheetCorrectionError::NegativeBreak);
    }
    if proposal.worked_minutes < 0 {
        return Err(TimesheetCorrectionError::NegativeWorked);
    }
    Ok(EffectiveTimesheetValues {
        start_time: start.format(CORRECTION_TIME_FORMAT).to_string(),
        end_time: end.format(CORRECTION_TIME_FORMAT).to_string(),
        break_minutes: proposal.break_minutes,
        worked_minutes: proposal.worked_minutes,
        notes: normalized_optional_text(proposal.notes.as_deref()),
    })
}

fn parse_correction_timestamp(
    value: &str,
) -> std::result::Result<NaiveDateTime, TimesheetCorrectionError> {
    NaiveDateTime::parse_from_str(value.trim(), CORRECTION_TIME_FORMAT)
        .ok()
        .or_else(|| crate::csv_import::parse_supported_timestamp(value))
        .ok_or_else(|| TimesheetCorrectionError::InvalidTimestamp(value.to_string()))
}

fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn correction_event_from_row(row: &rusqlite::Row<'_>) -> Result<TimesheetCorrectionEvent> {
    Ok(TimesheetCorrectionEvent {
        id: row.get(0)?,
        timesheet_id: row.get(1)?,
        actor_id: row.get(2)?,
        action_type: row.get(3)?,
        action_at: row.get(4)?,
        reason: row.get(5)?,
        before: EffectiveTimesheetValues {
            start_time: row.get(6)?,
            end_time: row.get(7)?,
            break_minutes: row.get(8)?,
            worked_minutes: row.get(9)?,
            notes: row.get(10)?,
        },
        after: EffectiveTimesheetValues {
            start_time: row.get(11)?,
            end_time: row.get(12)?,
            break_minutes: row.get(13)?,
            worked_minutes: row.get(14)?,
            notes: row.get(15)?,
        },
    })
}

fn normalize_person_name(value: &str) -> String {
    value
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;

    fn create_test_repository() -> TimesheetRepository {
        let connection = Connection::open_in_memory().unwrap();

        create_schema(&connection).unwrap();

        TimesheetRepository::new(connection)
    }

    fn test_entry() -> TimesheetEntry {
        TimesheetEntry {
            id: 0,
            pa_name: "Test PA".to_string(),
            personal_assistant_id: None,
            start_time: "09:00".to_string(),
            end_time: "17:00".to_string(),
            break_minutes: 30,
            worked_minutes: 450,
            hourly_rate: 15.0,
            amount: 112.5,
            notes: None,
        }
    }

    fn valid_imported_entry() -> TimesheetEntry {
        TimesheetEntry {
            id: 0,
            pa_name: "Test PA".to_string(),
            personal_assistant_id: None,
            start_time: "1 September 2026 at 09:00:00".to_string(),
            end_time: "1 September 2026 at 10:00:00".to_string(),
            break_minutes: 5,
            worked_minutes: 47,
            hourly_rate: 15.0,
            amount: 11.75,
            notes: Some("source note".to_string()),
        }
    }

    fn insert_valid_imported(repository: &TimesheetRepository) -> TimesheetEntry {
        repository.insert(&valid_imported_entry()).unwrap();
        repository.get_all_raw().unwrap().remove(0)
    }

    fn proposal(
        start: &str,
        end: &str,
        break_minutes: i64,
        worked_minutes: i64,
        notes: Option<&str>,
    ) -> TimesheetCorrectionProposal {
        TimesheetCorrectionProposal {
            start_time: start.to_string(),
            end_time: end.to_string(),
            break_minutes,
            worked_minutes,
            notes: notes.map(str::to_string),
        }
    }

    #[test]
    fn insert_and_get_timesheet() {
        let repository = create_test_repository();

        repository.insert(&test_entry()).unwrap();

        let entries = repository.get_all_raw().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pa_name, "Test PA");
    }

    #[test]
    fn import_audit_is_recorded() {
        let repository = create_test_repository();

        repository
            .add_import_audit(
                "2026-01-01 12:00:00",
                "/test/file.csv",
                "/archive/file.csv",
                1,
                1,
                0,
                "SUCCESS",
                None,
            )
            .unwrap();

        assert!(repository.has_successful_import("/test/file.csv").unwrap());
    }

    #[test]
    fn corrections_append_history_and_latest_event_drives_effective_projection() {
        let repository = create_test_repository();
        let raw = insert_valid_imported(&repository);
        let first = proposal(
            "2026-09-01T09:10",
            "2026-09-01T11:10",
            30,
            17,
            Some("first correction"),
        );
        let second = proposal(
            "2026-09-01T09:15",
            "2026-09-01T11:45",
            20,
            83,
            Some("final correction"),
        );

        let first_result = repository
            .append_correction(
                raw.id,
                &first,
                LOCAL_CORRECTION_ACTOR_ID,
                "2026-09-02T10:00:00Z",
                Some("Payroll queried the source"),
            )
            .unwrap();
        let second_result = repository
            .append_correction(
                raw.id,
                &second,
                LOCAL_CORRECTION_ACTOR_ID,
                "2026-09-02T10:05:00Z",
                None,
            )
            .unwrap();

        let history = repository.correction_history(raw.id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].before.worked_minutes, 47);
        assert_eq!(history[0].after.worked_minutes, 17);
        assert_eq!(history[1].before, history[0].after);
        assert_eq!(history[1].after.worked_minutes, 83);
        assert_eq!(history[0].actor_id, LOCAL_CORRECTION_ACTOR_ID);
        assert_eq!(history[0].action_type, "edit");
        assert_eq!(
            history[0].reason.as_deref(),
            Some("Payroll queried the source")
        );

        let effective = repository.get_all_effective().unwrap().remove(0);
        assert_eq!(effective.raw, raw);
        assert_eq!(effective.effective, history[1].after);
        assert_eq!(effective.latest_correction_event_id, second_result.event_id);
        assert!(first_result.event_id.is_some());
        assert_eq!(effective.effective.worked_minutes, 83);
        assert_ne!(
            effective.effective.worked_minutes,
            150 - effective.effective.break_minutes
        );
    }

    #[test]
    fn reversion_appends_event_and_restores_raw_effective_values() {
        let repository = create_test_repository();
        let raw = insert_valid_imported(&repository);
        repository
            .append_correction(
                raw.id,
                &proposal(
                    "2026-09-01T08:30",
                    "2026-09-01T10:30",
                    10,
                    100,
                    Some("changed"),
                ),
                LOCAL_CORRECTION_ACTOR_ID,
                "edit",
                None,
            )
            .unwrap();

        let reverted = repository
            .revert_correction_to_raw(
                raw.id,
                LOCAL_CORRECTION_ACTOR_ID,
                "revert",
                Some("Restore source interpretation"),
            )
            .unwrap();

        assert!(reverted.event_id.is_some());
        assert_eq!(repository.correction_history(raw.id).unwrap().len(), 2);
        assert_eq!(
            repository
                .correction_history(raw.id)
                .unwrap()
                .last()
                .unwrap()
                .action_type,
            "revert"
        );
        let effective = repository.get_all_effective().unwrap().remove(0);
        assert_eq!(effective.effective.start_time, "2026-09-01T09:00");
        assert_eq!(effective.effective.end_time, "2026-09-01T10:00");
        assert_eq!(effective.effective.break_minutes, raw.break_minutes);
        assert_eq!(effective.effective.worked_minutes, raw.worked_minutes);
        assert_eq!(effective.effective.notes, raw.notes);
        assert_eq!(repository.get_raw_by_id(raw.id).unwrap(), Some(raw));
    }

    #[test]
    fn identical_effective_correction_is_noop() {
        let repository = create_test_repository();
        let raw = insert_valid_imported(&repository);
        let unchanged = proposal(
            &raw.start_time,
            &raw.end_time,
            raw.break_minutes,
            raw.worked_minutes,
            raw.notes.as_deref(),
        );

        let result = repository
            .append_correction(raw.id, &unchanged, LOCAL_CORRECTION_ACTOR_ID, "no-op", None)
            .unwrap();

        assert_eq!(result.event_id, None);
        assert!(repository.correction_history(raw.id).unwrap().is_empty());
        let changed = proposal(
            "2026-09-01T09:05",
            "2026-09-01T10:05",
            7,
            49,
            Some("corrected"),
        );
        assert!(repository
            .append_correction(raw.id, &changed, LOCAL_CORRECTION_ACTOR_ID, "change", None,)
            .unwrap()
            .event_id
            .is_some());
        assert_eq!(
            repository
                .append_correction(
                    raw.id,
                    &changed,
                    LOCAL_CORRECTION_ACTOR_ID,
                    "repeat",
                    Some("this reason alone is not a substantive change"),
                )
                .unwrap()
                .event_id,
            None
        );
        assert_eq!(repository.correction_history(raw.id).unwrap().len(), 1);
        assert_eq!(repository.get_raw_by_id(raw.id).unwrap(), Some(raw));
    }

    #[test]
    fn invalid_correction_and_audit_insert_failure_leave_no_partial_event() {
        let repository = create_test_repository();
        let raw = insert_valid_imported(&repository);
        assert!(matches!(
            repository.append_correction(
                raw.id,
                &proposal("2026-09-01T10:00", "2026-09-01T09:00", 0, 60, None),
                LOCAL_CORRECTION_ACTOR_ID,
                "invalid",
                None,
            ),
            Err(TimesheetCorrectionError::EndNotAfterStart)
        ));
        assert!(matches!(
            repository.append_correction(
                raw.id,
                &proposal("2026-09-01T09:00", "2026-09-01T10:00", -1, 60, None),
                LOCAL_CORRECTION_ACTOR_ID,
                "invalid",
                None,
            ),
            Err(TimesheetCorrectionError::NegativeBreak)
        ));
        assert!(matches!(
            repository.append_correction(
                raw.id,
                &proposal("2026-09-01T09:00", "2026-09-01T10:00", 0, -1, None),
                LOCAL_CORRECTION_ACTOR_ID,
                "invalid",
                None,
            ),
            Err(TimesheetCorrectionError::NegativeWorked)
        ));
        repository
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_timesheet_correction
                 BEFORE INSERT ON timesheet_correction_events
                 BEGIN
                    SELECT RAISE(FAIL, 'injected correction failure');
                 END;",
            )
            .unwrap();
        assert!(repository
            .append_correction(
                raw.id,
                &proposal("2026-09-01T09:05", "2026-09-01T10:05", 0, 59, None),
                LOCAL_CORRECTION_ACTOR_ID,
                "failed",
                None,
            )
            .is_err());
        assert!(repository.correction_history(raw.id).unwrap().is_empty());
        assert_eq!(repository.get_raw_by_id(raw.id).unwrap(), Some(raw));
    }
}
