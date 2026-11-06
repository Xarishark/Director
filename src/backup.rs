//! Export and restore scenes and display presets as one local file. TV connections and
//! pairing keys stay on the PC.
use crate::{
    audio::AudioBackend as _,
    displays,
    scenes::{self, SoundOutput},
    windows_audio::{OperationLock, WindowsAudio},
    windows_scenes,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

const LIMIT: u64 = 8 * 1024 * 1024;
/// Version 2 drops audio presets: scenes hold their sound. Version 1 backups still restore;
/// their scenes' audio presets are migrated into the Sound step.
const VERSION: u8 = 2;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    version: u8,
    scenes: Vec<scenes::Scene>,
    displays: Vec<displays::Preset>,
    /// Version 1 audio presets, read only to migrate the scenes that used them.
    #[serde(default, skip_serializing)]
    audio: Option<serde_json::Value>,
    tv_names: BTreeMap<String, String>,
}

pub struct Import {
    backup: Backup,
    previous: windows_scenes::Catalog,
    pub preview: String,
}

/// The TVs a scene uses, for preparation or for sound.
fn scene_tvs(scene: &scenes::Scene) -> impl Iterator<Item = &String> {
    scene
        .tv
        .iter()
        .map(|t| &t.id)
        .chain(scene.sound.iter().filter_map(|s| match &s.output {
            Some(SoundOutput::Tv { id }) => Some(id),
            _ => None,
        }))
}

/// The audio devices a scene's Sound step names, as (input, id, name).
fn scene_devices(scene: &scenes::Scene) -> Vec<(bool, &str, &str)> {
    let Some(sound) = &scene.sound else {
        return vec![];
    };
    let mut devices = vec![];
    if let Some(SoundOutput::Device { id, name }) = &sound.output {
        devices.push((false, id.as_str(), name.as_str()));
    }
    if let Some(d) = &sound.calls {
        devices.push((false, d.id.as_str(), d.name.as_str()));
    }
    if let Some(m) = &sound.mic {
        devices.push((true, m.device.id.as_str(), m.device.name.as_str()));
    }
    devices
}

