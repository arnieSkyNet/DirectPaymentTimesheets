use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike};
use eframe::egui;

use crate::app::Application;
use crate::direct_shift_repository::{DirectShift, DirectShiftError};
use crate::models::PersonalAssistant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerPurpose {
    ClockIn,
    ClockOut,
    EditStart,
    EditEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecentShiftEditField {
    Date,
    Start,
    End,
    Break,
    ActualWorked,
}

#[derive(Clone, Debug)]
struct CompletedShiftEdit {
    shift_id: i64,
    start: NaiveDateTime,
    end: NaiveDateTime,
    break_minutes: i64,
    notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DateTimePickerState {
    purpose: PickerPurpose,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
}

impl DateTimePickerState {
    fn new(purpose: PickerPurpose, initial: NaiveDateTime) -> Self {
        Self {
            purpose,
            year: initial.year(),
            month: initial.month(),
            day: initial.day(),
            hour: initial.hour(),
            minute: initial.minute(),
        }
    }

    fn value(&self) -> Result<NaiveDateTime, String> {
        NaiveDate::from_ymd_opt(self.year, self.month, self.day)
            .and_then(|date| date.and_hms_opt(self.hour, self.minute, 0))
            .ok_or_else(|| "Select a valid date and time.".to_string())
    }
}

pub struct EnterHoursScreen {
    loaded: bool,
    assistants: Vec<PersonalAssistant>,
    selected_pa_id: Option<i64>,
    running_shift: Option<DirectShift>,
    recent_shifts: Vec<DirectShift>,
    notes: String,
    break_minutes: i64,
    picker: Option<DateTimePickerState>,
    confirm_undo: bool,
    completed_edit: Option<CompletedShiftEdit>,
    confirm_delete_shift_id: Option<i64>,
    status_message: String,
}

impl EnterHoursScreen {
    pub fn new() -> Self {
        Self {
            loaded: false,
            assistants: Vec::new(),
            selected_pa_id: None,
            running_shift: None,
            recent_shifts: Vec::new(),
            notes: String::new(),
            break_minutes: 0,
            picker: None,
            confirm_undo: false,
            completed_edit: None,
            confirm_delete_shift_id: None,
            status_message: String::new(),
        }
    }

    pub fn reload(&mut self) {
        self.loaded = false;
        self.picker = None;
        self.confirm_undo = false;
        self.completed_edit = None;
        self.confirm_delete_shift_id = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, application: &Application) {
        if !self.loaded {
            if let Err(error) = self.load(application) {
                self.status_message = format!("Unable to load direct shifts: {error}");
            }
        }

        ui.heading("Enter Hours/Shifts");
        ui.label("Record actual shift evidence. Payroll values and rounding are applied later.");
        ui.add_space(8.0);

        if self.assistants.is_empty() {
            ui.label("No maintained Personal Assistants are available.");
            self.draw_status(ui);
            return;
        }

        let previous_pa = self.selected_pa_id;
        let selected_name = self
            .assistants
            .iter()
            .find(|assistant| Some(assistant.id) == self.selected_pa_id)
            .map(assistant_name)
            .unwrap_or_else(|| "Select a Personal Assistant".to_string());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Personal Assistant:")
                    .strong()
                    .size(18.0),
            );
            egui::ComboBox::from_id_salt("direct_shift_pa")
                .width(280.0)
                .selected_text(egui::RichText::new(selected_name).size(18.0))
                .show_ui(ui, |ui| {
                    for assistant in &self.assistants {
                        ui.selectable_value(
                            &mut self.selected_pa_id,
                            Some(assistant.id),
                            assistant_name(assistant),
                        );
                    }
                });
        });
        if previous_pa != self.selected_pa_id {
            self.picker = None;
            self.confirm_undo = false;
            self.completed_edit = None;
            self.confirm_delete_shift_id = None;
            if let Err(error) = self.load_selected_pa(application) {
                self.status_message = format!("Unable to load shifts: {error}");
            }
        }

        ui.separator();
        if let Some(running) = self.running_shift.clone() {
            self.draw_running_shift(ui, application, &running);
        } else {
            self.draw_clock_in_actions(ui, application);
        }

        if self.picker.as_ref().is_some_and(|picker| {
            matches!(
                picker.purpose,
                PickerPurpose::ClockIn | PickerPurpose::ClockOut
            )
        }) {
            self.draw_picker(ui, application);
        }

        ui.separator();
        self.draw_recent_shifts(ui, application);
        self.draw_status(ui);
    }

    fn load(&mut self, application: &Application) -> Result<(), Box<dyn std::error::Error>> {
        self.assistants = application.personal_assistant_repository.get_all()?;
        if !self
            .assistants
            .iter()
            .any(|assistant| Some(assistant.id) == self.selected_pa_id)
        {
            self.selected_pa_id = self.assistants.first().map(|assistant| assistant.id);
        }
        self.load_selected_pa(application)?;
        self.loaded = true;
        Ok(())
    }

    fn load_selected_pa(&mut self, application: &Application) -> Result<(), DirectShiftError> {
        if let Some(pa_id) = self.selected_pa_id {
            self.running_shift = application
                .direct_shift_repository
                .get_running_for_pa(pa_id)?;
            self.recent_shifts = application
                .direct_shift_repository
                .recent_for_pa(pa_id, 12)?;
        } else {
            self.running_shift = None;
            self.recent_shifts.clear();
        }
        self.notes = self
            .running_shift
            .as_ref()
            .and_then(|shift| shift.notes.clone())
            .unwrap_or_default();
        self.break_minutes = self
            .running_shift
            .as_ref()
            .map(|shift| shift.break_minutes)
            .unwrap_or(0);
        Ok(())
    }

    fn draw_clock_in_actions(&mut self, ui: &mut egui::Ui, application: &Application) {
        ui.label(egui::RichText::new("No shift is currently running.").size(20.0));
        ui.horizontal_wrapped(|ui| {
            if large_button(ui, "Clock in at…").clicked() {
                self.picker = Some(DateTimePickerState::new(
                    PickerPurpose::ClockIn,
                    local_now_to_minute(),
                ));
            }
            if large_button(ui, "Clock in now").clicked() {
                self.clock_in(application, local_now_to_minute());
            }
        });
    }

    fn draw_running_shift(
        &mut self,
        ui: &mut egui::Ui,
        application: &Application,
        running: &DirectShift,
    ) {
        let pa_name = self
            .assistants
            .iter()
            .find(|assistant| assistant.id == running.personal_assistant_id)
            .map(assistant_name)
            .unwrap_or_else(|| "Unknown Personal Assistant".to_string());
        ui.group(|ui| {
            ui.label(egui::RichText::new(format!("Shift running — {pa_name}")).heading());
            match running.start() {
                Ok(start) => {
                    ui.label(format!("Started: {}", format_uk_date_time(start)));
                    let elapsed = Local::now().naive_local().signed_duration_since(start);
                    ui.label(
                        egui::RichText::new(format!(
                            "Elapsed: {}",
                            format_elapsed(elapsed.num_seconds().max(0))
                        ))
                        .strong()
                        .size(24.0),
                    );
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs(1));
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, error.to_string());
                }
            }
        });

        ui.label("Notes (optional):");
        ui.add_sized(
            [500.0_f32.min(ui.available_width()), 70.0],
            egui::TextEdit::multiline(&mut self.notes),
        );
        ui.horizontal(|ui| {
            ui.label("Break minutes:");
            ui.add(
                egui::DragValue::new(&mut self.break_minutes)
                    .range(0..=24 * 60)
                    .speed(1),
            );
            if ui.button("Save Notes").clicked() {
                match application.direct_shift_repository.save_running_notes(
                    running.id,
                    Some(&self.notes),
                    &Local::now().to_rfc3339(),
                ) {
                    Ok(()) => self.status_message = "Notes saved.".to_string(),
                    Err(error) => self.status_message = format!("Notes save failed: {error}"),
                }
            }
        });

        ui.horizontal_wrapped(|ui| {
            if large_button(ui, "Clock out at…").clicked() {
                self.picker = Some(DateTimePickerState::new(
                    PickerPurpose::ClockOut,
                    local_now_to_minute(),
                ));
            }
            if large_button(ui, "Clock out now").clicked() {
                self.clock_out(application, running.id, local_now_to_minute());
            }
            if large_button(ui, "Undo Clock In").clicked() {
                self.confirm_undo = true;
            }
        });

        if self.confirm_undo {
            ui.group(|ui| {
                ui.label("Cancel this accidental running clock-in? This cannot be undone.");
                ui.horizontal(|ui| {
                    if ui.button("Yes, undo clock-in").clicked() {
                        match application
                            .direct_shift_repository
                            .undo_clock_in(running.id, &Local::now().to_rfc3339())
                        {
                            Ok(()) => {
                                self.confirm_undo = false;
                                self.status_message = "Accidental clock-in removed.".to_string();
                                let _ = self.load_selected_pa(application);
                            }
                            Err(error) => {
                                self.status_message = format!("Undo failed: {error}");
                            }
                        }
                    }
                    if ui.button("Keep running shift").clicked() {
                        self.confirm_undo = false;
                    }
                });
            });
        }
    }

    fn draw_picker(&mut self, ui: &mut egui::Ui, application: &Application) {
        let mut confirm = false;
        let mut cancel = false;
        let picker = self.picker.as_mut().expect("picker checked above");
        ui.group(|ui| {
            let title = match picker.purpose {
                PickerPurpose::ClockIn => "Choose exact clock-in time",
                PickerPurpose::ClockOut => "Choose exact clock-out time",
                PickerPurpose::EditStart => "Correct exact start time",
                PickerPurpose::EditEnd => "Correct exact end time",
            };
            ui.label(egui::RichText::new(title).heading());
            ui.label("UK date and 24-hour time. Every minute is selectable.");
            ui.horizontal_wrapped(|ui| {
                spinner(ui, &mut picker.day, 1..=31, "Day");
                spinner(ui, &mut picker.month, 1..=12, "Month");
                spinner(ui, &mut picker.year, 2000..=2200, "Year");
                spinner(ui, &mut picker.hour, 0..=23, "Hour");
                spinner(ui, &mut picker.minute, 0..=59, "Minute");
            });
            match picker.value() {
                Ok(value) => {
                    ui.label(
                        egui::RichText::new(format_uk_date_time(value))
                            .strong()
                            .size(20.0),
                    );
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            }
            ui.horizontal(|ui| {
                confirm = ui.button("Use this time").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });

        if cancel {
            self.picker = None;
        } else if confirm {
            let picker = self.picker.clone().expect("picker remains present");
            match picker.value() {
                Ok(value) => match picker.purpose {
                    PickerPurpose::ClockIn => self.clock_in(application, value),
                    PickerPurpose::ClockOut => {
                        if let Some(id) = self.running_shift.as_ref().map(|shift| shift.id) {
                            self.clock_out(application, id, value);
                        }
                    }
                    PickerPurpose::EditStart => {
                        if let Some(edit) = &mut self.completed_edit {
                            edit.start = value;
                            self.picker = None;
                        }
                    }
                    PickerPurpose::EditEnd => {
                        if let Some(edit) = &mut self.completed_edit {
                            edit.end = value;
                            self.picker = None;
                        }
                    }
                },
                Err(error) => self.status_message = error,
            }
        }
    }

    fn clock_in(&mut self, application: &Application, start: NaiveDateTime) {
        let Some(pa_id) = self.selected_pa_id else {
            self.status_message = "Select a Personal Assistant.".to_string();
            return;
        };
        match application
            .direct_shift_repository
            .clock_in(pa_id, start, &Local::now().to_rfc3339())
        {
            Ok(_) => {
                self.picker = None;
                self.status_message = "Shift clocked in.".to_string();
                if let Err(error) = self.load_selected_pa(application) {
                    self.status_message = format!("Shift saved, but refresh failed: {error}");
                }
            }
            Err(error) => self.status_message = format!("Clock-in failed: {error}"),
        }
    }

    fn clock_out(&mut self, application: &Application, shift_id: i64, end: NaiveDateTime) {
        match application.direct_shift_repository.complete(
            shift_id,
            end,
            self.break_minutes,
            Some(&self.notes),
            &Local::now().to_rfc3339(),
        ) {
            Ok(_) => {
                self.picker = None;
                self.status_message = "Shift clocked out and saved.".to_string();
                if let Err(error) = self.load_selected_pa(application) {
                    self.status_message = format!("Shift saved, but refresh failed: {error}");
                }
            }
            Err(error) => self.status_message = format!("Clock-out failed: {error}"),
        }
    }

    fn draw_recent_shifts(&mut self, ui: &mut egui::Ui, application: &Application) {
        ui.heading("Recent Shifts");
        let completed = self
            .recent_shifts
            .iter()
            .filter(|shift| shift.end_time.is_some())
            .cloned()
            .collect::<Vec<_>>();
        if completed.is_empty() {
            ui.label("No completed direct shifts for this Personal Assistant.");
            return;
        }
        let mut delete_shift = None;
        draw_recent_shift_header(ui);
        for (index, shift) in completed.into_iter().enumerate() {
            let editing = self
                .completed_edit
                .as_ref()
                .is_some_and(|edit| edit.shift_id == shift.id);
            let background = if index % 2 == 1 {
                ui.visuals().faint_bg_color
            } else {
                egui::Color32::TRANSPARENT
            };
            egui::Frame::NONE
                .fill(background)
                .inner_margin(egui::Margin::symmetric(4, 3))
                .show(ui, |ui| {
                    if editing {
                        self.draw_inline_completed_edit(ui, application);
                    } else {
                        let start = shift.start().ok();
                        let end = shift.end().ok().flatten();
                        let mut clicked_field = None;
                        ui.horizontal(|ui| {
                            if recent_value_button(
                                ui,
                                90.0,
                                start
                                    .map(|value| value.format("%d/%m/%Y").to_string())
                                    .unwrap_or_default(),
                            )
                            .clicked()
                            {
                                clicked_field = Some(RecentShiftEditField::Date);
                            }
                            if recent_value_button(
                                ui,
                                62.0,
                                start
                                    .map(|value| value.format("%H:%M").to_string())
                                    .unwrap_or_default(),
                            )
                            .clicked()
                            {
                                clicked_field = Some(RecentShiftEditField::Start);
                            }
                            if recent_value_button(
                                ui,
                                112.0,
                                end.map(|value| value.format("%H:%M").to_string())
                                    .unwrap_or_default(),
                            )
                            .clicked()
                            {
                                clicked_field = Some(RecentShiftEditField::End);
                            }
                            if recent_value_button(ui, 68.0, format!("{} min", shift.break_minutes))
                                .clicked()
                            {
                                clicked_field = Some(RecentShiftEditField::Break);
                            }
                            if recent_value_button(
                                ui,
                                104.0,
                                shift
                                    .worked_minutes()
                                    .ok()
                                    .flatten()
                                    .map(format_minutes)
                                    .unwrap_or_else(|| "Invalid".to_string()),
                            )
                            .clicked()
                            {
                                clicked_field = Some(RecentShiftEditField::ActualWorked);
                            }
                            if ui
                                .add_sized(
                                    [62.0, 30.0],
                                    egui::Button::new(
                                        egui::RichText::new("Delete")
                                            .color(ui.visuals().error_fg_color),
                                    ),
                                )
                                .clicked()
                            {
                                delete_shift = Some(shift.id);
                            }
                        });
                        if let Some(field) = clicked_field {
                            if let Err(error) = self.enter_completed_edit_from_field(&shift, field)
                            {
                                self.status_message = error;
                            }
                        }
                    }
                });
        }
        if let Some(id) = delete_shift {
            self.confirm_delete_shift_id = Some(id);
            self.completed_edit = None;
            self.picker = None;
        }

        if let Some(id) = self.confirm_delete_shift_id {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Delete this completed shift?").strong());
                ui.label("It will disappear from normal use, but its evidence and audit history will be retained.");
                ui.horizontal(|ui| {
                    if ui.button("Yes, delete shift").clicked() {
                        match application.direct_shift_repository.soft_delete_completed(
                            id,
                            &Local::now().to_rfc3339(),
                        ) {
                            Ok(()) => {
                                self.confirm_delete_shift_id = None;
                                self.status_message = "Shift deleted from normal use; evidence retained in audit history.".to_string();
                                let _ = self.load_selected_pa(application);
                            }
                            Err(error) => self.status_message = format!("Delete failed: {error}"),
                        }
                    }
                    if ui.button("Keep shift").clicked() {
                        self.confirm_delete_shift_id = None;
                    }
                });
            });
        }
    }

    fn draw_inline_completed_edit(&mut self, ui: &mut egui::Ui, application: &Application) {
        let (shift_id, date, start, end) = {
            let edit = self
                .completed_edit
                .as_ref()
                .expect("inline editor checked above");
            (
                edit.shift_id,
                edit.start.format("%d/%m/%Y").to_string(),
                edit.start.format("%H:%M").to_string(),
                edit.end.format("%d/%m %H:%M").to_string(),
            )
        };
        let mut choose_start = false;
        let mut choose_end = false;
        ui.horizontal(|ui| {
            choose_start |= ui
                .add_sized([90.0, 34.0], egui::Button::new(date))
                .clicked();
            choose_start |= ui
                .add_sized([62.0, 34.0], egui::Button::new(start))
                .clicked();
            choose_end |= ui
                .add_sized([112.0, 34.0], egui::Button::new(end))
                .clicked();
            ui.add_sized(
                [68.0, 34.0],
                egui::DragValue::new(
                    &mut self
                        .completed_edit
                        .as_mut()
                        .expect("editor remains")
                        .break_minutes,
                )
                .range(0..=24 * 60)
                .speed(1)
                .suffix(" min"),
            );
            let worked = completed_edit_worked_minutes(
                self.completed_edit.as_ref().expect("editor remains"),
            )
            .map(format_minutes)
            .unwrap_or_else(|_| "Invalid".to_string());
            recent_cell(ui, 104.0, worked);
            recent_cell(ui, 54.0, "Editing");
        });

        if choose_start {
            let value = self.completed_edit.as_ref().expect("editor remains").start;
            self.picker = Some(DateTimePickerState::new(PickerPurpose::EditStart, value));
        } else if choose_end {
            let value = self.completed_edit.as_ref().expect("editor remains").end;
            self.picker = Some(DateTimePickerState::new(PickerPurpose::EditEnd, value));
        }

        ui.indent(("completed_shift_inline_edit", shift_id), |ui| {
            if inline_editor_places_notes_beside(ui.available_width()) {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| self.draw_inline_picker_or_hint(ui));
                    ui.add_space(10.0);
                    ui.vertical(|ui| self.draw_inline_notes(ui, 300.0));
                });
            } else {
                self.draw_inline_picker_or_hint(ui);
                let notes_width = ui.available_width();
                self.draw_inline_notes(ui, notes_width);
            }
            ui.label("Saving preserves the previous values in immutable audit history.");
            let mut save = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                save = ui
                    .add_sized([130.0, 36.0], egui::Button::new("Save changes"))
                    .clicked();
                cancel = ui
                    .add_sized([90.0, 36.0], egui::Button::new("Cancel"))
                    .clicked();
            });
            if cancel {
                self.cancel_completed_edit();
            } else if save {
                self.save_completed_edit(application);
            }
        });
    }

    fn draw_inline_picker_or_hint(&mut self, ui: &mut egui::Ui) {
        if self.picker.as_ref().is_some_and(|picker| {
            matches!(
                picker.purpose,
                PickerPurpose::EditStart | PickerPurpose::EditEnd
            )
        }) {
            self.draw_inline_edit_picker(ui);
        } else {
            ui.set_min_width(380.0_f32.min(ui.available_width()));
            ui.label(
                "Select Date, Start or End above to choose an exact UK date and 24-hour time.",
            );
        }
    }

    fn draw_inline_notes(&mut self, ui: &mut egui::Ui, preferred_width: f32) {
        ui.label("Notes (optional):");
        ui.add_sized(
            [preferred_width.min(ui.available_width()), 74.0],
            egui::TextEdit::multiline(
                &mut self.completed_edit.as_mut().expect("editor remains").notes,
            ),
        );
    }

    fn draw_inline_edit_picker(&mut self, ui: &mut egui::Ui) {
        let mut proposed = None;
        let picker = self.picker.as_mut().expect("inline picker checked above");
        ui.group(|ui| {
            let title = match picker.purpose {
                PickerPurpose::EditStart => "Correct exact start time",
                PickerPurpose::EditEnd => "Correct exact end time",
                _ => "Correct exact time",
            };
            ui.label(egui::RichText::new(title).heading());
            ui.label("UK date and 24-hour time. Every change updates the proposed shift.");
            ui.horizontal_wrapped(|ui| {
                spinner(ui, &mut picker.day, 1..=31, "Day");
                spinner(ui, &mut picker.month, 1..=12, "Month");
                spinner(ui, &mut picker.year, 2000..=2200, "Year");
                spinner(ui, &mut picker.hour, 0..=23, "Hour");
                spinner(ui, &mut picker.minute, 0..=59, "Minute");
            });
            match picker.value() {
                Ok(value) => {
                    proposed = Some((picker.purpose, value));
                    ui.label(
                        egui::RichText::new(format_uk_date_time(value))
                            .strong()
                            .size(20.0),
                    );
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            }
        });

        if let Some((purpose, value)) = proposed {
            if let Some(edit) = &mut self.completed_edit {
                apply_picker_value_to_completed_edit(edit, purpose, value);
            }
        }
    }

    fn enter_completed_edit_from_field(
        &mut self,
        shift: &DirectShift,
        field: RecentShiftEditField,
    ) -> Result<(), String> {
        self.begin_completed_edit(shift)?;
        let edit = self.completed_edit.as_ref().expect("editor just created");
        self.picker = match field {
            RecentShiftEditField::Date | RecentShiftEditField::Start => Some(
                DateTimePickerState::new(PickerPurpose::EditStart, edit.start),
            ),
            RecentShiftEditField::End => {
                Some(DateTimePickerState::new(PickerPurpose::EditEnd, edit.end))
            }
            RecentShiftEditField::Break | RecentShiftEditField::ActualWorked => None,
        };
        Ok(())
    }

    fn begin_completed_edit(&mut self, shift: &DirectShift) -> Result<(), String> {
        match (shift.start(), shift.end()) {
            (Ok(start), Ok(Some(end))) => {
                self.completed_edit = Some(CompletedShiftEdit {
                    shift_id: shift.id,
                    start,
                    end,
                    break_minutes: shift.break_minutes,
                    notes: shift.notes.clone().unwrap_or_default(),
                });
                self.picker = None;
                self.confirm_delete_shift_id = None;
                Ok(())
            }
            _ => Err(
                "The completed shift contains invalid stored dates and cannot be edited."
                    .to_string(),
            ),
        }
    }

    fn cancel_completed_edit(&mut self) {
        self.completed_edit = None;
        self.picker = None;
    }

    fn save_completed_edit(&mut self, application: &Application) {
        let edit = self.completed_edit.clone().expect("editor remains");
        match application.direct_shift_repository.edit_completed(
            edit.shift_id,
            edit.start,
            edit.end,
            edit.break_minutes,
            Some(&edit.notes),
            &Local::now().to_rfc3339(),
        ) {
            Ok(_) => {
                self.cancel_completed_edit();
                self.status_message =
                    "Completed shift corrected; previous values retained in audit history."
                        .to_string();
                if let Err(error) = self.load_selected_pa(application) {
                    self.status_message = format!("Shift saved, but refresh failed: {error}");
                }
            }
            Err(error) => self.status_message = format!("Shift correction failed: {error}"),
        }
    }

    fn draw_status(&self, ui: &mut egui::Ui) {
        if !self.status_message.is_empty() {
            ui.add_space(6.0);
            ui.label(format!("Status: {}", self.status_message));
        }
    }
}

