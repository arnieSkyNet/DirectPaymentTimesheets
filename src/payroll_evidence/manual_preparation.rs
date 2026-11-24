//! An explicit current-period draft kept separately from historical decisions.
use super::{lifecycle, reconciliation, *};
use crate::{
    pay_rate_allocation::{self, PayRatePortion, ReconciledPayrollHours},
    payroll_timesheet_repository::PayrollTimesheet,
    payroll_worked_item_repository::WorkedItemSnapshot,
};

const SOURCE: &str = "manual_payroll_preparation";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Draft {
    pub worked: [i64; 4],
    pub selected: Vec<i64>,
    signature: String,
    actual: [i64; 4],
    final_totals: [i64; 4],
    applications: Vec<(i64, i64)>,
    #[serde(default)]
    entered_on: Option<String>,
}

impl Draft {
    pub(crate) fn week_totals(&self) -> [i64; 4] { self.final_totals }
}

pub(crate) fn supersede_with_normal_save(db: &Connection, record: i64, at: &str) -> Result<()> {
    if let Some(draft) = latest(db, record)? {
        db.execute(
            "INSERT INTO shift_change_events(source,source_id,before_evidence,after_evidence,reason,review_signature,recorded_at,actor)
             VALUES(?1,?2,?3,'null','Saved in normal payroll preparation',?4,?5,'local_employer')",
            params![SOURCE, record, serde_json::to_string(&draft)?, draft.signature, at],
        )?;
    }
    Ok(())
}

pub(crate) struct Preview {
    pub actual: [i64; 4],
    pub shifts: Vec<(NaiveDate, i64)>,
    pub corrections: Vec<reconciliation::CorrectionApplication>,
    base: ReconciledPayrollHours,
    signature: String,
    revision: Option<i64>,
}

