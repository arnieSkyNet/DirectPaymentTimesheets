use super::lifecycle::Stage;
use super::*;
#[derive(Default)]
pub struct ReviewUi {
    complete: HashSet<String>,
    cached: std::collections::HashMap<i64, ReviewData>,
    error: String,
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
    ) -> bool {
        let Some(data) = self.cached.get(&record.id) else {
            return false;
        };
        let mut changed = false;
        {
            let stage = data.stage;
            if stage == Stage::Editable {
                if data.replacement {
                    ui.label("Replacement authorised: the original submission remains retained. Review and save preparation, then generate and send the replacement through the normal payroll workflow.");
                }
                if let Some(plan) = &data.plan {
                    for notice in &plan.notices {
                        ui.label(notice);
                    }
                    for review in &plan.historical_reviews {
                        egui::Frame::group(ui.style()).show(ui,|ui| {
                        ui.strong(format!("Historical payment review — {}",review.evidence.pa_name));
                        ui.label(format!("{}: {} – {}, {:.2} hours",review.evidence.label(),review.evidence.start,review.evidence.end,review.evidence.minutes as f64/60.0));
                        ui.label("This historical payroll is settled, but retained evidence cannot establish whether this shift was included. Original payroll totals will remain unchanged.");
                        let paid=ui.button("Already paid / included").clicked();
                        let unpaid=ui.button("Genuinely unpaid — carry forward").clicked();
                        if paid||unpaid {match reconciliation::historical_decision(app,&review,paid) {Ok(())=>changed=true,Err(e)=>self.error=e.to_string()}}
                    });
                    }
                }
            }
            if let Some(change) = &data.change {
                if change.changed {
                    egui::Frame::group(ui.style()).show(ui,|ui| {
                        ui.strong(format!("Submitted evidence review — {name}"));
                        ui.label(if stage==Stage::Settled {"Payslip Sent: settled payroll will not be rewritten."}else{"This PA's timesheet was already sent and current hours/evidence differ. Follow Payroll's instructions."});
                        ui.collapsing("Discrepancy and audit details",|ui|{ui.label(&change.description);});
                        let mut complete=self.complete.contains(&change.signature);
                        if change.requires_complete {
                            ui.checkbox(&mut complete,"I confirm actual evidence for the displayed aggregate reconciliation is complete");
                            if complete {self.complete.insert(change.signature.clone());}else{self.complete.remove(&change.signature);}
                        }
                        if stage==Stage::Submitted && ui.button("Correct / Resubmit Timesheet").clicked() {
                            match lifecycle::authorize_resubmission(app,record,&change.signature) {Ok(())=>changed=true,Err(e)=>self.error=e.to_string()}
                        }
                        if ui.add_enabled(!change.requires_complete||complete,egui::Button::new("Carry correction forward")).clicked() {
                            match reconciliation::carry(app,record,&change.signature,complete) {Ok(())=>changed=true,Err(e)=>self.error=e.to_string()}
                        }
                    });
                }
            }
            self.show_audit(ui, record.id, name);
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
