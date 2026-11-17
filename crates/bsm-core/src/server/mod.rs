//! Prepares the server folder, writes the config and starts/stops the server.
//!
//! * [`env`]: where things are on this PC ([`Env`]).
//! * [`checks`]: the checklist shown on the Setup page and by `bsm status`.
//! * [`setup`]: getting every file in place ([`run_setup`]).
//! * [`config`]: the generated `.cfg` and the in-game scripts.
//! * [`rotation`]: map rotations this program drives itself.
//! * [`lifecycle`]: start, status and stop of the server process.
//!
//! Everything public is re-exported here, so callers just use `server::start`.

use crate::content::{self, Content, Selection};
use crate::error::{io_ctx, Error, Result};
use crate::fsutil;
use crate::paths::norm;
use crate::settings::Settings;
use crate::steam::{self, SERVER_EXE, SERVER_SUBDIR};
use crate::{net, proc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::net::Ipv4Addr;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const CLIENT_EXE: &str = "boiii.exe";
/// Config written by this tool. Overwritten on every setup/start.
pub const CFG_NAME: &str = "servermaker_zm.cfg";
/// The user's own extra settings. Created once, never overwritten.
pub const CUSTOM_CFG_NAME: &str = "servermaker_custom.cfg";
/// Files every Zombies map needs.
pub const ZM_REQUIRED: [&str; 4] = ["zm_patch.ff", "zm_common.ff", "zm_levelcommon.ff", "zm_common.fd"];

pub use crate::content::OFFICIAL_MAPS;

/// The in-game mod menu and start options (written into the server folder
/// only while one of its options is switched on).
pub const MENU_SCRIPT: &str = include_str!("../../assets/servermaker_menu.gsc");
pub const MENU_SCRIPT_REL: &str = r"boiii\custom_scripts\zm\servermaker_menu.gsc";
/// Community lobby script from the BOIII server configuration: it makes
/// `sv_skip_lobby` load the map from `sv_maprotation`. Written only if the
/// server folder has none.
pub const LOBBY_SCRIPT: &str = include_str!("../../assets/server_lobby_selector.lua");
pub const LOBBY_SCRIPT_REL: &str = r"boiii\lobby_scripts\server_lobby_selector\__init__.lua";

pub(super) const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub(super) const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

mod checks;
mod config;
mod env;
mod lifecycle;
mod rotation;
mod setup;
#[cfg(test)]
mod tests;

pub use checks::*;
pub use config::*;
pub use env::*;
pub use lifecycle::*;
pub use rotation::*;
pub use setup::*;
