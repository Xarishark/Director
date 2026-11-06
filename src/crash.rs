//! Crash reports. When Director panics or its window stops with an error, a plain-text report
//! is saved in `Logs\Crashes`. The next start offers to report it on GitHub: that only opens
//! the browser on a new issue with the error filled in, and nothing is sent unless the user
//! submits it there. Reports never include pairing keys (the logs they quote exclude them).
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const ISSUES_URL: &str = "https://github.com/Xarishark/Director/issues/new";
/// Reports kept on disk; older ones are removed.
const KEEP: usize = 10;
/// Log lines quoted at the end of a report.
const LOG_LINES: usize = 40;
/// GitHub and browsers handle URLs up to about 8 KB; the issue body stays well under that.
const BODY_LIMIT: usize = 4000;

/// A saved report that hasn't been offered yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub path: PathBuf,
    /// What happened, in one line, for the dialog and the issue title.
    pub summary: String,
    pub text: String,
}

pub fn folder() -> Result<PathBuf, String> {
    crate::diagnostics::folder().map(|f| f.join("Crashes"))
}

/// Record panics. Call once at startup, after `diagnostics::init`.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        let place = info
            .location()
            .map_or(String::new(), |l| format!(" at {}:{}", l.file(), l.line()));
        let thread = std::thread::current();
        let message = format!(
            "{payload}{place} (thread {})",
            thread.name().unwrap_or("unnamed")
        );
        crate::diagnostics::event("app.panic", "error", &message);
        crate::diagnostics::flush();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let _ = record("Panic", &message, Some(&backtrace));
        previous(info);
    }));
}

/// Save a report and return its path. Errors are ignored by callers: a failing crash report
/// must not hide the original problem.
pub fn record(kind: &str, message: &str, backtrace: Option<&str>) -> Result<PathBuf, String> {
    let dir = folder()?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let log = crate::diagnostics::recent_text().unwrap_or_default();
    let text = format_report(&Facts {
        kind,
        message,
        backtrace,
        version: env!("CARGO_PKG_VERSION"),
        windows: &windows_version(),
        time_unix: now.as_secs(),
        log: &log,
    });
    let path = dir.join(format!("crash-{}.txt", now.as_millis()));
    fs::write(&path, text).map_err(|e| e.to_string())?;
    prune(&dir);
    Ok(path)
}

/// The newest report not offered yet, if any.
pub fn pending() -> Option<Report> {
    pending_in(&folder().ok()?)
}

/// Mark a report as offered, so it isn't shown again. The file stays for "Open report".
pub fn dismiss(report: &Report) {
    let _ = fs::rename(&report.path, report.path.with_extension("seen.txt"));
}

/// A new-issue link with the report's key facts. The log lines stay in the file; the user
/// can attach it after reading it.
pub fn issue_url(report: Option<&Report>) -> String {
    let (title, body) = match report {
        Some(r) => (
            format!("Crash: {}", truncate(&r.summary, 80)),
            issue_body(&r.text, &r.path),
        ),
        None => (
            String::new(),
            format!(
                "**What happened?**\n\n\n**What did you expect?**\n\n\n---\nDirector {} · {}\n",
                env!("CARGO_PKG_VERSION"),
                windows_version()
            ),
        ),
    };
    format!(
        "{ISSUES_URL}?title={}&body={}",
        encode(&title),
        encode(&body)
    )
}

/// Open a link or a file with its default app.
#[cfg(windows)]
pub fn open(target: &str) -> Result<(), String> {
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::PCWSTR,
    };
    let wide = target.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("open"),
            PCWSTR(wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err(format!("Windows couldn’t open it ({}).", result.0 as isize))
    } else {
        Ok(())
    }
}

struct Facts<'a> {
    kind: &'a str,
    message: &'a str,
    backtrace: Option<&'a str>,
    version: &'a str,
    windows: &'a str,
    time_unix: u64,
    log: &'a str,
}

const SUMMARY: &str = "Summary: ";
const DETAILS_END: &str = "---- Recent log";

fn format_report(f: &Facts) -> String {
    let mut text = format!(
        "Director crash report\n\n{SUMMARY}{}: {}\nDirector {} · {}\nTime (Unix): {}\n",
        f.kind,
        f.message.lines().next().unwrap_or_default(),
        f.version,
        f.windows,
        f.time_unix
    );
    text += &format!("\n{}\n", f.message);
    if let Some(bt) = f.backtrace {
        text += &format!("\nBacktrace:\n{bt}\n");
    }
    // recent_text is newest first; keep the newest lines.
    let log = f
        .log
        .split("\n\n")
        .take(LOG_LINES)
        .collect::<Vec<_>>()
        .join("\n\n");
    text += &format!("\n{DETAILS_END} (newest first; pairing keys are never logged)\n{log}\n");
    text
}

