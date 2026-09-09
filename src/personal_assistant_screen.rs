use crate::contracted_hours_repository::HoursBasis;
use eframe::egui;

use crate::app::Application;
use crate::models::PersonalAssistant;
use crate::pay_rate_repository::PersonalAssistantPayRate;
use crate::personal_assistant_repository::PersonalAssistantDeleteResult;

pub struct PersonalAssistantScreen {
    assistants: Vec<PersonalAssistant>,
    annual_leave: crate::annual_leave_summary::AnnualLeaveSummaryUi,
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
    new_hours_basis: HoursBasis,
    editing_contracted_hours_id: Option<i64>,
    confirm_delete: bool,
    loaded: bool,
    refresh_after_save: bool,
    status_message: String,
}

impl PersonalAssistantScreen {
    pub fn new() -> Self {
        Self {
            assistants: Vec::new(),
            annual_leave: Default::default(),
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
            new_hours_basis: HoursBasis::Contracted,
            editing_contracted_hours_id: None,

            confirm_delete: false,

            loaded: false,
            refresh_after_save: false,
            status_message: "Personal Assistants not loaded.".to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application) {
        if !self.loaded {
            self.load(application);
            self.loaded = true;
        }

        ui.heading("Personal Assistant Maintenance");

        if self.confirm_delete {
            let existing_assistant = self
                .editing_assistant
                .as_ref()
                .filter(|assistant| assistant.id != 0)
                .map(|assistant| {
                    (
                        assistant.id,
                        format!("{} {}", assistant.first_name, assistant.surname),
                    )
                });

            if let Some((personal_assistant_id, assistant_name)) = existing_assistant {
                let mut confirm = false;
                let mut cancel = false;

                egui::Window::new("Permanently delete Personal Assistant?")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        ui.label(format!(
                            "Permanently delete {}? This action cannot be undone.",
                            assistant_name
                        ));
                        ui.horizontal(|ui| {
                            confirm = ui
                                .button(
                                    egui::RichText::new("Confirm Permanent Deletion")
                                        .color(egui::Color32::RED),
                                )
                                .clicked();
                            cancel = ui.button("Cancel").clicked();
                        });
                    });

                if cancel {
                    self.confirm_delete = false;
                } else if confirm {
                    match application
                        .personal_assistant_repository
                        .delete_if_unreferenced(personal_assistant_id)
                    {
                        Ok(PersonalAssistantDeleteResult::Deleted) => {
                            self.editing_assistant = None;
                            self.selected_index = None;
                            self.pay_rates.clear();
                            self.contracted_hours.clear();
                            self.confirm_delete = false;
                            self.loaded = false;
                            self.status_message = format!(
                                "Personal Assistant {} permanently deleted.",
                                assistant_name
                            );
                        }
                        Ok(PersonalAssistantDeleteResult::HasDependentRecords) => {
                            self.confirm_delete = false;
                            self.status_message = historical_records_message(&assistant_name);
                        }
                        Ok(PersonalAssistantDeleteResult::NotFound) => {
                            self.confirm_delete = false;
                            self.loaded = false;
                            self.status_message =
                                "Personal Assistant was not found; the list will be refreshed."
                                    .to_string();
                        }
                        Err(error) => {
                            self.confirm_delete = false;
                            self.status_message =
                                format!("Failed deleting Personal Assistant: {}", error);
                        }
                    }
                }
            } else {
                self.confirm_delete = false;
            }
        }

        ui.separator();

