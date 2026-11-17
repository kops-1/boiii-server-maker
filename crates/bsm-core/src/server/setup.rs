//! Setup: server files, the BOIII client, Zombies files, workshop content
//! and the config, in that order. Every step is skipped when already done.

use super::*;

#[derive(Debug, Clone, Copy)]
pub struct SetupOptions {
    /// Download `boiii.exe` when no copy is found on this PC.
    pub download_client: bool,
    /// Compare the content of the game files with their source (the Steam
    /// server install and the game) and copy damaged ones again. Slower: it
    /// reads every file. Without it only missing files and wrong sizes are
    /// noticed.
    pub repair: bool,
}

impl Default for SetupOptions {
    fn default() -> Self {
        SetupOptions { download_client: true, repair: false }
    }
}

/// Files whose content never changes after install, so a difference from
/// the source means damage. Logs, player data and `boiii.exe` (which updates
/// itself) are not in this group.
pub(super) fn is_static_game_file(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name == CLIENT_EXE {
        return false;
    }
    matches!(
        path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(),
        Some("ff" | "fd" | "xpak" | "exe" | "dll" | "bmp")
    )
}

/// Game files in the server folder that are plainly damaged (they start with
/// zero bytes, as after a power loss during or right after a copy). A quick
/// look at the files this start needs; `Repair` does the full comparison.
pub fn damaged_files(env: &Env, map_file: Option<&Path>) -> Vec<PathBuf> {
    let zone = env.zone_dir();
    let mut candidates: Vec<PathBuf> = ZM_REQUIRED.iter().map(|f| zone.join(f)).collect();
    candidates.push(env.server_dir.join(SERVER_EXE));
    candidates.push(env.client_exe());
    if let Some(m) = map_file {
        candidates.push(m.to_path_buf());
    }
    candidates.into_iter().filter(|p| fsutil::looks_zeroed(p)).collect()
}

/// Where the `.ff` of the selected map is read from by the server.
pub fn map_file(env: &Env, sel: &Selection) -> PathBuf {
    match sel.map.kind {
        content::Kind::OfficialMap => env.zone_dir().join(format!("{}.ff", sel.map.name)),
        _ => sel.map.path.join(format!("{}.ff", sel.map.name)),
    }
}

pub(super) fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1_048_576.0)
}

/// Step 1: the dedicated server files.
pub fn ensure_server_files(env: &Env, opts: SetupOptions, log: &dyn Fn(&str)) -> Result<()> {
    if let Some(g) = &env.game_dir {
        if norm(g) == norm(&env.server_dir) {
            return Err(Error::Other("The server folder must not be the Black Ops III game folder.".into()));
        }
    }
    if env.has_server_files() && !opts.repair {
        log(&format!("Server files present: {}", env.server_dir.display()));
        return Ok(());
    }
    let Some(src) = &env.steam_server_dir else {
        if env.has_server_files() {
            log("Repair: the Steam install of the dedicated server is not on this PC, so the server's own files cannot be compared with it. The Zombies files are still compared with the game.");
            return Ok(());
        }
        return Err(Error::NeedServerFiles(
            "The dedicated server files (Steam tool \"Call of Duty: Black Ops III - Unranked Dedicated Server\", \
             app 545990) are not on this PC. Install them through Steam (Library > Tools) or download them with \
             steamcmd, then run Setup again."
                .into(),
        ));
    };
    if norm(src) == norm(&env.server_dir) {
        return Ok(());
    }
    let repairing = opts.repair && env.has_server_files();
    if repairing {
        log(&format!("Repair: comparing the server's game files with the Steam install at {} (this reads every file and can take a few minutes)...", src.display()));
    } else {
        log(&format!("Copying server files from the Steam install: {}", src.display()));
        log("Linked folders (workshop content, sounds) are linked again, not duplicated.");
    }
    let mut last = Instant::now();
    let mut count = 0u64;
    let verify: Option<&dyn Fn(&Path) -> bool> = if opts.repair { Some(&is_static_game_file) } else { None };
    let stats = fsutil::copy_tree(src, &env.server_dir, verify, &mut |p| {
        count += 1;
        if last.elapsed() > Duration::from_secs(2) {
            last = Instant::now();
            log(&format!(
                "  ... {count} files, now {}",
                p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            ));
        }
    })?;
    for w in &stats.warnings {
        log(&format!("  note: {w}"));
    }
    if repairing {
        log(&format!(
            "Repair: {} damaged file{} replaced, {} missing file{} copied ({}).",
            stats.files_repaired,
            if stats.files_repaired == 1 { "" } else { "s" },
            stats.files_copied - stats.files_repaired,
            if stats.files_copied - stats.files_repaired == 1 { "" } else { "s" },
            mb(stats.bytes_copied)
        ));
    } else {
        log(&format!(
            "Copied {} files ({}), {} already there, {} linked folders.",
            stats.files_copied,
            mb(stats.bytes_copied),
            stats.files_skipped,
            stats.junctions_created
        ));
    }
    if !env.has_server_files() {
        return Err(Error::Other(format!(
            "{SERVER_EXE} is still missing in {} after the copy.",
            env.server_dir.display()
        )));
    }
    Ok(())
}

