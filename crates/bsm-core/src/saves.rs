//! Saved games. The menu script writes them through BOIII's own file
//! functions into `<server>\boiii\scriptdata\servermaker\save_<slot>.txt`:
//!
//! ```text
//! BSMSAVE;1
//! map;zm_zod
//! round;12
//! power;1
//! player;<points>;<perk,perk,...|->;<weapon:clip:stock,...|->;<name>
//! ```
//!
//! This module only reads, lists and deletes them; loading happens inside the
//! game (the host app tells the script which slot to continue from).

use crate::error::{io_ctx, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SLOTS: [&str; 4] = ["1", "2", "3", "auto"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedPlayer {
    pub name: String,
    pub points: u32,
    pub perks: usize,
    pub weapons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Save {
    pub slot: String,
    pub map: String,
    pub round: u32,
    pub power: bool,
    pub players: Vec<SavedPlayer>,
    /// When the file was last written (seconds since 1970; 0 if unknown).
    pub when: u64,
}

pub fn dir(server_dir: &Path) -> PathBuf {
    server_dir.join("boiii").join("scriptdata").join("servermaker")
}

pub fn file(server_dir: &Path, slot: &str) -> PathBuf {
    dir(server_dir).join(format!("save_{slot}.txt"))
}

pub fn valid_slot(slot: &str) -> bool {
    SLOTS.contains(&slot)
}

/// Reads one save file's text. None if it is not a save.
pub fn parse(slot: &str, text: &str) -> Option<Save> {
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    if lines.next()? != "BSMSAVE;1" {
        return None;
    }
    let mut save = Save {
        slot: slot.to_string(),
        map: String::new(),
        round: 1,
        power: false,
        players: Vec::new(),
        when: 0,
    };
    for line in lines {
        let f: Vec<&str> = line.split(';').collect();
        match f.as_slice() {
            ["map", map, ..] => save.map = map.to_string(),
            ["round", round, ..] => save.round = round.parse().unwrap_or(1),
            ["power", power, ..] => save.power = *power == "1",
            ["player", points, perks, weapons, name @ ..] if !name.is_empty() => {
                let list = |s: &str| -> Vec<String> {
                    if s == "-" {
                        Vec::new()
                    } else {
                        s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect()
                    }
                };
                save.players.push(SavedPlayer {
                    name: name.join(";"),
                    points: points.parse().unwrap_or(0),
                    perks: list(perks).len(),
                    weapons: list(weapons)
                        .iter()
                        .map(|w| w.split(':').next().unwrap_or_default().to_string())
                        .collect(),
                });
            }
            _ => {}
        }
    }
    (!save.map.is_empty()).then_some(save)
}

/// All saves in the server folder, slot 1, 2, 3, then the autosave.
pub fn list(server_dir: &Path) -> Vec<Save> {
    SLOTS
        .iter()
        .filter_map(|slot| {
            let path = file(server_dir, slot);
            let text = fs::read_to_string(&path).ok()?;
            let mut save = parse(slot, &text)?;
            save.when = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(save)
        })
        .collect()
}

pub fn delete(server_dir: &Path, slot: &str) -> Result<bool> {
    let path = file(server_dir, slot);
    if !valid_slot(slot) || !path.exists() {
        return Ok(false);
    }
    io_ctx(fs::remove_file(&path), "delete", &path)?;
    Ok(true)
}

/// "3 minutes ago" style text for when a save was made.
pub fn age_text(when: u64) -> String {
    if when == 0 {
        return "-".into();
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(when);
    let s = now.saturating_sub(when);
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => crate::scores::date_text(when),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAVE: &str = "BSMSAVE;1\nmap;zm_zod\nround;12\npower;1\nplayer;18450;specialty_armorvest,specialty_fastreload,;ar_standard_upgraded:30:240,pistol_standard:12:80,;kops\nplayer;500;-;-;a;b\n";

    #[test]
    fn reads_a_save() {
        let s = parse("1", SAVE).unwrap();
        assert_eq!((s.map.as_str(), s.round, s.power, s.players.len()), ("zm_zod", 12, true, 2));
        assert_eq!(
            s.players[0],
            SavedPlayer {
                name: "kops".into(),
                points: 18450,
                perks: 2,
                weapons: vec!["ar_standard_upgraded".into(), "pistol_standard".into()]
            }
        );
        assert_eq!(s.players[1].name, "a;b", "a ';' in a name is kept");
        assert_eq!((s.players[1].perks, s.players[1].weapons.len()), (0, 0));
        assert!(parse("1", "not a save").is_none());
        assert!(parse("1", "BSMSAVE;1\nround;3\n").is_none(), "a save needs a map");
    }

    #[test]
    fn lists_and_deletes_in_slot_order() {
        let tmp = tempfile::tempdir().unwrap();
        let server = tmp.path();
        assert!(list(server).is_empty());
        fs::create_dir_all(dir(server)).unwrap();
        fs::write(file(server, "auto"), SAVE.replace("round;12", "round;14")).unwrap();
        fs::write(file(server, "2"), SAVE).unwrap();
        fs::write(dir(server).join("save_evil.txt"), SAVE).unwrap();
        fs::write(file(server, "3"), "garbage").unwrap();
        let saves = list(server);
        assert_eq!(
            saves.iter().map(|s| (s.slot.as_str(), s.round)).collect::<Vec<_>>(),
            vec![("2", 12), ("auto", 14)]
        );
        assert!(saves.iter().all(|s| s.when > 0));
        assert!(delete(server, "2").unwrap());
        assert!(!delete(server, "2").unwrap());
        assert!(!delete(server, "evil").unwrap(), "only the known slots can be deleted");
        assert!(dir(server).join("save_evil.txt").exists());
        assert_eq!(list(server).len(), 1);
        assert_eq!(age_text(0), "-");
    }
}
