use chrono::Datelike;
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
use crate::pdf_generator::{payroll_week_filename, PdfGenerator, TimesheetPdfData};
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

struct PendingEmailBatch {
    kind: PayrollEmailKind,
    stage: EmailBatchNoteStage,
    selected_personal_assistant_ids: Vec<i64>,
}

pub struct DirectPaymentApp {
    application: Application,
    version: String,
    status_message: String,
    last_import: Option<ImportSummary>,
    timesheets: Vec<TimesheetEntry>,
    payroll_schedules: Vec<PayrollSchedule>,
    preview_personal_assistant_id: Option<i64>,
    email_preview: Option<PayrollEmailPreview>,
    additional_notes_by_personal_assistant: HashMap<i64, String>,
    note_enabled_personal_assistant_ids: HashSet<i64>,
    pending_email_batch: Option<PendingEmailBatch>,
    email_settings_employer: Option<crate::models::Employer>,
    email_settings_payroll_provider: Option<crate::payroll_provider_repository::PayrollProvider>,
    employer_screen: crate::employer_screen::EmployerScreen,
    personal_assistant_screen: PersonalAssistantScreen,
    payroll_settings_screen: PayrollSettingsScreen,
    payroll_timesheet_screen: PayrollTimesheetScreen,
    application_settings_screen: ApplicationSettingsScreen,
    active_screen: ActiveScreen,
}

