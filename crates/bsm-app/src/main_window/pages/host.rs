//! Host tab: map and mod, server details, rotation, saved games and presets.

use crate::actions::Action;
use crate::background::Message;
use crate::theme::*;
use crate::widgets::{card, field_label, muted, switch_row};
use crate::{thumbs, App};
use bsm_core::content::{Item, Kind};
use bsm_core::saves::Save;
use bsm_core::{paths, saves};
use eframe::egui::{self, RichText, Rounding, Stroke};
use std::path::PathBuf;

const WORKSHOP_HINT: &str = "More maps and mods: subscribe in the Steam Workshop, let Steam finish, then \
    press \"Check again\" on the Setup page. Friends need the same map or mod; the game offers the \
    download when they join.";

const ROTATION_HINT: &str = "Play several maps one after another: when a game ends, the server loads \
    the next map, and starts again at the top after the last. Leave it empty to stay on one map.";

const SEVERAL_WORKSHOP_MAPS: &str = "Several workshop maps: joining players are only offered the download \
    of the map being played. They need the others already.";

const TOOL_ROTATES_HINT: &str = "This rotation has a workshop map, so this program changes the map: when \
    a game ends it restarts the server on the next one. Keep this window open. Players are dropped at \
    the change and rejoin with the same invite.";

const SAVES_HINT: &str = "Save from the mod menu in the game (Save / Load, slots 1-3), or let it save at \
    the start of every round. A save keeps the round, the power, and each player's points, weapons with \
    ammo, and perks. Doors, buildables and easter-egg progress are not saved.";

const PICTURE_SIZE: egui::Vec2 = egui::vec2(288.0, 162.0);

/// How a row of the rotation list should move.
enum RotationEdit {
    Up(usize),
    Down(usize),
    Remove(usize),
}

