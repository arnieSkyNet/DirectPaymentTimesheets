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
    source_signature: String,
    pub signature: String,
    pub description: String,
    pub changed: bool,
    pub components: Vec<Component>,
    pub requires_complete: bool,
    pub blocked: bool,
}
#[cfg(test)]
thread_local! {
    pub(crate) static TEST_TODAY: std::cell::Cell<Option<NaiveDate>> = const { std::cell::Cell::new(None) };
}

pub(crate) fn today() -> NaiveDate {
    #[cfg(test)]
    if let Some(date) = TEST_TODAY.with(std::cell::Cell::get) {
        return date;
    }
    chrono::Local::now().date_naive()
}

pub(super) fn period(app: &Application, record: &PayrollTimesheet) -> Result<(NaiveDate, NaiveDate)> {
    let schedule = app
        .payroll_schedule_repository
        .get_for_year_and_cycle(&record.payroll_year, record.cycle_number)?
        .ok_or("Payroll schedule unavailable; cannot attribute evidence")?;
    let start = crate::models::parse_employment_date(&schedule.first_week_commencing)
        .ok_or("Invalid payroll date")?;
    Ok((start, start + chrono::Duration::days(27)))
}
const OVERLAP: &str = "Overlapping historical payroll periods cover the same dates. Resolve the period dates before carrying hours forward.";

struct HistoricalPeriod {
    record: PayrollTimesheet,
    start: NaiveDate,
    end: NaiveDate,
    stage: Stage,
    submission: Option<i64>,
    version: Option<crate::timesheet_amendments::AcceptedAmendment>,
    items: Vec<WorkedItemSnapshot>,
    legacy_aggregate: bool,
}

fn historical_periods(
    app: &Application,
    db: &Connection,
    pa: i64,
) -> Result<Vec<HistoricalPeriod>> {
    let mut history = Vec::new();
    for record in app
        .payroll_timesheet_repository
        .get_all_for_personal_assistant(pa)?
    {
        let submission = lifecycle::latest(db, record.id)?;
        let stage = match lifecycle::stage(db, &record)? {
            Stage::Editable if submission.is_some() => Stage::Submitted,
            Stage::Editable => continue,
            stage => stage,
        };
        let (start, end) = match period(app, &record) {
            Ok(dates) => dates,
            Err(_) => {
                let weeks = match submission {
                    Some(id) => lifecycle::weeks(db, id)?,
                    None => app.payroll_timesheet_repository.get_weeks(record.id)?,
                };
                let Some(start) = weeks.iter().find(|week| week.week_number == 1)
                    .and_then(|week| crate::models::parse_employment_date(&week.week_commencing))
                else { continue };
                if weeks.len() != 4 || !weeks.iter().all(|week| {
                    (1..=4).contains(&week.week_number)
                        && crate::models::parse_employment_date(&week.week_commencing)
                            == Some(start + chrono::Duration::weeks(week.week_number - 1))
                }) {
                    continue;
                }
                (start, start + chrono::Duration::days(27))
            }
        };
        let version = match submission {
            Some(id) => crate::timesheet_amendments::accepted_evidence(db, id)?,
            None => None,
        };
        let items = lifecycle::retained_items(db, record.id, app, submission)?;
        let legacy_aggregate =
            stage == Stage::Settled && super::legacy_baseline::has_settlement(db, record.id)?;
        history.push(HistoricalPeriod {
            record,
            start,
            end,
            stage,
            submission,
            version,
            items,
            legacy_aggregate,
        });
    }
    Ok(history)
}

impl HistoricalPeriod {
    fn overlaps(&self, other: &Self) -> bool {
        self.record.id != other.record.id && self.start <= other.end && other.start <= self.end
    }

    fn contains(&self, date: NaiveDate) -> bool {
        (self.start..=self.end).contains(&date)
    }

    fn has_estimate(&self, date: NaiveDate) -> bool {
        let week = (date - self.start).num_days() / 7 + 1;
        (self.version.is_some() || (self.submission.is_some() && !self.legacy_aggregate))
            && estimate_minutes(&self.items, week, self.version.is_some()).is_some()
    }
}

fn net_corrections(db: &Connection, origin: i64, key: &str) -> Result<i64> {
    Ok(db.query_row("SELECT COALESCE(SUM(minutes),0) FROM payroll_corrections WHERE origin_payroll_timesheet_id=?1 AND evidence_key LIKE ?2",params![origin,format!("{key}|%")],|r|r.get(0))?)
}
// An unchanged raw duration is not a discrepancy against its rounded submitted
// duration. Never apply today's rounding configuration to historical submissions.
fn retained_actual_minutes(
    app: &Application,
    item: &WorkedItemSnapshot,
    current: Option<&WorkEvidence>,
) -> Result<i64> {
    let Some(current) = current else {
        return Ok(0);
    };
    let raw = item
        .source_evidence
        .as_deref()
        .and_then(|s| toml::from_str::<WorkEvidence>(s).ok());
    if raw.is_none_or(|raw| raw.minutes == current.minutes) {
        Ok(item.worked_minutes)
    } else {
        Ok(crate::pay_rate_allocation::payable_minutes_on(current.minutes, current.date()?, &app.context.config.payroll)?)
    }
}

// A retained shift remains associated with its paid week when its date is edited.
// Individual reconciliation accounts for a duration change separately.
fn retained_week(items: &[WorkedItemSnapshot], key: &str) -> Option<i64> {
    items.iter().find_map(|item| {
        (lifecycle::item_key(item).as_deref() == Some(key)).then_some(item.week_number)
    })
}

pub(crate) fn manual_hours_reason(week: NaiveDate, entered_on: NaiveDate) -> &'static str {
    if week > entered_on {
        "Manual current-period estimate"
    } else if week + chrono::Duration::days(6) < entered_on {
        "Manual confirmed worked hours"
    } else {
        "Manual hours for a partly elapsed week"
    }
}

pub(crate) fn manual_target(reason: Option<&str>) -> Option<(&str, i64)> {
    let (label, minutes) = reason?.split_once(" | approved current minutes: ")?;
    Some((label, minutes.parse().ok()?))
}

// An amended week can retain dated actuals alongside its estimate. Require
// a positive undated estimate balance and reject unrelated manual items or carry.
fn estimate_minutes(items: &[WorkedItemSnapshot], week: i64, amended: bool) -> Option<i64> {
    let mut estimate = 0i64;
    let mut actual = 0i64;
    for item in items.iter().filter(|item| item.week_number == week) {
        if amended && item.work_date.is_some() && lifecycle::item_key(item).is_some() {
            actual = actual.checked_add(item.worked_minutes)?;
            continue;
        }
        let estimate_reason = item.reason.as_deref().is_some_and(|reason| {
            // Retain the interpretation of previously accepted legacy
            // amendments. New preparation carries an explicit date-based reason.
            (amended && reason == "Employer amended saved timesheet total")
                || reason.split_whitespace().any(|word| {
                    word.eq_ignore_ascii_case("estimate") || word.eq_ignore_ascii_case("estimated")
                })
        });
        if item.source_type != "manual_adjustment"
            || item.work_date.is_some()
            || lifecycle::item_key(item).is_some()
            || !estimate_reason
        {
            return None;
        }
        estimate = estimate.checked_add(item.worked_minutes)?;
    }
    if estimate <= 0 {
        return None;
    }
    let total = estimate.checked_add(actual)?;
    (total > 0).then_some(total)
}

