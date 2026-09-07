use eframe::egui;

use crate::app::Application;
use crate::backup_service::BackupInfo;
use crate::config::ApplicationTheme;
use crate::theme::{apply_theme, theme_label};

pub struct ApplicationSettingsScreen {
    loaded: bool,

    theme: ApplicationTheme,
    hours_shift_date_time_spinner: bool,

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
    file_status: Option<(String, Vec<std::path::PathBuf>)>,
    backups: Vec<BackupInfo>,
    selected_backup: Option<std::path::PathBuf>,
    confirm_restore: bool,
    restart_message: Option<String>,
}

impl ApplicationSettingsScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,

            theme: ApplicationTheme::default(),
            hours_shift_date_time_spinner: false,

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
            file_status: None,
            backups: Vec::new(),
            selected_backup: None,
            confirm_restore: false,
            restart_message: None,
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
            crate::gui_controls::combo_box("application_theme")
                .selected_text(theme_label(self.theme))
                .show_ui(ui, |ui| {
                    crate::gui_controls::combo_value(
                        ui,
                        &mut self.theme,
                        ApplicationTheme::System,
                        "System",
                    );
                    crate::gui_controls::combo_value(
                        ui,
                        &mut self.theme,
                        ApplicationTheme::Light,
                        "Light",
                    );
                    crate::gui_controls::combo_value(
                        ui,
                        &mut self.theme,
                        ApplicationTheme::SoftLight,
                        "Soft Light",
                    );
                    crate::gui_controls::combo_value(
                        ui,
                        &mut self.theme,
                        ApplicationTheme::Dark,
                        "Dark",
                    );
                    crate::gui_controls::combo_value(
                        ui,
                        &mut self.theme,
                        ApplicationTheme::SoftDark,
                        "Soft Dark",
                    );
                    crate::gui_controls::combo_value(
                        ui,
                        &mut self.theme,
                        ApplicationTheme::Blue,
                        "Blue",
                    );
                    crate::gui_controls::combo_value(
                        ui,
                        &mut self.theme,
                        ApplicationTheme::AccessibleHighContrast,
                        "Accessible High Contrast",
                    );
                });

            if self.theme != previous_theme {
                apply_theme(ui.ctx(), self.theme);
            }

            let spinner_label = if self.hours_shift_date_time_spinner {
                "Hours Shift Date/Time Spinner: ON"
            } else {
                "Hours Shift Date/Time Spinner: OFF"
            };
            if ui.button(spinner_label).clicked() {
                self.hours_shift_date_time_spinner = !self.hours_shift_date_time_spinner;
            }
        });

        ui.separator();

        ui.heading("Folders");

        let mut folder_open_error = None;

        egui::Grid::new("application_settings_folders")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.label("CSV Import Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.csv_import),
                );

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.csv_import = path.to_string_lossy().to_string();
                        }
                    }
                    if folder_open_error.is_none() {
                        let path = crate::paths::expand_path(&std::path::PathBuf::from(
                            self.csv_import.trim(),
                        ));
                        folder_open_error = crate::folder_opener::button(ui, &path);
                    }
                });

                ui.end_row();

                ui.label("PDF Export Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.pdf_output),
                );

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.pdf_output = path.to_string_lossy().to_string();
                        }
                    }
                    if folder_open_error.is_none() {
                        let path = crate::paths::expand_path(&std::path::PathBuf::from(
                            self.pdf_output.trim(),
                        ));
                        folder_open_error = crate::folder_opener::button(ui, &path);
                    }
                });

                ui.end_row();

                ui.label("Email Archive Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.email_archive),
                );

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.email_archive = path.to_string_lossy().to_string();
                        }
                    }
                    if folder_open_error.is_none() {
                        let path = crate::paths::expand_path(&std::path::PathBuf::from(
                            self.email_archive.trim(),
                        ));
                        folder_open_error = crate::folder_opener::button(ui, &path);
                    }
                });

                ui.end_row();

                ui.label("Payslip Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.payslip_folder),
                );

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.payslip_folder = path.to_string_lossy().to_string();
                        }
                    }
                    if folder_open_error.is_none() {
                        let path = crate::paths::expand_path(&std::path::PathBuf::from(
                            self.payslip_folder.trim(),
                        ));
                        folder_open_error = crate::folder_opener::button(ui, &path);
                    }
                });

                ui.end_row();

                ui.label("Payroll Information / Bulletin Folder");

                ui.add_sized(
                    [400.0, 20.0],
                    egui::TextEdit::singleline(&mut self.payroll_information_folder),
                );

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.payroll_information_folder = path.to_string_lossy().to_string();
                        }
                    }
                    if folder_open_error.is_none() {
                        let path = crate::paths::expand_path(&std::path::PathBuf::from(
                            self.payroll_information_folder.trim(),
                        ));
                        folder_open_error = crate::folder_opener::button(ui, &path);
                    }
                });

                ui.end_row();
            });

        if let Some(error) = folder_open_error {
            self.status_message = error;
            self.file_status = None;
        }

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

        ui.heading("Backup");
        ui.label("Create a manual backup of the database and application configuration.");

        if ui.button("Create Backup").clicked() {
            match application.create_backup() {
                Ok(backup_path) => {
                    self.status_message =
                        format!("Backup created successfully at {}", backup_path.display());
                    self.file_status = Some((self.status_message.clone(), vec![backup_path]));
                    self.refresh_backups(application);
                }
                Err(error) => {
                    self.status_message = format!("Failed to create backup: {error}");
                    self.file_status = None;
                }
            }
        }

        ui.separator();
        ui.heading("Restore Backup");
        ui.label("Select a DirectPaymentTimesheets backup directory to restore.");

        if ui.button("Refresh Backup List").clicked() {
            self.refresh_backups(application);
        }

        egui::ScrollArea::vertical()
            .id_salt("restore_backup_list")
            .max_height(180.0)
            .show(ui, |ui| {
                if self.backups.is_empty() {
                    ui.label("No DirectPaymentTimesheets backups found.");
                }
                for backup in &self.backups {
                    let selected = self.selected_backup.as_ref() == Some(&backup.path);
                    let config_note = if backup.has_config {
                        "includes config.toml"
                    } else {
                        "database only"
                    };
                    if ui
                        .selectable_label(
                            selected,
                            format!(
                                "{} — {} — {}\n{}",
                                backup.directory_name,
                                backup.created_at,
                                config_note,
                                backup.path.display()
                            ),
                        )
                        .clicked()
                    {
                        self.selected_backup = Some(backup.path.clone());
                    }
                }
            });

        let restore_enabled = self.selected_backup.is_some() && self.restart_message.is_none();
        if ui
            .add_enabled(
                restore_enabled,
                egui::Button::new("Restore Selected Backup..."),
            )
            .clicked()
        {
            let selected = self.selected_backup.as_ref().unwrap();
            match application.validate_backup(selected) {
                Ok(validation) => {
                    self.status_message = format!(
                        "Backup validated successfully (schema version {}). Confirmation required.",
                        validation.schema_version
                    );
                    self.confirm_restore = true;
                }
                Err(error) => {
                    self.status_message = format!("Restore refused: {error}");
                }
            }
        }

        if self.confirm_restore {
            let selected = self.selected_backup.clone().unwrap();
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Confirm Backup Restoration")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.heading("Current application data and settings will be replaced");
                    ui.label(format!("Restore from: {}", selected.display()));
                    ui.label(
                        "A safety backup of the current database and configuration will be created first.",
                    );
                    ui.label(
                        "After restoration, you must close and restart DirectPaymentTimesheets before doing any further work.",
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .button("Restore Backup and Replace Current Data")
                            .clicked()
                        {
                            confirm = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });

            if cancel {
                self.confirm_restore = false;
                self.status_message = "Backup restoration cancelled; no data was changed.".into();
            } else if confirm {
                self.confirm_restore = false;
                match application.restore_backup(&selected) {
                    Ok(result) => {
                        let config_note = if result.config_restored {
                            "config.toml was restored."
                        } else {
                            "The selected backup had no config.toml; the current configuration was preserved."
                        };
                        let message = format!(
                            "Restore succeeded from {}. Safety backup created at {}. {} Close and restart DirectPaymentTimesheets before further work.",
                            result.restored_backup.display(),
                            result.safety_backup.display(),
                            config_note
                        );
                        self.status_message = message.clone();
                        self.file_status = Some((
                            message.clone(),
                            vec![result.restored_backup, result.safety_backup],
                        ));
                        self.restart_message = Some(message);
                    }
                    Err(error) => {
                        let message = format!("Restore failed: {error}");
                        self.status_message = message.clone();
                        self.file_status = None;
                        if error.restart_required() {
                            self.restart_message = Some(message);
                        }
                    }
                }
            }
        }

        ui.separator();

        let mut open_error = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(&self.status_message);
            if let Some((message, paths)) = &self.file_status {
                if message == &self.status_message {
                    for path in paths {
                        if open_error.is_none() {
                            open_error = crate::folder_opener::button(ui, path);
                        }
                    }
                }
            }
        });
        if let Some(error) = open_error {
            self.status_message = error;
            self.file_status = None;
        }
        open_email_settings
    }

    fn load(&mut self, application: &Application) {
        let config = &application.context.config;

        self.theme = config.theme;
        self.hours_shift_date_time_spinner = config.hours_shift_date_time_spinner;

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
        self.refresh_backups(application);
    }

    pub fn restart_message(&self) -> Option<&str> {
        self.restart_message.as_deref()
    }

    pub fn restart_paths(&self) -> &[std::path::PathBuf] {
        match (&self.restart_message, &self.file_status) {
            (Some(restart), Some((status, paths))) if restart == status => paths,
            _ => &[],
        }
    }

    fn refresh_backups(&mut self, application: &Application) {
        match application.discover_backups() {
            Ok(backups) => {
                if self
                    .selected_backup
                    .as_ref()
                    .is_some_and(|selected| !backups.iter().any(|backup| &backup.path == selected))
                {
                    self.selected_backup = None;
                }
                self.backups = backups;
            }
            Err(error) => {
                self.backups.clear();
                self.selected_backup = None;
                self.status_message = format!("Failed to list backups: {error}");
            }
        }
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
        application.context.config.hours_shift_date_time_spinner =
            self.hours_shift_date_time_spinner;

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
