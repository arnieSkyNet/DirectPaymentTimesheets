use eframe::egui;

use crate::app::Application;
use crate::models::Employer;

pub struct EmployerScreen {
    drawing: crate::signature::DrawingUi,
    employer: Option<Employer>,
    persisted_employer: Option<Employer>,
    confirm_open_email_settings: bool,
    loaded: bool,
    status_message: String,
}

impl EmployerScreen {
    pub fn new() -> Self {
        Self {
            drawing: Default::default(),
            employer: None,
            persisted_employer: None,
            confirm_open_email_settings: false,
            loaded: false,
            status_message: "Employer details not loaded.".to_string(),
        }
    }

    pub fn signature_saved(&mut self, owner: crate::signature::Owner, path: &str) {
        if let crate::signature::Owner::Employer(id) = owner {
            for cached in [&mut self.employer, &mut self.persisted_employer] {
                if let Some(record) = cached.as_mut().filter(|r| r.id == id) {
                    record.employer_signature = Some(path.into());
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application) -> bool {
        if let Some((owner, path)) = self.drawing.show(ui.ctx(), application) {
            self.signature_saved(owner, &path);
            self.status_message = "Drawn employer signature saved for future PDFs.".into();
        }
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
            let column_width = (ui.available_width() - ui.spacing().item_spacing.x).max(0.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(column_width * 0.75, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.columns(3, |columns| {
                            columns[0].label("Employer Name");
                            columns[0].add(
                                egui::TextEdit::singleline(&mut employer.name)
                                    .desired_width(columns[0].available_width()),
                            );

                            columns[1].label("Date of Birth");
                            let dob = employer.date_of_birth.get_or_insert(String::new());
                            crate::date_utils::edit(
                                &mut columns[1],
                                dob,
                                application.context.config.date_display_format,
                            );

                            columns[2].label("National Insurance Number");
                            let ni = employer
                                .national_insurance_number
                                .get_or_insert(String::new());
                            columns[2].add(
                                egui::TextEdit::singleline(ni)
                                    .desired_width(columns[2].available_width()),
                            );
                        });

                        ui.add_space(ui.spacing().item_spacing.y);
                        ui.columns(3, |columns| {
                            columns[0].label("Email Address");
                            let email = employer.email.get_or_insert(String::new());
                            columns[0].add(
                                egui::TextEdit::singleline(email)
                                    .desired_width(columns[0].available_width()),
                            );

                            columns[1].label("DP Account");
                            let account = employer
                                .reference_account_number
                                .get_or_insert(String::new());
                            columns[1].add(
                                egui::TextEdit::singleline(account)
                                    .desired_width(columns[1].available_width()),
                            );

                            columns[2].label("Telephone Number");
                            let telephone = employer.telephone.get_or_insert(String::new());
                            columns[2].add(
                                egui::TextEdit::singleline(telephone)
                                    .desired_width(columns[2].available_width()),
                            );
                        });

                        ui.add_space(ui.spacing().item_spacing.y);
                        if ui.button("Edit Email Signature").clicked() {
                            if has_unsaved_changes {
                                self.confirm_open_email_settings = true;
                            } else {
                                open_email_settings = true;
                            }
                        }
                    },
                );

                ui.allocate_ui_with_layout(
                    egui::vec2(column_width * 0.25, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label("Address");
                        let address = employer.address.get_or_insert(String::new());
                        ui.add_sized(
                            [ui.available_width(), 140.0],
                            egui::TextEdit::multiline(address),
                        );
                    },
                );
            });

            ui.separator();
            ui.heading("Signature");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Select Signature...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Signature Image", &["png", "jpg", "jpeg", "jfif", "jpe"])
                        .pick_file()
                    {
                        match crate::signature::select_import(&mut employer.employer_signature, &path) {
                            Ok(_) => {
                                self.status_message = "Valid employer signature selected. Save Employer to use this replacement.".into();
                            },
                            Err(e) => self.status_message = e.to_string(),
                        }
                    }
                }

                if ui.add_enabled(employer.id != 0, egui::Button::new("Draw Signature...")).clicked() {
                    if let Err(e) = crate::payroll_evidence::open(application).and_then(|db| self.drawing.open(crate::signature::Owner::Employer(employer.id), &db)) { self.status_message = e.to_string(); }
                }
                if employer.employer_signature.is_some() && ui.button("Clear Signature").clicked() {
                    employer.employer_signature = None;
                    self.status_message = "Employer signature cleared.".to_string();
                }
                if let Some(path) = employer.employer_signature.as_deref() {
                    if let Some(error) =
                        crate::folder_opener::button(ui, std::path::Path::new(path))
                    {
                        self.status_message = error;
                    }
                }
            });

            let signature_text = employer
                .employer_signature
                .as_deref()
                .unwrap_or("No signature selected");
            ui.label(format!("Employer Signature: {}", signature_text));

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
        if self
            .persisted_employer
            .as_ref()
            .and_then(|e| e.employer_signature.as_ref())
            != employer.employer_signature.as_ref()
        {
            if let Some(path) = employer.employer_signature.as_deref() {
                if let Err(e) = crate::signature::read(std::path::Path::new(path)) {
                    self.status_message = e.to_string();
                    return false;
                }
            }
        }
        let expected = self
            .persisted_employer
            .as_ref()
            .and_then(|e| e.employer_signature.as_deref());
        let _signature_guard = match crate::signature::edit_guard(
            application,
            (employer.id != 0).then_some(crate::signature::Owner::Employer(employer.id)),
            expected,
        ) {
            Ok(guard) => guard,
            Err(e) => {
                self.status_message = e.to_string();
                return false;
            }
        };
        let result = if employer.id == 0 {
            application.employer_repository.insert(employer)
        } else {
            application.employer_repository.update(employer)
        };
        match result {
            Ok(()) => {
                if employer.id == 0 {
                    // The drawing action needs the newly assigned durable owner ID.
                    if let Ok(records) = application.employer_repository.get_all() {
                        self.employer = records.into_iter().next();
                    }
                }
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
            drawing: Default::default(),
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
    #[test]
    fn drawn_signature_cache_refresh_preserves_edits_and_owner_identity() {
        let mut screen = EmployerScreen::new();
        screen.employer = Some(employer(1, "Unsaved name", Some("email text")));
        screen.persisted_employer = Some(employer(1, "Saved name", Some("email text")));
        screen.signature_saved(crate::signature::Owner::Pa(1), "wrong.png");
        assert_eq!(screen.employer.as_ref().unwrap().employer_signature, None);
        screen.signature_saved(crate::signature::Owner::Employer(1), "drawn.png");
        assert_eq!(screen.employer.as_ref().unwrap().name, "Unsaved name");
        assert_eq!(
            screen.persisted_employer.as_ref().unwrap().name,
            "Saved name"
        );
        assert_eq!(
            screen
                .employer
                .as_ref()
                .unwrap()
                .employer_signature
                .as_deref(),
            Some("drawn.png")
        );
        assert_eq!(
            screen
                .persisted_employer
                .as_ref()
                .unwrap()
                .employer_signature
                .as_deref(),
            Some("drawn.png")
        );
    }
}
