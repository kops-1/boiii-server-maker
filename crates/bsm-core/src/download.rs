//! File downloads using what every Windows already has (the system's own
//! HTTP stack, with `curl.exe` as a second attempt), so the tool needs no
//! bundled TLS library.

use crate::error::{io_ctx, Error, Result};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;

/// The BOIII client is an unofficial community program. It is never bundled
/// with this tool; it is fetched from the project's own release page.
pub const CLIENT_RELEASE_API: &str = "https://api.github.com/repos/Ezz-lol/boiii-free/releases/latest";
pub const CLIENT_DIRECT_URL: &str =
    "https://github.com/Ezz-lol/boiii-free/releases/latest/download/boiii.exe";
pub const CLIENT_PAGE: &str = "https://github.com/Ezz-lol/boiii-free/releases/latest";
pub const CLIENT_ASSET: &str = "boiii.exe";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[link(name = "urlmon")]
extern "system" {
    fn URLDownloadToFileW(
        caller: *mut c_void,
        url: *const u16,
        file: *const u16,
        reserved: u32,
        callback: *mut c_void,
    ) -> i32;
}
#[link(name = "wininet")]
extern "system" {
    fn DeleteUrlCacheEntryW(url: *const u16) -> i32;
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn via_urlmon(url: &str, dest: &Path) -> std::result::Result<(), String> {
    let wurl = wide(std::ffi::OsStr::new(url));
    let wdest = wide(dest.as_os_str());
    unsafe {
        // Avoid being handed an old copy from the system cache.
        DeleteUrlCacheEntryW(wurl.as_ptr());
        let hr =
            URLDownloadToFileW(std::ptr::null_mut(), wurl.as_ptr(), wdest.as_ptr(), 0, std::ptr::null_mut());
        if hr == 0 {
            Ok(())
        } else {
            Err(format!("Windows download failed (code 0x{:08X})", hr as u32))
        }
    }
}

fn via_curl(url: &str, dest: &Path) -> std::result::Result<(), String> {
    let curl = crate::paths::system32()
        .map(|d| d.join("curl.exe"))
        .filter(|p| p.is_file())
        .ok_or("curl.exe not available")?;
    let out = std::process::Command::new(curl)
        .args(["-L", "--fail", "--silent", "--show-error", "--user-agent", "BoiiiServerMaker", "-o"])
        .arg(dest)
        .arg(url)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("curl: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Downloads `url` to `dest` (written to `<dest>.part` first, so a broken
/// download never leaves a half file under the real name).
pub fn download(url: &str, dest: &Path) -> Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(Error::Other(format!("refusing to download from non-web address: {url}")));
    }
    if let Some(parent) = dest.parent() {
        io_ctx(fs::create_dir_all(parent), "create", parent)?;
    }
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = std::path::PathBuf::from(part);
    let _ = fs::remove_file(&part);

    let first = via_urlmon(url, &part);
    let ok = match &first {
        Ok(()) => true,
        Err(_) => {
            let _ = fs::remove_file(&part);
            via_curl(url, &part).is_ok()
        }
    };
    if !ok || !part.is_file() {
        let _ = fs::remove_file(&part);
        return Err(Error::Other(format!(
            "could not download {url}: {}. Check the internet connection.",
            first.err().unwrap_or_else(|| "no file received".into())
        )));
    }
    let _ = fs::remove_file(dest);
    io_ctx(fs::rename(&part, dest), "move download to", dest)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = io_ctx(fs::File::open(path), "open", path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = io_ctx(f.read(&mut buf), "read", path)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Download address and (if published) SHA-256 of the client in a GitHub
/// "latest release" answer.
pub fn client_asset_from_release(json: &str) -> Option<(String, Option<String>)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    for asset in v.get("assets")?.as_array()? {
        if asset.get("name").and_then(|n| n.as_str()).map(|n| n.eq_ignore_ascii_case(CLIENT_ASSET))
            != Some(true)
        {
            continue;
        }
        let url = asset.get("browser_download_url")?.as_str()?.to_string();
        if !url.starts_with("https://github.com/") {
            return None;
        }
        let digest = asset
            .get("digest")
            .and_then(|d| d.as_str())
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(|d| d.to_lowercase());
        return Some((url, digest));
    }
    None
}

/// True if the file starts like a Windows program and is not tiny.
pub fn looks_like_program(path: &Path) -> bool {
    let Ok(mut f) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2];
    f.read_exact(&mut magic).is_ok()
        && &magic == b"MZ"
        && f.metadata().map(|m| m.len() > 512 * 1024).unwrap_or(false)
}

/// Downloads the BOIII client to `dest`, checking the published checksum
/// when the release lists one.
pub fn fetch_client(dest: &Path, work_dir: &Path, log: &dyn Fn(&str)) -> Result<()> {
    io_ctx(fs::create_dir_all(work_dir), "create", work_dir)?;
    let json_path = work_dir.join("boiii-release.json");
    let (url, digest) = match download(CLIENT_RELEASE_API, &json_path)
        .ok()
        .and_then(|_| fs::read_to_string(&json_path).ok())
        .and_then(|t| client_asset_from_release(&t))
    {
        Some(found) => found,
        None => {
            log("Release information not available; using the direct download address without a checksum.");
            (CLIENT_DIRECT_URL.to_string(), None)
        }
    };
    let _ = fs::remove_file(&json_path);

    log(&format!("Downloading the BOIII client from {url}"));
    let tmp = work_dir.join("boiii.exe.download");
    download(&url, &tmp).map_err(|e| Error::NeedClient(format!("{e} You can also download boiii.exe yourself from {CLIENT_PAGE} and put it in the server folder.")))?;

    let verdict = (|| {
        if !looks_like_program(&tmp) {
            return Err("the downloaded file is not a Windows program".to_string());
        }
        if let Some(want) = &digest {
            let got = sha256_file(&tmp).map_err(|e| e.to_string())?;
            if &got != want {
                return Err(format!("checksum mismatch (expected {want}, got {got})"));
            }
            log("Checksum matches the one published with the release.");
        }
        Ok(())
    })();
    if let Err(why) = verdict {
        let _ = fs::remove_file(&tmp);
        return Err(Error::NeedClient(format!("BOIII client download rejected: {why}.")));
    }
    if let Some(parent) = dest.parent() {
        io_ctx(fs::create_dir_all(parent), "create", parent)?;
    }
    io_ctx(fs::copy(&tmp, dest), "copy to", dest)?;
    let _ = fs::remove_file(&tmp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// Serves one fixed answer to every request on a local port.
    fn serve(status: &'static str, body: &'static [u8], hits: usize) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(hits) {
                let Ok(mut s) = stream else { continue };
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let _ = write!(
                    s,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(body);
            }
        });
        port
    }

    #[test]
    fn downloads_from_a_local_web_server() {
        let tmp = tempfile::tempdir().unwrap();
        let port = serve("200 OK", b"file-content-123", 4);
        let dest = tmp.path().join("sub").join("out.bin");
        download(&format!("http://127.0.0.1:{port}/x.bin"), &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"file-content-123");
        assert!(!tmp.path().join("sub").join("out.bin.part").exists());
    }

    #[test]
    fn failed_download_leaves_nothing_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let port = serve("404 Not Found", b"nope", 4);
        let dest = tmp.path().join("out.bin");
        assert!(download(&format!("http://127.0.0.1:{port}/missing"), &dest).is_err());
        assert!(!dest.exists());
        assert!(download("file:///C:/x", &dest).is_err());
    }

    #[test]
    fn sha256_known_value() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("abc");
        fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn reads_release_json() {
        let json = r#"{"tag_name":"v3.0.0","assets":[
            {"name":"other.zip","browser_download_url":"https://github.com/a/b/releases/download/v1/other.zip"},
            {"name":"boiii.exe","digest":"sha256:ABCDEF","browser_download_url":"https://github.com/Ezz-lol/boiii-free/releases/download/v3.0.0/boiii.exe"}]}"#;
        assert_eq!(
            client_asset_from_release(json),
            Some((
                "https://github.com/Ezz-lol/boiii-free/releases/download/v3.0.0/boiii.exe".into(),
                Some("abcdef".into())
            ))
        );
        assert_eq!(client_asset_from_release(r#"{"assets":[]}"#), None);
        assert_eq!(client_asset_from_release("rate limited"), None);
        let evil =
            r#"{"assets":[{"name":"boiii.exe","browser_download_url":"https://example.com/boiii.exe"}]}"#;
        assert_eq!(client_asset_from_release(evil), None, "only github.com downloads are accepted");
    }
}
