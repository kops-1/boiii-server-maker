//! [`Env`]: where everything is on this PC, together with the user's settings.

use super::*;

/// Everything discovered about this PC plus the user's settings.
#[derive(Debug, Clone)]
pub struct Env {
    pub settings: Settings,
    pub data_dir: PathBuf,
    pub libraries: Vec<PathBuf>,
    /// Black Ops III folder (source of the Zombies files).
    pub game_dir: Option<PathBuf>,
    /// Dedicated server installed through the Steam client, if any.
    pub steam_server_dir: Option<PathBuf>,
    /// Folder the server runs from.
    pub server_dir: PathBuf,
}

impl Env {
    /// Looks at the real PC.
    pub fn discover(settings: Settings) -> Env {
        Env::new(settings, crate::paths::data_dir(), steam::libraries())
    }

    pub fn new(settings: Settings, data_dir: PathBuf, libraries: Vec<PathBuf>) -> Env {
        let game_dir = settings
            .game_dir
            .clone()
            .filter(|p| steam::is_game_dir(p))
            .or_else(|| steam::find_game(&libraries));
        let steam_server_dir = steam::find_steam_server(&libraries);
        let server_dir = settings.server_dir.clone().unwrap_or_else(|| default_server_dir(&data_dir));
        Env { settings, data_dir, libraries, game_dir, steam_server_dir, server_dir }
    }

    pub fn client_exe(&self) -> PathBuf {
        self.server_dir.join(CLIENT_EXE)
    }
    pub fn zone_dir(&self) -> PathBuf {
        self.server_dir.join("zone")
    }
    pub fn cfg_path(&self) -> PathBuf {
        self.zone_dir().join(CFG_NAME)
    }
    pub fn run_file(&self) -> PathBuf {
        self.data_dir.join("server-run.json")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
    pub fn has_server_files(&self) -> bool {
        steam::is_server_dir(&self.server_dir)
    }
    /// Official and workshop maps, and mods, found on this PC.
    pub fn content(&self) -> Content {
        content::scan(self.game_dir.as_deref(), &self.zone_dir(), &self.libraries)
    }
}

/// `<data>\server\UnrankedServer`: the same layout steamcmd produces.
pub fn default_server_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("server").join(SERVER_SUBDIR)
}
