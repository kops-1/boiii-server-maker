//! `bsm.exe`: the same actions as the window app, for a terminal or scripts.

use bsm_core::server::{self, Env, SetupOptions, State};
use bsm_core::settings::Settings;
use bsm_core::{net, paths, steamcmd, Error};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

const HELP: &str = "\
Boiii Server Maker - command line

  bsm status                 Show what was found on this PC and whether the server runs
  bsm setup [options]        Prepare the server (files, BOIII client, Zombies files, config)
  bsm start [options]        Start the server (add --wait 180 to wait until it answers)
  bsm play                   Start the game on this PC (on its own port) and show the
                             command that joins your server
  bsm favorite               Add this server to the Favorites list of the game on this PC
  bsm stop                   Stop the server
  bsm maps                   List the maps this PC can host (official and workshop)
  bsm mods                   List the workshop mods found on this PC
  bsm next                   Rotation with workshop maps: restart on the next map
  bsm scores                 Show the best finished games
  bsm saves                  Show the saved games
  bsm join                   Show the connect commands for friends and network advice
  bsm fetch-server --steam-user NAME
                             Download the server files with steamcmd (you type the
                             password in steamcmd itself; this tool never sees it)
  bsm import-data FILE.zip   Unpack a BOIII data archive into %LOCALAPPDATA%\\boiii

