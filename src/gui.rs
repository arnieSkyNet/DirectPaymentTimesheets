use eframe::egui;

pub struct DirectPaymentApp {
    version: String,
}

impl DirectPaymentApp {
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("DirectPaymentTimesheets");

            ui.label(format!(
                "Version {}",
                self.version
            ));

            ui.separator();

            ui.label("Application foundation ready.");

            ui.add_space(20.0);

            if ui.button("Import CSV").clicked() {
                println!("Import CSV clicked");
            }

            if ui.button("View Timesheets").clicked() {
                println!("View Timesheets clicked");
            }

            if ui.button("Settings").clicked() {
                println!("Settings clicked");
            }

            if ui.button("Exit").clicked() {
                ctx.send_viewport_cmd(
                    egui::ViewportCommand::Close,
                );
            }
        });
    }
}
