//! Things drawn on top of the page: the question/notice dialog and the toast.

use crate::theme::{ACCENT, FIELD, HIGHLIGHT};
use crate::widgets::big_button;
use crate::App;
use eframe::egui::{self, Color32, RichText, Rounding};
use std::time::Duration;

impl App {
    pub(crate) fn draw_overlays(&mut self, ctx: &egui::Context) {
        let mut close = None;
        if let Some(d) = &self.dialog {
            // Dim the window behind the question.
            let screen = ctx.screen_rect();
            egui::Area::new(egui::Id::new("dim")).order(egui::Order::Middle).fixed_pos(screen.min).show(
                ctx,
                |ui| {
                    ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(150));
                    ui.allocate_rect(screen, egui::Sense::click());
                },
            );
            egui::Window::new(RichText::new(&d.title).size(18.0).strong())
                .collapsible(false)
                .resizable(false)
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(20.0)))
                .show(ctx, |ui| {
                    ui.set_max_width(520.0);
                    egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(&d.text).size(14.5)).wrap());
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if big_button(ui, d.yes, ACCENT, true) {
                            close = Some(true);
                        }
                        if let Some(no) = d.no {
                            if big_button(ui, no, FIELD, true) {
                                close = Some(false);
                            }
                        }
                    });
                });
        }
        if let Some(answer) = close {
            if let Some(d) = self.dialog.take() {
                if let Some(reply) = d.reply {
                    let _ = reply.send(answer);
                }
            }
        }

        if let Some((text, since)) = &self.toast {
            if since.elapsed() > Duration::from_secs(4) {
                self.toast = None;
            } else {
                egui::Area::new(egui::Id::new("toast"))
                    .order(egui::Order::Tooltip)
                    .anchor(egui::Align2::CENTER_BOTTOM, [95.0, -22.0])
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(HIGHLIGHT)
                            .rounding(Rounding::same(10.0))
                            .inner_margin(egui::Margin::symmetric(16.0, 10.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(text).color(Color32::WHITE));
                            });
                    });
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }
    }
}
