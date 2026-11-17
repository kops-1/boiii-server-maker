//! Finds Steam, its library folders and installed apps without assuming any
//! drive letter or install location.

use crate::paths::norm;
use crate::vdf;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_BO3: u32 = 311210;
pub const APP_BO3_SERVER: u32 = 545990;
pub const SERVER_SUBDIR: &str = "UnrankedServer";
pub const SERVER_EXE: &str = "BlackOps3_UnrankedDedicatedServer.exe";

/// Steam install folders recorded in the registry (per-user and machine-wide).
pub fn steam_roots() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let mut out = Vec::new();
    let mut add = |hive, key: &str, value: &str| {
        if let Ok(k) = RegKey::predef(hive).open_subkey(key) {
            if let Ok(v) = k.get_value::<String, _>(value) {
                if !v.trim().is_empty() {
                    out.push(PathBuf::from(v.replace('/', "\\")));
                }
            }
        }
    };
    add(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath");
    add(HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath");
    add(HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath");
    out
}

/// All library folders reachable from the given Steam roots: the roots
/// themselves plus everything named in their `libraryfolders.vdf`.
pub fn libraries_from_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push = |p: PathBuf, out: &mut Vec<PathBuf>| {
        if p.join("steamapps").is_dir() && seen.insert(norm(&p)) {
            out.push(p);
        }
    };
    for root in roots {
        push(root.clone(), &mut out);
        for file in [
            root.join("steamapps").join("libraryfolders.vdf"),
            root.join("config").join("libraryfolders.vdf"),
        ] {
            if let Ok(text) = fs::read_to_string(&file) {
                for lib in vdf::library_paths(&text) {
                    push(lib, &mut out);
                }
            }
        }
    }
    out
}

/// Steam libraries on this PC. If the registry knows nothing (Steam removed,
/// or a copied library), common folder names are probed on every drive.
pub fn libraries() -> Vec<PathBuf> {
    let mut roots = steam_roots();
    let found = libraries_from_roots(&roots);
    if !found.is_empty() {
        return found;
    }
    for letter in b'C'..=b'Z' {
        let drive = PathBuf::from(format!("{}:\\", letter as char));
        if !drive.is_dir() {
            continue;
        }
        for rel in [
            "SteamLibrary",
            "Steam",
            r"Program Files (x86)\Steam",
            r"Program Files\Steam",
            r"Games\Steam",
            r"Games\SteamLibrary",
        ] {
            roots.push(drive.join(rel));
        }
    }
    libraries_from_roots(&roots)
}

/// Install folder of a Steam app, read from its manifest.
pub fn find_app(libraries: &[PathBuf], app_id: u32) -> Option<PathBuf> {
    for lib in libraries {
        let apps = lib.join("steamapps");
        let Ok(text) = fs::read_to_string(apps.join(format!("appmanifest_{app_id}.acf"))) else {
            continue;
        };
        if let Some(dir) = vdf::acf_installdir(&text) {
            let path = apps.join("common").join(dir);
            if path.is_dir() {
                return Some(path);
            }
        }
    }
    None
}

pub fn is_game_dir(p: &Path) -> bool {
    p.join("zone").is_dir()
}

pub fn is_server_dir(p: &Path) -> bool {
    p.join(SERVER_EXE).is_file()
}

/// Black Ops III install folder.
pub fn find_game(libraries: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = find_app(libraries, APP_BO3) {
        if is_game_dir(&p) {
            return Some(p);
        }
    }
    // Manifest missing (e.g. library copied by hand): look for the usual folder name.
    libraries
        .iter()
        .map(|l| l.join("steamapps").join("common").join("Call of Duty Black Ops III"))
        .find(|p| is_game_dir(p))
}

/// `UnrankedServer` folder of a dedicated server installed through Steam.
pub fn find_steam_server(libraries: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = find_app(libraries, APP_BO3_SERVER) {
        let s = p.join(SERVER_SUBDIR);
        if is_server_dir(&s) {
            return Some(s);
        }
    }
    for lib in libraries {
        let Ok(rd) = fs::read_dir(lib.join("steamapps").join("common")) else {
            continue;
        };
        for entry in rd.flatten() {
            let s = entry.path().join(SERVER_SUBDIR);
            if is_server_dir(&s) {
                return Some(s);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_lib(root: &Path) {
        fs::create_dir_all(root.join("steamapps").join("common")).unwrap();
    }

    fn vdf_escape(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "\\\\")
    }

    #[test]
    fn discovers_libraries_and_apps_anywhere() {
        let tmp = tempfile::tempdir().unwrap();
        let steam = tmp.path().join("Some Place").join("Steam");
        let lib2 = tmp.path().join("other drive").join("Lib");
        mk_lib(&steam);
        mk_lib(&lib2);
        fs::write(
            steam.join("steamapps").join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n\"1\"\n{{\n\"path\" \"{}\"\n}}\n\"2\"\n{{\n\"path\" \"Q:\\\\gone\"\n}}\n}}",
                vdf_escape(&steam),
                vdf_escape(&lib2)
            ),
        )
        .unwrap();

        let libs = libraries_from_roots(&[steam.clone(), steam.clone()]);
        assert_eq!(libs, vec![steam.clone(), lib2.clone()], "deduplicated, missing library dropped");

        // Game in the second library, found through its manifest.
        let game = lib2.join("steamapps").join("common").join("BO3 Custom Name");
        fs::create_dir_all(game.join("zone")).unwrap();
        fs::write(
            lib2.join("steamapps").join("appmanifest_311210.acf"),
            "\"AppState\"\n{\n\"appid\" \"311210\"\n\"installdir\" \"BO3 Custom Name\"\n}",
        )
        .unwrap();
        assert_eq!(find_game(&libs), Some(game));

        // Server without a manifest, found by scanning.
        assert_eq!(find_steam_server(&libs), None);
        let server = steam.join("steamapps").join("common").join("Whatever 545990").join(SERVER_SUBDIR);
        fs::create_dir_all(&server).unwrap();
        fs::write(server.join(SERVER_EXE), b"x").unwrap();
        assert_eq!(find_steam_server(&libs), Some(server));
    }

    #[test]
    fn nothing_found_is_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let libs = libraries_from_roots(&[tmp.path().join("nope")]);
        assert!(libs.is_empty());
        assert_eq!(find_game(&libs), None);
        assert_eq!(find_steam_server(&libs), None);
    }
}
