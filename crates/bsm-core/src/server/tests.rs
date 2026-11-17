use super::*;
use std::cell::RefCell;

/// A fake PC inside a temp folder: Steam with two libraries, the game in
/// one, the Steam-installed server (with a linked workshop folder) in the other.
struct Fake {
    _tmp: tempfile::TempDir,
    libs: Vec<PathBuf>,
    game: PathBuf,
    steam_server: PathBuf,
    data: PathBuf,
}

fn fake(with_game: bool, with_server: bool, client_in_game: bool) -> Fake {
    let tmp = tempfile::tempdir().unwrap();
    let lib1 = tmp.path().join("Steam");
    let lib2 = tmp.path().join("Second Library");
    for l in [&lib1, &lib2] {
        fs::create_dir_all(l.join("steamapps").join("common")).unwrap();
    }
    let game = lib2.join("steamapps").join("common").join("Call of Duty Black Ops III");
    if with_game {
        let zone = game.join("zone");
        fs::create_dir_all(zone.join("snd")).unwrap();
        for f in [
            "zm_patch.ff",
            "zm_common.ff",
            "zm_levelcommon.ff",
            "zm_common.fd",
            "zm_zod.ff",
            "en_zm_zod.ff",
            "zm_zod_patch.fd",
        ] {
            fs::write(zone.join(f), f.as_bytes()).unwrap();
        }
        for f in ["mp_common.ff", "core_patch.ff", "cp_common.fd", "readme.txt"] {
            fs::write(zone.join(f), b"not zombies").unwrap();
        }
        fs::write(zone.join("snd").join("zm.sabs"), b"sound").unwrap();
        if client_in_game {
            fs::write(game.join(CLIENT_EXE), b"MZ client").unwrap();
        }
        fs::write(
            lib2.join("steamapps").join("appmanifest_311210.acf"),
            "\"AppState\"\n{\n\"installdir\" \"Call of Duty Black Ops III\"\n}",
        )
        .unwrap();
    }
    let steam_server =
        lib1.join("steamapps").join("common").join("Call of Duty Black Ops III 545990").join(SERVER_SUBDIR);
    if with_server {
        fs::create_dir_all(steam_server.join("zone")).unwrap();
        fs::write(steam_server.join(SERVER_EXE), b"MZ server").unwrap();
        fs::write(steam_server.join("zone").join("core_common.ff"), b"core").unwrap();
        let workshop = lib1.join("steamapps").join("workshop").join("content").join("311210").join("42");
        fs::create_dir_all(&workshop).unwrap();
        fs::write(workshop.join("big_map.ff"), vec![1u8; 10_000]).unwrap();
        fsutil::make_junction(&workshop, &steam_server.join("usermaps").join("42").join("zone")).unwrap();
        fs::write(
            lib1.join("steamapps").join("appmanifest_545990.acf"),
            "\"AppState\"\n{\n\"installdir\" \"Call of Duty Black Ops III 545990\"\n}",
        )
        .unwrap();
    }
    let data = tmp.path().join("app data");
    Fake { libs: vec![lib1, lib2], game, steam_server, data, _tmp: tmp }
}

fn env_of(f: &Fake, settings: Settings) -> Env {
    Env::new(settings, f.data.clone(), f.libs.clone())
}

/// Default settings with the scoreboard off: a server with no script of ours.
fn plain() -> Settings {
    Settings { scoreboard: false, ..Default::default() }
}

fn no_download() -> SetupOptions {
    SetupOptions { download_client: false, repair: false }
}

