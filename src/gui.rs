use eframe::egui;

use crate::app::Application;

pub struct DirectPaymentApp {
    application: Application,
    version: String,
    status_message: String,
}

impl DirectPaymentApp {
    pub fn new(application: Application) -> Self {
        Self {
            version: application.context.version.clone(),
            application,
            status_message: "Application ready.".to_string(),
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
                            self.status_message = format!(
                                "Import complete. Files: {}, Rows: {}, Imported: {}, Skipped: {}, Failed: {}",
                                summary.files_processed,
                                summary.rows_processed,
                                summary.rows_imported,
                                summary.rows_skipped,
                                summary.files_failed
                            );
                        }

                        Err(error) => {
                            self.status_message = format!(
                                "Import failed: {}",
                                error
                            );
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

                ui.label(
                    format!(
                        "Status: {}",
                        self.status_message
                    )
                );
            });
    }
}
