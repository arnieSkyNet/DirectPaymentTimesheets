//! Read-only presentation of recorded payroll history.
use super::{lifecycle, open, Result, WorkEvidence};
use crate::{app::Application, payroll_timesheet_repository::PayrollTimesheet};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    when: Option<NaiveDateTime>,
    chronological_key: Option<NaiveDateTime>,
    pub summary: String,
    pub effect: String,
    pub details: Vec<String>,
}

impl Entry {
    fn new(
        at: Option<&str>,
        summary: impl Into<String>,
        effect: impl Into<String>,
        details: Vec<String>,
    ) -> Self {
        let (when, chronological_key) = match at
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        {
            Some(date) => (
                Some(date.with_timezone(&Local).naive_local()),
                Some(date.naive_utc()),
            ),
            None => {
                let local = at.and_then(|value| {
                    super::parse_clock(value)
                        .or_else(|| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok())
                });
                (
                    local,
                    local.map(|date| {
                        Local
                            .from_local_datetime(&date)
                            .earliest()
                            .map_or(date, |date| date.naive_utc())
                    }),
                )
            }
        };
        Self {
            when,
            chronological_key,
            summary: summary.into(),
            effect: effect.into(),
            details,
        }
    }

    pub fn date_label(&self) -> String {
        self.when.map_or_else(
            || "Date and time not recorded".into(),
            |when| when.format("%d/%m/%Y at %H:%M").to_string(),
        )
    }
}

fn duration(minutes: i64) -> String {
    let hours = minutes.unsigned_abs() / 60;
    let mins = minutes.unsigned_abs() % 60;
    let hour_text = if hours == 1 { "hour" } else { "hours" };
    let min_text = if mins == 1 { "minute" } else { "minutes" };
    match (hours, mins) {
        (0, _) => format!("{mins} {min_text}"),
        (_, 0) => format!("{hours} {hour_text}"),
        _ => format!("{hours} {hour_text} {mins} {min_text}"),
    }
}

fn signed_duration(minutes: i64) -> String {
    format!(
        "{}{}",
        if minutes < 0 { "−" } else { "" },
        duration(minutes)
    )
}

fn adjustment(minutes: i64) -> String {
    match minutes.cmp(&0) {
        std::cmp::Ordering::Less => format!("Deduct {}", duration(minutes)),
        std::cmp::Ordering::Greater => format!("Add {}", duration(minutes)),
        std::cmp::Ordering::Equal => "No adjustment".into(),
    }
}

#[derive(PartialEq)]
struct Shift {
    start: String,
    finish: Option<String>,
    breaks: i64,
    worked: Option<i64>,
    notes: Option<String>,
    rate: Option<f64>,
    amount: Option<f64>,
    source: &'static str,
}

impl Shift {
    fn from_evidence(e: WorkEvidence) -> Self {
        Self {
            start: e.start,
            finish: Some(e.end),
            breaks: e.break_minutes,
            worked: Some(e.minutes),
            notes: Some(e.notes),
            rate: None,
            amount: None,
            source: if e.source == "direct" {
                "Hours Shift"
            } else {
                "Hours Keeper"
            },
        }
    }

    fn describe(&self) -> String {
        let start = super::parse_clock(&self.start);
        let finish = self.finish.as_deref().and_then(super::parse_clock);
        let mut facts = vec![self.source.to_owned()];
        if let Some(start) = start {
            let mut timing = start.format("%d/%m/%Y, %H:%M").to_string();
            if let Some(finish) = finish {
                let format = if start.date() == finish.date() {
                    "%H:%M"
                } else {
                    "%d/%m/%Y at %H:%M"
                };
                timing.push_str(&format!(" to {}", finish.format(format)));
            }
            facts.push(timing);
        } else if let Some(finish) = finish {
            facts.push(format!("finished {}", finish.format("%d/%m/%Y at %H:%M")));
        }
        facts.push(format!("{} break", duration(self.breaks)));
        if let Some(worked) = self.worked {
            facts.push(format!("{} worked", duration(worked)));
        }
        if let Some(rate) = self.rate.filter(|value| value.is_finite()) {
            facts.push(format!("£{rate:.2} per hour recorded"));
        }
        if let Some(amount) = self.amount.filter(|value| value.is_finite()) {
            facts.push(format!("£{amount:.2} recorded amount"));
        }
        if let Some(notes) = self.notes.as_deref().filter(|text| !text.trim().is_empty()) {
            facts.push(format!("Note: {}", notes.trim()));
        }
        facts.join(" · ")
    }
}

