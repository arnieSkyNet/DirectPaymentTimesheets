use crate::annual_leave_settings_repository::AnnualLeaveSettings as StoredAnnualLeaveSettings;
use eframe::egui;

use crate::app::Application;
use crate::pay_rate_repository::PersonalAssistantPayRate;
use crate::payroll_provider_repository::PayrollProvider;
use chrono::Local;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PayrollTab {
    Provider,
    PayRates,
    Holidays,
    CsvImport,
    Footer,
}

impl PayrollTab {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Provider, "Payroll Provider"),
        (Self::PayRates, "Pay Rates"),
        (Self::Holidays, "Holidays & Leave"),
        (Self::CsvImport, "CSV Import"),
        (Self::Footer, "Timesheet Footer"),
    ];
}

pub struct PayrollSettingsScreen {
    loaded: bool,
    active_tab: PayrollTab,
    saved_form: String,
    annual_leave: AnnualLeaveSettings,

    public_holiday_region: crate::public_holidays::PublicHolidayRegion,
    holiday_refresh: crate::public_holidays::Refresh,
    frequency: String,
    rounding_minutes: i64,
    rounding_direction: String,
    csv_options: crate::csv_import::CsvImportConfig,
    csv_headings: [String; 8],
    start_of_workweek: String,

    provider_name: String,
    provider_email: String,
    provider_address: String,
    provider_telephone: String,

    standard_rate_effective_date: String,
    standard_rate_base: String,
    standard_rate_top_up: String,
    overtime_enabled: bool,
    timesheet_footer_text: String,
    timesheet_footer_font_size: f64,

    status_message: String,
    confirm_bulk_pay_rate_update: bool,
}

