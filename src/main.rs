#![windows_subsystem = "windows"]
mod gui;
use display_ui::{DisplayMessage, DisplayState};
use gui::{
    audio_relink_ui, close_guard, command_ui, crash_ui, display_setup_ui, display_ui, editor_flow,
    fps_ui, nav_guard, now_ui, options_ui, scene_ui, status_ui, tv_tip_ui, tv_ui, update_ui,
};
#[cfg(test)]
use gui::{preset_cards_preview, tv_controls_ui};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Now,
    Displays,
    Tvs,
    Options,
    Scenes,
}
use director::audio::Output;
use windows_reactor::*;

#[derive(Clone)]
enum Message {
    WindowSize(WindowSize),
    Navigate(Option<String>),
    /// Navigate without asking about unsaved changes, for links that come back to a draft.
    Go(String),
    Guard(nav_guard::GuardMessage),
    Command(command_ui::CommandMessage),
    Crash(crash_ui::CrashMessage),
    Display(DisplayMessage),
    Now(now_ui::NowMessage),
    Fps(fps_ui::FpsMessage),
    Tv(tv_ui::TvMessage),
    Scene(scene_ui::SceneMessage),
    Status(status_ui::StatusMessage),
    Tip(tv_tip_ui::TipMessage),
    AudioRelink(audio_relink_ui::RelinkMessage),
    OpenLogs,
    Options(options_ui::OptionsMessage),
    Update(update_ui::UpdateMessage),
    LogsOpened(Result<String, String>),
    GuardReady(Result<(), String>),
    Pane(bool),
    Error(String),
}
struct DirectorApp {
    window_width: f64,
    page: Page,
    now: now_ui::NowState,
    fps: fps_ui::FpsState,
    display: DisplayState,
    tv: tv_ui::TvState,
    scene: scene_ui::SceneState,
    flow: editor_flow::EditorFlow,
    nav: nav_guard::NavGuard,
    /// The scene or preset whose shortcut command is shown.
    command: Option<command_ui::Target>,
    /// A crash report from an earlier run, offered once at startup.
    crash: Option<director::crash::Report>,
    notices: status_ui::StatusState,
    tip: tv_tip_ui::TipState,
    update: update_ui::UpdateState,
    /// Windows audio devices, for TV audio links and relinking.
    outputs: Vec<Output>,
    inputs: Vec<Output>,
    pane_open: bool,
    options_dialog: Option<options_ui::OptionsDialog>,
    audio_relink: Option<audio_relink_ui::RelinkDraft>,
    busy: bool,
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
    /// Switch pages without asking about unsaved changes. Drafts stay open.
    pub(crate) fn navigate_now(&mut self, tag: Option<String>, context: &ComponentContext<Self>) {
        let Some(next) = tag.as_deref().and_then(page_for) else {
            return;
        };
        self.scene.open = None;
        self.scene.snapshot = None;
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
        } else if next == Page::Now && !self.now.dirty() {
            self.load_current(context);
        } else if next == Page::Displays && self.display.draft.is_none() {
            self.update_display(DisplayMessage::Refresh, context);
        } else if next == Page::Tvs && self.tv.draft.is_none() {
            self.update_tv(tv_ui::TvMessage::Load, context);
        }
    }
    fn failure(&mut self, error: String) {
        director::diagnostics::event("app.operation", "error", &error);
        self.guard_save_failed();
        self.end();
        self.error = true;
        self.status = "Could not complete the request.".into();
        self.details = error;
    }
    /// Ready for work that saves settings: nothing running and close protection installed.
    fn ready(&self) -> bool {
        !self.busy && self.guard_ready
    }
    /// Options sits in the pane footer as a button (reactor 0.100 has no footer menu items),
    /// styled like a navigation item, with the same selection pill when it's the open page.
    fn options_item(&self, cx: &ViewContext<Self>) -> View {
        let selected = self.page == Page::Options;
        // The pill and the selected fill sit outside the button, which clips its content.
        Border::new()
            .corner_radius(crate::gui::ui::RADIUS_S)
            .background(if selected {
                Brush::from(crate::gui::ui::SUBTLE_FILL)
            } else {
                Brush::from(Color::transparent())
            })
            .content(
                Grid::new()
                    .columns([GridLength::Pixel(3.0), GridLength::Star(1.0)])
                    .children((
                        Border::new()
                            .height(16.0)
                            .corner_radius(1.5)
                            .vertical_alignment(VerticalAlignment::Center)
                            .background(if selected {
                                Brush::from(ThemeBrush::Accent)
                            } else {
                                Brush::from(Color::transparent())
                            }),
                        Button::new()
                            .grid_column(1)
                            .automation_name("Options")
                            .style(ButtonStyle::Subtle)
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .horizontal_content_alignment(HorizontalAlignment::Left)
                            .is_enabled(!self.busy)
                            .on_click(cx.message(Message::Navigate(Some("options".into()))))
                            .content(
                                StackPanel::new()
                                    .orientation(Orientation::Horizontal)
                                    .spacing(12.0)
                                    .children((
                                        FontIcon::new().glyph("\u{e713}"),
                                        TextBlock::new()
                                            .text("Options")
                                            .vertical_alignment(VerticalAlignment::Center),
                                    )),
                            ),
                    )),
            )
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
}
/// The page a navigation tag names.
fn page_for(tag: &str) -> Option<Page> {
    Some(match tag {
        "current" => Page::Now,
        "displays" => Page::Displays,
        "tvs" => Page::Tvs,
        "options" => Page::Options,
        "scenes" => Page::Scenes,
        _ => return None,
    })
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
            // Scenes is the start page; Displays and TVs reload themselves on entry.
            let _ = context
                .sender()
                .send(Message::Scene(scene_ui::SceneMessage::Load));
        }
        work(context, tv_tip_ui::load);
        // Tests and preview fixtures never contact GitHub.
        #[cfg(not(test))]
        work(context, update_ui::startup);
        let app = Self {
            page: if NOTIFIED_RUN.get().is_some() {
                Page::Options
            } else {
                Page::Scenes
            },
            now: now_ui::NowState::default(),
            fps: fps_ui::FpsState::default(),
            display: DisplayState::default(),
            tv: tv_ui::TvState::default(),
            scene: scene_ui::SceneState::default(),
            flow: editor_flow::EditorFlow::default(),
            nav: nav_guard::NavGuard::default(),
            command: None,
            crash: director::crash::pending(),
            notices: status_ui::StatusState::default(),
            tip: tv_tip_ui::TipState::default(),
            update: update_ui::UpdateState::default(),
            outputs: vec![],
            inputs: vec![],
            window_width: 1000.0,
            pane_open: true,
            audio_relink: None,
            options_dialog: None,
            busy: false,
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
        self.track_unsaved();
        // Track before and after handling: before so a draft's first edit is compared with how
        // it opened, after so a draft that just opened already has its baseline.
        #[cfg(test)]
        let skip = preset_cards_preview::ENABLED.load(std::sync::atomic::Ordering::Acquire)
            && self.preview_cards_message(&message);
        #[cfg(not(test))]
        let skip = false;
        if !skip {
            match message {
                Message::WindowSize(size) => self.window_width = size.width,
                Message::AudioRelink(message) => self.update_audio_relink(message, context),
                Message::OpenLogs => self.open_logs(context),
                Message::Options(message) => self.update_options(message, context),
                Message::Update(message) => self.update_update(message, context),
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
                Message::Now(message) => self.update_now(message, context),
                Message::Scene(message) => self.update_scene(message, context),
                Message::Status(message) => self.update_status(message),
                Message::Tv(message) => self.update_tv(message, context),
                Message::Tip(message) => self.update_tip(message, context),
                Message::Navigate(tag) if !self.busy => self.navigate_guarded(tag, context),
                Message::Go(tag) if !self.busy => self.navigate_now(Some(tag), context),
                Message::Guard(message) => self.update_guard(message, context),
                Message::Command(message) => self.update_command(message, context),
                Message::Crash(message) => self.update_crash(message, context),
                Message::GuardReady(result) => match result {
                    Ok(()) => self.guard_ready = true,
                    Err(e) => {
                        self.guard_error = Some(e.clone());
                        self.failure(e);
                    }
                },
                Message::Pane(value) => self.pane_open = value,
                Message::Error(e) => self.failure(e),
                _ => {}
            }
        }
        self.track_unsaved();
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
        let content = if self.page == Page::Now {
            self.now_view(cx)
        } else if self.page == Page::Displays {
            self.display_view(cx)
        } else if self.page == Page::Tvs {
            self.tv_view(cx)
        } else if self.page == Page::Scenes {
            self.scene_view(cx)
        } else {
            self.options_view(cx)
        };
        // Keep each page's native scroll/focus tree separate; drafts remain in state.
        let content = Grid::new().keyed_children([KeyedView::new(
            match self.page {
                Page::Now => "current-page",
                Page::Displays => "display-page",
                Page::Tvs => "tv-page",
                Page::Options => "options-page",
                Page::Scenes => "scenes-page",
            },
            content,
        )]);
        let status = self.status_view(cx);
        // The update bar sits under the status area on the Scenes start page (Options → About
        // has the same controls), and steps aside while work runs (its own download shows
        // progress in the status area instead).
        let banner = self.page == Page::Scenes
            && self.update.available.is_some()
            && !self.update.dismissed
            && !self.busy;
        let body = Border::new()
            .padding(Thickness::new(
                crate::gui::ui::PAGE_PADDING_X,
                crate::gui::ui::PAGE_PADDING_Y,
                crate::gui::ui::PAGE_PADDING_X,
                crate::gui::ui::PAGE_PADDING_Y,
            ))
            .content(
                Grid::new()
                    .rows([GridLength::Star(1.0), GridLength::Auto, GridLength::Auto])
                    .children((
                        Border::new().grid_row(0).content(content),
                        // Only while something needs attention: an empty ScrollViewer still
                        // takes height and shortens the page above it.
                        if self.status_visible() {
                            Border::new()
                                .grid_row(1)
                                .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
                                .content(ScrollViewer::new().max_height(160.0).content(status))
                        } else {
                            View::empty()
                        },
                        if banner {
                            Border::new()
                                .grid_row(2)
                                .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
                                .content(self.update_banner(cx))
                        } else {
                            View::empty()
                        },
                    )),
            );
        // The window's TitleBar carries the app name and the pane toggle, as in Windows 11
        // Settings, so the pane starts directly with the pages.
        let title_bar = TitleBar::new()
            .preferred_height(WindowTitleBarHeight::Standard)
            .title("Director")
            .is_pane_toggle_button_visible(true)
            .on_pane_toggle_requested(cx.message(Message::Pane(!self.pane_open)));
        let nav = NavigationView::new()
            .grid_row(1)
            .is_pane_toggle_button_visible(false)
            .open_pane_length(crate::gui::ui::PANE_OPEN)
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
                            self.nav_item(Page::Now, "current", "Now", "\u{e9e9}"),
                        ),
                        KeyedView::new(
                            "displays",
                            self.nav_item(Page::Displays, "displays", "Displays", "\u{e7f8}"),
                        ),
                        KeyedView::new("tvs", self.nav_item(Page::Tvs, "tvs", "TVs", "\u{e7f4}")),
                    ],
                ),
                SlotView::new(NavigationViewSlot::PaneFooter, self.options_item(cx)),
                SlotView::new(NavigationViewSlot::Content, body),
            ]);
        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0)])
            .keyed_children([
                KeyedView::new("title-bar", title_bar),
                KeyedView::new(
                    format!(
                        "{}-{}",
                        if self.page == Page::Options {
                            "options-nav"
                        } else {
                            "device-nav"
                        },
                        self.nav.revision
                    ),
                    nav,
                ),
                KeyedView::new("guard-dialog", self.guard_dialog(cx)),
                KeyedView::new("command-dialog", self.command_dialog(cx)),
                KeyedView::new("crash-dialog", self.crash_dialog(cx)),
                KeyedView::new("options-dialog", self.options_dialog(cx)),
                KeyedView::new("audio-relink-dialog", self.audio_relink_dialog(cx)),
                KeyedView::new("display-dialog", self.display_dialog(cx)),
                KeyedView::new("current-dialog", self.current_dialog(cx)),
                KeyedView::new("tv-dialog", self.tv_dialog(cx)),
                KeyedView::new("scene-dialog", self.scene_dialog(cx)),
            ])
    }
}
fn main() {
    director::diagnostics::init();
    director::crash::install();
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
        let _ = director::crash::record("Window error", &error.to_string(), None);
        director::diagnostics::flush();
        eprintln!("Director could not open: {error}");
        std::process::exit(1);
    }
    director::diagnostics::event("app.session", "closed", "");
    director::diagnostics::flush();
}
