//! What the buttons do. Pages push an [`Action`] while drawing; [`App::perform`]
//! carries it out once the frame is finished.
//!
//! Quick actions run right here. Slow ones are handed to [`App::run_job`] as a
//! job function, which runs on a worker thread.

use crate::background::{JobResult, Message};
use crate::shell::{open_in_explorer, open_url};
use crate::{App, Dialog, Tab};
use bsm_core::server::{self, Env, SetupOptions, State};
use bsm_core::{game, netcheck, saves, steam, steamcmd, Error};
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};

/// Steam's app id of "Call of Duty: Black Ops III - Unranked Dedicated Server".
const SERVER_APP_ID: u32 = 545990;

/// Things the user asked for during a frame, carried out after drawing.
pub enum Action {
    // Server
    Start,
    Stop,
    Restart,
    Play,
    Setup,
    Repair,
    Refresh,

    // Friends
    CopyInvite,
    Favorite,
    NetCheck,
    /// Put text on the clipboard; the label names it in the toast ("Log copied").
    Copy(String, &'static str),

    // Server files and folders
    SteamInstall,
    Steamcmd,
    PickServerDir,
    PickGameDir,
    AutoDirs,
    Open(PathBuf),

    // Presets and saved games
    SavePreset,
    LoadPreset(String),
    DeletePreset(String),
    ContinueSave(String),
    DeleteSave(String),
}

impl App {
    pub(crate) fn perform(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Start => self.run_job(ctx, "Start server", start_job),
            Action::Stop => self.run_job(ctx, "Stop server", |env, log, _| server::stop(env, log).map(drop)),
            Action::Restart => self.run_job(ctx, "Restart server", |env, log, _| {
                server::stop(env, log)?;
                server::start(env, log).map(drop)
            }),
            Action::Setup => self.run_job(ctx, "Setup", |env, log, tx| setup_job(env, log, tx, false)),
            Action::Repair => self.run_job(ctx, "Repair files", |env, log, tx| setup_job(env, log, tx, true)),
            Action::Refresh => self.refresh(ctx),
            Action::Play => {
                let wake = ctx.clone();
                self.run_job(ctx, "Play", move |env, log, tx| play_job(env, log, tx, &wake));
            }

            Action::CopyInvite => {
                let invite = self.invite();
                ctx.copy_text(invite.clone());
                self.log.push(format!("Copied: {invite}"));
                self.toast("Invite copied. Friends paste it into the game console (~ key).");
            }
            Action::Favorite => self.add_favorite(),
            Action::NetCheck => {
                self.net_check_running = true;
                self.run_job(ctx, "Network check", |env, log, tx| {
                    let report = netcheck::check(env, log);
                    log(&netcheck::format(&report));
                    let _ = tx.send(Message::NetFindings(netcheck::findings(&report)));
                    Ok(())
                });
            }
            Action::Copy(text, label) => {
                ctx.copy_text(text);
                self.toast(format!("{label} copied"));
            }

            Action::SteamInstall => {
                open_url(&format!("steam://install/{SERVER_APP_ID}"));
                self.log.push(
                    "Asked Steam to install \"Call of Duty: Black Ops III - Unranked Dedicated Server\" \
                     (free tool, about 5 GB). When Steam has finished, press Start server."
                        .into(),
                );
                self.toast("Steam was asked to install the server tool");
            }
            Action::Steamcmd => self.download_with_steamcmd(ctx),
            Action::PickServerDir => {
                if let Some(dir) = rfd::FileDialog::new().set_title("Server folder").pick_folder() {
                    self.settings.server_dir = Some(dir);
                    self.refresh(ctx);
                }
            }
            Action::PickGameDir => self.pick_game_dir(ctx),
            Action::AutoDirs => {
                self.settings.server_dir = None;
                self.settings.game_dir = None;
                self.refresh(ctx);
            }
            Action::Open(path) => open_in_explorer(&path),

            Action::SavePreset => {
                let name = self.preset_name.trim().to_string();
                if self.settings.save_preset(&name) {
                    self.preset_pick = name.clone();
                    self.preset_name.clear();
                    self.toast(format!("Preset \"{name}\" saved"));
                } else {
                    self.toast("Type a name for the preset first");
                }
            }
            Action::LoadPreset(name) => {
                if let Some(preset) = self.settings.presets.get(&name).cloned() {
                    self.settings.apply_preset(&preset);
                    self.toast(format!("Preset \"{name}\" loaded. It applies at the next server start."));
                }
            }
            Action::DeletePreset(name) => {
                self.settings.presets.remove(&name);
                self.preset_pick.clear();
                self.toast(format!("Preset \"{name}\" deleted"));
            }
            Action::ContinueSave(slot) => self.continue_from_save(slot),
            Action::DeleteSave(slot) => self.delete_save(&slot),
        }
    }

