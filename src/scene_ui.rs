//! Native scene cards and editor; device work stays on the shared worker runner.
use crate::{DirectorApp, Message, work};
use director::{
    scenes::{self, Action, ActionList, Scene, TvPreparation},
    windows_audio::OperationLock,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use windows_reactor::*;
type Catalog = (
    Vec<Scene>,
    Vec<director::displays::Preset>,
    Vec<director::presets::Preset>,
    Vec<director::tvs::Tv>,
);
#[derive(Clone)]
pub(crate) enum SceneMessage {
    Load,
    Loaded(Result<Catalog, String>),
    Add,
    Edit(String),
    Cancel,
    Name(String),
    Display(Option<usize>),
    Audio(Option<usize>),
    Tv(Option<usize>),
    Wake(bool),
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
    /// Expand one step block in the editor, or collapse with `None`.
    Open(Option<crate::scene_editor_ui::Step>),
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
    pub(crate) audio: Vec<director::presets::Preset>,
    pub(crate) tvs: Vec<director::tvs::Tv>,
    pub(crate) draft: Option<Scene>,
    pub(crate) ready: bool,
    delete: Option<String>,
    token: u64,
    cancel: Option<Arc<AtomicBool>>,
    events: Option<Arc<Mutex<mpsc::Receiver<Event>>>>,
    confirm: Option<mpsc::Sender<director::display_apply::Decision>>,
    remaining: Option<u64>,
    /// The step block expanded in the editor.
    pub(crate) open: Option<crate::scene_editor_ui::Step>,
    /// The scene applied last during this Windows session.
    pub(crate) active: Option<String>,
}
fn msg(m: SceneMessage) -> Message {
    Message::Scene(m)
}
fn note(s: impl Into<String>) -> View {
    TextBlock::new()
        .text(s)
        .text_wrapping(TextWrapping::Wrap)
        .opacity(0.72)
        .into()
}
fn heading(s: &str) -> View {
    TextBlock::new()
        .text(s)
        .font_size(20.0)
        .font_weight(FontWeight::SEMI_BOLD)
        .into()
}
fn id() -> String {
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
                let (_, displays, audio, tvs) = director::windows_scenes::catalog()?;
                for scene in &next {
                    if !expected.iter().any(|old| old == scene) {
                        scenes::resolve(scene, &displays, &audio, &tvs)?;
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
            let linked = self.scene.tvs.iter().filter(|t| t.audio.is_some()).count();
            let create = match &m {
                SceneMessage::Display(Some(i)) if *i == self.scene.displays.len() + 1 => {
                    Some(Slot::Display)
                }
                SceneMessage::Audio(Some(i)) if *i == self.scene.audio.len() + linked + 1 => {
                    Some(Slot::Audio)
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
                    msg(SceneMessage::Loaded(director::windows_scenes::catalog()))
                });
            }
            SceneMessage::Loaded(result) => {
                self.end();
                match result {
                    Ok((s, d, a, t)) => {
                        self.scene.scenes = s;
                        self.scene.displays = d;
                        self.scene.audio = a;
                        self.scene.tvs = t;
                        self.scene.ready = true;
                        self.scene.active = director::scene_session::active();
                        self.status = "Ready.".into();
                        self.library_select_in_scene(cx);
                    }
                    Err(e) => {
                        self.scene.ready = false;
                        self.failure(e)
                    }
                }
            }
            SceneMessage::Add if ready => {
                self.scene.draft = Some(Scene::new(id(), ""));
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
            SceneMessage::Audio(index) if !self.busy && index.is_some() => {
                let chosen = index.and_then(|i| i.checked_sub(1));
                let preset = chosen
                    .and_then(|i| self.scene.audio.get(i))
                    .map(|p| p.id.clone());
                let linked = chosen
                    .and_then(|i| i.checked_sub(self.scene.audio.len()))
                    .and_then(|i| self.scene.tvs.iter().filter(|t| t.audio.is_some()).nth(i))
                    .map(|t| t.id.clone());
                if let Some(d) = &mut self.scene.draft {
                    d.audio = preset;
                    d.tv_audio = linked;
                }
            }
            SceneMessage::Tv(index) if !self.busy && index.is_some() => {
                let selected = index
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| self.scene.tvs.get(i))
                    .map(|t| TvPreparation {
                        id: t.id.clone(),
                        wake: t.can_wake(),
                        input: t
                            .pc_input
                            .as_ref()
                            .filter(|pc| t.control_inputs().iter().any(|i| i.id == pc.id))
                            .map(|i| i.id.clone()),
                    });
                if let Some(d) = &mut self.scene.draft {
                    d.tv = selected;
                }
            }
            SceneMessage::Wake(v) if !self.busy => {
                if let Some(t) = self.scene.draft.as_mut().and_then(|d| d.tv.as_mut()) {
                    t.wake = v;
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
                    }
                }
            }
            SceneMessage::Remove(list, i) if !self.busy => {
                if let Some(d) = &mut self.scene.draft
                    && i < d.list(list).len()
                {
                    d.list_mut(list).remove(i);
                }
            }
            SceneMessage::Open(step) => self.scene.open = step,
            SceneMessage::ClearStep(step) if !self.busy => {
                use crate::scene_editor_ui::Step;
                if let Some(d) = &mut self.scene.draft {
                    match step {
                        Step::Before => d.before.clear(),
                        Step::Tv => d.tv = None,
                        Step::Displays => d.display = None,
                        Step::Audio => {
                            d.audio = None;
                            d.tv_audio = None;
                        }
                        Step::Fps => d.fps = None,
                        Step::After => d.actions.clear(),
                        Step::Leave => d.leave.clear(),
                    }
                }
                if self.scene.open == Some(step) {
                    self.scene.open = None;
                }
            }
            SceneMessage::End if ready => {
                self.scene.token = self.scene.token.wrapping_add(1);
                let token = self.scene.token;
                self.begin("Ending scene…");
                work(cx, move || {
                    msg(SceneMessage::Finished(
                        token,
                        (|| {
                            let _lock = OperationLock::acquire()?;
                            let mut backend = director::windows_scenes::WindowsScene::new(
                                |_: &director::displays::Preset| false,
                            );
                            Ok(director::windows_scenes::end(&mut backend, |_| {}))
                        })(),
                    ))
                });
            }
            SceneMessage::Save if ready => {
                if let Some(mut d) = self.scene.draft.clone() {
                    d.name = d.name.trim().into();
                    if let Err(e) = scenes::resolve(
                        &d,
                        &self.scene.displays,
                        &self.scene.audio,
                        &self.scene.tvs,
                    ) {
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
            SceneMessage::Apply(id) if ready => {
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
                        let plan = director::windows_scenes::load_run_plan(&id, &name, false)?;
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
                        Ok(director::windows_scenes::run(
                            &mut backend,
                            &plan,
                            &cancel,
                            false,
                            |step| {
                                let _ = tx.send(Event::Step(step.into()));
                            },
                        ))
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
    pub(crate) fn scene_view(&self, cx: &ViewContext<Self>) -> View {
        if let Some(d) = &self.scene.draft {
            return self.scene_editor(cx, d);
        }
        let cards = self
            .scene
            .scenes
            .iter()
            .map(|s| {
                let apply = s.id.clone();
                let edit = s.id.clone();
                let delete = s.id.clone();
                let resolved =
                    scenes::resolve(s, &self.scene.displays, &self.scene.audio, &self.scene.tvs);
                let name = s.name.clone();
                let active = self.scene.active.as_deref() == Some(s.id.as_str());
                let body = StackPanel::new().spacing(8.0).children((
                    if active {
                        // End sits with the Active label so the header keeps room for the name.
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(12.0)
                            .children((
                                TextBlock::new()
                                    .text("● Active")
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .foreground(Color::rgb(108, 203, 95))
                                    .vertical_alignment(VerticalAlignment::Center),
                                HyperlinkButton::new()
                                    .automation_name(format!("End {}", name))
                                    .is_enabled(self.scene_ready())
                                    .on_click(cx.message(msg(SceneMessage::End)))
                                    .content("End scene")
                                    .tooltip("Run this scene’s When leaving actions. Your settings stay as they are."),
                            ))
                    } else {
                        View::empty()
                    },
                    s.display
                        .as_ref()
                        .and_then(|id| self.scene.displays.iter().find(|p| &p.id == id))
                        .map(|p| crate::preset_cards_ui::layout_preview(&p.layout, 124.0))
                        .unwrap_or_else(|| {
                            Border::new()
                                .height(100.0)
                                .content(note("Displays unchanged"))
                        }),
                    note(self.scene_summary(s)),
                    if let Err(e) = resolved {
                        TextBlock::new()
                            .text(e)
                            .foreground(Color::rgb(240, 130, 115))
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
                        body,
                        vec![
                            crate::preset_cards_ui::apply_button(
                                Button::new()
                                    .automation_name(format!("Apply {}", name))
                                    .is_enabled(
                                        self.scene_ready()
                                            && scenes::resolve(
                                                s,
                                                &self.scene.displays,
                                                &self.scene.audio,
                                                &self.scene.tvs,
                                            )
                                            .is_ok(),
                                    )
                                    .on_click(cx.callback(move |()| {
                                        msg(SceneMessage::Apply(apply.clone()))
                                    })),
                                "Apply scene",
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
                        crate::preset_cards_ui::SCENE_CARD_HEIGHT,
                    ),
                )
            })
            .collect::<Vec<_>>();
        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
            .children((
                crate::preset_ui::header(
                    "Scenes",
                    "Switch your whole setup in one click.",
                    vec![
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .automation_name("Add scene")
                            .is_enabled(self.scene_ready())
                            .on_click(cx.message(msg(SceneMessage::Add)))
                            .content(FontIcon::new().glyph(""))
                            .tooltip("Add scene"),
                    ],
                ),
                if self.scene.scenes.is_empty() {
                    Border::new()
                        .grid_row(1)
                        .padding(20.0)
                        .content(self.scene_first_run(cx))
                } else {
                    crate::preset_cards_ui::card_grid(
                        1,
                        crate::preset_cards_ui::SCENE_CARD_HEIGHT,
                        cards,
                    )
                },
                if self.scene.cancel.is_some() {
                    Button::new()
                        .grid_row(2)
                        .horizontal_alignment(HorizontalAlignment::Left)
                        .automation_name("Cancel scene")
                        .on_click(cx.message(msg(SceneMessage::Stop)))
                        .content("Cancel scene")
                } else {
                    View::empty()
                },
            ))
    }
    fn scene_summary(&self, s: &Scene) -> String {
        let display = s
            .display
            .as_ref()
            .map(|id| {
                self.scene
                    .displays
                    .iter()
                    .find(|p| &p.id == id)
                    .map_or("Missing display preset", |p| p.name.as_str())
            })
            .unwrap_or("Unchanged");
        let audio = s
            .audio
            .as_ref()
            .map(|id| {
                self.scene
                    .audio
                    .iter()
                    .find(|p| &p.id == id)
                    .map_or("Missing audio preset", |p| p.name.as_str())
                    .to_string()
            })
            .or_else(|| {
                s.tv_audio.as_ref().map(|id| {
                    self.scene
                        .tvs
                        .iter()
                        .find(|t| &t.id == id)
                        .and_then(|t| t.audio.as_ref())
                        .map_or("Missing TV audio", |a| a.name.as_str())
                        .into()
                })
            })
            .unwrap_or("Unchanged".into());
        let tv =
            s.tv.as_ref()
                .map(|p| {
                    self.scene
                        .tvs
                        .iter()
                        .find(|t| t.id == p.id)
                        .map_or("Missing TV", |t| t.name.as_str())
                })
                .unwrap_or("None");
        let mut lines = vec![];
        if !s.before.is_empty() {
            lines.push(format!("Before: {} action(s)", s.before.len()));
        }
        lines.push(format!("TV: {tv}"));
        lines.push(format!("Displays: {display}"));
        lines.push(format!("Audio: {audio}"));
        lines.push(format!(
            "FPS: {}",
            s.fps
                .as_ref()
                .map_or("Leave unchanged".into(), |r| r.label())
        ));
        lines.push(format!("After: {} action(s)", s.actions.len()));
        if !s.leave.is_empty() {
            lines.push(format!("When leaving: {} action(s)", s.leave.len()));
        }
        lines.join("\n")
    }
    /// First-run checklist: save the building blocks in the Library, then combine them here.
    fn scene_first_run(&self, cx: &ViewContext<Self>) -> View {
        let step = |key: &str, done: bool, title: &str, detail: &str, action: View| {
            KeyedView::new(
                key,
                Grid::new()
                    .columns([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
                    .column_spacing(12.0)
                    .children((
                        FontIcon::new()
                            .grid_column(0)
                            .glyph(if done { "\u{e73e}" } else { "\u{ea3a}" })
                            .opacity(if done { 1.0 } else { 0.5 })
                            .vertical_alignment(VerticalAlignment::Top)
                            .margin(Thickness::new(0.0, 3.0, 0.0, 0.0)),
                        StackPanel::new().grid_column(1).spacing(2.0).children((
                            TextBlock::new()
                                .text(title)
                                .font_weight(FontWeight::SEMI_BOLD),
                            note(detail),
                        )),
                        Border::new()
                            .grid_column(2)
                            .vertical_alignment(VerticalAlignment::Center)
                            .content(action),
                    )),
            )
        };
        let go = |label: &str, tag: &str| -> View {
            Button::new()
                .is_enabled(!self.busy)
                .on_click(cx.message(Message::Navigate(Some(tag.into()))))
                .content(label)
        };
        StackPanel::new()
            .spacing(16.0)
            .max_width(650.0)
            .horizontal_alignment(HorizontalAlignment::Left)
            .children((
                heading("Your setup, ready in one scene"),
                note("A scene combines saved building blocks from your Library and runs them in order."),
                StackPanel::new().spacing(14.0).keyed_children([
                    step(
                        "displays",
                        !self.scene.displays.is_empty(),
                        "1 · Save a display layout",
                        "Arrange your screens in Now, then save them as a display preset.",
                        go("Open Now", "current"),
                    ),
                    step(
                        "audio",
                        !self.scene.audio.is_empty(),
                        "2 · Save your sound (optional)",
                        "Pick an output and volume to switch to with the scene.",
                        go("Audio", "audio"),
                    ),
                    step(
                        "tv",
                        !self.scene.tvs.is_empty(),
                        "3 · Pair a TV (optional)",
                        "Lets the scene wake your TV and switch its input first.",
                        go("TVs", "tvs"),
                    ),
                    step(
                        "scene",
                        false,
                        "4 · Create the scene",
                        "Choose what it changes and what to open afterwards.",
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .is_enabled(self.scene_ready())
                            .on_click(cx.message(msg(SceneMessage::Add)))
                            .content("Create scene"),
                    ),
                ]),
            ))
    }
    pub(crate) fn scene_dialog(&self, cx: &ViewContext<Self>) -> View {
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