fn issue_body(text: &str, path: &Path) -> String {
    let details = text.split(DETAILS_END).next().unwrap_or(text).trim();
    let details = truncate(details, BODY_LIMIT);
    format!(
        "**What were you doing when it happened?**\n\n\n**Crash report**\n```\n{details}\n```\nThe full report, with recent log lines, is saved at `{}`. Attach it if you’re happy to share it.\n",
        path.display()
    )
}

fn pending_in(dir: &Path) -> Option<Report> {
    let mut files = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name.starts_with("crash-") && name.ends_with(".txt") && !name.ends_with(".seen.txt")
        })
        .collect::<Vec<_>>();
    files.sort();
    let path = files.pop()?;
    let text = fs::read_to_string(&path).ok()?;
    let summary = text
        .lines()
        .find_map(|l| l.strip_prefix(SUMMARY))
        .unwrap_or("Director stopped unexpectedly")
        .to_string();
    // Older unoffered reports are about the same session at best; don't queue them.
    for older in files {
        let _ = fs::rename(&older, older.with_extension("seen.txt"));
    }
    Some(Report {
        path,
        summary,
        text,
    })
}

fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-"))
        })
        .collect::<Vec<_>>();
    // Names start with the time in milliseconds, so sorting puts the oldest first.
    files.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.trim_start_matches("crash-").split('.').next())
            .and_then(|n| n.parse::<u128>().ok())
            .unwrap_or(0)
    });
    let extra = files.len().saturating_sub(KEEP);
    for old in &files[..extra] {
        let _ = fs::remove_file(old);
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

/// Percent-encode for a URL query value.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out += &format!("%{b:02X}");
        }
    }
    out
}

/// "Windows 25H2 (build 26300.1234)". Read from the registry, which also gives the feature
/// update name and revision that the version APIs don't.
fn windows_version() -> String {
    #[cfg(windows)]
    {
        let display = registry_value("DisplayVersion");
        let build = registry_value("CurrentBuild");
        let ubr = registry_dword("UBR");
        if let Some(build) = build {
            let build = match ubr {
                Some(u) => format!("{build}.{u}"),
                None => build,
            };
            return match display {
                Some(d) => format!("Windows {d} (build {build})"),
                None => format!("Windows build {build}"),
            };
        }
    }
    "Windows".into()
}

#[cfg(windows)]
const VERSION_KEY: windows::core::PCWSTR =
    windows::core::w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");

#[cfg(windows)]
fn registry_value(name: &str) -> Option<String> {
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut buffer = [0u16; 128];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            VERSION_KEY,
            windows::core::PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len])).filter(|s| !s.is_empty())
}

#[cfg(windows)]
fn registry_dword(name: &str) -> Option<u32> {
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW};
    let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            VERSION_KEY,
            windows::core::PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> String {
        format_report(&Facts {
            kind: "Window error",
            message: "StructureUnsupported (0x80004005)",
            backtrace: None,
            version: "0.10.0",
            windows: "Windows 10.0.26300",
            time_unix: 1,
            log: "newest\n\nolder",
        })
    }

    #[test]
    fn report_has_a_summary_and_the_issue_link_leaves_the_log_out() {
        let text = sample();
        assert!(text.contains("Summary: Window error: StructureUnsupported"));
        assert!(text.contains("newest"));
        let report = Report {
            path: PathBuf::from(r"C:\Logs\Crashes\crash-1.txt"),
            summary: "Window error: StructureUnsupported (0x80004005)".into(),
            text,
        };
        let url = issue_url(Some(&report));
        assert!(url.starts_with(
            "https://github.com/Xarishark/Director/issues/new?title=Crash%3A%20Window%20error"
        ));
        assert!(url.contains("StructureUnsupported"));
        assert!(!url.contains("newest"), "log lines stay in the file");
        assert!(!url.contains(' ') && !url.contains('\n'));
    }

    #[test]
    fn the_newest_report_is_offered_once_and_old_ones_are_pruned() {
        let dir = std::env::temp_dir().join(format!("director-crash-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(pending_in(&dir).is_none());
        for i in 0..12u32 {
            fs::write(dir.join(format!("crash-{}.txt", 100 + i)), sample()).unwrap();
        }
        prune(&dir);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), KEEP);
        let report = pending_in(&dir).unwrap();
        assert!(report.path.ends_with("crash-111.txt"));
        assert_eq!(
            report.summary,
            "Window error: StructureUnsupported (0x80004005)"
        );
        dismiss(&report);
        // Older ones were marked when the newest was offered; nothing is left to offer.
        assert!(pending_in(&dir).is_none());
        assert!(dir.join("crash-111.seen.txt").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_version_is_the_real_build_not_the_compatibility_one() {
        let v = windows_version();
        assert!(v.contains("build "), "{v}");
        assert!(!v.contains("9200"), "{v}");
    }

    #[test]
    fn encoding_keeps_unreserved_characters() {
        assert_eq!(encode("a b/ü"), "a%20b%2F%C3%BC");
    }
}
