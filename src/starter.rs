//! The first scene: save how the PC is set up right now (screens, sound and FPS limit) as one
//! scene, so any other scene can bring everything back in one click. Saving reuses matching
//! presets instead of duplicating them, and never changes a device.
use crate::{
    audio::Output,
    displays::{self, Layout},
    fps, presets,
    scenes::{self, Scene},
};

/// The default output a new audio preset starts from: the console default, else any active one.
pub fn default_output(outputs: &[Output]) -> Option<&Output> {
    outputs
        .iter()
        .find(|o| o.console && o.active)
        .or_else(|| outputs.iter().find(|o| o.active))
}

/// The global FPS limit to keep, from the providers' current values. A provider with a limit
/// wins over one without; RTSS wins when both have one. Zero means no global limit.
pub fn fps_request(current: &[fps::Snapshot]) -> Option<fps::Request> {
    let pick = current
        .iter()
        .filter(|s| s.value > 0)
        .max_by_key(|s| s.provider == fps::Provider::Rtss)
        .or_else(|| current.first())?;
    Some(fps::Request {
        provider: pick.provider,
        limit: (pick.value > 0).then_some(pick.value),
        vrr_safe: false,
    })
}

/// A short description of the screens, e.g. "3 screens · main 2560×1440 at 240 Hz".
pub fn screens_summary(layout: &Layout) -> String {
    let on = layout.monitors.iter().filter(|m| m.enabled).count();
    let count = if on == 1 {
        "1 screen".to_string()
    } else {
        format!("{on} screens")
    };
    match layout
        .monitors
        .iter()
        .filter(|m| m.enabled)
        .filter_map(|m| m.desktop.as_ref())
        .find(|d| d.primary())
    {
        Some(d) => format!(
            "{count} · main {}×{} at {} Hz",
            d.width,
            d.height,
            d.refresh_rate.hz().round()
        ),
        None => count,
    }
}

/// `base`, or `base 2`, `base 3`… so it doesn't match a taken name (ignoring case).
pub fn unique_name<'a>(base: &str, taken: impl IntoIterator<Item = &'a str> + Clone) -> String {
    let free = |n: &str| {
        !taken
            .clone()
            .into_iter()
            .any(|t| t.trim().eq_ignore_ascii_case(n))
    };
    if free(base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| free(n))
        .unwrap()
}

/// What to save, chosen on the starter card.
#[derive(Clone, Debug)]
pub struct Choice {
    pub name: String,
    pub layout: Option<Layout>,
    pub output: Option<Output>,
    pub fps: Option<fps::Request>,
}

/// The documents after saving the starter, and the scene it made.
#[derive(Debug, PartialEq)]
pub struct Plan {
    pub displays: Vec<displays::Preset>,
    pub audio: Vec<presets::Preset>,
    pub scene: Scene,
}

/// Work out the new documents. `new_id` makes IDs for the items it adds.
pub fn plan(
    choice: &Choice,
    displays: &[displays::Preset],
    audio: &[presets::Preset],
    mut new_id: impl FnMut() -> String,
    now_unix: u64,
) -> Result<Plan, String> {
    let name = choice.name.trim();
    if name.is_empty() {
        return Err("Give the scene a name.".into());
    }
    let mut displays = displays.to_vec();
    let mut audio = audio.to_vec();
    let mut scene = Scene::new(format!("scene-{}", new_id()), name);
    if let Some(layout) = &choice.layout {
        layout.validate()?;
        scene.display = Some(match displays.iter().find(|p| &p.layout == layout) {
            Some(p) => p.id.clone(),
            None => {
                let preset = displays::Preset {
                    id: new_id(),
                    name: unique_name(
                        &format!("{name} screens"),
                        displays.iter().map(|p| p.name.as_str()),
                    ),
                    captured_at_unix_seconds: now_unix,
                    confirm_changes: false,
                    layout: layout.clone(),
                };
                let id = preset.id.clone();
                displays.push(preset);
                id
            }
        });
    }
    if let Some(output) = &choice.output {
        let volume = output.volume_percent;
        scene.audio = Some(
            match audio
                .iter()
                .find(|p| p.output_id == output.id && p.volume_percent == volume)
            {
                Some(p) => p.id.clone(),
                None => {
                    let preset = presets::Preset {
                        id: new_id(),
                        name: unique_name(
                            &format!("{name} sound"),
                            audio.iter().map(|p| p.name.as_str()),
                        ),
                        output_id: output.id.clone(),
                        output_name: output.name.clone(),
                        volume_percent: volume,
                        keep_changes: false,
                        communications_output: None,
                        input: None,
                    };
                    let id = preset.id.clone();
                    audio.push(preset);
                    id
                }
            },
        );
    }
    scene.fps = choice.fps.clone();
    if scene.display.is_none() && scene.audio.is_none() && scene.fps.is_none() {
        return Err("Choose at least one thing to save.".into());
    }
    scene.validate()?;
    Ok(Plan {
        displays,
        audio,
        scene,
    })
}

