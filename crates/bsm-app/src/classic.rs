//! `BoiiiServerMakerClassic.exe`: the fallback window, built from classic Windows controls.
//!
//! Started by the main window when the graphics driver cannot run egui, or
//! directly with `BoiiiServerMaker.exe --classic`. Same features as the main
//! window, less polish.
#![windows_subsystem = "windows"]

extern crate native_windows_gui as nwg;

use bsm_core::server::{self, Env, SetupOptions, State};
use bsm_core::settings::Settings;
use bsm_core::{net, paths, steamcmd, Error, APP_TITLE};
use std::cell::{Cell, RefCell};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

enum Msg {
    Log(String),
    /// Fresh look at the PC: checklist text, maps, the folders in use.
    Refreshed {
        checks: String,
        maps: Vec<String>,
        mods: Vec<String>,
        server_dir: String,
        game_dir: String,
    },
    Status {
        text: String,
        state: State,
        port: u16,
    },
    Error(String),
    Info(String, String),
    /// steamcmd did not finish: ask whether to try again.
    AskRetry(Sender<bool>),
    /// A yes/no question from a worker thread (title, text, where to send the answer).
    AskYesNo(String, String, Sender<bool>),
    /// Put this text on the clipboard.
    Clipboard(String),
    Done,
}

#[derive(Default)]
struct App {
    window: nwg::Window,
    icon: Option<nwg::Icon>,
    notice: nwg::Notice,
    mono: nwg::Font,
    labels: Vec<nwg::Label>,

    server_dir: nwg::TextInput,
    game_dir: nwg::TextInput,
    map: nwg::ComboBox<String>,
    port: nwg::TextInput,
    players: nwg::TextInput,
    name: nwg::TextInput,
    password: nwg::TextInput,
    steam_user: nwg::TextInput,
    mod_box: nwg::ComboBox<String>,
    menu: nwg::CheckBox,
    menu_players: nwg::TextInput,
    god: nwg::CheckBox,
    ammo: nwg::CheckBox,
    perks: nwg::CheckBox,
    start_points: nwg::TextInput,

    btn_server_dir: nwg::Button,
    btn_open_dir: nwg::Button,
    btn_game_dir: nwg::Button,
    btn_auto: nwg::Button,
    btn_setup: nwg::Button,
    btn_start: nwg::Button,
    btn_stop: nwg::Button,
    btn_copy: nwg::Button,
    btn_join: nwg::Button,
    btn_play: nwg::Button,
    btn_steam: nwg::Button,
    btn_steamcmd: nwg::Button,
    btn_refresh: nwg::Button,
    btn_logs: nwg::Button,
    btn_repair: nwg::Button,

    checklist: nwg::TextBox,
    status: nwg::Label,
    log: nwg::TextBox,
    folder_dialog: nwg::FileDialog,

    tx: Option<Sender<Msg>>,
    rx: Option<Receiver<Msg>>,
    busy: Cell<bool>,
    /// Folders the user picked by hand (None = automatic).
    server_override: RefCell<Option<PathBuf>>,
    game_override: RefCell<Option<PathBuf>>,
    last_port: Cell<u16>,
    last_state: Cell<Option<State>>,
}

const NO_MOD: &str = "No mod";

/// `zm_town - TOWN REIMAGINED (workshop)` gives `zm_town`.
fn map_name(label: &str) -> String {
    label.split(" - ").next().unwrap_or(label).trim().to_string()
}

/// `Some mod title [2631943123]` gives `2631943123`; "No mod" gives "".
fn mod_id(label: &str) -> String {
    match (label.rfind('['), label.rfind(']')) {
        (Some(a), Some(b)) if a < b => label[a + 1..b].trim().to_string(),
        _ => String::new(),
    }
}

fn is_checked(c: &nwg::CheckBox) -> bool {
    c.check_state() == nwg::CheckBoxState::Checked
}

fn open_in_explorer(path: &Path) {
    let explorer = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("explorer.exe"))
        .unwrap_or_else(|| "explorer.exe".into());
    let _ = std::process::Command::new(explorer).arg(path).spawn();
}

