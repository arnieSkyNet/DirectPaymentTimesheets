use eframe::egui;

pub(super) struct MileageEditor {
    pub record_id: i64,
    pub week_id: i64,
    title: String,
    text: String,
    saved: f64,
    focus: bool,
    confirm_close: bool,
    pub error: String,
}

pub(super) enum Action {
    Save(f64),
    Close,
}

impl MileageEditor {
    pub fn new(record_id: i64, week_id: i64, name: &str, week: &str, value: f64) -> Self {
        Self {
            record_id,
            week_id,
            title: format!("Travel Miles — {name} — w/c {week}"),
            text: if value == 0.0 {
                String::new()
            } else {
                value.to_string()
            },
            saved: value,
            focus: true,
            confirm_close: false,
            error: String::new(),
        }
    }
    fn value(&self) -> Result<f64, &'static str> {
        if self.text.trim().is_empty() {
            return Ok(0.0);
        }
        self.text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or("Enter a finite, non-negative number of miles")
    }
    fn dirty(&self) -> bool {
        self.value().map_or(true, |v| v != self.saved)
    }
    fn request_close(&mut self) -> bool {
        self.confirm_close = self.dirty();
        !self.confirm_close
    }
    pub fn saved(&mut self, value: f64) {
        self.saved = value;
        self.text = if value == 0.0 {
            String::new()
        } else {
            value.to_string()
        };
        self.error.clear();
    }
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        let mut action = None;
        egui::Modal::new(egui::Id::new("weekly_mileage_editor")).show(ctx, |ui| {
            ui.heading(&self.title);
            if self.confirm_close {
                ui.label("Discard unsaved mileage changes?");
                ui.horizontal(|ui| {
                    if ui.button("Discard changes").clicked() {
                        action = Some(Action::Close);
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_close = false;
                    }
                });
                return;
            }
            ui.label("Miles for this week");
            let response = ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(180.0));
            if self.focus {
                response.request_focus();
                self.focus = false;
            }
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    match self.value() {
                        Ok(value) => action = Some(Action::Save(value)),
                        Err(error) => self.error = error.into(),
                    }
                }
                if ui.button("Clear").clicked() {
                    action = Some(Action::Save(0.0));
                }
                if ui.button("Close").clicked() && self.request_close() {
                    action = Some(Action::Close);
                }
            });
            if !self.error.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &self.error);
            }
        });
        action
    }
}

pub(super) fn cell(ui: &mut egui::Ui, enabled: bool, populated: bool) -> egui::Response {
    if !enabled {
        return ui.add(
            egui::Label::new("")
                .selectable(false)
                .sense(egui::Sense::hover()),
        );
    }
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(65.0, ui.spacing().interact_size.y),
        egui::Sense::click(),
    );
    let hover = response.hovered() && ui.is_enabled();
    if hover {
        ui.painter()
            .rect_filled(rect, 3.0, ui.visuals().widgets.hovered.bg_fill);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if populated {
            "•"
        } else if hover {
            "+"
        } else {
            ""
        },
        egui::FontId::proportional(14.0),
        ui.visuals().weak_text_color(),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Edit weekly travel miles")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blank_and_saved_values_close_cleanly_but_edits_require_confirmation() {
        let mut editor = MileageEditor::new(1, 2, "PA", "12/10/2026", 0.0);
        assert!(editor.text.is_empty() && editor.focus && editor.request_close());
        editor.text = "12.5".into();
        assert_eq!(editor.value(), Ok(12.5));
        assert!(!editor.request_close());
        editor.confirm_close = false;
        assert_eq!(editor.text, "12.5");
        editor.saved(12.5);
        assert!(editor.request_close());
        editor.text.clear();
        assert!(!editor.request_close());
        editor.saved(0.0);
        assert!(editor.request_close());
        for invalid in ["abc", "-1", "NaN", "inf"] {
            editor.text = invalid.into();
            assert!(editor.value().is_err());
            assert!(!editor.request_close());
        }
    }
    #[test]
    fn disabled_cells_have_no_click_sense_and_enabled_cells_do() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!cell(ui, false, true).sense.senses_click());
                assert!(cell(ui, true, false).sense.senses_click());
            });
        });
    }
}
