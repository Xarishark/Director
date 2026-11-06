//! Settings files under %LOCALAPPDATA%\Director: locating them and writing them atomically.
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A file in Director's settings folder. Never falls back to the working directory.
pub(crate) fn settings_path(filename: &str) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        use windows::Win32::{
            System::Com::CoTaskMemFree,
            UI::Shell::{FOLDERID_LocalAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
        };
        // Ask Windows for the actual folder, including configured redirection.
        // Never fall back to the working directory or a home-relative path.
        let allocated =
            unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None) }
                .map_err(|e| format!("Windows Local AppData folder is unavailable: {e}"))?;
        let directory = PathBuf::from(OsString::from_wide(unsafe { allocated.as_wide() }));
        unsafe { CoTaskMemFree(Some(allocated.0.cast())) };
        Ok(directory.join("Director").join(filename))
    }
    #[cfg(not(windows))]
    Err("Director settings require Windows.".into())
}
pub(crate) fn write_document<T: Serialize>(path: &Path, document: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid settings file location.")?;
    fs::create_dir_all(parent).map_err(|e| format!("Could not create the settings folder: {e}"))?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| {
        use std::io::Write;
        let bytes = serde_json::to_vec_pretty(document).map_err(|e| e.to_string())?;
        let mut file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e: String| format!("Could not save settings: {e}"))
}
#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };
    let from: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| e.to_string())
}
#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| e.to_string())
}
