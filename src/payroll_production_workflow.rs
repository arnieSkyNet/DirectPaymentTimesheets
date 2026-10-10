// Included in gui.rs: the Dashboard owns production selection, not preparation.
#[derive(Clone, Debug)]
struct ProductionChoice {
    id: i64,
    name: String,
    available: bool,
    detail: String,
    intent: Option<crate::timesheet_delivery::Intent>,
}
struct PendingGeneration {
    period: CapturedOperationalPayrollPeriod,
    revision: u64,
    selected_ids: Vec<i64>,
    choices: Vec<ProductionChoice>,
}
#[derive(Debug, PartialEq, Eq)]
enum ProductionOutcome {
    Completed,
    Skipped(String),
    Failed(String),
    NotAttempted,
}
#[derive(Debug)]
struct ProductionReport {
    action: &'static str,
    period: String,
    entries: Vec<(i64, String, ProductionOutcome)>,
}
impl ProductionReport {
    fn summary(&self) -> String {
        let failed = self
            .entries
            .iter()
            .filter(|(_, _, state)| matches!(state, ProductionOutcome::Failed(_)))
            .count();
        let unattempted = self
            .entries
            .iter()
            .filter(|(_, _, state)| matches!(state, ProductionOutcome::NotAttempted))
            .count();
        let skipped = self.entries.len() - self.completed() - failed - unattempted;
        format!("{}: {} completed, {skipped} skipped, {failed} failed/needs attention, {unattempted} not attempted.", self.action, self.completed())
    }
    fn completed(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, _, state)| *state == ProductionOutcome::Completed)
            .count()
    }
}

fn draw_production_choices(
    ui: &mut egui::Ui,
    choices: &[ProductionChoice],
    ids: &mut Vec<i64>,
    unsent_only: bool,
) {
    ui.horizontal(|ui| {
        if ui
            .button(if unsent_only {
                "Select all unsent"
            } else {
                "Select all available"
            })
            .clicked()
        {
            *ids = choices
                .iter()
                .filter(|c| {
                    c.available && (!unsent_only || !c.intent.as_ref().is_some_and(|i| i.resend))
                })
                .map(|c| c.id)
                .collect();
        }
        if ui.button("Clear selection").clicked() {
            ids.clear();
        }
    });
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            for choice in choices {
                let mut checked = ids.contains(&choice.id);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            choice.available,
                            egui::Checkbox::new(&mut checked, &choice.name),
                        )
                        .changed()
                    {
                        ids.retain(|id| *id != choice.id);
                        if checked {
                            ids.push(choice.id);
                        }
                    }
                    ui.label(&choice.detail);
                });
            }
        });
    ui.label(format!("{} PA(s) selected", ids.len()));
}

