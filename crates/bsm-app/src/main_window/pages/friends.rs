//! Friends tab: invite commands, tunnel address and the network check.

use crate::actions::Action;
use crate::theme::*;
use crate::widgets::{card, field_label, muted, status_dot, switch_row};
use crate::App;
use bsm_core::net::{self, AddrKind};
use bsm_core::netcheck::{self, Level};
use eframe::egui::{self, RichText, Rounding};

const TUNNEL_HINT: &str = "For friends over the internet when your provider shares one public address \
    (4G/5G) and you do not use a VPN: a free UDP tunnel such as playit.gg, pointing to 127.0.0.1 and \
    your server port.";

const NET_CHECK_HINT: &str =
    "Looks at the firewall, your public address and the BOIII server list. Changes nothing.";

/// Who can use a connect command for this kind of address.
fn address_note(kind: AddrKind) -> &'static str {
    match kind {
        AddrKind::Lan => "same home network",
        AddrKind::Radmin => "friends in your Radmin VPN network",
        AddrKind::SharedNat => "shared or VPN address",
        AddrKind::Public => "public address of this PC",
    }
}

fn level_color(level: Level) -> egui::Color32 {
    match level {
        Level::Good => GOOD,
        Level::Warn => WARN,
        Level::Info => MUTED,
    }
}

impl App {
    pub(crate) fn friends_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        card(ui, "Invite", |ui| self.invite_card(ui, actions));
        card(ui, "Tunnel address (Playit)", |ui| self.tunnel_card(ui));
        card(ui, "Can friends reach me?", |ui| self.net_check_card(ui, actions));
    }

    fn invite_card(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        muted(ui, "Friends open the game console with the ~ key and paste this:");
        ui.horizontal(|ui| {
            egui::Frame::none()
                .fill(FIELD)
                .rounding(Rounding::same(8.0))
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(self.invite()).monospace().size(15.0));
                });
            if ui.button("Copy").clicked() {
                actions.push(Action::CopyInvite);
            }
        });

        ui.add_space(6.0);
        let port = self.server_port();
        let addresses = net::addresses();
        if !addresses.is_empty() {
            field_label(ui, "Other addresses of this PC");
        }
        for address in &addresses {
            ui.horizontal(|ui| {
                let command = net::connect_command(&address.ip.to_string(), port, &self.settings.password);
                if ui.small_button("Copy").clicked() {
                    actions.push(Action::Copy(command.clone(), "Command"));
                }
                ui.label(RichText::new(command).monospace());
                muted(ui, address_note(address.kind));
            });
        }
    }

    fn tunnel_card(&mut self, ui: &mut egui::Ui) {
        muted(ui, TUNNEL_HINT);
        ui.horizontal(|ui| {
            let address = egui::TextEdit::singleline(&mut self.settings.playit_address)
                .desired_width(300.0)
                .hint_text("name.joinmc.link:12345");
            ui.add(address);
            let valid = netcheck::normalize_tunnel(&self.settings.playit_address).is_some();
            if !self.settings.playit_address.trim().is_empty() && !valid {
                ui.label(RichText::new("Needs the form host:port").color(WARN));
            }
        });

        let mut use_tunnel = self.settings.use_playit;
        switch_row(
            ui,
            &mut use_tunnel,
            "Use the tunnel address in invites",
            "Keep the tunnel program running while you host.",
        );
        // A half-typed address is never used in invites.
        let valid = netcheck::normalize_tunnel(&self.settings.playit_address).is_some();
        self.settings.use_playit = use_tunnel && valid;
    }

    fn net_check_card(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if ui.add_enabled(self.busy.is_none(), egui::Button::new("Check network")).clicked() {
                actions.push(Action::NetCheck);
            }
            if self.net_check_running {
                ui.add(egui::Spinner::new());
                muted(ui, "This takes a few seconds.");
            } else {
                muted(ui, NET_CHECK_HINT);
            }
        });

        let Some(findings) = &self.net_findings else { return };
        ui.add_space(4.0);
        for (level, text) in findings {
            ui.horizontal_top(|ui| {
                status_dot(ui, egui::vec2(14.0, 20.0), 5.0, level_color(*level));
                ui.add(egui::Label::new(RichText::new(text).size(14.5)).wrap());
            });
        }
    }
}
