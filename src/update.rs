//! Updates from GitHub Releases: find the latest version, download its setup, check the
//! file against the SHA-256 digest GitHub reports, and start it. Nothing runs unless the
//! window checks at startup (Options can turn that off) or the user asks.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const LATEST: &str = "https://api.github.com/repos/Xarishark/Director/releases/latest";
/// Setup's fixed release asset name (see the release job in the Windows build workflow).
const ASSET: &str = "Director-x64-setup.exe";
/// Downloads must come from this repository's releases; GitHub then redirects to its CDN.
const DOWNLOADS: &str = "https://github.com/Xarishark/Director/releases/download/";
const PAGES: &str = "https://github.com/Xarishark/Director/releases/";
/// Setup is a few MB; refuse anything far larger.
const MAX_SETUP: u64 = 128 * 1024 * 1024;
const MAX_JSON: u64 = 4 * 1024 * 1024;

/// A three-part version, as Cargo.toml and the release tags use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// `0.13.0` or `v0.13.0`; anything else (pre-release suffixes included) is refused.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let mut parts = text.split('.').map(|p| {
            if p.is_empty() || p.len() > 9 || !p.bytes().all(|b| b.is_ascii_digit()) {
                None
            } else {
                p.parse().ok()
            }
        });
        let version = Self(parts.next()??, parts.next()??, parts.next()??);
        parts.next().is_none().then_some(version)
    }
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("Cargo.toml has a three-part version")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The newest published release and its setup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The release page with its notes.
    pub page: String,
    pub download: String,
    pub size: u64,
    pub sha256: [u8; 32],
}

impl Release {
    pub fn is_newer(&self) -> bool {
        self.version > Version::current()
    }
}

#[derive(serde::Deserialize)]
struct ReleaseJson {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(serde::Deserialize)]
struct AssetJson {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// Read GitHub's "latest release" answer.
pub fn parse_release(json: &[u8]) -> Result<Release, String> {
    let release: ReleaseJson = serde_json::from_slice(json)
        .map_err(|_| "GitHub's release information could not be read.".to_string())?;
    if release.draft || release.prerelease {
        return Err("GitHub's latest release isn't a final release.".into());
    }
    let version = Version::parse(&release.tag_name).ok_or_else(|| {
        format!(
            "The latest release has an unexpected tag ({}).",
            release.tag_name
        )
    })?;
    if !release.html_url.starts_with(PAGES) {
        return Err("The latest release points to an unexpected page.".into());
    }
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == ASSET)
        .ok_or_else(|| {
            format!("Director {version} has no {ASSET} yet. Try again in a few minutes.")
        })?;
    if !asset.browser_download_url.starts_with(DOWNLOADS) {
        return Err("The setup download points to an unexpected address.".into());
    }
    if asset.size == 0 || asset.size > MAX_SETUP {
        return Err("The setup download has an unexpected size.".into());
    }
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .and_then(hex32)
        .ok_or("GitHub didn't report a checksum for setup, so Director can't verify it. Download it from the release page instead.")?;
    Ok(Release {
        version,
        page: release.html_url,
        download: asset.browser_download_url.clone(),
        size: asset.size,
        sha256,
    })
}

fn hex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// The latest release, whether or not it's newer than this version.
#[cfg(windows)]
pub fn latest() -> Result<Release, String> {
    let _operation = crate::diagnostics::Operation::start("update.check", None);
    let body = http::get(LATEST, "application/vnd.github+json", MAX_JSON)?;
    let release = parse_release(&body);
    crate::diagnostics::event(
        "update.check",
        if release.is_ok() { "ok" } else { "error" },
        &match &release {
            Ok(r) => r.version.to_string(),
            Err(e) => e.clone(),
        },
    );
    release
}

/// A newer release, if there is one.
#[cfg(windows)]
pub fn check() -> Result<Option<Release>, String> {
    latest().map(|r| r.is_newer().then_some(r))
}

/// Where downloaded setups go. Older downloads are removed before the next one.
fn download_folder() -> PathBuf {
    std::env::temp_dir().join("Director-update")
}

/// Download the release's setup and check its size and SHA-256 digest.
#[cfg(windows)]
pub fn download(release: &Release) -> Result<PathBuf, String> {
    let _operation = crate::diagnostics::Operation::start("update.download", None);
    let folder = download_folder();
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("Could not create a folder for the download: {e}"))?;
    let body = http::get(&release.download, "application/octet-stream", MAX_SETUP)?;
    verify(release, &body)?;
    let path = folder.join(format!("Director-{}-x64-setup.exe", release.version));
    std::fs::write(&path, &body).map_err(|e| format!("Could not save the download: {e}"))?;
    crate::diagnostics::event("update.download", "ok", &release.version.to_string());
    Ok(path)
}

