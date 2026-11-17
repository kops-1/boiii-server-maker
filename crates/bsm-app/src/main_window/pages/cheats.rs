//! Cheats tab: the in-game mod menu and switches that apply to every player.

use crate::actions::Action;
use crate::theme::*;
use crate::widgets::{card, muted, switch_row};
use crate::App;
use bsm_core::net;
use eframe::egui::{self, RichText, Rounding};

const MENU_PLAYERS_HINT: &str =
    "Player names separated by commas. Leave empty and every player on the server can open the menu.";

const MENU_CONTROLS: &str =
    "FIRE = down     AIM = up     USE = select     MELEE = back     AIM + MELEE = close";
const MENU_KEYS: &str = "D-pad or keys 1 2 = up / down     4 = select     3 = back";

const ARROW_KEYS_HINT: &str =
    "To use the arrow keys: paste it once into the game console (~). Each player does this once.";

const MENU_FEATURES: &str = "\
Player: god mode, infinite ammo, auto revive, invisible, run speed, fly, teleport, save and return to a position.
Weapons: give any weapon of the map, Pack-a-Punch one or all, max ammo, no spread.
Zombies: kill all, insta-kill, freeze, speed, bring to crosshair, stop spawning.
Also: points, perks, power-ups, rounds, power and doors, low gravity, and actions on everyone or one player.";

const ALL_OFF: &str =
    "Everything is off: the server runs a normal game and no extra script is placed in the server folder.";

/// Quick picks next to the starting points field.
const START_POINT_PRESETS: [(&str, u32); 4] =
    [("Normal", 0), ("5,000", 5_000), ("30,000", 30_000), ("100,000", 100_000)];

impl App {
    pub(crate) fn cheats_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if self.running() {
            self.restart_banner(ui, actions);
        }
        card(ui, "In-game mod menu", |ui| self.mod_menu_card(ui, actions));
        card(ui, "For every player, from the start", |ui| self.everyone_card(ui));
        if !self.settings.any_script_option() {
            muted(ui, ALL_OFF);
        }
    }

    /// Cheats are read when the server starts, so a running server needs a restart.
    fn restart_banner(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Frame::none()
            .fill(WARN_BANNER)
            .rounding(Rounding::same(10.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let text = "The server is running. Changes here apply after a restart.";
                    ui.label(RichText::new(text).color(WARN));
                    let restart = egui::Button::new("Restart server");
                    if ui.add_enabled(self.busy.is_none(), restart).clicked() {
                        actions.push(Action::Restart);
                    }
                });
            });
        ui.add_space(12.0);
    }

    fn mod_menu_card(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let settings = &mut self.settings;
        switch_row(ui, &mut settings.menu, "Mod menu", "Open it in the game: hold AIM and press MELEE.");
        if !settings.menu {
            return;
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Only for");
            let players = egui::TextEdit::singleline(&mut settings.menu_players)
                .desired_width(280.0)
                .hint_text("everyone");
            ui.add(players);
        });
        muted(ui, MENU_PLAYERS_HINT);
        ui.add_space(6.0);

        egui::Frame::none()
            .fill(FIELD)
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(MENU_CONTROLS).monospace().size(13.0));
                ui.label(RichText::new(MENU_KEYS).monospace().size(13.0));
                ui.horizontal(|ui| {
                    if ui.button("Copy arrow-key command").clicked() {
                        let command = net::ARROW_KEYS_COMMAND.to_string();
                        actions.push(Action::Copy(command, "Arrow-key command"));
                    }
                    muted(ui, ARROW_KEYS_HINT);
                });
                ui.add_space(4.0);
                muted(ui, MENU_FEATURES);
            });
    }

    fn everyone_card(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.settings;
        muted(ui, "These work without the menu.");
        ui.add_space(2.0);
        ui.columns(2, |columns| {
            let [left, right] = columns else { return };
            switch_row(left, &mut s.god_all, "God mode", "Nobody takes damage.");
            switch_row(left, &mut s.ammo_all, "Infinite ammo", "Weapons and grenades never run out.");
            switch_row(left, &mut s.start_perks, "All perks", "Everyone starts with every perk of the map.");
            switch_row(right, &mut s.instakill_all, "Insta-kill", "Zombies die in one hit, all game.");
            switch_row(right, &mut s.double_points, "Double points", "All game.");
            switch_row(right, &mut s.keep_perks, "Keep perks", "Perks stay when a player goes down.");
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Starting points");
            ui.add(egui::DragValue::new(&mut s.start_points).range(0..=10_000_000).speed(500));
            for (label, value) in START_POINT_PRESETS {
                if ui.selectable_label(s.start_points == value, label).clicked() {
                    s.start_points = value;
                }
            }
        });
    }
}