#[test]
fn power_loss_damage_is_noticed_and_repaired() {
    let f = fake(true, true, true);
    // Give the fake files real-looking content (the helper writes short text).
    let big = vec![7u8; 700_000];
    let mut exe = b"MZ".to_vec();
    exe.extend_from_slice(&big);
    fs::write(f.game.join(CLIENT_EXE), &exe).unwrap();
    fs::write(f.steam_server.join(SERVER_EXE), &exe).unwrap();
    fs::write(f.game.join("zone").join("zm_zod.ff"), &big).unwrap();
    fs::write(f.steam_server.join("zone").join("core_common.ff"), &big).unwrap();
    let env = env_of(&f, Settings::default());
    run_setup(&env, no_download(), &|_| {}).unwrap();
    let zone = env.zone_dir();
    fs::write(zone.join(CUSTOM_CFG_NAME), "set my_setting \"1\"\n").unwrap();

    // What a power cut right after the copy leaves behind: right sizes, no content.
    let zero = |p: &Path| fs::write(p, vec![0u8; fs::metadata(p).unwrap().len() as usize]).unwrap();
    zero(&zone.join("zm_zod.ff"));
    zero(&zone.join("core_common.ff"));
    zero(&env.client_exe());
    zero(&env.server_dir.join(LOBBY_SCRIPT_REL));
    // A log the server wrote must survive the repair.
    fs::write(env.server_dir.join("games_mp.log"), b"server wrote this").unwrap();
    fs::write(f.steam_server.join("games_mp.log"), b"old log from steam").unwrap();

    // The checklist says so, and start refuses instead of hanging at "loading".
    let all = checks(&env);
    assert!(
        all.iter().any(|c| c.name == "Zombies files"
            && c.state == CheckState::Missing
            && c.detail.contains("boiii.exe")),
        "{}",
        format_checks(&all)
    );
    let e = start(&env, &|_| {}).unwrap_err().to_string();
    assert!(e.contains("damaged") && e.contains("zm_zod.ff") && e.contains("Repair"), "{e}");

    // A normal setup already fixes what it can see cheaply: zeroed Zombies files, client and lobby script.
    run_setup(&env, no_download(), &|_| {}).unwrap();
    assert_eq!(fs::read(zone.join("zm_zod.ff")).unwrap(), big);
    assert_eq!(fs::read(env.client_exe()).unwrap(), exe);
    assert_eq!(fs::read_to_string(env.server_dir.join(LOBBY_SCRIPT_REL)).unwrap(), LOBBY_SCRIPT);
    // ...but not a damaged file of the server itself: that takes the full comparison.
    assert!(fsutil::looks_zeroed(&zone.join("core_common.ff")));

    let lines = RefCell::new(Vec::<String>::new());
    run_setup(&env, SetupOptions { download_client: false, repair: true }, &|l| {
        lines.borrow_mut().push(l.to_string())
    })
    .unwrap();
    assert_eq!(fs::read(zone.join("core_common.ff")).unwrap(), big);
    assert!(lines.borrow().iter().any(|l| l.contains("1 damaged file replaced")), "{:?}", lines.borrow());
    assert_eq!(fs::read(env.server_dir.join("games_mp.log")).unwrap(), b"server wrote this");
    assert_eq!(fs::read_to_string(zone.join(CUSTOM_CFG_NAME)).unwrap(), "set my_setting \"1\"\n");
    assert!(damaged_files(&env, None).is_empty());

    // A wiped custom config is replaced by the default one.
    zero(&zone.join(CUSTOM_CFG_NAME));
    run_setup(&env, no_download(), &|_| {}).unwrap();
    assert!(fs::read_to_string(zone.join(CUSTOM_CFG_NAME))
        .unwrap()
        .starts_with("// Your own server settings"));
}

#[test]
fn full_setup_on_a_fresh_data_folder() {
    let f = fake(true, true, true);
    let env = env_of(&f, plain());
    assert_eq!(env.game_dir.as_ref(), Some(&f.game));
    assert_eq!(env.steam_server_dir.as_ref(), Some(&f.steam_server));
    assert_eq!(env.server_dir, f.data.join("server").join("UnrankedServer"));
    assert!(checks(&env).iter().any(|c| c.name == "Server files" && c.state == CheckState::Todo));

    let lines = RefCell::new(Vec::<String>::new());
    run_setup(&env, no_download(), &|l| lines.borrow_mut().push(l.to_string())).unwrap();

    let s = &env.server_dir;
    assert!(s.join(SERVER_EXE).is_file());
    assert_eq!(fs::read(s.join(CLIENT_EXE)).unwrap(), b"MZ client");
    // Workshop link recreated, content not duplicated.
    let link = s.join("usermaps").join("42").join("zone");
    assert!(fsutil::is_reparse_point(&link));
    assert!(link.join("big_map.ff").is_file());
    // Only zombies files came from the game.
    let zone = s.join("zone");
    for f in ["zm_patch.ff", "zm_common.fd", "zm_zod.ff", "en_zm_zod.ff", "zm_zod_patch.fd", "core_common.ff"]
    {
        assert!(zone.join(f).is_file(), "{f}");
    }
    for f in ["mp_common.ff", "core_patch.ff", "cp_common.fd", "readme.txt"] {
        assert!(!zone.join(f).exists(), "{f} must not be copied");
    }
    assert!(fsutil::is_reparse_point(&zone.join("snd")));
    // Config.
    let cfg = fs::read_to_string(zone.join(CFG_NAME)).unwrap();
    assert!(cfg.contains("set sv_maprotation \"gametype zclassic map zm_zod\""));
    assert!(cfg.contains("set g_password \"\""));
    assert!(zone.join(CUSTOM_CFG_NAME).is_file());
    assert!(
        checks(&env).iter().all(|c| c.state == CheckState::Ok || c.state == CheckState::Info),
        "{}",
        format_checks(&checks(&env))
    );
    assert_eq!(env.content().maps.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), vec!["zm_zod"]);
    // A normal game: lobby script present, no menu script, no workshop id.
    assert!(s.join(LOBBY_SCRIPT_REL).is_file());
    assert!(!s.join(MENU_SCRIPT_REL).exists());
    assert!(cfg.contains("set workshop_id \"\"") && cfg.contains("set bsm_menu \"0\""));

    // Running it again changes nothing and does not fail; custom cfg survives.
    fs::write(zone.join(CUSTOM_CFG_NAME), "set x \"1\"\n").unwrap();
    lines.borrow_mut().clear();
    run_setup(&env, no_download(), &|l| lines.borrow_mut().push(l.to_string())).unwrap();
    assert!(lines.borrow().iter().any(|l| l.contains("0 copied")));
    assert_eq!(fs::read_to_string(zone.join(CUSTOM_CFG_NAME)).unwrap(), "set x \"1\"\n");
}

