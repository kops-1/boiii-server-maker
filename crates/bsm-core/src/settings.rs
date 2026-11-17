use crate::error::{io_ctx, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// User choices. Folders are only stored when the user picked one by hand;
/// otherwise they are discovered again on every run, so the settings file
/// stays valid when Steam or the tool is moved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Black Ops III folder chosen by the user (None = find it through Steam).
    pub game_dir: Option<PathBuf>,
    /// Server folder chosen by the user (None = `<data>\server\UnrankedServer`).
    pub server_dir: Option<PathBuf>,
    /// UDP port of the server. A game client on the same PC needs a different one.
    pub port: u16,
    pub map: String,
    pub server_name: String,
    /// Empty = anyone with the address can join.
    pub password: String,
    pub max_players: u8,
    /// Steam account name for the steamcmd fallback. The password is never stored.
    pub steam_user: String,
    /// Workshop id of the mod to load. Empty = no mod.
    pub mod_id: String,
    /// In-game mod menu (hold AIM and press MELEE).
    pub menu: bool,
    /// Player names allowed to open the menu, separated by commas. Empty = everyone.
    pub menu_players: String,
    /// God mode for every player.
    pub god_all: bool,
    /// Infinite ammo for every player.
    pub ammo_all: bool,
    /// Points every player has at least on first spawn. 0 = normal start.
    pub start_points: u32,
    /// Every player starts with all perks of the map.
    pub start_perks: bool,
    /// Every zombie dies in one hit, for the whole game.
    pub instakill_all: bool,
    /// Every player earns double points, for the whole game.
    pub double_points: bool,
    /// Players keep their perks when they go down.
    pub keep_perks: bool,
    /// Public address of a Playit (or other) UDP tunnel, `host:port`. Empty = none.
    pub playit_address: String,
    /// Hand out the tunnel address in invites instead of this PC's address.
    pub use_playit: bool,
    /// Saved combinations of map, mod and cheats, by name.
    pub presets: BTreeMap<String, Preset>,
    /// Maps played one after another. Empty = only `map`. When a game ends
    /// the server loads the next one and starts again at the top after the last.
    pub rotation: Vec<String>,
    /// Write each finished game (round, kills, points per player) to the scoreboard.
    pub scoreboard: bool,
    /// Save the game to the slot "auto" at the start of every round.
    pub autosave: bool,
    /// Slot to continue from at the next server start (1, 2, 3 or auto). Empty = new game.
    /// Used once: the server start clears it.
    pub continue_save: String,
}

/// What a preset remembers: the game to host, not the server's identity
/// (name, password, port stay as they are).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Preset {
    pub map: String,
    pub mod_id: String,
    pub menu: bool,
    pub menu_players: String,
    pub god_all: bool,
    pub ammo_all: bool,
    pub start_points: u32,
    pub start_perks: bool,
    pub instakill_all: bool,
    pub double_points: bool,
    pub keep_perks: bool,
    pub rotation: Vec<String>,
}

impl Settings {
    /// The maps the server plays, in order: the rotation, or just the chosen map.
    pub fn play_list(&self) -> Vec<String> {
        if self.rotation.is_empty() {
            vec![self.map.clone()]
        } else {
            self.rotation.clone()
        }
    }

    /// The current game choices as a preset.
    pub fn to_preset(&self) -> Preset {
        Preset {
            map: self.map.clone(),
            mod_id: self.mod_id.clone(),
            menu: self.menu,
            menu_players: self.menu_players.clone(),
            god_all: self.god_all,
            ammo_all: self.ammo_all,
            start_points: self.start_points,
            start_perks: self.start_perks,
            instakill_all: self.instakill_all,
            double_points: self.double_points,
            keep_perks: self.keep_perks,
            rotation: self.rotation.clone(),
        }
    }

    pub fn apply_preset(&mut self, p: &Preset) {
        self.map = p.map.clone();
        self.mod_id = p.mod_id.clone();
        self.menu = p.menu;
        self.menu_players = p.menu_players.clone();
        self.god_all = p.god_all;
        self.ammo_all = p.ammo_all;
        self.start_points = p.start_points;
        self.start_perks = p.start_perks;
        self.instakill_all = p.instakill_all;
        self.double_points = p.double_points;
        self.keep_perks = p.keep_perks;
        self.rotation = p.rotation.clone();
        self.sanitize();
    }

