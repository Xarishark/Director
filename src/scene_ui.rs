//! Native scene cards and editor; device work stays on the shared worker runner.
use crate::{DirectorApp, Message, work};
use director::{
    scenes::{self, Action, Scene, TvPreparation},
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
    AddUri,
    AddApp,
    Browse(usize, bool),
    ActionText(usize, u8, String),
    Move(usize, bool),
    Remove(usize),
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
    fn scene_ready(&self) -> bool {
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
                self.status =
                    "Choose what this scene changes. Saving leaves your setup untouched.".into();
            }
            SceneMessage::Edit(id) if ready => {
                self.scene.draft = self.scene.scenes.iter().find(|s| s.id == id).cloned();
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
            SceneMessage::AddUri if !self.busy => {
                if let Some(d) = &mut self.scene.draft
                    && d.actions.len() < 32
                {
                    d.actions.push(Action::Uri {
                        target: String::new(),
                    });
                }
            }
            SceneMessage::Browse(i, folder) if !self.busy => match pick_path(folder) {
                Ok(Some(path)) => self.update_scene(
                    SceneMessage::ActionText(i, if folder { 2 } else { 0 }, path),
                    cx,
                ),
                Ok(None) => {}
                Err(e) => self.failure(e),
            },
            SceneMessage::AddApp if !self.busy => {
                if let Some(d) = &mut self.scene.draft
                    && d.actions.len() < 32
                {
                    d.actions.push(Action::App {
                        path: String::new(),
                        args: vec![],
                        working_directory: None,
                        wait: false,
                    });
                }
            }
            SceneMessage::ActionText(i, field, value) if !self.busy => {
                if let Some(a) = self.scene.draft.as_mut().and_then(|d| d.actions.get_mut(i)) {
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
            SceneMessage::Move(i, up) if !self.busy => {
                if let Some(d) = &mut self.scene.draft {
                    let other = if up {
                        i.checked_sub(1)
                    } else {
                        i.checked_add(1)
                    };
                    if let Some(j) = other
                        && i < d.actions.len()
                        && j < d.actions.len()
                    {
                        d.actions.swap(i, j);
                    }
                }
            }
            SceneMessage::Remove(i) if !self.busy => {
                if let Some(d) = &mut self.scene.draft
                    && i < d.actions.len()
                {
                    d.actions.remove(i);
                }
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
                let body = StackPanel::new().spacing(8.0).children((
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
        format!(
            "TV: {tv}\nDisplays: {display}\nAudio: {audio}\nFPS: {}\nAfter setup: {} action(s)",
            s.fps
                .as_ref()
                .map_or("Leave unchanged".into(), |r| r.label()),
            s.actions.len()
        )
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
    fn scene_editor(&self, cx: &ViewContext<Self>, d: &Scene) -> View {
        let available = !self.busy;
        let wide = self.window_width >= 1150.0;
        let display_labels = std::iter::once("Leave displays unchanged".into())
            .chain(self.scene.displays.iter().map(|p| p.name.clone()))
            .chain(std::iter::once("New display preset…".into()))
            .collect::<Vec<String>>();
        let audio_labels = std::iter::once("Leave audio unchanged".into())
            .chain(
                self.scene
                    .audio
                    .iter()
                    .map(|p| format!("{} · {}", p.name, p.output_name)),
            )
            .chain(self.scene.tvs.iter().filter_map(|t| {
                t.audio
                    .as_ref()
                    .map(|a| format!("{} linked audio · {}", t.name, a.name))
            }))
            .chain(std::iter::once("New audio preset…".into()))
            .collect::<Vec<String>>();
        let ai = d
            .audio
            .as_ref()
            .and_then(|id| self.scene.audio.iter().position(|p| &p.id == id))
            .map(|i| i + 1)
            .or_else(|| {
                d.tv_audio
                    .as_ref()
                    .and_then(|id| {
                        self.scene
                            .tvs
                            .iter()
                            .filter(|t| t.audio.is_some())
                            .position(|t| &t.id == id)
                    })
                    .map(|i| i + 1 + self.scene.audio.len())
            })
            .unwrap_or(0);
        let tv =
            d.tv.as_ref()
                .and_then(|p| self.scene.tvs.iter().find(|t| t.id == p.id));
        let tv_labels = std::iter::once("No TV preparation".into())
            .chain(self.scene.tvs.iter().map(|t| t.name.clone()))
            .chain(std::iter::once("Add TV…".into()))
            .collect::<Vec<String>>();
        let fields=StackPanel::new().spacing(20.0).children((
 StackPanel::new().spacing(8.0).children((TextBlock::new().text("Scene name").font_weight(FontWeight::SEMI_BOLD),TextBox::new().automation_name("Scene name").text(d.name.clone()).is_enabled(available).on_text_changed(cx.callback(|s|msg(SceneMessage::Name(s)))))),
 StackPanel::new().spacing(10.0).children((heading("1 · Prepare TV"),note("Optional. Wait for your TV, then select its input."),ComboBox::new().automation_name("Scene TV").items_source(tv_labels).selected_index(Some(d.tv.as_ref().and_then(|p|self.scene.tvs.iter().position(|t|t.id==p.id)).map_or(0,|i|i+1))).is_enabled(available).horizontal_alignment(HorizontalAlignment::Stretch).on_selection_changed(cx.callback(|i|msg(SceneMessage::Tv(i)))),
 if let (Some(tv),Some(prep))=(tv,&d.tv){StackPanel::new().spacing(10.0).children((
 if tv.can_wake(){ToggleSwitch::new().automation_name("Wake TV if needed").is_on(prep.wake).is_enabled(available).on_toggled(cx.callback(|v|msg(SceneMessage::Wake(v)))).slot(ToggleSwitchSlot::Header,"Wake TV if needed")}else{View::empty()},
 if tv.brand==director::tvs::Brand::Lg || !tv.control_inputs().is_empty(){ComboBox::new().automation_name("Scene TV input").items_source(std::iter::once("Leave input unchanged".into()).chain(tv.control_inputs().iter().map(|i|i.label.clone())).collect::<Vec<String>>()).selected_index(Some(prep.input.as_ref().and_then(|id|tv.control_inputs().iter().position(|i|&i.id==id)).map_or(0,|i|i+1))).is_enabled(available).horizontal_alignment(HorizontalAlignment::Stretch).on_selection_changed(cx.callback(|i|msg(SceneMessage::Input(i)))).into()}else{View::empty()},
 ))}else{View::empty()},
 )),
 StackPanel::new().spacing(10.0).children((heading("2 · Displays"),ComboBox::new().automation_name("Scene display preset").items_source(display_labels).selected_index(Some(d.display.as_ref().and_then(|id|self.scene.displays.iter().position(|p|&p.id==id)).map_or(0,|i|i+1))).is_enabled(available).horizontal_alignment(HorizontalAlignment::Stretch).on_selection_changed(cx.callback(|i|msg(SceneMessage::Display(i)))),d.display.as_ref().and_then(|id|self.scene.displays.iter().find(|p|&p.id==id)).map(|p|crate::preset_cards_ui::layout_preview(&p.layout, 124.0)).unwrap_or_else(View::empty),Button::new().is_enabled(available).on_click(cx.message(Message::Navigate(Some("displays".into())))).content("Manage display presets"))),
 StackPanel::new().spacing(10.0).children((heading("3 · Audio"),ComboBox::new().automation_name("Scene audio preset").items_source(audio_labels).selected_index(Some(ai)).is_enabled(available).horizontal_alignment(HorizontalAlignment::Stretch).on_selection_changed(cx.callback(|i|msg(SceneMessage::Audio(i)))),note("Audio applies after displays. A linked TV output changes all three output roles, leaving volume and microphone unchanged. Choose an audio preset for more control."),Button::new().is_enabled(available).on_click(cx.message(Message::Navigate(Some("audio".into())))).content("Manage audio presets"))),
 self.fps_controls(cx,crate::fps_ui::Target::Scene),
 StackPanel::new().spacing(10.0).children((heading("5 · After setup"),note("These run in order, only after setup succeeds."),
 StackPanel::new().spacing(12.0).keyed_children(d.actions.iter().enumerate().map(|(i,a)|KeyedView::new(format!("action-{i}"),Border::new().padding(14.0).corner_radius(8.0).border_thickness(1.0).border_brush(Color::rgb(75,75,75)).content(StackPanel::new().spacing(10.0).children((
 Grid::new().columns([GridLength::Star(1.0),GridLength::Auto]).children((TextBlock::new().text(format!("{} · {}",i+1,if matches!(a,Action::Uri{..}){"Open URI"}else{"Launch app"})).font_weight(FontWeight::SEMI_BOLD),StackPanel::new().grid_column(1).orientation(Orientation::Horizontal).children((
 Button::new().automation_name(format!("Move action {} up",i+1)).is_enabled(available&&i>0).on_click(cx.callback(move|()|msg(SceneMessage::Move(i,true)))).content(FontIcon::new().glyph("")).tooltip("Move up"),
 Button::new().automation_name(format!("Move action {} down",i+1)).is_enabled(available&&i+1<d.actions.len()).on_click(cx.callback(move|()|msg(SceneMessage::Move(i,false)))).content(FontIcon::new().glyph("")).tooltip("Move down"),
 Button::new().automation_name(format!("Remove action {}",i+1)).is_enabled(available).on_click(cx.callback(move|()|msg(SceneMessage::Remove(i)))).content(crate::icons::delete()).tooltip("Remove action"),
 )))),
 match a{Action::Uri{target}=>View::from(TextBox::new().automation_name(format!("Action {} URI",i+1)).placeholder_text("steam://open/bigpicture").text(target.clone()).is_enabled(available).on_text_changed(cx.callback(move|s|msg(SceneMessage::ActionText(i,0,s))))),Action::Close{target,..}=>View::from(TextBox::new().automation_name(format!("Action {} program to close",i+1)).placeholder_text("steam.exe").text(target.clone()).is_enabled(available).on_text_changed(cx.callback(move|s|msg(SceneMessage::ActionText(i,0,s))))),Action::App{path,args,working_directory,..}=>StackPanel::new().spacing(8.0).children((
 StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((Button::new().automation_name(format!("Browse action {} executable",i+1)).is_enabled(available).on_click(cx.callback(move|()|msg(SceneMessage::Browse(i,false)))).content("Choose app…"),Button::new().automation_name(format!("Browse action {} folder",i+1)).is_enabled(available).on_click(cx.callback(move|()|msg(SceneMessage::Browse(i,true)))).content("Working folder…"))),
 TextBox::new().automation_name(format!("Action {} executable",i+1)).placeholder_text("Full path to an .exe").text(path.clone()).is_enabled(available).on_text_changed(cx.callback(move|s|msg(SceneMessage::ActionText(i,0,s)))),
 TextBox::new().automation_name(format!("Action {} arguments",i+1)).placeholder_text("Optional arguments · one per line").accepts_return(true).text(args.join("\n")).is_enabled(available).on_text_changed(cx.callback(move|s|msg(SceneMessage::ActionText(i,1,s)))),
 TextBox::new().automation_name(format!("Action {} working folder",i+1)).placeholder_text("Optional working folder").text(working_directory.clone().unwrap_or_default()).is_enabled(available).on_text_changed(cx.callback(move|s|msg(SceneMessage::ActionText(i,2,s)))),
 ))},
 ))))).collect::<Vec<_>>()),
 StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((Button::new().automation_name("Add app action").is_enabled(available&&d.actions.len()<32).on_click(cx.message(msg(SceneMessage::AddApp))).content("Add app"),Button::new().automation_name("Add URI action").is_enabled(available&&d.actions.len()<32).on_click(cx.message(msg(SceneMessage::AddUri))).content("Add URI"))),
 )),
 note("If a step fails, Director restores your previous settings. Change this in Options."),
 ));
        let summary = Border::new()
            .padding(20.0)
            .corner_radius(8.0)
            .border_thickness(1.0)
            .border_brush(Color::rgb(65, 65, 65))
            .content(StackPanel::new().spacing(16.0).children((
                heading("Your scene"),
                note(self.scene_summary(d)),
                note("TV → Displays → Audio → FPS → Apps & URIs"),
                note(format!("CLI: director scene apply \"{}\"", d.id)),
                if let Err(e) =
                    scenes::resolve(d, &self.scene.displays, &self.scene.audio, &self.scene.tvs)
                {
                    note(e)
                } else {
                    note("Ready to save. Saving applies nothing.")
                },
            )));
        let content: View = if wide {
            Grid::new()
                .columns([GridLength::Star(1.7), GridLength::Star(1.0)])
                .column_spacing(36.0)
                .children((
                    Border::new().content(fields),
                    Border::new()
                        .grid_column(1)
                        .vertical_alignment(VerticalAlignment::Top)
                        .content(summary),
                ))
        } else {
            StackPanel::new().spacing(24.0).children((fields, summary))
        };
        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
            .children((
                crate::preset_ui::header(
                    "Scene editor",
                    "Choose only what you want this scene to change.",
                    vec![],
                ),
                ScrollViewer::new()
                    .grid_row(1)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                    .content(content),
                StackPanel::new()
                    .grid_row(2)
                    .horizontal_alignment(HorizontalAlignment::Right)
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
                    .children((
                        Button::new()
                            .is_enabled(available)
                            .on_click(cx.message(msg(SceneMessage::Cancel)))
                            .content("Cancel"),
                        Button::new()
                            .automation_name("Save scene")
                            .style(ButtonStyle::Accent)
                            .is_enabled(
                                self.scene_ready()
                                    && scenes::resolve(
                                        d,
                                        &self.scene.displays,
                                        &self.scene.audio,
                                        &self.scene.tvs,
                                    )
                                    .is_ok(),
                            )
                            .on_click(cx.message(msg(SceneMessage::Save)))
                            .content("Save scene"),
                    )),
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
