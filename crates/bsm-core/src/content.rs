//! Maps and mods the server can run: the official Zombies maps that are
//! installed, plus Steam Workshop maps and mods found on this PC.

use crate::error::{Error, Result};
use crate::fsutil;
use crate::paths::norm;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const OFFICIAL_MAPS: [(&str, &str); 14] = [
    ("zm_zod", "Shadows of Evil"),
    ("zm_factory", "The Giant"),
    ("zm_castle", "Der Eisendrache"),
    ("zm_island", "Zetsubou No Shima"),
    ("zm_stalingrad", "Gorod Krovi"),
    ("zm_genesis", "Revelations"),
    ("zm_cosmodrome", "Ascension"),
    ("zm_theater", "Kino der Toten"),
    ("zm_moon", "Moon"),
    ("zm_prototype", "Nacht der Untoten"),
    ("zm_tomb", "Origins"),
    ("zm_temple", "Shangri-La"),
    ("zm_sumpf", "Shi No Numa"),
    ("zm_asylum", "Verruckt"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    OfficialMap,
    WorkshopMap,
    Mod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    /// Steam Workshop id; empty for official maps.
    pub id: String,
    /// Name the game uses: the map name (`zm_town`) or the mod's folder name.
    pub name: String,
    /// Title for people.
    pub title: String,
    /// Folder holding the item's files (`workshop.json`, `.ff`, ...). For
    /// official maps: the `zone` folder the map was found in.
    pub path: PathBuf,
}

impl Item {
    /// Text for lists: `zm_town - TOWN REIMAGINED (workshop)`.
    pub fn label(&self) -> String {
        match self.kind {
            Kind::OfficialMap => format!("{} - {}", self.name, self.title),
            Kind::WorkshopMap => format!("{} - {} (workshop)", self.name, self.title),
            Kind::Mod => format!("{} [{}]", self.title, self.id),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Content {
    pub maps: Vec<Item>,
    pub mods: Vec<Item>,
}

impl Content {
    pub fn map(&self, name: &str) -> Option<&Item> {
        self.maps.iter().find(|m| m.name.eq_ignore_ascii_case(name))
    }
    pub fn mod_by_id(&self, id: &str) -> Option<&Item> {
        self.mods.iter().find(|m| m.id == id)
    }
    pub fn workshop_items(&self) -> impl Iterator<Item = &Item> {
        self.maps.iter().filter(|m| m.kind == Kind::WorkshopMap).chain(self.mods.iter())
    }
}

/// Removes the game's colour codes (`^1`, `^7`, ...) from a title.
pub fn clean_title(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().map(|d| d.is_ascii_digit()).unwrap_or(false) {
            chars.next();
            continue;
        }
        if !c.is_control() {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn valid_folder_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn json_text(v: &serde_json::Value, key: &str) -> String {
    match v.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Reads one workshop item folder. None if it is not a usable Zombies map or mod.
fn read_workshop_item(dir: &Path) -> Option<Item> {
    let (json_path, content_dir) = [dir.join("workshop.json"), dir.join("zone").join("workshop.json")]
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| {
            let d = p.parent().unwrap_or(dir).to_path_buf();
            (p, d)
        })?;
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(json_path).ok()?).ok()?;
    let id = json_text(&v, "PublisherID");
    let name = json_text(&v, "FolderName");
    let kind = json_text(&v, "Type").to_lowercase();
    if !valid_folder_name(&name) || id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut title = clean_title(&json_text(&v, "Title"));
    if title.is_empty() {
        title = name.clone();
    }
    match kind.as_str() {
        // Only Zombies maps: this tool starts the zclassic game type.
        "map"
            if name.to_lowercase().starts_with("zm_") && content_dir.join(format!("{name}.ff")).is_file() =>
        {
            Some(Item { kind: Kind::WorkshopMap, id, name: name.to_lowercase(), title, path: content_dir })
        }
        "mod" if content_dir.join("core_mod.ff").is_file() => {
            Some(Item { kind: Kind::Mod, id, name, title, path: content_dir })
        }
        _ => None,
    }
}

/// Everything the server could run.
///
/// * Official maps whose `.ff` is in the server's or the game's `zone` folder.
/// * Workshop maps and mods in the game's `usermaps` / `mods` folders and in
///   the workshop folder of every Steam library.
pub fn scan(game_dir: Option<&Path>, server_zone: &Path, libraries: &[PathBuf]) -> Content {
    let mut content = Content::default();

    let mut zones = vec![server_zone.to_path_buf()];
    if let Some(g) = game_dir {
        zones.push(g.join("zone"));
    }
    for (name, title) in OFFICIAL_MAPS {
        if let Some(zone) = zones.iter().find(|z| z.join(format!("{name}.ff")).is_file()) {
            content.maps.push(Item {
                kind: Kind::OfficialMap,
                id: String::new(),
                name: name.into(),
                title: title.into(),
                path: zone.clone(),
            });
        }
    }

    let mut roots = Vec::new();
    if let Some(g) = game_dir {
        roots.push(g.join("usermaps"));
        roots.push(g.join("mods"));
    }
    for lib in libraries {
        roots.push(
            lib.join("steamapps").join("workshop").join("content").join(crate::steam::APP_BO3.to_string()),
        );
    }
    let mut seen = HashSet::new();
    let mut workshop = Vec::new();
    for root in roots {
        let Ok(rd) = fs::read_dir(&root) else { continue };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for dir in dirs {
            if let Some(item) = read_workshop_item(&dir) {
                if seen.insert(item.id.clone()) && !content.maps.iter().any(|m| m.name == item.name) {
                    workshop.push(item);
                }
            }
        }
    }
    workshop.sort_by_key(|a| a.title.to_lowercase());
    for item in workshop {
        match item.kind {
            Kind::Mod => content.mods.push(item),
            _ => content.maps.push(item),
        }
    }
    content
}

/// Where a workshop item is linked inside the server folder:
/// `usermaps\<id>\zone` or `mods\<id>\zone`.
pub fn server_link(server_dir: &Path, item: &Item) -> PathBuf {
    let parent = if item.kind == Kind::Mod { "mods" } else { "usermaps" };
    server_dir.join(parent).join(&item.id).join("zone")
}

/// Makes every workshop map and mod reachable from the server folder through
/// a link. The content itself stays where Steam put it; nothing is copied.
/// Returns how many links were created.
pub fn link_workshop(server_dir: &Path, content: &Content, log: &dyn Fn(&str)) -> u32 {
    let mut created = 0;
    for item in content.workshop_items() {
        let link = server_link(server_dir, item);
        if fs::symlink_metadata(&link).is_ok() {
            // Already there (a link from before, or a real folder the user made).
            if let Some(target) = fsutil::link_target(&link) {
                if norm(&target) != norm(&item.path) && !target.is_dir() {
                    log(&format!(
                        "  note: {} points to a folder that no longer exists ({}).",
                        link.display(),
                        target.display()
                    ));
                }
            }
            continue;
        }
        match fsutil::make_junction(&item.path, &link) {
            Ok(()) => created += 1,
            Err(e) => log(&format!("  note: could not link \"{}\": {e}", item.title)),
        }
    }
    created
}

/// What the server is asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The map the server starts on.
    pub map: Item,
    pub mod_item: Option<Item>,
    /// Further maps of the rotation, played after `map`.
    pub more: Vec<Item>,
}

impl Selection {
    /// Value of `fs_game` on the command line.
    pub fn all_maps(&self) -> impl Iterator<Item = &Item> {
        std::iter::once(&self.map).chain(self.more.iter())
    }

    /// `gametype zclassic map a map b`: the value of `sv_maprotation`.
    pub fn rotation_text(&self) -> String {
        let maps: Vec<String> = self.all_maps().map(|m| format!("map {}", m.name)).collect();
        format!("gametype zclassic {}", maps.join(" "))
    }

    /// The maps still to come after the first one, in the same form. The
    /// server starts on the first map directly, so its rotation pointer has
    /// to begin at the second; otherwise the first map is played twice.
    pub fn rotation_rest(&self) -> Option<String> {
        if self.more.is_empty() {
            return None;
        }
        let maps: Vec<String> = self.more.iter().map(|m| format!("map {}", m.name)).collect();
        Some(format!("gametype zclassic {}", maps.join(" ")))
    }

    pub fn fs_game(&self) -> String {
        match &self.mod_item {
            Some(m) => m.id.clone(),
            None if self.all_maps().any(|m| m.kind == Kind::WorkshopMap) => "usermaps".into(),
            None => String::new(),
        }
    }
    /// Value of the `workshop_id` dvar, which tells joining players what to download.
    /// (One id only: with several workshop maps in a rotation it names the first.)
    pub fn workshop_id(&self) -> String {
        match &self.mod_item {
            Some(m) => m.id.clone(),
            None => self
                .all_maps()
                .find(|m| m.kind == Kind::WorkshopMap)
                .map(|m| m.id.clone())
                .unwrap_or_default(),
        }
    }
}

/// Looks up the chosen map and mod. Errors name what is installed instead.
pub fn select(content: &Content, map: &str, mod_id: &str) -> Result<Selection> {
    let Some(map_item) = content.map(map) else {
        let have: Vec<&str> = content.maps.iter().map(|m| m.name.as_str()).collect();
        return Err(Error::Other(format!(
            "The map {map} is not installed (no {map}.ff). Installed maps: {}.",
            if have.is_empty() { "none".to_string() } else { have.join(", ") }
        )));
    };
    let mod_item = if mod_id.is_empty() {
        None
    } else {
        Some(content.mod_by_id(mod_id).cloned().ok_or_else(|| {
            let have: Vec<String> = content.mods.iter().map(|m| format!("{} ({})", m.id, m.title)).collect();
            Error::Other(format!(
                "The mod {mod_id} is not installed. Installed mods: {}.",
                if have.is_empty() { "none".to_string() } else { have.join(", ") }
            ))
        })?)
    };
    Ok(Selection { map: map_item.clone(), mod_item, more: Vec::new() })
}

/// Like [`select`], for a list of maps played in order.
pub fn select_list(content: &Content, maps: &[String], mod_id: &str) -> Result<Selection> {
    let first = maps.first().map(String::as_str).unwrap_or("");
    let mut sel = select(content, first, mod_id)?;
    for name in maps.iter().skip(1) {
        sel.more.push(select(content, name, "")?.map);
    }
    Ok(sel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workshop_item(
        root: &Path,
        id: &str,
        kind: &str,
        folder: &str,
        title: &str,
        complete: bool,
    ) -> PathBuf {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("workshop.json"),
            format!(r#"{{"Description":"x","FolderName":"{folder}","PublisherID":"{id}","Title":"{title}","Type":"{kind}"}}"#),
        )
        .unwrap();
        if complete {
            let ff = if kind == "mod" { "core_mod.ff".to_string() } else { format!("{folder}.ff") };
            fs::write(dir.join(ff), b"ff").unwrap();
        }
        dir
    }

    #[test]
    fn titles_lose_colour_codes() {
        assert_eq!(
            clean_title("^1[ZM] ^3All-around  Enhancement v4.1.7 ^7Lite"),
            "[ZM] All-around Enhancement v4.1.7 Lite"
        );
        assert_eq!(clean_title("2^10 is big"), "20 is big");
        assert_eq!(clean_title("a^b"), "a^b");
    }

    #[test]
    fn finds_official_and_workshop_content() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("game");
        let lib = tmp.path().join("lib");
        let server_zone = tmp.path().join("server").join("zone");
        fs::create_dir_all(game.join("zone")).unwrap();
        fs::create_dir_all(&server_zone).unwrap();
        fs::write(game.join("zone").join("zm_zod.ff"), b"x").unwrap();
        fs::write(server_zone.join("zm_factory.ff"), b"x").unwrap();
        let ws = lib.join("steamapps").join("workshop").join("content").join("311210");
        let town = workshop_item(&ws, "1624257987", "map", "zm_town", "TOWN REIMAGINED", true);
        workshop_item(
            &ws,
            "2631943123",
            "mod",
            "all_around_enhancement",
            "^1[ZM] ^3All-around Enhancement",
            true,
        );
        workshop_item(&ws, "111", "map", "mp_arena", "A multiplayer map", true);
        workshop_item(&ws, "222", "map", "zm_broken", "Download not finished", false);
        workshop_item(&ws, "333", "map", "zm_bad name;x", "Bad folder name", true);
        workshop_item(&ws, "abc", "map", "zm_bad_id", "Bad id", true);
        fs::create_dir_all(ws.join("444")).unwrap(); // no workshop.json
                                                     // The same item again in the game's usermaps folder must not be listed twice.
        workshop_item(&game.join("usermaps"), "1624257987", "map", "zm_town", "TOWN REIMAGINED", true);
        // Layout with the files in a "zone" subfolder.
        let nested = game.join("mods").join("555").join("zone");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("workshop.json"),
            r#"{"FolderName":"nested_mod","PublisherID":555,"Title":"Nested","Type":"MOD"}"#,
        )
        .unwrap();
        fs::write(nested.join("core_mod.ff"), b"x").unwrap();

        let c = scan(Some(&game), &server_zone, std::slice::from_ref(&lib));
        let maps: Vec<&str> = c.maps.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(maps, vec!["zm_zod", "zm_factory", "zm_town"]);
        let mods: Vec<&str> = c.mods.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(mods, vec!["2631943123", "555"]);
        assert_eq!(c.mods[0].title, "[ZM] All-around Enhancement");
        assert_eq!(c.mod_by_id("555").unwrap().path, nested);
        assert_eq!(c.map("ZM_TOWN").unwrap().label(), "zm_town - TOWN REIMAGINED (workshop)");
        assert_eq!(c.map("zm_zod").unwrap().label(), "zm_zod - Shadows of Evil");
        assert_eq!(c.mods[0].label(), "[ZM] All-around Enhancement [2631943123]");
        // The copy in the game's usermaps folder was seen first.
        assert_ne!(c.map("zm_town").unwrap().path, town);

        // Nothing found anywhere is fine.
        let empty = scan(None, &tmp.path().join("nope"), &[]);
        assert!(empty.maps.is_empty() && empty.mods.is_empty());
    }

    #[test]
    fn links_without_copying_and_only_once() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("ws");
        let town = workshop_item(&ws, "10", "map", "zm_town", "Town", true);
        let modd = workshop_item(&ws, "20", "mod", "my_mod", "My mod", true);
        let c = Content {
            maps: vec![Item {
                kind: Kind::WorkshopMap,
                id: "10".into(),
                name: "zm_town".into(),
                title: "Town".into(),
                path: town.clone(),
            }],
            mods: vec![Item {
                kind: Kind::Mod,
                id: "20".into(),
                name: "my_mod".into(),
                title: "My mod".into(),
                path: modd.clone(),
            }],
        };
        let server = tmp.path().join("server");
        assert_eq!(link_workshop(&server, &c, &|_| {}), 2);
        let map_link = server.join("usermaps").join("10").join("zone");
        let mod_link = server.join("mods").join("20").join("zone");
        assert!(fsutil::is_reparse_point(&map_link) && fsutil::is_reparse_point(&mod_link));
        assert!(map_link.join("zm_town.ff").is_file() && mod_link.join("core_mod.ff").is_file());
        assert_eq!(link_workshop(&server, &c, &|_| {}), 0, "existing links are left alone");
    }

    #[test]
    fn selection_rules() {
        let official = Item {
            kind: Kind::OfficialMap,
            id: String::new(),
            name: "zm_zod".into(),
            title: "Shadows of Evil".into(),
            path: PathBuf::new(),
        };
        let custom = Item {
            kind: Kind::WorkshopMap,
            id: "10".into(),
            name: "zm_town".into(),
            title: "Town".into(),
            path: PathBuf::new(),
        };
        let modd = Item {
            kind: Kind::Mod,
            id: "20".into(),
            name: "m".into(),
            title: "M".into(),
            path: PathBuf::new(),
        };
        let c = Content { maps: vec![official, custom], mods: vec![modd] };

        let s = select(&c, "zm_zod", "").unwrap();
        assert_eq!((s.fs_game().as_str(), s.workshop_id().as_str()), ("", ""));
        let s = select(&c, "zm_town", "").unwrap();
        assert_eq!((s.fs_game().as_str(), s.workshop_id().as_str()), ("usermaps", "10"));
        let s = select(&c, "zm_zod", "20").unwrap();
        assert_eq!((s.fs_game().as_str(), s.workshop_id().as_str()), ("20", "20"));
        let s = select(&c, "zm_town", "20").unwrap();
        assert_eq!((s.fs_game().as_str(), s.workshop_id().as_str()), ("20", "20"));

        let e = select(&c, "zm_moon", "").unwrap_err().to_string();
        assert!(e.contains("zm_moon") && e.contains("zm_zod") && e.contains("zm_town"), "{e}");
        let s = select_list(&c, &["zm_zod".to_string(), "zm_town".to_string(), "zm_zod".to_string()], "")
            .unwrap();
        assert_eq!(s.rotation_text(), "gametype zclassic map zm_zod map zm_town map zm_zod");
        assert_eq!(s.rotation_rest().as_deref(), Some("gametype zclassic map zm_town map zm_zod"));
        assert_eq!(select(&c, "zm_zod", "").unwrap().rotation_rest(), None);
        assert_eq!(
            (s.fs_game().as_str(), s.workshop_id().as_str()),
            ("usermaps", "10"),
            "a workshop map anywhere in the rotation"
        );
        assert!(select_list(&c, &["zm_zod".to_string(), "zm_moon".to_string()], "").is_err());
        let e = select(&c, "zm_zod", "99").unwrap_err().to_string();
        assert!(e.contains("99") && e.contains("20 (M)"), "{e}");
    }
}
