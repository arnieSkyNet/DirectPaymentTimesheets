use eframe::egui;

use crate::app::Application;
use crate::payroll_provider_repository::PayrollProvider;

pub struct PayrollSettingsScreen {
    loaded: bool,

    frequency: String,
    rounding_minutes: String,

    payroll_email: String,

    provider_name: String,
    provider_email: String,
    provider_address: String,
    provider_telephone: String,

    email_subject_format: String,

    overtime_enabled: bool,
    public_holiday_enabled: bool,

    provider_id: i64,

    status_message: String,
}

impl PayrollSettingsScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,

            frequency: String::new(),
            rounding_minutes: String::new(),

            payroll_email: String::new(),

            provider_name: String::new(),
            provider_email: String::new(),
            provider_address: String::new(),
            provider_telephone: String::new(),

            email_subject_format: String::new(),

            overtime_enabled: false,
            public_holiday_enabled: false,

            provider_id: 1,

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

        ui.separator();

        ui.heading("Payroll Department");

        ui.label("Payroll email (To)");
        ui.text_edit_singleline(&mut self.payroll_email);

        ui.separator();

        ui.heading("Payroll Provider");

        ui.label("Provider name");
        ui.text_edit_singleline(&mut self.provider_name);

        ui.label("Provider email");
        ui.text_edit_singleline(&mut self.provider_email);

        ui.label("Provider address");
        ui.add(egui::TextEdit::multiline(&mut self.provider_address).desired_rows(4));

        ui.label("Provider telephone");
        ui.text_edit_singleline(&mut self.provider_telephone);

        ui.separator();

        ui.heading("Email");

        ui.label("Timesheet PDF subject format");
        ui.text_edit_singleline(&mut self.email_subject_format);

        ui.label("Example: YYYYMMwWW becomes 202604w02");

        ui.separator();

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

        self.email_subject_format = payroll.email_subject_format.clone();

        self.overtime_enabled = payroll.overtime_enabled;

        self.public_holiday_enabled = payroll.public_holiday_enabled;

        if let Ok(Some(provider)) = application.payroll_provider_repository.get() {
            self.provider_id = provider.id;

            self.provider_name = provider.name.unwrap_or_default();
            self.provider_email = provider.email.unwrap_or_default();
            self.provider_address = provider.address.unwrap_or_default();
            self.provider_telephone = provider.telephone.unwrap_or_default();
        }

        self.status_message = "Payroll settings loaded.".to_string();
    }

    fn save(&mut self, application: &mut Application) {
        let payroll = &mut application.context.config.payroll;

        payroll.frequency = self.frequency.clone();

        payroll.rounding_minutes = self.rounding_minutes.parse().unwrap_or(15);

        payroll.payroll_email = optional_value(&self.payroll_email);

        payroll.email_subject_format = self.email_subject_format.clone();

        payroll.overtime_enabled = self.overtime_enabled;

        payroll.public_holiday_enabled = self.public_holiday_enabled;

        let provider = PayrollProvider {
            id: self.provider_id,
            name: optional_value(&self.provider_name),
            email: optional_value(&self.provider_email),
            address: optional_value(&self.provider_address),
            telephone: optional_value(&self.provider_telephone),
        };

        match application.payroll_provider_repository.save(&provider) {
            Ok(()) => {}

            Err(error) => {
                self.status_message = format!("Failed saving provider: {}", error);

                return;
            }
        }

        match application.save_config() {
            Ok(()) => {
                self.status_message = "Payroll settings saved.".to_string();
            }

            Err(error) => {
                self.status_message = format!("Failed saving settings: {}", error);
            }
        }
    }
}

fn optional_value(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
