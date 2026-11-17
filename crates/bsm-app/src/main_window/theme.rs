//! Colours, fonts and spacing of the main window.

use eframe::egui::{self, Color32, Rounding, Stroke};
use std::path::PathBuf;

// Surfaces, from the darkest (sidebar) to the lightest (input fields).
pub const BG: Color32 = Color32::from_rgb(18, 22, 30);
pub const SIDE: Color32 = Color32::from_rgb(13, 16, 23);
pub const CARD: Color32 = Color32::from_rgb(27, 33, 44);
pub const FIELD: Color32 = Color32::from_rgb(38, 46, 61);
pub const LINE: Color32 = Color32::from_rgb(58, 68, 88);

// Text.
pub const INK: Color32 = Color32::from_rgb(232, 236, 242);
pub const MUTED: Color32 = Color32::from_rgb(150, 160, 176);

// Meaning.
pub const ACCENT: Color32 = Color32::from_rgb(58, 134, 255);
pub const GOOD: Color32 = Color32::from_rgb(52, 199, 123);
pub const GOOD_BUTTON: Color32 = Color32::from_rgb(30, 150, 88);
pub const WARN: Color32 = Color32::from_rgb(240, 180, 60);
pub const BAD: Color32 = Color32::from_rgb(235, 87, 87);
/// Background of the "restart to apply" banner.
pub const WARN_BANNER: Color32 = Color32::from_rgb(52, 44, 22);
/// Selected sidebar entry, and the toast.
pub const HIGHLIGHT: Color32 = Color32::from_rgb(34, 54, 92);

// The Favorites star and the medals of the top three games.
pub const GOLD: Color32 = Color32::from_rgb(240, 196, 60);
pub const SILVER: Color32 = Color32::from_rgb(190, 198, 210);
pub const BRONZE: Color32 = Color32::from_rgb(205, 133, 80);

/// Reads one of Windows' own fonts; nothing is bundled with the program.
fn system_font(name: &str) -> Option<Vec<u8>> {
    let dir = std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join("Fonts"))?;
    std::fs::read(dir.join(name)).ok()
}

/// Sets the dark look of the whole window: fonts, colours, spacing.
pub fn apply_style(ctx: &egui::Context) {
    // Windows' own fonts, read from the PC; nothing is bundled.
    let mut fonts = egui::FontDefinitions::default();
    if let Some(bytes) = system_font("segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), egui::FontData::from_owned(bytes));
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    if let Some(bytes) = system_font("consola.ttf") {
        fonts.font_data.insert("consolas".into(), egui::FontData::from_owned(bytes));
        fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "consolas".into());
    }
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0_f32, LINE);
    v.window_rounding = Rounding::same(12.0);
    v.extreme_bg_color = FIELD;
    v.faint_bg_color = CARD;
    v.override_text_color = Some(INK);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgb(40, 84, 160);
    v.selection.stroke = Stroke::new(1.0_f32, INK);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = Rounding::same(7.0);
        w.fg_stroke = Stroke::new(1.0_f32, INK);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.inactive.bg_fill = FIELD;
    v.widgets.inactive.weak_bg_fill = FIELD;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = Color32::from_rgb(50, 60, 80);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(50, 60, 80);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.active.bg_fill = Color32::from_rgb(58, 70, 94);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(58, 70, 94);
    v.widgets.open.bg_fill = FIELD;
    v.widgets.open.weak_bg_fill = FIELD;
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        use egui::{FontFamily, FontId, TextStyle};
        s.text_styles = [
            (TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace)),
            (TextStyle::Small, FontId::new(12.5, FontFamily::Proportional)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(10.0, 9.0);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.combo_width = 260.0;
    });
}
