//! Alternative live setup view. It never reads or writes preset documents.
use crate::{
    DirectorApp, Message,
    display_editor_ui::EditorTarget,
    display_ui::{DisplayMessage, DisplayState},
    work,
};
use director::{
    audio::{self, AudioBackend, FailurePolicy, Output, Request},
    display_apply,
    display_editor::{Editor, Options},
    displays::{Layout, Preset},
    windows_audio::{OperationLock, WindowsAudio},
    windows_display_apply::WindowsDisplays,
    windows_display_modes, windows_displays,
};
use windows_reactor::*;
type Displays = Result<(Layout, Vec<Options>), String>;
#[derive(Clone)]
pub(crate) enum CurrentMessage {
    Load,
    Loaded(Displays, Result<Vec<Output>, String>),
    Display(DisplayMessage),
    ApplyDisplays,
    DisplaysApplied(display_apply::Report, Displays, Result<Vec<Output>, String>),
    Output(Option<usize>),
    Volume(Option<f64>),
    ApplyAudio,
    AudioApplied(audio::Report, Result<Vec<Output>, String>),
    /// Open a new Library preset from what Now has staged.
    SaveDisplays,
    SaveAudio,
}
#[derive(Default)]
pub(crate) struct CurrentState {
    pub display: DisplayState,
    baseline: Option<Layout>,
    outputs: Vec<Output>,
    output_id: String,
    volume: Option<f64>,
    volume_dirty: bool,
    audio_dirty: bool,
    pub(crate) fps_dirty: bool,
}
impl CurrentState {
    pub fn dirty(&self) -> bool {
        self.audio_dirty || self.fps_dirty || self.display_dirty()
    }
    fn display_dirty(&self) -> bool {
        self.display
            .draft
            .as_ref()
            .is_some_and(|d| self.baseline.as_ref() != Some(&d.layout))
    }
    fn set_displays(&mut self, layout: Layout, options: Vec<Options>) {
        self.baseline = Some(layout.clone());
        self.display.editor = Some(Editor::new(&layout, options));
        self.display.drag = None;
        self.display.draft = Some(Preset {
            id: "current-setup-session".into(),
            name: "Current setup".into(),
            captured_at_unix_seconds: 0,
            confirm_changes: false,
            layout,
        });
    }
    fn set_outputs(&mut self, outputs: Vec<Output>, reset: bool) {
        self.outputs = outputs;
        if reset {
            self.output_id = self
                .outputs
                .iter()
                .find(|o| o.console && o.active)
                .map_or(String::new(), |o| o.id.clone());
            self.volume = self
                .outputs
                .iter()
                .find(|o| o.id == self.output_id)
                .and_then(|o| o.volume_percent)
                .map(f64::from);
            self.audio_dirty = false;
            self.volume_dirty = false;
        }
    }
}
fn msg(value: CurrentMessage) -> Message {
    Message::Current(value)
}
fn read_displays() -> Displays {
    let layout = windows_displays::capture()?;
    let options = windows_display_modes::discover(&layout)?;
    Ok((layout, options))
}
fn outputs() -> Result<Vec<Output>, String> {
    WindowsAudio::new()?.outputs()
}
impl DirectorApp {
    pub(crate) fn load_current(&mut self, cx: &ComponentContext<Self>) {
        if self.busy {
            return;
        }
        #[cfg(test)]
        if crate::preset_cards_preview::ENABLED.load(std::sync::atomic::Ordering::Acquire) {
            self.fps = crate::fps_ui::FpsState::fixture();
            self.current.fps_dirty = false;
            return;
        }
        self.fps.live = None;
        self.current.fps_dirty = false;
        self.update_fps(crate::fps_ui::FpsMessage::Discover, cx);
        self.begin("Reading your current setup…");
        work(cx, || {
            msg(CurrentMessage::Loaded(read_displays(), outputs()))
        });
    }
    pub(crate) fn update_current(&mut self, event: CurrentMessage, cx: &ComponentContext<Self>) {
        match event {
            CurrentMessage::Load if !self.busy => self.load_current(cx),
            CurrentMessage::Loaded(displays, audio) => {
                self.end();
                let mut errors = vec![];
                match displays {
                    Ok((layout, options)) => self.current.set_displays(layout, options),
                    Err(e) => {
                        self.current.display = DisplayState::default();
                        self.current.baseline = None;
                        errors.push(e);
                    }
                }
                match audio {
                    Ok(list) => self.current.set_outputs(list, true),
                    Err(e) => {
                        self.current.set_outputs(vec![], true);
                        errors.push(e);
                    }
                }
                self.error = !errors.is_empty();
                self.status = if self.error {
                    "Could not read the entire current setup."
                } else {
                    "Now ready."
                }
                .into();
                self.details = errors.join("\n");
            }
            CurrentMessage::Display(DisplayMessage::CaptureLost)
                if !self.busy && self.current.display.drag.is_some() =>
            {
                let token = self.current.display.drag_token;
                work(cx, move || {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    msg(CurrentMessage::Display(DisplayMessage::CancelLostDrag(
                        token,
                    )))
                });
            }
            CurrentMessage::Display(event) if !self.busy => {
                // These editor messages are synchronous and perform no persistence
                // or device operations. Reuse the exact preset gesture/change logic.
                if matches!(
                    event,
                    DisplayMessage::Change(..)
                        | DisplayMessage::PointerDown(..)
                        | DisplayMessage::PointerMove(..)
                        | DisplayMessage::PointerUp(..)
                        | DisplayMessage::CancelDrag
                        | DisplayMessage::CancelLostDrag(..)
                        | DisplayMessage::Noop
                ) {
                    std::mem::swap(&mut self.display, &mut self.current.display);
                    self.update_display(event, cx);
                    std::mem::swap(&mut self.display, &mut self.current.display);
                    if !self.error && self.current.display_dirty() {
                        self.status = "Display changes ready to apply.".into();
                    }
                }
            }
            CurrentMessage::ApplyDisplays
                if !self.busy && self.guard_ready && self.current.display.drag.is_none() =>
            {
                if let (Some(draft), Some(editor)) =
                    (&self.current.display.draft, &self.current.display.editor)
                {
                    if let Err(e) = editor.validate(&draft.layout) {
                        self.failure(e);
                        return;
                    }
                    let layout = draft.layout.clone();
                    self.begin("Applying display changes…");
                    work(cx, move || {
                        let report = match OperationLock::acquire() {
                            Ok(_lock) => display_apply::apply(
                                &mut WindowsDisplays::default(),
                                &layout,
                                || true,
                            ),
                            Err(e) => display_apply::Report {
                                outcome: display_apply::Outcome::Failed,
                                message: "No display changes made.".into(),
                                details: vec![e],
                            },
                        };
                        msg(CurrentMessage::DisplaysApplied(
                            report,
                            read_displays(),
                            outputs(),
                        ))
                    });
                }
            }
            CurrentMessage::DisplaysApplied(report, actual, audio) => {
                self.end();
                self.error = !report.succeeded();
                self.status = report.message;
                self.details = report.details.join("\n");
                match actual {
                    Ok((layout, options)) => self.current.set_displays(layout, options),
                    Err(e) => {
                        self.current.display = DisplayState::default();
                        self.current.baseline = None;
                        self.error = true;
                        self.details.push_str(&format!(
                            "
Could not read the resulting display state: {e}"
                        ));
                    }
                }
                match audio {
                    Ok(list) => {
                        let reset = !self.current.audio_dirty;
                        self.current.set_outputs(list, reset)
                    }
                    Err(e) => self.details.push_str(&format!(
                        "
Could not read audio outputs: {e}"
                    )),
                }
            }
            CurrentMessage::Output(Some(index)) if !self.busy => {
                if let Some(output) = self.current.outputs.get(index)
                    && output.id != self.current.output_id
                {
                    self.current.output_id = output.id.clone();
                    self.current.volume = output.volume_percent.map(f64::from);
                    self.current.volume_dirty = false;
                    self.current.audio_dirty = true;
                }
            }
            CurrentMessage::Volume(value) if !self.busy && value != self.current.volume => {
                self.current.volume = value;
                self.current.volume_dirty = true;
                self.current.audio_dirty = true;
            }
            CurrentMessage::ApplyAudio if !self.busy && self.guard_ready => {
                let active = self
                    .current
                    .outputs
                    .iter()
                    .any(|o| o.id == self.current.output_id && o.active);
                let volume = if self.current.volume_dirty {
                    match self.current.volume {
                        Some(v) if v.is_finite() && (0.0..=100.0).contains(&v) => {
                            Some(v.round() as u8)
                        }
                        _ => {
                            self.failure("Choose a volume between 0 and 100%.".into());
                            return;
                        }
                    }
                } else {
                    None
                };
                if !active {
                    self.failure("Choose an available audio output.".into());
                    return;
                }
                let request = Request {
                    communications_output_id: None,
                    input: None,
                    output_id: self.current.output_id.clone(),
                    volume_percent: volume,
                };
                self.begin("Applying audio changes…");
                work(cx, move || {
                    let result = (|| {
                        let _lock = OperationLock::acquire()?;
                        let mut backend = WindowsAudio::new()?;
                        Ok::<_, String>(audio::apply(
                            &mut backend,
                            &request,
                            FailurePolicy::Revert,
                            |_| {},
                        ))
                    })();
                    let report = result.unwrap_or_else(|e| audio::Report {
                        outcome: audio::Outcome::PreflightFailed,
                        message: "No audio changes made.".into(),
                        details: vec![e],
                    });
                    msg(CurrentMessage::AudioApplied(report, outputs()))
                });
            }
            CurrentMessage::AudioApplied(report, audio) => {
                self.end();
                self.error = !report.succeeded();
                self.status = report.message;
                self.details = report.details.join("\n");
                match audio {
                    Ok(list) => self.current.set_outputs(list, true),
                    Err(e) => {
                        self.error = true;
                        self.details.push_str(&e)
                    }
                }
            }
            CurrentMessage::SaveDisplays if !self.busy && self.current.display.drag.is_none() => {
                // Seed from the staged layout so unapplied edits are saved too.
                let layout = self
                    .current
                    .display
                    .draft
                    .as_ref()
                    .map(|d| d.layout.clone());
                if layout.is_some() {
                    self.update_library(
                        crate::library_flow::LibraryMessage::NewDisplay(layout),
                        cx,
                    );
                }
            }
            CurrentMessage::SaveAudio if !self.busy && !self.current.output_id.is_empty() => {
                let seed = crate::library_flow::AudioSeed {
                    output_id: self.current.output_id.clone(),
                    volume: self.current.volume,
                    set_volume: self.current.volume_dirty,
                };
                self.update_library(
                    crate::library_flow::LibraryMessage::NewAudio(Some(seed)),
                    cx,
                );
            }
            _ => {}
        }
    }
    pub(crate) fn current_view(&self, cx: &ViewContext<Self>) -> View {
        let displays = if let (Some(draft), Some(editor)) =
            (&self.current.display.draft, &self.current.display.editor)
        {
            StackPanel::new().spacing(16.0).children((
                self.monitor_editor_for(&draft.layout, editor, cx, EditorTarget::Current),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .automation_name("Apply current display changes")
                            .is_enabled(
                                !self.busy
                                    && self.guard_ready
                                    && self.current.display_dirty()
                                    && self.current.display.drag.is_none(),
                            )
                            .on_click(cx.message(msg(CurrentMessage::ApplyDisplays)))
                            .content("Apply display changes"),
                        Button::new()
                            .automation_name("Save these displays as a preset")
                            .is_enabled(!self.busy && self.current.display.drag.is_none())
                            .on_click(cx.message(msg(CurrentMessage::SaveDisplays)))
                            .content("Save as display preset…")
                            .tooltip("Open a new display preset with this layout, including edits you haven’t applied"),
                    )),
            ))
        } else {
            TextBlock::new()
                .text(if self.busy {
                    "Reading your displays…"
                } else {
                    "Current display settings are unavailable."
                })
                .text_wrapping(TextWrapping::Wrap)
                .into()
        };
        let selected = self
            .current
            .outputs
            .iter()
            .position(|o| o.id == self.current.output_id);
        let active = selected.is_some_and(|i| self.current.outputs[i].active);
        let audio=StackPanel::new().spacing(12.0).children((
            TextBlock::new().text("Audio").font_size(24.0).font_weight(FontWeight::SEMI_BOLD),
            TextBlock::new().text("Change the default output and its volume. All three output roles follow the chosen output. Microphone settings stay unchanged.").text_wrapping(TextWrapping::Wrap).opacity(0.72),
            ComboBox::new().automation_name("Current audio output").horizontal_alignment(HorizontalAlignment::Stretch)
                .items_source(self.current.outputs.iter().map(|o|format!("{}{}",o.name,if o.active{""}else{" — unavailable"})))
                .selected_index(selected).is_enabled(!self.busy).placeholder_text("Choose an audio output")
                .on_selection_changed(cx.callback(|i|msg(CurrentMessage::Output(i)))).slot(ComboBoxSlot::Header,"Output device"),
            NumberBox::new().automation_name("Current output volume").minimum(0.0).maximum(100.0).value(self.current.volume).is_enabled(!self.busy&&active)
                .on_value_changed(cx.callback(|v|msg(CurrentMessage::Volume(v)))).slot(NumberBoxSlot::Header,"Volume (%)"),
            StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((
                Button::new().style(ButtonStyle::Accent).automation_name("Apply current audio changes")
                    .is_enabled(!self.busy&&self.guard_ready&&self.current.audio_dirty&&active)
                    .on_click(cx.message(msg(CurrentMessage::ApplyAudio))).content("Apply audio changes"),
                Button::new().automation_name("Save this audio as a preset")
                    .is_enabled(!self.busy&&selected.is_some())
                    .on_click(cx.message(msg(CurrentMessage::SaveAudio))).content("Save as audio preset…")
                    .tooltip("Open a new audio preset with this output and volume"),
            )),
        ));
        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0)])
            .children((
                crate::preset_ui::header(
                    "Now",
                    "Change what’s active right now. Nothing is saved unless you save it as a preset.",
                    vec![
                        Button::new()
                            .automation_name("Discard current setup edits")
                            .is_enabled(!self.busy)
                            .on_click(cx.message(msg(CurrentMessage::Load)))
                            .content("Discard edits")
                            .tooltip("Discard pending changes and read the current Windows setup"),
                    ],
                ),
                ScrollViewer::new()
                    .grid_row(1)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                    .content(
                        StackPanel::new()
                            .max_width(650.0)
                            .horizontal_alignment(HorizontalAlignment::Left)
                            .spacing(28.0)
                            .children((
                                TextBlock::new()
                                    .text("Displays")
                                    .font_size(24.0)
                                    .font_weight(FontWeight::SEMI_BOLD),
                                displays,
                                audio,
                                self.fps_controls(cx, crate::fps_ui::Target::Current),
                            )),
                    ),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout() -> Layout {
        use director::displays::{DesktopMode, Monitor, RefreshRate};
        Layout {
            monitors: vec![Monitor {
                adapter_luid: 1,
                target_id: 1,
                device_path: Some("monitor".into()),
                name: "Main".into(),
                connected: true,
                enabled: true,
                hdr: Some(false),
                desktop: Some(DesktopMode {
                    source_adapter_luid: 1,
                    source_id: 0,
                    source_name: "DISPLAY1".into(),
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    source_width: 1920,
                    source_height: 1080,
                    refresh_rate: RefreshRate {
                        numerator: 60,
                        denominator: 1,
                    },
                    rotation_degrees: 0,
                }),
            }],
        }
    }
    fn output(id: &str, default: bool) -> Output {
        Output {
            communications: false,
            id: id.into(),
            name: id.into(),
            state: "active".into(),
            active: true,
            console: default,
            multimedia: default,
            volume_percent: Some(40),
        }
    }
    #[test]
    fn live_edits_keep_an_independent_baseline_and_reset_without_presets() {
        let original = layout();
        let mut state = CurrentState::default();
        state.set_displays(
            original.clone(),
            vec![Options {
                hdr: Some(false),
                ..Default::default()
            }],
        );
        assert!(!state.dirty());
        let draft = state.display.draft.as_mut().unwrap();
        state
            .display
            .editor
            .as_mut()
            .unwrap()
            .change(
                &mut draft.layout,
                0,
                director::display_editor::Change::Hdr(true),
            )
            .unwrap();
        assert!(state.dirty());
        assert_eq!(state.baseline.as_ref(), Some(&original));
        assert_eq!(original.monitors[0].hdr, Some(false));
        state.set_displays(original, vec![Options::default()]);
        assert!(!state.dirty());
    }
    #[test]
    fn audio_discovery_preserves_pending_edits_after_a_display_change() {
        let mut state = CurrentState::default();
        state.set_outputs(vec![output("desk", true)], true);
        state.output_id = "tv".into();
        state.audio_dirty = true;
        state.volume = Some(25.0);
        state.volume_dirty = true;
        state.set_outputs(vec![output("desk", true), output("tv", false)], false);
        assert_eq!(state.output_id, "tv");
        assert_eq!(state.volume, Some(25.0));
        assert!(state.audio_dirty && state.volume_dirty);
        state.set_outputs(vec![output("desk", true), output("tv", false)], true);
        assert_eq!(state.output_id, "desk");
        assert!(!state.audio_dirty && !state.volume_dirty);
    }
}