        if ui.button("New Personal Assistant").clicked() {
            self.confirm_delete = false;
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
                leaving_date: None,
                signature: None,
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
                        self.confirm_delete = false;
                        self.selected_index = Some(index);
                        self.editing_assistant = Some(assistant.clone());

                        self.pay_rates = application
                            .pay_rate_repository
                            .get_all_for_personal_assistant(assistant.id)
                            .unwrap_or_default();

                        self.annual_leave.invalidate();
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
                        crate::date_utils::edit(&mut columns[0], assistant.date_of_birth.get_or_insert_with(String::new), application.context.config.date_display_format);

                        columns[1].label("Address");
                        edit_optional_multiline(&mut columns[1], &mut assistant.address);

                        columns[1].label("Postcode");
                        edit_optional_text(&mut columns[1], &mut assistant.postcode);

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

                    ui.horizontal(|ui| {
                        ui.heading("Signature");

                        if ui.button("Select Signature...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Signature Image", &["png", "jpg", "jpeg"])
                                .pick_file()
                            {
                                assistant.signature = Some(path.to_string_lossy().to_string());

                                self.status_message = "PA signature selected.".to_string();
                            }
                        }

                        if assistant.signature.is_some() && ui.button("Clear Signature").clicked() {
                            assistant.signature = None;
                            self.status_message = "PA signature cleared.".to_string();
                        }
                    });

                    let signature_text = assistant
                        .signature
                        .as_deref()
                        .unwrap_or("No signature selected");

