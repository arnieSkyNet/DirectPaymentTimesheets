use chrono::Datelike;
use eframe::egui;

use crate::app::Application;
use crate::application_settings_screen::ApplicationSettingsScreen;
use crate::import_service::ImportSummary;
use crate::models::TimesheetEntry;
use crate::payroll_schedule_repository::PayrollSchedule;
use crate::payroll_settings_screen::PayrollSettingsScreen;
use crate::payroll_timesheet_screen::PayrollTimesheetScreen;
use crate::pdf_generator::{payroll_week_filename, PdfGenerator, TimesheetPdfData};
use crate::personal_assistant_screen::PersonalAssistantScreen;

enum ActiveScreen {
    Dashboard,
    Employer,
    PersonalAssistant,
    PayrollSettings,
    PayrollTimesheet,
    ApplicationSettings,
}

pub struct DirectPaymentApp {
    application: Application,
    version: String,
    status_message: String,
    last_import: Option<ImportSummary>,
    timesheets: Vec<TimesheetEntry>,
    payroll_schedules: Vec<PayrollSchedule>,
    employer_screen: crate::employer_screen::EmployerScreen,
    personal_assistant_screen: PersonalAssistantScreen,
    payroll_settings_screen: PayrollSettingsScreen,
    payroll_timesheet_screen: PayrollTimesheetScreen,
    application_settings_screen: ApplicationSettingsScreen,
    active_screen: ActiveScreen,
}

impl DirectPaymentApp {
    pub fn new(application: Application) -> Self {
        Self {
            version: application.context.version.clone(),
            application,
            status_message: "Application ready.".to_string(),
            last_import: None,
            timesheets: Vec::new(),
            payroll_schedules: Vec::new(),
            employer_screen: crate::employer_screen::EmployerScreen::new(),
            personal_assistant_screen: PersonalAssistantScreen::new(),
            payroll_settings_screen: PayrollSettingsScreen::new(),
            payroll_timesheet_screen: PayrollTimesheetScreen::new(),
            application_settings_screen: ApplicationSettingsScreen::new(),
            active_screen: ActiveScreen::Dashboard,
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Direct Payments Timesheets");
                ui.label(format!("Version {}", self.version));

                if ui.button(" Settings").clicked() {
                    self.active_screen = ActiveScreen::ApplicationSettings;
                }
            });
        });

        egui::TopBottomPanel::bottom("navigation").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Dashboard").clicked() {
                    self.active_screen = ActiveScreen::Dashboard;
                }

                if ui.button("Employer Maintenance").clicked() {
                    self.active_screen = ActiveScreen::Employer;
                }

                if ui.button("Personal Assistant Maintenance").clicked() {
                    self.active_screen = ActiveScreen::PersonalAssistant;
                }

                if ui.button("Payroll Settings").clicked() {
                    self.active_screen = ActiveScreen::PayrollSettings;
                }

                if ui.button("Payroll Timesheet Preparation").clicked() {
                    self.active_screen = ActiveScreen::PayrollTimesheet;
                }

                if ui.button("Exit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.active_screen {
            ActiveScreen::Dashboard => {
                self.draw_dashboard(ui);
            }

            ActiveScreen::Employer => {
                self.employer_screen.show(ui, &self.application);
            }

            ActiveScreen::PersonalAssistant => {
                self.personal_assistant_screen.show(ui, &self.application);
            }

            ActiveScreen::PayrollSettings => {
                self.payroll_settings_screen.show(ui, &mut self.application);
            }

            ActiveScreen::PayrollTimesheet => {
                self.payroll_timesheet_screen.show(ui, &self.application);
            }

            ActiveScreen::ApplicationSettings => {
                self.application_settings_screen
                    .show(ui, &mut self.application);
            }
        });
    }
}

