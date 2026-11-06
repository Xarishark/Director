//! Independent preset references and a shared, ordered scene transaction.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub display: Option<String>,
    pub audio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tv_audio: Option<String>,
    pub tv: Option<TvPreparation>,
    #[serde(default)]
    pub fps: Option<crate::fps::Request>,
    /// Runs first, before any device changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<Action>,
    /// The After list: runs once every device step has succeeded.
    #[serde(default)]
    pub actions: Vec<Action>,
    /// Runs when another scene is applied or this one is ended.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leave: Vec<Action>,
    /// Version 1 per-scene policy. Read so old files load, never written; see `ScenePolicy`.
    #[serde(default, rename = "keep_on_failure", skip_serializing)]
    legacy_keep_on_failure: Option<bool>,
    #[serde(default, rename = "wait_seconds", skip_serializing)]
    legacy_wait_seconds: Option<u16>,
}
/// How scenes behave when a step fails or waits for devices. Global, from Preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenePolicy {
    pub keep_completed: bool,
    pub wait_seconds: u16,
}
impl Default for ScenePolicy {
    fn default() -> Self {
        Self {
            keep_completed: false,
            wait_seconds: 30,
        }
    }
}
/// Which action list of a scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionList {
    Before,
    After,
    Leave,
}
impl Scene {
    /// An empty scene that changes nothing yet.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            ..Default::default()
        }
    }
    pub fn list(&self, list: ActionList) -> &Vec<Action> {
        match list {
            ActionList::Before => &self.before,
            ActionList::After => &self.actions,
            ActionList::Leave => &self.leave,
        }
    }
    pub fn list_mut(&mut self, list: ActionList) -> &mut Vec<Action> {
        match list {
            ActionList::Before => &mut self.before,
            ActionList::After => &mut self.actions,
            ActionList::Leave => &mut self.leave,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TvPreparation {
    pub id: String,
    pub wake: bool,
    pub input: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Action {
    App {
        path: String,
        args: Vec<String>,
        working_directory: Option<String>,
        /// Wait for the program to exit (a non-zero exit or timeout fails the step).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        wait: bool,
    },
    Uri {
        target: String,
    },
    /// Close a running program by executable name (`steam.exe`) or full path.
    Close {
        target: String,
        /// End the program if it hasn't closed after being asked.
        #[serde(default)]
        force: bool,
    },
}
fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
impl Action {
    pub fn label(&self) -> String {
        match self {
            Self::App { path, wait, .. } => format!(
                "{} {}",
                if *wait { "Run" } else { "Launch" },
                file_name(path)
            ),
            Self::Uri { target } => format!("Open {target}"),
            Self::Close { target, .. } => format!("Close {}", file_name(target)),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Close { target, .. } => {
                if target.trim().is_empty()
                    || target.len() > 260
                    || target.chars().any(char::is_control)
                    || !target.to_lowercase().ends_with(".exe")
                {
                    return Err(
                        "Enter the program to close, such as steam.exe or its full path.".into(),
                    );
                }
            }
            Self::App {
                path,
                args,
                working_directory,
                ..
            } => {
                if !Path::new(path).is_absolute()
                    || !path.to_lowercase().ends_with(".exe")
                    || path.contains('\0')
                    || args.len() > 128
                    || args.iter().any(|a| a.contains('\0') || a.len() > 8192)
                    || working_directory
                        .as_ref()
                        .is_some_and(|p| !Path::new(p).is_absolute() || p.contains('\0'))
                {
                    return Err("Choose an absolute executable path and working folder; use one argument per line.".into());
                }
            }
            Self::Uri { target } => {
                let (scheme, rest) = target
                    .split_once(':')
                    .ok_or("Enter a complete URI, for example steam://open/bigpicture.")?;
                if scheme.is_empty()
                    || rest.is_empty()
                    || !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                    || !scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
                    || target.chars().any(char::is_control)
                    || target.len() > 8192
                {
                    return Err("Enter a valid URI.".into());
                }
            }
        }
        Ok(())
    }
}
impl Scene {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 200
            || self.name.trim().is_empty()
            || self.name.len() > 160
            || [&self.before, &self.actions, &self.leave]
                .iter()
                .any(|l| l.len() > 32)
            || [&self.display, &self.audio]
                .iter()
                .any(|v| v.as_ref().is_some_and(|s| s.is_empty() || s.len() > 200))
        {
            return Err("Give the scene a name and valid selections.".into());
        }
        if self.tv.as_ref().is_some_and(|t| {
            t.id.is_empty()
                || t.id.len() > 200
                || t.input
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || s.len() > 256)
        }) {
            return Err("Select a saved TV and input, or leave TV preparation off.".into());
        }
        if self.audio.is_some() && self.tv_audio.is_some() {
            return Err("Choose an audio preset or linked TV audio, not both.".into());
        }
        if self
            .tv_audio
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 200)
        {
            return Err("Choose a saved TV audio link.".into());
        }
        if self.tv.is_none()
            && self.display.is_none()
            && self.audio.is_none()
            && self.tv_audio.is_none()
            && self.fps.is_none()
            && self.before.is_empty()
            && self.actions.is_empty()
        {
            return Err("Choose at least one preset or action.".into());
        }
        if let Some(fps) = &self.fps {
            fps.validate()?;
        }
        for a in self.before.iter().chain(&self.actions).chain(&self.leave) {
            a.validate()?
        }
        Ok(())
    }
    /// Drop version 1 per-scene policy fields once they have been read.
    fn normalize(&mut self) {
        self.legacy_keep_on_failure = None;
        self.legacy_wait_seconds = None;
    }
}
#[derive(Clone)]
pub struct Plan {
    pub scene: Scene,
    pub display: Option<crate::displays::Preset>,
    pub audio: Option<crate::presets::Preset>,
    pub tv: Option<crate::tvs::Tv>,
    pub policy: ScenePolicy,
}
pub fn resolve(
    scene: &Scene,
    displays: &[crate::displays::Preset],
    audio: &[crate::presets::Preset],
    tvs: &[crate::tvs::Tv],
) -> Result<Plan, String> {
    scene.validate()?;
    let display = scene
        .display
        .as_ref()
        .map(|id| {
            displays
                .iter()
                .find(|p| &p.id == id)
                .cloned()
                .ok_or("The scene’s display preset is missing. Edit the scene to relink it.")
        })
        .transpose()?;
    let mut audio = scene
        .audio
        .as_ref()
        .map(|id| {
            audio
                .iter()
                .find(|p| &p.id == id)
                .cloned()
                .ok_or("The scene’s audio preset is missing. Edit the scene to relink it.")
        })
        .transpose()?;
    if let Some(id) = &scene.tv_audio {
        let tv = tvs
            .iter()
            .find(|t| &t.id == id)
            .ok_or("The linked audio TV is missing.")?;
        let link = tv
            .audio
            .as_ref()
            .ok_or("This TV has no linked audio output. Edit its TV connection first.")?;
        audio = Some(crate::presets::Preset {
            id: format!("tv-audio-{}", tv.id),
            name: format!("{} linked audio", tv.name),
            output_id: link.id.clone(),
            output_name: link.name.clone(),
            volume_percent: None,
            keep_changes: false,
            communications_output: None,
            input: None,
        });
    }
    let tv = scene
        .tv
        .as_ref()
        .map(|t| {
            tvs.iter()
                .find(|p| p.id == t.id)
                .cloned()
                .ok_or("The scene’s TV is missing. Edit the scene to relink it.")
        })
        .transpose()?;
    if let (Some(tv), Some(prep)) = (&tv, &scene.tv) {
        if tv.credential.is_none() {
            return Err("Pair the selected TV before applying this scene.".into());
        }
        if prep
            .input
            .as_ref()
            .is_some_and(|id| !tv.control_inputs().iter().any(|i| &i.id == id))
        {
            return Err("The selected TV input is no longer remembered. Edit the scene.".into());
        }
    }
    Ok(Plan {
        scene: scene.clone(),
        display,
        audio,
        tv,
        policy: ScenePolicy::default(),
    })
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    scenes: Vec<Scene>,
}
pub fn path() -> Result<PathBuf, String> {
    crate::presets::settings_path("scenes.json")
}
pub(crate) fn validate(scenes: &[Scene]) -> Result<(), String> {
    if scenes.len() > 256 {
        return Err("Too many scenes.".into());
    }
    for (i, s) in scenes.iter().enumerate() {
        s.validate()?;
        if scenes[..i]
            .iter()
            .any(|o| o.id == s.id || o.name.eq_ignore_ascii_case(&s.name))
        {
            return Err("Each scene needs a unique name.".into());
        }
    }
    Ok(())
}
const STORAGE_LIMIT: usize = 2 * 1024 * 1024;