/// Save the starter scene and any presets it needs. Changes no device. Returns the scene.
#[cfg(windows)]
pub fn save(choice: &Choice) -> Result<Scene, String> {
    let _lock = crate::windows_audio::OperationLock::acquire()?;
    let display_path = displays::path()?;
    let audio_path = presets::path()?;
    let scene_path = scenes::path()?;
    let old_displays = displays::load(&display_path)?;
    let old_audio = presets::load(&audio_path)?;
    let old_scenes = scenes::load(&scene_path)?;
    if old_scenes
        .iter()
        .any(|s| s.name.trim().eq_ignore_ascii_case(choice.name.trim()))
    {
        return Err(format!(
            "A scene named “{}” already exists.",
            choice.name.trim()
        ));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let mut counter = 0u128;
    let plan = plan(
        choice,
        &old_displays,
        &old_audio,
        || {
            counter += 1;
            format!("{}-{}", std::process::id(), now.as_nanos() + counter)
        },
        now.as_secs(),
    )?;
    // Presets first: a scene must never point at a preset that wasn't saved.
    if plan.displays != old_displays {
        displays::save(&display_path, &old_displays, &plan.displays)?;
    }
    if plan.audio != old_audio {
        presets::save(&audio_path, &old_audio, &plan.audio)?;
    }
    let mut next = old_scenes.clone();
    next.push(plan.scene.clone());
    next.sort_by_key(|s| s.name.to_lowercase());
    scenes::save(&scene_path, &old_scenes, &next)?;
    Ok(plan.scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::displays::{DesktopMode, Monitor, RefreshRate};

    fn output(id: &str, volume: Option<u8>) -> Output {
        Output {
            id: id.into(),
            name: format!("{id} name"),
            state: "active".into(),
            active: true,
            console: true,
            multimedia: true,
            communications: true,
            volume_percent: volume,
        }
    }
    fn snapshot(provider: fps::Provider, value: u32) -> fps::Snapshot {
        fps::Snapshot {
            provider,
            value,
            inherited: false,
            limiter_disabled: None,
        }
    }
    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("id{n}")
        }
    }

    #[test]
    fn names_are_made_unique_ignoring_case() {
        assert_eq!(unique_name("Desk screens", ["Other"]), "Desk screens");
        assert_eq!(
            unique_name("Desk screens", ["desk screens", "Desk screens 2"]),
            "Desk screens 3"
        );
    }

    #[test]
    fn fps_prefers_a_set_limit_and_rtss() {
        use fps::Provider::{Nvidia, Rtss};
        assert_eq!(fps_request(&[]), None);
        let none = fps_request(&[snapshot(Nvidia, 0)]).unwrap();
        assert_eq!((none.provider, none.limit), (Nvidia, None));
        let set = fps_request(&[snapshot(Nvidia, 0), snapshot(Rtss, 228)]).unwrap();
        assert_eq!((set.provider, set.limit), (Rtss, Some(228)));
        let both = fps_request(&[snapshot(Rtss, 120), snapshot(Nvidia, 141)]).unwrap();
        assert_eq!((both.provider, both.limit), (Rtss, Some(120)));
        let nvidia = fps_request(&[snapshot(Rtss, 0), snapshot(Nvidia, 141)]).unwrap();
        assert_eq!((nvidia.provider, nvidia.limit), (Nvidia, Some(141)));
    }

    #[test]
    fn default_output_prefers_the_console_default() {
        let mut a = output("a", None);
        a.console = false;
        let b = output("b", None);
        assert_eq!(default_output(&[a.clone(), b]).unwrap().id, "b");
        assert_eq!(default_output(&[a]).unwrap().id, "a");
    }

    #[test]
    fn sound_reuses_a_matching_preset_and_adds_one_otherwise() {
        let existing = presets::Preset {
            id: "speakers".into(),
            name: "Desk sound".into(),
            output_id: "spk".into(),
            output_name: "Speakers".into(),
            volume_percent: Some(35),
            keep_changes: false,
            communications_output: None,
            input: None,
        };
        let mut choice = Choice {
            name: " Desk ".into(),
            layout: None,
            output: Some(output("spk", Some(35))),
            fps: None,
        };
        let plan1 = plan(&choice, &[], std::slice::from_ref(&existing), ids(), 1).unwrap();
        assert_eq!(plan1.scene.name, "Desk");
        assert_eq!(plan1.scene.audio.as_deref(), Some("speakers"));
        assert_eq!(plan1.audio.len(), 1);
        choice.output = Some(output("spk", Some(50)));
        let plan2 = plan(&choice, &[], &[existing], ids(), 1).unwrap();
        assert_eq!(plan2.audio.len(), 2);
        assert_eq!(plan2.audio[1].name, "Desk sound 2");
        assert_eq!(plan2.audio[1].volume_percent, Some(50));
        assert_eq!(
            plan2.scene.audio.as_deref(),
            Some(plan2.audio[1].id.as_str())
        );
    }

    #[test]
    fn screens_reuse_an_identical_layout() {
        let desktop = DesktopMode {
            source_adapter_luid: 1,
            source_id: 0,
            source_name: r"\\.\DISPLAY1".into(),
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            source_width: 2560,
            source_height: 1440,
            refresh_rate: RefreshRate {
                numerator: 240,
                denominator: 1,
            },
            rotation_degrees: 0,
        };
        let screens = Layout {
            monitors: vec![Monitor {
                adapter_luid: 1,
                target_id: 1,
                device_path: None,
                name: "Main".into(),
                connected: true,
                hdr: None,
                enabled: true,
                desktop: Some(desktop),
            }],
        };
        assert_eq!(
            screens_summary(&screens),
            "1 screen · main 2560×1440 at 240 Hz"
        );
        let saved = displays::Preset {
            id: "home".into(),
            name: "Home".into(),
            captured_at_unix_seconds: 0,
            confirm_changes: true,
            layout: screens.clone(),
        };
        let choice = Choice {
            name: "Desk".into(),
            layout: Some(screens.clone()),
            output: None,
            fps: Some(fps::Request {
                provider: fps::Provider::Rtss,
                limit: Some(228),
                vrr_safe: false,
            }),
        };
        let reused = plan(&choice, std::slice::from_ref(&saved), &[], ids(), 1).unwrap();
        assert_eq!(reused.displays.len(), 1);
        assert_eq!(reused.scene.display.as_deref(), Some("home"));
        assert_eq!(reused.scene.fps.unwrap().limit, Some(228));
        let fresh = plan(&choice, &[], &[], ids(), 7).unwrap();
        assert_eq!(fresh.displays[0].name, "Desk screens");
        assert_eq!(fresh.displays[0].captured_at_unix_seconds, 7);
        assert!(!fresh.displays[0].confirm_changes);
    }

    #[test]
    fn nothing_chosen_or_no_name_is_refused() {
        let empty = Choice {
            name: "Desk".into(),
            layout: None,
            output: None,
            fps: None,
        };
        assert!(plan(&empty, &[], &[], ids(), 1).is_err());
        let unnamed = Choice {
            name: "  ".into(),
            output: Some(output("spk", None)),
            ..empty
        };
        assert!(plan(&unnamed, &[], &[], ids(), 1).is_err());
    }
}
