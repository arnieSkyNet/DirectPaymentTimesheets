use eframe::egui;

use crate::app::Application;
use crate::models::Employer;

pub struct EmployerScreen {
    employer: Option<Employer>,
    persisted_employer: Option<Employer>,
    confirm_open_email_settings: bool,
    loaded: bool,
    status_message: String,
}

impl EmployerScreen {
    pub fn new() -> Self {
        Self {
            employer: None,
            persisted_employer: None,
            confirm_open_email_settings: false,
            loaded: false,
            status_message: "Employer details not loaded.".to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application) -> bool {
        let mut open_email_settings = false;
        if !self.loaded {
            self.load(application);
            self.loaded = true;
        }

        ui.horizontal(|ui| {
            ui.heading("Employer Maintenance");

            if self.employer.is_none() {
                if ui.button("New Employer").clicked() {
                    self.employer = Some(Employer {
                        id: 0,
                        name: String::new(),
                        date_of_birth: None,
                        national_insurance_number: None,
                        reference_account_number: None,
                        address: None,
                        telephone: None,
                        email: None,
                        employer_signature: None,
                        email_signature: None,
                        default_pdf_template: None,
                        sick_pay_enabled: false,
                        mileage_enabled: false,
                    });

                    self.status_message = "New employer.".to_string();
                }
            }
        });

        ui.separator();

        let has_unsaved_changes = self.employer != self.persisted_employer;
        if self.confirm_open_email_settings {
            egui::Window::new("Employer Maintenance has unsaved changes.")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Save Changes and Continue").clicked() {
                            if self.save_employer(application) {
                                open_email_settings = true;
                                self.confirm_open_email_settings = false;
                            }
                        }
                        if ui.button("Continue Without Saving").clicked() {
                            self.employer = self.persisted_employer.clone();
                            self.confirm_open_email_settings = false;
                            open_email_settings = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_open_email_settings = false;
                        }
                    });
                });
        }

        if let Some(employer) = &mut self.employer {
            ui.columns(3, |columns| {
                // LEFT COLUMN

                columns[0].label("Employer Name");

                columns[0].text_edit_singleline(&mut employer.name);

                columns[0].add_space(15.0);

                columns[0].label("Email Address");

                let email = employer.email.get_or_insert(String::new());

                columns[0].text_edit_singleline(email);

                if columns[0].button("Edit Email Signature").clicked() {
                    if has_unsaved_changes {
                        self.confirm_open_email_settings = true;
                    } else {
                        open_email_settings = true;
                    }
                }

                columns[0].add_space(15.0);

                columns[0].label("Telephone Number");

                let telephone = employer.telephone.get_or_insert(String::new());

                columns[0].text_edit_singleline(telephone);

                // MIDDLE COLUMN

                columns[1].label("Date of Birth");

                let dob = employer.date_of_birth.get_or_insert(String::new());

                columns[1].add_sized([120.0, 20.0], egui::TextEdit::singleline(dob));

                columns[1].add_space(15.0);

                columns[1].label("National Insurance Number");

                let ni = employer
                    .national_insurance_number
                    .get_or_insert(String::new());

                columns[1].add_sized([140.0, 20.0], egui::TextEdit::singleline(ni));

                columns[1].add_space(15.0);

                columns[1].label("DP Account");

                let account = employer
                    .reference_account_number
                    .get_or_insert(String::new());

                columns[1].add_sized([140.0, 20.0], egui::TextEdit::singleline(account));

                // RIGHT COLUMN

                columns[2].label("Address");

                let address = employer.address.get_or_insert(String::new());

                columns[2].add_sized([250.0, 140.0], egui::TextEdit::multiline(address));
            });

            ui.separator();

            ui.heading("Signature");

            ui.horizontal(|ui| {
                ui.label("Employer Signature");

                let signature_text = employer
                    .employer_signature
                    .as_deref()
                    .unwrap_or("No signature selected");

                ui.label(signature_text);

                if ui.button("Select Signature...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Signature Image", &["png", "jpg", "jpeg"])
                        .pick_file()
                    {
                        employer.employer_signature = Some(path.to_string_lossy().to_string());

                        self.status_message = "Employer signature selected.".to_string();
                    }
                }

                if employer.employer_signature.is_some() && ui.button("Clear Signature").clicked() {
                    employer.employer_signature = None;
                    self.status_message = "Employer signature cleared.".to_string();
                }
            });

            ui.separator();

            ui.heading("Enable PA Features");

            ui.horizontal(|ui| {
                ui.checkbox(&mut employer.sick_pay_enabled, "Sickness / SSP");

                ui.checkbox(&mut employer.mileage_enabled, "Mileage Claims");
            });

            ui.separator();

            if ui.button("Save Employer").clicked() {
                self.save_employer(application);
            }
        } else {
            ui.label("No employer loaded.");
        }

        ui.separator();

        ui.label(&self.status_message);
        open_email_settings
    }

    pub fn synchronize_email_signature(&mut self, saved_employer: &Employer) {
        for cached_employer in [&mut self.employer, &mut self.persisted_employer] {
            if let Some(cached_employer) = cached_employer {
                if cached_employer.id == saved_employer.id {
                    cached_employer.email_signature = saved_employer.email_signature.clone();
                }
            }
        }
    }

    fn save_employer(&mut self, application: &Application) -> bool {
        let Some(employer) = &self.employer else {
            return false;
        };
        let result = if employer.id == 0 {
            application.employer_repository.insert(employer)
        } else {
            application.employer_repository.update(employer)
        };
        match result {
            Ok(()) => {
                self.persisted_employer = self.employer.clone();
                self.status_message = "Employer saved successfully.".to_string();
                true
            }
            Err(error) => {
                self.status_message = format!("Failed saving employer: {}", error);
                false
            }
        }
    }

    fn load(&mut self, application: &Application) {
        match application.employer_repository.get_all() {
            Ok(employers) => {
                self.employer = employers.into_iter().next();
                self.persisted_employer = self.employer.clone();

                self.status_message = "Employer loaded.".to_string();
            }

            Err(error) => {
                self.status_message = format!("Failed loading employer: {}", error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn employer(id: i64, name: &str, email_signature: Option<&str>) -> Employer {
        Employer {
            id,
            name: name.to_string(),
            date_of_birth: None,
            national_insurance_number: None,
            reference_account_number: None,
            address: None,
            telephone: None,
            email: None,
            employer_signature: None,
            email_signature: email_signature.map(str::to_string),
            default_pdf_template: None,
            sick_pay_enabled: false,
            mileage_enabled: false,
        }
    }

    #[test]
    fn synchronizing_email_signature_preserves_unrelated_unsaved_edits() {
        let mut screen = EmployerScreen {
            employer: Some(employer(1, "Unsaved employer name", Some("Old signature"))),
            persisted_employer: Some(employer(1, "Saved employer name", Some("Old signature"))),
            confirm_open_email_settings: false,
            loaded: true,
            status_message: String::new(),
        };
        let saved = employer(1, "Saved employer name", Some("New signature"));

        screen.synchronize_email_signature(&saved);

        assert_eq!(
            screen.employer.as_ref().unwrap().email_signature.as_deref(),
            Some("New signature")
        );
        assert_eq!(
            screen
                .persisted_employer
                .as_ref()
                .unwrap()
                .email_signature
                .as_deref(),
            Some("New signature")
        );
        assert_eq!(
            screen.employer.as_ref().unwrap().name,
            "Unsaved employer name"
        );
        assert_ne!(screen.employer, screen.persisted_employer);
    }
}