/// Step 2: `boiii.exe` inside the server folder. It is always started from
/// there by its full path, never through PATH (a stray copy in System32 would
/// otherwise win).
pub fn ensure_client(env: &Env, opts: SetupOptions, log: &dyn Fn(&str)) -> Result<()> {
    let dest = env.client_exe();
    if dest.is_file() {
        if !fsutil::looks_zeroed(&dest) {
            log(&format!("BOIII client present: {}", dest.display()));
            return Ok(());
        }
        // Zero-filled: it can never start, so replace it.
        log("The BOIII client in the server folder is damaged; replacing it.");
        io_ctx(fs::remove_file(&dest), "remove", &dest)?;
    }
    if let Some(src) = client_source(env).filter(|p| !fsutil::looks_zeroed(p)) {
        log(&format!("Copying the BOIII client from {}", src.display()));
        fsutil::copy_if_different(&src, &dest, false)?;
        return Ok(());
    }
    if !opts.download_client {
        return Err(Error::NeedClient(format!(
            "boiii.exe was not found. Download it from {} and put it in {}.",
            crate::download::CLIENT_PAGE,
            env.server_dir.display()
        )));
    }
    crate::download::fetch_client(&dest, &env.data_dir.join("tmp"), log)?;
    log(&format!("BOIII client saved to {}", dest.display()));
    Ok(())
}

/// Step 3: Zombies fast files (`*zm_*.ff`, `*zm_*.fd`) from the user's own
/// game install, plus a link to the game's sound folder.
pub fn copy_zombies_files(env: &Env, opts: SetupOptions, log: &dyn Fn(&str)) -> Result<()> {
    let zone = env.zone_dir();
    io_ctx(fs::create_dir_all(&zone), "create", &zone)?;
    match &env.game_dir {
        Some(game) => {
            let game_zone = game.join("zone");
            let names = list_zm_files(&game_zone);
            if names.is_empty() {
                log(&format!(
                    "No Zombies files in {}. Is the Zombies content installed in Steam?",
                    game_zone.display()
                ));
            }
            let (mut copied, mut bytes) = (0u32, 0u64);
            for name in &names {
                let from = game_zone.join(name);
                let to = zone.join(name);
                let len = fs::metadata(&from).map(|m| m.len()).unwrap_or(0);
                let present = fs::metadata(&to).map(|m| m.is_file() && m.len() == len).unwrap_or(false);
                // A file that starts with zeros is damaged whatever its size says.
                let check_content = opts.repair || (present && fsutil::looks_zeroed(&to));
                if fsutil::copy_if_different(&from, &to, check_content)? {
                    log(&format!(
                        "  {} {name} ({})",
                        if present { "replaced damaged" } else { "copied" },
                        mb(len)
                    ));
                    copied += 1;
                    bytes += len;
                }
            }
            log(&format!(
                "Zombies files: {} found in the game, {copied} copied ({}).",
                names.len(),
                mb(bytes)
            ));

            let snd_src = game_zone.join("snd");
            let snd_dst = zone.join("snd");
            if snd_src.is_dir() && fs::symlink_metadata(&snd_dst).is_err() {
                match fsutil::make_junction(&snd_src, &snd_dst) {
                    Ok(()) => log("Linked the game's sound folder into the server."),
                    Err(e) => {
                        log(&format!("  note: could not link the sound folder ({e}). The server still runs."))
                    }
                }
            }
        }
        None => log("Black Ops III was not found, so no Zombies files were copied."),
    }
    let missing = missing_required(&zone);
    if !missing.is_empty() {
        let msg = format!("Zombies files missing in the server folder: {}.", missing.join(", "));
        return Err(match env.game_dir {
            Some(_) => Error::Other(format!("{msg} Verify the Black Ops III files in Steam (Properties > Installed Files) and make sure Zombies is installed.")),
            None => Error::NeedGame(format!("{msg} Black Ops III was not found on this PC: install it in Steam or choose its folder.")),
        });
    }
    Ok(())
}

