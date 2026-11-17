//! Slow work on worker threads, and the messages they send back to the window.
//!
//! The window never blocks: a job runs on its own thread with a copy of the
//! settings and reports through [`Message`]s, which [`App::handle_messages`]
//! applies at the start of the next frame.

use crate::{App, Dialog, Discovery, Tab};
use bsm_core::netcheck::Level;
use bsm_core::server::{self, CheckState, Env, State, Status};
use bsm_core::settings::Settings;
use bsm_core::{content, paths, saves, scores, Error, APP_TITLE};
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// What a job returns; an error is shown to the user in a dialog.
pub type JobResult = Result<(), Error>;

/// What a worker thread can tell the window.
pub enum Message {
    /// A line (or several) for the Log tab.
    Log(String),
    /// A fresh look at the PC finished.
    Discovered(Discovery),
    /// The status poller's latest view of the server.
    Status(Status),
    /// A job failed; shown as a dialog and logged.
    Error(String),
    /// A notice with a title, shown as a dialog.
    Info(String, String),
    /// A question; the worker waits for the answer on `reply`.
    AskYesNo { title: String, text: String, yes: &'static str, no: &'static str, reply: Sender<bool> },
    /// Put this text on the clipboard.
    Clipboard(String),
    /// Results of the network check.
    NetFindings(Vec<(Level, String)>),
    /// A map picture was made (or could not be): map name and file.
    Picture(String, Option<PathBuf>),
    /// The scoreboard changed because a game finished.
    Scores(Vec<scores::Game>),
    /// The list of saved games changed.
    Saves(Vec<saves::Save>),
    /// Switch to this tab.
    GoTo(Tab),
    /// The running job is over.
    Done,
}

/// Looks at the PC: what is installed, what is missing, which maps and mods exist.
pub fn scan_pc(settings: Settings) -> Discovery {
    let env = Env::discover(settings);
    let checks = server::checks(&env);
    let content = env.content();
    let ready = checks
        .iter()
        .filter(|c| matches!(c.name, "Server files" | "BOIII client" | "Zombies files"))
        .all(|c| c.state == CheckState::Ok);
    Discovery {
        checks,
        maps: content.maps,
        mods: content.mods,
        server_dir: env.server_dir.clone(),
        game_dir: env.game_dir.clone(),
        ready,
    }
}

/// The Log tab keeps at most this many lines.
const MAX_LOG_LINES: usize = 3000;

/// How often the status poller looks at the server.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Time the players get to see the end-of-game screen before the map changes.
const GAME_OVER_PAUSE: Duration = Duration::from_secs(8);

/// A rotation with workshop maps is switched by this program rather than the
/// game (see [`server::tool_rotates`]): restart the server on the next map.
fn advance_rotation(settings: Settings, tx: &Sender<Message>) {
    let env = Env::discover(settings);
    let selection = content::select_list(&env.content(), &env.settings.play_list(), &env.settings.mod_id);
    let tool_rotates = selection.map(|s| server::tool_rotates(&s)).unwrap_or(false);
    if !tool_rotates || server::status(&env).state == State::Stopped {
        return;
    }
    std::thread::sleep(GAME_OVER_PAUSE);
    let log = |line: &str| {
        let _ = tx.send(Message::Log(line.to_string()));
    };
    log("--- Game over: changing to the next map of the rotation ---");
    if let Err(e) = server::next_map(&env, &log) {
        log(&format!("ERROR: {e}"));
    }
}

impl App {
    /// Keeps the status header, saved games and scoreboard up to date for as
    /// long as the window is open.
    pub(crate) fn start_status_poller(&self, ctx: egui::Context) {
        let tx = self.tx.clone();
        let mut last_saves: Option<Vec<saves::Save>> = None;
        std::thread::spawn(move || loop {
            let data = paths::data_dir();
            let env = Env::new(Settings::load(&data), data, Vec::new());
            if tx.send(Message::Status(server::status(&env))).is_err() {
                break; // the window is gone
            }

            // Saved games are written by the game; show new ones as they appear.
            let saves = saves::list(&env.server_dir);
            if last_saves.as_ref() != Some(&saves) {
                let _ = tx.send(Message::Saves(saves.clone()));
                last_saves = Some(saves);
            }

            // Finished games the server wrote to its log go onto the scoreboard.
            let finished_games = scores::import(&env.server_dir, &env.data_dir).unwrap_or(0);
            if finished_games > 0 {
                let _ = tx.send(Message::Scores(scores::best(scores::load(&env.data_dir))));
                advance_rotation(env.settings.clone(), &tx);
            }

            ctx.request_repaint();
            std::thread::sleep(POLL_INTERVAL);
        });
    }