fn estimate_binding(key: &str) -> Option<(i64, i64)> {
    let mut fields = key
        .split('|')
        .next()?
        .strip_prefix("estimate:week:")?
        .split(':');
    Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
}

// Versions created before Stage 2A could post corrections against an unsent
// document. Retain those rows for audit, without offering their unused balance.
fn correction_origin_accepted(db: &Connection, origin: i64, key: &str) -> rusqlite::Result<bool> {
    let key = key.split('|').next().unwrap_or(key);
    let key = key.strip_prefix("estimate:week:")
        .and_then(|key| key.splitn(3, ':').nth(2))
        .unwrap_or(key);
    let Some(document) = key.strip_prefix("version:")
        .and_then(|key| key.split(':').next())
        .and_then(|value| value.parse::<i64>().ok())
    else {
        return Ok(true);
    };
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM payroll_submissions WHERE payroll_timesheet_id=?1 AND document_id=?2)",
        params![origin, document],
        |row| row.get(0),
    )
}

fn previous_week_correction(key: &str) -> Option<(i64, &str)> {
    let (prefix, week) = key.split('|').next()?.rsplit_once(':')?;
    let (baseline, kind) = prefix.rsplit_once(':')?;
    match (baseline.split_once(':')?.0, kind) {
        ("submission", "aggregate") | ("version", "week") => Some((week.parse().ok()?, baseline)),
        _ => None,
    }
}

fn previous_item_correction(key: &str) -> Option<&str> {
    let (_, item) = key
        .split('|')
        .next()?
        .strip_prefix("submission:")?
        .split_once(':')?;
    Some(item)
}

fn estimate_changes(
    app: &Application,
    db: &Connection,
    record: &PayrollTimesheet,
    items: &[WorkedItemSnapshot],
    current: &[WorkEvidence],
    start: NaiveDate,
    baseline: &str,
    version: Option<&crate::timesheet_amendments::AcceptedAmendment>,
) -> Result<Vec<Component>> {
    let mut recorded_by_week = BTreeMap::<i64, i64>::new();
    if !(1..=4).any(|week| estimate_minutes(items, week, version.is_some()).is_some()) {
        return Ok(Vec::new());
    }
    let mut corrections = db.prepare(
        "SELECT id,evidence_key,minutes,work_date FROM payroll_corrections WHERE origin_payroll_timesheet_id=?1",
    )?;
    for row in corrections.query_map([record.id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })? {
        let (id, key, minutes, date) = row?;
        let (week, active) = if let Some((week, estimated)) = estimate_binding(&key) {
            (
                week,
                estimate_minutes(items, week, version.is_some()) == Some(estimated),
            )
        } else if let Some((week, source_baseline)) = previous_week_correction(&key) {
            // The old amended calculation could post reductions without
            // completeness confirmation. Only previously applied minutes count.
            (
                week,
                source_baseline == baseline && !(version.is_some() && minutes < 0),
            )
        } else if version.is_some()
            && previous_item_correction(&key).is_some_and(|key| {
                items
                    .iter()
                    .any(|item| lifecycle::item_key(item).as_deref() == Some(key))
            })
        {
            let Some(date) = date
                .as_deref()
                .and_then(crate::models::parse_employment_date)
            else {
                continue;
            };
            let day = (date - start).num_days();
            if !(0..28).contains(&day) {
                continue;
            }
            (day / 7 + 1, false)
        } else {
            continue;
        };
        // Retain unused balances for unchanged estimates across amendments.
        // When an estimate changes, only the applied balance offsets it.
        let accounted = if active && correction_origin_accepted(db, record.id, &key)? {
            minutes
        } else {
            lifecycle::submitted_correction_minutes(db, id, Some(record.id))?
        };
        let recorded = recorded_by_week.entry(week).or_default();
        *recorded = recorded
            .checked_add(accounted)
            .ok_or("Recorded correction overflow")?;
    }
    let mut components = Vec::new();
    for week in 1..=4 {
        let Some(estimated) = estimate_minutes(items, week, version.is_some()) else {
            continue;
        };
        let ws = start + chrono::Duration::days((week - 1) * 7);
        let we = ws + chrono::Duration::days(6);
        if we >= today() {
            continue;
        }
        let entries = estimate_evidence(app, db, record, ws, week, items, current)?;
        // An empty completed week can be confirmed as zero worked hours.
        // Evidence held by other payroll or excluded from this estimate is
        // not equivalent to an empty week.
        if entries.is_empty() && current.iter().any(|entry| {
            entry.minutes > 0 && entry.date().is_ok_and(|date| (ws..=we).contains(&date))
        }) {
            continue;
        }
        let before: HashSet<_> = version
            .into_iter()
            .flat_map(|version| &version.evidence)
            .filter(|(number, _, _)| *number == week)
            .map(|(_, key, fingerprint)| (key.clone(), fingerprint.clone()))
            .collect();
        let after: HashSet<_> = entries
            .iter()
            .map(|entry| (entry.key(), entry.fingerprint()))
            .collect();
        let recorded = recorded_by_week.get(&week).copied().unwrap_or_default();
        if !entries.is_empty() && before == after && recorded == 0 {
            continue;
        }
        let mut actual = 0i64;
        for entry in entries {
            let key = entry.key();
            let payable = match items.iter().find(|item| {
                lifecycle::item_key(item).as_deref() == Some(key.as_str())
            }) {
                Some(item) => retained_actual_minutes(app, item, Some(entry))?,
                None => crate::pay_rate_allocation::payable_minutes_on(entry.minutes, entry.date()?, &app.context.config.payroll)?,
            };
            actual = actual
                .checked_add(payable)
                .ok_or("Actual worked hours overflow")?;
        }
        let delta = actual
            .checked_sub(estimated)
            .and_then(|difference| difference.checked_sub(recorded))
            .ok_or("Estimate reconciliation overflow")?;
        if delta == 0 {
            continue;
        }
        let rate = crate::pay_rate_allocation::select_manual_adjustment_rate(
            &app.pay_rate_repository,
            record.personal_assistant_id,
            ws,
            we,
            delta,
        )?;
        components.push(Component {
            key: format!("estimate:week:{week}:{estimated}:{baseline}"),
            source: None,
            source_id: None,
            date: Some(ws.to_string()),
            minutes: delta,
            rate_id: rate.as_ref().map(|rate| rate.id),
            rate_date: rate.as_ref().map(|rate| rate.effective_date.clone()),
            rate: rate.map(|rate| rate.base_hourly_rate + rate.employer_top_up_rate),
            aggregate: true,
        });
    }
    Ok(components)
}

fn unresolved_in_period(
    pre: &Preflight, pa: i64, start: NaiveDate, end: NaiveDate, items: &[WorkedItemSnapshot],
) -> bool {
    pre.unresolved.iter().any(|group| group.candidates.iter().any(|entry| {
        entry.pa == pa && (entry.date().is_ok_and(|date| (start..=end).contains(&date))
            || retained_week(items, &entry.key()).is_some())
    }))
}

pub fn changes(app: &Application, record: &PayrollTimesheet) -> Result<Changes> {
    let db = open(app)?;
    let evidence = load_for_pa(&db, record.personal_assistant_id)?;
    let pre = preflight_for_pa(&db, &evidence, record.personal_assistant_id)?;
    changes_prepared(app, record, &pre)
}

pub(crate) fn changes_prepared(
    app: &Application,
    record: &PayrollTimesheet,
    pre: &Preflight,
) -> Result<Changes> {
    let db = open(app)?;
    changes_in_connection(app, &db, record, pre)
}

