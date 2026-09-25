use eframe::egui;
use std::collections::{HashMap, HashSet};

use crate::app::Application;
use crate::application_settings_screen::ApplicationSettingsScreen;
use crate::config::ApplicationTheme;
use crate::email_service::PayrollEmailPreview;
use crate::enter_hours_screen::EnterHoursScreen;
use crate::import_service::ImportSummary;
use crate::models::TimesheetEntry;
use crate::payroll_schedule_repository::PayrollSchedule;
use crate::payroll_settings_screen::PayrollSettingsScreen;
use crate::payroll_timesheet_repository::{PayrollTimesheetPublicHoliday, PayrollTimesheetWeek};
use crate::payroll_timesheet_screen::PayrollTimesheetScreen;
use crate::payroll_worked_item_repository::{PayrollWorkedItemRepository, SnapshotState};
use crate::pdf_generator::{PdfGenerator, PublicHolidayPdfEntry, TimesheetPdfData};
use crate::personal_assistant_screen::PersonalAssistantScreen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveScreen {
    Dashboard,
    EnterHours,
    Employer,
    PersonalAssistant,
    PayrollSettings,
    PayrollTimesheet,
    ApplicationSettings,
    EmailSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimesheetSortColumn {
    PaName,
    Start,
    End,
    Worked,
    Rate,
    Amount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimesheetSortState {
    column: TimesheetSortColumn,
    direction: SortDirection,
    pa_date_direction: SortDirection,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DashboardStatusPanelStyle {
    fill: egui::Color32,
    stroke: egui::Stroke,
}

#[derive(Default)]
struct DashboardWorkflowActions {
    enter_hours: bool,
    import_hours_csv: bool,
    view_imported_hours: bool,
    prepare_timesheets: bool,
    generate_timesheets: bool,
    email_timesheets: bool,
    import_payroll_documents: bool,
    email_payslips: bool,
    view_payroll_schedule: bool,
}

impl Default for TimesheetSortState {
    fn default() -> Self {
        Self {
            column: TimesheetSortColumn::Start,
            direction: SortDirection::Descending,
            pa_date_direction: SortDirection::Descending,
        }
    }
}

impl TimesheetSortState {
    fn select(&mut self, column: TimesheetSortColumn) {
        if self.column == column {
            self.direction = match self.direction {
                SortDirection::Ascending => SortDirection::Descending,
                SortDirection::Descending => SortDirection::Ascending,
            };
        } else {
            self.column = column;
            self.direction = SortDirection::Ascending;
        }
    }
}

#[derive(Clone, Copy)]
enum PayrollEmailKind {
    Timesheet,
    Payslip,
}

#[derive(Clone, Copy)]
enum EmailBatchNoteStage {
    SelectRecipients,
    ChooseAdditionalNote,
    EditAdditionalNotes,
    ConfirmDispatch,
}

#[derive(Clone)]
struct PendingEmailBatch {
    kind: PayrollEmailKind,
    stage: EmailBatchNoteStage,
    selected_personal_assistant_ids: Vec<i64>,
    choices: Vec<ProductionChoice>,
    payroll_period: Option<CapturedOperationalPayrollPeriod>,
    operational_selection_revision: u64,
}

struct PendingPayrollReturnImport {
    source_path: std::path::PathBuf,
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
    duplicate_ui: crate::payroll_evidence::duplicate_ui::DuplicateUi,
    application: Application,
    version: String,
    about_open: bool,
    about_schema_version: String,
    update_check: crate::update_check::UpdateCheck,
    initial_size_pending: bool,
    status_message: String,
    file_status: Option<(String, Vec<std::path::PathBuf>)>,
    last_import: Option<ImportSummary>,
    payroll_document_import: Option<crate::archive::PayrollReturnImportResult>,
    timesheets: Vec<TimesheetEntry>,
    timesheet_sort: TimesheetSortState,
    timesheet_pa_filter: Option<String>,
    timesheet_all_cycles: bool,
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
    pending_generation: Option<PendingGeneration>,
    production_report: Option<ProductionReport>,
    pending_payroll_return_import: Option<PendingPayrollReturnImport>,
    email_settings_employer: Option<crate::models::Employer>,
    email_settings_payroll_provider: Option<crate::payroll_provider_repository::PayrollProvider>,
    employer_screen: crate::employer_screen::EmployerScreen,
    personal_assistant_screen: PersonalAssistantScreen,
    payroll_settings_screen: PayrollSettingsScreen,
    payroll_timesheet_screen: PayrollTimesheetScreen,
    pending_preparation_navigation: Option<(ActiveScreen, OperationalPayrollPeriodState, bool)>,
    preparation_close_allowed: bool,
    enter_hours_screen: EnterHoursScreen,
    application_settings_screen: ApplicationSettingsScreen,
    active_screen: ActiveScreen,
    restart_required_message: Option<String>,
    restart_required_paths: Vec<std::path::PathBuf>,
}

impl DirectPaymentApp {
    pub fn new(application: Application) -> Self {
        let (operational_payroll_schedules, operational_payroll_period, operational_error) =
            initial_operational_payroll_period(&application);
        Self {
            duplicate_ui: Default::default(),
            version: application.context.version.clone(),
            about_open: false,
            about_schema_version: String::new(),
            update_check: Default::default(),
            initial_size_pending: true,
            application,
            status_message: "Application ready.".to_string(),
            file_status: None,
            last_import: None,
            payroll_document_import: None,
            timesheets: Vec::new(),
            timesheet_sort: TimesheetSortState::default(),
            timesheet_pa_filter: None,
            timesheet_all_cycles: false,
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
            pending_generation: None,
            production_report: None,
            pending_payroll_return_import: None,
            email_settings_employer: None,
            email_settings_payroll_provider: None,
            employer_screen: crate::employer_screen::EmployerScreen::new(),
            personal_assistant_screen: PersonalAssistantScreen::new(),
            payroll_settings_screen: PayrollSettingsScreen::new(),
            payroll_timesheet_screen: PayrollTimesheetScreen::new(),
            pending_preparation_navigation: None,
            preparation_close_allowed: false,
            enter_hours_screen: EnterHoursScreen::new(),
            application_settings_screen: ApplicationSettingsScreen::new(),
            active_screen: ActiveScreen::Dashboard,
            restart_required_message: None,
            restart_required_paths: Vec::new(),
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::date_utils::set_display(ctx, self.application.context.config.date_display_format);
        if self.initial_size_pending {
            self.initial_size_pending = false;
            let initial_size = ctx.input(|input| {
                if input.viewport().maximized == Some(true) {
                    None
                } else {
                    Some(crate::application::initial_window_size(
                        input.viewport().monitor_size,
                    ))
                }
            });
            if let Some(size) = initial_size {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            }
        }
        let was_preparation = self.active_screen == ActiveScreen::PayrollTimesheet;
        let mut requested_close = ctx.input(|i| i.viewport().close_requested());
        if was_preparation
            && !self.preparation_close_allowed
            && self.payroll_timesheet_screen.has_unsaved_changes()
            && requested_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.draw_about(ctx);
        if let Some(message) = &self.restart_required_message {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Restart Required");
                ui.horizontal_wrapped(|ui| {
                    ui.label(message);
                    for path in &self.restart_required_paths {
                        let _ = crate::folder_opener::button(ui, path);
                    }
                });
                ui.separator();
                if ui.button("Close DirectPaymentTimesheets").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            return;
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(format!("DirectPaymentTimesheets v{}", self.version));
                ui.label(
                    egui::RichText::new(format!(
                        "(DBschema {})",
                        crate::database::CURRENT_SCHEMA_VERSION
                    ))
                    .size(13.0),
                );

                if ui.button("Settings").clicked() {
                    self.active_screen = ActiveScreen::ApplicationSettings;
                }

                if ui.button("Email Settings").clicked() {
                    self.active_screen = ActiveScreen::EmailSettings;
                }

                if ui.button("About").clicked() {
                    // Only read metadata when opening About; never migrate or write.
                    self.about_schema_version = rusqlite::Connection::open_with_flags(
                        &self.application.context.environment.database_path,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )
                    .and_then(|connection| {
                        connection.query_row(
                            "SELECT version FROM schema_version LIMIT 1",
                            [],
                            |row| row.get::<_, i64>(0),
                        )
                    })
                    .map(|version| version.to_string())
                    .unwrap_or_else(|_| "unavailable".into());
                    self.about_open = true;
                }

                ui.add_space(10.0);
                if ui.button("Exit").clicked() {
                    requested_close = true;
                }
            });
        });

        egui::TopBottomPanel::bottom("navigation").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
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

                if ui.button("Exit").clicked() {
                    requested_close = true;
                }
            });
        });

        if !self.defer_preparation_navigation(was_preparation, requested_close) && requested_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.active_screen {
            ActiveScreen::Dashboard => {
                self.draw_dashboard(ui);
            }

            ActiveScreen::EnterHours => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.enter_hours_screen.show(ui, &self.application);
                });
            }

            ActiveScreen::Employer => {
                if egui::ScrollArea::vertical()
                    .show(ui, |ui| self.employer_screen.show(ui, &self.application))
                    .inner
                {
                    self.active_screen = ActiveScreen::EmailSettings;
                }
            }

            ActiveScreen::PersonalAssistant => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.personal_assistant_screen.show(ui, &self.application);
                });
            }

            ActiveScreen::PayrollSettings => {
                if egui::ScrollArea::vertical()
                    .show(ui, |ui| {
                        self.payroll_settings_screen.show(ui, &mut self.application)
                    })
                    .inner
                {
                    self.active_screen = ActiveScreen::EmailSettings;
                }
            }

            ActiveScreen::PayrollTimesheet => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Payroll Timesheet Preparation");
                    let previous_period = self.operational_payroll_period.clone();
                    self.draw_operational_payroll_period_selector(ui);
                    self.guard_preparation_period_change(previous_period);
                    ui.separator();

                    match self.selected_operational_payroll_schedule() {
                        Ok(schedule) => {
                            let period_label = payroll_schedule_label(&schedule);
                            self.payroll_timesheet_screen.show(
                                ui,
                                &self.application,
                                &schedule,
                                &period_label,
                            );
                        }
                        Err(error) => {
                            ui.label(format!(
                                "Unable to load the selected payroll period: {error}"
                            ));
                        }
                    }
                });
            }

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
                    self.restart_required_paths =
                        self.application_settings_screen.restart_paths().to_vec();
                }
            }

            ActiveScreen::EmailSettings => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_email_settings(ui);
                });
            }
        });
        if self.pending_preparation_navigation.is_some() {
            if let Some(proceed) = self
                .payroll_timesheet_screen
                .unsaved_dialog(ctx, &self.application)
            {
                if self.finish_preparation_navigation(proceed) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

impl DirectPaymentApp {
    fn defer_preparation_navigation(&mut self, was_preparation: bool, close: bool) -> bool {
        if was_preparation
            && self.payroll_timesheet_screen.has_unsaved_changes()
            && (self.active_screen != ActiveScreen::PayrollTimesheet || close)
        {
            self.pending_preparation_navigation = Some((
                self.active_screen,
                self.operational_payroll_period.clone(),
                close,
            ));
            self.active_screen = ActiveScreen::PayrollTimesheet;
            true
        } else {
            false
        }
    }

    fn guard_preparation_period_change(&mut self, previous: OperationalPayrollPeriodState) {
        if previous != self.operational_payroll_period
            && self.payroll_timesheet_screen.has_unsaved_changes()
        {
            self.pending_preparation_navigation = Some((
                ActiveScreen::PayrollTimesheet,
                self.operational_payroll_period.clone(),
                false,
            ));
            self.operational_payroll_period = previous;
        }
    }

    fn finish_preparation_navigation(&mut self, proceed: bool) -> bool {
        let Some((screen, period, close)) = self.pending_preparation_navigation.take() else {
            return false;
        };
        if proceed {
            self.active_screen = screen;
            self.operational_payroll_period = period;
            self.preparation_close_allowed = close;
            close
        } else {
            false
        }
    }

    fn draw_about(&mut self, ctx: &egui::Context) {
        self.update_check.poll();
        egui::Window::new("About DirectPaymentTimesheets")
            .open(&mut self.about_open)
            .collapsible(false)
            .default_width(460.0)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.heading("DirectPaymentTimesheets");
                ui.label(format!("Application version: {}", self.version));
                ui.label(format!(
                    "Database schema version: {}",
                    self.about_schema_version
                ));
                ui.label("Developed by Mark Worsdall");
                ui.label("Development: ArnieSkyNet / DirectPaymentTimesheets project on GitHub.");
                ui.label(format!("License: {}", env!("CARGO_PKG_LICENSE")));
                ui.separator();
                ui.label(crate::update_check::InstallationKind::current().guidance());
                if ui
                    .add_enabled(
                        !self.update_check.running(),
                        egui::Button::new("Check for updates"),
                    )
                    .clicked()
                {
                    self.update_check.start(ctx.clone());
                }
                if let Some(status) = &self.update_check.status {
                    ui.label(status);
                }
                ui.hyperlink_to("GitHub releases", crate::update_check::RELEASES_URL);
                ui.hyperlink_to("Project source and tags", crate::update_check::SOURCE_URL);
            });
    }

    fn clear_pending_email_batch(&mut self) {
        self.pending_email_batch = None;
        self.additional_notes_by_personal_assistant.clear();
        self.note_enabled_personal_assistant_ids.clear();
    }

    fn begin_email_batch(&mut self, kind: PayrollEmailKind) {
        let schedule = match self.selected_operational_payroll_schedule() {
            Ok(schedule) => Some(schedule),
            Err(_)
                if matches!(kind, PayrollEmailKind::Payslip)
                    && self.operational_payroll_period.selected.is_none() =>
            {
                None
            }
            Err(error) => {
                self.status_message = format!("Could not begin production email batch: {error}");
                return;
            }
        };
        self.additional_notes_by_personal_assistant.clear();
        self.note_enabled_personal_assistant_ids.clear();
        let choices = if matches!(kind, PayrollEmailKind::Timesheet) {
            match self.production_choices(schedule.as_ref().unwrap(), true) {
                Ok(choices) => choices,
                Err(error) => {
                    self.status_message = error.to_string();
                    return;
                }
            }
        } else {
            Vec::new()
        };
        let selected_personal_assistant_ids = if matches!(kind, PayrollEmailKind::Timesheet) {
            choices
                .iter()
                .filter(|c| c.available)
                .map(|c| c.id)
                .collect()
        } else {
            self.email_assistants(kind)
                .unwrap_or_default()
                .into_iter()
                .map(|pa| pa.id)
                .collect()
        };
        self.pending_email_batch = Some(PendingEmailBatch {
            kind,
            stage: if matches!(kind, PayrollEmailKind::Timesheet) {
                EmailBatchNoteStage::SelectRecipients
            } else {
                EmailBatchNoteStage::ChooseAdditionalNote
            },
            choices,
            selected_personal_assistant_ids,
            payroll_period: schedule.as_ref().map(capture_operational_payroll_period),
            operational_selection_revision: self.operational_payroll_period.revision,
        });
    }

    fn draw_additional_note_prompt(&mut self, ui: &mut egui::Ui) {
        self.draw_recipient_selection(ui);
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
            let assistants = if self
                .pending_email_batch
                .as_ref()
                .is_some_and(|b| matches!(b.kind, PayrollEmailKind::Timesheet))
            {
                crate::payroll_evidence::open(&self.application).ok().map(|db| ids.iter()
                    .filter_map(|id| crate::personal_assistant_repository::PersonalAssistantRepository::get_by_id_on(&db,*id).ok()).collect::<Vec<_>>()).unwrap_or_default()
            } else {
                self.email_assistants(PayrollEmailKind::Payslip)
                    .unwrap_or_default()
            };
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
                    if matches!(batch.kind, PayrollEmailKind::Timesheet) {
                        for choice in batch.choices.iter().filter(|c| batch.selected_personal_assistant_ids.contains(&c.id)) {
                            ui.label(format!("{} — {}", choice.name, choice.detail));
                        }
                        ui.label("Only these PAs will be sent. Current candidate evidence is checked again before each send.");
                    }
                    if let Some(period) = &batch.payroll_period {
                        ui.label(format!(
                            "Payroll period: {}",
                            crate::date_utils::calendar_text(
                                crate::date_utils::preference(ui),
                                &period.display_label
                            )
                        ));
                        match captured_operational_period_timing(
                            period,
                            chrono::Local::now().date_naive(),
                        ) {
                            Some(OperationalPeriodTiming::Historical) => {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    "Historical payroll period",
                                );
                            }
                            Some(OperationalPeriodTiming::Future) => {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    "Future payroll period",
                                );
                            }
                            Some(OperationalPeriodTiming::Current) | None => {}
                        }
                    } else {
                        ui.label("PA payroll documents (no four-week period)");
                    }
                    ui.horizontal(|ui| {
                        send = ui.button("Send").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });

            if cancel {
                self.clear_pending_email_batch();
            } else if send {
                if matches!(batch.kind, PayrollEmailKind::Timesheet) {
                    match self.dispatch_timesheet_selection(&batch) {
                        Ok(report) => {
                            self.status_message = report.summary();
                            self.production_report = Some(report);
                            self.clear_pending_email_batch();
                        }
                        Err(error) => {
                            self.status_message = format!("Timesheet email refused: {error}");
                        }
                    }
                    return;
                }
                let result = self
                    .validated_schedule_for_email_batch(&batch)
                    .and_then(|schedule| self.email_payslips(schedule.as_ref()));
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
            crate::gui_controls::combo_box("email_transport")
                .selected_text(&email.smtp_transport)
                .show_ui(ui, |ui| {
                    crate::gui_controls::combo_value(
                        ui,
                        &mut email.smtp_transport,
                        "Local SMTP Server".to_string(),
                        "Local SMTP Server",
                    );
                    crate::gui_controls::combo_value(
                        ui,
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

            crate::gui_controls::combo_box("email_subject_insert_field")
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
        ui.heading("Test Email");
        ui.label("Test delivery uses only the configured test recipients.");

        let assistants = match self.payslip_email_assistants() {
            Ok(assistants) => assistants.into_iter().collect::<Vec<_>>(),
            Err(error) => {
                ui.label(format!("Unable to load Personal Assistants: {}", error));
                return;
            }
        };

        if assistants.is_empty() {
            ui.label("No Personal Assistants are eligible for the selected payroll period or have unsent payroll documents.");
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
            crate::gui_controls::combo_box("test_timesheet_personal_assistant")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for assistant in &assistants {
                        crate::gui_controls::combo_value(
                            ui,
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
                .selected_period_assistants()?
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
            verify_timesheet_candidate_for_attachment(
                &self.application,
                assistant.id,
                &current_schedule,
                &attachment_path,
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
                .payslip_email_assistants()?
                .into_iter()
                .find(|assistant| assistant.id == personal_assistant_id)
                .ok_or("No unsent PA payroll documents are available for this test email, or the selected Personal Assistant was not found.")?;

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

            let current_schedule = self
                .operational_payroll_period
                .selected
                .as_ref()
                .map(|_| self.selected_operational_payroll_schedule())
                .transpose()?;
            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);
            let bundle = self.unsent_payslip_documents(&assistant, current_schedule.as_ref())?;

            let (subject, body) = crate::email_service::payroll_document_wording(
                &bundle,
                &self.application.context.config.payroll.email_subject_format,
                &self.application.context.config.payroll.payslip_email_body,
            );
            self.application.send_test_payslip_email(
                sender_email,
                pa_test_email,
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                current_schedule
                    .as_ref()
                    .filter(|_| bundle.email_types.contains(&"payslip")),
                &bundle.paths,
                body,
                subject,
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

    fn payslip_email_assistants(
        &self,
    ) -> Result<Vec<crate::models::PersonalAssistant>, Box<dyn std::error::Error>> {
        let mut assistants = if self.operational_payroll_period.selected.is_some() {
            self.selected_period_assistants()?
        } else {
            Vec::new()
        };
        for assistant in self.application.personal_assistant_repository.get_all()? {
            if !assistants
                .iter()
                .any(|selected| selected.id == assistant.id)
                && self.application.payroll_timesheet_email_repository.documents_for_pa(assistant.id)?.iter().any(|d| !matches!(d.delivery_state, crate::payroll_timesheet_email_repository::EmailDeliveryState::Sent { .. }))
            {
                assistants.push(assistant);
            }
        }
        Ok(assistants)
    }

    fn email_assistants(
        &self,
        kind: PayrollEmailKind,
    ) -> Result<Vec<crate::models::PersonalAssistant>, Box<dyn std::error::Error>> {
        match kind {
            PayrollEmailKind::Payslip => self.payslip_email_assistants(),
            PayrollEmailKind::Timesheet => self.selected_period_assistants(),
        }
    }

    fn selected_period_assistants(
        &self,
    ) -> Result<Vec<crate::models::PersonalAssistant>, Box<dyn std::error::Error>> {
        let schedule = self.selected_operational_payroll_schedule()?;
        let start = parse_date_checked(&schedule.first_week_commencing)
            .ok_or("Invalid payroll period start date.")?;
        let records = self
            .application
            .payroll_timesheet_repository
            .get_all_for_cycle(&schedule.payroll_year, schedule.cycle_number)?;
        self.application
            .personal_assistant_repository
            .get_all()?
            .into_iter()
            .filter_map(|assistant| {
                match assistant.eligible_for_period(
                    start,
                    start + chrono::Duration::days(27),
                    records
                        .iter()
                        .any(|record| record.personal_assistant_id == assistant.id),
                ) {
                    Ok(true) => Some(Ok(assistant)),
                    Ok(false) => None,
                    Err(error) => Some(Err(error.into())),
                }
            })
            .collect()
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
    ) -> Result<Option<PayrollSchedule>, Box<dyn std::error::Error>> {
        let Some(period) = &batch.payroll_period else {
            if matches!(batch.kind, PayrollEmailKind::Payslip)
                && self.operational_payroll_period.selected.is_none()
            {
                return Ok(None);
            }
            return Err("Payroll selection changed; restart the email batch.".into());
        };
        let schedule = self
            .application
            .payroll_schedule_repository
            .get_for_year_and_cycle(&period.key.payroll_year, period.key.cycle_number)?;
        validate_captured_email_batch_period(
            period,
            batch.operational_selection_revision,
            &self.operational_payroll_period,
            schedule.as_ref(),
        )?;
        schedule
            .map(Some)
            .ok_or_else(|| "The captured payroll period is unavailable.".into())
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
                .map(|s| {
                    crate::date_utils::calendar_text(
                        crate::date_utils::preference(ui),
                        &payroll_schedule_label(s),
                    )
                })
                .unwrap_or_else(|| "Select a payroll period".to_string());
            let mut selected_key = self.operational_payroll_period.selected.clone();

            crate::gui_controls::combo_box("operational_payroll_period")
                .height(220.0)
                .width(540.0)
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for schedule in &self.operational_payroll_schedules {
                        crate::gui_controls::combo_value(
                            ui,
                            &mut selected_key,
                            Some(operational_period_key(schedule)),
                            crate::date_utils::calendar_text(
                                crate::date_utils::preference(ui),
                                &payroll_schedule_label(schedule),
                            ),
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
        self.payroll_document_import = None;
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Payroll Documents (ZIP, PDF or Prep Sheet DOCX)",
                &["zip", "pdf", "docx"],
            )
            .pick_file()
        else {
            return;
        };
        match self.application.payroll_source_requires_period(&path) {
            Ok(false) => {
                self.import_selected_payroll_documents(&path, None);
                return;
            }
            Ok(true) => {}
            Err(error) => {
                self.status_message = format!("Payroll document classification failed: {error}");
                return;
            }
        }
        let result = (|| -> Result<PendingPayrollReturnImport, Box<dyn std::error::Error>> {
            let today = chrono::Local::now().date_naive();
            let current_schedule_id = self
                .application
                .resolve_payroll_schedule(today)
                .ok()
                .map(|schedule| schedule.id);
            let schedules = ordered_payroll_schedules(
                self.application.payroll_source_period_candidates(&path)?,
                current_schedule_id,
            );
            let selected_schedule_id =
                default_payroll_return_schedule_id(&schedules, current_schedule_id)
                    .ok_or("No payroll periods are available. Use Import Payroll Documents to select an individual Payroll Prep Sheet PDF first.")?;

            Ok(PendingPayrollReturnImport {
                source_path: path,
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
        egui::Window::new("Choose ordinary payslip period")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "Choose the payroll period for the ordinary payslips only. Other documents use their own information.",
                );

                let selected_text = selected_payroll_return_schedule(
                    &pending.schedules,
                    pending.selected_schedule_id,
                )
                .map(|s| crate::date_utils::calendar_text(crate::date_utils::preference(ui), &payroll_schedule_label(s)))
                .unwrap_or_else(|| "Select a payroll period".to_string());

                crate::gui_controls::combo_box("payroll_return_schedule_selection")
                    .width(540.0)
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for schedule in &pending.schedules {
                            crate::gui_controls::combo_value(
                                ui,
                                &mut pending.selected_schedule_id,
                                schedule.id,
                                crate::date_utils::calendar_text(crate::date_utils::preference(ui), &payroll_schedule_label(schedule)),
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
                                ui.label(crate::date_utils::calendar_text(crate::date_utils::preference(ui), &detail));
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
                    import = ui.button("Import Payroll Documents").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });

        if cancel {
            self.pending_payroll_return_import = None;
            self.status_message = "Payroll return import cancelled; no files were changed.".into();
        } else if import {
            let pending = self.pending_payroll_return_import.take().unwrap();
            let Some(schedule) =
                selected_payroll_return_schedule(&pending.schedules, pending.selected_schedule_id)
                    .cloned()
            else {
                self.status_message =
                    "Payroll document import failed: no payroll period was selected.".into();
                return;
            };
            self.import_selected_payroll_documents(&pending.source_path, Some(&schedule));
        }
    }

    fn import_selected_payroll_documents(
        &mut self,
        path: &std::path::Path,
        schedule: Option<&PayrollSchedule>,
    ) {
        self.payroll_document_import = None;
        self.file_status = None;
        match self.application.import_payroll_documents(path, schedule) {
            Ok(result) => {
                self.status_message = payroll_return_status_message(&result);
                self.payroll_document_import = Some(result);
                if let Err(error) = self.refresh_operational_payroll_schedules() {
                    self.operational_payroll_period_error = Some(format!(
                        "Could not refresh operational payroll periods: {error}"
                    ));
                }
            }
            Err(error) => {
                self.status_message = format!("Payroll document import failed: {error}");
                self.file_status = None;
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
        if !self.duplicate_ui.pending.is_empty() {
            self.duplicate_ui.show(ui, &self.application);
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Dashboard");

            self.draw_operational_payroll_period_selector(ui);

            ui.separator();

            let actions = draw_dashboard_workflow_actions(ui);

                if actions.enter_hours {
                    self.enter_hours_screen.reload();
                    navigate_to_enter_hours(&mut self.active_screen);
                }

                if actions.import_hours_csv {
                    match self.application.import_csv() {
                        Ok(summary) => {
                            self.status_message = if summary.has_failures() {
                                format!(
                                    "Import completed with {} refused and {} failed file(s). Review the Import Summary.",
                                    summary.files_refused, summary.files_failed
                                )
                            } else {
                                format!(
                                    "Import completed: {} file(s) succeeded, {} already imported.",
                                    summary.files_succeeded, summary.files_already_imported
                                )
                            };
                            self.file_status = Some((
                                self.status_message.clone(),
                                summary.archived_paths.clone(),
                            ));
                            if summary.files_succeeded>0 {
                                if let Err(e)=self.duplicate_ui.refresh(&self.application,None) {
                                    self.status_message=format!("Import retained; duplicate preflight failed: {e}");
                                }
                                self.duplicate_ui.pending.retain(|g|g.candidates.iter().any(|e| e.date().is_ok_and(|d|summary.affected_pa_dates.contains(&(e.pa,d)))));
                            }
                            self.last_import = Some(summary);
                        }

                        Err(error) => {
                            self.status_message = format!("Import failed: {}", error);
                            self.file_status = None;
                            self.last_import = None;
                        }
                    }
                }

                if actions.view_imported_hours {
                    match self.application.get_timesheets() {
                        Ok(entries) => {
                            self.timesheets = entries;
                            self.timesheet_sort = TimesheetSortState::default();
                            self.timesheet_pa_filter = None;
                            self.timesheet_all_cycles = false;

                            self.status_message =
                                format!("Loaded {} timesheets.", self.timesheets.len());
                        }

                        Err(error) => {
                            self.status_message = format!("Failed loading timesheets: {}", error);
                        }
                    }
                }
                if actions.prepare_timesheets {
                    self.payroll_timesheet_screen.reload();
                    self.active_screen = ActiveScreen::PayrollTimesheet;
                }

                if actions.generate_timesheets {
                    self.begin_generation();
                }

                if actions.email_timesheets {
                    self.begin_email_batch(PayrollEmailKind::Timesheet);
                }

                if actions.import_payroll_documents {
                    self.begin_payroll_return_import();
                }

                if actions.email_payslips {
                    self.begin_email_batch(PayrollEmailKind::Payslip);
                }

                if actions.view_payroll_schedule {
                    self.begin_view_payroll_schedule();
                }

            self.draw_generation_selection(ui);
            self.draw_production_report(ui);
            self.draw_additional_note_prompt(ui);

            ui.separator();

            let theme = self.application.context.config.theme;

            dashboard_status_panel(ui, 0, theme, |ui| {
                ui.heading("Import Summary");

                match &self.last_import {
                    Some(summary) => {
                        ui.label(format!("Files discovered: {}", summary.files_discovered));
                        ui.label(format!("Files attempted: {}", summary.files_processed));
                        ui.label(format!("Files succeeded: {}", summary.files_succeeded));
                        ui.label(format!(
                            "Files already imported: {}",
                            summary.files_already_imported
                        ));
                        ui.label(format!("Files refused: {}", summary.files_refused));
                        ui.label(format!("Files failed: {}", summary.files_failed));
                        ui.label(format!("Rows imported: {}", summary.rows_imported));
                        ui.label(format!("Rows skipped: {}", summary.rows_skipped));
                        for message in &summary.failure_messages {
                            ui.label(egui::RichText::new(message).color(egui::Color32::RED));
                        }
                        for path in &summary.orphaned_archives {
                            ui.horizontal(|ui| {
                                ui.label(format!("Recoverable orphan archive: {}", path.display()));
                                let _ = crate::folder_opener::button(ui, path);
                            });
                        }
                    }

                    None => {
                        ui.label("No import performed yet.");
                    }
                }
            });

            dashboard_status_panel(ui, 1, theme, |ui| {
                let mut open_error = None;
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Status: {}", self.status_message));
                    if let Some((message, paths)) = &self.file_status {
                        if message == &self.status_message {
                            for path in paths {
                                if open_error.is_none() {
                                    open_error = crate::folder_opener::button(ui, path);
                                }
                            }
                        }
                    }
                });
                if let Some(result) = &self.payroll_document_import {
                    ui.separator();
                    // This is independently labelled history, not a link attached
                    // to whichever unrelated status message was set most recently.
                    if let Some(error) = draw_payroll_document_import_result(ui, result) {
                        open_error = Some(error);
                    }
                }
                if let Some(error) = open_error {
                    self.status_message = error;
                    self.file_status = None;
                }
            });

            dashboard_status_panel(ui, 2, theme, |ui| {
                if let Some(selected_year) = draw_payroll_schedule_year_selector(
                    ui,
                    &self.payroll_schedule_years,
                    self.selected_payroll_schedule_year.as_deref(),
                ) {
                    self.load_payroll_schedule_year(&selected_year);
                }
                draw_payroll_schedule(ui, &self.payroll_schedules);
            });
            dashboard_status_panel(ui, 3, theme, |ui| {
                let scope = imported_cycle_scope(&self.application.payroll_schedule_repository, chrono::Local::now().date_naive());
                draw_timesheets(ui, &self.timesheets, &mut self.timesheet_sort, &mut self.timesheet_pa_filter, &mut self.timesheet_all_cycles, &scope);
            });
            dashboard_status_panel(ui, 4, theme, |ui| {
                self.draw_timesheet_email_status(ui);
            });
            self.draw_payroll_return_schedule_dialog(ui.ctx());
        });
    }

    fn draw_email_preview_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Email Preview");
        ui.label("Previews never send an email or update the sent status.");

        let assistants = match self.payslip_email_assistants() {
            Ok(assistants) => assistants.into_iter().collect::<Vec<_>>(),
            Err(error) => {
                ui.label(format!("Unable to load Personal Assistants: {}", error));
                return;
            }
        };

        if assistants.is_empty() {
            ui.label(
                "No Personal Assistants are relevant to the selected payroll period for preview.",
            );
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
            crate::gui_controls::combo_box("email_preview_personal_assistant")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for assistant in &assistants {
                        crate::gui_controls::combo_value(
                            ui,
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
            for path in &preview.additional_attachment_paths {
                ui.label(format!("Attachment: {}", path.display()));
            }
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
                .email_assistants(kind)?
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

            let current_schedule = if matches!(kind, PayrollEmailKind::Payslip)
                && self.operational_payroll_period.selected.is_none()
            {
                None
            } else {
                Some(self.selected_operational_payroll_schedule()?)
            };

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);
            let bundle = if matches!(kind, PayrollEmailKind::Payslip) {
                Some(self.unsent_payslip_documents(&assistant, current_schedule.as_ref())?)
            } else {
                None
            };
            let (attachment_path, body) = match kind {
                PayrollEmailKind::Timesheet => (
                    crate::payroll_file_naming::timesheet_path(
                        &crate::paths::expand_path(
                            &self.application.context.config.folders.pdf_output,
                        ),
                        &personal_assistant_name,
                        current_schedule
                            .as_ref()
                            .ok_or("Timesheets require a payroll period")?,
                    )?,
                    &self.application.context.config.payroll.timesheet_email_body,
                ),
                PayrollEmailKind::Payslip => (
                    bundle
                        .as_ref()
                        .and_then(|bundle| bundle.paths.first())
                        .cloned()
                        .ok_or("No unsent PA payroll documents are available for this period.")?,
                    &self.application.context.config.payroll.payslip_email_body,
                ),
            };

            if matches!(kind, PayrollEmailKind::Timesheet) {
                verify_timesheet_candidate_for_attachment(
                    &self.application,
                    assistant.id,
                    current_schedule
                        .as_ref()
                        .ok_or("Timesheets require a payroll period")?,
                    &attachment_path,
                )?;
            }

            let (subject, body) = bundle.as_ref().map_or(
                (
                    self.application
                        .context
                        .config
                        .payroll
                        .email_subject_format
                        .as_str(),
                    body.as_str(),
                ),
                |bundle| {
                    crate::email_service::payroll_document_wording(
                        bundle,
                        &self.application.context.config.payroll.email_subject_format,
                        body,
                    )
                },
            );
            let mut preview = self.application.preview_payroll_email(
                &payroll_department_email,
                employer_email,
                assistant.email.as_deref(),
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                current_schedule.as_ref().filter(|_| {
                    bundle
                        .as_ref()
                        .is_none_or(|b| b.email_types.contains(&"payslip"))
                }),
                &attachment_path,
                body,
                subject,
                self.additional_notes_by_personal_assistant
                    .get(&assistant.id)
                    .map(String::as_str),
                employer.email_signature.as_deref(),
            )?;
            if let Some(bundle) = bundle {
                preview.additional_attachment_paths = bundle.paths.into_iter().skip(1).collect();
            }
            Ok(preview)
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

    fn unsent_payslip_documents<'a>(
        &self,
        assistant: &crate::models::PersonalAssistant,
        schedule: impl Into<Option<&'a PayrollSchedule>>,
    ) -> Result<crate::payslip_delivery_service::PayslipEmailBundle, Box<dyn std::error::Error>>
    {
        let root =
            crate::paths::expand_path(&self.application.context.config.folders.payslip_folder);
        let name = format!("{} {}", assistant.first_name, assistant.surname);
        let schedule = schedule.into();
        let payslip = schedule
            .map(|s| crate::payroll_file_naming::payslip_path(&root, &name, s))
            .transpose()?;
        crate::payslip_delivery_service::select_payroll_documents(
            &self.application.payroll_timesheet_email_repository,
            assistant.id,
            schedule.zip(payslip.as_deref()).map(|(s, path)| {
                (
                    crate::payslip_delivery_service::PayslipDeliveryIdentity {
                        personal_assistant_id: assistant.id,
                        payroll_year: &s.payroll_year,
                        cycle_number: s.cycle_number,
                    },
                    path,
                )
            }),
        )
    }

    fn email_payslips(
        &self,
        schedule: Option<&PayrollSchedule>,
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

        let required_assistants = if schedule.is_some() {
            self.selected_period_assistants()?
        } else {
            Vec::new()
        };
        let assistants = self.payslip_email_assistants()?;

        let mut sent = 0usize;

        for assistant in &assistants {
            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);

            let bundle = self.unsent_payslip_documents(assistant, schedule)?;
            let Some((attachment, additional_attachments)) = bundle.paths.split_first() else {
                continue;
            };
            let personal_assistant_email = assistant.email.as_deref();
            let (subject, body) = crate::email_service::payroll_document_wording(
                &bundle,
                &self.application.context.config.payroll.email_subject_format,
                &self.application.context.config.payroll.payslip_email_body,
            );

            let attempted_at = chrono::Local::now().to_rfc3339();
            crate::payslip_delivery_service::send_payroll_bundle(
                &self.application.payroll_timesheet_email_repository,
                assistant.id,
                schedule.map(
                    |s| crate::payslip_delivery_service::PayslipDeliveryIdentity {
                        personal_assistant_id: assistant.id,
                        payroll_year: &s.payroll_year,
                        cycle_number: s.cycle_number,
                    },
                ),
                &bundle,
                &attempted_at,
                || {
                    self.application.send_payroll_email_with_attachments(
                        payroll_department_email,
                        employer_email,
                        personal_assistant_email,
                        &personal_assistant_name,
                        assistant.date_of_birth.as_deref(),
                        assistant.national_insurance_number.as_deref(),
                        schedule.filter(|_| bundle.email_types.contains(&"payslip")),
                        attachment,
                        body,
                        subject,
                        self.additional_notes_by_personal_assistant
                            .get(&assistant.id)
                            .map(String::as_str),
                        employer.email_signature.as_deref(),
                        additional_attachments,
                    )
                },
            )?;

            sent += 1;
        }

        if let Some(schedule) = schedule {
            let required = required_assistants
                .iter()
                .map(
                    |assistant| crate::payslip_delivery_service::PayslipDeliveryIdentity {
                        personal_assistant_id: assistant.id,
                        payroll_year: &schedule.payroll_year,
                        cycle_number: schedule.cycle_number,
                    },
                )
                .collect::<Vec<_>>();
            if !required.is_empty() {
                crate::payslip_delivery_service::mark_schedule_sent_if_complete(
                    &self.application.payroll_timesheet_email_repository,
                    &self.application.payroll_schedule_repository,
                    schedule.id,
                    &required,
                )?;
            }
        }
        Ok(sent)
    }

    fn email_timesheet(
        &self,
        schedule: &PayrollSchedule,
        assistant: &crate::models::PersonalAssistant,
    ) -> Result<bool, Box<dyn std::error::Error>> {
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

        let pdf_output_folder =
            crate::paths::expand_path(&self.application.context.config.folders.pdf_output);

        let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);

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

        Ok(true)
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

        let assistants = match self.selected_period_assistants() {
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

                for assistant in &assistants {
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

        ui.separator();
        ui.heading("Payslip Delivery Status");
        egui::Grid::new("payslip_email_status")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Personal Assistant");
                ui.label("Status");
                ui.label("Recorded");
                ui.end_row();

                for assistant in assistants {
                    let name = format!("{} {}", assistant.first_name, assistant.surname);
                    let status = self
                        .application
                        .payroll_timesheet_email_repository
                        .get_for_pa_and_cycle(
                            assistant.id,
                            &payroll_year,
                            current_schedule.cycle_number,
                            "payslip",
                        );
                    ui.label(name);
                    match status {
                        Ok(Some(status)) => match status.delivery_state {
                            crate::payroll_timesheet_email_repository::EmailDeliveryState::Sent {
                                sent_at,
                            } => {
                                ui.label("Sent");
                                ui.label(display_email_status_time(&sent_at));
                            }
                            crate::payroll_timesheet_email_repository::EmailDeliveryState::Indeterminate {
                                attempted_at,
                            } => {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    "Delivery uncertain — do not resend",
                                );
                                ui.label(display_email_status_time(&attempted_at));
                            }
                            crate::payroll_timesheet_email_repository::EmailDeliveryState::Unsent => {
                                ui.label("Not sent");
                                ui.label("");
                            }
                        },
                        Ok(None) => {
                            ui.label("Not sent");
                            ui.label("");
                        }
                        Err(error) => {
                            ui.label("Error");
                            ui.label(error.to_string());
                        }
                    }
                    ui.end_row();
                }
            });
    }

    fn generate_payroll_timesheet(
        &self,
        current_schedule: &PayrollSchedule,
        assistant: &crate::models::PersonalAssistant,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let employers = self.application.employer_repository.get_all()?;

        let employer = employers
            .into_iter()
            .next()
            .ok_or("No employer has been configured.")?;

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

        let output_dir =
            crate::paths::expand_path(&self.application.context.config.folders.pdf_output);

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

        crate::payroll_evidence::lifecycle::ensure_generatable(
            &crate::payroll_evidence::open(&self.application)?,
            payroll_timesheet.id,
        )?;
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
        let historical_backfill = self
            .application
            .payroll_worked_item_repository
            .get_manual_adjustments(payroll_timesheet.id)?
            .iter()
            .any(|adjustment| {
                crate::historical_adjustment_compatibility::is_historical_adjustment_reason(
                    adjustment.reason.as_deref(),
                )
            });
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
        } else if historical_backfill && payroll_timesheet.previous_cycle_hours.is_some() {
            Some(crate::pay_rate_allocation::PreviousCycleContext {
                payroll_timesheet_id: payroll_timesheet.id,
                week_three_start: week_dates[0],
                legacy_adjustment_minutes: payroll_timesheet
                    .previous_cycle_hours
                    .map(|hours| (hours * 60.0).round() as i64)
                    .unwrap_or(0),
            })
        } else {
            None
        };
        let reconciled = crate::payroll_evidence::reconciliation::calculate_for_generation(
            &self.application,
            &payroll_timesheet,
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
                .map(|hours| hours.summary_value().to_string())
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
            crate::pay_rate_allocation::format_total_minutes(reconciled.week_totals_minutes[index])
        });

        let annual_leave_hours = [
            format_pdf_hours(payroll_weeks[0].annual_leave_hours),
            format_pdf_hours(payroll_weeks[1].annual_leave_hours),
            format_pdf_hours(payroll_weeks[2].annual_leave_hours),
            format_pdf_hours(payroll_weeks[3].annual_leave_hours),
        ];

        let sickness_periods = crate::pdf_generator::sickness_periods_for_weeks(
            &crate::sickness_period_repository::SicknessPeriodRepository::new(
                crate::payroll_evidence::open(&self.application)?,
            ),
            assistant.id,
            &week_dates,
        )?;

        let public_holidays = self
            .application
            .payroll_timesheet_repository
            .get_public_holidays(payroll_timesheet.id)?;

        validate_public_holidays_for_generation(
            &payroll_weeks,
            &public_holidays,
            snapshot_state_for_generation(
                &self.application.payroll_worked_item_repository,
                payroll_timesheet.id,
            )?,
        )?;
        let public_holiday_entries = std::array::from_fn(|index| {
            public_holidays
                .iter()
                .filter(|holiday| holiday.week_number == (index + 1) as i64 && holiday.hours > 0.0)
                .map(|holiday| PublicHolidayPdfEntry {
                    hours: format_pdf_hours(holiday.hours),
                    date: holiday.holiday_date.clone(),
                })
                .collect::<Vec<_>>()
        });

        let travel_miles = [
            format_pdf_hours(payroll_weeks[0].travel_miles),
            format_pdf_hours(payroll_weeks[1].travel_miles),
            format_pdf_hours(payroll_weeks[2].travel_miles),
            format_pdf_hours(payroll_weeks[3].travel_miles),
        ];

        let previous_cycle_hours = (reconciled.previous_cycle_minutes != 0).then(|| {
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
            payroll_department_notes: &payroll_timesheet.payroll_department_notes,
            schedule: &current_schedule,
            employer_name: &employer.name,

            personal_assistant_name: &personal_assistant_name,

            national_insurance_number: assistant.national_insurance_number.as_deref().unwrap_or(""),

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

            sickness_periods,

            public_holidays: public_holiday_entries,

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
                    &self.application.context.config.payroll,
                )
            },
        )?;

        Ok(true)
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
        return Err("The operational payroll period selection changed after this payroll operation began. Cancel this operation and start it again for the intended payroll period.".into());
    }

    let stored_schedule = stored_schedule.ok_or_else(|| {
        "The payroll period captured for this payroll operation no longer exists. Cancel this operation and select an available payroll period."
    })?;
    if !crate::date_utils::same(
        &stored_schedule.first_week_commencing,
        &captured.first_week_commencing,
    ) || !crate::date_utils::same(&stored_schedule.pay_date, &captured.pay_date)
    {
        return Err("The payroll schedule dates changed after this payroll operation began. Nothing was processed; cancel this operation and start it again using the updated payroll period.".into());
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

fn payroll_return_status_message(result: &crate::archive::PayrollReturnImportResult) -> String {
    if result.publication_failure.is_some() || !result.prep_sheet_failures.is_empty() {
        "Payroll document import completed with issues. See the result below.".into()
    } else {
        "Payroll document import completed. See the result below.".into()
    }
}

fn payroll_document_count_rows(
    result: &crate::archive::PayrollReturnImportResult,
) -> [(&'static str, usize, usize); 4] {
    [
        (
            "Cycle-associated ordinary payslips",
            result.payslips_imported,
            result.payslips_already_present,
        ),
        (
            "Historical / unassociated payslip archives",
            result.archival_payslips_imported,
            result.archival_payslips_already_present,
        ),
        (
            "P60/P45 supplements",
            result.supplements_imported,
            result.supplements_already_present,
        ),
        (
            "Information documents (includes Payroll Prep Sheets)",
            result.information_files_imported,
            result.information_files_already_present,
        ),
    ]
}

fn draw_payroll_document_import_result(
    ui: &mut egui::Ui,
    result: &crate::archive::PayrollReturnImportResult,
) -> Option<String> {
    ui.heading("Last payroll-document import");
    for failure in result
        .publication_failure
        .iter()
        .chain(result.prep_sheet_failures.iter())
    {
        ui.colored_label(ui.visuals().error_fg_color, failure);
    }
    egui::Grid::new("payroll_document_import_counts").show(ui, |ui| {
        ui.label("Files");
        ui.label("Imported / stored");
        ui.label("Already present");
        ui.end_row();
        for (label, imported, present) in payroll_document_count_rows(result) {
            ui.label(label);
            ui.label(imported.to_string());
            ui.label(present.to_string());
            ui.end_row();
        }
    });
    ui.label(format!(
        "Entries skipped: {} · Payroll schedule entries imported: {}",
        result.files_skipped, result.schedule_entries_imported
    ));
    ui.label("File counts describe storage, not overall import success. Schedule entries may come from parsing an already-present prep sheet.");
    if result.archival_payslips_imported > 0 || result.archival_payslips_already_present > 0 {
        ui.label("Unassociated payslips are archived only: not automatically email-eligible and do not settle payroll.");
    }
    let mut open_error = None;
    egui::CollapsingHeader::new("Details")
        .id_salt("payroll_document_import_details")
        .default_open(false)
        .show(ui, |ui| {
            for detail in &result.details {
                ui.label(detail);
            }
            for (heading, paths) in [
                ("Published files", &result.published_paths),
                (
                    "Payroll Prep Sheets available for parsing (new or already present)",
                    &result.prep_sheet_paths,
                ),
            ] {
                if !paths.is_empty() {
                    ui.strong(heading);
                    for path in paths {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(path.display().to_string());
                            if let Some(error) = crate::folder_opener::button(ui, path) {
                                open_error = Some(error);
                            }
                        });
                    }
                }
            }
        });
    open_error
}

fn display_email_status_time(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date_time| {
            date_time
                .with_timezone(&chrono::Local)
                .format("%d %b %Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
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

fn snapshot_state_for_generation(
    repository: &PayrollWorkedItemRepository,
    payroll_timesheet_id: i64,
) -> rusqlite::Result<Option<SnapshotState>> {
    Ok(repository
        .snapshot_metadata(payroll_timesheet_id)?
        .map(|metadata| metadata.state))
}

fn validate_public_holidays_for_generation(
    weeks: &[PayrollTimesheetWeek],
    holidays: &[PayrollTimesheetPublicHoliday],
    snapshot_state: Option<SnapshotState>,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(
        snapshot_state,
        Some(SnapshotState::Submitted | SnapshotState::Indeterminate)
    ) {
        return Ok(());
    }
    for holiday in holidays {
        if !holiday.hours.is_finite() || holiday.hours < 0.0 {
            return Err(format!(
                "Public-holiday hours for {} must be a finite value of zero or more.",
                holiday.holiday_date
            )
            .into());
        }
    }
    for week in weeks {
        let detail_total: f64 = holidays
            .iter()
            .filter(|holiday| holiday.week_number == week.week_number)
            .map(|holiday| holiday.hours)
            .sum();
        if (week.public_holiday_hours - detail_total).abs() > 0.000_001 {
            return Err(format!(
                "Public-holiday data for payroll week {} is inconsistent: the compatibility aggregate is {}, while dated entries total {}. Review Payroll Timesheet Preparation before generating.",
                week.week_number,
                format_pdf_hours(week.public_holiday_hours),
                format_pdf_hours(detail_total)
            )
            .into());
        }
    }
    Ok(())
}

fn verify_timesheet_candidate_for_attachment(
    application: &Application,
    personal_assistant_id: i64,
    schedule: &PayrollSchedule,
    attachment_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let payroll_timesheet = application
        .payroll_timesheet_repository
        .get_for_cycle_and_pa(
            &schedule.payroll_year,
            schedule.cycle_number,
            personal_assistant_id,
        )?
        .ok_or("No Payroll Timesheet Preparation record exists for this Personal Assistant.")?;
    crate::payroll_snapshot_service::verify_preview_or_test_attachment(
        &application.payroll_worked_item_repository,
        payroll_timesheet.id,
        attachment_path,
    )?;
    Ok(())
}

fn navigate_to_enter_hours(active_screen: &mut ActiveScreen) {
    *active_screen = ActiveScreen::EnterHours;
}

const WORKFLOW_COLUMN_WIDTHS: [f32; 3] = [225.0, 225.0, 185.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DashboardWorkflowLayout {
    AlignedColumns,
    WrappedRows,
}

fn dashboard_workflow_layout(
    available_width: f32,
    horizontal_spacing: f32,
) -> DashboardWorkflowLayout {
    let aligned_width = WORKFLOW_COLUMN_WIDTHS.iter().sum::<f32>() + horizontal_spacing * 2.0;
    if available_width >= aligned_width {
        DashboardWorkflowLayout::AlignedColumns
    } else {
        DashboardWorkflowLayout::WrappedRows
    }
}

fn draw_dashboard_workflow_actions(ui: &mut egui::Ui) -> DashboardWorkflowActions {
    let mut actions = DashboardWorkflowActions::default();
    let available_width = ui.available_width();
    let spacing = ui.spacing().item_spacing.x;

    match dashboard_workflow_layout(available_width, spacing) {
        DashboardWorkflowLayout::AlignedColumns => {
            egui::Grid::new("dashboard_workflow_actions")
                .spacing([spacing, ui.spacing().item_spacing.y])
                .show(ui, |ui| {
                    actions.enter_hours = workflow_button(ui, 0, "Enter Hours/Shifts");
                    actions.import_hours_csv = workflow_button(ui, 1, "Import Hours CSV");
                    actions.view_imported_hours = workflow_button(ui, 2, "View Imported Hours");
                    ui.end_row();

                    actions.prepare_timesheets =
                        workflow_button(ui, 0, "Payroll Timesheet Preparation");
                    actions.generate_timesheets =
                        workflow_button(ui, 1, "Generate Payroll Timesheets");
                    actions.email_timesheets = workflow_button(ui, 2, "Email Payroll Timesheets");
                    ui.end_row();

                    actions.import_payroll_documents =
                        workflow_button(ui, 0, "Import Payroll Documents");
                    actions.email_payslips = workflow_button(ui, 1, "Email Payslips");
                    actions.view_payroll_schedule = workflow_button(ui, 2, "View Payroll Schedule");
                    ui.end_row();
                });
        }
        DashboardWorkflowLayout::WrappedRows => {
            let widths = WORKFLOW_COLUMN_WIDTHS.map(|width| width.min(available_width));
            ui.horizontal_wrapped(|ui| {
                actions.enter_hours = sized_workflow_button(ui, widths[0], "Enter Hours/Shifts");
                actions.import_hours_csv = sized_workflow_button(ui, widths[1], "Import Hours CSV");
                actions.view_imported_hours =
                    sized_workflow_button(ui, widths[2], "View Imported Hours");
            });
            ui.horizontal_wrapped(|ui| {
                actions.prepare_timesheets =
                    sized_workflow_button(ui, widths[0], "Payroll Timesheet Preparation");
                actions.generate_timesheets =
                    sized_workflow_button(ui, widths[1], "Generate Payroll Timesheets");
                actions.email_timesheets =
                    sized_workflow_button(ui, widths[2], "Email Payroll Timesheets");
            });
            ui.horizontal_wrapped(|ui| {
                actions.import_payroll_documents =
                    sized_workflow_button(ui, widths[0], "Import Payroll Documents");
                actions.email_payslips = sized_workflow_button(ui, widths[1], "Email Payslips");
                actions.view_payroll_schedule =
                    sized_workflow_button(ui, widths[2], "View Payroll Schedule");
            });
        }
    }

    actions
}

fn workflow_button(ui: &mut egui::Ui, column: usize, label: &str) -> bool {
    sized_workflow_button(ui, WORKFLOW_COLUMN_WIDTHS[column], label)
}

fn sized_workflow_button(ui: &mut egui::Ui, width: f32, label: &str) -> bool {
    ui.add_sized(
        [width, ui.spacing().interact_size.y],
        egui::Button::new(label),
    )
    .clicked()
}

fn dashboard_status_panel(
    ui: &mut egui::Ui,
    displayed_index: usize,
    theme: ApplicationTheme,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let style = dashboard_status_panel_style(displayed_index, theme, ui.visuals());
    egui::Frame::new()
        .fill(style.fill)
        .stroke(style.stroke)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .outer_margin(egui::Margin::symmetric(0, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

fn dashboard_status_panel_style(
    displayed_index: usize,
    theme: ApplicationTheme,
    visuals: &egui::Visuals,
) -> DashboardStatusPanelStyle {
    if theme == ApplicationTheme::AccessibleHighContrast {
        return DashboardStatusPanelStyle {
            fill: visuals.panel_fill,
            stroke: visuals.widgets.noninteractive.bg_stroke,
        };
    }

    let faint_weight = if displayed_index % 2 == 0 { 20 } else { 35 };
    DashboardStatusPanelStyle {
        fill: blend_theme_colors(visuals.panel_fill, visuals.faint_bg_color, faint_weight),
        stroke: egui::Stroke::NONE,
    }
}

fn blend_theme_colors(
    base: egui::Color32,
    accent: egui::Color32,
    accent_percent: u16,
) -> egui::Color32 {
    let mix = |base: u8, accent: u8| {
        let base_weight = 100 - accent_percent;
        ((u16::from(base) * base_weight + u16::from(accent) * accent_percent) / 100) as u8
    };
    egui::Color32::from_rgb(
        mix(base.r(), accent.r()),
        mix(base.g(), accent.g()),
        mix(base.b(), accent.b()),
    )
}

fn parse_date_checked(value: &str) -> Option<chrono::NaiveDate> {
    crate::date_utils::parse_legacy(value).ok()
}

fn format_date(date: chrono::NaiveDate) -> String {
    crate::date_utils::uk(date)
}

// Names need the most room; timestamps include strings such as
// "30 January 2026 at 09:00:00" and retain their existing presentation.
// Monetary columns stay compact, with a little more room for total amounts.
const TIMESHEET_COLUMN_WIDTHS: [f32; 6] = [240.0, 210.0, 210.0, 85.0, 65.0, 80.0];

fn draw_timesheets(
    ui: &mut egui::Ui,
    timesheets: &[TimesheetEntry],
    sort_state: &mut TimesheetSortState,
    pa_filter: &mut Option<String>,
    all_cycles: &mut bool,
    scope: &Result<(chrono::NaiveDate, chrono::NaiveDate), String>,
) {
    ui.separator();

    ui.heading("Timesheets");

    if timesheets.is_empty() {
        ui.label("No timesheets loaded.");
        return;
    }

    let pa_names = imported_pa_names(timesheets);
    if pa_filter
        .as_ref()
        .is_some_and(|name| !pa_names.contains(&name.as_str()))
    {
        *pa_filter = None;
    }
    ui.label(format!(
        "PA filter: {}",
        pa_filter.as_deref().unwrap_or("All PAs")
    ));

    match scope {
        Ok((start, end)) if !*all_cycles => {
            ui.label(format!(
                "Cycle scope: current and previous payroll cycles — {} to {} (shift start date)",
                start.format("%d/%m/%Y"),
                end.format("%d/%m/%Y")
            ));
        }
        Ok(_) => {
            ui.label("Cycle scope: all cycles");
        }
        Err(error) => {
            ui.label(format!(
                "Cycle scope: all cycles — schedule information unavailable: {error}"
            ));
        }
    }
    if !*all_cycles
        && scope.is_ok()
        && timesheets
            .iter()
            .any(|entry| crate::csv_import::parse_supported_timestamp(&entry.start_time).is_none())
    {
        ui.label("Entries with unrecognised start dates are also shown so they are not hidden.");
    }

    if sort_state.column == TimesheetSortColumn::PaName {
        ui.horizontal(|ui| {
            ui.label("Dates within each PA:");
            ui.selectable_value(
                &mut sort_state.pa_date_direction,
                SortDirection::Descending,
                "Newest first",
            );
            ui.selectable_value(
                &mut sort_state.pa_date_direction,
                SortDirection::Ascending,
                "Oldest first",
            );
        });
    }

    let visible_count = filtered_timesheets(timesheets, *sort_state, pa_filter.as_deref())
        .into_iter()
        .filter(|entry| imported_entry_in_scope(entry, *all_cycles, scope))
        .count();
    ui.label(format!(
        "Showing {visible_count} of {} loaded entries",
        timesheets.len()
    ));

    egui::Grid::new("timesheet_grid")
        .num_columns(6)
        .striped(true)
        .show(ui, |ui| {
            timesheet_control_heading(
                ui,
                sort_state,
                TimesheetSortColumn::PaName,
                "PA Name",
                28.0,
                |ui| {
                    let menu = ui.menu_button("   ", |ui| {
                        if ui
                            .selectable_label(pa_filter.is_none(), "All PAs")
                            .clicked()
                        {
                            *pa_filter = None;
                            ui.close();
                        }
                        for name in &pa_names {
                            if ui
                                .selectable_label(pa_filter.as_deref() == Some(*name), *name)
                                .clicked()
                            {
                                *pa_filter = Some((*name).to_owned());
                                ui.close();
                            }
                        }
                    });
                    let rect = egui::Rect::from_center_size(
                        menu.response.rect.center(),
                        egui::vec2(12.0, 12.0),
                    );
                    let color = if pa_filter.is_some() {
                        ui.visuals().selection.stroke.color
                    } else {
                        ui.visuals().text_color()
                    };
                    // Draw a funnel without depending on an icon font.
                    ui.painter().add(egui::Shape::closed_line(
                        vec![
                            rect.left_top(),
                            rect.right_top(),
                            rect.center() + egui::vec2(2.0, 0.0),
                            rect.center() + egui::vec2(2.0, 6.0),
                            rect.center() + egui::vec2(-2.0, 6.0),
                            rect.center() + egui::vec2(-2.0, 0.0),
                        ],
                        egui::Stroke::new(1.0_f32, color),
                    ));
                    menu.response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            "Filter imported hours by PA",
                        )
                    });
                    menu.response.on_hover_text("Filter imported hours by PA");
                },
            );
            timesheet_control_heading(
                ui,
                sort_state,
                TimesheetSortColumn::Start,
                "Start",
                133.0,
                |ui| {
                    ui.checkbox(all_cycles, "Include all cycles");
                },
            );
            timesheet_sort_heading(ui, sort_state, TimesheetSortColumn::End, "End");
            timesheet_sort_heading(ui, sort_state, TimesheetSortColumn::Worked, "Worked");
            timesheet_sort_heading(ui, sort_state, TimesheetSortColumn::Rate, "Rate");
            timesheet_sort_heading(ui, sort_state, TimesheetSortColumn::Amount, "Amount");
            ui.end_row();

            for entry in filtered_timesheets(timesheets, *sort_state, pa_filter.as_deref())
                .into_iter()
                .filter(|entry| imported_entry_in_scope(entry, *all_cycles, scope))
            {
                for (text, width) in [
                    entry.pa_name.clone(),
                    entry.start_time.clone(),
                    entry.end_time.clone(),
                    format_worked_time(entry.worked_minutes),
                    format!("£{:.2}", entry.hourly_rate),
                    format!("£{:.2}", entry.amount),
                ]
                .into_iter()
                .zip(TIMESHEET_COLUMN_WIDTHS)
                {
                    // add_sized uses a centered-and-justified layout. Use an
                    // explicit left-to-right cell instead, with the same text
                    // inset as the header button.
                    let padding = ui.spacing().button_padding.x;
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, ui.spacing().interact_size.y),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.add_space(padding);
                            ui.add(egui::Label::new(text).wrap());
                        },
                    );
                }
                ui.end_row();
            }
        });
}

fn imported_cycle_scope(
    repository: &crate::payroll_schedule_repository::PayrollScheduleRepository,
    today: chrono::NaiveDate,
) -> Result<(chrono::NaiveDate, chrono::NaiveDate), String> {
    let current = repository
        .resolve_for_date(today)
        .map_err(|error| error.to_string())?;
    let previous = repository
        .resolve_previous(&current)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No immediately preceding payroll cycle is available.".to_owned())?;
    let start =
        parse_date_checked(&previous.first_week_commencing).ok_or("Invalid previous cycle date")?;
    let end = parse_date_checked(&current.first_week_commencing)
        .ok_or("Invalid current cycle date")?
        + chrono::Duration::days(27);
    Ok((start, end))
}

fn imported_entry_in_scope(
    entry: &TimesheetEntry,
    all_cycles: bool,
    scope: &Result<(chrono::NaiveDate, chrono::NaiveDate), String>,
) -> bool {
    if all_cycles {
        return true;
    }
    match (
        scope,
        crate::csv_import::parse_supported_timestamp(&entry.start_time),
    ) {
        (Ok((start, end)), Some(date)) => *start <= date.date() && date.date() <= *end,
        _ => true,
    }
}

// The heading has one outer rectangle, with disjoint sort and control hit areas.
fn timesheet_control_heading(
    ui: &mut egui::Ui,
    sort_state: &mut TimesheetSortState,
    column: TimesheetSortColumn,
    label: &str,
    control_width: f32,
    control: impl FnOnce(&mut egui::Ui),
) {
    // Allocate exactly one parent grid cell. All heading widgets must live in
    // this child UI: put/scope_builder also advance a grid when called on it.
    ui.allocate_ui_with_layout(
        egui::vec2(
            TIMESHEET_COLUMN_WIDTHS[column as usize],
            ui.spacing().interact_size.y,
        ),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(
                    TIMESHEET_COLUMN_WIDTHS[column as usize],
                    ui.spacing().interact_size.y,
                ),
                egui::Sense::hover(),
            );
            let visuals = &ui.visuals().widgets.inactive;
            ui.painter().rect(
                rect,
                visuals.corner_radius,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
            let split = rect.right() - control_width - 4.0;
            let sort_rect = egui::Rect::from_min_max(rect.min, egui::pos2(split, rect.bottom()));
            let control_rect = egui::Rect::from_min_max(
                egui::pos2(split + 2.0, rect.top()),
                rect.max - egui::vec2(2.0, 0.0),
            );
            let indicator = if sort_state.column == column {
                match sort_state.direction {
                    SortDirection::Ascending => " ▲",
                    SortDirection::Descending => " ▼",
                }
            } else {
                ""
            };
            if ui
                .put(
                    sort_rect,
                    egui::Button::new((format!("{label}{indicator}"), egui::Atom::grow()))
                        .frame(false),
                )
                .clicked()
            {
                sort_state.select(column);
            }
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(control_rect)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
                control,
            );
        },
    );
}

fn timesheet_sort_heading(
    ui: &mut egui::Ui,
    sort_state: &mut TimesheetSortState,
    column: TimesheetSortColumn,
    label: &str,
) {
    let indicator = if sort_state.column == column {
        match sort_state.direction {
            SortDirection::Ascending => " ▲",
            SortDirection::Descending => " ▼",
        }
    } else {
        ""
    };
    if ui
        .add_sized(
            [
                TIMESHEET_COLUMN_WIDTHS[column as usize],
                ui.spacing().interact_size.y,
            ],
            // The trailing growing atom consumes spare width to the right of
            // the text, retaining the native full-cell button interaction and
            // theme feedback without centering the caption.
            egui::Button::new((format!("{label}{indicator}"), egui::Atom::grow()))
                .wrap_mode(egui::TextWrapMode::Extend),
        )
        .clicked()
    {
        sort_state.select(column);
    }
}

fn imported_pa_names(timesheets: &[TimesheetEntry]) -> Vec<&str> {
    let mut names: Vec<_> = timesheets
        .iter()
        .map(|entry| entry.pa_name.as_str())
        .collect();
    names.sort_by_cached_key(|name| (name.to_lowercase(), *name));
    names.dedup();
    names
}

fn filtered_timesheets<'a>(
    timesheets: &'a [TimesheetEntry],
    sort_state: TimesheetSortState,
    pa_filter: Option<&str>,
) -> Vec<&'a TimesheetEntry> {
    sorted_timesheets(timesheets, sort_state)
        .into_iter()
        .filter(|entry| pa_filter.is_none_or(|name| entry.pa_name == name))
        .collect()
}

fn sorted_timesheets(
    timesheets: &[TimesheetEntry],
    sort_state: TimesheetSortState,
) -> Vec<&TimesheetEntry> {
    let mut sorted: Vec<_> = timesheets.iter().collect();
    sorted.sort_by(|left, right| {
        let ordering = match sort_state.column {
            TimesheetSortColumn::PaName => directed_ordering(
                left.pa_name
                    .to_lowercase()
                    .cmp(&right.pa_name.to_lowercase()),
                sort_state.direction,
            )
            .then_with(|| {
                compare_timesheet_datetimes(
                    &left.start_time,
                    &right.start_time,
                    sort_state.pa_date_direction,
                )
            }),
            TimesheetSortColumn::Start => compare_timesheet_datetimes(
                &left.start_time,
                &right.start_time,
                sort_state.direction,
            ),
            TimesheetSortColumn::End => {
                compare_timesheet_datetimes(&left.end_time, &right.end_time, sort_state.direction)
            }
            TimesheetSortColumn::Worked => directed_ordering(
                left.worked_minutes.cmp(&right.worked_minutes),
                sort_state.direction,
            ),
            TimesheetSortColumn::Rate => {
                compare_timesheet_numbers(left.hourly_rate, right.hourly_rate, sort_state.direction)
            }
            TimesheetSortColumn::Amount => {
                compare_timesheet_numbers(left.amount, right.amount, sort_state.direction)
            }
        };
        ordering.then_with(|| left.id.cmp(&right.id))
    });
    sorted
}

fn directed_ordering(ordering: std::cmp::Ordering, direction: SortDirection) -> std::cmp::Ordering {
    match direction {
        SortDirection::Ascending => ordering,
        SortDirection::Descending => ordering.reverse(),
    }
}

fn compare_timesheet_datetimes(
    left: &str,
    right: &str,
    direction: SortDirection,
) -> std::cmp::Ordering {
    match (
        crate::csv_import::parse_supported_timestamp(left),
        crate::csv_import::parse_supported_timestamp(right),
    ) {
        (Some(left), Some(right)) => directed_ordering(left.cmp(&right), direction),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.cmp(right),
    }
}

fn compare_timesheet_numbers(
    left: f64,
    right: f64,
    direction: SortDirection,
) -> std::cmp::Ordering {
    match (left.is_finite(), right.is_finite()) {
        (true, true) => directed_ordering(left.total_cmp(&right), direction),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => left.total_cmp(&right),
    }
}

#[cfg(test)]
mod timesheet_sort_tests {
    use super::*;

    fn entry(
        id: i64,
        name: &str,
        start: &str,
        end: &str,
        worked: i64,
        rate: f64,
        amount: f64,
    ) -> TimesheetEntry {
        TimesheetEntry {
            id,
            pa_name: name.to_string(),
            personal_assistant_id: Some(id),
            start_time: start.to_string(),
            end_time: end.to_string(),
            break_minutes: 0,
            worked_minutes: worked,
            hourly_rate: rate,
            amount,
            notes: None,
        }
    }

    fn ids(entries: &[TimesheetEntry], state: TimesheetSortState) -> Vec<i64> {
        sorted_timesheets(entries, state)
            .into_iter()
            .map(|entry| entry.id)
            .collect()
    }

    fn entries() -> Vec<TimesheetEntry> {
        vec![
            entry(
                1,
                "zoe",
                "30 January 2026 at 09:00:00",
                "30 January 2026 at 10:00:00",
                90,
                15.0,
                22.5,
            ),
            entry(
                2,
                "Alice",
                "2 February 2026 at 09:00:00",
                "2 February 2026 at 12:00:00",
                30,
                12.0,
                6.0,
            ),
            entry(
                3,
                "bob",
                "1 February 2026 at 09:00:00",
                "1 February 2026 at 11:00:00",
                60,
                14.0,
                14.0,
            ),
        ]
    }

    #[test]
    fn default_is_newest_start_first_using_chronological_values() {
        assert_eq!(
            ids(&entries(), TimesheetSortState::default()),
            vec![2, 3, 1]
        );
    }

    #[test]
    fn selecting_active_start_toggles_to_oldest_first() {
        let mut state = TimesheetSortState::default();
        state.select(TimesheetSortColumn::Start);
        assert_eq!(state.direction, SortDirection::Ascending);
        assert_eq!(ids(&entries(), state), vec![1, 3, 2]);
    }

    #[test]
    fn pa_name_sorts_case_insensitively_and_toggles_direction() {
        let mut state = TimesheetSortState::default();
        state.select(TimesheetSortColumn::PaName);
        assert_eq!(ids(&entries(), state), vec![2, 3, 1]);
        state.select(TimesheetSortColumn::PaName);
        assert_eq!(ids(&entries(), state), vec![1, 3, 2]);
    }

    #[test]
    fn pa_date_order_is_independent_of_name_order() {
        let mut values = entries();
        let mut older = values[1].clone();
        older.id = 4;
        older.pa_name = "alice".into();
        older.start_time = "31 January 2026 at 09:00:00".into();
        values.push(older);
        let mut state = TimesheetSortState::default();
        state.select(TimesheetSortColumn::PaName);
        assert_eq!(ids(&values, state), vec![2, 4, 3, 1]);
        state.pa_date_direction = SortDirection::Ascending;
        assert_eq!(ids(&values, state), vec![4, 2, 3, 1]);
        state.select(TimesheetSortColumn::PaName);
        assert_eq!(ids(&values, state), vec![1, 3, 4, 2]);
        state.pa_date_direction = SortDirection::Descending;
        assert_eq!(ids(&values, state), vec![1, 3, 2, 4]);
    }

    #[test]
    fn pa_filter_lists_loaded_names_and_preserves_date_order() {
        let mut values = entries();
        let mut older = values[1].clone();
        older.id = 4;
        older.start_time = "31 January 2026 at 09:00:00".into();
        values.push(older);
        assert_eq!(imported_pa_names(&values), vec!["Alice", "bob", "zoe"]);
        let mut state = TimesheetSortState::default();
        state.select(TimesheetSortColumn::PaName);
        let filtered_ids = |state, filter| {
            filtered_timesheets(&values, state, filter)
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(filtered_ids(state, Some("Alice")), vec![2, 4]);
        state.pa_date_direction = SortDirection::Ascending;
        assert_eq!(filtered_ids(state, Some("Alice")), vec![4, 2]);
        assert_eq!(filtered_ids(state, Some("bob")), vec![3]);
        assert_eq!(filtered_ids(state, None), vec![4, 2, 3, 1]);
        state.select(TimesheetSortColumn::PaName);
        assert_eq!(filtered_ids(state, None), vec![1, 3, 4, 2]);
        assert_eq!(
            values.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert!(imported_pa_names(&[]).is_empty());
    }

    #[test]
    fn imported_scope_uses_schedule_rollover_and_inclusive_work_dates() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        let repository =
            crate::payroll_schedule_repository::PayrollScheduleRepository::new(connection);
        let today = chrono::NaiveDate::from_ymd_opt(2027, 3, 25).unwrap();
        assert!(imported_cycle_scope(&repository, today).is_err());
        // Use an independent connection fixture with year rollover before April.
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        for (year, cycle, start) in [("2026/27", 13, "22/02/2027"), ("2027/28", 1, "22/03/2027")] {
            connection.execute("INSERT INTO payroll_schedules (payroll_year, cycle_number, first_week_commencing, latest_posting_date, pay_date, created_at, payslips_sent) VALUES (?1, ?2, ?3, '01/04/2027', '08/04/2027', 'test', 0)", rusqlite::params![year, cycle, start]).unwrap();
        }
        let repository =
            crate::payroll_schedule_repository::PayrollScheduleRepository::new(connection);
        let scope = imported_cycle_scope(&repository, today);
        assert_eq!(
            scope,
            Ok((
                chrono::NaiveDate::from_ymd_opt(2027, 2, 22).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2027, 4, 18).unwrap()
            ))
        );
        for (start, expected) in [
            ("2027-02-21 at 23:59:00", false),
            ("2027-02-22 at 00:00:00", true),
            ("2027-04-18 at 23:59:00", true),
            ("2027-04-19 at 00:00:00", false),
            ("unknown", true),
        ] {
            let row = entry(1, "Alice", start, start, 60, 12.0, 12.0);
            assert_eq!(
                imported_entry_in_scope(&row, false, &scope),
                expected,
                "{start}"
            );
            assert!(imported_entry_in_scope(&row, true, &scope));
            assert!(imported_entry_in_scope(
                &row,
                false,
                &Err("No schedule".into())
            ));
        }
        // The first known cycle has no previous schedule: do not silently hide data.
        assert!(imported_cycle_scope(
            &repository,
            chrono::NaiveDate::from_ymd_opt(2027, 2, 25).unwrap()
        )
        .is_err());
    }

    #[test]
    fn compound_headings_each_occupy_one_grid_cell() {
        let ctx = egui::Context::default();
        let mut state = TimesheetSortState::default();
        let mut headings = Vec::new();
        let mut cells = Vec::new();
        for _ in 0..3 {
            headings.clear();
            cells.clear();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        egui::Grid::new("heading_alignment_test")
                            .num_columns(6)
                            .show(ui, |ui| {
                                for (column, label) in [
                                    (TimesheetSortColumn::PaName, "PA Name"),
                                    (TimesheetSortColumn::Start, "Start"),
                                    (TimesheetSortColumn::End, "End"),
                                    (TimesheetSortColumn::Worked, "Worked"),
                                    (TimesheetSortColumn::Rate, "Rate"),
                                    (TimesheetSortColumn::Amount, "Amount"),
                                ] {
                                    headings.push(ui.next_widget_position());
                                    if matches!(
                                        column,
                                        TimesheetSortColumn::PaName | TimesheetSortColumn::Start
                                    ) {
                                        let width = if column == TimesheetSortColumn::PaName {
                                            28.0
                                        } else {
                                            133.0
                                        };
                                        timesheet_control_heading(
                                            ui,
                                            &mut state,
                                            column,
                                            label,
                                            width,
                                            |ui| {
                                                if column == TimesheetSortColumn::PaName {
                                                    ui.menu_button("   ", |_| {});
                                                } else {
                                                    ui.checkbox(&mut false, "Include all cycles");
                                                }
                                            },
                                        );
                                    } else {
                                        timesheet_sort_heading(ui, &mut state, column, label);
                                    }
                                }
                                ui.end_row();
                                for width in TIMESHEET_COLUMN_WIDTHS {
                                    cells.push(ui.next_widget_position());
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(width, ui.spacing().interact_size.y),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label("entry");
                                        },
                                    );
                                }
                                ui.end_row();
                            });
                    });
                },
            );
        }
        for (heading, cell) in headings.iter().zip(&cells) {
            assert!(
                (heading.x - cell.x).abs() < 0.1,
                "heading {heading:?}, data {cell:?}"
            );
            assert!((heading.y - headings[0].y).abs() < 0.1);
            assert!(cell.y > heading.y);
        }
    }

    #[test]
    fn heading_control_click_does_not_sort() {
        let ctx = egui::Context::default();
        let mut state = TimesheetSortState::default();
        let mut enabled = false;
        let mut control_rect = egui::Rect::NOTHING;
        let mut frame = |events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        timesheet_control_heading(
                            ui,
                            &mut state,
                            TimesheetSortColumn::Start,
                            "Start",
                            133.0,
                            |ui| {
                                control_rect = ui.checkbox(&mut enabled, "Include all cycles").rect;
                            },
                        );
                    });
                },
            );
            control_rect
        };
        let pos = frame(vec![]).center();
        frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(enabled);
        assert_eq!(state, TimesheetSortState::default());
    }

    #[test]
    fn worked_rate_and_amount_sort_numerically() {
        let values = entries();
        for (column, expected) in [
            (TimesheetSortColumn::Worked, vec![2, 3, 1]),
            (TimesheetSortColumn::Rate, vec![2, 3, 1]),
            (TimesheetSortColumn::Amount, vec![2, 3, 1]),
        ] {
            let mut state = TimesheetSortState::default();
            state.select(column);
            assert_eq!(ids(&values, state), expected);
            state.select(column);
            assert_eq!(ids(&values, state), vec![1, 3, 2]);
        }
    }

    #[test]
    fn end_sorts_chronologically_and_legacy_non_finite_numbers_sort_last() {
        let values = entries();
        let mut state = TimesheetSortState::default();
        state.select(TimesheetSortColumn::End);
        assert_eq!(ids(&values, state), vec![1, 3, 2]);

        let rates = vec![
            entry(1, "A", "bad", "bad", 0, f64::NAN, 0.0),
            entry(2, "B", "bad", "bad", 0, 10.0, 0.0),
        ];
        state.select(TimesheetSortColumn::Rate);
        assert_eq!(ids(&rates, state), vec![2, 1]);
        state.select(TimesheetSortColumn::Rate);
        assert_eq!(ids(&rates, state), vec![2, 1]);
    }
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
        crate::gui_controls::combo_box("view_payroll_schedule_year")
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                for payroll_year in payroll_years {
                    crate::gui_controls::combo_value(
                        ui,
                        &mut selected,
                        payroll_year.clone(),
                        payroll_year,
                    );
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
                ui.label(crate::date_utils::screen(
                    ui,
                    &schedule.first_week_commencing,
                ));
                ui.label(crate::date_utils::screen(ui, &schedule.latest_posting_date));
                ui.label(crate::date_utils::screen(ui, &schedule.pay_date));

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

    #[test]
    fn enter_hours_action_uses_the_existing_active_screen_navigation_state() {
        let mut active_screen = ActiveScreen::Dashboard;

        navigate_to_enter_hours(&mut active_screen);

        assert_eq!(active_screen, ActiveScreen::EnterHours);
    }

    #[test]
    fn dashboard_renders_exact_three_row_workflow_with_unified_import() {
        fn collect(shape: &egui::Shape, labels: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => labels.push((text.galley.job.text.clone(), text.pos)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, labels);
                    }
                }
                _ => {}
            }
        }
        let expected = [
            [
                "Enter Hours/Shifts",
                "Import Hours CSV",
                "View Imported Hours",
            ],
            [
                "Payroll Timesheet Preparation",
                "Generate Payroll Timesheets",
                "Email Payroll Timesheets",
            ],
            [
                "Import Payroll Documents",
                "Email Payslips",
                "View Payroll Schedule",
            ],
        ];
        for width in [900.0, 500.0] {
            let context = egui::Context::default();
            let mut labels = Vec::new();
            for _ in 0..2 {
                let output = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            draw_dashboard_workflow_actions(ui);
                        });
                    },
                );
                labels.clear();
                for shape in &output.shapes {
                    collect(&shape.shape, &mut labels);
                }
            }
            assert_eq!(labels.len(), 9);
            for (row_index, row) in expected.iter().enumerate() {
                let positions = row.map(|label| {
                    labels
                        .iter()
                        .find(|(text, _)| text == label)
                        .expect(label)
                        .1
                });
                if width > 800.0 {
                    assert!((positions[0].y - positions[1].y).abs() < 1.0);
                    assert!((positions[1].y - positions[2].y).abs() < 1.0);
                    assert!(positions[0].x < positions[1].x && positions[1].x < positions[2].x);
                    if row_index > 0 {
                        let previous = labels
                            .iter()
                            .find(|(text, _)| text == expected[row_index - 1][0])
                            .unwrap()
                            .1;
                        assert!(previous.y < positions[0].y);
                    }
                }
            }
            assert!(!labels
                .iter()
                .any(|(text, _)| text == "Import Payroll Prep Sheet"));
        }
    }

    #[test]
    fn dashboard_workflow_uses_aligned_columns_only_when_they_fit() {
        let spacing = 8.0;
        let required = WORKFLOW_COLUMN_WIDTHS.iter().sum::<f32>() + spacing * 2.0;

        assert_eq!(
            dashboard_workflow_layout(required, spacing),
            DashboardWorkflowLayout::AlignedColumns
        );
        assert_eq!(
            dashboard_workflow_layout(required - 1.0, spacing),
            DashboardWorkflowLayout::WrappedRows
        );
    }

    #[test]
    fn dashboard_status_panel_styles_are_deterministic_for_all_themes() {
        let visuals = egui::Visuals::dark();
        let themes = [
            ApplicationTheme::System,
            ApplicationTheme::Light,
            ApplicationTheme::SoftLight,
            ApplicationTheme::Dark,
            ApplicationTheme::SoftDark,
            ApplicationTheme::Blue,
            ApplicationTheme::AccessibleHighContrast,
        ];

        for theme in themes {
            let first = dashboard_status_panel_style(0, theme, &visuals);
            let second = dashboard_status_panel_style(1, theme, &visuals);
            assert_eq!(first, dashboard_status_panel_style(2, theme, &visuals));

            if theme == ApplicationTheme::AccessibleHighContrast {
                assert_eq!(first.fill, visuals.panel_fill);
                assert_eq!(second.fill, visuals.panel_fill);
                assert_ne!(first.stroke, egui::Stroke::NONE);
            } else {
                assert_ne!(first.fill, second.fill);
                assert_eq!(first.stroke, egui::Stroke::NONE);
                assert_eq!(second.stroke, egui::Stroke::NONE);
            }
        }
    }

    #[test]
    fn payroll_document_summary_preserves_grouped_counts_and_issue_status() {
        let mut result = crate::archive::PayrollReturnImportResult {
            payslips_imported: 1,
            payslips_already_present: 2,
            archival_payslips_imported: 3,
            archival_payslips_already_present: 4,
            supplements_imported: 5,
            supplements_already_present: 6,
            information_files_imported: 7,
            information_files_already_present: 8,
            details: vec!["Planning detail, not a success claim".into()],
            ..Default::default()
        };
        let counts = payroll_document_count_rows(&result);
        assert_eq!(
            counts.map(|(_, a, b)| (a, b)),
            [(1, 2), (3, 4), (5, 6), (7, 8)]
        );
        assert!(counts[2].0.contains("P60/P45"));
        assert!(counts[3].0.contains("includes Payroll Prep Sheets"));
        assert_eq!(
            payroll_return_status_message(&result),
            "Payroll document import completed. See the result below."
        );
        result.prep_sheet_failures.push("Parsing failed".into());
        assert!(payroll_return_status_message(&result).contains("with issues"));
        result.prep_sheet_failures.clear();
        result.publication_failure = Some("Registration failed".into());
        assert!(payroll_return_status_message(&result).contains("with issues"));
        assert!(!payroll_return_status_message(&result).contains(&result.details[0]));
    }

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

        assert!(ids.contains(&1));
        assert!(!ids.contains(&2));
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

