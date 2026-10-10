//! Stage 7 mutations share the production OS lock, then an immediate SQLite
//! transaction. Source identity and retained submission evidence never change.
use crate::payroll_evidence::{self, WorkEvidence};
use crate::repository::{
    self, EffectiveTimesheetEntry, TimesheetCorrectionAction, TimesheetCorrectionProposal,
};
use rusqlite::{params, Connection, OptionalExtension};
pub type Result<T> = payroll_evidence::Result<T>;

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE imported_hours_inclusion_events(
        id INTEGER PRIMARY KEY,timesheet_id INTEGER NOT NULL REFERENCES timesheets(id),
        before_excluded INTEGER NOT NULL CHECK(before_excluded IN (0,1)),
        after_excluded INTEGER NOT NULL CHECK(after_excluded IN (0,1)),
        reason TEXT NOT NULL CHECK(length(trim(reason))>0),actor TEXT NOT NULL,
        recorded_at TEXT NOT NULL,review_signature TEXT NOT NULL,
        before_evidence TEXT NOT NULL,after_evidence TEXT NOT NULL,
        authorised INTEGER NOT NULL CHECK(authorised IN (0,1)),
        CHECK(before_excluded<>after_excluded));
        CREATE INDEX imported_hours_inclusion_latest ON imported_hours_inclusion_events(timesheet_id,id);
        CREATE TRIGGER imported_hours_inclusion_immutable BEFORE UPDATE ON imported_hours_inclusion_events BEGIN SELECT RAISE(ABORT,'Immutable inclusion history'); END;
        CREATE TRIGGER imported_hours_inclusion_retained BEFORE DELETE ON imported_hours_inclusion_events BEGIN SELECT RAISE(ABORT,'Retained inclusion history'); END;
        CREATE TRIGGER stage7_imported_correction_immutable BEFORE UPDATE ON timesheet_correction_events BEGIN SELECT RAISE(ABORT,'Immutable imported correction history'); END;
        CREATE TRIGGER stage7_imported_correction_retained BEFORE DELETE ON timesheet_correction_events BEGIN SELECT RAISE(ABORT,'Retained imported correction history'); END;
        UPDATE schema_version SET version=40;")?;
    let tables = tx
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for table in tables {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER dpt40_{table}_{op} BEFORE {op} ON {} BEGIN SELECT CASE WHEN dpt_schema_version()<40 THEN RAISE(ABORT,'Incompatible application schema') END; END;",crate::database_recovery::quote(&table)))?;
        }
    }
    tx.commit()
}
#[cfg(test)]
pub(crate) fn remove_schema_40_fixture(db: &Connection) {
    let guards = db
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND (name LIKE 'dpt40_%' OR name LIKE 'stage7_%')")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for g in guards {
        db.execute_batch(&format!(
            "DROP TRIGGER {}",
            crate::database_recovery::quote(&g)
        ))
        .unwrap();
    }
    db.execute_batch("DROP TABLE IF EXISTS imported_hours_inclusion_events")
        .unwrap();
}
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub entry: EffectiveTimesheetEntry,
    pub inclusion_event: Option<i64>,
    pub excluded: bool,
    pub source_event: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Edit(TimesheetCorrectionProposal),
    Revert,
    Exclude,
    Restore,
}

