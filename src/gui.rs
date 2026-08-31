use eframe::egui;
use std::collections::{HashMap, HashSet};

use crate::app::Application;
use crate::application_settings_screen::ApplicationSettingsScreen;
use crate::email_service::PayrollEmailPreview;
use crate::import_service::ImportSummary;
use crate::models::TimesheetEntry;
use crate::payroll_schedule_repository::PayrollSchedule;
use crate::payroll_settings_screen::PayrollSettingsScreen;
use crate::payroll_timesheet_screen::PayrollTimesheetScreen;
use crate::pdf_generator::{PdfGenerator, TimesheetPdfData};
use crate::personal_assistant_screen::PersonalAssistantScreen;

enum ActiveScreen {
    Dashboard,
    Employer,
    PersonalAssistant,
    PayrollSettings,
    PayrollTimesheet,
    ApplicationSettings,
    EmailSettings,
}

#[derive(Clone, Copy)]
enum PayrollEmailKind {
    Timesheet,
    Payslip,
}

#[derive(Clone, Copy)]
enum EmailBatchNoteStage {
    ChooseAdditionalNote,
    EditAdditionalNotes,
    ConfirmDispatch,
}

#[derive(Clone)]
struct PendingEmailBatch {
    kind: PayrollEmailKind,
    stage: EmailBatchNoteStage,
    selected_personal_assistant_ids: Vec<i64>,
    payroll_period: CapturedOperationalPayrollPeriod,
    operational_selection_revision: u64,
}

struct PendingPayrollReturnImport {
    schedules: Vec<PayrollSchedule>,
    selected_schedule_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OperationalPayrollPeriodKey {
    payroll_year: String,
    cycle_number: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedOperationalPayrollPeriod {
    key: OperationalPayrollPeriodKey,
    first_week_commencing: String,
    pay_date: String,
    display_label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OperationalPayrollPeriodState {
    selected: Option<OperationalPayrollPeriodKey>,
    explicitly_selected: bool,
    revision: u64,
}

impl OperationalPayrollPeriodState {
    fn select(&mut self, schedule: &PayrollSchedule, current: Option<&PayrollSchedule>) {
        let selected = operational_period_key(schedule);
        if self.selected.as_ref() != Some(&selected) {
            self.revision = self.revision.saturating_add(1);
        }
        self.selected = Some(selected);
        self.explicitly_selected = current.is_none_or(|current| {
            operational_period_key(current) != operational_period_key(schedule)
        });
    }

    fn use_current(&mut self, current: &PayrollSchedule) {
        let selected = operational_period_key(current);
        if self.selected.as_ref() != Some(&selected) {
            self.revision = self.revision.saturating_add(1);
        }
        self.selected = Some(selected);
        self.explicitly_selected = false;
    }
}

pub struct DirectPaymentApp {
    application: Application,
    version: String,
    status_message: String,
    last_import: Option<ImportSummary>,
    timesheets: Vec<TimesheetEntry>,
    payroll_schedules: Vec<PayrollSchedule>,
    payroll_schedule_years: Vec<String>,
    selected_payroll_schedule_year: Option<String>,
    operational_payroll_schedules: Vec<PayrollSchedule>,
    operational_payroll_period: OperationalPayrollPeriodState,
    operational_payroll_period_error: Option<String>,
    preview_personal_assistant_id: Option<i64>,
    email_preview: Option<PayrollEmailPreview>,
    additional_notes_by_personal_assistant: HashMap<i64, String>,
    note_enabled_personal_assistant_ids: HashSet<i64>,
    pending_email_batch: Option<PendingEmailBatch>,
    pending_payroll_return_import: Option<PendingPayrollReturnImport>,
    email_settings_employer: Option<crate::models::Employer>,
    email_settings_payroll_provider: Option<crate::payroll_provider_repository::PayrollProvider>,
    employer_screen: crate::employer_screen::EmployerScreen,
    personal_assistant_screen: PersonalAssistantScreen,
    payroll_settings_screen: PayrollSettingsScreen,
    payroll_timesheet_screen: PayrollTimesheetScreen,
    application_settings_screen: ApplicationSettingsScreen,
    active_screen: ActiveScreen,
    restart_required_message: Option<String>,
}

impl DirectPaymentApp {
    pub fn new(application: Application) -> Self {
        let (operational_payroll_schedules, operational_payroll_period, operational_error) =
            initial_operational_payroll_period(&application);
        Self {
            version: application.context.version.clone(),
            application,
            status_message: "Application ready.".to_string(),
            last_import: None,
            timesheets: Vec::new(),
            payroll_schedules: Vec::new(),
            payroll_schedule_years: Vec::new(),
            selected_payroll_schedule_year: None,
            operational_payroll_schedules,
            operational_payroll_period,
            operational_payroll_period_error: operational_error,
            preview_personal_assistant_id: None,
            email_preview: None,
            additional_notes_by_personal_assistant: HashMap::new(),
            note_enabled_personal_assistant_ids: HashSet::new(),
            pending_email_batch: None,
            pending_payroll_return_import: None,
            email_settings_employer: None,
            email_settings_payroll_provider: None,
            employer_screen: crate::employer_screen::EmployerScreen::new(),
            personal_assistant_screen: PersonalAssistantScreen::new(),
            payroll_settings_screen: PayrollSettingsScreen::new(),
            payroll_timesheet_screen: PayrollTimesheetScreen::new(),
            application_settings_screen: ApplicationSettingsScreen::new(),
            active_screen: ActiveScreen::Dashboard,
            restart_required_message: None,
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(message) = &self.restart_required_message {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Restart Required");
                ui.label(message);
                ui.separator();
                if ui.button("Close DirectPaymentTimesheets").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            return;
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Direct Payments Timesheets");
                ui.label(format!("Version {}", self.version));

                if ui.button(" Settings").clicked() {
                    self.active_screen = ActiveScreen::ApplicationSettings;
                }

                if ui.button("Email Settings").clicked() {
                    self.active_screen = ActiveScreen::EmailSettings;
                }
            });
        });

        egui::TopBottomPanel::bottom("navigation").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Dashboard").clicked() {
                    self.active_screen = ActiveScreen::Dashboard;
                }

                if ui.button("Employer Maintenance").clicked() {
                    self.active_screen = ActiveScreen::Employer;
                }

                if ui.button("Personal Assistant Maintenance").clicked() {
                    self.active_screen = ActiveScreen::PersonalAssistant;
                }

                if ui.button("Payroll Settings").clicked() {
                    self.active_screen = ActiveScreen::PayrollSettings;
                }

                if ui.button("Payroll Timesheet Preparation").clicked() {
                    self.payroll_timesheet_screen.reload();
                    self.active_screen = ActiveScreen::PayrollTimesheet;
                }

                if ui.button("Exit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.active_screen {
            ActiveScreen::Dashboard => {
                self.draw_dashboard(ui);
            }

            ActiveScreen::Employer => {
                if self.employer_screen.show(ui, &self.application) {
                    self.active_screen = ActiveScreen::EmailSettings;
                }
            }

            ActiveScreen::PersonalAssistant => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.personal_assistant_screen.show(ui, &self.application);
                });
            }

            ActiveScreen::PayrollSettings => {
                if self.payroll_settings_screen.show(ui, &mut self.application) {
                    self.active_screen = ActiveScreen::EmailSettings;
                }
            }

            ActiveScreen::PayrollTimesheet => match self.selected_operational_payroll_schedule() {
                Ok(schedule) => {
                    let period_label = payroll_schedule_label(&schedule);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.payroll_timesheet_screen.show(
                            ui,
                            &self.application,
                            &schedule,
                            &period_label,
                        );
                    });
                }
                Err(error) => {
                    ui.heading("Payroll Timesheet Preparation");
                    ui.label(format!(
                        "Unable to load the selected payroll period: {error}"
                    ));
                }
            },

            ActiveScreen::ApplicationSettings => {
                let mut open_email_settings = false;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    open_email_settings = self
                        .application_settings_screen
                        .show(ui, &mut self.application);
                });
                if open_email_settings {
                    self.active_screen = ActiveScreen::EmailSettings;
                }
                if let Some(message) = self.application_settings_screen.restart_message() {
                    self.restart_required_message = Some(message.to_string());
                }
            }

            ActiveScreen::EmailSettings => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_email_settings(ui);
                });
            }
        });
    }
}

impl DirectPaymentApp {
    fn clear_pending_email_batch(&mut self) {
        self.pending_email_batch = None;
        self.additional_notes_by_personal_assistant.clear();
        self.note_enabled_personal_assistant_ids.clear();
    }

    fn begin_email_batch(&mut self, kind: PayrollEmailKind) {
        let schedule = match self.selected_operational_payroll_schedule() {
            Ok(schedule) => schedule,
            Err(error) => {
                self.status_message = format!("Could not begin production email batch: {error}");
                return;
            }
        };
        self.additional_notes_by_personal_assistant.clear();
        self.note_enabled_personal_assistant_ids.clear();
        let selected_personal_assistant_ids = self
            .application
            .personal_assistant_repository
            .get_all()
            .unwrap_or_default()
            .into_iter()
            .filter(|assistant| {
                assistant
                    .employment_status
                    .as_deref()
                    .map(|status| status.trim().eq_ignore_ascii_case("active"))
                    .unwrap_or(true)
            })
            .map(|assistant| assistant.id)
            .collect();
        self.pending_email_batch = Some(PendingEmailBatch {
            kind,
            stage: EmailBatchNoteStage::ChooseAdditionalNote,
            selected_personal_assistant_ids,
            payroll_period: capture_operational_payroll_period(&schedule),
            operational_selection_revision: self.operational_payroll_period.revision,
        });
    }

