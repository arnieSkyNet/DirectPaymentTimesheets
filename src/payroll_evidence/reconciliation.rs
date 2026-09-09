//! Payroll lifecycle and outstanding corrections; independent of UI and rates.
use super::lifecycle::{self, Stage};
use super::*;
use crate::payroll_timesheet_repository::PayrollTimesheet;
use crate::payroll_worked_item_repository::WorkedItemSnapshot;

#[derive(Clone, Debug)]
pub struct Component {
    pub key: String,
    pub source: Option<String>,
    pub source_id: Option<i64>,
    pub date: Option<String>,
    pub minutes: i64,
    pub rate_id: Option<i64>,
    pub rate_date: Option<String>,
    pub rate: Option<f64>,
    pub aggregate: bool,
}
pub struct Changes {
    pub signature: String,
    pub description: String,
    pub changed: bool,
    pub components: Vec<Component>,
    pub requires_complete: bool,
}
fn period(app: &Application, record: &PayrollTimesheet) -> Result<(NaiveDate, NaiveDate)> {
    let schedule = app
        .payroll_schedule_repository
        .get_all()?
        .into_iter()
        .find(|s| s.payroll_year == record.payroll_year && s.cycle_number == record.cycle_number)
        .ok_or("Payroll schedule unavailable; cannot attribute evidence")?;
    let start = crate::models::parse_employment_date(&schedule.first_week_commencing)
        .ok_or("Invalid payroll date")?;
    Ok((start, start + chrono::Duration::days(27)))
}
fn net_corrections(db: &Connection, origin: i64, key: &str) -> Result<i64> {
    Ok(db.query_row("SELECT COALESCE(SUM(minutes),0) FROM payroll_corrections WHERE origin_payroll_timesheet_id=?1 AND evidence_key LIKE ?2",params![origin,format!("{key}|%")],|r|r.get(0))?)
}
// An unchanged raw duration is not a discrepancy against its rounded submitted
// duration. Never apply today's rounding configuration to historical submissions.
fn retained_actual_minutes(item: &WorkedItemSnapshot, current: Option<&WorkEvidence>) -> i64 {
    let Some(current) = current else {
        return 0;
    };
    let raw = item
        .source_evidence
        .as_deref()
        .and_then(|s| toml::from_str::<WorkEvidence>(s).ok());
    if raw.is_some_and(|raw| raw.minutes == current.minutes) {
        item.worked_minutes
    } else {
        current.minutes
    }
}

pub fn changes(app: &Application, record: &PayrollTimesheet) -> Result<Changes> {
    let db = open(app)?;
    let evidence = load(app)?;
    let pre = preflight(&db, &evidence)?;
    changes_prepared(app, record, &pre)
}