fn assistant_name(assistant: &PersonalAssistant) -> String {
    format!("{} {}", assistant.first_name, assistant.surname)
}

fn local_now_to_minute() -> NaiveDateTime {
    let now = Local::now().naive_local();
    now.with_second(0)
        .and_then(|value| value.with_nanosecond(0))
        .expect("valid local minute")
}

fn format_uk_date_time(value: NaiveDateTime) -> String {
    value.format("%d/%m/%Y %H:%M").to_string()
}

fn format_elapsed(total_seconds: i64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn format_minutes(minutes: i64) -> String {
    format!("{}h {:02}m", minutes / 60, minutes % 60)
}

fn completed_edit_worked_minutes(edit: &CompletedShiftEdit) -> Result<i64, &'static str> {
    if edit.end < edit.start || edit.break_minutes < 0 {
        return Err("invalid completed shift");
    }
    let worked = edit
        .end
        .signed_duration_since(edit.start)
        .num_minutes()
        .checked_sub(edit.break_minutes)
        .ok_or("invalid completed shift")?;
    if worked < 0 {
        return Err("invalid completed shift");
    }
    Ok(worked)
}

fn apply_picker_value_to_completed_edit(
    edit: &mut CompletedShiftEdit,
    purpose: PickerPurpose,
    value: NaiveDateTime,
) {
    match purpose {
        PickerPurpose::EditStart => edit.start = value,
        PickerPurpose::EditEnd => edit.end = value,
        PickerPurpose::ClockIn | PickerPurpose::ClockOut => {}
    }
}