    fn draw_additional_note_prompt(&mut self, ui: &mut egui::Ui) {
        let stage = self.pending_email_batch.as_ref().map(|batch| batch.stage);
        if matches!(stage, Some(EmailBatchNoteStage::ChooseAdditionalNote)) {
            ui.label("Additional note for this email batch?");
            ui.horizontal(|ui| {
                if ui.button("Add Additional Note").clicked() {
                    if let Some(batch) = &mut self.pending_email_batch {
                        batch.stage = EmailBatchNoteStage::EditAdditionalNotes;
                    }
                }
                if ui.button("No Additional Note").clicked() {
                    self.additional_notes_by_personal_assistant.clear();
                    if let Some(batch) = &mut self.pending_email_batch {
                        batch.stage = EmailBatchNoteStage::ConfirmDispatch;
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.clear_pending_email_batch();
                }
            });
        }

        let ids = self.pending_email_batch.as_ref().and_then(|batch| {
            matches!(batch.stage, EmailBatchNoteStage::EditAdditionalNotes)
                .then(|| batch.selected_personal_assistant_ids.clone())
        });
        if let Some(ids) = ids {
            ui.separator();
            ui.label("Additional notes by Personal Assistant");
            let assistants = self
                .application
                .personal_assistant_repository
                .get_all()
                .unwrap_or_default();
            for assistant in assistants
                .into_iter()
                .filter(|assistant| ids.contains(&assistant.id))
            {
                let id = assistant.id;
                let mut enabled = self.note_enabled_personal_assistant_ids.contains(&id);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut enabled, "Add note").changed() {
                        if enabled {
                            self.note_enabled_personal_assistant_ids.insert(id);
                        } else {
                            self.note_enabled_personal_assistant_ids.remove(&id);
                            self.additional_notes_by_personal_assistant.remove(&id);
                        }
                    }
                    ui.label(format!("{} {}", assistant.first_name, assistant.surname));
                });
                if enabled {
                    let note = self
                        .additional_notes_by_personal_assistant
                        .entry(id)
                        .or_default();
                    ui.add_sized([600.0, 80.0], egui::TextEdit::multiline(note));
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Continue").clicked() {
                    if let Some(batch) = &mut self.pending_email_batch {
                        batch.stage = EmailBatchNoteStage::ConfirmDispatch;
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.clear_pending_email_batch();
                }
            });
        }

        let confirmation_batch = self.pending_email_batch.as_ref().and_then(|batch| {
            matches!(batch.stage, EmailBatchNoteStage::ConfirmDispatch).then_some(batch.clone())
        });
        if let Some(batch) = confirmation_batch {
            let email_type = match batch.kind {
                PayrollEmailKind::Timesheet => "Timesheets",
                PayrollEmailKind::Payslip => "Payslips",
            };
            let mut send = false;
            let mut cancel = false;
            egui::Window::new("Confirm email batch")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label(format!("Ready to send {}.", email_type));
                    ui.label(format!(
                        "Payroll period: {}",
                        batch.payroll_period.display_label
                    ));
                    match captured_operational_period_timing(
                        &batch.payroll_period,
                        chrono::Local::now().date_naive(),
                    ) {
                        Some(OperationalPeriodTiming::Historical) => {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                "Historical payroll period",
                            );
                        }
                        Some(OperationalPeriodTiming::Future) => {
                            ui.colored_label(ui.visuals().warn_fg_color, "Future payroll period");
                        }
                        Some(OperationalPeriodTiming::Current) | None => {}
                    }
                    ui.horizontal(|ui| {
                        send = ui.button("Send").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });

            if cancel {
                self.clear_pending_email_batch();
            } else if send {
                let result = self
                    .validated_schedule_for_email_batch(&batch)
                    .and_then(|schedule| match batch.kind {
                        PayrollEmailKind::Timesheet => self.email_timesheets(&schedule),
                        PayrollEmailKind::Payslip => self.email_payslips(&schedule),
                    });
                match result {
                    Ok(count) => {
                        self.status_message =
                            format!("{} emailed successfully: {}.", email_type, count);
                        self.clear_pending_email_batch();
                    }
                    Err(error) => {
                        self.status_message = format!("{} email failed: {}", email_type, error);
                    }
                }
            }
        }
    }

    fn draw_email_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Email Settings");
        ui.label("SMTP and test-recipient settings. Production email actions are unchanged.");
        ui.separator();

        let email = &mut self.application.context.config.email;
        ui.horizontal(|ui| {
            ui.label("Email transport");
            egui::ComboBox::from_id_salt("email_transport")
                .selected_text(&email.smtp_transport)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut email.smtp_transport,
                        "Local SMTP Server".to_string(),
                        "Local SMTP Server",
                    );
                    ui.selectable_value(
                        &mut email.smtp_transport,
                        "SMTP Server".to_string(),
                        "SMTP Server",
                    );
                });
        });

        if email.smtp_transport == "SMTP Server" {
            ui.label("SMTP Host");
            ui.text_edit_singleline(&mut email.smtp_host);
            ui.label("SMTP Port");
            ui.add(egui::DragValue::new(&mut email.smtp_port).range(1..=65535));
            ui.label("SMTP Username");
            ui.text_edit_singleline(&mut email.smtp_username);
            ui.label("SMTP Password");
            ui.add(egui::TextEdit::singleline(&mut email.smtp_password).password(true));
        } else {
            ui.label("Uses localhost:25 with no configurable credentials.");
        }
        ui.label("Payroll test email address");
        ui.text_edit_singleline(&mut email.payroll_test_email_address);
        ui.label("PA test email address");
        ui.text_edit_singleline(&mut email.pa_test_email_address);

        self.draw_test_timesheet_email_action(ui);

        ui.separator();
        self.draw_email_preview_controls(ui);

        ui.separator();
        if self.email_settings_payroll_provider.is_none() {
            self.email_settings_payroll_provider = self
                .application
                .payroll_provider_repository
                .get()
                .ok()
                .flatten();
        }

        ui.heading("Payroll Department");
        if let Some(provider) = &mut self.email_settings_payroll_provider {
            ui.label("Payroll Department email address");
            ui.text_edit_singleline(
                provider
                    .payroll_department_email
                    .get_or_insert(String::new()),
            );
            if ui.button("Save Payroll Department Email").clicked() {
                match self
                    .application
                    .payroll_provider_repository
                    .update_payroll_department_email(
                        provider.id,
                        provider.payroll_department_email.as_deref(),
                    ) {
                    Ok(()) => {
                        self.status_message = "Payroll Department email address saved.".to_string()
                    }
                    Err(error) => {
                        self.status_message =
                            format!("Failed saving Payroll Department email address: {}", error)
                    }
                }
            }
        } else {
            ui.label("No Payroll Provider has been configured.");
        }

        ui.separator();
        ui.label("Email Subject Format");
        ui.horizontal(|ui| {
            use egui::TextBuffer as _;

            let subject = &mut self.application.context.config.payroll.email_subject_format;
            let mut text_edit_output = egui::TextEdit::singleline(subject).show(ui);
            let cursor_index = text_edit_output
                .state
                .cursor
                .char_range()
                .map(|cursor_range| cursor_range.primary.index)
                .unwrap_or_else(|| subject.chars().count());
            let mut placeholder_to_insert = None;

            egui::ComboBox::from_id_salt("email_subject_insert_field")
                .selected_text("Insert Field")
                .show_ui(ui, |ui| {
                    if ui.button("Personal Assistant Name").clicked() {
                        placeholder_to_insert = Some("{Personal Assistant Name}");
                        ui.close();
                    }

                    if ui.button("Personal Assistant DOB").clicked() {
                        placeholder_to_insert = Some("{Personal Assistant DOB}");
                        ui.close();
                    }

                    if ui.button("Personal Assistant NI").clicked() {
                        placeholder_to_insert = Some("{Personal Assistant NI}");
                        ui.close();
                    }

                    if ui.button("Payroll Period (YYYYMMwWW)").clicked() {
                        placeholder_to_insert = Some("{YYYYMMwWW}");
                        ui.close();
                    }
                });

            if let Some(placeholder) = placeholder_to_insert {
                let inserted_characters = subject.insert_text(placeholder, cursor_index);
                let new_cursor_index = cursor_index + inserted_characters;
                text_edit_output
                    .state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(new_cursor_index),
                    )));
                text_edit_output
                    .state
                    .store(ui.ctx(), text_edit_output.response.id);
                text_edit_output.response.request_focus();
            }
        });
        ui.label(
            "Available fields: {Personal Assistant Name}, {Personal Assistant DOB}, {Personal Assistant NI}, {YYYYMMwWW}",
        );
        ui.label("Timesheet Email Body");
        ui.add_sized(
            [600.0, 120.0],
            egui::TextEdit::multiline(
                &mut self.application.context.config.payroll.timesheet_email_body,
            ),
        );
        ui.label("Payslip Email Body");
        ui.add_sized(
            [600.0, 120.0],
            egui::TextEdit::multiline(
                &mut self.application.context.config.payroll.payslip_email_body,
            ),
        );

        if self.email_settings_employer.is_none() {
            self.email_settings_employer = self
                .application
                .employer_repository
                .get_all()
                .ok()
                .and_then(|employers| employers.into_iter().next());
        }

        ui.separator();
        ui.heading("Employer Email Signature");
        let mut saved_employer = None;
        if let Some(employer) = &mut self.email_settings_employer {
            ui.add_sized(
                [600.0, 120.0],
                egui::TextEdit::multiline(employer.email_signature.get_or_insert(String::new())),
            );
            if ui.button("Save Employer Email Signature").clicked() {
                match self
                    .application
                    .employer_repository
                    .update_email_signature(employer.id, employer.email_signature.as_deref())
                {
                    Ok(()) => {
                        saved_employer = Some(employer.clone());
                        self.status_message = "Employer email signature saved.".to_string();
                    }
                    Err(error) => {
                        self.status_message =
                            format!("Failed saving employer email signature: {}", error)
                    }
                }
            }
        } else {
            ui.label("No employer has been configured.");
        }

        if let Some(saved_employer) = saved_employer {
            self.employer_screen
                .synchronize_email_signature(&saved_employer);
        }

        if ui.button("Save Email Settings").clicked() {
            match self.application.save_config() {
                Ok(()) => self.status_message = "Email settings saved.".to_string(),
                Err(error) => {
                    self.status_message = format!("Failed saving email settings: {}", error)
                }
            }
        }
        ui.label(&self.status_message);
    }

    fn draw_test_timesheet_email_action(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Test Timesheet Email");
        ui.label("Test delivery uses only the configured test recipients.");

        let assistants = match self.application.personal_assistant_repository.get_all() {
            Ok(assistants) => assistants
                .into_iter()
                .filter(|assistant| {
                    assistant
                        .employment_status
                        .as_deref()
                        .map(|status| status.trim().eq_ignore_ascii_case("active"))
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>(),
            Err(error) => {
                ui.label(format!("Unable to load Personal Assistants: {}", error));
                return;
            }
        };

        if assistants.is_empty() {
            ui.label("No active Personal Assistants are available for a test email.");
            return;
        }

        if !assistants
            .iter()
            .any(|assistant| Some(assistant.id) == self.preview_personal_assistant_id)
        {
            self.preview_personal_assistant_id = Some(assistants[0].id);
        }

        let selected_name = assistants
            .iter()
            .find(|assistant| Some(assistant.id) == self.preview_personal_assistant_id)
            .map(|assistant| format!("{} {}", assistant.first_name, assistant.surname))
            .unwrap_or_default();

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("test_timesheet_personal_assistant")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for assistant in &assistants {
                        ui.selectable_value(
                            &mut self.preview_personal_assistant_id,
                            Some(assistant.id),
                            format!("{} {}", assistant.first_name, assistant.surname),
                        );
                    }
                });

            if ui.button("Test Timesheet Email").clicked() {
                self.send_test_timesheet_email();
            }

            if ui.button("Test Payslip Email").clicked() {
                self.send_test_payslip_email();
            }
        });
    }

    fn send_test_timesheet_email(&mut self) {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let payroll_test_email = self
                .application
                .context
                .config
                .email
                .payroll_test_email_address
                .trim();
            if payroll_test_email.is_empty() {
                return Err(
                    "Payroll test email address is required before sending a test email.".into(),
                );
            }

            let personal_assistant_id = self
                .preview_personal_assistant_id
                .ok_or("Select a Personal Assistant to test an email.")?;
            let assistant = self
                .application
                .personal_assistant_repository
                .get_all()?
                .into_iter()
                .find(|assistant| assistant.id == personal_assistant_id)
                .ok_or("Selected Personal Assistant was not found.")?;

            let employer = self
                .email_settings_employer
                .clone()
                .or_else(|| {
                    self.application
                        .employer_repository
                        .get_all()
                        .ok()
                        .and_then(|employers| employers.into_iter().next())
                })
                .ok_or("No employer has been configured.")?;
            let sender_email = employer
                .email
                .as_deref()
                .map(str::trim)
                .filter(|email| !email.is_empty())
                .ok_or("Employer has no email address.")?;

            let current_schedule = self.selected_operational_payroll_schedule()?;

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);
            let attachment_path = crate::payroll_file_naming::timesheet_path(
                &crate::paths::expand_path(&self.application.context.config.folders.pdf_output),
                &personal_assistant_name,
                &current_schedule,
            )?;
            let pa_test_email = self
                .application
                .context
                .config
                .email
                .pa_test_email_address
                .as_str();

            self.application.send_test_payroll_email(
                sender_email,
                payroll_test_email,
                Some(pa_test_email),
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                &current_schedule,
                &attachment_path,
                &self.application.context.config.payroll.timesheet_email_body,
                self.additional_notes_by_personal_assistant
                    .get(&assistant.id)
                    .map(String::as_str),
                employer.email_signature.as_deref(),
            )
        })();

        match result {
            Ok(()) => self.status_message = "Test timesheet email sent.".to_string(),
            Err(error) => self.status_message = format!("Test timesheet email failed: {}", error),
        }
    }

    fn send_test_payslip_email(&mut self) {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let pa_test_email = self
                .application
                .context
                .config
                .email
                .pa_test_email_address
                .trim();
            if pa_test_email.is_empty() {
                return Err(
                    "PA test email address is required before sending a test email.".into(),
                );
            }

            let personal_assistant_id = self
                .preview_personal_assistant_id
                .ok_or("Select a Personal Assistant to test an email.")?;
            let assistant = self
                .application
                .personal_assistant_repository
                .get_all()?
                .into_iter()
                .find(|assistant| assistant.id == personal_assistant_id)
                .ok_or("Selected Personal Assistant was not found.")?;

            let employer = self
                .email_settings_employer
                .clone()
                .or_else(|| {
                    self.application
                        .employer_repository
                        .get_all()
                        .ok()
                        .and_then(|employers| employers.into_iter().next())
                })
                .ok_or("No employer has been configured.")?;
            let sender_email = employer
                .email
                .as_deref()
                .map(str::trim)
                .filter(|email| !email.is_empty())
                .ok_or("Employer has no email address.")?;

            let current_schedule = self.selected_operational_payroll_schedule()?;

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);
            let attachment_path = crate::payroll_file_naming::payslip_path(
                &crate::paths::expand_path(&self.application.context.config.folders.payslip_folder),
                &personal_assistant_name,
                &current_schedule,
            )?;

            self.application.send_test_payslip_email(
                sender_email,
                pa_test_email,
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                &current_schedule,
                &attachment_path,
                &self.application.context.config.payroll.payslip_email_body,
                self.additional_notes_by_personal_assistant
                    .get(&assistant.id)
                    .map(String::as_str),
                employer.email_signature.as_deref(),
            )
        })();

        match result {
            Ok(()) => self.status_message = "Test payslip email sent.".to_string(),
            Err(error) => self.status_message = format!("Test payslip email failed: {}", error),
        }
    }

    fn selected_operational_payroll_schedule(
        &self,
    ) -> Result<PayrollSchedule, Box<dyn std::error::Error>> {
        let selected = self
            .operational_payroll_period
            .selected
            .as_ref()
            .ok_or("No operational payroll period is selected.")?;
        self.application
            .payroll_schedule_repository
            .get_for_year_and_cycle(&selected.payroll_year, selected.cycle_number)?
            .ok_or_else(|| {
                format!(
                    "The selected payroll period for {} is no longer available. Refresh the payroll periods and select another period.",
                    selected.payroll_year
                )
                .into()
            })
    }

    fn validated_schedule_for_email_batch(
        &self,
        batch: &PendingEmailBatch,
    ) -> Result<PayrollSchedule, Box<dyn std::error::Error>> {
        let schedule = self
            .application
            .payroll_schedule_repository
            .get_for_year_and_cycle(
                &batch.payroll_period.key.payroll_year,
                batch.payroll_period.key.cycle_number,
            )?;
        validate_captured_email_batch_period(
            &batch.payroll_period,
            batch.operational_selection_revision,
            &self.operational_payroll_period,
            schedule.as_ref(),
        )?;
        schedule.ok_or_else(|| "The captured payroll period is unavailable.".into())
    }

    fn refresh_operational_payroll_schedules(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let schedules = ordered_payroll_schedules(
            self.application.payroll_schedule_repository.get_all()?,
            None,
        );
        let current = self
            .application
            .payroll_schedule_repository
            .resolve_for_date(chrono::Local::now().date_naive())
            .ok();
        let mut needs_default = self.operational_payroll_period.selected.is_none();
        if let Some(selected) = &self.operational_payroll_period.selected {
            if !schedules
                .iter()
                .any(|schedule| operational_period_key(schedule) == *selected)
            {
                needs_default = true;
                self.email_preview = None;
                self.operational_payroll_period_error = Some(
                    "The previously selected payroll period is no longer available. Select another payroll period."
                        .to_string(),
                );
            }
        }
        if needs_default {
            self.operational_payroll_period =
                default_operational_payroll_period(&schedules, current.as_ref());
        }
        self.operational_payroll_schedules = schedules;
        Ok(())
    }

    fn draw_operational_payroll_period_selector(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label("Payroll period for generation and email operations:");

            if self.operational_payroll_schedules.is_empty() {
                ui.label("No imported payroll schedules are available.");
                if let Some(error) = &self.operational_payroll_period_error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                return;
            }

            let current = self
                .application
                .payroll_schedule_repository
                .resolve_for_date(chrono::Local::now().date_naive())
                .ok();
            let selected_schedule =
                self.operational_payroll_period
                    .selected
                    .as_ref()
                    .and_then(|selected| {
                        self.operational_payroll_schedules
                            .iter()
                            .find(|schedule| operational_period_key(schedule) == *selected)
                    });
            let selected_text = selected_schedule
                .map(payroll_schedule_label)
                .unwrap_or_else(|| "Select a payroll period".to_string());
            let mut selected_key = self.operational_payroll_period.selected.clone();

            egui::ComboBox::from_id_salt("operational_payroll_period")
                .width(540.0)
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for schedule in &self.operational_payroll_schedules {
                        ui.selectable_value(
                            &mut selected_key,
                            Some(operational_period_key(schedule)),
                            payroll_schedule_label(schedule),
                        );
                    }
                });

            if selected_key != self.operational_payroll_period.selected {
                if let Some(selected_key) = selected_key {
                    if let Some(schedule) = self
                        .operational_payroll_schedules
                        .iter()
                        .find(|schedule| operational_period_key(schedule) == selected_key)
                    {
                        self.operational_payroll_period
                            .select(schedule, current.as_ref());
                        self.operational_payroll_period_error = None;
                        self.email_preview = None;
                    }
                }
            }

            if let Some(schedule) =
                self.operational_payroll_period
                    .selected
                    .as_ref()
                    .and_then(|selected| {
                        self.operational_payroll_schedules
                            .iter()
                            .find(|schedule| operational_period_key(schedule) == *selected)
                    })
            {
                match operational_period_timing(schedule, chrono::Local::now().date_naive()) {
                    Some(OperationalPeriodTiming::Historical) => {
                        ui.label("Historical payroll period selected");
                    }
                    Some(OperationalPeriodTiming::Future) => {
                        ui.label("Future payroll period selected");
                    }
                    Some(OperationalPeriodTiming::Current) | None => {}
                }
            }

            if self.operational_payroll_period.explicitly_selected {
                if let Some(current) = current {
                    if ui.button("Use current payroll period").clicked() {
                        self.operational_payroll_period.use_current(&current);
                        self.operational_payroll_period_error = None;
                        self.email_preview = None;
                    }
                }
            }

            if let Some(error) = &self.operational_payroll_period_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        });
    }

    fn begin_payroll_return_import(&mut self) {
        let result = (|| -> Result<PendingPayrollReturnImport, Box<dyn std::error::Error>> {
            let today = chrono::Local::now().date_naive();
            let current_schedule_id = self
                .application
                .resolve_payroll_schedule(today)
                .ok()
                .map(|schedule| schedule.id);
            let schedules = ordered_payroll_schedules(
                self.application.payroll_schedule_repository.get_all()?,
                current_schedule_id,
            );
            let selected_schedule_id =
                default_payroll_return_schedule_id(&schedules, current_schedule_id)
                    .ok_or("No imported Payroll Prep Sheet schedules are available.")?;

            Ok(PendingPayrollReturnImport {
                schedules,
                selected_schedule_id,
            })
        })();

        match result {
            Ok(pending) => self.pending_payroll_return_import = Some(pending),
            Err(error) => {
                self.pending_payroll_return_import = None;
                self.status_message = format!("Payroll return import failed: {error}");
            }
        }
    }

    fn draw_payroll_return_schedule_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = &mut self.pending_payroll_return_import else {
            return;
        };

        let mut import = false;
        let mut cancel = false;
        egui::Window::new("Choose Payroll Return period")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "Choose the payroll period this return belongs to, then select the Payroll Return ZIP file.",
                );

                let selected_text = selected_payroll_return_schedule(
                    &pending.schedules,
                    pending.selected_schedule_id,
                )
                .map(payroll_schedule_label)
                .unwrap_or_else(|| "Select a payroll period".to_string());

                egui::ComboBox::from_id_salt("payroll_return_schedule_selection")
                    .width(540.0)
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for schedule in &pending.schedules {
                            ui.selectable_value(
                                &mut pending.selected_schedule_id,
                                schedule.id,
                                payroll_schedule_label(schedule),
                            );
                        }
                    });

                if let Some(schedule) = selected_payroll_return_schedule(
                    &pending.schedules,
                    pending.selected_schedule_id,
                ) {
                    ui.separator();
                    match payroll_schedule_details(schedule) {
                        Ok(details) => {
                            for detail in details {
                                ui.label(detail);
                            }
                        }
                        Err(error) => {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                format!("Cannot use this schedule: {error}"),
                            );
                        }
                    }
                }

                ui.horizontal(|ui| {
                    import = ui.button("Choose Payroll Return ZIP").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });

        if cancel {
            self.pending_payroll_return_import = None;
            self.status_message = "Payroll return import cancelled; no files were changed.".into();
        } else if import {
            let schedule = self
                .pending_payroll_return_import
                .as_ref()
                .and_then(|pending| {
                    selected_payroll_return_schedule(
                        &pending.schedules,
                        pending.selected_schedule_id,
                    )
                })
                .cloned();
            self.pending_payroll_return_import = None;

            let Some(schedule) = schedule else {
                self.status_message =
                    "Payroll return import failed: no payroll period was selected.".to_string();
                return;
            };
            if crate::payroll_file_naming::paye_week(&schedule).is_err() {
                self.status_message =
                    "Payroll return import failed: the selected schedule has an invalid pay date."
                        .to_string();
                return;
            }

            if let Some(path) = rfd::FileDialog::new()
                .add_filter("ZIP files", &["zip"])
                .pick_file()
            {
                match self.application.import_payroll_return(&path, &schedule) {
                    Ok(result) => {
                        self.status_message = format!(
                            "Payroll return imported: {} payslip(s), {} information file(s).",
                            result.payslips_imported, result.information_files_imported
                        );
                    }
                    Err(error) => {
                        self.status_message = format!("Payroll return import failed: {error}");
                    }
                }
            }
        }
    }

    fn begin_view_payroll_schedule(&mut self) {
        let result = (|| -> Result<(Vec<String>, String), Box<dyn std::error::Error>> {
            let schedules = self.application.payroll_schedule_repository.get_all()?;
            let years = payroll_schedule_years_newest_first(&schedules);
            if years.is_empty() {
                return Err("No imported payroll schedules are available.".into());
            }

            let today = chrono::Local::now().date_naive();
            let current_year = match self
                .application
                .payroll_schedule_repository
                .resolve_for_date(today)
            {
                Ok(schedule) => Some(schedule.payroll_year),
                Err(
                    crate::payroll_schedule_repository::PayrollScheduleResolutionError::NoCurrentCycle(
                        _,
                    ),
                ) => None,
                Err(error) => {
                    return Err(format!(
                        "Could not determine the current payroll year: {error}"
                    )
                    .into());
                }
            };
            let selected_year = default_payroll_schedule_year(&years, current_year.as_deref())
                .ok_or("No imported payroll schedules are available.")?;

            Ok((years, selected_year))
        })();

        match result {
            Ok((years, selected_year)) => {
                self.payroll_schedule_years = years;
                self.load_payroll_schedule_year(&selected_year);
            }
            Err(error) => {
                self.payroll_schedule_years.clear();
                self.selected_payroll_schedule_year = None;
                self.payroll_schedules.clear();
                self.status_message = format!("Failed loading payroll schedule: {error}");
            }
        }
    }

    fn load_payroll_schedule_year(&mut self, payroll_year: &str) {
        match self.application.get_payroll_schedule(payroll_year) {
            Ok(schedules) => {
                self.payroll_schedules = schedules;
                self.selected_payroll_schedule_year = Some(payroll_year.to_string());
                self.status_message = format!(
                    "Loaded {} payroll schedule entries for {}.",
                    self.payroll_schedules.len(),
                    payroll_year
                );
            }
            Err(error) => {
                self.payroll_schedules.clear();
                self.status_message =
                    format!("Failed loading payroll schedule for {payroll_year}: {error}");
            }
        }
    }

    fn draw_dashboard(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Dashboard");

            self.draw_operational_payroll_period_selector(ui);

            ui.separator();

            if ui.button("Import CSV").clicked() {
                match self.application.import_csv() {
                    Ok(summary) => {
                        self.status_message = "Import completed successfully.".to_string();
                        self.last_import = Some(summary);
                    }

                    Err(error) => {
                        self.status_message = format!("Import failed: {}", error);
                        self.last_import = None;
                    }
                }
            }

            if ui.button("View Imported CSV").clicked() {
                match self.application.get_timesheets() {
                    Ok(entries) => {
                        self.timesheets = entries;

                        self.status_message =
                            format!("Loaded {} timesheets.", self.timesheets.len());
                    }

                    Err(error) => {
                        self.status_message = format!("Failed loading timesheets: {}", error);
                    }
                }
            }

            if ui.button("Generate Payroll Timesheets").clicked() {
                match self.generate_payroll_timesheets() {
                    Ok(count) => {
                        self.status_message =
                            format!("Payroll timesheets generated: {} PDF(s).", count);
                    }

                    Err(error) => {
                        self.status_message =
                            format!("Payroll timesheet generation failed: {}", error);
                    }
                }
            }

            if ui.button("Email Timesheets").clicked() {
                self.begin_email_batch(PayrollEmailKind::Timesheet);
            }

            if ui.button("Import Payroll Return").clicked() {
                self.begin_payroll_return_import();
            }

            if ui.button("Email Payslips").clicked() {
                self.begin_email_batch(PayrollEmailKind::Payslip);
            }

            if ui.button("Import Payroll Prep Sheet").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Payroll Prep Sheet", &["pdf", "docx"])
                    .pick_file()
                {
                    match self.application.import_payroll_prep_sheet(&path) {
                        Ok(count) => {
                            self.status_message =
                                format!("Payroll Prep Sheet imported: {} schedule entries.", count);
                            if let Err(error) = self.refresh_operational_payroll_schedules() {
                                self.operational_payroll_period_error = Some(format!(
                                    "Could not refresh operational payroll periods: {error}"
                                ));
                            }
                        }

                        Err(error) => {
                            self.status_message =
                                format!("Payroll Prep Sheet import failed: {}", error);
                        }
                    }
                }
            }

            if ui.button("View Payroll Schedule").clicked() {
                self.begin_view_payroll_schedule();
            }

            self.draw_additional_note_prompt(ui);

            ui.separator();

            ui.heading("Import Summary");

            match &self.last_import {
                Some(summary) => {
                    ui.label(format!("Files discovered: {}", summary.files_discovered));
                    ui.label(format!("Files processed: {}", summary.files_processed));
                    ui.label(format!("Rows imported: {}", summary.rows_imported));
                    ui.label(format!("Rows skipped: {}", summary.rows_skipped));
                }

                None => {
                    ui.label("No import performed yet.");
                }
            }

            ui.separator();

            ui.label(format!("Status: {}", self.status_message));

            if let Some(selected_year) = draw_payroll_schedule_year_selector(
                ui,
                &self.payroll_schedule_years,
                self.selected_payroll_schedule_year.as_deref(),
            ) {
                self.load_payroll_schedule_year(&selected_year);
            }
            draw_payroll_schedule(ui, &self.payroll_schedules);
            draw_timesheets(ui, &self.timesheets);
            self.draw_timesheet_email_status(ui);
            self.draw_payroll_return_schedule_dialog(ui.ctx());
        });
    }

    fn draw_email_preview_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Email Preview");
        ui.label("Previews never send an email or update the sent status.");

        let assistants = match self.application.personal_assistant_repository.get_all() {
            Ok(assistants) => assistants
                .into_iter()
                .filter(|assistant| {
                    assistant
                        .employment_status
                        .as_deref()
                        .map(|status| status.trim().eq_ignore_ascii_case("active"))
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>(),
            Err(error) => {
                ui.label(format!("Unable to load Personal Assistants: {}", error));
                return;
            }
        };

        if assistants.is_empty() {
            ui.label("No active Personal Assistants are available for preview.");
            return;
        }

        if !assistants
            .iter()
            .any(|assistant| Some(assistant.id) == self.preview_personal_assistant_id)
        {
            self.preview_personal_assistant_id = Some(assistants[0].id);
        }

        let selected_name = assistants
            .iter()
            .find(|assistant| Some(assistant.id) == self.preview_personal_assistant_id)
            .map(|assistant| format!("{} {}", assistant.first_name, assistant.surname))
            .unwrap_or_default();

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("email_preview_personal_assistant")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for assistant in &assistants {
                        ui.selectable_value(
                            &mut self.preview_personal_assistant_id,
                            Some(assistant.id),
                            format!("{} {}", assistant.first_name, assistant.surname),
                        );
                    }
                });

            if ui.button("Preview Timesheet Email").clicked() {
                self.load_email_preview(PayrollEmailKind::Timesheet);
            }

            if ui.button("Preview Payslip Email").clicked() {
                self.load_email_preview(PayrollEmailKind::Payslip);
            }
        });

        if let Some(preview) = &self.email_preview {
            ui.separator();
            ui.label(format!("From: {}", preview.from));
            ui.label(format!("To: {}", preview.to));
            ui.label(format!("CC: {}", preview.cc.as_deref().unwrap_or("None")));
            ui.label(format!("BCC: {}", preview.bcc.as_deref().unwrap_or("None")));
            ui.label(format!("Subject: {}", preview.subject));
            ui.label(format!("Attachment: {}", preview.attachment_path));
            ui.label("Body:");
            ui.add_sized(
                [600.0, 140.0],
                egui::TextEdit::multiline(&mut preview.body.clone()).interactive(false),
            );
        }
    }

    fn load_email_preview(&mut self, kind: PayrollEmailKind) {
        let result = (|| -> Result<PayrollEmailPreview, Box<dyn std::error::Error>> {
            let personal_assistant_id = self
                .preview_personal_assistant_id
                .ok_or("Select a Personal Assistant to preview an email.")?;

            let assistant = self
                .application
                .personal_assistant_repository
                .get_all()?
                .into_iter()
                .find(|assistant| assistant.id == personal_assistant_id)
                .ok_or("Selected Personal Assistant was not found.")?;

            let employer = self
                .application
                .employer_repository
                .get_all()?
                .into_iter()
                .next()
                .ok_or("No employer has been configured.")?;

            let employer_email = employer
                .email
                .as_deref()
                .map(str::trim)
                .filter(|email| !email.is_empty())
                .ok_or("Employer has no email address.")?;

            let payroll_department_email = self
                .application
                .payroll_provider_repository
                .get()?
                .and_then(|provider| provider.payroll_department_email)
                .map(|email| email.trim().to_string())
                .filter(|email| !email.is_empty())
                .ok_or("Payroll Department has no email address.")?;

            let current_schedule = self.selected_operational_payroll_schedule()?;

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);
            let (attachment_path, body) = match kind {
                PayrollEmailKind::Timesheet => (
                    crate::payroll_file_naming::timesheet_path(
                        &crate::paths::expand_path(
                            &self.application.context.config.folders.pdf_output,
                        ),
                        &personal_assistant_name,
                        &current_schedule,
                    )?,
                    &self.application.context.config.payroll.timesheet_email_body,
                ),
                PayrollEmailKind::Payslip => (
                    crate::payroll_file_naming::payslip_path(
                        &crate::paths::expand_path(
                            &self.application.context.config.folders.payslip_folder,
                        ),
                        &personal_assistant_name,
                        &current_schedule,
                    )?,
                    &self.application.context.config.payroll.payslip_email_body,
                ),
            };

            self.application.preview_payroll_email(
                &payroll_department_email,
                employer_email,
                assistant.email.as_deref(),
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                &current_schedule,
                &attachment_path,
                body,
                self.additional_notes_by_personal_assistant
                    .get(&assistant.id)
                    .map(String::as_str),
                employer.email_signature.as_deref(),
            )
        })();

        match result {
            Ok(preview) => {
                self.email_preview = Some(preview);
                self.status_message = "Email preview generated. No email was sent.".to_string();
            }
            Err(error) => {
                self.email_preview = None;
                self.status_message = format!("Email preview failed: {}", error);
            }
        }
    }

    fn email_payslips(
        &self,
        schedule: &PayrollSchedule,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let employers = self.application.employer_repository.get_all()?;

        let employer = employers
            .into_iter()
            .next()
            .ok_or("No employer has been configured.")?;

        let employer_email = employer
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .ok_or("Employer has no email address.")?;

        let payroll_provider = self
            .application
            .payroll_provider_repository
            .get()?
            .ok_or("No Payroll Provider has been configured.")?;

        let payroll_department_email = payroll_provider
            .payroll_department_email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .ok_or("Payroll Department has no email address.")?;

        let payroll_year = schedule.payroll_year.clone();

        let assistants = self.application.personal_assistant_repository.get_all()?;

        let payslip_folder =
            crate::paths::expand_path(&self.application.context.config.folders.payslip_folder);

        let mut sent = 0usize;

        for assistant in &assistants {
            let is_active = match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            };

            if !is_active {
                continue;
            }

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);

            let existing_status = self
                .application
                .payroll_timesheet_email_repository
                .get_for_pa_and_cycle(
                    assistant.id,
                    &payroll_year,
                    schedule.cycle_number,
                    "payslip",
                )?;

            if existing_status
                .as_ref()
                .and_then(|status| status.sent_at.as_ref())
                .is_some()
            {
                continue;
            }

            let personal_assistant_email = assistant.email.as_deref();

            let payslip_path = crate::payroll_file_naming::payslip_path(
                &payslip_folder,
                &personal_assistant_name,
                schedule,
            )?;

            if !payslip_path.exists() {
                return Err(format!(
                    "Payslip PDF not found for {}: {}",
                    personal_assistant_name,
                    payslip_path.display()
                )
                .into());
            }

            self.application.send_payroll_email(
                payroll_department_email,
                employer_email,
                personal_assistant_email,
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                schedule,
                &payslip_path,
                &self.application.context.config.payroll.payslip_email_body,
                self.additional_notes_by_personal_assistant
                    .get(&assistant.id)
                    .map(String::as_str),
                employer.email_signature.as_deref(),
            )?;

            let sent_at = chrono::Local::now().to_rfc3339();

            self.application
                .payroll_timesheet_email_repository
                .mark_sent(
                    assistant.id,
                    &payroll_year,
                    schedule.cycle_number,
                    "payslip",
                    &sent_at,
                )?;

            sent += 1;
        }

        let all_active_sent = assistants
            .iter()
            .filter(|assistant| match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            })
            .all(|assistant| {
                self.application
                    .payroll_timesheet_email_repository
                    .get_for_pa_and_cycle(
                        assistant.id,
                        &payroll_year,
                        schedule.cycle_number,
                        "payslip",
                    )
                    .ok()
                    .flatten()
                    .and_then(|status| status.sent_at)
                    .is_some()
            });

        if all_active_sent {
            self.application
                .payroll_schedule_repository
                .mark_payslips_sent(schedule.id)?;
        }

        Ok(sent)
    }

    fn email_timesheets(
        &self,
        schedule: &PayrollSchedule,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let employers = self.application.employer_repository.get_all()?;

        let employer = employers
            .into_iter()
            .next()
            .ok_or("No employer has been configured.")?;

        let employer_email = employer
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .ok_or("Employer has no email address.")?;

        let payroll_provider = self
            .application
            .payroll_provider_repository
            .get()?
            .ok_or("No Payroll Provider has been configured.")?;

        let payroll_department_email = payroll_provider
            .payroll_department_email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .ok_or("Payroll Department has no email address.")?;

        let payroll_year = schedule.payroll_year.clone();

        let assistants = self.application.personal_assistant_repository.get_all()?;

        let pdf_output_folder =
            crate::paths::expand_path(&self.application.context.config.folders.pdf_output);

        let mut sent = 0usize;

        for assistant in &assistants {
            let is_active = match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            };

            if !is_active {
                continue;
            }

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);

            let existing_status = self
                .application
                .payroll_timesheet_email_repository
                .get_for_pa_and_cycle(
                    assistant.id,
                    &payroll_year,
                    schedule.cycle_number,
                    "timesheet",
                )?;

            if existing_status
                .as_ref()
                .and_then(|status| status.sent_at.as_ref())
                .is_some()
            {
                continue;
            }

            let personal_assistant_email = assistant.email.as_deref();

            let timesheet_path = PdfGenerator::timesheet_output_path(
                &pdf_output_folder,
                &personal_assistant_name,
                schedule,
            )?;

            if !timesheet_path.exists() {
                return Err(format!(
                    "Timesheet PDF not found for {}: {}",
                    personal_assistant_name,
                    timesheet_path.display()
                )
                .into());
            }

            let payroll_timesheet = self
                .application
                .payroll_timesheet_repository
                .get_for_cycle_and_pa(&payroll_year, schedule.cycle_number, assistant.id)?
                .ok_or_else(|| {
                    format!(
                        "No Payroll Timesheet Preparation record exists for {}.",
                        personal_assistant_name
                    )
                })?;
            let attempted_at = chrono::Local::now().to_rfc3339();
            crate::payroll_snapshot_service::send_production_candidate(
                &self.application.payroll_worked_item_repository,
                payroll_timesheet.id,
                assistant.id,
                &payroll_year,
                schedule.cycle_number,
                &timesheet_path,
                &attempted_at,
                || {
                    self.application.send_payroll_email(
                        payroll_department_email,
                        employer_email,
                        personal_assistant_email,
                        &personal_assistant_name,
                        assistant.date_of_birth.as_deref(),
                        assistant.national_insurance_number.as_deref(),
                        schedule,
                        &timesheet_path,
                        &self.application.context.config.payroll.timesheet_email_body,
                        self.additional_notes_by_personal_assistant
                            .get(&assistant.id)
                            .map(String::as_str),
                        employer.email_signature.as_deref(),
                    )
                },
            )?;

            sent += 1;
        }

        Ok(sent)
    }

    fn draw_timesheet_email_status(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Timesheet Email Status");

        let current_schedule = match self.selected_operational_payroll_schedule() {
            Ok(schedule) => schedule,
            Err(error) => {
                ui.label(format!(
                    "Unable to load the selected payroll period: {}",
                    error
                ));
                return;
            }
        };
        let payroll_year = current_schedule.payroll_year.clone();

        let assistants = match self.application.personal_assistant_repository.get_all() {
            Ok(assistants) => assistants,
            Err(error) => {
                ui.label(format!("Unable to load Personal Assistants: {}", error));
                return;
            }
        };

        egui::Grid::new("timesheet_email_status")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Personal Assistant");
                ui.label("Status");
                ui.label("Sent");
                ui.end_row();

                for assistant in assistants {
                    let is_active = match &assistant.employment_status {
                        Some(status) => status.trim().eq_ignore_ascii_case("active"),
                        None => true,
                    };

                    if !is_active {
                        continue;
                    }

                    let name = format!("{} {}", assistant.first_name, assistant.surname);

                    let status = self
                        .application
                        .payroll_timesheet_email_repository
                        .get_for_pa_and_cycle(
                            assistant.id,
                            &payroll_year,
                            current_schedule.cycle_number,
                            "timesheet",
                        );

                    match status {
                        Ok(Some(status)) if status.sent_at.is_some() => {
                            let sent_at = status.sent_at.unwrap();

                            let display_time = chrono::DateTime::parse_from_rfc3339(&sent_at)
                                .map(|date_time| {
                                    date_time
                                        .with_timezone(&chrono::Local)
                                        .format("%d %b %Y %H:%M")
                                        .to_string()
                                })
                                .unwrap_or(sent_at);

                            ui.label(name);
                            ui.label("Sent");
                            ui.label(display_time);
                        }

                        Ok(_) => {
                            ui.label(name);
                            ui.label("Not sent");
                            ui.label("");
                        }

                        Err(error) => {
                            ui.label(name);
                            ui.label("Error");
                            ui.label(format!("{}", error));
                        }
                    }

                    ui.end_row();
                }
            });
    }

    fn generate_payroll_timesheets(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let employers = self.application.employer_repository.get_all()?;

        let employer = employers
            .into_iter()
            .next()
            .ok_or("No employer has been configured.")?;

        let current_schedule = self.selected_operational_payroll_schedule()?;
        let payroll_year = current_schedule.payroll_year.clone();
        let previous_schedule = self
            .application
            .payroll_schedule_repository
            .resolve_previous(&current_schedule)?;

        let first_week = parse_date_checked(&current_schedule.first_week_commencing)
            .ok_or("Invalid first week commencing date in payroll schedule.")?;

        let week_dates = [
            first_week,
            first_week + chrono::Duration::days(7),
            first_week + chrono::Duration::days(14),
            first_week + chrono::Duration::days(21),
        ];

        let week_date_strings = [
            format_date(week_dates[0]),
            format_date(week_dates[1]),
            format_date(week_dates[2]),
            format_date(week_dates[3]),
        ];

        let assistants = self.application.personal_assistant_repository.get_all()?;
        let existing_payroll_timesheets = self
            .application
            .payroll_timesheet_repository
            .get_all_for_cycle(&payroll_year, current_schedule.cycle_number)?;
        let existing_personal_assistant_ids = existing_payroll_timesheets
            .iter()
            .map(|record| record.personal_assistant_id)
            .collect::<HashSet<_>>();
        let all_timesheets = self.application.get_timesheets()?;

        let output_dir =
            crate::paths::expand_path(&self.application.context.config.folders.pdf_output);

        let mut generated = 0usize;

        for assistant in &assistants {
            if !personal_assistant_is_eligible_for_generation(
                assistant.employment_status.as_deref(),
                existing_personal_assistant_ids.contains(&assistant.id),
            ) {
                continue;
            }

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);

            let payroll_timesheet = self
                .application
                .payroll_timesheet_repository
                .get_for_cycle_and_pa(&payroll_year, current_schedule.cycle_number, assistant.id)?
                .ok_or_else(|| {
                    format!(
                        "No Payroll Timesheet Preparation record exists for {}.",
                        personal_assistant_name
                    )
                })?;

            let payroll_weeks = self
                .application
                .payroll_timesheet_repository
                .get_weeks(payroll_timesheet.id)?;

            if payroll_weeks.len() != 4 {
                return Err(format!(
                    "Expected 4 payroll weeks for {}, found {}.",
                    personal_assistant_name,
                    payroll_weeks.len()
                )
                .into());
            }

            let previous_record = if let Some(previous_schedule) = &previous_schedule {
                self.application
                    .payroll_timesheet_repository
                    .get_for_cycle_and_pa(
                        &previous_schedule.payroll_year,
                        previous_schedule.cycle_number,
                        assistant.id,
                    )?
            } else {
                None
            };
            let previous_context = if let Some(previous_record) = &previous_record {
                let previous_weeks = self
                    .application
                    .payroll_timesheet_repository
                    .get_weeks(previous_record.id)?;
                previous_weeks.get(2).and_then(|week| {
                    parse_date_checked(&week.week_commencing).map(|week_three_start| {
                        crate::pay_rate_allocation::PreviousCycleContext {
                            payroll_timesheet_id: previous_record.id,
                            week_three_start,
                            legacy_adjustment_minutes: payroll_timesheet
                                .previous_cycle_hours
                                .map(|hours| (hours * 60.0).round() as i64)
                                .unwrap_or(0),
                        }
                    })
                })
            } else {
                None
            };
            let reconciled = crate::pay_rate_allocation::reconcile_payroll_hours(
                &self.application.pay_rate_repository,
                &self.application.payroll_worked_item_repository,
                &all_timesheets,
                assistant.id,
                payroll_timesheet.id,
                &week_dates,
                previous_context.as_ref(),
            )
            .map_err(|error| {
                format!("Cannot generate payroll timesheet for {personal_assistant_name}: {error}")
            })?;

            let contracted_for_week = |week_date| -> rusqlite::Result<String> {
                Ok(self
                    .application
                    .contracted_hours_repository
                    .get_for_personal_assistant_as_of(assistant.id, week_date)?
                    .map(|hours| hours.contracted_hours)
                    .unwrap_or_else(|| "Unavailable".to_string()))
            };
            let contracted_hours_by_week = [
                contracted_for_week(week_dates[0])?,
                contracted_for_week(week_dates[1])?,
                contracted_for_week(week_dates[2])?,
                contracted_for_week(week_dates[3])?,
            ];
            let contracted_weekly_hours = crate::pdf_generator::contracted_hours_summary(
                &contracted_hours_by_week,
                &week_date_strings,
            );

            let hours_worked: [String; 4] = std::array::from_fn(|index| {
                crate::pay_rate_allocation::format_total_minutes(
                    reconciled.week_totals_minutes[index],
                )
            });

            let annual_leave_hours = [
                format_pdf_hours(payroll_weeks[0].annual_leave_hours),
                format_pdf_hours(payroll_weeks[1].annual_leave_hours),
                format_pdf_hours(payroll_weeks[2].annual_leave_hours),
                format_pdf_hours(payroll_weeks[3].annual_leave_hours),
            ];

            let sick_leave_hours = [
                format_pdf_hours(payroll_weeks[0].sick_leave_hours),
                format_pdf_hours(payroll_weeks[1].sick_leave_hours),
                format_pdf_hours(payroll_weeks[2].sick_leave_hours),
                format_pdf_hours(payroll_weeks[3].sick_leave_hours),
            ];

            let public_holiday_hours = [
                format_pdf_hours(payroll_weeks[0].public_holiday_hours),
                format_pdf_hours(payroll_weeks[1].public_holiday_hours),
                format_pdf_hours(payroll_weeks[2].public_holiday_hours),
                format_pdf_hours(payroll_weeks[3].public_holiday_hours),
            ];

            let public_holidays = self
                .application
                .payroll_timesheet_repository
                .get_public_holidays(payroll_timesheet.id)?;

            let public_holiday_dates = [
                public_holidays
                    .iter()
                    .filter(|holiday| holiday.week_number == 1 && holiday.hours > 0.0)
                    .map(|holiday| {
                        format!(
                            "{} ({})",
                            format_pdf_hours(holiday.hours),
                            holiday.holiday_date
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                public_holidays
                    .iter()
                    .filter(|holiday| holiday.week_number == 2 && holiday.hours > 0.0)
                    .map(|holiday| {
                        format!(
                            "{} ({})",
                            format_pdf_hours(holiday.hours),
                            holiday.holiday_date
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                public_holidays
                    .iter()
                    .filter(|holiday| holiday.week_number == 3 && holiday.hours > 0.0)
                    .map(|holiday| {
                        format!(
                            "{} ({})",
                            format_pdf_hours(holiday.hours),
                            holiday.holiday_date
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                public_holidays
                    .iter()
                    .filter(|holiday| holiday.week_number == 4 && holiday.hours > 0.0)
                    .map(|holiday| {
                        format!(
                            "{} ({})",
                            format_pdf_hours(holiday.hours),
                            holiday.holiday_date
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ];

            let travel_miles = [
                format_pdf_hours(payroll_weeks[0].travel_miles),
                format_pdf_hours(payroll_weeks[1].travel_miles),
                format_pdf_hours(payroll_weeks[2].travel_miles),
                format_pdf_hours(payroll_weeks[3].travel_miles),
            ];

            let previous_cycle_hours = (reconciled.previous_cycle_minutes > 0).then(|| {
                crate::pay_rate_allocation::format_total_minutes(reconciled.previous_cycle_minutes)
            });

            let employer_signature_path = employer
                .employer_signature
                .as_deref()
                .map(|path| crate::paths::expand_path(&std::path::PathBuf::from(path)))
                .filter(|path| path.exists());

            let pa_signature_path = assistant
                .signature
                .as_deref()
                .map(|path| crate::paths::expand_path(&std::path::PathBuf::from(path)))
                .filter(|path| path.exists());

            let data = TimesheetPdfData {
                schedule: &current_schedule,
                employer_name: &employer.name,

                personal_assistant_name: &personal_assistant_name,

                national_insurance_number: assistant
                    .national_insurance_number
                    .as_deref()
                    .unwrap_or(""),

                contracted_weekly_hours: &contracted_weekly_hours,

                week_commencing_dates: [
                    &week_date_strings[0],
                    &week_date_strings[1],
                    &week_date_strings[2],
                    &week_date_strings[3],
                ],

                hours_worked: [
                    &hours_worked[0],
                    &hours_worked[1],
                    &hours_worked[2],
                    &hours_worked[3],
                ],

                annual_leave_hours: [
                    &annual_leave_hours[0],
                    &annual_leave_hours[1],
                    &annual_leave_hours[2],
                    &annual_leave_hours[3],
                ],

                sick_leave_hours: [
                    &sick_leave_hours[0],
                    &sick_leave_hours[1],
                    &sick_leave_hours[2],
                    &sick_leave_hours[3],
                ],

                public_holiday_hours: [
                    &public_holiday_hours[0],
                    &public_holiday_hours[1],
                    &public_holiday_hours[2],
                    &public_holiday_hours[3],
                ],

                public_holiday_dates: [
                    &public_holiday_dates[0],
                    &public_holiday_dates[1],
                    &public_holiday_dates[2],
                    &public_holiday_dates[3],
                ],

                travel_miles: [
                    &travel_miles[0],
                    &travel_miles[1],
                    &travel_miles[2],
                    &travel_miles[3],
                ],

                previous_cycle_hours: previous_cycle_hours.as_deref(),

                employer_signature_path: employer_signature_path.as_deref(),

                pa_signature_path: pa_signature_path.as_deref(),
            };

            let captured_at = chrono::Local::now().to_rfc3339();
            let final_pdf_path = PdfGenerator::output_path(&output_dir, &data)?;
            let week_ids = [
                payroll_weeks[0].id,
                payroll_weeks[1].id,
                payroll_weeks[2].id,
                payroll_weeks[3].id,
            ];
            crate::payroll_snapshot_service::publish_candidate(
                &self.application.payroll_worked_item_repository,
                crate::payroll_snapshot_service::CandidatePublication {
                    payroll_timesheet_id: payroll_timesheet.id,
                    items: &reconciled.snapshot_items,
                    final_pdf_path: &final_pdf_path,
                    generated_at: &captured_at,
                    previous_cycle_minutes: reconciled.previous_cycle_minutes,
                    week_ids: &week_ids,
                    week_totals_minutes: &reconciled.week_totals_minutes,
                },
                |temporary_path| {
                    PdfGenerator::generate_to_path(
                        temporary_path,
                        &data,
                        &self.application.context.config.pdf,
                    )
                },
            )?;

            generated += 1;
        }

        Ok(generated)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperationalPeriodTiming {
    Historical,
    Current,
    Future,
}

fn operational_period_key(schedule: &PayrollSchedule) -> OperationalPayrollPeriodKey {
    OperationalPayrollPeriodKey {
        payroll_year: schedule.payroll_year.clone(),
        cycle_number: schedule.cycle_number,
    }
}

fn personal_assistant_is_eligible_for_generation(
    employment_status: Option<&str>,
    has_selected_period_record: bool,
) -> bool {
    employment_status
        .map(|status| status.trim().eq_ignore_ascii_case("active"))
        .unwrap_or(true)
        || has_selected_period_record
}

fn capture_operational_payroll_period(
    schedule: &PayrollSchedule,
) -> CapturedOperationalPayrollPeriod {
    CapturedOperationalPayrollPeriod {
        key: operational_period_key(schedule),
        first_week_commencing: schedule.first_week_commencing.clone(),
        pay_date: schedule.pay_date.clone(),
        display_label: payroll_schedule_label(schedule),
    }
}

fn captured_operational_period_timing(
    captured: &CapturedOperationalPayrollPeriod,
    today: chrono::NaiveDate,
) -> Option<OperationalPeriodTiming> {
    let first_week = parse_date_checked(&captured.first_week_commencing)?;
    if today < first_week {
        Some(OperationalPeriodTiming::Future)
    } else if today > first_week + chrono::Duration::days(27) {
        Some(OperationalPeriodTiming::Historical)
    } else {
        Some(OperationalPeriodTiming::Current)
    }
}

fn validate_captured_email_batch_period(
    captured: &CapturedOperationalPayrollPeriod,
    captured_revision: u64,
    operational_state: &OperationalPayrollPeriodState,
    stored_schedule: Option<&PayrollSchedule>,
) -> Result<(), Box<dyn std::error::Error>> {
    if operational_state.revision != captured_revision
        || operational_state.selected.as_ref() != Some(&captured.key)
    {
        return Err("The operational payroll period selection changed after this email batch began. Cancel this batch and start it again for the intended payroll period.".into());
    }

    let stored_schedule = stored_schedule.ok_or_else(|| {
        "The payroll period captured for this email batch no longer exists. Cancel this batch and select an available payroll period."
    })?;
    if stored_schedule.first_week_commencing != captured.first_week_commencing
        || stored_schedule.pay_date != captured.pay_date
    {
        return Err("The payroll schedule dates changed after this email batch began. Nothing was sent; cancel this batch and start it again using the updated payroll period.".into());
    }

    Ok(())
}

fn initial_operational_payroll_period(
    application: &Application,
) -> (
    Vec<PayrollSchedule>,
    OperationalPayrollPeriodState,
    Option<String>,
) {
    let schedules = match application.payroll_schedule_repository.get_all() {
        Ok(schedules) => ordered_payroll_schedules(schedules, None),
        Err(error) => {
            return (
                Vec::new(),
                OperationalPayrollPeriodState::default(),
                Some(format!(
                    "Could not load operational payroll periods: {error}"
                )),
            );
        }
    };
    if schedules.is_empty() {
        return (
            schedules,
            OperationalPayrollPeriodState::default(),
            Some("No imported payroll schedules are available.".to_string()),
        );
    }

    let today = chrono::Local::now().date_naive();
    let current = match application
        .payroll_schedule_repository
        .resolve_for_date(today)
    {
        Ok(current) => Some(current),
        Err(
            crate::payroll_schedule_repository::PayrollScheduleResolutionError::NoCurrentCycle(_),
        ) => None,
        Err(error) => {
            return (
                schedules,
                OperationalPayrollPeriodState::default(),
                Some(format!(
                    "Could not determine the default operational payroll period: {error}"
                )),
            );
        }
    };
    let state = default_operational_payroll_period(&schedules, current.as_ref());

    (schedules, state, None)
}

fn default_operational_payroll_period(
    schedules: &[PayrollSchedule],
    current: Option<&PayrollSchedule>,
) -> OperationalPayrollPeriodState {
    OperationalPayrollPeriodState {
        selected: current
            .or_else(|| schedules.first())
            .map(operational_period_key),
        explicitly_selected: false,
        revision: 0,
    }
}

fn operational_period_timing(
    schedule: &PayrollSchedule,
    today: chrono::NaiveDate,
) -> Option<OperationalPeriodTiming> {
    let first_week = parse_date_checked(&schedule.first_week_commencing)?;
    if today < first_week {
        Some(OperationalPeriodTiming::Future)
    } else if today > first_week + chrono::Duration::days(27) {
        Some(OperationalPeriodTiming::Historical)
    } else {
        Some(OperationalPeriodTiming::Current)
    }
}

fn ordered_payroll_schedules(
    mut schedules: Vec<PayrollSchedule>,
    current_schedule_id: Option<i64>,
) -> Vec<PayrollSchedule> {
    schedules.sort_by(|left, right| {
        let left_date = parse_date_checked(&left.first_week_commencing);
        let right_date = parse_date_checked(&right.first_week_commencing);
        match (left_date, right_date) {
            (Some(left_date), Some(right_date)) => right_date.cmp(&left_date),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| right.payroll_year.cmp(&left.payroll_year))
        .then_with(|| right.cycle_number.cmp(&left.cycle_number))
        .then_with(|| right.id.cmp(&left.id))
    });

    if let Some(current_schedule_id) = current_schedule_id {
        if let Some(index) = schedules
            .iter()
            .position(|schedule| schedule.id == current_schedule_id)
        {
            let current = schedules.remove(index);
            schedules.insert(0, current);
        }
    }

    schedules
}

fn default_payroll_return_schedule_id(
    schedules: &[PayrollSchedule],
    current_schedule_id: Option<i64>,
) -> Option<i64> {
    current_schedule_id
        .filter(|id| schedules.iter().any(|schedule| schedule.id == *id))
        .or_else(|| schedules.first().map(|schedule| schedule.id))
}

fn selected_payroll_return_schedule(
    schedules: &[PayrollSchedule],
    selected_schedule_id: i64,
) -> Option<&PayrollSchedule> {
    schedules
        .iter()
        .find(|schedule| schedule.id == selected_schedule_id)
}

fn payroll_schedule_label(schedule: &PayrollSchedule) -> String {
    let paye_week = crate::payroll_file_naming::paye_week(schedule)
        .map(|week| week.to_string())
        .unwrap_or_else(|_| "unavailable".to_string());

    format!(
        "{} · Week {} · {} · Pay {}",
        schedule.payroll_year,
        paye_week,
        payroll_schedule_period(schedule),
        schedule.pay_date
    )
}

fn payroll_schedule_period(schedule: &PayrollSchedule) -> String {
    parse_date_checked(&schedule.first_week_commencing)
        .map(|start| {
            format!(
                "{} to {}",
                start.format("%d/%m/%Y"),
                (start + chrono::Duration::days(27)).format("%d/%m/%Y")
            )
        })
        .unwrap_or_else(|| format!("from {}", schedule.first_week_commencing))
}

fn payroll_schedule_details(
    schedule: &PayrollSchedule,
) -> Result<[String; 4], Box<dyn std::error::Error>> {
    Ok([
        format!("Payroll year: {}", schedule.payroll_year),
        format!(
            "Payroll week: {}",
            crate::payroll_file_naming::paye_week(schedule)?
        ),
        format!("Timesheet period: {}", payroll_schedule_period(schedule)),
        format!("Scheduled pay date: {}", schedule.pay_date),
    ])
}

fn format_pdf_hours(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    if value.fract() == 0.0 {
        return format!("{:.0}", value);
    }

    let text = format!("{:.2}", value);

    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn parse_date_checked(value: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(value.trim(), "%d/%m/%Y")
        .or_else(|_| chrono::NaiveDate::parse_from_str(value.trim(), "%d-%m-%Y"))
        .or_else(|_| chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d"))
        .ok()
}

fn format_date(date: chrono::NaiveDate) -> String {
    date.format("%d/%m/%Y").to_string()
}

fn draw_timesheets(ui: &mut egui::Ui, timesheets: &[TimesheetEntry]) {
    ui.separator();

    ui.heading("Timesheets");

    if timesheets.is_empty() {
        ui.label("No timesheets loaded.");
        return;
    }

    egui::Grid::new("timesheet_grid")
        .striped(true)
        .show(ui, |ui| {
            ui.label("PA Name");
            ui.label("Start");
            ui.label("End");
            ui.label("Worked");
            ui.label("Rate");
            ui.label("Amount");
            ui.end_row();

            for entry in timesheets {
                ui.label(&entry.pa_name);
                ui.label(&entry.start_time);
                ui.label(&entry.end_time);
                ui.label(format_worked_time(entry.worked_minutes));
                ui.label(format!("£{:.2}", entry.hourly_rate));
                ui.label(format!("£{:.2}", entry.amount));
                ui.end_row();
            }
        });
}

fn format_worked_time(minutes: i64) -> String {
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    format!("{}h {}m", hours, remaining_minutes)
}

fn payroll_schedule_years_newest_first(schedules: &[PayrollSchedule]) -> Vec<String> {
    let ordered = ordered_payroll_schedules(schedules.to_vec(), None);
    let mut seen = HashSet::new();
    ordered
        .into_iter()
        .filter_map(|schedule| {
            seen.insert(schedule.payroll_year.clone())
                .then_some(schedule.payroll_year)
        })
        .collect()
}

fn default_payroll_schedule_year(
    payroll_years: &[String],
    current_payroll_year: Option<&str>,
) -> Option<String> {
    current_payroll_year
        .filter(|current| payroll_years.iter().any(|year| year == current))
        .map(str::to_string)
        .or_else(|| payroll_years.first().cloned())
}

fn draw_payroll_schedule_year_selector(
    ui: &mut egui::Ui,
    payroll_years: &[String],
    selected_payroll_year: Option<&str>,
) -> Option<String> {
    let mut selected = selected_payroll_year
        .map(str::to_string)
        .or_else(|| payroll_years.first().cloned())?;
    let original = selected.clone();

    ui.horizontal(|ui| {
        ui.label("Payroll year:");
        egui::ComboBox::from_id_salt("view_payroll_schedule_year")
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                for payroll_year in payroll_years {
                    ui.selectable_value(&mut selected, payroll_year.clone(), payroll_year);
                }
            });
    });

    (selected != original).then_some(selected)
}

fn draw_payroll_schedule(ui: &mut egui::Ui, schedules: &[PayrollSchedule]) {
    ui.separator();

    ui.heading("Payroll Schedule");

    if schedules.is_empty() {
        ui.label("No payroll schedule loaded.");
        return;
    }

    egui::Grid::new("payroll_schedule_grid")
        .striped(true)
        .show(ui, |ui| {
            ui.label("Cycle");
            ui.label("First Week Commencing");
            ui.label("Latest Posting Date");
            ui.label("Pay Date");
            ui.label("Payslips");
            ui.end_row();

            for schedule in schedules {
                ui.label(schedule.cycle_number.to_string());
                ui.label(&schedule.first_week_commencing);
                ui.label(&schedule.latest_posting_date);
                ui.label(&schedule.pay_date);

                if schedule.payslips_sent {
                    ui.label("Sent");
                } else {
                    ui.label("Not sent");
                }

                ui.end_row();
            }
        });
}

#[cfg(test)]
mod payroll_return_schedule_selection_tests {
    use super::*;

    fn schedule(
        id: i64,
        payroll_year: &str,
        cycle_number: i64,
        first_week: &str,
        pay_date: &str,
    ) -> PayrollSchedule {
        PayrollSchedule {
            id,
            payroll_year: payroll_year.to_string(),
            cycle_number,
            first_week_commencing: first_week.to_string(),
            latest_posting_date: String::new(),
            pay_date: pay_date.to_string(),
            created_at: String::new(),
            payslips_sent: false,
        }
    }

    #[test]
    fn current_schedule_is_promoted_and_selected_by_default() {
        let schedules = ordered_payroll_schedules(
            vec![
                schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
                schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
            ],
            Some(1),
        );

        assert_eq!(schedules[0].id, 1);
        assert_eq!(
            default_payroll_return_schedule_id(&schedules, Some(1)),
            Some(1)
        );
    }

    #[test]
    fn schedules_are_newest_first_across_payroll_years_without_a_current_cycle() {
        let schedules = ordered_payroll_schedules(
            vec![
                schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
                schedule(3, "2027/28", 2, "19/04/2027", "14/05/2027"),
                schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
            ],
            None,
        );

        assert_eq!(
            schedules
                .iter()
                .map(|schedule| schedule.id)
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert_eq!(
            default_payroll_return_schedule_id(&schedules, None),
            Some(3)
        );
    }

    #[test]
    fn a_historical_schedule_can_be_selected_explicitly() {
        let schedules = vec![
            schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
            schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
        ];

        let selected = selected_payroll_return_schedule(&schedules, 1).unwrap();

        assert_eq!(selected.payroll_year, "2026/27");
        assert_eq!(selected.cycle_number, 13);
    }

    #[test]
    fn selected_historical_schedule_drives_the_import_destination_not_today_schedule() {
        let schedules = vec![
            schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
            schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
        ];
        let selected = selected_payroll_return_schedule(&schedules, 1).unwrap();

        let destination = crate::payroll_file_naming::payslip_path(
            std::path::Path::new("/payslips"),
            "Alex Smith",
            selected,
        )
        .unwrap();

        assert!(destination.starts_with("/payslips/2026 to 2027"));
        assert!(!destination.starts_with("/payslips/2027 to 2028"));
    }

    #[test]
    fn schedule_label_contains_all_disambiguating_period_information() {
        let item = schedule(6, "2026/27", 6, "10/08/2026", "04/09/2026");

        let label = payroll_schedule_label(&item);

        assert!(label.contains("2026/27"));
        assert!(label.contains("Week 22"));
        assert!(label.contains("10/08/2026 to 06/09/2026"));
        assert!(label.contains("Pay 04/09/2026"));
        assert!(!label.contains("C6"));
        assert!(!label.contains("cycle"));
        assert!(!label.contains("PAYE"));
    }

    #[test]
    fn schedule_details_use_payroll_user_terminology_only() {
        let item = schedule(6, "2026/27", 6, "10/08/2026", "04/09/2026");

        let details = payroll_schedule_details(&item).unwrap();

        assert_eq!(
            details,
            [
                "Payroll year: 2026/27",
                "Payroll week: 22",
                "Timesheet period: 10/08/2026 to 06/09/2026",
                "Scheduled pay date: 04/09/2026",
            ]
        );
        assert!(!details.iter().any(|detail| detail.contains("cycle")));
        assert!(!details.iter().any(|detail| detail.contains("PAYE")));
    }

    #[test]
    fn payroll_schedule_view_defaults_to_the_current_year() {
        let years = vec!["2027/28".to_string(), "2026/27".to_string()];

        assert_eq!(
            default_payroll_schedule_year(&years, Some("2026/27")),
            Some("2026/27".to_string())
        );
    }

    #[test]
    fn future_year_is_available_without_replacing_the_current_default() {
        let schedules = vec![
            schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
            schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
        ];

        let years = payroll_schedule_years_newest_first(&schedules);

        assert_eq!(years, vec!["2027/28", "2026/27"]);
        assert_eq!(
            default_payroll_schedule_year(&years, Some("2026/27")),
            Some("2026/27".to_string())
        );
    }

    #[test]
    fn explicit_future_year_selection_identifies_only_its_schedule_rows() {
        let schedules = vec![
            schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
            schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
            schedule(3, "2027/28", 2, "19/04/2027", "14/05/2027"),
        ];

        let displayed = schedules
            .iter()
            .filter(|schedule| schedule.payroll_year == "2027/28")
            .collect::<Vec<_>>();

        assert_eq!(displayed.len(), 2);
        assert!(displayed
            .iter()
            .all(|schedule| schedule.payroll_year == "2027/28"));
        assert_eq!(
            displayed
                .iter()
                .map(|schedule| schedule.cycle_number)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn payroll_schedule_view_falls_back_to_most_recent_imported_year() {
        let schedules = vec![
            schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
            schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
        ];
        let years = payroll_schedule_years_newest_first(&schedules);

        assert_eq!(
            default_payroll_schedule_year(&years, None),
            Some("2027/28".to_string())
        );
    }

    #[test]
    fn payroll_schedule_years_are_ordered_newest_first_and_deduplicated() {
        let schedules = vec![
            schedule(3, "2027/28", 2, "19/04/2027", "14/05/2027"),
            schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
            schedule(4, "2028/29", 1, "20/03/2028", "14/04/2028"),
            schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
        ];

        assert_eq!(
            payroll_schedule_years_newest_first(&schedules),
            vec!["2028/29", "2027/28", "2026/27"]
        );
    }

    #[test]
    fn viewing_and_selecting_payroll_years_creates_no_directories() {
        let directory = tempfile::TempDir::new().unwrap();
        let schedules = vec![
            schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027"),
            schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027"),
        ];

        let years = payroll_schedule_years_newest_first(&schedules);
        let selected = default_payroll_schedule_year(&years, Some("2027/28"));

        assert_eq!(selected, Some("2027/28".to_string()));
        assert_eq!(directory.path().read_dir().unwrap().count(), 0);
    }

    #[test]
    fn operational_period_defaults_to_current_schedule_not_newest_future_schedule() {
        let current = schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027");
        let future = schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027");
        let schedules = ordered_payroll_schedules(vec![current.clone(), future], None);

        let state = default_operational_payroll_period(&schedules, Some(&current));

        assert_eq!(state.selected, Some(operational_period_key(&current)));
        assert!(!state.explicitly_selected);
    }

    #[test]
    fn operational_period_defaults_to_newest_schedule_when_none_is_current() {
        let older = schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027");
        let newer = schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027");
        let schedules = ordered_payroll_schedules(vec![older, newer.clone()], None);

        let state = default_operational_payroll_period(&schedules, None);

        assert_eq!(state.selected, Some(operational_period_key(&newer)));
        assert!(!state.explicitly_selected);
    }

    #[test]
    fn historical_and_future_operational_selections_are_explicit_and_can_reset_to_current() {
        let historical = schedule(1, "2026/27", 12, "25/01/2027", "19/02/2027");
        let current = schedule(2, "2026/27", 13, "22/02/2027", "19/03/2027");
        let future = schedule(3, "2027/28", 1, "22/03/2027", "16/04/2027");
        let mut state = default_operational_payroll_period(&[current.clone()], Some(&current));

        state.select(&historical, Some(&current));
        assert_eq!(state.selected, Some(operational_period_key(&historical)));
        assert!(state.explicitly_selected);
        assert_eq!(
            operational_period_timing(
                &historical,
                chrono::NaiveDate::from_ymd_opt(2027, 3, 1).unwrap()
            ),
            Some(OperationalPeriodTiming::Historical)
        );

        state.select(&future, Some(&current));
        assert_eq!(state.selected, Some(operational_period_key(&future)));
        assert!(state.explicitly_selected);
        assert_eq!(
            operational_period_timing(
                &future,
                chrono::NaiveDate::from_ymd_opt(2027, 3, 1).unwrap()
            ),
            Some(OperationalPeriodTiming::Future)
        );

        state.use_current(&current);
        assert_eq!(state.selected, Some(operational_period_key(&current)));
        assert!(!state.explicitly_selected);
    }

    #[test]
    fn operational_selection_is_independent_of_return_and_schedule_view_selections() {
        let current = schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027");
        let future = schedule(2, "2027/28", 1, "22/03/2027", "16/04/2027");
        let state = default_operational_payroll_period(&[current.clone()], Some(&current));
        let return_schedules = [current.clone(), future.clone()];

        let return_selection =
            selected_payroll_return_schedule(&return_schedules, future.id).unwrap();
        let viewed_year = default_payroll_schedule_year(
            &["2027/28".to_string(), "2026/27".to_string()],
            Some("2027/28"),
        );

        assert_eq!(return_selection.payroll_year, "2027/28");
        assert_eq!(viewed_year.as_deref(), Some("2027/28"));
        assert_eq!(state.selected, Some(operational_period_key(&current)));
    }

    #[test]
    fn historical_operational_selection_survives_actions_and_drives_paths_and_dates() {
        let current = schedule(2, "2027/28", 6, "09/08/2027", "03/09/2027");
        let selected = schedule(1, "2026/27", 6, "10/08/2026", "04/09/2026");
        let mut state = default_operational_payroll_period(&[current.clone()], Some(&current));
        state.select(&selected, Some(&current));
        let selected_key = state.selected.clone();

        let timesheet = crate::payroll_file_naming::timesheet_path(
            std::path::Path::new("/timesheets/2027 to 2028"),
            "Alex Smith",
            &selected,
        )
        .unwrap();
        let payslip = crate::payroll_file_naming::payslip_path(
            std::path::Path::new("/payslips/2027 to 2028"),
            "Alex Smith",
            &selected,
        )
        .unwrap();
        let first_week = parse_date_checked(&selected.first_week_commencing).unwrap();
        let week_dates = (0..4)
            .map(|week| first_week + chrono::Duration::days(week * 7))
            .collect::<Vec<_>>();

        assert_eq!(state.selected, selected_key);
        assert!(state.explicitly_selected);
        assert_eq!(selected.payroll_year, "2026/27");
        assert_eq!(week_dates[0].format("%d/%m/%Y").to_string(), "10/08/2026");
        assert_eq!(week_dates[3].format("%d/%m/%Y").to_string(), "31/08/2026");
        assert_eq!(
            timesheet,
            std::path::Path::new("/timesheets/2026 to 2027/Timesheet - Alex Smith - 202608w22.pdf")
        );
        assert_eq!(
            payslip,
            std::path::Path::new("/payslips/2026 to 2027/Payslip for Week 22 for Alex Smith.pdf")
        );
    }

    #[test]
    fn timesheet_status_lookup_uses_the_selected_year_and_cycle() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        let repository =
            crate::payroll_timesheet_email_repository::PayrollTimesheetEmailRepository::new(
                connection,
            );
        let current = schedule(2, "2027/28", 6, "09/08/2027", "03/09/2027");
        let selected = schedule(1, "2026/27", 6, "10/08/2026", "04/09/2026");
        let mut state = default_operational_payroll_period(&[current.clone()], Some(&current));
        state.select(&selected, Some(&current));

        repository
            .mark_sent(42, "2026/27", 6, "timesheet", "2026-09-01T10:00:00Z")
            .unwrap();
        let key = state.selected.unwrap();

        assert!(repository
            .get_for_pa_and_cycle(42, &key.payroll_year, key.cycle_number, "timesheet")
            .unwrap()
            .unwrap()
            .sent_at
            .is_some());
        assert!(repository
            .get_for_pa_and_cycle(42, "2027/28", 6, "timesheet")
            .unwrap()
            .is_none());
    }

    #[test]
    fn selecting_an_operational_period_has_no_filesystem_or_database_side_effect() {
        let directory = tempfile::TempDir::new().unwrap();
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        let count_before: i64 = connection
            .query_row("SELECT COUNT(*) FROM payroll_schedules", [], |row| {
                row.get(0)
            })
            .unwrap();
        let current = schedule(1, "2026/27", 13, "22/02/2027", "19/03/2027");
        let historical = schedule(2, "2026/27", 12, "25/01/2027", "19/02/2027");
        let mut state = default_operational_payroll_period(&[current.clone()], Some(&current));

        state.select(&historical, Some(&current));

        let count_after: i64 = connection
            .query_row("SELECT COUNT(*) FROM payroll_schedules", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(directory.path().read_dir().unwrap().count(), 0);
        assert_eq!(count_after, count_before);
    }

    #[test]
    fn production_batch_capture_preserves_current_historical_and_future_period_facts() {
        let historical = schedule(1, "2026/27", 5, "13/07/2026", "07/08/2026");
        let current = schedule(2, "2026/27", 6, "10/08/2026", "04/09/2026");
        let future = schedule(3, "2026/27", 7, "07/09/2026", "02/10/2026");
        let today = chrono::NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();

        let captures = [
            (
                capture_operational_payroll_period(&historical),
                OperationalPeriodTiming::Historical,
            ),
            (
                capture_operational_payroll_period(&current),
                OperationalPeriodTiming::Current,
            ),
            (
                capture_operational_payroll_period(&future),
                OperationalPeriodTiming::Future,
            ),
        ];

        for (captured, expected_timing) in captures {
            assert_eq!(
                captured_operational_period_timing(&captured, today),
                Some(expected_timing)
            );
            assert!(!captured.key.payroll_year.is_empty());
            assert!(!captured.first_week_commencing.is_empty());
            assert!(!captured.pay_date.is_empty());
        }
    }

    #[test]
    fn production_confirmation_period_text_uses_week_terminology_without_internal_cycle() {
        let captured = capture_operational_payroll_period(&schedule(
            6,
            "2026/27",
            6,
            "10/08/2026",
            "04/09/2026",
        ));
        let confirmation = format!("Payroll period: {}", captured.display_label);

        assert_eq!(
            confirmation,
            "Payroll period: 2026/27 · Week 22 · 10/08/2026 to 06/09/2026 · Pay 04/09/2026"
        );
        assert!(!confirmation.contains("cycle"));
        assert!(!confirmation.contains("C6"));
    }

    #[test]
    fn production_batch_refuses_any_selection_change_even_if_changed_back() {
        let current = schedule(1, "2026/27", 6, "10/08/2026", "04/09/2026");
        let other = schedule(2, "2026/27", 7, "07/09/2026", "02/10/2026");
        let captured = capture_operational_payroll_period(&current);
        let mut state = default_operational_payroll_period(&[current.clone()], Some(&current));
        let captured_revision = state.revision;

        state.select(&other, Some(&current));
        state.use_current(&current);

        let error = validate_captured_email_batch_period(
            &captured,
            captured_revision,
            &state,
            Some(&current),
        )
        .unwrap_err();
        assert!(error.to_string().contains("selection changed"));
    }

    #[test]
    fn production_batch_accepts_unchanged_selection_and_schedule() {
        let selected = schedule(1, "2026/27", 6, "10/08/2026", "04/09/2026");
        let captured = capture_operational_payroll_period(&selected);
        let state = default_operational_payroll_period(&[selected.clone()], Some(&selected));

        validate_captured_email_batch_period(&captured, state.revision, &state, Some(&selected))
            .unwrap();
    }

    #[test]
    fn production_batch_refuses_missing_or_materially_changed_schedule() {
        let selected = schedule(1, "2026/27", 6, "10/08/2026", "04/09/2026");
        let captured = capture_operational_payroll_period(&selected);
        let state = default_operational_payroll_period(&[selected.clone()], Some(&selected));

        let missing = validate_captured_email_batch_period(&captured, state.revision, &state, None)
            .unwrap_err();
        assert!(missing.to_string().contains("no longer exists"));

        let mut changed = selected;
        changed.pay_date = "05/09/2026".to_string();
        let changed_error =
            validate_captured_email_batch_period(&captured, state.revision, &state, Some(&changed))
                .unwrap_err();
        assert!(changed_error.to_string().contains("schedule dates changed"));
    }

    #[test]
    fn production_uses_captured_period_for_attachment_and_status_identity() {
        let selected = schedule(1, "2026/27", 6, "10/08/2026", "04/09/2026");
        let captured = capture_operational_payroll_period(&selected);
        let state = default_operational_payroll_period(&[selected.clone()], Some(&selected));
        validate_captured_email_batch_period(&captured, state.revision, &state, Some(&selected))
            .unwrap();

        let timesheet = crate::payroll_file_naming::timesheet_path(
            std::path::Path::new("/timesheets/2027 to 2028"),
            "Alex Smith",
            &selected,
        )
        .unwrap();
        let payslip = crate::payroll_file_naming::payslip_path(
            std::path::Path::new("/payslips/2027 to 2028"),
            "Alex Smith",
            &selected,
        )
        .unwrap();
        assert!(timesheet.ends_with("2026 to 2027/Timesheet - Alex Smith - 202608w22.pdf"));
        assert!(payslip.ends_with("2026 to 2027/Payslip for Week 22 for Alex Smith.pdf"));

        let connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        let repository =
            crate::payroll_timesheet_email_repository::PayrollTimesheetEmailRepository::new(
                connection,
            );
        repository
            .mark_sent(
                42,
                &captured.key.payroll_year,
                captured.key.cycle_number,
                "timesheet",
                "2026-09-01T10:00:00Z",
            )
            .unwrap();
        assert!(repository
            .get_for_pa_and_cycle(42, "2026/27", 6, "timesheet")
            .unwrap()
            .is_some());
        assert!(repository
            .get_for_pa_and_cycle(42, "2027/28", 6, "timesheet")
            .unwrap()
            .is_none());
    }

    #[test]
    fn generation_eligibility_includes_active_and_none_status_personal_assistants() {
        assert!(personal_assistant_is_eligible_for_generation(
            Some("Active"),
            false
        ));
        assert!(personal_assistant_is_eligible_for_generation(None, false));
    }

    #[test]
    fn generation_eligibility_includes_inactive_only_with_selected_period_record() {
        assert!(personal_assistant_is_eligible_for_generation(
            Some("Inactive"),
            true
        ));
        assert!(!personal_assistant_is_eligible_for_generation(
            Some("Inactive"),
            false
        ));
    }

    #[test]
    fn inactive_historical_generation_uses_selected_schedule_without_creating_unrelated_records() {
        let directory = tempfile::TempDir::new().unwrap();
        let database = directory.path().join("test.sqlite");
        let connection = rusqlite::Connection::open(&database).unwrap();
        crate::database::create_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO payroll_timesheets (
                personal_assistant_id, payroll_year, cycle_number,
                previous_cycle_hours, created_at, updated_at
             ) VALUES (1, '2026/27', 6, NULL, 'created', 'created')",
                [],
            )
            .unwrap();
        drop(connection);
        let repository = crate::payroll_timesheet_repository::PayrollTimesheetRepository::new(
            rusqlite::Connection::open(&database).unwrap(),
        );
        let records = repository.get_all_for_cycle("2026/27", 6).unwrap();
        let ids = records
            .iter()
            .map(|record| record.personal_assistant_id)
            .collect::<std::collections::HashSet<_>>();
        let historical = schedule(6, "2026/27", 6, "10/08/2026", "04/09/2026");

        assert!(personal_assistant_is_eligible_for_generation(
            Some("Inactive"),
            ids.contains(&1)
        ));
        assert!(!personal_assistant_is_eligible_for_generation(
            Some("Inactive"),
            ids.contains(&2)
        ));
        let path = crate::payroll_file_naming::timesheet_path(
            std::path::Path::new("/timesheets/2027 to 2028"),
            "Historical PA",
            &historical,
        )
        .unwrap();
        assert_eq!(
            path,
            std::path::Path::new(
                "/timesheets/2026 to 2027/Timesheet - Historical PA - 202608w22.pdf"
            )
        );
        assert_eq!(repository.get_all_for_cycle("2026/27", 6).unwrap().len(), 1);
    }
}