fn open_url(url: &str) {
    let cmd = std::env::var_os("ComSpec").map(PathBuf::from).unwrap_or_else(|| "cmd.exe".into());
    let _ = std::process::Command::new(cmd)
        .args(["/c", "start", "", url])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

impl App {
    fn tx(&self) -> Sender<Msg> {
        self.tx.as_ref().expect("channel").clone()
    }

    fn append_log(&self, line: &str) {
        if self.log.len() > 200_000 {
            self.log.set_text("");
        }
        self.log.appendln(&line.replace("\r\n", "\n").replace('\n', "\r\n"));
        self.log.scroll_lastline();
    }

    /// Settings as currently shown in the window.
    fn collect(&self) -> Result<Settings, String> {
        let mut s = Settings::load(&paths::data_dir());
        s.port = self
            .port
            .text()
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|p| *p >= 1024)
            .ok_or("The port must be a number from 1024 to 65535.")?;
        s.max_players = self
            .players
            .text()
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|p| (1..=8).contains(p))
            .ok_or("Players must be a number from 1 to 8.")?;
        s.map = map_name(&self.map.selection_string().unwrap_or_else(|| s.map.clone()));
        s.server_name = self.name.text().trim().to_string();
        s.password = self.password.text().trim().to_string();
        s.steam_user = self.steam_user.text().trim().to_string();
        s.server_dir = self.server_override.borrow().clone();
        s.game_dir = self.game_override.borrow().clone();
        s.mod_id = mod_id(&self.mod_box.selection_string().unwrap_or_default());
        s.menu = is_checked(&self.menu);
        s.menu_players = self.menu_players.text();
        s.god_all = is_checked(&self.god);
        s.ammo_all = is_checked(&self.ammo);
        s.start_perks = is_checked(&self.perks);
        let points = self.start_points.text();
        s.start_points = if points.trim().is_empty() {
            0
        } else {
            points
                .trim()
                .parse::<u32>()
                .map_err(|_| "Start points must be a whole number (0 = normal start).")?
        };
        s.sanitize();
        server::validate_settings(&s).map_err(|e| e.to_string())?;
        Ok(s)
    }

    fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
        for b in [
            &self.btn_setup,
            &self.btn_start,
            &self.btn_play,
            &self.btn_stop,
            &self.btn_steam,
            &self.btn_steamcmd,
            &self.btn_refresh,
            &self.btn_repair,
            &self.btn_server_dir,
            &self.btn_game_dir,
            &self.btn_auto,
        ] {
            b.set_enabled(!busy);
        }
    }

    /// Runs `job` on a worker thread with the settings from the window, then
    /// refreshes the checklist.
    fn run_job<F>(&self, title: &str, job: F)
    where
        F: FnOnce(&Env, &dyn Fn(&str), &Sender<Msg>) -> Result<(), Error> + Send + 'static,
    {
        if self.busy.get() {
            return;
        }
        let settings = match self.collect() {
            Ok(s) => s,
            Err(e) => {
                nwg::modal_error_message(&self.window, APP_TITLE, &e);
                return;
            }
        };
        if let Err(e) = settings.save(&paths::data_dir()) {
            nwg::modal_error_message(&self.window, APP_TITLE, &e.to_string());
            return;
        }
        self.set_busy(true);
        self.append_log(&format!("--- {title} ---"));
        let tx = self.tx();
        let wake = self.notice.sender();
        std::thread::spawn(move || {
            let env = Env::discover(settings.clone());
            let log_tx = tx.clone();
            let log = move |line: &str| {
                let _ = log_tx.send(Msg::Log(line.to_string()));
                wake.notice();
            };
            if let Err(e) = job(&env, &log, &tx) {
                let _ = tx.send(Msg::Error(e.to_string()));
            }
            let _ = tx.send(refresh_msg(settings));
            let _ = tx.send(Msg::Done);
            wake.notice();
        });
    }

    fn refresh(&self) {
        self.run_job("Checking this PC", |_, _, _| Ok(()));
    }

    fn pick_folder(&self, title: &str) -> Option<PathBuf> {
        let _ = title;
        if !self.folder_dialog.run(Some(&self.window)) {
            return None;
        }
        self.folder_dialog.get_selected_item().ok().map(PathBuf::from)
    }

    fn on_click(&self, handle: &nwg::ControlHandle) {
        if handle == &self.btn_setup.handle {
            self.run_job("Setup", |env, log, tx| {
                match server::run_setup(env, SetupOptions::default(), log) {
                    Err(Error::NeedServerFiles(m)) => {
                        let _ = tx.send(Msg::Info(
                            "Server files needed".into(),
                            format!(
                                "{m}\r\n\r\nUse one of the two buttons below the checklist:\r\n\r\n\
                             - \"Install server through Steam\": easiest when Steam is installed on this PC. \
                             When Steam has finished, press \"Run setup\" again.\r\n\
                             - \"Download with steamcmd\": works without the Steam client."
                            ),
                        ));
                        Ok(())
                    }
                    other => other,
                }
            });
        } else if handle == &self.btn_start.handle {
            self.run_job("Start server", |env, log, _| {
                server::start(env, log)?;
                if env.settings.menu {
                    log("Mod menu in the game: hold AIM and press MELEE to open. FIRE = down, AIM = up, USE = select, MELEE = back.");
                }
                log("If Windows asks whether boiii.exe may use the network, choose Allow.");
                log("You can close this window; the server keeps running until you press Stop server.");
                Ok(())
            });
        } else if handle == &self.btn_repair.handle {
            self.run_job("Repair files", |env, log, _| {
                if server::status(env).state != State::Stopped {
                    return Err(Error::other("Stop the server first, then repair."));
                }
                server::run_setup(env, SetupOptions { repair: true, ..SetupOptions::default() }, log)
            });
        } else if handle == &self.btn_stop.handle {
            self.run_job("Stop server", |env, log, _| server::stop(env, log).map(|_| ()));
        } else if handle == &self.btn_play.handle {
            let wake = self.notice.sender();
            self.run_job("Play", move |env, log, tx| {
                let launched = match bsm_core::game::launch(env, false, log) {
                    Err(Error::NeedClient(why)) => {
                        // Adding a file to the game folder is the user's decision.
                        let (reply_tx, reply_rx) = channel();
                        let _ = tx.send(Msg::AskYesNo(
                            "Play".into(),
                            format!("{why}\r\n\r\nCopy boiii.exe from the server folder into the game folder now? Nothing else in the game folder is changed."),
                            reply_tx,
                        ));
                        wake.notice();
                        if !reply_rx.recv().unwrap_or(false) {
                            log("Play cancelled: boiii.exe is not in the game folder.");
                            return Ok(());
                        }
                        bsm_core::game::launch(env, true, log)?
                    }
                    other => other?,
                };
                let _ = tx.send(Msg::Clipboard(launched.plan.connect.clone()));
                let running = server::status(env).state != State::Stopped;
                log(&bsm_core::game::how_to_join(&launched, running));
                log("The command is on the clipboard now.");
                if !env.settings.mod_id.is_empty() {
                    log("This server runs a mod: the game loads it when you join and may restart itself once to do so.");
                }
                Ok(())
            });
        } else if handle == &self.btn_refresh.handle {
            self.refresh();
        } else if handle == &self.btn_copy.handle {
            let addrs = net::addresses();
            let ip = net::preferred(&addrs).map(|a| a.ip.to_string()).unwrap_or_else(|| "127.0.0.1".into());
            let port = self.last_port.get();
            let cmd = net::connect_command(&ip, port, self.password.text().trim());
            nwg::Clipboard::set_data_text(&self.window, &cmd);
            self.append_log(&format!(
                "Copied to the clipboard: {cmd}   (other addresses: press \"How friends join\")"
            ));
        } else if handle == &self.btn_join.handle {
            let help = net::join_help(self.last_port.get(), self.password.text().trim(), &net::addresses());
            self.append_log(&help);
            nwg::modal_info_message(&self.window, "How friends join", &help);
        } else if handle == &self.btn_steam.handle {
            open_url("steam://install/545990");
            self.append_log("Asked Steam to install \"Call of Duty: Black Ops III - Unranked Dedicated Server\" (free tool, about 5 GB).");
            self.append_log("If nothing opens, Steam is not installed: use \"Download with steamcmd\" instead. When Steam has finished, press \"Run setup\".");
        } else if handle == &self.btn_steamcmd.handle {
            self.steamcmd();
        } else if handle == &self.btn_logs.handle {
            let dir = paths::data_dir();
            let logs = dir.join("logs");
            open_in_explorer(if logs.is_dir() { &logs } else { &dir });
        } else if handle == &self.btn_open_dir.handle {
            let dir = PathBuf::from(self.server_dir.text());
            if dir.is_dir() {
                open_in_explorer(&dir);
            } else {
                self.append_log("The server folder does not exist yet. Run setup first.");
            }
        } else if handle == &self.btn_server_dir.handle {
            if let Some(dir) = self.pick_folder("Server folder") {
                *self.server_override.borrow_mut() = Some(dir);
                self.refresh();
            }
        } else if handle == &self.btn_game_dir.handle {
            if let Some(dir) = self.pick_folder("Black Ops III folder") {
                if !bsm_core::steam::is_game_dir(&dir) {
                    nwg::modal_error_message(
                        &self.window,
                        APP_TITLE,
                        "That folder has no \"zone\" folder, so it is not a Black Ops III install.",
                    );
                    return;
                }
                *self.game_override.borrow_mut() = Some(dir);
                self.refresh();
            }
        } else if handle == &self.btn_auto.handle {
            *self.server_override.borrow_mut() = None;
            *self.game_override.borrow_mut() = None;
            self.refresh();
        }
    }

    fn steamcmd(&self) {
        let user = self.steam_user.text().trim().to_string();
        if !steamcmd::valid_user(&user) {
            nwg::modal_error_message(&self.window, APP_TITLE, "Type your Steam login name into the \"Steam login name\" box first.\r\n\r\nOnly the name is needed here. The password is typed later, in steamcmd's own window.");
            return;
        }
        if self.server_override.borrow().is_some() {
            nwg::modal_error_message(
                &self.window,
                APP_TITLE,
                "steamcmd downloads into the default server folder. Press \"Automatic folders\" first.",
            );
            return;
        }
        let params = nwg::MessageParams {
            title: "Download with steamcmd",
            content: &format!("This downloads Valve's steamcmd, then the server files (Steam app 545990) with your Steam account \"{user}\".\r\n\r\n{}", steamcmd::LOGIN_HELP),
            buttons: nwg::MessageButtons::OkCancel,
            icons: nwg::MessageIcons::Info,
        };
        if nwg::modal_message(&self.window, &params) != nwg::MessageChoice::Ok {
            return;
        }
        let wake = self.notice.sender();
        self.run_job("Download server files with steamcmd", move |env, log, tx| {
            let exe = steamcmd::ensure(&env.data_dir, log)?;
            let root = env.data_dir.join("server");
            loop {
                log("steamcmd is running in its own window...");
                if let Some(dir) = steamcmd::run_once(&exe, &root, &env.settings.steam_user, true)? {
                    log(&format!(
                        "Server files downloaded to {}. Press \"Run setup\" to finish.",
                        dir.display()
                    ));
                    return Ok(());
                }
                let (reply_tx, reply_rx) = channel();
                let _ = tx.send(Msg::AskRetry(reply_tx));
                wake.notice();
                if !reply_rx.recv().unwrap_or(false) {
                    log("steamcmd download cancelled.");
                    return Ok(());
                }
            }
        });
    }

    fn on_notice(&self) {
        let Some(rx) = self.rx.as_ref() else { return };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Log(line) => self.append_log(&line),
                Msg::Refreshed { checks, mut maps, mut mods, server_dir, game_dir } => {
                    self.checklist.set_text(&checks);
                    self.server_dir.set_text(&server_dir);
                    self.game_dir.set_text(&game_dir);

                    // Keep what is selected, also when it is not installed (setup then says so).
                    let current_map = map_name(&self.map.selection_string().unwrap_or_default());
                    let selected = match maps.iter().find(|l| map_name(l) == current_map) {
                        Some(label) => label.clone(),
                        None => {
                            if !current_map.is_empty() {
                                maps.push(current_map.clone());
                            }
                            current_map
                        }
                    };
                    self.map.set_collection(maps);
                    self.map.set_selection_string(&selected);

                    let current_mod = mod_id(&self.mod_box.selection_string().unwrap_or_default());
                    mods.insert(0, NO_MOD.to_string());
                    let selected = if current_mod.is_empty() {
                        NO_MOD.to_string()
                    } else {
                        match mods.iter().find(|l| mod_id(l) == current_mod) {
                            Some(label) => label.clone(),
                            None => {
                                let label = format!("(not installed) [{current_mod}]");
                                mods.push(label.clone());
                                label
                            }
                        }
                    };
                    self.mod_box.set_collection(mods);
                    self.mod_box.set_selection_string(&selected);
                }
                Msg::Status { text, state, port } => {
                    self.status.set_text(&text);
                    self.last_port.set(port);
                    if self.last_state.get() != Some(state) {
                        if self.last_state.get().is_some() {
                            self.append_log(&text);
                        }
                        self.last_state.set(Some(state));
                    }
                }
                Msg::Error(e) => {
                    self.append_log(&format!("ERROR: {e}"));
                    nwg::modal_error_message(&self.window, APP_TITLE, &e);
                }
                Msg::Info(title, text) => {
                    self.append_log(&text);
                    nwg::modal_info_message(&self.window, &title, &text);
                }
                Msg::AskYesNo(title, text, reply) => {
                    let params = nwg::MessageParams {
                        title: &title,
                        content: &text,
                        buttons: nwg::MessageButtons::YesNo,
                        icons: nwg::MessageIcons::Question,
                    };
                    let _ = reply.send(nwg::modal_message(&self.window, &params) == nwg::MessageChoice::Yes);
                }
                Msg::Clipboard(text) => nwg::Clipboard::set_data_text(&self.window, &text),
                Msg::AskRetry(reply) => {
                    let params = nwg::MessageParams {
                        title: "steamcmd did not finish",
                        content: "The server files are not complete yet.\r\n\r\nMost common reason: the sign-in was not approved in the Steam mobile app in time, or the password was mistyped.\r\n\r\nRetry: get your phone ready first, then approve the sign-in as soon as steamcmd asks.",
                        buttons: nwg::MessageButtons::RetryCancel,
                        icons: nwg::MessageIcons::Warning,
                    };
                    let again = nwg::modal_message(&self.window, &params) == nwg::MessageChoice::Retry;
                    let _ = reply.send(again);
                }
                Msg::Done => self.set_busy(false),
            }
        }
    }
}

