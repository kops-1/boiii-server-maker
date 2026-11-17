//! Small process helpers on top of the Win32 API.

use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

const SYNCHRONIZE: u32 = 0x0010_0000;
const STILL_ACTIVE: u32 = 259;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    /// Full path of the program file.
    pub exe: PathBuf,
    /// Creation time (Windows FILETIME). Together with the pid this identifies
    /// one process even after the pid number is reused.
    pub created: u64,
}

/// Details of a running process, or None if it does not exist, has exited or
/// may not be inspected.
pub fn info(pid: u32) -> Option<ProcInfo> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut result = None;
        let mut code = 0u32;
        if GetExitCodeProcess(h, &mut code) != 0 && code == STILL_ACTIVE {
            let mut buf = vec![0u16; 32768];
            let mut len = buf.len() as u32;
            let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let (mut c, mut e, mut k, mut u) = (zero, zero, zero, zero);
            if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0
                && GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0
            {
                result = Some(ProcInfo {
                    pid,
                    exe: PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])),
                    created: ((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64,
                });
            }
        }
        CloseHandle(h);
        result
    }
}

/// Pids of all processes whose file name matches (case-insensitive).
pub fn pids_by_name(name: &str) -> Vec<u32> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if exe.eq_ignore_ascii_case(name) {
                    out.push(entry.th32ProcessID);
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    out
}

/// True if both paths name the same file. Windows can report a different
/// spelling for a running program than the one it was started with (a folder
/// reached through a link, or app-data redirection for packaged apps), so the
/// fully resolved paths are compared as well as the plain text.
pub fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    use crate::paths::norm;
    if norm(a) == norm(b) {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => norm(&x) == norm(&y),
        _ => false,
    }
}

/// Running processes started from exactly this program file.
pub fn running_from(exe: &std::path::Path) -> Vec<ProcInfo> {
    let Some(name) = exe.file_name().map(|n| n.to_string_lossy().to_string()) else {
        return Vec::new();
    };
    pids_by_name(&name).into_iter().filter_map(info).filter(|p| same_file(&p.exe, exe)).collect()
}

/// Stops child processes started afterwards from inheriting this program's
/// own input/output handles. Without this a long-running child (the server)
/// keeps the caller's output pipe open, and a script reading this program's
/// output waits until the server exits.
pub fn keep_std_handles_private() {
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    unsafe {
        for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let h = GetStdHandle(id);
            if !h.is_null() && h != INVALID_HANDLE_VALUE {
                SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

/// Ends a process and waits up to five seconds for it to be gone.
pub fn terminate(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = TerminateProcess(h, 1) != 0;
        if ok {
            WaitForSingleObject(h, 5000);
        }
        CloseHandle(h);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_file_sees_through_links() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("a.exe"), b"x").unwrap();
        std::fs::write(real.join("b.exe"), b"x").unwrap();
        let link = tmp.path().join("link");
        crate::fsutil::make_junction(&real, &link).unwrap();
        assert!(same_file(&link.join("a.exe"), &real.join("a.exe")));
        assert!(same_file(&real.join("A.EXE"), &real.join("a.exe")));
        assert!(!same_file(&link.join("a.exe"), &real.join("b.exe")));
        assert!(!same_file(&real.join("missing.exe"), &real.join("a.exe")));
    }

    #[test]
    fn sees_itself_and_can_end_a_child() {
        let me = info(std::process::id()).expect("own process");
        assert!(same_file(&me.exe, &std::env::current_exe().unwrap()));
        assert!(running_from(&me.exe).iter().any(|p| p.pid == me.pid));

        let sys = crate::paths::system32().unwrap();
        let mut child = std::process::Command::new(sys.join("ping.exe"))
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let ci = info(child.id()).expect("child visible");
        assert!(ci.created >= me.created);
        assert!(terminate(child.id()));
        let _ = child.wait();
        assert!(info(child.id()).is_none());
        assert!(info(0xFFFF_FFF0).is_none());
    }
}
