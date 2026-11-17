//! Build helper: `cargo xtask package`
//!
//! Builds the release programs and produces, in `dist\`:
//!
//! * `BoiiiServerMakerSetup.exe`: the setup program with a zip of the files
//!   to install appended to it.
//! * `BoiiiServerMaker-portable.zip`: the same files to unpack anywhere.
//!
//! The installer contains only this project's own programs and a readme.
//! No game files, no BOIII client.

use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// (file inside the installer, file under target\release)
const PROGRAMS: [(&str, &str); 3] = [
    ("BoiiiServerMaker.exe", "BoiiiServerMaker.exe"),
    ("BoiiiServerMakerClassic.exe", "BoiiiServerMakerClassic.exe"),
    ("bsm.exe", "bsm.exe"),
];
const STUB: &str = "bsm-setup-stub.exe";
const OUTPUT: &str = "BoiiiServerMakerSetup.exe";
const PORTABLE_OUTPUT: &str = "BoiiiServerMaker-portable.zip";

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    // Optional second argument: file name of the installer inside dist\.
    let output = std::env::args().nth(2).unwrap_or_else(|| OUTPUT.to_string());
    let result = match task.as_str() {
        "package" if !output.contains(['/', '\\']) && output.to_lowercase().ends_with(".exe") => {
            package(&output)
        }
        _ => Err("usage: cargo xtask package [InstallerFileName.exe]".to_string()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask sits inside the workspace").to_path_buf()
}

fn package(output: &str) -> Result<(), String> {
    let root = workspace_root();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());

    // The programs that get shipped are built separately with source paths
    // rewritten, so the build PC's user name and folders are not embedded in
    // them (compilers store source paths for error messages).
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".cargo")));
    let mut flags = vec!["-C".to_string(), "target-feature=+crt-static".to_string()];
    flags.push(format!("--remap-path-prefix={}=.", root.display()));
    if let Some(home) = &cargo_home {
        flags.push(format!("--remap-path-prefix={}=cargo", home.display()));
    }
    let target_dir = root.join("target").join("package");

    println!("Building release programs...");
    let status = Command::new(cargo)
        .current_dir(&root)
        .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\x1f"))
        .env("CARGO_TARGET_DIR", &target_dir)
        .args(["build", "--release", "-p", "bsm-app", "-p", "bsm-setup"])
        .status()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if !status.success() {
        return Err("cargo build failed".into());
    }

    let release = target_dir.join("release");
    // Refuse to ship a program that still names this PC's user folder.
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        let needle = profile.to_string_lossy().to_lowercase().into_bytes();
        for file in PROGRAMS.iter().map(|(_, b)| *b).chain([STUB]) {
            let bytes =
                fs::read(release.join(file)).map_err(|e| format!("{file}: {e}"))?.to_ascii_lowercase();
            if needle.len() > 3 && bytes.windows(needle.len()).any(|w| w == &needle[..]) {
                return Err(format!(
                    "{file} contains the path {}; not packaging it",
                    profile.to_string_lossy()
                ));
            }
        }
    }
    println!("Installer files:");
    let zip_bytes = zip_programs(&root, &release, Layout::Installer)?;

    let stub_path = release.join(STUB);
    let stub = fs::read(&stub_path).map_err(|e| format!("{}: {e}", stub_path.display()))?;
    let dist = root.join("dist");
    fs::create_dir_all(&dist).map_err(|e| e.to_string())?;

    // The portable version: the same programs in a folder, plus `portable.txt`,
    // which makes them keep their data in a `data` folder beside them.
    let portable = dist.join(PORTABLE_OUTPUT);
    let portable_bytes = zip_programs(&root, &release, Layout::Portable)?;
    fs::write(&portable, &portable_bytes).map_err(|e| format!("{}: {e}", portable.display()))?;
    println!("Portable zip written: {} ({} KB)", portable.display(), portable_bytes.len() / 1024);

    let out = dist.join(output);
    bsm_core::payload::write_installer(&stub, &zip_bytes, &out)
        .map_err(|e| format!("{}: {e}", out.display()))?;

    // Read it back the way the installer will.
    let check = bsm_core::payload::read_payload(&out).map_err(|e| e.to_string())?;
    if check.as_deref() != Some(&zip_bytes[..]) {
        return Err("the written installer does not contain a readable payload".into());
    }
    let size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!("Installer written: {} ({} KB)", out.display(), size / 1024);
    Ok(())
}

/// How the shipped files are arranged inside a zip.
#[derive(Clone, Copy, PartialEq)]
enum Layout {
    /// Files at the top level; the installer unpacks them into the chosen folder.
    Installer,
    /// Everything inside a `BoiiiServerMaker` folder, plus the portable marker.
    Portable,
}

/// Zips the release programs and the user guide.
fn zip_programs(root: &Path, release: &Path, layout: Layout) -> Result<Vec<u8>, String> {
    let folder = match layout {
        Layout::Installer => "",
        Layout::Portable => "BoiiiServerMaker/",
    };
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (name, built) in PROGRAMS {
        let path = release.join(built);
        files.push((name.to_string(), fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?));
    }
    let readme = root.join("installer").join("README.txt");
    files.push(("README.txt".into(), fs::read(&readme).map_err(|e| format!("{}: {e}", readme.display()))?));
    if layout == Layout::Portable {
        files.push(("portable.txt".into(), Vec::new()));
    }

    let mut zip_bytes = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut zip_bytes);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in &files {
        zip.start_file(format!("{folder}{name}"), options).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())?;
        if layout == Layout::Installer {
            println!("  + {name} ({} KB)", bytes.len() / 1024);
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(zip_bytes.into_inner())
}
