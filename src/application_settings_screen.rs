use eframe::egui;

use crate::app::Application;
use crate::backup_service::BackupInfo;
use crate::config::ApplicationTheme;
use crate::theme::{apply_theme, theme_label};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApplicationTab {
    Appearance,
    Folders,
    Pdf,
    Storage,
    Backup,
}

impl ApplicationTab {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Appearance, "Appearance"),
        (Self::Folders, "Folders"),
        (Self::Pdf, "PDF Settings"),
        (Self::Storage, "Storage & Security"),
        (Self::Backup, "Backup & Restore"),
    ];
}

pub struct ApplicationSettingsScreen {
    loaded: bool,
    active_tab: ApplicationTab,
    saved_form: String,

    theme: ApplicationTheme,
    dashboard_buttons: crate::config::DashboardButtons,
    date_display_format: crate::date_utils::DateDisplayFormat,
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
    storage_assessment: Vec<String>,
    storage_assessment_pending: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    backups: Vec<BackupInfo>,
    selected_backup: Option<std::path::PathBuf>,
    confirm_restore: bool,
    restore_plan: Option<crate::backup_service::RestorePlan>,
    restore_reason: String,
    restore_acknowledged: bool,
    restart_message: Option<String>,
}

impl ApplicationSettingsScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,
            active_tab: ApplicationTab::Appearance,
            saved_form: String::new(),

            theme: ApplicationTheme::default(),
            dashboard_buttons: Default::default(),
            date_display_format: Default::default(),
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
            storage_assessment: Vec::new(),
            storage_assessment_pending: None,
            backups: Vec::new(),
            selected_backup: None,
            confirm_restore: false,
            restore_plan: None,
            restore_reason: String::new(),
            restore_acknowledged: false,
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

        ui.horizontal_wrapped(|ui| {
            for (tab, label) in ApplicationTab::ALL {
                ui.selectable_value(&mut self.active_tab, tab, label);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if crate::gui_controls::save_button(
                ui,
                "Save Application Settings",
                self.form_state() != self.saved_form,
            )
            .clicked()
            {
                match self.save(application) {
                    Ok(()) => {
                        self.saved_form = self.form_state();
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
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt(("application_settings_tab", self.active_tab as u8))
            .show(ui, |ui| match self.active_tab {
                ApplicationTab::Appearance => self.show_appearance(ui, application),
                ApplicationTab::Folders => self.show_folders(ui, application),
                ApplicationTab::Pdf => self.show_pdf(ui, application),
                ApplicationTab::Storage => self.show_storage(ui, application),
                ApplicationTab::Backup => self.show_backup(ui, application),
            });
        open_email_settings
    }

    fn show_appearance(&mut self, ui: &mut egui::Ui, _application: &mut Application) {
        ui.heading("Appearance");
        ui.horizontal_wrapped(|ui| {
            ui.label("Calendar date display");
            crate::gui_controls::combo_box("calendar_date_display")
                .selected_text(self.date_display_format.label())
                .show_ui(ui, |ui| {
                    for format in crate::date_utils::DateDisplayFormat::ALL {
                        ui.selectable_value(&mut self.date_display_format, format, format.label());
                    }
                });
        });

        ui.horizontal_wrapped(|ui| {
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
        ui.heading("Dashboard Buttons");
        ui.label("Dashboard Button Height");
        ui.add(egui::Slider::new(&mut self.dashboard_buttons.height, 24.0..=52.0).step_by(1.0).suffix(" px"));
        ui.label("Dashboard Button Font Size");
        ui.add(egui::Slider::new(&mut self.dashboard_buttons.font_size, 11.0..=18.0).step_by(1.0).suffix(" pt"));
        ui.label("Live preview · Save Application Settings to retain these values.");
        ui.small("Buttons reserve enough space for their text. Scroll horizontally in narrow windows.");
        ui.push_id("dashboard_buttons_preview", |ui| {
            crate::gui::draw_dashboard_workflow_actions(ui, self.dashboard_buttons);
        });
        ui.separator();
    }

    fn show_folders(&mut self, ui: &mut egui::Ui, _application: &mut Application) {
        ui.heading("Folders");

        let mut folder_open_error = None;

        egui::Grid::new("application_settings_folders")
            .num_columns(1)
            .striped(true)
            .show(ui, |ui| {
                ui.label("CSV Import Folder");

                ui.end_row();
                ui.add_sized(
                    [ui.available_width().min(700.0), 20.0],
                    egui::TextEdit::singleline(&mut self.csv_import),
                );

                ui.end_row();
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

                ui.end_row();
                ui.add_sized(
                    [ui.available_width().min(700.0), 20.0],
                    egui::TextEdit::singleline(&mut self.pdf_output),
                );

                ui.end_row();
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

                ui.end_row();
                ui.add_sized(
                    [ui.available_width().min(700.0), 20.0],
                    egui::TextEdit::singleline(&mut self.email_archive),
                );

                ui.end_row();
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

                ui.end_row();
                ui.add_sized(
                    [ui.available_width().min(700.0), 20.0],
                    egui::TextEdit::singleline(&mut self.payslip_folder),
                );

                ui.end_row();
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

                ui.end_row();
                ui.add_sized(
                    [ui.available_width().min(700.0), 20.0],
                    egui::TextEdit::singleline(&mut self.payroll_information_folder),
                );

                ui.end_row();
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
    }

    fn show_pdf(&mut self, ui: &mut egui::Ui, _application: &mut Application) {
        ui.heading("PDF Fonts");

        egui::Grid::new("application_settings_fonts")
            .num_columns(1)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Regular Font");

                ui.end_row();
                ui.add_sized(
                    [ui.available_width().min(700.0), 20.0],
                    egui::TextEdit::singleline(&mut self.regular_font),
                );

                ui.end_row();
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

                ui.end_row();
                ui.add_sized(
                    [ui.available_width().min(700.0), 20.0],
                    egui::TextEdit::singleline(&mut self.bold_font),
                );

                ui.end_row();
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
    }

    fn show_storage(&mut self, ui: &mut egui::Ui, application: &mut Application) {
        ui.heading("Storage privacy");
        ui.label("Assess existing file permissions without reading document contents or changing access. Shared business folders may intentionally allow other people. Any repair requires a separate review; this application does not repair existing permissions automatically.");
        if let Some(receiver) = &self.storage_assessment_pending {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
            match receiver.try_recv() {
                Ok(messages) => {
                    self.storage_assessment = messages;
                    self.storage_assessment_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.storage_assessment =
                        vec!["Assessment stopped; retry when storage is available.".into()];
                    self.storage_assessment_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.label("Assessing storage in the background…");
                }
            }
        }
        if ui
            .add_enabled(
                self.storage_assessment_pending.is_none(),
                egui::Button::new("Assess Storage Permissions"),
            )
            .clicked()
        {
            let config = &application.context.config;
            let roots = vec![
                (application.context.environment.data_dir.clone(), false),
                (crate::paths::expand_path(&config.folders.csv_import), true),
                (crate::paths::expand_path(&config.folders.pdf_output), true),
                (
                    crate::paths::expand_path(&config.folders.email_archive),
                    true,
                ),
                (
                    crate::paths::expand_path(&config.folders.payslip_folder),
                    true,
                ),
                (
                    crate::paths::expand_path(&config.folders.payroll_information_folder),
                    true,
                ),
            ];
            let (sender, receiver) = std::sync::mpsc::channel();
            match std::thread::Builder::new()
                .name("storage-permissions-assessment".into())
                .spawn(move || {
                    let messages = roots
                        .into_iter()
                        .flat_map(|(root, shared)| crate::private_storage::assess(&root, shared))
                        .collect();
                    let _ = sender.send(messages);
                }) {
                Ok(_) => self.storage_assessment_pending = Some(receiver),
                Err(_) => {
                    self.storage_assessment =
                        vec!["Could not start assessment; no changes made.".into()]
                }
            }
        }
        if !self.storage_assessment.is_empty() {
            egui::ScrollArea::vertical()
                .id_salt("storage_assessment")
                .max_height(180.0)
                .show(ui, |ui| {
                    for message in &self.storage_assessment {
                        ui.label(message);
                    }
                });
        }
        ui.separator();
    }

    fn show_backup(&mut self, ui: &mut egui::Ui, application: &mut Application) {
        ui.heading("Backup");
        ui.label("Create a verified database/configuration recovery copy, including committed WAL data. External PDFs, payslips, signatures and business folders are not copied; protect those separately. The recovery manifest lists their known locations.");

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

        let restore_clicked = ui
            .horizontal_wrapped(|ui| {
                if ui.button("Refresh Backup List").clicked() {
                    self.refresh_backups(application);
                }
                let restore_enabled =
                    self.selected_backup.is_some() && self.restart_message.is_none();
                ui.add_enabled(
                    restore_enabled,
                    egui::Button::new("Restore Selected Backup..."),
                )
                .clicked()
            })
            .inner;
        if restore_clicked {
            let selected = self.selected_backup.as_ref().unwrap();
            match application.preview_backup_restore(selected) {
                Ok(validation) => {
                    self.status_message = format!(
                        "Backup validated successfully (schema version {}). Confirmation required.",
                        validation.schema_version
                    );
                    self.restore_plan = Some(validation);
                    self.restore_acknowledged = false;
                    self.restore_reason.clear();
                    self.confirm_restore = true;
                }
                Err(error) => {
                    self.status_message = format!("Restore refused: {error}");
                }
            }
        }

        self.backup_list(ui);

        if self.confirm_restore {
            let plan = self.restore_plan.clone().unwrap();
            let selected = plan.backup.clone();
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Confirm Backup Restoration")
                .collapsible(false)
                .default_width(600.0)
                .resizable(true)
                .show(ui.ctx(), |ui| {
                    ui.heading("Destructive rollback: current database and settings will be replaced");
                    ui.colored_label(ui.visuals().warn_fg_color,"Newer payroll submissions, accepted sends and other evidence may be removed from the active database. This can cause duplicate payroll processing. They remain in the separate verified recovery copy.");
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for (table,count) in &plan.replaced_rows {
                            ui.label(format!("{count} current row(s) absent or different in the restored data: {table}"));
                        }
                    });
                    ui.label("PDFs, payslips, signatures and other external business files are NOT restored. Protect/reconcile these separately. Close all other application instances before proceeding.");
                    ui.label("Document the reason and evidence for this rollback:");
                    ui.text_edit_multiline(&mut self.restore_reason);
                    ui.checkbox(&mut self.restore_acknowledged,"I explicitly authorise this reviewed rollback, including loss of the listed active evidence, and understand the duplicate-email risk.");
                    ui.label(format!("Restore from: {}", selected.display()));
                    ui.label(
                        "A safety backup of the current database and configuration will be created first.",
                    );
                    ui.label(
                        "After restoration, you must close and restart DirectPaymentTimesheets before doing any further work.",
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(self.restore_acknowledged && !self.restore_reason.trim().is_empty(), egui::Button::new("Restore Backup and Replace Current Data"))
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
                self.restore_plan = None;
                self.restore_acknowledged = false;
                self.status_message = "Backup restoration cancelled; no data was changed.".into();
            } else if confirm {
                self.confirm_restore = false;
                let approval = crate::backup_service::RestoreAuthorisation {
                    plan,
                    reason: self.restore_reason.clone(),
                    acknowledged: self.restore_acknowledged,
                };
                match application.restore_backup(&selected, &approval) {
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
    }

    fn backup_list(&mut self, ui: &mut egui::Ui) -> egui::scroll_area::ScrollAreaOutput<()> {
        // Leave room for the existing bottom separator and a small breathing gap.
        // The enclosing tab scroll area has unbounded content height; use its
        // visible clip edge so the backup list alone consumes the remaining space.
        let height = (ui.clip_rect().bottom() - ui.next_widget_position().y - 16.0).max(0.0);
        egui::ScrollArea::vertical()
            .id_salt("restore_backup_list")
            .max_height(height)
            .min_scrolled_height(0.0)
            .auto_shrink([false, false])
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
                        self.confirm_restore = false;
                        self.restore_plan = None;
                        self.restore_acknowledged = false;
                    }
                }
            })
    }

    fn form_state(&self) -> String { vec![format!("{:?}", self.dashboard_buttons),format!("{:?}", self.theme),format!("{:?}", self.date_display_format),format!("{:?}", self.hours_shift_date_time_spinner),format!("{:?}", self.csv_import),format!("{:?}", self.pdf_output),format!("{:?}", self.email_archive),format!("{:?}", self.payslip_folder),format!("{:?}", self.payroll_information_folder),format!("{:?}", self.regular_font),format!("{:?}", self.bold_font),format!("{:?}", self.week_commencing_font_size),format!("{:?}", self.hours_font_size),format!("{:?}", self.information_font_size)].join("\0") }
    fn load(&mut self, application: &Application) {
        let config = &application.context.config;

        self.theme = config.theme;
        self.dashboard_buttons = config.dashboard_buttons.normalised();
        self.date_display_format = config.date_display_format;
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

        self.saved_form = self.form_state();
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

    fn save(&mut self, application: &mut Application) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(all(windows, debug_assertions, feature = "windows-dev-isolation"))]
        crate::development::validate_folder_draft(&[
            std::path::PathBuf::from(self.csv_import.trim()),
            std::path::PathBuf::from(self.pdf_output.trim()),
            std::path::PathBuf::from(self.email_archive.trim()),
            std::path::PathBuf::from(self.payslip_folder.trim()),
            std::path::PathBuf::from(self.payroll_information_folder.trim()),
        ])?;
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
        application.context.config.dashboard_buttons = self.dashboard_buttons.normalised();
        application.context.config.date_display_format = self.date_display_format;
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
        self.saved_form = self.form_state();
        Ok(())
    }
}

#[cfg(test)] mod save_dirty_tests {
    use super::*;
    #[test] fn application_settings_save_colour_lifecycle() {
        use crate::gui_controls::acceptance::{frame,click};
        let (_dir,mut app)=crate::payroll_timesheet_screen::tests::test_application();let ctx=egui::Context::default();let mut screen=ApplicationSettingsScreen::new();
        assert!(!frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Application Settings").1);
        screen.hours_font_size="12".into();let(pos,blue)=frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Application Settings");assert!(blue);
        click(&ctx,pos,|ui|{screen.show(ui,&mut app);},"Save Application Settings");
        assert!(!frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Application Settings").1);
    }
    #[test] fn failed_save_retains_dirty_and_success_resets_baseline() {
        let (_dir,mut app)=crate::payroll_timesheet_screen::tests::test_application();
        let mut screen=ApplicationSettingsScreen::new();screen.load(&app);
        assert_eq!(screen.form_state(),screen.saved_form);
        screen.hours_font_size="invalid".into();assert!(screen.save(&mut app).is_err());
        assert_ne!(screen.form_state(),screen.saved_form);
        screen.hours_font_size="12".into();screen.save(&mut app).unwrap();
        assert_eq!(screen.form_state(),screen.saved_form);
    }
}

#[cfg(test)]
mod tab_tests {
    use super::*;
    use crate::payroll_settings_screen::tab_acceptance::{draw, select};

    #[test]
    fn application_tabs_navigate_keep_edits_and_save_across_tabs() {
        for width in [600.0, 1000.0] {
            let (_dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
            let ctx = egui::Context::default();
            let mut screen = ApplicationSettingsScreen::new();
            draw(&ctx, width, vec![], |ui| {
                screen.show(ui, &mut app);
            });
            let baseline = screen.form_state();
            for (tab, label) in ApplicationTab::ALL {
                select(&ctx, width, label, |ui| {
                    screen.show(ui, &mut app);
                });
                assert_eq!(screen.active_tab, tab);
                assert_eq!(screen.form_state(), baseline);
                let texts = draw(&ctx, width, vec![], |ui| {
                    screen.show(ui, &mut app);
                });
                assert!(texts.contains_key("Save Application Settings"));
                for (_, tab_label) in ApplicationTab::ALL {
                    let rect = texts.get(tab_label).expect("All tab labels remain visible");
                    assert!(rect.min.x >= 0.0 && rect.max.x <= width);
                }
                if tab == ApplicationTab::Folders {
                    assert!(texts.contains_key("CSV Import Folder"));
                    assert!(!texts.contains_key("Calendar date display"));
                }
            }
            screen.hours_shift_date_time_spinner = !screen.hours_shift_date_time_spinner;
            screen.pdf_output = app
                .context
                .environment
                .data_dir
                .join("synthetic-pdfs")
                .display()
                .to_string();
            screen.hours_font_size = "12".into();
            let edited = screen.form_state();
            for (_, label) in ApplicationTab::ALL {
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
                        "Save Application Settings"
                    )
                    .1,
                    "Dirty Save indicator must remain active on {label}"
                );
            }
            select(&ctx, width, "Storage & Security", |ui| {
                screen.show(ui, &mut app);
            });
            select(&ctx, width, "Save Application Settings", |ui| {
                screen.show(ui, &mut app);
            });
            assert_eq!(screen.status_message, "Application settings saved.");
            let mut reloaded = ApplicationSettingsScreen::new();
            reloaded.load(&app);
            assert_eq!(screen.form_state(), reloaded.form_state());
            assert_eq!(screen.saved_form, screen.form_state());
            assert!(
                !crate::gui_controls::acceptance::frame(
                    &ctx,
                    vec![],
                    |ui| {
                        screen.show(ui, &mut app);
                    },
                    "Save Application Settings"
                )
                .1,
                "Successful Save clears the dirty indicator"
            );
            let config = crate::config::AppConfig::load(
                &app.context.environment.data_dir.join("config.toml"),
            )
            .unwrap();
            assert_eq!(config.pdf.hours_font_size, 12.0);
            assert_eq!(
                config.folders.pdf_output.to_string_lossy(),
                screen.pdf_output
            );
        }
    }
}

#[cfg(test)]
mod backup_layout_tests {
    use super::*;
    use crate::payroll_settings_screen::tab_acceptance::{draw, select};

    fn entries(count: usize) -> Vec<BackupInfo> {
        (0..count)
            .map(|i| BackupInfo {
                path: std::path::PathBuf::from(format!("/synthetic-backups/20261108-1200{i:02}")),
                directory_name: format!("20261108-1200{i:02}"),
                created_at: "Synthetic backup".into(),
                has_config: true,
            })
            .collect()
    }
    #[test]
    fn list_fills_remaining_viewport_and_scrolls_only_for_overflow() {
        for width in [1000.0, 600.0] {
            let ctx = egui::Context::default();
            let mut screen = ApplicationSettingsScreen::new();
            for count in [0, 1, 3, 50, 1] {
                screen.backups = entries(count);
                let mut sizes = None;
                for _ in 0..4 {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 800.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let outer = egui::ScrollArea::vertical().id_salt("test_tab").show(
                                    ui,
                                    |ui| {
                                        ui.label("Toolbar remains above the backup list");
                                        let clip_bottom = ui.clip_rect().bottom();
                                        let list = screen.backup_list(ui);
                                        sizes = Some((
                                            list.content_size.y,
                                            list.inner_rect,
                                            clip_bottom,
                                        ));
                                    },
                                );
                                assert_eq!(
                                    outer.state.offset.y, 0.0,
                                    "Only backup entries should scroll"
                                );
                            });
                        },
                    );
                }
                let (content, rect, bottom) = sizes.unwrap();
                assert!(
                    rect.height() > 600.0,
                    "Fixed-height list regression: {rect:?}"
                );
                assert!(rect.bottom() <= bottom - 15.0, "Bottom spacing lost");
                assert_eq!(content>rect.height()+1.0,count==50,"Scrollbar must reflect actual overflow: count={count}, content={content}, rect={rect:?}");
            }
        }
    }
    #[test]
    fn toolbar_precedes_list_and_restore_requires_selection_and_keeps_confirmation() {
        for width in [1000.0, 600.0] {
            let (_dir, mut app) = crate::payroll_timesheet_screen::tests::test_application();
            app.save_config().unwrap();
            let backup = app.create_backup().unwrap();
            let mut screen = ApplicationSettingsScreen::new();
            screen.load(&app);
            screen.loaded = true;
            screen.active_tab = ApplicationTab::Backup;
            let ctx = egui::Context::default();
            let mut texts = draw(&ctx, width, vec![], |ui| {
                screen.show(ui, &mut app);
            });
            for _ in 0..3 {
                texts = draw(&ctx, width, vec![], |ui| {
                    screen.show(ui, &mut app);
                });
            }
            let refresh = texts.get("Refresh Backup List").unwrap();
            let restore = texts.get("Restore Selected Backup...").unwrap();
            assert!((refresh.center().y - restore.center().y).abs() < 1.0);
            assert!(restore.max.x <= width);
            let entry_label = texts
                .keys()
                .find(|t| t.contains(&backup.display().to_string()))
                .unwrap()
                .clone();
            assert!(texts[&entry_label].min.y > restore.max.y);
            let status = screen.status_message.clone();
            select(&ctx, width, "Restore Selected Backup...", |ui| {
                screen.show(ui, &mut app);
            });
            assert_eq!(screen.status_message, status);
            assert!(!screen.confirm_restore);
            select(&ctx, width, &entry_label, |ui| {
                screen.show(ui, &mut app);
            });
            assert_eq!(screen.selected_backup.as_ref(), Some(&backup));
            let fingerprint =
                crate::database_recovery::fingerprint(app.repository.connection()).unwrap();
            select(&ctx, width, "Restore Selected Backup...", |ui| {
                screen.show(ui, &mut app);
            });
            assert!(screen.confirm_restore);
            assert!(screen.restore_plan.is_some());
            assert!(!screen.restore_acknowledged);
            select(&ctx, width, "Cancel", |ui| {
                screen.show(ui, &mut app);
            });
            assert!(!screen.confirm_restore);
            assert!(screen.restore_plan.is_none());
            assert_eq!(
                crate::database_recovery::fingerprint(app.repository.connection()).unwrap(),
                fingerprint
            );
            screen.restart_message = Some("Synthetic restart required".into());
            select(&ctx, width, "Restore Selected Backup...", |ui| {
                screen.show(ui, &mut app);
            });
            assert!(!screen.confirm_restore);
        }
    }
}

