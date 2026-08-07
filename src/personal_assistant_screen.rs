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
    editing_pay_rate_id: Option<i64>,

    contracted_hours: Vec<crate::contracted_hours_repository::ContractedHoursEntry>,
    new_hours_effective_date: String,
    new_contracted_hours: String,
    editing_contracted_hours_id: Option<i64>,
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
            editing_pay_rate_id: None,
            contracted_hours: Vec::new(),
            new_hours_effective_date: String::new(),
            new_contracted_hours: String::new(),
            editing_contracted_hours_id: None,

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
                start_date: None,
            });

            self.pay_rates.clear();
            self.contracted_hours.clear();

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

                        self.contracted_hours = application
                            .contracted_hours_repository
                            .get_all_for_personal_assistant(assistant.id)
                            .unwrap_or_default();
                    }
                }
            });

            ui.separator();

            ui.vertical(|ui| {
                ui.heading("Details");

                if let Some(assistant) = &mut self.editing_assistant {
                    ui.columns(2, |columns| {
                        columns[0].label("First Name");
                        columns[0].text_edit_singleline(&mut assistant.first_name);

                        columns[1].label("Surname");
                        columns[1].text_edit_singleline(&mut assistant.surname);

                        columns[0].label("Date of Birth");
                        edit_optional_text(&mut columns[0], &mut assistant.date_of_birth);

                        columns[1].label("Address");
                        edit_optional_multiline(&mut columns[1], &mut assistant.address);

                        columns[0].label("National Insurance Number");
                        edit_optional_text(
                            &mut columns[0],
                            &mut assistant.national_insurance_number,
                        );

                        columns[0].label("Email Address");
                        edit_optional_text(&mut columns[0], &mut assistant.email);

                        columns[1].label("Telephone Number");
                        edit_optional_text(&mut columns[1], &mut assistant.telephone);
                    });

                    ui.separator();

                    let mut active = assistant.employment_status.as_deref() == Some("Active");

                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut active, "Active").changed() {
                            assistant.employment_status = if active {
                                Some("Active".to_string())
                            } else {
                                Some("Inactive".to_string())
                            };
                        }

                        ui.checkbox(&mut assistant.sick_pay_enabled, "Enable sickness");
                        ui.checkbox(&mut assistant.mileage_enabled, "Enable mileage");

                        ui.label("Start date");

                        if assistant.start_date.is_none() {
                            assistant.start_date = Some(String::new());
                        }

                        if let Some(start_date) = &mut assistant.start_date {
                            ui.text_edit_singleline(start_date);
                        }
                    });

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

                    ui.heading("Contracted Weekly Hours");

                    if assistant.id == 0 {
                        ui.label("Save the Personal Assistant before adding contracted hours.");
                    } else {
                        for hours in self.contracted_hours.clone() {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{}  {}",
                                    hours.effective_date, hours.contracted_hours
                                ));

                                if ui.button("Edit").clicked() {
                                    self.editing_contracted_hours_id = Some(hours.id);

                                    self.new_hours_effective_date = hours.effective_date.clone();

                                    self.new_contracted_hours = hours.contracted_hours.clone();
                                }

                                if ui.button("Delete").clicked() {
                                    match application.contracted_hours_repository.delete(hours.id) {
                                        Ok(()) => {
                                            self.contracted_hours = application
                                                .contracted_hours_repository
                                                .get_all_for_personal_assistant(assistant.id)
                                                .unwrap_or_default();

                                            self.status_message =
                                                "Contracted hours deleted.".to_string();
                                        }

                                        Err(error) => {
                                            self.status_message =
                                                format!("Delete failed: {}", error);
                                        }
                                    }
                                }
                            });
                        }

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label("Effective Date");

                            ui.text_edit_singleline(&mut self.new_hours_effective_date);

                            ui.label("Contracted Hours");

                            ui.text_edit_singleline(&mut self.new_contracted_hours);
                        });

                        if ui.button("Save Contracted Hours").clicked() {
                            let hours = crate::contracted_hours_repository::ContractedHoursEntry {
                                id: self.editing_contracted_hours_id.unwrap_or(0),
                                personal_assistant_id: assistant.id,
                                effective_date: self.new_hours_effective_date.clone(),
                                contracted_hours: self.new_contracted_hours.clone(),
                                created_at: chrono::Local::now().format("%Y-%m-%d").to_string(),
                            };

                            let result = if self.editing_contracted_hours_id.is_some() {
                                application.contracted_hours_repository.update(&hours)
                            } else {
                                application.contracted_hours_repository.insert(&hours)
                            };

                            match result {
                                Ok(()) => {
                                    self.contracted_hours = application
                                        .contracted_hours_repository
                                        .get_all_for_personal_assistant(assistant.id)
                                        .unwrap_or_default();

                                    self.new_hours_effective_date.clear();
                                    self.new_contracted_hours.clear();
                                    self.editing_contracted_hours_id = None;

                                    self.status_message = "Contracted hours saved.".to_string();
                                }

                                Err(error) => {
                                    self.status_message =
                                        format!("Contracted hours save failed: {}", error);
                                }
                            }
                        }
                    }

                    ui.separator();

                    ui.heading("Pay Rates");

                    if assistant.id == 0 {
                        ui.label("Save the Personal Assistant before adding pay rates.");
                    } else {
                        for rate in self.pay_rates.clone() {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{}  £{:.2}  Top up £{:.2}",
                                    rate.effective_date,
                                    rate.base_hourly_rate,
                                    rate.employer_top_up_rate
                                ));

                                if ui.button("Edit").clicked() {
                                    self.editing_pay_rate_id = Some(rate.id);

                                    self.new_rate_effective_date = rate.effective_date.clone();

                                    self.new_rate_base = rate.base_hourly_rate.to_string();

                                    self.new_rate_top_up = rate.employer_top_up_rate.to_string();
                                }

                                if ui.button("Delete").clicked() {
                                    match application.pay_rate_repository.delete(rate.id) {
                                        Ok(()) => {
                                            self.pay_rates = application
                                                .pay_rate_repository
                                                .get_all_for_personal_assistant(assistant.id)
                                                .unwrap_or_default();

                                            self.status_message = "Pay rate deleted.".to_string();
                                        }

                                        Err(error) => {
                                            self.status_message =
                                                format!("Delete failed: {}", error);
                                        }
                                    }
                                }
                            });
                        }

                        ui.separator();

                        ui.columns(3, |columns| {
                            columns[0].label("Effective Date");
                            columns[0].text_edit_singleline(&mut self.new_rate_effective_date);

                            columns[1].label("Base Hourly Rate");
                            columns[1].text_edit_singleline(&mut self.new_rate_base);

                            columns[2].label("Employer Top Up Rate");
                            columns[2].text_edit_singleline(&mut self.new_rate_top_up);
                        });

                        let button_text = if self.editing_pay_rate_id.is_some() {
                            "Update Pay Rate"
                        } else {
                            "Save Pay Rate"
                        };

                        if ui.button(button_text).clicked() {
                            let rate = PersonalAssistantPayRate {
                                id: self.editing_pay_rate_id.unwrap_or(0),
                                personal_assistant_id: assistant.id,
                                effective_date: self.new_rate_effective_date.clone(),
                                base_hourly_rate: self.new_rate_base.parse().unwrap_or(0.0),
                                employer_top_up_rate: self.new_rate_top_up.parse().unwrap_or(0.0),
                                created_at: chrono::Local::now().format("%Y-%m-%d").to_string(),
                            };

                            let result = if self.editing_pay_rate_id.is_some() {
                                application.pay_rate_repository.update(&rate)
                            } else {
                                application.pay_rate_repository.insert(&rate)
                            };

                            match result {
                                Ok(()) => {
                                    self.pay_rates = application
                                        .pay_rate_repository
                                        .get_all_for_personal_assistant(assistant.id)
                                        .unwrap_or_default();

                                    self.status_message = "Pay rate saved.".to_string();

                                    self.new_rate_effective_date.clear();
                                    self.new_rate_base.clear();
                                    self.new_rate_top_up.clear();

                                    self.editing_pay_rate_id = None;
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

fn edit_optional_multiline(ui: &mut egui::Ui, value: &mut Option<String>) {
    if value.is_none() {
        *value = Some(String::new());
    }

    if let Some(text) = value {
        ui.add(egui::TextEdit::multiline(text).desired_rows(4));
    }
}