impl DirectPaymentApp {
    pub fn new(application: Application) -> Self {
        Self {
            version: application.context.version.clone(),
            application,
            status_message: "Application ready.".to_string(),
            last_import: None,
            timesheets: Vec::new(),
            payroll_schedules: Vec::new(),
            preview_personal_assistant_id: None,
            email_preview: None,
            additional_notes_by_personal_assistant: HashMap::new(),
            note_enabled_personal_assistant_ids: HashSet::new(),
            pending_email_batch: None,
            email_settings_employer: None,
            email_settings_payroll_provider: None,
            employer_screen: crate::employer_screen::EmployerScreen::new(),
            personal_assistant_screen: PersonalAssistantScreen::new(),
            payroll_settings_screen: PayrollSettingsScreen::new(),
            payroll_timesheet_screen: PayrollTimesheetScreen::new(),
            application_settings_screen: ApplicationSettingsScreen::new(),
            active_screen: ActiveScreen::Dashboard,
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                self.personal_assistant_screen.show(ui, &self.application);
            }

            ActiveScreen::PayrollSettings => {
                if self.payroll_settings_screen.show(ui, &mut self.application) {
                    self.active_screen = ActiveScreen::EmailSettings;
                }
            }

            ActiveScreen::PayrollTimesheet => {
                self.payroll_timesheet_screen.show(ui, &self.application);
            }

            ActiveScreen::ApplicationSettings => {
                if self
                    .application_settings_screen
                    .show(ui, &mut self.application)
                {
                    self.active_screen = ActiveScreen::EmailSettings;
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

        let confirmation_kind = self.pending_email_batch.as_ref().and_then(|batch| {
            matches!(batch.stage, EmailBatchNoteStage::ConfirmDispatch).then_some(batch.kind)
        });
        if let Some(kind) = confirmation_kind {
            let email_type = match kind {
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
                    ui.horizontal(|ui| {
                        send = ui.button("Send").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });

            if cancel {
                self.clear_pending_email_batch();
            } else if send {
                let result = match kind {
                    PayrollEmailKind::Timesheet => self.email_timesheets(),
                    PayrollEmailKind::Payslip => self.email_payslips(),
                };
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

            let payroll_year = current_payroll_year();
            let today = chrono::Local::now().date_naive();
            let current_schedule = self
                .application
                .get_payroll_schedule(&payroll_year)?
                .into_iter()
                .filter_map(|schedule| {
                    let first_week = parse_date_checked(&schedule.first_week_commencing)?;
                    let cycle_end = first_week + chrono::Duration::days(27);
                    (first_week <= today && today <= cycle_end).then_some(schedule)
                })
                .max_by_key(|schedule| schedule.first_week_commencing.clone())
                .ok_or("No current payroll cycle was found.")?;

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);
            let attachment_path =
                crate::paths::expand_path(&self.application.context.config.folders.pdf_output)
                    .join(format!(
                        "Timesheet - {} - {}.pdf",
                        personal_assistant_name,
                        payroll_week_filename(&current_schedule.first_week_commencing)
                    ));
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
                &current_schedule.first_week_commencing,
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

            let payroll_year = current_payroll_year();
            let today = chrono::Local::now().date_naive();
            let current_schedule = self
                .application
                .get_payroll_schedule(&payroll_year)?
                .into_iter()
                .filter_map(|schedule| {
                    let first_week = parse_date_checked(&schedule.first_week_commencing)?;
                    let cycle_end = first_week + chrono::Duration::days(27);
                    (first_week <= today && today <= cycle_end).then_some(schedule)
                })
                .max_by_key(|schedule| schedule.first_week_commencing.clone())
                .ok_or("No current payroll cycle was found.")?;

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);
            let attachment_path =
                crate::paths::expand_path(&self.application.context.config.folders.payslip_folder)
                    .join(format!(
                        "Payslip for Week {} for {}.pdf",
                        payroll_week_filename(&current_schedule.first_week_commencing)
                            .rsplit_once('w')
                            .map(|(_, week)| week.to_string())
                            .unwrap_or_else(|| current_schedule.cycle_number.to_string()),
                        personal_assistant_name
                    ));

            self.application.send_test_payslip_email(
                sender_email,
                pa_test_email,
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                &current_schedule.first_week_commencing,
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

    fn draw_dashboard(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Dashboard");

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
                    self.status_message =
                        format!("Failed loading timesheets: {}", error);
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

        if ui.button("Email Timesheets").clicked() { self.begin_email_batch(PayrollEmailKind::Timesheet); }

        if ui.button("Import Payroll Return").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("ZIP files", &["zip"])
                .pick_file()
            {
                let payroll_year = current_payroll_year();

                match self.application.get_payroll_schedule(&payroll_year) {
                    Ok(schedules) => {
                        let today = chrono::Local::now().date_naive();

                        let current_schedule = schedules
                            .into_iter()
                            .filter_map(|schedule| {
                                let first_week = chrono::NaiveDate::parse_from_str(
                                    &schedule.first_week_commencing,
                                    "%d/%m/%Y",
                                )
                                .ok()?;

                                let cycle_end = first_week + chrono::Duration::days(27);

                                if first_week <= today && today <= cycle_end {
                                    Some((first_week, schedule))
                                } else {
                                    None
                                }
                            })
                            .max_by_key(|(date, _)| *date)
                            .map(|(_, schedule)| schedule);

                        match current_schedule {
                            Some(schedule) => {
                                match self.application.import_payroll_return(
                                    &path,
                                    &payroll_year,
                                    schedule.cycle_number,
                                ) {
                                    Ok(result) => {
                                        self.status_message = format!(
                                            "Payroll return imported: {} payslip(s), {} information file(s).",
                                            result.payslips_imported,
                                            result.information_files_imported
                                        );
                                    }

                                    Err(error) => {
                                        self.status_message =
                                            format!("Payroll return import failed: {}", error);
                                    }
                                }
                            }

                            None => {
                                self.status_message =
                                    "No current payroll cycle found.".to_string();
                            }
                        }
                    }

                    Err(error) => {
                        self.status_message =
                            format!("Failed loading payroll schedule: {}", error);
                    }
                }
            }
        }

        if ui.button("Email Payslips").clicked() { self.begin_email_batch(PayrollEmailKind::Payslip); }

        if ui.button("Import Payroll Prep Sheet").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Payroll Prep Sheet", &["pdf", "docx"])
                .pick_file()
            {
                match self.application.import_payroll_prep_sheet(&path) {
                    Ok(count) => {
                        self.status_message =
                            format!("Payroll Prep Sheet imported: {} schedule entries.", count);
                    }

                    Err(error) => {
                        self.status_message =
                            format!("Payroll Prep Sheet import failed: {}", error);
                    }
                }
            }
        }

        if ui.button("View Payroll Schedule").clicked() {
            let payroll_year = current_payroll_year();

            match self.application.get_payroll_schedule(&payroll_year) {
                Ok(schedules) => {
                    self.payroll_schedules = schedules;

                    self.status_message = format!(
                        "Loaded {} payroll schedule entries.",
                        self.payroll_schedules.len()
                    );
                }

                Err(error) => {
                    self.status_message =
                        format!("Failed loading payroll schedule: {}", error);
                }
            }
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

        draw_payroll_schedule(ui, &self.payroll_schedules);
        draw_timesheets(ui, &self.timesheets);
        self.draw_timesheet_email_status(ui);
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

            let payroll_year = current_payroll_year();
            let today = chrono::Local::now().date_naive();
            let current_schedule = self
                .application
                .get_payroll_schedule(&payroll_year)?
                .into_iter()
                .filter_map(|schedule| {
                    let first_week = parse_date_checked(&schedule.first_week_commencing)?;
                    let cycle_end = first_week + chrono::Duration::days(27);
                    (first_week <= today && today <= cycle_end).then_some(schedule)
                })
                .max_by_key(|schedule| schedule.first_week_commencing.clone())
                .ok_or("No current payroll cycle was found.")?;

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);
            let (attachment_path, body) = match kind {
                PayrollEmailKind::Timesheet => {
                    let filename = format!(
                        "Timesheet - {} - {}.pdf",
                        personal_assistant_name,
                        payroll_week_filename(&current_schedule.first_week_commencing)
                    );
                    (
                        crate::paths::expand_path(
                            &self.application.context.config.folders.pdf_output,
                        )
                        .join(filename),
                        &self.application.context.config.payroll.timesheet_email_body,
                    )
                }
                PayrollEmailKind::Payslip => {
                    let filename = format!(
                        "Payslip for Week {} for {}.pdf",
                        payroll_week_filename(&current_schedule.first_week_commencing)
                            .rsplit_once('w')
                            .map(|(_, week)| week.to_string())
                            .unwrap_or_else(|| current_schedule.cycle_number.to_string()),
                        personal_assistant_name
                    );
                    (
                        crate::paths::expand_path(
                            &self.application.context.config.folders.payslip_folder,
                        )
                        .join(filename),
                        &self.application.context.config.payroll.payslip_email_body,
                    )
                }
            };

            self.application.preview_payroll_email(
                &payroll_department_email,
                employer_email,
                assistant.email.as_deref(),
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                &current_schedule.first_week_commencing,
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

    fn email_payslips(&self) -> Result<usize, Box<dyn std::error::Error>> {
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

        let payroll_year = current_payroll_year();

        let schedules = self.application.get_payroll_schedule(&payroll_year)?;

        let today = chrono::Local::now().date_naive();

        let current_schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date_checked(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(first_week, _)| *first_week)
            .map(|(_, schedule)| schedule)
            .ok_or_else(|| {
                format!(
                    "No current payroll cycle was found for {}.",
                    today.format("%d/%m/%Y")
                )
            })?;

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
                    current_schedule.cycle_number,
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

            let filename = format!(
                "Payslip for Week {} for {}.pdf",
                payroll_week_filename(&current_schedule.first_week_commencing)
                    .rsplit_once('w')
                    .map(|(_, week)| week.to_string())
                    .unwrap_or_else(|| current_schedule.cycle_number.to_string()),
                personal_assistant_name
            );

            let payslip_path = payslip_folder.join(filename);

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
                &current_schedule.first_week_commencing,
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
                    current_schedule.cycle_number,
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
                        current_schedule.cycle_number,
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
                .mark_payslips_sent(current_schedule.id)?;
        }

        Ok(sent)
    }

    fn email_timesheets(&self) -> Result<usize, Box<dyn std::error::Error>> {
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

        let payroll_year = current_payroll_year();

        let schedules = self.application.get_payroll_schedule(&payroll_year)?;

        let today = chrono::Local::now().date_naive();

        let current_schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date_checked(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(first_week, _)| *first_week)
            .map(|(_, schedule)| schedule)
            .ok_or_else(|| {
                format!(
                    "No current payroll cycle was found for {}.",
                    today.format("%d/%m/%Y")
                )
            })?;

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
                    current_schedule.cycle_number,
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

            let filename = format!(
                "Timesheet - {} - {}.pdf",
                personal_assistant_name,
                payroll_week_filename(&current_schedule.first_week_commencing)
            );

            let timesheet_path = pdf_output_folder.join(filename);

            if !timesheet_path.exists() {
                return Err(format!(
                    "Timesheet PDF not found for {}: {}",
                    personal_assistant_name,
                    timesheet_path.display()
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
                &current_schedule.first_week_commencing,
                &timesheet_path,
                &self.application.context.config.payroll.timesheet_email_body,
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
                    current_schedule.cycle_number,
                    "timesheet",
                    &sent_at,
                )?;

            sent += 1;
        }

        Ok(sent)
    }

    fn draw_timesheet_email_status(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Timesheet Email Status");

        let payroll_year = current_payroll_year();

        let schedules = match self.application.get_payroll_schedule(&payroll_year) {
            Ok(schedules) => schedules,
            Err(error) => {
                ui.label(format!("Unable to load payroll schedule: {}", error));
                return;
            }
        };

        let today = chrono::Local::now().date_naive();

        let current_schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date_checked(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(first_week, _)| *first_week)
            .map(|(_, schedule)| schedule);

        let current_schedule = match current_schedule {
            Some(schedule) => schedule,
            None => {
                ui.label("No current payroll cycle found.");
                return;
            }
        };

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

        let payroll_year = current_payroll_year();

        let schedules = self.application.get_payroll_schedule(&payroll_year)?;

        if schedules.is_empty() {
            return Err(
                format!("No payroll schedule has been loaded for {}.", payroll_year).into(),
            );
        }

        let today = chrono::Local::now().date_naive();

        let current_schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date_checked(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(first_week, _)| *first_week)
            .map(|(_, schedule)| schedule)
            .ok_or_else(|| {
                format!(
                    "No current payroll cycle was found for {}.",
                    today.format("%d/%m/%Y")
                )
            })?;

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

        let output_dir =
            crate::paths::expand_path(&self.application.context.config.folders.pdf_output);

        let mut generated = 0usize;

        for assistant in &assistants {
            let is_active = match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            };

            if !is_active {
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

            let pay_rate = self
                .application
                .get_current_pay_rate_for_personal_assistant(assistant.id)?;

            let pay_rate = match pay_rate {
                Some(rate) => rate.base_hourly_rate + rate.employer_top_up_rate,
                None => 0.0,
            };

            let contracted_hours = self
                .application
                .contracted_hours_repository
                .get_current_for_personal_assistant(assistant.id)?;

            let contracted_weekly_hours = contracted_hours
                .map(|hours| hours.contracted_hours)
                .unwrap_or_else(|| "0".to_string());

            let hours_worked = [
                format_pdf_hours(payroll_weeks[0].worked_hours),
                format_pdf_hours(payroll_weeks[1].worked_hours),
                format_pdf_hours(payroll_weeks[2].worked_hours),
                format_pdf_hours(payroll_weeks[3].worked_hours),
            ];

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

            let previous_cycle_hours = payroll_timesheet.previous_cycle_hours.map(format_pdf_hours);

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
                employer_name: &employer.name,

                personal_assistant_name: &personal_assistant_name,

                national_insurance_number: assistant
                    .national_insurance_number
                    .as_deref()
                    .unwrap_or(""),

                contracted_weekly_hours: &contracted_weekly_hours,

                pay_rate,

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

            PdfGenerator::generate(&output_dir, &data, &self.application.context.config.pdf)?;

            generated += 1;
        }

        Ok(generated)
    }
}

fn current_payroll_year() -> String {
    let today = chrono::Local::now().date_naive();

    let start_year = if today.month() >= 4 {
        today.year()
    } else {
        today.year() - 1
    };

    format!("{}/{}", start_year, (start_year + 1) % 100)
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
