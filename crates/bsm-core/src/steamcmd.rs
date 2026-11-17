//! Fallback for PCs where the dedicated server is not installed through the
//! Steam client: Valve's `steamcmd` downloads app 545990.
//!
//! The tool never sees or stores a password. steamcmd runs in its own console
//! window and the user types the password (and approves Steam Guard) there.

use crate::error::{io_ctx, Error, Result};
use crate::steam::{APP_BO3_SERVER, SERVER_EXE, SERVER_SUBDIR};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

/// Valve's official download address for steamcmd.
pub const STEAMCMD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip";

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub const LOGIN_HELP: &str = "A black console window opens and asks for your Steam password. \
Type it there and press Enter (nothing is shown while you type; this tool never sees it).\r\n\
If your account uses the Steam mobile app (Steam Guard): open the app on your phone right away \
and approve the sign-in. steamcmd only waits a short time. If it gives up, choose Retry; \
after one successful sign-in steamcmd remembers this PC and does not ask again.\r\n\
If it asks for a Steam Guard code instead, type the code from the app or e-mail.\r\n\
The download is about 5 GB. Close the console window when it says it is done.";

pub fn steamcmd_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("steamcmd")
}

/// Steam account names: letters, digits, `_`, `.`, `-`, `@`. Anything else is
/// refused so it cannot be mistaken for another steamcmd command.
pub fn valid_user(user: &str) -> bool {
    !user.is_empty()
        && user.len() <= 64
        && user.chars().all(|c| c.is_ascii_alphanumeric() || "_.-@".contains(c))
}

/// Downloads and unpacks steamcmd if it is not there yet.
pub fn ensure(data_dir: &Path, log: &dyn Fn(&str)) -> Result<PathBuf> {
    let dir = steamcmd_dir(data_dir);
    let exe = dir.join("steamcmd.exe");
    if exe.is_file() {
        return Ok(exe);
    }
    log(&format!("Downloading steamcmd from Valve ({STEAMCMD_URL})"));
    let zip = dir.join("steamcmd.zip");
    crate::download::download(STEAMCMD_URL, &zip)?;
    crate::fsutil::extract_zip_file(&zip, &dir)?;
    let _ = fs::remove_file(&zip);
    if !exe.is_file() {
        return Err(Error::Other("steamcmd.zip did not contain steamcmd.exe".into()));
    }
    Ok(exe)
}

/// The steamcmd arguments for installing the server into `install_root`
/// (the server then lives in `<install_root>\UnrankedServer`).
pub fn arguments(install_root: &Path, user: &str) -> String {
    format!(
        "+force_install_dir \"{}\" +login {} +app_update {} validate +quit",
        install_root.display(),
        user,
        APP_BO3_SERVER
    )
}

pub fn installed_server(install_root: &Path) -> Option<PathBuf> {
    let s = install_root.join(SERVER_SUBDIR);
    s.join(SERVER_EXE).is_file().then_some(s)
}

/// Runs one steamcmd attempt and waits for it. `own_window` opens a separate
/// console that stays open until a key is pressed (for the window app); without
/// it steamcmd uses the current console (for the command-line tool).
/// Returns the server folder if the files are there afterwards.
pub fn run_once(
    steamcmd_exe: &Path,
    install_root: &Path,
    user: &str,
    own_window: bool,
) -> Result<Option<PathBuf>> {
    if !valid_user(user) {
        return Err(Error::Other(
            "Enter your Steam account name (the login name, not the display name).".into(),
        ));
    }
    if install_root.to_string_lossy().contains('"') {
        return Err(Error::Other("The server folder path contains a quote character.".into()));
    }
    io_ctx(fs::create_dir_all(install_root), "create", install_root)?;
    let args = arguments(install_root, user);
    let status = if own_window {
        let cmd = std::env::var_os("ComSpec")
            .map(PathBuf::from)
            .or_else(|| crate::paths::system32().map(|d| d.join("cmd.exe")))
            .ok_or_else(|| Error::Other("cmd.exe not found".into()))?;
        // cmd /c ""program" args & pause" : the outer quotes are removed by cmd.
        let line = format!(
            "/c \"\"{}\" {} & echo. & echo You can close this window. & pause\"",
            steamcmd_exe.display(),
            args
        );
        std::process::Command::new(cmd).raw_arg(line).creation_flags(CREATE_NEW_CONSOLE).status()
    } else {
        std::process::Command::new(steamcmd_exe).raw_arg(&args).status()
    };
    io_ctx(status, "run", steamcmd_exe)?;
    Ok(installed_server(install_root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_names() {
        assert!(valid_user("some_user-1.x"));
        assert!(valid_user("me@example.com"));
        for bad in ["", "a b", "x +quit", "a\"b", "a;b", "a&b"] {
            assert!(!valid_user(bad), "{bad}");
        }
    }

    #[test]
    fn argument_line() {
        let a = arguments(Path::new(r"X:\My Data\server"), "user1");
        assert_eq!(
            a,
            "+force_install_dir \"X:\\My Data\\server\" +login user1 +app_update 545990 validate +quit"
        );
        assert!(!a.to_lowercase().contains("anonymous"));
    }

    #[test]
    fn bad_user_is_refused_before_anything_runs() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(run_once(&tmp.path().join("steamcmd.exe"), tmp.path(), "x +quit", false).is_err());
        assert_eq!(installed_server(tmp.path()), None);
    }
}
