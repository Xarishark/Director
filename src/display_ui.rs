//! Native display capture/list/editor; audio state and device changes stay separate.
use crate::{DirectorApp, Message, work};
use director::{
    display_apply::{self, Decision, Outcome, Report},
    windows_display_apply::WindowsDisplays,
};
use director::{
    displays::{self, Layout, Preset},
    windows_audio::OperationLock,
    windows_displays,
};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use windows_reactor::*;
#[derive(Clone)]
pub(crate) enum DisplayMessage {
    Setup(crate::display_setup_ui::SetupMessage),
    Apply(String),
    Countdown(u64, Option<u64>),
    Confirm(u64, ContentDialogResult),
    Applied(u64, Report, Result<Layout, String>),
    Refresh,
    Loaded(Result<Vec<Preset>, String>, Result<Layout, String>),
    Noop,
    Create,
    PointerDown(usize, PointerEventInfo),
    /// Select a display, from the map or the list of displays that are off.
    Select(usize),
    /// TEMP: comparison. Switch the display editor between its two layouts.
    View(EditorView),
    PointerMove(PointerEventInfo),
    PointerUp(PointerEventInfo),
    CancelDrag,
    CaptureLost,
    CancelLostDrag(u64),
    EditorLoaded(
        bool,
        Result<(Layout, Vec<director::display_editor::Options>), String>,
    ),
    Change(usize, director::display_editor::Change),
    Capture,
    Captured(Result<Layout, String>),
    Edit(String),
    Name(String),
    ConfirmChanges(bool),
    Cancel,
    Save,
    Saved(Result<Vec<Preset>, String>),
    Delete(String),
    DeleteClosed(ContentDialogResult),
}
/// TEMP: comparison. The display editor's two layouts, to pick one after trying both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EditorView {
    /// Windows-style: select a display in the map, and its settings show below.
    #[default]
    One,
    /// A card for every display.
    All,
}
#[derive(Default)]
pub(crate) struct DisplayState {
    pub setup: crate::display_setup_ui::SetupState,
    pub drag_token: u64,
    pub orientation_revision: u64,
    pub drag: Option<director::display_editor::Drag>,
    /// The display selected in the arrangement map.
    pub selected: usize,
    /// TEMP: comparison. The display editor's layout; read from the preset state for both
    /// Now and presets.
    pub view: EditorView,
    pub editor: Option<director::display_editor::Editor>,
    pub progress: Option<Arc<Mutex<mpsc::Receiver<u64>>>>,
    pub confirmation: Option<(u64, mpsc::Sender<Decision>)>,
    pub remaining: u64,
    pub next_token: u64,
    pub presets: Vec<Preset>,
    pub current: Option<Layout>,
    pub draft: Option<Preset>,
    pub store_ready: bool,
    pub pending_delete: Option<String>,
}
fn msg(value: DisplayMessage) -> Message {
    Message::Display(value)
}
fn monitor_details(layout: &Layout) -> View {
    StackPanel::new().spacing(20.0).keyed_children(
        layout
            .monitors
            .iter()
            .enumerate()
            .map(|(i, m)| {
                KeyedView::new(
                    format!("monitor-{i}"),
                    StackPanel::new().spacing(4.0).children((
                        TextBlock::new()
                            .text(&m.name)
                            .font_size(16.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(m.summary())
                            .opacity(crate::ui::SECONDARY)
                            .text_wrapping(TextWrapping::Wrap),
                    )),
                )
            })
            .collect::<Vec<_>>(),
    )
}
impl DirectorApp {
    fn display_tick(&self, token: u64, cx: &ComponentContext<Self>) {
        if let Some(progress) = self.display.progress.clone() {
            work(cx, move || {
                msg(DisplayMessage::Countdown(
                    token,
                    progress
                        .lock()
                        .ok()
                        .and_then(|receiver| receiver.recv().ok()),
                ))
            });
        }
    }
    pub(crate) fn display_ready(&self) -> bool {
        !self.busy
            && self.guard_ready
            && self.display.store_ready
            && self.display.setup.recovery.is_none()
    }
    fn display_persist(&mut self, next: Vec<Preset>, cx: &ComponentContext<Self>) {
        let expected = self.display.presets.clone();
        self.begin("Saving display presets…");
        work(cx, move || {
            msg(DisplayMessage::Saved((|| {
                let _lock = OperationLock::acquire()?;
                for old in &expected {
                    if !next.iter().any(|p| p.id == old.id) {
                        director::scenes::check_delete("display", &old.id)?;
                    }
                }
                displays::save(&displays::path()?, &expected, &next)?;
                Ok(next)
            })()))
        });
    }
    pub(crate) fn update_display(&mut self, message: DisplayMessage, cx: &ComponentContext<Self>) {
        match message {
            DisplayMessage::Setup(message) => self.update_display_setup(message, cx),
            DisplayMessage::Apply(id)
                if self.display_ready()
                    && self.display.draft.is_none()
                    && self.display.pending_delete.is_none() =>
            {
                if let Some(preset) = self.display.presets.iter().find(|p| p.id == id).cloned() {
                    self.display.next_token = self.display.next_token.wrapping_add(1);
                    let token = self.display.next_token;
                    let (tx, rx) = mpsc::channel();
                    self.display.confirmation = preset.confirm_changes.then_some((token, tx));
                    self.display.remaining = 0;
                    self.begin(format!(
                        "Applying “{}”… Please wait before closing.",
                        preset.name
                    ));
                    let (progress_tx, progress_rx) = mpsc::channel();
                    self.display.progress = preset
                        .confirm_changes
                        .then(|| Arc::new(Mutex::new(progress_rx)));
                    work(cx, move || {
                        let result = (|| {
                            let _lock = OperationLock::acquire()?;
                            let mut backend = WindowsDisplays::default();
                            let report = display_apply::apply(&mut backend, &preset.layout, || {
                                !preset.confirm_changes
                                    || display_apply::wait_for_confirmation(
                                        &rx,
                                        Duration::from_secs(15),
                                        |seconds| progress_tx.send(seconds).is_ok(),
                                    )
                            });
                            Ok::<_, String>(report)
                        })();
                        let report = result.unwrap_or_else(|e| Report {
                            outcome: Outcome::Failed,
                            message: "No display changes made.".into(),
                            details: vec![e],
                        });
                        msg(DisplayMessage::Applied(
                            token,
                            report,
                            windows_displays::capture(),
                        ))
                    });
                    self.display_tick(token, cx);
                }
            }
            DisplayMessage::Countdown(token, Some(seconds))
                if self
                    .display
                    .confirmation
                    .as_ref()
                    .is_some_and(|(active, _)| *active == token) =>
            {
                self.display.remaining = seconds;
                self.display_tick(token, cx);
                self.status =
                    format!("Confirm your display settings. Reverting in {seconds} seconds.");
            }
            DisplayMessage::Confirm(token, result)
                if self
                    .display
                    .confirmation
                    .as_ref()
                    .is_some_and(|(active, _)| *active == token) =>
            {
                let (_, sender) = self.display.confirmation.take().unwrap();
                let keep = result == ContentDialogResult::Primary && self.display.remaining > 0;
                let _ = sender.send(if keep {
                    Decision::Keep
                } else {
                    Decision::Revert
                });
                self.display.remaining = 0;
                self.status = if keep {
                    "Keeping display settings…"
                } else {
                    "Restoring previous display settings…"
                }
                .into();
            }
            DisplayMessage::Applied(token, report, current) if token == self.display.next_token => {
                self.end();
                self.display.confirmation = None;
                self.display.progress = None;
                self.display.remaining = 0;
                self.error = matches!(report.outcome, Outcome::Failed | Outcome::RecoveryFailed);
                self.status = report.message;
                self.details = report.details.join("\n");
                self.announce(cx);
                match current {
                    Ok(layout) => self.display.current = Some(layout),
                    Err(e) => {
                        self.display.current = None;
                        self.details
                            .push_str(&format!("\nCould not refresh current displays: {e}"));
                    }
                }
            }
            DisplayMessage::Refresh if !self.busy && self.display.draft.is_none() => {
                self.begin("Reading display presets and the current layout…");
                work(cx, || {
                    msg(DisplayMessage::Loaded(
                        displays::path().and_then(|p| displays::load(&p)),
                        windows_displays::capture(),
                    ))
                });
            }
            DisplayMessage::Loaded(presets, current) => {
                self.end();
                let mut errors = self.guard_error.clone().into_iter().collect::<Vec<_>>();
                match presets {
                    Ok(p) => {
                        self.display.presets = p;
                        self.display.store_ready = true;
                    }
                    Err(e) => {
                        self.display.store_ready = false;
                        errors.push(e);
                    }
                }
                match current {
                    Ok(layout) => self.display.current = Some(layout),
                    Err(e) => {
                        self.display.current = None;
                        errors.push(e);
                    }
                }
                self.error = !errors.is_empty();
                self.status = if self.error {
                    "Could not refresh everything."
                } else {
                    "Ready."
                }
                .into();
                self.details = errors.join("\n");
                self.library_resume(cx);
            }
            DisplayMessage::Create if self.display_ready() && self.display.draft.is_none() => {
                self.begin("Reading monitors and supported display modes…");
                work(cx, || {
                    msg(DisplayMessage::EditorLoaded(
                        true,
                        (|| {
                            let layout = windows_displays::capture()?;
                            let options = director::windows_display_modes::discover(&layout)?;
                            Ok((layout, options))
                        })(),
                    ))
                });
            }
            DisplayMessage::Select(index) => self.display.selected = index,
            DisplayMessage::View(view) => self.display.view = view,
            DisplayMessage::PointerDown(index, point)
                if !self.busy && point.is_left_button_pressed =>
            {
                // A press selects the screen; moving while pressed drags it.
                self.display.selected = index;
                let (width, height) = self.map_size();
                if let Some(d) = &self.display.draft
                    && point.capture_succeeded
                {
                    match director::display_editor::Drag::begin(
                        &d.layout,
                        index,
                        point.window_x,
                        point.window_y,
                        director::display_editor::MapTransform::new(&d.layout, width, height),
                    ) {
                        Ok(drag) => {
                            self.display.drag_token = self.display.drag_token.wrapping_add(1);
                            self.display.drag = Some(drag);
                        }
                        Err(e) => self.failure(e),
                    }
                }
            }
            DisplayMessage::PointerMove(point) if !self.busy => {
                if let (Some(drag), Some(draft)) = (&self.display.drag, &mut self.display.draft)
                    && point.is_left_button_pressed
                    && let Ok(layout) = drag.moved(point.window_x, point.window_y)
                {
                    draft.layout = layout;
                }
            }
            DisplayMessage::PointerUp(point) if !self.busy => {
                if let Some(drag) = self.display.drag.take()
                    && let Some(draft) = &mut self.display.draft
                {
                    let result = drag
                        .moved(point.window_x, point.window_y)
                        .and_then(|layout| drag.finish(&layout));
                    match result {
                        Ok(layout) => {
                            draft.layout = layout;
                            self.error = false;
                            self.details.clear();
                            self.status = "Layout updated. Save when you are ready.".into();
                        }
                        Err(e) => {
                            draft.layout = drag.before;
                            self.failure(e);
                        }
                    }
                }
            }
            DisplayMessage::CaptureLost if self.display.drag.is_some() => {
                let token = self.display.drag_token;
                // Native ReleasePointerCapture can enqueue CaptureLost before
                // PointerReleased. Give that queued release a chance to finish.
                work(cx, move || {
                    std::thread::sleep(Duration::from_millis(25));
                    msg(DisplayMessage::CancelLostDrag(token))
                });
            }
            DisplayMessage::CancelLostDrag(token) if token == self.display.drag_token => {
                self.update_display(DisplayMessage::CancelDrag, cx);
            }
            DisplayMessage::CancelDrag => {
                if let Some(drag) = self.display.drag.take()
                    && let Some(draft) = &mut self.display.draft
                {
                    draft.layout = drag.before;
                }
            }
            DisplayMessage::EditorLoaded(create, result) => {
                self.end();
                self.display.setup = Default::default();
                #[cfg(test)]
                let result = if create
                    && crate::display_setup_ui::DETECTION_PREVIEW
                        .load(std::sync::atomic::Ordering::Acquire)
                {
                    self.tip.tvs = vec![crate::display_setup_ui::preview_tv()];
                    self.tip.catalog_loaded = true;
                    Ok(crate::display_setup_ui::preview_editor())
                } else {
                    result
                };
                match result {
                    Ok((mut layout, options)) => {
                        for (monitor, option) in layout.monitors.iter_mut().zip(&options) {
                            if monitor.hdr.is_none() {
                                monitor.hdr = option.hdr;
                            }
                        }
                        self.display.editor =
                            Some(director::display_editor::Editor::new(&layout, options));
                        if create {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default();
                            self.display.draft = Some(Preset {
                                id: format!("{}-{}", std::process::id(), now.as_nanos()),
                                name: String::new(),
                                captured_at_unix_seconds: now.as_secs(),
                                confirm_changes: false,
                                layout: layout.clone(),
                            });
                        }
                        if !create && let Some(draft) = &mut self.display.draft {
                            draft.layout = layout;
                        }
                        self.status="Configure your layout, then save the preset. Your screens stay unchanged.".into();
                        self.rebase_display();
                    }
                    Err(e) => self.failure(e),
                }
            }
            DisplayMessage::Change(index, change) if !self.busy => {
                if let (Some(editor), Some(draft)) =
                    (&mut self.display.editor, &mut self.display.draft)
                {
                    let orientation =
                        matches!(&change, director::display_editor::Change::Rotation(_));
                    match editor.change(&mut draft.layout, index, change) {
                        Ok(()) => {
                            if orientation {
                                self.display.orientation_revision =
                                    self.display.orientation_revision.wrapping_add(1);
                            }
                            self.error = false;
                            self.details.clear();
                            self.status = "Draft updated. Your screens are unchanged.".into();
                        }
                        Err(e) => self.failure(e),
                    }
                }
            }
            DisplayMessage::Capture if self.display_ready() && self.display.draft.is_none() => {
                self.begin("Capturing your current monitor layout…");
                work(cx, || {
                    msg(DisplayMessage::Captured(windows_displays::capture()))
                });
            }
            DisplayMessage::Captured(result) => match result {
                Ok(layout) => {
                    self.end();
                    self.display.current = Some(layout.clone());
                    self.display.editor = None;
                    self.display.drag = None;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    self.display.draft = Some(Preset {
                        id: format!("{}-{}", std::process::id(), now.as_nanos()),
                        name: String::new(),
                        captured_at_unix_seconds: now.as_secs(),
                        confirm_changes: false,
                        layout,
                    });
                    self.status = "Layout captured. Give it a name to save it.".into();
                }
                Err(e) => self.failure(e),
            },
            DisplayMessage::Edit(id) if self.display_ready() => {
                self.display.editor = None;
                self.display.drag = None;
                self.display.draft = self.display.presets.iter().find(|p| p.id == id).cloned();
                self.error = false;
                self.details.clear();
                if let Some(draft) = &self.display.draft {
                    let layout = draft.layout.clone();
                    self.begin("Reading supported modes for this preset…");
                    work(cx, move || {
                        msg(DisplayMessage::EditorLoaded(
                            false,
                            (|| {
                                let options = director::windows_display_modes::discover(&layout)?;
                                Ok((layout, options))
                            })(),
                        ))
                    });
                }
            }
            DisplayMessage::Name(name) if !self.busy => {
                if let Some(d) = &mut self.display.draft {
                    d.name = name;
                }
            }
            DisplayMessage::ConfirmChanges(value) if !self.busy => {
                if let Some(d) = &mut self.display.draft {
                    d.confirm_changes = value;
                }
            }
            DisplayMessage::Cancel if !self.busy && self.display.setup.recovery.is_none() => {
                self.display.setup = Default::default();
                self.display.draft = None;
                self.display.editor = None;
                self.display.drag = None;
                self.error = false;
                self.status = "Ready.".into();
                self.details.clear();
                self.library_cancelled(crate::library_flow::Slot::Display, cx);
            }
            DisplayMessage::Save if self.display_ready() && self.display.drag.is_none() => {
                if let Some(mut preset) = self.display.draft.clone() {
                    if let Some(editor) = &self.display.editor
                        && let Err(e) = editor.validate(&preset.layout)
                    {
                        self.failure(e);
                        return;
                    }
                    preset.name = preset.name.trim().into();
                    let mut next = self.display.presets.clone();
                    if let Some(old) = next.iter_mut().find(|p| p.id == preset.id) {
                        *old = preset;
                    } else {
                        next.push(preset);
                    }
                    next.sort_by_key(|p| p.name.to_lowercase());
                    self.display_persist(next, cx);
                }
            }
            DisplayMessage::Saved(result) => match result {
                Ok(p) => {
                    self.end();
                    let added = crate::library_flow::added_id(
                        self.display.presets.iter().map(|p| p.id.as_str()),
                        p.iter().map(|p| p.id.as_str()),
                    );
                    self.display.presets = p;
                    self.display.draft = None;
                    self.display.editor = None;
                    self.display.drag = None;
                    self.display.pending_delete = None;
                    self.status = "Display presets saved. Your screens are unchanged.".into();
                    self.guard_saved(true, cx);
                    self.announce(cx);
                    self.library_saved(crate::library_flow::Slot::Display, added, cx);
                }
                Err(e) => self.failure(e),
            },
            DisplayMessage::Delete(id) if self.display_ready() => {
                self.display.pending_delete = Some(id)
            }
            DisplayMessage::DeleteClosed(result) if !self.busy => {
                if let Some(id) = self.display.pending_delete.take()
                    && result == ContentDialogResult::Primary
                {
                    self.display_persist(
                        self.display
                            .presets
                            .iter()
                            .filter(|p| p.id != id)
                            .cloned()
                            .collect(),
                        cx,
                    );
                }
            }
            _ => {}
        }
    }
    pub(crate) fn display_view(&self, cx: &ViewContext<Self>) -> View {
        if let Some(d) = &self.display.draft {
            let editing = self.display.presets.iter().any(|p| p.id == d.id);
            let body = StackPanel::new()
                .spacing(crate::ui::GAP_L)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .children((
                    self.tv_setup_tip(cx, editing),
                    StackPanel::new().spacing(crate::ui::GAP_S).children((
                        crate::ui::label("Preset name"),
                        TextBox::new()
                            .text(&d.name)
                            .automation_name("Display preset name")
                            .width(360.0)
                            .horizontal_alignment(HorizontalAlignment::Left)
                            .placeholder_text("For example, Desk monitors")
                            .is_enabled(!self.busy)
                            .on_text_changed(cx.callback(|name| msg(DisplayMessage::Name(name)))),
                        crate::ui::caption(d.layout.summary()),
                    )),
                    if let Some(editor) = &self.display.editor {
                        self.monitor_editor(&d.layout, editor, cx)
                    } else if self.busy {
                        crate::ui::note("Loading display controls…")
                    } else {
                        monitor_details(&d.layout)
                    },
                    StackPanel::new().spacing(4.0).children((
                        ToggleSwitch::new()
                            .automation_name("Ask to keep display changes")
                            .is_on(d.confirm_changes)
                            .is_enabled(!self.busy)
                            .on_toggled(
                                cx.callback(|value| msg(DisplayMessage::ConfirmChanges(value))),
                            )
                            .slot(ToggleSwitchSlot::Header, "Ask to keep display changes"),
                        crate::ui::caption(
                            "When off, verified changes are kept automatically. Failed changes still revert.",
                        ),
                    )),
                ));
            let can_save = self.display_ready()
                && self.display.drag.is_none()
                && !d.name.trim().is_empty()
                && d.name.trim().len() <= 160
                && self.draft_changed(crate::Page::Displays);
            return crate::ui::page(
                crate::ui::header(
                    if editing {
                        "Edit display preset"
                    } else {
                        "New display preset"
                    },
                    "A saved layout for your scenes. Saving never changes your screens.",
                    vec![],
                ),
                body,
                Some(crate::ui::footer(
                    crate::ui::note(if self.draft_changed(crate::Page::Displays) {
                        "Saving won’t change your screens."
                    } else {
                        "No changes to save."
                    }),
                    vec![
                        Button::new()
                            .is_enabled(!self.busy && self.display.setup.recovery.is_none())
                            .on_click(cx.message(msg(DisplayMessage::Cancel)))
                            .content("Cancel"),
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .automation_name("Save display preset")
                            .is_enabled(can_save)
                            .on_click(cx.message(msg(DisplayMessage::Save)))
                            .content("Save preset"),
                    ],
                )),
            );
        }
        let rows = self.display_preset_cards(cx);
        let list = if rows.is_empty() {
            Border::new()
                .grid_row(1)
                .content(crate::ui::empty_state(
                    if self.busy {
                        "Reading your layouts…"
                    } else {
                        "Keep your monitor setup ready"
                    },
                    "Choose + to build a layout, capture the one you use now, or arrange your screens in Now and save them from there.",
                    Some(
                        Button::new()
                            .is_enabled(!self.busy)
                            .on_click(cx.message(crate::Message::Navigate(Some("current".into()))))
                            .content("Open Now"),
                    ),
                ))
        } else {
            crate::preset_cards_ui::card_grid(1, crate::preset_cards_ui::DISPLAY_CARD_HEIGHT, rows)
        };
        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0)])
            .children((
                crate::ui::header(
                    "Displays",
                    "Saved monitor layouts you can use in scenes.",
                    vec![
                        crate::ui::current(
                            "Current layout",
                            self.display
                                .current
                                .as_ref()
                                .map_or("Current layout unavailable".into(), Layout::summary),
                        ),
                        Button::new()
                            .automation_name("Capture current layout")
                            .is_enabled(self.display_ready())
                            .on_click(cx.message(msg(DisplayMessage::Capture)))
                            .content(FontIcon::new().glyph(""))
                            .tooltip("Capture current layout"),
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .automation_name("Add display preset")
                            .is_enabled(self.display_ready())
                            .on_click(cx.message(msg(DisplayMessage::Create)))
                            .content(FontIcon::new().glyph(""))
                            .tooltip("Add display preset"),
                    ],
                ),
                list,
            ))
    }
    pub(crate) fn display_dialog(&self, cx: &ViewContext<Self>) -> View {
        if let Some((token, _)) = &self.display.confirmation
            && self.display.remaining > 0
        {
            let token = *token;
            return ContentDialog::new().is_open(true).title("Keep these display settings?").primary_button_text("Keep settings").close_button_text("Revert").on_closed(cx.callback(move|result|msg(DisplayMessage::Confirm(token,result)))).content(TextBlock::new().text(format!("Your previous layout will be restored in {} seconds unless you keep these settings.",self.display.remaining)).text_wrapping(TextWrapping::Wrap));
        }
        self.display
            .pending_delete
            .as_ref()
            .map(|id| {
                let name = self
                    .display
                    .presets
                    .iter()
                    .find(|p| &p.id == id)
                    .map_or("this layout", |p| p.name.as_str());
                ContentDialog::new()
                    .is_open(true)
                    .title(format!("Delete “{name}”? "))
                    .primary_button_text("Delete display preset")
                    .close_button_text("Cancel")
                    .on_closed(cx.callback(|r| msg(DisplayMessage::DeleteClosed(r))))
                    .content("This removes the saved display preset. Your screens will not change.")
            })
            .unwrap_or_else(View::empty)
    }
}
