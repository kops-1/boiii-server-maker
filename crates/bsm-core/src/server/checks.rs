//! The checklist: what is in place and what is still missing.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Ok,
    /// Not done yet, but Setup can do it by itself.
    Todo,
    /// Needs the user (install something, choose a folder).
    Missing,
    Info,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub state: CheckState,
    pub detail: String,
}

pub(super) fn zm_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("zm_") && (n.ends_with(".ff") || n.ends_with(".fd"))
}

pub(super) fn list_zm_files(zone: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(zone)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| zm_file(n))
        .collect();
    v.sort();
    v
}

pub(super) fn missing_required(zone: &Path) -> Vec<&'static str> {
    ZM_REQUIRED.iter().copied().filter(|f| !zone.join(f).is_file()).collect()
}

/// Official Zombies maps whose files are in `zone`, as (name, title).
pub fn installed_maps(zone: &Path) -> Vec<(&'static str, &'static str)> {
    OFFICIAL_MAPS.iter().copied().filter(|(n, _)| zone.join(format!("{n}.ff")).is_file()).collect()
}

pub(super) fn client_source(env: &Env) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(g) = &env.game_dir {
        candidates.push(g.join(CLIENT_EXE));
    }
    if let Some(s) = &env.steam_server_dir {
        candidates.push(s.join(CLIENT_EXE));
    }
    candidates.into_iter().find(|p| p.is_file() && norm(p) != norm(&env.client_exe()))
}

/// What is ready and what is not. Shown by the window app and `bsm status`.
pub fn checks(env: &Env) -> Vec<Check> {
    use CheckState::*;
    let mut out = Vec::new();

    out.push(match env.libraries.len() {
        0 => Check { name: "Steam", state: Info, detail: "not found on this PC".into() },
        n => Check {
            name: "Steam",
            state: Ok,
            detail: format!(
                "{n} librar{}: {}",
                if n == 1 { "y" } else { "ies" },
                env.libraries.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
        },
    });

    out.push(match &env.game_dir {
        Some(g) => Check { name: "Black Ops III", state: Ok, detail: g.display().to_string() },
        None => Check {
            name: "Black Ops III",
            state: Missing,
            detail: "not found. Install it in Steam, or choose its folder.".into(),
        },
    });

    out.push(if env.has_server_files() {
        Check { name: "Server files", state: Ok, detail: env.server_dir.display().to_string() }
    } else if let Some(s) = &env.steam_server_dir {
        Check {
            name: "Server files",
            state: Todo,
            detail: format!("will be copied from the Steam install at {}", s.display()),
        }
    } else {
        Check {
            name: "Server files",
            state: Missing,
            detail: "not on this PC. Use \"Install through Steam\" or \"Download with steamcmd\".".into(),
        }
    });

    out.push(if env.client_exe().is_file() {
        Check { name: "BOIII client", state: Ok, detail: env.client_exe().display().to_string() }
    } else if let Some(src) = client_source(env) {
        Check { name: "BOIII client", state: Todo, detail: format!("will be copied from {}", src.display()) }
    } else {
        Check {
            name: "BOIII client",
            state: Todo,
            detail: "will be downloaded from the BOIII project's release page".into(),
        }
    });

    let zone = env.zone_dir();
    let have = list_zm_files(&zone).len();
    let missing = missing_required(&zone);
    let damaged = damaged_files(env, None);
    out.push(if !damaged.is_empty() {
        Check {
            name: "Zombies files",
            state: Missing,
            detail: format!(
                "damaged: {}. Press \"Repair files\".",
                damaged
                    .iter()
                    .map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    } else if have > 0 && missing.is_empty() {
        let maps: Vec<&str> = installed_maps(&zone).iter().map(|(_, t)| *t).collect();
        Check {
            name: "Zombies files",
            state: Ok,
            detail: format!(
                "{have} files; maps: {}",
                if maps.is_empty() { "none".into() } else { maps.join(", ") }
            ),
        }
    } else if env.game_dir.is_some() {
        Check {
            name: "Zombies files",
            state: Todo,
            detail: "will be copied from your Black Ops III install".into(),
        }
    } else {
        Check {
            name: "Zombies files",
            state: Missing,
            detail: format!("missing ({}) and Black Ops III was not found", missing.join(", ")),
        }
    });

    let found = env.content();
    let workshop_maps = found.maps.iter().filter(|m| m.kind == content::Kind::WorkshopMap).count();
    out.push(Check {
        name: "Workshop",
        state: Info,
        detail: format!(
            "{workshop_maps} Zombies map{} and {} mod{} found",
            if workshop_maps == 1 { "" } else { "s" },
            found.mods.len(),
            if found.mods.len() == 1 { "" } else { "s" }
        ),
    });
    out.push(match content::select_list(&found, &env.settings.play_list(), &env.settings.mod_id) {
        Result::Ok(sel) => Check {
            name: "Selected",
            state: Ok,
            detail: format!(
                "{}{}{}",
                sel.map.label(),
                if sel.more.is_empty() {
                    String::new()
                } else {
                    format!(
                        " then {}",
                        sel.more.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ")
                    )
                },
                sel.mod_item.as_ref().map(|m| format!(" + mod {}", m.label())).unwrap_or_default()
            ),
        },
        Err(e) if env.has_server_files() || env.game_dir.is_some() => {
            Check { name: "Selected", state: Missing, detail: e.to_string() }
        }
        Err(_) => Check {
            name: "Selected",
            state: Todo,
            detail: format!("{} (checked after the files are in place)", env.settings.map),
        },
    });

    out.push(if env.cfg_path().is_file() {
        Check { name: "Server config", state: Ok, detail: env.cfg_path().display().to_string() }
    } else {
        Check { name: "Server config", state: Todo, detail: "will be written".into() }
    });

    let s = &env.settings;
    let mut extras = Vec::new();
    if s.menu {
        extras.push(if s.menu_players.is_empty() {
            "menu for everyone".to_string()
        } else {
            format!("menu for {}", s.menu_players)
        });
    }
    if s.god_all {
        extras.push("god mode for all".into());
    }
    if s.ammo_all {
        extras.push("infinite ammo for all".into());
    }
    if s.start_points > 0 {
        extras.push(format!("{} start points", s.start_points));
    }
    if s.start_perks {
        extras.push("all perks at start".into());
    }
    if s.instakill_all {
        extras.push("insta-kill".into());
    }
    if s.double_points {
        extras.push("double points".into());
    }
    if s.keep_perks {
        extras.push("perks kept when down".into());
    }
    out.push(Check {
        name: "Mod menu",
        state: Info,
        detail: if extras.is_empty() { "off (normal game)".into() } else { extras.join(", ") },
    });

    let boiii_data = crate::paths::boiii_data_dir().join("data");
    out.push(if boiii_data.is_dir() {
        Check { name: "BOIII data", state: Ok, detail: boiii_data.display().to_string() }
    } else {
        Check {
            name: "BOIII data",
            state: Info,
            detail: "not present yet. BOIII downloads it on its first start (needs internet).".into(),
        }
    });
    out
}

pub fn format_checks(checks: &[Check]) -> String {
    checks
        .iter()
        .map(|c| {
            let mark = match c.state {
                CheckState::Ok => "[ OK ]",
                CheckState::Todo => "[todo]",
                CheckState::Missing => "[ !! ]",
                CheckState::Info => "[info]",
            };
            format!("{mark} {:<14} {}", c.name, c.detail)
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}
