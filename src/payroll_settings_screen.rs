use crate::annual_leave_settings_repository::AnnualLeaveSettings as StoredAnnualLeaveSettings;
use eframe::egui;

use crate::app::Application;
use crate::pay_rate_repository::PersonalAssistantPayRate;
use crate::payroll_provider_repository::PayrollProvider;
use chrono::Local;

pub struct PayrollSettingsScreen {
    loaded: bool,
    annual_leave: AnnualLeaveSettings,

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
            annual_leave: AnnualLeaveSettings::default(),

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

        ui.heading("Payroll Rules");

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Payroll Frequency");

                crate::gui_controls::combo_box("payroll_frequency")
                    .selected_text(&self.frequency)
                    .show_ui(ui, |ui| {
                        for option in [
                            "Weekly",
                            "Bi-Weekly",
                            "Semi-Monthly",
                            "Monthly",
                            "Every Four Weeks",
                            "Quarterly",
                        ] {
                            crate::gui_controls::combo_value(
                                ui,
                                &mut self.frequency,
                                option.to_string(),
                                option,
                            );
                        }
                    });
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

                crate::gui_controls::combo_box("workweek_start")
                    .selected_text(&self.start_of_workweek)
                    .show_ui(ui, |ui| {
                        for day in [
                            "Sunday",
                            "Monday",
                            "Tuesday",
                            "Wednesday",
                            "Thursday",
                            "Friday",
                            "Saturday",
                        ] {
                            crate::gui_controls::combo_value(
                                ui,
                                &mut self.start_of_workweek,
                                day.to_string(),
                                day,
                            );
                        }
                    });
            });
        });

        ui.separator();

        ui.checkbox(&mut self.overtime_enabled, "Enable overtime calculations");
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
            if ui.button("Save Payroll Settings").clicked() {
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

    fn load(&mut self, application: &Application) {
        let payroll = &application.context.config.payroll;

        self.annual_leave.load(application);
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

        self.status_message = "Payroll settings loaded.".to_string();
    }

    fn save(&mut self, application: &mut Application) {
        self.save_settings(application);
        self.annual_leave.status = self.status_message.clone();
    }

    fn save_settings(&mut self, application: &mut Application) {
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
        let payroll = &mut application.context.config.payroll;

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

        if let Err(error) = application.save_config() {
            self.status_message = format!(
                "Failed saving payroll settings: {error}. Annual-leave settings were not saved."
            );
            return;
        }

        match application
            .annual_leave_settings_repository
            .save(&annual_leave)
        {
            Ok(settings) => {
                self.annual_leave.fill(&settings);
                self.status_message = "Payroll settings saved.".into();
            }
            Err(error) => {
                self.status_message = format!("Payroll Settings save incomplete: other payroll settings were saved, but annual-leave settings failed: {}", annual_leave_error(error));
            }
        }
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
                rusqlite::Connection::open(&application.context.environment.database_path).unwrap();
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
                assert_eq!(
                    application.annual_leave_settings_repository.get().unwrap(),
                    StoredAnnualLeaveSettings::default()
                );
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM annual_leave_settings", [], |row| row
                            .get::<_, i64>(
                            0
                        ))
                        .unwrap(),
                    0
                );
                if scenario == "invalid" {
                    assert!(!directory.path().join("config.toml").exists());
                }
                if scenario == "annual_failure" {
                    assert!(screen.status_message.contains("save incomplete"));
                    assert!(screen
                        .status_message
                        .contains("other payroll settings were saved"));
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