#[test]
fn no_paths_from_the_build_machine_are_used() {
    // Everything the setup touched must live inside the fake PC.
    let f = fake(true, true, true);
    let env = env_of(&f, Settings::default());
    let lines = RefCell::new(Vec::<String>::new());
    run_setup(&env, no_download(), &|l| lines.borrow_mut().push(l.to_string())).unwrap();
    let root = norm(f._tmp.path());
    for p in [
        &env.server_dir,
        env.game_dir.as_ref().unwrap(),
        env.steam_server_dir.as_ref().unwrap(),
        &env.cfg_path(),
    ] {
        assert!(norm(p).starts_with(&root), "{} is outside the test folder", p.display());
    }
    let cfg = fs::read_to_string(env.cfg_path()).unwrap();
    assert!(!cfg.contains(":\\"), "config must not contain absolute paths");
    assert!(!launch_arguments(27017, "").contains(":\\"));
}

fn add_workshop(f: &Fake, id: &str, kind: &str, folder: &str, title: &str) -> PathBuf {
    let dir = f.libs[0].join("steamapps").join("workshop").join("content").join("311210").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("workshop.json"),
        format!(r#"{{"FolderName":"{folder}","PublisherID":"{id}","Title":"{title}","Type":"{kind}"}}"#),
    )
    .unwrap();
    let ff = if kind == "mod" { "core_mod.ff".to_string() } else { format!("{folder}.ff") };
    fs::write(dir.join(ff), b"ff").unwrap();
    dir
}