#[cfg(test)]
mod preparation_navigation_tests {
    use super::*;
    use crate::payroll_timesheet_screen::{tests::dirty_preparation_fixture, UnsavedChoice};

    #[test]
    fn every_screen_destination_and_window_close_obey_save_discard_cancel() {
        for destination in [
            ActiveScreen::Dashboard,
            ActiveScreen::Employer,
            ActiveScreen::PersonalAssistant,
            ActiveScreen::PayrollSettings,
            ActiveScreen::ApplicationSettings,
            ActiveScreen::EmailSettings,
            ActiveScreen::EnterHours,
            ActiveScreen::PayrollTimesheet,
        ] {
            for choice in [
                UnsavedChoice::Save,
                UnsavedChoice::Discard,
                UnsavedChoice::Cancel,
            ] {
                let (_dir, application, screen) = dirty_preparation_fixture();
                let mut app = DirectPaymentApp::new(application);
                app.payroll_timesheet_screen = screen;
                app.active_screen = destination;
                let close = destination == ActiveScreen::PayrollTimesheet;
                assert!(app.defer_preparation_navigation(true, close));
                assert_eq!(app.active_screen, ActiveScreen::PayrollTimesheet);
                let proceed = app
                    .payroll_timesheet_screen
                    .resolve_unsaved(&app.application, choice)
                    .unwrap();
                let closing = app.finish_preparation_navigation(proceed);
                if matches!(choice, UnsavedChoice::Cancel) {
                    assert_eq!(app.active_screen, ActiveScreen::PayrollTimesheet);
                    assert!(app.payroll_timesheet_screen.has_unsaved_changes());
                    assert!(!closing);
                } else {
                    assert_eq!(app.active_screen, destination);
                    assert!(!app.payroll_timesheet_screen.has_unsaved_changes());
                    assert_eq!(closing, close);
                }
                assert!(app.pending_preparation_navigation.is_none());
            }
        }
    }

