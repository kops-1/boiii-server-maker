//! The installer is the setup program with a zip archive appended:
//!
//! ```text
//! [setup program][zip archive][zip length: u64 LE][magic: 8 bytes]
//! ```

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const MAGIC: &[u8; 8] = b"BSMPAYL1";

/// Writes `stub` followed by the payload and its footer to `out`.
pub fn write_installer(stub: &[u8], zip: &[u8], out: &Path) -> io::Result<()> {
    let mut f = File::create(out)?;
    f.write_all(stub)?;
    f.write_all(zip)?;
    f.write_all(&(zip.len() as u64).to_le_bytes())?;
    f.write_all(MAGIC)?;
    f.flush()
}

/// Reads the zip archive appended to `exe`. None if the file carries none.
pub fn read_payload(exe: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut f = File::open(exe)?;
    let total = f.metadata()?.len();
    if total < 16 {
        return Ok(None);
    }
    f.seek(SeekFrom::Start(total - 16))?;
    let mut footer = [0u8; 16];
    f.read_exact(&mut footer)?;
    if &footer[8..] != MAGIC {
        return Ok(None);
    }
    let len = u64::from_le_bytes(footer[..8].try_into().unwrap());
    if len > total - 16 {
        return Ok(None);
    }
    f.seek(SeekFrom::Start(total - 16 - len))?;
    let mut zip = vec![0u8; len as usize];
    f.read_exact(&mut zip)?;
    Ok(Some(zip))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("setup.exe");
        write_installer(b"MZ-stub-bytes", b"PK-zip-bytes", &out).unwrap();
        assert_eq!(read_payload(&out).unwrap().as_deref(), Some(&b"PK-zip-bytes"[..]));

        let plain = tmp.path().join("plain.exe");
        std::fs::write(&plain, b"MZ just a program without payload").unwrap();
        assert_eq!(read_payload(&plain).unwrap(), None);
        std::fs::write(&plain, b"tiny").unwrap();
        assert_eq!(read_payload(&plain).unwrap(), None);
    }
}