    fn add_favorite(&mut self) {
        let mut settings = self.settings.clone();
        settings.sanitize();
        let (title, text) = match game::add_favorite(&Env::discover(settings)) {
            Ok((true, endpoint)) => (
                "Added to Favorites",
                format!(
                    "{endpoint} is now in the game's Favorites.\n\nIn the game: open the server browser \
                     and choose Favorites. The server has to be running to show up. If you change the \
                     port, press the star again."
                ),
            ),
            Ok((false, endpoint)) => {
                ("Already a favourite", format!("{endpoint} is already in the game's Favorites."))
            }
            Err(e) => ("Favorites", e.to_string()),
        };
        self.log.push(format!("{title}: {}", text.lines().next().unwrap_or_default()));
        self.dialog = Some(Dialog::notice(title, text));
    }

    fn pick_game_dir(&mut self, ctx: &egui::Context) {
        let Some(dir) = rfd::FileDialog::new().set_title("Black Ops III folder").pick_folder() else {
            return;
        };
        if steam::is_game_dir(&dir) {
            self.settings.game_dir = Some(dir);
            self.refresh(ctx);
        } else {
            self.dialog = Some(Dialog::notice(
                "Not a game folder",
                "That folder has no \"zone\" folder, so it is not a Black Ops III install.",
            ));
        }
    }

    /// The next server start loads this save (and its map) instead of starting fresh.
    fn continue_from_save(&mut self, slot: String) {
        let Some(save) = self.saves.iter().find(|s| s.slot == slot).cloned() else { return };
        self.settings.continue_save = slot;
        self.settings.rotation.clear();
        self.settings.map = save.map.clone();
        let next_step = if self.running() { "Restart the server" } else { "Start the server" };
        self.toast(format!("{next_step} to continue at round {} on {}", save.round, save.map));
    }

