//! The installer. The files to install are a zip archive appended to this
//! program by `cargo xtask package` (see `bsm_core::payload`).
//!
//! Per-user install: no administrator rights, nothing written outside the
//! chosen folder, the user's Start Menu/Desktop and the user's own
//! "Apps & features" entry. The same program, copied as `uninstall.exe`,
//! removes exactly what it installed.
//!
//! Silent use:  BoiiiServerMakerSetup.exe --silent [--dir DIR] [--no-start-menu] [--desktop] [--launch]
//!              uninstall.exe --uninstall --silent
#![windows_subsystem = "windows"]

extern crate native_windows_gui as nwg;

use bsm_core::{paths, APP_NAME, APP_TITLE, VERSION};
use std::cell::RefCell;
use std::fs;
use std::io::{Cursor, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const MAIN_EXE: &str = "BoiiiServerMaker.exe";
const UNINSTALLER: &str = "uninstall.exe";
const MANIFEST: &str = "install-manifest.txt";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\BoiiiServerMaker";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
struct Options {
    dir: PathBuf,
    start_menu: bool,
    desktop: bool,
    launch: bool,
}

fn default_dir() -> PathBuf {
    paths::local_app_data().join("Programs").join(APP_NAME)
}

fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    unsafe {
        let mut ptr: *mut u16 = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut ptr);
        if hr != 0 || ptr.is_null() {
            return None;
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let path = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        CoTaskMemFree(ptr as *const _);
        Some(PathBuf::from(path))
    }
}

fn start_menu_link() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Programs)
        .map(|p| p.join(format!("{APP_TITLE}.lnk")))
}

fn desktop_link() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Desktop).map(|p| p.join(format!("{APP_TITLE}.lnk")))
}

fn make_shortcut(target: &Path, link: &Path) -> Result<(), String> {
    let mut sl = mslnk::ShellLink::new(target).map_err(|e| format!("shortcut: {e:?}"))?;
    sl.set_working_dir(target.parent().map(|p| p.to_string_lossy().to_string()));
    sl.set_name(Some(APP_TITLE.to_string()));
    sl.create_lnk(link).map_err(|e| format!("shortcut {}: {e:?}", link.display()))
}

fn install(opts: &Options) -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let payload = bsm_core::payload::read_payload(&me)
        .map_err(|e| e.to_string())?
        .ok_or("This installer file is incomplete (no program files inside). Download it again.")?;
    let dir = &opts.dir;
    if dir.as_os_str().is_empty() || !dir.is_absolute() {
        return Err("Choose a full folder path, for example a folder on your desktop or in Documents.".into());
    }
    fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;

    let files = bsm_core::fsutil::extract_zip(Cursor::new(payload), dir)
        .map_err(|e| format!("{e}\r\n\r\nIf {APP_TITLE} is running, close it and try again."))?;
    let mut manifest = String::new();
    for f in &files {
        manifest.push_str(&format!("file:{}\r\n", f.display()));
    }

    let uninstaller = dir.join(UNINSTALLER);
    if paths::norm(&me) != paths::norm(&uninstaller) {
        fs::copy(&me, &uninstaller).map_err(|e| format!("Cannot write {}: {e}", uninstaller.display()))?;
    }

    let target = dir.join(MAIN_EXE);
    let mut links = Vec::new();
    if opts.start_menu {
        links.extend(start_menu_link());
    }
    if opts.desktop {
        links.extend(desktop_link());
    }
    for link in links {
        // A shortcut that cannot be created is not worth failing the install.
        if make_shortcut(&target, &link).is_ok() {
            manifest.push_str(&format!("shortcut:{}\r\n", link.display()));
        }
    }
    fs::write(dir.join(MANIFEST), manifest).map_err(|e| format!("Cannot write the install list: {e}"))?;

    // Entry in Settings > Apps for this user.
    let size_kb: u64 =
        files.iter().filter_map(|f| fs::metadata(dir.join(f)).ok()).map(|m| m.len()).sum::<u64>() / 1024;
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    if let Ok((key, _)) = hkcu.create_subkey(UNINSTALL_KEY) {
        let q = |p: &Path| format!("\"{}\"", p.display());
        let _ = key.set_value("DisplayName", &APP_TITLE);
        let _ = key.set_value("DisplayVersion", &VERSION);
        let _ = key.set_value("Publisher", &"Boiii Server Maker");
        let _ = key.set_value("InstallLocation", &dir.display().to_string());
        let _ = key.set_value("DisplayIcon", &target.display().to_string());
        let _ = key.set_value("UninstallString", &format!("{} --uninstall", q(&uninstaller)));
        let _ = key.set_value("QuietUninstallString", &format!("{} --uninstall --silent", q(&uninstaller)));
        let _ = key.set_value("NoModify", &1u32);
        let _ = key.set_value("NoRepair", &1u32);
        let _ = key.set_value("EstimatedSize", &(size_kb as u32));
    }

    if opts.launch {
        let _ = std::process::Command::new(&target).current_dir(dir).spawn();
    }
    Ok(())
}

