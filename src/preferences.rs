//! Small per-user preferences, separate from device presets.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    #[serde(default)]
    pub hide_tv_setup_tip: bool,
    #[serde(default)]
    pub notify_scene_failures: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    preferences: Preferences,
}
pub fn path() -> Result<PathBuf, String> {
    crate::presets::settings_path("preferences.json")
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
    crate::presets::write_document(
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
        let hidden = Preferences {
            hide_tv_setup_tip: true,
            notify_scene_failures: true,
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
