use eframe::egui;

use crate::app::Application;
use crate::models::Employer;

pub struct EmployerScreen {
    employer: Option<Employer>,
    loaded: bool,
    status_message: String,
}

impl EmployerScreen {
    pub fn new() -> Self {
        Self {
            employer: None,
            loaded: false,
            status_message: "Employer details not loaded.".to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application) {
        if !self.loaded {
            self.load(application);

            self.loaded = true;
        }

        ui.heading("Employer Maintenance");

        ui.separator();

        match &mut self.employer {
            Some(employer) => {
                ui.label("Employer Details");

                ui.add_space(10.0);

                ui.label("Name");
                ui.text_edit_singleline(&mut employer.name);

                ui.label("Address");
                edit_optional_text(ui, &mut employer.address);

                ui.label("Postcode");
                edit_optional_text(ui, &mut employer.postcode);

                ui.label("Telephone");
                edit_optional_text(ui, &mut employer.telephone);

                ui.label("Email");
                edit_optional_text(ui, &mut employer.email);

                ui.separator();

                ui.checkbox(
                    &mut employer.sick_pay_enabled,
                    "Enable sickness hours / SSP",
                );

                ui.checkbox(&mut employer.mileage_enabled, "Enable mileage claims");

                ui.separator();

                if ui.button("Save Employer").clicked() {
                    match application.employer_repository.update(employer) {
                        Ok(()) => {
                            self.status_message =
                                "Employer details saved successfully.".to_string();
                        }

                        Err(error) => {
                            self.status_message = format!("Failed saving employer: {}", error);
                        }
                    }
                }
            }

            None => {
                ui.label("No employer record found.");

                if ui.button("Create Employer").clicked() {
                    let employer = Employer {
                        id: 0,
                        name: "New Employer".to_string(),
                        address: None,
                        postcode: None,
                        telephone: None,
                        email: None,
                        payroll_provider: None,
                        payroll_provider_address: None,
                        payroll_provider_phone: None,
                        employer_signature: None,
                        default_pdf_template: None,
                        sick_pay_enabled: false,
                        mileage_enabled: false,
                    };

                    match application.employer_repository.insert(&employer) {
                        Ok(()) => {
                            self.loaded = false;

                            self.status_message = "Employer created.".to_string();
                        }

                        Err(error) => {
                            self.status_message = format!("Failed creating employer: {}", error);
                        }
                    }
                }
            }
        }

        ui.separator();

        ui.label(&self.status_message);
    }

    fn load(&mut self, application: &Application) {
        match application.employer_repository.get_all() {
            Ok(employers) => {
                self.employer = employers.into_iter().next();

                self.status_message = "Employer loaded.".to_string();
            }

            Err(error) => {
                self.status_message = format!("Failed loading employer: {}", error);
            }
        }
    }
}

fn edit_optional_text(ui: &mut egui::Ui, value: &mut Option<String>) {
    match value {
        Some(text) => {
            ui.text_edit_singleline(text);
        }

        None => {
            let mut new_value = String::new();

            if ui.text_edit_singleline(&mut new_value).changed() {
                *value = Some(new_value);
            }
        }
    }
}
