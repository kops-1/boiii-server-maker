//! `BoiiiServerMaker.exe`: the main window.
//!
//! Drawn with egui, so it looks the same on every PC. If the graphics
//! driver cannot run it, the classic window (`BoiiiServerMakerClassic.exe`)
//! is started instead.
//!
//! How the pieces fit together:
//!
//! * [`App`] (this file) holds all window state and is drawn once per frame.
//! * `pages/` draws the sidebar, the status header and one module per tab.
//!   Drawing never does slow work: buttons only push an [`actions::Action`].
//! * `actions.rs` carries those actions out after the frame is drawn.
//! * `background.rs` runs slow work (setup, start, network checks) on worker
//!   threads, which report back through [`background::Message`]s.
#![windows_subsystem = "windows"]

mod actions;
mod background;
mod overlay;
mod pages;
mod shell;
mod theme;
mod thumbs;
mod widgets;

use actions::Action;
use background::Message;
use bsm_core::content::Item;
use bsm_core::netcheck::Level;
use bsm_core::saves::Save;
use bsm_core::scores::{self, Game};
use bsm_core::server::{self, Check, State, Status};
use bsm_core::settings::Settings;
use bsm_core::{net, paths, APP_TITLE, VERSION};
use eframe::egui::{self, RichText};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;
use theme::{apply_style, BG, WARN};

/// The window icon; the same artwork is embedded in the .exe by `build.rs`.
const ICON_PNG: &[u8] = include_bytes!("../../../../assets/branding/icon.png");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Host,
    Cheats,
    Friends,
    Setup,
    Scores,
    Log,
}

/// What a fresh look at the PC found.
#[derive(Default, Clone)]
struct Discovery {
    checks: Vec<Check>,
    maps: Vec<Item>,
    mods: Vec<Item>,
    server_dir: PathBuf,
    game_dir: Option<PathBuf>,
    /// Server files, client and Zombies files are in place.
    ready: bool,
}

impl Discovery {
    /// The display title of a map ("Shadows of Evil" for `zm_zod`), or the
    /// internal name when that map is not installed.
    fn map_title(&self, name: &str) -> String {
        match self.maps.iter().find(|m| m.name == name) {
            Some(map) => map.title.clone(),
            None => name.to_string(),
        }
    }
}

/// A question or notice shown over the window.
struct Dialog {
    title: String,
    text: String,
    yes: &'static str,
    no: Option<&'static str>,
    reply: Option<Sender<bool>>,
}

impl Dialog {
    /// A message with a single OK button.
    fn notice(title: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into().replace("\r\n", "\n");
        Dialog { title: title.into(), text, yes: "OK", no: None, reply: None }
    }
}

struct App {
    /// Settings as edited in the window.
    settings: Settings,
    /// Settings as last written to disk; compared with `settings` to autosave.
    saved: Settings,
    /// Why the edited settings cannot be saved yet, shown under the page.
    settings_error: Option<String>,

    tab: Tab,
    discovery: Discovery,
    /// Set once the first look at the PC has come back.
    discovered: bool,
    status: Option<Status>,
    /// Last server state seen, so state changes can be written to the log.
    last_state: Option<State>,
    log: Vec<String>,
    /// Title of the job running on a worker thread, if any.
    busy: Option<String>,

    /// Worker threads send their results here.
    tx: Sender<Message>,
    rx: Receiver<Message>,
    dialog: Option<Dialog>,
    toast: Option<(String, Instant)>,

    net_findings: Option<Vec<(Level, String)>>,
    net_check_running: bool,
    /// Map name -> picture file (`None` while there is none, or none yet).
    pictures: HashMap<String, Option<PathBuf>>,
    preset_name: String,
    preset_pick: String,
    rotation_pick: String,
    scores: Vec<Game>,
    saves: Vec<Save>,
    /// Actions named on the command line (`--do start`), carried out once the window is ready.
    queued: Vec<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_style(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let data = paths::data_dir();
        let settings = Settings::load(&data);
        let (tx, rx) = channel();
        // BSM_START_TAB opens a given page first (used for screenshots and support).
        let tab = match std::env::var("BSM_START_TAB").unwrap_or_default().to_lowercase().as_str() {
            "cheats" => Tab::Cheats,
            "friends" => Tab::Friends,
            "setup" => Tab::Setup,
            "scores" => Tab::Scores,
            "log" => Tab::Log,
            _ => Tab::Host,
        };
        let mut app = App {
            saved: settings.clone(),
            settings,
            tab,
            discovery: Discovery::default(),
            discovered: false,
            status: None,
            last_state: None,
            log: vec![format!("{APP_TITLE} {VERSION} - data folder: {}", data.display())],
            busy: None,
            tx,
            rx,
            dialog: None,
            toast: None,
            net_findings: None,
            net_check_running: false,
            pictures: HashMap::new(),
            preset_name: String::new(),
            preset_pick: String::new(),
            settings_error: None,
            scores: scores::best(scores::load(&data)),
            rotation_pick: String::new(),
            saves: Vec::new(),
            queued: {
                let args: Vec<String> = std::env::args().collect();
                args.windows(2).filter(|w| w[0] == "--do").map(|w| w[1].to_lowercase()).collect()
            },
        };
        app.start_status_poller(cc.egui_ctx.clone());
        app.refresh(&cc.egui_ctx);
        app
    }