pub fn export(path: &Path) -> Result<(), String> {
    let _lock = OperationLock::acquire()?;
    let (scenes, displays, tvs) = windows_scenes::catalog()?;
    let tv_names = tvs
        .into_iter()
        .filter(|tv| {
            scenes
                .iter()
                .any(|scene| scene_tvs(scene).any(|id| *id == tv.id))
        })
        .map(|tv| (tv.id, tv.name))
        .collect();
    let backup = Backup {
        version: VERSION,
        scenes,
        displays,
        audio: None,
        tv_names,
    };
    let bytes = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("The backup is too large.".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Could not create backup. Choose a new file name. {e}"))?;
    let result = file.write_all(&bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(path);
        return Err(format!("Could not write backup: {error}"));
    }
    Ok(())
}

fn read(path: &Path) -> Result<Backup, String> {
    let file = File::open(path).map_err(|e| format!("Could not open backup: {e}"))?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read backup: {e}"))?;
    if bytes.len() as u64 > LIMIT {
        return Err("The backup is too large.".into());
    }
    let mut backup: Backup =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Director backup: {e}"))?;
    if !(1..=VERSION).contains(&backup.version) {
        return Err("This backup version is unsupported.".into());
    }
    let legacy = backup
        .audio
        .take()
        .and_then(|v| v.get("presets").cloned().or(Some(v)));
    scenes::migrate(&mut backup.scenes, legacy);
    scenes::validate_backup(&backup.scenes)?;
    displays::validate(&backup.displays)?;
    Ok(backup)
}

pub fn preview(path: &Path) -> Result<Import, String> {
    let backup = read(path)?;
    let _lock = OperationLock::acquire()?;
    let previous = windows_scenes::catalog()?;
    let mut preview = format!(
        "Restore {} scenes and {} display presets.\n\nThis replaces your current {} scenes and {} display presets. TV connections stay on this PC.",
        backup.scenes.len(),
        backup.displays.len(),
        previous.0.len(),
        previous.1.len(),
    );
    let mut issues = Vec::new();
    for scene in &backup.scenes {
        if let Err(error) = scenes::resolve(scene, &backup.displays, &previous.2) {
            let names = scene_tvs(scene)
                .filter_map(|id| backup.tv_names.get(id))
                .map(String::as_str)
                .collect::<Vec<_>>();
            let context = if names.is_empty() {
                String::new()
            } else {
                format!(" ({})", names.join(", "))
            };
            issues.push(format!("{}{} · {error}", scene.name, context));
        }
    }
    if let Ok(live) = crate::windows_displays::capture() {
        for preset in &backup.displays {
            for monitor in preset
                .layout
                .monitors
                .iter()
                .filter(|monitor| monitor.enabled)
            {
                if !monitor.device_path.as_ref().is_some_and(|path| {
                    live.monitors.iter().any(|m| {
                        m.device_path
                            .as_ref()
                            .is_some_and(|current| current.eq_ignore_ascii_case(path))
                    })
                }) {
                    issues.push(format!("{} · Check display {}", preset.name, monitor.name));
                }
            }
        }
    }
    if let Ok(mut audio) = WindowsAudio::new() {
        for (input, devices) in [(false, audio.outputs()), (true, audio.inputs())] {
            let Ok(devices) = devices else { continue };
            for scene in &backup.scenes {
                for (_, id, name) in scene_devices(scene)
                    .into_iter()
                    .filter(|(kind, ..)| *kind == input)
                {
                    let resolved = audio.resolve_id(id, input);
                    if !devices.iter().any(|device| device.id == resolved) {
                        issues.push(format!("{} · Check audio device {name}", scene.name));
                    }
                }
            }
        }
    }
    if !issues.is_empty() {
        preview.push_str(
            "\n\nCheck these links before applying. Offline devices may return when connected.\n",
        );
        preview.push_str(
            &issues
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
        );
        if issues.len() > 20 {
            preview.push_str(&format!("\n… and {} more.", issues.len() - 20));
        }
    }
    if backup.scenes.iter().any(|scene| !scene.actions.is_empty()) {
        preview.push_str("\n\nReview restored app and link actions before applying a scene.");
    }
    Ok(Import {
        backup,
        previous,
        preview,
    })
}

pub fn restore(import: &Import) -> Result<(), String> {
    let _lock = OperationLock::acquire()?;
    if windows_scenes::catalog()? != import.previous {
        return Err(
            "Director settings changed after the preview. Preview the backup again.".into(),
        );
    }
    restore_catalog(import, &scenes::path()?, &displays::path()?)
}

fn restore_catalog(import: &Import, scene_path: &Path, display_path: &Path) -> Result<(), String> {
    let (old_scenes, old_displays, _) = &import.previous;
    let Backup {
        scenes: new_scenes,
        displays: new_displays,
        ..
    } = &import.backup;
    displays::save(display_path, old_displays, new_displays)?;
    if let Err(error) = scenes::save(scene_path, old_scenes, new_scenes) {
        return Err(
            match displays::save(display_path, new_displays, old_displays) {
                Ok(()) => format!("{error} Previous display presets restored."),
                Err(recovery) => {
                    format!("{error} Could not restore the previous display presets: {recovery}")
                }
            },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_rolls_back_display_presets_when_the_scene_write_fails() {
        let folder =
            std::env::temp_dir().join(format!("director-backup-test-{}", std::process::id()));
        fs::create_dir(&folder).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(folder.clone());
        let scene_path = folder.join("scenes.json");
        let display_path = folder.join("displays.json");
        let original = displays::Preset {
            id: "old".into(),
            name: "Desk".into(),
            captured_at_unix_seconds: 0,
            confirm_changes: false,
            layout: crate::display_apply::tests::layout(0),
        };
        displays::save(&display_path, &[], std::slice::from_ref(&original)).unwrap();

        let imported = Backup {
            version: VERSION,
            scenes: vec![],
            displays: vec![],
            audio: None,
            tv_names: BTreeMap::new(),
        };
        let file = folder.join("Director.director-backup");
        fs::write(&file, serde_json::to_vec(&imported).unwrap()).unwrap();
        let import = Import {
            backup: read(&file).unwrap(),
            previous: (vec![], vec![original.clone()], vec![]),
            preview: String::new(),
        };

        // The scene store's temporary file is blocked only after the display write succeeds.
        let blocker = scene_path.with_extension("json.tmp");
        fs::create_dir(&blocker).unwrap();
        let error = restore_catalog(&import, &scene_path, &display_path).unwrap_err();
        assert!(
            error.contains("Previous display presets restored."),
            "{error}"
        );
        assert_eq!(displays::load(&display_path).unwrap(), vec![original]);

        fs::remove_dir(blocker).unwrap();
        restore_catalog(&import, &scene_path, &display_path).unwrap();
        assert!(displays::load(&display_path).unwrap().is_empty());
    }

    #[test]
    fn version_one_backups_migrate_audio_presets_into_scene_sound() {
        let folder =
            std::env::temp_dir().join(format!("director-backup-v1-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let file = folder.join("old.director-backup");
        let v1 = serde_json::json!({
            "version": 1,
            "scenes": [{"id":"s","name":"Desk","display":null,"audio":"p","tv":null,"actions":[]}],
            "displays": [],
            "audio": [{"id":"p","name":"Desk sound","output_id":"soundbar","output_name":"SOUNDBAR","volume_percent":10,"keep_changes":false}],
            "tv_names": {}
        });
        fs::write(&file, serde_json::to_vec(&v1).unwrap()).unwrap();
        let backup = read(&file).unwrap();
        let sound = backup.scenes[0].sound.as_ref().unwrap();
        assert_eq!(sound.volume, Some(10));
        assert!(matches!(&sound.output, Some(SoundOutput::Device { id, .. }) if id == "soundbar"));
        let _ = fs::remove_dir_all(folder);
    }
}
