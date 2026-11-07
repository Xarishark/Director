//! Now: what's active right now. Display and FPS edits are staged and applied together from
//! the page footer; sound changes (output, volume, mute, microphone) apply as soon as they're made. It never
//! reads or writes saved scenes or presets.
use crate::{
    DirectorApp, Message,
    display_editor_ui::EditorTarget,
    display_ui::{DisplayMessage, DisplayState},
    work,
};
use director::{
    audio::{AudioBackend, Output, Role},
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
/// Which Windows roles an output change sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Roles {
    /// Apps, media and calls.
    All,
    /// Apps and media, leaving a separate calls device as it is.
    Media,
    /// Calls only.
    Calls,
}
/// A live sound change, applied right away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SoundChange {
    Use { input: bool, roles: Roles },
    Volume { input: bool, percent: u8 },
    ToggleMute { input: bool },
}
#[derive(Clone)]
pub(crate) enum CurrentMessage {
    Load,
    Loaded(Displays, Devices),
    Display(DisplayMessage),
    ApplyDisplays,
    /// Apply every staged change: displays first, then the FPS limit.
    ApplyAll,
    DisplaysApplied(display_apply::Report, Displays, Devices),
    /// Pick the default output (false) or microphone (true).
    Device(bool, Option<usize>),
    /// A volume slider moved, for the output (false) or microphone (true).
    Volume(bool, f64),
    /// A slider has rested since change u64; send its level if nothing newer came in.
    VolumeSettled(bool, u64),
    ToggleMute(bool),
    /// Use a separate output for calls (true), or the main output for calls too (false).
    SplitCalls(bool),
    /// Pick the calls output, by its index in the shown outputs.
    CallsDevice(Option<usize>),
    /// A live sound change finished, with the devices read afterwards.
    SoundDone(Result<String, String>, Devices),
    /// Ask for a name, then save what Now has staged as a new display preset.
    SaveDisplays,
    /// The name typed in the save dialog.
    SaveName(String),
    SaveClosed(ContentDialogResult),
    /// The preset was saved (its name), or why not.
    DisplaysSaved(Result<String, String>),
    /// Open a new scene whose Sound step holds the current output and microphone, at their
    /// volume and mute, with the Sound step's dialog open.
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
    /// The calls switch is on, even while calls still use the main output.
    calls_split: bool,
    /// The save-as-preset dialog is open, with the name typed so far.
    save_name: Option<String>,
    /// Why the last save failed, shown in the reopened dialog.
    save_error: Option<String>,
    /// Bumped to open the dialog again: WinUI closes it itself, and an unchanged dialog
    /// isn't reopened.
    save_revision: u64,
    /// The slider tokens when that change started; a slider moved since then keeps its place.
    tokens_at_start: (u64, u64),
    pub(crate) fps_dirty: bool,
    /// Apply the staged FPS limit once the display change running now succeeds.
    fps_after_displays: bool,
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
    /// The output Windows uses for calls (the communications role).
    fn calls_device(&self) -> Option<&Output> {
        self.outputs.iter().find(|o| o.communications && o.active)
    }
    /// Calls use their own output: the switch is on, or Windows already has one set.
    fn calls_separate(&self) -> bool {
        self.calls_split
            || self
                .calls_device()
                .zip(self.default_device(false))
                .is_some_and(|(calls, main)| calls.id != main.id)
    }
    /// The devices a picker offers: the connected ones, plus the default (and, for outputs,
    /// the calls device) if they aren't.
    fn shown(&self, input: bool) -> Vec<&Output> {
        let list = if input { &self.inputs } else { &self.outputs };
        let current = self.default_device(input).map(|d| d.id.clone());
        let calls = (!input)
            .then(|| self.calls_device().map(|d| d.id.clone()))
            .flatten();
        list.iter()
            .filter(|o| {
                o.active || current.as_ref() == Some(&o.id) || calls.as_ref() == Some(&o.id)
            })
            .collect()
    }
    /// A scene Sound step holding what Now shows: the output and microphone with their volume
    /// (the slider, which leads the device while a change settles) and mute state.
    fn sound_now(&self, output: &Output) -> director::scenes::Sound {
        let percent = |v: Option<f64>| {
            v.filter(|v| v.is_finite())
                .map(|v| v.round().clamp(0.0, 100.0) as u8)
        };
        director::scenes::Sound {
            output: Some(director::scenes::SoundOutput::Device {
                id: output.id.clone(),
                name: output.name.clone(),
            }),
            volume: percent(self.volume),
            mute: output.muted,
            calls: self
                .calls_device()
                .filter(|c| self.calls_separate() && c.id != output.id)
                .map(|c| director::audio::DeviceSelection {
                    id: c.id.clone(),
                    name: c.name.clone(),
                }),
            mic: self.default_device(true).map(|mic| director::scenes::Mic {
                device: director::audio::DeviceSelection {
                    id: mic.id.clone(),
                    name: mic.name.clone(),
                },
                volume: percent(self.mic_volume),
                mute: mic.muted,
            }),
        }
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
                    SoundChange::Use { input, roles } => audio_control::use_device(
                        &mut audio,
                        target.as_deref().unwrap_or_default(),
                        input,
                        match roles {
                            Roles::All => &[],
                            Roles::Media => &[Role::Console, Role::Multimedia],
                            Roles::Calls => &[Role::Communications],
                        },
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
            CurrentMessage::ApplyAll
                if !self.busy && self.guard_ready && self.current.display.drag.is_none() =>
            {
                if self.current.display_dirty() {
                    self.current.fps_after_displays = self.current.fps_dirty;
                    self.update_current(CurrentMessage::ApplyDisplays, cx);
                } else if self.current.fps_dirty {
                    self.update_fps(crate::fps_ui::FpsMessage::Apply, cx);
                }
            }
            CurrentMessage::DisplaysApplied(report, actual, audio) => {
                self.end();
                let then_fps = std::mem::take(&mut self.current.fps_after_displays);
                let succeeded = report.succeeded();
                self.error = !succeeded;
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
                // A failed display change leaves the FPS edit staged rather than half-applying.
                if then_fps && succeeded && !self.error {
                    self.update_fps(crate::fps_ui::FpsMessage::Apply, cx);
                }
            }
            CurrentMessage::Device(input, Some(index)) if !self.busy => {
                if let Some(device) = self.current.shown(input).get(index)
                    && self.current.default_device(input).map(|d| &d.id) != Some(&device.id)
                {
                    let id = device.id.clone();
                    // With a separate calls device, the main output leaves calls alone.
                    let roles = if !input && self.current.calls_separate() {
                        Roles::Media
                    } else {
                        Roles::All
                    };
                    self.sound_change(SoundChange::Use { input, roles }, Some(id), cx);
                }
            }
            CurrentMessage::SplitCalls(on) if !self.busy && !self.current.sound_pending => {
                // Re-renders report the switch's current state; only act on a real change.
                if on == self.current.calls_separate() {
                    return;
                }
                self.current.calls_split = on;
                if !on
                    && let Some(main) = self.current.default_device(false)
                    && self.current.calls_device().map(|c| &c.id) != Some(&main.id)
                {
                    let id = main.id.clone();
                    self.sound_change(
                        SoundChange::Use {
                            input: false,
                            roles: Roles::Calls,
                        },
                        Some(id),
                        cx,
                    );
                }
            }
            CurrentMessage::CallsDevice(Some(index)) if !self.busy => {
                if let Some(device) = self.current.shown(false).get(index)
                    && self.current.calls_device().map(|d| &d.id) != Some(&device.id)
                {
                    let id = device.id.clone();
                    self.sound_change(
                        SoundChange::Use {
                            input: false,
                            roles: Roles::Calls,
                        },
                        Some(id),
                        cx,
                    );
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
                if self.current.display.draft.is_some() {
                    self.current.save_name = Some(String::new());
                    self.current.save_error = None;
                    self.current.save_revision += 1;
                }
            }
            CurrentMessage::SaveName(name) => {
                if let Some(current) = &mut self.current.save_name {
                    *current = name;
                }
            }
            CurrentMessage::SaveClosed(result) => {
                let Some(name) = self.current.save_name.clone() else {
                    return;
                };
                let name = name.trim().to_string();
                if result != ContentDialogResult::Primary {
                    self.current.save_name = None;
                    self.current.save_error = None;
                    return;
                }
                // Seed from the staged layout so unapplied edits are saved too.
                let (Some(draft), Some(editor)) =
                    (&self.current.display.draft, &self.current.display.editor)
                else {
                    self.current.save_name = None;
                    return;
                };
                let error = if name.is_empty() {
                    Some("Enter a name for the preset.".to_string())
                } else if name.len() > 160 {
                    Some("Use a name of 160 characters or fewer.".to_string())
                } else {
                    editor.validate(&draft.layout).err()
                };
                if let Some(e) = error {
                    // Reopen the dialog with the reason and the name kept.
                    self.current.save_name = Some(name);
                    self.current.save_error = Some(e);
                    self.current.save_revision += 1;
                    return;
                }
                let layout = draft.layout.clone();
                self.begin("Saving display preset…");
                work(cx, move || {
                    msg(CurrentMessage::DisplaysSaved((|| {
                        let _lock = OperationLock::acquire()?;
                        let path = director::displays::path()?;
                        let existing = director::displays::load(&path)?;
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default();
                        let mut next = existing.clone();
                        next.push(Preset {
                            id: format!("{}-{}", std::process::id(), now.as_nanos()),
                            name: name.clone(),
                            captured_at_unix_seconds: now.as_secs(),
                            confirm_changes: false,
                            layout,
                        });
                        next.sort_by_key(|p| p.name.to_lowercase());
                        // Refuses a name whose command name another preset already uses.
                        director::displays::save(&path, &existing, &next)?;
                        Ok(name)
                    })()))
                });
            }
            CurrentMessage::DisplaysSaved(result) => {
                self.end();
                match result {
                    Ok(name) => {
                        self.current.save_name = None;
                        self.current.save_error = None;
                        self.error = false;
                        self.details.clear();
                        // Show it among the presets; Now keeps its staged edits. The list's
                        // load clears notices, so it shows the confirmation once loaded.
                        self.display.notice_after_load =
                            Some(format!("Saved “{name}”. Your screens are unchanged."));
                        self.navigate_now(Some("displays".into()), cx);
                    }
                    Err(e) => {
                        self.current.save_error = Some(e);
                        self.current.save_revision += 1;
                    }
                }
            }
            CurrentMessage::SaveAudio if !self.busy => {
                let Some(output) = self.current.default_device(false) else {
                    return;
                };
                let mut scene = director::scenes::Scene::new(crate::scene_ui::new_id(), "");
                scene.sound = Some(self.current.sound_now(output));
                self.update_library(
                    crate::library_flow::LibraryMessage::NewScene(Box::new(scene)),
                    cx,
                );
            }
            _ => {}
        }
    }
    /// The optional separate calls output: a switch, and its picker while on.
    fn calls_row(&self, cx: &ViewContext<Self>) -> View {
        let state = &self.current;
        let available = !self.busy && !state.sound_pending && self.guard_ready;
        let on = state.calls_separate();
        let devices = state.shown(false);
        let current = state.calls_device();
        // The picker is always there, beside its switch and greyed out while the switch is
        // off, so turning it on doesn't push anything down the page.
        Grid::new()
            .columns([GridLength::Auto, GridLength::Star(1.0)])
            .column_spacing(crate::ui::GAP_L)
            .children((
                ToggleSwitch::new()
                    .automation_name("Use a different device for calls")
                    .vertical_alignment(VerticalAlignment::Bottom)
                    .is_on(on)
                    .is_enabled(available && state.default_device(false).is_some())
                    .on_toggled(cx.callback(|v| msg(CurrentMessage::SplitCalls(v))))
                    .slot(ToggleSwitchSlot::Header, "Different device for calls"),
                ComboBox::new()
                    .grid_column(1)
                    .automation_name("Calls output")
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .vertical_alignment(VerticalAlignment::Bottom)
                    .items_source(devices.iter().map(|o| o.name.clone()).collect::<Vec<_>>())
                    .selected_index(current.and_then(|c| devices.iter().position(|o| o.id == c.id)))
                    .placeholder_text("Same as the output above")
                    .is_enabled(available && on)
                    .on_selection_changed(cx.callback(|i| msg(CurrentMessage::CallsDevice(i))))
                    .slot(ComboBoxSlot::Header, "Calls output"),
            ))
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
                                        (true, false) => "\u{e720}",
                                        (true, true) => "\u{ec54}",
                                    }),
                                    TextBlock::new().text(if muted { "Unmute" } else { "Mute" }),
                                )),
                        )
                        .tooltip(if muted {
                            format!("The {kind} is muted. Select to unmute.")
                        } else {
                            format!("Mute the {kind}")
                        }),
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
                        .opacity(crate::ui::SECONDARY)
                        .vertical_alignment(VerticalAlignment::Center),
                )),
        ))
    }
    /// What Apply changes would apply, for its tooltip.
    fn staged_summary(&self) -> &'static str {
        match (self.current.display_dirty(), self.current.fps_dirty) {
            (true, true) => "Apply the display and FPS limit changes",
            (true, false) => "Apply the display changes",
            (false, true) => "Apply the FPS limit change",
            (false, false) => "Change a display or the FPS limit first",
        }
    }
    pub(crate) fn current_view(&self, cx: &ViewContext<Self>) -> View {
        use crate::ui;
        let dragging = self.current.display.drag.is_some();
        let dirty = self.current.dirty();
        let can_apply = !self.busy
            && self.guard_ready
            && !dragging
            && (self.current.display_dirty() || self.current.fps_dirty)
            && (!self.current.fps_dirty || self.live_fps_ready());
        // Apply and Discard sit in the map's bottom-right corner, as in Windows Settings.
        // Apply also applies a staged FPS limit, which says so in its card.
        let map_actions = vec![
            Button::new()
                .automation_name("Discard current setup edits")
                .is_enabled(!self.busy && dirty)
                .on_click(cx.message(msg(CurrentMessage::Load)))
                .content("Discard")
                .tooltip("Drop staged changes and read the current Windows setup again"),
            Button::new()
                .style(ButtonStyle::Accent)
                .automation_name("Apply current changes")
                .is_enabled(can_apply)
                .on_click(cx.message(msg(CurrentMessage::ApplyAll)))
                .content("Apply changes")
                .tooltip(self.staged_summary()),
        ];
        let displays = if let (Some(draft), Some(editor)) =
            (&self.current.display.draft, &self.current.display.editor)
        {
            self.monitor_editor_with(
                &draft.layout,
                editor,
                cx,
                EditorTarget::Current,
                map_actions,
            )
        } else if self.busy {
            ui::loading("Reading your displays…")
        } else {
            ui::note("Current display settings are unavailable.")
        };
        let displays = ui::section(
            "Displays",
            "Select a display to find its settings. Drag displays to rearrange them.",
            Some(
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .automation_name("Save these displays as a preset")
                    .is_enabled(!self.busy && !dragging && self.current.display.draft.is_some())
                    .on_click(cx.message(msg(CurrentMessage::SaveDisplays)))
                    .content("Save as display preset…")
                    .tooltip("Open a new display preset with this layout, including edits you haven’t applied"),
            ),
            displays,
        );
        let sound = ui::column(
            ui::FORM_WIDTH,
            0.0,
            (ui::section(
                "Sound",
                "Changes apply as you make them. The output becomes the default for apps and media, and for calls unless you pick a calls device below.",
                Some(
                    Button::new()
                        .style(ButtonStyle::Subtle)
                        .automation_name("Save this sound as a scene")
                        .is_enabled(!self.busy && self.current.default_device(false).is_some())
                        .on_click(cx.message(msg(CurrentMessage::SaveAudio)))
                        .content("Save as scene…")
                        .tooltip("Open a new scene whose Sound step uses this output and volume"),
                ),
                StackPanel::new().spacing(ui::GAP_L).children((
                    self.sound_row(cx, false),
                    self.sound_row(cx, true),
                    self.calls_row(cx),
                )),
            ),),
        );
        // A form-width card: its few short controls don't need the whole window.
        let fps = ui::column(
            ui::FORM_WIDTH,
            0.0,
            (self.fps_controls(cx, crate::fps_ui::Target::Current),),
        );
        // No page title: the sidebar already says Now, and the sections explain themselves.
        ui::page(
            View::empty(),
            StackPanel::new().spacing(ui::GAP_XXL).children((
                // Above the line waits for Apply changes; below it applies at once.
                displays,
                fps,
                ui::divider("Sound changes below apply right away"),
                sound,
            )),
            None,
        )
    }
}