#[test]
fn workshop_map_mod_and_menu_are_set_up_and_removed_again() {
    let f = fake(true, true, true);
    let town = add_workshop(&f, "1624257987", "map", "zm_town", "TOWN REIMAGINED");
    add_workshop(&f, "2631943123", "mod", "all_around_enhancement", "^1[ZM] ^3All-around Enhancement");
    let mut s = Settings {
        map: "zm_town".into(),
        mod_id: "2631943123".into(),
        menu: true,
        menu_players: " Ali , BOB ".into(),
        god_all: true,
        start_points: 30000,
        ..Default::default()
    };
    s.sanitize();
    let env = env_of(&f, s.clone());

    let lines = RefCell::new(Vec::<String>::new());
    run_setup(&env, no_download(), &|l| lines.borrow_mut().push(l.to_string())).unwrap();
    let srv = &env.server_dir;

    // Both workshop items are linked, not copied.
    let map_link = srv.join("usermaps").join("1624257987").join("zone");
    let mod_link = srv.join("mods").join("2631943123").join("zone");
    assert!(fsutil::is_reparse_point(&map_link) && fsutil::is_reparse_point(&mod_link));
    assert_eq!(norm(&fsutil::link_target(&map_link).unwrap()), norm(&town));
    assert!(map_link.join("zm_town.ff").is_file());

    let cfg = fs::read_to_string(env.cfg_path()).unwrap();
    for line in [
        "set sv_maprotation \"gametype zclassic map zm_town\"",
        "set workshop_id \"2631943123\"",
        "set bsm_menu \"1\"",
        "set bsm_menu_players \"ali,bob\"",
        "set bsm_god \"1\"",
        "set bsm_ammo \"0\"",
        "set bsm_start_points \"30000\"",
        "set bsm_start_perks \"0\"",
    ] {
        assert!(cfg.contains(line), "missing: {line}\n{cfg}");
    }
    assert_eq!(fs::read_to_string(srv.join(MENU_SCRIPT_REL)).unwrap(), MENU_SCRIPT);
    assert!(srv.join(LOBBY_SCRIPT_REL).is_file());
    let all = checks(&env);
    assert!(
        all.iter().any(|c| c.name == "Selected"
            && c.state == CheckState::Ok
            && c.detail.contains("zm_town")
            && c.detail.contains("All-around")),
        "{}",
        format_checks(&all)
    );
    assert!(all.iter().any(|c| c.name == "Mod menu"
        && c.detail.contains("menu for ali,bob")
        && c.detail.contains("god mode")));

    // Workshop map without a mod: fs_game "usermaps", the map's own id.
    let mut s2 = s.clone();
    s2.mod_id.clear();
    let sel = content::select(&env_of(&f, s2.clone()).content(), "zm_town", "").unwrap();
    assert_eq!(
        launch_arguments(27017, &sel.fs_game()),
        "-headless +set fs_game \"usermaps\" +set net_port \"27017\" +set logfile 2 +exec servermaker_zm.cfg"
    );
    assert_eq!(sel.workshop_id(), "1624257987");

    // Everything switched off again: the menu script goes away, a user-edited lobby script stays.
    fs::write(srv.join(LOBBY_SCRIPT_REL), "-- edited by the user").unwrap();
    let env_off = env_of(&f, plain());
    run_setup(&env_off, no_download(), &|_| {}).unwrap();
    assert!(!srv.join(MENU_SCRIPT_REL).exists());
    assert_eq!(fs::read_to_string(srv.join(LOBBY_SCRIPT_REL)).unwrap(), "-- edited by the user");
    let cfg = fs::read_to_string(env_off.cfg_path()).unwrap();
    assert!(
        cfg.contains("set workshop_id \"\"")
            && cfg.contains("set bsm_menu \"0\"")
            && cfg.contains("map zm_zod\"")
    );

    // A rotation with a workshop map: one map at a time, switched by this tool.
    let mut r = plain();
    r.rotation = vec!["zm_zod".into(), "zm_town".into()];
    r.sanitize();
    let env_r = env_of(&f, r);
    let lines = RefCell::new(Vec::<String>::new());
    run_setup(&env_r, no_download(), &|l| lines.borrow_mut().push(l.to_string())).unwrap();
    let cfg = fs::read_to_string(env_r.cfg_path()).unwrap();
    assert!(
        cfg.contains("set sv_maprotation \"gametype zclassic map zm_zod\"\n")
            && !cfg.contains("sv_maprotationcurrent"),
        "{cfg}"
    );
    assert!(lines.borrow().iter().any(|l| l.contains("Map 1 of 2")));
    set_rotation_index(&env_r, 1).unwrap();
    run_setup(&env_r, no_download(), &|_| {}).unwrap();
    let cfg = fs::read_to_string(env_r.cfg_path()).unwrap();
    assert!(
        cfg.contains("set sv_maprotation \"gametype zclassic map zm_town\"\n")
            && cfg.contains("set workshop_id \"1624257987\""),
        "{cfg}"
    );
    set_rotation_index(&env_r, 2).unwrap();
    run_setup(&env_r, no_download(), &|_| {}).unwrap();
    assert!(fs::read_to_string(env_r.cfg_path()).unwrap().contains("map zm_zod\""), "wraps around");
    // Official maps only: the game rotates by itself.
    let official_only =
        content::select_list(&env_r.content(), &["zm_zod".to_string(), "zm_zod".to_string()], "").unwrap();
    assert!(!tool_rotates(&official_only));
    assert_eq!(official_only.rotation_rest().as_deref(), Some("gametype zclassic map zm_zod"));

    // A mod that is not installed is refused with the list of installed ones.
    let s3 = Settings { mod_id: "999".into(), ..Default::default() };
    let e = run_setup(&env_of(&f, s3), no_download(), &|_| {}).unwrap_err().to_string();
    assert!(e.contains("999") && e.contains("2631943123"), "{e}");
}

