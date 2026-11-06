pub use crate::audio::DeviceSelection;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub output_id: String,
    pub output_name: String,
    pub volume_percent: Option<u8>,
    pub keep_changes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communications_output: Option<DeviceSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSelection {
    pub id: String,
    pub name: String,
    pub volume_percent: Option<u8>,
}
impl Preset {
    pub fn request(&self) -> crate::audio::Request {
        crate::audio::Request {
            output_id: Some(self.output_id.clone()),
            volume_percent: self.volume_percent,
            mute: None,
            communications_output_id: self.communications_output.as_ref().map(|d| d.id.clone()),
            input: self.input.as_ref().map(|d| crate::audio::InputRequest {
                device_id: d.id.clone(),
                volume_percent: d.volume_percent,
                mute: None,
            }),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    presets: Vec<Preset>,
}

pub fn path() -> Result<PathBuf, String> {
    crate::storage::settings_path("audio-presets.json")
}
pub fn load(path: &Path) -> Result<Vec<Preset>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Could not read saved presets: {e}")),
    };
    let doc: Document = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Could not read saved presets; the file was left unchanged: {e}"))?;
    if doc.version != 1 {
        return Err("Unsupported preset file version; the file was left unchanged.".into());
    }
    validate(&doc.presets)?;
    Ok(doc.presets)
}
pub(crate) fn validate(presets: &[Preset]) -> Result<(), String> {
    for (i, p) in presets.iter().enumerate() {
        if p.id.is_empty()
            || p.name.trim().is_empty()
            || p.name.len() > 160
            || p.output_id.is_empty()
            || p.volume_percent.is_some_and(|v| v > 100)
            || p.communications_output
                .as_ref()
                .is_some_and(|d| d.id.is_empty() || d.name.trim().is_empty())
            || p.input.as_ref().is_some_and(|d| {
                d.id.is_empty()
                    || d.name.trim().is_empty()
                    || d.volume_percent.is_some_and(|v| v > 100)
            })
        {
            return Err(
                "Presets need a name (up to 160 bytes), an output, and a volume from 0 to 100."
                    .into(),
            );
        }
        if presets[..i]
            .iter()
            .any(|other| other.id == p.id || other.name.to_lowercase() == p.name.to_lowercase())
        {
            return Err("Each audio preset needs a unique name.".into());
        }
    }
    Ok(())
}
/// Caller holds the cross-process operation lock. Refuse stale-window overwrites.
pub fn save(path: &Path, expected: &[Preset], next: &[Preset]) -> Result<(), String> {
    validate(next)?;
    crate::cli_name::check_new_names(expected, next, |p| (&p.id, &p.name), "audio preset")?;
    if load(path)? != expected {
        return Err("Presets changed in another window. Refresh before saving again.".into());
    }
    crate::storage::write_document(
        path,
        &Document {
            version: 1,
            presets: next.to_vec(),
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn preset() -> Preset {
        Preset {
            communications_output: None,
            input: None,
            id: "one".into(),
            name: "Desk".into(),
            output_id: "endpoint".into(),
            output_name: "Speakers".into(),
            volume_percent: None,
            keep_changes: false,
        }
    }
    struct Folder(PathBuf);
    impl Folder {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "director-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn file(&self) -> PathBuf {
            self.0.join("presets.json")
        }
    }
    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn round_trip_edit_and_delete() {
        let dir = Folder::new();
        let path = dir.file();
        let first = vec![preset()];
        assert!(load(&path).unwrap().is_empty());
        save(&path, &[], &first).unwrap();
        assert_eq!(load(&path).unwrap(), first);
        let mut edited = first.clone();
        edited[0].name = "TV".into();
        edited[0].volume_percent = Some(0);
        save(&path, &first, &edited).unwrap();
        assert_eq!(load(&path).unwrap(), edited);
        save(&path, &edited, &[]).unwrap();
        assert!(load(&path).unwrap().is_empty());
    }
    #[test]
    fn stale_writer_cannot_overwrite() {
        let dir = Folder::new();
        let path = dir.file();
        let first = vec![preset()];
        save(&path, &[], &first).unwrap();
        assert!(save(&path, &[], &[]).is_err());
        assert_eq!(load(&path).unwrap(), first);
    }
    #[test]
    fn corrupt_or_future_file_is_preserved() {
        let dir = Folder::new();
        let path = dir.file();
        for data in ["broken", r#"{"version":2,"presets":[]}"#] {
            fs::write(&path, data).unwrap();
            assert!(save(&path, &[], &[preset()]).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), data);
        }
    }
    #[test]
    fn rejects_duplicate_names_and_invalid_values() {
        let first = preset();
        let mut second = first.clone();
        second.id = "two".into();
        second.name = "desk".into();
        assert!(validate(&[first, second]).is_err());
        let mut p = preset();
        p.volume_percent = Some(101);
        assert!(validate(&[p]).is_err());
    }
}

#[cfg(test)]
#[test]
fn optional_audio_routing_round_trips_and_legacy_presets_follow_the_main_output() {
    let json = r#"{"id":"legacy","name":"Desk","output_id":"speaker","output_name":"Speakers","volume_percent":null,"keep_changes":false}"#;
    let mut p: Preset = serde_json::from_str(json).unwrap();
    assert!(p.request().communications_output_id.is_none());
    assert!(p.request().input.is_none());
    p.communications_output = Some(DeviceSelection {
        id: "headset".into(),
        name: "Calls headset".into(),
    });
    p.input = Some(InputSelection {
        id: "mic".into(),
        name: "Desk microphone".into(),
        volume_percent: Some(35),
    });
    validate(&[p.clone()]).unwrap();
    let saved = serde_json::to_string(&p).unwrap();
    let read: Preset = serde_json::from_str(&saved).unwrap();
    assert_eq!(read, p);
    assert_eq!(
        read.request().communications_output_id.as_deref(),
        Some("headset")
    );
    assert_eq!(read.request().input.unwrap().volume_percent, Some(35));
}
