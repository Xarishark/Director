use crate::{
    audio::AudioBackend as _,
    displays, presets, scenes,
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    version: u8,
    scenes: Vec<scenes::Scene>,
    displays: Vec<displays::Preset>,
    audio: Vec<presets::Preset>,
    tv_names: BTreeMap<String, String>,
}

pub struct Import {
    backup: Backup,
    previous: windows_scenes::Catalog,
    pub preview: String,
}

pub fn export(path: &Path) -> Result<(), String> {
    let _lock = OperationLock::acquire()?;
    let (scenes, displays, audio, tvs) = windows_scenes::catalog()?;
    let tv_names = tvs
        .into_iter()
        .filter(|tv| {
            scenes.iter().any(|scene| {
                scene.tv_audio.as_ref() == Some(&tv.id)
                    || scene.tv.as_ref().is_some_and(|prep| prep.id == tv.id)
            })
        })
        .map(|tv| (tv.id, tv.name))
        .collect();
    let backup = Backup {
        version: 1,
        scenes,
        displays,
        audio,
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
    let backup: Backup =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Director backup: {e}"))?;
    if backup.version != 1 {
        return Err("This backup version is unsupported.".into());
    }
    scenes::validate_backup(&backup.scenes)?;
    displays::validate(&backup.displays)?;
    presets::validate(&backup.audio)?;
    Ok(backup)
}

pub fn preview(path: &Path) -> Result<Import, String> {
    let backup = read(path)?;
    let _lock = OperationLock::acquire()?;
    let previous = windows_scenes::catalog()?;
    let mut preview = format!(
        "Restore {} scenes, {} display presets, and {} audio presets.\n\nThis replaces your current {} scenes, {} display presets, and {} audio presets. TV connections stay on this PC.",
        backup.scenes.len(),
        backup.displays.len(),
        backup.audio.len(),
        previous.0.len(),
        previous.1.len(),
        previous.2.len(),
    );
    let mut issues = Vec::new();
    for scene in &backup.scenes {
        if let Err(error) = scenes::resolve(scene, &backup.displays, &backup.audio, &previous.3) {
            let names = scene
                .tv
                .iter()
                .map(|tv| &tv.id)
                .chain(scene.tv_audio.iter())
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
            for preset in &backup.audio {
                let endpoints = if input {
                    preset
                        .input
                        .iter()
                        .map(|device| (device.id.as_str(), device.name.as_str()))
                        .collect::<Vec<_>>()
                } else {
                    std::iter::once((preset.output_id.as_str(), preset.output_name.as_str()))
                        .chain(
                            preset
                                .communications_output
                                .iter()
                                .map(|device| (device.id.as_str(), device.name.as_str())),
                        )
                        .collect()
                };
                for (id, name) in endpoints {
                    let resolved = audio.resolve_id(id, input);
                    if !devices.iter().any(|device| device.id == resolved) {
                        issues.push(format!("{} · Check audio device {name}", preset.name));
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
    restore_catalog(
        import,
        &scenes::path()?,
        &displays::path()?,
        &presets::path()?,
    )
}

fn restore_catalog(
    import: &Import,
    scene_path: &Path,
    display_path: &Path,
    audio_path: &Path,
) -> Result<(), String> {
    let (old_scenes, old_displays, old_audio, _) = &import.previous;
    let Backup {
        scenes: new_scenes,
        displays: new_displays,
        audio: new_audio,
        ..
    } = &import.backup;

    displays::save(display_path, old_displays, new_displays)?;
    let mut audio_saved = false;
    let result = (|| {
        presets::save(audio_path, old_audio, new_audio)?;
        audio_saved = true;
        scenes::save(scene_path, old_scenes, new_scenes)
    })();
    if let Err(error) = result {
        let mut recovery = Vec::new();
        if audio_saved && let Err(error) = presets::save(audio_path, new_audio, old_audio) {
            recovery.push(error);
        }
        if let Err(error) = displays::save(display_path, new_displays, old_displays) {
            recovery.push(error);
        }
        return Err(if recovery.is_empty() {
            format!("{error} Previous presets restored.")
        } else {
            format!(
                "{error} Could not fully restore the previous presets: {}",
                recovery.join("; ")
            )
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_rolls_back_earlier_catalogs_when_the_scene_write_fails() {
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
        let audio_path = folder.join("audio.json");
        let original = presets::Preset {
            id: "old".into(),
            name: "Desk".into(),
            output_id: "id".into(),
            output_name: "Speakers".into(),
            volume_percent: None,
            keep_changes: false,
            communications_output: None,
            input: None,
        };
        presets::save(&audio_path, &[], std::slice::from_ref(&original)).unwrap();

        let imported = Backup {
            version: 1,
            scenes: vec![],
            displays: vec![],
            audio: vec![],
            tv_names: BTreeMap::new(),
        };
        let file = folder.join("Director.director-backup");
        fs::write(&file, serde_json::to_vec(&imported).unwrap()).unwrap();
        let import = Import {
            backup: read(&file).unwrap(),
            previous: (vec![], vec![], vec![original.clone()], vec![]),
            preview: String::new(),
        };

        // The scene store's temporary file is blocked only after earlier writes succeed.
        let blocker = scene_path.with_extension("json.tmp");
        fs::create_dir(&blocker).unwrap();
        let error = restore_catalog(&import, &scene_path, &display_path, &audio_path).unwrap_err();
        assert!(error.contains("Previous presets restored."));
        assert_eq!(presets::load(&audio_path).unwrap(), vec![original]);

        fs::remove_dir(blocker).unwrap();
        restore_catalog(&import, &scene_path, &display_path, &audio_path).unwrap();
        assert!(presets::load(&audio_path).unwrap().is_empty());
    }
}
