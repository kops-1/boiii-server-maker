//! The parts of the window that are always visible (sidebar and status
//! header), plus one module per tab.

use crate::actions::Action;
use crate::theme::*;
use crate::widgets::{big_button, status_dot};
use crate::{App, Tab};
use bsm_core::server::State;
use bsm_core::VERSION;
use eframe::egui::{self, Color32, RichText, Rounding};

mod cheats;
mod friends;
mod host;
mod log;
mod scores;
mod setup;

const TABS: [(Tab, &str); 6] = [
    (Tab::Host, "Host"),
    (Tab::Cheats, "Cheats"),
    (Tab::Friends, "Friends"),
    (Tab::Setup, "Setup"),
    (Tab::Scores, "Scores"),
    (Tab::Log, "Log"),
];

/// What the header says about the server: a coloured dot, one word and a line of detail.
struct StatusLine {
    color: Color32,
    headline: &'static str,
    detail: String,
}

impl App {
    pub(crate) fn sidebar(&mut self, ctx: &egui::Context) {
        let frame = egui::Frame::none().fill(SIDE).inner_margin(egui::Margin::same(14.0));
        egui::SidePanel::left("nav").exact_width(190.0).resizable(false).frame(frame).show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("Boiii").size(26.0).strong().color(ACCENT));
            ui.label(RichText::new("Server Maker").size(17.0).color(INK));
            ui.add_space(18.0);

            let needs_setup = self.discovered && !self.discovery.ready;
            for (tab, label) in TABS {
                let selected = self.tab == tab;
                let mut text = RichText::new(label).size(16.0);
                if selected {
                    text = text.strong().color(Color32::WHITE);
                }
                let button = egui::Button::new(text)
                    .fill(if selected { HIGHLIGHT } else { Color32::TRANSPARENT })
                    .min_size(egui::vec2(162.0, 38.0));
                let response = ui.add(button);

                // A dot on Setup while something still has to be done there.
                if tab == Tab::Setup && needs_setup {
                    let dot = response.rect.right_center() - egui::vec2(16.0, 0.0);
                    ui.painter().circle_filled(dot, 5.0, WARN);
                }
                if response.clicked() && !selected {
                    self.tab = tab;
                    if tab == Tab::Setup {
                        self.refresh(ctx);
                    }
                }
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.label(RichText::new(format!("Version {VERSION}")).color(MUTED).size(12.5));
            });
        });
    }

    fn status_line(&self) -> StatusLine {
        let Some(status) = &self.status else {
            return StatusLine { color: MUTED, headline: "Checking", detail: String::new() };
        };
        match status.state {
            State::Running => {
                let info = |key: &str| status.info.get(key).cloned().unwrap_or_else(|| "?".into());
                let map = self.discovery.map_title(&info("mapname"));
                let players = format!("{}/{} players", info("clients"), info("sv_maxclients"));
                StatusLine {
                    color: GOOD,
                    headline: "Running",
                    detail: format!("{map}   -   {players}   -   UDP port {}", status.port),
                }
            }
            State::Starting => StatusLine {
                color: WARN,
                headline: "Starting",
                detail: format!("Loading the map on UDP port {}. This takes up to a minute.", status.port),
            },
            State::Stopped => StatusLine {
                color: MUTED,
                headline: "Stopped",
                detail: "Choose a map and press Start server.".into(),
            },
        }
    }

    /// The card at the top: server name, state, and the main buttons.
    pub(crate) fn header(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let status = self.status_line();
        let frame = egui::Frame::none()
            .fill(CARD)
            .rounding(Rounding::same(12.0))
            .inner_margin(egui::Margin::same(18.0));
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                status_dot(ui, egui::vec2(16.0, 40.0), 7.0, status.color);

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    // The server's name, as players see it in the server list.
                    let name = self.settings.server_name.trim();
                    let name = if name.is_empty() { "Unnamed server" } else { name };
                    ui.label(RichText::new(name).size(24.0).strong().color(Color32::WHITE));

                    let detail = match &self.busy {
                        Some(job) => format!("{job}..."),
                        None => status.detail,
                    };
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let headline = status.headline.to_uppercase();
                        ui.label(RichText::new(headline).size(13.0).strong().color(status.color));
                        ui.label(RichText::new(detail).color(MUTED).size(14.0));
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.header_buttons(ui, actions);
                });
            });
        });
        ui.add_space(12.0);
    }

    /// Laid out right to left, so the first button added sits at the far right.
    fn header_buttons(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let idle = self.busy.is_none();
        if big_button(ui, "Copy invite", FIELD, true) {
            actions.push(Action::CopyInvite);
        }

        let star = egui::Button::new(RichText::new("\u{2605}").size(19.0).color(GOLD))
            .fill(FIELD)
            .min_size(egui::vec2(40.0, 38.0));
        let star = ui.add(star).on_hover_text("Add this server to the Favorites list in the game on this PC");
        if star.clicked() {
            actions.push(Action::Favorite);
        }

        if big_button(ui, "Play", ACCENT, idle) {
            actions.push(Action::Play);
        }
        if self.running() {
            if big_button(ui, "Stop server", BAD, idle) {
                actions.push(Action::Stop);
            }
        } else if big_button(ui, "Start server", GOOD_BUTTON, idle) {
            actions.push(Action::Start);
        }
        if !idle {
            ui.add(egui::Spinner::new().size(22.0));
        }
    }
}