pub(crate) fn changes_prepared(
    app: &Application,
    record: &PayrollTimesheet,
    pre: &Preflight,
) -> Result<Changes> {
    let db = open(app)?;
    let (start, end) = period(app, record)?;
    if pre.unresolved.iter().any(|g| {
        g.candidates
            .iter()
            .any(|e| e.pa == record.personal_assistant_id && e.date().is_ok_and(|d| d <= end))
    }) {
        return Err("Resolve possible duplicates before reviewing payroll discrepancies".into());
    }
    let current = pre
        .eligible
        .iter()
        .filter(|e| e.pa == record.personal_assistant_id)
        .cloned()
        .collect::<Vec<_>>();
    let old = lifecycle::submitted_items(&db, record.id, app)?;
    let sid = lifecycle::latest(&db, record.id)?.unwrap_or(0);
    let mut components = Vec::new();
    let mut materially_changed = false;
    for item in &old {
        let Some(key) = lifecycle::item_key(item) else {
            continue;
        };
        let current_item = current.iter().find(|e| e.key() == key);
        let actual = retained_actual_minutes(item, current_item);
        let prefix = format!("submission:{sid}:{key}");
        let delta = actual - item.worked_minutes - net_corrections(&db, record.id, &prefix)?;
        materially_changed |= item
            .source_evidence
            .as_deref()
            .and_then(|s| toml::from_str::<WorkEvidence>(s).ok())
            .is_some_and(|previous| {
                current_item.is_none_or(|e| e.fingerprint() != previous.fingerprint())
            });
        if delta != 0 {
            components.push(Component {
                key: prefix,
                source: Some(
                    if item.direct_shift_id.is_some() {
                        "direct"
                    } else {
                        "imported"
                    }
                    .into(),
                ),
                source_id: item.direct_shift_id.or(item.timesheet_id),
                date: item.work_date.clone(),
                minutes: delta,
                rate_id: item.pay_rate_id,
                rate_date: item.pay_rate_effective_date.clone(),
                rate: item.total_hourly_rate,
                aggregate: false,
            });
        }
    }
    // Opaque weekly estimates cannot be inferred from absence of later evidence.
    // Their proposed difference is displayed for an explicit review only.
    let weeks = app.payroll_timesheet_repository.get_weeks(record.id)?;
    let legacy_aggregate = lifecycle::stage(&db, record)? == Stage::Settled
        && super::legacy_baseline::has_settlement(&db, record.id)?;
    for week in weeks {
        let ws = crate::models::parse_employment_date(&week.week_commencing)
            .ok_or("Invalid week date")?;
        let we = ws + chrono::Duration::days(6);
        if we > chrono::Local::now().date_naive() {
            continue;
        }
        let week_items = old
            .iter()
            .filter(|i| i.week_number == week.week_number)
            .collect::<Vec<_>>();
        let opaque = week_items.is_empty()
            || week_items
                .iter()
                .any(|i| i.source_type == "manual_adjustment");
        // Cutover aggregates remain authoritative: source membership was never
        // established, so their comparison cannot establish an estimate correction.
        // New historical sources still receive their individual payment review.
        if !opaque || legacy_aggregate {
            continue;
        }
        let mut actual = 0i64;
        for e in current
            .iter()
            .filter(|e| e.date().is_ok_and(|d| d >= ws && d <= we))
        {
            let retained_member = old
                .iter()
                .any(|i| lifecycle::item_key(i).as_deref() == Some(&e.key()));
            if !retained_member
                && (lifecycle::has_decision(&db, record.id, "unpaid", &e.fingerprint())?
                    || consumed_elsewhere(app, &db, record, e)?)
            {
                continue;
            }
            actual = actual
                .checked_add(
                    old.iter()
                        .find(|i| lifecycle::item_key(i).as_deref() == Some(&e.key()))
                        .map_or(e.minutes, |i| retained_actual_minutes(i, Some(e))),
                )
                .ok_or("Actual worked hours overflow")?;
        }
        let submitted = (week.worked_hours * 60.0).round() as i64;
        let prefix = format!("submission:{sid}:aggregate:{}", week.week_number);
        // Remove the complete known-source difference, including differences
        // already recorded in earlier individual corrections. Otherwise a later
        // aggregate confirmation would deduct those same hours a second time.
        let already_known: i64 = week_items
            .iter()
            .filter_map(|i| lifecycle::item_key(i).map(|key| (i, key)))
            .map(|(i, key)| {
                current
                    .iter()
                    .find(|e| e.key() == key)
                    .map_or(0, |e| retained_actual_minutes(i, Some(e)))
                    - i.worked_minutes
            })
            .sum();
        let delta = actual - submitted - net_corrections(&db, record.id, &prefix)? - already_known;
        if delta != 0 {
            components.push(Component {
                key: prefix,
                source: None,
                source_id: None,
                date: None,
                minutes: delta,
                rate_id: None,
                rate_date: None,
                rate: None,
                aggregate: true,
            });
        }
    }
    // New dated work in a period with dated membership is a distinct discrepancy.
    for e in current
        .iter()
        .filter(|e| e.date().is_ok_and(|d| d >= start && d <= end))
    {
        if old
            .iter()
            .any(|i| lifecycle::item_key(i).as_deref() == Some(&e.key()))
        {
            continue;
        }
        if lifecycle::stage(&db, record)? == Stage::Settled
            && super::legacy_baseline::contains(&db, record.id, e)?
        {
            continue;
        }
        let week = ((e.date()? - start).num_days() / 7) + 1;
        if !old.iter().any(|i| i.week_number == week)
            || old
                .iter()
                .any(|i| i.week_number == week && i.source_type == "manual_adjustment")
        {
            continue;
        }
        if consumed_elsewhere(app, &db, record, e)? {
            continue;
        }
        let prefix = format!("submission:{sid}:new:{}", e.key());
        let delta = e.minutes - net_corrections(&db, record.id, &prefix)?;
        if delta != 0 {
            let rate = app
                .pay_rate_repository
                .get_for_personal_assistant_as_of(e.pa, e.date()?)?
                .ok_or("Historical pay rate unavailable")?;
            components.push(Component {
                key: prefix,
                source: Some(e.source.clone()),
                source_id: Some(e.id),
                date: Some(e.date()?.to_string()),
                minutes: delta,
                rate_id: Some(rate.id),
                rate_date: Some(rate.effective_date),
                rate: Some(rate.base_hourly_rate + rate.employer_top_up_rate),
                aggregate: false,
            });
        }
    }
    let signature = hash(&format!("{sid}:{}", lifecycle::fingerprint(&current)));
    let requires_complete = components.iter().any(|c| c.aggregate && c.minutes < 0);
    let sent_at: Option<String> = db
        .query_row(
            "SELECT submitted_at FROM payroll_submissions WHERE id=?1",
            [sid],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let description = components
        .iter()
        .map(|c| {
            format!(
                "{}: {:+.2} hours{}",
                c.key,
                c.minutes as f64 / 60.0,
                if c.aggregate {
                    " (aggregate evidence; review actual completeness)"
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let description = format!(
        "Timesheet submitted: {}.\n{}",
        sent_at
            .as_deref()
            .unwrap_or("date unavailable in retained history"),
        description
    );
    Ok(Changes {
        signature: signature.clone(),
        description,
        changed: (materially_changed
            && lifecycle::stage(&db, record)? != Stage::Settled
            && !lifecycle::has_decision(&db, record.id, "carry", &signature)?)
            || !components.is_empty(),
        components,
        requires_complete,
    })
}
fn consumed_elsewhere(
    app: &Application,
    db: &Connection,
    record: &PayrollTimesheet,
    e: &WorkEvidence,
) -> Result<bool> {
    for r in app
        .payroll_timesheet_repository
        .get_all_for_personal_assistant(e.pa)?
    {
        if r.id != record.id
            && matches!(
                lifecycle::stage(db, &r)?,
                Stage::Submitted | Stage::Settled | Stage::Indeterminate
            )
            && lifecycle::submitted_items(db, r.id, app)?
                .iter()
                .any(|i| lifecycle::item_key(i).as_deref() == Some(&e.key()))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn insert_component(
    db: &Connection,
    record: &PayrollTimesheet,
    sid: Option<i64>,
    decision: Option<i64>,
    c: &Component,
    signature: &str,
) -> Result<()> {
    if c.minutes == 0 {
        return Ok(());
    }
    let serial: i64 = db.query_row(
        "SELECT COUNT(*) FROM payroll_corrections WHERE origin_payroll_timesheet_id=?1",
        [record.id],
        |r| r.get(0),
    )?;
    db.execute("INSERT INTO payroll_corrections(personal_assistant_id,origin_payroll_timesheet_id,submission_id,decision_id,evidence_key,source,source_id,work_date,minutes,pay_rate_id,pay_rate_effective_date,total_hourly_rate,reason,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",params![record.personal_assistant_id,record.id,sid,decision,format!("{}|{signature}|{serial}",c.key),c.source,c.source_id,c.date,c.minutes,c.rate_id,c.rate_date,c.rate,if c.aggregate {"Confirmed aggregate estimate reconciliation"}else{"Audited submitted/actual evidence difference"},now()])?;
    Ok(())
}
pub fn carry(
    app: &Application,
    record: &PayrollTimesheet,
    expected: &str,
    complete: bool,
) -> Result<()> {
    let change = changes(app, record)?;
    let db = open(app)?;
    let tx = db.unchecked_transaction()?;
    if !matches!(
        lifecycle::stage(&tx, record)?,
        Stage::Submitted | Stage::Settled
    ) {
        return Err("Only submitted/settled payroll can carry a correction".into());
    }
    if expected != change.signature || !change.changed {
        return Err("Discrepancy changed; refresh before deciding".into());
    }
    if change.requires_complete && !complete {
        return Err(
            "Confirm actual evidence is complete before creating an aggregate negative correction"
                .into(),
        );
    }
    let sid = lifecycle::latest(&tx, record.id)?;
    if change.requires_complete {
        lifecycle::record_decision(
            &tx,
            record.id,
            sid,
            "complete_actuals",
            expected,
            &change.description,
        )?;
    }
    let decision =
        lifecycle::record_decision(&tx, record.id, sid, "carry", expected, &change.description)?;
    let evidence = load(app)?;
    let (start, _) = period(app, record)?;
    for c in &change.components {
        insert_component(&tx, record, sid, Some(decision), c, expected)?;
        let correction = tx.last_insert_rowid();
        if c.aggregate {
            let week = c
                .key
                .rsplit(':')
                .next()
                .and_then(|s| s.parse::<i64>().ok())
                .ok_or("Invalid aggregate portion")?;
            let ws = start + chrono::Duration::days((week - 1) * 7);
            for e in evidence.iter().filter(|e| {
                e.pa == record.personal_assistant_id
                    && !e.deleted
                    && e.date()
                        .is_ok_and(|d| d >= ws && d <= ws + chrono::Duration::days(6))
            }) {
                tx.execute(
                    "INSERT INTO payroll_correction_evidence VALUES (?1,?2,?3,?4,?5)",
                    params![
                        correction,
                        e.source,
                        e.id,
                        e.fingerprint(),
                        toml::to_string(e)?
                    ],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}
#[derive(Clone)]
pub struct HistoricalReview {
    pub record: PayrollTimesheet,
    pub evidence: WorkEvidence,
}
pub fn historical_decision(app: &Application, review: &HistoricalReview, paid: bool) -> Result<()> {
    let db = open(app)?;
    let tx = db.unchecked_transaction()?;
    if lifecycle::stage(&tx, &review.record)? != Stage::Settled {
        return Err("Historical payroll is no longer definitively settled".into());
    }
    let current = load(app)?
        .into_iter()
        .find(|e| {
            e.key() == review.evidence.key()
                && e.fingerprint() == review.evidence.fingerprint()
                && !e.deleted
        })
        .ok_or("Historical evidence changed; refresh review")?;
    let key = current.fingerprint();
    if lifecycle::has_decision(
        &tx,
        review.record.id,
        if paid { "unpaid" } else { "already_paid" },
        &key,
    )? {
        return Err("This evidence already has the opposite payment determination".into());
    }
    lifecycle::record_decision(
        &tx,
        review.record.id,
        lifecycle::latest(&tx, review.record.id)?,
        if paid { "already_paid" } else { "unpaid" },
        &key,
        &toml::to_string(&current)?,
    )?;
    tx.commit()?;
    Ok(())
}

#[derive(Clone)]
pub struct Plan {
    pub work: Vec<WorkEvidence>,
    pub historical_reviews: Vec<HistoricalReview>,
    pub notices: Vec<String>,
}
pub fn plan(app: &Application, record: &PayrollTimesheet) -> Result<Plan> {
    let db = open(app)?;
    let all = load(app)?;
    let pre = preflight(&db, &all)?;
    plan_prepared(app, record, &pre)
}

pub(crate) fn plan_prepared(
    app: &Application,
    record: &PayrollTimesheet,
    pre: &Preflight,
) -> Result<Plan> {
    let db = open(app)?;
    let (start, end) = period(app, record)?;
    if pre.unresolved.iter().any(|g| {
        g.candidates
            .iter()
            .any(|e| e.pa == record.personal_assistant_id && e.date().is_ok_and(|d| d <= end))
    }) {
        return Err("Unresolved possible duplicate shifts; open Payroll Timesheet Preparation to select candidates".into());
    }
    let records = app
        .payroll_timesheet_repository
        .get_all_for_personal_assistant(record.personal_assistant_id)?;
    // A period's retained history is immutable during this plan. Read each
    // prior record once, rather than once for every source shift.
    let mut history = std::collections::HashMap::new();
    let mut periods = std::collections::HashMap::new();
    let mut work = Vec::new();
    let mut historical_reviews = Vec::new();
    let mut notices = Vec::new();
    for e in pre
        .eligible
        .iter()
        .filter(|e| e.pa == record.personal_assistant_id && e.minutes > 0)
        .cloned()
    {
        let date = e.date()?;
        if date > end {
            continue;
        }
        let corrected:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_corrections WHERE personal_assistant_id=?1 AND source=?2 AND source_id=?3)",params![e.pa,e.source,e.id],|r|r.get(0))?;
        let covered:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_correction_evidence WHERE source=?1 AND source_id=?2 AND fingerprint=?3)",params![e.source,e.id,e.fingerprint()],|r|r.get(0))?;
        if corrected || covered {
            continue;
        }
        let mut held = false;
        for previous in &records {
            if previous.id == record.id {
                continue;
            }
            if !history.contains_key(&previous.id) {
                let stage = lifecycle::stage(&db, previous)?;
                let stage =
                    if stage == Stage::Editable && lifecycle::latest(&db, previous.id)?.is_some() {
                        Stage::Submitted
                    } else {
                        stage
                    };
                let items = if stage == Stage::Editable {
                    Vec::new()
                } else {
                    lifecycle::submitted_items(&db, previous.id, app)?
                };
                history.insert(previous.id, (stage, items));
            }
            let (stage, previous_items) = &history[&previous.id];
            let stage = *stage;
            if stage == Stage::Editable {
                continue;
            }
            if previous_items
                .iter()
                .any(|i| lifecycle::item_key(i).as_deref() == Some(&e.key()))
            {
                held = true;
                break;
            }
            if !periods.contains_key(&previous.id) {
                periods.insert(previous.id, period(app, previous)?);
            }
            let (ps, pe) = periods[&previous.id];
            if date < ps || date > pe {
                continue;
            }
            if stage == Stage::Indeterminate {
                notices.push(format!(
                    "{}: historical delivery is indeterminate; evidence held",
                    e.key()
                ));
                held = true;
                break;
            }
            if stage == Stage::Submitted {
                notices.push(format!("{}: original timesheet was sent; review Correct / Resubmit or Carry correction forward in its preparation",e.key()));
                held = true;
                break;
            }
            if stage == Stage::Settled {
                if super::legacy_baseline::contains(&db, previous.id, &e)? {
                    held = true;
                    break;
                }
                // Even a submitted item list may include undated estimates. Lack of
                // exact paid intervals cannot establish that a new shift was unpaid.
                let exact = lifecycle::has_exact_intervals(&previous_items);
                if !exact {
                    if lifecycle::has_decision(&db, previous.id, "already_paid", &e.fingerprint())?
                    {
                        held = true;
                        break;
                    }
                    if !lifecycle::has_decision(&db, previous.id, "unpaid", &e.fingerprint())? {
                        historical_reviews.push(HistoricalReview {
                            record: previous.clone(),
                            evidence: e.clone(),
                        });
                        held = true;
                        break;
                    }
                }
            }
        }
        // Work represented by a correction must not also be added as a full shift.
        let corrected:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_corrections WHERE personal_assistant_id=?1 AND source=?2 AND source_id=?3)",params![e.pa,e.source,e.id],|r|r.get(0))?;
        if !held && !corrected {
            work.push(e);
        }
    }
    let _ = start;
    Ok(Plan {
        work,
        historical_reviews,
        notices,
    })
}

#[derive(Clone, Debug)]
pub struct CorrectionApplication {
    pub id: i64,
    pub minutes: i64,
    pub date: Option<String>,
    pub rate_id: Option<i64>,
    pub rate_date: Option<String>,
    pub rate: Option<f64>,
    pub reason: String,
}
pub fn outstanding(
    app: &Application,
    record: &PayrollTimesheet,
) -> Result<Vec<CorrectionApplication>> {
    let db = open(app)?;
    let mut rows=db.prepare("SELECT id,minutes,work_date,pay_rate_id,pay_rate_effective_date,total_hourly_rate,reason FROM payroll_corrections WHERE personal_assistant_id=?1 AND origin_payroll_timesheet_id<>?2 ORDER BY created_at,id")?.query_map(params![record.personal_assistant_id,record.id],|r|Ok(CorrectionApplication{id:r.get(0)?,minutes:r.get(1)?,date:r.get(2)?,rate_id:r.get(3)?,rate_date:r.get(4)?,rate:r.get(5)?,reason:r.get(6)?}))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for c in &mut rows {
        for r in app
            .payroll_timesheet_repository
            .get_all_for_personal_assistant(record.personal_assistant_id)?
        {
            if r.id == record.id {
                continue;
            }
            if lifecycle::latest(&db, r.id)?.is_some()
                || matches!(
                    lifecycle::stage(&db, &r)?,
                    Stage::Submitted | Stage::Settled | Stage::Indeterminate
                )
            {
                if let Some(sid) = lifecycle::latest(&db, r.id)? {
                    let used:i64=db.query_row("SELECT COALESCE(SUM(minutes),0) FROM payroll_submission_corrections WHERE submission_id=?1 AND correction_id=?2",params![sid,c.id],|r|r.get(0))?;
                    c.minutes -= used;
                }
            }
        }
    }
    rows.retain(|c| c.minutes != 0);
    Ok(rows)
}
/// Positive corrections supply capacity before negatives; negative remainders
/// stay individually outstanding. No weekly payable total is allowed below zero.
pub fn apply_corrections(
    totals: &mut [i64; 4],
    corrections: &[CorrectionApplication],
) -> Result<Vec<(i64, i64)>> {
    let mut applied = Vec::new();
    for c in corrections.iter().filter(|c| c.minutes > 0) {
        totals[0] = totals[0]
            .checked_add(c.minutes)
            .ok_or("Worked hours overflow")?;
        applied.push((c.id, c.minutes));
    }
    if totals.iter().any(|n| *n < 0) {
        return Err("Negative underlying payable hours require review".into());
    }
    for c in corrections.iter().filter(|c| c.minutes < 0) {
        let mut left = c.minutes.checked_neg().ok_or("Correction overflow")?;
        let original = left;
        for total in totals.iter_mut() {
            let used = left.min(*total);
            *total -= used;
            left -= used;
        }
        if original != left {
            applied.push((c.id, -(original - left)));
        }
    }
    Ok(applied)
}
pub fn calculate(
    app: &Application,
    record: &PayrollTimesheet,
    weeks: &[NaiveDate; 4],
    legacy: Option<&crate::pay_rate_allocation::PreviousCycleContext>,
) -> Result<crate::pay_rate_allocation::ReconciledPayrollHours> {
    let db = open(app)?;
    if lifecycle::stage(&db, record)? != Stage::Editable {
        return Err(
            "Submitted, settled or uncertain payroll is protected from recalculation".into(),
        );
    }
    sync_settled_corrections(app, record.personal_assistant_id)?;
    let plan = plan(app, record)?;
    calculate_prepared(app, record, weeks, legacy, &plan)
}

// Used only immediately after refresh_record in preparation loading. The plan
// includes the full duplicate and historical safety checks, after settlement sync.
pub(crate) fn calculate_prepared(
    app: &Application,
    record: &PayrollTimesheet,
    weeks: &[NaiveDate; 4],
    legacy: Option<&crate::pay_rate_allocation::PreviousCycleContext>,
    plan: &Plan,
) -> Result<crate::pay_rate_allocation::ReconciledPayrollHours> {
    let db = open(app)?;
    if lifecycle::stage(&db, record)? != Stage::Editable {
        return Err(
            "Submitted, settled or uncertain payroll is protected from recalculation".into(),
        );
    }
    if !plan.historical_reviews.is_empty() {
        return Err(
            "Historical payment status needs review in Payroll Timesheet Preparation".into(),
        );
    }
    // Preserve the established opaque historical-backfill path, not its old
    // previous-cycle-only membership scan. Dated late work is selected above.
    let empty: Vec<crate::models::TimesheetEntry> = Vec::new();
    let historical_backfill = app
        .payroll_worked_item_repository
        .get_manual_adjustments(record.id)?
        .iter()
        .any(|a| crate::historical_payroll_backfill::is_backfill_reason(a.reason.as_deref()));
    let legacy = if historical_backfill { legacy } else { None };
    let mut result = crate::pay_rate_allocation::reconcile_payroll_hours(
        &app.pay_rate_repository,
        &app.payroll_worked_item_repository,
        &empty,
        record.personal_assistant_id,
        record.id,
        weeks,
        legacy,
    )?;
    for e in &plan.work {
        let date = e.date()?;
        let late = date < weeks[0];
        let index = if late {
            0
        } else {
            ((date - weeks[0]).num_days() / 7) as usize
        };
        crate::pay_rate_allocation::add_evidence(
            &app.pay_rate_repository,
            &mut result,
            e,
            index,
            late,
            &app.context.config.payroll,
        )?;
    }
    let corrections = outstanding(app, record)?;
    let mut applications = Vec::new();
    for c in corrections
        .iter()
        .filter(|c| c.minutes > 0)
        .chain(corrections.iter().filter(|c| c.minutes < 0))
    {
        let before = result.week_totals_minutes;
        let applied = if c.minutes > 0 {
            result.week_totals_minutes[0] = result.week_totals_minutes[0]
                .checked_add(c.minutes)
                .ok_or("Worked hours overflow")?;
            vec![(c.id, c.minutes)]
        } else {
            apply_corrections(&mut result.week_totals_minutes, std::slice::from_ref(c))?
        };
        for (id, minutes) in applied {
            applications.push((id, minutes));
            result.previous_cycle_minutes += minutes;
            for (week, previous) in before.into_iter().enumerate() {
                let part = result.week_totals_minutes[week] - previous;
                if part == 0 {
                    continue;
                }
                result.snapshot_items.push(WorkedItemSnapshot {
                    week_number: (week + 1) as i64,
                    source_type: "carry_correction".into(),
                    timesheet_id: None,
                    direct_shift_id: None,
                    source_evidence: None,
                    work_date: c.date.clone(),
                    worked_minutes: part,
                    pay_rate_id: c.rate_id,
                    pay_rate_effective_date: c.rate_date.clone(),
                    total_hourly_rate: c.rate,
                    reason: Some(format!("Correction {}: {}", c.id, c.reason)),
                });
                result.weeks[week].push(crate::pay_rate_allocation::PayRatePortion {
                    worked_minutes: part,
                    total_hourly_rate: c.rate,
                    effective_date: c
                        .rate_date
                        .as_deref()
                        .and_then(crate::models::parse_employment_date),
                    rate_id: c.rate_id,
                    is_previous_cycle: true,
                    is_opaque_legacy: c.rate.is_none(),
                });
            }
        }
    }
    if result.week_totals_minutes.iter().any(|n| *n < 0) {
        return Err("Negative final payable hours require review".into());
    }
    // Reservations are candidates, not consumption. Submitted applications are
    // copied into immutable history only after successful production send.
    let tx = db.unchecked_transaction()?;
    let existing=tx.prepare("SELECT correction_id,minutes FROM payroll_correction_applications WHERE payroll_timesheet_id=?1 ORDER BY correction_id")?.query_map([record.id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut ordered = applications;
    ordered.sort();
    if existing != ordered {
        tx.execute(
            "DELETE FROM payroll_correction_applications WHERE payroll_timesheet_id=?1",
            [record.id],
        )?;
        for (id, minutes) in ordered {
            tx.execute(
                "INSERT INTO payroll_correction_applications VALUES (?1,?2,?3)",
                params![record.id, id, minutes],
            )?;
        }
    }
    tx.commit()?;
    Ok(result)
}

/// A concrete audited change to settled source evidence is its own authority.
/// Aggregate estimates are deliberately excluded from this automatic path.
pub fn sync_settled_corrections(app: &Application, pa: i64) -> Result<()> {
    let db = open(app)?;
    for record in app
        .payroll_timesheet_repository
        .get_all_for_personal_assistant(pa)?
    {
        if lifecycle::stage(&db, &record)? != Stage::Settled {
            continue;
        }
        let old = lifecycle::submitted_items(&db, record.id, app)?;
        if old.is_empty() {
            continue;
        }
        let changes = changes(app, &record)?;
        let exact = lifecycle::has_exact_intervals(&old);
        let all = load(app)?;
        let tx = db.unchecked_transaction()?;
        for c in changes.components.iter().filter(|c| !c.aggregate) {
            let known_edit = c
                .source
                .as_ref()
                .zip(c.source_id)
                .is_some_and(|(source, id)| {
                    old.iter().any(|i| {
                        lifecycle::item_key(i).as_deref() == Some(&format!("{source}:{id}"))
                            && all
                                .iter()
                                .find(|e| e.source == *source && e.id == id)
                                .is_some_and(|e| e.deleted || e.minutes != i.worked_minutes)
                    })
                });
            if exact || known_edit {
                insert_component(
                    &tx,
                    &record,
                    lifecycle::latest(&tx, record.id)?,
                    None,
                    c,
                    &changes.signature,
                )?;
            }
        }
        tx.commit()?;
    }
    Ok(())
}

pub fn historical_save_notice(
    app: &Application,
    pa: i64,
    date: NaiveDate,
) -> Result<Option<String>> {
    let db = open(app)?;
    for r in app
        .payroll_timesheet_repository
        .get_all_for_personal_assistant(pa)?
    {
        let (start, end) = period(app, &r)?;
        if date >= start && date <= end && lifecycle::stage(&db, &r)? == Stage::Settled {
            return Ok(Some("This shift belongs to a payroll period whose payslip has already been sent. It will be checked against paid evidence and, if outstanding, carried forward. Settled payroll remains unchanged.".into()));
        }
    }
    Ok(None)
}

pub fn audit_lines(app: &Application, record: &PayrollTimesheet) -> Result<Vec<String>> {
    let db = open(app)?;
    let mut lines = Vec::new();
    let baseline: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM payroll_legacy_settlements WHERE payroll_timesheet_id=?1)",
        [record.id],
        |r| r.get(0),
    )?;
    if baseline {
        lines.push("Legacy cutover settlement retained: matching pre-cutover evidence is not a new unpaid claim. Historical aggregates remain authoritative; individual paid shift membership is not asserted.".into());
    }
    let mut stmt=db.prepare("SELECT id,submitted_at,supersedes_id,pdf_sha256,length(pdf_bytes) FROM payroll_submissions WHERE payroll_timesheet_id=?1 ORDER BY id")?;
    for row in stmt.query_map([record.id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<i64>>(4)?,
        ))
    })? {
        let (id, at, prior, digest, bytes) = row?;
        lines.push(format!("Submission {id}: {}; supersedes {prior:?}; attachment SHA-256 {digest}; retained bytes {bytes:?}",at.as_deref().unwrap_or("historical send time unavailable")));
    }
    let mut stmt=db.prepare("SELECT kind,decided_at,evidence FROM payroll_reconciliation_decisions WHERE payroll_timesheet_id=?1 ORDER BY id")?;
    for row in stmt.query_map([record.id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (kind, at, evidence) = row?;
        lines.push(format!("{at}: {kind}\n{evidence}"));
    }
    let mut stmt=db.prepare("SELECT id,origin_payroll_timesheet_id,minutes,reason FROM payroll_corrections WHERE personal_assistant_id=?1 ORDER BY id")?;
    for row in stmt.query_map([record.personal_assistant_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (id, origin, minutes, reason) = row?;
        let reserved:i64=db.query_row("SELECT COALESCE(SUM(a.minutes),0) FROM payroll_submission_corrections a JOIN payroll_submissions s ON s.id=a.submission_id WHERE a.correction_id=?1 AND s.id=(SELECT MAX(id) FROM payroll_submissions WHERE payroll_timesheet_id=s.payroll_timesheet_id)",[id],|r|r.get(0))?;
        lines.push(format!("Correction {id}, original preparation {origin}: {:+.2} hours; remaining outside submitted payroll {:+.2} hours — {reason}",minutes as f64/60.0,(minutes-reserved) as f64/60.0));
    }
    let mut stmt=db.prepare("SELECT d.id,d.winner_source,d.winner_id,d.decided_at,d.invalidated_at,m.evidence FROM payroll_duplicate_decisions d JOIN payroll_duplicate_members m ON m.decision_id=d.id ORDER BY d.id,m.source,m.source_id")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, String>(5)?,
        ))
    })? {
        let (id, source, winner, at, invalidated, text) = row?;
        let e: WorkEvidence = toml::from_str(&text)?;
        if e.pa == record.personal_assistant_id {
            lines.push(format!("Duplicate decision {id} at {at}: winner {source}:{winner}; {} {} — {} to {}; invalidated {invalidated:?}",e.key(),if e.source==source&&e.id==winner {"selected"}else{"excluded / not selected"},e.start,e.end));
        }
    }
    Ok(lines)
}
