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
                ui.heading(details_heading(self.editing_assistant.as_ref()));

                if let Some(assistant) = &mut self.editing_assistant {
                    ui.columns(2, |columns| {
                        columns[0].label("Given name(s)");
                        columns[0].text_edit_singleline(&mut assistant.first_name);

                        columns[1].label("Surname");
                        columns[1].text_edit_singleline(&mut assistant.surname);

                    });

                    ui.add_space(3.0);
                    let phone_label = compact_personal_details(ui, assistant, application.context.config.date_display_format);
                    ui.add_space(ui.text_style_height(&egui::TextStyle::Body));
                    // Anchor the address to the actual Phone label, including after
                    // contact wrapping, instead of reserving a competing column.
                    let indent = (phone_label.left() - ui.cursor().left()).max(0.0);
                    let address_width = (ui.available_width() - indent).min(250.0);
                    let employment_width = (indent - ui.spacing().item_spacing.x).max(0.0);
                    // If Phone wrapped close to the left edge, stack gracefully.
                    // Otherwise both blocks share a band, with Address anchored to Phone.
                    let side_by_side = employment_width >= 340.0;
                    if !side_by_side {
                        employment_controls(ui, assistant, application.context.config.date_display_format);
                    }
                    ui.horizontal_top(|ui| {
                        if side_by_side {
                            ui.allocate_ui_with_layout(egui::vec2(employment_width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.set_min_width(employment_width);
                                employment_controls(ui, assistant, application.context.config.date_display_format);
                            });
                        } else {
                            ui.add_space(indent);
                        }
                        ui.allocate_ui_with_layout(egui::vec2(address_width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.label("Address");
                            edit_optional_multiline(ui, &mut assistant.address);
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Postcode");
                                ui.add(egui::TextEdit::singleline(assistant.postcode.get_or_insert_with(String::new)).desired_width(100.0));
                            });
                        });
                    });

                    // Keep the full signature path below both employment and address content.
                    ui.add_space(ui.text_style_height(&egui::TextStyle::Body));
                    ui.horizontal_wrapped(|ui| {
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
                        if let Some(path) = assistant.signature.as_deref() {
                            if let Some(error) = crate::folder_opener::button(ui, std::path::Path::new(path)) {
                                self.status_message = error;
                            }
                        }
                    });

                    let signature_text = assistant
                        .signature
                        .as_deref()
                        .unwrap_or("No signature selected");

                    ui.label(format!("PA Signature: {}", signature_text));

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
                    let assistant_id = assistant.id;
                    maintenance_panels(ui, |ui, panel, section| match panel {
                        0 => self.show_pay_rates(ui, application, assistant_id, section),
                        _ => self.show_contracted_hours(ui, application, assistant_id, section),
                    });
                } else {
                    ui.label("Select an assistant or create a new one.");
                }
            });
        });

        ui.separator();

        ui.label(&self.status_message);
    }

    fn show_contracted_hours(
        &mut self,
        ui: &mut egui::Ui,
        application: &Application,
        assistant_id: i64,
        section: MaintenanceSection,
    ) {
        if section == MaintenanceSection::History {
            ui.heading("Contracted Weekly Hours");
        }
        if assistant_id == 0 && section == MaintenanceSection::History {
            ui.label("Save the Personal Assistant before adding contracted hours.");
        } else if assistant_id != 0 {
            if section == MaintenanceSection::History {
                for hours in self.contracted_hours.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "{}  {}  {}",
                            application
                                .context
                                .config
                                .date_display_format
                                .display(&hours.effective_date),
                            hours.hours_basis.label(),
                            if hours.hours_basis == HoursBasis::Contracted {
                                &hours.contracted_hours
                            } else {
                                ""
                            }
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
                                        .get_all_for_personal_assistant(assistant_id)
                                        .unwrap_or_default();

                                    self.status_message = "Contracted hours deleted.".to_string();
                                }

                                Err(error) => {
                                    self.status_message = format!("Delete failed: {}", error);
                                }
                            }
                        }
                    });
                }

                ui.separator();
            }
            if section == MaintenanceSection::Form {
                compact_form(
                    ui,
                    ["Effective from", "Hours basis", "Weekly hours"],
                    |ui, field| match field {
                        0 => compact_date_edit(
                            ui,
                            &mut self.new_hours_effective_date,
                            application.context.config.date_display_format,
                        ),
                        1 => {
                            crate::gui_controls::combo_box("contracted_hours_basis")
                                .width(90.0)
                                .selected_text(self.new_hours_basis.label())
                                .show_ui(ui, |ui| {
                                    for basis in [HoursBasis::Contracted, HoursBasis::Variable] {
                                        crate::gui_controls::combo_value(
                                            ui,
                                            &mut self.new_hours_basis,
                                            basis,
                                            basis.label(),
                                        );
                                    }
                                });
                        }
                        _ => {
                            weekly_hours_edit(
                                ui,
                                self.new_hours_basis,
                                &mut self.new_contracted_hours,
                            );
                        }
                    },
                );
            }

            let mut save = false;
            if section == MaintenanceSection::Save {
                let actions = history_actions(
                    ui,
                    "Save Hours Basis",
                    self.editing_contracted_hours_id.is_some(),
                );
                save = actions.0;
                if actions.1 {
                    self.cancel_hours_edit();
                }
            }
            if save {
                let hours = crate::contracted_hours_repository::ContractedHoursEntry {
                    id: self.editing_contracted_hours_id.unwrap_or(0),
                    personal_assistant_id: assistant_id,
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
                            .get_all_for_personal_assistant(assistant_id)
                            .unwrap_or_default();

                        self.new_hours_effective_date.clear();
                        self.new_contracted_hours.clear();
                        self.new_hours_basis = HoursBasis::Contracted;
                        self.editing_contracted_hours_id = None;

                        self.status_message = "Contracted hours saved.".to_string();
                    }

                    Err(error) => {
                        self.status_message = format!("Contracted hours save failed: {}", error);
                    }
                }
            }
        }
    }

    fn show_pay_rates(
        &mut self,
        ui: &mut egui::Ui,
        application: &Application,
        assistant_id: i64,
        section: MaintenanceSection,
    ) {
        if section == MaintenanceSection::History {
            ui.heading("Pay Rates");
        }
        if assistant_id == 0 && section == MaintenanceSection::History {
            ui.label("Save the Personal Assistant before adding pay rates.");
        } else if assistant_id != 0 {
            if section == MaintenanceSection::History {
                for rate in self.pay_rates.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "{}  £{:.2}  Top up £{:.2}",
                            application
                                .context
                                .config
                                .date_display_format
                                .display(&rate.effective_date),
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
                                        .get_all_for_personal_assistant(assistant_id)
                                        .unwrap_or_default();

                                    self.status_message = "Pay rate deleted.".to_string();
                                }

                                Err(error) => {
                                    self.status_message = format!("Delete failed: {}", error);
                                }
                            }
                        }
                    });
                }

                ui.separator();
            }
            if section == MaintenanceSection::Form {
                compact_form(
                    ui,
                    ["Effective date", "Base hourly rate", "Employer top-up rate"],
                    |ui, field| match field {
                        0 => compact_date_edit(
                            ui,
                            &mut self.new_rate_effective_date,
                            application.context.config.date_display_format,
                        ),
                        1 => {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_rate_base)
                                    .desired_width(80.0)
                                    .hint_text("12.71"),
                            );
                        }
                        _ => {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_rate_top_up)
                                    .desired_width(80.0)
                                    .hint_text("0.00"),
                            );
                        }
                    },
                );
            }

            let mut save = false;
            if section == MaintenanceSection::Save {
                let actions =
                    history_actions(ui, "Save Pay Rate", self.editing_pay_rate_id.is_some());
                save = actions.0;
                if actions.1 {
                    self.cancel_pay_rate_edit();
                }
            }
            if save {
                let rate = PersonalAssistantPayRate {
                    id: self.editing_pay_rate_id.unwrap_or(0),
                    personal_assistant_id: assistant_id,
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
                            .get_all_for_personal_assistant(assistant_id)
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
    }

    fn cancel_pay_rate_edit(&mut self) {
        self.editing_pay_rate_id = None;
        self.new_rate_effective_date.clear();
        self.new_rate_base.clear();
        self.new_rate_top_up.clear();
    }

    fn cancel_hours_edit(&mut self) {
        self.editing_contracted_hours_id = None;
        self.new_hours_effective_date.clear();
        self.new_contracted_hours.clear();
        self.new_hours_basis = HoursBasis::Contracted;
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

fn details_heading(assistant: Option<&PersonalAssistant>) -> String {
    let name = assistant
        .map(|assistant| {
            [assistant.first_name.trim(), assistant.surname.trim()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    if name.is_empty() {
        "Details".into()
    } else {
        format!("Details of {name}")
    }
}

// Allocate complete label/field pairs before placing them, allowing a narrow
// window to wrap the pair instead of clipping the field at the right edge.
fn personal_detail_field(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    field: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    let label_width = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                label.into(),
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            )
            .size()
            .x
    });
    let pair_width = label_width + ui.spacing().item_spacing.x + width + 8.0;
    let available = ui.max_rect().width();
    let layout = if pair_width <= available {
        egui::Layout::left_to_right(egui::Align::Center)
    } else {
        egui::Layout::top_down(egui::Align::Min)
    };
    ui.allocate_ui_with_layout(
        egui::vec2(pair_width.min(available), ui.spacing().interact_size.y),
        layout,
        |ui| {
            let label_rect = ui.label(label).rect;
            field(ui);
            label_rect
        },
    )
    .inner
}

fn compact_personal_details(
    ui: &mut egui::Ui,
    assistant: &mut PersonalAssistant,
    format: crate::date_utils::DateDisplayFormat,
) -> egui::Rect {
    let mut phone_label = egui::Rect::NOTHING;
    ui.scope(|ui| {
        // Compact spacing is local to this row; application accessibility styles remain intact.
        ui.spacing_mut().item_spacing.x = 4.0;
        // Use the entire details width above the two-area employment/address band.
        // Wrap only when the measured labels and explicitly sized inputs cannot fit.
        let fields = [
            ("DoB", 105.0),
            ("NI No.", 100.0),
            ("Email", 200.0),
            ("Phone", 120.0),
        ];
        let font = egui::TextStyle::Body.resolve(ui.style());
        let required_width: f32 = fields
            .iter()
            .map(|(label, width)| {
                ui.fonts_mut(|fonts| {
                    fonts
                        .layout_no_wrap((*label).into(), font.clone(), ui.visuals().text_color())
                        .size()
                        .x
                }) + width
                    + 8.0
                    + ui.spacing().item_spacing.x
            })
            .sum::<f32>()
            + 3.0 * ui.spacing().item_spacing.x;
        let wrap = ui.available_width() < required_width;
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(wrap),
            |ui| {
                personal_detail_field(ui, "DoB", 105.0, |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(105.0, ui.spacing().interact_size.y),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            crate::date_utils::edit(
                                ui,
                                assistant.date_of_birth.get_or_insert_with(String::new),
                                format,
                            );
                        },
                    );
                });
                personal_detail_field(ui, "NI No.", 100.0, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(
                            assistant
                                .national_insurance_number
                                .get_or_insert_with(String::new),
                        )
                        .desired_width(100.0),
                    );
                });
                personal_detail_field(ui, "Email", 200.0, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(assistant.email.get_or_insert_with(String::new))
                            .desired_width(200.0),
                    );
                });
                phone_label = personal_detail_field(ui, "Phone", 120.0, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(
                            assistant.telephone.get_or_insert_with(String::new),
                        )
                        .desired_width(120.0),
                    );
                });
            },
        );
    });
    phone_label
}

