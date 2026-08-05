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

        if ui.button("New Employer").clicked() {
            self.employer = Some(Employer {
                id: 0,
                name: String::new(),
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
            });

            self.status_message = "New employer.".to_string();
        }

        ui.separator();

        match &mut self.employer {
            Some(employer) => {
                ui.label("Employer Details");

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

                ui.label("Payroll Provider");

                ui.label("Provider Name");
                edit_optional_text(ui, &mut employer.payroll_provider);

                ui.label("Provider Address");
                edit_optional_text(ui, &mut employer.payroll_provider_address);

                ui.label("Provider Telephone");
                edit_optional_text(ui, &mut employer.payroll_provider_phone);

                ui.separator();

                ui.label("Documents");

                ui.label("Employer Signature");
                edit_optional_text(ui, &mut employer.employer_signature);

                ui.label("Default PDF Template");
                edit_optional_text(ui, &mut employer.default_pdf_template);

                ui.separator();

                ui.checkbox(
                    &mut employer.sick_pay_enabled,
                    "Enable sickness hours / SSP",
                );

                ui.checkbox(&mut employer.mileage_enabled, "Enable mileage claims");

                ui.separator();

                if ui.button("Save Employer").clicked() {
                    let result = if employer.id == 0 {
                        application.employer_repository.insert(employer)
                    } else {
                        application.employer_repository.update(employer)
                    };

                    match result {
                        Ok(()) => {
                            self.status_message = "Employer saved successfully.".to_string();
                        }

                        Err(error) => {
                            self.status_message = format!("Failed saving employer: {}", error);
                        }
                    }
                }
            }

            None => {
                ui.label("No employer loaded.");

                ui.label("Click New Employer to create one.");
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
    if value.is_none() {
        *value = Some(String::new());
    }

    if let Some(text) = value {
        ui.text_edit_singleline(text);
    }
}
