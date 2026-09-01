use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension};
use std::fmt;

pub const DIRECT_SHIFT_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M";
pub const LOCAL_ACTOR_ID: &str = "local_employer";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectShift {
    pub id: i64,
    pub personal_assistant_id: i64,
    pub start_time: String,
    pub end_time: Option<String>,
    pub break_minutes: i64,
    pub notes: Option<String>,
    pub source_type: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub deleted_by: Option<String>,
}

impl DirectShift {
    pub fn start(&self) -> Result<NaiveDateTime, DirectShiftError> {
        parse_shift_time(&self.start_time)
    }

    pub fn end(&self) -> Result<Option<NaiveDateTime>, DirectShiftError> {
        self.end_time.as_deref().map(parse_shift_time).transpose()
    }

    pub fn worked_minutes(&self) -> Result<Option<i64>, DirectShiftError> {
        let Some(end) = self.end()? else {
            return Ok(None);
        };
        Ok(Some(validate_completion(
            self.start()?,
            end,
            self.break_minutes,
        )?))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct DirectShiftAudit {
    pub id: i64,
    pub direct_shift_id: i64,
    pub actor_id: String,
    pub action_type: String,
    pub action_at: String,
    pub before_start_time: Option<String>,
    pub before_end_time: Option<String>,
    pub before_break_minutes: Option<i64>,
    pub before_notes: Option<String>,
    pub before_deleted_at: Option<String>,
    pub after_start_time: Option<String>,
    pub after_end_time: Option<String>,
    pub after_break_minutes: Option<i64>,
    pub after_notes: Option<String>,
    pub after_deleted_at: Option<String>,
}

#[derive(Debug)]
pub enum DirectShiftError {
    Database(rusqlite::Error),
    InvalidTimestamp(String),
    EndBeforeStart,
    InvalidBreak,
    AlreadyRunning,
    NotRunning,
    NotCompleted,
    NotFound,
    Deleted,
}

impl fmt::Display for DirectShiftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::InvalidTimestamp(value) => write!(formatter, "invalid shift date/time: {value}"),
            Self::EndBeforeStart => {
                write!(formatter, "clock-out time cannot precede clock-in time")
            }
            Self::InvalidBreak => write!(
                formatter,
                "break minutes cannot be negative or exceed the shift duration"
            ),
            Self::AlreadyRunning => write!(
                formatter,
                "this Personal Assistant already has a running shift"
            ),
            Self::NotRunning => write!(formatter, "the shift is no longer running"),
            Self::NotCompleted => {
                write!(formatter, "only a completed shift can be edited or deleted")
            }
            Self::NotFound => write!(formatter, "the direct shift was not found"),
            Self::Deleted => write!(formatter, "the direct shift has already been deleted"),
        }
    }
}

impl std::error::Error for DirectShiftError {}
impl From<rusqlite::Error> for DirectShiftError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

pub struct DirectShiftRepository {
    connection: Connection,
}