/// Finds the chosen map and mod and links all workshop content into the
/// server folder.
pub fn prepare_content(env: &Env, log: &dyn Fn(&str)) -> Result<Selection> {
    let found = env.content();
    let linked = content::link_workshop(&env.server_dir, &found, log);
    if linked > 0 {
        log(&format!(
            "Linked {linked} workshop item{} into the server folder (nothing copied).",
            if linked == 1 { "" } else { "s" }
        ));
    }
    let mut sel = content::select_list(&found, &env.settings.play_list(), &env.settings.mod_id)?;
    if !sel.more.is_empty() {
        log(&format!(
            "Rotation: {}",
            sel.all_maps().map(|m| m.name.as_str()).collect::<Vec<_>>().join(" > ")
        ));
    }
    if tool_rotates(&sel) {
        // The game crashes when it changes away from a workshop map, so the
        // server is given one map at a time and restarted for the next.
        let all: Vec<content::Item> = sel.all_maps().cloned().collect();
        let index = rotation_index(env) % all.len();
        log(&format!("Map {} of {} in the rotation. This program restarts the server on the next map when a game ends.", index + 1, all.len()));
        sel = Selection { map: all[index].clone(), mod_item: sel.mod_item, more: Vec::new() };
    }
    log(&format!(
        "Map: {}{}",
        sel.map.label(),
        sel.mod_item.as_ref().map(|m| format!("   Mod: {}", m.label())).unwrap_or_default()
    ));
    Ok(sel)
}

/// Step 4: write the config files.
pub fn write_config(env: &Env, sel: &Selection, log: &dyn Fn(&str)) -> Result<()> {
    let text = config_text(&env.settings, sel)?;
    let zone = env.zone_dir();
    io_ctx(fs::create_dir_all(&zone), "create", &zone)?;
    let path = env.cfg_path();
    io_ctx(fs::write(&path, text), "write", &path)?;
    let custom = zone.join(CUSTOM_CFG_NAME);
    if !custom.exists() || wiped_text(&custom) {
        io_ctx(
            fs::write(&custom, "// Your own server settings. Boiii Server Maker never overwrites this file.\n// Example:  set scr_firstGumFree \"0\"\n"),
            "write",
            &custom,
        )?;
    }
    log(&format!(
        "Config written: {} (map {}, port {})",
        path.display(),
        env.settings.map,
        env.settings.port
    ));
    Ok(())
}

pub(super) fn damaged_error(damaged: &[PathBuf]) -> Error {
    Error::Other(format!(
        "These files in the server folder are damaged (empty content, usually after a power cut or a full disk): {}. \
         Use \"Repair files\" (command line: bsm setup --repair). For a workshop map, let Steam verify or download it again.",
        damaged.iter().map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).collect::<Vec<_>>().join(", ")
    ))
}

/// True if a small text file this tool wrote has been wiped (contains zero bytes).
pub(super) fn wiped_text(path: &Path) -> bool {
    fs::read(path).map(|b| b.contains(&0)).unwrap_or(false)
}

/// Runs all setup steps. Safe to run again at any time: finished steps are
/// detected and skipped.
pub fn run_setup(env: &Env, opts: SetupOptions, log: &dyn Fn(&str)) -> Result<()> {
    log(&format!("Server folder: {}", env.server_dir.display()));
    if opts.repair && status(env).state != State::Stopped {
        return Err(Error::Other(
            "Stop the server first, then repair: files in use cannot be replaced safely.".into(),
        ));
    }
    ensure_server_files(env, opts, log)?;
    ensure_client(env, opts, log)?;
    copy_zombies_files(env, opts, log)?;
    let sel = prepare_content(env, log)?;
    let damaged = damaged_files(env, Some(&map_file(env, &sel)));
    if !damaged.is_empty() {
        return Err(damaged_error(&damaged));
    }
    install_scripts(env, log)?;
    write_config(env, &sel, log)?;
    if !crate::paths::boiii_data_dir().join("data").is_dir() {
        log("Note: BOIII's own data folder does not exist yet. BOIII downloads it on first start, which needs internet access.");
    }
    log("Setup complete.");
    Ok(())
}

/// Unpacks a BOIII data archive (such as `boiii-server-files.zip`) into
/// `%LOCALAPPDATA%\boiii`, for PCs where BOIII cannot download it itself.
pub fn import_boiii_data(zip_path: &Path, boiii_dir: &Path, work_dir: &Path) -> Result<u64> {
    let tmp = work_dir.join("boiii-data-import");
    if tmp.exists() {
        io_ctx(fs::remove_dir_all(&tmp), "clear", &tmp)?;
    }
    fsutil::extract_zip_file(zip_path, &tmp)?;
    let root = if tmp.join("boiii").join("data").is_dir() {
        tmp.join("boiii")
    } else if tmp.join("data").is_dir() {
        tmp.clone()
    } else {
        let _ = fs::remove_dir_all(&tmp);
        return Err(Error::Other("This archive does not contain a BOIII \"data\" folder.".into()));
    };
    let stats = fsutil::copy_tree(&root, boiii_dir, None, &mut |_| {})?;
    let _ = fs::remove_dir_all(&tmp);
    Ok(stats.files_copied)
}
