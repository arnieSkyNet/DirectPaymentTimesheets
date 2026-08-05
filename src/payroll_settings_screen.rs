use eframe::egui;

use crate::app::Application;

pub struct PayrollSettingsScreen {
    loaded: bool,
    frequency: String,
    rounding_minutes: String,
    payroll_email: String,
    bcc_email: String,
    overtime_enabled: bool,
    public_holiday_enabled: bool,
    status_message: String,
}

impl PayrollSettingsScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,
            frequency: String::new(),
            rounding_minutes: String::new(),
            payroll_email: String::new(),
            bcc_email: String::new(),
            overtime_enabled: false,
            public_holiday_enabled: false,
            status_message: "Payroll settings not loaded.".to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &mut Application) {
        if !self.loaded {
            self.load(application);
            self.loaded = true;
        }

        ui.heading("Payroll Settings");

        ui.separator();

        ui.label("Payroll frequency");
        ui.text_edit_singleline(&mut self.frequency);

        ui.label("Rounding minutes");
        ui.text_edit_singleline(&mut self.rounding_minutes);

        ui.label("Payroll email");
        ui.text_edit_singleline(&mut self.payroll_email);

        ui.label("BCC email");
        ui.text_edit_singleline(&mut self.bcc_email);

        ui.checkbox(&mut self.overtime_enabled, "Enable overtime calculations");

        ui.checkbox(
            &mut self.public_holiday_enabled,
            "Enable public holiday payments",
        );

        ui.separator();

        if ui.button("Save Payroll Settings").clicked() {
            self.save(application);
        }

        ui.separator();

        ui.label(&self.status_message);
    }

    fn load(&mut self, application: &Application) {
        let payroll = &application.context.config.payroll;

        self.frequency = payroll.frequency.clone();

        self.rounding_minutes = payroll.rounding_minutes.to_string();

        self.payroll_email = payroll.payroll_email.clone().unwrap_or_default();

        self.bcc_email = payroll.bcc_email.clone().unwrap_or_default();

        self.overtime_enabled = payroll.overtime_enabled;

        self.public_holiday_enabled = payroll.public_holiday_enabled;

        self.status_message = "Payroll settings loaded.".to_string();
    }

    fn save(&mut self, application: &mut Application) {
        application.context.config.payroll.frequency = self.frequency.clone();

        application.context.config.payroll.rounding_minutes =
            self.rounding_minutes.parse().unwrap_or(15);

        application.context.config.payroll.payroll_email = if self.payroll_email.trim().is_empty() {
            None
        } else {
            Some(self.payroll_email.clone())
        };

        application.context.config.payroll.bcc_email = if self.bcc_email.trim().is_empty() {
            None
        } else {
            Some(self.bcc_email.clone())
        };

        application.context.config.payroll.overtime_enabled = self.overtime_enabled;

        application.context.config.payroll.public_holiday_enabled = self.public_holiday_enabled;

        match application.save_config() {
            Ok(()) => {
                self.status_message = "Payroll settings saved.".to_string();
            }

            Err(error) => {
                self.status_message = format!("Failed saving payroll settings: {}", error);
            }
        }
    }
}