fn changes_in_connection(
    app: &Application,
    db: &Connection,
    record: &PayrollTimesheet,
    pre: &Preflight,
) -> Result<Changes> {
    let history = historical_periods(app, db, record.personal_assistant_id)?;
    changes_with_history(app, db, record, pre, &history)
}

fn changes_with_history(
    app: &Application,
    db: &Connection,
    record: &PayrollTimesheet,
    pre: &Preflight,
    history: &[HistoricalPeriod],
) -> Result<Changes> {
    let baseline = history.iter().find(|entry| entry.record.id == record.id);
    if baseline.is_some_and(|entry| history.iter().any(|other| entry.overlaps(other))) {
        return Ok(Changes {
            source_signature: String::new(),
            signature: String::new(),
            changed: false,
            description: OVERLAP.into(),
            components: Vec::new(),
            requires_complete: false,
            blocked: true,
        });
    }
    let (start, end) = match baseline {
        Some(entry) => (entry.start, entry.end),
        None => period(app, record)?,
    };
    if unresolved_in_period(pre, record.personal_assistant_id, start, end,
        baseline.map_or(&[][..], |entry| entry.items.as_slice()))
    {
        return Err("Resolve possible duplicates before reviewing payroll discrepancies".into());
    }
    let current = pre
        .eligible
        .iter()
        .filter(|e| e.pa == record.personal_assistant_id)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(entry) = baseline {
        if let Some(version) = &entry.version {
            return amended_changes(app, db, entry, version, &current);
        }
    }
    let old = baseline.map_or(&[][..], |entry| entry.items.as_slice());
    let sid = baseline.and_then(|entry| entry.submission).unwrap_or(0);
    let baseline_key = format!("submission:{sid}");
    let legacy_aggregate = baseline.is_some_and(|entry| entry.legacy_aggregate);
    let mut components = if sid != 0 && !legacy_aggregate {
        estimate_changes(app, db, record, old, &current, start, &baseline_key, None)?
    } else {
        Vec::new()
    };
    let mut materially_changed = false;
    for item in old {
        let Some(key) = lifecycle::item_key(item) else {
            continue;
        };
        let current_item = current.iter().find(|e| e.key() == key);
        let actual = retained_actual_minutes(app, item, current_item)?;
        let prefix = format!("{baseline_key}:{key}");
        let mut delta = actual - item.worked_minutes - net_corrections(db, record.id, &prefix)?;
        // Once a source correction is recorded, keep that payable result
        // when the effective raw duration has not changed again.
        if delta != 0 {
            if let Some(entry) = current_item {
                if latest_corrected_raw_minutes(db, record.id, &prefix)? == Some(entry.minutes) {
                    delta = 0;
                }
            }
        }
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
    let weeks = if sid != 0 { lifecycle::weeks(db, sid)? } else { Vec::new() };
    for week in weeks {
        let ws = crate::models::parse_employment_date(&week.week_commencing)
            .ok_or("Invalid week date")?;
        let we = ws + chrono::Duration::days(6);
        if we > today() {
            continue;
        }
        let week_items = old
            .iter()
            .filter(|i| i.week_number == week.week_number)
            .collect::<Vec<_>>();
        // A confirmed manual total already is actual evidence. Without dated
        // membership, a later import may overlap it and cannot explain a delta.
        if week_items.iter().any(|item| item.reason.as_deref() == Some("Manual confirmed worked hours")) {
            continue;
        }
        let opaque = week_items.is_empty()
            || week_items
                .iter()
                .any(|i| i.source_type == "manual_adjustment");
        // Cutover aggregates remain authoritative: source membership was never
        // established, so their comparison cannot establish an estimate correction.
        // New historical sources still receive their individual payment review.
        if !opaque || legacy_aggregate
            || (sid != 0 && estimate_minutes(old, week.week_number, false).is_some())
        {
            continue;
        }
        let mut actual = 0i64;
        for e in &current {
            if retained_week(old, &e.key()).is_some()
                || !e.date().is_ok_and(|d| (ws..=we).contains(&d))
            {
                continue;
            }
            if lifecycle::has_decision(db, record.id, "unpaid", &e.fingerprint())?
                || consumed_elsewhere(app, db, record, e)?
            {
                continue;
            }
            let payable = crate::pay_rate_allocation::payable_minutes_on(e.minutes, e.date()?, &app.context.config.payroll)?;
            actual = actual.checked_add(payable).ok_or("Actual worked hours overflow")?;
        }
        let submitted = (week.worked_hours * 60.0).round() as i64;
        let prefix = format!("{baseline_key}:aggregate:{}", week.week_number);
        // Known items and their edits are accounted for by the individual
        // corrections above. Compare only the unallocated portion here.
        let known_paid: i64 = week_items
            .iter()
            .filter(|item| lifecycle::item_key(item).is_some())
            .map(|item| item.worked_minutes)
            .sum();
        let delta = actual - (submitted - known_paid) - net_corrections(db, record.id, &prefix)?;
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
        if lifecycle::stage(db, record)? == Stage::Settled
            && super::legacy_baseline::contains(db, record.id, e)?
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
        if consumed_elsewhere(app, db, record, e)? {
            continue;
        }
        let prefix = format!("{baseline_key}:new:{}", e.key());
        // A newly identified shift uses the same payable rounding as ordinary
        // allocation. The accepted items in this week retain their paid minutes.
        let payable = crate::pay_rate_allocation::payable_minutes_on(e.minutes, e.date()?, &app.context.config.payroll)?;
        let delta = payable - net_corrections(db, record.id, &prefix)?;
        if delta != 0 && latest_corrected_raw_minutes(db, record.id, &prefix)? != Some(e.minutes) {
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
    let mut signature = hash(&format!("{baseline_key}:{}", lifecycle::fingerprint(&current)));
    let source_signature = signature.clone();
    let mut prospective = false;
    for e in &current {
        prospective |= crate::shift_changes::prospective(db, &e.source, e.id)?;
    }
    for item in old {
        if let Some(key) = lifecycle::item_key(item) {
            if let Some((source, id)) = key.split_once(':') {
                if let Ok(id) = id.parse::<i64>() {
                    prospective |= crate::shift_changes::prospective(db, source, id)?;
                }
            }
        }
    }
    if prospective || components.iter().any(|c| c.aggregate && c.minutes < 0) {
        signature = hash(&format!("{signature}:{components:?}"));
    }

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
        source_signature: source_signature.clone(),
        signature: signature.clone(),
        description,
        changed: (materially_changed
            && lifecycle::stage(db, record)? != Stage::Settled
            && !lifecycle::has_decision(db, record.id, "carry", &source_signature)?)
            || !components.is_empty(),
        components,
        requires_complete,
        blocked: false,
    })
}
// Unchanged source exports are retained historical facts, not permission to
// overwrite the employer's corrected total. Reconcile only later material source
// changes against the accepted version using the established carry engine.
fn amended_changes(
    app: &Application,
    db: &Connection,
    baseline: &HistoricalPeriod,
    version: &crate::timesheet_amendments::AcceptedAmendment,
    current: &[WorkEvidence],
) -> Result<Changes> {
    let record = &baseline.record;
    let sid = baseline.submission.ok_or("Accepted amendment has no submission")?;
    let mut components = estimate_changes(
        app,
        db,
        record,
        &baseline.items,
        current,
        baseline.start,
        &format!("version:{}", version.document),
        Some(version),
    )?;
    for week in lifecycle::weeks(db, sid)? {
        if estimate_minutes(&baseline.items, week.week_number, true).is_some() {
            continue;
        }
        if baseline.items.iter().any(|item| item.week_number == week.week_number
            && item.reason.as_deref() == Some("Manual confirmed worked hours"))
        {
            continue;
        }
        let submitted = (week.worked_hours * 60.0).round() as i64;
        let ws = crate::date_utils::parse_legacy(&week.week_commencing)?;
        let we = ws + chrono::Duration::days(6);
        if we > today() {
            continue;
        }
        let mut before = version
            .evidence
            .iter()
            .filter(|(w, key, _)| {
                retained_week(&baseline.items, key).unwrap_or(*w) == week.week_number
            })
            .map(|(_, key, fp)| (key.clone(), fp.clone()))
            .collect::<Vec<_>>();
        let entries = current
            .iter()
            .filter(|e| {
                let key = e.key();
                retained_week(&baseline.items, &key)
                    .or_else(|| {
                        version.evidence.iter().find_map(|(week, source, _)| {
                            (*source == key).then_some(*week)
                        })
                    })
                    .map_or_else(
                        || e.date().is_ok_and(|d| (ws..=we).contains(&d)),
                        |retained| retained == week.week_number,
                    )
            })
            .collect::<Vec<_>>();
        let mut after = entries
            .iter()
            .map(|e| (e.key(), e.fingerprint()))
            .collect::<Vec<_>>();
        before.sort();
        after.sort();
        // CSV import and clock-out both append source audit events. Their
        // presence is provenance, never a veto on newly acquired actual hours.
        let actual = if before == after {
            submitted
        } else {
            // Keep the accepted total intact when the only changes are new
            // entries. The accepted total can include a manual downward amendment.
            let mut original_unchanged = true;
            for (key, fingerprint) in &before {
                if after.iter().any(|fact| &fact.0 == key && &fact.1 == fingerprint) {
                    continue;
                }
                let retained = baseline.items.iter().find(|item| {
                    lifecycle::item_key(item).as_deref() == Some(key.as_str())
                });
                let current = entries.iter().find(|entry| &entry.key() == key);
                match (retained, current) {
                    (Some(item), Some(entry))
                        if item.source_evidence.as_deref()
                            .and_then(|raw| toml::from_str::<WorkEvidence>(raw).ok())
                            .is_some_and(|raw| &raw.fingerprint() == fingerprint)
                            && retained_actual_minutes(app, item, Some(entry))? == item.worked_minutes => {}
                    _ => {
                        original_unchanged = false;
                        break;
                    }
                }
            }
            let mut minutes = if original_unchanged { submitted } else { 0 };
            for e in entries {
                let key = e.key();
                let retained = baseline.items.iter().find(|item| {
                    lifecycle::item_key(item).as_deref() == Some(key.as_str())
                });
                if (original_unchanged
                    && (retained.is_some() || before.contains(&(key, e.fingerprint()))))
                    || consumed_elsewhere(app, db, record, e)?
                {
                    continue;
                }
                let payable = match retained {
                    Some(item) => retained_actual_minutes(app, item, Some(e))?,
                    None => crate::pay_rate_allocation::payable_minutes_on(e.minutes, e.date()?, &app.context.config.payroll)?,
                };
                minutes = minutes.checked_add(payable).ok_or("Worked hours overflow")?;
            }
            minutes
        };
        let key = format!("version:{}:week:{}", version.document, week.week_number);
        let delta = actual - submitted - net_corrections(db, record.id, &key)?;
        if delta != 0 {
            components.push(Component {
                key,
                source: None,
                source_id: None,
                date: Some(ws.to_string()),
                minutes: delta,
                rate_id: None,
                rate_date: None,
                rate: None,
                aggregate: delta < 0,
            });
        }
    }
    let signature = hash(&format!(
        "{}:{:?}:{:?}",
        version.document,
        lifecycle::fingerprint(current),
        components
    ));
    let requires_complete = components.iter().any(|c| c.aggregate && c.minutes < 0);
    let description = components
        .iter()
        .map(|c| format!("{}: {:+.2} hours", c.key, c.minutes as f64 / 60.0))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Changes {
        source_signature: signature.clone(),
        signature,
        changed: !components.is_empty(),
        description: format!(
            "Accepted submission {sid} (document {}) supplies the baseline.\n{description}",
            version.document
        ),
        components,
        requires_complete,
        blocked: false,
    })
}