    /// Saves the current game choices under `name`. Returns false for an empty name.
    pub fn save_preset(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || name.len() > 60 {
            return false;
        }
        let preset = self.to_preset();
        self.presets.insert(name.to_string(), preset);
        true
    }
}

/// `" Ali , BOB,, "` becomes `"ali,bob"`: the form the menu script compares names with.
pub fn normalize_players(list: &str) -> String {
    list.split(',').map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty()).collect::<Vec<_>>().join(",")
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            game_dir: None,
            server_dir: None,
            port: 27017,
            map: "zm_zod".into(),
            server_name: "BOIII Zombies Server".into(),
            password: String::new(),
            max_players: 4,
            steam_user: String::new(),
            mod_id: String::new(),
            menu: false,
            menu_players: String::new(),
            god_all: false,
            ammo_all: false,
            start_points: 0,
            start_perks: false,
            instakill_all: false,
            double_points: false,
            keep_perks: false,
            playit_address: String::new(),
            use_playit: false,
            presets: BTreeMap::new(),
            rotation: Vec::new(),
            scoreboard: true,
            autosave: false,
            continue_save: String::new(),
        }
    }
}

impl Settings {
    pub fn file(data_dir: &Path) -> PathBuf {
        data_dir.join("settings.json")
    }

    /// Missing file gives defaults. An unreadable file is kept as
    /// `settings.json.corrupt` and replaced by defaults so the tool still starts.
    pub fn load(data_dir: &Path) -> Settings {
        let path = Self::file(data_dir);
        let Ok(text) = fs::read_to_string(&path) else {
            return Settings::default();
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(mut s) => {
                s.sanitize();
                s
            }
            Err(_) => {
                let _ = fs::copy(&path, path.with_extension("json.corrupt"));
                Settings::default()
            }
        }
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        io_ctx(fs::create_dir_all(data_dir), "create", data_dir)?;
        let path = Self::file(data_dir);
        let text = serde_json::to_string_pretty(self)?;
        io_ctx(fs::write(&path, text), "write", &path)
    }

    pub fn sanitize(&mut self) {
        if self.port < 1024 {
            self.port = 27017;
        }
        if self.max_players == 0 || self.max_players > 8 {
            self.max_players = 4;
        }
        if self.map.trim().is_empty() {
            self.map = "zm_zod".into();
        }
        if self.server_name.trim().is_empty() {
            self.server_name = "BOIII Zombies Server".into();
        }
        self.map = self.map.trim().to_lowercase();
        self.mod_id = self.mod_id.trim().to_string();
        self.menu_players = normalize_players(&self.menu_players);
        self.start_points = self.start_points.min(10_000_000);
        if !crate::saves::valid_slot(self.continue_save.trim()) {
            self.continue_save.clear();
        }
        self.rotation = self
            .rotation
            .iter()
            .map(|m| m.trim().to_lowercase())
            .filter(|m| !m.is_empty())
            .take(30)
            .collect();
        // With a rotation, the first map is the one the server starts on.
        if let Some(first) = self.rotation.first() {
            self.map = first.clone();
        }
        // A tunnel address that is not host:port is dropped rather than handed to friends.
        self.playit_address = crate::netcheck::normalize_tunnel(&self.playit_address).unwrap_or_default();
        if self.playit_address.is_empty() {
            self.use_playit = false;
        }
    }

    /// True if the menu script has anything to do.
    pub fn any_script_option(&self) -> bool {
        self.menu
            || self.god_all
            || self.ammo_all
            || self.start_perks
            || self.start_points > 0
            || self.instakill_all
            || self.double_points
            || self.keep_perks
            || self.scoreboard
            || self.autosave
            || !self.continue_save.is_empty()
    }

