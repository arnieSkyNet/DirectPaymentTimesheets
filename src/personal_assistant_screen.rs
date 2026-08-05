use eframe::egui;

use crate::app::Application;
use crate::models::PersonalAssistant;

pub struct PersonalAssistantScreen {
    assistants: Vec<PersonalAssistant>,
    selected_index: Option<usize>,
    editing_assistant: Option<PersonalAssistant>,
    loaded: bool,
    status_message: String,
}

impl PersonalAssistantScreen {
    pub fn new() -> Self {
        Self {
            assistants: Vec::new(),
            selected_index: None,
            editing_assistant: None,
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
                                self.editing_assistant = None;
                            }

                            Err(error) => {
                                self.status_message = format!("Save failed: {}", error);
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
