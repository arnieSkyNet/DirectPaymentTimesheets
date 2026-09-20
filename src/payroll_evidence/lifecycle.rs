use super::*;
use crate::payroll_timesheet_repository::PayrollTimesheet;
use crate::payroll_worked_item_repository::WorkedItemSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Editable,
    Submitted,
    Settled,
    Indeterminate,
}
pub fn stage(db: &Connection, record: &PayrollTimesheet) -> Result<Stage> {
    let status = |kind: &str| -> Result<Option<String>> {
        Ok(db.query_row("SELECT sent_at FROM payroll_timesheet_email_status WHERE personal_assistant_id=?1 AND payroll_year=?2 AND cycle_number=?3 AND email_type=?4",params![record.personal_assistant_id,record.payroll_year,record.cycle_number,kind],|r|r.get(0)).optional()?.flatten())
    };
    if let Some(sent) = status("payslip")? {
        return Ok(if sent.starts_with("indeterminate:") {
            Stage::Indeterminate
        } else {
            Stage::Settled
        });
    }
    let state: Option<String> = db
        .query_row(
            "SELECT state FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",
            [record.id],
            |r| r.get(0),
        )
        .optional()?;
    if state.as_deref() == Some("indeterminate") {
        return Ok(Stage::Indeterminate);
    }
    let timesheet_status = status("timesheet")?;
    if timesheet_status
        .as_ref()
        .is_some_and(|s| s.starts_with("indeterminate:"))
    {
        return Ok(Stage::Indeterminate);
    }
    if state.as_deref() == Some("submitted") || timesheet_status.is_some() {
        return Ok(Stage::Submitted);
    }
    Ok(Stage::Editable)
}
pub fn latest(db: &Connection, record: i64) -> Result<Option<i64>> {
    Ok(db.query_row("SELECT id FROM payroll_submissions WHERE payroll_timesheet_id=?1 ORDER BY id DESC LIMIT 1",[record],|r|r.get(0)).optional()?)
}
/// Called inside the same transaction as successful submission state persistence.
pub fn archive_submission(db: &Connection, record: i64, at: &str) -> Result<()> {
    let (path,digest):(String,String) = db.query_row("SELECT pdf_path,pdf_sha256 FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",[record],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let bytes = if std::path::Path::new(&path).exists() {
        Some(std::fs::read(&path)?)
    } else {
        None
    };
    if bytes
        .as_ref()
        .is_some_and(|bytes| format!("{:x}", Sha256::digest(bytes)) != digest)
    {
        return Err("Submission attachment changed".into());
    }
    db.execute("INSERT INTO payroll_submissions(payroll_timesheet_id,submitted_at,supersedes_id,pdf_path,pdf_sha256,pdf_bytes,payroll_department_notes) VALUES (?1,?2,?3,?4,?5,?6,(SELECT payroll_department_notes FROM payroll_timesheets WHERE id=?1))",params![record,at,latest(db,record)?,path,digest,bytes])?;
    let id = db.last_insert_rowid();
    for (target, source) in [
        (
            "payroll_submission_items",
            "payroll_timesheet_worked_item_snapshots",
        ),
        ("payroll_submission_weeks", "payroll_timesheet_weeks"),
        ("payroll_submission_leave", "payroll_timesheet_annual_leave"),
        (
            "payroll_submission_holidays",
            "payroll_timesheet_public_holidays",
        ),
    ] {
        db.execute(
            &format!(
                "INSERT INTO {target} SELECT ?1,s.* FROM {source} s WHERE payroll_timesheet_id=?2"
            ),
            params![id, record],
        )?;
    }
    db.execute("INSERT INTO payroll_submission_corrections SELECT ?1,correction_id,minutes FROM payroll_correction_applications WHERE payroll_timesheet_id=?2",params![id,record])?;
    Ok(())
}
pub fn record_decision(
    db: &Connection,
    record: i64,
    submission: Option<i64>,
    kind: &str,
    key: &str,
    evidence: &str,
) -> Result<i64> {
    db.execute("INSERT OR IGNORE INTO payroll_reconciliation_decisions(payroll_timesheet_id,submission_id,kind,evidence_key,evidence,decided_at,actor) VALUES (?1,?2,?3,?4,?5,?6,'local_employer')",params![record,submission,kind,key,evidence,now()])?;
    Ok(db.query_row("SELECT id FROM payroll_reconciliation_decisions WHERE payroll_timesheet_id=?1 AND kind=?2 AND evidence_key=?3",params![record,kind,key],|r|r.get(0))?)
}
pub fn has_decision(db: &Connection, record: i64, kind: &str, key: &str) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_reconciliation_decisions WHERE payroll_timesheet_id=?1 AND kind=?2 AND evidence_key=?3)",params![record,kind,key],|r|r.get(0))?)
}
pub fn items(db: &Connection, submission: i64) -> Result<Vec<WorkedItemSnapshot>> {
    Ok(db.prepare("SELECT week_number,source_type,timesheet_id,work_date,worked_minutes,pay_rate_id,pay_rate_effective_date,total_hourly_rate,reason,direct_shift_id,source_evidence FROM payroll_submission_items WHERE submission_id=?1 ORDER BY id")?
        .query_map([submission], |r|Ok(WorkedItemSnapshot {week_number:r.get(0)?,source_type:r.get(1)?,timesheet_id:r.get(2)?,work_date:r.get(3)?,worked_minutes:r.get(4)?,pay_rate_id:r.get(5)?,pay_rate_effective_date:r.get(6)?,total_hourly_rate:r.get(7)?,reason:r.get(8)?,direct_shift_id:r.get(9)?,source_evidence:r.get(10)?}))?.collect::<rusqlite::Result<Vec<_>>>()?)
}
pub fn item_key(item: &WorkedItemSnapshot) -> Option<String> {
    item.timesheet_id
        .map(|id| format!("imported:{id}"))
        .or_else(|| item.direct_shift_id.map(|id| format!("direct:{id}")))
}
pub fn submitted_items(
    db: &Connection,
    record: i64,
    app: &Application,
) -> Result<Vec<WorkedItemSnapshot>> {
    let mut retained = match latest(db, record)? {
        Some(id) => items(db, id)?,
        None => {
            if app
                .payroll_worked_item_repository
                .snapshot_metadata(record)?
                .is_some_and(|m| {
                    m.state == crate::payroll_worked_item_repository::SnapshotState::Submitted
                })
            {
                app.payroll_worked_item_repository
                    .get_snapshot_items(record)?
            } else {
                Vec::new()
            }
        }
    };
    let pa: Option<i64> = db
        .query_row(
            "SELECT personal_assistant_id FROM payroll_timesheets WHERE id=?1",
            [record],
            |r| r.get(0),
        )
        .optional()?;
    // Before schema 28 payroll used immutable raw imports. A retained submitted
    // source ID, matching worked date and matching duration can still establish
    // exact intervals from that original source. This is a read-time join, never
    // a fabricated snapshot/backfill or a lookup of mutable effective clocks.
    for item in &mut retained {
        if item.source_evidence.is_some() {
            continue;
        }
        let Some(id) = item.timesheet_id else {
            continue;
        };
        let Some(raw) = app.repository.get_raw_by_id(id)? else {
            continue;
        };
        let (Some(start), Some(end)) = (parse_clock(&raw.start_time), parse_clock(&raw.end_time))
        else {
            continue;
        };
        if raw.personal_assistant_id != pa
            || raw.worked_minutes != item.worked_minutes
            || item
                .work_date
                .as_deref()
                .and_then(crate::models::parse_employment_date)
                != Some(start.date())
        {
            continue;
        }
        let e = WorkEvidence {
            source: "imported".into(),
            id,
            pa: pa.ok_or("Submission PA unavailable")?,
            pa_name: raw.pa_name,
            start: start.format("%Y-%m-%dT%H:%M:%S").to_string(),
            end: end.format("%Y-%m-%dT%H:%M:%S").to_string(),
            break_minutes: raw.break_minutes,
            minutes: raw.worked_minutes,
            notes: raw.notes.unwrap_or_default(),
            deleted: false,
        };
        item.source_evidence = Some(toml::to_string(&e)?);
    }
    Ok(retained)
}

