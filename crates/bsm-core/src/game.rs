//! Starts the BOIII game client on this PC so the host can play on their own
//! server. The client runs from the Black Ops III folder and needs its own
//! UDP port, because the server already holds one.

use crate::error::{io_ctx, Error, Result};
use crate::server::{self, Env, CLIENT_EXE};
use crate::{fsutil, net, proc};
use std::path::PathBuf;

/// What [`prepare`] worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// `boiii.exe` in the game folder.
    pub exe: PathBuf,
    pub game_dir: PathBuf,
    /// UDP port for the game client (never the server's).
    pub client_port: u16,
    /// Port of the server to join.
    pub server_port: u16,
    /// Text to paste into the game console to join the server.
    pub connect: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub plan: Plan,
    /// Process id of the game, old or new.
    pub pid: u32,
    /// The game was already open; nothing new was started.
    pub already_running: bool,
}

/// Command line of the game client: skip BOIII's launcher window and use a
/// port of its own.
pub fn client_arguments(client_port: u16) -> String {
    format!("-launch +set net_port \"{client_port}\"")
}

/// First free UDP port after the server's.
pub fn pick_client_port(server_port: u16) -> Result<u16> {
    let start = if server_port == u16::MAX { 27018 } else { server_port + 1 };
    (start.max(1024)..=u16::MAX)
        .find(|p| *p != server_port && net::port_free(*p))
        .ok_or_else(|| Error::Other("No free UDP port was found for the game.".into()))
}

/// Checks everything needed to start the game and works out the ports.
///
/// The game needs `boiii.exe` inside the Black Ops III folder. If it is not
/// there and `install_client` is true, the copy from the server folder is
/// placed there; otherwise `Error::NeedClient` says so and nothing is changed.
pub fn prepare(env: &Env, install_client: bool, log: &dyn Fn(&str)) -> Result<Plan> {
    let Some(game_dir) = env.game_dir.clone() else {
        return Err(Error::NeedGame(
            "Black Ops III was not found on this PC. Install it in Steam or choose its folder.".into(),
        ));
    };
    let exe = game_dir.join(CLIENT_EXE);
    if !exe.is_file() || fsutil::looks_zeroed(&exe) {
        let source = env.client_exe();
        if !source.is_file() || fsutil::looks_zeroed(&source) {
            return Err(Error::Other(format!(
                "boiii.exe is neither in the game folder nor in the server folder. Run setup first, or put boiii.exe into {}.",
                game_dir.display()
            )));
        }
        if !install_client {
            return Err(Error::NeedClient(format!(
                "The BOIII client (boiii.exe) is not in your Black Ops III folder yet:\r\n{}\r\n\r\nIt has to be there to play.",
                game_dir.display()
            )));
        }
        log(&format!("Copying boiii.exe into the game folder: {}", exe.display()));
        if exe.exists() {
            io_ctx(std::fs::remove_file(&exe), "replace", &exe)?;
        }
        fsutil::copy_if_different(&source, &exe, false)?;
    }

    let status = server::status(env);
    let server_port = status.port;
    let client_port = pick_client_port(server_port)?;
    let connect = net::connect_command("127.0.0.1", server_port, &env.settings.password);
    Ok(Plan { exe, game_dir, client_port, server_port, connect })
}

