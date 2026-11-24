use super::lifecycle::Stage;
use super::*;
#[derive(Default)]
pub struct ReviewUi {
    complete: HashSet<(i64, String)>,
    cached: std::collections::HashMap<i64, ReviewData>,
    error: String,
    #[cfg(test)]
    manual: std::collections::HashMap<i64, super::manual_preparation::Editor>,
    historical_rows: std::collections::HashMap<i64, Vec<HistoricalRow>>,
}
const INCLUDED: &str = "Already included in previous payroll";
const CARRY_FORWARD: &str = "Extra hours not included in previous payroll — carry forward";

struct HistoricalRow {
    review: reconciliation::HistoricalReview,
}

struct ReviewData {
    stage: Stage,
    replacement: bool,
    manual_saved: bool,
    plan: Option<reconciliation::Plan>,
    change: Option<reconciliation::Changes>,
    audit: Vec<audit::Entry>,
}
impl ReviewUi {
    pub fn refresh_record(
        &mut self,
        app: &Application,
        record: &crate::payroll_timesheet_repository::PayrollTimesheet,
        duplicates: bool,
        preflight: &Preflight,
    ) -> Result<()> {
        self.cached.remove(&record.id);
        let db = open(app)?;
        let stage = lifecycle::stage(&db, record)?;
        let manual_saved = stage == Stage::Editable
            && super::manual_preparation::latest(&db, record.id)?.is_some();
        let plan = if stage == Stage::Editable && !duplicates {
            let plan = reconciliation::plan_prepared(app, record, preflight)?;
            if !manual_saved {
                reconciliation::sync_settled_corrections(app, record.personal_assistant_id)?;
                Some(reconciliation::plan_prepared(app, record, preflight)?)
            } else {
                Some(plan)
            }
        } else {
            None
        };
        let change = if matches!(stage, Stage::Submitted | Stage::Settled) && !duplicates {
            Some(reconciliation::changes_prepared(app, record, preflight)?)
        } else {
            None
        };
        let rows = self.historical_rows.entry(record.id).or_default();
        let reviews = plan
            .as_ref()
            .map_or(&[][..], |plan| plan.historical_reviews.as_slice());
        rows.clear();
        rows.extend(reviews.iter().cloned().map(|review| HistoricalRow { review }));
        self.cached.insert(
            record.id,
            ReviewData {
                stage,
                manual_saved,
                replacement: stage == Stage::Editable
                    && lifecycle::latest(&db, record.id)?.is_some(),
                plan,
                change,
                audit: audit::history(app, record)?,
            },
        );
        Ok(())
    }
    pub fn preparation_plan(&self, id: i64) -> Option<&reconciliation::Plan> {
        self.cached.get(&id).and_then(|data| data.plan.as_ref())
    }
    pub fn unresolved(&self, id: i64) -> bool {
        self.cached.get(&id).is_some_and(|d| {
            d.replacement
                || d.stage == Stage::Indeterminate
                || d.plan
                    .as_ref()
                    .is_some_and(|p| !p.historical_reviews.is_empty() || !p.notices.is_empty())
                || d.change.as_ref().is_some_and(|c| c.changed || c.blocked)
        })
    }
    pub fn refresh_audit(&mut self,app:&Application,record:&crate::payroll_timesheet_repository::PayrollTimesheet)->Result<()> {
        if let Some(data)=self.cached.get_mut(&record.id) {data.audit=audit::history(app,record)?;}
        Ok(())
    }
    pub fn historical(&self,id:i64)->bool {
        self.cached.get(&id).is_some_and(|d|matches!(d.stage,Stage::Submitted|Stage::Settled))
    }
    pub fn blocks_preparation(&self, id: i64) -> bool {
        self.cached.get(&id).is_some_and(|data| {
            data.manual_saved
        })
    }

