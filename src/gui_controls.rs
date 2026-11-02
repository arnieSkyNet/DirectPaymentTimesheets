//! Shared ComboBox interaction: scrollbar gestures keep the popup open,
//! outside clicks dismiss it, and selecting an item closes it (even if unchanged).
pub fn combo_box(id_salt: impl std::hash::Hash) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id_salt)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}

pub fn combo_value<T: PartialEq>(
    ui: &mut egui::Ui,
    current: &mut T,
    selected: T,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let response = ui.selectable_value(current, selected, text);
    if response.clicked() {
        ui.close();
    }
    response
}

/// One consistent treatment for enabled, unsaved Save actions in every theme.
pub fn save_button(ui: &mut egui::Ui, label: impl Into<String>, dirty: bool) -> egui::Response {
    save_button_enabled(ui, label, dirty, true)
}
pub fn save_button_enabled(
    ui: &mut egui::Ui,
    label: impl Into<String>,
    dirty: bool,
    enabled: bool,
) -> egui::Response {
    let label = label.into();
    let button = if dirty && enabled && ui.is_enabled() {
        egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE))
            .fill(egui::Color32::from_rgb(0, 85, 170))
    } else {
        egui::Button::new(label)
    };
    ui.add_enabled(enabled, button)
}

#[cfg(test)]
mod save_tests {
    use super::*;
    fn blue(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Rect(r) => r.fill == egui::Color32::from_rgb(0, 85, 170),
            egui::Shape::Vec(v) => v.iter().any(blue),
            _ => false,
        }
    }
    #[test]
    fn save_highlight_respects_every_theme_and_disabled_parent() {
        use crate::config::ApplicationTheme::*;
        for theme in [
            Dark,
            Light,
            System,
            SoftLight,
            SoftDark,
            Blue,
            AccessibleHighContrast,
        ] {
            let ctx = egui::Context::default();
            crate::theme::apply_theme(&ctx, theme);
            for (dirty, enabled, parent, expected) in [
                (false, true, true, false),
                (true, true, true, true),
                (true, false, true, false),
                (true, true, false, false),
                (false, true, true, false),
            ] {
                let out = ctx.run(Default::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.add_enabled_ui(parent, |ui| {
                            save_button_enabled(ui, "Save", dirty, enabled);
                        });
                    });
                });
                assert_eq!(
                    out.shapes.iter().any(|s| blue(&s.shape)),
                    expected,
                    "theme {theme:?}"
                );
            }
        }
    }
    #[test]
    fn save_highlight_tracks_dirty_enabled_and_successful_reset() {
        let ctx = egui::Context::default();
        for (dirty, enabled, expected) in [
            (false, true, false),
            (true, true, true),
            (true, false, false),
            (false, true, false),
        ] {
            let out = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    save_button_enabled(ui, "Save", dirty, enabled);
                });
            });
            assert_eq!(out.shapes.iter().any(|s| blue(&s.shape)), expected);
        }
    }
}

#[cfg(test)]
pub(crate) mod acceptance {
    pub fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        mut draw: impl FnMut(&mut egui::Ui),
        label: &str,
    ) -> (egui::Pos2, bool) {
        fn collect(
            s: &egui::Shape,
            label: &str,
            text: &mut Option<egui::Pos2>,
            blue: &mut Vec<egui::Rect>,
        ) {
            match s {
                egui::Shape::Text(t) if t.galley.job.text == label => {
                    *text = Some(t.pos + t.galley.size() / 2.0)
                }
                egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgb(0, 85, 170) => {
                    blue.push(r.rect)
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        collect(s, label, text, blue)
                    }
                }
                _ => {}
            }
        }
        // Headless assertions inspect settled paint, not modal fade animation.
        ctx.style_mut(|style| style.animation_time = 0.0);
        let mut events = events;
        for _ in 0..3 {
            let out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 4000.0),
                    )),
                    events: std::mem::take(&mut events),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| draw(ui));
                },
            );
            let mut pos = None;
            let mut blue = Vec::new();
            for s in out.shapes {
                collect(&s.shape, label, &mut pos, &mut blue);
            }
            if let Some(pos) = pos {
                return (pos, blue.iter().any(|r| r.contains(pos)));
            }
        }
        panic!("Missing Save control after settling frames: {label}");
    }
    pub fn click(
        ctx: &egui::Context,
        pos: egui::Pos2,
        mut draw: impl FnMut(&mut egui::Ui),
        label: &str,
    ) {
        for pressed in [true, false] {
            frame(
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                &mut draw,
                label,
            );
        }
    }
}