                    ui.label(format!("PA Signature: {}", signature_text));

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
                    });

                    ui.horizontal(|ui| {
                        ui.label("Start date");

                        if assistant.start_date.is_none() {
                            assistant.start_date = Some(String::new());
                        }

                        if let Some(start_date) = &mut assistant.start_date {
                            crate::date_utils::edit(ui, start_date, application.context.config.date_display_format);
                        }
                        ui.label("Leaving date (optional)");
                        crate::date_utils::edit(ui, assistant.leaving_date.get_or_insert_with(String::new), application.context.config.date_display_format);
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.button("Save Personal Assistant").clicked() {
                            let result = if assistant.id == 0 {
                                application.personal_assistant_repository.insert(assistant)
                            } else {
                                application.personal_assistant_repository.update(assistant)
                            };

                            match result {
                                Ok(()) => {
                                    self.status_message = "Personal Assistant saved.".to_string();
                                    self.annual_leave.invalidate();
                                    self.refresh_after_save = true;
                                    self.loaded = false;
                                }

                                Err(error) => {
                                    self.status_message = format!("Save failed: {}", error);
                                }
                            }
                        }

                        if assistant.id != 0
                            && ui
                                .button(
                                    egui::RichText::new("Delete Personal Assistant")
                                        .color(egui::Color32::RED),
                                )
                                .clicked()
                        {
                            let assistant_name =
                                format!("{} {}", assistant.first_name, assistant.surname);
                            match application
                                .personal_assistant_repository
                                .has_dependent_records(assistant.id)
                            {
                                Ok(true) => {
                                    self.status_message =
                                        historical_records_message(&assistant_name);
                                }
                                Ok(false) => self.confirm_delete = true,
                                Err(error) => {
                                    self.status_message = format!(
                                        "Unable to check whether this Personal Assistant can be deleted: {}",
                                        error
                                    );
                                }
                            }
                        }
                    });

                    ui.separator();

                    self.annual_leave.show(ui, application, assistant.id);
                    ui.separator();
                    ui.heading("Contracted Weekly Hours");

                    if assistant.id == 0 {
                        ui.label("Save the Personal Assistant before adding contracted hours.");
                    } else {
                        for hours in self.contracted_hours.clone() {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{}  {}  {}",
                                    application.context.config.date_display_format.display(&hours.effective_date), hours.hours_basis.label(),
                                    if hours.hours_basis == HoursBasis::Contracted { &hours.contracted_hours } else { "" }
                                ));

                                if ui.button("Edit").clicked() {
                                    self.editing_contracted_hours_id = Some(hours.id);

                                    self.new_hours_effective_date = hours.effective_date.clone();

                                    self.new_contracted_hours = hours.contracted_hours.clone();
                                    self.new_hours_basis = hours.hours_basis;
                                }

                                if ui.button("Delete").clicked() {
                                    match application.contracted_hours_repository.delete(hours.id) {
                                        Ok(()) => {
                                            self.annual_leave.invalidate();
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

                        ui.horizontal_wrapped(|ui| {
                            ui.label("Effective Date");

                            crate::date_utils::edit(ui, &mut self.new_hours_effective_date, application.context.config.date_display_format);

                            crate::gui_controls::combo_box("contracted_hours_basis")
                                .selected_text(self.new_hours_basis.label())
                                .show_ui(ui, |ui| {
                                    for basis in [HoursBasis::Contracted, HoursBasis::Variable] {
                                        crate::gui_controls::combo_value(ui, &mut self.new_hours_basis, basis, basis.label());
                                    }
                                });
                            ui.label("Contracted Hours (required for Contracted)");
                            ui.add_enabled(self.new_hours_basis == HoursBasis::Contracted,
                                egui::TextEdit::singleline(&mut self.new_contracted_hours));
                            if self.new_hours_basis == HoursBasis::Variable { ui.label("Not applicable"); }
                        });

                        if ui.button("Save Contracted Hours").clicked() {
                            let hours = crate::contracted_hours_repository::ContractedHoursEntry {
                                id: self.editing_contracted_hours_id.unwrap_or(0),
                                personal_assistant_id: assistant.id,
                                effective_date: self.new_hours_effective_date.clone(),
                                contracted_hours: self.new_contracted_hours.clone(),
                                hours_basis: self.new_hours_basis,
                                created_at: chrono::Local::now().format("%Y-%m-%d").to_string(),
                            };

                            let result = if self.editing_contracted_hours_id.is_some() {
                                application.contracted_hours_repository.update(&hours)
                            } else {
                                application.contracted_hours_repository.insert(&hours)
                            };

                            match result {
                                Ok(()) => {
                                    self.annual_leave.invalidate();
                                    self.contracted_hours = application
                                        .contracted_hours_repository
                                        .get_all_for_personal_assistant(assistant.id)
                                        .unwrap_or_default();

                                    self.new_hours_effective_date.clear();
                                    self.new_contracted_hours.clear();
                                    self.new_hours_basis = HoursBasis::Contracted;
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
                                    application.context.config.date_display_format.display(&rate.effective_date),
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
                            crate::date_utils::edit(&mut columns[0], &mut self.new_rate_effective_date, application.context.config.date_display_format);

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
        let refresh_after_save = std::mem::take(&mut self.refresh_after_save);
        match application.personal_assistant_repository.get_all() {
            Ok(assistants) => {
                self.assistants = assistants;
                self.status_message = if refresh_after_save {
                    "Personal Assistant saved."
                } else {
                    "Personal Assistants loaded."
                }
                .to_string();
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

fn historical_records_message(assistant_name: &str) -> String {
    format!(
        "{} cannot be permanently deleted because historical payroll or timesheet data exists. Change their employment status to inactive/no longer employed instead.",
        assistant_name
    )
}

#[cfg(test)]
mod save_status_tests {
    use super::*;

    #[test]
    fn successful_save_refresh_keeps_confirmation_but_load_failure_remains_visible() {
        let (_directory, application) = crate::payroll_timesheet_screen::tests::test_application();
        let mut screen = PersonalAssistantScreen::new();
        screen.load(&application);
        assert_eq!(screen.status_message, "Personal Assistants loaded.");
        screen.refresh_after_save = true;
        screen.load(&application);
        assert_eq!(screen.status_message, "Personal Assistant saved.");
        assert!(!screen.refresh_after_save);
        // An unrelated later load retains its original status behaviour.
        screen.load(&application);
        assert_eq!(screen.status_message, "Personal Assistants loaded.");
        rusqlite::Connection::open(&application.context.environment.database_path)
            .unwrap()
            .execute("DROP TABLE personal_assistants", [])
            .unwrap();
        screen.refresh_after_save = true;
        screen.load(&application);
        assert!(screen
            .status_message
            .starts_with("Failed loading Personal Assistants:"));
        assert!(!screen.refresh_after_save);
    }
}