impl DirectPaymentApp {
    /// Read-only discovery. A broken row is unavailable, not a batch-wide error.
    fn production_choices(
        &self,
        schedule: &PayrollSchedule,
        sending: bool,
    ) -> Result<Vec<ProductionChoice>, Box<dyn std::error::Error>> {
        use crate::payroll_evidence::lifecycle::{stage, Stage};
        let db = crate::payroll_evidence::open(&self.application)?;
        let start = crate::date_utils::parse_legacy(&schedule.first_week_commencing)?;
        let mut choices = Vec::new();
        for pa in self.application.personal_assistant_repository.get_all()? {
            let has_record: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheets WHERE personal_assistant_id=?1 AND payroll_year=?2 AND cycle_number=?3)", rusqlite::params![pa.id,schedule.payroll_year,schedule.cycle_number], |r|r.get(0))?;
            let eligibility =
                pa.eligible_for_period(start, start + chrono::Duration::days(27), has_record);
            if matches!(eligibility, Ok(false)) {
                continue;
            }
            let mut intent = None;
            let readiness = (|| -> Result<String, Box<dyn std::error::Error>> {
                eligibility?;
                let record = self
                    .application
                    .payroll_timesheet_repository
                    .get_for_cycle_and_pa(&schedule.payroll_year, schedule.cycle_number, pa.id)?
                    .ok_or("Preparation missing — prepare and save this PA first")?;
                let current_stage = stage(&db, &record)?;
                match current_stage {
                    Stage::Settled if !sending => return Err("Settled — protected".into()),
                    Stage::Indeterminate => {
                        return Err("Delivery uncertain — do not regenerate or resend".into())
                    }
                    _ => {}
                }
                let metadata = self
                    .application
                    .payroll_worked_item_repository
                    .snapshot_metadata(record.id)?;
                if sending {
                    metadata
                        .as_ref()
                        .ok_or("No current PDF — generate this PA's PDF first")?;
                    let path = crate::payroll_snapshot_service::attachment_path(
                        &self.application,
                        pa.id,
                        schedule,
                    )?;
                    // No evidence preflight here: displaying unselected PAs must never write.
                    crate::payroll_snapshot_service::verify_preview_or_test_attachment(
                        &self.application.payroll_worked_item_repository,
                        record.id,
                        &path,
                    )?;
                    intent = Some(crate::timesheet_delivery::capture(&db, record.id)?);
                    Ok(format!(
                        "{}: {}",
                        if metadata
                            .as_ref()
                            .is_some_and(|m| m.state == SnapshotState::Submitted)
                        {
                            "Submitted / sent — resend existing PDF"
                        } else if current_stage == Stage::Submitted {
                            "Previously submitted / sent — regenerated PDF ready"
                        } else {
                            "Candidate ready (evidence checked on send)"
                        },
                        path.display()
                    ))
                } else {
                    Ok(if current_stage == Stage::Submitted {
                        "Submitted / sent — regenerate from current data"
                    } else if metadata.is_some() {
                        "Editable — replaces existing candidate"
                    } else {
                        "Editable — ready to generate"
                    }
                    .into())
                }
            })();
            choices.push(ProductionChoice {
                id: pa.id,
                name: format!("{} {}", pa.first_name, pa.surname),
                available: readiness.is_ok(),
                detail: readiness.unwrap_or_else(|e| e.to_string()),
                intent,
            });
        }
        Ok(choices)
    }

    fn begin_generation(&mut self) {
        let result = (|| -> Result<PendingGeneration, Box<dyn std::error::Error>> {
            let schedule = self.selected_operational_payroll_schedule()?;
            let choices = self.production_choices(&schedule, false)?;
            Ok(PendingGeneration {
                period: capture_operational_payroll_period(&schedule),
                revision: self.operational_payroll_period.revision,
                selected_ids: choices
                    .iter()
                    .filter(|c| c.available)
                    .map(|c| c.id)
                    .collect(),
                choices,
            })
        })();
        match result {
            Ok(pending) => self.pending_generation = Some(pending),
            Err(e) => self.status_message = format!("Cannot begin generation: {e}"),
        }
    }

    fn validated_generation_schedule(
        &self,
        pending: &PendingGeneration,
    ) -> Result<PayrollSchedule, Box<dyn std::error::Error>> {
        let schedule = self
            .application
            .payroll_schedule_repository
            .get_for_year_and_cycle(
                &pending.period.key.payroll_year,
                pending.period.key.cycle_number,
            )?;
        validate_captured_email_batch_period(
            &pending.period,
            pending.revision,
            &self.operational_payroll_period,
            schedule.as_ref(),
        )?;
        schedule.ok_or_else(|| "Captured payroll period no longer exists".into())
    }
    fn generate_selection(
        &self,
        pending: &PendingGeneration,
    ) -> Result<ProductionReport, Box<dyn std::error::Error>> {
        let schedule = self.validated_generation_schedule(pending)?;
        self.process_selected(&schedule, &pending.selected_ids, "Generation", |pa| {
            self.generate_payroll_timesheet(&schedule, pa)
        })
    }
    fn dispatch_timesheet_selection(
        &self,
        batch: &PendingEmailBatch,
    ) -> Result<ProductionReport, Box<dyn std::error::Error>> {
        let schedule = self
            .validated_schedule_for_email_batch(batch)?
            .ok_or("Timesheets require a period")?;
        if batch.stage != EmailBatchNoteStage::ConfirmDispatch
            || batch.approved_selection != batch.selected_personal_assistant_ids
        {
            return Err("Selection changed or not confirmed; review the batch again".into());
        }
        if batch.choices.iter().any(|c| {
            batch.selected_personal_assistant_ids.contains(&c.id)
                && c.intent.as_ref().is_some_and(|i| i.resend)
        }) && !batch.resend_acknowledged
        {
            return Err("Explicit resend acknowledgement required".into());
        }
        self.process_selected(
            &schedule,
            &batch.selected_personal_assistant_ids,
            "Timesheet email",
            |pa| self.email_timesheet_intent(&schedule, pa, batch),
        )
    }

    /// Stop on the first failure, retaining explicit results for every captured ID.
    /// Fetch/validate only the PA about to execute; never reload the whole run.
    fn process_selected<F>(
        &self,
        schedule: &PayrollSchedule,
        ids: &[i64],
        action: &'static str,
        mut operation: F,
    ) -> Result<ProductionReport, Box<dyn std::error::Error>>
    where
        F: FnMut(&crate::models::PersonalAssistant) -> Result<bool, Box<dyn std::error::Error>>,
    {
        use crate::payroll_evidence::lifecycle::{stage, Stage};
        if ids.is_empty() {
            return Err("Select at least one Personal Assistant".into());
        }
        if ids.iter().copied().collect::<HashSet<_>>().len() != ids.len() {
            return Err("Duplicate PA selection".into());
        }
        let db = crate::payroll_evidence::open(&self.application)?;
        let start = crate::date_utils::parse_legacy(&schedule.first_week_commencing)?;
        let mut report = ProductionReport {
            action,
            period: capture_operational_payroll_period(schedule).display_label,
            entries: Vec::new(),
        };
        let mut failed = false;
        for &id in ids {
            let mut name = db
                .query_row(
                    "SELECT first_name || ' ' || surname FROM personal_assistants WHERE id=?1",
                    [id],
                    |r| r.get::<_, String>(0),
                )
                .unwrap_or_else(|_| format!("PA {id}"));
            if failed {
                report
                    .entries
                    .push((id, name, ProductionOutcome::NotAttempted));
                continue;
            }
            let result = (|| -> Result<ProductionOutcome, Box<dyn std::error::Error>> {
                let pa = crate::personal_assistant_repository::PersonalAssistantRepository::get_by_id_on(&db,id)?;
                name = format!("{} {}", pa.first_name, pa.surname);
                let record = self
                    .application
                    .payroll_timesheet_repository
                    .get_for_cycle_and_pa(&schedule.payroll_year, schedule.cycle_number, id)?;
                if !pa.eligible_for_period(
                    start,
                    start + chrono::Duration::days(27),
                    record.is_some(),
                )? {
                    return Err("PA is not eligible for the captured payroll period".into());
                }
                let record =
                    record.ok_or("Preparation missing — prepare and save this PA first")?;
                match stage(&db, &record)? {
                    Stage::Settled if action == "Generation" => {
                        return Ok(ProductionOutcome::Skipped(
                            "Already submitted/settled — protected".into(),
                        ))
                    }
                    Stage::Indeterminate => {
                        return Err("Delivery uncertain — automatic retry refused".into())
                    }
                    _ => {}
                }
                Ok(if operation(&pa)? {
                    ProductionOutcome::Completed
                } else {
                    ProductionOutcome::Skipped("Already processed — protected".into())
                })
            })();
            let outcome = result.unwrap_or_else(|e| {
                failed = true;
                ProductionOutcome::Failed(e.to_string())
            });
            report.entries.push((id, name, outcome));
        }
        Ok(report)
    }

    fn draw_generation_selection(&mut self, ui: &mut egui::Ui) {
        let Some(mut pending) = self.pending_generation.take() else {
            return;
        };
        let mut execute = false;
        let mut cancel = false;
        egui::Window::new("Generate payroll timesheets — select PAs")
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                ui.label(&pending.period.display_label);
                draw_production_choices(ui, &pending.choices, &mut pending.selected_ids, false);
                ui.horizontal(|ui| {
                    execute = ui
                        .add_enabled(
                            !pending.selected_ids.is_empty(),
                            egui::Button::new("Generate selected PAs"),
                        )
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if execute {
            match self.generate_selection(&pending) {
                Ok(report) => {
                    self.status_message = report.summary();
                    self.file_status = (report.completed() > 0).then(|| {
                        (
                            self.status_message.clone(),
                            vec![crate::paths::expand_path(
                                &self.application.context.config.folders.pdf_output,
                            )],
                        )
                    });
                    self.production_report = Some(report);
                }
                Err(e) => {
                    self.status_message = format!("Generation refused: {e}");
                }
            }
        } else if !cancel {
            self.pending_generation = Some(pending);
        }
    }
    fn draw_recipient_selection(&mut self, ui: &mut egui::Ui) {
        let Some(batch) = self
            .pending_email_batch
            .as_mut()
            .filter(|b| matches!(b.stage, EmailBatchNoteStage::SelectRecipients))
        else {
            return;
        };
        let mut cancel = false;
        let payslips = matches!(batch.kind, PayrollEmailKind::Payslip);
        egui::Window::new(if payslips { "Email payslips — select PAs" } else { "Email payroll timesheets — select PAs" })
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                if let Some(period) = &batch.payroll_period {
                    ui.label(&period.display_label);
                }
                if payslips {
                    ui.label("Choose the PAs for this batch. Reviewed supplements marked Needs sending are included as shown. Each message goes to the PA, with a blind copy to the employer and no payroll department copy.");
                }
                let previous_selection=batch.selected_personal_assistant_ids.clone();
                draw_production_choices(
                    ui,
                    &batch.choices,
                    &mut batch.selected_personal_assistant_ids,
                    !payslips,
                );
                if previous_selection!=batch.selected_personal_assistant_ids {batch.resend_acknowledged=false;batch.approved.clear();batch.approved_selection.clear();}
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !batch.selected_personal_assistant_ids.is_empty(),
                            egui::Button::new(if payslips { "Review selected recipients" } else { "Continue to email notes" }),
                        )
                        .clicked()
                    {
                        batch.confirm_selection();
                    }
                    cancel = ui.button("Cancel").clicked();
                });
                if batch.selected_personal_assistant_ids.is_empty() {
                    ui.label("Select at least one Personal Assistant to continue.");
                }
            });
        if cancel {
            self.clear_pending_email_batch();
        }
    }
    fn draw_production_report(&self, ui: &mut egui::Ui) {
        if let Some(report) = &self.production_report {
            ui.group(|ui| {
                ui.strong(format!("{} results — {}",report.action,report.period));
                for (id,name,outcome) in &report.entries {
                    let text = match outcome {
                        ProductionOutcome::Completed => "Completed".into(),
                        ProductionOutcome::Skipped(reason) => format!("Skipped: {reason}"),
                        ProductionOutcome::Failed(reason) => format!("Failed / needs attention: {reason}"),
                        ProductionOutcome::NotAttempted => "Not attempted after earlier failure".into(),
                    };
                    ui.label(format!("{name} (PA {id}): {text}"));
                }
                ui.label("Start a new selection to regenerate or resend. Selecting a sent PA for email sends the existing PDF again.");
            });
        }
    }

    // Existing batch eligibility tests retain the all-PA execution entry point.
    #[cfg(test)]
    fn generate_payroll_timesheets(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let schedule = self.selected_operational_payroll_schedule()?;
        let ids = self
            .selected_period_assistants()?
            .iter()
            .map(|pa| pa.id)
            .collect::<Vec<_>>();
        let report = self.process_selected(&schedule, &ids, "Generation", |pa| {
            self.generate_payroll_timesheet(&schedule, pa)
        })?;
        if let Some((_, _, ProductionOutcome::Failed(error))) = report
            .entries
            .iter()
            .find(|(_, _, r)| matches!(r, ProductionOutcome::Failed(_)))
        {
            return Err(error.clone().into());
        }
        Ok(report.completed())
    }
}

