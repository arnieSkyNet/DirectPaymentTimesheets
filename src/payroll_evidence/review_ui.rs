use super::lifecycle::Stage;
use super::*;
#[derive(Default)]
pub struct ReviewUi {
    complete: HashSet<(i64, String)>,
    cached: std::collections::HashMap<i64, ReviewData>,
    error: String,
    historical_rows: std::collections::HashMap<i64, Vec<HistoricalRow>>,
}
const INCLUDED: &str = "Already included in previous payroll";
const CARRY_FORWARD: &str = "Extra hours not included in previous payroll — carry forward";

struct HistoricalRow {
    review: reconciliation::HistoricalReview,
    saved: Option<bool>,
    height: f32,
}

struct ReviewData {
    stage: Stage,
    replacement: bool,
    plan: Option<reconciliation::Plan>,
    change: Option<reconciliation::Changes>,
    audit: Vec<String>,
}
impl ReviewUi {
    pub fn refresh_record(
        &mut self,
        app: &Application,
        record: &crate::payroll_timesheet_repository::PayrollTimesheet,
        duplicates: bool,
        preflight: &Preflight,
    ) -> Result<()> {
        let db = open(app)?;
        let stage = lifecycle::stage(&db, record)?;
        if stage == Stage::Editable && !duplicates {
            reconciliation::sync_settled_corrections(app, record.personal_assistant_id)?;
        }
        let plan = if stage == Stage::Editable && !duplicates {
            Some(reconciliation::plan_prepared(app, record, preflight)?)
        } else {
            None
        };
        let change = if matches!(stage, Stage::Submitted | Stage::Settled) && !duplicates {
            Some(reconciliation::changes_prepared(app, record, preflight)?)
        } else {
            None
        };
        if let Some(plan) = &plan {
            let rows = self.historical_rows.entry(record.id).or_default();
            rows.retain(|row| {
                row.saved.is_some()
                    || plan
                        .historical_reviews
                        .iter()
                        .any(|review| review.evidence.key() == row.review.evidence.key())
            });
            for review in &plan.historical_reviews {
                if !rows
                    .iter()
                    .any(|row| row.review.evidence.key() == review.evidence.key())
                {
                    rows.push(HistoricalRow {
                        review: review.clone(),
                        saved: None,
                        height: 0.0,
                    });
                }
            }
        }
        self.cached.insert(
            record.id,
            ReviewData {
                stage,
                replacement: stage == Stage::Editable
                    && lifecycle::latest(&db, record.id)?.is_some(),
                plan,
                change,
                audit: reconciliation::audit_lines(app, record)?,
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
                || d.change.as_ref().is_some_and(|c| c.changed)
        })
    }
    pub fn refresh_audit(&mut self,app:&Application,record:&crate::payroll_timesheet_repository::PayrollTimesheet)->Result<()> {
        if let Some(data)=self.cached.get_mut(&record.id) {data.audit=reconciliation::audit_lines(app,record)?;}
        Ok(())
    }
    pub fn historical(&self,id:i64)->bool {
        self.cached.get(&id).is_some_and(|d|matches!(d.stage,Stage::Submitted|Stage::Settled))
    }
    pub fn blocks_preparation(&self, id: i64) -> bool {
        self.cached.get(&id).is_some_and(|d| {
            d.plan
                .as_ref()
                .is_some_and(|p| !p.historical_reviews.is_empty())
        })
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
                        let remaining = rows.iter().filter(|row| row.saved.is_none()).count();
                        ui.strong(format!("Historical payment reviews remaining: {remaining}"));
                        for row in rows {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.set_min_height(row.height);
                                let review = &row.review;
                                ui.strong(format!("Historical payment review — {}", review.evidence.pa_name));
                                ui.label(format!("{}: {} – {}, {:.2} hours", review.evidence.label(), review.evidence.start, review.evidence.end, review.evidence.minutes as f64 / 60.0));
                                if let Some(paid) = row.saved {
                                    ui.strong(format!("Saved: {}", if paid { INCLUDED } else { CARRY_FORWARD }));
                                } else {
                                    ui.label("This historical payroll is settled, but retained evidence cannot establish whether this shift was included. Original payroll totals will remain unchanged.");
                                    let paid = ui.button(INCLUDED).clicked();
                                    let unpaid = ui.button(CARRY_FORWARD).clicked();
                                    if paid || unpaid {
                                        match reconciliation::historical_decision(app, review, paid) {
                                            Ok(()) => {
                                                row.saved = Some(paid);
                                                self.error.clear();
                                                changed = true;
                                                ui.ctx().request_repaint();
                                            }
                                            Err(e) => self.error = e.to_string(),
                                        }
                                    }
                                }
                                row.height = ui.min_rect().height();
                            });
                        }
                    }
                }
            }
            if matches!(stage,Stage::Submitted|Stage::Settled) {
                ui.label("Amend hours directly, Save and regenerate the PDF. The latest regenerated saved version is used for subsequent reconciliation. Email sending is a separate explicit action.");
                if let Some(change)=&data.change {
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
            self.show_audit(ui, record.id, name);
        }

        // The parent preserves this UI cache across evidence reloads so that
        // historical row confirmations survive. Refresh a completed carry here
        // to remove its action immediately and retain the new audit decision.
        if changed && self.historical(record.id) {
            match reconciliation::changes(app, record) {
                Ok(change) => {
                    if let Some(data) = self.cached.get_mut(&record.id) {
                        data.change = Some(change);
                    }
                    if let Err(error) = self.refresh_audit(app, record) {
                        self.error = error.to_string();
                    }
                }
                Err(error) => self.error = error.to_string(),
            }
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        changed
    }
    pub fn show_audit(&self, ui: &mut egui::Ui, record_id: i64, name: &str) {
        if let Some(data) = self.cached.get(&record_id).filter(|d| !d.audit.is_empty()) {
            ui.collapsing(format!("Payroll audit/details — {name}"), |ui| {
                for line in &data.audit {
                    ui.label(line);
                }
            });
        }
    }
}