pub(crate) fn validate_backup(scenes: &[Scene]) -> Result<(), String> {
    validate(scenes)?;
    let bytes = serde_json::to_vec_pretty(&Document {
        version: VERSION,
        scenes: scenes.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() > STORAGE_LIMIT {
        return Err("The restored scene catalog would exceed Director's storage limit.".into());
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<Vec<Scene>, String> {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("Could not read scenes: {e}")),
    };
    if data.len() > STORAGE_LIMIT {
        return Err("Scene file is too large. It was left unchanged.".into());
    }
    let mut doc: Document = serde_json::from_slice(&data)
        .map_err(|_| "Could not read scenes. The file was left unchanged.")?;
    if doc.version != 1 && doc.version != VERSION {
        return Err(
            "These scenes require a newer Director version. The file was left unchanged.".into(),
        );
    }
    doc.scenes.iter_mut().for_each(Scene::normalize);
    validate(&doc.scenes)?;
    Ok(doc.scenes)
}
pub fn save(path: &Path, expected: &[Scene], next: &[Scene]) -> Result<(), String> {
    validate(next)?;
    if load(path)? != expected {
        return Err("Scenes changed in another window. Reopen Scenes before saving.".into());
    }
    crate::presets::write_document(
        path,
        &Document {
            version: VERSION,
            scenes: next.to_vec(),
        },
    )
}
/// Version 2 adds Before/Leave actions, waited apps and Close actions, and drops the
/// per-scene failure policy. Version 1 files still load.
const VERSION: u32 = 2;
pub fn references(kind: &str, id: &str) -> Result<Vec<String>, String> {
    Ok(load(&path()?)?
        .into_iter()
        .filter(|s| match kind {
            "audio" => s.audio.as_deref() == Some(id),
            "display" => s.display.as_deref() == Some(id),
            "tv" => s.tv.as_ref().is_some_and(|t| t.id == id) || s.tv_audio.as_deref() == Some(id),
            _ => false,
        })
        .map(|s| s.name)
        .collect())
}
pub fn check_delete(kind: &str, id: &str) -> Result<(), String> {
    let names = references(kind, id)?;
    if names.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Used by scenes: {}. Edit those scenes before deleting it.",
            names.join(", ")
        ))
    }
}
#[derive(Clone, Debug)]
pub struct Report {
    pub success: bool,
    pub message: String,
    pub details: Vec<String>,
}
pub trait Backend {
    type Snapshot;
    fn snapshot(&mut self, plan: &Plan) -> Result<Self::Snapshot, String>;
    fn tv(
        &mut self,
        plan: &Plan,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<(), String>;
    fn display(&mut self, plan: &Plan) -> Result<(), String>;
    fn audio(
        &mut self,
        plan: &Plan,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<(), String>;
    fn fps(&mut self, plan: &Plan) -> Result<(), String>;
    /// Run an app, URI or Close action. A waited app may take up to `wait_seconds`.
    fn launch(&mut self, action: &Action, wait_seconds: u16) -> Result<(), String>;
    fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), String>;
}
pub fn apply(
    backend: &mut impl Backend,
    plan: &Plan,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&str),
) -> Report {
    let _operation = crate::diagnostics::Operation::start("scene.apply", Some(&plan.scene.id));
    let snapshot = match backend.snapshot(plan) {
        Ok(s) => s,
        Err(e) => {
            return Report {
                success: false,
                message: "Scene was not applied.".into(),
                details: vec![e],
            };
        }
    };
    let mut details = vec![];
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
            for action in &plan.scene.before {
                if cancel.load(Ordering::Acquire) {
                    return Err("Scene canceled.".into());
                }
                progress(&action.label());
                backend.launch(action, plan.policy.wait_seconds)?;
                details.push(action_done(action));
            }
            for (enabled, stage) in [
                (plan.tv.is_some(), 0),
                (plan.display.is_some(), 1),
                (plan.audio.is_some(), 2),
                (plan.scene.fps.is_some(), 3),
            ] {
                if cancel.load(Ordering::Acquire) {
                    return Err("Scene canceled.".into());
                }
                if !enabled {
                    continue;
                }
                let label = [
                    "Preparing TV…",
                    "Applying displays…",
                    "Waiting for audio devices…",
                    "Applying FPS limit…",
                ][stage];
                progress(label);
                crate::diagnostics::event("scene.stage", "started", label);
                match stage {
                    0 => backend.tv(plan, cancel, &mut progress)?,
                    1 => backend.display(plan)?,
                    2 => backend.audio(plan, cancel, &mut progress)?,
                    _ => backend.fps(plan)?,
                }
                if stage == 0
                    && plan
                        .tv
                        .as_ref()
                        .is_some_and(|t| t.brand != crate::tvs::Brand::Lg)
                    && plan.scene.tv.as_ref().is_some_and(|t| t.input.is_some())
                {
                    details.push(
                        "TV direct input command was sent; input readback is unavailable.".into(),
                    );
                }
                details.push(format!(
                    "{} completed.",
                    ["TV preparation", "Displays", "Audio", "FPS limit"][stage]
                ));
            }
            for action in &plan.scene.actions {
                if cancel.load(Ordering::Acquire) {
                    return Err("Scene canceled.".into());
                }
                progress(&action.label());
                backend.launch(action, plan.policy.wait_seconds)?;
                details.push(action_done(action));
            }
            Ok(())
        }))
        .unwrap_or_else(|_| Err("A scene step stopped unexpectedly.".into()));
    match result {
        Ok(()) => Report {
            success: true,
            message: format!("“{}” applied.", plan.scene.name),
            details,
        },
        Err(e) => {
            details.push(e);
            let message = if plan.policy.keep_completed {
                "Scene stopped. Completed changes were kept."
            } else {
                progress("Restoring previous settings…");
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    backend.restore(&snapshot)
                }))
                .unwrap_or_else(|_| Err("Restoration stopped unexpectedly.".into()))
                {
                    Ok(()) => "Scene stopped. Previous Windows settings restored.",
                    Err(e) => {
                        details.push(e);
                        "Scene stopped. Some Windows settings could not be restored."
                    }
                }
            };
            if plan.tv.is_some() || !plan.scene.actions.is_empty() || !plan.scene.before.is_empty()
            {
                details
                    .push("TV power/input changes and dispatched apps/URIs are not undone.".into())
            }
            Report {
                success: false,
                message: message.into(),
                details,
            }
        }
    }
}
fn action_done(action: &Action) -> String {
    match action {
        Action::App { wait: true, .. } => format!("{} finished.", action.label()),
        Action::Close { .. } => format!("{}: done.", action.label()),
        _ => format!("{} dispatched.", action.label()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Scene {
        Scene {
            id: "s".into(),
            name: "TV gaming".into(),
            display: None,
            audio: None,
            tv_audio: None,
            tv: None,
            fps: None,
            actions: vec![Action::Uri {
                target: "steam://open/bigpicture".into(),
            }],
            ..Default::default()
        }
    }
    #[derive(Default)]
    struct Fake {
        calls: Vec<&'static str>,
        fail: bool,
        fail_run: bool,
    }
    impl Backend for Fake {
        type Snapshot = ();
        fn snapshot(&mut self, _: &Plan) -> Result<(), String> {
            self.calls.push("snapshot");
            Ok(())
        }
        fn tv(&mut self, _: &Plan, _: &AtomicBool, _: &mut dyn FnMut(&str)) -> Result<(), String> {
            self.calls.push("tv");
            Ok(())
        }
        fn display(&mut self, _: &Plan) -> Result<(), String> {
            self.calls.push("display");
            Ok(())
        }
        fn audio(
            &mut self,
            _: &Plan,
            _: &AtomicBool,
            _: &mut dyn FnMut(&str),
        ) -> Result<(), String> {
            self.calls.push("audio");
            if self.fail {
                Err("missing HDMI".into())
            } else {
                Ok(())
            }
        }
        fn fps(&mut self, _: &Plan) -> Result<(), String> {
            self.calls.push("fps");
            if self.fail {
                Err("FPS write failed".into())
            } else {
                Ok(())
            }
        }
        fn launch(&mut self, action: &Action, _: u16) -> Result<(), String> {
            match action {
                Action::App { wait: true, .. } => {
                    self.calls.push("run");
                    if self.fail_run {
                        return Err("exited with code 1".into());
                    }
                }
                Action::Close { .. } => self.calls.push("close"),
                _ => self.calls.push("launch"),
            }
            Ok(())
        }
        fn restore(&mut self, _: &()) -> Result<(), String> {
            self.calls.push("restore");
            Ok(())
        }
    }

    #[test]
    fn fps_runs_before_launch_and_failure_blocks_launch_with_recovery() {
        let mut s = scene();
        s.fps = Some(crate::fps::Request {
            provider: crate::fps::Provider::Nvidia,
            limit: Some(120),
        });
        let p = resolve(&s, &[], &[], &[]).unwrap();
        let mut b = Fake {
            fail: false,
            ..Default::default()
        };
        assert!(apply(&mut b, &p, &AtomicBool::new(false), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "fps", "launch"]);
        b.calls.clear();
        b.fail = true;
        assert!(!apply(&mut b, &p, &AtomicBool::new(false), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "fps", "restore"]);
        let mut p = p;
        p.policy.keep_completed = true;
        b.calls.clear();
        assert!(!apply(&mut b, &p, &AtomicBool::new(false), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "fps"]);
        let mut legacy = serde_json::to_value(scene()).unwrap();
        legacy.as_object_mut().unwrap().remove("fps");
        let legacy: Scene = serde_json::from_value(legacy).unwrap();
        assert!(legacy.fps.is_none());
        let mut only = s;
        only.actions.clear();
        assert!(only.validate().is_ok());
    }
    #[test]
    fn storage_conflicts_and_corruption_preserve_data() {
        let path =
            std::env::temp_dir().join(format!("director-scenes-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let s = scene();
        save(&path, &[], std::slice::from_ref(&s)).unwrap();
        assert_eq!(load(&path).unwrap(), [s]);
        let before = fs::read(&path).unwrap();
        assert!(save(&path, &[], &[]).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&path, b"broken").unwrap();
        assert!(save(&path, &[], &[]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        let _ = fs::remove_file(path);
    }
    #[test]
    fn missing_references_fail_before_actions() {
        let mut s = scene();
        s.display = Some("gone".into());
        assert!(resolve(&s, &[], &[], &[]).is_err());
    }
    #[test]
    fn validation_checks_actions_and_empty_scene() {
        let mut s = scene();
        assert!(s.validate().is_ok());
        s.actions.clear();
        assert!(s.validate().is_err());
        s.actions.push(Action::Uri {
            target: "not a uri".into(),
        });
        assert!(s.validate().is_err());
    }
    #[test]
    fn actions_only_and_cancel_restore() {
        let p = resolve(&scene(), &[], &[], &[]).unwrap();
        let mut b = Fake {
            fail: false,
            ..Default::default()
        };
        assert!(apply(&mut b, &p, &AtomicBool::new(false), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "launch"]);
        b.calls.clear();
        assert!(!apply(&mut b, &p, &AtomicBool::new(true), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "restore"]);
    }
    #[test]
    fn stage_order_stops_before_launch_and_recovers() {
        let mut p = resolve(&scene(), &[], &[], &[]).unwrap();
        p.audio = Some(crate::presets::Preset {
            id: "a".into(),
            name: "a".into(),
            output_id: "tv".into(),
            output_name: "TV".into(),
            volume_percent: None,
            keep_changes: false,
            communications_output: None,
            input: None,
        });
        p.display = Some(crate::displays::Preset {
            id: "d".into(),
            name: "d".into(),
            layout: crate::display_apply::tests::layout(0),
            confirm_changes: false,
            captured_at_unix_seconds: 0,
        });
        let mut b = Fake {
            fail: true,
            ..Default::default()
        };
        assert!(!apply(&mut b, &p, &AtomicBool::new(false), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "display", "audio", "restore"]);
        b.calls.clear();
        p.policy.keep_completed = true;
        assert!(!apply(&mut b, &p, &AtomicBool::new(false), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "display", "audio"]);
    }
    fn linked_tv() -> crate::tvs::Tv {
        serde_json::from_value(serde_json::json!({"id":"tv","name":"C1","address":"192.0.2.50","display":null,"credential":[1],"audio":{"id":"hdmi","name":"LG HDMI"}})).unwrap()
    }
    #[test]
    fn tv_link_resolves_without_preparation_or_duplicated_preset() {
        let mut s = scene();
        s.actions.clear();
        s.tv_audio = Some("tv".into());
        let t = linked_tv();
        let p = resolve(&s, &[], &[], std::slice::from_ref(&t)).unwrap();
        assert!(p.tv.is_none());
        let audio = p.audio.unwrap();
        assert_eq!(audio.output_id, "hdmi");
        assert!(
            audio.volume_percent.is_none()
                && audio.input.is_none()
                && audio.communications_output.is_none()
        );
        let mut missing = t;
        missing.audio = None;
        assert!(resolve(&s, &[], &[], &[missing]).is_err());
    }
    #[test]
    fn full_stage_order_and_cancel_after_display() {
        let mut s = scene();
        s.tv = Some(TvPreparation {
            id: "tv".into(),
            wake: false,
            input: None,
        });
        s.tv_audio = Some("tv".into());
        let mut p = resolve(&s, &[], &[], &[linked_tv()]).unwrap();
        p.display = Some(crate::displays::Preset {
            id: "d".into(),
            name: "d".into(),
            layout: crate::display_apply::tests::layout(0),
            confirm_changes: false,
            captured_at_unix_seconds: 0,
        });
        let mut b = Fake {
            fail: false,
            ..Default::default()
        };
        assert!(apply(&mut b, &p, &AtomicBool::new(false), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "tv", "display", "audio", "launch"]);
        let cancel = AtomicBool::new(false);
        b.calls.clear();
        assert!(
            !apply(&mut b, &p, &cancel, |step| {
                if step == "Applying displays…" {
                    cancel.store(true, Ordering::Release)
                }
            })
            .success
        );
        assert_eq!(b.calls, ["snapshot", "tv", "display", "restore"]);
    }
    #[test]
    fn duplicate_names_and_audio_sources_are_rejected() {
        let path =
            std::env::temp_dir().join(format!("director-scene-dupe-{}.json", std::process::id()));
        let mut second = scene();
        second.id = "second".into();
        assert!(save(&path, &[], &[scene(), second]).is_err());
        let mut s = scene();
        s.audio = Some("preset".into());
        s.tv_audio = Some("tv".into());
        assert!(s.validate().is_err());
    }
    fn run(path: &str) -> Action {
        Action::App {
            path: path.into(),
            args: vec![],
            working_directory: None,
            wait: true,
        }
    }
    #[test]
    fn before_runs_first_and_a_failed_wait_stops_and_restores() {
        let mut s = scene();
        s.before = vec![run("C:/setup.exe")];
        s.fps = Some(crate::fps::Request {
            provider: crate::fps::Provider::Nvidia,
            limit: Some(120),
        });
        let p = resolve(&s, &[], &[], &[]).unwrap();
        let mut b = Fake::default();
        assert!(apply(&mut b, &p, &AtomicBool::new(false), |_| {}).success);
        assert_eq!(b.calls, ["snapshot", "run", "fps", "launch"]);
        b.calls.clear();
        b.fail_run = true;
        let r = apply(&mut b, &p, &AtomicBool::new(false), |_| {});
        assert!(!r.success);
        assert_eq!(b.calls, ["snapshot", "run", "restore"]);
    }
    #[test]
    fn version_one_files_load_and_save_as_version_two_without_policy_fields() {
        let path =
            std::env::temp_dir().join(format!("director-scenes-v1-{}.json", std::process::id()));
        let v1 = serde_json::json!({"version":1,"scenes":[{"id":"s","name":"Desk","display":null,"audio":null,"tv":null,"actions":[{"kind":"uri","target":"steam://close/bigpicture"}],"keep_on_failure":true,"wait_seconds":45}]});
        fs::write(&path, serde_json::to_vec(&v1).unwrap()).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded[0].legacy_keep_on_failure, None);
        save(&path, &loaded, &loaded).unwrap();
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["version"], 2);
        let scene = &written["scenes"][0];
        assert!(scene.get("keep_on_failure").is_none() && scene.get("wait_seconds").is_none());
        assert_eq!(load(&path).unwrap(), loaded);
        let _ = fs::remove_file(path);
    }
    #[test]
    fn close_actions_need_an_executable() {
        let bad = Action::Close {
            target: "steam".into(),
            force: false,
        };
        assert!(bad.validate().is_err());
    }
}
