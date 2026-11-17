//! Scores tab: the best finished games.

use crate::theme::*;
use crate::widgets::{card, muted, switch_row};
use crate::App;
use bsm_core::scores::{self, Game};
use eframe::egui::{self, Color32, RichText, Rounding};

/// The scoreboard shows at most this many games.
const SHOWN_GAMES: usize = 50;

const RECORD_HINT: &str = "When a game ends, the round reached and each player's kills and points are \
    saved. Applies at the next server start.";

const NO_GAMES_YET: &str = "No finished games yet. A game is recorded when it ends (everyone is down), \
    while this program or the server keeps running.";

fn medal(rank: usize) -> Color32 {
    match rank {
        1 => GOLD,
        2 => SILVER,
        3 => BRONZE,
        _ => MUTED,
    }
}

impl App {
    pub(crate) fn scores_page(&mut self, ui: &mut egui::Ui) {
        card(ui, "Best games", |ui| {
            switch_row(ui, &mut self.settings.scoreboard, "Record finished games", RECORD_HINT);
            ui.add_space(4.0);
            if self.scores.is_empty() {
                muted(ui, NO_GAMES_YET);
                return;
            }
            for (index, game) in self.scores.iter().take(SHOWN_GAMES).enumerate() {
                self.game_row(ui, index + 1, game);
                ui.add_space(6.0);
            }
        });
    }

    fn game_row(&self, ui: &mut egui::Ui, rank: usize, game: &Game) {
        let map_title = self.discovery.map_title(&game.map);
        egui::Frame::none()
            .fill(FIELD)
            .rounding(Rounding::same(10.0))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("#{rank}")).size(16.0).strong().color(medal(rank)));
                    ui.label(RichText::new(format!("Round {}", game.round)).size(19.0).strong());
                    ui.label(RichText::new(map_title).size(15.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        muted(ui, &scores::date_text(game.when));
                    });
                });
                for p in &game.players {
                    ui.horizontal(|ui| {
                        let name = egui::Label::new(RichText::new(&p.name).strong()).truncate();
                        ui.add_sized([200.0, 18.0], name);
                        let stats = format!(
                            "{} kills   {} headshots   {} points   {} downs",
                            p.kills, p.headshots, p.points, p.downs
                        );
                        muted(ui, &stats);
                    });
                }
            });
    }
}