    #[test]
    fn period_dropdown_restores_old_selection_until_navigation_is_confirmed() {
        for choice in [
            UnsavedChoice::Save,
            UnsavedChoice::Discard,
            UnsavedChoice::Cancel,
        ] {
            let (_dir, application, screen) = dirty_preparation_fixture();
            let mut app = DirectPaymentApp::new(application);
            app.payroll_timesheet_screen = screen;
            app.active_screen = ActiveScreen::PayrollTimesheet;
            let previous = app.operational_payroll_period.clone();
            let requested = OperationalPayrollPeriodState {
                selected: Some(OperationalPayrollPeriodKey {
                    payroll_year: "2027/28".into(),
                    cycle_number: 1,
                }),
                explicitly_selected: true,
                revision: previous.revision + 1,
            };
            app.operational_payroll_period = requested.clone();
            app.guard_preparation_period_change(previous.clone());
            assert_eq!(app.operational_payroll_period, previous);
            let proceed = app
                .payroll_timesheet_screen
                .resolve_unsaved(&app.application, choice)
                .unwrap();
            assert!(!app.finish_preparation_navigation(proceed));
            assert_eq!(
                app.operational_payroll_period,
                if proceed { requested } else { previous }
            );
        }
    }

    #[test]
    fn unchanged_navigation_does_not_open_confirmation() {
        let (_dir, application, screen) = dirty_preparation_fixture();
        let mut app = DirectPaymentApp::new(application);
        app.payroll_timesheet_screen = screen;
        app.payroll_timesheet_screen
            .resolve_unsaved(&app.application, UnsavedChoice::Discard)
            .unwrap();
        app.active_screen = ActiveScreen::Dashboard;
        assert!(!app.defer_preparation_navigation(true, false));
        assert!(!app.defer_preparation_navigation(true, true));
        let previous = app.operational_payroll_period.clone();
        app.operational_payroll_period.revision += 1;
        app.guard_preparation_period_change(previous);
        assert!(app.pending_preparation_navigation.is_none());
    }
}