#[cfg(test)]
#[path = "payroll_production_tests.rs"]
mod payroll_production_tests;

impl DirectPaymentApp {
    fn draw_delivery_attempt_history(&mut self, ui: &mut egui::Ui) {
        let result = (|| -> crate::payroll_evidence::Result<_> {
            let db = crate::payroll_evidence::open(&self.application)?;
            let history = crate::timesheet_delivery::history(&db)?;
            let legacy=db.prepare("SELECT p.id,p.payroll_year||' payroll period '||p.cycle_number FROM payroll_timesheets p LEFT JOIN payroll_timesheet_snapshot_states s ON s.payroll_timesheet_id=p.id WHERE (s.state='indeterminate' OR EXISTS(SELECT 1 FROM payroll_timesheet_email_status e WHERE e.personal_assistant_id=p.personal_assistant_id AND e.payroll_year=p.payroll_year AND e.cycle_number=p.cycle_number AND e.email_type='timesheet' AND e.sent_at LIKE 'indeterminate:%')) AND NOT EXISTS(SELECT 1 FROM timesheet_delivery_attempts a WHERE a.payroll_timesheet_id=p.id AND a.outcome='uncertain' AND a.resolved_at IS NULL)")?.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let reviews=db.prepare("SELECT payroll_timesheet_id,decision,reason,actor,reviewed_at,original_evidence FROM timesheet_delivery_reviews WHERE attempt_id IS NULL ORDER BY id DESC")?.query_map([],|r|Ok(format!("Timesheet {}: {} — {} — {} at {}. Original: {}",r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((history, legacy, reviews))
        })();
        match result {
            Err(e) => {
                ui.label(format!("Delivery history unavailable: {e}"));
            }
            Ok((history, legacy, reviews)) => {
                ui.collapsing("Timesheet transport attempts and uncertainty review",|ui|{
                ui.label("Accepted means SMTP transport acceptance; it does not prove recipient delivery. An interrupted/uncertain attempt must never be automatically retried.");
                for entry in history {
                    ui.collapsing(format!("Attempt {} — {} — {} — {}",entry.id,entry.period,entry.classification,entry.outcome),|ui|{
                        ui.label(format!("Document {}; submission {:?}; {}; Message-ID {}",entry.document,entry.submission,entry.recipients,entry.message_id));
                        ui.label(format!("Started {}; completed {}. {}",entry.started,entry.completed.as_deref().unwrap_or("not established"),entry.detail));
                        if let Some(resolution)=entry.resolution {ui.label(format!("Audited review: {resolution}"));}
                        else if entry.outcome=="uncertain" {self.draw_attempt_review_controls(ui,entry.id,false);}
                    });
                }
                for (record,period) in legacy {
                    ui.label(format!("Legacy uncertain timesheet {record} — {period}. No historical attempt is invented."));
                    self.draw_attempt_review_controls(ui,record,true);
                }
                for review in reviews {ui.label(review);}
            });
            }
        }
        if let Some((id, decision, reason, legacy)) = self.pending_delivery_review.clone() {
            let review_context = (|| -> crate::payroll_evidence::Result<String> {
                let db = crate::payroll_evidence::open(&self.application)?;
                if legacy {
                    Ok(db.query_row("SELECT payroll_year||' payroll period '||cycle_number FROM payroll_timesheets WHERE id=?1",[id],|r|r.get(0))?)
                } else {
                    Ok(db.query_row("SELECT p.payroll_year||' payroll period '||p.cycle_number||'; document '||a.document_id||'; '||a.classification||'; '||a.recipients||'; Message-ID '||a.message_id FROM timesheet_delivery_attempts a JOIN payroll_timesheets p ON p.id=a.payroll_timesheet_id WHERE a.id=?1",[id],|r|r.get(0))?)
                }
            })();
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Confirm audited delivery review").collapsible(false).show(ui.ctx(),|ui|{
                ui.label(format!("{} {id}: {decision}",if legacy {"Legacy timesheet"} else {"Delivery attempt"}));
                ui.label(review_context.as_ref().map(String::as_str).unwrap_or("Review identity unavailable; cannot confirm"));
                ui.label(&reason);
                ui.label("Confirm only with evidence establishing acceptance or non-acceptance. Inconclusive evidence must remain blocked. This decision is permanently recorded and does not send email.");
                confirm=ui.add_enabled(review_context.is_ok(),egui::Button::new("Confirm documented decision")).clicked();
                cancel=ui.button("Cancel review").clicked();
            });
            if confirm {
                let outcome = (|| -> crate::payroll_evidence::Result<()> {
                    let mut db = crate::payroll_evidence::open(&self.application)?;
                    if legacy {
                        crate::timesheet_delivery::review_legacy(&mut db, id, &decision, &reason)
                    } else {
                        crate::timesheet_delivery::review(&mut db, id, &decision, &reason)
                    }
                })();
                self.status_message = match outcome {
                    Ok(()) => "Delivery review recorded; start a new selection to continue.".into(),
                    Err(e) => format!("Review refused; delivery remains protected: {e}"),
                };
                self.pending_delivery_review = None;
                self.delivery_review_reason.clear();
                self.clear_pending_email_batch();
            } else if cancel {
                self.pending_delivery_review = None;
            }
        }
    }
    fn draw_attempt_review_controls(&mut self, ui: &mut egui::Ui, id: i64, legacy: bool) {
        ui.label("Record how acceptance/non-acceptance was verified (for example, payroll confirmation or SMTP logs). Do not infer non-delivery from an exception.");
        ui.text_edit_multiline(&mut self.delivery_review_reason);
        let ready = !self.delivery_review_reason.trim().is_empty();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(ready, egui::Button::new("Review: acceptance established"))
                .clicked()
            {
                self.pending_delivery_review = Some((
                    id,
                    "accepted".into(),
                    self.delivery_review_reason.clone(),
                    legacy,
                ));
            }
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new("Review: non-acceptance established"),
                )
                .clicked()
            {
                self.pending_delivery_review = Some((
                    id,
                    "confirmed_not_sent".into(),
                    self.delivery_review_reason.clone(),
                    legacy,
                ));
            }
        });
    }
}
