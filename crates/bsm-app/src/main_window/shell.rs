//! Handing things over to Windows: open a folder in Explorer, open a URL.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn open_in_explorer(path: &Path) {
    let explorer = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("explorer.exe"))
        .unwrap_or_else(|| "explorer.exe".into());
    let _ = Command::new(explorer).arg(path).spawn();
}

pub fn open_url(url: &str) {
    let cmd = std::env::var_os("ComSpec").map(PathBuf::from).unwrap_or_else(|| "cmd.exe".into());
    let _ = Command::new(cmd).args(["/c", "start", "", url]).creation_flags(CREATE_NO_WINDOW).spawn();
}