/// Starts the game, unless it is already open.
pub fn launch(env: &Env, install_client: bool, log: &dyn Fn(&str)) -> Result<Launched> {
    let plan = prepare(env, install_client, log)?;
    if let Some(p) = proc::running_from(&plan.exe).first() {
        log(&format!("The game is already open (process {}).", p.pid));
        return Ok(Launched { plan, pid: p.pid, already_running: true });
    }
    proc::keep_std_handles_private();
    use std::os::windows::process::CommandExt;
    let child = std::process::Command::new(&plan.exe) // full path on purpose
        .raw_arg(client_arguments(plan.client_port))
        .current_dir(&plan.game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let child = io_ctx(child, "start", &plan.exe)?;
    log(&format!("Game started: \"{}\" {}", plan.exe.display(), client_arguments(plan.client_port)));
    Ok(Launched { pid: child.id(), plan, already_running: false })
}

/// Where the game keeps its favourite servers: one `address:port` per line.
pub fn favorites_file(game_dir: &std::path::Path) -> PathBuf {
    game_dir.join("boiii_players").join("user").join("favorite_servers.txt")
}

/// Adds `address:port` to the game's favourites. Returns false if it was
/// already there. Other lines in the file are kept as they are.
pub fn add_favorite_line(file: &std::path::Path, endpoint: &str) -> Result<bool> {
    let existing = std::fs::read_to_string(file).unwrap_or_default();
    if existing.lines().any(|l| l.trim().eq_ignore_ascii_case(endpoint)) {
        return Ok(false);
    }
    if let Some(parent) = file.parent() {
        io_ctx(std::fs::create_dir_all(parent), "create", parent)?;
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push_str("\r\n");
    }
    text.push_str(endpoint);
    text.push_str("\r\n");
    io_ctx(std::fs::write(file, text), "write", file)?;
    Ok(true)
}

/// Puts this PC's server into the favourites of the game on this PC
/// (`127.0.0.1:<port>`), so it shows under Favorites in the server browser.
/// The game reads the list when it starts and writes it back when it closes,
/// so it has to be closed. Returns (added, endpoint).
pub fn add_favorite(env: &Env) -> Result<(bool, String)> {
    let Some(game_dir) = env.game_dir.clone() else {
        return Err(Error::NeedGame(
            "Black Ops III was not found on this PC. Install it in Steam or choose its folder.".into(),
        ));
    };
    if !proc::running_from(&game_dir.join(CLIENT_EXE)).is_empty() {
        return Err(Error::Other(
            "Close the game first, then press the star again. The game reads its favourites when it starts."
                .into(),
        ));
    }
    let endpoint = format!("127.0.0.1:{}", server::status(env).port);
    let added = add_favorite_line(&favorites_file(&game_dir), &endpoint)?;
    Ok((added, endpoint))
}

/// What to tell the player after the game was started.
pub fn how_to_join(launched: &Launched, server_running: bool) -> String {
    let mut s = String::new();
    if !server_running {
        s.push_str("Note: the server is not running yet. Press \"2. Start server\" and wait until it says RUNNING.\r\n");
    }
    s.push_str(&format!(
        "To join your server: wait for the game's main menu, press the ~ key (console), paste with Ctrl+V and press Enter.\r\nThe command is:  {}",
        launched.plan.connect
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use std::fs;
    use std::net::{Ipv4Addr, UdpSocket};

    struct Fake {
        _tmp: tempfile::TempDir,
        env: Env,
        game: PathBuf,
    }

    fn fake(with_game: bool, client_in_game: bool, client_in_server: bool, settings: Settings) -> Fake {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().join("lib");
        fs::create_dir_all(lib.join("steamapps").join("common")).unwrap();
        let game = lib.join("steamapps").join("common").join("Call of Duty Black Ops III");
        if with_game {
            fs::create_dir_all(game.join("zone")).unwrap();
            if client_in_game {
                fs::write(game.join(CLIENT_EXE), b"MZ game client").unwrap();
            }
        }
        let env = Env::new(settings, tmp.path().join("data"), vec![lib]);
        if client_in_server {
            fs::create_dir_all(&env.server_dir).unwrap();
            fs::write(env.client_exe(), b"MZ server copy").unwrap();
        }
        Fake { _tmp: tmp, env, game }
    }

    #[test]
    fn arguments_and_ports() {
        assert_eq!(client_arguments(27018), "-launch +set net_port \"27018\"");
        let held = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let server_port = held.local_addr().unwrap().port();
        let p = pick_client_port(server_port).unwrap();
        // (Other tests open ports at the same time, so only the rule itself is checked.)
        assert!(p != server_port && p > server_port.min(u16::MAX - 1));
        assert_ne!(pick_client_port(u16::MAX).unwrap(), u16::MAX);
    }

    #[test]
    fn plan_uses_the_game_folder_and_the_server_settings() {
        let s = Settings { port: 27030, password: "secret".into(), ..Default::default() };
        let f = fake(true, true, false, s);
        let plan = prepare(&f.env, false, &|_| {}).unwrap();
        assert_eq!(plan.exe, f.game.join(CLIENT_EXE));
        assert_eq!(plan.game_dir, f.game);
        assert_eq!(plan.server_port, 27030);
        assert_ne!(plan.client_port, 27030);
        assert_eq!(plan.connect, "password secret; connect 127.0.0.1:27030");
        let launched = Launched { plan, pid: 1, already_running: false };
        assert!(how_to_join(&launched, false).contains("not running yet"));
        assert!(!how_to_join(&launched, true).contains("not running yet"));
        assert!(how_to_join(&launched, true).contains("connect 127.0.0.1:27030"));
    }

    #[test]
    fn missing_client_in_the_game_folder_is_only_added_with_permission() {
        let f = fake(true, false, true, Settings::default());
        match prepare(&f.env, false, &|_| {}) {
            Err(Error::NeedClient(m)) => assert!(m.contains("Black Ops III")),
            other => panic!("expected NeedClient, got {other:?}"),
        }
        assert!(!f.game.join(CLIENT_EXE).exists(), "nothing may be written without permission");

        let plan = prepare(&f.env, true, &|_| {}).unwrap();
        assert_eq!(fs::read(&plan.exe).unwrap(), b"MZ server copy");
        assert_eq!(fs::read(f.env.client_exe()).unwrap(), b"MZ server copy", "the server's copy stays");

        // A zero-filled client in the game folder counts as missing.
        fs::write(&plan.exe, vec![0u8; 100]).unwrap();
        assert!(matches!(prepare(&f.env, false, &|_| {}), Err(Error::NeedClient(_))));
        prepare(&f.env, true, &|_| {}).unwrap();
        assert_eq!(fs::read(f.game.join(CLIENT_EXE)).unwrap(), b"MZ server copy");
    }

    #[test]
    fn favourites_are_added_once_and_other_lines_survive() {
        let s = Settings { port: 27030, ..Default::default() };
        let f = fake(true, true, false, s);
        let file = favorites_file(&f.game);
        assert_eq!(add_favorite(&f.env).unwrap(), (true, "127.0.0.1:27030".to_string()));
        assert_eq!(fs::read_to_string(&file).unwrap(), "127.0.0.1:27030\r\n");
        assert_eq!(add_favorite(&f.env).unwrap(), (false, "127.0.0.1:27030".to_string()));

        // A file the game wrote without a final line break, with a friend's server in it.
        fs::write(&file, "51.2.3.4:27017").unwrap();
        assert!(add_favorite_line(&file, "127.0.0.1:27030").unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), "51.2.3.4:27017\r\n127.0.0.1:27030\r\n");
        assert!(!add_favorite_line(&file, "51.2.3.4:27017").unwrap());

        let none = fake(false, false, false, Settings::default());
        assert!(matches!(add_favorite(&none.env), Err(Error::NeedGame(_))));
    }

    #[test]
    fn clear_errors_without_game_or_client() {
        let f = fake(false, false, true, Settings::default());
        assert!(matches!(prepare(&f.env, true, &|_| {}), Err(Error::NeedGame(_))));
        let f = fake(true, false, false, Settings::default());
        let e = prepare(&f.env, true, &|_| {}).unwrap_err().to_string();
        assert!(e.contains("Run setup first"), "{e}");
    }
}