fn consumed_elsewhere(
    app: &Application,
    db: &Connection,
    record: &PayrollTimesheet,
    e: &WorkEvidence,
) -> Result<bool> {
    if crate::timesheet_delivery::work_reserved(db, record.id, &e.source, e.id)? {
        return Ok(true);
    }
    for r in app
        .payroll_timesheet_repository
        .get_all_for_personal_assistant(e.pa)?
    {
        if r.id != record.id
            && (lifecycle::stage(db, &r)? != Stage::Editable
                || lifecycle::latest(db, r.id)?.is_some())
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
    db.execute("INSERT INTO payroll_corrections(personal_assistant_id,origin_payroll_timesheet_id,submission_id,decision_id,evidence_key,source,source_id,work_date,minutes,pay_rate_id,pay_rate_effective_date,total_hourly_rate,reason,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",params![record.personal_assistant_id,record.id,sid,decision,format!("{}|{signature}|{serial}",c.key),c.source,c.source_id,c.date,c.minutes,c.rate_id,c.rate_date,c.rate,if estimate_binding(&c.key).is_some() && decision.is_none() {"Automatic estimate reconciliation"} else if c.aggregate {"Confirmed aggregate estimate reconciliation"} else {"Audited submitted/actual evidence difference"},now()])?;
    Ok(())
}
fn latest_corrected_raw_minutes(db: &Connection, origin: i64, key: &str) -> Result<Option<i64>> {
    let evidence = db
        .query_row(
            "SELECT e.evidence FROM payroll_corrections c
             LEFT JOIN payroll_correction_evidence e ON e.correction_id=c.id
                 AND e.source=c.source AND e.source_id=c.source_id
             WHERE c.origin_payroll_timesheet_id=?1 AND c.evidence_key LIKE ?2
             ORDER BY c.id DESC LIMIT 1",
            params![origin, format!("{key}|%")],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    Ok(evidence
        .as_deref()
        .map(toml::from_str::<WorkEvidence>)
        .transpose()?
        .map(|entry| entry.minutes))
}

fn archive_source_correction(
    db: &Connection,
    correction: i64,
    component: &Component,
    eligible: &[WorkEvidence],
) -> Result<()> {
    if let Some(entry) = eligible.iter().find(|entry| {
        component.source.as_deref() == Some(entry.source.as_str())
            && component.source_id == Some(entry.id)
    }) {
        db.execute(
            "INSERT INTO payroll_correction_evidence VALUES (?1,?2,?3,?4,?5)",
            params![
                correction,
                entry.source,
                entry.id,
                entry.fingerprint(),
                toml::to_string(entry)?
            ],
        )?;
    }
    Ok(())
}

fn estimate_evidence<'a>(
    app: &Application,
    db: &Connection,
    record: &PayrollTimesheet,
    start: NaiveDate,
    week: i64,
    items: &[WorkedItemSnapshot],
    eligible: &'a [WorkEvidence],
) -> Result<Vec<&'a WorkEvidence>> {
    let end = start + chrono::Duration::days(6);
    let mut actuals = Vec::new();
    for entry in eligible.iter().filter(|entry| {
        entry.pa == record.personal_assistant_id
            && entry.minutes > 0
            && retained_week(items, &entry.key()).map_or_else(
                || entry.date().is_ok_and(|date| (start..=end).contains(&date)),
                |retained| retained == week,
            )
    }) {
        if !lifecycle::has_decision(db, record.id, "unpaid", &entry.fingerprint())?
            && !consumed_elsewhere(app, db, record, entry)?
        {
            actuals.push(entry);
        }
    }
    Ok(actuals)
}

