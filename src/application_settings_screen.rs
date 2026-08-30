use eframe::egui;

use crate::app::Application;
use crate::config::ApplicationTheme;
use crate::theme::{apply_theme, theme_label};

pub struct ApplicationSettingsScreen {
    loaded: bool,

    theme: ApplicationTheme,

    csv_import: String,
    pdf_output: String,
    email_archive: String,
    payslip_folder: String,
    payroll_information_folder: String,

    regular_font: String,
    bold_font: String,

    week_commencing_font_size: String,
    hours_font_size: String,
    information_font_size: String,

    status_message: String,
}

impl ApplicationSettingsScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,

            theme: ApplicationTheme::default(),

            csv_import: String::new(),
            pdf_output: String::new(),
            email_archive: String::new(),
            payslip_folder: String::new(),
            payroll_information_folder: String::new(),

            regular_font: String::new(),
            bold_font: String::new(),

            week_commencing_font_size: String::new(),
            hours_font_size: String::new(),
            information_font_size: String::new(),

            status_message: "Application settings not loaded.".to_string(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &mut Application) -> bool {
        let mut open_email_settings = false;
        if !self.loaded {
            self.load(application);
            self.loaded = true;
        }

        ui.heading("Application Settings");

        ui.separator();

        ui.heading("Appearance");

        ui.horizontal(|ui| {
            ui.label("Application Theme");

            let previous_theme = self.theme;
            egui::ComboBox::from_id_salt("application_theme")
                .selected_text(theme_label(self.theme))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.theme, ApplicationTheme::System, "System");
                    ui.selectable_value(&mut self.theme, ApplicationTheme::Light, "Light");
                    ui.selectable_value(&mut self.theme, ApplicationTheme::SoftLight, "Soft Light");
                    ui.selectable_value(&mut self.theme, ApplicationTheme::Dark, "Dark");
                    ui.selectable_value(&mut self.theme, ApplicationTheme::SoftDark, "Soft Dark");
                    ui.selectable_value(&mut self.theme, ApplicationTheme::Blue, "Blue");
                    ui.selectable_value(
                        &mut self.theme,
                        ApplicationTheme::AccessibleHighContrast,
                        "Accessible High Contrast",
                    );
                });

            if self.theme != previous_theme {
                apply_theme(ui.ctx(), self.theme);
            }
        });

        ui.separator();

        ui.heading("Folders");

        egui::Grid::new("application_settings_folders")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.label("CSV Import Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.csv_import),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.csv_import = path.to_string_lossy().to_string();
                    }
                }

                ui.end_row();

                ui.label("PDF Export Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.pdf_output),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.pdf_output = path.to_string_lossy().to_string();
                    }
                }

                ui.end_row();

                ui.label("Email Archive Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.email_archive),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.email_archive = path.to_string_lossy().to_string();
                    }
                }

                ui.end_row();

                ui.label("Payslip Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.payslip_folder),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.payslip_folder = path.to_string_lossy().to_string();
                    }
                }

                ui.end_row();

                ui.label("Payroll Information / Bulletin Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.payroll_information_folder),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.payroll_information_folder = path.to_string_lossy().to_string();
                    }
                }

                ui.end_row();
            });

        ui.separator();

        ui.heading("PDF Fonts");

        egui::Grid::new("application_settings_fonts")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Regular Font");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.regular_font),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Font", &["ttf", "otf"])
                        .pick_file()
                    {
                        self.regular_font = path.to_string_lossy().to_string();
                    }
                }

                ui.end_row();

                ui.label("Bold Font");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.bold_font),
                );

                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Font", &["ttf", "otf"])
                        .pick_file()
                    {
                        self.bold_font = path.to_string_lossy().to_string();
                    }
                }

                ui.end_row();
            });

        ui.separator();

        ui.heading("PDF Font Sizes");

        egui::Grid::new("application_settings_font_sizes")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Week Commencing Dates");

                ui.add_sized(
                    [100.0, 20.0],
                    egui::TextEdit::singleline(&mut self.week_commencing_font_size),
                );

                ui.end_row();

                ui.label("Hours");

                ui.add_sized(
                    [100.0, 20.0],
                    egui::TextEdit::singleline(&mut self.hours_font_size),
                );

                ui.end_row();

                ui.label("Information / Secondary Text");

                ui.add_sized(
                    [100.0, 20.0],
                    egui::TextEdit::singleline(&mut self.information_font_size),
                );

                ui.end_row();
            });

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("Save Application Settings").clicked() {
                match self.save(application) {
                    Ok(()) => {
                        self.status_message = "Application settings saved.".to_string();
                    }

                    Err(error) => {
                        self.status_message =
                            format!("Failed saving application settings: {}", error);
                    }
                }
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
        let config = &application.context.config;

        self.theme = config.theme;

        self.csv_import = config.folders.csv_import.to_string_lossy().to_string();
        self.pdf_output = config.folders.pdf_output.to_string_lossy().to_string();
        self.email_archive = config.folders.email_archive.to_string_lossy().to_string();
        self.payslip_folder = config.folders.payslip_folder.to_string_lossy().to_string();
        self.payroll_information_folder = config
            .folders
            .payroll_information_folder
            .to_string_lossy()
            .to_string();

        self.regular_font = config.pdf.regular_font.to_string_lossy().to_string();
        self.bold_font = config.pdf.bold_font.to_string_lossy().to_string();

        self.week_commencing_font_size = config.pdf.week_commencing_font_size.to_string();
        self.hours_font_size = config.pdf.hours_font_size.to_string();
        self.information_font_size = config.pdf.information_font_size.to_string();

        self.status_message = "Application settings loaded.".to_string();
    }

    fn save(&self, application: &mut Application) -> Result<(), Box<dyn std::error::Error>> {
        let week_commencing_font_size = self
            .week_commencing_font_size
            .trim()
            .parse::<f64>()
            .map_err(|_| "Invalid Week Commencing font size.")?;

        let hours_font_size = self
            .hours_font_size
            .trim()
            .parse::<f64>()
            .map_err(|_| "Invalid Hours font size.")?;

        let information_font_size = self
            .information_font_size
            .trim()
            .parse::<f64>()
            .map_err(|_| "Invalid Information font size.")?;

        if week_commencing_font_size <= 0.0
            || hours_font_size <= 0.0
            || information_font_size <= 0.0
        {
            return Err("PDF font sizes must be greater than zero.".into());
        }

        application.context.config.theme = self.theme;

        application.context.config.folders.csv_import =
            std::path::PathBuf::from(self.csv_import.trim());

        application.context.config.folders.pdf_output =
            std::path::PathBuf::from(self.pdf_output.trim());

        application.context.config.folders.email_archive =
            std::path::PathBuf::from(self.email_archive.trim());

        application.context.config.folders.payslip_folder =
            std::path::PathBuf::from(self.payslip_folder.trim());

        application
            .context
            .config
            .folders
            .payroll_information_folder =
            std::path::PathBuf::from(self.payroll_information_folder.trim());

        application.context.config.pdf.regular_font =
            std::path::PathBuf::from(self.regular_font.trim());

        application.context.config.pdf.bold_font = std::path::PathBuf::from(self.bold_font.trim());

        application.context.config.pdf.week_commencing_font_size = week_commencing_font_size;

        application.context.config.pdf.hours_font_size = hours_font_size;

        application.context.config.pdf.information_font_size = information_font_size;

        application.save_config()?;

        Ok(())
    }
}