#[test]
fn menu_script_is_complete_and_matches_the_config() {
    // Every dvar the config writes is read by the script, and the other way round.
    for dvar in [
        "bsm_menu",
        "bsm_menu_players",
        "bsm_god",
        "bsm_ammo",
        "bsm_start_points",
        "bsm_start_perks",
        "bsm_instakill",
        "bsm_double_points",
        "bsm_keep_perks",
        "bsm_scoreboard",
        "bsm_autosave",
        "bsm_autoload",
    ] {
        assert!(MENU_SCRIPT.contains(&format!("(\"{dvar}\")")), "script does not read {dvar}");
        let s = Settings { menu: true, ..Default::default() };
        assert!(
            config_text(&s, &official("zm_zod")).unwrap().contains(&format!("set {dvar} ")),
            "config does not set {dvar}"
        );
    }
    // Every action a menu item names is handled, and braces balance.
    let mut actions = Vec::new();
    for line in MENU_SCRIPT.lines().filter(|l| l.contains("self bsm_add(")) {
        // bsm_add(label, "action", arg): the action is the last quoted text before the final argument.
        let body = line.trim().trim_end_matches(';').trim_end_matches(')');
        let (before_last_arg, _) = body.rsplit_once(", ").expect("three arguments");
        let quoted: Vec<&str> = before_last_arg.split('"').collect();
        actions.push(quoted[quoted.len() - 2].to_string());
    }
    assert!(actions.len() > 60, "expected a full menu, found {} items", actions.len());
    for a in actions.iter().filter(|a| *a != "menu" && *a != "target") {
        assert!(MENU_SCRIPT.contains(&format!("case \"{a}\":")), "menu action {a} has no handler");
    }
    for menu in [
        "main", "player", "points", "weapons", "give", "perks", "powerups", "zombies", "round", "map",
        "everyone", "players", "target",
    ] {
        assert!(MENU_SCRIPT.contains(&format!("case \"{menu}\":")), "menu {menu} is not built");
    }
    assert_eq!(MENU_SCRIPT.matches('{').count(), MENU_SCRIPT.matches('}').count());
    assert_eq!(MENU_SCRIPT.matches('(').count(), MENU_SCRIPT.matches(')').count());
    assert!(MENU_SCRIPT.is_ascii());
}

fn official(map: &str) -> Selection {
    Selection {
        map: content::Item {
            kind: content::Kind::OfficialMap,
            id: String::new(),
            name: map.into(),
            title: String::new(),
            path: PathBuf::new(),
        },
        mod_item: None,
        more: Vec::new(),
    }
}

#[test]
fn missing_server_files_is_reported_as_such() {
    let f = fake(true, false, true);
    let env = env_of(&f, Settings::default());
    match run_setup(&env, no_download(), &|_| {}) {
        Err(Error::NeedServerFiles(_)) => {}
        other => panic!("expected NeedServerFiles, got {other:?}"),
    }
    assert!(checks(&env).iter().any(|c| c.name == "Server files" && c.state == CheckState::Missing));
}

#[test]
fn missing_client_without_download_is_reported() {
    let f = fake(true, true, false);
    let env = env_of(&f, Settings::default());
    match run_setup(&env, no_download(), &|_| {}) {
        Err(Error::NeedClient(m)) => assert!(m.contains("github.com")),
        other => panic!("expected NeedClient, got {other:?}"),
    }
}

#[test]
fn missing_game_is_reported() {
    let f = fake(false, true, false);
    fs::write(f.steam_server.join(CLIENT_EXE), b"MZ client").unwrap();
    let env = env_of(&f, Settings::default());
    assert!(env.game_dir.is_none());
    match run_setup(&env, no_download(), &|_| {}) {
        Err(Error::NeedGame(_)) => {}
        other => panic!("expected NeedGame, got {other:?}"),
    }
}

#[test]
fn uninstalled_map_is_refused_with_the_list_of_installed_ones() {
    let f = fake(true, true, true);
    let s = Settings { map: "zm_moon".into(), ..Default::default() };
    let env = env_of(&f, s);
    let e = run_setup(&env, no_download(), &|_| {}).unwrap_err().to_string();
    assert!(e.contains("zm_moon") && e.contains("zm_zod"), "{e}");
}

#[test]
fn user_chosen_folders_win() {
    let f = fake(true, true, true);
    let s = Settings {
        server_dir: Some(f._tmp.path().join("my server")),
        game_dir: Some(f._tmp.path().join("not a game")),
        ..Default::default()
    };
    let env = env_of(&f, s);
    assert_eq!(env.server_dir, f._tmp.path().join("my server"));
    assert_eq!(env.game_dir.as_ref(), Some(&f.game), "invalid game folder falls back to discovery");
}

