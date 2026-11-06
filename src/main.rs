#![windows_subsystem = "windows"]
mod audio_relink_ui;
mod close_guard;
mod current_ui;
mod display_editor_ui;
mod display_setup_ui;
mod display_ui;
mod fps_ui;
mod icons;
mod library_flow;
mod options_ui;
#[cfg(test)]
mod preset_cards_preview;
mod preset_cards_ui;
mod preset_ui;
mod samsung_ui;
mod scene_ui;
mod status_ui;
mod tv_controls_ui;
mod tv_power_state;
mod tv_tip_ui;
mod tv_ui;
use display_ui::{DisplayMessage, DisplayState};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Current,
    Audio,
    Displays,
    Tvs,
    Options,
    Scenes,
}
use director::{
    audio::{self, Output},
    presets::{self, Preset},
    windows_audio::{OperationLock, WindowsAudio},
};
use windows_reactor::*;

type Loaded = (
    Result<Vec<Preset>, String>,
    Result<Vec<Output>, String>,
    Result<Vec<Output>, String>,
);
#[derive(Clone)]
struct Draft {
    id: Option<String>,
    name: String,
    output_id: String,
    communications_output_id: Option<String>,
    input_id: Option<String>,
    input_set_volume: bool,
    input_volume: Option<f64>,
    set_volume: bool,
    volume: Option<f64>,
}
#[derive(Clone)]
enum Message {
    WindowSize(WindowSize),
    Navigate(Option<String>),
    Display(DisplayMessage),
    Current(current_ui::CurrentMessage),
    Fps(fps_ui::FpsMessage),
    Tv(tv_ui::TvMessage),
    Scene(scene_ui::SceneMessage),
    Status(status_ui::StatusMessage),
    Tip(tv_tip_ui::TipMessage),
    AudioRelink(audio_relink_ui::RelinkMessage),
    OpenLogs,
    Options(options_ui::OptionsMessage),
    LogsOpened(Result<String, String>),
    Refresh,
    Loaded(Loaded),
    GuardReady(Result<(), String>),
    Add,
    Edit(String),
    Cancel,
    Name(String),
    Output(Option<usize>),
    SeparateCommunications(bool),
    CommunicationsOutput(Option<usize>),
    ChangeInput(bool),
    InputDevice(Option<usize>),
    SetInputVolume(bool),
    InputVolume(Option<f64>),
    SetVolume(bool),
    Volume(Option<f64>),
    CopyId,
    DeviceId(bool),
    Pane(bool),
    Save,
    Saved(Result<Vec<Preset>, String>),
    Delete(String),
    DeleteClosed(ContentDialogResult),
    Apply(String),
    Applied(audio::Report, Result<Vec<Output>, String>),
    Error(String),
}
struct DirectorApp {
    window_width: f64,
    page: Page,
    current: current_ui::CurrentState,
    fps: fps_ui::FpsState,
    display: DisplayState,
    tv: tv_ui::TvState,
    scene: scene_ui::SceneState,
    flow: library_flow::LibraryFlow,
    notices: status_ui::StatusState,
    tip: tv_tip_ui::TipState,
    presets: Vec<Preset>,
    outputs: Vec<Output>,
    inputs: Vec<Output>,
    draft: Option<Draft>,
    pane_open: bool,
    options_dialog: Option<options_ui::OptionsDialog>,
    show_device_id: bool,
    pending_delete: Option<String>,
    audio_relink: Option<audio_relink_ui::RelinkDraft>,
    busy: bool,
    store_ready: bool,
    guard_ready: bool,
    guard_error: Option<String>,
    error: bool,
    status: String,
    details: String,
}
fn work(context: &ComponentContext<DirectorApp>, job: impl FnOnce() -> Message + Send + 'static) {
    let _ = context.spawn_background_with_rejection(
        move |_| {
            let _operation = director::diagnostics::Operation::start("app.worker", None);
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).unwrap_or_else(|_| {
                Message::Error(
                    "The background operation stopped unexpectedly. Reopen this page before trying again."
                        .into(),
                )
            })
        },
        Message::Error(
            "The background result could not be delivered. Reopen this page before retrying.".into(),
        ),
    );
}
impl DirectorApp {
    fn begin(&mut self, status: impl Into<String>) {
        self.busy = true;
        close_guard::set_busy(true);
        self.error = false;
        self.notices.clear();
        self.status = status.into();
        director::diagnostics::event("app.operation", "started", &self.status);
        self.details.clear();
    }
    fn end(&mut self) {
        self.busy = false;
        close_guard::set_busy(false);
    }
    fn failure(&mut self, error: String) {
        director::diagnostics::event("app.operation", "error", &error);
        self.end();
        self.error = true;
        self.status = "Could not complete the request.".into();
        self.details = error;
    }
    fn ready(&self) -> bool {
        !self.busy && self.store_ready && self.guard_ready
    }
    fn draft_valid(&self) -> bool {
        self.ready()
            && self.draft.as_ref().is_some_and(|d| {
                !d.name.trim().is_empty()
                    && d.name.trim().len() <= 160
                    && !d.output_id.is_empty()
                    && d.communications_output_id
                        .as_ref()
                        .is_none_or(|id| !id.is_empty())
                    && d.input_id.as_ref().is_none_or(|id| !id.is_empty())
                    && (d.input_id.is_none()
                        || !d.input_set_volume
                        || d.input_volume.is_some_and(|v| {
                            v.is_finite() && (0.0..=100.0).contains(&v) && v.fract() == 0.0
                        }))
                    && (!d.set_volume
                        || d.volume.is_some_and(|v| {
                            v.is_finite() && (0.0..=100.0).contains(&v) && v.fract() == 0.0
                        }))
            })
    }
    fn current(&self) -> String {
        let name = |role| {
            self.outputs
                .iter()
                .find(|o| match role {
                    audio::Role::Console => o.console,
                    audio::Role::Multimedia => o.multimedia,
                    audio::Role::Communications => o.communications,
                })
                .map_or("None", |o| o.name.as_str())
        };
        if self
            .outputs
            .iter()
            .any(|o| o.console && o.multimedia && o.communications)
        {
            name(audio::Role::Console).into()
        } else {
            format!(
                "{} (system) · {} (media) · {} (calls)",
                name(audio::Role::Console),
                name(audio::Role::Multimedia),
                name(audio::Role::Communications)
            )
        }
    }
    fn persist(&mut self, next: Vec<Preset>, context: &ComponentContext<Self>) {
        let expected = self.presets.clone();
        self.begin("Saving presets…");
        work(context, move || {
            Message::Saved((|| {
                let _lock = OperationLock::acquire()?;
                for old in &expected {
                    if !next.iter().any(|p| p.id == old.id) {
                        director::scenes::check_delete("audio", &old.id)?;
                    }
                }
                presets::save(&presets::path()?, &expected, &next)?;
                Ok(next)
            })())
        });
    }
    fn nav_item(&self, page: Page, tag: &str, label: &str, glyph: &str) -> View {
        NavigationViewItem::new()
            .tag(tag)
            .is_selected(self.page == page)
            .selects_on_invoked(!self.busy)
            .slots([
                SlotView::new(NavigationViewItemSlot::Content, label),
                SlotView::new(NavigationViewItemSlot::Icon, FontIcon::new().glyph(glyph)),
            ])
    }
    fn main_view(&self, cx: &ViewContext<Self>) -> View {
        let rows = self.audio_preset_cards(cx);
        let list = if self.presets.is_empty() {
            Border::new().grid_row(2).padding(Thickness::xy(8.0,28.0)).content(StackPanel::new().spacing(12.0).children((
                TextBlock::new().text(if self.busy {"Loading your presets…"}else{"Your first preset starts here"}).font_size(20.0),
                TextBlock::new().text("Choose an output, give it a name, and keep it ready. Saving does not change your audio. You can also save the sound you’ve set in Now.").text_wrapping(TextWrapping::Wrap).max_width(500.0).horizontal_alignment(HorizontalAlignment::Left),
                Button::new().is_enabled(!self.busy).on_click(cx.message(Message::Navigate(Some("current".into())))).content("Open Now"),
            )))
        } else {
            crate::preset_cards_ui::card_grid(2, crate::preset_cards_ui::AUDIO_CARD_HEIGHT, rows)
        };
        Grid::new()
            .rows([GridLength::Auto, GridLength::Auto, GridLength::Star(1.0)])
            .children((
                crate::preset_ui::header(
                    "Audio",
                    "Saved sound setups you can use in scenes.",
                    vec![
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .automation_name("Add preset")
                            .is_enabled(self.ready())
                            .on_click(cx.message(Message::Add))
                            .content(FontIcon::new().glyph(""))
                            .tooltip("Add audio preset"),
                    ],
                ),
                Border::new()
                    .grid_row(1)
                    .padding(16.0)
                    .margin(Thickness::new(0.0, 0.0, 0.0, 16.0))
                    .corner_radius(8.0)
                    .content(
                        StackPanel::new().spacing(4.0).children((
                            TextBlock::new()
                                .text("CURRENT OUTPUT")
                                .font_size(10.0)
                                .opacity(0.65),
                            TextBlock::new()
                                .text(self.current())
                                .text_trimming(TextTrimming::CharacterEllipsis),
                        )),
                    ),
                list,
            ))
    }
    fn editor_view(&self, cx: &ViewContext<Self>, d: &Draft) -> View {
        let index = self.outputs.iter().position(|o| o.id == d.output_id);
        let detail = self
            .outputs
            .iter()
            .find(|o| o.id == d.output_id)
            .map(|o| format!("{} · {}", o.name, o.state))
            .unwrap_or("Choose an output device.".into());
        Grid::new().rows([GridLength::Auto,GridLength::Star(1.0),GridLength::Auto]).children((
            StackPanel::new().grid_row(0).spacing(6.0).margin(Thickness::new(0.0,0.0,0.0,20.0)).children((TextBlock::new().text(if d.id.is_some(){"Edit audio preset"}else{"New audio preset"}).font_size(28.0).font_weight(FontWeight::SEMI_BOLD),TextBlock::new().text("Save your choices. Apply them when you’re ready.").opacity(0.72))),
            ScrollViewer::new().grid_row(1).horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled).content(StackPanel::new().spacing(14.0).max_width(650.0).horizontal_alignment(HorizontalAlignment::Left).children((
                TextBlock::new().text("Preset name").font_weight(FontWeight::SEMI_BOLD),
                TextBox::new().text(d.name.clone()).placeholder_text("e.g. Desk, Headphones, or TV").automation_name("Preset name").is_enabled(!self.busy).on_text_changed(cx.callback(Message::Name)),
                TextBlock::new().text("Output device").font_weight(FontWeight::SEMI_BOLD),
                ComboBox::new().automation_name("Output device").horizontal_alignment(HorizontalAlignment::Stretch).items_source(self.outputs.iter().map(audio_relink_ui::device_label)).selected_index(index).is_enabled(!self.busy).on_selection_changed(cx.callback(Message::Output)),
                TextBlock::new().text(detail).font_size(12.0).opacity(0.72).text_wrapping(TextWrapping::Wrap),
                self.audio_relink_button(cx, &d.output_id, None, false),
                Expander::new().is_expanded(self.show_device_id).on_is_expanded_changed(cx.callback(Message::DeviceId)).slots([SlotView::new(ExpanderSlot::Header,"Device ID"),SlotView::new(ExpanderSlot::Content,StackPanel::new().spacing(10.0).children((TextBlock::new().text(d.output_id.clone()).text_wrapping(TextWrapping::Wrap),Button::new().on_click(cx.message(Message::CopyId)).content("Copy device ID"))))]),
                Grid::new().columns([GridLength::Star(1.0),GridLength::Pixel(130.0)]).children((
                    ToggleSwitch::new().grid_column(0).automation_name("Set volume").is_on(d.set_volume).is_enabled(!self.busy).on_toggled(cx.callback(Message::SetVolume)).slot(ToggleSwitchSlot::Header,"Set volume"),
                    NumberBox::new().grid_column(1).automation_name("Volume percentage").height(36.0).vertical_alignment(VerticalAlignment::Center).minimum(0.0).maximum(100.0).value(d.volume).is_enabled(!self.busy && d.set_volume).on_value_changed(cx.callback(Message::Volume)),
                )),
                TextBlock::new().text("Off keeps the device’s current volume. Use a whole percentage from 0 to 100.").font_size(12.0).opacity(0.72).text_wrapping(TextWrapping::Wrap),
                ToggleSwitch::new().automation_name("Separate communications output").is_on(d.communications_output_id.is_some()).is_enabled(!self.busy).on_toggled(cx.callback(Message::SeparateCommunications)).slot(ToggleSwitchSlot::Header,"Separate communications output"),
                if let Some(id)=&d.communications_output_id {
                    StackPanel::new().spacing(8.0).children((
                        ComboBox::new().automation_name("Communications output device").horizontal_alignment(HorizontalAlignment::Stretch)
                            .items_source(self.outputs.iter().map(audio_relink_ui::device_label))
                            .selected_index(self.outputs.iter().position(|o|&o.id==id)).is_enabled(!self.busy)
                            .on_selection_changed(cx.callback(Message::CommunicationsOutput)).slot(ComboBoxSlot::Header,"Calls output"),
                        self.audio_relink_button(cx, id, None, false),
                    ))
                }else{TextBlock::new().text("Calls use the main output too. All three Windows output roles follow it.").font_size(12.0).opacity(0.72).text_wrapping(TextWrapping::Wrap).into()},
                ToggleSwitch::new().automation_name("Change microphone").is_on(d.input_id.is_some()).is_enabled(!self.busy).on_toggled(cx.callback(Message::ChangeInput)).slot(ToggleSwitchSlot::Header,"Change microphone"),
                if let Some(id)=&d.input_id {
                    StackPanel::new().spacing(12.0).children((
                        ComboBox::new().automation_name("Microphone device").horizontal_alignment(HorizontalAlignment::Stretch)
                            .items_source(self.inputs.iter().map(audio_relink_ui::device_label))
                            .selected_index(self.inputs.iter().position(|o|&o.id==id)).is_enabled(!self.busy)
                            .on_selection_changed(cx.callback(Message::InputDevice)).slot(ComboBoxSlot::Header,"Microphone"),
                        self.audio_relink_button(cx, id, None, true),
                        ToggleSwitch::new().automation_name("Set microphone volume").is_on(d.input_set_volume).is_enabled(!self.busy).on_toggled(cx.callback(Message::SetInputVolume)).slot(ToggleSwitchSlot::Header,"Set microphone volume"),
                        if d.input_set_volume{NumberBox::new().automation_name("Microphone volume percentage").minimum(0.0).maximum(100.0).value(d.input_volume).is_enabled(!self.busy).on_value_changed(cx.callback(Message::InputVolume)).into()}else{View::empty()},
                    ))
                }else{TextBlock::new().text("Microphone and input roles stay unchanged.").font_size(12.0).opacity(0.72).into()},
            ))),
            Grid::new().grid_row(2).margin(Thickness::new(0.0,20.0,0.0,0.0)).columns([GridLength::Star(1.0),GridLength::Auto]).children((
                TextBlock::new().grid_column(0).text("Saving won’t change your audio.").opacity(0.72).vertical_alignment(VerticalAlignment::Center),
                StackPanel::new().grid_column(1).orientation(Orientation::Horizontal).spacing(8.0).children((Button::new().is_enabled(!self.busy).on_click(cx.message(Message::Cancel)).content("Cancel"),Button::new().style(ButtonStyle::Accent).is_enabled(self.draft_valid()).on_click(cx.message(Message::Save)).content("Save preset"))),
            )),
        ))
    }
}
static NOTIFIED_RUN: std::sync::OnceLock<String> = std::sync::OnceLock::new();