pub fn fingerprint(items: &[WorkEvidence]) -> String {
    let mut keys = items
        .iter()
        .map(WorkEvidence::fingerprint)
        .collect::<Vec<_>>();
    keys.sort();
    hash(&keys.join("|"))
}
pub fn authorize_resubmission(
    app: &Application,
    record: &PayrollTimesheet,
    expected: &str,
) -> Result<()> {
    let db = open(app)?;
    let changes = super::reconciliation::changes(app, record)?;
    let tx = db.unchecked_transaction()?;
    if stage(&tx, record)? != Stage::Submitted {
        return Err("Only a sent, unsettled timesheet can be corrected/resubmitted".into());
    }
    if changes.signature != expected || !changes.changed {
        return Err("Evidence changed; refresh the discrepancy before proceeding".into());
    }
    if latest(&tx, record.id)?.is_none() {
        return Err(
            "No retained submission baseline; original submitted evidence cannot be reconstructed"
                .into(),
        );
    }
    record_decision(
        &tx,
        record.id,
        latest(&tx, record.id)?,
        "resubmit",
        expected,
        &changes.description,
    )?;
    // The immutable submission tables retain the exact original. The ordinary
    // candidate slot becomes editable only following this explicit decision.
    tx.execute(
        "DELETE FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",
        [record.id],
    )?;
    tx.execute(
        "DELETE FROM payroll_timesheet_worked_item_snapshots WHERE payroll_timesheet_id=?1",
        [record.id],
    )?;
    // Keep delivery history in submissions; current delivery awaits replacement.
    tx.execute("UPDATE payroll_timesheet_email_status SET sent_at=NULL WHERE personal_assistant_id=?1 AND payroll_year=?2 AND cycle_number=?3 AND email_type='timesheet'",params![record.personal_assistant_id,record.payroll_year,record.cycle_number])?;
    tx.commit()?;
    Ok(())
}