pub fn snapshot(db: &Connection, id: i64) -> Result<Snapshot> {
    let raw = repository::get_raw_on(db, id)?.ok_or("Imported row no longer exists")?;
    let entry = repository::effective_entry_on(db, raw)?;
    let inclusion:Option<(i64,bool)>=db.query_row("SELECT id,after_excluded FROM imported_hours_inclusion_events WHERE timesheet_id=?1 ORDER BY id DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let source_event = db.query_row(
        "SELECT MAX(id) FROM shift_change_events WHERE source='imported' AND source_id=?1",
        [id],
        |r| r.get(0),
    )?;
    Ok(Snapshot {
        entry,
        inclusion_event: inclusion.map(|x| x.0),
        excluded: inclusion.is_some_and(|x| x.1),
        source_event,
    })
}
fn values(s: &Snapshot, change: &Change) -> Result<repository::EffectiveTimesheetValues> {
    Ok(match change {
        Change::Edit(p) => repository::validate_and_normalize_proposal(p)?,
        Change::Revert => {
            repository::validate_and_normalize_proposal(&TimesheetCorrectionProposal {
                start_time: s.entry.raw.start_time.clone(),
                end_time: s.entry.raw.end_time.clone(),
                break_minutes: s.entry.raw.break_minutes,
                worked_minutes: s.entry.raw.worked_minutes,
                notes: s.entry.raw.notes.clone(),
            })?
        }
        _ => s.entry.effective.clone(),
    })
}
fn evidence(s: &Snapshot, change: Option<&Change>) -> Result<WorkEvidence> {
    let v = match change {
        Some(c) => values(s, c)?,
        None => s.entry.effective.clone(),
    };
    Ok(WorkEvidence {source:"imported".into(),id:s.entry.raw.id,pa:s.entry.raw.personal_assistant_id.ok_or("This legacy row has no maintained PA identity; payroll identity review is required before changing it")?,pa_name:s.entry.raw.pa_name.clone(),start:v.start_time,end:v.end_time,break_minutes:v.break_minutes,minutes:v.worked_minutes,notes:v.notes.unwrap_or_default(),deleted:match change {Some(Change::Exclude)=>true,Some(Change::Restore)=>false,_=>s.excluded}})
}
/// Includes raw/original, effective, proposed and every retained submission cycle,
/// plus counterpart cycles whose duplicate eligibility this mutation may change.
pub fn review(
    db: &Connection,
    s: &Snapshot,
    change: &Change,
) -> Result<crate::shift_changes::Review> {
    if snapshot(db, s.entry.raw.id)? != *s {
        return Err("Imported evidence changed; refresh before reviewing".into());
    }
    let before = evidence(s, None)?;
    let after = evidence(s, Some(change))?;
    let mut ranges = vec![
        (before.start_time()?, before.end_time()?),
        (after.start_time()?, after.end_time()?),
    ];
    if let (Some(a), Some(b)) = (
        crate::csv_import::parse_supported_timestamp(&s.entry.raw.start_time),
        crate::csv_import::parse_supported_timestamp(&s.entry.raw.end_time),
    ) {
        ranges.push((a, b));
    }
    ranges.extend(crate::shift_changes::retained_ranges(
        db, "imported", before.id,
    )?);
    let current = payroll_evidence::load_for_pa(db, before.pa)?;
    let mut proposed = current.clone();
    if let Some(e) = proposed.iter_mut().find(|e| e.key() == before.key()) {
        *e = after.clone();
    }
    let mut context = String::new();
    for rows in [&current, &proposed] {
        for group in crate::shift_changes::linked_groups(db, rows)? {
            if group.candidates.iter().any(|e| e.key() == before.key()) {
                context.push_str(&format!("{group:?}"));
                let decision:Option<i64>=db.query_row("SELECT id FROM payroll_duplicate_decisions WHERE group_fingerprint=?1 AND invalidated_at IS NULL",[&group.fingerprint],|r|r.get(0)).optional()?;
                context.push_str(&format!("{decision:?}"));
                for e in group.candidates {
                    ranges.push((e.start_time()?, e.end_time()?));
                    ranges.extend(crate::shift_changes::retained_ranges(db, &e.source, e.id)?);
                }
            }
        }
    }
    // Financial reconciliation decisions/applications can change without a source
    // row edit. Pin them too, so an old approval cannot span a new obligation.
    let decisions=db.prepare("SELECT d.id,d.kind,d.evidence_key,d.decided_at FROM payroll_reconciliation_decisions d JOIN payroll_timesheets p ON p.id=d.payroll_timesheet_id WHERE p.personal_assistant_id=?1 ORDER BY d.id")?.query_map([before.pa],|r|Ok(format!("decision:{}:{}:{}:{}",r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    context.push_str(&format!("{decisions:?}"));
    let obligations=db.prepare("SELECT c.id,c.minutes,a.payroll_timesheet_id,a.minutes FROM payroll_corrections c LEFT JOIN payroll_correction_applications a ON a.correction_id=c.id WHERE c.personal_assistant_id=?1 ORDER BY c.id,a.payroll_timesheet_id")?.query_map([before.pa],|r|Ok(format!("obligation:{}:{}:{:?}:{:?}",r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,Option<i64>>(2)?,r.get::<_,Option<i64>>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    context.push_str(&format!("{obligations:?}"));
    let mut review = crate::shift_changes::review(db, before.pa, &ranges)?;
    review.signature = crate::shift_changes::digest(
        format!("{}:{s:?}:{change:?}:{context}", review.signature).as_bytes(),
    );
    Ok(review)
}

pub fn apply(
    db: &Connection,
    expected: &Snapshot,
    change: &Change,
    reason: &str,
    signature: &str,
    authorised: bool,
) -> Result<Snapshot> {
    if reason.trim().is_empty() {
        return Err("Record a reason for this change".into());
    }
    let _lock = crate::timesheet_delivery::production_lock(db)?;
    let tx = rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)?;
    let current = snapshot(&tx, expected.entry.raw.id)?;
    if current != *expected {
        return Err(
            "Imported evidence changed since it was displayed; refresh and review again".into(),
        );
    }
    let review = review(&tx, &current, change)?;
    if review.signature != signature {
        return Err("Payroll or duplicate evidence changed; review again".into());
    }
    if review.protected && !authorised {
        return Err("Explicitly authorise the displayed protected-cycle correction".into());
    }
    let before = evidence(&current, None)?;
    let after = evidence(&current, Some(change))?;
    let ids=tx.prepare("SELECT payroll_timesheet_id FROM payroll_timesheet_snapshot_states WHERE state='candidate' AND payroll_timesheet_id IN (SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1)")?.query_map([before.pa],|r|r.get::<_,i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let candidates = ids
        .into_iter()
        .map(|id| {
            Ok((
                id,
                payroll_evidence::lifecycle::candidate_signature(&tx, id)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    match change {
        Change::Edit(_) | Change::Revert => {
            let proposal = match change {
                Change::Edit(p) => Some(p),
                _ => None,
            };
            repository::append_correction_on(
                &tx,
                current.entry.raw.id,
                if matches!(change, Change::Revert) {
                    TimesheetCorrectionAction::Revert
                } else {
                    TimesheetCorrectionAction::Edit
                },
                proposal,
                repository::LOCAL_CORRECTION_ACTOR_ID,
                &payroll_evidence::now(),
                Some(reason),
                Some(signature),
            )?;
        }
        Change::Exclude | Change::Restore => {
            if before.deleted == after.deleted {
                return Err("Inclusion state already changed; refresh".into());
            }
            tx.execute("INSERT INTO imported_hours_inclusion_events(timesheet_id,before_excluded,after_excluded,reason,actor,recorded_at,review_signature,before_evidence,after_evidence,authorised) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![before.id,before.deleted,after.deleted,reason.trim(),repository::LOCAL_CORRECTION_ACTOR_ID,payroll_evidence::now(),review.signature,toml::to_string(&before)?,toml::to_string(&after)?,authorised])?;
            crate::shift_changes::event(
                &tx,
                "imported",
                before.id,
                &toml::to_string(&before)?,
                &toml::to_string(&after)?,
                reason.trim(),
                signature,
            )?;
        }
    }
    // Revalidate duplicate inventory within this transaction, retaining obsolete
    // decisions as audit. Submitted snapshots and financial obligations stay intact.
    let evidence = payroll_evidence::load_for_pa(&tx, before.pa)?;
    payroll_evidence::preflight_for_pa(&tx, &evidence, before.pa)?;
    // Dates moving out of a candidate must also invalidate its original membership.
    for (id, previous_signature) in candidates {
        // A database/validation error rolls the entire mutation back. Do not
        // treat a generic failure as evidence that an unrelated PDF is stale.
        if previous_signature != payroll_evidence::lifecycle::candidate_signature(&tx, id)? {
            tx.execute("DELETE FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1 AND state='candidate'",[id])?;
            tx.execute(
                "DELETE FROM payroll_candidate_checks WHERE payroll_timesheet_id=?1",
                [id],
            )?;
            tx.execute(
                "DELETE FROM payroll_timesheet_worked_item_snapshots WHERE payroll_timesheet_id=?1",
                [id],
            )?;
        }
    }
    let saved = snapshot(&tx, before.id)?;
    tx.commit()?;
    Ok(saved)
}

pub fn payable_status(db: &Connection, s: &Snapshot) -> Result<String> {
    if s.excluded {
        return Ok("Excluded by explicit review".into());
    }
    let e = evidence(s, None)?;
    for g in crate::shift_changes::linked_groups(db, &payroll_evidence::load_for_pa(db, e.pa)?)? {
        if g.candidates.iter().any(|c| c.key() == e.key()) {
            let winner:Option<(String,i64)>=db.query_row("SELECT winner_source,winner_id FROM payroll_duplicate_decisions WHERE group_fingerprint=?1 AND invalidated_at IS NULL",[g.fingerprint],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            return Ok(match winner {
                None => "Pending duplicate/correction review",
                Some((ref source, id))
                    if source == "separate" || (source == "imported" && id == e.id) =>
                {
                    "Eligible source evidence"
                }
                _ => "Excluded by duplicate choice",
            }
            .into());
        }
    }
    Ok("Eligible source evidence (submission membership reviewed separately)".into())
}
pub fn consistency_warning(s: &Snapshot) -> Option<String> {
    let v = &s.entry.effective;
    let parse = |s: &str| {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
            .ok()
            .or_else(|| crate::csv_import::parse_supported_timestamp(s))
    };
    let start = parse(&v.start_time)?;
    let end = parse(&v.end_time)?;
    let expected = (end - start).num_minutes() - v.break_minutes;
    (expected!=v.worked_minutes).then(||format!("Worked duration is independently authoritative: {} minutes; elapsed minus breaks is {expected} minutes. Check the source; no automatic recalculation.",v.worked_minutes))
}
#[cfg(test)]
#[path = "imported_hours_tests.rs"]
mod tests;
