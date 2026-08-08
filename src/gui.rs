use eframe::egui;

use crate::app::Application;
use crate::employer_screen::EmployerScreen;
use crate::import_service::ImportSummary;
use crate::models::TimesheetEntry;
use crate::payroll_settings_screen::PayrollSettingsScreen;
use crate::personal_assistant_screen::PersonalAssistantScreen;

enum ActiveScreen {
    Dashboard,
    Employer,
    PersonalAssistant,
    PayrollSettings,
}

pub struct DirectPaymentApp {
    application: Application,
    version: String,
    status_message: String,
    last_import: Option<ImportSummary>,
    timesheets: Vec<TimesheetEntry>,
    employer_screen: EmployerScreen,
    personal_assistant_screen: PersonalAssistantScreen,
    payroll_settings_screen: PayrollSettingsScreen,
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
            employer_screen: EmployerScreen::new(),
            personal_assistant_screen: PersonalAssistantScreen::new(),
            payroll_settings_screen: PayrollSettingsScreen::new(),
            active_screen: ActiveScreen::Dashboard,
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.heading("Direct Payments Timesheets");
            ui.label(format!("Version {}", self.version));
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
                self.employer_screen.show(ui, &self.application);
            }

            ActiveScreen::PersonalAssistant => {
                self.personal_assistant_screen.show(ui, &self.application);
            }

            ActiveScreen::PayrollSettings => {
                self.payroll_settings_screen.show(ui, &mut self.application);
            }
        });
    }
}

impl DirectPaymentApp {
    fn draw_dashboard(&mut self, ui: &mut egui::Ui) {
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

        if ui.button("View Timesheets").clicked() {
            match self.application.get_timesheets() {
                Ok(entries) => {
                    self.timesheets = entries;

                    self.status_message = format!("Loaded {} timesheets.", self.timesheets.len());
                }

                Err(error) => {
                    self.status_message = format!("Failed loading timesheets: {}", error);
                }
            }
        }

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

        draw_timesheets(ui, &self.timesheets);
    }
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