impl PayrollSettingsScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,
            active_tab: PayrollTab::Provider,
            saved_form: String::new(),
            annual_leave: AnnualLeaveSettings::default(),

            public_holiday_region: Default::default(),
            holiday_refresh: Default::default(),
            frequency: "Every Four Weeks".to_string(),
            rounding_minutes: 15,
            rounding_direction: "Up".to_string(),
            csv_options: Default::default(),
            csv_headings: std::array::from_fn(|i| {
                crate::csv_import::CsvImportConfig::default().headings[i].join(" | ")
            }),
            start_of_workweek: "Monday".to_string(),

            provider_name: String::new(),
            provider_email: String::new(),
            provider_address: String::new(),
            provider_telephone: String::new(),

            standard_rate_effective_date: String::new(),
            standard_rate_base: String::new(),
            standard_rate_top_up: String::new(),
            overtime_enabled: false,
            timesheet_footer_text: crate::config::default_timesheet_footer_text(),
            timesheet_footer_font_size: 7.0,

            status_message: "Payroll settings not loaded.".to_string(),
            confirm_bulk_pay_rate_update: false,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &mut Application) -> bool {
        let mut open_email_settings = false;
        if !self.loaded {
            self.load(application);
            self.loaded = true;
        }

        ui.heading("Payroll Settings");

        ui.separator();

        ui.horizontal_wrapped(|ui| {
            for (tab, label) in PayrollTab::ALL {
                ui.selectable_value(&mut self.active_tab, tab, label);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if crate::gui_controls::save_button(
                ui,
                "Save Payroll Settings",
                self.form_state() != self.saved_form,
            )
            .clicked()
            {
                self.save(application);
            }

            if ui.button("Email Settings").clicked() {
                open_email_settings = true;
            }
        });

        ui.separator();

        ui.label(&self.status_message);
        egui::ScrollArea::vertical()
            .id_salt(("payroll_settings_tab", self.active_tab as u8))
            .show(ui, |ui| match self.active_tab {
                PayrollTab::Provider => self.show_provider(ui, application),
                PayrollTab::PayRates => self.show_payrates(ui, application),
                PayrollTab::Holidays => self.show_holidays(ui, application),
                PayrollTab::CsvImport => self.show_csvimport(ui, application),
                PayrollTab::Footer => self.show_footer(ui, application),
            });
        open_email_settings
    }

    fn show_provider(&mut self, ui: &mut egui::Ui, _application: &mut Application) {
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
        ui.heading("Payroll Rules");

        let rules_layout = if ui.available_width() < 760.0 {
            egui::Layout::top_down(egui::Align::Min)
        } else {
            egui::Layout::left_to_right(egui::Align::Min)
        };
        ui.with_layout(rules_layout, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(240.0);
                ui.label("Payroll Frequency");

                ui.label(format!(
                    "Stored preference: {} (not applied)",
                    self.frequency
                ));
                ui.label("Payroll periods come from the imported four-week schedule.");
            });

            ui.add_space(30.0);

            ui.vertical(|ui| {
                ui.label("Rounding Minutes");

                crate::gui_controls::combo_box("rounding_minutes")
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
                                ui.close();
                                self.rounding_minutes = minutes;
                                self.rounding_direction = direction.to_string();
                            }
                        }
                    });
            });

            ui.add_space(30.0);

            ui.vertical(|ui| {
                ui.set_max_width(240.0);
                ui.label("Start of Workweek");

                ui.label(format!(
                    "Stored preference: {} (not applied)",
                    self.start_of_workweek
                ));
                ui.label("Week commencing dates come from the imported payroll schedule.");
            });
        });

        ui.separator();

        ui.label(self.unsupported_overtime_notice());
        ui.label("These unsupported preferences are retained for compatibility. Saving settings does not enable their calculations.");
    }

    fn show_payrates(&mut self, ui: &mut egui::Ui, application: &mut Application) {
        ui.heading("National / Standard Pay Rate Update");

        ui.columns(3, |columns| {
            columns[0].label("Effective date:");
            crate::date_utils::edit(
                &mut columns[0],
                &mut self.standard_rate_effective_date,
                application.context.config.date_display_format,
            );

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
    }

    fn show_holidays(&mut self, ui: &mut egui::Ui, application: &mut Application) {
        ui.heading("Public holidays");
        ui.label("Public-holiday region");
        crate::gui_controls::combo_box("public_holiday_region")
            .selected_text(self.public_holiday_region.label())
            .show_ui(ui, |ui| {
                for region in crate::public_holidays::PublicHolidayRegion::ALL {
                    ui.selectable_value(&mut self.public_holiday_region, region, region.label());
                }
            });
        ui.label("Public-holiday region — save settings to apply. Identification never automatically subtracts worked hours or establishes contractual entitlement.");
        let today = chrono::Local::now().date_naive();
        let calendar = crate::public_holidays::period(
            &application.context.environment.cache_dir,
            self.public_holiday_region,
            today,
            today,
            chrono::Utc::now().timestamp(),
        );
        ui.label(&calendar.status);
        for warning in &calendar.warnings {
            ui.label(warning);
        }
        if ui
            .add_enabled(
                !self.holiday_refresh.running(),
                egui::Button::new("Refresh holiday data"),
            )
            .clicked()
        {
            self.holiday_refresh.tick(
                &application.context.environment.cache_dir,
                &application.context.environment.database_path,
                chrono::Utc::now().timestamp(),
                ui.ctx(),
                true,
            );
        }
        if let Some(status) = &self.holiday_refresh.status {
            ui.label(status);
        }
        ui.separator();
        self.annual_leave.show(ui);
    }

    fn show_csvimport(&mut self, ui: &mut egui::Ui, _application: &mut Application) {
        ui.heading("CSV Import");
        ui.label("Applies to new CSV evidence. Existing imports and archives retain their interpretation.");
        ui.horizontal(|ui| {
            ui.label("CSV rows to skip");
            ui.add(egui::DragValue::new(&mut self.csv_options.introductory_rows).range(0..=10000));
        });
        ui.label("Hours calculation method");
        crate::gui_controls::combo_box("csv_hours_method")
            .selected_text(self.csv_options.hours_method.label())
            .show_ui(ui, |ui| {
                for method in crate::csv_import::HoursMethod::ALL {
                    ui.selectable_value(&mut self.csv_options.hours_method, method, method.label());
                }
            });
        self.csv_options.require_method_fields();
        ui.label("Payroll rounding is applied once per shift in preparation. Disabled and unknown columns are ignored. Omitted Rate/Amount display as Missing.");
        if self.csv_options.hours_method.deducts_break() {
            ui.label("Break is required on every shift, including an explicit zero for no break. It cannot exceed the shift duration or the selected hours.");
        } else {
            ui.label("Optional Break may be omitted or blank; it does not reduce imported hours in this method.");
        }
        ui.label("Heading aliases: separate alternatives with |. Required columns are enabled and locked; disabling a column keeps its aliases.");
        ui.separator();
        ui.heading("CSV heading mappings");
        csv_heading_editors(ui, &mut self.csv_headings, &mut self.csv_options);
        ui.separator();
        csv_column_preview(ui, &self.csv_headings, &self.csv_options);
    }

    fn show_footer(&mut self, ui: &mut egui::Ui, _application: &mut Application) {
        ui.heading("Payroll-timesheet footer / instructions");
        ui.label("Organisation-specific instructions printed below the signature dates. Blank lines are preserved; leave empty for no footer.");
        ui.add(
            egui::TextEdit::multiline(&mut self.timesheet_footer_text)
                .desired_rows(5)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.label("Footer font size");
            crate::gui_controls::combo_box("timesheet_footer_font_size")
                .selected_text(format!("{} pt", self.timesheet_footer_font_size))
                .show_ui(ui, |ui| {
                    for size in crate::config::FOOTER_FONT_SIZES {
                        crate::gui_controls::combo_value(
                            ui,
                            &mut self.timesheet_footer_font_size,
                            f64::from(size),
                            format!("{size} pt"),
                        );
                    }
                });
        });
        ui.label("Text must fit at the selected size. If generation reports overflow, shorten the text or choose a smaller font size.");

        ui.separator();
    }

    pub fn refresh_holidays(&mut self, application: &Application, ctx: &egui::Context) {
        self.holiday_refresh.tick(
            &application.context.environment.cache_dir,
            &application.context.environment.database_path,
            chrono::Utc::now().timestamp(),
            ctx,
            false,
        );
    }

    fn form_state(&self) -> String { vec![format!("{:?}", self.csv_options),format!("{:?}", self.csv_headings),format!("{:?}", self.public_holiday_region),format!("{:?}", self.frequency),format!("{:?}", self.rounding_minutes),format!("{:?}", self.rounding_direction),format!("{:?}", self.start_of_workweek),format!("{:?}", self.provider_name),format!("{:?}", self.provider_email),format!("{:?}", self.provider_address),format!("{:?}", self.provider_telephone),format!("{:?}", self.overtime_enabled),format!("{:?}", self.timesheet_footer_text),format!("{:?}", self.timesheet_footer_font_size),format!("{:?}", self.annual_leave.contracted_from),format!("{:?}", self.annual_leave.weeks),format!("{:?}", self.annual_leave.variable_from),format!("{:?}", self.annual_leave.percentage)].join("\0") }
    fn load(&mut self, application: &Application) {
        self.csv_options = application.context.config.csv_import.clone();
        self.csv_headings = std::array::from_fn(|i| self.csv_options.headings[i].join(" | "));
        let payroll = &application.context.config.payroll;

        self.annual_leave.load(application);
        self.public_holiday_region = payroll.public_holiday_region;
        self.frequency = payroll.frequency.clone();
        self.rounding_minutes = payroll.rounding_minutes;
        self.rounding_direction = payroll.rounding_direction.clone();
        self.start_of_workweek = payroll.start_of_workweek.clone();

        self.overtime_enabled = payroll.overtime_enabled;
        self.timesheet_footer_text = payroll.timesheet_footer_text.clone();
        self.timesheet_footer_font_size =
            crate::config::normalise_footer_font_size(payroll.timesheet_footer_font_size);

        if let Ok(Some(provider)) = application.payroll_provider_repository.get() {
            self.provider_name = provider.name.unwrap_or_default();
            self.provider_email = provider.email.unwrap_or_default();
            self.provider_address = provider.address.unwrap_or_default();
            self.provider_telephone = provider.telephone.unwrap_or_default();
        }

        self.saved_form = self.form_state();
        self.status_message = "Payroll settings loaded.".to_string();
    }

    fn unsupported_overtime_notice(&self) -> String {
        format!(
            "Overtime calculation is not implemented. Stored preference: {} (not applied).",
            if self.overtime_enabled {
                "enabled"
            } else {
                "disabled"
            }
        )
    }

    fn save(&mut self, application: &mut Application) {
        self.save_settings(application);
        self.annual_leave.status = self.status_message.clone();
    }

    fn save_settings(&mut self, application: &mut Application) {
        let db = match crate::payroll_evidence::open(application) {
            Ok(db) => db,
            Err(error) => {
                self.status_message = error.to_string();
                return;
            }
        };
        let _guard = match crate::timesheet_delivery::production_lock(&db) {
            Ok(guard) => guard,
            Err(error) => {
                self.status_message = error.to_string();
                return;
            }
        };
        // Validate all annual-leave inputs before any existing payroll writes.
        let annual_leave = match self.annual_leave.draft() {
            Ok(settings) => settings,
            Err(error) => {
                self.status_message = format!(
                    "Payroll Settings save failed: {}",
                    annual_leave_error(error)
                );
                return;
            }
        };
        let mut csv_options = self.csv_options.clone();
        csv_options.headings = std::array::from_fn(|i| self.csv_headings[i].split('|').map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect());
        if let Err(error) = csv_options.validate() {
            self.status_message = format!("Payroll Settings save failed: {error}");
            return;
        }
        let mut candidate = application.context.config.clone();
        candidate.csv_import = csv_options;
        let payroll = &mut candidate.payroll;

        payroll.public_holiday_region = self.public_holiday_region;
        payroll.frequency = self.frequency.clone();

        payroll.rounding_minutes = self.rounding_minutes;
        payroll.rounding_direction = self.rounding_direction.clone();

        payroll.start_of_workweek = self.start_of_workweek.clone();

        payroll.overtime_enabled = self.overtime_enabled;
        payroll.timesheet_footer_text = self.timesheet_footer_text.clone();
        payroll.timesheet_footer_font_size =
            crate::config::normalise_footer_font_size(self.timesheet_footer_font_size);

        let payroll_department_email = match application.payroll_provider_repository.get() {
            Ok(provider) => provider.and_then(|provider| provider.payroll_department_email),
            Err(error) => {
                self.status_message = format!("Failed loading payroll provider: {}", error);
                return;
            }
        };

        let provider = PayrollProvider {
            id: 1,
            name: optional_value(&self.provider_name),
            email: optional_value(&self.provider_email),
            address: optional_value(&self.provider_address),
            telephone: optional_value(&self.provider_telephone),
            payroll_department_email,
        };

        if let Err(error) = application.payroll_provider_repository.save(&provider) {
            self.status_message = format!("Failed saving payroll provider: {}", error);
            return;
        }

        // Both database writes can fail. Complete them before publishing the
        // candidate so no failed database save activates new payroll rules.
        let settings = match application.annual_leave_settings_repository.save(&annual_leave) {
            Ok(settings) => settings,
            Err(error) => {
                self.status_message = format!("Payroll settings save failed: {}. Rounding and holiday region are unchanged; provider details may have been saved. Retry to finish.", annual_leave_error(error));
                return;
            }
        };
        if let Err(error) = application.save_config_candidate(candidate) {
            self.status_message = format!(
                "Payroll settings save failed: {error}. Rounding and holiday region are unchanged; provider and annual-leave details were saved. Your edits are retained; retry to finish."
            );
            return;
        }
        self.annual_leave.fill(&settings);
        self.csv_options = application.context.config.csv_import.clone();
        self.saved_form = self.form_state();
        self.status_message = "Payroll settings saved.".into();
    }
}

