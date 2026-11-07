//! Native scene cards and editor; device work stays on the shared worker runner.
use crate::{DirectorApp, Message, work};
use director::{
    audio::{DeviceSelection, Output},
    scenes::{self, Action, ActionList, Scene, Sound, SoundOutput, TvPreparation},
    windows_audio::OperationLock,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use windows_reactor::*;
/// Scenes, display presets, TVs, and the audio outputs and inputs the Sound step can pick.
type Catalog = (
    Vec<Scene>,
    Vec<director::displays::Preset>,
    Vec<director::tvs::Tv>,
    Vec<Output>,
    Vec<Output>,
);
/// An edit in the Sound step dialog.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SoundEdit {
    /// 0 leaves the output as is, then the devices, then each TV's sound.
    Output(Option<usize>),
    SetVolume(bool),
    Volume(f64),
    /// 0 leaves mute as is, 1 unmutes, 2 mutes.
    Mute(Option<usize>),
    Calls(bool),
    CallsDevice(Option<usize>),
    Mic(bool),
    MicDevice(Option<usize>),
    SetMicVolume(bool),
    MicVolume(f64),
    MicMute(Option<usize>),
}
/// Device choices for a Sound step ComboBox: what Windows lists (including remembered offline
/// devices), plus the saved device when Windows no longer knows it.
pub(crate) fn device_choices(
    devices: &[Output],
    saved: Option<&DeviceSelection>,
) -> Vec<DeviceSelection> {
    let mut list = devices
        .iter()
        .map(|d| DeviceSelection {
            id: d.id.clone(),
            name: d.name.clone(),
        })
        .collect::<Vec<_>>();
    if let Some(saved) = saved
        && !list.iter().any(|d| d.id == saved.id)
    {
        list.push(saved.clone());
    }
    list
}
/// The device a Sound step's output names, if it names one.
pub(crate) fn sound_device(sound: &Sound) -> Option<DeviceSelection> {
    match &sound.output {
        Some(SoundOutput::Device { id, name }) => Some(DeviceSelection {
            id: id.clone(),
            name: name.clone(),
        }),
        _ => None,
    }
}
pub(crate) fn mute_choice(mute: Option<bool>) -> usize {
    match mute {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    }
}
fn mute_from(index: usize) -> Option<bool> {
    match index {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}
#[derive(Clone)]
pub(crate) enum SceneMessage {
    Load,
    Loaded(Box<Result<Catalog, String>>),
    Add,
    Edit(String),
    Cancel,
    Name(String),
    Display(Option<usize>),
    Sound(SoundEdit),
    Tv(Option<usize>),
    Wake(bool),
    ReturnEnabled(bool),
    ReturnDestination(Option<usize>),
    /// Turn the scene's TV off when the scene is left.
    TvOffWhenLeaving(bool),
    Input(Option<usize>),
    AddUri(ActionList),
    AddApp(ActionList),
    AddClose(ActionList),
    Browse(ActionList, usize, bool),
    ActionText(ActionList, usize, u8, String),
    /// Wait for an app, or force-close for a Close action.
    ActionToggle(ActionList, usize, bool),
    Move(ActionList, usize, bool),
    Remove(ActionList, usize),
    /// Open or close an app action's Advanced options.
    ActionAdvanced(ActionList, usize, bool),
    /// Open a step's editor dialog (`None` closes it, keeping the changes).
    Open(Option<crate::scene_editor_ui::Step>),
    /// The step dialog closed: Done keeps the changes, anything else restores them.
    StepClosed(ContentDialogResult),
    /// Reset a step to "leave unchanged".
    ClearStep(crate::scene_editor_ui::Step),
    End,
    Save,
    Saved(Result<Vec<Scene>, String>),
    Delete(String),
    Deleted(ContentDialogResult),
    Apply(String),
    Stop,
    Progress(u64, Option<Event>),
    Confirm(ContentDialogResult),
    Finished(u64, Result<scenes::Report, String>),
    /// The first-run card that saves the current setup as a scene.
    Starter(crate::starter_ui::StarterMessage),
}
#[derive(Clone)]
pub(crate) enum Event {
    Step(String),
    Confirm(u64),
}
#[derive(Default)]
pub(crate) struct SceneState {
    pub(crate) scenes: Vec<Scene>,
    pub(crate) displays: Vec<director::displays::Preset>,
    pub(crate) tvs: Vec<director::tvs::Tv>,
    pub(crate) draft: Option<Scene>,
    pub(crate) ready: bool,
    delete: Option<String>,
    token: u64,
    cancel: Option<Arc<AtomicBool>>,
    events: Option<Arc<Mutex<mpsc::Receiver<Event>>>>,
    confirm: Option<mpsc::Sender<director::display_apply::Decision>>,
    remaining: Option<u64>,
    /// The step whose editor dialog is open.
    pub(crate) open: Option<crate::scene_editor_ui::Step>,
    /// The draft as it was when the step dialog opened, for Cancel.
    pub(crate) snapshot: Option<Scene>,
    /// The scene applied last during this Windows session.
    pub(crate) active: Option<String>,
    /// App actions whose Advanced options section is open.
    pub(crate) advanced: Vec<(ActionList, usize)>,
    /// The first-run card: what it read and what's ticked.
    pub(crate) starter: crate::starter_ui::StarterState,
}
fn msg(m: SceneMessage) -> Message {
    Message::Scene(m)
}
use crate::ui::note;
/// The height a scene card needs: its header, the 124 DIP hero panel and its text lines.
/// Lines are estimated from their length, since a long line wraps in the 312 DIP column.
fn scene_card_height(summary: &str, error: Option<String>) -> f64 {
    const LINE: f64 = 20.0;
    // About this many characters fit on a line of a card.
    const CHARS: usize = 42;
    let lines = |text: &str| -> f64 {
        text.lines()
            .map(|l| l.chars().count().div_ceil(CHARS).max(1))
            .sum::<usize>() as f64
    };
    // Padding, header row and spacing, then the hero panel and the gap under it.
    let mut height = 28.0 + 32.0 + 12.0 + 124.0 + 8.0 + lines(summary) * LINE;
    if let Some(e) = error {
        height += lines(&e) * LINE + 8.0;
    }
    // A little room so a misjudged wrap doesn't clip the last line.
    height + 12.0
}
/// What a hero panel's bar shows.
enum Level {
    None,
    Percent(u8),
    Muted,
}
/// The large panel at the top of a scene card that has no screens to preview: a big icon,
/// the main value and an optional volume bar.
fn hero_panel(glyph: &str, title: String, level: Level, lines: Vec<String>) -> View {
    const BAR: f64 = 150.0;
    let bar: View = match level {
        Level::None => View::empty(),
        Level::Muted => TextBlock::new().text("Muted").opacity(0.72).into(),
        Level::Percent(v) => StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .children((
                Grid::new()
                    .width(BAR)
                    .height(4.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        Border::new()
                            .corner_radius(2.0)
                            .background(Color::argb(60, 128, 128, 128)),
                        Border::new()
                            .corner_radius(2.0)
                            .horizontal_alignment(HorizontalAlignment::Left)
                            .width(BAR * f64::from(v.min(100)) / 100.0)
                            .background(ThemeBrush::Accent),
                    )),
                TextBlock::new().text(format!("{v}%")).opacity(0.72),
            )),
    };
    let lines = lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            KeyedView::new(
                i.to_string(),
                TextBlock::new()
                    .text(line)
                    .font_size(crate::ui::CAPTION)
                    .opacity(0.72)
                    .text_trimming(TextTrimming::CharacterEllipsis),
            )
        })
        .collect::<Vec<_>>();
    Border::new()
        .height(124.0)
        .padding(Thickness::xy(16.0, 12.0))
        .corner_radius(crate::ui::RADIUS_S)
        .background(crate::ui::SUBTLE_FILL)
        .content(
            Grid::new()
                .columns([GridLength::Auto, GridLength::Star(1.0)])
                .column_spacing(16.0)
                .children((
                    Viewbox::new()
                        .width(36.0)
                        .height(36.0)
                        .opacity(crate::ui::SECONDARY)
                        .vertical_alignment(VerticalAlignment::Center)
                        .slot(ViewboxSlot::Child, FontIcon::new().glyph(glyph)),
                    StackPanel::new()
                        .grid_column(1)
                        .spacing(6.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            TextBlock::new()
                                .text(title)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .text_trimming(TextTrimming::CharacterEllipsis),
                            bar,
                            StackPanel::new().spacing(2.0).keyed_children(lines),
                        )),
                )),
        )
}
pub(crate) fn new_id() -> String {
    format!(
        "scene-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

impl DirectorApp {
    pub(crate) fn scene_ready(&self) -> bool {
        !self.busy && self.guard_ready && self.scene.ready
    }
    fn scene_save(&mut self, next: Vec<Scene>, cx: &ComponentContext<Self>) {
        #[cfg(test)]
        if crate::preset_cards_preview::ENABLED.load(Ordering::Acquire) {
            self.scene.scenes = next;
            if crate::fps_ui::PREVIEW.load(Ordering::Acquire) {
                std::fs::write(
                    ".localonly/fps-scenes-fixture.json",
                    serde_json::to_vec_pretty(&self.scene.scenes).unwrap(),
                )
                .unwrap();
            }
            self.scene.draft = None;
            self.scene.delete = None;
            self.status = "Scenes saved. Preview only; no settings changed.".into();
            self.announce(cx);
            return;
        }
        let expected = self.scene.scenes.clone();
        self.begin("Saving scenes…");
        work(cx, move || {
            msg(SceneMessage::Saved((|| {
                let _lock = OperationLock::acquire()?;
                let (_, displays, tvs) = director::windows_scenes::catalog()?;
                for scene in &next {
                    if !expected.iter().any(|old| old == scene) {
                        scenes::resolve(scene, &displays, &tvs)?;
                    }
                }
                scenes::save(&scenes::path()?, &expected, &next)?;
                Ok(next)
            })()))
        });
    }
    fn scene_poll(&self, token: u64, cx: &ComponentContext<Self>) {
        if let Some(events) = self.scene.events.clone() {
            work(cx, move || {
                msg(SceneMessage::Progress(
                    token,
                    events
                        .lock()
                        .ok()
                        .and_then(|r| r.recv_timeout(std::time::Duration::from_millis(300)).ok()),
                ))
            });
        }
    }
    pub(crate) fn update_scene(&mut self, m: SceneMessage, cx: &ComponentContext<Self>) {
        if matches!(&m, SceneMessage::Load) {
            self.update_fps(crate::fps_ui::FpsMessage::Discover, cx);
        }
        let ready = self.scene_ready();
        // The last item of each section's dropdown creates a new Library item in place.
        if ready && self.scene.draft.is_some() {
            use crate::library_flow::{LibraryMessage, Slot};
            let create = match &m {
                SceneMessage::Display(Some(i)) if *i == self.scene.displays.len() + 1 => {
                    Some(Slot::Display)
                }
                SceneMessage::Tv(Some(i)) if *i == self.scene.tvs.len() + 1 => Some(Slot::Tv),
                _ => None,
            };
            if let Some(slot) = create {
                self.update_library(LibraryMessage::FromScene(slot), cx);
                return;
            }
        }
        match m {
            SceneMessage::Load if !self.busy => {
                self.begin("Reading scenes and presets…");
                work(cx, || {
                    msg(SceneMessage::Loaded(Box::new(
                        director::windows_scenes::catalog().map(|(scenes, displays, tvs)| {
                            // Device lists only feed the Sound step's pickers; a failure
                            // leaves them empty rather than blocking Scenes.
                            let devices = |input| {
                                director::windows_audio::WindowsAudio::new()
                                    .and_then(|mut a| a.picker_devices(input))
                                    .unwrap_or_default()
                            };
                            (scenes, displays, tvs, devices(false), devices(true))
                        }),
                    )))
                });
            }
            SceneMessage::Loaded(result) => {
                self.end();
                match *result {
                    Ok((s, d, t, outputs, inputs)) => {
                        self.scene.scenes = s;
                        self.scene.displays = d;
                        self.scene.tvs = t;
                        self.outputs = outputs;
                        self.inputs = inputs;
                        self.scene.ready = true;
                        self.scene.active = director::scene_session::active();
                        self.status = "Ready.".into();
                        self.library_select_in_scene(cx);
                        self.library_resume(cx);
                        self.starter_loaded(cx);
                    }
                    Err(e) => {
                        self.scene.ready = false;
                        self.failure(e)
                    }
                }
            }
            SceneMessage::Starter(m) => self.update_starter(m, cx),
            SceneMessage::Add if ready => {
                self.scene.draft = Some(Scene::new(new_id(), ""));
                self.scene.open = None;
                self.status =
                    "Choose what this scene changes. Saving leaves your setup untouched.".into();
            }
            SceneMessage::Edit(id) if ready => {
                self.scene.draft = self.scene.scenes.iter().find(|s| s.id == id).cloned();
                self.scene.open = None;
                self.error = false;
                self.details.clear();
            }
            SceneMessage::Cancel if !self.busy => {
                self.scene.draft = None;
                self.status = "Ready.".into();
                self.error = false;
                self.details.clear();
            }
            SceneMessage::Name(s) if !self.busy => {
                if let Some(d) = &mut self.scene.draft {
                    d.name = s;
                }
            }
            SceneMessage::Display(index) if !self.busy && index.is_some() => {
                let selected = index
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| self.scene.displays.get(i))
                    .map(|p| p.id.clone());
                if let Some(d) = &mut self.scene.draft {
                    d.display = selected;
                }
            }
            SceneMessage::ReturnEnabled(enabled) if !self.busy => {
                if let Some(d) = &mut self.scene.draft {
                    // Turning Return on picks the first other scene; with none it stays off.
                    d.return_to = enabled
                        .then(|| {
                            d.return_to.clone().or_else(|| {
                                self.scene
                                    .scenes
                                    .iter()
                                    .find(|s| s.id != d.id)
                                    .map(|s| s.id.clone())
                            })
                        })
                        .flatten();
                }
            }
            SceneMessage::ReturnDestination(Some(index)) if !self.busy => {
                if let Some(d) = &mut self.scene.draft
                    && d.return_to.is_some()
                    && let Some(target) =
                        self.scene.scenes.iter().filter(|s| s.id != d.id).nth(index)
                {
                    d.return_to = Some(target.id.clone());
                }
            }
            SceneMessage::Sound(edit) if !self.busy => self.edit_sound(edit),
            SceneMessage::Tv(index) if !self.busy && index.is_some() => {
                let selected = index
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| self.scene.tvs.get(i))
                    .map(|t| TvPreparation {
                        off_when_leaving: false,
                        id: t.id.clone(),
                        wake: t.can_wake(),
                        input: t
                            .pc_input
                            .as_ref()
                            .filter(|pc| t.control_inputs().iter().any(|i| i.id == pc.id))
                            .map(|i| i.id.clone()),
                    });
                if let Some(d) = &mut self.scene.draft {
                    let off = d.tv.as_ref().is_some_and(|t| t.off_when_leaving);
                    d.tv = selected.map(|mut t| {
                        t.off_when_leaving = off;
                        t
                    });
                }
            }
            SceneMessage::Wake(v) if !self.busy => {
                if let Some(t) = self.scene.draft.as_mut().and_then(|d| d.tv.as_mut()) {
                    t.wake = v;
                }
            }
            SceneMessage::TvOffWhenLeaving(v) if !self.busy => {
                if let Some(t) = self.scene.draft.as_mut().and_then(|d| d.tv.as_mut()) {
                    t.off_when_leaving = v;
                }
            }
            SceneMessage::Input(index) if !self.busy && index.is_some() => {
                let selected = self
                    .scene
                    .draft
                    .as_ref()
                    .and_then(|d| d.tv.as_ref())
                    .and_then(|p| self.scene.tvs.iter().find(|t| t.id == p.id))
                    .and_then(|t| {
                        index
                            .and_then(|i| i.checked_sub(1))
                            .and_then(|i| t.control_inputs().get(i).cloned())
                    })
                    .map(|i| i.id.clone());
                if let Some(t) = self.scene.draft.as_mut().and_then(|d| d.tv.as_mut()) {
                    t.input = selected;
                }
            }
            SceneMessage::AddUri(list)
            | SceneMessage::AddApp(list)
            | SceneMessage::AddClose(list)
                if !self.busy =>
            {
                let action = match m {
                    SceneMessage::AddUri(_) => Action::Uri {
                        target: String::new(),
                    },
                    SceneMessage::AddClose(_) => Action::Close {
                        target: String::new(),
                        force: false,
                    },
                    _ => Action::App {
                        path: String::new(),
                        args: vec![],
                        working_directory: None,
                        // A setup step usually has to finish before devices change.
                        wait: list == ActionList::Before,
                    },
                };
                if let Some(d) = &mut self.scene.draft
                    && d.list(list).len() < 32
                {
                    d.list_mut(list).push(action);
                }
            }
            SceneMessage::Browse(list, i, folder) if !self.busy => match pick_path(folder) {
                Ok(Some(path)) => self.update_scene(
                    SceneMessage::ActionText(list, i, if folder { 2 } else { 0 }, path),
                    cx,
                ),
                Ok(None) => {}
                Err(e) => self.failure(e),
            },
            SceneMessage::ActionText(list, i, field, value) if !self.busy => {
                if let Some(a) = self
                    .scene
                    .draft
                    .as_mut()
                    .and_then(|d| d.list_mut(list).get_mut(i))
                {
                    match a {
                        Action::Uri { target } | Action::Close { target, .. } => *target = value,
                        Action::App {
                            path,
                            args,
                            working_directory,
                            ..
                        } => match field {
                            0 => *path = value,
                            1 => {
                                *args = if value.is_empty() {
                                    vec![]
                                } else {
                                    value
                                        .split('\n')
                                        .map(|s| s.trim_end_matches('\r').to_string())
                                        .collect()
                                }
                            }
                            _ => {
                                *working_directory =
                                    if value.is_empty() { None } else { Some(value) }
                            }
                        },
                    }
                }
            }
            SceneMessage::ActionToggle(list, i, on) if !self.busy => {
                if let Some(a) = self
                    .scene
                    .draft
                    .as_mut()
                    .and_then(|d| d.list_mut(list).get_mut(i))
                {
                    match a {
                        Action::App { wait, .. } => *wait = on,
                        Action::Close { force, .. } => *force = on,
                        Action::Uri { .. } => {}
                    }
                }
            }
            SceneMessage::Move(list, i, up) if !self.busy => {
                if let Some(d) = &mut self.scene.draft {
                    let actions = d.list_mut(list);
                    let other = if up {
                        i.checked_sub(1)
                    } else {
                        i.checked_add(1)
                    };
                    if let Some(j) = other
                        && i < actions.len()
                        && j < actions.len()
                    {
                        actions.swap(i, j);
                        self.scene.advanced.clear();
                    }
                }
            }
            SceneMessage::Remove(list, i) if !self.busy => {
                if let Some(d) = &mut self.scene.draft
                    && i < d.list(list).len()
                {
                    d.list_mut(list).remove(i);
                }
                // Positions shift, so forget which sections were open.
                self.scene.advanced.clear();
            }
            SceneMessage::ActionAdvanced(list, i, open) => {
                self.scene.advanced.retain(|k| *k != (list, i));
                if open {
                    self.scene.advanced.push((list, i));
                }
            }
            SceneMessage::Open(step) => {
                self.scene.snapshot = step.and(self.scene.draft.clone());
                self.scene.open = step;
            }
            SceneMessage::StepClosed(result) => {
                self.scene.open = None;
                if let Some(before) = self.scene.snapshot.take()
                    && result != ContentDialogResult::Primary
                {
                    self.scene.draft = Some(before);
                }
                // A Sound step left with nothing to change is the same as no Sound step.
                if let Some(d) = &mut self.scene.draft
                    && d.sound.as_ref().is_some_and(|s| !s.changes_something())
                {
                    d.sound = None;
                }
            }
            SceneMessage::ClearStep(step) if !self.busy => {
                use crate::scene_editor_ui::Step;
                if let Some(d) = &mut self.scene.draft {
                    match step {
                        Step::Before => d.before.clear(),
                        Step::Tv => d.tv = None,
                        Step::Displays => d.display = None,
                        Step::Sound => d.sound = None,
                        Step::Fps => d.fps = None,
                        Step::After => d.actions.clear(),
                        Step::Leave => d.leave.clear(),
                        Step::Return => d.return_to = None,
                    }
                }
                if self.scene.open == Some(step) {
                    self.scene.open = None;
                    self.scene.snapshot = None;
                }
            }
            SceneMessage::Save if ready => {
                if let Some(mut d) = self.scene.draft.clone() {
                    d.name = d.name.trim().into();
                    if let Err(e) = scenes::resolve(&d, &self.scene.displays, &self.scene.tvs) {
                        self.failure(e);
                        return;
                    }
                    let mut next = self.scene.scenes.clone();
                    if let Some(old) = next.iter_mut().find(|s| s.id == d.id) {
                        *old = d;
                    } else {
                        next.push(d);
                    }
                    next.sort_by_key(|s| s.name.to_lowercase());
                    self.scene_save(next, cx);
                }
            }
            SceneMessage::Saved(result) => {
                self.end();
                match result {
                    Ok(s) => {
                        self.scene.scenes = s;
                        self.scene.draft = None;
                        self.scene.delete = None;
                        self.error = false;
                        self.status = "Scenes saved. Your setup is unchanged.".into();
                        self.guard_saved(true, cx);
                        self.starter_probe_if_needed(cx);
                        self.details.clear();
                        self.announce(cx);
                    }
                    Err(e) => self.failure(e),
                }
            }
            SceneMessage::Delete(id) if ready => self.scene.delete = Some(id),
            SceneMessage::Deleted(result) if !self.busy => {
                if let Some(id) = self.scene.delete.take()
                    && result == ContentDialogResult::Primary
                {
                    let next = self
                        .scene
                        .scenes
                        .iter()
                        .filter(|s| s.id != id)
                        .cloned()
                        .collect();
                    self.scene_save(next, cx);
                }
            }
            SceneMessage::Apply(_) | SceneMessage::End if ready => {
                let requested = match m {
                    SceneMessage::Apply(id) => Some(id),
                    _ => None,
                };
                let id = requested
                    .clone()
                    .or_else(|| self.scene.active.clone())
                    .unwrap_or_default();
                self.scene.token = self.scene.token.wrapping_add(1);
                let token = self.scene.token;
                let name = self
                    .scene
                    .scenes
                    .iter()
                    .find(|scene| scene.id == id)
                    .map_or_else(|| id.clone(), |scene| scene.name.clone());
                let cancel = Arc::new(AtomicBool::new(false));
                self.scene.cancel = Some(cancel.clone());
                let (tx, rx) = mpsc::channel();
                let (ct, cr) = mpsc::channel();
                self.scene.confirm = Some(ct);
                self.scene.events = Some(Arc::new(Mutex::new(rx)));
                self.scene.remaining = None;
                self.begin("Starting scene…");
                work(cx, move || {
                    let result = (|| {
                        let _lock = OperationLock::acquire()?;
                        let confirmation_tx = tx.clone();
                        let confirm_cancel = cancel.clone();
                        let mut backend = director::windows_scenes::WindowsScene::new(
                            move |_: &director::displays::Preset| {
                                director::display_apply::wait_for_confirmation(
                                    &cr,
                                    std::time::Duration::from_secs(15),
                                    |seconds| {
                                        !confirm_cancel.load(Ordering::Acquire)
                                            && confirmation_tx.send(Event::Confirm(seconds)).is_ok()
                                    },
                                )
                            },
                        );
                        let progress = |step: &str| {
                            let _ = tx.send(Event::Step(step.into()));
                        };
                        match requested {
                            Some(id) => {
                                let plan =
                                    director::windows_scenes::load_run_plan(&id, &name, false)?;
                                Ok(director::windows_scenes::run(
                                    &mut backend,
                                    &plan,
                                    &cancel,
                                    false,
                                    progress,
                                ))
                            }
                            None => Ok(director::windows_scenes::end(
                                &mut backend,
                                &cancel,
                                false,
                                progress,
                            )),
                        }
                    })();
                    msg(SceneMessage::Finished(token, result))
                });
                self.scene_poll(token, cx);
            }
            SceneMessage::Stop => {
                if let Some(c) = &self.scene.cancel {
                    c.store(true, Ordering::Release);
                    self.status =
                        "Canceling scene… Waiting for the current request and recovery.".into();
                }
            }
            SceneMessage::Progress(token, event)
                if token == self.scene.token && self.scene.cancel.is_some() =>
            {
                match event {
                    Some(Event::Step(s)) => {
                        self.scene.remaining = None;
                        self.status = s;
                    }
                    Some(Event::Confirm(n)) => {
                        self.scene.remaining = Some(n);
                        self.status = format!("Keep display changes? Reverting in {n} seconds.");
                    }
                    None => {}
                }
                self.scene_poll(token, cx);
            }
            SceneMessage::Confirm(result) => {
                if let Some(tx) = &self.scene.confirm {
                    let _ = tx.send(if result == ContentDialogResult::Primary {
                        director::display_apply::Decision::Keep
                    } else {
                        director::display_apply::Decision::Revert
                    });
                }
                self.scene.remaining = None;
            }
            SceneMessage::Finished(token, result) if token == self.scene.token => {
                self.end();
                self.scene.events = None;
                self.scene.cancel = None;
                self.scene.confirm = None;
                self.scene.remaining = None;
                self.scene.active = director::scene_session::active();
                match result {
                    Ok(r) => {
                        self.error = !r.success;
                        self.status = r.message;
                        self.details = r.details.join("\n");
                        self.announce(cx);
                        director::diagnostics::event(
                            "scene.result",
                            if r.success { "ok" } else { "error" },
                            &format!("{} {}", self.status, self.details),
                        );
                    }
                    Err(e) => self.failure(e),
                }
            }
            _ => {}
        }
    }
    /// "End CONSOLE" next to +, only while the active scene has something to run when left.
    fn end_scene_button(&self, cx: &ViewContext<Self>) -> View {
        let Some(scene) = self
            .scene
            .active
            .as_ref()
            .and_then(|id| self.scene.scenes.iter().find(|s| &s.id == id))
            .filter(|s| s.has_leave_work())
        else {
            return View::empty();
        };
        let mut work: Vec<String> = scene.leave.iter().map(|a| a.label()).collect();
        if let Some(prep) = scene.tv.as_ref().filter(|t| t.off_when_leaving) {
            let tv = self
                .scene
                .tvs
                .iter()
                .find(|t| t.id == prep.id)
                .map_or("the TV".to_string(), |t| t.name.clone());
            work.push(format!("Turn off {tv}"));
        }
        if scene.return_to.is_some() {
            work.push(format!("Return to {}", self.return_label(scene)));
        }
        Button::new()
            .automation_name(format!("End {}", scene.name))
            .is_enabled(self.scene_ready())
            .on_click(cx.message(msg(SceneMessage::End)))
            .content(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        FontIcon::new()
                            .glyph("\u{e7e8}")
                            .vertical_alignment(VerticalAlignment::Center),
                        TextBlock::new().text(format!("End {}", scene.name)),
                    )),
            )
            .tooltip(format!("Runs: {}.", work.join(", ")))
    }
    fn return_label(&self, scene: &Scene) -> String {
        scene.return_to.as_ref().map_or(String::new(), |id| {
            self.scene
                .scenes
                .iter()
                .find(|s| &s.id == id)
                .map_or("missing scene".into(), |s| s.name.clone())
        })
    }
    pub(crate) fn scene_view(&self, cx: &ViewContext<Self>) -> View {
        if let Some(d) = &self.scene.draft {
            return self.scene_editor(cx, d);
        }
        // Every card gets the height the fullest one needs, so the grid lines up without a
        // fixed gap under short summaries.
        let card_height = self
            .scene
            .scenes
            .iter()
            .map(|s| {
                scene_card_height(
                    &self.scene_summary(s),
                    scenes::resolve(s, &self.scene.displays, &self.scene.tvs).err(),
                )
            })
            .fold(0.0, f64::max);
        let cards = self
            .scene
            .scenes
            .iter()
            .map(|s| {
                let apply = s.id.clone();
                let edit = s.id.clone();
                let delete = s.id.clone();
                let resolved = scenes::resolve(s, &self.scene.displays, &self.scene.tvs);
                let name = s.name.clone();
                let active = self.scene.active.as_deref() == Some(s.id.as_str());
                let returns = active && s.return_to.is_some();
                let action_label = if returns {
                    format!("Return to {}", self.return_label(s))
                } else {
                    "Apply".into()
                };
                // The Active dot sits before the name in a slot every scene card reserves, so
                // marking a scene never moves its name or changes the card's height.
                let lead = if active {
                    let label = if returns {
                        format!("Active · {action_label}")
                    } else {
                        "Active".into()
                    };
                    TextBlock::new()
                        .text("●")
                        .foreground(crate::ui::SUCCESS)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .automation_name(label.as_str())
                        .tooltip(label.as_str())
                } else {
                    View::empty()
                };
                let body = StackPanel::new().spacing(8.0).children((
                    self.scene_hero(s),
                    note(self.scene_summary(s)),
                    if let Err(e) = resolved {
                        TextBlock::new()
                            .text(e)
                            .foreground(crate::ui::critical())
                            .text_wrapping(TextWrapping::Wrap)
                            .into()
                    } else {
                        View::empty()
                    },
                ));
                KeyedView::new(
                    s.id.clone(),
                    crate::preset_cards_ui::card(
                        &name,
                        Some(lead),
                        crate::command_ui::command_button(
                            cx,
                            crate::command_ui::Target {
                                kind: crate::command_ui::Kind::Scene,
                                name: s.name.clone(),
                                id: s.id.clone(),
                                can_end: s.has_leave_work(),
                            },
                        ),
                        body,
                        vec![
                            crate::preset_cards_ui::action_button(
                                Button::new()
                                    .automation_name(if returns {
                                        action_label.clone()
                                    } else {
                                        format!("Apply {name}")
                                    })
                                    .is_enabled(
                                        self.scene_ready()
                                            && (returns
                                                || scenes::resolve(
                                                    s,
                                                    &self.scene.displays,
                                                    &self.scene.tvs,
                                                )
                                                .is_ok()),
                                    )
                                    .on_click(cx.callback(move |()| {
                                        msg(SceneMessage::Apply(apply.clone()))
                                    })),
                                if returns { "Return" } else { "Apply" },
                                if returns {
                                    &action_label
                                } else {
                                    "Apply scene"
                                },
                            ),
                            crate::preset_cards_ui::card_icon_button(Button::new())
                                .automation_name(format!("Edit {}", name))
                                .is_enabled(self.scene_ready())
                                .on_click(
                                    cx.callback(move |()| msg(SceneMessage::Edit(edit.clone()))),
                                )
                                .content(crate::preset_cards_ui::card_edit_icon())
                                .tooltip("Edit scene"),
                            crate::preset_cards_ui::card_icon_button(Button::new())
                                .automation_name(format!("Delete {}", name))
                                .is_enabled(self.scene_ready())
                                .on_click(
                                    cx.callback(move |()| {
                                        msg(SceneMessage::Delete(delete.clone()))
                                    }),
                                )
                                .content(crate::icons::delete_small())
                                .tooltip("Delete scene"),
                        ],
                        card_height,
                        true,
                    ),
                )
            })
            .collect::<Vec<_>>();
        Grid::new()
            .rows([
                GridLength::Auto,
                GridLength::Auto,
                GridLength::Star(1.0),
                GridLength::Auto,
            ])
            .children((
                crate::ui::header(
                    "Scenes",
                    "Switch your whole setup in one click.",
                    vec![
                        self.end_scene_button(cx),
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .automation_name("Add scene")
                            .is_enabled(self.scene_ready())
                            .on_click(cx.message(msg(SceneMessage::Add)))
                            .content(FontIcon::new().glyph(""))
                            .tooltip("Add scene"),
                    ],
                ),
                Border::new().grid_row(1).content(self.next_scene_tip(cx)),
                if self.scene.scenes.is_empty() {
                    ScrollViewer::new().grid_row(2).content(
                        Border::new()
                            .padding(Thickness::new(0.0, 4.0, 0.0, 20.0))
                            .content(self.starter_card(cx)),
                    )
                } else {
                    crate::preset_cards_ui::card_grid(2, card_height, cards)
                },
                if self.scene.cancel.is_some() {
                    Button::new()
                        .grid_row(3)
                        .horizontal_alignment(HorizontalAlignment::Left)
                        .automation_name("Cancel scene")
                        .on_click(cx.message(msg(SceneMessage::Stop)))
                        .content("Cancel scene")
                } else {
                    View::empty()
                },
            ))
    }
    /// Apply an edit from the Sound step dialog to the draft.
    fn edit_sound(&mut self, edit: SoundEdit) {
        let tvs = self
            .scene
            .tvs
            .iter()
            .filter(|t| t.audio.is_some())
            .map(|t| t.id.clone())
            .collect::<Vec<_>>();
        let (outputs, inputs) = (&self.outputs, &self.inputs);
        let Some(d) = &mut self.scene.draft else {
            return;
        };
        let sound = d.sound.get_or_insert_with(Sound::default);
        let percent = |v: f64| v.round().clamp(0.0, 100.0) as u8;
        let level_of = |devices: &[Output], id: Option<&str>| {
            id.and_then(|id| devices.iter().find(|o| o.id == id))
                .and_then(|o| o.volume_percent)
                .unwrap_or(50)
        };
        let default_device = |devices: &[Output], prefer: fn(&Output) -> bool| {
            devices
                .iter()
                .find(|o| o.active && prefer(o))
                .or_else(|| devices.iter().find(|o| o.active))
                .map(|o| DeviceSelection {
                    id: o.id.clone(),
                    name: o.name.clone(),
                })
        };
        match edit {
            SoundEdit::Output(Some(i)) => {
                let devices = device_choices(outputs, sound_device(sound).as_ref());
                sound.output = match i.checked_sub(1) {
                    None => None,
                    Some(i) if i < devices.len() => Some(SoundOutput::Device {
                        id: devices[i].id.clone(),
                        name: devices[i].name.clone(),
                    }),
                    Some(i) => tvs
                        .get(i - devices.len())
                        .map(|id| SoundOutput::Tv { id: id.clone() }),
                };
            }
            SoundEdit::SetVolume(on) => {
                let id = sound_device(sound).map(|d| d.id);
                sound.volume = on.then(|| level_of(outputs, id.as_deref()));
            }
            SoundEdit::Volume(v) if sound.volume.is_some() && v.is_finite() => {
                sound.volume = Some(percent(v));
            }
            SoundEdit::Mute(Some(i)) => sound.mute = mute_from(i),
            SoundEdit::Calls(on) => {
                sound.calls = if on {
                    sound
                        .calls
                        .clone()
                        .or_else(|| default_device(outputs, |o| o.communications))
                } else {
                    None
                };
            }
            SoundEdit::CallsDevice(Some(i)) if sound.calls.is_some() => {
                if let Some(device) = device_choices(outputs, sound.calls.as_ref()).get(i) {
                    sound.calls = Some(device.clone());
                }
            }
            SoundEdit::Mic(on) => {
                sound.mic = if on {
                    sound.mic.clone().or_else(|| {
                        default_device(inputs, |o| o.console).map(|device| scenes::Mic {
                            device,
                            volume: None,
                            mute: None,
                        })
                    })
                } else {
                    None
                };
            }
            SoundEdit::MicDevice(Some(i)) => {
                if let Some(mic) = &mut sound.mic
                    && let Some(device) = device_choices(inputs, Some(&mic.device)).get(i)
                {
                    mic.device = device.clone();
                }
            }
            SoundEdit::SetMicVolume(on) => {
                if let Some(mic) = &mut sound.mic {
                    mic.volume = on.then(|| level_of(inputs, Some(&mic.device.id)));
                }
            }
            SoundEdit::MicVolume(v) if v.is_finite() => {
                if let Some(mic) = &mut sound.mic
                    && mic.volume.is_some()
                {
                    mic.volume = Some(percent(v));
                }
            }
            SoundEdit::MicMute(Some(i)) => {
                if let Some(mic) = &mut sound.mic {
                    mic.mute = mute_from(i);
                }
            }
            _ => {}
        }
    }
    /// A Sound step in a few words, for example "SOUNDBAR · 35% · mic Astro".
    pub(crate) fn sound_label(&self, sound: &Sound) -> String {
        let mut parts = vec![match &sound.output {
            Some(SoundOutput::Device { name, .. }) => name.clone(),
            Some(SoundOutput::Tv { id }) => self
                .scene
                .tvs
                .iter()
                .find(|t| &t.id == id)
                .map_or("Missing TV".into(), |t| format!("{}'s sound", t.name)),
            None => "Current output".into(),
        }];
        if let Some(v) = sound.volume {
            parts.push(format!("{v}%"));
        }
        match sound.mute {
            Some(true) => parts.push("muted".into()),
            Some(false) => parts.push("unmuted".into()),
            None => {}
        }
        if let Some(calls) = &sound.calls {
            parts.push(format!("calls on {}", calls.name));
        }
        if let Some(mic) = &sound.mic {
            parts.push(format!("mic {}", mic.device.name));
        }
        if !sound.changes_something() {
            return "Nothing set".into();
        }
        parts.join(" · ")
    }
    /// The top of a scene card shows the scene's main change: its screens, else its sound,
    /// its TV, its FPS limit or its actions.
    fn scene_hero(&self, s: &Scene) -> View {
        if let Some(p) = s
            .display
            .as_ref()
            .and_then(|id| self.scene.displays.iter().find(|p| &p.id == id))
        {
            return crate::preset_cards_ui::layout_preview(&p.layout, 124.0);
        }
        if let Some(sound) = &s.sound {
            let output = match &sound.output {
                Some(SoundOutput::Device { name, .. }) => name.clone(),
                Some(SoundOutput::Tv { id }) => self
                    .scene
                    .tvs
                    .iter()
                    .find(|t| &t.id == id)
                    .map_or("Missing TV".into(), |t| format!("TV's sound · {}", t.name)),
                None => "Current output".into(),
            };
            let mut lines = vec![];
            if let Some(calls) = &sound.calls {
                lines.push(format!("Calls: {}", calls.name));
            }
            if let Some(mic) = &sound.mic {
                lines.push(format!(
                    "Mic: {}{}",
                    mic.device.name,
                    mic.volume.map_or(String::new(), |v| format!(" · {v}%"))
                ));
            }
            return hero_panel(
                "\u{e767}",
                output,
                if sound.mute == Some(true) {
                    Level::Muted
                } else {
                    sound.volume.map_or(Level::None, Level::Percent)
                },
                lines,
            );
        }
        if let Some(prep) = &s.tv {
            let tv = self.scene.tvs.iter().find(|t| t.id == prep.id);
            let input = tv.and_then(|tv| {
                prep.input.as_ref().and_then(|id| {
                    tv.control_inputs()
                        .iter()
                        .find(|i| &i.id == id)
                        .map(|i| i.label.clone())
                })
            });
            return hero_panel(
                "\u{e7f4}",
                tv.map_or("Missing TV".into(), |t| t.name.clone()),
                Level::None,
                input.into_iter().collect(),
            );
        }
        if let Some(fps) = &s.fps {
            return hero_panel("\u{e916}", fps.label(), Level::None, vec![]);
        }
        let actions = s.before.len() + s.actions.len();
        hero_panel(
            "\u{e768}",
            format!("{actions} action{}", if actions == 1 { "" } else { "s" }),
            Level::None,
            vec![],
        )
    }
    /// One line per step the scene changes, in run order. Steps it leaves alone are omitted.
    fn scene_summary(&self, s: &Scene) -> String {
        let actions = |n: usize| {
            if n == 1 {
                "1 action".to_string()
            } else {
                format!("{n} actions")
            }
        };
        let mut lines = vec![];
        if !s.before.is_empty() {
            lines.push(format!("Before: {}", actions(s.before.len())));
        }
        if let Some(p) = &s.tv {
            let tv = self
                .scene
                .tvs
                .iter()
                .find(|t| t.id == p.id)
                .map_or("Missing TV", |t| t.name.as_str());
            lines.push(format!("TV: {tv}"));
        }
        if let Some(id) = &s.display {
            let display = self
                .scene
                .displays
                .iter()
                .find(|p| &p.id == id)
                .map_or("Missing display preset", |p| p.name.as_str());
            lines.push(format!("Displays: {display}"));
        }
        if let Some(sound) = &s.sound {
            lines.push(format!("Sound: {}", self.sound_label(sound)));
        }
        if let Some(fps) = &s.fps {
            lines.push(format!("FPS: {}", fps.label()));
        }
        if !s.actions.is_empty() {
            lines.push(format!("After: {}", actions(s.actions.len())));
        }
        let tv_off = s.tv.as_ref().filter(|t| t.off_when_leaving).map(|p| {
            self.scene
                .tvs
                .iter()
                .find(|t| t.id == p.id)
                .map_or("turn off the TV".to_string(), |t| {
                    format!("turn off {}", t.name)
                })
        });
        match (tv_off, s.leave.len()) {
            (None, 0) => {}
            (None, n) => lines.push(format!("When leaving: {}", actions(n))),
            (Some(off), 0) => lines.push(format!("When leaving: {off}")),
            (Some(off), n) => lines.push(format!("When leaving: {off} · {}", actions(n))),
        }
        if lines.is_empty() {
            "This scene doesn’t change anything yet. Edit it to add steps.".into()
        } else {
            lines.join("\n")
        }
    }
    pub(crate) fn scene_dialog(&self, cx: &ViewContext<Self>) -> View {
        if self.scene.open.is_some() && self.scene.draft.is_some() {
            return self.step_dialog(cx);
        }
        if let Some(seconds) = self.scene.remaining {
            return ContentDialog::new()
                .is_open(true)
                .title("Keep these display settings?")
                .primary_button_text("Keep settings")
                .close_button_text("Revert")
                .on_closed(cx.callback(|r| msg(SceneMessage::Confirm(r))))
                .content(note(format!(
                    "Your previous layout will be restored in {seconds} seconds unless confirmed."
                )));
        }
        if let Some(id) = &self.scene.delete {
            let name = self
                .scene
                .scenes
                .iter()
                .find(|s| &s.id == id)
                .map_or("this scene", |s| s.name.as_str());
            return ContentDialog::new()
                .is_open(true)
                .title(format!("Delete “{name}”?"))
                .primary_button_text("Delete scene")
                .close_button_text("Cancel")
                .on_closed(cx.callback(|r| msg(SceneMessage::Deleted(r))))
                .content(note(
                    "Your presets, TV connections and current setup stay unchanged.",
                ));
        }
        View::empty()
    }
}

