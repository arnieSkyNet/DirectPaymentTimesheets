//! Reconciliation of separate original evidence sources, before rate allocation.
use crate::app::Application;
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn now() -> String {
    chrono::Local::now().to_rfc3339()
}
pub fn open(app: &Application) -> Result<Connection> {
    Ok(Connection::open(&app.context.environment.database_path)?)
}
pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    // Widen only the nullable-rate exception for explicit reconciliation items.
    // Keep all original snapshot IDs/data and the imported identity uniqueness.
    tx.execute_batch("ALTER TABLE payroll_timesheet_worked_item_snapshots RENAME TO payroll_worked_items_before_28;
        DROP INDEX payroll_snapshot_raw_shift; DROP INDEX payroll_snapshot_timesheet_id;
        CREATE TABLE payroll_timesheet_worked_item_snapshots (
            id INTEGER PRIMARY KEY, payroll_timesheet_id INTEGER NOT NULL, week_number INTEGER NOT NULL,
            source_type TEXT NOT NULL, timesheet_id INTEGER, work_date TEXT, worked_minutes INTEGER NOT NULL,
            pay_rate_id INTEGER, pay_rate_effective_date TEXT, total_hourly_rate REAL, reason TEXT, captured_at TEXT NOT NULL,
            direct_shift_id INTEGER, source_evidence TEXT,
            CHECK(timesheet_id IS NULL OR direct_shift_id IS NULL),
            CHECK((source_type='legacy_previous_cycle_adjustment' AND timesheet_id IS NULL AND direct_shift_id IS NULL AND work_date IS NULL AND pay_rate_id IS NULL AND pay_rate_effective_date IS NULL AND total_hourly_rate IS NULL) OR source_type='carry_correction' OR
                (source_type<>'legacy_previous_cycle_adjustment' AND pay_rate_id IS NOT NULL AND pay_rate_effective_date IS NOT NULL AND total_hourly_rate IS NOT NULL))
        );
        INSERT INTO payroll_timesheet_worked_item_snapshots SELECT *,NULL,NULL FROM payroll_worked_items_before_28;
        DROP TABLE payroll_worked_items_before_28;
        CREATE UNIQUE INDEX payroll_snapshot_raw_shift ON payroll_timesheet_worked_item_snapshots(payroll_timesheet_id,timesheet_id) WHERE timesheet_id IS NOT NULL;
        CREATE UNIQUE INDEX payroll_snapshot_direct_shift ON payroll_timesheet_worked_item_snapshots(payroll_timesheet_id,direct_shift_id) WHERE direct_shift_id IS NOT NULL;
        CREATE INDEX payroll_snapshot_timesheet_id ON payroll_timesheet_worked_item_snapshots(timesheet_id);")?;
    tx.execute_batch(include_str!("payroll_evidence/schema.sql"))?;
    // Preserve only existing facts. Old snapshots have no exact clock evidence.
    tx.execute_batch("INSERT INTO payroll_submissions (payroll_timesheet_id,submitted_at,pdf_path,pdf_sha256)
        SELECT payroll_timesheet_id,submitted_at,pdf_path,pdf_sha256
        FROM payroll_timesheet_snapshot_states WHERE state='submitted';
        INSERT INTO payroll_submission_items SELECT s.id,i.* FROM payroll_timesheet_worked_item_snapshots i JOIN payroll_submissions s ON s.payroll_timesheet_id=i.payroll_timesheet_id;
        INSERT INTO payroll_submission_weeks SELECT s.id,w.* FROM payroll_timesheet_weeks w JOIN payroll_submissions s ON s.payroll_timesheet_id=w.payroll_timesheet_id;
        INSERT INTO payroll_submission_leave SELECT s.id,w.* FROM payroll_timesheet_annual_leave w JOIN payroll_submissions s ON s.payroll_timesheet_id=w.payroll_timesheet_id;
        INSERT INTO payroll_submission_holidays SELECT s.id,w.* FROM payroll_timesheet_public_holidays w JOIN payroll_submissions s ON s.payroll_timesheet_id=w.payroll_timesheet_id;
        UPDATE schema_version SET version=28;")?;
    tx.commit()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkEvidence {
    pub source: String,
    pub id: i64,
    pub pa: i64,
    pub pa_name: String,
    pub start: String,
    pub end: String,
    pub break_minutes: i64,
    pub minutes: i64,
    pub notes: String,
    pub deleted: bool,
}
impl WorkEvidence {
    pub fn key(&self) -> String {
        format!("{}:{}", self.source, self.id)
    }
    pub fn start_time(&self) -> Result<NaiveDateTime> {
        parse_clock(&self.start).ok_or_else(|| "Invalid evidence start".into())
    }
    pub fn end_time(&self) -> Result<NaiveDateTime> {
        parse_clock(&self.end).ok_or_else(|| "Invalid evidence end".into())
    }
    pub fn date(&self) -> Result<NaiveDate> {
        self.start_time().map(|d| d.date()).or_else(|_| {
            crate::models::parse_employment_date(&self.start)
                .ok_or_else(|| "Invalid evidence date".into())
        })
    }
    pub fn fingerprint(&self) -> String {
        hash(&format!(
            "{:?}",
            (
                &self.source,
                self.id,
                self.pa,
                &self.start,
                &self.end,
                self.break_minutes,
                self.minutes,
                self.deleted
            )
        ))
    }
    pub fn label(&self) -> &'static str {
        if self.source == "imported" {
            "Hours Keeper"
        } else {
            "Hours Shift"
        }
    }
}
fn parse_clock(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M").ok())
        .or_else(|| crate::csv_import::parse_supported_timestamp(value))
}
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
pub fn load(app: &Application) -> Result<Vec<WorkEvidence>> {
    load_connection(&open(app)?)
}
pub fn load_connection(db: &Connection) -> Result<Vec<WorkEvidence>> {
    let normalise = |value: String| {
        parse_clock(&value)
            .map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string())
            .unwrap_or(value)
    };
    let mut result = Vec::new();
    let mut stmt=db.prepare("SELECT t.id,t.personal_assistant_id,t.pa_name,
        COALESCE(c.after_start_time,t.start_time),COALESCE(c.after_end_time,t.end_time),
        COALESCE(c.after_break_minutes,t.break_minutes),COALESCE(c.after_worked_minutes,t.worked_minutes),
        CASE WHEN c.id IS NULL THEN t.notes ELSE c.after_notes END
        FROM timesheets t LEFT JOIN timesheet_correction_events c ON c.id=(SELECT MAX(id) FROM timesheet_correction_events WHERE timesheet_id=t.id)
        WHERE t.personal_assistant_id IS NOT NULL ORDER BY t.id")?;
    for row in stmt.query_map([], |r| {
        Ok(WorkEvidence {
            source: "imported".into(),
            id: r.get(0)?,
            pa: r.get(1)?,
            pa_name: r.get(2)?,
            start: normalise(r.get(3)?),
            end: normalise(r.get(4)?),
            break_minutes: r.get(5)?,
            minutes: r.get(6)?,
            notes: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            deleted: false,
        })
    })? {
        result.push(row?);
    }
    let mut stmt=db.prepare("SELECT d.id,d.personal_assistant_id,COALESCE(p.first_name||' '||p.surname,''),d.start_time,d.end_time,d.break_minutes,d.notes,d.deleted_at FROM direct_shifts d LEFT JOIN personal_assistants p ON p.id=d.personal_assistant_id WHERE d.end_time IS NOT NULL ORDER BY d.id")?;
    for row in stmt.query_map([], |r| {
        Ok(WorkEvidence {
            source: "direct".into(),
            id: r.get(0)?,
            pa: r.get(1)?,
            pa_name: r.get(2)?,
            start: normalise(r.get(3)?),
            end: normalise(r.get(4)?),
            break_minutes: r.get(5)?,
            minutes: 0,
            notes: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
            deleted: r.get::<_, Option<String>>(7)?.is_some(),
        })
    })? {
        let mut e = row?;
        e.minutes = crate::direct_shift_repository::validate_completion(
            e.start_time()?,
            e.end_time()?,
            e.break_minutes,
        )?;
        result.push(e);
    }
    for e in &result {
        e.date()?;
        if e.minutes < 0 {
            return Err("Invalid negative source worked duration".into());
        }
    }
    Ok(result)
}
#[derive(Clone, Debug)]
pub struct DuplicateGroup {
    pub fingerprint: String,
    pub candidates: Vec<WorkEvidence>,
}
pub fn groups(evidence: &[WorkEvidence]) -> Result<Vec<DuplicateGroup>> {
    let mut by_day: BTreeMap<(i64, NaiveDate), Vec<WorkEvidence>> = BTreeMap::new();
    for e in evidence.iter().filter(|e| !e.deleted) {
        if e.end_time()
            .ok()
            .zip(e.start_time().ok())
            .is_some_and(|(end, start)| end > start)
        {
            by_day.entry((e.pa, e.date()?)).or_default().push(e.clone());
        }
    }
    let mut result = Vec::new();
    for (_, mut rows) in by_day {
        rows.sort_by_key(|e| (e.start.clone(), e.key()));
        let mut component = Vec::new();
        let mut max_end = None;
        for e in rows {
            let start = e.start_time()?;
            if max_end.is_some_and(|end| start >= end) {
                push_group(&mut result, &mut component);
                max_end = None;
            }
            max_end = Some(max_end.map_or(e.end_time()?, |end: NaiveDateTime| {
                end.max(e.end_time().unwrap())
            }));
            component.push(e);
        }
        push_group(&mut result, &mut component);
    }
    Ok(result)
}
fn push_group(out: &mut Vec<DuplicateGroup>, component: &mut Vec<WorkEvidence>) {
    if component.len() > 1 {
        component.sort_by_key(WorkEvidence::key);
        out.push(DuplicateGroup {
            fingerprint: hash(
                &component
                    .iter()
                    .map(WorkEvidence::fingerprint)
                    .collect::<Vec<_>>()
                    .join("|"),
            ),
            candidates: std::mem::take(component),
        });
    } else {
        component.clear();
    }
}
pub struct Preflight {
    pub unresolved: Vec<DuplicateGroup>,
    pub eligible: Vec<WorkEvidence>,
}
pub fn preflight(db: &Connection, evidence: &[WorkEvidence]) -> Result<Preflight> {
    let groups = groups(evidence)?;
    let active: HashSet<_> = groups.iter().map(|g| g.fingerprint.clone()).collect();
    let tx = db.unchecked_transaction()?;
    let decisions = tx.prepare("SELECT id,group_fingerprint FROM payroll_duplicate_decisions WHERE invalidated_at IS NULL")?
        .query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, fingerprint) in decisions {
        if !active.contains(&fingerprint) {
            tx.execute("UPDATE payroll_duplicate_decisions SET invalidated_at=?1,invalidation_reason='Material evidence or overlapping membership changed' WHERE id=?2",params![now(),id])?;
        }
    }
    let mut excluded = HashSet::new();
    let mut unresolved = Vec::new();
    for group in groups {
        let winner: Option<(String,i64)> = tx.query_row("SELECT winner_source,winner_id FROM payroll_duplicate_decisions WHERE group_fingerprint=?1 AND invalidated_at IS NULL",[&group.fingerprint],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        for e in &group.candidates {
            if winner
                .as_ref()
                .is_none_or(|(source, id)| source != &e.source || *id != e.id)
            {
                excluded.insert(e.key());
            }
        }
        if winner.is_none() {
            unresolved.push(group);
        }
    }
    tx.commit()?;
    Ok(Preflight {
        unresolved,
        eligible: evidence
            .iter()
            .filter(|e| !e.deleted && !excluded.contains(&e.key()))
            .cloned()
            .collect(),
    })
}
pub fn resolve(
    db: &Connection,
    evidence: &[WorkEvidence],
    selections: &[(String, String)],
) -> Result<()> {
    let current = groups(evidence)?;
    let tx = db.unchecked_transaction()?;
    for (fingerprint, key) in selections {
        let group = current
            .iter()
            .find(|g| &g.fingerprint == fingerprint)
            .ok_or("Evidence changed; review refreshed candidates")?;
        let winner = group
            .candidates
            .iter()
            .find(|e| &e.key() == key)
            .ok_or("Select exactly one candidate per group")?;
        tx.execute("INSERT INTO payroll_duplicate_decisions(group_fingerprint,winner_source,winner_id,decided_at,actor) VALUES (?1,?2,?3,?4,'local_employer')",params![fingerprint,winner.source,winner.id,now()])?;
        let id = tx.last_insert_rowid();
        for e in &group.candidates {
            tx.execute(
                "INSERT INTO payroll_duplicate_members VALUES (?1,?2,?3,?4,?5)",
                params![id, e.source, e.id, e.fingerprint(), toml::to_string(e)?],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub mod duplicate_ui;
pub mod lifecycle;
pub mod reconciliation;
pub mod review_ui;
#[cfg(test)]
mod tests;

pub mod legacy_baseline;
