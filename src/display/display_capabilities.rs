//! Previously detected capabilities; never replace a complete inventory with inactive fallbacks.
use crate::{
    display::display_detection::Learned,
    display::display_editor::{Editor, Mode, Options},
    display::displays::{DesktopMode, Layout},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub device_path: String,
    pub name: String,
    pub modes: Vec<Mode>,
    pub hdr_available: bool,
    pub desktop: DesktopMode,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    entries: Vec<Entry>,
}
pub fn path() -> Result<PathBuf, String> {
    crate::storage::settings_path("display-capabilities.json")
}
pub fn load(path: &Path) -> Result<Vec<Entry>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("Could not read remembered display options: {e}")),
    };
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Remembered display options are too large.".into());
    }
    let document: Document = serde_json::from_slice(&bytes).map_err(
        |_| "Remembered display options could not be read; the file was left unchanged.",
    )?;
    if document.version != 1 {
        return Err("Remembered display options require a newer Director version.".into());
    }
    validate(&document.entries)?;
    Ok(document.entries)
}
fn validate(entries: &[Entry]) -> Result<(), String> {
    if entries.len() > 128 {
        return Err("Too many remembered displays.".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for e in entries {
        if e.device_path.is_empty()
            || e.device_path.len() > 4096
            || !ids.insert(e.device_path.to_lowercase())
            || e.modes.is_empty()
            || e.modes.len() > 16384
            || e.modes.iter().any(|m| {
                m.width == 0
                    || m.height == 0
                    || m.width > 65536
                    || m.height > 65536
                    || m.refresh.numerator == 0
                    || m.refresh.denominator == 0
            })
            || !e.modes.iter().any(|m| {
                m.width == e.desktop.source_width
                    && m.height == e.desktop.source_height
                    && m.refresh == e.desktop.refresh_rate
            })
        {
            return Err(
                "Remembered display options are invalid; the file was left unchanged.".into(),
            );
        }
    }
    Ok(())
}
pub fn remember(path: &Path, learned: &Learned) -> Result<(), String> {
    if learned.options.warning.is_some() {
        return Err("Partial display options cannot replace remembered capabilities.".into());
    }
    let mut entries = load(path)?;
    entries.retain(|e| !e.device_path.eq_ignore_ascii_case(&learned.device_path));
    entries.push(Entry {
        device_path: learned.device_path.clone(),
        name: learned.name.clone(),
        modes: learned.options.modes.clone(),
        hdr_available: learned.options.hdr.is_some(),
        desktop: learned.desktop.clone(),
    });
    validate(&entries)?;
    crate::storage::write_document(
        path,
        &Document {
            version: 1,
            entries,
        },
    )
}
pub fn merge(layout: &Layout, options: &mut [Options], entries: &[Entry]) {
    for (m, option) in layout.monitors.iter().zip(options) {
        if !m.enabled
            && let Some(e) = m.device_path.as_ref().and_then(|id| {
                entries
                    .iter()
                    .find(|e| e.device_path.eq_ignore_ascii_case(id))
            })
        {
            option.modes = e.modes.clone();
            option.hdr = e.hdr_available.then_some(false);
            option.warning = Some("Previously detected · checked again when applied.".into());
        }
    }
}
/// Prefer Windows' native dimensions over a larger driver/virtual resolution.
/// Rates are compared as exact rational values, never rounded labels.
pub fn recommended_mode(options: &Options, preferred: Option<(u32, u32)>) -> Option<Mode> {
    let (width, height) = preferred?;
    options
        .modes
        .iter()
        .filter(|m| m.width == width && m.height == height)
        .max_by(|a, b| {
            (u64::from(a.refresh.numerator) * u64::from(b.refresh.denominator))
                .cmp(&(u64::from(b.refresh.numerator) * u64::from(a.refresh.denominator)))
        })
        .cloned()
}

pub fn apply_to_editor(
    layout: &mut Layout,
    editor: &mut Editor,
    learned: &Learned,
) -> Result<(), String> {
    let i = layout
        .monitors
        .iter()
        .position(|m| {
            m.device_path
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(&learned.device_path))
        })
        .ok_or("This display is no longer in the draft.")?;

    editor.options[i] = learned.options.clone();
    editor.remembered[i] = Some(learned.desktop.clone());
    layout.monitors[i].connected = true;
    if !layout.monitors[i].enabled {
        editor.change(
            layout,
            i,
            crate::display::display_editor::Change::Enabled(true),
        )?;
    } else if let Some(d) = &mut layout.monitors[i].desktop {
        let (x, y, rotation) = (d.x, d.y, d.rotation_degrees);
        *d = learned.desktop.clone();
        d.x = x;
        d.y = y;
        d.rotation_degrees = rotation;
        (d.width, d.height) = if matches!(rotation, 90 | 270) {
            (d.source_height, d.source_width)
        } else {
            (d.source_width, d.source_height)
        };
    }
    if layout.monitors[i].hdr.is_none() {
        layout.monitors[i].hdr = learned.options.hdr;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn learned() -> Learned {
        let refresh = crate::display::displays::RefreshRate {
            numerator: 120000,
            denominator: 1001,
        };
        Learned {
            device_path: "test-tv".into(),
            name: "TV".into(),
            options: Options {
                modes: vec![Mode {
                    width: 3840,
                    height: 2160,
                    refresh: refresh.clone(),
                }],
                warning: None,
                hdr: Some(false),
            },
            desktop: DesktopMode {
                source_adapter_luid: 1,
                source_id: 1,
                source_name: "fixture".into(),
                x: 1920,
                y: 0,
                width: 3840,
                height: 2160,
                source_width: 3840,
                source_height: 2160,
                refresh_rate: refresh,
                rotation_degrees: 0,
            },
        }
    }
    #[test]
    fn detection_replaces_a_supported_fallback_with_native_resolution_and_fastest_exact_rate() {
        let mut detected = learned();
        let fallback = Mode {
            width: 640,
            height: 480,
            refresh: crate::display::displays::RefreshRate {
                numerator: 60,
                denominator: 1,
            },
        };
        detected.options.modes.extend([
            fallback.clone(),
            Mode {
                width: 3840,
                height: 2160,
                refresh: crate::display::displays::RefreshRate {
                    numerator: 120,
                    denominator: 1,
                },
            },
            Mode {
                width: 7680,
                height: 4320,
                refresh: crate::display::displays::RefreshRate {
                    numerator: 60,
                    denominator: 1,
                },
            },
        ]);
        let best = recommended_mode(&detected.options, Some((3840, 2160))).unwrap();
        assert_eq!(best.refresh.numerator, 120);
        assert_eq!(best.refresh.denominator, 1);
        assert!(recommended_mode(&detected.options, None).is_none());
        assert!(recommended_mode(&detected.options, Some((123, 456))).is_none());
        detected.desktop.refresh_rate = best.refresh;
        let mut old = detected.desktop.clone();
        old.source_width = 640;
        old.source_height = 480;
        old.width = 480;
        old.height = 640;
        old.rotation_degrees = 90;
        old.x = 2560;
        old.y = -100;
        old.refresh_rate = fallback.refresh;
        let mut layout = Layout {
            monitors: vec![crate::display::displays::Monitor {
                adapter_luid: 1,
                target_id: 1,
                device_path: Some("test-tv".into()),
                name: "TV".into(),
                connected: true,
                enabled: true,
                hdr: None,
                desktop: Some(old),
            }],
        };
        let mut editor = Editor::new(&layout, vec![detected.options.clone()]);
        // 640x480 is valid: being in the inventory must not keep it selected after detection.
        apply_to_editor(&mut layout, &mut editor, &detected).unwrap();
        let mode = layout.monitors[0].desktop.as_ref().unwrap();
        assert_eq!((mode.source_width, mode.source_height), (3840, 2160));
        assert_eq!((mode.width, mode.height), (2160, 3840));
        assert_eq!((mode.x, mode.y, mode.rotation_degrees), (2560, -100, 90));
        assert_eq!(mode.refresh_rate.numerator, 120);
    }

    #[test]
    fn cache_preserves_exact_modes_and_rejects_partial_or_corrupt_overwrites() {
        let path = std::env::temp_dir().join(format!(
            "director-capabilities-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut detected = learned();
        remember(&path, &detected).unwrap();
        let entries = load(&path).unwrap();
        assert_eq!(entries[0].modes[0].refresh.denominator, 1001);
        let original = std::fs::read(&path).unwrap();
        detected.options.warning = Some("partial".into());
        assert!(remember(&path, &detected).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let layout = Layout {
            monitors: vec![crate::display::displays::Monitor {
                adapter_luid: 1,
                target_id: 1,
                device_path: Some("TEST-TV".into()),
                name: "renamed".into(),
                connected: false,
                hdr: None,
                enabled: false,
                desktop: None,
            }],
        };
        let mut options = vec![Options::default()];
        merge(&layout, &mut options, &entries);
        assert_eq!(options[0].modes[0].width, 3840);
        assert!(
            options[0]
                .warning
                .as_ref()
                .unwrap()
                .starts_with("Previously detected")
        );
        let broken = b"broken";
        std::fs::write(&path, broken).unwrap();
        detected.options.warning = None;
        assert!(remember(&path, &detected).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), broken);
        std::fs::remove_file(path).unwrap();
    }
}