fn weekly_hours_edit(ui: &mut egui::Ui, basis: HoursBasis, value: &mut String) -> egui::Response {
    // Guidance is a placeholder in a separate empty buffer, never persisted or
    // substituted for the user's contracted-hours draft when switching basis.
    let mut guidance = String::new();
    let contracted = basis == HoursBasis::Contracted;
    ui.add_enabled(
        contracted,
        egui::TextEdit::singleline(if contracted { value } else { &mut guidance })
            .desired_width(110.0)
            .hint_text(if contracted { "" } else { "Not applicable" }),
    )
    .on_hover_text("Weekly hours are required for Contracted; not applicable for Variable.")
}

fn history_actions(ui: &mut egui::Ui, save_label: &str, editing: bool) -> (bool, bool) {
    ui.horizontal_wrapped(|ui| {
        let save = ui.button(save_label).clicked();
        let cancel = editing && ui.button("Cancel").clicked();
        (save, cancel)
    })
    .inner
}

// Employment follows the full-width personal/contact and address bands.
fn employment_controls(
    ui: &mut egui::Ui,
    assistant: &mut PersonalAssistant,
    format: crate::date_utils::DateDisplayFormat,
) {
    let mut active = assistant.employment_status.as_deref() == Some("Active");

    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut active, "Active").changed() {
            assistant.employment_status = if active {
                Some("Active".to_string())
            } else {
                Some("Inactive".to_string())
            };
        }

        ui.checkbox(&mut assistant.mileage_enabled, "Enable mileage");
    });
    ui.horizontal_wrapped(|ui| {
        ui.horizontal(|ui| {
            ui.label("Start date");
            if assistant.start_date.is_none() {
                assistant.start_date = Some(String::new());
            }
            if let Some(start_date) = &mut assistant.start_date {
                compact_date_edit(ui, start_date, format);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Leaving date");
            compact_date_edit(
                ui,
                assistant.leaving_date.get_or_insert_with(String::new),
                format,
            );
        });
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MaintenanceSection {
    History,
    Form,
    Save,
}

// Shared rows align the forms and buttons even when history lengths or validation
// messages differ. Stacked panels retain their own history/form/save order.
fn maintenance_panels(
    ui: &mut egui::Ui,
    mut show: impl FnMut(&mut egui::Ui, usize, MaintenanceSection),
) {
    let sections = [
        MaintenanceSection::History,
        MaintenanceSection::Form,
        MaintenanceSection::Save,
    ];
    if ui.available_width() >= 800.0 {
        for section in sections {
            ui.columns(2, |columns| {
                for (index, column) in columns.iter_mut().enumerate() {
                    column.push_id(("maintenance_panel", index, section), |ui| {
                        show(ui, index, section)
                    });
                }
            });
        }
    } else {
        for index in 0..2 {
            if index > 0 {
                ui.separator();
            }
            for section in sections {
                ui.push_id(("maintenance_panel", index, section), |ui| {
                    show(ui, index, section)
                });
            }
        }
    }
}

// Labels above compact fields, wrapping whole label/field pairs on small windows.
fn compact_form(ui: &mut egui::Ui, labels: [&str; 3], mut field: impl FnMut(&mut egui::Ui, usize)) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let widths = labels.map(|label| {
        ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(label.to_string(), font.clone(), ui.visuals().text_color())
                .size()
                .x
        })
    });
    ui.horizontal_wrapped(|ui| {
        for (index, label) in labels.into_iter().enumerate() {
            // Give egui the complete pair's width before placement so it can wrap
            // before laying out the label or shrinking the numeric field.
            let width = widths[index].max(
                [
                    120.0,
                    100.0,
                    if labels[2] == "Weekly hours" {
                        110.0
                    } else {
                        80.0
                    },
                ][index]
                    + 8.0,
            );
            ui.allocate_ui_with_layout(
                egui::vec2(
                    width,
                    ui.spacing().interact_size.y * 2.0 + ui.spacing().item_spacing.y,
                ),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.label(label);
                    field(ui, index);
                },
            );
        }
    });
}