fn refresh_msg(settings: Settings) -> Msg {
    let env = Env::discover(settings);
    let found = env.content();
    Msg::Refreshed {
        checks: server::format_checks(&server::checks(&env)),
        maps: found.maps.iter().map(|m| m.label()).collect(),
        mods: found.mods.iter().map(|m| m.label()).collect(),
        server_dir: env.server_dir.display().to_string(),
        game_dir: env
            .game_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(not found)".into()),
    }
}

/// The program icon that `build.rs` embedded as resource 1.
fn embedded_icon() -> Option<nwg::Icon> {
    nwg::EmbedResource::load(None).ok()?.icon(1, None)
}

fn build() -> Result<Rc<App>, nwg::NwgError> {
    let data = paths::data_dir();
    let settings = Settings::load(&data);
    let mut app = App::default();
    let (tx, rx) = channel();
    app.tx = Some(tx);
    app.rx = Some(rx);
    app.last_port.set(settings.port);
    *app.server_override.borrow_mut() = settings.server_dir.clone();
    *app.game_override.borrow_mut() = settings.game_dir.clone();

    app.icon = embedded_icon();
    nwg::Window::builder()
        .icon(app.icon.as_ref())
        .size((940, 774))
        .position((200, 40))
        .title(&format!("{APP_TITLE} {}", bsm_core::VERSION))
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE | nwg::WindowFlags::MINIMIZE_BOX)
        .build(&mut app.window)?;
    nwg::Notice::builder().parent(&app.window).build(&mut app.notice)?;
    nwg::Font::builder().family("Consolas").size(15).build(&mut app.mono)?;
    nwg::FileDialog::builder()
        .title("Choose a folder")
        .action(nwg::FileDialogAction::OpenDirectory)
        .build(&mut app.folder_dialog)?;

    let w = &app.window;
    let mut labels = Vec::new();
    let mut label = |text: &str, x: i32, y: i32, width: i32| -> Result<(), nwg::NwgError> {
        let mut l = nwg::Label::default();
        nwg::Label::builder().text(text).position((x, y + 3)).size((width, 22)).parent(w).build(&mut l)?;
        labels.push(l);
        Ok(())
    };
    let input = |c: &mut nwg::TextInput, text: &str, x: i32, y: i32, width: i32, readonly: bool| {
        nwg::TextInput::builder()
            .text(text)
            .position((x, y))
            .size((width, 24))
            .readonly(readonly)
            .parent(w)
            .build(c)
    };
    let button = |c: &mut nwg::Button, text: &str, x: i32, y: i32, width: i32| {
        nwg::Button::builder().text(text).position((x, y)).size((width, 28)).parent(w).build(c)
    };

    // Folders
    label("Server folder", 10, 10, 110)?;
    input(&mut app.server_dir, "", 125, 10, 560, true)?;
    button(&mut app.btn_server_dir, "Change...", 693, 8, 90)?;
    button(&mut app.btn_open_dir, "Open folder", 791, 8, 139)?;
    label("Black Ops III", 10, 42, 110)?;
    input(&mut app.game_dir, "", 125, 42, 560, true)?;
    button(&mut app.btn_game_dir, "Change...", 693, 40, 90)?;
    button(&mut app.btn_auto, "Automatic folders", 791, 40, 139)?;

    let check = |c: &mut nwg::CheckBox, text: &str, on: bool, x: i32, y: i32, width: i32| {
        let state = if on { nwg::CheckBoxState::Checked } else { nwg::CheckBoxState::Unchecked };
        nwg::CheckBox::builder()
            .text(text)
            .check_state(state)
            .position((x, y))
            .size((width, 24))
            .parent(w)
            .build(c)
    };

    // Server settings
    label("Map", 10, 76, 110)?;
    nwg::ComboBox::builder()
        .collection(vec![settings.map.clone()])
        .selected_index(Some(0))
        .position((125, 75))
        .size((400, 24))
        .parent(w)
        .build(&mut app.map)?;
    label("UDP port", 545, 76, 70)?;
    input(&mut app.port, &settings.port.to_string(), 620, 75, 70, false)?;
    label("Max players (1-8)", 710, 76, 130)?;
    input(&mut app.players, &settings.max_players.to_string(), 845, 75, 40, false)?;

    label("Mod", 10, 108, 110)?;
    let first_mod =
        if settings.mod_id.is_empty() { NO_MOD.to_string() } else { format!("mod [{}]", settings.mod_id) };
    nwg::ComboBox::builder()
        .collection(vec![first_mod])
        .selected_index(Some(0))
        .position((125, 107))
        .size((400, 24))
        .parent(w)
        .build(&mut app.mod_box)?;
    label("Password", 545, 108, 70)?;
    input(&mut app.password, &settings.password, 620, 107, 170, false)?;
    label("(empty = none)", 800, 108, 130)?;

    label("Server name", 10, 140, 110)?;
    input(&mut app.name, &settings.server_name, 125, 139, 400, false)?;

    label("Steam login name", 10, 172, 110)?;
    input(&mut app.steam_user, &settings.steam_user, 125, 171, 260, false)?;
    label("Only for \"Download with steamcmd\". Your password is never typed here.", 405, 172, 525)?;

    // Mod menu and start options
    label("Mod menu", 10, 204, 110)?;
    check(&mut app.menu, "In-game mod menu (AIM + MELEE)", settings.menu, 125, 203, 240)?;
    label("Only for players:", 385, 204, 110)?;
    input(&mut app.menu_players, &settings.menu_players, 500, 203, 180, false)?;
    label("(names, comma separated; empty = all)", 690, 204, 240)?;

    label("For everyone", 10, 236, 110)?;
    check(&mut app.god, "God mode", settings.god_all, 125, 235, 100)?;
    check(&mut app.ammo, "Infinite ammo", settings.ammo_all, 235, 235, 120)?;
    check(&mut app.perks, "All perks at start", settings.start_perks, 365, 235, 150)?;
    label("Start points", 545, 236, 80)?;
    input(&mut app.start_points, &settings.start_points.to_string(), 630, 235, 90, false)?;
    label("(0 = normal start)", 730, 236, 200)?;

    // Checklist
    nwg::TextBox::builder()
        .text("Checking this PC...")
        .position((10, 268))
        .size((920, 176))
        .readonly(true)
        .font(Some(&app.mono))
        .flags(nwg::TextBoxFlags::VISIBLE | nwg::TextBoxFlags::VSCROLL | nwg::TextBoxFlags::AUTOVSCROLL)
        .parent(w)
        .build(&mut app.checklist)?;

    // Actions
    button(&mut app.btn_setup, "1. Run setup", 10, 452, 130)?;
    button(&mut app.btn_start, "2. Start server", 148, 452, 130)?;
    button(&mut app.btn_play, "3. Play", 286, 452, 110)?;
    button(&mut app.btn_stop, "Stop server", 404, 452, 120)?;
    button(&mut app.btn_copy, "Copy connect command", 532, 452, 180)?;
    button(&mut app.btn_join, "How friends join", 720, 452, 150)?;
    button(&mut app.btn_steam, "Install server through Steam", 10, 486, 210)?;
    button(&mut app.btn_steamcmd, "Download with steamcmd", 230, 486, 190)?;
    button(&mut app.btn_refresh, "Check again", 430, 486, 110)?;
    button(&mut app.btn_logs, "Open log folder", 550, 486, 130)?;
    button(&mut app.btn_repair, "Repair files", 690, 486, 150)?;

    nwg::Label::builder()
        .text("Server: checking...")
        .position((10, 524))
        .size((920, 22))
        .parent(w)
        .build(&mut app.status)?;

    nwg::TextBox::builder()
        .text("")
        .position((10, 550))
        .size((920, 214))
        .readonly(true)
        .font(Some(&app.mono))
        .flags(nwg::TextBoxFlags::VISIBLE | nwg::TextBoxFlags::VSCROLL | nwg::TextBoxFlags::AUTOVSCROLL)
        .parent(w)
        .build(&mut app.log)?;
    app.log.set_limit(1_000_000);

    app.labels = labels;
    Ok(Rc::new(app))
}