/// Vertical mappings retain aliases while optional fields are disabled. Use a
/// moderate editor width on desktop and stack the editor below its label when narrow.
fn csv_heading_editors(
    ui: &mut egui::Ui,
    headings: &mut [String; 8],
    config: &mut crate::csv_import::CsvImportConfig,
) -> [egui::Response; 8] {
    config.require_method_fields();
    let side_by_side = ui.available_width() >= 560.0;
    std::array::from_fn(|i| {
        let required = config.hours_method.requires(i);
        let label = format!(
            "{} ({})",
            crate::csv_import::FIELD_NAMES[i],
            if required { "required" } else { "optional" }
        );
        let field_label = |ui: &mut egui::Ui, enabled: &mut bool| {
            ui.add_enabled_ui(!required, |ui| {
                if side_by_side {
                    ui.set_min_width(235.0);
                }
                ui.add(egui::Checkbox::new(enabled, label.clone()))
            })
            .inner
            .on_hover_text(if required {
                "Required by the selected hours method"
            } else {
                "Enable or ignore this CSV column; aliases are preserved"
            });
        };
        let edit = |ui: &mut egui::Ui, text: &mut String, enabled: bool| {
            let width = ui.available_width().min(480.0);
            ui.add_enabled_ui(enabled, |ui| {
                ui.add_sized(
                    [width, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(text)
                        .desired_width(width)
                        .id(egui::Id::new(("csv_heading_mapping", i))),
                )
            })
            .inner
        };
        if side_by_side {
            ui.horizontal(|ui| {
                field_label(ui, &mut config.enabled[i]);
                edit(ui, &mut headings[i], config.enabled[i])
            })
            .inner
        } else {
            field_label(ui, &mut config.enabled[i]);
            edit(ui, &mut headings[i], config.enabled[i])
        }
    })
}

fn csv_preview_columns(
    headings: &[String; 8],
    config: &crate::csv_import::CsvImportConfig,
) -> Vec<(String, &'static str)> {
    let examples = [
        "Example PA",
        "02/04/2026 at 09:00",
        "02/04/2026 at 10:00",
        "0h 10m",
        "1h 00m",
        "12.50",
        "12.50",
        "Example note",
    ];
    (0..8)
        .filter(|&i| config.enabled[i])
        .map(|i| {
            let heading = headings[i]
                .split('|')
                .map(str::trim)
                .find(|v| !v.is_empty())
                .unwrap_or(crate::csv_import::FIELD_NAMES[i])
                .to_string();
            (heading, examples[i])
        })
        .collect()
}
/// Content widths reserve full example values and wrap headings only as needed.
/// Share spare width up to the natural heading widths, with a small amount of
/// breathing room, rather than stretching a few enabled fields across the screen.
fn csv_preview_widths(ui: &egui::Ui, columns: &[(String, &str)], available: f32) -> Vec<f32> {
    let measure = |text: &str, strong: bool| {
        let text = if strong {
            egui::RichText::new(text).strong()
        } else {
            egui::RichText::new(text)
        };
        egui::WidgetText::from(text)
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Body,
            )
            .size()
            .x
    };
    let minima: Vec<f32> = columns
        .iter()
        .map(|(heading, sample)| {
            let word = heading
                .split_whitespace()
                .map(|w| measure(w, true))
                .fold(0.0_f32, f32::max)
                .min(120.0);
            measure(sample, false).max(word).ceil() + 2.0
        })
        .collect();
    let preferred: Vec<f32> = columns
        .iter()
        .zip(&minima)
        .map(|((heading, _), minimum)| {
            (measure(heading, true).min(180.0).ceil() + 2.0).max(*minimum)
        })
        .collect();
    // Include each cell's inner margin and the current theme's border width.
    let overhead = 2.0 * (6.0 + ui.visuals().widgets.noninteractive.bg_stroke.width);
    let budget = (available - overhead * columns.len() as f32 - 2.0).max(0.0);
    let minimum_total: f32 = minima.iter().sum();
    let preferred_total: f32 = preferred.iter().sum();
    if budget < minimum_total {
        return minima;
    }
    if budget >= preferred_total {
        let extra = if columns.is_empty() {
            0.0
        } else {
            ((budget - preferred_total) / columns.len() as f32).min(12.0)
        };
        return preferred.into_iter().map(|w| (w + extra).floor()).collect();
    }
    let share = (budget - minimum_total) / (preferred_total - minimum_total);
    minima
        .iter()
        .zip(preferred)
        .map(|(minimum, preferred)| (minimum + share * (preferred - minimum)).floor())
        .collect()
}

fn csv_column_preview(
    ui: &mut egui::Ui,
    headings: &[String; 8],
    config: &crate::csv_import::CsvImportConfig,
) -> egui::scroll_area::ScrollAreaOutput<()> {
    ui.label("Enabled columns — live example");
    ui.label("Illustration only: CSV columns may appear in any order. Each cell shows the first heading alias.");
    let columns = csv_preview_columns(headings, config);
    let widths = csv_preview_widths(ui, &columns, ui.available_width());
    let header_height = columns
        .iter()
        .zip(&widths)
        .map(|((heading, _), width)| {
            egui::WidgetText::from(egui::RichText::new(heading).strong())
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Wrap),
                    *width,
                    egui::TextStyle::Body,
                )
                .size()
                .y
                .ceil()
        })
        .fold(28.0_f32, f32::max);
    let sample_height = ui.text_style_height(&egui::TextStyle::Body).ceil() + 2.0;
    egui::ScrollArea::horizontal()
        .id_salt("csv_columns_preview")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for ((heading, sample), width) in columns.iter().zip(widths) {
                    egui::Frame::new()
                        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                        .fill(ui.visuals().faint_bg_color)
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.set_width(width);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(width, header_height),
                                    egui::Layout::top_down(egui::Align::Center),
                                    |ui| {
                                        ui.set_min_size(egui::vec2(width, header_height));
                                        ui.add(egui::Label::new(egui::RichText::new(heading).strong()).wrap());
                                    },
                                );
                                ui.separator();
                                ui.add_sized(
                                    [width, sample_height],
                                    egui::Label::new(*sample).extend(),
                                );
                            });
                        });
                }
            });
        })
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

fn annual_leave_error(error: rusqlite::Error) -> String {
    match error {
        rusqlite::Error::InvalidParameterName(message) => message,
        other => other.to_string(),
    }
}

