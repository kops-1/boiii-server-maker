use std::fmt;

/// Errors the user can act on are separate variants so the UI can offer the
/// matching next step.
#[derive(Debug)]
pub enum Error {
    /// The dedicated server files (Steam app 545990) are not on this PC yet.
    NeedServerFiles(String),
    /// Black Ops III itself was not found (needed for the Zombies files).
    NeedGame(String),
    /// No `boiii.exe` was found and it could not be downloaded.
    NeedClient(String),
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NeedServerFiles(m) | Error::NeedGame(m) | Error::NeedClient(m) | Error::Other(m) => {
                f.write_str(m)
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Other(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Other(e.to_string())
    }
}

/// Adds the path to an I/O error, which otherwise only says "Access is denied".
pub fn io_ctx<T>(r: std::io::Result<T>, what: &str, path: &std::path::Path) -> Result<T> {
    r.map_err(|e| Error::Other(format!("{what} {}: {e}", path.display())))
}