fn archive_estimate_evidence(
    app: &Application,
    db: &Connection,
    record: &PayrollTimesheet,
    component: &Component,
    correction: i64,
    eligible: &[WorkEvidence],
) -> Result<()> {
    let start = crate::date_utils::parse_legacy(
        component
            .date
            .as_deref()
            .ok_or("Missing estimate week date")?,
    )?;
    let (week, _) = estimate_binding(&component.key).ok_or("Invalid estimate portion")?;
    let submission = lifecycle::latest(db, record.id)?;
    let items = lifecycle::retained_items(db, record.id, app, submission)?;
    for entry in estimate_evidence(app, db, record, start, week, &items, eligible)? {
        db.execute(
            "INSERT INTO payroll_correction_evidence VALUES (?1,?2,?3,?4,?5)",
            params![
                correction,
                entry.source,
                entry.id,
                entry.fingerprint(),
                toml::to_string(entry)?
            ],
        )?;
    }
    Ok(())
}

pub fn carry(
    app: &Application,
    record: &PayrollTimesheet,
    expected: &str,
    complete: bool,
) -> Result<()> {
    let db = open(app)?;
    let _guard = crate::timesheet_delivery::production_lock(&db)?;
    let tx = db.unchecked_transaction()?;
    tx.execute("UPDATE schema_version SET version=version", [])?;
    let evidence = load_for_pa(&tx, record.personal_assistant_id)?;
    let pre = preflight_for_pa(&tx, &evidence, record.personal_assistant_id)?;
    let change = changes_in_connection(app, &tx, record, &pre)?;

    if !matches!(
        lifecycle::stage(&tx, record)?,
        Stage::Submitted | Stage::Settled
    ) {
        return Err("Only submitted/settled payroll can carry a correction".into());
    }
    if change.blocked {
        return Err(change.description.into());
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
    let decision = lifecycle::record_decision(
        &tx,
        record.id,
        sid,
        "carry",
        &change.source_signature,
        &change.description,
    )?;
    let (start, _) = period(app, record)?;
    let items = lifecycle::retained_items(&tx, record.id, app, sid)?;
    for c in &change.components {
        insert_component(&tx, record, sid, Some(decision), c, expected)?;
        let correction = tx.last_insert_rowid();
        if c.source.is_some() {
            archive_source_correction(&tx, correction, c, &pre.eligible)?;
        } else if estimate_binding(&c.key).is_some() {
            archive_estimate_evidence(app, &tx, record, c, correction, &pre.eligible)?;
        } else if c.aggregate {
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
                    && retained_week(&items, &e.key()).is_none_or(|paid_week| paid_week == week)
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
    target: PayrollTimesheet,
    submission: Option<i64>,
    pub evidence: WorkEvidence,
}
impl HistoricalReview {
    pub(crate) fn same_evidence(&self, other: &Self) -> bool {
        self.record.id == other.record.id
            && self.submission == other.submission
            && self.evidence.key() == other.evidence.key()
            && self.evidence.fingerprint() == other.evidence.fingerprint()
    }
}

#[derive(Clone, Copy)]
pub(crate) enum HistoricalDecision {
    AlreadyPaid,
    Unpaid,
    Dismiss,
}

const HISTORICAL_DISMISSAL: &str = "historical_payment_dismissal";

pub(crate) fn historical_decision(
    app: &Application,
    review: &HistoricalReview,
    decision: HistoricalDecision,
) -> Result<()> {
    let db = open(app)?;
    let tx = rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)?;
    let evidence = load_for_pa(&tx, review.record.personal_assistant_id)?;
    let pre = preflight_for_pa(&tx, &evidence, review.record.personal_assistant_id)?;
    let fresh = plan_in_connection(app, &tx, &review.target, &pre)?;
    let current = fresh
        .historical_reviews
        .iter()
        .find(|candidate| candidate.same_evidence(review))
        .map(|candidate| &candidate.evidence)
        .ok_or("Historical payroll or evidence changed; refresh review")?;
    let key = current.fingerprint();
    if matches!(decision, HistoricalDecision::Dismiss) {
        tx.execute(
            "INSERT INTO shift_change_events(source,source_id,before_evidence,after_evidence,reason,review_signature,recorded_at,actor)
             VALUES(?1,?2,'',?3,'Dismissed historical payment review; no payroll adjustment',?4,?5,'local_employer')",
            params![HISTORICAL_DISMISSAL, review.record.id, toml::to_string(current)?, key, now()],
        )?;
        tx.commit()?;
        return Ok(());
    }
    let paid = matches!(decision, HistoricalDecision::AlreadyPaid);
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

// Older accepted PDFs can contain a positive manual weekly amount without
// saying which intervals it covered or even whether it was an estimate.
// Unmatched source IDs are not evidence of extra payable hours in that case.
fn covered_by_opaque_week(
    app: &Application,
    db: &Connection,
    period: &HistoricalPeriod,
    date: NaiveDate,
    eligible: &[WorkEvidence],
) -> Result<bool> {
    let Some(submission) = period.submission else { return Ok(false) };
    let week = (date - period.start).num_days() / 7 + 1;
    if !period.items.iter().any(|item| {
        item.week_number == week && item.source_type == "manual_adjustment"
            && item.work_date.is_none() && lifecycle::item_key(item).is_none()
            && item.worked_minutes > 0
    }) {
        return Ok(false);
    }
    let Some(accepted) = lifecycle::weeks(db, submission)?.into_iter()
        .find(|row| row.week_number == week)
    else { return Ok(false) };
    let carried: i64 = period.items.iter().filter(|item| {
        item.week_number == week && (matches!(item.source_type.as_str(),
            "carry_correction" | "legacy_previous_cycle_adjustment" | "previous_cycle_late_shift")
            || item.work_date.as_deref().and_then(crate::models::parse_employment_date)
                .is_some_and(|day| day < period.start))
    }).map(|item| item.worked_minutes).sum();
    let start = period.start + chrono::Duration::weeks(week - 1);
    let end = start + chrono::Duration::days(6);
    let mut observed = 0i64;
    for entry in eligible.iter().filter(|entry| entry.pa == period.record.personal_assistant_id) {
        let day = entry.date()?;
        if (start..=end).contains(&day) {
            let Ok(minutes) = crate::pay_rate_allocation::payable_minutes_on(
                entry.minutes, day, &app.context.config.payroll,
            ) else { return Ok(false) };
            let Some(total) = observed.checked_add(minutes) else { return Ok(false) };
            observed = total;
        }
    }
    let accepted_minutes = (accepted.worked_hours * 60.0).round() as i64;
    // Negative carry can reduce the paid total but cannot establish extra work.
    Ok(observed > 0 && observed <= accepted_minutes.saturating_sub(carried.max(0)))
}

#[derive(Clone)]
pub struct Plan {
    pub work: Vec<WorkEvidence>,
    pub historical_reviews: Vec<HistoricalReview>,
    pub notices: Vec<String>,
    pub overlap: bool,
}
pub fn plan(app: &Application, record: &PayrollTimesheet) -> Result<Plan> {
    let db = open(app)?;
    let all = load_for_pa(&db, record.personal_assistant_id)?;
    let pre = preflight_for_pa(&db, &all, record.personal_assistant_id)?;
    plan_prepared(app, record, &pre)
}

pub(crate) fn plan_prepared(
    app: &Application,
    record: &PayrollTimesheet,
    pre: &Preflight,
) -> Result<Plan> {
    let db = open(app)?;
    plan_in_connection(app, &db, record, pre)
}

fn plan_in_connection(
    app: &Application,
    db: &Connection,
    record: &PayrollTimesheet,
    pre: &Preflight,
) -> Result<Plan> {
    let (start, end) = period(app, record)?;
    if unresolved_in_period(pre, record.personal_assistant_id, start, end, &[]) {
        return Err("Unresolved possible duplicate shifts; open Payroll Timesheet Preparation to select candidates".into());
    }
    let history = historical_periods(app, db, record.personal_assistant_id)?;
    let mut plan = Plan {
        work: Vec::new(),
        historical_reviews: Vec::new(),
        notices: Vec::new(),
        overlap: false,
    };
    if pre.unresolved.iter().any(|group| group.candidates.iter().any(|entry| {
        entry.pa == record.personal_assistant_id && entry.date().is_ok_and(|date| date < start)
    })) {
        plan.notices.push("Unresolved historical duplicate work is excluded from current preparation.".into());
    }
    let mut unmapped_history = false;
    for prior in app.payroll_timesheet_repository.get_all_for_personal_assistant(record.personal_assistant_id)? {
        if prior.id != record.id && history.iter().all(|entry| entry.record.id != prior.id)
            && (lifecycle::latest(db, prior.id)?.is_some() || lifecycle::stage(db, &prior)? != Stage::Editable)
        {
            unmapped_history = true;
            break;
        }
    }
    if unmapped_history {
        plan.notices.push("Some historical payroll dates could not be verified. Unattributed historical hours are excluded from current preparation.".into());
    }
    let mut payslip_cycles = BTreeMap::new();
    for e in pre
        .eligible
        .iter()
        .filter(|e| e.pa == record.personal_assistant_id && e.minutes > 0)
    {
        let date = e.date()?;
        if date > end {
            continue;
        }
        let mut candidates = history
            .iter()
            .filter(|entry| entry.record.id != record.id && entry.contains(date));
        let previous = candidates.next();
        if candidates.next().is_some()
            || previous.is_some_and(|entry| history.iter().any(|other| entry.overlaps(other)))
            || (previous.is_some() && date >= start)
        {
            plan.overlap = true;
            continue;
        }
        let corrected:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_corrections WHERE personal_assistant_id=?1 AND source=?2 AND source_id=?3)",params![e.pa,e.source,e.id],|r|r.get(0))?;
        if corrected {
            continue;
        }
        let mut coverage = db.prepare(
            "SELECT c.id,c.origin_payroll_timesheet_id,c.evidence_key
             FROM payroll_correction_evidence e JOIN payroll_corrections c ON c.id=e.correction_id
             WHERE e.source=?1 AND e.source_id=?2 AND e.fingerprint=?3",
        )?;
        let mut covered = false;
        for row in coverage.query_map(params![e.source, e.id, e.fingerprint()], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })? {
            let (id, origin, key) = row?;
            if correction_origin_accepted(db, origin, &key)?
                || lifecycle::submitted_correction_minutes(db, id, None)? != 0
            {
                covered = true;
                break;
            }
        }
        if covered {
            continue;
        }
        // A retained identity remains paid even if a later edit moves its date.
        let key = e.key();
        if history.iter().any(|entry| {
            entry.record.id != record.id
                && entry
                    .items
                    .iter()
                    .any(|item| lifecycle::item_key(item).as_deref() == Some(key.as_str()))
        }) {
            continue;
        }
        if crate::timesheet_delivery::work_reserved(db, record.id, &e.source, e.id)? {
            plan.notices.push(format!("{key}: work reserved by an unresolved delivery attempt"));
            continue;
        }
        if let Some(prior) = previous.filter(|prior| prior.stage == Stage::Settled) {
            if super::legacy_baseline::contains(db, prior.record.id, e)? {
                continue;
            }
        }
        if date < start && previous.is_none_or(|entry| {
            entry.submission.is_none() && entry.items.is_empty()
                && entry.stage != Stage::Indeterminate
        }) {
            if let Ok(schedule) = app.payroll_schedule_repository.resolve_for_date(date) {
                let key = (schedule.payroll_year.clone(), schedule.cycle_number);
                let (paid, observed) = payslip_cycles.entry(key).or_insert_with(|| (
                    super::payslip_hours::paid_hours(app, db, record.personal_assistant_id, &schedule), Some(0i64),
                ));
                if !matches!(paid, super::payslip_hours::Evidence::Missing) {
                    *observed = observed.and_then(|sum| {
                        let payable = crate::pay_rate_allocation::payable_minutes_on(
                            e.minutes, date, &app.context.config.payroll,
                        ).ok()?;
                        sum.checked_add(payable)
                    });
                    continue;
                }
            }
        }
        let Some(previous) = previous else {
            if date < start && unmapped_history {
                continue;
            }
            if date >= start && unmapped_history && consumed_elsewhere(app, db, record, e)? {
                continue;
            }
            plan.work.push(e.clone());
            continue;
        };
        let prior = &previous.record;
        let fingerprint = e.fingerprint();
        if previous.stage == Stage::Indeterminate {
            plan.notices.push(format!(
                "{key}: historical delivery is indeterminate; evidence held"
            ));
        } else if previous.version.is_some()
            || (previous.has_estimate(date)
                && !lifecycle::has_decision(db, prior.id, "unpaid", &fingerprint)?)
        {
            // These shifts are accounted for by the historical baseline.
        } else if previous.stage == Stage::Submitted {
            plan.notices.push(format!("{key}: original timesheet was sent; amend the original preparation and submit the replacement to use corrected hours"));
        } else if previous.stage == Stage::Settled {
            let unpaid = lifecycle::has_decision(db, prior.id, "unpaid", &fingerprint)?;
            let exact = lifecycle::has_exact_intervals(&previous.items);
            if exact && crate::shift_changes::prospective(db, &e.source, e.id)? && !unpaid {
                plan.notices.push(format!("{key}: new historical evidence requires reviewed Carry correction forward in its original payroll preparation"));
            } else if exact || !lifecycle::has_decision(db, prior.id, "already_paid", &fingerprint)? {
                if exact || unpaid {
                    plan.work.push(e.clone());
                } else if !covered_by_opaque_week(app, db, previous, date, &pre.eligible)?
                    && !db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM shift_change_events WHERE source=?1 AND source_id=?2 AND review_signature=?3)",
                        params![HISTORICAL_DISMISSAL, prior.id, fingerprint],
                        |row| row.get::<_, bool>(0),
                    )? {
                    plan.historical_reviews.push(HistoricalReview {
                        record: prior.clone(),
                        target: record.clone(),
                        submission: previous.submission,
                        evidence: e.clone(),
                    });
                }
            }
        }
    }
    for ((year, cycle), (paid, observed)) in payslip_cycles {
        use super::payslip_hours::Evidence;
        let message = match paid {
            Evidence::PaidMinutes(paid) => {
                let actual = observed.map_or_else(|| "unverified actual hours".to_string(),
                    |minutes| format!("{:.2} hours in currently unmatched actual records", minutes as f64 / 60.0));
                format!("Historical payslip for {year}, cycle {cycle}: {:.2} paid working hours and {actual}. The payslip supplies a cycle total. No weekly adjustment was inferred.", paid as f64 / 60.0)
            },
            Evidence::Unreadable => format!(
                "Historical payslip for {year}, cycle {cycle}: paid working hours could not be verified. Those historical hours are excluded from current preparation.",
            ),
            Evidence::Missing => continue,
        };
        plan.notices.push(message);
    }
    if !plan.overlap {
        let overlaps = history
            .iter()
            .filter(|entry| history.iter().any(|other| entry.overlaps(other)))
            .map(|entry| entry.record.id)
            .collect::<HashSet<_>>();
        if !overlaps.is_empty() {
            let mut statement = db.prepare(
                "SELECT id,origin_payroll_timesheet_id,minutes FROM payroll_corrections WHERE personal_assistant_id=?1",
            )?;
            for row in statement.query_map([record.personal_assistant_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })? {
                let (id, origin, minutes) = row?;
                if overlaps.contains(&origin)
                    && origin != record.id
                    && minutes != lifecycle::submitted_correction_minutes(db, id, Some(record.id))?
                {
                    plan.overlap = true;
                    break;
                }
            }
        }
    }
    if plan.overlap {
        plan.notices.push(OVERLAP.into());
    }
    Ok(plan)
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
    let rows=db.prepare("SELECT id,minutes,work_date,pay_rate_id,pay_rate_effective_date,total_hourly_rate,reason FROM payroll_corrections WHERE personal_assistant_id=?1 AND origin_payroll_timesheet_id<>?2 ORDER BY created_at,id")?.query_map(params![record.personal_assistant_id,record.id],|r|Ok(CorrectionApplication{id:r.get(0)?,minutes:r.get(1)?,date:r.get(2)?,rate_id:r.get(3)?,rate_date:r.get(4)?,rate:r.get(5)?,reason:r.get(6)?}))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let (target_start, _) = period(app, record)?;
    let history = historical_periods(app, &db, record.personal_assistant_id)?;
    let mut current = Vec::new();
    for mut correction in rows {
        let (origin, key): (i64, String) = db.query_row(
            "SELECT origin_payroll_timesheet_id,evidence_key FROM payroll_corrections WHERE id=?1",
            [correction.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if !correction_origin_accepted(&db, origin, &key)? {
            continue;
        }
        let baseline = history.iter().find(|entry| entry.record.id == origin);
        if baseline.is_some_and(|entry| history.iter().any(|other| entry.overlaps(other))) {
            continue;
        }
        let version = baseline.and_then(|entry| entry.version.as_ref());
        if let Some((week, estimated)) = estimate_binding(&key) {
            let prior_items = baseline.map_or(&[][..], |entry| entry.items.as_slice());
            if estimate_minutes(prior_items, week, version.is_some()) != Some(estimated)
                || correction.date.as_deref().is_some_and(|date| {
                    crate::models::parse_employment_date(date)
                        .is_some_and(|date| date + chrono::Duration::days(6) >= target_start)
                })
            {
                continue;
            }
        } else if let Some(version) = &version {
            if !key.starts_with(&format!("version:{}:", version.document))
                || (correction.minutes < 0
                    && previous_week_correction(&key).is_some_and(|(week, _)| {
                        baseline.is_some_and(|entry| estimate_minutes(&entry.items, week, true).is_some())
                    }))
            {
                continue;
            }
        }
        if crate::timesheet_delivery::correction_reserved(&db, record.id, correction.id)? {
            continue;
        }
        correction.minutes -= lifecycle::submitted_correction_minutes(&db, correction.id, Some(record.id))?;
        current.push(correction);
    }
    let mut rows = current;
    rows.retain(|c| c.minutes != 0);
    Ok(rows)
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

/// Generation deliberately recalculates submitted payroll from current source data.
pub fn calculate_for_generation(
    app: &Application,
    record: &PayrollTimesheet,
    weeks: &[NaiveDate; 4],
    legacy: Option<&crate::pay_rate_allocation::PreviousCycleContext>,
) -> Result<crate::pay_rate_allocation::ReconciledPayrollHours> {
    if crate::timesheet_amendments::saved(&*open(app)?,record.id)? {
        lifecycle::ensure_generatable(&*open(app)?,record.id)?;
        return crate::timesheet_amendments::hours(app,record);
    }
    if let Some(draft) = super::manual_preparation::latest(&*open(app)?, record.id)? {
        return super::manual_preparation::generate(app, record, weeks, &draft);
    }
    if let Some(hours) = crate::sickness_service::historical_hours(&*open(app)?, record.id)? {
        lifecycle::ensure_generatable(&*open(app)?, record.id)?;
        return Ok(hours);
    }
    if lifecycle::stage(&*open(app)?, record)? == Stage::Editable {
        return calculate(app, record, weeks, legacy);
    }
    lifecycle::ensure_generatable(&*open(app)?, record.id)?;
    sync_settled_corrections(app, record.personal_assistant_id)?;
    let plan = plan(app, record)?;
    calculate_plan(app, record, weeks, legacy, &plan)
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
    calculate_plan(app, record, weeks, legacy, plan)
}

fn calculate_plan(
    app: &Application,
    record: &PayrollTimesheet,
    weeks: &[NaiveDate; 4],
    legacy: Option<&crate::pay_rate_allocation::PreviousCycleContext>,
    plan: &Plan,
) -> Result<crate::pay_rate_allocation::ReconciledPayrollHours> {
    let db = open(app)?;
    // Preserve the established opaque historical-backfill path, not its old
    // previous-cycle-only membership scan. Dated late work is selected above.
    let empty: Vec<crate::models::TimesheetEntry> = Vec::new();
    let adjustments = app.payroll_worked_item_repository.get_manual_adjustments(record.id)?;
    let historical_backfill = adjustments
        .iter()
        .any(|a| {
            crate::historical_adjustment_compatibility::is_historical_adjustment_reason(
                a.reason.as_deref(),
            )
        });
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
    for adjustment in &adjustments {
        if let Some((label, minutes)) = manual_target(adjustment.reason.as_deref()) {
            let index = (adjustment.week_number - 1) as usize;
            crate::pay_rate_allocation::apply_manual_target(
                &app.pay_rate_repository, &mut result, record.personal_assistant_id,
                weeks[index], index, minutes, label,
            )?;
        }
    }
    let corrections = outstanding(app, record)?;
    let applications = apply_balances(&mut result, &corrections)?;
    reserve_applications(&db, record.id, applications)?;
    Ok(result)
}

pub(super) fn apply_balances(
    result: &mut crate::pay_rate_allocation::ReconciledPayrollHours,
    corrections: &[CorrectionApplication],
) -> Result<Vec<(i64, i64)>> {
    let credits = corrections.iter().filter(|correction| correction.minutes > 0);
    let deductions = corrections.iter().filter(|correction| correction.minutes < 0);
    // Preserve preparations where credits cover a signed manual edit. Only
    // then allow approved deductions to take Week 1 below zero.
    let week_one_with_credits = credits.clone()
        .try_fold(result.week_totals_minutes[0], |total, correction| {
            total.checked_add(correction.minutes)
        })
        .ok_or("Worked hours overflow")?;
    if week_one_with_credits < 0 || result.week_totals_minutes[1..].iter().any(|minutes| *minutes < 0) {
        return Err("Negative underlying worked hours require review".into());
    }
    let mut applications = Vec::new();
    for correction in credits.chain(deductions) {
        let minutes = correction.minutes;
        result.week_totals_minutes[0] = result.week_totals_minutes[0]
            .checked_add(minutes)
            .ok_or("Worked hours overflow")?;
        result.previous_cycle_minutes = result.previous_cycle_minutes
            .checked_add(minutes)
            .ok_or("Previous-cycle hours overflow")?;
        applications.push((correction.id, minutes));
        result.snapshot_items.push(WorkedItemSnapshot {
            week_number: 1,
            source_type: "carry_correction".into(),
            timesheet_id: None,
            direct_shift_id: None,
            source_evidence: None,
            work_date: correction.date.clone(),
            worked_minutes: minutes,
            pay_rate_id: correction.rate_id,
            pay_rate_effective_date: correction.rate_date.clone(),
            total_hourly_rate: correction.rate,
            reason: Some(format!("Correction {}: {}", correction.id, correction.reason)),
        });
        result.weeks[0].push(crate::pay_rate_allocation::PayRatePortion {
            worked_minutes: minutes,
            total_hourly_rate: correction.rate,
            effective_date: correction.date.as_deref().and_then(crate::models::parse_employment_date),
            rate_id: correction.rate_id,
            is_previous_cycle: true,
            is_opaque_legacy: correction.rate.is_none(),
        });
    }
    Ok(applications)
}

pub(super) fn reserve_applications(
    db: &Connection,
    record: i64,
    applications: Vec<(i64, i64)>,
) -> Result<()> {
    // Reservations are candidates, not consumption. Submitted applications are
    // copied into immutable history only after successful production send.
    let tx = db.unchecked_transaction()?;
    let existing=tx.prepare("SELECT correction_id,minutes FROM payroll_correction_applications WHERE payroll_timesheet_id=?1 ORDER BY correction_id")?.query_map([record],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut ordered = applications;
    ordered.sort();
    if existing != ordered {
        tx.execute(
            "DELETE FROM payroll_correction_applications WHERE payroll_timesheet_id=?1",
            [record],
        )?;
        for (id, minutes) in ordered {
            tx.execute(
                "INSERT INTO payroll_correction_applications VALUES (?1,?2,?3)",
                params![record, id, minutes],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// A concrete audited change to settled source evidence is its own authority.
/// A closed estimate week can generate an automatic credit from observed work.
/// A deduction requires confirmation of complete actuals.
pub fn sync_settled_corrections(app: &Application, pa: i64) -> Result<()> {
    let db = open(app)?;
    // Resolve periods and post differences under one writer transaction.
    let tx = rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)?;
    let history = historical_periods(app, &tx, pa)?;
    let candidates = history
        .iter()
        .filter(|entry| {
            !entry.items.is_empty()
                && (entry.stage == Stage::Settled
                    || (entry.stage == Stage::Submitted
                        && entry.submission.is_some()
                        && (entry.version.is_some()
                            || (1..=4).any(|week| {
                                estimate_minutes(&entry.items, week, false).is_some()
                            }))))
                && !history.iter().any(|other| entry.overlaps(other))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(());
    }
    let all = load_for_pa(&tx, pa)?;
    let pre = preflight_for_pa(&tx, &all, pa)?;
    for entry in candidates {
        if unresolved_in_period(&pre, pa, entry.start, entry.end, &entry.items) {
            continue;
        }
        let record = &entry.record;
        let authority = &entry.version;
        let old = &entry.items;
        let submission = entry.submission;
        let stage = entry.stage;
        let changes = changes_with_history(app, &tx, record, &pre, &history)?;
        let exact = lifecycle::has_exact_intervals(old);
        for c in &changes.components {
            if estimate_binding(&c.key).is_some() {
                if (submission.is_some() || authority.is_some()) && c.minutes > 0 {
                    insert_component(&tx, record, submission, None, c, &changes.signature)?;
                    archive_estimate_evidence(
                        app,
                        &tx,
                        record,
                        c,
                        tx.last_insert_rowid(),
                        &pre.eligible,
                    )?;
                }
                continue;
            }
            if c.aggregate || (stage == Stage::Submitted && authority.is_none()) {
                continue;
            }
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
            // Stage 4 approvals concern source changes only. Financial carry-forward
            // still requires the existing explicit historical reconciliation action.
            let prospective = match (&c.source, c.source_id) {
                (Some(source), Some(id)) => crate::shift_changes::prospective(&tx, source, id)?,
                _ => false,
            };
            if (exact || known_edit || authority.is_some()) && !prospective {
                insert_component(&tx, record, submission, None, c, &changes.signature)?;
                if c.source.is_some() {
                    archive_source_correction(&tx, tx.last_insert_rowid(), c, &pre.eligible)?;
                }
            }
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn historical_save_notice(
    app: &Application,
    pa: i64,
    date: NaiveDate,
) -> Result<Option<String>> {
    let db = open(app)?;
    let history = historical_periods(app, &db, pa)?;
    let mut matches = history.iter().filter(|entry| entry.contains(date));
    if let Some(entry) = matches.next() {
        if matches.next().is_some() || history.iter().any(|other| entry.overlaps(other)) {
            return Ok(Some(OVERLAP.into()));
        }
        if entry.stage == Stage::Settled {
            if entry.version.is_some() {
                return Ok(Some("Actual hours saved. Subsequent payroll reconciles against the latest corrected timesheet; the historical submission does not lock new hours.".into()));
            }
            return Ok(Some("This shift belongs to a payroll period whose payslip has already been sent. Review it against paid evidence in the original Payroll Timesheet Preparation and explicitly approve any correction to carry forward. Settled payroll remains unchanged.".into()));
        }
    }
    Ok(None)
}

pub fn audit_lines(app: &Application, record: &PayrollTimesheet) -> Result<Vec<String>> {
    let db = open(app)?;
    let mut lines = Vec::new();
    if crate::timesheet_amendments::saved(&db,record.id)? {
        let mut revisions=db.prepare("SELECT id,event,document_id,recorded_at,actor,before_state,after_state FROM timesheet_amendment_versions WHERE payroll_timesheet_id=?1 ORDER BY id")?;
        for row in revisions.query_map([record.id],|r|Ok(format!("Timesheet version {}: {} — document {:?}, {} by {}\nBefore: {}\nAfter: {}",r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<i64>>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<String>>(5)?.unwrap_or_default(),r.get::<_,String>(6)?)))? {lines.push(row?);}
    }
    let mut changes=db.prepare("SELECT e.recorded_at,e.source,e.source_id,e.reason,e.before_evidence,e.after_evidence FROM shift_change_events e LEFT JOIN timesheets t ON e.source='imported' AND t.id=e.source_id LEFT JOIN direct_shifts d ON e.source='direct' AND d.id=e.source_id WHERE COALESCE(t.personal_assistant_id,d.personal_assistant_id)=?1 ORDER BY e.id")?;
    for row in changes.query_map([record.personal_assistant_id], |r| {
        Ok(format!(
            "{}: {}:{} — {}\nBefore: {}\nAfter: {}",
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?
        ))
    })? {
        lines.push(row?);
    }
    let mut deferred=db.prepare("SELECT deferred_at,evidence FROM shift_review_deferrals WHERE personal_assistant_id=?1 ORDER BY id")?;
    for row in deferred.query_map([record.personal_assistant_id], |r| {
        Ok(format!(
            "{}: Shift review deferred; affected payroll remains blocked until review\n{}",
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?
        ))
    })? {
        lines.push(row?);
    }
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
        let reserved = lifecycle::submitted_correction_minutes(&db, id, None)?;
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
