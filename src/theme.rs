//! Visual theme for CatchYT — a calm dark palette with a violet accent.

use egui::{Color32, Rounding, Stroke, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(139, 92, 246); // violet-500
pub const ACCENT_DIM: Color32 = Color32::from_rgb(109, 72, 200);
pub const BG: Color32 = Color32::from_rgb(17, 18, 24);
pub const PANEL: Color32 = Color32::from_rgb(24, 26, 34);
pub const CARD: Color32 = Color32::from_rgb(31, 34, 44);
pub const TEXT: Color32 = Color32::from_rgb(226, 228, 235);
pub const MUTED: Color32 = Color32::from_rgb(148, 152, 165);
pub const OK: Color32 = Color32::from_rgb(52, 199, 123);
pub const WARN: Color32 = Color32::from_rgb(240, 173, 78);
pub const ERR: Color32 = Color32::from_rgb(240, 96, 96);

/// Apply the CatchYT look to an egui context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(13, 14, 19);
    visuals.faint_bg_color = CARD;

    let rounding = Rounding::same(10.0);
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.rounding = rounding;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);

    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.weak_bg_fill = CARD;
    visuals.widgets.inactive.rounding = rounding;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 46, 60);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(42, 46, 60);
    visuals.widgets.hovered.rounding = rounding;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_DIM);

    visuals.widgets.active.bg_fill = ACCENT_DIM;
    visuals.widgets.active.weak_bg_fill = ACCENT_DIM;
    visuals.widgets.active.rounding = rounding;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);

    visuals.selection.bg_fill = ACCENT.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.hyperlink_color = ACCENT;

    ctx.set_visuals(visuals);

    // A touch more breathing room than the egui defaults.
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    ctx.set_style(style);
}