    fn running(&self) -> bool {
        self.status.as_ref().map(|s| s.state != State::Stopped).unwrap_or(false)
    }

    fn server_port(&self) -> u16 {
        self.status
            .as_ref()
            .filter(|s| s.state != State::Stopped)
            .map(|s| s.port)
            .unwrap_or(self.settings.port)
    }

    /// The command a friend pastes into the game console.
    fn invite(&self) -> String {
        let port = self.server_port();
        if let Some(tunnel) = self.settings.tunnel_address() {
            return if self.settings.password.is_empty() {
                format!("connect {tunnel}")
            } else {
                format!("password {}; connect {tunnel}", self.settings.password)
            };
        }
        let addrs = net::addresses();
        let ip = net::preferred(&addrs).map(|a| a.ip.to_string()).unwrap_or_else(|| "127.0.0.1".into());
        net::connect_command(&ip, port, &self.settings.password)
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    /// Saves the settings shortly after they change, when they are valid.
    fn autosave(&mut self) {
        if self.settings == self.saved || self.busy.is_some() {
            return;
        }
        let mut s = self.settings.clone();
        s.sanitize();
        match server::validate_settings(&s) {
            Ok(()) => {
                self.settings_error = None;
                if s.save(&paths::data_dir()).is_ok() {
                    self.saved = self.settings.clone();
                }
            }
            Err(e) => self.settings_error = Some(e.to_string()),
        }
    }

    /// `BoiiiServerMaker.exe --do start` (also stop, setup, netcheck, favorite),
    /// for shortcuts and scripts. Runs one queued action whenever the window is idle.
    fn run_queued_action(&mut self, ctx: &egui::Context) {
        let idle = self.discovered && self.busy.is_none() && self.dialog.is_none();
        if !idle || self.queued.is_empty() {
            return;
        }
        let action = match self.queued.remove(0).as_str() {
            "start" => Action::Start,
            "stop" => Action::Stop,
            "setup" => Action::Setup,
            "netcheck" => Action::NetCheck,
            "favorite" => Action::Favorite,
            _ => return,
        };
        self.perform(ctx, action);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages(ctx);
        let mut actions = Vec::new();

        self.sidebar(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                ui.add_enabled_ui(self.dialog.is_none(), |ui| {
                    self.header(ui, &mut actions);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        match self.tab {
                            Tab::Host => self.host_page(ui, ctx, &mut actions),
                            Tab::Cheats => self.cheats_page(ui, &mut actions),
                            Tab::Friends => self.friends_page(ui, &mut actions),
                            Tab::Setup => self.setup_page(ui, &mut actions),
                            Tab::Scores => self.scores_page(ui),
                            Tab::Log => self.log_page(ui, &mut actions),
                        }
                        if let Some(e) = &self.settings_error {
                            ui.label(RichText::new(e).color(WARN));
                        }
                    });
                });
            });
        self.draw_overlays(ctx);

        for action in actions {
            self.perform(ctx, action);
        }
        self.run_queued_action(ctx);
        self.autosave();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let mut s = self.settings.clone();
        s.sanitize();
        if server::validate_settings(&s).is_ok() {
            let _ = s.save(&paths::data_dir());
        }
    }
}

fn window_icon() -> Option<Arc<egui::IconData>> {
    let image = image::load_from_memory(ICON_PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(Arc::new(egui::IconData { rgba: image.into_raw(), width, height }))
}

/// Starts the classic window when this one cannot be shown.
fn fall_back_to_classic() {
    let classic = paths::exe_dir().join("BoiiiServerMakerClassic.exe");
    if classic.is_file() {
        let _ = std::process::Command::new(classic).spawn();
    }
}

fn main() {
    if std::env::args().any(|a| a == "--classic") {
        fall_back_to_classic();
        return;
    }
    // Try DirectX/Vulkan first (works with a broken or basic display driver
    // and over Remote Desktop), then OpenGL, then the classic window.
    let mut errors = Vec::new();
    for renderer in [eframe::Renderer::Wgpu, eframe::Renderer::Glow] {
        let mut viewport = egui::ViewportBuilder::default()
            .with_title(format!("{APP_TITLE} {VERSION}"))
            .with_inner_size([1060.0, 760.0])
            .with_min_inner_size([940.0, 620.0]);
        if let Some(icon) = window_icon() {
            viewport = viewport.with_icon(icon);
        }
        let options = eframe::NativeOptions { renderer, viewport, ..Default::default() };
        match eframe::run_native(APP_TITLE, options, Box::new(|cc| Ok(Box::new(App::new(cc))))) {
            Ok(()) => return,
            Err(e) => errors.push(format!("{renderer:?}: {e}")),
        }
    }
    // Keep the reason, so a support question can be answered.
    let _ = std::fs::write(
        paths::data_dir().join("window-error.txt"),
        format!(
            "The main window could not be shown.\r\n{}\r\nThe classic window was started instead.\r\n",
            errors.join("\r\n")
        ),
    );
    fall_back_to_classic();
}