fn pick_path(folder: bool) -> Result<Option<String>, String> {
    use windows::{
        Win32::{
            System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
            UI::Shell::{
                Common::COMDLG_FILTERSPEC, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
                FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
            },
        },
        core::{PCWSTR, w},
    };
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| e.to_string())?;
        let mut options = dialog.GetOptions().map_err(|e| e.to_string())?
            | FOS_FORCEFILESYSTEM
            | FOS_FILEMUSTEXIST;
        if folder {
            options |= FOS_PICKFOLDERS;
        } else {
            dialog
                .SetFileTypes(&[COMDLG_FILTERSPEC {
                    pszName: w!("Applications"),
                    pszSpec: w!("*.exe"),
                }])
                .map_err(|e| e.to_string())?;
        }
        dialog.SetOptions(options).map_err(|e| e.to_string())?;
        match dialog.Show(Some(crate::close_guard::window()?)) {
            Ok(()) => {}
            Err(e) if e.code().0 == 0x800704C7u32 as i32 => return Ok(None),
            Err(e) => return Err(format!("Could not open the file picker: {e}")),
        }
        let value = dialog
            .GetResult()
            .and_then(|i| i.GetDisplayName(SIGDN_FILESYSPATH))
            .map_err(|e| e.to_string())?;
        let path = PCWSTR(value.0).to_string().map_err(|e| e.to_string());
        CoTaskMemFree(Some(value.0.cast()));
        path.map(Some)
    }
}

#[cfg(test)]
mod card_height_tests {
    use super::scene_card_height;
    #[test]
    fn cards_grow_with_their_lines_and_long_lines_wrap() {
        let three = scene_card_height(
            "Displays: Desk
Sound: Soundbar · 10%
FPS: RTSS · 228 FPS",
            None,
        );
        let six = scene_card_height(
            "a
b
c
d
e
f",
            None,
        );
        assert!((six - three - 60.0).abs() < 1e-9);
        // A line twice as long as fits counts as two.
        let wrapped = scene_card_height(&"x".repeat(80), None);
        let single = scene_card_height("x", None);
        assert!((wrapped - single - 20.0).abs() < 1e-9);
        assert!(scene_card_height("x", Some("Missing TV".into())) > single);
    }
}