impl Component for DirectorApp {
    type Input = ();
    type Message = Message;
    fn create(_: &(), context: &ComponentContext<Self>) -> Self {
        if let Some(id) = NOTIFIED_RUN.get() {
            let _ = context
                .sender()
                .send(Message::Options(options_ui::OptionsMessage::Logs(Some(
                    id.clone(),
                ))));
        } else {
            // The card fixture seeds its preview data from the audio load.
            #[cfg(test)]
            if preset_cards_preview::ENABLED.load(std::sync::atomic::Ordering::Acquire) {
                let _ = context.sender().send(Message::Refresh);
            }
            // Scenes is the start page; Library pages reload themselves on entry.
            let _ = context
                .sender()
                .send(Message::Scene(scene_ui::SceneMessage::Load));
        }
        work(context, tv_tip_ui::load);
        let app = Self {
            page: if NOTIFIED_RUN.get().is_some() {
                Page::Options
            } else {
                Page::Scenes
            },
            current: current_ui::CurrentState::default(),
            fps: fps_ui::FpsState::default(),
            display: DisplayState::default(),
            tv: tv_ui::TvState::default(),
            scene: scene_ui::SceneState::default(),
            flow: library_flow::LibraryFlow::default(),
            notices: status_ui::StatusState::default(),
            tip: tv_tip_ui::TipState::default(),
            presets: vec![],
            outputs: vec![],
            inputs: vec![],
            draft: None,
            window_width: 1000.0,
            pane_open: true,
            show_device_id: false,
            pending_delete: None,
            audio_relink: None,
            options_dialog: None,
            busy: false,
            store_ready: false,
            guard_ready: false,
            guard_error: None,
            error: false,
            status: "Loading…".into(),
            details: String::new(),
        };
        #[cfg(test)]
        let app = if tv_ui::GUIDE_PREVIEW.load(std::sync::atomic::Ordering::Acquire) {
            let mut app = app;
            app.install_tv_guide_preview();
            app
        } else {
            app
        };
        #[cfg(test)]
        let app = if tv_controls_ui::CONTROL_PREVIEW.load(std::sync::atomic::Ordering::Acquire) {
            let mut app = app;
            app.install_tv_controls_preview();
            app
        } else {
            app
        };
        app
    }
    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        #[cfg(test)]
        if preset_cards_preview::ENABLED.load(std::sync::atomic::Ordering::Acquire)
            && self.preview_cards_message(&message)
        {
            return;
        }
        match message {
            Message::WindowSize(size) => self.window_width = size.width,
            Message::AudioRelink(message) => self.update_audio_relink(message, context),
            Message::OpenLogs => self.open_logs(context),
            Message::Options(message) => self.update_options(message, context),
            Message::LogsOpened(result) => {
                director::diagnostics::result("app.logs.open", &result);
                match result {
                    Ok(location) => {
                        self.status = "Log folder opened.".into();
                        self.details = location;
                        self.error = false;
                        self.announce(context);
                    }
                    Err(e) => {
                        self.error = true;
                        self.status = "Could not open logs.".into();
                        self.details = e;
                    }
                }
            }
            Message::Display(message) => self.update_display(message, context),
            Message::Fps(message) => self.update_fps(message, context),
            Message::Current(message) => self.update_current(message, context),
            Message::Scene(message) => self.update_scene(message, context),
            Message::Status(message) => self.update_status(message),
            Message::Tv(message) => self.update_tv(message, context),
            Message::Tip(message) => self.update_tip(message, context),
            Message::Navigate(tag) if !self.busy => {
                let next = if tag.as_deref() == Some("current") {
                    Page::Current
                } else if tag.as_deref() == Some("displays") {
                    Page::Displays
                } else if tag.as_deref() == Some("tvs") {
                    Page::Tvs
                } else if tag.as_deref() == Some("options") {
                    Page::Options
                } else if tag.as_deref() == Some("scenes") {
                    Page::Scenes
                } else if tag.as_deref() == Some("audio") {
                    Page::Audio
                } else {
                    return;
                };
                if self.page == next {
                    return;
                }
                self.page = next;
                self.flow.navigated();
                self.error = false;
                self.status = "Ready.".into();
                self.details.clear();
                if next == Page::Scenes {
                    self.update_scene(scene_ui::SceneMessage::Load, context);
                } else if next == Page::Current && !self.current.dirty() {
                    self.load_current(context);
                } else if next == Page::Displays && self.display.draft.is_none() {
                    self.update_display(DisplayMessage::Refresh, context);
                } else if next == Page::Tvs && self.tv.draft.is_none() {
                    self.update_tv(tv_ui::TvMessage::Load, context);
                } else if next == Page::Audio && self.draft.is_none() {
                    let _ = context.sender().send(Message::Refresh);
                }
            }
            Message::GuardReady(result) => match result {
                Ok(()) => self.guard_ready = true,
                Err(e) => {
                    self.guard_error = Some(e.clone());
                    self.failure(e);
                }
            },
            Message::Refresh if !self.busy => {
                self.begin("Reading presets and Windows audio…");
                work(context, || {
                    Message::Loaded((
                        presets::path().and_then(|p| presets::load(&p)),
                        WindowsAudio::new().and_then(|mut b| b.picker_devices(false)),
                        WindowsAudio::new().and_then(|mut b| b.picker_devices(true)),
                    ))
                });
            }
            Message::Loaded((loaded, outputs, inputs)) => {
                self.end();
                let mut errors = self.guard_error.clone().into_iter().collect::<Vec<_>>();
                match loaded {
                    Ok(p) => {
                        self.presets = p;
                        self.store_ready = true;
                    }
                    Err(e) => {
                        self.store_ready = false;
                        errors.push(e);
                    }
                }
                match outputs {
                    Ok(mut o) => {
                        match director::tvs::path().and_then(|p| director::tvs::load(&p)) {
                            Ok(tvs) => {
                                for tv in tvs {
                                    if let Some(link) = tv.audio
                                        && !o.iter().any(|d| d.id == link.id)
                                    {
                                        o.push(Output {
                                            id: link.id,
                                            name: link.name,
                                            state: "Remembered · unavailable".into(),
                                            active: false,
                                            console: false,
                                            multimedia: false,
                                            communications: false,
                                            volume_percent: None,
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                errors.push(format!("Could not read remembered TV audio: {e}"))
                            }
                        }
                        self.outputs = o;
                    }
                    Err(e) => errors.push(e),
                }
                match inputs {
                    Ok(list) => self.inputs = list,
                    Err(e) => errors.push(format!("Could not read microphone devices: {e}")),
                }
                self.error = !errors.is_empty();
                self.status = if self.error {
                    "Could not refresh everything."
                } else {
                    "Ready."
                }
                .into();
                self.details = errors.join("\n");
                self.library_resume(context);
            }
            Message::Add if self.ready() => {
                self.show_device_id = false;
                let output = self
                    .outputs
                    .iter()
                    .find(|o| o.console && o.active)
                    .or_else(|| self.outputs.iter().find(|o| o.active));
                self.draft = Some(Draft {
                    id: None,
                    name: String::new(),
                    output_id: output.map_or(String::new(), |o| o.id.clone()),
                    communications_output_id: None,
                    input_id: None,
                    input_set_volume: false,
                    input_volume: Some(50.0),
                    set_volume: false,
                    volume: Some(
                        output
                            .and_then(|o| o.volume_percent)
                            .map_or(50.0, f64::from),
                    ),
                });
                self.error = false;
                self.status = "Create an audio preset.".into();
                self.details.clear();
            }
            Message::Edit(id) if self.ready() => {
                self.show_device_id = false;
                if let Some(p) = self.presets.iter().find(|p| p.id == id) {
                    if !self.outputs.iter().any(|o| o.id == p.output_id) {
                        self.outputs.push(Output {
                            communications: false,
                            id: p.output_id.clone(),
                            name: p.output_name.clone(),
                            state: "Unavailable".into(),
                            active: false,
                            console: false,
                            multimedia: false,
                            volume_percent: None,
                        });
                    }
                    for selection in p.communications_output.iter() {
                        if !self.outputs.iter().any(|o| o.id == selection.id) {
                            self.outputs.push(Output {
                                id: selection.id.clone(),
                                name: selection.name.clone(),
                                state: "Remembered · unavailable".into(),
                                active: false,
                                console: false,
                                multimedia: false,
                                communications: false,
                                volume_percent: None,
                            });
                        }
                    }
                    for selection in p.input.iter() {
                        if !self.inputs.iter().any(|o| o.id == selection.id) {
                            self.inputs.push(Output {
                                id: selection.id.clone(),
                                name: selection.name.clone(),
                                state: "Remembered · unavailable".into(),
                                active: false,
                                console: false,
                                multimedia: false,
                                communications: false,
                                volume_percent: None,
                            });
                        }
                    }
                    self.draft = Some(Draft {
                        id: Some(p.id.clone()),
                        name: p.name.clone(),
                        output_id: p.output_id.clone(),
                        communications_output_id: p
                            .communications_output
                            .as_ref()
                            .map(|d| d.id.clone()),
                        input_id: p.input.as_ref().map(|d| d.id.clone()),
                        input_set_volume: p
                            .input
                            .as_ref()
                            .is_some_and(|d| d.volume_percent.is_some()),
                        input_volume: Some(f64::from(
                            p.input
                                .as_ref()
                                .and_then(|d| d.volume_percent)
                                .unwrap_or(50),
                        )),
                        set_volume: p.volume_percent.is_some(),
                        volume: Some(f64::from(p.volume_percent.unwrap_or(50))),
                    });
                    self.error = false;
                    self.status = "Edit your audio preset.".into();
                    self.details.clear();
                }
            }
            Message::Cancel if !self.busy => {
                self.draft = None;
                self.error = false;
                self.status = "Ready.".into();
                self.details.clear();
                self.library_cancelled(library_flow::Slot::Audio, context);
            }
            Message::Name(name) if !self.busy => {
                if let Some(d) = &mut self.draft {
                    d.name = name;
                }
            }
            Message::Output(index) if !self.busy => {
                if let (Some(d), Some(o)) =
                    (&mut self.draft, index.and_then(|i| self.outputs.get(i)))
                {
                    d.output_id = o.id.clone();
                }
            }
            Message::SeparateCommunications(on) if !self.busy => {
                if let Some(d) = &mut self.draft {
                    d.communications_output_id = on.then(|| {
                        self.outputs
                            .iter()
                            .find(|o| o.active && o.communications)
                            .map_or(d.output_id.clone(), |o| o.id.clone())
                    });
                }
            }
            Message::CommunicationsOutput(index) if !self.busy => {
                if let Some(d) = &mut self.draft
                    && d.communications_output_id.is_some()
                {
                    d.communications_output_id = Some(
                        index
                            .and_then(|i| self.outputs.get(i))
                            .map_or(String::new(), |o| o.id.clone()),
                    );
                }
            }
            Message::ChangeInput(on) if !self.busy => {
                if let Some(d) = &mut self.draft {
                    d.input_id = on.then(|| {
                        self.inputs
                            .iter()
                            .find(|o| o.active && o.console)
                            .or_else(|| self.inputs.iter().find(|o| o.active))
                            .map_or(String::new(), |o| o.id.clone())
                    });
                }
            }
            Message::InputDevice(index) if !self.busy => {
                if let Some(d) = &mut self.draft
                    && d.input_id.is_some()
                {
                    d.input_id = Some(
                        index
                            .and_then(|i| self.inputs.get(i))
                            .map_or(String::new(), |o| o.id.clone()),
                    );
                }
            }
            Message::SetInputVolume(on) if !self.busy => {
                if let Some(d) = &mut self.draft {
                    d.input_set_volume = on;
                }
            }
            Message::InputVolume(value) if !self.busy => {
                if let Some(d) = &mut self.draft {
                    d.input_volume = value;
                }
            }
            Message::SetVolume(value) if !self.busy => {
                if let Some(d) = &mut self.draft {
                    d.set_volume = value;
                }
            }
            Message::Volume(value) if !self.busy => {
                if let Some(d) = &mut self.draft {
                    d.volume = value;
                }
            }
            Message::DeviceId(value) => self.show_device_id = value,
            Message::CopyId => {
                if let Some(d) = &self.draft {
                    let result = (|| {
                        use windows::ApplicationModel::DataTransfer::{Clipboard, DataPackage};
                        let data = DataPackage::new()?;
                        data.SetText(&windows::core::HSTRING::from(&d.output_id))?;
                        Clipboard::SetContent(&data)
                    })();
                    if let Err(e) = result {
                        self.failure(format!("Could not copy the device ID: {e}"));
                    }
                }
            }
            Message::Pane(value) => self.pane_open = value,
            Message::Save if self.draft_valid() => {
                let d = self.draft.as_ref().unwrap();
                let Some(o) = self.outputs.iter().find(|o| o.id == d.output_id) else {
                    return;
                };
                let id = d.id.clone().unwrap_or_else(|| {
                    format!(
                        "{}-{}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    )
                });
                let p = Preset {
                    communications_output: d
                        .communications_output_id
                        .as_ref()
                        .and_then(|id| self.outputs.iter().find(|o| &o.id == id))
                        .map(|o| presets::DeviceSelection {
                            id: o.id.clone(),
                            name: o.name.clone(),
                        }),
                    input: d
                        .input_id
                        .as_ref()
                        .and_then(|id| self.inputs.iter().find(|o| &o.id == id))
                        .map(|o| presets::InputSelection {
                            id: o.id.clone(),
                            name: o.name.clone(),
                            volume_percent: if d.input_set_volume {
                                d.input_volume.map(|v| v as u8)
                            } else {
                                None
                            },
                        }),
                    id: id.clone(),
                    name: d.name.trim().into(),
                    output_id: d.output_id.clone(),
                    output_name: o.name.clone(),
                    volume_percent: if d.set_volume {
                        d.volume.map(|v| v as u8)
                    } else {
                        None
                    },
                    keep_changes: false,
                };
                let mut next = self.presets.clone();
                if let Some(existing) = next.iter_mut().find(|p| p.id == id) {
                    *existing = p;
                } else {
                    next.push(p);
                }
                next.sort_by_key(|p| p.name.to_lowercase());
                self.persist(next, context);
            }
            Message::Saved(result) => match result {
                Ok(p) => {
                    self.end();
                    let added = library_flow::added_id(
                        self.presets.iter().map(|p| p.id.as_str()),
                        p.iter().map(|p| p.id.as_str()),
                    );
                    self.presets = p;
                    self.draft = None;
                    self.pending_delete = None;
                    self.status = "Presets saved. Your audio is unchanged.".into();
                    self.announce(context);
                    self.library_saved(library_flow::Slot::Audio, added, context);
                }
                Err(e) => self.failure(e),
            },
            Message::Delete(id) if self.ready() => {
                self.pending_delete = Some(id);
            }
            Message::DeleteClosed(result) if !self.busy => {
                if let Some(id) = self.pending_delete.take()
                    && result == ContentDialogResult::Primary
                {
                    let next = self
                        .presets
                        .iter()
                        .filter(|p| p.id != id)
                        .cloned()
                        .collect();
                    self.persist(next, context);
                }
            }
            Message::Apply(id) if self.ready() => {
                if let Some(p) = self.presets.iter().find(|p| p.id == id).cloned() {
                    self.begin(format!(
                        "Applying “{}”… Please wait before closing.",
                        p.name
                    ));
                    work(context, move || {
                        let result = (|| {
                            let mut backend = WindowsAudio::new()?;
                            let _lock = OperationLock::acquire()?;
                            let report = audio::apply(
                                &mut backend,
                                &p.request(),
                                director::preferences::current().audio_policy(),
                                |_| {},
                            );
                            let outputs = backend.picker_devices(false);
                            Ok((report, outputs))
                        })();
                        match result {
                            Ok((report, outputs)) => Message::Applied(report, outputs),
                            Err(e) => Message::Error(e),
                        }
                    });
                }
            }
            Message::Applied(report, outputs) => {
                self.end();
                self.error = !report.succeeded();
                self.status = report.message;
                self.details = report.details.join("\n");
                self.announce(context);
                match outputs {
                    Ok(o) => self.outputs = o,
                    Err(e) => {
                        self.details
                            .push_str(&format!("\nCould not refresh device status: {e}"));
                    }
                }
            }
            Message::Error(e) => self.failure(e),
            _ => {}
        }
    }
    fn view(&self, _: &(), cx: &mut ViewContext<Self>) -> View {
        cx.window_title("Director");
        let mut visuals =
            WindowVisuals::new()
                .client_size(1000.0, 700.0)
                .constraints(WindowConstraints {
                    min_width: Some(760.0),
                    min_height: Some(600.0),
                    ..Default::default()
                });
        static ICON: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        if let Some(path) = ICON
            .get_or_init(|| {
                std::env::current_exe()
                    .ok()
                    .map(|p| p.with_file_name("director.ico"))
                    .filter(|p| p.is_file())
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .as_deref()
        {
            visuals = visuals.icon(path);
        }
        cx.window_visuals(visuals);
        let sender = cx.sender();
        cx.use_effect("close-protection", (), move || {
            let _ = sender.send(Message::GuardReady(close_guard::install()));
            None
        });
        cx.on_window_size(cx.callback(Message::WindowSize));
        let setup_sender = cx.sender();
        let setup_visible = self.page == Page::Displays && self.display.setup.index.is_some();
        cx.use_effect(
            "display-setup-focus",
            (
                self.page,
                self.display.setup.token,
                self.display.setup.review,
                self.display.setup.stage,
                self.display.setup.index,
                self.busy,
            ),
            move || {
                if setup_visible {
                    let _ = setup_sender.send(Message::Display(display_ui::DisplayMessage::Setup(
                        display_setup_ui::SetupMessage::Focus,
                    )));
                }
                None
            },
        );
        let content = if self.page == Page::Current {
            self.current_view(cx)
        } else if self.page == Page::Displays {
            self.display_view(cx)
        } else if self.page == Page::Tvs {
            self.tv_view(cx)
        } else if self.page == Page::Scenes {
            self.scene_view(cx)
        } else if self.page == Page::Options {
            self.options_view(cx)
        } else if let Some(d) = &self.draft {
            self.editor_view(cx, d)
        } else {
            self.main_view(cx)
        };
        // Keep each page's native scroll/focus tree separate; drafts remain in state.
        let content = Grid::new().keyed_children([KeyedView::new(
            match self.page {
                Page::Current => "current-page",
                Page::Audio => "audio-page",
                Page::Displays => "display-page",
                Page::Tvs => "tv-page",
                Page::Options => "options-page",
                Page::Scenes => "scenes-page",
            },
            content,
        )]);
        let status = self.status_view(cx);
        let body = Border::new()
            .padding(Thickness::new(28.0, 20.0, 28.0, 20.0))
            .content(
                Grid::new()
                    .rows([GridLength::Star(1.0), GridLength::Auto])
                    .row_spacing(if self.status_visible() { 16.0 } else { 0.0 })
                    .children((
                        Border::new().grid_row(0).content(content),
                        Border::new()
                            .grid_row(1)
                            .content(ScrollViewer::new().max_height(160.0).content(status)),
                    )),
            );
        let nav = NavigationView::new()
            .pane_title("Director")
            .open_pane_length(200.0)
            .pane_display_mode(NavigationViewPaneDisplayMode::Left)
            .is_pane_open(self.pane_open)
            .on_is_pane_open_changed(cx.callback(Message::Pane))
            .on_selected_tag_changed(cx.callback(Message::Navigate))
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .is_settings_visible(false)
            .slots([
                SlotView::collection(
                    NavigationViewSlot::MenuItems,
                    [
                        KeyedView::new(
                            "scenes",
                            self.nav_item(Page::Scenes, "scenes", "Scenes", "\u{e768}"),
                        ),
                        KeyedView::new(
                            "current",
                            self.nav_item(Page::Current, "current", "Now", "\u{e9e9}"),
                        ),
                        KeyedView::new(
                            "library",
                            NavigationViewItem::new()
                                .tag("library")
                                .selects_on_invoked(false)
                                .is_expanded(true)
                                .slots([
                                    SlotView::new(NavigationViewItemSlot::Content, "Library"),
                                    SlotView::new(
                                        NavigationViewItemSlot::Icon,
                                        FontIcon::new().glyph("\u{e8f1}"),
                                    ),
                                    SlotView::collection(
                                        NavigationViewItemSlot::MenuItems,
                                        [
                                            KeyedView::new(
                                                "displays",
                                                self.nav_item(
                                                    Page::Displays,
                                                    "displays",
                                                    "Displays",
                                                    "\u{e7f8}",
                                                ),
                                            ),
                                            KeyedView::new(
                                                "audio",
                                                self.nav_item(
                                                    Page::Audio,
                                                    "audio",
                                                    "Audio",
                                                    "\u{e767}",
                                                ),
                                            ),
                                            KeyedView::new(
                                                "tvs",
                                                self.nav_item(Page::Tvs, "tvs", "TVs", "\u{e7f4}"),
                                            ),
                                        ],
                                    ),
                                ]),
                        ),
                    ],
                ),
                SlotView::new(
                    NavigationViewSlot::PaneFooter,
                    Button::new()
                        .automation_name("Options")
                        .style(ButtonStyle::Subtle)
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .is_enabled(!self.busy)
                        .on_click(cx.message(Message::Navigate(Some("options".into()))))
                        .content(
                            StackPanel::new()
                                .orientation(Orientation::Horizontal)
                                .spacing(12.0)
                                .children((
                                    FontIcon::new().glyph("\u{e713}"),
                                    TextBlock::new().text("Options"),
                                )),
                        ),
                ),
                SlotView::new(NavigationViewSlot::Content, body),
            ]);
        let dialog = self
            .pending_delete
            .as_ref()
            .map(|id| {
                let name = self
                    .presets
                    .iter()
                    .find(|p| &p.id == id)
                    .map_or("this preset", |p| p.name.as_str());
                ContentDialog::new()
                    .is_open(true)
                    .title(format!("Delete “{name}”?"))
                    .primary_button_text("Delete preset")
                    .close_button_text("Cancel")
                    .on_closed(cx.callback(Message::DeleteClosed))
                    .content("This removes the saved preset. Your current audio will not change.")
            })
            .unwrap_or_else(View::empty);
        Grid::new().keyed_children([
            KeyedView::new(
                if self.page == Page::Options {
                    "options-nav"
                } else {
                    "device-nav"
                },
                nav,
            ),
            KeyedView::new("audio-dialog", dialog),
            KeyedView::new("options-dialog", self.options_dialog(cx)),
            KeyedView::new("audio-relink-dialog", self.audio_relink_dialog(cx)),
            KeyedView::new("display-dialog", self.display_dialog(cx)),
            KeyedView::new("tv-dialog", self.tv_dialog(cx)),
            KeyedView::new("scene-dialog", self.scene_dialog(cx)),
        ])
    }
}
fn main() {
    director::diagnostics::init();
    director::diagnostics::event("app.session", "started", env!("CARGO_PKG_VERSION"));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run_id = match args.as_slice() {
        [uri] => director::notifications::activation_id(uri),
        _ => None,
    };
    if let Some(id) = run_id {
        let _ = NOTIFIED_RUN.set(id.to_owned());
    } else if !args.is_empty() {
        unsafe {
            let _ = windows::Win32::System::Console::AttachConsole(u32::MAX);
        }
        let code = director::cli::run(&args);
        director::diagnostics::event("app.cli", "exit", &code.to_string());
        director::diagnostics::flush();
        std::process::exit(code);
    }
    if let Err(error) = App::run_component::<DirectorApp>(()) {
        director::diagnostics::event("app.window", "error", &error.to_string());
        director::diagnostics::flush();
        eprintln!("Director could not open: {error}");
        std::process::exit(1);
    }
    director::diagnostics::event("app.session", "closed", "");
    director::diagnostics::flush();
}