    #[cfg(test)]
    pub(crate) fn show_manual(
        &mut self,
        ui: &mut egui::Ui,
        app: &Application,
        record: &crate::payroll_timesheet_repository::PayrollTimesheet,
    ) {
        let Some(data) = self.cached.get(&record.id) else {
            return;
        };
        if !data.plan.as_ref().is_some_and(|plan| {
            !plan.overlap && (data.manual_saved || !plan.historical_reviews.is_empty())
        }) {
            return;
        }
        ui.group(|ui| {
            ui.heading("Manual preparation — current period");
            let label = if self.manual.contains_key(&record.id) {
                "Refresh manual preparation"
            } else {
                "Open manual preparation"
            };
            if ui.button(label).clicked() {
                let dates = match reconciliation::period(app, record) {
                    Ok((start, _)) => {
                        std::array::from_fn(|index| start + chrono::Duration::weeks(index as i64))
                    }
                    Err(error) => {
                        ui.label(error.to_string());
                        return;
                    }
                };
                match super::manual_preparation::Editor::open(app, record, &dates) {
                    Ok(editor) => {
                        self.manual.insert(record.id, editor);
                        self.error.clear();
                    }
                    Err(error) => {
                        self.manual.remove(&record.id);
                        self.error = error.to_string();
                    }
                }
            }
            if let Some(editor) = self.manual.get_mut(&record.id) {
                editor.show(ui, app, record);
            } else if !self.error.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &self.error);
            }
        });
    }

    pub fn show_pa(
        &mut self,
        ui: &mut egui::Ui,
        app: &Application,
        record: &crate::payroll_timesheet_repository::PayrollTimesheet,
        name: &str,
        preparation_dirty: bool,
    ) -> bool {
        let Some(data) = self.cached.get(&record.id) else {
            return false;
        };
        let mut changed = false;
        {
            let stage = data.stage;
            if stage == Stage::Editable {
                if data.replacement {
                    ui.label("Earlier submission retained: amend and save preparation, then generate the replacement PDF. Sending remains a separate explicit action.");
                }
                if let Some(plan) = &data.plan {
                    for notice in &plan.notices {
                        ui.label(notice);
                    }
                    if let Some(rows) = self.historical_rows.get_mut(&record.id) {
                        if !rows.is_empty() {
                            ui.strong(format!("Historical payment reviews remaining: {}", rows.len()));
                            ui.label("Current preparation and submission can continue with these historical entries held.");
                        }
                        rows.retain(|row| {
                            let review = &row.review;
                            let result = egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.strong(format!("Historical payment review — {}", review.evidence.pa_name));
                                ui.label(format!("{}: {} – {}, {:.2} hours", review.evidence.label(), review.evidence.start, review.evidence.end, review.evidence.minutes as f64 / 60.0));
                                ui.label("This historical payroll is settled, but retained evidence cannot establish whether this shift was included. Original payroll totals will remain unchanged.");
                                let mut decision = None;
                                for (label, choice) in [
                                    (INCLUDED, reconciliation::HistoricalDecision::AlreadyPaid),
                                    (CARRY_FORWARD, reconciliation::HistoricalDecision::Unpaid),
                                    ("Dismiss — no payroll adjustment required", reconciliation::HistoricalDecision::Dismiss),
                                ] {
                                    if ui.button(label).clicked() {
                                        decision = Some(choice);
                                    }
                                }
                                decision.map(|choice| reconciliation::historical_decision(app, review, choice))
                            }).inner;
                            match result {
                                Some(Ok(())) => {
                                    self.error.clear();
                                    changed = true;
                                    ui.ctx().request_repaint();
                                    false
                                }
                                Some(Err(error)) => {
                                    self.error = error.to_string();
                                    true
                                }
                                None => true,
                            }
                        });
                    }
                }
            }
            if matches!(stage,Stage::Submitted|Stage::Settled) {
                ui.label("Amend the hours, save, and generate a replacement PDF. Reconciliation uses the last accepted submission until the replacement is accepted.");
                if let Some(change)=&data.change {
                    if change.blocked {
                        ui.label(&change.description);
                    }
                    if change.changed {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.strong(format!("Carry-forward review — {name}"));
                            ui.label(&change.description);
                            ui.label("Carry this difference into a later payroll cycle. Original submissions and documents are retained.");
                            let key = (record.id, change.signature.clone());
                            let mut complete = self.complete.contains(&key);
                            if change.requires_complete {
                                ui.checkbox(&mut complete, "I confirm actual evidence for the displayed aggregate reconciliation is complete");
                                if complete {
                                    self.complete.insert(key.clone());
                                } else {
                                    self.complete.remove(&key);
                                }
                            }
                            if preparation_dirty {
                                ui.label("Save or discard preparation changes before carrying a correction.");
                            }
                            if ui.add_enabled(
                                !preparation_dirty && (!change.requires_complete || complete),
                                egui::Button::new("Carry correction forward"),
                            ).clicked() {
                                match reconciliation::carry(app, record, &change.signature, complete) {
                                    Ok(()) => {
                                        self.complete.remove(&key);
                                        self.error.clear();
                                        changed = true;
                                    }
                                    Err(error) => self.error = error.to_string(),
                                }
                            }
                        });
                    }
                }
            }
        }

        if changed {
            if let Some(plan) = self.cached.get_mut(&record.id).and_then(|data| data.plan.as_mut()) {
                if let Some(rows) = self.historical_rows.get(&record.id) {
                    plan.historical_reviews.retain(|review| rows.iter().any(|row| row.review.same_evidence(review)));
                }
            }
        }
        // Refresh a completed carry so its issue panel disappears on repaint.
        if changed && self.historical(record.id) {
            match reconciliation::changes(app, record) {
                Ok(change) => {
                    if let Some(data) = self.cached.get_mut(&record.id) {
                        data.change = Some(change);
                    }
                }
                Err(error) => self.error = error.to_string(),
            }
        }
        if changed {
            if let Err(error) = self.refresh_audit(app, record) {
                self.error = error.to_string();
            }
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        self.show_audit(ui, record.id, name);
        changed
    }
    pub fn show_audit(&self, ui: &mut egui::Ui, record_id: i64, name: &str) {
        if let Some(data) = self.cached.get(&record_id).filter(|d| !d.audit.is_empty()) {
            egui::CollapsingHeader::new(format!("Payroll audit/details — {name}"))
                .id_salt(("payroll_history", record_id))
                .show(ui, |ui| {
                    ui.label("Oldest first. Current issues are listed separately.");
                    let mut previous_date = None;
                    for entry in &data.audit {
                        let date = entry.date_label();
                        if previous_date.as_ref() != Some(&date) {
                            ui.separator();
                            ui.strong(&date);
                            previous_date = Some(date);
                        }
                        ui.label(egui::RichText::new(&entry.summary).strong());
                        for detail in &entry.details {
                            ui.label(detail);
                        }
                        if !entry.effect.is_empty() {
                            ui.label(&entry.effect);
                        }
                    }
                });
        }
    }
}