fn verify(release: &Release, body: &[u8]) -> Result<(), String> {
    if body.len() as u64 != release.size {
        return Err("The download was incomplete. Try again.".into());
    }
    if Sha256::digest(body).as_slice() != release.sha256 {
        return Err("The download doesn't match GitHub's checksum, so Director didn't run it. Try again, or download setup from the release page.".into());
    }
    Ok(())
}

/// Whether this copy runs from the installed location, so setup replaces it.
#[cfg(windows)]
pub fn installed() -> bool {
    static INSTALLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *INSTALLED.get_or_init(runs_from_program_files)
}

#[cfg(windows)]
fn runs_from_program_files() -> bool {
    let Some(programs) =
        std::env::var_os("ProgramW6432").or_else(|| std::env::var_os("ProgramFiles"))
    else {
        return false;
    };
    let expected = Path::new(&programs).join("Director").join("director.exe");
    std::env::current_exe().is_ok_and(|exe| {
        exe.to_string_lossy()
            .eq_ignore_ascii_case(&expected.to_string_lossy())
    })
}

/// Start the downloaded setup. It keeps the desktop shortcut as it is now, asks Windows for
/// permission, replaces the installed version and offers to open Director when it's done.
#[cfg(windows)]
pub fn launch(setup: &Path) -> Result<(), String> {
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{PCWSTR, w},
    };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let file = wide(&setup.to_string_lossy());
    let arguments = wide(if desktop_shortcut() {
        "DesktopShortcut=1"
    } else {
        "DesktopShortcut=0"
    });
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(file.as_ptr()),
            PCWSTR(arguments.as_ptr()),
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err(format!(
            "Windows couldn't start setup ({}). It's saved at {}.",
            result.0 as isize,
            setup.display()
        ));
    }
    crate::diagnostics::event("update.setup", "started", &setup.to_string_lossy());
    Ok(())
}

/// The installer records the desktop shortcut choice in HKLM\Software\Director\Installer.
#[cfg(windows)]
fn desktop_shortcut() -> bool {
    use windows::{
        Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW},
        core::w,
    };
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("Software\\Director\\Installer"),
            w!("DesktopShortcut"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    result.is_ok() && value == 1
}

#[cfg(windows)]
mod http {
    use std::{ffi::c_void, ptr};
    use windows::{
        Win32::Networking::WinHttp::*,
        core::{PCWSTR, w},
    };

    struct Handle(*mut c_void);
    impl Handle {
        fn new(raw: *mut c_void) -> Result<Self, String> {
            if raw.is_null() {
                Err(offline(windows::core::Error::from_thread()))
            } else {
                Ok(Self(raw))
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            let _ = unsafe { WinHttpCloseHandle(self.0) };
        }
    }
    fn offline(error: windows::core::Error) -> String {
        format!(
            "Couldn't reach GitHub. Check your internet connection and try again. ({})",
            error.code()
        )
    }

    /// An HTTPS GET that follows redirects (never to plain HTTP) and reads at most `limit`
    /// bytes. Uses the system's proxy settings.
    pub(super) fn get(url: &str, accept: &str, limit: u64) -> Result<Vec<u8>, String> {
        let rest = url
            .strip_prefix("https://")
            .ok_or("Updates only use HTTPS.")?;
        let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let path = if path.is_empty() { "/" } else { path };
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let agent = wide(concat!("Director/", env!("CARGO_PKG_VERSION")));
        let session = Handle::new(unsafe {
            WinHttpOpen(
                PCWSTR(agent.as_ptr()),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        })?;
        unsafe { WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, 30_000) }
            .map_err(offline)?;
        let host = wide(host);
        let connection = Handle::new(unsafe {
            WinHttpConnect(
                session.0,
                PCWSTR(host.as_ptr()),
                INTERNET_DEFAULT_HTTPS_PORT,
                0,
            )
        })?;
        let path = wide(path);
        let request = Handle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                w!("GET"),
                PCWSTR(path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        })?;
        let headers = wide(&format!(
            "Accept: {accept}\r\nX-GitHub-Api-Version: 2022-11-28\r\n"
        ));
        unsafe {
            WinHttpSendRequest(
                request.0,
                Some(&headers[..headers.len() - 1]),
                None,
                0,
                0,
                0,
            )
        }
        .map_err(offline)?;
        unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) }.map_err(offline)?;
        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        unsafe {
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some((&mut status as *mut u32).cast()),
                &mut size,
                ptr::null_mut(),
            )
        }
        .map_err(offline)?;
        match status {
            200 => {}
            403 | 429 => {
                return Err(
                    "GitHub is limiting update checks from this network. Try again later.".into(),
                );
            }
            404 => return Err("No Director release was found on GitHub.".into()),
            _ => {
                return Err(format!(
                    "GitHub answered with an error ({status}). Try again later."
                ));
            }
        }
        let mut body = Vec::new();
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            unsafe {
                WinHttpReadData(
                    request.0,
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as u32,
                    &mut read,
                )
            }
            .map_err(offline)?;
            if read == 0 {
                return Ok(body);
            }
            body.extend_from_slice(&chunk[..read as usize]);
            if body.len() as u64 > limit {
                return Err("GitHub sent more data than expected.".into());
            }
        }
    }
}