#[derive(Default)]
struct AnnualLeaveSettings {
    contracted_from: String,
    weeks: String,
    variable_from: String,
    percentage: String,
    status: String,
}
impl AnnualLeaveSettings {
    fn fill(&mut self, settings: &StoredAnnualLeaveSettings) {
        self.contracted_from = settings.contracted_effective_from.clone();
        self.weeks = settings.statutory_weeks.to_string();
        self.variable_from = settings.variable_effective_from.clone();
        self.percentage = settings.accrual_percentage.to_string();
    }
    fn load(&mut self, application: &Application) {
        match application.annual_leave_settings_repository.get() {
            Ok(settings) => self.fill(&settings),
            Err(error) => self.status = format!("Unable to load Annual Leave Settings: {error}"),
        }
    }
    fn draft(&self) -> rusqlite::Result<StoredAnnualLeaveSettings> {
        let parse = |text: &str, label: &str| {
            text.trim().parse::<f64>().map_err(|_| {
                rusqlite::Error::InvalidParameterName(format!("{label} must be a number."))
            })
        };
        StoredAnnualLeaveSettings {
            contracted_effective_from: self.contracted_from.clone(),
            statutory_weeks: parse(&self.weeks, "Statutory weeks")?,
            variable_effective_from: self.variable_from.clone(),
            accrual_percentage: parse(&self.percentage, "Accrual percentage")?,
        }
        .validated()
    }
    fn show(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Annual Leave");
        ui.group(|ui| {
            ui.label("Contracted-hours annual leave");
            ui.horizontal_wrapped(|ui| {
                ui.label("Effective from (DD/MM)");
                ui.add(egui::TextEdit::singleline(&mut self.contracted_from).desired_width(70.0));
                ui.label("Statutory annual-leave weeks");
                ui.add(egui::TextEdit::singleline(&mut self.weeks).desired_width(70.0));
            });
        });
        ui.group(|ui| {
            ui.label("Variable-hours annual leave");
            ui.horizontal_wrapped(|ui| {
                ui.label("Effective from (DD/MM)");
                ui.add(egui::TextEdit::singleline(&mut self.variable_from).desired_width(70.0));
                ui.label("Accrual percentage");
                ui.add(egui::TextEdit::singleline(&mut self.percentage).desired_width(70.0));
            });
        });
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
    }
}
#[cfg(test)]
mod annual_leave_settings_tests {
    use super::*;
    #[test]
    fn shared_save_reports_success_and_failures_without_false_success() {
        for scenario in ["success", "invalid", "config_failure", "annual_failure"] {
            let (directory, mut application) =
                crate::payroll_timesheet_screen::tests::test_application();
            // Keep all config-save filesystem effects inside this test directory.
            let folders = &mut application.context.config.folders;
            folders.csv_import = directory.path().join("csv");
            folders.pdf_output = directory.path().join("pdf");
            folders.email_archive = directory.path().join("email");
            folders.payslip_folder = directory.path().join("payslips");
            folders.payroll_information_folder = directory.path().join("information");
            let mut screen = PayrollSettingsScreen::new();
            screen.load(&application);
            screen.timesheet_footer_text = "First\n\nSecond\n".into();
            screen.timesheet_footer_font_size = 9.0;
            screen.annual_leave.contracted_from = "1/1".into();
            screen.annual_leave.weeks = "6".into();
            screen.annual_leave.variable_from = "1/9".into();
            screen.annual_leave.percentage = "13".into();
            let connection =
                crate::database::open(&application.context.environment.database_path).unwrap();
            match scenario {
                "invalid" => screen.annual_leave.variable_from = "31/02".into(),
                "config_failure" => std::fs::create_dir(directory.path().join("config.toml")).unwrap(),
                "annual_failure" => connection.execute_batch("CREATE TRIGGER refuse_annual_settings BEFORE INSERT ON annual_leave_settings BEGIN SELECT RAISE(ABORT, 'annual failure'); END;").unwrap(),
                _ => {}
            }
            screen.save(&mut application);
            assert_eq!(screen.annual_leave.status, screen.status_message);
            if scenario == "success" {
                assert_eq!(screen.status_message, "Payroll settings saved.");
                let saved = application.annual_leave_settings_repository.get().unwrap();
                assert_eq!(saved.contracted_effective_from, "01/01");
                assert_eq!(saved.variable_effective_from, "01/09");
                assert_eq!(
                    (saved.statutory_weeks, saved.accrual_percentage),
                    (6.0, 13.0)
                );
                assert!(directory.path().join("config.toml").is_file());
                let persisted =
                    crate::config::AppConfig::load(&directory.path().join("config.toml")).unwrap();
                assert_eq!(persisted.payroll.timesheet_footer_text, "First\n\nSecond\n");
                assert_eq!(persisted.payroll.timesheet_footer_font_size, 9.0);
                screen.load(&application);
                assert_eq!(screen.timesheet_footer_text, "First\n\nSecond\n");
                assert_eq!(screen.timesheet_footer_font_size, 9.0);
            } else {
                assert_ne!(screen.status_message, "Payroll settings saved.");
                let stored = application.annual_leave_settings_repository.get().unwrap();
                if scenario == "config_failure" {
                    assert_eq!(stored.statutory_weeks, 6.0);
                    assert!(screen.status_message.contains("provider and annual-leave details were saved"));
                } else {
                    assert_eq!(stored, StoredAnnualLeaveSettings::default());
                }
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM annual_leave_settings", [], |row| row
                            .get::<_, i64>(
                            0
                        ))
                        .unwrap(),
                    if scenario == "config_failure" { 1 } else { 0 }
                );
                if scenario == "invalid" {
                    assert!(!directory.path().join("config.toml").exists());
                }
                if scenario == "annual_failure" {
                    assert!(screen.status_message.contains("Rounding and holiday region are unchanged"));
                    assert!(!directory.path().join("config.toml").exists());
                }
            }
        }
    }

    #[test]
    fn editor_defaults_and_independent_recurring_values() {
        let mut editor = AnnualLeaveSettings::default();
        editor.fill(&StoredAnnualLeaveSettings::default());
        assert_eq!(editor.contracted_from, "01/04");
        assert_eq!(editor.variable_from, "01/04");
        assert_eq!(editor.weeks, "5.6");
        assert_eq!(editor.percentage, "12.07");
        editor.contracted_from = "1/1".into();
        editor.variable_from = "15/9".into();
        let settings = editor.draft().unwrap();
        assert_eq!(settings.contracted_effective_from, "01/01");
        assert_eq!(settings.variable_effective_from, "15/09");
        editor.variable_from = "15/09/2026".into();
        assert!(editor.draft().is_err());
    }
}

#[cfg(test)]
mod pre_release_safeguard_tests {
    use super::*;
    #[test]
    fn unsupported_preferences_are_presented_as_not_applied_without_edit_controls() {
        let (_directory, mut app) = crate::payroll_timesheet_screen::tests::test_application();
        app.context.config.payroll.frequency = "Monthly".into();
        app.context.config.payroll.start_of_workweek = "Sunday".into();
        app.context.config.payroll.overtime_enabled = true;
        let mut screen = PayrollSettingsScreen::new();
        let ctx = egui::Context::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 2200.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    screen.show(ui, &mut app);
                });
            },
        );
        fn collect(shape: &egui::Shape, text: &mut String) {
            match shape {
                egui::Shape::Text(value) => {
                    text.push_str(&value.galley.job.text);
                    text.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, text);
                    }
                }
                _ => {}
            }
        }
        let mut text = String::new();
        for shape in output.shapes {
            collect(&shape.shape, &mut text);
        }
        assert!(text.contains("Monthly (not applied)"));
        assert!(text.contains("Sunday (not applied)"));
        assert!(text.contains("Overtime calculation is not implemented"));
        assert!(!text.contains("Enable overtime calculations"));
        assert!(text.contains("Rounding Minutes"));
        assert_eq!(screen.frequency, "Monthly");
        assert_eq!(screen.start_of_workweek, "Sunday");
        assert!(screen.overtime_enabled);
    }
    #[test]
    fn saving_functioning_settings_preserves_legacy_unsupported_preferences() {
        for overtime in [false, true] {
            let (directory, mut app) = crate::payroll_timesheet_screen::tests::test_application();
            app.context.config.payroll.frequency = "Legacy custom frequency".into();
            app.context.config.payroll.start_of_workweek = "Sunday".into();
            app.context.config.payroll.overtime_enabled = overtime;
            let mut screen = PayrollSettingsScreen::new();
            screen.load(&app);
            screen.rounding_minutes = 5;
            screen.rounding_direction = "Down".into();
            screen.save(&mut app);
            assert_eq!(screen.status_message, "Payroll settings saved.");
            let persisted =
                crate::config::AppConfig::load(&directory.path().join("config.toml")).unwrap();
            assert_eq!(persisted.payroll.frequency, "Legacy custom frequency");
            assert_eq!(persisted.payroll.start_of_workweek, "Sunday");
            assert_eq!(persisted.payroll.overtime_enabled, overtime);
            assert_eq!(persisted.payroll.rounding_minutes, 5);
            assert_eq!(persisted.payroll.rounding_direction, "Down");
        }
    }
}

#[cfg(test)] mod save_dirty_tests {
    use super::*;
    #[test] fn payroll_settings_save_colour_lifecycle() {
        use crate::gui_controls::acceptance::{frame,click};
        let (_dir,mut app)=crate::payroll_timesheet_screen::tests::test_application();let ctx=egui::Context::default();let mut screen=PayrollSettingsScreen::new();
        assert!(!frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Payroll Settings").1);
        screen.timesheet_footer_text="Changed synthetic footer".into();let(pos,blue)=frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Payroll Settings");assert!(blue);
        click(&ctx,pos,|ui|{screen.show(ui,&mut app);},"Save Payroll Settings");
        assert!(!frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Payroll Settings").1);
    }
    #[test] fn save_baseline_resets_only_on_success() {
        let (_dir,mut app)=crate::payroll_timesheet_screen::tests::test_application();
        let mut screen=PayrollSettingsScreen::new();screen.load(&app);
        assert_eq!(screen.form_state(),screen.saved_form);
        let valid=screen.annual_leave.weeks.clone();screen.annual_leave.weeks="invalid".into();
        screen.save(&mut app);assert_ne!(screen.form_state(),screen.saved_form);
        screen.annual_leave.weeks=valid;screen.timesheet_footer_text="Synthetic changed footer".into();
        screen.save(&mut app);assert_eq!(screen.form_state(),screen.saved_form);
    }
}