pub(crate) fn latest(db: &Connection, record: i64) -> Result<Option<Draft>> {
    let stored: Option<(String, String)> = db.query_row(
        "SELECT after_evidence, recorded_at FROM shift_change_events WHERE source=?1 AND source_id=?2 ORDER BY id DESC LIMIT 1",
        params![SOURCE, record], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    let Some((payload, recorded_at)) = stored else { return Ok(None) };
    let mut draft: Option<Draft> = serde_json::from_str(&payload)?;
    if let Some(draft) = draft.as_mut().filter(|draft| draft.entered_on.is_none()) {
        draft.entered_on = chrono::DateTime::parse_from_rfc3339(&recorded_at)
            .ok().map(|date| date.date_naive().to_string());
    }
    Ok(draft)
}

fn revision(db: &Connection, record: i64) -> Result<Option<i64>> {
    Ok(db.query_row(
        "SELECT MAX(id) FROM shift_change_events WHERE source=?1 AND source_id=?2",
        params![SOURCE, record],
        |row| row.get(0),
    )?)
}

fn signature(db: &Connection, app: &Application, record: i64) -> Result<String> {
    let settings = &app.context.config.payroll;
    Ok(hash(&format!(
        "{}:{}:{}",
        lifecycle::candidate_signature(db, record)?,
        settings.rounding_minutes,
        settings.rounding_direction
    )))
}

fn verify_editable(db: &Connection, record: &PayrollTimesheet) -> Result<()> {
    if lifecycle::stage(db, record)? != lifecycle::Stage::Editable
        || lifecycle::latest(db, record.id)?.is_some()
    {
        return Err("Manual preparation requires a current, unsubmitted timesheet".into());
    }
    Ok(())
}

pub(crate) fn preview(
    app: &Application,
    record: &PayrollTimesheet,
    dates: &[NaiveDate; 4],
) -> Result<Preview> {
    let start = reconciliation::period(app, record)?.0;
    if *dates != std::array::from_fn(|index| start + chrono::Duration::weeks(index as i64)) {
        return Err("Payroll period changed; reopen manual preparation".into());
    }
    let db = open(app)?;
    verify_editable(&db, record)?;
    let saved_revision = revision(&db, record.id)?;
    let evidence_signature = signature(&db, app, record.id)?;
    let plan = reconciliation::plan(app, record)?;
    if plan.overlap
        || plan
            .notices
            .iter()
            .any(|notice| notice.contains("delivery is indeterminate"))
    {
        return Err(
            "Resolve overlapping periods or uncertain delivery before manual preparation".into(),
        );
    }
    if plan.historical_reviews.is_empty() && latest(&db, record.id)?.is_none() {
        return Err(
            "Manual preparation is available when historical payment reviews block this timesheet"
                .into(),
        );
    }
    let mut base = ReconciledPayrollHours {
        weeks: std::array::from_fn(|_| Vec::new()),
        week_totals_minutes: [0; 4],
        previous_cycle_minutes: 0,
        snapshot_items: Vec::new(),
    };
    let mut shifts = Vec::new();
    for entry in &plan.work {
        let date = entry.date()?;
        let day = (date - dates[0]).num_days();
        if !(0..28).contains(&day) {
            continue;
        }
        pay_rate_allocation::add_evidence(
            &app.pay_rate_repository,
            &mut base,
            entry,
            (day / 7) as usize,
            false,
            &app.context.config.payroll,
        )?;
        shifts.push((date, entry.minutes));
    }
    let records = app
        .payroll_timesheet_repository
        .get_all_for_personal_assistant(record.personal_assistant_id)?;
    let mut corrections = Vec::new();
    for correction in reconciliation::outstanding(app, record)? {
        let origin: i64 = db.query_row(
            "SELECT origin_payroll_timesheet_id FROM payroll_corrections WHERE id=?1",
            [correction.id],
            |row| row.get(0),
        )?;
        if let Some(prior) = records.iter().find(|prior| prior.id == origin) {
            if reconciliation::period(app, prior)?.1 < dates[0] {
                corrections.push(correction);
            }
        }
    }
    if signature(&db, app, record.id)? != evidence_signature
        || revision(&db, record.id)? != saved_revision
    {
        return Err("Payroll evidence changed while opening manual preparation; reopen it".into());
    }
    Ok(Preview {
        actual: base.week_totals_minutes,
        shifts,
        corrections,
        base,
        signature: evidence_signature,
        revision: saved_revision,
    })
}

impl Preview {
    pub(crate) fn totals(&self, worked: [i64; 4], selected: &[i64]) -> Result<([i64; 4], i64)> {
        let mut result = self.base.clone();
        result.week_totals_minutes = worked;
        reconciliation::apply_balances(&mut result, &self.selected(selected)?)?;
        Ok((result.week_totals_minutes, result.previous_cycle_minutes))
    }

    fn selected(&self, selected: &[i64]) -> Result<Vec<reconciliation::CorrectionApplication>> {
        let corrections = self
            .corrections
            .iter()
            .filter(|c| selected.contains(&c.id))
            .cloned()
            .collect::<Vec<_>>();
        if corrections.len() != selected.len() {
            return Err(
                "Selected historical adjustments changed; reopen manual preparation".into(),
            );
        }
        Ok(corrections)
    }

    fn calculate(
        &self,
        app: &Application,
        record: &PayrollTimesheet,
        dates: &[NaiveDate; 4],
        worked: [i64; 4],
        selected: &[i64],
        entered_on: Option<NaiveDate>,
    ) -> Result<(ReconciledPayrollHours, Draft)> {
        let mut result = self.base.clone();
        for (index, total) in worked.into_iter().enumerate() {
            if !(0..=7 * 24 * 60).contains(&total) {
                return Err("Each week must have between 0 and 168 worked hours".into());
            }
            let minutes = total - self.actual[index];
            if minutes == 0 {
                continue;
            }
            let date = dates[index];
            let rate = pay_rate_allocation::select_manual_adjustment_rate(
                &app.pay_rate_repository,
                record.personal_assistant_id,
                date,
                date + chrono::Duration::days(6),
                minutes,
            )?
            .ok_or("No applicable pay rate for the manually edited week")?;
            let hourly = rate.base_hourly_rate + rate.employer_top_up_rate;
            result.weeks[index].push(PayRatePortion {
                worked_minutes: minutes,
                total_hourly_rate: Some(hourly),
                effective_date: crate::models::parse_employment_date(&rate.effective_date),
                rate_id: Some(rate.id),
                is_previous_cycle: false,
                is_opaque_legacy: false,
            });
            result.snapshot_items.push(WorkedItemSnapshot {
                week_number: index as i64 + 1,
                source_type: "manual_adjustment".into(),
                timesheet_id: None,
                direct_shift_id: None,
                source_evidence: None,
                work_date: None,
                worked_minutes: minutes,
                pay_rate_id: Some(rate.id),
                pay_rate_effective_date: Some(rate.effective_date),
                total_hourly_rate: Some(hourly),
                reason: entered_on
                    .map(|entered_on| reconciliation::manual_hours_reason(date, entered_on))
                    .or(Some("Manual current-period hours edit"))
                    .map(str::to_owned),
            });
        }
        result.week_totals_minutes = worked;
        let applications = reconciliation::apply_balances(&mut result, &self.selected(selected)?)?;
        let draft = Draft {
            worked,
            selected: selected.to_vec(),
            signature: self.signature.clone(),
            actual: self.actual,
            final_totals: result.week_totals_minutes,
            applications,
            entered_on: entered_on.map(|date| date.to_string()),
        };
        Ok((result, draft))
    }
}

pub(crate) fn save(
    app: &Application,
    record: &PayrollTimesheet,
    dates: &[NaiveDate; 4],
    displayed: &Preview,
    worked: [i64; 4],
    selected: &[i64],
) -> Result<i64> {
    let db = open(app)?;
    let _guard = crate::timesheet_delivery::production_lock(&db)?;
    let fresh = preview(app, record, dates)?;
    let entered_on = Some(reconciliation::today());
    let (_, approved) = displayed.calculate(app, record, dates, worked, selected, entered_on)?;
    let (_, draft) = fresh.calculate(app, record, dates, worked, selected, entered_on)?;
    if approved != draft || fresh.revision != displayed.revision {
        return Err("Payroll evidence or carry changed; reopen manual preparation and verify the totals again".into());
    }
    let tx = rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)?;
    verify_editable(&tx, record)?;
    if signature(&tx, app, record.id)? != draft.signature
        || revision(&tx, record.id)? != fresh.revision
    {
        return Err(
            "Payroll evidence changed; reopen manual preparation and verify the totals again"
                .into(),
        );
    }
    let before = latest(&tx, record.id)?
        .map(|old| serde_json::to_string(&old))
        .transpose()?
        .unwrap_or_default();
    tx.execute(
        "INSERT INTO shift_change_events(source,source_id,before_evidence,after_evidence,reason,review_signature,recorded_at,actor)
         VALUES(?1,?2,?3,?4,'Confirmed manual payroll preparation',?5,?6,'local_employer')",
        params![SOURCE, record.id, before, serde_json::to_string(&draft)?, draft.signature, now()],
    )?;
    let saved_revision = tx.last_insert_rowid();
    tx.execute("DELETE FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1 AND state='candidate'", [record.id])?;
    tx.commit()?;
    Ok(saved_revision)
}