#[cfg(test)]
mod payroll_period_eligibility_tests {
    use super::*;
    use crate::payroll_timesheet_screen::tests::{
        employment_period_fixture, load_period_for_eligibility_test,
    };

    // Exercise the real GUI -> application -> test SMTP transport path against
    // an isolated database and a loopback SMTP sink, never a configured server.
    #[test]
    fn test_payslip_email_selects_production_bundle_without_writing_delivery_state() {
        payroll_document_email_composition_cases(true);
    }

    #[test]
    fn payroll_document_result_is_independent_of_status_and_cleared_on_failed_import() {
        let (dir, application, _, _) = employment_period_fixture();
        let mut app = DirectPaymentApp::new(application);
        app.payroll_document_import = Some(crate::archive::PayrollReturnImportResult {
            supplements_imported: 2,
            details: vec!["Stored result detail".into()],
            ..Default::default()
        });
        app.status_message = "Unrelated status".into();
        assert_eq!(
            app.payroll_document_import
                .as_ref()
                .unwrap()
                .supplements_imported,
            2
        );
        assert!(app.file_status.is_none());
        app.import_selected_payroll_documents(&dir.path().join("missing.zip"), None);
        assert!(app
            .status_message
            .starts_with("Payroll document import failed:"));
        assert!(app.payroll_document_import.is_none());
        assert!(app.file_status.is_none());
        assert!(app.last_import.is_none());
    }