#[test]
fn config_rejects_injection_and_honours_settings() {
    let s = Settings {
        server_name: "My ^1Red Server".into(),
        password: "secret".into(),
        max_players: 2,
        map: "zm_factory".into(),
        ..Default::default()
    };
    let cfg = config_text(&s, &official("zm_factory")).unwrap();
    assert!(cfg.contains("set live_steam_server_name \"My ^1Red Server\""));
    assert!(cfg.contains("set g_password \"secret\""));
    assert!(cfg.contains("set com_maxclients \"2\""));
    assert!(cfg.contains("map zm_factory\""));

    for (name, pw, map) in [
        ("a\"b", "", "zm_zod"),
        ("ok", "x;quit", "zm_zod"),
        ("ok", "", "zm_zod; quit"),
        ("ok", "", "../x"),
        ("a\nb", "", "zm_zod"),
        ("ok", "a b", "zm_zod"),
    ] {
        let s =
            Settings { server_name: name.into(), password: pw.into(), map: map.into(), ..Default::default() };
        assert!(validate_settings(&s).is_err(), "{name:?} {pw:?} {map:?}");
    }
    let mut s = Settings::default();
    assert!(validate_settings(&s).is_ok());
    s.menu_players = "ali\"; quit".into();
    assert!(validate_settings(&s).is_err());
    s.menu_players = "ali, bob".into();
    s.mod_id = "12 +quit".into();
    assert!(validate_settings(&s).is_err());
}

#[test]
fn launch_line_matches_the_known_working_one() {
    assert_eq!(
        launch_arguments(27017, ""),
        "-headless +set fs_game \"\" +set net_port \"27017\" +set logfile 2 +exec servermaker_zm.cfg"
    );
    assert!(launch_arguments(27030, "").contains("\"27030\""));
    assert!(launch_arguments(27017, "2631943123").contains("+set fs_game \"2631943123\""));
}

#[test]
fn status_is_stopped_and_stop_is_harmless_when_nothing_runs() {
    let f = fake(true, true, true);
    let env = env_of(&f, Settings::default());
    let st = status(&env);
    assert_eq!(st.state, State::Stopped);
    assert_eq!(st.summary(), "Server: stopped");
    assert_eq!(stop(&env, &|_| {}).unwrap(), 0);
    // Not set up yet: start refuses with a clear message.
    assert!(start(&env, &|_| {}).unwrap_err().to_string().contains("Run Setup"));
}

#[test]
fn stale_run_file_is_not_mistaken_for_a_running_server() {
    let f = fake(true, true, true);
    let env = env_of(&f, Settings::default());
    fs::create_dir_all(&env.data_dir).unwrap();
    // Our own pid, but wrong creation time and program: must not match.
    let run = RunInfo {
        pid: std::process::id(),
        created: 1,
        exe: env.client_exe(),
        port: 27017,
        map: "zm_zod".into(),
        log: PathBuf::from("x.log"),
    };
    fs::write(env.run_file(), serde_json::to_string(&run).unwrap()).unwrap();
    assert_eq!(status(&env).state, State::Stopped);
    assert_eq!(stop(&env, &|_| {}).unwrap(), 0, "must never kill an unrelated process");
    assert!(proc::info(std::process::id()).is_some());
}

#[test]
fn start_refuses_a_busy_port() {
    let f = fake(true, true, true);
    let held = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    let s = Settings { port: held.local_addr().unwrap().port(), ..Default::default() };
    let env = env_of(&f, s);
    run_setup(&env, no_download(), &|_| {}).unwrap();
    let e = start(&env, &|_| {}).unwrap_err().to_string();
    assert!(e.contains("already in use"), "{e}");
}

#[test]
fn imports_boiii_data_archive() {
    use std::io::Write;
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("boiii-server-files.zip");
    {
        let mut w = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let opt = zip::write::SimpleFileOptions::default();
        w.start_file("boiii/data/scripts/x.gsc", opt).unwrap();
        w.write_all(b"script").unwrap();
        w.finish().unwrap();
    }
    let target = tmp.path().join("LocalAppData").join("boiii");
    assert_eq!(import_boiii_data(&zip_path, &target, &tmp.path().join("work")).unwrap(), 1);
    assert_eq!(fs::read(target.join("data").join("scripts").join("x.gsc")).unwrap(), b"script");
    assert!(!tmp.path().join("work").join("boiii-data-import").exists());
}
