use eframe::egui::style::WidgetVisuals;
use eframe::egui::{self, Color32, Stroke, Theme, ThemePreference, Visuals};

use crate::config::ApplicationTheme;

pub fn theme_label(theme: ApplicationTheme) -> &'static str {
    match theme {
        ApplicationTheme::System => "System",
        ApplicationTheme::Light => "Light",
        ApplicationTheme::SoftLight => "Soft Light",
        ApplicationTheme::Dark => "Dark",
        ApplicationTheme::SoftDark => "Soft Dark",
        ApplicationTheme::Blue => "Blue",
        ApplicationTheme::AccessibleHighContrast => "Accessible High Contrast",
    }
}

pub fn apply_theme(ctx: &egui::Context, theme: ApplicationTheme) {
    // Restore egui's standard styles first so custom palettes cannot leak into
    // System, Light, or Dark when switching themes at runtime.
    ctx.set_style_of(Theme::Light, Theme::Light.default_style());
    ctx.set_style_of(Theme::Dark, Theme::Dark.default_style());

    match theme {
        ApplicationTheme::System => ctx.set_theme(ThemePreference::System),
        ApplicationTheme::Light => ctx.set_theme(ThemePreference::Light),
        ApplicationTheme::Dark => ctx.set_theme(ThemePreference::Dark),
        ApplicationTheme::SoftLight => apply_custom_theme(ctx, Theme::Light, soft_light_visuals()),
        ApplicationTheme::SoftDark => apply_custom_theme(ctx, Theme::Dark, soft_dark_visuals()),
        ApplicationTheme::Blue => apply_custom_theme(ctx, Theme::Dark, blue_visuals()),
        ApplicationTheme::AccessibleHighContrast => {
            apply_custom_theme(ctx, Theme::Dark, accessible_high_contrast_visuals());
        }
    }
}

fn apply_custom_theme(ctx: &egui::Context, base_theme: Theme, visuals: Visuals) {
    let mut style = base_theme.default_style();
    style.visuals = visuals;
    ctx.set_style_of(base_theme, style);
    ctx.set_theme(base_theme);
}

fn soft_light_visuals() -> Visuals {
    let mut visuals = Visuals::light();
    visuals.panel_fill = Color32::from_rgb(239, 237, 231);
    visuals.window_fill = Color32::from_rgb(246, 244, 238);
    visuals.extreme_bg_color = Color32::from_rgb(252, 251, 247);
    visuals.text_edit_bg_color = Some(Color32::from_rgb(250, 249, 245));
    visuals.faint_bg_color = Color32::from_rgb(229, 227, 221);
    visuals.code_bg_color = Color32::from_rgb(224, 227, 224);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(239, 237, 231);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(224, 224, 218);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(211, 218, 216);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(196, 209, 207);
    visuals
}

fn soft_dark_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(37, 39, 43);
    visuals.window_fill = Color32::from_rgb(43, 45, 50);
    visuals.extreme_bg_color = Color32::from_rgb(28, 30, 34);
    visuals.text_edit_bg_color = Some(Color32::from_rgb(31, 33, 37));
    visuals.faint_bg_color = Color32::from_rgb(47, 50, 55);
    visuals.code_bg_color = Color32::from_rgb(31, 35, 40);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(43, 45, 50);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(54, 57, 63);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(82, 86, 94));
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(67, 73, 81);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(76, 86, 96);
    visuals
}

fn blue_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(30, 39, 49);
    visuals.window_fill = Color32::from_rgb(35, 45, 56);
    visuals.extreme_bg_color = Color32::from_rgb(23, 31, 40);
    visuals.text_edit_bg_color = Some(Color32::from_rgb(25, 34, 44));
    visuals.faint_bg_color = Color32::from_rgb(39, 51, 64);
    visuals.code_bg_color = Color32::from_rgb(25, 37, 49);
    visuals.hyperlink_color = Color32::from_rgb(126, 178, 218);
    visuals.selection.bg_fill = Color32::from_rgb(50, 91, 124);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(35, 45, 56);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(45, 59, 73);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(70, 91, 109));
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(58, 78, 96);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(66, 91, 113);
    visuals
}

fn accessible_high_contrast_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    let white = Color32::WHITE;
    let black = Color32::BLACK;
    let boundary = Color32::from_rgb(230, 230, 230);
    let yellow = Color32::from_rgb(255, 224, 0);
    let cyan = Color32::from_rgb(0, 220, 255);

    visuals.panel_fill = black;
    visuals.window_fill = Color32::from_rgb(12, 12, 12);
    visuals.window_stroke = Stroke::new(2.0_f32, white);
    visuals.extreme_bg_color = black;
    visuals.text_edit_bg_color = Some(black);
    visuals.faint_bg_color = Color32::from_rgb(38, 38, 38);
    visuals.code_bg_color = Color32::from_rgb(25, 25, 25);
    visuals.weak_text_color = Some(Color32::from_rgb(210, 210, 210));
    visuals.hyperlink_color = cyan;
    visuals.selection.bg_fill = yellow;
    visuals.selection.stroke = Stroke::new(2.0_f32, black);
    visuals.warn_fg_color = yellow;
    visuals.error_fg_color = Color32::from_rgb(255, 100, 100);
    visuals.disabled_alpha = 0.75;
    visuals.button_frame = true;
    visuals.collapsing_header_frame = true;
    visuals.indent_has_left_vline = true;
    visuals.widgets.noninteractive = widget_visuals(black, black, boundary, white, 1.5, 0.0);
    visuals.widgets.inactive = widget_visuals(
        Color32::from_rgb(20, 20, 20),
        Color32::from_rgb(20, 20, 20),
        white,
        white,
        2.0,
        0.0,
    );
    visuals.widgets.hovered = widget_visuals(yellow, yellow, white, black, 2.0, 2.0);
    visuals.widgets.active = widget_visuals(cyan, cyan, white, black, 2.5, 2.0);
    visuals.widgets.open = widget_visuals(yellow, yellow, white, black, 2.0, 1.0);
    visuals
}

fn widget_visuals(
    bg_fill: Color32,
    weak_bg_fill: Color32,
    boundary: Color32,
    foreground: Color32,
    stroke_width: f32,
    expansion: f32,
) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill,
        weak_bg_fill,
        bg_stroke: Stroke::new(stroke_width, boundary),
        fg_stroke: Stroke::new(stroke_width, foreground),
        expansion,
        ..Visuals::dark().widgets.inactive
    }
}