impl DirectShiftRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn clock_in(
        &self,
        pa_id: i64,
        start: NaiveDateTime,
        at: &str,
    ) -> Result<DirectShift, DirectShiftError> {
        let tx = self.connection.unchecked_transaction()?;
        if get_running_on(&tx, pa_id)?.is_some() {
            return Err(DirectShiftError::AlreadyRunning);
        }
        tx.execute(
            "INSERT INTO direct_shifts (personal_assistant_id,start_time,end_time,break_minutes,notes,source_type,created_at,updated_at,deleted_at,deleted_by)
             VALUES (?1,?2,NULL,0,NULL,'direct',?3,?3,NULL,NULL)",
            params![pa_id, format_shift_time(start), at],
        )?;
        let id = tx.last_insert_rowid();
        let after = get_on(&tx, id)?.ok_or(DirectShiftError::NotFound)?;
        insert_audit(&tx, id, "clock_in", at, None, Some(&after))?;
        tx.commit()?;
        Ok(after)
    }

    pub fn complete(
        &self,
        id: i64,
        end: NaiveDateTime,
        break_minutes: i64,
        notes: Option<&str>,
        at: &str,
    ) -> Result<DirectShift, DirectShiftError> {
        let tx = self.connection.unchecked_transaction()?;
        let before = get_on(&tx, id)?.ok_or(DirectShiftError::NotRunning)?;
        ensure_current(&before)?;
        if before.end_time.is_some() {
            return Err(DirectShiftError::NotRunning);
        }
        validate_completion(before.start()?, end, break_minutes)?;
        tx.execute(
            "UPDATE direct_shifts SET end_time=?1,break_minutes=?2,notes=?3,updated_at=?4
             WHERE id=?5 AND end_time IS NULL AND deleted_at IS NULL",
            params![
                format_shift_time(end),
                break_minutes,
                normalized_note(notes),
                at,
                id
            ],
        )?;
        let after = get_on(&tx, id)?.ok_or(DirectShiftError::NotRunning)?;
        insert_audit(&tx, id, "clock_out", at, Some(&before), Some(&after))?;
        tx.commit()?;
        Ok(after)
    }

    pub fn save_running_notes(
        &self,
        id: i64,
        notes: Option<&str>,
        at: &str,
    ) -> Result<(), DirectShiftError> {
        let tx = self.connection.unchecked_transaction()?;
        let before = get_on(&tx, id)?.ok_or(DirectShiftError::NotRunning)?;
        ensure_current(&before)?;
        if before.end_time.is_some() {
            return Err(DirectShiftError::NotRunning);
        }
        tx.execute("UPDATE direct_shifts SET notes=?1,updated_at=?2 WHERE id=?3 AND end_time IS NULL AND deleted_at IS NULL", params![normalized_note(notes),at,id])?;
        let after = get_on(&tx, id)?.ok_or(DirectShiftError::NotRunning)?;
        insert_audit(&tx, id, "edit", at, Some(&before), Some(&after))?;
        tx.commit()?;
        Ok(())
    }

    pub fn edit_completed(
        &self,
        id: i64,
        start: NaiveDateTime,
        end: NaiveDateTime,
        break_minutes: i64,
        notes: Option<&str>,
        at: &str,
    ) -> Result<DirectShift, DirectShiftError> {
        validate_completion(start, end, break_minutes)?;
        let tx = self.connection.unchecked_transaction()?;
        let before = get_on(&tx, id)?.ok_or(DirectShiftError::NotFound)?;
        ensure_current(&before)?;
        if before.end_time.is_none() {
            return Err(DirectShiftError::NotCompleted);
        }
        let start_time = format_shift_time(start);
        let end_time = format_shift_time(end);
        let notes = normalized_note(notes);
        if before.start_time == start_time
            && before.end_time.as_deref() == Some(end_time.as_str())
            && before.break_minutes == break_minutes
            && before.notes == notes
        {
            tx.commit()?;
            return Ok(before);
        }
        tx.execute(
            "UPDATE direct_shifts SET start_time=?1,end_time=?2,break_minutes=?3,notes=?4,updated_at=?5
             WHERE id=?6 AND end_time IS NOT NULL AND deleted_at IS NULL",
            params![start_time,end_time,break_minutes,notes,at,id],
        )?;
        let after = get_on(&tx, id)?.ok_or(DirectShiftError::NotFound)?;
        insert_audit(&tx, id, "edit", at, Some(&before), Some(&after))?;
        tx.commit()?;
        Ok(after)
    }

    pub fn soft_delete_completed(&self, id: i64, at: &str) -> Result<(), DirectShiftError> {
        let tx = self.connection.unchecked_transaction()?;
        let before = get_on(&tx, id)?.ok_or(DirectShiftError::NotFound)?;
        ensure_current(&before)?;
        if before.end_time.is_none() {
            return Err(DirectShiftError::NotCompleted);
        }
        tx.execute("UPDATE direct_shifts SET deleted_at=?1,deleted_by=?2,updated_at=?1 WHERE id=?3 AND end_time IS NOT NULL AND deleted_at IS NULL", params![at,LOCAL_ACTOR_ID,id])?;
        let after = get_on(&tx, id)?.ok_or(DirectShiftError::NotFound)?;
        insert_audit(&tx, id, "delete", at, Some(&before), Some(&after))?;
        tx.commit()?;
        Ok(())
    }

    pub fn undo_clock_in(&self, id: i64, at: &str) -> Result<(), DirectShiftError> {
        let tx = self.connection.unchecked_transaction()?;
        let before = get_on(&tx, id)?.ok_or(DirectShiftError::NotRunning)?;
        ensure_current(&before)?;
        if before.end_time.is_some() {
            return Err(DirectShiftError::NotRunning);
        }
        insert_audit(&tx, id, "cancel_clock_in", at, Some(&before), None)?;
        if tx.execute(
            "DELETE FROM direct_shifts WHERE id=?1 AND end_time IS NULL AND deleted_at IS NULL",
            params![id],
        )? != 1
        {
            return Err(DirectShiftError::NotRunning);
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_running_for_pa(&self, pa_id: i64) -> Result<Option<DirectShift>, DirectShiftError> {
        Ok(get_running_on(&self.connection, pa_id)?)
    }

    pub fn recent_for_pa(
        &self,
        pa_id: i64,
        limit: i64,
    ) -> Result<Vec<DirectShift>, DirectShiftError> {
        let mut statement = self.connection.prepare(
            "SELECT id,personal_assistant_id,start_time,end_time,break_minutes,notes,source_type,created_at,updated_at,deleted_at,deleted_by
             FROM direct_shifts WHERE personal_assistant_id=?1 AND deleted_at IS NULL ORDER BY start_time DESC,id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![pa_id, limit], direct_shift_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    #[allow(dead_code)]
    pub fn get_including_deleted(&self, id: i64) -> Result<Option<DirectShift>, DirectShiftError> {
        Ok(get_on(&self.connection, id)?)
    }

    #[allow(dead_code)]
    pub fn audit_for_shift(&self, id: i64) -> Result<Vec<DirectShiftAudit>, DirectShiftError> {
        let mut statement = self.connection.prepare(
            "SELECT id,direct_shift_id,actor_id,action_type,action_at,before_start_time,before_end_time,before_break_minutes,before_notes,before_deleted_at,after_start_time,after_end_time,after_break_minutes,after_notes,after_deleted_at
             FROM direct_shift_audit WHERE direct_shift_id=?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![id], |row| {
            Ok(DirectShiftAudit {
                id: row.get(0)?,
                direct_shift_id: row.get(1)?,
                actor_id: row.get(2)?,
                action_type: row.get(3)?,
                action_at: row.get(4)?,
                before_start_time: row.get(5)?,
                before_end_time: row.get(6)?,
                before_break_minutes: row.get(7)?,
                before_notes: row.get(8)?,
                before_deleted_at: row.get(9)?,
                after_start_time: row.get(10)?,
                after_end_time: row.get(11)?,
                after_break_minutes: row.get(12)?,
                after_notes: row.get(13)?,
                after_deleted_at: row.get(14)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

pub fn validate_completion(
    start: NaiveDateTime,
    end: NaiveDateTime,
    break_minutes: i64,
) -> Result<i64, DirectShiftError> {
    if end < start {
        return Err(DirectShiftError::EndBeforeStart);
    }
    if break_minutes < 0 {
        return Err(DirectShiftError::InvalidBreak);
    }
    end.signed_duration_since(start)
        .num_minutes()
        .checked_sub(break_minutes)
        .filter(|v| *v >= 0)
        .ok_or(DirectShiftError::InvalidBreak)
}

pub fn format_shift_time(value: NaiveDateTime) -> String {
    value.format(DIRECT_SHIFT_TIME_FORMAT).to_string()
}
pub fn parse_shift_time(value: &str) -> Result<NaiveDateTime, DirectShiftError> {
    NaiveDateTime::parse_from_str(value, DIRECT_SHIFT_TIME_FORMAT)
        .map_err(|_| DirectShiftError::InvalidTimestamp(value.to_string()))
}
fn ensure_current(shift: &DirectShift) -> Result<(), DirectShiftError> {
    if shift.deleted_at.is_some() {
        Err(DirectShiftError::Deleted)
    } else {
        Ok(())
    }
}
fn normalized_note(note: Option<&str>) -> Option<String> {
    note.map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn get_running_on(connection: &Connection, pa_id: i64) -> rusqlite::Result<Option<DirectShift>> {
    connection.query_row(
        "SELECT id,personal_assistant_id,start_time,end_time,break_minutes,notes,source_type,created_at,updated_at,deleted_at,deleted_by
         FROM direct_shifts WHERE personal_assistant_id=?1 AND end_time IS NULL AND deleted_at IS NULL",
        params![pa_id],direct_shift_from_row,
    ).optional()
}
fn get_on(connection: &Connection, id: i64) -> rusqlite::Result<Option<DirectShift>> {
    connection.query_row(
        "SELECT id,personal_assistant_id,start_time,end_time,break_minutes,notes,source_type,created_at,updated_at,deleted_at,deleted_by FROM direct_shifts WHERE id=?1",
        params![id],direct_shift_from_row,
    ).optional()
}
fn insert_audit(
    connection: &Connection,
    id: i64,
    action: &str,
    at: &str,
    before: Option<&DirectShift>,
    after: Option<&DirectShift>,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO direct_shift_audit (direct_shift_id,actor_id,action_type,action_at,before_personal_assistant_id,before_start_time,before_end_time,before_break_minutes,before_notes,before_updated_at,before_deleted_at,before_deleted_by,after_personal_assistant_id,after_start_time,after_end_time,after_break_minutes,after_notes,after_updated_at,after_deleted_at,after_deleted_by)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        params![id,LOCAL_ACTOR_ID,action,at,
            before.map(|v|v.personal_assistant_id),before.map(|v|v.start_time.as_str()),before.and_then(|v|v.end_time.as_deref()),before.map(|v|v.break_minutes),before.and_then(|v|v.notes.as_deref()),before.map(|v|v.updated_at.as_str()),before.and_then(|v|v.deleted_at.as_deref()),before.and_then(|v|v.deleted_by.as_deref()),
            after.map(|v|v.personal_assistant_id),after.map(|v|v.start_time.as_str()),after.and_then(|v|v.end_time.as_deref()),after.map(|v|v.break_minutes),after.and_then(|v|v.notes.as_deref()),after.map(|v|v.updated_at.as_str()),after.and_then(|v|v.deleted_at.as_deref()),after.and_then(|v|v.deleted_by.as_deref())],
    )?;
    Ok(())
}
fn direct_shift_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DirectShift> {
    Ok(DirectShift {
        id: row.get(0)?,
        personal_assistant_id: row.get(1)?,
        start_time: row.get(2)?,
        end_time: row.get(3)?,
        break_minutes: row.get(4)?,
        notes: row.get(5)?,
        source_type: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        deleted_at: row.get(9)?,
        deleted_by: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn repository() -> DirectShiftRepository {
        let c = Connection::open_in_memory().unwrap();
        crate::database::create_schema(&c).unwrap();
        DirectShiftRepository::new(c)
    }
    fn time(v: &str) -> NaiveDateTime {
        parse_shift_time(v).unwrap()
    }
    fn completed(r: &DirectShiftRepository) -> DirectShift {
        let s = r.clock_in(1, time("2026-09-01T08:07"), "created").unwrap();
        r.complete(
            s.id,
            time("2026-09-01T09:17"),
            10,
            Some("Visit"),
            "completed",
        )
        .unwrap()
    }

    #[test]
    fn creation_and_completion_append_actor_and_before_after_audit() {
        let r = repository();
        let s = completed(&r);
        let a = r.audit_for_shift(s.id).unwrap();
        assert_eq!(
            a.iter().map(|v| v.action_type.as_str()).collect::<Vec<_>>(),
            vec!["clock_in", "clock_out"]
        );
        assert!(a[0].before_start_time.is_none());
        assert_eq!(a[0].after_start_time.as_deref(), Some("2026-09-01T08:07"));
        assert_eq!(a[1].after_end_time.as_deref(), Some("2026-09-01T09:17"));
        assert!(a.iter().all(|v| v.actor_id == LOCAL_ACTOR_ID));
    }
    #[test]
    fn running_shift_survives_reopen_and_one_running_rule_remains() {
        let d = tempfile::TempDir::new().unwrap();
        let p = d.path().join("d.sqlite");
        let c = Connection::open(&p).unwrap();
        crate::database::create_schema(&c).unwrap();
        let r = DirectShiftRepository::new(c);
        let s = r.clock_in(3, time("2026-09-01T08:23"), "created").unwrap();
        assert!(matches!(
            r.clock_in(3, time("2026-09-01T08:24"), "again"),
            Err(DirectShiftError::AlreadyRunning)
        ));
        drop(r);
        let reopened = DirectShiftRepository::new(Connection::open(p).unwrap());
        assert_eq!(reopened.get_running_for_pa(3).unwrap().unwrap().id, s.id);
    }
    #[test]
    fn edits_preserve_before_after_and_append() {
        let r = repository();
        let s = completed(&r);
        r.edit_completed(
            s.id,
            time("2026-09-01T08:00"),
            time("2026-09-01T09:30"),
            15,
            Some("Corrected"),
            "edit-1",
        )
        .unwrap();
        r.edit_completed(
            s.id,
            time("2026-09-01T08:05"),
            time("2026-09-01T09:35"),
            5,
            Some("Final"),
            "edit-2",
        )
        .unwrap();
        let a = r.audit_for_shift(s.id).unwrap();
        assert_eq!(a.len(), 4);
        assert_eq!(a[2].before_start_time.as_deref(), Some("2026-09-01T08:07"));
        assert_eq!(a[2].after_start_time.as_deref(), Some("2026-09-01T08:00"));
        assert_eq!(a[3].before_notes.as_deref(), Some("Corrected"));
        assert_eq!(a[3].after_notes.as_deref(), Some("Final"));
    }

    #[test]
    fn unchanged_completed_edit_is_noop_without_timestamp_or_audit() {
        let r = repository();
        let before = completed(&r);
        let audit_before = r.audit_for_shift(before.id).unwrap();

        let returned = r
            .edit_completed(
                before.id,
                before.start().unwrap(),
                before.end().unwrap().unwrap(),
                before.break_minutes,
                before.notes.as_deref(),
                "must-not-be-stored",
            )
            .unwrap();

        assert_eq!(returned, before);
        assert_eq!(r.get_including_deleted(before.id).unwrap().unwrap(), before);
        assert_eq!(r.audit_for_shift(before.id).unwrap(), audit_before);
        assert_eq!(returned.updated_at, "completed");
    }
    #[test]
    fn invalid_edit_changes_neither_state_nor_audit() {
        let r = repository();
        let s = completed(&r);
        let a = r.audit_for_shift(s.id).unwrap();
        assert!(matches!(
            r.edit_completed(
                s.id,
                time("2026-09-01T10:00"),
                time("2026-09-01T09:00"),
                0,
                None,
                "bad"
            ),
            Err(DirectShiftError::EndBeforeStart)
        ));
        assert_eq!(r.get_including_deleted(s.id).unwrap().unwrap(), s);
        assert_eq!(r.audit_for_shift(s.id).unwrap(), a);
    }

    #[test]
    fn audit_insert_failure_rolls_back_the_shift_mutation() {
        let r = repository();
        let s = completed(&r);
        let audit_before = r.audit_for_shift(s.id).unwrap();
        r.connection
            .execute_batch(
                "CREATE TRIGGER reject_direct_shift_audit
                 BEFORE INSERT ON direct_shift_audit
                 BEGIN
                     SELECT RAISE(FAIL, 'injected audit failure');
                 END;",
            )
            .unwrap();

        assert!(r
            .edit_completed(
                s.id,
                time("2026-09-01T08:00"),
                time("2026-09-01T09:30"),
                0,
                Some("must roll back"),
                "failed-edit",
            )
            .is_err());
        assert_eq!(r.get_including_deleted(s.id).unwrap().unwrap(), s);
        assert_eq!(r.audit_for_shift(s.id).unwrap(), audit_before);
    }
    #[test]
    fn soft_delete_hides_but_retains_evidence() {
        let r = repository();
        let s = completed(&r);
        r.soft_delete_completed(s.id, "deleted").unwrap();
        assert!(r.recent_for_pa(1, 10).unwrap().is_empty());
        let kept = r.get_including_deleted(s.id).unwrap().unwrap();
        assert_eq!(kept.deleted_by.as_deref(), Some(LOCAL_ACTOR_ID));
        assert_eq!(
            r.audit_for_shift(s.id).unwrap().last().unwrap().action_type,
            "delete"
        );
    }
    #[test]
    fn undo_retains_creation_and_cancellation_audit() {
        let r = repository();
        let s = r.clock_in(1, time("2026-09-01T08:00"), "created").unwrap();
        r.undo_clock_in(s.id, "cancelled").unwrap();
        assert!(r.get_including_deleted(s.id).unwrap().is_none());
        let a = r.audit_for_shift(s.id).unwrap();
        assert_eq!(
            a.iter().map(|v| v.action_type.as_str()).collect::<Vec<_>>(),
            vec!["clock_in", "cancel_clock_in"]
        );
        assert_eq!(a[1].before_start_time.as_deref(), Some("2026-09-01T08:00"));
        assert!(a[1].after_start_time.is_none());
    }
    #[test]
    fn completion_and_undo_restrictions_are_enforced() {
        let r = repository();
        let s = r.clock_in(1, time("2026-09-01T08:00"), "created").unwrap();
        assert!(matches!(
            r.complete(s.id, time("2026-09-01T07:59"), 0, None, "bad"),
            Err(DirectShiftError::EndBeforeStart)
        ));
        assert!(matches!(
            r.complete(s.id, time("2026-09-01T08:03"), 4, None, "bad"),
            Err(DirectShiftError::InvalidBreak)
        ));
        let done = r
            .complete(s.id, time("2026-09-01T09:00"), 0, None, "done")
            .unwrap();
        assert_eq!(done.worked_minutes().unwrap(), Some(60));
        assert!(matches!(
            r.undo_clock_in(s.id, "cancel"),
            Err(DirectShiftError::NotRunning)
        ));
    }
}
