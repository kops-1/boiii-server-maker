//! Setup tab: the checklist, getting the server files, and folder choices.

use crate::actions::Action;
use crate::theme::*;
use crate::widgets::{big_button, card, muted, status_dot};
use crate::App;
use bsm_core::server::{Check, CheckState};
use eframe::egui::{self, RichText};

const REPAIR_HINT: &str = "Compares every game file with its source and replaces damaged ones. \
    Use it if the server stays at \"loading the map\".";

const SERVER_TOOL: &str =
    "The free Steam tool \"Call of Duty: Black Ops III - Unranked Dedicated Server\". Needed once.";

const STEAMCMD_HINT: &str = "Without the Steam client: steamcmd. Only your login name goes here; \
    the password is typed in steamcmd's own window.";

/// Colour and short label for a checklist entry.
fn check_badge(state: CheckState) -> (egui::Color32, &'static str) {
    match state {
        CheckState::Ok => (GOOD, "OK"),
        CheckState::Todo => (ACCENT, "Setup does it"),
        CheckState::Missing => (BAD, "Needs you"),
        CheckState::Info => (MUTED, "Info"),
    }
}

/// A path in a grid cell, cut off with "..." when it does not fit.
fn path_label(ui: &mut egui::Ui, text: String) {
    ui.add(egui::Label::new(RichText::new(text).monospace().size(13.0)).truncate());
}

impl App {
    pub(crate) fn setup_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        card(ui, "Checklist", |ui| self.checklist_card(ui, actions));
        card(ui, "Server files", |ui| self.server_files_card(ui, actions));
        card(ui, "Folders", |ui| self.folders_card(ui, actions));
    }

    fn checklist_card(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let idle = self.busy.is_none();
        if !self.discovered {
            muted(ui, "Checking this PC...");
        }
        egui::Grid::new("checks").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
            for check in &self.discovery.checks {
                checklist_row(ui, check);
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if big_button(ui, "Run setup", ACCENT, idle) {
                actions.push(Action::Setup);
            }
            if ui.add_enabled(idle, egui::Button::new("Check again")).clicked() {
                actions.push(Action::Refresh);
            }
            let repair = ui.add_enabled(idle && !self.running(), egui::Button::new("Repair files"));
            if repair.on_hover_text(REPAIR_HINT).clicked() {
                actions.push(Action::Repair);
            }
        });
    }

    fn server_files_card(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let idle = self.busy.is_none();
        muted(ui, SERVER_TOOL);
        ui.horizontal(|ui| {
            if ui.add_enabled(idle, egui::Button::new("Install through Steam")).clicked() {
                actions.push(Action::SteamInstall);
            }
            muted(ui, "Easiest. When Steam has finished, press Start server.");
        });

        ui.separator();
        muted(ui, STEAMCMD_HINT);
        ui.horizontal(|ui| {
            let login = egui::TextEdit::singleline(&mut self.settings.steam_user)
                .desired_width(220.0)
                .hint_text("Steam login name");
            ui.add(login);
            if ui.add_enabled(idle, egui::Button::new("Download with steamcmd")).clicked() {
                actions.push(Action::Steamcmd);
            }
        });
    }

    fn folders_card(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let idle = self.busy.is_none();
        let server_dir = &self.discovery.server_dir;
        egui::Grid::new("folders").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Server");
            path_label(ui, server_dir.display().to_string());
            ui.horizontal(|ui| {
                if ui.add_enabled(idle, egui::Button::new("Change")).clicked() {
                    actions.push(Action::PickServerDir);
                }
                if ui.add_enabled(server_dir.is_dir(), egui::Button::new("Open")).clicked() {
                    actions.push(Action::Open(server_dir.clone()));
                }
            });
            ui.end_row();

            ui.label("Black Ops III");
            let game_dir = match &self.discovery.game_dir {
                Some(dir) => dir.display().to_string(),
                None => "not found".into(),
            };
            path_label(ui, game_dir);
            if ui.add_enabled(idle, egui::Button::new("Change")).clicked() {
                actions.push(Action::PickGameDir);
            }
            ui.end_row();
        });

        let chosen_by_hand = self.settings.server_dir.is_some() || self.settings.game_dir.is_some();
        if chosen_by_hand && ui.add_enabled(idle, egui::Button::new("Automatic folders")).clicked() {
            actions.push(Action::AutoDirs);
        }
    }
}

fn checklist_row(ui: &mut egui::Ui, check: &Check) {
    let (color, label) = check_badge(check.state);
    ui.horizontal(|ui| {
        status_dot(ui, egui::vec2(12.0, 20.0), 5.0, color);
        ui.label(RichText::new(label).color(color).size(13.0));
    });
    ui.label(RichText::new(check.name).strong());
    ui.add(egui::Label::new(RichText::new(&check.detail).color(MUTED).size(13.5)).wrap());
    ui.end_row();
}
