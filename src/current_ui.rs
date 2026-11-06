//! Now: what's active right now. Display and FPS edits are staged and applied together;
//! sound changes (output, volume, mute, microphone) apply as soon as they're made. It never
//! reads or writes saved scenes or presets.
use crate::{
    DirectorApp, Message,
    display_editor_ui::EditorTarget,
    display_ui::{DisplayMessage, DisplayState},
    work,
};
use director::{
    audio::{AudioBackend, Output},
    audio_control::{self, Level, MuteChange},
    display_apply,
    display_editor::{Editor, Options},
    displays::{Layout, Preset},
    windows_audio::{OperationLock, WindowsAudio},
    windows_display_apply::WindowsDisplays,
    windows_display_modes, windows_displays,
};
use windows_reactor::*;
type Displays = Result<(Layout, Vec<Options>), String>;
/// Audio outputs and inputs.
type Devices = Result<(Vec<Output>, Vec<Output>), String>;
/// How long a volume slider must rest before the level is sent to Windows.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(150);
/// A live sound change, applied right away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SoundChange {
    Use { input: bool },
    Volume { input: bool, percent: u8 },
    ToggleMute { input: bool },
}
#[derive(Clone)]
pub(crate) enum CurrentMessage {
    Load,
    Loaded(Displays, Devices),
    Display(DisplayMessage),
    ApplyDisplays,
    DisplaysApplied(display_apply::Report, Displays, Devices),
    /// Pick the default output (false) or microphone (true).
    Device(bool, Option<usize>),
    /// A volume slider moved, for the output (false) or microphone (true).
    Volume(bool, f64),
    /// A slider has rested since change u64; send its level if nothing newer came in.
    VolumeSettled(bool, u64),
    ToggleMute(bool),
    /// A live sound change finished, with the devices read afterwards.
    SoundDone(Result<String, String>, Devices),
    /// Open a new display preset from what Now has staged.
    SaveDisplays,
    /// Open a new scene whose Sound step holds the current output and volume.
    SaveAudio,
}
#[derive(Default)]
pub(crate) struct CurrentState {
    pub display: DisplayState,
    baseline: Option<Layout>,
    outputs: Vec<Output>,
    inputs: Vec<Output>,
    /// The slider positions, which lead the device while a change settles.
    volume: Option<f64>,
    mic_volume: Option<f64>,
    /// Bumped on every slider move, so only the last move of a drag is sent.
    volume_token: u64,
    mic_token: u64,
    /// A live sound change is running.
    sound_pending: bool,
    /// The slider tokens when that change started; a slider moved since then keeps its place.
    tokens_at_start: (u64, u64),
    pub(crate) fps_dirty: bool,
}
impl CurrentState {
    pub fn dirty(&self) -> bool {
        self.fps_dirty || self.display_dirty()
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
    /// The default output or microphone right now.
    fn default_device(&self, input: bool) -> Option<&Output> {
        let list = if input { &self.inputs } else { &self.outputs };
        list.iter().find(|o| o.console && o.active)
    }
    /// The devices a picker offers: the connected ones, plus the default if it isn't.
    fn shown(&self, input: bool) -> Vec<&Output> {
        let list = if input { &self.inputs } else { &self.outputs };
        let current = self.default_device(input).map(|d| d.id.clone());
        list.iter()
            .filter(|o| o.active || current.as_ref() == Some(&o.id))
            .collect()
    }
    /// Take fresh device lists; the sliders follow the devices again.
    fn set_devices(&mut self, outputs: Vec<Output>, inputs: Vec<Output>) {
        self.outputs = outputs;
        self.inputs = inputs;
        self.volume = self
            .default_device(false)
            .and_then(|o| o.volume_percent)
            .map(f64::from);
        self.mic_volume = self
            .default_device(true)
            .and_then(|o| o.volume_percent)
            .map(f64::from);
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
fn devices() -> Devices {
    let mut audio = WindowsAudio::new()?;
    Ok((audio.outputs()?, audio.inputs()?))
}
impl DirectorApp {
    /// Run one live sound change in the background, then reread the devices.
    fn sound_change(
        &mut self,
        change: SoundChange,
        target: Option<String>,
        cx: &ComponentContext<Self>,
    ) {
        if !self.guard_ready || self.current.sound_pending {
            return;
        }
        let state = &mut self.current;
        state.sound_pending = true;
        state.tokens_at_start = (state.volume_token, state.mic_token);
        crate::close_guard::set_busy(true);
        work(cx, move || {
            let result = (|| {
                let _lock = OperationLock::acquire()?;
                let mut audio = WindowsAudio::new()?;
                match change {
                    SoundChange::Use { input } => audio_control::use_device(
                        &mut audio,
                        target.as_deref().unwrap_or_default(),
                        input,
                        &[],
                    ),
                    SoundChange::Volume { input, percent } => {
                        audio_control::set_volume(&mut audio, None, input, Level::Absolute(percent))
                    }
                    SoundChange::ToggleMute { input } => {
                        audio_control::set_mute(&mut audio, None, input, MuteChange::Toggle)
                    }
                }
            })();
            msg(CurrentMessage::SoundDone(result, devices()))
        });
    }
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
            msg(CurrentMessage::Loaded(read_displays(), devices()))
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
                    Ok((outputs, inputs)) => self.current.set_devices(outputs, inputs),
                    Err(e) => {
                        self.current.set_devices(vec![], vec![]);
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
                            devices(),
                        ))
                    });
                }
            }
            CurrentMessage::DisplaysApplied(report, actual, audio) => {
                self.end();
                self.error = !report.succeeded();
                self.status = report.message;
                self.details = report.details.join("\n");
                self.announce(cx);
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
                    Ok((outputs, inputs)) => self.current.set_devices(outputs, inputs),
                    Err(e) => self.details.push_str(&format!(
                        "
Could not read audio outputs: {e}"
                    )),
                }
            }
            CurrentMessage::Device(input, Some(index)) if !self.busy => {
                if let Some(device) = self.current.shown(input).get(index)
                    && self.current.default_device(input).map(|d| &d.id) != Some(&device.id)
                {
                    let id = device.id.clone();
                    self.sound_change(SoundChange::Use { input }, Some(id), cx);
                }
            }
            CurrentMessage::Volume(input, value) if !self.busy && value.is_finite() => {
                let state = &mut self.current;
                let (slider, token) = if input {
                    (&mut state.mic_volume, &mut state.mic_token)
                } else {
                    (&mut state.volume, &mut state.volume_token)
                };
                // Re-rendering moves the slider to the device level too; that isn't an edit.
                // Without a device level (still loading, or no device) there's nothing to change.
                if slider.is_none_or(|old| (old - value).abs() < 0.5) {
                    return;
                }
                *slider = Some(value);
                *token = token.wrapping_add(1);
                let token = *token;
                work(cx, move || {
                    std::thread::sleep(SETTLE);
                    msg(CurrentMessage::VolumeSettled(input, token))
                });
            }
            CurrentMessage::VolumeSettled(input, token) => {
                let state = &self.current;
                let (latest, value) = if input {
                    (state.mic_token, state.mic_volume)
                } else {
                    (state.volume_token, state.volume)
                };
                if token != latest {
                    return;
                }
                if self.busy || state.sound_pending {
                    // Try again once the running change has finished.
                    work(cx, move || {
                        std::thread::sleep(SETTLE);
                        msg(CurrentMessage::VolumeSettled(input, token))
                    });
                    return;
                }
                if let Some(value) = value {
                    let percent = value.round().clamp(0.0, 100.0) as u8;
                    self.sound_change(SoundChange::Volume { input, percent }, None, cx);
                }
            }
            CurrentMessage::ToggleMute(input) if !self.busy && !self.current.sound_pending => {
                self.sound_change(SoundChange::ToggleMute { input }, None, cx);
            }
            CurrentMessage::SoundDone(result, devices) => {
                self.current.sound_pending = false;
                crate::close_guard::set_busy(false);
                match &result {
                    Ok(message) => {
                        self.error = false;
                        self.status = message.clone();
                        self.details.clear();
                    }
                    Err(e) => {
                        self.error = true;
                        self.status = "Could not change the sound.".into();
                        self.details = e.clone();
                    }
                }
                if let Ok((outputs, inputs)) = devices {
                    // Keep a slider that moved again while this change ran where it is.
                    let (volume, mic) = (self.current.volume, self.current.mic_volume);
                    self.current.set_devices(outputs, inputs);
                    let state = &mut self.current;
                    let (output_at_start, mic_at_start) = state.tokens_at_start;
                    if state.volume_token != output_at_start {
                        state.volume = volume.or(state.volume);
                    }
                    if state.mic_token != mic_at_start {
                        state.mic_volume = mic.or(state.mic_volume);
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
            CurrentMessage::SaveAudio if !self.busy => {
                let Some(output) = self.current.default_device(false) else {
                    return;
                };
                let mut scene = director::scenes::Scene::new(crate::scene_ui::new_id(), "");
                scene.sound = Some(director::scenes::Sound {
                    output: Some(director::scenes::SoundOutput::Device {
                        id: output.id.clone(),
                        name: output.name.clone(),
                    }),
                    volume: self
                        .current
                        .volume
                        .filter(|v| v.is_finite())
                        .map(|v| v.round().clamp(0.0, 100.0) as u8),
                    ..Default::default()
                });
                self.update_library(
                    crate::library_flow::LibraryMessage::NewScene(Box::new(scene)),
                    cx,
                );
            }
            _ => {}
        }
    }
    /// The output (false) or microphone (true): device picker, mute button and volume slider.
    fn sound_row(&self, cx: &ViewContext<Self>, input: bool) -> View {
        let state = &self.current;
        let current = state.default_device(input);
        let available = !self.busy && !state.sound_pending && self.guard_ready;
        let muted = current.and_then(|d| d.muted) == Some(true);
        let volume = if input {
            state.mic_volume
        } else {
            state.volume
        };
        let kind = if input { "microphone" } else { "output" };
        let devices = state.shown(input);
        StackPanel::new().spacing(8.0).children((
            ComboBox::new()
                .automation_name(if input {
                    "Default microphone"
                } else {
                    "Default audio output"
                })
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .items_source(devices.iter().map(|o| o.name.clone()).collect::<Vec<_>>())
                .selected_index(current.and_then(|c| devices.iter().position(|o| o.id == c.id)))
                .placeholder_text(if input {
                    "No microphone"
                } else {
                    "No audio output"
                })
                .is_enabled(available)
                .on_selection_changed(cx.callback(move |i: Option<usize>| {
                    // The index is into the connected devices shown, not the full list.
                    msg(CurrentMessage::Device(input, i))
                }))
                .slot(
                    ComboBoxSlot::Header,
                    if input { "Microphone" } else { "Output" },
                ),
            Grid::new()
                .columns([
                    GridLength::Auto,
                    GridLength::Star(1.0),
                    GridLength::Pixel(44.0),
                ])
                .column_spacing(12.0)
                .children((
                    Button::new()
                        .automation_name(format!(
                            "{} {kind}",
                            if muted { "Unmute" } else { "Mute" }
                        ))
                        .is_enabled(available && current.is_some_and(|c| c.muted.is_some()))
                        .on_click(cx.message(msg(CurrentMessage::ToggleMute(input))))
                        .content(
                            StackPanel::new()
                                .orientation(Orientation::Horizontal)
                                .spacing(8.0)
                                .children((
                                    FontIcon::new().glyph(match (input, muted) {
                                        (false, false) => "\u{e767}",
                                        (false, true) => "\u{e74f}",
                                        (true, _) => "\u{e720}",
                                    }),
                                    TextBlock::new().text(if muted { "Muted" } else { "On" }),
                                )),
                        ),
                    Slider::new()
                        .grid_column(1)
                        .automation_name(format!(
                            "{} volume",
                            if input { "Microphone" } else { "Output" }
                        ))
                        .minimum(0.0)
                        .maximum(100.0)
                        .step_frequency(1.0)
                        .value(volume.unwrap_or(0.0))
                        .is_enabled(!self.busy && self.guard_ready && volume.is_some())
                        .vertical_alignment(VerticalAlignment::Center)
                        .on_value_changed(
                            cx.callback(move |v: f64| msg(CurrentMessage::Volume(input, v))),
                        ),
                    TextBlock::new()
                        .grid_column(2)
                        .text(volume.map_or(String::new(), |v| format!("{}%", v.round())))
                        .vertical_alignment(VerticalAlignment::Center),
                )),
        ))
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
        let audio = StackPanel::new().spacing(14.0).children((
            TextBlock::new()
                .text("Sound")
                .font_size(24.0)
                .font_weight(FontWeight::SEMI_BOLD),
            TextBlock::new()
                .text("Changes apply right away. The output takes all three Windows roles.")
                .text_wrapping(TextWrapping::Wrap)
                .opacity(0.72),
            self.sound_row(cx, false),
            self.sound_row(cx, true),
            Button::new()
                .automation_name("Save this sound as a scene")
                .is_enabled(!self.busy && self.current.default_device(false).is_some())
                .on_click(cx.message(msg(CurrentMessage::SaveAudio)))
                .content("Save as scene…")
                .tooltip("Open a new scene whose Sound step uses this output and volume"),
        ));
        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0)])
            .children((
                crate::ui::header(
                    "Now",
                    "Change what’s active right now. Nothing is saved unless you save it.",
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
            muted: None,
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
    fn sliders_follow_the_default_devices_and_pickers_hide_disconnected_ones() {
        let mut state = CurrentState::default();
        let mut gone = output("gone", false);
        gone.active = false;
        let mut mic = output("mic", true);
        mic.volume_percent = Some(80);
        state.set_devices(
            vec![output("tv", false), output("desk", true), gone],
            vec![mic],
        );
        assert_eq!(state.default_device(false).unwrap().id, "desk");
        assert_eq!(state.volume, Some(40.0));
        assert_eq!(state.mic_volume, Some(80.0));
        let shown = state
            .shown(false)
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(shown, ["tv", "desk"]);
        state.set_devices(vec![], vec![]);
        assert!(state.default_device(false).is_none() && state.volume.is_none());
    }
}