/// Reports the server state every two seconds, also when it was started or
/// stopped from somewhere else.
fn start_status_poller(tx: Sender<Msg>, wake: nwg::NoticeSender) {
    std::thread::spawn(move || loop {
        let data = paths::data_dir();
        let env = Env::new(Settings::load(&data), data, Vec::new());
        let st = server::status(&env);
        if tx.send(Msg::Status { text: st.summary(), state: st.state, port: st.port }).is_err() {
            break;
        }
        wake.notice();
        std::thread::sleep(Duration::from_secs(2));
    });
}

/// Segoe UI at the normal Windows dialog size, for every control.
fn set_ui_font() {
    let mut font = nwg::Font::default();
    if nwg::Font::builder().family("Segoe UI").size(16).build(&mut font).is_ok() {
        nwg::Font::set_global_default(Some(font));
    }
}

fn main() {
    if let Err(e) = nwg::init() {
        nwg::fatal_message(APP_TITLE, &format!("Could not start the user interface: {e}"));
    }
    set_ui_font();
    let app = match build() {
        Ok(app) => app,
        Err(e) => nwg::fatal_message(APP_TITLE, &format!("Could not build the window: {e}")),
    };

    let handler_app = app.clone();
    let handler = nwg::full_bind_event_handler(&app.window.handle, move |event, _data, handle| match event {
        nwg::Event::OnButtonClick => handler_app.on_click(&handle),
        nwg::Event::OnNotice => handler_app.on_notice(),
        nwg::Event::OnWindowClose if handle == handler_app.window.handle => {
            nwg::stop_thread_dispatch();
        }
        _ => {}
    });

    app.append_log(&format!(
        "{APP_TITLE} {} - data folder: {}",
        bsm_core::VERSION,
        paths::data_dir().display()
    ));
    app.append_log("Press \"1. Run setup\", then \"2. Start server\", then \"3. Play\" to open the game on this PC. Setup can be run again at any time.");
    start_status_poller(app.tx(), app.notice.sender());
    app.refresh();

    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&handler);
}