impl DirectPaymentApp {
    fn draw_dashboard(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Dashboard");

        ui.separator();

        if ui.button("Import CSV").clicked() {
            match self.application.import_csv() {
                Ok(summary) => {
                    self.status_message = "Import completed successfully.".to_string();
                    self.last_import = Some(summary);
                }

                Err(error) => {
                    self.status_message = format!("Import failed: {}", error);
                    self.last_import = None;
                }
            }
        }

        if ui.button("View Imported CSV").clicked() {
            match self.application.get_timesheets() {
                Ok(entries) => {
                    self.timesheets = entries;

                    self.status_message =
                        format!("Loaded {} timesheets.", self.timesheets.len());
                }

                Err(error) => {
                    self.status_message =
                        format!("Failed loading timesheets: {}", error);
                }
            }
        }

        if ui.button("Generate Payroll Timesheets").clicked() {
            match self.generate_payroll_timesheets() {
                Ok(count) => {
                    self.status_message =
                        format!("Payroll timesheets generated: {} PDF(s).", count);
                }

                Err(error) => {
                    self.status_message =
                        format!("Payroll timesheet generation failed: {}", error);
                }
            }
        }

        if ui.button("Email Timesheets").clicked() {
            match self.email_timesheets() {
                Ok(count) => {
                    self.status_message =
                        format!("Timesheets emailed successfully: {}.", count);
                }

                Err(error) => {
                    self.status_message =
                        format!("Timesheet email failed: {}", error);
                }
            }
        }

        if ui.button("Import Payroll Return").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("ZIP files", &["zip"])
                .pick_file()
            {
                let payroll_year = current_payroll_year();

                match self.application.get_payroll_schedule(&payroll_year) {
                    Ok(schedules) => {
                        let today = chrono::Local::now().date_naive();

                        let current_schedule = schedules
                            .into_iter()
                            .filter_map(|schedule| {
                                let first_week = chrono::NaiveDate::parse_from_str(
                                    &schedule.first_week_commencing,
                                    "%d/%m/%Y",
                                )
                                .ok()?;

                                let cycle_end = first_week + chrono::Duration::days(27);

                                if first_week <= today && today <= cycle_end {
                                    Some((first_week, schedule))
                                } else {
                                    None
                                }
                            })
                            .max_by_key(|(date, _)| *date)
                            .map(|(_, schedule)| schedule);

                        match current_schedule {
                            Some(schedule) => {
                                match self.application.import_payroll_return(
                                    &path,
                                    &payroll_year,
                                    schedule.cycle_number,
                                ) {
                                    Ok(result) => {
                                        self.status_message = format!(
                                            "Payroll return imported: {} payslip(s), {} information file(s).",
                                            result.payslips_imported,
                                            result.information_files_imported
                                        );
                                    }

                                    Err(error) => {
                                        self.status_message =
                                            format!("Payroll return import failed: {}", error);
                                    }
                                }
                            }

                            None => {
                                self.status_message =
                                    "No current payroll cycle found.".to_string();
                            }
                        }
                    }

                    Err(error) => {
                        self.status_message =
                            format!("Failed loading payroll schedule: {}", error);
                    }
                }
            }
        }

        if ui.button("Email Payslips").clicked() {
            match self.email_payslips() {
                Ok(count) => {
                    self.status_message =
                        format!("Payslips emailed successfully: {}.", count);
                }

                Err(error) => {
                    self.status_message =
                        format!("Payslip email failed: {}", error);
                }
            }
        }

