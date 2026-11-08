//! Scenes: display preset references, inline sound/TV/FPS steps and actions, applied as one
//! ordered transaction.
use crate::audio::DeviceSelection;
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
    /// Output, volume, mute and microphone, held in the scene itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<Sound>,
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
    /// The ID of the scene that End (or running this active scene again) applies after the
    /// leave steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_to: Option<String>,
}
/// Resolve before any leave actions. A missing destination must keep the scene active.
pub fn return_destination<'a>(
    scene: &Scene,
    catalog: &'a [Scene],
) -> Result<Option<&'a Scene>, String> {
    let Some(id) = &scene.return_to else {
        return Ok(None);
    };
    if *id == scene.id {
        return Err("A scene cannot return to itself. Edit its Return step.".into());
    }
    catalog
        .iter()
        .find(|s| s.id == *id)
        .map(Some)
        .ok_or_else(|| "The return scene was removed. Edit this scene's Return step.".into())
}

/// Running a different scene always chooses it directly, even if the active scene has Return.
pub fn apply_destination<'a>(
    requested: &'a Scene,
    active: Option<&str>,
    all: &'a [Scene],
) -> Result<&'a Scene, String> {
    if active == Some(requested.id.as_str()) {
        return Ok(return_destination(requested, all)?.unwrap_or(requested));
    }
    Ok(requested)
}