/// Removes what [`install`] put into `dir`. The user's server folder and
/// settings (in the data folder) are left alone.
fn uninstall(dir: &Path) -> Result<(), String> {
    let manifest_path = dir.join(MANIFEST);
    let manifest = fs::read_to_string(&manifest_path).map_err(|_| {
        format!("{} is not a {APP_TITLE} install folder (no {MANIFEST}). Nothing was removed.", dir.display())
    })?;
    let mut failed = Vec::new();
    for line in manifest.lines() {
        if let Some(rel) = line.strip_prefix("file:") {
            let rel = Path::new(rel);
            // Only plain relative names from our own list.
            if rel.is_absolute() || rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                continue;
            }
            let p = dir.join(rel);
            if p.exists() && fs::remove_file(&p).is_err() {
                failed.push(p.display().to_string());
            }
        } else if let Some(link) = line.strip_prefix("shortcut:") {
            let p = Path::new(link);
            if p.extension().map(|e| e.eq_ignore_ascii_case("lnk")).unwrap_or(false) {
                let _ = fs::remove_file(p);
            }
        }
    }
    if !failed.is_empty() {
        return Err(format!(
            "Could not remove:\r\n{}\r\n\r\nClose {APP_TITLE} and try again.",
            failed.join("\r\n")
        ));
    }
    let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY);

    // This program cannot delete itself while running: a short delayed
    // command removes it, the list and then the folder if it is empty.
    let cmd = std::env::var_os("ComSpec").map(PathBuf::from).unwrap_or_else(|| "cmd.exe".into());
    let line = format!(
        "/c \"ping -n 3 127.0.0.1 >nul & del /f /q \"{}\" \"{}\" & rmdir \"{}\"\"",
        dir.join(UNINSTALLER).display(),
        manifest_path.display(),
        dir.display()
    );
    let _ = std::process::Command::new(cmd)
        .raw_arg(line)
        .current_dir(std::env::temp_dir())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    Ok(())
}

fn silent_log(text: &str) {
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("BoiiiServerMakerSetup.log"))
    {
        let _ = writeln!(f, "{text}");
    }
}

#[derive(Default)]
struct Ui {
    window: nwg::Window,
    icon: Option<nwg::Icon>,
    labels: Vec<nwg::Label>,
    dir: nwg::TextInput,
    browse: nwg::Button,
    start_menu: nwg::CheckBox,
    desktop: nwg::CheckBox,
    launch: nwg::CheckBox,
    install: nwg::Button,
    cancel: nwg::Button,
    dialog: nwg::FileDialog,
    done: RefCell<bool>,
}

fn checked(c: &nwg::CheckBox) -> bool {
    c.check_state() == nwg::CheckBoxState::Checked
}

/// The program icon that `build.rs` embedded as resource 1.
fn embedded_icon() -> Option<nwg::Icon> {
    nwg::EmbedResource::load(None).ok()?.icon(1, None)
}