impl DirectorApp {
    /// Now's save-as-preset dialog: just a name. No tooltips inside dialogs.
    pub(crate) fn current_dialog(&self, cx: &ViewContext<Self>) -> View {
        let Some(name) = &self.current.save_name else {
            return View::empty();
        };
        let dialog = ContentDialog::new()
            .is_open(true)
            .title("Save as display preset")
            .primary_button_text("Save")
            .close_button_text("Cancel")
            .is_primary_button_enabled(!self.busy && !name.trim().is_empty())
            .on_closed(cx.callback(|result| msg(CurrentMessage::SaveClosed(result))))
            .content(
                StackPanel::new().spacing(crate::ui::GAP_M).width(360.0).children((
                    TextBox::new()
                        .automation_name("Display preset name")
                        .text(name.clone())
                        .placeholder_text("For example, Desk monitors")
                        .on_text_changed(cx.callback(|v| msg(CurrentMessage::SaveName(v))))
                        .slot(TextBoxSlot::Header, "Name"),
                    crate::ui::caption(
                        "Saves the layout shown here, including changes you haven’t applied. Your screens don’t change.",
                    ),
                    match &self.current.save_error {
                        Some(e) => TextBlock::new()
                            .text(e.clone())
                            .foreground(crate::ui::critical())
                            .text_wrapping(TextWrapping::Wrap)
                            .into(),
                        None => View::empty(),
                    },
                )),
            );
        Grid::new().keyed_children([KeyedView::new(
            format!("save-dialog-{}", self.current.save_revision),
            dialog,
        )])
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
    fn save_as_scene_keeps_the_output_and_microphone_as_shown() {
        let mut state = CurrentState::default();
        let mut speakers = output("speakers", true);
        speakers.muted = Some(false);
        let mut mic = output("mic", true);
        mic.volume_percent = Some(80);
        mic.muted = Some(true);
        state.set_devices(vec![speakers], vec![mic]);
        // A slider that moved but hasn't settled yet still wins.
        state.volume = Some(55.4);
        let sound = state.sound_now(state.default_device(false).unwrap());
        assert_eq!(
            sound.output,
            Some(director::scenes::SoundOutput::Device {
                id: "speakers".into(),
                name: "speakers".into(),
            })
        );
        assert_eq!((sound.volume, sound.mute), (Some(55), Some(false)));
        let mic = sound.mic.unwrap();
        assert_eq!(mic.device.id, "mic");
        assert_eq!((mic.volume, mic.mute), (Some(80), Some(true)));
    }
    #[test]
    fn a_separate_calls_device_is_detected_and_saved_with_the_scene() {
        let mut state = CurrentState::default();
        let mut desk = output("desk", true);
        desk.communications = true;
        state.set_devices(vec![desk, output("headset", false)], vec![]);
        // Calls follow the main output: no separate device, none saved.
        assert!(!state.calls_separate());
        let main = state.default_device(false).unwrap().clone();
        assert!(state.sound_now(&main).calls.is_none());
        // Windows already sends calls to the headset.
        let mut desk = output("desk", true);
        desk.communications = false;
        let mut headset = output("headset", false);
        headset.communications = true;
        state.set_devices(vec![desk, headset], vec![]);
        assert!(state.calls_separate());
        let main = state.default_device(false).unwrap().clone();
        assert_eq!(
            state.sound_now(&main).calls.map(|c| c.id),
            Some("headset".to_string())
        );
        // The switch alone turns the picker on before a different device is chosen.
        let mut desk = output("desk", true);
        desk.communications = true;
        state.set_devices(vec![desk, output("headset", false)], vec![]);
        state.calls_split = true;
        assert!(state.calls_separate());
        let main = state.default_device(false).unwrap().clone();
        assert!(state.sound_now(&main).calls.is_none());
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
