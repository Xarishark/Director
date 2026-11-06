//! Small per-user preferences, separate from device presets.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    #[serde(default)]
    pub hide_tv_setup_tip: bool,
    #[serde(default)]
    pub notify_scene_failures: bool,
    /// Keep steps that finished when a later scene or audio preset step fails.
    #[serde(default)]
    pub keep_completed_changes: bool,
    /// How long scenes wait for a TV or audio device, and for waited apps (1–120 s).
    #[serde(default = "default_wait")]
    pub device_wait_seconds: u16,
    /// The Scenes page's "make your next scene" tip was dismissed. Written only when set, so
    /// older builds keep reading the file until the tip is dismissed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_next_scene_tip: bool,
}
fn default_wait() -> u16 {
    30
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            hide_tv_setup_tip: false,
            notify_scene_failures: false,
            keep_completed_changes: false,
            device_wait_seconds: default_wait(),
            hide_next_scene_tip: false,
        }
    }
}
impl Preferences {
    pub fn scene_policy(&self) -> crate::scenes::ScenePolicy {
        crate::scenes::ScenePolicy {
            keep_completed: self.keep_completed_changes,
            wait_seconds: self.device_wait_seconds.clamp(1, 120),
        }
    }
    pub fn audio_policy(&self) -> crate::audio::FailurePolicy {
        if self.keep_completed_changes {
            crate::audio::FailurePolicy::KeepChanges
        } else {
            crate::audio::FailurePolicy::Revert
        }
    }
}
/// The saved preferences, or the defaults when they can't be read (a run must not fail
/// because the preferences file is unreadable).
pub fn current() -> Preferences {
    path().and_then(|p| load(&p)).unwrap_or_default()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    preferences: Preferences,
}
pub fn path() -> Result<PathBuf, String> {
    crate::storage::settings_path("preferences.json")
}
pub fn load(path: &Path) -> Result<Preferences, String> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(e) => return Err(format!("Could not read preferences: {e}")),
    };
    let document: Document = serde_json::from_slice(&data)
        .map_err(|_| "Preferences could not be read. The file was left unchanged.")?;
    if document.version != 1 {
        return Err(
            "These preferences require a newer Director version. The file was left unchanged."
                .into(),
        );
    }
    Ok(document.preferences)
}
pub fn save(path: &Path, expected: &Preferences, next: &Preferences) -> Result<(), String> {
    if load(path)? != *expected {
        return Err(
            "Preferences changed in another window. Reopen Director before changing them.".into(),
        );
    }
    crate::storage::write_document(
        path,
        &Document {
            version: 1,
            preferences: next.clone(),
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preference_round_trip_rejects_stale_and_future_writes() {
        let path = std::env::temp_dir().join(format!(
            "director-preferences-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let initial = load(&path).unwrap();
        assert_eq!(initial.device_wait_seconds, 30);
        assert!(!initial.hide_next_scene_tip);
        std::fs::write(
            &path,
            br#"{"version":1,"preferences":{"hide_tv_setup_tip":false,"notify_scene_failures":false,"keep_completed_changes":false,"device_wait_seconds":30}}"#,
        )
        .unwrap();
        assert_eq!(load(&path).unwrap(), initial, "older files still load");
        std::fs::remove_file(&path).unwrap();
        assert_eq!(initial.audio_policy(), crate::audio::FailurePolicy::Revert);
        let hidden = Preferences {
            hide_tv_setup_tip: true,
            notify_scene_failures: true,
            keep_completed_changes: true,
            device_wait_seconds: 45,
            hide_next_scene_tip: true,
        };
        save(&path, &initial, &hidden).unwrap();
        assert_eq!(load(&path).unwrap(), hidden);
        assert!(save(&path, &initial, &initial).is_err());
        let future = br#"{"version":99,"preferences":{"hide_tv_setup_tip":false}}"#;
        std::fs::write(&path, future).unwrap();
        assert!(save(&path, &hidden, &initial).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), future);
        std::fs::write(&path, b"broken").unwrap();
        assert!(save(&path, &initial, &hidden).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken");
        std::fs::remove_file(path).unwrap();
    }
}
