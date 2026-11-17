//! Log tab: everything the jobs wrote, with copy and open-folder buttons.

use crate::actions::Action;
use crate::widgets::card;
use crate::App;
use bsm_core::paths;
use eframe::egui;

impl App {
    pub(crate) fn log_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        card(ui, "Log", |ui| {
            ui.horizontal(|ui| {
                if ui.button("Copy all").clicked() {
                    actions.push(Action::Copy(self.log.join("\r\n"), "Log"));
                }
                if ui.button("Open data folder").clicked() {
                    actions.push(Action::Open(paths::data_dir()));
                }
            });
            let text = self.log.join("\n");
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(ui.available_height() - 20.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut text.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .frame(false),
                    );
                });
        });
    }
}
