//! Transactional, date-only sickness changes. Never calculates SSP or alters settlement.
use crate::payroll_evidence::{lifecycle, now, Result};
use crate::payroll_timesheet_repository::PayrollTimesheet;
use crate::sickness_period_repository::SicknessPeriod;
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Evidence {
    pub periods: Vec<SicknessPeriod>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Editable,
    EditableOnly,
    AuthorisedCorrection,
}
#[derive(Clone, Debug)]
pub struct Review {
    pub signature: String,
    pub description: String,
}

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    // Existing schema-36 compatibility triggers reject a version-37 connection.
    // Remove/reinstall them only inside this atomic migration.
    let triggers = tx
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'dpt36_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for trigger in triggers {
        tx.execute_batch(&format!("DROP TRIGGER \"{trigger}\""))?;
    }
    tx.execute_batch("ALTER TABLE personal_assistant_sickness_periods RENAME TO sickness_periods_before37;
        DROP INDEX idx_sickness_periods_pa_dates;
        CREATE TABLE personal_assistant_sickness_periods (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        personal_assistant_id INTEGER NOT NULL REFERENCES personal_assistants(id),
        start_date TEXT NOT NULL CHECK(length(start_date)=10 AND start_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
        end_date TEXT NOT NULL CHECK(length(end_date)=10 AND end_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'), CHECK(end_date>=start_date));
        INSERT INTO personal_assistant_sickness_periods SELECT * FROM sickness_periods_before37;
        DROP TABLE sickness_periods_before37;
        CREATE INDEX idx_sickness_periods_pa_dates ON personal_assistant_sickness_periods(personal_assistant_id,start_date,end_date);
        CREATE TABLE sickness_changes (
        id INTEGER PRIMARY KEY, personal_assistant_id INTEGER NOT NULL,
        period_id INTEGER, before_evidence TEXT NOT NULL, after_evidence TEXT NOT NULL,
        reason TEXT NOT NULL CHECK(length(trim(reason))>0), scope TEXT NOT NULL,
        actor TEXT NOT NULL, changed_at TEXT NOT NULL);
        CREATE TABLE sickness_cycle_overrides (
        payroll_timesheet_id INTEGER PRIMARY KEY, evidence TEXT NOT NULL);
        CREATE TABLE sickness_corrections (
        id INTEGER PRIMARY KEY, change_id INTEGER NOT NULL REFERENCES sickness_changes(id),
        payroll_timesheet_id INTEGER NOT NULL, original_submission_id INTEGER NOT NULL,
        decision_id INTEGER NOT NULL REFERENCES payroll_reconciliation_decisions(id),
        before_evidence TEXT NOT NULL, after_evidence TEXT NOT NULL,
        original_totals TEXT NOT NULL, reason TEXT NOT NULL, authorised_at TEXT NOT NULL,
        actor TEXT NOT NULL, document_id INTEGER, retired_at TEXT);
        CREATE TABLE sickness_document_evidence (
        document_id INTEGER PRIMARY KEY REFERENCES timesheet_documents(id),
        payroll_timesheet_id INTEGER NOT NULL, evidence TEXT NOT NULL,
        correction_id INTEGER REFERENCES sickness_corrections(id));
        CREATE TABLE sickness_submission_evidence (
        submission_id INTEGER PRIMARY KEY REFERENCES payroll_submissions(id),
        document_id INTEGER NOT NULL REFERENCES sickness_document_evidence(document_id),
        evidence TEXT NOT NULL, correction_id INTEGER);
        CREATE TABLE sickness_attempt_evidence (
        attempt_id INTEGER PRIMARY KEY REFERENCES timesheet_delivery_attempts(id),
        document_id INTEGER NOT NULL REFERENCES sickness_document_evidence(document_id),
        evidence TEXT NOT NULL, correction_id INTEGER);
        CREATE TRIGGER sickness_changes_no_update BEFORE UPDATE ON sickness_changes BEGIN SELECT RAISE(ABORT,'Immutable sickness audit'); END;
        CREATE TRIGGER sickness_changes_no_delete BEFORE DELETE ON sickness_changes BEGIN SELECT RAISE(ABORT,'Retained sickness audit'); END;
        CREATE TRIGGER sickness_correction_no_delete BEFORE DELETE ON sickness_corrections BEGIN SELECT RAISE(ABORT,'Retained sickness correction'); END;
        CREATE TRIGGER sickness_correction_immutable BEFORE UPDATE ON sickness_corrections
        WHEN OLD.id IS NOT NEW.id OR OLD.change_id IS NOT NEW.change_id OR OLD.payroll_timesheet_id IS NOT NEW.payroll_timesheet_id OR OLD.original_submission_id IS NOT NEW.original_submission_id OR OLD.decision_id IS NOT NEW.decision_id OR OLD.before_evidence IS NOT NEW.before_evidence OR OLD.after_evidence IS NOT NEW.after_evidence OR OLD.original_totals IS NOT NEW.original_totals OR OLD.reason IS NOT NEW.reason OR OLD.authorised_at IS NOT NEW.authorised_at OR OLD.actor IS NOT NEW.actor OR (OLD.document_id IS NOT NULL AND OLD.document_id IS NOT NEW.document_id) OR (OLD.retired_at IS NOT NULL AND OLD.retired_at IS NOT NEW.retired_at)
        BEGIN SELECT RAISE(ABORT,'Immutable sickness authorisation'); END;
        UPDATE schema_version SET version=37;")?;
    for table in [
        "sickness_document_evidence",
        "sickness_submission_evidence",
        "sickness_attempt_evidence",
    ] {
        for op in ["UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER retain_{table}_{op} BEFORE {op} ON {table} BEGIN SELECT RAISE(ABORT,'Immutable sickness evidence'); END;"))?;
        }
    }
    let tables = tx
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for table in tables {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER dpt37_{table}_{op} BEFORE {op} ON {table} BEGIN SELECT CASE WHEN dpt_schema_version()<37 THEN RAISE(ABORT,'Incompatible application schema') END; END;"))?;
        }
    }
    tx.commit()
}
fn encode(e: &Evidence) -> Result<String> {
    Ok(toml::to_string(e)?)
}
fn decode(s: &str) -> Result<Evidence> {
    Ok(toml::from_str(s)?)
}
pub fn record(db: &Connection, id: i64) -> Result<PayrollTimesheet> {
    Ok(db.query_row("SELECT id,personal_assistant_id,payroll_year,cycle_number,previous_cycle_hours,created_at,updated_at,payroll_department_notes,actual_in_lieu_hours,actual_in_lieu_updated_at FROM payroll_timesheets WHERE id=?1",[id],|r|Ok(PayrollTimesheet{id:r.get(0)?,personal_assistant_id:r.get(1)?,payroll_year:r.get(2)?,cycle_number:r.get(3)?,previous_cycle_hours:r.get(4)?,created_at:r.get(5)?,updated_at:r.get(6)?,payroll_department_notes:r.get(7)?,actual_in_lieu_hours:r.get(8)?,actual_in_lieu_updated_at:r.get(9)?}))?)
}
fn bounds(db: &Connection, id: i64) -> Result<(NaiveDate, NaiveDate)> {
    let start:String = db.query_row("SELECT s.first_week_commencing FROM payroll_timesheets p JOIN payroll_schedules s ON s.payroll_year=p.payroll_year AND s.cycle_number=p.cycle_number WHERE p.id=?1",[id],|r|r.get(0))?;
    let start = crate::date_utils::parse_legacy(&start)?;
    Ok((start, start + Duration::days(27)))
}
fn raw_evidence(db: &Connection, id: i64) -> Result<Evidence> {
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheets p JOIN payroll_schedules s ON s.payroll_year=p.payroll_year AND s.cycle_number=p.cycle_number WHERE p.id=?1)", [id], |r|r.get(0))?;
    if !exists {
        return Ok(Evidence::default());
    }
    let pa = record(db, id)?.personal_assistant_id;
    let (start, end) = bounds(db, id)?;
    let periods = db.prepare("SELECT id,personal_assistant_id,start_date,end_date FROM personal_assistant_sickness_periods WHERE personal_assistant_id=?1 AND start_date<=?3 AND end_date>=?2 ORDER BY start_date,end_date,id")?
        .query_map(params![pa,crate::date_utils::iso(start),crate::date_utils::iso(end)],|r|Ok(SicknessPeriod{id:r.get(0)?,personal_assistant_id:r.get(1)?,start_date:r.get(2)?,end_date:r.get(3)?}))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Evidence { periods })
}
pub fn evidence(db: &Connection, id: i64) -> Result<Evidence> {
    let override_: Option<String> = db
        .query_row(
            "SELECT evidence FROM sickness_cycle_overrides WHERE payroll_timesheet_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    match override_ {
        Some(s) => decode(&s),
        None => raw_evidence(db, id),
    }
}
pub fn signature(db: &Connection, id: i64) -> Result<String> {
    // Some low-level legacy fixtures intentionally have no schedule/preparation.
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheets p JOIN payroll_schedules s ON s.payroll_year=p.payroll_year AND s.cycle_number=p.cycle_number WHERE p.id=?1)",[id],|r|r.get(0))?;
    if !exists {
        return Ok("sickness:no-period".into());
    }
    Ok(format!(
        "sickness:{:x}",
        Sha256::digest(encode(&evidence(db, id)?)?)
    ))
}
pub fn projection(e: &Evidence, weeks: &[NaiveDate; 4]) -> Result<[Vec<[String; 2]>; 4]> {
    let mut result = std::array::from_fn(|_| Vec::new());
    for (i, start) in weeks.iter().enumerate() {
        for p in &e.periods {
            let a = crate::date_utils::parse_legacy(&p.start_date)?;
            let b = crate::date_utils::parse_legacy(&p.end_date)?;
            if a <= *start + Duration::days(6) && b >= *start {
                result[i].push([
                    format!("({} to", crate::date_utils::compact(&p.start_date)?),
                    format!("{})", crate::date_utils::compact(&p.end_date)?),
                ]);
            }
        }
    }
    Ok(result)
}
fn normalise(p: &SicknessPeriod) -> Result<SicknessPeriod> {
    let mut p = p.clone();
    let start = crate::date_utils::parse_input(&p.start_date)?;
    let end = crate::date_utils::parse_input(&p.end_date)?;
    if end < start {
        return Err("End date cannot precede start date".into());
    }
    p.start_date = crate::date_utils::iso(start);
    p.end_date = crate::date_utils::iso(end);
    Ok(p)
}
fn affected(
    db: &Connection,
    pa: i64,
    old: Option<&SicknessPeriod>,
    new: Option<&SicknessPeriod>,
) -> Result<Vec<PayrollTimesheet>> {
    let ids = db
        .prepare("SELECT id FROM payroll_timesheets WHERE personal_assistant_id=?1 ORDER BY id")?
        .query_map([pa], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut result = Vec::new();
    for id in ids {
        let (a, b) = bounds(db, id)?;
        if [old, new].into_iter().flatten().any(|p| {
            p.start_date <= crate::date_utils::iso(b) && p.end_date >= crate::date_utils::iso(a)
        }) {
            result.push(record(db, id)?);
        }
    }
    Ok(result)
}
fn stored(db: &Connection, id: i64) -> Result<Option<SicknessPeriod>> {
    Ok(db.query_row("SELECT id,personal_assistant_id,start_date,end_date FROM personal_assistant_sickness_periods WHERE id=?1",[id],|r|Ok(SicknessPeriod{id:r.get(0)?,personal_assistant_id:r.get(1)?,start_date:r.get(2)?,end_date:r.get(3)?})).optional()?)
}
/// A retained view may be corrected locally, or transferred into active dates.
/// Never overwrite a different PA-wide version sharing the historical identity.
fn scoped_transfer(
    db: &Connection,
    selected: i64,
    old: Option<&SicknessPeriod>,
    new: Option<&SicknessPeriod>,
) -> Result<bool> {
    let Some(old) = old else {
        return Ok(false);
    };
    let canonical = stored(db, old.id)?;
    if canonical.as_ref() == Some(old) {
        return Ok(false);
    }
    let Some(new) = new else {
        return Ok(false);
    };
    let (a, b) = bounds(db, selected)?;
    let transfer =
        new.start_date < crate::date_utils::iso(a) || new.end_date > crate::date_utils::iso(b);
    if transfer && canonical.is_some() {
        return Err("Retained sickness dates conflict with an existing PA-wide version. Transfer refused without changes; reconcile that active period first, or correct dates within this historical cycle.".into());
    }
    Ok(transfer)
}
// Legacy delivery markers can exist without a preparation record. Such a cycle
// cannot supply the retained baseline required by the correction lifecycle.
fn check_unprepared_protected_cycles(
    db: &Connection,
    pa: i64,
    old: Option<&SicknessPeriod>,
    new: Option<&SicknessPeriod>,
) -> Result<()> {
    let mut stmt=db.prepare("SELECT DISTINCT s.payroll_year,s.cycle_number,s.first_week_commencing FROM payroll_schedules s JOIN payroll_timesheet_email_status e ON e.payroll_year=s.payroll_year AND e.cycle_number=s.cycle_number WHERE e.personal_assistant_id=?1 AND e.email_type IN ('timesheet','payslip') AND e.sent_at IS NOT NULL AND NOT EXISTS(SELECT 1 FROM payroll_timesheets p WHERE p.personal_assistant_id=?1 AND p.payroll_year=s.payroll_year AND p.cycle_number=s.cycle_number)")?;
    let rows = stmt
        .query_map([pa], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (year, cycle, start) in rows {
        let start = crate::date_utils::parse_legacy(&start)?;
        let end = start + Duration::days(27);
        if [old, new].into_iter().flatten().any(|p| {
            p.start_date <= crate::date_utils::iso(end)
                && p.end_date >= crate::date_utils::iso(start)
        }) {
            return Err(format!("Protected payroll {year} period {cycle} has delivery history without a retained preparation/submission baseline; sickness change refused. Recover and review its original payroll evidence first.").into());
        }
    }
    Ok(())
}
pub fn review(
    db: &Connection,
    selected: i64,
    old: Option<&SicknessPeriod>,
    new: Option<&SicknessPeriod>,
) -> Result<Review> {
    let pa = record(db, selected)?.personal_assistant_id;
    if !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM personal_assistants WHERE id=?1)",
        [pa],
        |r| r.get::<_, bool>(0),
    )? {
        return Err("Personal Assistant not found".into());
    }
    if new.is_some_and(|p| p.id != old.map_or(0, |p| p.id)) {
        return Err("Sickness identity changed; reopen the editor".into());
    }

    if old
        .into_iter()
        .chain(new)
        .any(|p| p.personal_assistant_id != pa)
    {
        return Err("Sickness period belongs to another Personal Assistant".into());
    }
    if let Some(old) = old {
        if stored(db, old.id)?.as_ref() != Some(old)
            && !evidence(db, selected)?.periods.contains(old)
        {
            return Err("Sickness dates changed in another editor; reopen before saving".into());
        }
    }
    let new = new.map(normalise).transpose()?;
    check_unprepared_protected_cycles(db, pa, old, new.as_ref())?;
    let rows = affected(db, pa, old, new.as_ref())?;
    let transfer = scoped_transfer(db, selected, old, new.as_ref())?;

    let revision = revision(db, pa)?;
    let mut details = vec![
        format!("Sickness audit revision {revision}"),
        format!("Selected payroll record {selected}; before={old:?}; after={new:?}"),
    ];
    let mut description = Vec::new();
    if transfer {
        description.push("Transfer retained dates into every destination cycle under a new active identity; original historical identity remains retained.".into());
    }
    for r in rows {
        let stage = lifecycle::stage(db, &r)?;
        let original = lifecycle::latest(db, r.id)?;
        let doc:Option<i64>=db.query_row("SELECT document_id FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",[r.id],|r|r.get(0)).optional()?.flatten();
        if let Some(sid) = original {
            details.push(format!(
                "Original financial evidence:{}",
                original_payload(db, sid)?
            ));
        }
        details.push(format!(
            "{}:{stage:?}:{original:?}:{doc:?}:{}:{}",
            r.id,
            format!("{}:{:?}", signature(db, r.id)?, correction(db, r.id)?),
            totals(db, r.id)?
        ));
        let (a, b) = bounds(db, r.id)?;
        if stage != lifecycle::Stage::Editable {
            let has_original: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM sickness_submission_evidence WHERE submission_id=?1)",
                [original],
                |r| r.get(0),
            )?;
            if !has_original {
                description.push("Legacy submission has no captured structured sickness dates: review its original PDF and verify the previously recorded dates before authorising.".into());
            }
        }
        description.push(format!(
            "{} period {} ({} to {}): {stage:?}",
            r.payroll_year, r.cycle_number, a, b
        ));
    }
    Ok(Review {
        signature: format!("{:x}", Sha256::digest(details.join("|"))),
        description: description.join("\n"),
    })
}
fn totals(db: &Connection, id: i64) -> Result<String> {
    let r = record(db, id)?;
    let weeks=db.prepare("SELECT week_number,worked_hours,annual_leave_hours,sick_leave_hours,public_holiday_hours,travel_miles FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=?1 ORDER BY week_number")?.query_map([id],|r|Ok(format!("{}:{:?}:{:?}:{:?}:{:?}:{:?}",r.get::<_,i64>(0)?,r.get::<_,f64>(1)?,r.get::<_,f64>(2)?,r.get::<_,f64>(3)?,r.get::<_,f64>(4)?,r.get::<_,f64>(5)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(format!("{:?}|{}", r.previous_cycle_hours, weeks.join("|")))
}
/// All affected cycles, audit, scope overrides and invalidation share one writer transaction.
pub fn mutate(
    db: &Connection,
    selected: i64,
    old: Option<&SicknessPeriod>,
    new: Option<&SicknessPeriod>,
    scope: Scope,
    reason: &str,
    expected: &str,
) -> Result<Option<SicknessPeriod>> {
    let _guard = crate::timesheet_delivery::production_lock(db)?;
    let tx = db.unchecked_transaction()?;
    tx.execute("UPDATE schema_version SET version=version", [])?;
    if reason.trim().is_empty() {
        return Err("Document a sickness change reason before saving".into());
    }
    if review(&tx, selected, old, new)?.signature != expected {
        return Err("Sickness/payroll evidence changed; review again before saving".into());
    }
    let pa = record(&tx, selected)?.personal_assistant_id;
    let new = new.map(normalise).transpose()?;
    if let Some(p) = &new {
        let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM personal_assistant_sickness_periods WHERE personal_assistant_id=?1 AND start_date=?2 AND end_date=?3 AND id<>?4)",params![pa,p.start_date,p.end_date,old.map_or(0,|p|p.id)],|r|r.get(0))?;
        if duplicate {
            return Err("That exact sickness period already exists; no change saved".into());
        }
    }
    let rows = affected(&tx, pa, old, new.as_ref())?;
    let transfer = scoped_transfer(&tx, selected, old, new.as_ref())?;
    let scoped_only = old.is_some_and(|p| stored(&tx, p.id).is_ok_and(|s| s.as_ref() != Some(p)));

    if scope == Scope::EditableOnly
        && !rows
            .iter()
            .any(|r| lifecycle::stage(&tx, r).is_ok_and(|s| s == lifecycle::Stage::Editable))
    {
        return Err(
            "No editable affected cycle; use an authorised correction for protected payroll".into(),
        );
    }
    let mut before = Vec::new();
    for r in &rows {
        let stage = lifecycle::stage(&tx, r)?;
        if stage == lifecycle::Stage::Indeterminate {
            return Err("Delivery uncertain: resolve audited uncertainty before changing overlapping sickness".into());
        }
        if stage != lifecycle::Stage::Editable && scope == Scope::Editable {
            return Err("Affected payroll is protected; review an authorised sickness-only correction or explicitly limit the change to editable cycles".into());
        }
        if stage != lifecycle::Stage::Editable
            && scope == Scope::AuthorisedCorrection
            && lifecycle::latest(&tx, r.id)?.is_none()
        {
            return Err(
                "No retained original submission; recover historical evidence before correcting"
                    .into(),
            );
        }
        let prior = evidence(&tx, r.id)?;
        if transfer && old.is_some_and(|o| prior.periods.iter().any(|p| p.id == o.id && p != o)) {
            return Err("Conflicting retained sickness versions in an affected cycle; transfer refused without changes. Reconcile the cycle-specific dates first.".into());
        }
        if new.as_ref().is_some_and(|n| {
            prior.periods.iter().any(|p| {
                Some(p.id) != old.map(|p| p.id)
                    && p.start_date == n.start_date
                    && p.end_date == n.end_date
            })
        }) {
            return Err("That exact sickness period already exists in an affected cycle".into());
        }
        before.push((r, stage, prior));
    }
    let id = if transfer {
        let p = new.as_ref().ok_or("Transfer dates missing")?;
        tx.execute("INSERT INTO personal_assistant_sickness_periods(personal_assistant_id,start_date,end_date) VALUES (?1,?2,?3)",params![pa,p.start_date,p.end_date])?;
        Some(tx.last_insert_rowid())
    } else if scoped_only {
        new.as_ref().map(|p| p.id)
    } else {
        match (old, new.as_ref()) {
            (Some(old), Some(p)) => {
                tx.execute("UPDATE personal_assistant_sickness_periods SET start_date=?1,end_date=?2 WHERE id=?3",params![p.start_date,p.end_date,old.id])?;
                Some(old.id)
            }
            (Some(old), None) => {
                tx.execute(
                    "DELETE FROM personal_assistant_sickness_periods WHERE id=?1",
                    [old.id],
                )?;
                None
            }
            (None, Some(p)) => {
                tx.execute("INSERT INTO personal_assistant_sickness_periods(personal_assistant_id,start_date,end_date) SELECT id,?2,?3 FROM personal_assistants WHERE id=?1",params![pa,p.start_date,p.end_date])?;
                Some(tx.last_insert_rowid())
            }
            (None, None) => return Err("No sickness change selected".into()),
        }
    };
    let saved = if scoped_only && !transfer {
        new.clone()
    } else {
        id.map(|id| stored(&tx, id)).transpose()?.flatten()
    };
    let single = |p: Option<&SicknessPeriod>| Evidence {
        periods: p.cloned().into_iter().collect(),
    };
    tx.execute("INSERT INTO sickness_changes(personal_assistant_id,period_id,before_evidence,after_evidence,reason,scope,actor,changed_at) VALUES (?1,?2,?3,?4,?5,?6,'local_employer',?7)",params![pa,old.map(|p|p.id).or(id),encode(&single(old))?,encode(&single(saved.as_ref()))?,reason.trim(),format!("{scope:?}"),now()])?;
    let change = tx.last_insert_rowid();
    for (r, stage, prior) in before {
        if stage != lifecycle::Stage::Editable && scope == Scope::EditableOnly {
            tx.execute("INSERT INTO sickness_cycle_overrides VALUES (?1,?2) ON CONFLICT(payroll_timesheet_id) DO NOTHING",params![r.id,encode(&prior)?])?;
            continue;
        }
        // Local historical corrections retain other cycle views. Transfers publish
        // their new identity into all destinations after full-union validation.
        if scoped_only && !transfer && r.id != selected {
            continue;
        }
        let mut after = prior.clone();
        if let Some(old) = old {
            after.periods.retain(|p| p.id != old.id);
        }
        if let Some(p) = &saved {
            let (a, b) = bounds(&tx, r.id)?;
            if p.start_date <= crate::date_utils::iso(b) && p.end_date >= crate::date_utils::iso(a)
            {
                after.periods.push(p.clone());
            }
        }
        if after == prior {
            continue;
        }
        after.periods.sort_by(|a, b| {
            (&a.start_date, &a.end_date, a.id).cmp(&(&b.start_date, &b.end_date, b.id))
        });
        if stage != lifecycle::Stage::Editable {
            let sid = lifecycle::latest(&tx, r.id)?.ok_or("Original submission unavailable")?;
            let original_evidence: Option<String> = tx
                .query_row(
                    "SELECT evidence FROM sickness_submission_evidence WHERE submission_id=?1",
                    [sid],
                    |r| r.get(0),
                )
                .optional()?;
            let original_evidence = original_evidence.unwrap_or(encode(&prior)?);
            let decision = lifecycle::record_decision(
                &tx,
                r.id,
                Some(sid),
                "resubmit",
                &format!("sickness:{change}:{}", r.id),
                &format!(
                    "Authorised sickness-only correction: {}\n{}",
                    reason.trim(),
                    expected
                ),
            )?;
            tx.execute("INSERT INTO sickness_corrections(change_id,payroll_timesheet_id,original_submission_id,decision_id,before_evidence,after_evidence,original_totals,reason,authorised_at,actor) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'local_employer')",params![change,r.id,sid,decision,original_evidence,encode(&after)?,authorised_totals(&tx,r.id,sid)?,reason.trim(),now()])?;
        }
        // Store a per-cycle view only where already scoped/frozen or protected.
        let has_override: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sickness_cycle_overrides WHERE payroll_timesheet_id=?1)",
            [r.id],
            |r| r.get(0),
        )?;
        if stage != lifecycle::Stage::Editable || has_override || scoped_only {
            tx.execute("INSERT INTO sickness_cycle_overrides VALUES (?1,?2) ON CONFLICT(payroll_timesheet_id) DO UPDATE SET evidence=excluded.evidence",params![r.id,encode(&after)?])?;
        }
        // Never delete original document/submission bytes. Only unsent candidates are removed.
        tx.execute("DELETE FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1 AND state='candidate'",[r.id])?;
        tx.execute(
            "DELETE FROM payroll_candidate_checks WHERE payroll_timesheet_id=?1",
            [r.id],
        )?;
    }
    tx.commit()?;
    Ok(saved)
}