        if ui.button("Import Payroll Prep Sheet").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Payroll Prep Sheet", &["pdf", "docx"])
                .pick_file()
            {
                match self.application.import_payroll_prep_sheet(&path) {
                    Ok(count) => {
                        self.status_message =
                            format!("Payroll Prep Sheet imported: {} schedule entries.", count);
                    }

                    Err(error) => {
                        self.status_message =
                            format!("Payroll Prep Sheet import failed: {}", error);
                    }
                }
            }
        }

        if ui.button("View Payroll Schedule").clicked() {
            let payroll_year = current_payroll_year();

            match self.application.get_payroll_schedule(&payroll_year) {
                Ok(schedules) => {
                    self.payroll_schedules = schedules;

                    self.status_message = format!(
                        "Loaded {} payroll schedule entries.",
                        self.payroll_schedules.len()
                    );
                }

                Err(error) => {
                    self.status_message =
                        format!("Failed loading payroll schedule: {}", error);
                }
            }
        }

        ui.separator();

        ui.heading("Import Summary");

        match &self.last_import {
            Some(summary) => {
                ui.label(format!("Files discovered: {}", summary.files_discovered));
                ui.label(format!("Files processed: {}", summary.files_processed));
                ui.label(format!("Rows imported: {}", summary.rows_imported));
                ui.label(format!("Rows skipped: {}", summary.rows_skipped));
            }

            None => {
                ui.label("No import performed yet.");
            }
        }

        ui.separator();

        ui.label(format!("Status: {}", self.status_message));

        draw_payroll_schedule(ui, &self.payroll_schedules);
        draw_timesheets(ui, &self.timesheets);
        self.draw_timesheet_email_status(ui);
        });
    }

    fn email_payslips(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let employers = self.application.employer_repository.get_all()?;

        let employer = employers
            .into_iter()
            .next()
            .ok_or("No employer has been configured.")?;

        let employer_email = employer
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .ok_or("Employer has no email address.")?;

        let payroll_provider = self
            .application
            .payroll_provider_repository
            .get()?
            .ok_or("No Payroll Provider has been configured.")?;

        let payroll_department_email = payroll_provider
            .payroll_department_email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .ok_or("Payroll Department has no email address.")?;

        let payroll_year = current_payroll_year();

        let schedules = self.application.get_payroll_schedule(&payroll_year)?;

        let today = chrono::Local::now().date_naive();

        let current_schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date_checked(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(first_week, _)| *first_week)
            .map(|(_, schedule)| schedule)
            .ok_or_else(|| {
                format!(
                    "No current payroll cycle was found for {}.",
                    today.format("%d/%m/%Y")
                )
            })?;

        let assistants = self.application.personal_assistant_repository.get_all()?;

        let pdf_output_folder =
            crate::paths::expand_path(&self.application.context.config.folders.pdf_output);


        let mut sent = 0usize;

        for assistant in &assistants {
            let is_active = match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            };

            if !is_active {
                continue;
            }

            let personal_assistant_name =
                format!("{} {}", assistant.first_name, assistant.surname);

            let existing_status = self
                .application
                .payroll_timesheet_email_repository
                .get_for_pa_and_cycle(
                    assistant.id,
                    &payroll_year,
                    current_schedule.cycle_number,
                )?;

            if existing_status
                .as_ref()
                .and_then(|status| status.sent_at.as_ref())
                .is_some()
            {
                continue;
            }

            let recipient_email = assistant
                .email
                .as_deref()
                .map(str::trim)
                .filter(|email| !email.is_empty())
                .ok_or_else(|| {
                    format!(
                        "Personal Assistant {} has no email address.",
                        personal_assistant_name
                    )
                })?;

            let filename = format!(
                "Timesheet - {} - {}.pdf",
                personal_assistant_name,
                payroll_week_filename(&current_schedule.first_week_commencing)
            );

            let timesheet_path = pdf_output_folder.join(filename);

            if !timesheet_path.exists() {
                return Err(format!(
                    "Timesheet PDF not found for {}: {}",
                    personal_assistant_name,
                    timesheet_path.display()
                )
                .into());
            }

            self.application.send_timesheet_email(
                payroll_department_email,
                employer_email,
                recipient_email,
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                &current_schedule.first_week_commencing,
                &timesheet_path,
                &self.application.context.config.payroll.timesheet_email_body,
                employer.email_signature.as_deref(),
            )?;

            let sent_at = chrono::Local::now().to_rfc3339();

            self.application
                .payroll_timesheet_email_repository
                .mark_sent(
                    assistant.id,
                    &payroll_year,
                    current_schedule.cycle_number,
                    &sent_at,
                )?;

            sent += 1;
        }

        let all_active_sent = assistants.iter().filter(|assistant| {
            match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            }
        }).all(|assistant| {
            self.application
                .payroll_timesheet_email_repository
                .get_for_pa_and_cycle(
                    assistant.id,
                    &payroll_year,
                    current_schedule.cycle_number,
                )
                .ok()
                .flatten()
                .and_then(|status| status.sent_at)
                .is_some()
        });

        if all_active_sent {
            self.application
                .payroll_schedule_repository
                .mark_payslips_sent(current_schedule.id)?;
        }

        Ok(sent)
    }

    fn email_timesheets(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let employers = self.application.employer_repository.get_all()?;

        let employer = employers
            .into_iter()
            .next()
            .ok_or("No employer has been configured.")?;

        let employer_email = employer
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .ok_or("Employer has no email address.")?;

        let payroll_provider = self
            .application
            .payroll_provider_repository
            .get()?
            .ok_or("No Payroll Provider has been configured.")?;

        let payroll_department_email = payroll_provider
            .payroll_department_email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .ok_or("Payroll Department has no email address.")?;

        let payroll_year = current_payroll_year();

        let schedules = self.application.get_payroll_schedule(&payroll_year)?;

        let today = chrono::Local::now().date_naive();

        let current_schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date_checked(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(first_week, _)| *first_week)
            .map(|(_, schedule)| schedule)
            .ok_or_else(|| {
                format!(
                    "No current payroll cycle was found for {}.",
                    today.format("%d/%m/%Y")
                )
            })?;

        let assistants = self.application.personal_assistant_repository.get_all()?;

        let pdf_output_folder =
            crate::paths::expand_path(&self.application.context.config.folders.pdf_output);

        let mut sent = 0usize;

        for assistant in &assistants {
            let is_active = match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            };

            if !is_active {
                continue;
            }

            let personal_assistant_name =
                format!("{} {}", assistant.first_name, assistant.surname);

            let existing_status = self
                .application
                .payroll_timesheet_email_repository
                .get_for_pa_and_cycle(
                    assistant.id,
                    &payroll_year,
                    current_schedule.cycle_number,
                )?;

            if existing_status
                .as_ref()
                .and_then(|status| status.sent_at.as_ref())
                .is_some()
            {
                continue;
            }

            let personal_assistant_email = assistant
                .email
                .as_deref()
                .map(str::trim)
                .filter(|email| !email.is_empty())
                .ok_or_else(|| {
                    format!(
                        "Personal Assistant {} has no email address.",
                        personal_assistant_name
                    )
                })?;

            let filename = format!(
                "Timesheet - {} - {}.pdf",
                personal_assistant_name,
                payroll_week_filename(&current_schedule.first_week_commencing)
            );

            let timesheet_path = pdf_output_folder.join(filename);

            if !timesheet_path.exists() {
                return Err(format!(
                    "Timesheet PDF not found for {}: {}",
                    personal_assistant_name,
                    timesheet_path.display()
                )
                .into());
            }

            self.application.send_timesheet_email(
                payroll_department_email,
                employer_email,
                personal_assistant_email,
                &personal_assistant_name,
                assistant.date_of_birth.as_deref(),
                assistant.national_insurance_number.as_deref(),
                &current_schedule.first_week_commencing,
                &timesheet_path,
                &self.application.context.config.payroll.timesheet_email_body,
                employer.email_signature.as_deref(),
            )?;

            let sent_at = chrono::Local::now().to_rfc3339();

            self.application
                .payroll_timesheet_email_repository
                .mark_sent(
                    assistant.id,
                    &payroll_year,
                    current_schedule.cycle_number,
                    &sent_at,
                )?;

            sent += 1;
        }

        Ok(sent)
    }

    fn draw_timesheet_email_status(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Timesheet Email Status");

        let payroll_year = current_payroll_year();

        let schedules = match self.application.get_payroll_schedule(&payroll_year) {
            Ok(schedules) => schedules,
            Err(error) => {
                ui.label(format!("Unable to load payroll schedule: {}", error));
                return;
            }
        };

        let today = chrono::Local::now().date_naive();

        let current_schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date_checked(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(first_week, _)| *first_week)
            .map(|(_, schedule)| schedule);

        let current_schedule = match current_schedule {
            Some(schedule) => schedule,
            None => {
                ui.label("No current payroll cycle found.");
                return;
            }
        };

        let assistants = match self.application.personal_assistant_repository.get_all() {
            Ok(assistants) => assistants,
            Err(error) => {
                ui.label(format!("Unable to load Personal Assistants: {}", error));
                return;
            }
        };

        egui::Grid::new("timesheet_email_status")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Personal Assistant");
                ui.label("Status");
                ui.label("Sent");
                ui.end_row();

                for assistant in assistants {
                    let is_active = match &assistant.employment_status {
                        Some(status) => status.trim().eq_ignore_ascii_case("active"),
                        None => true,
                    };

                    if !is_active {
                        continue;
                    }

                    let name = format!("{} {}", assistant.first_name, assistant.surname);

                    let status = self
                        .application
                        .payroll_timesheet_email_repository
                        .get_for_pa_and_cycle(
                            assistant.id,
                            &payroll_year,
                            current_schedule.cycle_number,
                        );

                    match status {
                        Ok(Some(status)) if status.sent_at.is_some() => {
                            let sent_at = status.sent_at.unwrap();

                            let display_time =
                                chrono::DateTime::parse_from_rfc3339(&sent_at)
                                    .map(|date_time| {
                                        date_time
                                            .with_timezone(&chrono::Local)
                                            .format("%d %b %Y %H:%M")
                                            .to_string()
                                    })
                                    .unwrap_or(sent_at);

                            ui.label(name);
                            ui.label("Sent");
                            ui.label(display_time);
                        }

                        Ok(_) => {
                            ui.label(name);
                            ui.label("Not sent");
                            ui.label("");
                        }

                        Err(error) => {
                            ui.label(name);
                            ui.label("Error");
                            ui.label(format!("{}", error));
                        }
                    }

                    ui.end_row();
                }
            });
    }

    fn generate_payroll_timesheets(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let employers = self.application.employer_repository.get_all()?;

        let employer = employers
            .into_iter()
            .next()
            .ok_or("No employer has been configured.")?;

        let payroll_year = current_payroll_year();

        let schedules = self.application.get_payroll_schedule(&payroll_year)?;

        if schedules.is_empty() {
            return Err(
                format!("No payroll schedule has been loaded for {}.", payroll_year).into(),
            );
        }

        let today = chrono::Local::now().date_naive();

        let current_schedule = schedules
            .into_iter()
            .filter_map(|schedule| {
                let first_week = parse_date_checked(&schedule.first_week_commencing)?;

                let cycle_end = first_week + chrono::Duration::days(27);

                if first_week <= today && today <= cycle_end {
                    Some((first_week, schedule))
                } else {
                    None
                }
            })
            .max_by_key(|(first_week, _)| *first_week)
            .map(|(_, schedule)| schedule)
            .ok_or_else(|| {
                format!(
                    "No current payroll cycle was found for {}.",
                    today.format("%d/%m/%Y")
                )
            })?;

        let first_week = parse_date_checked(&current_schedule.first_week_commencing)
            .ok_or("Invalid first week commencing date in payroll schedule.")?;

        let week_dates = [
            first_week,
            first_week + chrono::Duration::days(7),
            first_week + chrono::Duration::days(14),
            first_week + chrono::Duration::days(21),
        ];

        let week_date_strings = [
            format_date(week_dates[0]),
            format_date(week_dates[1]),
            format_date(week_dates[2]),
            format_date(week_dates[3]),
        ];

        let assistants = self.application.personal_assistant_repository.get_all()?;

        let output_dir =
            crate::paths::expand_path(&self.application.context.config.folders.pdf_output);

        let mut generated = 0usize;

        for assistant in &assistants {
            let is_active = match &assistant.employment_status {
                Some(status) => status.trim().eq_ignore_ascii_case("active"),
                None => true,
            };

            if !is_active {
                continue;
            }

            let personal_assistant_name = format!("{} {}", assistant.first_name, assistant.surname);

            let payroll_timesheet = self
                .application
                .payroll_timesheet_repository
                .get_for_cycle_and_pa(&payroll_year, current_schedule.cycle_number, assistant.id)?
                .ok_or_else(|| {
                    format!(
                        "No Payroll Timesheet Preparation record exists for {}.",
                        personal_assistant_name
                    )
                })?;

            let payroll_weeks = self
                .application
                .payroll_timesheet_repository
                .get_weeks(payroll_timesheet.id)?;

            if payroll_weeks.len() != 4 {
                return Err(format!(
                    "Expected 4 payroll weeks for {}, found {}.",
                    personal_assistant_name,
                    payroll_weeks.len()
                )
                .into());
            }

            let pay_rate = self
                .application
                .get_current_pay_rate_for_personal_assistant(assistant.id)?;

            let pay_rate = match pay_rate {
                Some(rate) => rate.base_hourly_rate + rate.employer_top_up_rate,
                None => 0.0,
            };

            let contracted_hours = self
                .application
                .contracted_hours_repository
                .get_current_for_personal_assistant(assistant.id)?;

            let contracted_weekly_hours = contracted_hours
                .map(|hours| hours.contracted_hours)
                .unwrap_or_else(|| "0".to_string());

            let hours_worked = [
                format_pdf_hours(payroll_weeks[0].worked_hours),
                format_pdf_hours(payroll_weeks[1].worked_hours),
                format_pdf_hours(payroll_weeks[2].worked_hours),
                format_pdf_hours(payroll_weeks[3].worked_hours),
            ];

            let annual_leave_hours = [
                format_pdf_hours(payroll_weeks[0].annual_leave_hours),
                format_pdf_hours(payroll_weeks[1].annual_leave_hours),
                format_pdf_hours(payroll_weeks[2].annual_leave_hours),
                format_pdf_hours(payroll_weeks[3].annual_leave_hours),
            ];

            let sick_leave_hours = [
                format_pdf_hours(payroll_weeks[0].sick_leave_hours),
                format_pdf_hours(payroll_weeks[1].sick_leave_hours),
                format_pdf_hours(payroll_weeks[2].sick_leave_hours),
                format_pdf_hours(payroll_weeks[3].sick_leave_hours),
            ];

            let public_holiday_hours = [
                format_pdf_hours(payroll_weeks[0].public_holiday_hours),
                format_pdf_hours(payroll_weeks[1].public_holiday_hours),
                format_pdf_hours(payroll_weeks[2].public_holiday_hours),
                format_pdf_hours(payroll_weeks[3].public_holiday_hours),
            ];

            let public_holidays = self
                .application
                .payroll_timesheet_repository
                .get_public_holidays(payroll_timesheet.id)?;

            let public_holiday_dates = [
                public_holidays
                    .iter()
                    .filter(|holiday| holiday.week_number == 1 && holiday.hours > 0.0)
                    .map(|holiday| {
                        format!(
                            "{} ({})",
                            format_pdf_hours(holiday.hours),
                            holiday.holiday_date
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                public_holidays
                    .iter()
                    .filter(|holiday| holiday.week_number == 2 && holiday.hours > 0.0)
                    .map(|holiday| {
                        format!(
                            "{} ({})",
                            format_pdf_hours(holiday.hours),
                            holiday.holiday_date
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                public_holidays
                    .iter()
                    .filter(|holiday| holiday.week_number == 3 && holiday.hours > 0.0)
                    .map(|holiday| {
                        format!(
                            "{} ({})",
                            format_pdf_hours(holiday.hours),
                            holiday.holiday_date
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                public_holidays
                    .iter()
                    .filter(|holiday| holiday.week_number == 4 && holiday.hours > 0.0)
                    .map(|holiday| {
                        format!(
                            "{} ({})",
                            format_pdf_hours(holiday.hours),
                            holiday.holiday_date
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ];

            let travel_miles = [
                format_pdf_hours(payroll_weeks[0].travel_miles),
                format_pdf_hours(payroll_weeks[1].travel_miles),
                format_pdf_hours(payroll_weeks[2].travel_miles),
                format_pdf_hours(payroll_weeks[3].travel_miles),
            ];

            let previous_cycle_hours =
                payroll_timesheet.previous_cycle_hours.map(format_pdf_hours);

            let employer_signature_path = employer
                .employer_signature
                .as_deref()
                .map(|path| crate::paths::expand_path(&std::path::PathBuf::from(path)))
                .filter(|path| path.exists());

            let pa_signature_path = assistant
                .signature
                .as_deref()
                .map(|path| crate::paths::expand_path(&std::path::PathBuf::from(path)))
                .filter(|path| path.exists());

            let data = TimesheetPdfData {
                employer_name: &employer.name,

                personal_assistant_name: &personal_assistant_name,

                national_insurance_number: assistant
                    .national_insurance_number
                    .as_deref()
                    .unwrap_or(""),

                contracted_weekly_hours: &contracted_weekly_hours,

                pay_rate,

                week_commencing_dates: [
                    &week_date_strings[0],
                    &week_date_strings[1],
                    &week_date_strings[2],
                    &week_date_strings[3],
                ],

                hours_worked: [
                    &hours_worked[0],
                    &hours_worked[1],
                    &hours_worked[2],
                    &hours_worked[3],
                ],

                annual_leave_hours: [
                    &annual_leave_hours[0],
                    &annual_leave_hours[1],
                    &annual_leave_hours[2],
                    &annual_leave_hours[3],
                ],

                sick_leave_hours: [
                    &sick_leave_hours[0],
                    &sick_leave_hours[1],
                    &sick_leave_hours[2],
                    &sick_leave_hours[3],
                ],

                public_holiday_hours: [
                    &public_holiday_hours[0],
                    &public_holiday_hours[1],
                    &public_holiday_hours[2],
                    &public_holiday_hours[3],
                ],

                public_holiday_dates: [
                    &public_holiday_dates[0],
                    &public_holiday_dates[1],
                    &public_holiday_dates[2],
                    &public_holiday_dates[3],
                ],

                travel_miles: [
                    &travel_miles[0],
                    &travel_miles[1],
                    &travel_miles[2],
                    &travel_miles[3],
                ],

                previous_cycle_hours: previous_cycle_hours.as_deref(),

                employer_signature_path: employer_signature_path.as_deref(),

                pa_signature_path: pa_signature_path.as_deref(),
            };

            PdfGenerator::generate(&output_dir, &data, &self.application.context.config.pdf)?;

            generated += 1;
        }

        Ok(generated)
    }
}

fn current_payroll_year() -> String {
    let today = chrono::Local::now().date_naive();

    let start_year = if today.month() >= 4 {
        today.year()
    } else {
        today.year() - 1
    };

    format!("{}/{}", start_year, (start_year + 1) % 100)
}

fn format_pdf_hours(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    if value.fract() == 0.0 {
        return format!("{:.0}", value);
    }

    let text = format!("{:.2}", value);

    text.trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn parse_date_checked(value: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(value.trim(), "%d/%m/%Y")
        .or_else(|_| chrono::NaiveDate::parse_from_str(value.trim(), "%d-%m-%Y"))
        .or_else(|_| chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d"))
        .ok()
}

fn format_date(date: chrono::NaiveDate) -> String {
    date.format("%d/%m/%Y").to_string()
}

fn draw_timesheets(ui: &mut egui::Ui, timesheets: &[TimesheetEntry]) {
    ui.separator();

    ui.heading("Timesheets");

    if timesheets.is_empty() {
        ui.label("No timesheets loaded.");
        return;
    }

    egui::Grid::new("timesheet_grid")
        .striped(true)
        .show(ui, |ui| {
            ui.label("PA Name");
            ui.label("Start");
            ui.label("End");
            ui.label("Worked");
            ui.label("Rate");
            ui.label("Amount");
            ui.end_row();

            for entry in timesheets {
                ui.label(&entry.pa_name);
                ui.label(&entry.start_time);
                ui.label(&entry.end_time);
                ui.label(format_worked_time(entry.worked_minutes));
                ui.label(format!("£{:.2}", entry.hourly_rate));
                ui.label(format!("£{:.2}", entry.amount));
                ui.end_row();
            }
        });
}

fn format_worked_time(minutes: i64) -> String {
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    format!("{}h {}m", hours, remaining_minutes)
}

fn draw_payroll_schedule(ui: &mut egui::Ui, schedules: &[PayrollSchedule]) {
    ui.separator();

    ui.heading("Payroll Schedule");

    if schedules.is_empty() {
        ui.label("No payroll schedule loaded.");
        return;
    }

    egui::Grid::new("payroll_schedule_grid")
        .striped(true)
        .show(ui, |ui| {
            ui.label("Cycle");
            ui.label("First Week Commencing");
            ui.label("Latest Posting Date");
            ui.label("Pay Date");
            ui.label("Payslips");
            ui.end_row();

            for schedule in schedules {
                ui.label(schedule.cycle_number.to_string());
                ui.label(&schedule.first_week_commencing);
                ui.label(&schedule.latest_posting_date);
                ui.label(&schedule.pay_date);

                if schedule.payslips_sent {
                    ui.label("Sent");
                } else {
                    ui.label("Not sent");
                }

                ui.end_row();
            }
        });
}

