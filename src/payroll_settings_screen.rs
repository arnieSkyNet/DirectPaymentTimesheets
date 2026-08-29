use eframe::egui;

use crate::app::Application;
use crate::pay_rate_repository::PersonalAssistantPayRate;
use crate::payroll_provider_repository::PayrollProvider;
use chrono::Local;

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
    timesheet_email_body: String,
    standard_rate_effective_date: String,
    standard_rate_base: String,
    standard_rate_top_up: String,
    overtime_enabled: bool,
    public_holiday_enabled: bool,

    status_message: String,
    confirm_bulk_pay_rate_update: bool,
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
            timesheet_email_body: String::new(),
            standard_rate_effective_date: String::new(),
            standard_rate_base: String::new(),
            standard_rate_top_up: String::new(),
            overtime_enabled: false,
            public_holiday_enabled: false,

            status_message: "Payroll settings not loaded.".to_string(),
            confirm_bulk_pay_rate_update: false,
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

            columns[1].horizontal(|ui| {
                ui.text_edit_singleline(&mut self.email_subject_format);

                egui::ComboBox::from_id_salt("email_subject_insert_field")
                    .selected_text("Insert Field")
                    .show_ui(ui, |ui| {
                        if ui.button("Personal Assistant Name").clicked() {
                            self.email_subject_format
                                .push_str("{Personal Assistant Name}");
                            ui.close();
                        }

                        if ui.button("Personal Assistant DOB").clicked() {
                            self.email_subject_format
                                .push_str("{Personal Assistant DOB}");
                            ui.close();
                        }

                        if ui.button("Personal Assistant NI").clicked() {
                            self.email_subject_format
                                .push_str("{Personal Assistant NI}");
                            ui.close();
                        }

                        if ui.button("Payroll Period (YYYYMMwWW)").clicked() {
                            self.email_subject_format.push_str("{YYYYMMwWW}");
                            ui.close();
                        }
                    });
            });

            columns[1].label(
                "Available fields: {Personal Assistant Name}, {Personal Assistant DOB},                  {Personal Assistant NI}, {YYYYMMwWW}",
            );
        });

        ui.label("Timesheet Email Body");

        ui.add_sized(
            [600.0, 120.0],
            egui::TextEdit::multiline(&mut self.timesheet_email_body),
        );

        ui.label("The Employer Email Signature is automatically appended to this body.");

        ui.separator();
        ui.heading("National / Standard Pay Rate Update");

        ui.columns(3, |columns| {
            columns[0].label("Effective date:");
            columns[0].text_edit_singleline(&mut self.standard_rate_effective_date);

            columns[1].label("Basic hourly rate:");
            columns[1].text_edit_singleline(&mut self.standard_rate_base);

            columns[2].label("Employer top up rate:");
            columns[2].text_edit_singleline(&mut self.standard_rate_top_up);
        });

        if ui
            .button("Create new pay-rate entry for all active Personal Assistants")
            .clicked()
        {
            self.confirm_bulk_pay_rate_update = true;
        }

        if self.confirm_bulk_pay_rate_update {
            ui.separator();

            ui.label("Create new pay-rate entry for all active Personal Assistants?");

            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    self.confirm_bulk_pay_rate_update = false;
                }

                if ui.button("Create Pay Rates").clicked() {
                    match create_bulk_pay_rates(
                        application,
                        &self.standard_rate_effective_date,
                        &self.standard_rate_base,
                        &self.standard_rate_top_up,
                    ) {
                        Ok(count) => {
                            self.status_message = format!(
                                "Created new pay-rate entries for {} active Personal Assistants.",
                                count
                            );

                            self.standard_rate_effective_date.clear();
                            self.standard_rate_base.clear();
                            self.standard_rate_top_up.clear();
                        }

                        Err(error) => {
                            self.status_message =
                                format!("Failed creating pay-rate entries: {}", error);
                        }
                    }

                    self.confirm_bulk_pay_rate_update = false;
                }
            });
        }

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

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.overtime_enabled, "Enable overtime calculations");

            ui.add_space(30.0);

            ui.checkbox(
                &mut self.public_holiday_enabled,
                "Enable public holiday payments",
            );
        });

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

        if let Ok(Some(provider)) = application.payroll_provider_repository.get() {
            self.payroll_email = provider.payroll_department_email.unwrap_or_default();
        }

        self.email_subject_format = payroll.email_subject_format.clone();
        self.timesheet_email_body = payroll.timesheet_email_body.clone();

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

        payroll.email_subject_format = self.email_subject_format.clone();
        payroll.timesheet_email_body = self.timesheet_email_body.clone();

        payroll.overtime_enabled = self.overtime_enabled;

        payroll.public_holiday_enabled = self.public_holiday_enabled;

        let provider = PayrollProvider {
            id: 1,
            name: optional_value(&self.provider_name),
            email: optional_value(&self.provider_email),
            address: optional_value(&self.provider_address),
            telephone: optional_value(&self.provider_telephone),
            payroll_department_email: optional_value(&self.payroll_email),
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

fn create_bulk_pay_rates(
    application: &Application,
    effective_date: &str,
    base_rate: &str,
    top_up_rate: &str,
) -> Result<usize, String> {
    let base_hourly_rate: f64 = base_rate
        .parse()
        .map_err(|_| "Basic hourly rate must be a valid number.".to_string())?;

    let employer_top_up_rate: f64 = if top_up_rate.trim().is_empty() {
        0.0
    } else {
        top_up_rate
            .parse()
            .map_err(|_| "Employer top up rate must be a valid number.".to_string())?
    };

    let assistants = application
        .personal_assistant_repository
        .get_active()
        .map_err(|error| error.to_string())?;

    let mut created = 0;

    for assistant in assistants {
        let rate = PersonalAssistantPayRate {
            id: 0,
            personal_assistant_id: assistant.id,
            effective_date: effective_date.to_string(),
            base_hourly_rate,
            employer_top_up_rate,
            created_at: Local::now().format("%Y-%m-%d").to_string(),
        };

        application
            .pay_rate_repository
            .insert(&rate)
            .map_err(|error| error.to_string())?;

        created += 1;
    }

    Ok(created)
}

fn optional_value(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
