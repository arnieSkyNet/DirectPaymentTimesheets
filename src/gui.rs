use eframe::egui;

use crate::app::Application;
use crate::import_service::ImportSummary;
use crate::models::TimesheetEntry;
use crate::payroll_schedule_repository::PayrollSchedule;
use crate::payroll_settings_screen::PayrollSettingsScreen;
use crate::payroll_timesheet_screen::PayrollTimesheetScreen;
use crate::pdf_generator::{PdfGenerator, TimesheetPdfData};
use crate::personal_assistant_screen::PersonalAssistantScreen;

enum ActiveScreen {
    Dashboard,
    Employer,
    PersonalAssistant,
    PayrollSettings,
    PayrollTimesheet,
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
            active_screen: ActiveScreen::Dashboard,
        }
    }
}

impl eframe::App for DirectPaymentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.heading("Direct Payments Timesheets");
            ui.label(format!("Version {}", self.version));
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
        });
    }
}

impl DirectPaymentApp {
    fn draw_dashboard(&mut self, ui: &mut egui::Ui) {
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
            match self.application.get_payroll_schedule("2026/27") {
                Ok(schedules) => {
                    self.payroll_schedules = schedules;

                    self.status_message = format!(
                        "Loaded {} payroll schedule entries.",
                        self.payroll_schedules.len()
                    );
                }

                Err(error) => {
                    self.status_message = format!("Failed loading payroll schedule: {}", error);
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
                    self.status_message = format!("Payroll timesheet generation failed: {}", error);
                }
            }
        }

        if ui.button("View Timesheets").clicked() {
            match self.application.get_timesheets() {
                Ok(entries) => {
                    self.timesheets = entries;

                    self.status_message = format!("Loaded {} timesheets.", self.timesheets.len());
                }

                Err(error) => {
                    self.status_message = format!("Failed loading timesheets: {}", error);
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
    }

    fn generate_payroll_timesheets(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let employers = self.application.employer_repository.get_all()?;

        let employer = employers
            .into_iter()
            .next()
            .ok_or("No employer has been configured.")?;

        let schedules = self.application.get_payroll_schedule("2026/27")?;

        if schedules.is_empty() {
            return Err("No payroll schedule has been loaded for 2026/27.".into());
        }

        let today = chrono::Local::now().date_naive();

        // Find the most recently completed payroll cycle.
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

        let signatures_dir = crate::paths::expand_path(&std::path::PathBuf::from(
            "~/.directpaymenttimesheets/signatures",
        ));

        let pa_signatures_dir = signatures_dir.join("pa");

        let employer_signature = signatures_dir.join("employer-rgb.png");

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

            // --------------------------------------------------------
            // Get the saved Payroll Timesheet Preparation record.
            // --------------------------------------------------------

            let payroll_timesheet = self
                .application
                .payroll_timesheet_repository
                .get_for_cycle_and_pa("2026/27", current_schedule.cycle_number, assistant.id)?
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

            // --------------------------------------------------------
            // Pay rate
            // --------------------------------------------------------

            let pay_rate = self
                .application
                .get_current_pay_rate_for_personal_assistant(assistant.id)?;

            let pay_rate = match pay_rate {
                Some(rate) => rate.base_hourly_rate + rate.employer_top_up_rate,

                None => 0.0,
            };

            // --------------------------------------------------------
            // Contracted hours
            // --------------------------------------------------------

            let contracted_hours = self
                .application
                .contracted_hours_repository
                .get_current_for_personal_assistant(assistant.id)?;

            let contracted_weekly_hours = contracted_hours
                .map(|hours| hours.contracted_hours)
                .unwrap_or_else(|| "0".to_string());

            // --------------------------------------------------------
            // Convert prepared values to PDF strings.
            // --------------------------------------------------------

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

            let previous_cycle_hours = payroll_timesheet.previous_cycle_hours.map(format_pdf_hours);

            // --------------------------------------------------------
            // PA signature
            // --------------------------------------------------------

            let pa_signature_filename =
                signature_filename(&assistant.first_name, &assistant.surname);

            let pa_signature = pa_signatures_dir.join(pa_signature_filename);

            let employer_signature_path = if employer_signature.exists() {
                Some(employer_signature.as_path())
            } else {
                None
            };

            let pa_signature_path = if pa_signature.exists() {
                Some(pa_signature.as_path())
            } else {
                None
            };

            // --------------------------------------------------------
            // Build PDF data from the SAVED preparation record.
            // --------------------------------------------------------

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

                employer_signature_path,

                pa_signature_path,
            };

            PdfGenerator::generate(&output_dir, &data)?;

            generated += 1;
        }

        Ok(generated)
    }
}

fn format_pdf_hours(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    if value.fract() == 0.0 {
        return format!("{:.0}", value);
    }

    let text = format!("{:.2}", value);

    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn signature_filename(first_name: &str, surname: &str) -> String {
    format!(
        "{}-{}-rgb.png",
        first_name.trim().to_lowercase(),
        surname.trim().to_lowercase()
    )
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
            ui.end_row();

            for schedule in schedules {
                ui.label(schedule.cycle_number.to_string());

                ui.label(&schedule.first_week_commencing);

                ui.label(&schedule.latest_posting_date);

                ui.label(&schedule.pay_date);

                ui.end_row();
            }
        });
}
