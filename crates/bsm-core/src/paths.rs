use std::env;
use std::path::PathBuf;

/// Folder that holds the running program.
pub fn exe_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `%LOCALAPPDATA%` of the current user.
pub fn local_app_data() -> PathBuf {
    if let Some(v) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(v);
    }
    if let Some(v) = env::var_os("USERPROFILE") {
        return PathBuf::from(v).join("AppData").join("Local");
    }
    exe_dir()
}

/// Where the tool keeps settings, logs, steamcmd and (by default) the server.
///
/// 1. `BSM_DATA_DIR` if set (used by tests and by people who want it elsewhere).
/// 2. `<program folder>\data` if a file named `portable.txt` sits next to the
///    program: the whole folder can then be moved or carried on a USB drive.
/// 3. `%LOCALAPPDATA%\BoiiiServerMaker` otherwise.
pub fn data_dir() -> PathBuf {
    if let Some(v) = env::var_os("BSM_DATA_DIR") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    let exe = exe_dir();
    if exe.join("portable.txt").exists() {
        return exe.join("data");
    }
    local_app_data().join(crate::APP_NAME)
}

/// The folder BOIII itself keeps its scripts and player data in.
pub fn boiii_data_dir() -> PathBuf {
    local_app_data().join("boiii")
}

/// Windows system folder, from the environment rather than a fixed `C:\Windows`.
pub fn system32() -> Option<PathBuf> {
    env::var_os("SystemRoot").or_else(|| env::var_os("windir")).map(|r| PathBuf::from(r).join("System32"))
}

/// Lower-case, backslash-separated form used to compare paths.
pub fn norm(p: &std::path::Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
    s.trim_end_matches('\\').to_string()
}
