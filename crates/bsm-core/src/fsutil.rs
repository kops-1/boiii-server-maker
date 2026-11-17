//! File helpers: a folder copy that recreates junctions instead of following
//! them, and zip extraction.

use crate::error::{io_ctx, Error, Result};
use std::fs;
use std::io::{Read, Seek};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// True for junctions and symbolic links (the entry itself, not its target).
pub fn is_reparse_point(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

/// Where a junction or directory symlink points.
pub fn link_target(path: &Path) -> Option<PathBuf> {
    if let Ok(t) = junction::get_target(path) {
        return Some(t);
    }
    let t = fs::read_link(path).ok()?;
    let s = t.to_string_lossy();
    let s = s.strip_prefix(r"\??\").or_else(|| s.strip_prefix(r"\\?\")).unwrap_or(&s);
    let t = PathBuf::from(s);
    Some(if t.is_absolute() { t } else { path.parent()?.join(t) })
}

/// Creates a directory junction at `link` pointing to `target`.
pub fn make_junction(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        io_ctx(fs::create_dir_all(parent), "create", parent)?;
    }
    junction::create(target, link)
        .map_err(|e| Error::Other(format!("create link {} -> {}: {e}", link.display(), target.display())))
}

#[derive(Debug, Default, Clone)]
pub struct CopyStats {
    pub files_copied: u64,
    pub files_skipped: u64,
    pub bytes_copied: u64,
    pub junctions_created: u64,
    /// Files that were present with the right size but different content
    /// (only counted when contents are verified).
    pub files_repaired: u64,
    pub warnings: Vec<String>,
}

/// True if both files have exactly the same content.
pub fn files_equal(a: &Path, b: &Path) -> std::io::Result<bool> {
    let mut fa = fs::File::open(a)?;
    let mut fb = fs::File::open(b)?;
    if fa.metadata()?.len() != fb.metadata()?.len() {
        return Ok(false);
    }
    let mut ba = vec![0u8; 1 << 20];
    let mut bb = vec![0u8; 1 << 20];
    loop {
        let n = read_full(&mut fa, &mut ba)?;
        let m = read_full(&mut fb, &mut bb)?;
        if n != m || ba[..n] != bb[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn read_full(f: &mut fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = f.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Asks Windows to write a just-copied file to the disk now. A copy
/// otherwise sits in memory for a while, and a power cut in that time leaves
/// a file of the right size filled with zeros.
fn flush_to_disk(path: &Path) {
    if let Ok(f) = fs::OpenOptions::new().write(true).open(path) {
        let _ = f.sync_all();
    }
}

/// True if a file that should hold data starts with nothing but zero bytes:
/// the mark of a copy that was cut off by a power loss.
pub fn looks_zeroed(path: &Path) -> bool {
    let Ok(mut f) = fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 64];
    match read_full(&mut f, &mut head) {
        Ok(n) if n > 0 => head[..n].iter().all(|b| *b == 0),
        _ => false,
    }
}

/// Copies `src` into `dst`.
///
/// * Linked folders (junctions, directory symlinks) are **not** followed. A
///   link to the same target is created instead, so linked workshop content
///   is not duplicated (on the reference PC that would have been ~65 GB).
/// * Files already present with the same size are skipped, so an interrupted
///   copy can simply be run again.
/// * `verify`: for source files it returns true for, the content is compared
///   as well and a file that differs is copied again (repair).
/// * `progress` is called with each file about to be copied or compared.
pub fn copy_tree(
    src: &Path,
    dst: &Path,
    verify: Option<&dyn Fn(&Path) -> bool>,
    progress: &mut dyn FnMut(&Path),
) -> Result<CopyStats> {
    let mut stats = CopyStats::default();
    copy_dir(src, dst, verify, progress, &mut stats)?;
    Ok(stats)
}

fn copy_dir(
    src: &Path,
    dst: &Path,
    verify: Option<&dyn Fn(&Path) -> bool>,
    progress: &mut dyn FnMut(&Path),
    stats: &mut CopyStats,
) -> Result<()> {
    io_ctx(fs::create_dir_all(dst), "create", dst)?;
    for entry in io_ctx(fs::read_dir(src), "read", src)? {
        let entry = io_ctx(entry, "read", src)?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let meta = io_ctx(fs::symlink_metadata(&from), "inspect", &from)?;
        let reparse = meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        let is_dir = meta.file_attributes() & 0x10 != 0; // FILE_ATTRIBUTE_DIRECTORY

        if reparse && is_dir {
            if fs::symlink_metadata(&to).is_ok() {
                stats.files_skipped += 1;
                continue;
            }
            match link_target(&from) {
                Some(target) if target.is_dir() => match make_junction(&target, &to) {
                    Ok(()) => stats.junctions_created += 1,
                    Err(e) => stats.warnings.push(e.to_string()),
                },
                Some(target) => stats.warnings.push(format!(
                    "skipped link {} (target {} is missing)",
                    from.display(),
                    target.display()
                )),
                None => stats.warnings.push(format!("skipped link {} (unreadable target)", from.display())),
            }
        } else if is_dir {
            copy_dir(&from, &to, verify, progress, stats)?;
        } else {
            // Regular file, or a file symlink (fs::copy copies its content).
            let len = fs::metadata(&from).map(|m| m.len()).unwrap_or(meta.len());
            let existing = fs::metadata(&to).ok().filter(|m| m.is_file()).map(|m| m.len());
            match (existing, verify) {
                // Plain copy: a file of the right size is taken as done.
                (Some(have), None) if have == len => {
                    stats.files_skipped += 1;
                    continue;
                }
                // Repair: files outside the verified group (logs, player data) belong
                // to the server by now and are never overwritten, whatever their size.
                (Some(_), Some(must_match)) if !must_match(&from) => {
                    stats.files_skipped += 1;
                    continue;
                }
                (Some(have), Some(_)) => {
                    progress(&from);
                    if have == len && files_equal(&from, &to).unwrap_or(false) {
                        stats.files_skipped += 1;
                        continue;
                    }
                    stats.files_repaired += 1;
                }
                _ => progress(&from),
            }
            io_ctx(fs::copy(&from, &to), "copy to", &to)?;
            flush_to_disk(&to);
            stats.files_copied += 1;
            stats.bytes_copied += len;
        }
    }
    Ok(())
}

/// Copies one file if the destination is missing or has a different size
/// or, with `verify`, different content. Returns true when a copy was made.
pub fn copy_if_different(from: &Path, to: &Path, verify: bool) -> Result<bool> {
    let len = io_ctx(fs::metadata(from), "inspect", from)?.len();
    let same_size = fs::metadata(to).map(|m| m.is_file() && m.len() == len).unwrap_or(false);
    if same_size && (!verify || files_equal(from, to).unwrap_or(false)) {
        return Ok(false);
    }
    if let Some(parent) = to.parent() {
        io_ctx(fs::create_dir_all(parent), "create", parent)?;
    }
    io_ctx(fs::copy(from, to), "copy to", to)?;
    flush_to_disk(to);
    Ok(true)
}

/// Extracts a zip archive into `dest`. Entries that would land outside
/// `dest` are refused. Returns the relative paths of the files written.
pub fn extract_zip<R: Read + Seek>(reader: R, dest: &Path) -> Result<Vec<PathBuf>> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| Error::Other(format!("not a valid zip archive: {e}")))?;
    let mut written = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| Error::Other(format!("zip entry {i}: {e}")))?;
        let Some(rel) = file.enclosed_name() else {
            return Err(Error::Other(format!("zip entry with unsafe path refused: {}", file.name())));
        };
        let out = dest.join(&rel);
        if file.is_dir() {
            io_ctx(fs::create_dir_all(&out), "create", &out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            io_ctx(fs::create_dir_all(parent), "create", parent)?;
        }
        let mut target = io_ctx(fs::File::create(&out), "write", &out)?;
        io_ctx(std::io::copy(&mut file, &mut target), "write", &out)?;
        written.push(rel);
    }
    Ok(written)
}

pub fn extract_zip_file(zip_path: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    let f = io_ctx(fs::File::open(zip_path), "open", zip_path)?;
    extract_zip(f, dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn copy_recreates_junctions_instead_of_following_them() {
        let tmp = tempfile::tempdir().unwrap();
        let workshop = tmp.path().join("workshop").join("123");
        fs::create_dir_all(&workshop).unwrap();
        fs::write(workshop.join("huge.ff"), vec![7u8; 4096]).unwrap();

        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("zone")).unwrap();
        fs::write(src.join("server.exe"), b"exe").unwrap();
        fs::write(src.join("zone").join("a.ff"), b"aaaa").unwrap();
        make_junction(&workshop, &src.join("mods").join("123").join("zone")).unwrap();
        assert!(is_reparse_point(&src.join("mods").join("123").join("zone")));

        let dst = tmp.path().join("dst");
        let mut seen = Vec::new();
        let stats = copy_tree(&src, &dst, None, &mut |p| seen.push(p.to_path_buf())).unwrap();

        assert_eq!(stats.files_copied, 2);
        assert_eq!(stats.bytes_copied, 7, "linked content must not be counted or copied");
        assert_eq!(stats.junctions_created, 1);
        assert!(stats.warnings.is_empty(), "{:?}", stats.warnings);
        let link = dst.join("mods").join("123").join("zone");
        assert!(is_reparse_point(&link), "destination must be a link, not a real folder");
        assert_eq!(crate::paths::norm(&link_target(&link).unwrap()), crate::paths::norm(&workshop));
        assert_eq!(fs::read(link.join("huge.ff")).unwrap().len(), 4096, "content reachable through the link");
        assert!(!seen.iter().any(|p| p.ends_with("huge.ff")));

        // Second run copies nothing.
        let again = copy_tree(&src, &dst, None, &mut |_| {}).unwrap();
        assert_eq!((again.files_copied, again.junctions_created), (0, 0));
        assert_eq!(again.files_skipped, 3);
    }

    #[test]
    fn verify_finds_and_repairs_a_file_zeroed_by_a_power_loss() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        fs::create_dir_all(src.join("zone")).unwrap();
        let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8 + 1).collect();
        fs::write(src.join("zone").join("map.ff"), &big).unwrap();
        fs::write(src.join("zone").join("other.ff"), b"other").unwrap();
        fs::write(src.join("notes.log"), b"source log").unwrap();
        copy_tree(&src, &dst, None, &mut |_| {}).unwrap();

        // Same size, content gone; and a log the server has rewritten since.
        fs::write(dst.join("zone").join("map.ff"), vec![0u8; big.len()]).unwrap();
        fs::write(dst.join("notes.log"), b"server log").unwrap();
        assert!(looks_zeroed(&dst.join("zone").join("map.ff")));
        assert!(!looks_zeroed(&src.join("zone").join("map.ff")));
        assert!(!looks_zeroed(&dst.join("missing")));
        assert!(!files_equal(&src.join("zone").join("map.ff"), &dst.join("zone").join("map.ff")).unwrap());

        // The normal run only looks at sizes and sees nothing wrong.
        let plain = copy_tree(&src, &dst, None, &mut |_| {}).unwrap();
        assert_eq!((plain.files_copied, plain.files_repaired), (0, 0));

        // Repair compares the game files and leaves the log alone.
        let only_ff = |p: &Path| p.extension().map(|e| e == "ff").unwrap_or(false);
        let fixed = copy_tree(&src, &dst, Some(&only_ff), &mut |_| {}).unwrap();
        assert_eq!((fixed.files_copied, fixed.files_repaired, fixed.files_skipped), (1, 1, 2));
        assert_eq!(fs::read(dst.join("zone").join("map.ff")).unwrap(), big);
        assert_eq!(fs::read(dst.join("notes.log")).unwrap(), b"server log");

        // A difference in the last byte of a large file is found too.
        let mut tail = big.clone();
        *tail.last_mut().unwrap() ^= 0xFF;
        fs::write(dst.join("zone").join("map.ff"), &tail).unwrap();
        assert!(copy_if_different(&src.join("zone").join("map.ff"), &dst.join("zone").join("map.ff"), true)
            .unwrap());
        assert!(!copy_if_different(&src.join("zone").join("map.ff"), &dst.join("zone").join("map.ff"), true)
            .unwrap());
        assert!(files_equal(&src.join("zone").join("map.ff"), &dst.join("zone").join("map.ff")).unwrap());
    }

    #[test]
    fn dangling_junction_is_a_warning_not_a_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        fs::create_dir_all(&target).unwrap();
        let src = tmp.path().join("src");
        make_junction(&target, &src.join("link")).unwrap();
        fs::remove_dir(&target).unwrap();
        let stats = copy_tree(&src, &tmp.path().join("dst"), None, &mut |_| {}).unwrap();
        assert_eq!(stats.warnings.len(), 1);
        assert!(!tmp.path().join("dst").join("link").exists());
    }

    #[test]
    fn zip_roundtrip_and_path_traversal_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opt = zip::write::SimpleFileOptions::default();
            w.start_file("a/b.txt", opt).unwrap();
            w.write_all(b"hello").unwrap();
            w.finish().unwrap();
        }
        buf.set_position(0);
        let files = extract_zip(buf, tmp.path()).unwrap();
        assert_eq!(files, vec![PathBuf::from("a/b.txt")]);
        assert_eq!(fs::read(tmp.path().join("a").join("b.txt")).unwrap(), b"hello");

        let mut evil = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut evil);
            w.start_file("../outside.txt", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        evil.set_position(0);
        assert!(extract_zip(evil, &tmp.path().join("in")).is_err());
        assert!(!tmp.path().join("outside.txt").exists());
    }
}
