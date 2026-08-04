use eframe::egui;

use crate::app::Application;
use crate::import_service::ImportSummary;

pub struct DirectPaymentApp {
    application: Application,
    version: String,
    status_message: String,
    last_import: Option<ImportSummary>,
}

impl DirectPaymentApp {
    pub fn new(application: Application) -> Self {
        Self {
            version: application.context.version.clone(),
            application,
            status_message: "Application ready.".to_string(),
            last_import: None,
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        egui::TopBottomPanel::top("header")
            .show(ctx, |ui| {
                ui.heading("DirectPaymentTimesheets");

                ui.label(format!(
                    "Version {}",
                    self.version
                ));
            });

        egui::CentralPanel::default()
            .show(ctx, |ui| {
                ui.add_space(10.0);

                ui.heading("Dashboard");

                ui.separator();

                if ui.button("Import CSV").clicked() {
                    match self.application.import_csv() {
                        Ok(summary) => {
                            self.status_message =
                                "Import completed successfully."
                                .to_string();

                            self.last_import = Some(summary);
                        }

                        Err(error) => {
                            self.status_message = format!(
                                "Import failed: {}",
                                error
                            );

                            self.last_import = None;
                        }
                    }
                }

                if ui.button("View Timesheets").clicked() {
                    self.status_message =
                        "View Timesheets selected.".to_string();
                }

                if ui.button("Settings").clicked() {
                    self.status_message =
                        "Settings selected.".to_string();
                }

                if ui.button("Exit").clicked() {
                    ctx.send_viewport_cmd(
                        egui::ViewportCommand::Close,
                    );
                }

                ui.add_space(20.0);

                ui.separator();

                ui.heading("Import Summary");

                match &self.last_import {
                    Some(summary) => {
                        ui.label(format!(
                            "Files discovered: {}",
                            summary.files_discovered
                        ));

                        ui.label(format!(
                            "Already imported: {}",
                            summary.files_already_imported
                        ));

                        ui.label(format!(
                            "Files processed: {}",
                            summary.files_processed
                        ));

                        ui.label(format!(
                            "Rows processed: {}",
                            summary.rows_processed
                        ));

                        ui.label(format!(
                            "Rows imported: {}",
                            summary.rows_imported
                        ));

                        ui.label(format!(
                            "Rows skipped: {}",
                            summary.rows_skipped
                        ));

                        ui.label(format!(
                            "Files failed: {}",
                            summary.files_failed
                        ));
                    }
                    None => {
                        ui.label(
                            "No import performed yet."
                        );
                    }
                }

                ui.add_space(20.0);

                ui.separator();

                ui.label(format!(
                    "Status: {}",
                    self.status_message
                ));
            });
    }
}