pub(crate) fn generate(
    app: &Application,
    record: &PayrollTimesheet,
    dates: &[NaiveDate; 4],
    draft: &Draft,
) -> Result<ReconciledPayrollHours> {
    let db = open(app)?;
    lifecycle::ensure_generatable(&db, record.id)?;
    let fresh = preview(app, record, dates)?;
    let entered_on = draft.entered_on.as_deref().map(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d")).transpose()?;
    let (result, verified) = fresh.calculate(app, record, dates, draft.worked, &draft.selected, entered_on)?;
    if verified != *draft {
        return Err("Manual payroll evidence or carry changed; reopen preparation and confirm the totals again".into());
    }
    reconciliation::reserve_applications(&db, record.id, verified.applications)?;
    Ok(result)
}

pub(super) struct Editor {
    preview: Preview,
    dates: [NaiveDate; 4],
    pub(super) dirty: bool,
    hours: [f64; 4],
    selected: Vec<i64>,
    confirmed: bool,
    status: String,
}

impl Editor {
    pub(super) fn open(
        app: &Application,
        record: &PayrollTimesheet,
        dates: &[NaiveDate; 4],
    ) -> Result<Self> {
        let preview = preview(app, record, dates)?;
        let db = super::open(app)?;
        let saved = latest(&db, record.id)?;
        let worked = saved.as_ref().map_or(preview.actual, |draft| draft.worked);
        let selected = saved.map_or_else(Vec::new, |draft| {
            draft
                .selected
                .into_iter()
                .filter(|id| preview.corrections.iter().any(|entry| entry.id == *id))
                .collect()
        });
        Ok(Self {
            preview,
            dates: *dates,
            dirty: false,
            hours: worked.map(|minutes| minutes as f64 / 60.0),
            selected,
            confirmed: false,
            status: String::new(),
        })
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, app: &Application, record: &PayrollTimesheet) {
        let format = pay_rate_allocation::format_total_minutes;
        ui.label("Enter current-period worked hours, including any estimates. Historical reviews stay open.");
        ui.collapsing("Current-period actual shifts", |ui| {
            for (date, minutes) in &self.preview.shifts {
                ui.label(format!("{date} · {} recorded hours", format(*minutes)));
            }
        });
        let mut edited = false;
        for (week, hours) in self.hours.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Week {} · {} actual hours after rounding",
                    week + 1,
                    format(self.preview.actual[week])
                ));
                edited |= ui
                    .add(
                        egui::DragValue::new(hours)
                            .range(0.0..=168.0)
                            .max_decimals(4)
                            .suffix(" hours"),
                    )
                    .changed();
            });
        }
        ui.strong("Historical adjustment");
        ui.label("Only explicitly selected, previously recorded carry can be included.");
        for correction in &self.preview.corrections {
            let mut included = self.selected.contains(&correction.id);
            if ui
                .checkbox(
                    &mut included,
                    format!(
                        "{} hours · {} · reference {}",
                        format(correction.minutes),
                        correction.reason,
                        correction.id
                    ),
                )
                .changed()
            {
                edited = true;
                if included {
                    self.selected.push(correction.id);
                } else {
                    self.selected.retain(|id| *id != correction.id);
                }
            }
        }
        if edited {
            self.dirty = true;
            self.confirmed = false;
            self.status.clear();
        }
        let worked = self.hours.map(|hours| (hours * 60.0).round() as i64);
        let (final_totals, carry) = match self.preview.totals(worked, &self.selected) {
            Ok(totals) => totals,
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
                return;
            }
        };
        ui.label(format!(
            "Historical adjustment included: {} hours",
            format(carry)
        ));
        for (index, minutes) in final_totals.iter().enumerate() {
            ui.label(format!(
                "Week {} final worked hours: {}",
                index + 1,
                format(*minutes)
            ));
        }
        ui.strong(format!(
            "Final worked hours: {}",
            format(final_totals.iter().sum())
        ));
        ui.checkbox(
            &mut self.confirmed,
            "I verified the final worked hours and historical adjustment",
        );
        if ui
            .add_enabled(
                self.confirmed,
                egui::Button::new("Save manual preparation for PDF generation"),
            )
            .clicked()
        {
            self.status = match save(
                app,
                record,
                &self.dates,
                &self.preview,
                worked,
                &self.selected,
            ) {
                Ok(revision) => {
                    self.preview.revision = Some(revision);
                    self.dirty = false;
                    self.confirmed = false;
                    "Manual preparation saved. Generate the PDF through Generate Payroll Timesheets.".into()
                }
                Err(error) => error.to_string(),
            };
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
    }
}