#[cfg(test)]
mod save_consistency_tests {
    use super::*;
    use crate::public_holidays::PublicHolidayRegion;

    fn assert_effective(
        app: &Application,
        minutes: i64,
        direction: &str,
        region: PublicHolidayRegion,
        payable: i64,
    ) {
        let payroll = &app.context.config.payroll;
        assert_eq!(payroll.rounding_minutes, minutes);
        assert_eq!(payroll.rounding_direction, direction);
        assert_eq!(payroll.public_holiday_region, region);
        assert_eq!(
            crate::pay_rate_allocation::payable_minutes(34, payroll).unwrap(),
            payable
        );
    }

    #[test]
    fn failed_saves_retain_effective_rules_persisted_rules_and_drafts_until_retry() {
        for failure in [
            "invalid",
            "provider_read",
            "provider_write",
            "annual_write",
            "config_write",
            "credential",
        ] {
            let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
            app.save_config().unwrap();
            let path = dir.path().join("config.toml");
            let before = std::fs::read(&path).unwrap();
            let db = crate::payroll_evidence::open(&app).unwrap();
            let mut screen = PayrollSettingsScreen::new();
            screen.load(&app);
            let baseline = screen.saved_form.clone();
            screen.rounding_minutes = 5;
            screen.rounding_direction = "Down".into();
            screen.public_holiday_region = PublicHolidayRegion::Scotland;
            screen.provider_name = "Synthetic retry provider".into();
            let valid_weeks = screen.annual_leave.weeks.clone();
            let data_dir = app.context.environment.data_dir.clone();
            match failure {
                "invalid" => screen.annual_leave.weeks = "invalid".into(),
                "provider_read" => db.execute_batch("ALTER TABLE payroll_provider RENAME TO fixture_provider").unwrap(),
                "provider_write" => db.execute_batch("CREATE TRIGGER refuse_provider BEFORE INSERT ON payroll_provider BEGIN SELECT RAISE(ABORT,'synthetic provider refusal'); END;").unwrap(),
                "annual_write" => db.execute_batch("CREATE TRIGGER refuse_annual BEFORE INSERT ON annual_leave_settings BEGIN SELECT RAISE(ABORT,'synthetic annual refusal'); END;").unwrap(),
                "config_write" => app.context.environment.data_dir = dir.path().join("missing-parent"),
                "credential" => app.context.config.email.smtp_credential_error = Some("Synthetic unavailable credential".into()),
                _ => unreachable!(),
            }
            screen.save(&mut app);
            assert_ne!(
                screen.status_message, "Payroll settings saved.",
                "{failure}"
            );
            assert_effective(&app, 15, "Up", PublicHolidayRegion::EnglandAndWales, 45);
            assert_eq!(std::fs::read(&path).unwrap(), before, "{failure}");
            assert_eq!(screen.rounding_minutes, 5);
            assert_eq!(screen.rounding_direction, "Down");
            assert_eq!(screen.public_holiday_region, PublicHolidayRegion::Scotland);
            assert_eq!(screen.provider_name, "Synthetic retry provider");
            assert_eq!(screen.saved_form, baseline);
            assert_ne!(screen.form_state(), screen.saved_form);
            match failure {
                "invalid" => screen.annual_leave.weeks = valid_weeks,
                "provider_read" => db
                    .execute_batch("ALTER TABLE fixture_provider RENAME TO payroll_provider")
                    .unwrap(),
                "provider_write" => db.execute_batch("DROP TRIGGER refuse_provider").unwrap(),
                "annual_write" => db.execute_batch("DROP TRIGGER refuse_annual").unwrap(),
                "config_write" => app.context.environment.data_dir = data_dir,
                "credential" => app.context.config.email.smtp_credential_error = None,
                _ => unreachable!(),
            }
            screen.save(&mut app);
            assert_eq!(
                screen.status_message, "Payroll settings saved.",
                "{failure}"
            );
            assert_effective(&app, 5, "Down", PublicHolidayRegion::Scotland, 30);
            let persisted = crate::config::AppConfig::load(&path).unwrap();
            assert_eq!(persisted.payroll.rounding_minutes, 5);
            assert_eq!(persisted.payroll.rounding_direction, "Down");
            assert_eq!(
                persisted.payroll.public_holiday_region,
                PublicHolidayRegion::Scotland
            );
            assert_eq!(screen.form_state(), screen.saved_form);
        }
    }

