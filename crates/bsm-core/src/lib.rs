//! Core logic of Boiii Server Maker: find Steam and Black Ops III, prepare a
//! BOIII dedicated server folder, write its config, and start/stop/query it.
//!
//! Nothing in here contains a fixed drive letter, user name or Steam library
//! path. Locations are discovered at run time (registry, `libraryfolders.vdf`)
//! and the tool's own data lives in the per-user app data folder.

pub mod content;
pub mod download;
pub mod error;
pub mod fsutil;
pub mod game;
pub mod net;
pub mod netcheck;
pub mod paths;
pub mod payload;
pub mod proc;
pub mod saves;
pub mod scores;
pub mod server;
pub mod settings;
pub mod steam;
pub mod steamcmd;
pub mod vdf;

pub use error::{Error, Result};

pub const APP_NAME: &str = "BoiiiServerMaker";
pub const APP_TITLE: &str = "Boiii Server Maker";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
