//! The scoreboard: finished games, read from the lines the menu script
//! writes into the server's game log, kept in `<data>\scores.json`.
//!
//! ```text
//! BSM_GAME;<id>;<map>;<round>;<player count>
//! BSM_PLAYER;<id>;<name>;<kills>;<points>;<downs>;<headshots>
//! ```

use crate::error::{io_ctx, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlayerScore {
    pub name: String,
    pub kills: u32,
    pub points: u32,
    pub downs: u32,
    pub headshots: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Game {
    /// Number the script gave the game; with `map` it tells games apart.
    pub id: String,
    /// When the tool recorded it (seconds since 1970).
    pub when: u64,
    pub map: String,
    pub round: u32,
    pub players: Vec<PlayerScore>,
}

impl Game {
    pub fn kills(&self) -> u32 {
        self.players.iter().map(|p| p.kills).sum()
    }
}

pub fn file(data_dir: &Path) -> PathBuf {
    data_dir.join("scores.json")
}

/// Where the server writes its game log.
pub fn log_file(server_dir: &Path) -> PathBuf {
    server_dir.join("boiii").join("games_zm.log")
}

pub fn load(data_dir: &Path) -> Vec<Game> {
    fs::read_to_string(file(data_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(data_dir: &Path, games: &[Game]) -> Result<()> {
    io_ctx(fs::create_dir_all(data_dir), "create", data_dir)?;
    let path = file(data_dir);
    io_ctx(fs::write(&path, serde_json::to_string_pretty(games)?), "write", &path)
}

/// Reads the finished games out of a game log. Lines may carry the engine's
/// time stamp in front; anything else in the log is ignored.
pub fn parse_log(text: &str) -> Vec<Game> {
    let mut games: Vec<Game> = Vec::new();
    for line in text.lines() {
        if let Some(pos) = line.find("BSM_GAME;") {
            let f: Vec<&str> = line[pos..].trim().split(';').collect();
            if f.len() >= 4 {
                games.push(Game {
                    id: f[1].to_string(),
                    when: 0,
                    map: f[2].to_string(),
                    round: f[3].parse().unwrap_or(0),
                    players: Vec::new(),
                });
            }
        } else if let Some(pos) = line.find("BSM_PLAYER;") {
            let f: Vec<&str> = line[pos..].trim().split(';').collect();
            // A name may itself contain ';': the four numbers are counted from the end.
            if f.len() >= 7 {
                let n = f.len();
                let num = |i: usize| f[i].parse::<u32>().unwrap_or(0);
                let player = PlayerScore {
                    name: f[2..n - 4].join(";"),
                    kills: num(n - 4),
                    points: num(n - 3),
                    downs: num(n - 2),
                    headshots: num(n - 1),
                };
                if let Some(game) = games.iter_mut().rev().find(|g| g.id == f[1]) {
                    game.players.push(player);
                }
            }
        }
    }
    games
}

/// Adds the games in the server's log that the scoreboard does not have yet.
/// Returns how many were added.
pub fn import(server_dir: &Path, data_dir: &Path) -> Result<usize> {
    let Ok(bytes) = fs::read(log_file(server_dir)) else {
        return Ok(0);
    };
    let found = parse_log(&String::from_utf8_lossy(&bytes));
    if found.is_empty() {
        return Ok(0);
    }
    let mut games = load(data_dir);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut added = 0;
    for mut g in found {
        if games.iter().any(|k| k.id == g.id && k.map == g.map) {
            continue;
        }
        g.when = now;
        games.push(g);
        added += 1;
    }
    if added > 0 {
        save(data_dir, &games)?;
    }
    Ok(added)
}

/// Best first: highest round, then most kills, then newest.
pub fn best(mut games: Vec<Game>) -> Vec<Game> {
    games.sort_by(|a, b| b.round.cmp(&a.round).then(b.kills().cmp(&a.kills())).then(b.when.cmp(&a.when)));
    games
}

/// Days since 1970 to a `YYYY-MM-DD` date (UTC), for showing when a game was played.
pub fn date_text(when: u64) -> String {
    if when == 0 {
        return "-".into();
    }
    let days = (when / 86_400) as i64;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "  0:00 InitGame: \\g_gametype\\zclassic\n  5:12 BSM_GAME;42;zm_zod;12;2\n  5:12 BSM_PLAYER;42;player1;230;18450;3;61\n  5:12 BSM_PLAYER;42;a;b;7;900;1;0\n  5:13 ShutdownGame:\n 21:40 BSM_GAME;77;zm_town;4;1\n 21:40 BSM_PLAYER;77;player1;30;2100;2;5\nBSM_PLAYER;999;orphan;1;1;1;1\nBSM_GAME;bad\n";

    #[test]
    fn reads_games_from_the_log() {
        let games = parse_log(LOG);
        assert_eq!(games.len(), 2);
        assert_eq!((games[0].map.as_str(), games[0].round, games[0].players.len()), ("zm_zod", 12, 2));
        assert_eq!(
            games[0].players[0],
            PlayerScore { name: "player1".into(), kills: 230, points: 18450, downs: 3, headshots: 61 }
        );
        assert_eq!(games[0].players[1].name, "a;b", "a ';' in a name does not shift the numbers");
        assert_eq!(games[0].players[1].kills, 7);
        assert_eq!(games[0].kills(), 237);
        assert_eq!(games[1].players.len(), 1);
        assert!(parse_log("nothing here").is_empty());
    }

    #[test]
    fn import_adds_each_game_once_and_best_sorts_by_round() {
        let tmp = tempfile::tempdir().unwrap();
        let server = tmp.path().join("server");
        let data = tmp.path().join("data");
        assert_eq!(import(&server, &data).unwrap(), 0, "no log yet");
        fs::create_dir_all(server.join("boiii")).unwrap();
        fs::write(log_file(&server), LOG).unwrap();
        assert_eq!(import(&server, &data).unwrap(), 2);
        assert_eq!(import(&server, &data).unwrap(), 0, "same log again adds nothing");
        // The server appends a new game.
        fs::write(
            log_file(&server),
            format!("{LOG}BSM_GAME;90;zm_zod;25;1\nBSM_PLAYER;90;player1;700;90000;0;300\n"),
        )
        .unwrap();
        assert_eq!(import(&server, &data).unwrap(), 1);

        let ranked = best(load(&data));
        assert_eq!(ranked.iter().map(|g| g.round).collect::<Vec<_>>(), vec![25, 12, 4]);
        assert!(ranked.iter().all(|g| g.when > 0));
    }

    #[test]
    fn dates() {
        assert_eq!(date_text(0), "-");
        assert_eq!(date_text(86_400), "1970-01-02");
        assert_eq!(date_text(1_794_744_589), "2026-11-15");
        assert_eq!(date_text(951_782_400), "2000-02-29");
    }
}