pub fn payroll_items_equal(left: &[WorkedItemSnapshot], right: &[WorkedItemSnapshot]) -> bool {
    fn normalise(items: &[WorkedItemSnapshot]) -> Vec<WorkedItemSnapshot> {
        items
            .iter()
            .cloned()
            .map(|mut i| {
                if let Some(text) = &i.source_evidence {
                    if let Ok(mut e) = toml::from_str::<WorkEvidence>(text) {
                        e.notes.clear();
                        e.pa_name.clear();
                        i.source_evidence = toml::to_string(&e).ok();
                    }
                }
                i
            })
            .collect()
    }
    normalise(left) == normalise(right)
}
/// Validate retained candidate membership against current source evidence before
/// SMTP. New overlaps, material edits and changed duplicate choices require a
/// fresh preparation/PDF; a notes-only edit does not invalidate payroll.
pub fn verify_candidate_evidence(db: &Connection, record: i64) -> Result<()> {
    let items:Vec<Option<String>>=db.prepare("SELECT source_evidence FROM payroll_timesheet_worked_item_snapshots WHERE payroll_timesheet_id=?1")?.query_map([record],|r|r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let captured = items
        .into_iter()
        .flatten()
        .map(|s| toml::from_str::<WorkEvidence>(&s))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure_editable(db, record)?;
    let recorded: Option<String> = db
        .query_row(
            "SELECT evidence_signature FROM payroll_candidate_checks WHERE payroll_timesheet_id=?1",
            [record],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(old) = recorded {
        if old != candidate_signature(db, record)? {
            return Err(
                "Payroll evidence changed after PDF generation; reopen preparation and regenerate"
                    .into(),
            );
        }
    }
    for e in &captured {
        let consumed:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_submission_items i JOIN payroll_submissions s ON s.id=i.submission_id JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE p.id<>?1 AND s.id=(SELECT MAX(id) FROM payroll_submissions WHERE payroll_timesheet_id=p.id) AND ((?2='imported' AND i.timesheet_id=?3) OR (?2='direct' AND i.direct_shift_id=?3)))",params![record,e.source,e.id],|r|r.get(0))?;
        if consumed {
            return Err("Evidence was included in another submission; regenerate this payroll before sending".into());
        }
    }
    let applications=db.prepare("SELECT a.minutes,c.minutes,c.id FROM payroll_correction_applications a JOIN payroll_corrections c ON c.id=a.correction_id WHERE a.payroll_timesheet_id=?1")?.query_map([record],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (applied, total, id) in applications {
        let consumed:i64=db.query_row("SELECT COALESCE(SUM(a.minutes),0) FROM payroll_submission_corrections a JOIN payroll_submissions s ON s.id=a.submission_id WHERE a.correction_id=?1 AND s.payroll_timesheet_id<>?2 AND s.id=(SELECT MAX(id) FROM payroll_submissions WHERE payroll_timesheet_id=s.payroll_timesheet_id)",params![id,record],|r|r.get(0))?;
        let remaining = total - consumed;
        if remaining.signum() != applied.signum()
            || applied.unsigned_abs() > remaining.unsigned_abs()
        {
            return Err(
                "Outstanding correction changed after generation; regenerate before sending".into(),
            );
        }
    }
    if captured.is_empty() {
        return Ok(());
    } // Existing schema-19 candidates have no clock capture.
    let pa: i64 = db.query_row(
        "SELECT personal_assistant_id FROM payroll_timesheets WHERE id=?1",
        [record],
        |r| r.get(0),
    )?;
    let current = load_for_pa(db, pa)?;
    let pre = preflight_for_pa(db, &current, pa)?;
    for e in &captured {
        if !pre
            .eligible
            .iter()
            .any(|c| c.key() == e.key() && c.fingerprint() == e.fingerprint())
        {
            return Err("Worked evidence or duplicate selection changed; reopen preparation and regenerate before sending".into());
        }
    }
    let (pa,first):(i64,String)=db.query_row("SELECT p.personal_assistant_id,s.first_week_commencing FROM payroll_timesheets p JOIN payroll_schedules s ON s.payroll_year=p.payroll_year AND s.cycle_number=p.cycle_number WHERE p.id=?1",[record],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let start = crate::models::parse_employment_date(&first).ok_or("Invalid schedule date")?;
    let end = start + chrono::Duration::days(27);
    if pre.unresolved.iter().any(|g| {
        g.candidates
            .iter()
            .any(|e| e.pa == pa && e.date().is_ok_and(|d| d <= end))
    }) {
        return Err("New possible duplicates require review before sending".into());
    }
    for e in pre
        .eligible
        .iter()
        .filter(|e| e.pa == pa && e.date().is_ok_and(|d| d >= start && d <= end) && e.minutes > 0)
    {
        if !captured.iter().any(|c| c.key() == e.key()) {
            return Err("A new shift was recorded after generation; reopen preparation and regenerate before sending".into());
        }
    }
    Ok(())
}

pub fn ensure_editable(db: &Connection, id: i64) -> Result<()> {
    let record:Option<PayrollTimesheet>=db.query_row("SELECT id,personal_assistant_id,payroll_year,cycle_number,previous_cycle_hours,created_at,updated_at,payroll_department_notes,actual_in_lieu_hours,actual_in_lieu_updated_at FROM payroll_timesheets WHERE id=?1",[id],|r|Ok(PayrollTimesheet{id:r.get(0)?,personal_assistant_id:r.get(1)?,payroll_year:r.get(2)?,cycle_number:r.get(3)?,previous_cycle_hours:r.get(4)?,created_at:r.get(5)?,updated_at:r.get(6)?,payroll_department_notes:r.get(7)?,actual_in_lieu_hours:r.get(8)?,actual_in_lieu_updated_at:r.get(9)?})).optional()?;
    if let Some(record) = record {
        if stage(db, &record)? != Stage::Editable {
            return Err("Submitted, settled or indeterminate payroll is protected".into());
        }
    }
    Ok(())
}
pub fn candidate_signature(db: &Connection, id: i64) -> Result<String> {
    let pa: Option<i64> = db
        .query_row(
            "SELECT personal_assistant_id FROM payroll_timesheets WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(pa) = pa else {
        return Ok(String::new());
    };
    let first:Option<String>=db.query_row("SELECT s.first_week_commencing FROM payroll_timesheets p JOIN payroll_schedules s ON s.payroll_year=p.payroll_year AND s.cycle_number=p.cycle_number WHERE p.id=?1",[id],|r|r.get(0)).optional()?;
    let through = first
        .as_deref()
        .and_then(crate::models::parse_employment_date)
        .map(|d| d + chrono::Duration::days(27));
    let relevant = load_for_pa(db, pa)?
        .into_iter()
        .filter(|e| e.pa == pa && through.is_none_or(|end| e.date().is_ok_and(|d| d <= end)))
        .collect::<Vec<_>>();
    let group_keys = groups(&relevant)?
        .into_iter()
        .map(|g| g.fingerprint)
        .collect::<HashSet<_>>();
    let mut values = relevant
        .iter()
        .map(WorkEvidence::fingerprint)
        .collect::<Vec<_>>();
    values.push(format!("period:{first:?}"));
    let mut statement=db.prepare("SELECT group_fingerprint,winner_source,winner_id FROM payroll_duplicate_decisions WHERE invalidated_at IS NULL AND id IN (
        SELECT m.decision_id FROM payroll_duplicate_members m
        LEFT JOIN timesheets t ON m.source='imported' AND t.id=m.source_id
        LEFT JOIN direct_shifts d ON m.source='direct' AND d.id=m.source_id
        WHERE t.personal_assistant_id=?1 OR d.personal_assistant_id=?1
    ) ORDER BY id")?;
    for row in statement.query_map([pa], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (key, source, id) = row?;
        if group_keys.contains(&key) {
            values.push(format!("{key}:{source}:{id}"));
        }
    }
    let mut statement=db.prepare("SELECT id,effective_date,base_hourly_rate,employer_top_up_rate FROM personal_assistant_pay_rates WHERE personal_assistant_id=?1 ORDER BY id")?;
    for row in statement.query_map([pa], |r| {
        Ok(format!(
            "rate:{}:{}:{}:{}",
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, f64>(2)?,
            r.get::<_, f64>(3)?
        ))
    })? {
        values.push(row?);
    }
    let mut statement=db.prepare("SELECT id,minutes,evidence_key FROM payroll_corrections WHERE personal_assistant_id=?1 ORDER BY id")?;
    for row in statement.query_map([pa], |r| {
        Ok(format!(
            "correction:{}:{}:{}",
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?
        ))
    })? {
        values.push(row?);
    }
    let mut statement=db.prepare("SELECT s.id,s.payroll_timesheet_id FROM payroll_submissions s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE p.personal_assistant_id=?1 AND p.id<>?2 ORDER BY s.id")?;
    for row in statement.query_map(params![pa, id], |r| {
        Ok(format!(
            "submission:{}:{}",
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?
        ))
    })? {
        values.push(row?);
    }
    values.sort();
    Ok(hash(&values.join("|")))
}

pub fn has_exact_intervals(items: &[WorkedItemSnapshot]) -> bool {
    let sources = items
        .iter()
        .filter(|i| i.source_type != "carry_correction")
        .collect::<Vec<_>>();
    !sources.is_empty()
        && sources.iter().all(|i| {
            i.source_evidence
                .as_deref()
                .and_then(|s| toml::from_str::<WorkEvidence>(s).ok())
                .is_some_and(|e| e.start_time().is_ok() && e.end_time().is_ok())
        })
}