    #[test]
    fn all_selectable_rules_save_consistently_and_preserve_other_settings() {
        for (minutes, up, down) in [
            (1, 34, 34),
            (5, 35, 30),
            (10, 40, 30),
            (15, 45, 30),
            (30, 60, 30),
            (60, 60, 0),
        ] {
            for (direction, payable) in [("Up", up), ("Down", down)] {
                let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
                let mut screen = PayrollSettingsScreen::new();
                screen.load(&app);
                screen.rounding_minutes = minutes;
                screen.rounding_direction = direction.into();
                screen.public_holiday_region = PublicHolidayRegion::NorthernIreland;
                let email_before = toml::to_string(&app.context.config.email).unwrap();
                screen.save(&mut app);
                assert_eq!(screen.status_message, "Payroll settings saved.");
                assert_effective(
                    &app,
                    minutes,
                    direction,
                    PublicHolidayRegion::NorthernIreland,
                    payable,
                );
                let persisted =
                    crate::config::AppConfig::load(&dir.path().join("config.toml")).unwrap();
                assert_eq!(persisted.payroll.rounding_minutes, minutes);
                assert_eq!(persisted.payroll.rounding_direction, direction);
                assert_eq!(
                    persisted.payroll.public_holiday_region,
                    PublicHolidayRegion::NorthernIreland
                );
                assert_eq!(
                    toml::to_string(&app.context.config.email).unwrap(),
                    email_before
                );
                assert_eq!(screen.form_state(), screen.saved_form);
            }
        }
    }
    #[test]
    fn failed_save_keeps_blue_button_and_visible_draft_then_retry_resets_it() {
        use crate::gui_controls::acceptance::{click, frame};
        let (_dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
        app.save_config().unwrap();
        let root = app.context.environment.data_dir.clone();
        let mut screen = PayrollSettingsScreen::new();
        screen.load(&app);
        screen.loaded = true;
        screen.rounding_minutes = 30;
        screen.rounding_direction = "Down".into();
        screen.public_holiday_region = PublicHolidayRegion::NorthernIreland;
        app.context.environment.data_dir = root.join("missing-parent");
        let ctx = egui::Context::default();
        let (pos, blue) = frame(
            &ctx,
            vec![],
            |ui| {
                screen.show(ui, &mut app);
            },
            "Save Payroll Settings",
        );
        assert!(blue);
        click(
            &ctx,
            pos,
            |ui| {
                screen.show(ui, &mut app);
            },
            "Save Payroll Settings",
        );
        assert!(screen.status_message.contains("save failed"));
        assert!(
            frame(
                &ctx,
                vec![],
                |ui| {
                    screen.show(ui, &mut app);
                },
                "Save Payroll Settings"
            )
            .1
        );
        assert_effective(&app, 15, "Up", PublicHolidayRegion::EnglandAndWales, 45);
        assert_eq!(screen.rounding_minutes, 30);
        assert_eq!(
            screen.public_holiday_region,
            PublicHolidayRegion::NorthernIreland
        );
        app.context.environment.data_dir = root;
        let (pos, _) = frame(
            &ctx,
            vec![],
            |ui| {
                screen.show(ui, &mut app);
            },
            "Save Payroll Settings",
        );
        click(
            &ctx,
            pos,
            |ui| {
                screen.show(ui, &mut app);
            },
            "Save Payroll Settings",
        );
        assert_eq!(screen.status_message, "Payroll settings saved.");
        assert!(
            !frame(
                &ctx,
                vec![],
                |ui| {
                    screen.show(ui, &mut app);
                },
                "Save Payroll Settings"
            )
            .1
        );
        assert_effective(&app, 30, "Down", PublicHolidayRegion::NorthernIreland, 30);
    }

    #[test]
    fn application_save_keeps_verified_password_reference_and_baseline_in_sync() {
        let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
        app.save_config().unwrap();
        let path = dir.path().join("config.toml");
        let original = std::fs::read(&path).unwrap();
        app.context.config.email.smtp_transport = "SMTP Server".into();
        app.context.config.email.smtp_host = "synthetic.example.invalid".into();
        app.context.config.email.smtp_username = "synthetic@example.test".into();
        app.context.config.email.smtp_password = "SYNTHETIC_SETTINGS_SAVE_SECRET".into();
        let root = app.context.environment.data_dir.clone();
        app.context.environment.data_dir = root.join("missing-parent");
        assert!(app.save_config().is_err());
        assert!(app.context.config.email.smtp_credential_ref.is_none());
        assert!(app.context.config.email.smtp_password_baseline.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        app.context.environment.data_dir = root;
        app.save_config().unwrap();
        let saved = app.context.config.email.clone();
        assert!(saved.smtp_credential_ref.is_some());
        assert_eq!(saved.smtp_password_baseline, saved.smtp_password);
        assert!(saved.smtp_credential_error.is_none());
        let persisted = crate::config::AppConfig::load(&path).unwrap().email;
        assert_eq!(persisted.smtp_credential_ref, saved.smtp_credential_ref);
        assert_eq!(persisted.smtp_password, saved.smtp_password);
        assert!(!std::fs::read_to_string(&path)
            .unwrap()
            .contains("SYNTHETIC_SETTINGS_SAVE_SECRET"));
        app.save_config().unwrap();
        assert_eq!(
            app.context.config.email.smtp_credential_ref,
            saved.smtp_credential_ref
        );
    }

    #[test]
    fn partial_database_saves_are_reported_without_activating_config_and_retry_is_idempotent() {
        for failure in ["annual", "config"] {
            let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
            app.save_config().unwrap();
            let config_path = dir.path().join("config.toml");
            let original_config = std::fs::read(&config_path).unwrap();
            app.payroll_provider_repository
                .save(&PayrollProvider {
                    id: 1,
                    name: Some("Original synthetic provider".into()),
                    email: None,
                    address: None,
                    telephone: None,
                    payroll_department_email: Some("payroll@example.test".into()),
                })
                .unwrap();
            let original_leave = StoredAnnualLeaveSettings::default();
            app.annual_leave_settings_repository
                .save(&original_leave)
                .unwrap();
            let db = crate::payroll_evidence::open(&app).unwrap();
            let mut screen = PayrollSettingsScreen::new();
            screen.load(&app);
            let baseline = screen.saved_form.clone();
            screen.provider_name = "Changed synthetic provider".into();
            screen.provider_telephone = "synthetic contact".into();
            screen.annual_leave.weeks = "6".into();
            screen.annual_leave.percentage = "14".into();
            screen.rounding_minutes = 60;
            screen.rounding_direction = "Down".into();
            screen.public_holiday_region = PublicHolidayRegion::Scotland;
            let root = app.context.environment.data_dir.clone();
            if failure == "annual" {
                db.execute_batch("CREATE TRIGGER refuse_partial_annual BEFORE INSERT ON annual_leave_settings BEGIN SELECT RAISE(ABORT,'synthetic refusal'); END;").unwrap();
            } else {
                app.context.environment.data_dir = dir.path().join("missing-config-parent");
            }
            screen.save(&mut app);
            assert!(screen
                .status_message
                .contains("Rounding and holiday region are unchanged"));
            let provider = app.payroll_provider_repository.get().unwrap().unwrap();
            assert_eq!(provider.name.as_deref(), Some("Changed synthetic provider"));
            assert_eq!(provider.telephone.as_deref(), Some("synthetic contact"));
            assert_eq!(
                provider.payroll_department_email.as_deref(),
                Some("payroll@example.test")
            );
            let leave = app.annual_leave_settings_repository.get().unwrap();
            if failure == "annual" {
                assert_eq!(leave, original_leave);
                assert!(screen
                    .status_message
                    .contains("provider details may have been saved"));
                db.execute_batch("DROP TRIGGER refuse_partial_annual")
                    .unwrap();
            } else {
                assert_eq!(leave.statutory_weeks, 6.0);
                assert_eq!(leave.accrual_percentage, 14.0);
                assert!(screen
                    .status_message
                    .contains("provider and annual-leave details were saved"));
                app.context.environment.data_dir = root;
            }
            assert_effective(&app, 15, "Up", PublicHolidayRegion::EnglandAndWales, 45);
            assert_eq!(std::fs::read(&config_path).unwrap(), original_config);
            assert_eq!(screen.saved_form, baseline);
            assert_ne!(screen.form_state(), screen.saved_form);
            assert_eq!(screen.annual_leave.weeks, "6");
            assert_eq!(screen.rounding_minutes, 60);
            assert_eq!(screen.public_holiday_region, PublicHolidayRegion::Scotland);
            screen.save(&mut app);
            assert_eq!(screen.status_message, "Payroll settings saved.");
            assert_effective(&app, 60, "Down", PublicHolidayRegion::Scotland, 0);
            let persisted = crate::config::AppConfig::load(&config_path).unwrap();
            assert_eq!(persisted.payroll.rounding_minutes, 60);
            assert_eq!(persisted.payroll.rounding_direction, "Down");
            assert_eq!(
                persisted.payroll.public_holiday_region,
                PublicHolidayRegion::Scotland
            );
            assert_eq!(
                app.annual_leave_settings_repository
                    .get()
                    .unwrap()
                    .statutory_weeks,
                6.0
            );
            assert_eq!(screen.form_state(), screen.saved_form);
            for table in ["payroll_provider", "annual_leave_settings"] {
                assert_eq!(
                    db.query_row::<i64, _, _>(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                        .get(0))
                        .unwrap(),
                    1
                );
            }
        }
    }

}

#[cfg(test)]
mod csv_settings_tests {
    use super::*;
    #[test]
    fn payroll_csv_mapping_saves_and_invalid_draft_preserves_effective_settings() {
        let (_dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
        let mut screen = PayrollSettingsScreen::new();
        screen.load(&app);
        let baseline = screen.form_state();
        screen.csv_options.introductory_rows = 0;
        screen.csv_options.hours_method = crate::csv_import::HoursMethod::EndMinusStart;
        screen.csv_headings[0] = "Worker | Employee".into();
        screen.csv_headings[3].clear();
        assert_ne!(screen.form_state(), baseline);
        screen.save(&mut app);
        assert_eq!(screen.status_message, "Payroll settings saved.");
        let expected = app.context.config.csv_import.clone();
        assert_eq!(expected.headings[0], ["Worker", "Employee"]);
        assert!(expected.headings[3].is_empty());
        assert_eq!(screen.saved_form, screen.form_state());
        let path = app.context.environment.data_dir.join("config.toml");
        let persisted = std::fs::read(&path).unwrap();
        let restored: crate::config::AppConfig =
            toml::from_str(std::str::from_utf8(&persisted).unwrap()).unwrap();
        assert_eq!(restored.csv_import, expected);
        screen.csv_headings[1] = "Worker".into();
        screen.save(&mut app);
        assert!(screen.status_message.contains("unique"));
        assert_eq!(app.context.config.csv_import, expected);
        assert_eq!(std::fs::read(path).unwrap(), persisted);
        screen.load(&app);
        assert_eq!(screen.csv_options, expected);
    }
    #[test]
    fn failed_payroll_config_save_keeps_csv_draft_without_activating_it() {
        let (dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
        app.save_config().unwrap();
        let original = app.context.config.csv_import.clone();
        let persisted = std::fs::read(dir.path().join("config.toml")).unwrap();
        let mut screen = PayrollSettingsScreen::new();
        screen.load(&app);
        screen.csv_options.introductory_rows = 0;
        screen.csv_options.hours_method = crate::csv_import::HoursMethod::EndMinusStart;
        let real_dir = app.context.environment.data_dir.clone();
        app.context.environment.data_dir = dir.path().join("missing-parent");
        screen.save(&mut app);
        assert_ne!(screen.status_message, "Payroll settings saved.");
        assert_eq!(app.context.config.csv_import, original);
        assert_eq!(screen.csv_options.introductory_rows, 0);
        assert_eq!(
            std::fs::read(dir.path().join("config.toml")).unwrap(),
            persisted
        );
        app.context.environment.data_dir = real_dir;
        screen.save(&mut app);
        assert_eq!(screen.status_message, "Payroll settings saved.");
        assert_eq!(app.context.config.csv_import.introductory_rows, 0);
    }
    #[test]
    fn heading_editors_remain_wide_in_bounds_and_editable_at_narrow_widths() {
        for width in [1000.0, 720.0, 640.0, 420.0, 320.0] {
            let ctx = egui::Context::default();
            let mut headings = std::array::from_fn(|i| {
                crate::csv_import::CsvImportConfig::default().headings[i].join(" | ")
            });
            let draw =
                |ctx: &egui::Context, headings: &mut [String; 8], events: Vec<egui::Event>| {
                    let mut bounds = Vec::new();
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 1200.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                bounds = csv_heading_editors(ui, headings, &mut Default::default())
                                    .into_iter()
                                    .map(|r| r.rect)
                                    .collect();
                            });
                        },
                    );
                    bounds
                };
            for _ in 0..2 {
                draw(&ctx, &mut headings, vec![]);
            }
            for rect in draw(&ctx, &mut headings, vec![]) {
                assert!(rect.width() >= 280.0 && rect.width() <= 480.0, "width {width}: {rect:?}");
                assert!(
                    rect.left() >= 0.0 && rect.right() <= width,
                    "editor exceeds width {width}: {rect:?}"
                );
            }
            let updated = "Personal Assistant Full Name | Employee Display Name";
            ctx.memory_mut(|m| m.request_focus(egui::Id::new(("csv_heading_mapping", 0usize))));
            draw(
                &ctx,
                &mut headings,
                vec![
                    egui::Event::Key {
                        key: egui::Key::A,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            ctrl: true,
                            command: true,
                            ..Default::default()
                        },
                    },
                    egui::Event::Paste(updated.into()),
                ],
            );
            assert_eq!(headings[0], updated, "editing at width {width}");
        }
    }

}