    /// Runs `job` on a worker thread with the current settings, then looks at the PC again.
    ///
    /// The job gets the environment, a function that writes a line to the Log
    /// tab, and the channel for any other [`Message`]. Only one job runs at a
    /// time; while one is busy this does nothing.
    pub(crate) fn run_job<F>(&mut self, ctx: &egui::Context, title: &str, job: F)
    where
        F: FnOnce(&Env, &dyn Fn(&str), &Sender<Message>) -> JobResult + Send + 'static,
    {
        if self.busy.is_some() {
            return;
        }
        let mut settings = self.settings.clone();
        settings.sanitize();
        if let Err(e) = server::validate_settings(&settings) {
            self.dialog = Some(Dialog::notice("Check the settings", e.to_string()));
            return;
        }
        if let Err(e) = settings.save(&paths::data_dir()) {
            self.dialog = Some(Dialog::notice(APP_TITLE, e.to_string()));
            return;
        }
        self.saved = settings.clone();
        self.busy = Some(title.to_string());
        self.log.push(format!("--- {title} ---"));
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let env = Env::discover(settings.clone());
            let log_tx = tx.clone();
            let log_ctx = ctx.clone();
            let log = move |line: &str| {
                let _ = log_tx.send(Message::Log(line.to_string()));
                log_ctx.request_repaint();
            };
            if let Err(e) = job(&env, &log, &tx) {
                let _ = tx.send(Message::Error(e.to_string()));
            }
            let _ = tx.send(Message::Discovered(scan_pc(settings)));
            let _ = tx.send(Message::Done);
            ctx.request_repaint();
        });
    }

    /// Looks at the PC again (the checklist, maps and mods).
    pub(crate) fn refresh(&mut self, ctx: &egui::Context) {
        self.run_job(ctx, "Checking this PC", |_, _, _| Ok(()));
    }

    /// Applies everything the worker threads sent since the last frame.
    pub(crate) fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Message::Log(line) => {
                    for l in line.replace("\r\n", "\n").split('\n') {
                        self.log.push(l.to_string());
                    }
                    // Keep memory bounded on servers that run for days.
                    if self.log.len() > MAX_LOG_LINES {
                        self.log.drain(..MAX_LOG_LINES / 3);
                    }
                }
                Message::Discovered(discovery) => {
                    // First look: send a PC that is not set up yet to the Setup page.
                    if !self.discovered && !discovery.ready {
                        self.tab = Tab::Setup;
                    }
                    self.discovered = true;
                    self.discovery = discovery;
                }
                Message::Status(status) => {
                    if self.last_state != Some(status.state) {
                        if self.last_state.is_some() {
                            self.log.push(status.summary());
                        }
                        self.last_state = Some(status.state);
                    }
                    self.status = Some(status);
                }
                Message::Error(e) => {
                    self.log.push(format!("ERROR: {e}"));
                    self.dialog = Some(Dialog::notice("That did not work", e));
                }
                Message::Info(title, text) => self.dialog = Some(Dialog::notice(title, text)),
                Message::AskYesNo { title, text, yes, no, reply } => {
                    self.dialog = Some(Dialog { title, text, yes, no: Some(no), reply: Some(reply) })
                }
                Message::Clipboard(text) => ctx.copy_text(text),
                Message::NetFindings(findings) => {
                    self.net_findings = Some(findings);
                    self.net_check_running = false;
                }
                Message::Picture(name, path) => {
                    self.pictures.insert(name, path);
                }
                Message::Saves(saves) => self.saves = saves,
                Message::Scores(games) => {
                    self.scores = games;
                    self.toast("A finished game was added to the scoreboard");
                }
                Message::GoTo(tab) => self.tab = tab,
                Message::Done => {
                    // The server start uses up "continue from save"; take that from the file.
                    let disk = Settings::load(&paths::data_dir());
                    self.settings.continue_save = disk.continue_save.clone();
                    self.saved.continue_save = disk.continue_save;
                    self.busy = None;
                    self.net_check_running = false;
                }
            }
        }
    }
}