impl App {
    pub(crate) fn host_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, actions: &mut Vec<Action>) {
        card(ui, "Map", |ui| self.map_card(ui, ctx));
        card(ui, "Server", |ui| self.server_card(ui));
        card(ui, "Map rotation", |ui| self.rotation_card(ui));
        card(ui, "Saved games", |ui| self.saves_card(ui, actions));
        card(ui, "Presets", |ui| self.presets_card(ui, actions));
    }

    fn map_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let maps = self.discovery.maps.clone();
        let mods = self.discovery.mods.clone();
        let current = maps.iter().find(|m| m.name == self.settings.map).cloned();

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(430.0);

                field_label(ui, "Map");
                let rotating = !self.settings.rotation.is_empty();
                if rotating {
                    muted(ui, "A rotation is set below. The server starts on its first map.");
                }
                let shown = match &current {
                    Some(map) => map.title.clone(),
                    None => format!("{} (not installed)", self.settings.map),
                };
                ui.add_enabled_ui(!rotating, |ui| {
                    egui::ComboBox::from_id_salt("map")
                        .width(410.0)
                        .height(360.0)
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for map in &maps {
                                let label = match map.kind {
                                    Kind::WorkshopMap => format!("{}   (workshop)", map.title),
                                    _ => map.title.clone(),
                                };
                                ui.selectable_value(&mut self.settings.map, map.name.clone(), label);
                            }
                        });
                });
                if maps.is_empty() && self.discovered {
                    let text = "No Zombies maps found. Is Black Ops III with Zombies installed?";
                    ui.label(RichText::new(text).color(WARN));
                }

                ui.add_space(4.0);
                field_label(ui, "Mod");
                let shown = if self.settings.mod_id.is_empty() {
                    "No mod".to_string()
                } else {
                    match mods.iter().find(|m| m.id == self.settings.mod_id) {
                        Some(m) => m.title.clone(),
                        None => format!("{} (not installed)", self.settings.mod_id),
                    }
                };
                egui::ComboBox::from_id_salt("mod").width(410.0).height(360.0).selected_text(shown).show_ui(
                    ui,
                    |ui| {
                        ui.selectable_value(&mut self.settings.mod_id, String::new(), "No mod");
                        for m in &mods {
                            ui.selectable_value(&mut self.settings.mod_id, m.id.clone(), m.title.clone());
                        }
                    },
                );
                ui.add_space(2.0);
                muted(ui, WORKSHOP_HINT);
            });

            ui.add_space(8.0);
            if let Some(map) = &current {
                self.map_picture(ui, ctx, map);
            }
        });
    }

    /// The map's picture, or its name on a plain tile while there is none.
    fn map_picture(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, map: &Item) {
        let rounding = Rounding::same(10.0);
        match self.picture(map, ctx) {
            Some(path) => {
                let uri = format!("file://{}", path.display().to_string().replace('\\', "/"));
                ui.add(egui::Image::new(uri).fit_to_exact_size(PICTURE_SIZE).rounding(rounding));
            }
            None => {
                let (rect, _) = ui.allocate_exact_size(PICTURE_SIZE, egui::Sense::hover());
                let painter = ui.painter();
                painter.rect(rect, rounding, FIELD, Stroke::NONE);
                let font = egui::FontId::proportional(18.0);
                painter.text(rect.center(), egui::Align2::CENTER_CENTER, &map.title, font, MUTED);
            }
        }
    }

    /// Picture for a map; starts making one for an official map the first time.
    fn picture(&mut self, item: &Item, ctx: &egui::Context) -> Option<PathBuf> {
        if let Some(known) = self.pictures.get(&item.name) {
            return known.clone();
        }
        let data = paths::data_dir();
        let existing = thumbs::existing(item, &data);
        if existing.is_none() && item.kind == Kind::OfficialMap {
            if let Some(game) = self.discovery.game_dir.clone() {
                let (tx, name, ctx) = (self.tx.clone(), item.name.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let made = thumbs::make_official(&name, &game, &data);
                    let _ = tx.send(Message::Picture(name, made));
                    ctx.request_repaint();
                });
            }
        }
        self.pictures.insert(item.name.clone(), existing.clone());
        existing
    }

    fn server_card(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("server").num_columns(2).spacing([14.0, 10.0]).show(ui, |ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut self.settings.server_name).desired_width(360.0));
            ui.end_row();

            ui.label("Password");
            let password = egui::TextEdit::singleline(&mut self.settings.password)
                .desired_width(220.0)
                .hint_text("none: anyone can join");
            ui.add(password);
            ui.end_row();

            ui.label("Players");
            ui.add(egui::Slider::new(&mut self.settings.max_players, 1..=8));
            ui.end_row();

            ui.label("UDP port");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.settings.port).range(1024..=65535).speed(1));
                muted(ui, "Change it only if 27017 is taken. The game on this PC uses the next free port.");
            });
            ui.end_row();
        });
    }

    fn rotation_card(&mut self, ui: &mut egui::Ui) {
        muted(ui, ROTATION_HINT);
        let maps = self.discovery.maps.clone();
        let title_of = |name: &str| match maps.iter().find(|m| m.name == name) {
            Some(m) => m.title.clone(),
            None => format!("{name} (not installed)"),
        };

        let mut edit = None;
        let count = self.settings.rotation.len();
        for (i, name) in self.settings.rotation.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).color(MUTED));
                ui.add_sized([300.0, 24.0], egui::Label::new(title_of(name)).truncate());
                if ui.add_enabled(i > 0, egui::Button::new("Up")).clicked() {
                    edit = Some(RotationEdit::Up(i));
                }
                if ui.add_enabled(i + 1 < count, egui::Button::new("Down")).clicked() {
                    edit = Some(RotationEdit::Down(i));
                }
                if ui.button("Remove").clicked() {
                    edit = Some(RotationEdit::Remove(i));
                }
            });
        }
        let rotation = &mut self.settings.rotation;
        match edit {
            Some(RotationEdit::Up(i)) => rotation.swap(i, i - 1),
            Some(RotationEdit::Down(i)) => rotation.swap(i, i + 1),
            Some(RotationEdit::Remove(i)) => {
                rotation.remove(i);
            }
            None => {}
        }
        // The server always starts on the first map of the rotation.
        if let Some(first) = self.settings.rotation.first() {
            self.settings.map = first.clone();
        }

        ui.horizontal(|ui| {
            if self.rotation_pick.is_empty() {
                self.rotation_pick = self.settings.map.clone();
            }
            egui::ComboBox::from_id_salt("rotation_add")
                .width(300.0)
                .height(360.0)
                .selected_text(title_of(&self.rotation_pick))
                .show_ui(ui, |ui| {
                    for m in &maps {
                        ui.selectable_value(&mut self.rotation_pick, m.name.clone(), m.title.clone());
                    }
                });
            if ui.button("Add to rotation").clicked() && !self.rotation_pick.is_empty() {
                // The first map added joins the map already chosen above.
                if self.settings.rotation.is_empty() && self.rotation_pick != self.settings.map {
                    self.settings.rotation.push(self.settings.map.clone());
                }
                self.settings.rotation.push(self.rotation_pick.clone());
            }
            if !self.settings.rotation.is_empty() && ui.button("Clear").clicked() {
                self.settings.rotation.clear();
            }
        });

        let is_workshop = |name: &String| maps.iter().any(|m| &m.name == name && m.kind == Kind::WorkshopMap);
        let workshop_maps = self.settings.rotation.iter().filter(|n| is_workshop(n)).count();
        if workshop_maps > 1 {
            ui.label(RichText::new(SEVERAL_WORKSHOP_MAPS).color(WARN).size(13.5));
        }
        if self.settings.rotation.len() > 1 && workshop_maps > 0 {
            muted(ui, TOOL_ROTATES_HINT);
        }
    }

    fn saves_card(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        muted(ui, SAVES_HINT);
        switch_row(
            ui,
            &mut self.settings.autosave,
            "Autosave every round",
            "Saves to the slot \"auto\". Applies at the next server start.",
        );
        if !self.settings.continue_save.is_empty() {
            ui.horizontal(|ui| {
                let text = format!("Next start continues from slot {}.", self.settings.continue_save);
                ui.label(RichText::new(text).color(GOOD));
                if ui.small_button("New game instead").clicked() {
                    self.settings.continue_save.clear();
                }
            });
        }
        if self.saves.is_empty() {
            muted(ui, "No saved games yet.");
        }
        for save in self.saves.clone() {
            self.save_row(ui, &save, actions);
            ui.add_space(4.0);
        }
    }

    fn save_row(&self, ui: &mut egui::Ui, save: &Save, actions: &mut Vec<Action>) {
        let map_title = self.discovery.map_title(&save.map);
        egui::Frame::none()
            .fill(FIELD)
            .rounding(Rounding::same(10.0))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let slot = match save.slot.as_str() {
                        "auto" => "Autosave".to_string(),
                        number => format!("Slot {number}"),
                    };
                    ui.label(RichText::new(slot).strong().color(ACCENT));
                    ui.label(RichText::new(format!("Round {}", save.round)).size(17.0).strong());
                    ui.label(map_title);
                    if save.power {
                        muted(ui, "power on");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Delete").clicked() {
                            actions.push(Action::DeleteSave(save.slot.clone()));
                        }
                        let chosen = self.settings.continue_save == save.slot;
                        let label = if chosen { "Chosen" } else { "Continue" };
                        if ui.add_enabled(!chosen, egui::Button::new(label)).clicked() {
                            actions.push(Action::ContinueSave(save.slot.clone()));
                        }
                        muted(ui, &saves::age_text(save.when));
                    });
                });
                for p in &save.players {
                    let weapons = p.weapons.len();
                    let line = format!(
                        "{}   {} points   {weapons} weapon(s)   {} perk(s)",
                        p.name, p.points, p.perks
                    );
                    muted(ui, &line);
                }
            });
    }

    fn presets_card(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        muted(ui, "A preset remembers the map, the mod and the cheat switches.");
        ui.horizontal(|ui| {
            let names: Vec<String> = self.settings.presets.keys().cloned().collect();
            let shown = if self.preset_pick.is_empty() { "Choose a preset" } else { &self.preset_pick };
            egui::ComboBox::from_id_salt("preset").width(240.0).selected_text(shown.to_string()).show_ui(
                ui,
                |ui| {
                    for name in &names {
                        ui.selectable_value(&mut self.preset_pick, name.clone(), name);
                    }
                },
            );
            let picked = self.settings.presets.contains_key(&self.preset_pick);
            if ui.add_enabled(picked, egui::Button::new("Load")).clicked() {
                actions.push(Action::LoadPreset(self.preset_pick.clone()));
            }
            if ui.add_enabled(picked, egui::Button::new("Delete")).clicked() {
                actions.push(Action::DeletePreset(self.preset_pick.clone()));
            }
        });
        ui.horizontal(|ui| {
            let name = egui::TextEdit::singleline(&mut self.preset_name)
                .desired_width(240.0)
                .hint_text("Name for the current choices");
            ui.add(name);
            if ui.button("Save as preset").clicked() {
                actions.push(Action::SavePreset);
            }
        });
    }
}
