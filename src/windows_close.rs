//! Close a running program by executable name or path, for scene "Close app" actions.
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM, WPARAM},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
            QueryFullProcessImageNameW, TerminateProcess,
        },
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
    },
};
use windows::core::{BOOL, PWSTR};

/// How long a program gets to close after being asked.
const GRACE: Duration = Duration::from_secs(10);

/// Whether a running program matches the target: a bare name (`steam.exe`) matches by file
/// name, anything with a folder must match the full path. Both ignore case.
fn matches(target: &str, exe_name: &str, full_path: Option<&str>) -> bool {
    if target.contains(['\\', '/']) {
        full_path.is_some_and(|p| {
            p.replace('/', "\\")
                .eq_ignore_ascii_case(&target.replace('/', "\\"))
        })
    } else {
        exe_name.eq_ignore_ascii_case(target)
    }
}

fn full_path(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result
        .ok()
        .map(|()| String::from_utf16_lossy(&buffer[..size as usize]))
}

/// Process IDs of running programs that match the target.
fn running(target: &str) -> Result<Vec<u32>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|e| format!("Could not list running programs: {e}"))?;
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut pids = vec![];
    let mut next = unsafe { Process32FirstW(snapshot, &mut entry) };
    while next.is_ok() {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        let pid = entry.th32ProcessID;
        let path = target
            .contains(['\\', '/'])
            .then(|| full_path(pid))
            .flatten();
        if pid != std::process::id() && matches(target, &name, path.as_deref()) {
            pids.push(pid);
        }
        next = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    let _ = unsafe { CloseHandle(snapshot) };
    Ok(pids)
}

unsafe extern "system" fn ask_to_close(window: HWND, pids: LPARAM) -> BOOL {
    let pids = unsafe { &*(pids.0 as *const Vec<u32>) };
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    if pids.contains(&pid) && unsafe { IsWindowVisible(window) }.as_bool() {
        let _ = unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    }
    BOOL(1)
}

/// Ask every matching program to close, wait for it, and end it only when `force` is set.
/// A program that isn't running counts as closed.
pub fn close(target: &str, force: bool) -> Result<(), String> {
    let pids = running(target)?;
    if pids.is_empty() {
        return Ok(());
    }
    let _ = unsafe {
        EnumWindows(
            Some(ask_to_close),
            LPARAM(&pids as *const Vec<u32> as isize),
        )
    };
    let deadline = Instant::now() + GRACE;
    while Instant::now() < deadline {
        if running(target)?.is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let left = running(target)?;
    if left.is_empty() {
        return Ok(());
    }
    if !force {
        return Err(format!(
            "{target} is still running after being asked to close. Turn on “End it if it doesn’t close” to force it."
        ));
    }
    for pid in left {
        let process = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }
            .map_err(|e| format!("Could not end {target}: {e}"))?;
        let result = unsafe { TerminateProcess(process, 1) };
        let _ = unsafe { CloseHandle(process) };
        result.map_err(|e| format!("Could not end {target}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_match_by_file_name_and_paths_match_exactly() {
        assert!(matches("steam.exe", "Steam.exe", None));
        assert!(!matches("steam.exe", "steamwebhelper.exe", None));
        assert!(matches(
            "C:/Program Files (x86)/Steam/steam.exe",
            "steam.exe",
            Some(r"C:\Program Files (x86)\Steam\steam.exe")
        ));
        assert!(!matches(
            r"C:\Games\steam.exe",
            "steam.exe",
            Some(r"C:\Program Files (x86)\Steam\steam.exe")
        ));
    }
}