#[cfg(test)]
mod dashboard_button_settings_tests {
    use super::*;
    #[test]
    fn dashboard_buttons_preview_dirty_save_failure_and_restart() {
        use crate::gui_controls::acceptance::{frame,click};
        let (_dir,mut app)=crate::payroll_timesheet_screen::tests::test_application();
        let ctx=egui::Context::default();let mut screen=ApplicationSettingsScreen::new();
        assert!(!frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Application Settings").1);
        screen.dashboard_buttons.height=48.0;
        screen.dashboard_buttons.font_size=18.0;
        let (_,dirty)=frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Application Settings");
        assert!(dirty);
        // A draft previews immediately, without prematurely changing saved settings.
        assert_eq!(app.context.config.dashboard_buttons,Default::default());
        screen.hours_font_size="invalid".into();
        assert!(screen.save(&mut app).is_err());
        assert_ne!(screen.form_state(),screen.saved_form);
        assert_eq!(app.context.config.dashboard_buttons,Default::default());
        screen.hours_font_size="12".into();
        let (pos,_)=frame(&ctx,vec![],|ui|{screen.show(ui,&mut app);},"Save Application Settings");
        click(&ctx,pos,|ui|{screen.show(ui,&mut app);},"Save Application Settings");
        assert_eq!(screen.status_message,"Application settings saved.");
        assert_eq!(screen.form_state(),screen.saved_form);
        let loaded=crate::config::AppConfig::load(&app.context.environment.data_dir.join("config.toml")).unwrap();
        assert_eq!(loaded.dashboard_buttons,screen.dashboard_buttons);
        app.context.config=loaded;
        let mut restarted=ApplicationSettingsScreen::new();restarted.load(&app);
        assert_eq!(restarted.dashboard_buttons,screen.dashboard_buttons);
        assert_eq!(restarted.form_state(),restarted.saved_form);
    }
}
