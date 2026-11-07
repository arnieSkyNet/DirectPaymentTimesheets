use crate::annual_leave_settings_repository::AnnualLeaveSettings as StoredAnnualLeaveSettings;
use eframe::egui;

use crate::app::Application;
use crate::pay_rate_repository::PersonalAssistantPayRate;
use crate::payroll_provider_repository::PayrollProvider;
use chrono::Local;

pub struct PayrollSettingsScreen {
    loaded: bool,
    saved_form: String,
    annual_leave: AnnualLeaveSettings,

    public_holiday_region: crate::public_holidays::PublicHolidayRegion,
    holiday_refresh: crate::public_holidays::Refresh,
    frequency: String,
    rounding_minutes: i64,
    rounding_direction: String,
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
            saved_form: String::new(),
            annual_leave: AnnualLeaveSettings::default(),

            public_holiday_region: Default::default(),
            holiday_refresh: Default::default(),
            frequency: "Every Four Weeks".to_string(),
            rounding_minutes: 15,
            rounding_direction: "Up".to_string(),
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
        ui.heading("Payroll Rules");

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
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
        self.annual_leave.show(ui);

        ui.separator();
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

        ui.horizontal(|ui| {
            if crate::gui_controls::save_button(ui,"Save Payroll Settings",self.form_state()!=self.saved_form).clicked() {
                self.save(application);
            }

            if ui.button("Email Settings").clicked() {
                open_email_settings = true;
            }
        });

        ui.separator();

        ui.label(&self.status_message);
        open_email_settings
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

    fn form_state(&self) -> String { vec![format!("{:?}", self.public_holiday_region),format!("{:?}", self.frequency),format!("{:?}", self.rounding_minutes),format!("{:?}", self.rounding_direction),format!("{:?}", self.start_of_workweek),format!("{:?}", self.provider_name),format!("{:?}", self.provider_email),format!("{:?}", self.provider_address),format!("{:?}", self.provider_telephone),format!("{:?}", self.overtime_enabled),format!("{:?}", self.timesheet_footer_text),format!("{:?}", self.timesheet_footer_font_size),format!("{:?}", self.annual_leave.contracted_from),format!("{:?}", self.annual_leave.weeks),format!("{:?}", self.annual_leave.variable_from),format!("{:?}", self.annual_leave.percentage)].join("\0") }
    fn load(&mut self, application: &Application) {
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
        let mut candidate = application.context.config.clone();
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
        self.saved_form = self.form_state();
        self.status_message = "Payroll settings saved.".into();
    }
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