// Older shift events stored Rust's debug representation. Decode only the known
// fields and never substitute a current edited shift for an earlier snapshot.
fn recorded_shift(text: &str) -> Option<Shift> {
    if let Ok(evidence) = toml::from_str::<WorkEvidence>(text) {
        return Some(Shift::from_evidence(evidence));
    }
    fn string(input: &mut &str) -> Option<String> {
        let mut values = serde_json::Deserializer::from_str(input).into_iter::<String>();
        let value = values.next()?.ok()?;
        *input = input.get(values.byte_offset()..)?;
        Some(value)
    }
    fn optional(input: &mut &str) -> Option<Option<String>> {
        if let Some(rest) = input.strip_prefix("None") {
            *input = rest;
            return Some(None);
        }
        *input = input.strip_prefix("Some(")?;
        let value = string(input)?;
        *input = input.strip_prefix(')')?;
        Some(Some(value))
    }
    fn integer(input: &mut &str) -> Option<i64> {
        let (value, rest) = input.split_once(", ")?;
        *input = rest;
        value.parse().ok()
    }
    let (direct, mut input) =
        if let Some(input) = text.strip_prefix("EffectiveTimesheetValues { start_time: ") {
            (false, input)
        } else {
            (
                true,
                text.strip_prefix("DirectShift { id: ")?
                    .split_once(", start_time: ")?
                    .1,
            )
        };
    let start = string(&mut input)?;
    input = input.strip_prefix(", end_time: ")?;
    let finish = if direct {
        optional(&mut input)?
    } else {
        Some(string(&mut input)?)
    };
    input = input.strip_prefix(", break_minutes: ")?;
    let breaks = integer(&mut input)?;
    let worked = if direct {
        finish.as_deref().and_then(|end| {
            crate::direct_shift_repository::validate_completion(
                super::parse_clock(&start)?,
                super::parse_clock(end)?,
                breaks,
            )
            .ok()
        })
    } else {
        input = input.strip_prefix("worked_minutes: ")?;
        Some(integer(&mut input)?)
    };
    let notes = input
        .strip_prefix("notes: ")
        .and_then(|mut text| optional(&mut text))
        .flatten();
    Some(Shift {
        start,
        finish,
        breaks,
        worked,
        notes,
        rate: None,
        amount: None,
        source: if direct {
            "Hours Shift"
        } else {
            "Hours Keeper"
        },
    })
}

fn shift_change(before: Option<Shift>, after: Option<Shift>) -> Vec<String> {
    match (before, after) {
        (Some(before), Some(after)) if before != after => vec![
            format!("Before: {}", before.describe()),
            format!("After: {}", after.describe()),
        ],
        (_, Some(shift)) | (Some(shift), None) => vec![shift.describe()],
        (None, None) => Vec::new(),
    }
}

fn uk_date(date: &str) -> Option<String> {
    crate::date_utils::parse_legacy(date)
        .ok()
        .map(|date| date.format("%d/%m/%Y").to_string())
}

fn hours(value: f64) -> Option<String> {
    let minutes = (value * 60.0).round();
    (minutes.is_finite() && minutes.abs() < i64::MAX as f64)
        .then(|| signed_duration(minutes as i64))
}

fn week_summary(number: i64, date: Option<&str>, figures: [Option<f64>; 5]) -> Option<String> {
    let mut fields = Vec::new();
    for (index, (value, label)) in figures
        .into_iter()
        .zip([
            "worked",
            "annual leave",
            "sick leave",
            "public holiday",
            "miles",
        ])
        .enumerate()
    {
        if let Some(value) = value.filter(|v| v.is_finite() && (index == 0 || *v != 0.0)) {
            if index == 4 {
                fields.push(format!("{value} {label}"));
            } else if let Some(value) = hours(value) {
                fields.push(format!("{value} {label}"));
            }
        }
    }
    if fields.is_empty() {
        return None;
    }
    let week = date.and_then(uk_date).map_or_else(
        || format!("Week {number}"),
        |date| format!("Week {number} from {date}"),
    );
    Some(format!("{week}: {}", fields.join(" · ")))
}

