//! The active scene: the last scene applied successfully during this Windows session.
//!
//! It is stored with the time Windows started, so it survives closing Director and is shared
//! by the window and the CLI, but a PC restart forgets it (the setup has reset by then).
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    scene_id: String,
    /// When Windows started, in Unix seconds.
    booted_at: u64,
}

/// Boot times from the same session differ only by clock adjustments and rounding.
const SAME_BOOT_SECONDS: u64 = 120;

pub fn path() -> Result<PathBuf, String> {
    crate::presets::settings_path("active-scene.json")
}

/// When Windows started, in Unix seconds.
fn booted_at() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    #[cfg(windows)]
    let up = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } / 1000;
    #[cfg(not(windows))]
    let up = 0;
    now.saturating_sub(up)
}

fn read(path: &Path, booted_at: u64) -> Option<String> {
    let doc: Document = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (doc.version == 1 && doc.booted_at.abs_diff(booted_at) <= SAME_BOOT_SECONDS)
        .then_some(doc.scene_id)
}

fn write(path: &Path, scene_id: Option<&str>, booted_at: u64) -> Result<(), String> {
    match scene_id {
        Some(id) => crate::presets::write_document(
            path,
            &Document {
                version: 1,
                scene_id: id.into(),
                booted_at,
            },
        ),
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("Could not clear the active scene: {e}"))
            }
            _ => Ok(()),
        },
    }
}

/// The active scene's ID, if one was applied since Windows started.
pub fn active() -> Option<String> {
    read(&path().ok()?, booted_at())
}

/// Record the active scene, or clear it with `None`.
pub fn set(scene_id: Option<&str>) -> Result<(), String> {
    write(&path()?, scene_id, booted_at())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_active_scene_is_forgotten_after_a_restart() {
        let path =
            std::env::temp_dir().join(format!("director-active-scene-{}.json", std::process::id()));
        let boot = 1_000_000;
        write(&path, Some("console"), boot).unwrap();
        assert_eq!(read(&path, boot + 30).as_deref(), Some("console"));
        assert_eq!(read(&path, boot + 3_600), None);
        write(&path, None, boot).unwrap();
        assert_eq!(read(&path, boot), None);
        write(&path, None, boot).unwrap();
    }
}
