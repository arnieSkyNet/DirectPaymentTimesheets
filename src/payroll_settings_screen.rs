use eframe::egui;

use crate::app::Application;

pub struct PayrollSettingsScreen {
    loaded: bool,
    status_message: String,
}

impl PayrollSettingsScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,
            status_message: "Payroll settings not loaded.".to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application) {
        if !self.loaded {
            self.loaded = true;
        }

        ui.heading("Payroll Settings");

        ui.separator();

        ui.label(format!(
            "Payroll frequency: {}",
            application.context.config.payroll.frequency
        ));

        ui.label(format!(
            "Rounding minutes: {}",
            application.context.config.payroll.rounding_minutes
        ));

        ui.label(format!(
            "Overtime enabled: {}",
            application.context.config.payroll.overtime_enabled
        ));

        ui.label(format!(
            "Public holidays enabled: {}",
            application.context.config.payroll.public_holiday_enabled
        ));

        ui.separator();

        ui.label(&self.status_message);
    }
}
