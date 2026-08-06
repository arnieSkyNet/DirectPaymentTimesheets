use eframe::egui;

use crate::app::Application;
use crate::payroll_provider_repository::PayrollProvider;

pub struct PayrollSettingsScreen {
    loaded: bool,

    frequency: String,
    rounding_minutes: i64,
    rounding_direction: String,
    start_of_workweek: String,

    payroll_email: String,

    provider_name: String,
    provider_email: String,
    provider_address: String,
    provider_telephone: String,

    email_subject_format: String,

    overtime_enabled: bool,
    public_holiday_enabled: bool,

    status_message: String,
}

impl PayrollSettingsScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,

            frequency: "Every Four Weeks".to_string(),
            rounding_minutes: 15,
            rounding_direction: "Up".to_string(),
            start_of_workweek: "Monday".to_string(),

            payroll_email: String::new(),

            provider_name: String::new(),
            provider_email: String::new(),
            provider_address: String::new(),
            provider_telephone: String::new(),

            email_subject_format: String::new(),

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

        ui.heading("Payroll Provider");

        ui.columns(2, |columns| {
            columns[0].label("Provider Name");
            columns[0].text_edit_singleline(&mut self.provider_name);

            columns[0].label("Provider Email");
            columns[0].text_edit_singleline(&mut self.provider_email);

            columns[0].label("Provider Telephone");
            columns[0].text_edit_singleline(&mut self.provider_telephone);

            columns[1].label("Provider Address");
            columns[1].add(egui::TextEdit::multiline(&mut self.provider_address).desired_rows(5));
        });

        ui.separator();

        ui.heading("Payroll Department");

        ui.columns(2, |columns| {
            columns[0].label("Send Timesheet PDF To");
            columns[0].text_edit_singleline(&mut self.payroll_email);

            columns[1].label("PDF Email Subject Format");
            columns[1].text_edit_singleline(&mut self.email_subject_format);

            columns[1].label("Example: YYYYMMwWW becomes 202604w02");
        });

        ui.separator();

        ui.heading("Payroll Rules");

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Payroll Frequency");

                egui::ComboBox::from_id_salt("payroll_frequency")
                    .selected_text(&self.frequency)
                    .show_ui(ui, |ui| {
                        for option in [
                            "Weekly",
                            "Bi-Weekly",
                            "Semi-Monthly",
                            "Monthly",
                            "Every Four Weeks",
                            "Quarterly",
                        ] {
                            ui.selectable_value(&mut self.frequency, option.to_string(), option);
                        }
                    });
            });

            ui.add_space(30.0);

            ui.vertical(|ui| {
                ui.label("Rounding Minutes");

                egui::ComboBox::from_id_salt("rounding_minutes")
                    .selected_text(format!(
                        "{} Minutes {}",
                        self.rounding_minutes, self.rounding_direction
                    ))
                    .show_ui(ui, |ui| {
                        for (minutes, direction) in [
                            (1, "Up"),
                            (1, "Down"),
                            (5, "Up"),
                            (5, "Down"),
                            (10, "Up"),
                            (10, "Down"),
                            (15, "Up"),
                            (15, "Down"),
                            (30, "Up"),
                            (30, "Down"),
                            (60, "Up"),
                            (60, "Down"),
                        ] {
                            if ui
                                .selectable_label(
                                    self.rounding_minutes == minutes
                                        && self.rounding_direction == direction,
                                    format!("{} Minutes {}", minutes, direction),
                                )
                                .clicked()
                            {
                                self.rounding_minutes = minutes;
                                self.rounding_direction = direction.to_string();
                            }
                        }
                    });
            });

            ui.add_space(30.0);

            ui.vertical(|ui| {
                ui.label("Start of Workweek");

                egui::ComboBox::from_id_salt("workweek_start")
                    .selected_text(&self.start_of_workweek)
                    .show_ui(ui, |ui| {
                        for day in [
                            "Sunday",
                            "Monday",
                            "Tuesday",
                            "Wednesday",
                            "Thursday",
                            "Friday",
                            "Saturday",
                        ] {
                            ui.selectable_value(&mut self.start_of_workweek, day.to_string(), day);
                        }
                    });
            });
        });

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
        self.rounding_minutes = payroll.rounding_minutes;
        self.rounding_direction = payroll.rounding_direction.clone();
        self.start_of_workweek = payroll.start_of_workweek.clone();

        self.payroll_email = payroll.payroll_email.clone().unwrap_or_default();

        self.email_subject_format = payroll.email_subject_format.clone();

        self.overtime_enabled = payroll.overtime_enabled;
        self.public_holiday_enabled = payroll.public_holiday_enabled;

        if let Ok(Some(provider)) = application.payroll_provider_repository.get() {
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

        payroll.rounding_minutes = self.rounding_minutes;
        payroll.rounding_direction = self.rounding_direction.clone();

        payroll.start_of_workweek = self.start_of_workweek.clone();

        payroll.payroll_email = optional_value(&self.payroll_email);

        payroll.email_subject_format = self.email_subject_format.clone();

        payroll.overtime_enabled = self.overtime_enabled;

        payroll.public_holiday_enabled = self.public_holiday_enabled;

        let provider = PayrollProvider {
            id: 1,
            name: optional_value(&self.provider_name),
            email: optional_value(&self.provider_email),
            address: optional_value(&self.provider_address),
            telephone: optional_value(&self.provider_telephone),
        };

        if let Err(error) = application.payroll_provider_repository.save(&provider) {
            self.status_message = format!("Failed saving payroll provider: {}", error);
            return;
        }

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

fn optional_value(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
