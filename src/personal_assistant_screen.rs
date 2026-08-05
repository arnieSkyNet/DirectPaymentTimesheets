use eframe::egui;

use crate::app::Application;
use crate::models::PersonalAssistant;
use crate::pay_rate_repository::PersonalAssistantPayRate;

pub struct PersonalAssistantScreen {
    assistants: Vec<PersonalAssistant>,
    selected_index: Option<usize>,
    editing_assistant: Option<PersonalAssistant>,

    pay_rates: Vec<PersonalAssistantPayRate>,
    new_rate_effective_date: String,
    new_rate_base: String,
    new_rate_top_up: String,

    loaded: bool,
    status_message: String,
}

impl PersonalAssistantScreen {
    pub fn new() -> Self {
        Self {
            assistants: Vec::new(),
            selected_index: None,
            editing_assistant: None,

            pay_rates: Vec::new(),
            new_rate_effective_date: String::new(),
            new_rate_base: String::new(),
            new_rate_top_up: String::new(),

            loaded: false,
            status_message: "Personal Assistants not loaded.".to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application) {
        if !self.loaded {
            self.load(application);
            self.loaded = true;
        }

        ui.heading("Personal Assistant Maintenance");

        ui.separator();

        if ui.button("New Personal Assistant").clicked() {
            self.selected_index = None;
            self.editing_assistant = Some(PersonalAssistant {
                id: 0,
                first_name: String::new(),
                surname: String::new(),
                date_of_birth: None,
                national_insurance_number: None,
                address: None,
                postcode: None,
                telephone: None,
                email: None,
                employment_status: Some("Active".to_string()),
                sick_pay_enabled: false,
                mileage_enabled: false,
            });

            self.pay_rates.clear();
            self.status_message = "New Personal Assistant.".to_string();
        }

        ui.separator();

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Existing Assistants");

                for (index, assistant) in self.assistants.iter().enumerate() {
                    if ui
                        .selectable_label(
                            self.selected_index == Some(index),
                            format!("{} {}", assistant.first_name, assistant.surname),
                        )
                        .clicked()
                    {
                        self.selected_index = Some(index);
                        self.editing_assistant = Some(assistant.clone());

                        self.pay_rates = application
                            .pay_rate_repository
                            .get_all_for_personal_assistant(assistant.id)
                            .unwrap_or_default();
                    }
                }
            });

            ui.separator();

            ui.vertical(|ui| {
                ui.heading("Details");

                if let Some(assistant) = &mut self.editing_assistant {
                    ui.label("First Name");
                    ui.text_edit_singleline(&mut assistant.first_name);

                    ui.label("Surname");
                    ui.text_edit_singleline(&mut assistant.surname);

                    ui.label("Date of Birth");
                    edit_optional_text(ui, &mut assistant.date_of_birth);

                    ui.label("National Insurance Number");
                    edit_optional_text(ui, &mut assistant.national_insurance_number);

                    ui.label("Address");
                    edit_optional_text(ui, &mut assistant.address);

                    ui.label("Postcode");
                    edit_optional_text(ui, &mut assistant.postcode);

                    ui.label("Telephone");
                    edit_optional_text(ui, &mut assistant.telephone);

                    ui.label("Email");
                    edit_optional_text(ui, &mut assistant.email);

                    ui.label("Employment Status");
                    edit_optional_text(ui, &mut assistant.employment_status);

                    ui.checkbox(
                        &mut assistant.sick_pay_enabled,
                        "Enable sickness hours / SSP",
                    );

                    ui.checkbox(&mut assistant.mileage_enabled, "Enable mileage claims");

                    ui.separator();

                    if ui.button("Save Personal Assistant").clicked() {
                        let result = if assistant.id == 0 {
                            application.personal_assistant_repository.insert(assistant)
                        } else {
                            application.personal_assistant_repository.update(assistant)
                        };

                        match result {
                            Ok(()) => {
                                self.status_message = "Personal Assistant saved.".to_string();
                                self.loaded = false;
                            }
                            Err(error) => {
                                self.status_message = format!("Save failed: {}", error);
                            }
                        }
                    }

                    ui.separator();

                    ui.heading("Pay Rates");

                    if assistant.id == 0 {
                        ui.label("Save the Personal Assistant before adding pay rates.");
                    } else {
                        for rate in &self.pay_rates {
                            ui.label(format!(
                                "{}  £{:.2}  Top up £{:.2}",
                                rate.effective_date,
                                rate.base_hourly_rate,
                                rate.employer_top_up_rate
                            ));
                        }

                        ui.separator();

                        ui.label("Effective Date");
                        ui.text_edit_singleline(&mut self.new_rate_effective_date);

                        ui.label("Base Hourly Rate");
                        ui.text_edit_singleline(&mut self.new_rate_base);

                        ui.label("Employer Top Up Rate");
                        ui.text_edit_singleline(&mut self.new_rate_top_up);

                        if ui.button("Save Pay Rate").clicked() {
                            let rate = PersonalAssistantPayRate {
                                id: 0,
                                personal_assistant_id: assistant.id,
                                effective_date: self.new_rate_effective_date.clone(),
                                base_hourly_rate: self.new_rate_base.parse().unwrap_or(0.0),
                                employer_top_up_rate: self.new_rate_top_up.parse().unwrap_or(0.0),
                                created_at: chrono::Local::now().format("%Y-%m-%d").to_string(),
                            };

                            match application.pay_rate_repository.insert(&rate) {
                                Ok(()) => {
                                    self.pay_rates = application
                                        .pay_rate_repository
                                        .get_all_for_personal_assistant(assistant.id)
                                        .unwrap_or_default();

                                    self.status_message = "Pay rate saved.".to_string();

                                    self.new_rate_effective_date.clear();
                                    self.new_rate_base.clear();
                                    self.new_rate_top_up.clear();
                                }

                                Err(error) => {
                                    self.status_message = format!("Pay rate failed: {}", error);
                                }
                            }
                        }
                    }
                } else {
                    ui.label("Select an assistant or create a new one.");
                }
            });
        });

        ui.separator();

        ui.label(&self.status_message);
    }

    fn load(&mut self, application: &Application) {
        match application.personal_assistant_repository.get_all() {
            Ok(assistants) => {
                self.assistants = assistants;
                self.status_message = "Personal Assistants loaded.".to_string();
            }

            Err(error) => {
                self.status_message = format!("Failed loading Personal Assistants: {}", error);
            }
        }
    }
}

fn edit_optional_text(ui: &mut egui::Ui, value: &mut Option<String>) {
    if value.is_none() {
        *value = Some(String::new());
    }

    if let Some(text) = value {
        ui.text_edit_singleline(text);
    }
}