/// `director update [--install]`.
#[cfg(windows)]
pub fn cli(args: &[String]) -> Result<i32, String> {
    let install = match args {
        [] => false,
        [flag] if flag == "--install" => true,
        _ => return Err("Use director update or director update --install.".into()),
    };
    let current = Version::current();
    let Some(release) = check()? else {
        println!("Director {current} is up to date.");
        return Ok(0);
    };
    println!(
        "Director {} is available (you have {current}).",
        release.version
    );
    println!("Release notes: {}", release.page);
    if !install {
        println!("Run director update --install to download and start setup.");
        return Ok(0);
    }
    println!("Downloading setup…");
    let setup = download(&release)?;
    println!("Checksum verified. Starting setup; confirm the Windows prompt to install.");
    launch(&setup)?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(tag: &str, digest: Option<&str>) -> Vec<u8> {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/Xarishark/Director/releases/tag/{tag}"),
            "draft": false,
            "prerelease": false,
            "assets": [
                {"name": "notes.txt", "browser_download_url": "https://example.com/x", "size": 3},
                {
                    "name": ASSET,
                    "browser_download_url": format!("https://github.com/Xarishark/Director/releases/download/{tag}/{ASSET}"),
                    "size": 3,
                    "digest": digest,
                }
            ]
        })
        .to_string()
        .into_bytes()
    }
    // SHA-256 of "abc".
    const ABC: &str = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    /// Contacts GitHub and downloads the latest setup; never starts it.
    #[test]
    #[ignore = "network"]
    #[cfg(windows)]
    fn downloads_and_verifies_the_latest_setup() {
        let release = latest().unwrap();
        let path = download(&release).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), release.size);
        std::fs::remove_dir_all(download_folder()).unwrap();
    }

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(Version::parse("v0.13.0"), Some(Version(0, 13, 0)));
        assert_eq!(Version::parse("1.2.10"), Some(Version(1, 2, 10)));
        for bad in [
            "",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "v1.2.3-beta",
            "1..2",
            "+1.2.3",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert!(Version(0, 13, 1) > Version(0, 13, 0));
        assert!(Version(0, 14, 0) > Version(0, 13, 9));
        assert!(Version(1, 0, 0) > Version(0, 99, 99));
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn reads_the_setup_asset_and_checks_the_download() {
        let release = parse_release(&json("v99.0.0", Some(ABC))).unwrap();
        assert_eq!(release.version, Version(99, 0, 0));
        assert!(release.is_newer());
        assert!(
            release
                .download
                .ends_with("/v99.0.0/Director-x64-setup.exe")
        );
        assert!(verify(&release, b"abc").is_ok());
        assert!(verify(&release, b"abd").is_err());
        assert!(verify(&release, b"abcd").is_err());
        let old = parse_release(&json("v0.0.1", Some(ABC))).unwrap();
        assert!(!old.is_newer());
    }

    #[test]
    fn refuses_releases_it_cannot_verify() {
        assert!(parse_release(&json("v99.0.0", None)).is_err());
        assert!(parse_release(&json("v99.0.0", Some("sha1:abc"))).is_err());
        assert!(parse_release(&json("nightly", Some(ABC))).is_err());
        assert!(parse_release(b"not json").is_err());
        let moved = String::from_utf8(json("v99.0.0", Some(ABC)))
            .unwrap()
            .replace(
                "https://github.com/Xarishark/Director/releases/download",
                "https://example.com",
            );
        assert!(parse_release(moved.as_bytes()).is_err());
        let pre = String::from_utf8(json("v99.0.0", Some(ABC)))
            .unwrap()
            .replace("\"prerelease\":false", "\"prerelease\":true");
        assert!(parse_release(pre.as_bytes()).is_err());
    }
}
