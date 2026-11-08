//! Saved desktop layouts. Capture and storage only; no display mutation API.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshRate {
    pub numerator: u32,
    pub denominator: u32,
}
impl RefreshRate {
    pub fn hz(&self) -> f64 {
        if self.denominator == 0 {
            0.0
        } else {
            self.numerator as f64 / self.denominator as f64
        }
    }
    pub fn label(&self) -> String {
        format!("{:.2} Hz", self.numerator as f64 / self.denominator as f64)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopMode {
    pub source_adapter_luid: u64,
    pub source_id: u32,
    pub source_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub refresh_rate: RefreshRate,
    pub rotation_degrees: u16,
}
impl Layout {
    /// The refresh rate of the primary display, if one is on.
    pub fn primary_refresh_hz(&self) -> Option<f64> {
        self.monitors
            .iter()
            .filter(|m| m.enabled)
            .filter_map(|m| m.desktop.as_ref())
            .find(|d| d.primary())
            .map(|d| d.refresh_rate.hz())
            .filter(|hz| *hz > 0.0)
    }
}
impl DesktopMode {
    pub fn primary(&self) -> bool {
        self.x == 0 && self.y == 0
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Monitor {
    // Current adapter/target locators are snapshots, not durable hardware IDs.
    pub adapter_luid: u64,
    pub target_id: u32,
    pub device_path: Option<String>,
    pub name: String,
    pub connected: bool,
    /// None leaves the screen's HDR state as it is when the preset is applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr: Option<bool>,
    pub enabled: bool,
    pub desktop: Option<DesktopMode>,
}
impl Monitor {
    pub fn summary(&self) -> String {
        match &self.desktop {
            Some(d) => format!(
                "{} × {} · {} · Position {}, {} · {}°{}",
                d.width,
                d.height,
                d.refresh_rate.label(),
                d.x,
                d.y,
                d.rotation_degrees,
                if d.primary() { " · Primary" } else { "" }
            ),
            None => if self.connected {
                "Disabled"
            } else {
                "Disconnected"
            }
            .into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub monitors: Vec<Monitor>,
}
impl Layout {
    pub fn summary(&self) -> String {
        let enabled = self.monitors.iter().filter(|m| m.enabled).count();
        format!(
            "{enabled} enabled · {} disabled or disconnected",
            self.monitors.len() - enabled
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.monitors.is_empty()
            || !self
                .monitors
                .iter()
                .any(|m| m.enabled && m.desktop.as_ref().is_some_and(DesktopMode::primary))
        {
            return Err(
                "The captured layout needs an enabled primary display. Refresh and capture again."
                    .into(),
            );
        }
        for (i, m) in self.monitors.iter().enumerate() {
            if m.name.trim().is_empty()
                || m.enabled != m.desktop.is_some()
                || self.monitors[..i]
                    .iter()
                    .any(|p| (p.adapter_luid, p.target_id) == (m.adapter_luid, m.target_id))
            {
                return Err(
                    "The display layout contains missing or duplicate monitor details.".into(),
                );
            }
            if let Some(d) = &m.desktop
                && (d.width == 0
                    || d.height == 0
                    || d.source_width == 0
                    || d.source_height == 0
                    || d.source_name.is_empty()
                    || d.refresh_rate.numerator == 0
                    || d.refresh_rate.denominator == 0
                    || ![0, 90, 180, 270].contains(&d.rotation_degrees))
            {
                return Err(
                    "The display layout contains invalid mode or refresh-rate details.".into(),
                );
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub captured_at_unix_seconds: u64,
    pub confirm_changes: bool,
    pub layout: Layout,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    presets: Vec<Preset>,
}
pub fn path() -> Result<PathBuf, String> {
    crate::storage::settings_path("display-presets.json")
}
pub fn validate(presets: &[Preset]) -> Result<(), String> {
    for (i, p) in presets.iter().enumerate() {
        if p.id.is_empty() || p.name.trim().is_empty() || p.name.len() > 160 {
            return Err("Display presets need a name of up to 160 bytes.".into());
        }
        if presets[..i]
            .iter()
            .any(|other| other.id == p.id || other.name.to_lowercase() == p.name.to_lowercase())
        {
            return Err("Each display preset needs a unique name.".into());
        }
        p.layout.validate()?;
    }
    Ok(())
}
pub fn load(path: &Path) -> Result<Vec<Preset>, String> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("Could not read display presets: {e}")),
    };
    let doc: Document = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Could not read display presets; the file was left unchanged: {e}"))?;
    if doc.version != 1 {
        return Err("Unsupported display-preset version; the file was left unchanged.".into());
    }
    validate(&doc.presets)?;
    Ok(doc.presets)
}
/// Caller holds Director's cross-process operation lock.
pub fn save(path: &Path, expected: &[Preset], next: &[Preset]) -> Result<(), String> {
    validate(next)?;
    crate::cli_name::check_new_names(expected, next, |p| (&p.id, &p.name), "display preset")?;
    if load(path)? != expected {
        return Err(
            "Display presets changed in another window. Refresh before saving again.".into(),
        );
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
pub(crate) mod tests {
    use super::*;
    pub fn layout() -> Layout {
        Layout {
            monitors: vec![Monitor {
                adapter_luid: 1,
                target_id: 7,
                device_path: Some("monitor-a".into()),
                name: "Main monitor".into(),
                connected: true,
                hdr: None,
                enabled: true,
                desktop: Some(DesktopMode {
                    source_adapter_luid: 1,
                    source_id: 0,
                    source_name: "DISPLAY1".into(),
                    x: 0,
                    y: 0,
                    width: 2560,
                    height: 1440,
                    source_width: 2560,
                    source_height: 1440,
                    refresh_rate: RefreshRate {
                        numerator: 60000,
                        denominator: 1001,
                    },
                    rotation_degrees: 0,
                }),
            }],
        }
    }
    fn sample() -> Preset {
        Preset {
            id: "display-one".into(),
            name: "Desk".into(),
            captured_at_unix_seconds: 123,
            confirm_changes: true,
            layout: layout(),
        }
    }
    #[test]
    fn clone_negative_positions_and_fractional_rates_survive_round_trip() {
        let mut p = sample();
        let mut clone = p.layout.monitors[0].clone();
        clone.target_id = 8;
        clone.device_path = Some("monitor-b".into());
        p.layout.monitors.push(clone);
        let mut left = p.layout.monitors[0].clone();
        left.target_id = 9;
        let mode = left.desktop.as_mut().unwrap();
        mode.source_id = 1;
        mode.x = -1440;
        mode.y = -200;
        mode.width = 1440;
        mode.height = 2560;
        mode.rotation_degrees = 90;
        p.layout.monitors.push(left);
        validate(&[p.clone()]).unwrap();
        let json = serde_json::to_vec(&p).unwrap();
        let restored: Preset = serde_json::from_slice(&json).unwrap();
        assert_eq!(p, restored);
        assert_eq!(
            restored.layout.monitors[0]
                .desktop
                .as_ref()
                .unwrap()
                .refresh_rate
                .label(),
            "59.94 Hz"
        );
    }
    #[test]
    fn confirmation_opt_out_round_trips() {
        let original = sample();
        let mut automatic = original.clone();
        automatic.confirm_changes = false;
        let restored: Preset =
            serde_json::from_slice(&serde_json::to_vec(&automatic).unwrap()).unwrap();
        assert!(!restored.confirm_changes);
        assert_eq!(restored.layout, original.layout);
        assert_eq!(
            restored.captured_at_unix_seconds,
            original.captured_at_unix_seconds
        );
    }
    #[test]
    fn rejects_duplicate_names_targets_and_invalid_layouts() {
        let a = sample();
        let mut b = a.clone();
        b.id = "different".into();
        b.name = "desk".into();
        assert!(validate(&[a.clone(), b]).is_err());
        let mut p = a.clone();
        p.layout.monitors.push(p.layout.monitors[0].clone());
        assert!(validate(&[p]).is_err());
        for (numerator, denominator) in [(0, 1), (60, 0)] {
            let mut p = a.clone();
            p.layout.monitors[0].desktop.as_mut().unwrap().refresh_rate = RefreshRate {
                numerator,
                denominator,
            };
            assert!(validate(&[p]).is_err());
        }
        let mut p = a.clone();
        p.layout.monitors[0].desktop.as_mut().unwrap().x = 20;
        assert!(validate(&[p]).is_err());
        let mut p = a;
        p.layout.monitors[0].enabled = false;
        assert!(validate(&[p]).is_err());
    }
    #[test]
    fn storage_renames_without_recapture_and_preserves_stale_or_bad_files() {
        let dir = std::env::temp_dir().join(format!(
            "director-display-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let file = dir.join("display-presets.json");
        let first = vec![sample()];
        save(&file, &[], &first).unwrap();
        let mut renamed = first.clone();
        renamed[0].name = "Gaming".into();
        save(&file, &first, &renamed).unwrap();
        assert_eq!(load(&file).unwrap(), renamed);
        assert!(save(&file, &first, &[]).is_err());
        assert_eq!(load(&file).unwrap(), renamed);
        save(&file, &renamed, &[]).unwrap();
        for bytes in ["broken", r#"{"version":2,"presets":[]}"#] {
            fs::write(&file, bytes).unwrap();
            assert!(save(&file, &[], &first).is_err());
            assert_eq!(fs::read_to_string(&file).unwrap(), bytes);
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