    #[test]
    fn production_payroll_document_composition_preserves_document_delivery_tracking() {
        payroll_document_email_composition_cases(false);
    }

    fn payroll_document_email_composition_cases(test_send: bool) {
        use std::io::{BufRead, BufReader, Write};
        for (ordinary, kinds, sent, has_schedule) in [
            (false, vec!["P60"], false, false),
            (false, vec!["P45"], false, false),
            (true, vec![], false, true),
            (true, vec!["P60", "P45"], false, true),
            (true, vec!["P60", "P45"], true, true),
            (false, vec!["P60", "P45"], true, false),
            (false, vec![], false, true),
            (false, vec!["P60"], false, true),
            (false, vec!["P45"], false, true),
            (false, vec!["P60", "P45"], false, true),
        ] {
            let (dir, mut application, _, current) = employment_period_fixture();
            application.context.config.folders.payslip_folder = dir.path().join("payslips");
            application
                .context
                .config
                .folders
                .payroll_information_folder = dir.path().join("info");
            let db =
                rusqlite::Connection::open(&application.context.environment.database_path).unwrap();
            db.execute_batch("UPDATE employers SET email='employer@example.com', email_signature='Employer signature'; UPDATE personal_assistants SET email='production-pa@example.com'; INSERT INTO payroll_provider(id, name, payroll_department_email) VALUES(1, 'Provider', 'production-payroll@example.com');").unwrap();
            let pa = application
                .personal_assistant_repository
                .get_all()
                .unwrap()
                .into_iter()
                .find(|pa| pa.id == if ordinary || kinds.is_empty() { 2 } else { 1 })
                .unwrap();
            application.context.config.payroll.email_subject_format =
                "Timesheet - {Personal Assistant Name} {YYYYMMwWW}".into();
            application.context.config.payroll.payslip_email_body =
                "Please find attached your payslip for the payroll period.".into();
            let name = format!("{} {}", pa.first_name, pa.surname);
            for kind in &kinds {
                let path = dir.path().join(format!("{kind} for {name}.pdf"));
                std::fs::write(&path, format!("%PDF-1.4 {kind} document")).unwrap();
                application.import_payroll_documents(&path, None).unwrap();
            }
            if sent {
                db.execute_batch("UPDATE imported_payroll_documents SET sent_at='already-sent'")
                    .unwrap();
            }
            if ordinary {
                let path = crate::payroll_file_naming::payslip_path(
                    &application.context.config.folders.payslip_folder,
                    &name,
                    &current,
                )
                .unwrap();
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b"%PDF-1.4 ordinary").unwrap();
            }
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            application.context.config.email.smtp_transport = "SMTP Server".into();
            application.context.config.email.smtp_host = "127.0.0.1".into();
            application.context.config.email.smtp_port = listener.local_addr().unwrap().port();
            application.context.config.email.smtp_username.clear();
            application.context.config.email.pa_test_email_address =
                "  test-pa@example.com  ".into();
            application.context.config.email.payroll_test_email_address =
                "test-payroll@example.com".into();
            let mut app = DirectPaymentApp::new(application);
            if has_schedule {
                app.operational_payroll_period.select(&current, None);
            } else {
                app.operational_payroll_period.selected = None;
            }
            app.preview_personal_assistant_id = Some(pa.id);
            let expected = app
                .unsent_payslip_documents(&pa, has_schedule.then_some(&current))
                .unwrap();
            assert_eq!(
                expected.paths.len(),
                usize::from(ordinary) + if sent { 0 } else { kinds.len() }
            );
            let before: i64 = db
                .query_row("PRAGMA data_version", [], |row| row.get(0))
                .unwrap();
            // data_version changes on ANY commit from another connection, covering
            // all delivery, schedule, evidence and settlement tables, even if a
            // later write were to restore their original values.
            if expected.paths.is_empty() {
                app.send_test_payslip_email();
                assert!(
                    app.status_message
                        .contains("No unsent PA payroll documents"),
                    "{}",
                    app.status_message
                );
                assert!(!app.status_message.contains("No such file"));
            } else {
                assert!(app
                    .payslip_email_assistants()
                    .unwrap()
                    .iter()
                    .any(|a| a.id == pa.id));
                listener.set_nonblocking(true).unwrap();
                let server = std::thread::spawn(move || {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(e)
                                if e.kind() == std::io::ErrorKind::WouldBlock
                                    && std::time::Instant::now() < deadline =>
                            {
                                std::thread::sleep(std::time::Duration::from_millis(10))
                            }
                            Err(e) => panic!("Test SMTP accept failed: {e}"),
                        }
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                        .unwrap();
                    stream.write_all(b"220 localhost test\r\n").unwrap();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut recipients = Vec::new();
                    let mut message = String::new();
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap() == 0 {
                            break;
                        }
                        if line.starts_with("RCPT TO:") {
                            recipients.push(line.trim().to_string());
                        }
                        if line.starts_with("DATA") {
                            stream.write_all(b"354 send data\r\n").unwrap();
                            loop {
                                line.clear();
                                assert!(reader.read_line(&mut line).unwrap() > 0);
                                if line == ".\r\n" {
                                    break;
                                }
                                message.push_str(&line);
                            }
                            stream.write_all(b"250 accepted\r\n").unwrap();
                            break;
                        }
                        stream.write_all(b"250 OK\r\n").unwrap();
                    }
                    (recipients, message)
                });
                // Preview and production transport must agree on the bundle wording.
                app.load_email_preview(PayrollEmailKind::Payslip);
                let preview = app.email_preview.as_ref().expect(&app.status_message);
                let expected_subject = if ordinary {
                    format!(
                        "Timesheet - {name} {}",
                        crate::payroll_file_naming::payroll_period_code(&current).unwrap()
                    )
                } else {
                    format!("Payroll documents - {name}")
                };
                let expected_body = if ordinary {
                    "Please find attached your payslip for the payroll period."
                } else if expected.paths.len() == 1 {
                    "Please find attached your payroll document."
                } else {
                    "Please find attached your payroll documents."
                };
                assert_eq!(preview.subject, expected_subject);
                assert_eq!(
                    preview.body,
                    format!("{expected_body}\n\nEmployer signature")
                );
                if test_send {
                    app.send_test_payslip_email();
                    assert_eq!(app.status_message, "Test payslip email sent.");
                } else {
                    assert_eq!(
                        app.email_payslips(has_schedule.then_some(&current))
                            .unwrap(),
                        1
                    );
                }
                let (recipients, message) = server.join().unwrap();
                if test_send {
                    assert_eq!(recipients, vec!["RCPT TO:<test-pa@example.com>"]);
                    assert!(message.contains("To: test-pa@example.com"));
                    assert!(!message.contains("Cc:") && !message.contains("Bcc:"));
                    assert!(
                        !message.contains("production-pa@example.com")
                            && !message.contains("production-payroll@example.com")
                            && !message.contains("test-payroll@example.com")
                    );
                    assert!(message.contains(&format!("Subject: TEST: {expected_subject}")));
                    assert!(message.contains("TEST:\r\n"));
                } else {
                    assert_eq!(recipients.len(), 3);
                    for recipient in [
                        "production-payroll@example.com",
                        "production-pa@example.com",
                        "employer@example.com",
                    ] {
                        assert!(recipients.contains(&format!("RCPT TO:<{recipient}>")));
                    }
                    assert!(!message.contains("TEST:"));
                    assert!(message.contains(&format!("Subject: {expected_subject}")));
                    for id in &expected.document_ids {
                        let sent_at: Option<String> = db
                            .query_row(
                                "SELECT sent_at FROM imported_payroll_documents WHERE id=?1",
                                [id],
                                |row| row.get(0),
                            )
                            .unwrap();
                        assert!(sent_at.is_some_and(|value| !value.starts_with("INDETERMINATE")));
                    }
                    assert!(app
                        .unsent_payslip_documents(&pa, has_schedule.then_some(&current))
                        .unwrap()
                        .paths
                        .is_empty());
                }
                assert!(message.contains(expected_body));
                assert!(message.contains("Employer signature"));
                if !ordinary {
                    assert!(!message.contains("Timesheet -"));
                    assert!(
                        !message.contains("your payslip") && !message.contains("payroll period")
                    );
                    assert!(!message.contains(
                        &crate::payroll_file_naming::payroll_period_code(&current).unwrap()
                    ));
                }
                assert_eq!(
                    message.matches("Content-Disposition: attachment").count(),
                    expected.paths.len()
                );
                for path in &expected.paths {
                    assert!(
                        message.contains(path.file_name().unwrap().to_str().unwrap()),
                        "Missing attachment in {message}"
                    );
                }
            }
            // The timesheet action still requires a schedule and period-eligible PA,
            // even when the shared picker includes an inactive document recipient.
            if !has_schedule || (!ordinary && !kinds.is_empty()) {
                app.send_test_timesheet_email();
                assert!(app
                    .status_message
                    .starts_with("Test timesheet email failed:"));
                assert!(!app.status_message.contains("No such file"));
            }
            let after: i64 = db
                .query_row("PRAGMA data_version", [], |row| row.get(0))
                .unwrap();
            if test_send || expected.paths.is_empty() {
                assert_eq!(
                    before, after,
                    "Test email must never commit production state"
                );
            } else {
                assert_ne!(before, after, "Production must record successful delivery");
            }
        }
    }

    #[test]
    fn standalone_document_preview_and_batch_do_not_require_a_schedule() {
        let (dir, mut application, _, _) = employment_period_fixture();
        application.context.config.folders.payslip_folder = dir.path().join("payslips");
        application
            .context
            .config
            .folders
            .payroll_information_folder = dir.path().join("info");
        application.payroll_timesheet_email_repository.connection.execute_batch("UPDATE employers SET email='employer@example.com'; INSERT INTO payroll_provider(id, name, payroll_department_email) VALUES(1, 'Provider', 'payroll@example.com');").unwrap();
        let pa = application
            .personal_assistant_repository
            .get_all()
            .unwrap()
            .remove(0);
        let path = dir
            .path()
            .join(format!("P60 for {} {}.pdf", pa.first_name, pa.surname));
        std::fs::write(&path, b"%PDF-1.4 document").unwrap();
        application.import_payroll_documents(&path, None).unwrap();
        let mut app = DirectPaymentApp::new(application);
        app.operational_payroll_period.selected = None;
        app.preview_personal_assistant_id = Some(pa.id);
        app.load_email_preview(PayrollEmailKind::Payslip);
        let preview = app.email_preview.as_ref().expect(&app.status_message);
        assert!(preview.subject.contains("Payroll documents"));
        assert_eq!(
            app.unsent_payslip_documents(&pa, None)
                .unwrap()
                .document_ids
                .len(),
            1
        );
        app.begin_email_batch(PayrollEmailKind::Payslip);
        let batch = app.pending_email_batch.as_ref().expect(&app.status_message);
        assert!(batch.payroll_period.is_none());
        assert!(app
            .validated_schedule_for_email_batch(batch)
            .unwrap()
            .is_none());
        app.begin_email_batch(PayrollEmailKind::Timesheet);
        assert!(app.status_message.contains("Could not begin"));
    }

    #[test]
    fn later_supplements_import_and_preview_without_resending_an_ordinary_payslip() {
        use std::io::Write;
        for types in [vec!["P60"], vec!["P45"], vec!["P60", "P45"]] {
            for ordinary_sent in [false, true] {
                let (dir, mut application, _, current) = employment_period_fixture();
                application.context.config.folders.payslip_folder = dir.path().join("payslips");
                application
                    .context
                    .config
                    .folders
                    .payroll_information_folder = dir.path().join("info");
                let db = rusqlite::Connection::open(&application.context.environment.database_path)
                    .unwrap();
                db.execute_batch("UPDATE employers SET email='employer@example.com'; INSERT INTO payroll_provider(id, name, payroll_department_email) VALUES(1, 'Provider', 'payroll@example.com'); UPDATE personal_assistants SET email='pa@example.com' WHERE id=1;").unwrap();
                let before = format!(
                    "{:?}",
                    application.personal_assistant_repository.get_all().unwrap()
                );
                let pa = application
                    .personal_assistant_repository
                    .get_all()
                    .unwrap()
                    .into_iter()
                    .find(|pa| pa.id == 1)
                    .unwrap();
                let full_name = format!("{} {}", pa.first_name, pa.surname);
                let ordinary_path = crate::payroll_file_naming::payslip_path(
                    &application.context.config.folders.payslip_folder,
                    &full_name,
                    &current,
                )
                .unwrap();
                if ordinary_sent {
                    std::fs::create_dir_all(ordinary_path.parent().unwrap()).unwrap();
                    std::fs::write(&ordinary_path, b"%PDF-1.4 previously sent").unwrap();
                    application
                        .payroll_timesheet_email_repository
                        .mark_sent(
                            pa.id,
                            &current.payroll_year,
                            current.cycle_number,
                            "payslip",
                            "previous-send",
                        )
                        .unwrap();
                }
                let zip_path = dir.path().join("later-documents.zip");
                let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
                let mut names = types
                    .iter()
                    .map(|kind| format!("Provider - {kind} Summary for {full_name}.pdf"))
                    .collect::<Vec<_>>();
                names.push(format!("P30 Employer's Payslip {full_name}.pdf"));
                for name in names {
                    zip.start_file(name, zip::write::SimpleFileOptions::default())
                        .unwrap();
                    zip.write_all(b"%PDF-1.4 document").unwrap();
                }
                zip.finish().unwrap();
                let imported = application
                    .import_payroll_documents(&zip_path, Some(&current))
                    .unwrap();
                assert_eq!(imported.supplements_imported, types.len());
                assert_eq!(imported.payslips_imported, 0);
                let mut app = DirectPaymentApp::new(application);
                app.operational_payroll_period.select(&current, None);
                app.preview_personal_assistant_id = Some(pa.id);
                app.load_email_preview(PayrollEmailKind::Payslip);
                let preview = app.email_preview.as_ref().expect(&app.status_message);
                let bundle = app.unsent_payslip_documents(&pa, &current).unwrap();
                assert_eq!(bundle.paths.len(), types.len());
                assert!(!bundle.paths.contains(&ordinary_path));
                assert_eq!(
                    preview.attachment_path,
                    bundle.paths[0].display().to_string()
                );
                assert_eq!(preview.additional_attachment_paths, bundle.paths[1..]);
                assert_eq!(preview.bcc.as_deref(), Some("pa@example.com"));
                assert!(bundle
                    .email_types
                    .iter()
                    .all(|kind| matches!(*kind, "p60" | "p45")));
                let prior = app
                    .application
                    .payroll_timesheet_email_repository
                    .get_for_pa_and_cycle(
                        pa.id,
                        &current.payroll_year,
                        current.cycle_number,
                        "payslip",
                    )
                    .unwrap();
                if ordinary_sent {
                    assert_eq!(prior.unwrap().sent_at.as_deref(), Some("previous-send"));
                } else {
                    assert!(prior.is_none());
                }
                assert_eq!(
                    before,
                    format!(
                        "{:?}",
                        app.application
                            .personal_assistant_repository
                            .get_all()
                            .unwrap()
                    )
                );
            }
        }
    }

    #[test]
    fn combined_preview_selects_only_imported_pa_context_pdfs_and_preserves_employment() {
        use std::io::Write;
        for types in [vec!["P60"], vec!["P45"], vec!["P60", "P45"]] {
            let (dir, mut application, _, current) = employment_period_fixture();
            application.context.config.folders.payslip_folder = dir.path().join("payslips");
            application
                .context
                .config
                .folders
                .payroll_information_folder = dir.path().join("info");
            let db =
                rusqlite::Connection::open(&application.context.environment.database_path).unwrap();
            db.execute_batch("UPDATE employers SET email='employer@example.com'; INSERT INTO payroll_provider(id, name, payroll_department_email) VALUES(1, 'Provider', 'payroll@example.com'); UPDATE personal_assistants SET email='pa@example.com' WHERE id=1;").unwrap();
            let before = format!(
                "{:?}",
                application.personal_assistant_repository.get_all().unwrap()
            );
            let pa = application
                .personal_assistant_repository
                .get_all()
                .unwrap()
                .into_iter()
                .find(|pa| pa.id == 1)
                .unwrap();
            let name = format!("{} {}", pa.first_name, pa.surname);
            let zip_path = dir.path().join("return.zip");
            let mut names = vec![
                format!("Payslip {name}.pdf"),
                format!("P30 Employer's Payslip {name}.pdf"),
            ];
            names.extend(
                types
                    .iter()
                    .map(|kind| format!("Provider - {kind} Summary for {name}.pdf")),
            );
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            for name in names {
                zip.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"%PDF-1.4 document").unwrap();
            }
            zip.finish().unwrap();
            application
                .import_payroll_return(&zip_path, &current)
                .unwrap();
            let mut app = DirectPaymentApp::new(application);
            app.operational_payroll_period.select(&current, None);
            app.preview_personal_assistant_id = Some(1);
            app.load_email_preview(PayrollEmailKind::Payslip);
            let preview = app.email_preview.as_ref().expect(&app.status_message);
            assert!(preview.attachment_path.contains("Payslip for Week"));
            assert_eq!(preview.additional_attachment_paths.len(), types.len());
            for kind in &types {
                assert!(preview.additional_attachment_paths.iter().any(|path| path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(kind)));
                assert!(app
                    .application
                    .payroll_timesheet_email_repository
                    .get_for_pa_and_cycle(
                        1,
                        &current.payroll_year,
                        current.cycle_number,
                        &kind.to_lowercase()
                    )
                    .unwrap()
                    .is_none());
            }
            assert!(preview
                .additional_attachment_paths
                .iter()
                .all(|path| !path.to_string_lossy().contains("P30")));
            assert_eq!(preview.to, "payroll@example.com");
            assert_eq!(preview.bcc.as_deref(), Some("pa@example.com"));
            assert_eq!(
                before,
                format!(
                    "{:?}",
                    app.application
                        .personal_assistant_repository
                        .get_all()
                        .unwrap()
                )
            );
        }
    }

    #[test]
    fn later_document_has_independent_delivery_identity_and_identical_reimport_is_safe() {
        use std::io::Write;
        for marker in ["sent", "indeterminate:attempt"] {
            let (dir, mut application, _, current) = employment_period_fixture();
            application.context.config.folders.payslip_folder = dir.path().join("payslips");
            application
                .context
                .config
                .folders
                .payroll_information_folder = dir.path().join("info");
            let pa = application
                .personal_assistant_repository
                .get_all()
                .unwrap()
                .remove(0);
            let zip_path = dir.path().join("return.zip");
            let write_zip = |variant: &str| {
                let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
                zip.start_file(
                    format!("P60 {variant} for {} {}.pdf", pa.first_name, pa.surname),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
                zip.write_all(b"%PDF-1.4 document").unwrap();
                zip.finish().unwrap();
            };
            write_zip("original");
            application
                .import_payroll_return(&zip_path, &current)
                .unwrap();
            let repository = &application.payroll_timesheet_email_repository;
            let id = repository.documents_for_pa(pa.id).unwrap()[0].id;
            repository
                .connection
                .execute(
                    "UPDATE imported_payroll_documents SET sent_at = ?1 WHERE id = ?2",
                    rusqlite::params![marker, id],
                )
                .unwrap();
            assert_eq!(
                application
                    .import_payroll_return(&zip_path, &current)
                    .unwrap()
                    .supplements_already_present,
                1
            );
            write_zip("additional");
            assert_eq!(
                application
                    .import_payroll_documents(&zip_path, None)
                    .unwrap()
                    .supplements_imported,
                1
            );
            let documents = repository.documents_for_pa(pa.id).unwrap();
            assert_eq!(documents.len(), 2);
            assert_eq!(documents[0].id, id);
            assert_ne!(
                documents[0].delivery_state,
                crate::payroll_timesheet_email_repository::EmailDeliveryState::Unsent
            );
            assert_eq!(
                documents[1].delivery_state,
                crate::payroll_timesheet_email_repository::EmailDeliveryState::Unsent
            );
        }
    }

    #[test]
    fn imported_p45_keeps_former_pa_access_without_changing_employment_or_timesheet_scope() {
        use std::io::Write;
        let (dir, mut application, _, current) = employment_period_fixture();
        application.context.config.folders.payslip_folder = dir.path().join("payslips");
        application
            .context
            .config
            .folders
            .payroll_information_folder = dir.path().join("info");
        let before = format!(
            "{:?}",
            application.personal_assistant_repository.get_all().unwrap()
        );
        let pa = application
            .personal_assistant_repository
            .get_all()
            .unwrap()
            .into_iter()
            .find(|pa| pa.id == 1)
            .unwrap();
        let zip_path = dir.path().join("return.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        for name in [
            format!("Provider - P45 for {} {}.pdf", pa.first_name, pa.surname),
            format!("P60 for {} {}.pdf", pa.first_name, pa.surname),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"%PDF-1.4 document").unwrap();
        }
        zip.finish().unwrap();
        application
            .import_payroll_return(&zip_path, &current)
            .unwrap();
        let mut app = DirectPaymentApp::new(application);
        app.operational_payroll_period.select(&current, None);
        assert!(!app
            .selected_period_assistants()
            .unwrap()
            .iter()
            .any(|pa| pa.id == 1));
        assert!(app
            .payslip_email_assistants()
            .unwrap()
            .iter()
            .any(|pa| pa.id == 1));
        app.begin_email_batch(PayrollEmailKind::Payslip);
        assert!(app
            .pending_email_batch
            .as_ref()
            .unwrap()
            .selected_personal_assistant_ids
            .contains(&1));
        app.begin_email_batch(PayrollEmailKind::Timesheet);
        assert!(!app
            .pending_email_batch
            .as_ref()
            .unwrap()
            .selected_personal_assistant_ids
            .contains(&1));
        assert_eq!(
            before,
            format!(
                "{:?}",
                app.application
                    .personal_assistant_repository
                    .get_all()
                    .unwrap()
            )
        );
        assert!(app
            .application
            .payroll_timesheet_repository
            .get_all_for_cycle(&current.payroll_year, current.cycle_number)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn dashboard_email_batches_and_generation_share_period_eligibility() {
        let (_dir, application, historical, current) = employment_period_fixture();
        let mut app = DirectPaymentApp::new(application);
        for (schedule, expected) in [(&historical, vec![1, 3, 5, 8]), (&current, vec![2, 4, 5])] {
            app.operational_payroll_period.select(schedule, None);
            let selected = app.selected_period_assistants().unwrap();
            assert_eq!(selected.iter().map(|p| p.id).collect::<Vec<_>>(), expected);
            assert_eq!(selected.len(), expected.len());
            // Selection/preview/counting must not create payroll records.
            assert!(app
                .application
                .payroll_timesheet_repository
                .get_all_for_cycle(&schedule.payroll_year, schedule.cycle_number)
                .unwrap()
                .is_empty());
            for kind in [PayrollEmailKind::Timesheet, PayrollEmailKind::Payslip] {
                app.begin_email_batch(kind);
                let pending = app.pending_email_batch.as_ref().unwrap();
                if matches!(kind, PayrollEmailKind::Timesheet) {
                    assert_eq!(
                        pending.choices.iter().map(|p| p.id).collect::<Vec<_>>(),
                        expected
                    );
                    assert!(pending.selected_personal_assistant_ids.is_empty());
                    assert!(pending.choices.iter().all(|p| !p.available));
                } else {
                    assert_eq!(pending.selected_personal_assistant_ids, expected);
                }
                app.clear_pending_email_batch();
            }
            app.payroll_timesheet_screen =
                load_period_for_eligibility_test(&app.application, schedule);
            assert_eq!(app.generate_payroll_timesheets().unwrap(), expected.len());
            app.begin_email_batch(PayrollEmailKind::Timesheet);
            assert_eq!(
                app.pending_email_batch
                    .as_ref()
                    .unwrap()
                    .selected_personal_assistant_ids,
                expected
            );
            app.clear_pending_email_batch();
            let records = app
                .application
                .payroll_timesheet_repository
                .get_all_for_cycle(&schedule.payroll_year, schedule.cycle_number)
                .unwrap();
            assert_eq!(records.len(), expected.len());
            for record in records {
                assert!(expected.contains(&record.personal_assistant_id));
                assert_eq!(
                    app.application
                        .payroll_worked_item_repository
                        .snapshot_metadata(record.id)
                        .unwrap()
                        .unwrap()
                        .state,
                    SnapshotState::Candidate
                );
            }
        }
    }

    #[test]
    fn stored_out_of_overlap_payroll_is_in_dashboard_and_email_scope_but_protected_from_generation()
    {
        let (_dir, application, historical, _) = employment_period_fixture();
        let mut app = DirectPaymentApp::new(application);
        app.operational_payroll_period.select(&historical, None);
        app.payroll_timesheet_screen =
            load_period_for_eligibility_test(&app.application, &historical);
        app.application
            .payroll_timesheet_email_repository
            .mark_sent(
                1,
                &historical.payroll_year,
                historical.cycle_number,
                "payslip",
                "2026-09-04T10:00:00Z",
            )
            .unwrap();
        let db =
            rusqlite::Connection::open(&app.application.context.environment.database_path).unwrap();
        db.execute("UPDATE personal_assistants SET start_date='01/01/2027',leaving_date='01/02/2027' WHERE id=1", []).unwrap();
        let records_before = app
            .application
            .payroll_timesheet_repository
            .get_all_for_cycle(&historical.payroll_year, historical.cycle_number)
            .unwrap();
        assert_eq!(
            app.selected_period_assistants()
                .unwrap()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![1, 3, 5, 8]
        );
        app.begin_email_batch(PayrollEmailKind::Timesheet);
        let pending = app.pending_email_batch.as_ref().unwrap();
        assert!(pending.choices.iter().any(|p| p.id == 1 && !p.available));
        assert!(!pending.selected_personal_assistant_ids.contains(&1));
        assert_eq!(app.generate_payroll_timesheets().unwrap(), 3);
        assert_eq!(
            app.application
                .payroll_timesheet_repository
                .get_all_for_cycle(&historical.payroll_year, historical.cycle_number)
                .unwrap()
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            records_before.iter().map(|r| r.id).collect::<Vec<_>>()
        );
        let settled = records_before
            .iter()
            .find(|r| r.personal_assistant_id == 1)
            .unwrap();
        assert!(app
            .application
            .payroll_worked_item_repository
            .snapshot_metadata(settled.id)
            .unwrap()
            .is_none());
        assert_eq!(
            crate::payroll_evidence::lifecycle::stage(&db, settled).unwrap(),
            crate::payroll_evidence::lifecycle::Stage::Settled
        );
    }
}

include!("payroll_production_workflow.rs");