fn install_window(defaults: Options) -> Result<(), nwg::NwgError> {
    let mut ui = Ui { icon: embedded_icon(), ..Default::default() };
    nwg::Window::builder()
        .icon(ui.icon.as_ref())
        .size((580, 270))
        .position((300, 200))
        .title(&format!("{APP_TITLE} {VERSION} Setup"))
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .build(&mut ui.window)?;
    nwg::FileDialog::builder()
        .title("Install folder")
        .action(nwg::FileDialogAction::OpenDirectory)
        .build(&mut ui.dialog)?;
    let w = &ui.window;
    let mut labels = Vec::new();
    for (text, y, h) in [
        ("This installs Boiii Server Maker for your Windows account only. No administrator rights are needed.", 12, 20),
        ("It contains no game files. Black Ops III, the dedicated server files and the BOIII client are found or fetched on this PC by the tool itself.", 36, 40),
        ("Install to:", 88, 20),
    ] {
        let mut l = nwg::Label::default();
        nwg::Label::builder().text(text).position((12, y)).size((556, h)).parent(w).build(&mut l)?;
        labels.push(l);
    }
    nwg::TextInput::builder()
        .text(&defaults.dir.display().to_string())
        .position((12, 110))
        .size((456, 24))
        .parent(w)
        .build(&mut ui.dir)?;
    nwg::Button::builder()
        .text("Browse...")
        .position((476, 108))
        .size((92, 28))
        .parent(w)
        .build(&mut ui.browse)?;
    let on = |b: bool| if b { nwg::CheckBoxState::Checked } else { nwg::CheckBoxState::Unchecked };
    nwg::CheckBox::builder()
        .text("Start Menu shortcut")
        .check_state(on(defaults.start_menu))
        .position((12, 146))
        .size((250, 22))
        .parent(w)
        .build(&mut ui.start_menu)?;
    nwg::CheckBox::builder()
        .text("Desktop shortcut")
        .check_state(on(defaults.desktop))
        .position((12, 170))
        .size((250, 22))
        .parent(w)
        .build(&mut ui.desktop)?;
    nwg::CheckBox::builder()
        .text("Start Boiii Server Maker when finished")
        .check_state(on(defaults.launch))
        .position((12, 194))
        .size((320, 22))
        .parent(w)
        .build(&mut ui.launch)?;
    nwg::Button::builder()
        .text("Install")
        .position((366, 228))
        .size((96, 30))
        .parent(w)
        .build(&mut ui.install)?;
    nwg::Button::builder()
        .text("Cancel")
        .position((472, 228))
        .size((96, 30))
        .parent(w)
        .build(&mut ui.cancel)?;
    ui.labels = labels;

    let ui = Rc::new(ui);
    let h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |event, _data, handle| match event {
        nwg::Event::OnWindowClose if handle == h.window.handle => nwg::stop_thread_dispatch(),
        nwg::Event::OnButtonClick if handle == h.cancel.handle => nwg::stop_thread_dispatch(),
        nwg::Event::OnButtonClick if handle == h.browse.handle => {
            if h.dialog.run(Some(&h.window)) {
                if let Ok(picked) = h.dialog.get_selected_item() {
                    let mut p = PathBuf::from(picked);
                    // Install into a folder of its own, not loose into e.g. Documents.
                    if p.file_name().map(|n| n != APP_NAME).unwrap_or(true) {
                        p = p.join(APP_NAME);
                    }
                    h.dir.set_text(&p.display().to_string());
                }
            }
        }
        nwg::Event::OnButtonClick if handle == h.install.handle => {
            if *h.done.borrow() {
                return;
            }
            let opts = Options {
                dir: PathBuf::from(h.dir.text().trim()),
                start_menu: checked(&h.start_menu),
                desktop: checked(&h.desktop),
                launch: checked(&h.launch),
            };
            match install(&opts) {
                Ok(()) => {
                    *h.done.borrow_mut() = true;
                    nwg::modal_info_message(
                        &h.window,
                        APP_TITLE,
                        &format!("Installed to {}.\r\n\r\nTo remove it later: Windows Settings > Apps, or run uninstall.exe in that folder.", opts.dir.display()),
                    );
                    nwg::stop_thread_dispatch();
                }
                Err(e) => {
                    nwg::modal_error_message(&h.window, APP_TITLE, &e);
                }
            }
        }
        _ => {}
    });
    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&handler);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |names: &[&str]| args.iter().any(|a| names.iter().any(|n| a.eq_ignore_ascii_case(n)));
    let value = |name: &str| {
        args.iter().position(|a| a.eq_ignore_ascii_case(name)).and_then(|i| args.get(i + 1)).cloned()
    };
    let silent = has(&["--silent", "/S", "/silent", "/quiet"]);
    let me = std::env::current_exe().unwrap_or_default();
    let is_uninstaller = me.file_name().map(|n| n.eq_ignore_ascii_case(UNINSTALLER)).unwrap_or(false);
    let uninstall_mode = has(&["--uninstall", "/uninstall"]) || (is_uninstaller && !has(&["--install"]));

    if silent {
        let result = if uninstall_mode {
            uninstall(&paths::exe_dir())
        } else {
            install(&Options {
                dir: value("--dir").map(PathBuf::from).unwrap_or_else(default_dir),
                start_menu: !has(&["--no-start-menu"]),
                desktop: has(&["--desktop"]),
                launch: has(&["--launch"]),
            })
        };
        match result {
            Ok(()) => {
                silent_log(if uninstall_mode { "uninstall: ok" } else { "install: ok" });
                std::process::exit(0)
            }
            Err(e) => {
                silent_log(&format!("error: {e}"));
                std::process::exit(1)
            }
        }
    }

    if nwg::init().is_err() {
        std::process::exit(1);
    }
    let mut font = nwg::Font::default();
    if nwg::Font::builder().family("Segoe UI").size(16).build(&mut font).is_ok() {
        nwg::Font::set_global_default(Some(font));
    }

    if uninstall_mode {
        let params = nwg::MessageParams {
            title: APP_TITLE,
            content: &format!(
                "Remove {APP_TITLE} from this PC?\r\n\r\nYour server folder, settings and logs in\r\n{}\r\nare kept. Delete that folder yourself if you no longer want them.",
                paths::local_app_data().join(APP_NAME).display()
            ),
            buttons: nwg::MessageButtons::YesNo,
            icons: nwg::MessageIcons::Question,
        };
        if nwg::message(&params) != nwg::MessageChoice::Yes {
            return;
        }
        match uninstall(&paths::exe_dir()) {
            Ok(()) => {
                nwg::simple_message(APP_TITLE, &format!("{APP_TITLE} was removed."));
            }
            Err(e) => {
                nwg::error_message(APP_TITLE, &e);
                std::process::exit(1);
            }
        }
        return;
    }

    let defaults = Options {
        dir: value("--dir").map(PathBuf::from).unwrap_or_else(default_dir),
        start_menu: true,
        desktop: false,
        launch: true,
    };
    if let Err(e) = install_window(defaults) {
        nwg::error_message(APP_TITLE, &format!("Could not show the setup window: {e}"));
        std::process::exit(1);
    }
}