#[cfg(test)]
pub(crate) mod tab_acceptance {
    use super::*;
    use std::collections::BTreeMap;

    /// Settled, clipped text geometry from an actual egui frame, at desktop sizes.
    pub(crate) fn draw(
        ctx: &egui::Context,
        width: f32,
        events: Vec<egui::Event>,
        mut show: impl FnMut(&mut egui::Ui),
    ) -> BTreeMap<String, egui::Rect> {
        fn collect(
            shape: &egui::Shape,
            clip: egui::Rect,
            texts: &mut BTreeMap<String, egui::Rect>,
        ) {
            match shape {
                egui::Shape::Text(t) => {
                    let rect = egui::Rect::from_min_size(t.pos, t.galley.size());
                    if clip.intersects(rect) {
                        texts.insert(t.galley.job.text.clone(), rect);
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, clip, texts);
                    }
                }
                _ => {}
            }
        }
        ctx.style_mut(|s| s.animation_time = 0.0);
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(ui));
            },
        );
        let mut texts = BTreeMap::new();
        for shape in out.shapes {
            collect(&shape.shape, shape.clip_rect, &mut texts);
        }
        texts
    }

    pub(crate) fn select(
        ctx: &egui::Context,
        width: f32,
        label: &str,
        mut show: impl FnMut(&mut egui::Ui),
    ) {
        let mut texts = draw(ctx, width, vec![], &mut show);
        for _ in 0..3 {
            texts = draw(ctx, width, vec![], &mut show);
        }
        let rect = texts
            .get(label)
            .unwrap_or_else(|| panic!("Missing tab {label}"));
        assert!(
            rect.min.x >= 0.0 && rect.max.x <= width,
            "Tab {label} outside {width}: {rect:?}"
        );
        let pos = rect.center();
        for pressed in [true, false] {
            draw(
                ctx,
                width,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                &mut show,
            );
        }
    }

    #[test]
    fn payroll_tabs_navigate_keep_edits_and_save_across_tabs() {
        for width in [600.0, 1000.0] {
            let (_dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
            let ctx = egui::Context::default();
            let mut screen = PayrollSettingsScreen::new();
            draw(&ctx, width, vec![], |ui| {
                screen.show(ui, &mut app);
            });
            let baseline = screen.form_state();
            for (tab, label) in PayrollTab::ALL {
                select(&ctx, width, label, |ui| {
                    screen.show(ui, &mut app);
                });
                assert_eq!(screen.active_tab, tab);
                assert_eq!(
                    screen.form_state(),
                    baseline,
                    "Navigation must not dirty the form"
                );
                let texts = draw(&ctx, width, vec![], |ui| {
                    screen.show(ui, &mut app);
                });
                assert!(texts.contains_key("Save Payroll Settings"));
                for (_, tab_label) in PayrollTab::ALL {
                    let rect = texts.get(tab_label).expect("All tab labels remain visible");
                    assert!(rect.min.x >= 0.0 && rect.max.x <= width);
                }
                if tab == PayrollTab::Provider {
                    for label in ["Payroll Frequency", "Rounding Minutes", "Start of Workweek"] {
                        let rect = texts.get(label).expect("Payroll Rules must remain visible");
                        assert!(rect.max.x <= width, "{label} clipped at {width}");
                    }
                }
                if tab == PayrollTab::CsvImport {
                    assert!(texts.contains_key("CSV rows to skip"));
                    assert!(!texts.contains_key("Provider Name"));
                }
            }
            screen.provider_name = "Synthetic Payroll Provider".into();
            screen.csv_options.introductory_rows = 3;
            screen.csv_options.hours_method = crate::csv_import::HoursMethod::EndMinusStart;
            screen.csv_headings[0] = "Worker | Employee".into();
            screen.timesheet_footer_text = "Synthetic footer".into();
            screen.annual_leave.weeks = "6".into();
            let edited = screen.form_state();
            for (_, label) in PayrollTab::ALL {
                select(&ctx, width, label, |ui| {
                    screen.show(ui, &mut app);
                });
                assert_eq!(screen.form_state(), edited);
                assert_ne!(screen.saved_form, edited);
                assert!(
                    crate::gui_controls::acceptance::frame(
                        &ctx,
                        vec![],
                        |ui| {
                            screen.show(ui, &mut app);
                        },
                        "Save Payroll Settings"
                    )
                    .1,
                    "Dirty Save indicator must remain active on {label}"
                );
            }
            // Save from a tab that does not own the edited fields.
            select(&ctx, width, "Pay Rates", |ui| {
                screen.show(ui, &mut app);
            });
            select(&ctx, width, "Save Payroll Settings", |ui| {
                screen.show(ui, &mut app);
            });
            assert_eq!(screen.status_message, "Payroll settings saved.");
            let mut reloaded = PayrollSettingsScreen::new();
            reloaded.load(&app);
            assert_eq!(reloaded.form_state(), screen.form_state());
            assert_eq!(screen.saved_form, screen.form_state());
            assert!(
                !crate::gui_controls::acceptance::frame(
                    &ctx,
                    vec![],
                    |ui| {
                        screen.show(ui, &mut app);
                    },
                    "Save Payroll Settings"
                )
                .1,
                "Successful Save clears the dirty indicator"
            );
            let config = crate::config::AppConfig::load(
                &app.context.environment.data_dir.join("config.toml"),
            )
            .unwrap();
            assert_eq!(config.csv_import.introductory_rows, 3);
            assert_eq!(config.payroll.timesheet_footer_text, "Synthetic footer");
        }
    }
}

