use eframe::egui;

use crate::app::Application;
use crate::import_service::ImportSummary;
use crate::models::TimesheetEntry;

pub struct DirectPaymentApp {
    application: Application,
    version: String,
    status_message: String,
    last_import: Option<ImportSummary>,
    timesheets: Vec<TimesheetEntry>,
}

impl DirectPaymentApp {
    pub fn new(application: Application) -> Self {
        Self {
            version: application.context.version.clone(),
            application,
            status_message: "Application ready.".to_string(),
            last_import: None,
            timesheets: Vec::new(),
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.heading("DirectPaymentTimesheets");

            ui.label(format!("Version {}", self.version));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);

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

            if ui.button("View Timesheets").clicked() {
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

            if ui.button("Settings").clicked() {
                self.status_message = "Settings selected.".to_string();
            }

            if ui.button("Exit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            ui.add_space(20.0);

            ui.separator();

            ui.heading("Import Summary");

            match &self.last_import {
                Some(summary) => {
                    ui.label(format!("Files discovered: {}", summary.files_discovered));

                    ui.label(format!(
                        "Already imported: {}",
                        summary.files_already_imported
                    ));

                    ui.label(format!("Files processed: {}", summary.files_processed));

                    ui.label(format!("Rows processed: {}", summary.rows_processed));

                    ui.label(format!("Rows imported: {}", summary.rows_imported));

                    ui.label(format!("Rows skipped: {}", summary.rows_skipped));

                    ui.label(format!("Files failed: {}", summary.files_failed));
                }

                None => {
                    ui.label("No import performed yet.");
                }
            }

            ui.add_space(20.0);

            ui.separator();

            ui.heading("System Status");

            ui.label("Database: Connected");

            ui.label(format!(
                "Data Directory: {:?}",
                self.application.context.environment.data_dir
            ));

            let import_folder =
                crate::paths::expand_path(&self.application.context.config.folders.csv_import);

            ui.label(format!("Import Folder: {:?}", import_folder));

            draw_timesheets(ui, &self.timesheets);

            ui.separator();

            ui.label(format!("Status: {}", self.status_message));
        });
    }
}

fn draw_timesheets(ui: &mut egui::Ui, timesheets: &[TimesheetEntry]) {
    ui.add_space(20.0);

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

    fn format_worked_time(minutes: i64) -> String {
        let hours = minutes / 60;
        let remaining_minutes = minutes % 60;

        format!("{}h {}m", hours, remaining_minutes)
    }
}