Options (saved for next time):
  --port N           UDP port of the server (default 27017)
  --map NAME         Zombies map, e.g. zm_zod
  --name TEXT        Server name
  --password TEXT    Join password (--password \"\" for none)
  --players N        Maximum players, 1-8
  --mod ID           Workshop mod to load (see: bsm mods); \"none\" for no mod
  --rotation A,B,C   Maps played one after another (none = stay on --map)
  --scoreboard on|off  Record finished games (see: bsm scores)
  --autosave on|off  Save the game at the start of every round (slot auto)
  --continue SLOT    (start) Continue from saved game 1, 2, 3 or auto
  --menu on|off      In-game mod menu (hold AIM and press MELEE)
  --menu-players L   Names allowed to open the menu, comma separated; \"all\" = everyone
  --god on|off       God mode for every player
  --ammo on|off      Infinite ammo for every player
  --start-points N   Points every player starts with (0 = normal)
  --start-perks on|off  Every player starts with all perks
  --server-dir DIR   Run the server from this folder (\"auto\" = default)
  --game-dir DIR     Black Ops III folder (\"auto\" = find through Steam)
  --no-download      Do not download boiii.exe if it is missing
  --repair           (setup) Compare every game file with its source and replace
                     damaged ones, for example after a power cut
";

struct Args {
    command: String,
    positional: Vec<String>,
    wait: Option<u64>,
    steam_user: Option<String>,
    no_download: bool,
    repair: bool,
    install_client: bool,
}

fn parse(settings: &mut Settings) -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let command = it.next().unwrap_or_else(|| "help".into());
    let mut a = Args {
        command,
        positional: Vec::new(),
        wait: None,
        steam_user: None,
        no_download: false,
        repair: false,
        install_client: false,
    };
    while let Some(arg) = it.next() {
        // A value that looks like another option means the real value went missing
        // (some shells drop an empty ""), so say so instead of swallowing the next option.
        let mut value = |name: &str| match it.next() {
            Some(v) if v.starts_with("--") => {
                Err(format!("{name} needs a value, but the next word is the option {v}"))
            }
            Some(v) => Ok(v),
            None => Err(format!("{name} needs a value")),
        };
        match arg.as_str() {
            "--port" => {
                settings.port = value("--port")?
                    .parse()
                    .ok()
                    .filter(|p| *p >= 1024)
                    .ok_or("--port must be a number from 1024 to 65535")?
            }
            "--map" => settings.map = value("--map")?,
            "--name" => settings.server_name = value("--name")?,
            "--password" => settings.password = value("--password")?,
            "--players" => {
                settings.max_players = value("--players")?
                    .parse()
                    .ok()
                    .filter(|p| (1..=8).contains(p))
                    .ok_or("--players must be 1 to 8")?
            }
            "--mod" => {
                let v = value("--mod")?;
                settings.mod_id = if v.eq_ignore_ascii_case("none") { String::new() } else { v };
            }
            "--rotation" => {
                let v = value("--rotation")?;
                settings.rotation = if v.eq_ignore_ascii_case("none") {
                    Vec::new()
                } else {
                    v.split(',').map(|m| m.trim().to_string()).collect()
                };
            }
            "--autosave" => settings.autosave = on_off("--autosave", &value("--autosave")?)?,
            "--continue" => {
                let v = value("--continue")?;
                if !bsm_core::saves::valid_slot(&v) {
                    return Err("--continue takes 1, 2, 3 or auto".into());
                }
                settings.continue_save = v;
            }
            "--scoreboard" => settings.scoreboard = on_off("--scoreboard", &value("--scoreboard")?)?,
            "--menu" => settings.menu = on_off("--menu", &value("--menu")?)?,
            "--menu-players" => {
                let v = value("--menu-players")?;
                settings.menu_players = if v.eq_ignore_ascii_case("all") || v.eq_ignore_ascii_case("everyone")
                {
                    String::new()
                } else {
                    v
                };
            }
            "--god" => settings.god_all = on_off("--god", &value("--god")?)?,
            "--ammo" => settings.ammo_all = on_off("--ammo", &value("--ammo")?)?,
            "--start-perks" => settings.start_perks = on_off("--start-perks", &value("--start-perks")?)?,
            "--start-points" => {
                settings.start_points =
                    value("--start-points")?.parse().map_err(|_| "--start-points needs a number")?
            }
            "--server-dir" => {
                let v = value("--server-dir")?;
                settings.server_dir = (v != "auto").then(|| PathBuf::from(v));
            }
            "--game-dir" => {
                let v = value("--game-dir")?;
                settings.game_dir = (v != "auto").then(|| PathBuf::from(v));
            }
            "--wait" => a.wait = Some(value("--wait")?.parse().map_err(|_| "--wait needs seconds")?),
            "--steam-user" => a.steam_user = Some(value("--steam-user")?),
            "--no-download" => a.no_download = true,
            "--repair" => a.repair = true,
            "--install-client" => a.install_client = true,
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => a.positional.push(other.to_string()),
        }
    }
    Ok(a)
}

fn on_off(name: &str, v: &str) -> Result<bool, String> {
    match v.to_lowercase().as_str() {
        "on" | "1" | "true" | "yes" => Ok(true),
        "off" | "0" | "false" | "no" => Ok(false),
        _ => Err(format!("{name} needs on or off")),
    }
}

fn log(line: &str) {
    println!("{line}");
}

fn print_status(env: &Env) -> State {
    println!("Data folder:   {}", env.data_dir.display());
    println!("Server folder: {}", env.server_dir.display());
    println!("{}", server::format_checks(&server::checks(env)));
    let st = server::status(env);
    println!("{}", st.summary());
    if let Some(l) = &st.log {
        // BOIII prints little when headless; only point at the file if it has content.
        if st.state != State::Stopped && std::fs::metadata(l).map(|m| m.len() > 0).unwrap_or(false) {
            println!("Output: {}", l.display());
        }
    }
    st.state
}

fn run() -> Result<i32, Error> {
    let data = paths::data_dir();
    let mut settings = Settings::load(&data);
    let args = parse(&mut settings).map_err(Error::other)?;
    settings.sanitize();

    match args.command.as_str() {
        "help" | "--help" | "-h" | "/?" => {
            print!("{HELP}");
            Ok(0)
        }
        "version" | "--version" => {
            println!("{} {}", bsm_core::APP_TITLE, bsm_core::VERSION);
            Ok(0)
        }
        "status" | "detect" => {
            let env = Env::discover(settings);
            print_status(&env);
            Ok(0)
        }
        "maps" => {
            let env = Env::discover(settings);
            let found = env.content();
            for m in &found.maps {
                println!("{}{}", m.label(), if m.name == env.settings.map { "   <- selected" } else { "" });
            }
            if found.maps.is_empty() {
                println!("No Zombies maps found. Is Black Ops III with Zombies installed?");
            }
            Ok(0)
        }
        "mods" => {
            let env = Env::discover(settings);
            let found = env.content();
            for m in &found.mods {
                println!(
                    "{:<11} {}{}",
                    m.id,
                    m.title,
                    if m.id == env.settings.mod_id { "   <- selected" } else { "" }
                );
            }
            if found.mods.is_empty() {
                println!("No workshop mods found. Subscribe to a mod in the Steam Workshop and let Steam download it.");
            }
            Ok(0)
        }
        "setup" => {
            server::validate_settings(&settings)?;
            settings.save(&data)?;
            let env = Env::discover(settings);
            match server::run_setup(
                &env,
                SetupOptions { download_client: !args.no_download, repair: args.repair },
                &log,
            ) {
                Ok(()) => Ok(0),
                Err(e @ Error::NeedServerFiles(_)) => {
                    eprintln!("{e}");
                    eprintln!("\nEither install it in Steam (steam://install/545990), or run:\n  bsm fetch-server --steam-user YOUR_STEAM_LOGIN");
                    Ok(2)
                }
                Err(e) => Err(e),
            }
        }
        "start" => {
            server::validate_settings(&settings)?;
            settings.save(&data)?;
            let env = Env::discover(settings);
            server::start(&env, &log)?;
            if let Some(secs) = args.wait {
                let st = server::wait_until_running(&env, Duration::from_secs(secs));
                println!("{}", st.summary());
                return Ok(if st.state == State::Running { 0 } else { 3 });
            }
            Ok(0)
        }
        "stop" => {
            let env = Env::discover(settings);
            server::stop(&env, &log)?;
            Ok(0)
        }
        "favorite" => {
            let env = Env::discover(settings);
            let (added, endpoint) = bsm_core::game::add_favorite(&env)?;
            println!(
                "{endpoint} {} the game's Favorites.",
                if added { "was added to" } else { "is already in" }
            );
            Ok(0)
        }
        "next" => {
            let env = Env::discover(settings);
            if !server::next_map(&env, &log)? {
                println!("There is no rotation that this tool switches (none set, or official maps only: the game changes those itself).");
            }
            Ok(0)
        }
        "saves" => {
            let env = Env::discover(settings);
            let saves = bsm_core::saves::list(&env.server_dir);
            if saves.is_empty() {
                println!(
                    "No saved games. Save from the mod menu in the game (Save / Load), or use --autosave on."
                );
            }
            for s in &saves {
                println!(
                    "slot {:<5} round {:<3} {:<22} {}{}",
                    s.slot,
                    s.round,
                    s.map,
                    bsm_core::saves::age_text(s.when),
                    if s.power { "   power on" } else { "" }
                );
                for p in &s.players {
                    println!(
                        "      {:<20} {} points, {} weapons, {} perks",
                        p.name,
                        p.points,
                        p.weapons.len(),
                        p.perks
                    );
                }
            }
            Ok(0)
        }
        "scores" => {
            let env = Env::discover(settings);
            let added = bsm_core::scores::import(&env.server_dir, &env.data_dir)?;
            if added > 0 {
                println!("{added} new game(s) added.");
            }
            let games = bsm_core::scores::best(bsm_core::scores::load(&env.data_dir));
            if games.is_empty() {
                println!("No finished games yet.");
            }
            for (i, g) in games.iter().take(20).enumerate() {
                println!(
                    "#{:<2} round {:<3} {:<22} {}",
                    i + 1,
                    g.round,
                    g.map,
                    bsm_core::scores::date_text(g.when)
                );
                for p in &g.players {
                    println!(
                        "      {:<20} {} kills, {} points, {} downs",
                        p.name, p.kills, p.points, p.downs
                    );
                }
            }
            Ok(0)
        }
        "play" => {
            let env = Env::discover(settings);
            let launched = match bsm_core::game::launch(&env, args.install_client, &log) {
                Err(Error::NeedClient(m)) => {
                    eprintln!(
                        "{}\n\nTo let this tool copy it there, run:  bsm play --install-client",
                        m.replace("\r\n", "\n")
                    );
                    return Ok(2);
                }
                other => other?,
            };
            let running = server::status(&env).state != State::Stopped;
            println!("{}", bsm_core::game::how_to_join(&launched, running).replace("\r\n", "\n"));
            Ok(0)
        }
        "join" => {
            let env = Env::discover(settings);
            let port = server::status(&env).port;
            println!(
                "{}",
                net::join_help(port, &env.settings.password, &net::addresses()).replace("\r\n", "\n")
            );
            Ok(0)
        }
        "fetch-server" => {
            if let Some(u) = args.steam_user {
                settings.steam_user = u;
            }
            settings.save(&data)?;
            let env = Env::discover(settings);
            if env.settings.server_dir.is_some() {
                return Err(Error::other(
                    "fetch-server installs into the default server folder. Run with --server-dir auto.",
                ));
            }
            let exe = steamcmd::ensure(&env.data_dir, &log)?;
            let root = env.data_dir.join("server");
            println!("\n{}\n", steamcmd::LOGIN_HELP.replace("\r\n", "\n"));
            loop {
                if let Some(dir) = steamcmd::run_once(&exe, &root, &env.settings.steam_user, false)? {
                    println!("\nServer files downloaded to {}. Now run: bsm setup", dir.display());
                    return Ok(0);
                }
                print!("\nThe server files are not complete yet (sign-in timed out or was refused?). Try again? [Y/n] ");
                let _ = std::io::stdout().flush();
                let mut answer = String::new();
                if std::io::stdin().lock().read_line(&mut answer).is_err()
                    || answer.trim().eq_ignore_ascii_case("n")
                {
                    return Ok(2);
                }
            }
        }
        "debug-proc" => {
            let env = Env::discover(settings);
            println!("client exe: {}", env.client_exe().display());
            for pid in bsm_core::proc::pids_by_name("boiii.exe") {
                println!("pid {pid}: {:?}", bsm_core::proc::info(pid));
            }
            println!("running_from: {:?}", bsm_core::proc::running_from(&env.client_exe()));
            Ok(0)
        }
        "import-data" => {
            let zip =
                args.positional.first().ok_or_else(|| Error::other("usage: bsm import-data FILE.zip"))?;
            let n = server::import_boiii_data(
                std::path::Path::new(zip),
                &paths::boiii_data_dir(),
                &data.join("tmp"),
            )?;
            println!("{n} files unpacked into {}", paths::boiii_data_dir().display());
            Ok(0)
        }
        other => Err(Error::other(format!("unknown command \"{other}\". Run: bsm help"))),
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {}", e.to_string().replace("\r\n", "\n"));
            std::process::exit(1);
        }
    }
}