    fn delete_save(&mut self, slot: &str) {
        let server_dir = self.discovery.server_dir.clone();
        match saves::delete(&server_dir, slot) {
            Ok(_) => {
                self.saves = saves::list(&server_dir);
                if self.settings.continue_save == slot {
                    self.settings.continue_save.clear();
                }
                self.toast(format!("Save slot {slot} deleted"));
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    fn download_with_steamcmd(&mut self, ctx: &egui::Context) {
        if !steamcmd::valid_user(self.settings.steam_user.trim()) {
            self.dialog = Some(Dialog::notice(
                "Steam login name needed",
                "Type your Steam login name into the box first.\n\nOnly the name is needed here. \
                 The password is typed later, in steamcmd's own window.",
            ));
            return;
        }
        if self.settings.server_dir.is_some() {
            self.dialog = Some(Dialog::notice(
                "steamcmd",
                "steamcmd downloads into the default server folder. Press \"Automatic folders\" first.",
            ));
            return;
        }
        let wake = ctx.clone();
        self.run_job(ctx, "Download server files with steamcmd", move |env, log, tx| {
            steamcmd_job(env, log, tx, &wake)
        });
    }
}

// ----------------------------------------------------------------------------
// Jobs: these run on a worker thread (see `App::run_job`).

/// Asks the user a yes/no question from a worker thread and waits for the answer.
fn ask(tx: &Sender<Message>, wake: &egui::Context, title: &str, text: String, yes: &'static str) -> bool {
    let (reply_tx, reply_rx) = channel();
    let question = Message::AskYesNo { title: title.to_string(), text, yes, no: "Cancel", reply: reply_tx };
    let _ = tx.send(question);
    wake.request_repaint();
    reply_rx.recv().unwrap_or(false)
}

/// Runs setup. When the server files are missing, sends the user to the Setup
/// page with instructions instead of failing; returns false in that case.
fn ensure_setup(
    env: &Env,
    options: SetupOptions,
    log: &dyn Fn(&str),
    tx: &Sender<Message>,
    then: &str,
) -> Result<bool, Error> {
    match server::run_setup(env, options, log) {
        Ok(()) => Ok(true),
        Err(Error::NeedServerFiles(why)) => {
            let _ = tx.send(Message::GoTo(Tab::Setup));
            let _ = tx.send(Message::Info(
                "Server files needed".into(),
                format!(
                    "{why}\n\nOn the Setup page use \"Install through Steam\" (easiest), wait for Steam \
                     to finish, then {then}"
                ),
            ));
            Ok(false)
        }
        Err(other) => Err(other),
    }
}

fn setup_job(env: &Env, log: &dyn Fn(&str), tx: &Sender<Message>, repair: bool) -> JobResult {
    let options = SetupOptions { repair, ..SetupOptions::default() };
    let then = "start again. Without the Steam client use \"Download with steamcmd\".";
    ensure_setup(env, options, log, tx, then).map(drop)
}

fn start_job(env: &Env, log: &dyn Fn(&str), tx: &Sender<Message>) -> JobResult {
    // Setup is quick when everything is in place, so Start always makes sure of it.
    if !ensure_setup(env, SetupOptions::default(), log, tx, "press Start again.")? {
        return Ok(());
    }
    server::start(env, log)?;
    if env.settings.menu {
        log("Mod menu in the game: hold AIM and press MELEE. FIRE = down, AIM = up, USE = select, MELEE = back.");
    }
    log("If Windows asks whether boiii.exe may use the network, choose Allow.");
    log("You can close this window; the server keeps running until you press Stop.");
    Ok(())
}

fn play_job(env: &Env, log: &dyn Fn(&str), tx: &Sender<Message>, wake: &egui::Context) -> JobResult {
    let launched = match game::launch(env, false, log) {
        // boiii.exe is missing from the game folder: offer to copy it there.
        Err(Error::NeedClient(why)) => {
            let question = format!(
                "{}\n\nCopy boiii.exe from the server folder into the game folder now? \
                 Nothing else in the game folder is changed.",
                why.replace("\r\n", "\n")
            );
            if !ask(tx, wake, "Play", question, "Copy it") {
                log("Play cancelled: boiii.exe is not in the game folder.");
                return Ok(());
            }
            game::launch(env, true, log)?
        }
        other => other?,
    };

    let connect = launched.plan.connect.clone();
    let _ = tx.send(Message::Clipboard(connect.clone()));
    let running = server::status(env).state != State::Stopped;
    log(&game::how_to_join(&launched, running));

    let not_running_yet = if running {
        ""
    } else {
        "The server is not running yet: press Start server and wait for RUNNING.\n\n"
    };
    let _ = tx.send(Message::Info(
        "Game started".into(),
        format!(
            "{not_running_yet}When the game shows its main menu:\n\n\
             1. Press the ~ key (console)\n2. Paste with Ctrl+V\n3. Press Enter\n\n\
             The join command is already on your clipboard:\n{connect}"
        ),
    ));
    Ok(())
}

fn steamcmd_job(env: &Env, log: &dyn Fn(&str), tx: &Sender<Message>, wake: &egui::Context) -> JobResult {
    let intro = format!(
        "This downloads Valve's steamcmd, then the server files (Steam app {SERVER_APP_ID}) with your \
         Steam account \"{}\".\n\n{}",
        env.settings.steam_user,
        steamcmd::LOGIN_HELP.replace("\r\n", "\n")
    );
    if !ask(tx, wake, "Download with steamcmd", intro, "Start download") {
        return Ok(());
    }

    let exe = steamcmd::ensure(&env.data_dir, log)?;
    let root = env.data_dir.join("server");
    loop {
        log("steamcmd is running in its own window...");
        if let Some(dir) = steamcmd::run_once(&exe, &root, &env.settings.steam_user, true)? {
            log(&format!("Server files downloaded to {}. Press Start server.", dir.display()));
            return Ok(());
        }
        let retry = "The server files are not complete yet.\n\nMost common reason: the sign-in was not \
                     approved in the Steam mobile app in time, or the password was mistyped.\n\nGet your \
                     phone ready first, then approve the sign-in as soon as steamcmd asks.";
        if !ask(tx, wake, "steamcmd did not finish", retry.into(), "Try again") {
            log("steamcmd download cancelled.");
            return Ok(());
        }
    }
}