pub fn correction(db: &Connection, id: i64) -> Result<Option<i64>> {
    Ok(db.query_row("SELECT id FROM sickness_corrections WHERE payroll_timesheet_id=?1 AND retired_at IS NULL ORDER BY id DESC LIMIT 1",[id],|r|r.get(0)).optional()?)
}
pub fn correction_open(db: &Connection, id: i64) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sickness_corrections c WHERE c.payroll_timesheet_id=?1 AND c.retired_at IS NULL AND c.id=(SELECT MAX(id) FROM sickness_corrections WHERE payroll_timesheet_id=?1 AND retired_at IS NULL) AND NOT EXISTS(SELECT 1 FROM sickness_submission_evidence s WHERE s.correction_id=c.id))",[id],|r|r.get(0))?)
}
// Compare business values, ignoring row identity and publication timestamps.
// The same exact columns are used for original and candidate evidence.
fn payload_rows(db: &Connection, table: &str, key: &str, id: i64) -> Result<Vec<String>> {
    let columns = db
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let columns = columns
        .into_iter()
        .filter(|c| {
            !matches!(
                c.as_str(),
                "id" | "submission_id" | "payroll_timesheet_id" | "captured_at"
            )
        })
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(",");
    let mut stmt = db.prepare(&format!("SELECT {columns} FROM {table} WHERE {key}=?1"))?;
    let n = stmt.column_count();
    let mut values = stmt
        .query_map([id], |r| {
            (0..n)
                .map(|i| r.get::<_, rusqlite::types::Value>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
                .map(|v| format!("{v:?}"))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    values.sort();
    Ok(values)
}
const FINANCIAL_TABLES: &[(&str, &str)] = &[
    (
        "payroll_timesheet_worked_item_snapshots",
        "payroll_submission_items",
    ),
    ("payroll_timesheet_weeks", "payroll_submission_weeks"),
    ("payroll_timesheet_annual_leave", "payroll_submission_leave"),
    (
        "payroll_timesheet_public_holidays",
        "payroll_submission_holidays",
    ),
    (
        "payroll_correction_applications",
        "payroll_submission_corrections",
    ),
];
fn original_payload(db: &Connection, sid: i64) -> Result<String> {
    let mut values = Vec::new();
    for (_, table) in FINANCIAL_TABLES {
        values.push(format!(
            "{table}:{:?}",
            payload_rows(db, table, "submission_id", sid)?
        ));
    }
    let digest: String = db.query_row(
        "SELECT pdf_sha256 FROM payroll_submissions WHERE id=?1",
        [sid],
        |r| r.get(0),
    )?;
    Ok(format!(
        "{:x}",
        Sha256::digest(format!("{sid}:{digest}:{}", values.join("|")))
    ))
}
fn original_carry(db: &Connection, sid: i64) -> Result<i64> {
    let first:String=db.query_row("SELECT week_commencing FROM payroll_submission_weeks WHERE submission_id=?1 AND week_number=1",[sid],|r|r.get(0))?;
    let first = crate::date_utils::parse_legacy(&first)?;
    lifecycle::items(db, sid)?
        .iter()
        .filter(|i| {
            matches!(
                i.source_type.as_str(),
                "legacy_previous_cycle_adjustment"
                    | "previous_cycle_late_shift"
                    | "carry_correction"
            ) || (i.source_type == "direct_shift"
                && i.work_date
                    .as_deref()
                    .is_some_and(|d| crate::date_utils::parse_legacy(d).is_ok_and(|d| d < first)))
        })
        .try_fold(0i64, |n, i| {
            n.checked_add(i.worked_minutes)
                .ok_or_else(|| "Original carry-forward overflow".into())
        })
}
fn verify_original_financials(
    db: &Connection,
    id: i64,
    sid: i64,
    include_items: bool,
) -> Result<()> {
    for (current, original) in FINANCIAL_TABLES {
        if !include_items && *current == "payroll_timesheet_worked_item_snapshots" {
            continue;
        }
        if payload_rows(db, current, "payroll_timesheet_id", id)?
            != payload_rows(db, original, "submission_id", sid)?
        {
            return Err(format!("Sickness-only correction cannot change payroll amounts or evidence: differs from authorised original submission ({current}); reconcile original payroll evidence before correcting dates").into());
        }
    }
    let carry = record(db, id)?
        .previous_cycle_hours
        .map_or(0, |h| (h * 60.0).round() as i64);
    if carry != original_carry(db, sid)? {
        return Err("Sickness-only carry-forward differs from retained original evidence; recover/reconcile the original submission before correcting dates".into());
    }
    Ok(())
}
fn authorised_totals(db: &Connection, id: i64, sid: i64) -> Result<String> {
    verify_original_financials(db, id, sid, false)?;
    Ok(format!(
        "{}|original:{}",
        totals(db, id)?,
        original_payload(db, sid)?
    ))
}
pub fn verify_candidate_correction(db: &Connection, id: i64) -> Result<()> {
    verify_correction(db, id)?;
    let sid: i64 = db.query_row(
        "SELECT original_submission_id FROM sickness_corrections WHERE id=?1",
        [correction(db, id)?.ok_or("Sickness correction authority missing")?],
        |r| r.get(0),
    )?;
    verify_original_financials(db, id, sid, true)
}
pub fn verify_correction(db: &Connection, id: i64) -> Result<()> {
    if let Some(c) = correction(db, id)? {
        let original: String = db.query_row(
            "SELECT original_totals FROM sickness_corrections WHERE id=?1",
            [c],
            |r| r.get(0),
        )?;
        let sid: i64 = db.query_row(
            "SELECT original_submission_id FROM sickness_corrections WHERE id=?1",
            [c],
            |r| r.get(0),
        )?;
        let expected = if original.contains("|original:") {
            authorised_totals(db, id, sid)?
        } else {
            verify_original_financials(db, id, sid, false)?;
            totals(db, id)?
        };
        if original != expected {
            return Err(
                "Sickness-only correction cannot change payroll amounts or weekly totals".into(),
            );
        }
    }
    Ok(())
}
pub fn document_is_correction(db: &Connection, id: i64) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheet_snapshot_states s JOIN sickness_document_evidence d ON d.document_id=s.document_id WHERE s.payroll_timesheet_id=?1 AND d.correction_id IS NOT NULL)",[id],|r|r.get(0))?)
}
pub fn capture_document(db: &Connection, record: i64, document: i64) -> Result<()> {
    let c = correction(db, record)?;
    let evidence = encode(&evidence(db, record)?)?;
    db.execute(
        "INSERT INTO sickness_document_evidence VALUES (?1,?2,?3,?4)",
        params![document, record, evidence, c],
    )?;
    if let Some(c) = c {
        db.execute(
            "UPDATE sickness_corrections SET document_id=?1 WHERE id=?2 AND document_id IS NULL",
            params![document, c],
        )?;
    }
    Ok(())
}
pub fn correction_note(db: &Connection, id: i64) -> Result<Option<String>> {
    if correction(db, id)?.is_none() {
        return Ok(None);
    }
    // The full reason and original/corrected dates stay in the immutable audit.
    // Keep the transport explanation short enough for the existing notes area.
    Ok(Some("Sickness Information Correction: Payroll to assess financial impact; settled amounts unchanged; no SSP calculation.".into()))
}
/// Add only populated notes. Existing ordinary notes count as one note unless numbered already.
pub fn notes_with_correction(base: &str, correction: Option<&str>) -> String {
    let Some(c) = correction.filter(|s| !s.trim().is_empty()) else {
        return base.into();
    };
    let populated = !base.trim().is_empty();
    let count = base
        .lines()
        .map(|s| s.trim_start().chars().take_while(|c| *c == '*').count())
        .max()
        .unwrap_or(0)
        .max(usize::from(populated));
    if !populated {
        format!("* {c}")
    } else {
        format!("{}\n{} {c}", base.trim_end(), "*".repeat(count + 1))
    }
}

pub fn legacy_warning(db: &Connection, id: i64) -> Result<bool> {
    let hours:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=?1 AND sick_leave_hours<>0)",[id],|r|r.get(0))?;
    Ok(hours && evidence(db, id)?.periods.is_empty())
}
pub fn audit(db: &Connection, pa: i64) -> Result<Vec<String>> {
    let mut lines = db.prepare("SELECT changed_at||' — '||actor||' — '||scope||': '||reason||char(10)||'Before: '||before_evidence||char(10)||'After: '||after_evidence FROM sickness_changes WHERE personal_assistant_id=?1 ORDER BY id DESC")?.query_map([pa], |r|r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    let corrections = db.prepare("SELECT 'Sickness-only authority #'||c.id||' — payroll record '||c.payroll_timesheet_id||' — original submission '||c.original_submission_id||' — reconciliation decision '||c.decision_id||char(10)||c.authorised_at||' — '||c.actor||': '||c.reason||char(10)||'Original / operator-verified legacy dates: '||c.before_evidence||char(10)||'Corrected dates: '||c.after_evidence||char(10)||'Superseded authority: '||COALESCE(c.retired_at,'no') FROM sickness_corrections c JOIN sickness_changes a ON a.id=c.change_id WHERE a.personal_assistant_id=?1 ORDER BY c.id DESC")?.query_map([pa],|r|r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    lines.extend(corrections);
    Ok(lines)
}

pub fn generation_allowed(db: &Connection, id: i64) -> Result<bool> {
    Ok(correction_open(db,id)? || db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheet_snapshot_states s JOIN sickness_document_evidence d ON d.document_id=s.document_id WHERE s.payroll_timesheet_id=?1 AND s.state='candidate' AND d.correction_id IS NOT NULL)",[id],|r|r.get(0))?)
}
pub fn historical_hours(
    db: &Connection,
    id: i64,
) -> Result<Option<crate::pay_rate_allocation::ReconciledPayrollHours>> {
    let Some(c) = correction(db, id)? else {
        return Ok(None);
    };
    verify_correction(db, id)?;
    let sid: i64 = db.query_row(
        "SELECT original_submission_id FROM sickness_corrections WHERE id=?1",
        [c],
        |r| r.get(0),
    )?;
    let mut totals = [0; 4];
    let mut stmt=db.prepare("SELECT week_number,worked_hours FROM payroll_submission_weeks WHERE submission_id=?1 ORDER BY week_number")?;
    let rows = stmt
        .query_map([sid], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 4 {
        return Err(
            "Original four-week submission evidence unavailable; sickness correction refused"
                .into(),
        );
    }
    for (week, hours) in rows {
        if !(1..=4).contains(&week) {
            return Err("Invalid original submission week".into());
        }
        totals[(week - 1) as usize] = (hours * 60.0).round() as i64;
    }
    let current=db.prepare("SELECT worked_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=?1 ORDER BY week_number")?.query_map([id],|r|r.get::<_,f64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if current.len() != 4
        || current
            .iter()
            .enumerate()
            .any(|(i, h)| (*h * 60.0).round() as i64 != totals[i])
    {
        return Err("Current worked totals differ from original submission; sickness-only correction refused".into());
    }
    Ok(Some(crate::pay_rate_allocation::ReconciledPayrollHours {
        weeks: std::array::from_fn(|_| Vec::new()),
        week_totals_minutes: totals,
        previous_cycle_minutes: original_carry(db, sid)?,
        snapshot_items: lifecycle::items(db, sid)?,
    }))
}

#[cfg(test)]
#[path = "sickness_service_tests.rs"]
mod tests;

pub fn revision(db: &Connection, pa: i64) -> Result<i64> {
    Ok(db.query_row(
        "SELECT COALESCE(MAX(id),0) FROM sickness_changes WHERE personal_assistant_id=?1",
        [pa],
        |r| r.get(0),
    )?)
}