/// A scene's Sound step. Anything left as None stays as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sound {
    /// The default output; None keeps the current one, and volume and mute apply to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<SoundOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
    /// A separate communications (calls) output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calls: Option<DeviceSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mic: Option<Mic>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SoundOutput {
    Device {
        id: String,
        name: String,
    },
    /// The audio output linked to a saved TV.
    Tv {
        id: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mic {
    pub device: DeviceSelection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
}
impl Sound {
    /// Whether applying this step would change anything.
    pub fn changes_something(&self) -> bool {
        self.output.is_some()
            || self.volume.is_some()
            || self.mute.is_some()
            || self.calls.is_some()
            || self.mic.is_some()
    }
    fn validate(&self) -> Result<(), String> {
        let bad_id = |id: &str| id.is_empty() || id.len() > 512;
        let bad_name = |name: &str| name.len() > 256;
        if self.volume.is_some_and(|v| v > 100)
            || self
                .mic
                .as_ref()
                .is_some_and(|m| m.volume.is_some_and(|v| v > 100))
        {
            return Err("Sound volumes must be from 0 to 100.".into());
        }
        if match &self.output {
            Some(SoundOutput::Device { id, name }) => bad_id(id) || bad_name(name),
            Some(SoundOutput::Tv { id }) => bad_id(id),
            None => false,
        } || self
            .calls
            .as_ref()
            .is_some_and(|d| bad_id(&d.id) || bad_name(&d.name))
            || self
                .mic
                .as_ref()
                .is_some_and(|m| bad_id(&m.device.id) || bad_name(&m.device.name))
        {
            return Err("Choose valid sound devices.".into());
        }
        Ok(())
    }
}
/// How scenes behave when a step fails or waits for devices. Global, from Preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenePolicy {
    pub keep_completed: bool,
    pub wait_seconds: u16,
    /// How long a TV that reports its input is held on the scene's input (0 = off).
    pub input_guard_seconds: u16,
}
impl Default for ScenePolicy {
    fn default() -> Self {
        Self {
            keep_completed: false,
            wait_seconds: 30,
            input_guard_seconds: crate::tv_input_guard::DEFAULT_SECONDS,
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
    /// Turn this TV off when the scene is left (another scene applied, or End).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub off_when_leaving: bool,
}
impl Scene {
    /// Whether leaving this scene does anything (leave actions or turning the TV off).
    pub fn has_leave_work(&self) -> bool {
        self.return_to.is_some()
            || !self.leave.is_empty()
            || self.tv.as_ref().is_some_and(|t| t.off_when_leaving)
    }
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
            Self::App { path, wait, .. } => {
                let name = file_name(path);
                format!(
                    "{} {}",
                    if *wait { "Run" } else { "Launch" },
                    if name.is_empty() { "app".into() } else { name }
                )
            }
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
            || self
                .display
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 200)
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
        if let Some(id) = &self.return_to
            && (id.is_empty() || id.len() > 200 || id == &self.id)
        {
            return Err("Choose another scene to return to.".into());
        }
        if let Some(sound) = &self.sound {
            sound.validate()?;
        }
        if self.tv.is_none()
            && self.display.is_none()
            && self.sound.is_none()
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
}
#[derive(Clone)]
pub struct Plan {
    pub scene: Scene,
    pub display: Option<crate::displays::Preset>,
    /// The resolved Sound step, with a TV's sound turned into its linked device.
    pub sound: Option<crate::audio::Request>,
    pub tv: Option<crate::tvs::Tv>,
    pub policy: ScenePolicy,
}
pub fn resolve(
    scene: &Scene,
    displays: &[crate::displays::Preset],
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
    let sound = scene
        .sound
        .as_ref()
        .map(|s| sound_request(s, tvs))
        .transpose()?;
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
        sound,
        tv,
        policy: ScenePolicy::default(),
    })
}
/// The audio request for a Sound step. A TV's sound becomes the device linked to that TV.
pub fn sound_request(
    sound: &Sound,
    tvs: &[crate::tvs::Tv],
) -> Result<crate::audio::Request, String> {
    if !sound.changes_something() {
        return Err("The scene's Sound step changes nothing. Edit the scene.".into());
    }
    let output_id = match &sound.output {
        Some(SoundOutput::Device { id, .. }) => Some(id.clone()),
        Some(SoundOutput::Tv { id }) => {
            let tv = tvs
                .iter()
                .find(|t| &t.id == id)
                .ok_or("The TV used for sound is missing. Edit the scene.")?;
            let link = tv
                .audio
                .as_ref()
                .ok_or("This TV has no linked audio output. Edit its TV connection first.")?;
            Some(link.id.clone())
        }
        None => None,
    };
    Ok(crate::audio::Request {
        output_id,
        volume_percent: sound.volume,
        mute: sound.mute,
        communications_output_id: sound.calls.as_ref().map(|d| d.id.clone()),
        input: sound.mic.as_ref().map(|m| crate::audio::InputRequest {
            device_id: m.device.id.clone(),
            volume_percent: m.volume,
            mute: m.mute,
        }),
    })
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    scenes: Vec<Scene>,
}
pub fn path() -> Result<PathBuf, String> {
    crate::storage::settings_path("scenes.json")
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
    let doc: Document = serde_json::from_slice(&data)
        .map_err(|_| "Could not read scenes. The file was left unchanged.")?;
    if doc.version != VERSION {
        return Err(
            "These scenes require a newer Director version. The file was left unchanged.".into(),
        );
    }
    validate(&doc.scenes)?;
    Ok(doc.scenes)
}
pub fn save(path: &Path, expected: &[Scene], next: &[Scene]) -> Result<(), String> {
    validate(next)?;
    crate::cli_name::check_new_names(expected, next, |s| (&s.id, &s.name), "scene")?;
    if load(path)? != expected {
        return Err("Scenes changed in another window. Reopen Scenes before saving.".into());
    }
    crate::storage::write_document(
        path,
        &Document {
            version: VERSION,
            scenes: next.to_vec(),
        },
    )
}
/// The scenes file format. Other versions are refused and left unchanged.
const VERSION: u32 = 1;
pub fn references(kind: &str, id: &str) -> Result<Vec<String>, String> {
    Ok(load(&path()?)?
        .into_iter()
        .filter(|s| match kind {
            "display" => s.display.as_deref() == Some(id),
            "tv" => s.tv.as_ref().is_some_and(|t| t.id == id)
                || s.sound.as_ref().is_some_and(
                    |sound| matches!(&sound.output, Some(SoundOutput::Tv { id: tv }) if tv == id),
                ),
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
    /// Turn a TV off, for scenes that do so when they're left.
    fn tv_off(&mut self, tv: &crate::tvs::Tv) -> Result<(), String>;
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
                (plan.sound.is_some(), 2),
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
                    "Waiting for sound devices…",
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
                    ["TV preparation", "Displays", "Sound", "FPS limit"][stage]
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
/// Run a scene's When leaving actions. A failing action never blocks the next scene, so
/// every action runs and the failures come back as warnings.
pub fn leave(
    backend: &mut impl Backend,
    scene: &Scene,
    tvs: &[crate::tvs::Tv],
    policy: ScenePolicy,
    mut progress: impl FnMut(&str),
) -> Vec<String> {
    let _operation = crate::diagnostics::Operation::start("scene.leave", Some(&scene.id));
    let mut warnings = vec![];
    for action in &scene.leave {
        progress(&format!("Leaving “{}”: {}", scene.name, action.label()));
        if let Err(e) = backend.launch(action, policy.wait_seconds) {
            crate::diagnostics::event("scene.leave", "error", &e);
            warnings.push(format!(
                "Leaving “{}”: {} failed. {e}",
                scene.name,
                action.label()
            ));
        }
    }
    // Last, so apps closed above while the TV was still on.
    if let Some(prep) = scene.tv.as_ref().filter(|t| t.off_when_leaving) {
        match tvs.iter().find(|t| t.id == prep.id) {
            Some(tv) => {
                progress(&format!(
                    "Leaving “{}”: Turning off {}",
                    scene.name, tv.name
                ));
                if let Err(e) = backend.tv_off(tv) {
                    crate::diagnostics::event("scene.leave", "error", &e);
                    warnings.push(format!(
                        "Leaving “{}”: couldn’t turn off {}. {e}",
                        scene.name, tv.name
                    ));
                }
            }
            None => warnings.push(format!(
                "Leaving “{}”: its TV was removed, so it wasn’t turned off.",
                scene.name
            )),
        }
    }
    warnings
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Scene {
        Scene {
            id: "s".into(),
            name: "TV gaming".into(),
            display: None,
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
        fn tv_off(&mut self, _: &crate::tvs::Tv) -> Result<(), String> {
            self.calls.push("tv-off");
            if self.fail_run {
                Err("TV didn't answer".into())
            } else {
                Ok(())
            }
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
            vrr_safe: false,
        });
        let p = resolve(&s, &[], &[]).unwrap();
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
    fn return_targets_resolve_by_id_and_fail_closed() {
        let mut game = scene();
        let desk = Scene::new("desk", "Renamed Desk");
        let catalog = [desk];
        assert!(return_destination(&game, &catalog).unwrap().is_none());
        assert_eq!(
            apply_destination(&game, Some(&game.id), &catalog).unwrap(),
            &game
        );
        game.return_to = Some("desk".into());
        assert!(game.has_leave_work());
        assert_eq!(
            return_destination(&game, &catalog).unwrap(),
            Some(&catalog[0])
        );
        // Running the active scene again returns; running another scene goes straight there.
        assert_eq!(
            apply_destination(&game, Some(&game.id), &catalog).unwrap(),
            &catalog[0]
        );
        assert_eq!(
            apply_destination(&game, Some("desk"), &catalog).unwrap(),
            &game
        );
        assert!(return_destination(&game, &[]).is_err());
        assert!(apply_destination(&game, Some(&game.id), &[]).is_err());
        game.return_to = Some(game.id.clone());
        assert!(game.validate().is_err());
        assert!(return_destination(&game, &catalog).is_err());
        let json = serde_json::to_value(Scene {
            return_to: Some("desk".into()),
            ..Scene::new("game", "Game")
        })
        .unwrap();
        assert_eq!(json["return_to"], "desk");
    }

    #[test]
    fn missing_references_fail_before_actions() {
        let mut s = scene();
        s.display = Some("gone".into());
        assert!(resolve(&s, &[], &[]).is_err());
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
        let p = resolve(&scene(), &[], &[]).unwrap();
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
        let mut p = resolve(&scene(), &[], &[]).unwrap();
        p.sound = Some(crate::audio::Request {
            output_id: Some("tv".into()),
            volume_percent: None,
            mute: None,
            communications_output_id: None,
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
        serde_json::from_value(serde_json::json!({"id":"tv","name":"C1","brand":"lg","connection":"encrypted","address":"192.0.2.50","display":null,"credential":[1],"audio":{"id":"hdmi","name":"LG HDMI"}})).unwrap()
    }
    fn tv_sound(volume: Option<u8>) -> Sound {
        Sound {
            output: Some(SoundOutput::Tv { id: "tv".into() }),
            volume,
            ..Default::default()
        }
    }
    #[test]
    fn tv_link_resolves_without_preparation_or_duplicated_preset() {
        let mut s = scene();
        s.actions.clear();
        s.sound = Some(tv_sound(None));
        let t = linked_tv();
        let p = resolve(&s, &[], std::slice::from_ref(&t)).unwrap();
        assert!(p.tv.is_none());
        let audio = p.sound.unwrap();
        assert_eq!(audio.output_id.as_deref(), Some("hdmi"));
        assert!(
            audio.volume_percent.is_none()
                && audio.input.is_none()
                && audio.communications_output_id.is_none()
        );
        let mut missing = t;
        missing.audio = None;
        assert!(resolve(&s, &[], &[missing]).is_err());
    }
    #[test]
    fn full_stage_order_and_cancel_after_display() {
        let mut s = scene();
        s.tv = Some(TvPreparation {
            off_when_leaving: false,
            id: "tv".into(),
            wake: false,
            input: None,
        });
        s.sound = Some(tv_sound(None));
        let mut p = resolve(&s, &[], &[linked_tv()]).unwrap();
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
        s.sound = Some(Sound {
            volume: Some(101),
            ..Default::default()
        });
        assert!(s.validate().is_err());
        s.sound = Some(Sound::default());
        assert!(s.validate().is_ok());
        assert!(
            resolve(&s, &[], &[])
                .err()
                .unwrap()
                .contains("changes nothing")
        );
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
            vrr_safe: false,
        });
        let p = resolve(&s, &[], &[]).unwrap();
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
    fn leave_runs_every_action_and_reports_failures_as_warnings() {
        let mut s = scene();
        s.leave = vec![
            run("C:/cleanup.exe"),
            Action::Close {
                target: "steam.exe".into(),
                force: false,
            },
        ];
        assert!(s.validate().is_ok());
        let mut b = Fake {
            fail_run: true,
            ..Default::default()
        };
        let warnings = leave(&mut b, &s, &[], ScenePolicy::default(), |_| {});
        assert_eq!(b.calls, ["run", "close"]);
        assert_eq!(warnings.len(), 1);
    }
    #[test]
    fn files_save_as_version_one_and_other_versions_are_refused() {
        let path =
            std::env::temp_dir().join(format!("director-scenes-version-{}.json", std::process::id()));
        let scenes = vec![scene()];
        save(&path, &[], &scenes).unwrap();
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["version"], 1);
        assert_eq!(load(&path).unwrap(), scenes);
        let mut other = written.clone();
        other["version"] = 2.into();
        fs::write(&path, serde_json::to_vec(&other).unwrap()).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), serde_json::to_vec(&other).unwrap());
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
    #[test]
    fn linked_tv_audio_volume_reaches_the_audio_request() {
        let mut s = scene();
        s.sound = Some(Sound {
            mute: Some(false),
            ..tv_sound(Some(35))
        });
        let plan = resolve(&s, &[], &[linked_tv()]).unwrap();
        let request = plan.sound.unwrap();
        assert_eq!(request.volume_percent, Some(35));
        assert_eq!(request.mute, Some(false));
    }
    #[test]
    fn leaving_turns_the_tv_off_after_the_leave_actions() {
        let tv = serde_json::from_value::<crate::tvs::Tv>(serde_json::json!({"id":"tv","name":"C1","brand":"lg","connection":"encrypted","address":"192.0.2.50","display":null,"credential":[1]})).unwrap();
        let mut s = scene();
        s.tv = Some(TvPreparation {
            id: "tv".into(),
            wake: true,
            input: None,
            off_when_leaving: true,
        });
        s.leave = vec![Action::Close {
            target: "steam.exe".into(),
            force: false,
        }];
        assert!(s.has_leave_work());
        let mut b = Fake::default();
        assert!(
            leave(
                &mut b,
                &s,
                std::slice::from_ref(&tv),
                ScenePolicy::default(),
                |_| {}
            )
            .is_empty()
        );
        assert_eq!(b.calls, ["close", "tv-off"]);
        b.calls.clear();
        b.fail_run = true;
        assert_eq!(
            leave(&mut b, &s, &[tv], ScenePolicy::default(), |_| {}).len(),
            1
        );
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["tv"]["off_when_leaving"], true);
        s.tv.as_mut().unwrap().off_when_leaving = false;
        s.leave.clear();
        assert!(!s.has_leave_work());
    }
}
