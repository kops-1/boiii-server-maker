//! Small building blocks shared by the pages: cards, switches, buttons.

use crate::theme::{ACCENT, CARD, LINE, MUTED};
use eframe::egui::{self, Color32, RichText, Rounding, Stroke};

/// A rounded panel with an optional title; every page is a column of these.
pub fn card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD)
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(RichText::new(title).size(17.0).strong());
                ui.add_space(2.0);
            }
            add(ui);
        });
    ui.add_space(12.0);
}

/// Secondary text: hints and explanations.
pub fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED).size(13.5));
}

/// A filled call-to-action button. Returns true when clicked.
pub fn big_button(ui: &mut egui::Ui, text: &str, fill: Color32, enabled: bool) -> bool {
    let button = egui::Button::new(RichText::new(text).color(Color32::WHITE).strong().size(15.5))
        .fill(fill)
        .min_size(egui::vec2(0.0, 38.0));
    ui.add_enabled(enabled, button).clicked()
}

/// An on/off switch.
pub fn switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = egui::vec2(42.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool(response.id, *on);
        let radius = rect.height() / 2.0;
        let fill = Color32::from_rgb(
            egui::lerp(LINE.r() as f32..=ACCENT.r() as f32, t) as u8,
            egui::lerp(LINE.g() as f32..=ACCENT.g() as f32, t) as u8,
            egui::lerp(LINE.b() as f32..=ACCENT.b() as f32, t) as u8,
        );
        ui.painter().rect(rect, Rounding::same(radius), fill, Stroke::NONE);
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        ui.painter().circle(egui::pos2(x, rect.center().y), radius - 3.0, Color32::WHITE, Stroke::NONE);
    }
    response
}

/// A switch with a title and a line of explanation.
pub fn switch_row(ui: &mut egui::Ui, on: &mut bool, title: &str, hint: &str) {
    ui.horizontal(|ui| {
        switch(ui, on);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new(title).size(15.0));
            if !hint.is_empty() {
                ui.label(RichText::new(hint).color(MUTED).size(13.0));
            }
        });
    });
}

/// The small caption above an input.
pub fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED).size(13.0));
}

/// A coloured dot in a box of `size`, used as a status light.
pub fn status_dot(ui: &mut egui::Ui, size: egui::Vec2, radius: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
}