fn saved_details(value: &serde_json::Value) -> Vec<String> {
    let mut details = Vec::new();
    // Amendment snapshots contain the columns in schema order. Read only
    // documented business fields and leave stored identifiers untouched.
    if let Some(weeks) = value["weeks"].as_array() {
        details.extend(weeks.iter().filter_map(|row| {
            week_summary(
                row[2].as_i64()?,
                row[3].as_str(),
                std::array::from_fn(|index| row[index + 4].as_f64()),
            )
        }));
    }
    for (key, label) in [("leave", "Annual leave"), ("holidays", "Public holiday")] {
        if let Some(rows) = value[key].as_array() {
            details.extend(rows.iter().filter_map(|row| {
                Some(format!(
                    "{label} on {}: {}",
                    uk_date(row[3].as_str()?)?,
                    hours(row[4].as_f64()?)?
                ))
            }));
        }
    }
    if let Some(rows) = value["manual"].as_array() {
        details.extend(rows.iter().filter_map(|row| {
            Some(format!(
                "Week {} manual hours adjustment: {}",
                row[2].as_i64()?,
                adjustment(row[3].as_i64()?)
            ))
        }));
    }
    if let Some(carry) = value["carry"]
        .as_f64()
        .filter(|value| *value != 0.0)
        .and_then(hours)
    {
        details.push(format!("Hours carried from an earlier period: {carry}"));
    }
    if let Some(notes) = value["notes"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
    {
        details.push(format!("Payroll notes: {}", notes.trim()));
    }
    details
}

fn payable_detail(
    week: i64,
    minutes: i64,
    rate: Option<f64>,
    work_date: Option<&str>,
    evidence: Option<&str>,
) -> String {
    let work = evidence
        .and_then(recorded_shift)
        .map(|shift| shift.describe())
        .or_else(|| work_date.and_then(uk_date));
    let mut entry = format!("Week {week}");
    if let Some(work) = work {
        entry.push_str(&format!(" · {work}"));
    }
    entry.push_str(&format!(" · {} payable", signed_duration(minutes)));
    if let Some(rate) = rate.filter(|rate| rate.is_finite()) {
        entry.push_str(&format!(" at £{rate:.2} per hour"));
    }
    entry
}

fn version_details(before: Option<&str>, after: &str, generated: bool) -> Vec<String> {
    use serde_json::Value;
    let before = before
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .map(|saved| saved_details(&saved))
        .unwrap_or_default();
    let Ok(after) = serde_json::from_str::<Value>(after) else {
        return Vec::new();
    };
    if generated {
        let mut details = saved_details(&after["details"]);
        if let Some(totals) = after["totals"].as_array() {
            details.extend(totals.iter().enumerate().filter_map(|(index, minutes)| {
                Some(format!(
                    "Week {} on the PDF: {} of payable work",
                    index + 1,
                    signed_duration(minutes.as_i64()?)
                ))
            }));
        }
        if let Some(items) = after["items"].as_array() {
            details.extend(items.iter().filter_map(|item| {
                Some(payable_detail(
                    item["week_number"].as_i64()?,
                    item["worked_minutes"].as_i64()?,
                    item["total_hourly_rate"].as_f64(),
                    item["work_date"].as_str(),
                    item["source_evidence"].as_str(),
                ))
            }));
        }
        return details;
    }
    let after = saved_details(&after);
    if before.is_empty() {
        return after;
    }
    before
        .iter()
        .filter(|line| !after.contains(line))
        .map(|line| format!("Before: {line}"))
        .chain(
            after
                .iter()
                .filter(|line| !before.contains(line))
                .map(|line| format!("After: {line}")),
        )
        .collect()
}

pub(crate) fn history(app: &Application, record: &PayrollTimesheet) -> Result<Vec<Entry>> {
    let db = open(app)?;
    let mut entries = Vec::new();

    let mut stmt = db.prepare(
        "SELECT event,recorded_at,before_state,after_state
         FROM timesheet_amendment_versions WHERE payroll_timesheet_id=?1 ORDER BY id",
    )?;
    let mut rows = stmt.query([record.id])?;
    while let Some(row) = rows.next()? {
        let event: String = row.get(0)?;
        let at: String = row.get(1)?;
        let before: Option<String> = row.get(2)?;
        let after: String = row.get(3)?;
        let (summary, effect) = match event.as_str() {
            "save" => (
                "Timesheet preparation saved",
                "The save did not change the submitted timesheet.",
            ),
            "generated" => (
                "Timesheet PDF generated",
                "The PDF was not submitted by this action.",
            ),
            _ => ("Timesheet version recorded", "Payroll effect not recorded."),
        };
        entries.push(Entry::new(
            Some(&at),
            summary,
            effect,
            version_details(before.as_deref(), &after, event == "generated"),
        ));
    }

    let mut stmt = db.prepare(
        "SELECT e.recorded_at,e.source,e.source_id,e.reason,e.before_evidence,e.after_evidence
         FROM shift_change_events e
         LEFT JOIN timesheets t ON e.source='imported' AND t.id=e.source_id
         LEFT JOIN direct_shifts d ON e.source='direct' AND d.id=e.source_id
         WHERE COALESCE(t.personal_assistant_id,d.personal_assistant_id)=?1
            OR (e.source IN ('historical_payment_dismissal','manual_payroll_preparation') AND e.source_id=?2)
         ORDER BY e.id",
    )?;
    let mut rows = stmt.query([record.personal_assistant_id, record.id])?;
    while let Some(row) = rows.next()? {
        let at: String = row.get(0)?;
        let source: String = row.get(1)?;
        let id: i64 = row.get(2)?;
        let reason: String = row.get(3)?;
        let before: String = row.get(4)?;
        let after: String = row.get(5)?;
        let mut details = if source == "manual_payroll_preparation" {
            let totals = |value| {
                serde_json::from_str::<super::manual_preparation::Draft>(value)
                    .ok()
                    .map(|draft| {
                        draft
                            .week_totals()
                            .iter()
                            .enumerate()
                            .map(|(index, &minutes)| {
                                format!("Week {}: {}", index + 1, signed_duration(minutes))
                            })
                            .collect::<Vec<_>>()
                            .join(" · ")
                    })
            };
            [("Before", totals(&before)), ("After", totals(&after))]
                .into_iter()
                .filter_map(|(label, totals)| totals.map(|totals| format!("{label}: {totals}")))
                .collect()
        } else {
            let mut new = recorded_shift(&after);
            if new.is_none() && source == "imported" && before.is_empty() {
                if let Some(raw) = crate::repository::get_raw_on(&db, id)? {
                    new = Some(Shift {
                        start: raw.start_time,
                        finish: Some(raw.end_time),
                        breaks: raw.break_minutes,
                        worked: Some(raw.worked_minutes),
                        notes: raw.notes,
                        rate: Some(raw.hourly_rate),
                        amount: Some(raw.amount),
                        source: "Hours Keeper",
                    });
                }
            }
            let old = recorded_shift(&before);
            if source == "direct" && old.as_ref().is_some_and(|shift| shift.finish.is_none()) {
                shift_change(None, new)
            } else {
                shift_change(old, new)
            }
        };
        let (summary, effect) = match (source.as_str(), reason.as_str()) {
            ("historical_payment_dismissal", _) => (
                "Historical payment review dismissed",
                "No adjustment. No finding on whether the shift was paid.",
            ),
            ("manual_payroll_preparation", _) => (
                if after == "null" {
                    "Earlier manual preparation replaced by a normal save"
                } else {
                    "Manual payroll preparation saved"
                },
                "Preparation only. No timesheet submitted.",
            ),
            (_, "Explicit duplicate/correction review") => {
                // The retained duplicate decision below supplies the outcome.
                continue;
            }
            ("imported", _) if before.is_empty() => (
                "Shift imported from Hours Keeper",
                "Import recorded the shift, not payment.",
            ),
            ("imported", _) => {
                let exclusion = |text| toml::from_str::<WorkEvidence>(text).ok().map(|e| e.deleted);
                let summary = match (exclusion(&before), exclusion(&after)) {
                    (Some(false), Some(true)) => "Imported shift excluded",
                    (Some(true), Some(false)) => "Imported shift restored",
                    _ => "Imported shift amended",
                };
                if reason != "Editable imported correction" && !reason.trim().is_empty() {
                    details.push(format!("Recorded reason: {}", reason.trim()));
                }
                (
                    summary,
                    "Changed shift evidence. Payment is not established by this event.",
                )
            }
            (_, "New completed shift; historical payment remains subject to review") => (
                "Hours Shift entry completed",
                "Shift logged; no payment decision recorded.",
            ),
            (_, "Editable shift deletion") => (
                "Hours Shift entry removed",
                "Removed from available shift evidence.",
            ),
            _ => {
                if reason != "Editable shift correction" && !reason.trim().is_empty() {
                    details.push(format!("Recorded reason: {}", reason.trim()));
                }
                (
                    "Hours Shift entry amended",
                    "Changed shift evidence. Payment is not established by this event.",
                )
            }
        };
        entries.push(Entry::new(Some(&at), summary, effect, details));
    }

    let mut stmt = db.prepare(
        "SELECT deferred_at,evidence FROM shift_review_deferrals WHERE personal_assistant_id=?1 ORDER BY id",
    )?;
    let mut rows = stmt.query([record.personal_assistant_id])?;
    while let Some(row) = rows.next()? {
        let at: String = row.get(0)?;
        let evidence: String = row.get(1)?;
        let details = evidence
            .split("\n\n")
            .filter_map(recorded_shift)
            .map(|shift| shift.describe())
            .collect();
        entries.push(Entry::new(
            Some(&at),
            "Shift review deferred",
            "No adjustment was recorded by deferring.",
            details,
        ));
    }

    let mut stmt =
        db.prepare("SELECT sent_at FROM payroll_legacy_settlements WHERE payroll_timesheet_id=?1")?;
    let mut rows = stmt.query([record.id])?;
    if let Some(row) = rows.next()? {
        let at: String = row.get(0)?;
        entries.push(Entry::new(
            Some(&at),
            "Historical payroll settlement retained",
            "Recorded totals stand. They do not identify which individual shifts were paid.",
            Vec::new(),
        ));
    }

    let mut stmt = db.prepare(
        "SELECT id,submitted_at,supersedes_id,payroll_department_notes
         FROM payroll_submissions WHERE payroll_timesheet_id=?1 ORDER BY id",
    )?;
    let mut submitted_weeks = db.prepare(
        "SELECT week_number,week_commencing,worked_hours,annual_leave_hours,sick_leave_hours,public_holiday_hours,travel_miles
         FROM payroll_submission_weeks WHERE submission_id=?1 ORDER BY week_number",
    )?;
    let mut submitted_items = db.prepare(
        "SELECT week_number,worked_minutes,total_hourly_rate,work_date,source_evidence
         FROM payroll_submission_items WHERE submission_id=?1 ORDER BY week_number,work_date,id",
    )?;
    let mut submitted_days = db.prepare(
        "SELECT leave_date,hours,'Annual leave' FROM payroll_submission_leave WHERE submission_id=?1
         UNION ALL SELECT holiday_date,hours,'Public holiday' FROM payroll_submission_holidays WHERE submission_id=?1",
    )?;
    let mut rows = stmt.query([record.id])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let at: Option<String> = row.get(1)?;
        let prior: Option<i64> = row.get(2)?;
        let notes: Option<String> = row.get(3)?;
        let (summary, effect) = if prior.is_some() {
            ("Replacement timesheet submitted", "Replaced the earlier figures for reconciliation. Payment is not confirmed by submission.")
        } else {
            (
                "Timesheet submission recorded",
                "Figures submitted for payroll. Payment is not confirmed by submission.",
            )
        };
        let mut details = Vec::new();
        let mut weeks = submitted_weeks.query([id])?;
        while let Some(week) = weeks.next()? {
            let date: String = week.get(1)?;
            if let Some(detail) = week_summary(
                week.get(0)?,
                Some(&date),
                [
                    week.get(2)?,
                    week.get(3)?,
                    week.get(4)?,
                    week.get(5)?,
                    week.get(6)?,
                ],
            ) {
                details.push(detail);
            }
        }
        let mut items = submitted_items.query([id])?;
        while let Some(item) = items.next()? {
            let work_date: Option<String> = item.get(3)?;
            let evidence: Option<String> = item.get(4)?;
            details.push(payable_detail(
                item.get(0)?,
                item.get(1)?,
                item.get(2)?,
                work_date.as_deref(),
                evidence.as_deref(),
            ));
        }
        let mut days = submitted_days.query([id])?;
        while let Some(day) = days.next()? {
            let date: String = day.get(0)?;
            if let (Some(date), Some(hours)) = (uk_date(&date), hours(day.get(1)?)) {
                details.push(format!("{} on {date}: {hours}", day.get::<_, String>(2)?));
            }
        }
        if let Some(notes) = notes.filter(|notes| !notes.trim().is_empty()) {
            details.push(format!("Payroll notes: {}", notes.trim()));
        }
        entries.push(Entry::new(at.as_deref(), summary, effect, details));
    }

    let mut stmt = db.prepare(
        "SELECT kind,decided_at,evidence FROM payroll_reconciliation_decisions WHERE payroll_timesheet_id=?1 ORDER BY id",
    )?;
    let mut rows = stmt.query([record.id])?;
    while let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let at: String = row.get(1)?;
        let evidence: String = row.get(2)?;
        let (summary, effect) = match kind.as_str() {
            "already_paid" => ("Shift marked as included in previous payroll", "This decision did not add hours to later payroll."),
            "unpaid" => ("Shift marked for carry-forward", "The decision identified extra hours for a later period. It does not confirm they were submitted or paid."),
            "carry" => ("Carry-forward correction approved", "A correction was recorded for later payroll. The original submission remained unchanged."),
            "complete_actuals" => ("Actual hours evidence confirmed complete", "This confirmation supported reconciliation. It did not itself change payroll figures."),
            _ => ("Payroll review decision recorded", "The payroll effect is not recorded here."),
        };
        entries.push(Entry::new(
            Some(&at),
            summary,
            effect,
            shift_change(None, recorded_shift(&evidence)),
        ));
    }

    let mut stmt = db.prepare(
        "SELECT id,minutes,reason,created_at,work_date,total_hourly_rate
         FROM payroll_corrections WHERE personal_assistant_id=?1 ORDER BY id",
    )?;
    let mut rows = stmt.query([record.personal_assistant_id])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let minutes: i64 = row.get(1)?;
        let reason: String = row.get(2)?;
        let at: String = row.get(3)?;
        let work_date: Option<String> = row.get(4)?;
        let rate: Option<f64> = row.get(5)?;
        let submitted = lifecycle::submitted_correction_minutes(&db, id, None)?;
        let remaining = minutes.checked_sub(submitted);
        let summary = match reason.as_str() {
            "Automatic estimate reconciliation" => {
                "Automatic correction comparing estimated and actual hours"
            }
            "Confirmed aggregate estimate reconciliation" => {
                "Correction based on confirmed actual hours"
            }
            "Audited submitted/actual evidence difference" => {
                "Correction based on a reviewed difference between submitted and actual hours"
            }
            _ => "Payroll correction recorded",
        };
        let mut details = Vec::new();
        if let Some(date) = work_date.as_deref().and_then(uk_date) {
            details.push(format!("Work date: {date}"));
        }
        if let Some(rate) = rate.filter(|value| value.is_finite()) {
            details.push(format!("Recorded rate: £{rate:.2} per hour"));
        }
        let effect = match remaining {
            Some(0) => format!(
                "{}; fully included in submitted payroll.",
                adjustment(minutes)
            ),
            Some(_) if submitted == 0 => {
                format!("{}; still outside submitted payroll.", adjustment(minutes))
            }
            Some(remaining) => format!(
                "{}; submitted: {}; remaining: {}.",
                adjustment(minutes),
                adjustment(submitted).to_lowercase(),
                adjustment(remaining).to_lowercase()
            ),
            None => format!(
                "{}; the remaining adjustment could not be displayed.",
                adjustment(minutes)
            ),
        };
        entries.push(Entry::new(Some(&at), summary, effect, details));
    }

    let mut stmt = db.prepare(
        "SELECT d.id,d.winner_source,d.winner_id,d.decided_at,d.invalidated_at,m.evidence
         FROM payroll_duplicate_decisions d JOIN payroll_duplicate_members m ON m.decision_id=d.id
         ORDER BY d.id,m.source,m.source_id",
    )?;
    let mut rows = stmt.query([])?;
    let mut last_invalidated = None;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let source: String = row.get(1)?;
        let winner: i64 = row.get(2)?;
        let at: String = row.get(3)?;
        let invalidated: Option<String> = row.get(4)?;
        let text: String = row.get(5)?;
        let evidence: WorkEvidence = toml::from_str(&text)?;
        if evidence.pa != record.personal_assistant_id {
            continue;
        }
        let summary = if source == "separate" {
            "Shift retained as separate in a duplicate review"
        } else if evidence.source == source && evidence.id == winner {
            "Shift selected in a duplicate review"
        } else {
            "Shift set aside in a duplicate review"
        };
        entries.push(Entry::new(
            Some(&at),
            summary,
            "No change to a submitted timesheet.",
            vec![Shift::from_evidence(evidence).describe()],
        ));
        if let Some(invalidated) = invalidated.filter(|_| last_invalidated != Some(id)) {
            last_invalidated = Some(id);
            entries.push(Entry::new(
                Some(&invalidated),
                "Earlier duplicate decision stopped applying",
                "No change to a submitted timesheet.",
                Vec::new(),
            ));
        }
    }

    // Sort dated events oldest first; keep undated records visible at the end.
    entries.sort_by_key(|entry| (entry.chronological_key.is_none(), entry.chronological_key));
    Ok(entries)
}