    /// Address to hand to friends through the tunnel, if one is set and switched on.
    pub fn tunnel_address(&self) -> Option<&str> {
        (self.use_playit && !self.playit_address.is_empty()).then_some(self.playit_address.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        let s = Settings { port: 27030, map: "zm_factory".into(), ..Default::default() };
        s.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path()), s);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(Settings::file(dir.path()), "{ not json").unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        assert!(dir.path().join("settings.json.corrupt").exists());
    }

    #[test]
    fn bad_values_are_repaired() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(Settings::file(dir.path()), r#"{"port": 80, "max_players": 99, "map": ""}"#).unwrap();
        let s = Settings::load(dir.path());
        assert_eq!((s.port, s.max_players, s.map.as_str()), (27017, 4, "zm_zod"));
    }

    #[test]
    fn presets_remember_the_game_but_not_the_server_identity() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings {
            map: "zm_town".into(),
            mod_id: "123".into(),
            menu: true,
            double_points: true,
            start_points: 5000,
            ..Default::default()
        };
        assert!(!s.save_preset("  "));
        assert!(s.save_preset(" Town with cheats "));

        s.apply_preset(&Preset { map: "ZM_ZOD ".into(), ..Preset::default() });
        s.server_name = "My server".into();
        s.port = 27030;
        assert_eq!(
            (s.map.as_str(), s.mod_id.as_str(), s.menu, s.double_points, s.start_points),
            ("zm_zod", "", false, false, 0)
        );

        s.save(dir.path()).unwrap();
        let mut loaded = Settings::load(dir.path());
        let preset = loaded.presets.get("Town with cheats").cloned().expect("preset survives save and load");
        loaded.apply_preset(&preset);
        assert_eq!(
            (
                loaded.map.as_str(),
                loaded.mod_id.as_str(),
                loaded.menu,
                loaded.double_points,
                loaded.start_points
            ),
            ("zm_town", "123", true, true, 5000)
        );
        assert_eq!(
            (loaded.server_name.as_str(), loaded.port),
            ("My server", 27030),
            "name and port are not part of a preset"
        );
        assert!(loaded.any_script_option());
    }

    fn s_rotation_check() {
        let mut s =
            Settings { rotation: vec![" ZM_Town ".into(), "".into(), "zm_zod".into()], ..Default::default() };
        s.sanitize();
        assert_eq!(s.play_list(), vec!["zm_town".to_string(), "zm_zod".to_string()]);
        assert_eq!(s.map, "zm_town", "the server starts on the first map of the rotation");
    }

    #[test]
    fn tunnel_address_is_validated() {
        let mut s = Settings {
            playit_address: " Some-Name.joinmc.link:12345 ".into(),
            use_playit: true,
            ..Default::default()
        };
        s.sanitize();
        assert_eq!(s.tunnel_address(), Some("some-name.joinmc.link:12345"));
        s.use_playit = false;
        assert_eq!(s.tunnel_address(), None);
        s.use_playit = true;
        s.playit_address = "not an address".into();
        s.sanitize();
        assert_eq!((s.playit_address.as_str(), s.use_playit, s.tunnel_address()), ("", false, None));
    }

    #[test]
    fn old_settings_files_still_load_and_new_fields_are_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        // A file written before maps/mods/menu existed.
        fs::write(Settings::file(dir.path()), r#"{"port": 27030, "map": "zm_zod", "server_name": "Old"}"#)
            .unwrap();
        let s = Settings::load(dir.path());
        assert_eq!((s.port, s.menu, s.mod_id.as_str(), s.start_points), (27030, false, "", 0));
        assert!(s.scoreboard && s.any_script_option(), "the scoreboard is on by default");
        let mut off = s.clone();
        off.scoreboard = false;
        assert!(!off.any_script_option());
        assert_eq!(s.play_list(), vec!["zm_zod".to_string()]);
        s_rotation_check();

        fs::write(Settings::file(dir.path()), r#"{"map": " ZM_Town ", "menu_players": " Ali , BOB,, ", "start_points": 4000000000, "menu": true}"#).unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(
            (s.map.as_str(), s.menu_players.as_str(), s.start_points),
            ("zm_town", "ali,bob", 10_000_000)
        );
        assert!(s.any_script_option());
    }
}