// Constrain only layout; the shared editor owns display, input and placeholder behaviour.
fn compact_date_edit(
    ui: &mut egui::Ui,
    value: &mut String,
    format: crate::date_utils::DateDisplayFormat,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(120.0, ui.spacing().interact_size.y),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            crate::date_utils::edit(ui, value, format);
        },
    );
}

fn edit_optional_multiline(ui: &mut egui::Ui, value: &mut Option<String>) {
    if value.is_none() {
        *value = Some(String::new());
    }

    if let Some(text) = value {
        ui.add(egui::TextEdit::multiline(text).desired_rows(3));
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
    fn details_heading_uses_current_names_without_mutating_them() {
        let mut assistant: PersonalAssistant = PersonalAssistant {
            id: 0,
            first_name: "  Mary Jane ".into(),
            surname: " Smith ".into(),
            date_of_birth: None,
            national_insurance_number: None,
            address: None,
            postcode: None,
            telephone: None,
            email: None,
            employment_status: None,
            mileage_enabled: false,
            start_date: None,
            leaving_date: None,
            signature: None,
        };
        assert_eq!(details_heading(None), "Details");
        assert_eq!(
            details_heading(Some(&assistant)),
            "Details of Mary Jane Smith"
        );
        assert_eq!(assistant.first_name, "  Mary Jane ");
        assistant.first_name.clear();
        assert_eq!(details_heading(Some(&assistant)), "Details of Smith");
        assistant.surname = "  ".into();
        assert_eq!(details_heading(Some(&assistant)), "Details");
    }

    #[test]
    fn cancelling_history_edits_discards_drafts_without_changing_history() {
        let mut screen = PersonalAssistantScreen::new();
        screen.pay_rates.push(PersonalAssistantPayRate {
            id: 7,
            personal_assistant_id: 1,
            effective_date: "2026-04-01".into(),
            base_hourly_rate: 12.71,
            employer_top_up_rate: 0.0,
            created_at: "2026-04-01".into(),
        });
        screen.editing_pay_rate_id = Some(7);
        screen.new_rate_effective_date = "invalid draft".into();
        screen.new_rate_base = "99".into();
        screen.new_rate_top_up = "5".into();
        screen.cancel_pay_rate_edit();
        assert_eq!(screen.editing_pay_rate_id, None);
        assert!(screen.new_rate_effective_date.is_empty());
        assert!(screen.new_rate_base.is_empty());
        assert!(screen.new_rate_top_up.is_empty());
        assert_eq!(screen.pay_rates.len(), 1);
        assert_eq!(screen.pay_rates[0].id, 7);
        assert_eq!(screen.pay_rates[0].base_hourly_rate, 12.71);
        assert_eq!(screen.pay_rates[0].employer_top_up_rate, 0.0);
        assert_eq!(screen.pay_rates[0].effective_date, "2026-04-01");
        for basis in [HoursBasis::Contracted, HoursBasis::Variable] {
            screen.contracted_hours =
                vec![crate::contracted_hours_repository::ContractedHoursEntry {
                    id: 8,
                    personal_assistant_id: 1,
                    effective_date: "2026-04-01".into(),
                    contracted_hours: "20".into(),
                    hours_basis: basis,
                    created_at: "2026-04-01".into(),
                }];
            screen.editing_contracted_hours_id = Some(8);
            screen.new_hours_effective_date = "invalid draft".into();
            screen.new_contracted_hours = "99".into();
            screen.new_hours_basis = basis;
            screen.cancel_hours_edit();
            assert_eq!(screen.editing_contracted_hours_id, None);
            assert!(screen.new_hours_effective_date.is_empty());
            assert!(screen.new_contracted_hours.is_empty());
            assert_eq!(screen.new_hours_basis, HoursBasis::Contracted);
            assert_eq!(screen.contracted_hours.len(), 1);
            assert_eq!(screen.contracted_hours[0].id, 8);
            assert_eq!(screen.contracted_hours[0].hours_basis, basis);
            assert_eq!(screen.contracted_hours[0].contracted_hours, "20");
            assert_eq!(screen.contracted_hours[0].effective_date, "2026-04-01");
        }
    }

    #[test]
    fn variable_hours_guidance_is_disabled_and_does_not_replace_the_draft() {
        let ctx = egui::Context::default();
        let mut value = "24.5".to_string();
        let mut sizes = Vec::new();
        for basis in [
            HoursBasis::Contracted,
            HoursBasis::Variable,
            HoursBasis::Contracted,
        ] {
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = weekly_hours_edit(ui, basis, &mut value);
                    assert_eq!(response.enabled(), basis == HoursBasis::Contracted);
                    sizes.push(response.rect.size());
                });
            });
            assert_eq!(value, "24.5");
        }
        assert!(sizes.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn maintenance_panels_fit_desktop_and_stack_on_narrow_windows() {
        let (_directory, application) = crate::payroll_timesheet_screen::tests::test_application();
        let mut screen = PersonalAssistantScreen::new();
        // Unequal histories and Variable's extra form guidance must not offset saves.
        screen.pay_rates = (1..=3)
            .map(|id| PersonalAssistantPayRate {
                id,
                personal_assistant_id: 1,
                effective_date: "2026-04-01".into(),
                base_hourly_rate: 12.71,
                employer_top_up_rate: 0.0,
                created_at: "2026-04-01".into(),
            })
            .collect();
        screen.new_hours_basis = HoursBasis::Variable;
        screen.editing_pay_rate_id = Some(1);
        screen.editing_contracted_hours_id = Some(2);
        for width in [300.0, 420.0, 820.0, 1000.0] {
            let ctx = egui::Context::default();
            let mut bounds = [egui::Rect::NOTHING; 2];
            let mut saves = [egui::Rect::NOTHING; 2];
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let available = ui.max_rect();
                        maintenance_panels(ui, |ui, panel, section| {
                            let rect = ui
                                .scope(|ui| match panel {
                                    0 => screen.show_pay_rates(ui, &application, 1, section),
                                    _ => screen.show_contracted_hours(ui, &application, 1, section),
                                })
                                .response
                                .rect;
                            bounds[panel] = bounds[panel].union(rect);
                            if section == MaintenanceSection::Save {
                                saves[panel] = rect;
                            }
                        });
                        for rect in bounds {
                            assert!(rect.right() <= available.right() + 1.0);
                        }
                    });
                },
            );
            if width > 800.0 {
                assert!((saves[0].top() - saves[1].top()).abs() < 1.0);
                assert!(bounds[0].right() <= bounds[1].left());
                assert!((bounds[0].top() - bounds[1].top()).abs() < 1.0);
            } else {
                assert!(bounds[0].bottom() <= bounds[1].top());
            }
        }
    }

    #[test]
    fn compact_forms_keep_fields_under_labels_and_wrap_on_narrow_widths() {
        for width in [240.0, 400.0] {
            let ctx = egui::Context::default();
            let mut fields = [egui::Rect::NOTHING; 3];
            let mut values = [String::new(), String::new(), String::new()];
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 600.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let available = ui.max_rect();
                        compact_form(
                            ui,
                            ["Effective date", "Base hourly rate", "Employer top-up rate"],
                            |ui, index| {
                                fields[index] = ui
                                    .add(
                                        egui::TextEdit::singleline(&mut values[index])
                                            .desired_width(if index == 0 { 120.0 } else { 80.0 })
                                            .hint_text(if index == 1 { "12.71" } else { "0.00" }),
                                    )
                                    .rect;
                            },
                        );
                        for rect in fields {
                            assert!(rect.right() <= available.right() + 1.0);
                        }
                    });
                },
            );
            assert!(
                values.iter().all(String::is_empty),
                "Placeholders must not populate values"
            );
            if width == 400.0 {
                assert!(fields
                    .windows(2)
                    .all(|pair| (pair[0].top() - pair[1].top()).abs() < 1.0
                        && pair[0].right() < pair[1].left()));
            } else {
                assert!(fields[2].top() > fields[0].top());
            }
        }
    }

    #[test]
    fn compact_dates_reuse_shared_editor_without_changing_stored_values() {
        for format in crate::date_utils::DateDisplayFormat::ALL {
            for source in ["2012-07-19", "", "invalid legacy date"] {
                let ctx = egui::Context::default();
                let mut value = source.to_string();
                let _ = ctx.run(Default::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let rect = ui
                            .scope(|ui| compact_date_edit(ui, &mut value, format))
                            .response
                            .rect;
                        assert!(rect.width() <= 121.0);
                    });
                });
                assert_eq!(value, source);
            }
        }
    }

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