fn draw_recent_shift_header(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        recent_cell(ui, 90.0, "Date");
        recent_cell(ui, 62.0, "Start");
        recent_cell(ui, 112.0, "End");
        recent_cell(ui, 68.0, "Break");
        recent_cell(ui, 104.0, "Actual worked");
    });
}

fn recent_value_button(
    ui: &mut egui::Ui,
    width: f32,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.add_sized([width, 30.0], egui::Button::new(text))
}

fn recent_cell(ui: &mut egui::Ui, width: f32, text: impl Into<egui::WidgetText>) {
    ui.add_sized([width, 30.0], egui::Label::new(text));
}

fn inline_editor_places_notes_beside(available_width: f32) -> bool {
    available_width >= 740.0
}

fn large_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [190.0_f32.min(ui.available_width()), 48.0],
        egui::Button::new(egui::RichText::new(label).size(18.0)),
    )
}

fn spinner<T>(ui: &mut egui::Ui, value: &mut T, range: std::ops::RangeInclusive<T>, label: &str)
where
    T: egui::emath::Numeric,
{
    ui.vertical(|ui| {
        ui.label(label);
        ui.add_sized(
            [72.0, 38.0],
            egui::DragValue::new(value).range(range).speed(1),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_shift(id: i64, start: &str, end: &str, break_minutes: i64) -> DirectShift {
        DirectShift {
            id,
            personal_assistant_id: 1,
            start_time: start.to_string(),
            end_time: Some(end.to_string()),
            break_minutes,
            notes: Some("original note".to_string()),
            source_type: "direct".to_string(),
            created_at: "created".to_string(),
            updated_at: "updated".to_string(),
            deleted_at: None,
            deleted_by: None,
        }
    }

    #[test]
    fn picker_supports_every_individual_minute_and_uk_display() {
        let mut picker = DateTimePickerState::new(
            PickerPurpose::ClockIn,
            NaiveDate::from_ymd_opt(2026, 9, 1)
                .unwrap()
                .and_hms_opt(8, 0, 0)
                .unwrap(),
        );
        for minute in 0..=59 {
            picker.minute = minute;
            assert_eq!(picker.value().unwrap().minute(), minute);
        }
        picker.minute = 7;
        assert_eq!(
            format_uk_date_time(picker.value().unwrap()),
            "01/09/2026 08:07"
        );
    }

    #[test]
    fn picker_rejects_invalid_calendar_dates() {
        let picker = DateTimePickerState {
            purpose: PickerPurpose::ClockOut,
            year: 2026,
            month: 2,
            day: 30,
            hour: 9,
            minute: 0,
        };
        assert!(picker.value().is_err());
    }

    #[test]
    fn inline_edit_mode_targets_only_the_selected_completed_row() {
        let first = completed_shift(1, "2026-09-01T08:00", "2026-09-01T10:00", 0);
        let second = completed_shift(2, "2026-09-02T09:00", "2026-09-02T11:00", 0);
        let mut screen = EnterHoursScreen::new();

        screen.begin_completed_edit(&first).unwrap();
        assert_eq!(screen.completed_edit.as_ref().unwrap().shift_id, 1);
        screen.begin_completed_edit(&second).unwrap();
        assert_eq!(screen.completed_edit.as_ref().unwrap().shift_id, 2);
    }

    #[test]
    fn every_displayed_row_value_enters_inline_edit_mode() {
        let shift = completed_shift(3, "2026-09-01T08:00", "2026-09-01T10:00", 5);
        let cases = [
            (RecentShiftEditField::Date, Some(PickerPurpose::EditStart)),
            (RecentShiftEditField::Start, Some(PickerPurpose::EditStart)),
            (RecentShiftEditField::End, Some(PickerPurpose::EditEnd)),
            (RecentShiftEditField::Break, None),
            (RecentShiftEditField::ActualWorked, None),
        ];

        for (field, expected_picker) in cases {
            let mut screen = EnterHoursScreen::new();
            screen
                .enter_completed_edit_from_field(&shift, field)
                .unwrap();
            assert_eq!(screen.completed_edit.as_ref().unwrap().shift_id, shift.id);
            assert_eq!(
                screen.picker.as_ref().map(|picker| picker.purpose),
                expected_picker
            );
        }
    }

    #[test]
    fn inline_editor_uses_side_by_side_notes_only_when_width_allows() {
        assert!(!inline_editor_places_notes_beside(739.9));
        assert!(inline_editor_places_notes_beside(740.0));
        assert!(inline_editor_places_notes_beside(1200.0));
    }

    #[test]
    fn cancelling_inline_edit_discards_proposed_values_without_touching_source_row() {
        let shift = completed_shift(7, "2026-09-01T08:00", "2026-09-01T10:00", 10);
        let original = shift.clone();
        let mut screen = EnterHoursScreen::new();
        screen.begin_completed_edit(&shift).unwrap();
        let edit = screen.completed_edit.as_mut().unwrap();
        edit.start = NaiveDate::from_ymd_opt(2026, 9, 3)
            .unwrap()
            .and_hms_opt(12, 17, 0)
            .unwrap();
        edit.notes = "unsaved".to_string();

        screen.cancel_completed_edit();

        assert!(screen.completed_edit.is_none());
        assert_eq!(shift, original);
    }

    #[test]
    fn inline_actual_worked_preview_uses_proposed_exact_times_and_break() {
        let edit = CompletedShiftEdit {
            shift_id: 1,
            start: NaiveDate::from_ymd_opt(2026, 9, 1)
                .unwrap()
                .and_hms_opt(23, 30, 0)
                .unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 9, 2)
                .unwrap()
                .and_hms_opt(2, 15, 0)
                .unwrap(),
            break_minutes: 20,
            notes: String::new(),
        };
        assert_eq!(completed_edit_worked_minutes(&edit), Ok(145));

        let mut invalid = edit;
        invalid.break_minutes = 166;
        assert!(completed_edit_worked_minutes(&invalid).is_err());
    }

    #[test]
    fn inline_picker_updates_proposed_value_and_preview_without_mutating_source() {
        let shift = completed_shift(9, "2026-09-01T08:00", "2026-09-01T10:00", 15);
        let original = shift.clone();
        let mut screen = EnterHoursScreen::new();
        screen.begin_completed_edit(&shift).unwrap();
        let proposed_end = NaiveDate::from_ymd_opt(2026, 9, 2)
            .unwrap()
            .and_hms_opt(0, 7, 0)
            .unwrap();

        apply_picker_value_to_completed_edit(
            screen.completed_edit.as_mut().unwrap(),
            PickerPurpose::EditEnd,
            proposed_end,
        );

        let edit = screen.completed_edit.as_ref().unwrap();
        assert_eq!(edit.end, proposed_end);
        assert_eq!(completed_edit_worked_minutes(edit), Ok(952));
        assert_eq!(shift, original);
    }

    #[test]
    fn save_payload_is_the_currently_displayed_proposed_edit() {
        let shift = completed_shift(10, "2026-09-01T08:00", "2026-09-01T10:00", 0);
        let mut screen = EnterHoursScreen::new();
        screen.begin_completed_edit(&shift).unwrap();
        let proposed_start = NaiveDate::from_ymd_opt(2026, 8, 31)
            .unwrap()
            .and_hms_opt(23, 59, 0)
            .unwrap();
        let proposed_end = NaiveDate::from_ymd_opt(2026, 9, 1)
            .unwrap()
            .and_hms_opt(2, 26, 0)
            .unwrap();
        let edit = screen.completed_edit.as_mut().unwrap();
        apply_picker_value_to_completed_edit(edit, PickerPurpose::EditStart, proposed_start);
        apply_picker_value_to_completed_edit(edit, PickerPurpose::EditEnd, proposed_end);
        edit.break_minutes = 17;
        edit.notes = "displayed proposal".to_string();

        let save_payload = screen.completed_edit.clone().unwrap();
        assert_eq!(save_payload.start, proposed_start);
        assert_eq!(save_payload.end, proposed_end);
        assert_eq!(save_payload.break_minutes, 17);
        assert_eq!(save_payload.notes, "displayed proposal");
        assert_eq!(completed_edit_worked_minutes(&save_payload), Ok(130));
    }
}
