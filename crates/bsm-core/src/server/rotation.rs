//! Map rotations that this program drives itself (see [`tool_rotates`]).

use super::*;

/// True if the map changes of this rotation have to be done by this tool.
///
/// Measured on BOIII 3.0.0: the dedicated server crashes when it leaves a
/// workshop map for another map ("sound bank ... failed to load"), while
/// reloading the same map and going from an official map to a workshop map
/// both work. Rotations of official maps only are left to the game.
pub fn tool_rotates(sel: &Selection) -> bool {
    !sel.more.is_empty() && sel.all_maps().any(|m| m.kind == content::Kind::WorkshopMap)
}

pub(super) fn rotation_file(env: &Env) -> PathBuf {
    env.data_dir.join("rotation-position.txt")
}

/// Which map of a tool-driven rotation is up (counts on; taken modulo the length).
pub fn rotation_index(env: &Env) -> usize {
    fs::read_to_string(rotation_file(env)).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(0)
}

pub fn set_rotation_index(env: &Env, index: usize) -> Result<()> {
    io_ctx(fs::create_dir_all(&env.data_dir), "create", &env.data_dir)?;
    io_ctx(fs::write(rotation_file(env), index.to_string()), "write", &rotation_file(env))
}

/// For a tool-driven rotation: moves to the next map by restarting the
/// server. Returns false (and does nothing) when the game rotates by itself
/// or there is no rotation.
pub fn next_map(env: &Env, log: &dyn Fn(&str)) -> Result<bool> {
    let found = env.content();
    let sel = content::select_list(&found, &env.settings.play_list(), &env.settings.mod_id)?;
    if !tool_rotates(&sel) {
        return Ok(false);
    }
    let count = 1 + sel.more.len();
    set_rotation_index(env, (rotation_index(env) + 1) % count)?;
    stop(env, log)?;
    start(env, log)?;
    Ok(true)
}