#[cfg(test)]
mod four_method_ui_tests {
    use super::tab_acceptance::{draw, select};
    use super::*;
    use crate::csv_import::{CsvImportConfig, HoursMethod};
    #[test]
    fn checkboxes_lock_required_fields_retain_aliases_and_update_live_preview() {
        for width in [1000.0, 600.0, 420.0] {
            let ctx = egui::Context::default();
            let mut config = CsvImportConfig::default();
            let mut headings = std::array::from_fn(|i| config.headings[i].join(" | "));
            let original = headings.clone();
            let show =
                |ui: &mut egui::Ui, headings: &mut [String; 8], config: &mut CsvImportConfig| {
                    csv_heading_editors(ui, headings, config);
                    csv_column_preview(ui, headings, config);
                };
            for _ in 0..3 {
                draw(&ctx, width, vec![], |ui| {
                    show(ui, &mut headings, &mut config)
                });
            }
            let texts = draw(&ctx, width, vec![], |ui| {
                show(ui, &mut headings, &mut config)
            });
            let header = texts.get("Client Name").expect("Spreadsheet heading");
            let sample = texts.get("Example PA").expect("Spreadsheet sample row");
            assert!(
                sample.top() > header.bottom(),
                "Sample must be beneath its heading, not beside it"
            );
            select(&ctx, width, "Rate (optional)", |ui| {
                show(ui, &mut headings, &mut config)
            });
            assert!(!config.enabled[5]);
            assert_eq!(headings, original);
            assert!(!csv_preview_columns(&headings, &config)
                .iter()
                .any(|(h, _)| h == "Rate/h"));
            select(&ctx, width, "Rate (optional)", |ui| {
                show(ui, &mut headings, &mut config)
            });
            assert!(config.enabled[5]);
            assert_eq!(headings, original);
            assert_eq!(csv_preview_columns(&headings, &config).len(), 8);
            for label in [
                "PA (required)",
                "Start (required)",
                "End (required)",
                "Worked Hours (required)",
            ] {
                select(&ctx, width, label, |ui| {
                    show(ui, &mut headings, &mut config)
                });
            }
            assert!(
                config.enabled[0] && config.enabled[1] && config.enabled[2] && config.enabled[4]
            );
            config.hours_method = HoursMethod::EndMinusStart;
            select(&ctx, width, "Worked Hours (optional)", |ui| {
                show(ui, &mut headings, &mut config)
            });
            select(&ctx, width, "Break (optional)", |ui| {
                show(ui, &mut headings, &mut config)
            });
            assert!(!config.enabled[3] && !config.enabled[4]);
            assert_eq!(csv_preview_columns(&headings, &config).len(), 6);
            config.hours_method = HoursMethod::WorkedHoursMinusBreak;
            draw(&ctx, width, vec![], |ui| {
                show(ui, &mut headings, &mut config)
            });
            assert!(config.enabled[3] && config.enabled[4]);
            assert_eq!(csv_preview_columns(&headings, &config).len(), 8);
            select(&ctx, width, "Break (required)", |ui| {
                show(ui, &mut headings, &mut config)
            });
            assert!(config.enabled[3]);
            assert_eq!(headings, original);
            headings[0] = "Worker | Employee".into();
            assert_eq!(csv_preview_columns(&headings, &config)[0].0, "Worker");
        }
    }
    #[test]
    fn four_method_column_configuration_saves_and_reloads_with_disabled_aliases() {
        let (_dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
        let mut screen = PayrollSettingsScreen::new();
        screen.load(&app);
        for method in HoursMethod::ALL {
            screen.csv_options.hours_method = method;
            screen.csv_options.enabled = [false; 8];
            screen.csv_options.require_method_fields();
            screen.csv_headings[5] = "Retained Rate | Hourly Rate".into();
            screen.save(&mut app);
            assert_eq!(screen.status_message, "Payroll settings saved.");
            let persisted = crate::config::AppConfig::load(
                &app.context.environment.data_dir.join("config.toml"),
            )
            .unwrap();
            assert_eq!(persisted.csv_import, screen.csv_options);
            assert!(!persisted.csv_import.enabled[5]);
            assert_eq!(
                persisted.csv_import.headings[5],
                ["Retained Rate", "Hourly Rate"]
            );
            let mut reloaded = PayrollSettingsScreen::new();
            reloaded.load(&app);
            assert_eq!(reloaded.form_state(), screen.form_state());
        }
    }
}

#[cfg(test)]
mod compact_preview_tests {
    use super::*;
    #[test]
    fn compact_columns_fit_normal_width_and_scroll_only_when_needed() {
        for custom in [false, true] {
            for width in [1000.0, 900.0, 600.0, 420.0] {
                let ctx = egui::Context::default();
                let mut config = crate::csv_import::CsvImportConfig::default();
                let headings = if custom {
                    [
                        "Personal Assistant Full Name",
                        "Shift Start Date and Time",
                        "Shift End Date and Time",
                        "Unpaid Break Duration",
                        "Payroll Worked Duration",
                        "Original Hourly Rate",
                        "Original Shift Amount",
                        "Original Shift Notes",
                    ]
                    .map(String::from)
                } else {
                    std::array::from_fn(|i| config.headings[i].join(" | "))
                };
                // Reuse the same context/scroll area while changing enabled columns.
                for enabled in [
                    [true; 8],
                    [true, true, true, false, true, false, false, true],
                    [true, true, true, false, false, false, false, false],
                    [true; 8],
                ] {
                    config.enabled = enabled;
                    let mut geometry = None;
                    let mut rects = Vec::new();
                    for _ in 0..4 {
                        let output = ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 800.0),
                                )),
                                ..Default::default()
                            },
                            |ctx| {
                                egui::CentralPanel::default().show(ctx, |ui| {
                                    let columns = csv_preview_columns(&headings, &config);
                                    let widths =
                                        csv_preview_widths(ui, &columns, ui.available_width());
                                    assert_eq!(
                                        widths.len(),
                                        enabled.iter().filter(|v| **v).count()
                                    );
                                    for ((_, sample), w) in columns.iter().zip(&widths) {
                                        let size = egui::WidgetText::from(*sample)
                                            .into_galley(
                                                ui,
                                                Some(egui::TextWrapMode::Extend),
                                                f32::INFINITY,
                                                egui::TextStyle::Body,
                                            )
                                            .size();
                                        assert!(*w >= size.x, "Example must not clip: {sample}");
                                    }
                                    let preview = csv_column_preview(ui, &headings, &config);
                                    geometry =
                                        Some((preview.content_size.x, preview.inner_rect.width()));
                                });
                            },
                        );
                        rects.clear();
                        fn collect(
                            shape: &egui::Shape,
                            clip: egui::Rect,
                            rects: &mut Vec<(String, egui::Rect)>,
                        ) {
                            match shape {
                                egui::Shape::Text(text) => {
                                    // Centered galleys have negative local x coordinates.
                                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                                    if clip.intersects(rect) {
                                        rects.push((text.galley.job.text.clone(), rect));
                                    }
                                }
                                egui::Shape::Vec(shapes) => {
                                    for shape in shapes {
                                        collect(shape, clip, rects);
                                    }
                                }
                                _ => {}
                            }
                        }
                        for shape in output.shapes {
                            collect(&shape.shape, shape.clip_rect, &mut rects);
                        }
                    }
                    let (content, viewport) = geometry.unwrap();
                    if width >= 900.0 || enabled.iter().filter(|v| **v).count() == 3 {
                        assert!(content<=viewport+1.0,"Unnecessary scroll custom={custom} width={width} content={content} viewport={viewport}");
                        assert!(rects.iter().all(|(_,r)|r.max.x<=width),"Text exceeds viewport custom={custom} width={width} content={content} viewport={viewport} enabled={enabled:?}: {:?}",rects.iter().filter(|(_,r)|r.max.x>width).collect::<Vec<_>>());
                    } else if enabled == [true; 8] {
                        assert!(
                            content > viewport,
                            "Narrow all-column preview should scroll instead of squeezing examples"
                        );
                    }
                }
            }
        }
    }
}
