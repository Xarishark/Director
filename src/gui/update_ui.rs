//! Updates in the window: the startup check, the "new version" banner, and Options' About
//! controls. Installing downloads setup, verifies it, starts it and closes Director so
//! setup can replace the files.
use crate::{DirectorApp, Message, Page, work};
use director::update::{self, Release};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use windows_reactor::*;

#[derive(Default)]
pub(crate) struct UpdateState {
    /// A newer release found by the last check.
    pub available: Option<Release>,
    /// The banner was closed for this session.
    pub dismissed: bool,
    pub checking: bool,
    /// While setup downloads: bytes so far (shared with the worker), the expected size, and
    /// the count last shown.
    download: Option<(Arc<AtomicU64>, u64, u64)>,
    /// What the last check from Options found, shown under its button.
    pub result: Option<Result<String, String>>,
}

#[derive(Clone)]
pub(crate) enum UpdateMessage {
    /// Ask GitHub; `true` when the user asked from Options.
    Check(bool),
    Checked(bool, Result<Option<Release>, String>),
    Install,
    Started(Result<(), String>),
    ReleaseNotes,
    /// Redraw the download progress.
    Tick,
    Dismiss,
}

fn msg(message: UpdateMessage) -> Message {
    Message::Update(message)
}

/// The startup check, unless Options turned it off.
#[cfg(not(test))]
pub(crate) fn startup() -> Message {
    let result = if director::preferences::current().check_for_updates {
        update::check()
    } else {
        Ok(None)
    };
    msg(UpdateMessage::Checked(false, result))
}

impl DirectorApp {
    pub(crate) fn update_update(&mut self, message: UpdateMessage, cx: &ComponentContext<Self>) {
        match message {
            UpdateMessage::Check(manual) if !self.update.checking => {
                self.update.checking = true;
                self.update.result = None;
                work(cx, move || {
                    msg(UpdateMessage::Checked(manual, update::check()))
                });
            }
            UpdateMessage::Checked(manual, result) => {
                self.update.checking = false;
                // The startup check stays quiet unless it finds something; its errors are
                // only logged.
                let result = match result {
                    Ok(Some(release)) => {
                        let text = format!("Director {} is available.", release.version);
                        if manual {
                            self.update.dismissed = false;
                        }
                        self.update.available = Some(release);
                        Ok(text)
                    }
                    Ok(None) => {
                        self.update.available = None;
                        Ok(format!(
                            "Director {} is up to date.",
                            update::Version::current()
                        ))
                    }
                    Err(e) => Err(e),
                };
                self.update.result = manual.then_some(result);
            }
            UpdateMessage::ReleaseNotes => {
                let page = self.update.available.as_ref().map_or(
                    "https://github.com/Xarishark/Director/releases/latest",
                    |r| r.page.as_str(),
                );
                if let Err(e) = director::crash::open(page) {
                    self.failure(e);
                }
            }
            UpdateMessage::Install if self.ready() => {
                let Some(release) = self.update.available.clone() else {
                    return;
                };
                if !update::installed() {
                    // A copy outside Program Files: setup would install beside it, so let the
                    // user decide from the release page.
                    self.update_update(UpdateMessage::ReleaseNotes, cx);
                    return;
                }
                if self.now.dirty()
                    || [Page::Displays, Page::Tvs, Page::Scenes]
                        .into_iter()
                        .any(|p| self.draft_changed(p))
                {
                    self.failure("Apply or discard your changes before updating Director.".into());
                    return;
                }
                self.begin(format!(
                    "Downloading Director {}. When it’s ready, confirm the Windows prompt; Director closes and reopens after the update.",
                    release.version
                ));
                let received = Arc::new(AtomicU64::new(0));
                self.update.download = Some((received.clone(), release.size, 0));
                work(cx, move || {
                    let reopen = update::installed_exe();
                    msg(UpdateMessage::Started(
                        update::download(&release, Some(&received))
                            .and_then(|setup| update::launch(&setup, reopen.as_deref())),
                    ))
                });
                tick(cx);
            }
            UpdateMessage::Tick => {
                if let Some((received, _, shown)) = &mut self.update.download {
                    *shown = received.load(Ordering::Relaxed);
                    tick(cx);
                }
            }
            UpdateMessage::Started(result) => {
                self.update.download = None;
                self.end();
                match result {
                    Ok(()) => {
                        self.status =
                            "Setup started. Director reopens when the update is done.".into();
                        director::diagnostics::flush();
                        if let Ok(window) = crate::gui::close_guard::window() {
                            use windows::Win32::{
                                Foundation::{LPARAM, WPARAM},
                                UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE},
                            };
                            let _ = unsafe {
                                PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0))
                            };
                        }
                    }
                    Err(e) => self.failure(e),
                }
            }
            UpdateMessage::Dismiss => self.update.dismissed = true,
            _ => {}
        }
    }

    /// Download progress from 0 to 1, while setup downloads.
    pub(crate) fn update_progress(&self) -> Option<f64> {
        self.update
            .download
            .as_ref()
            .map(|(_, total, shown)| (*shown as f64 / (*total).max(1) as f64).min(1.0))
    }

    fn install_label(&self) -> &'static str {
        if update::installed() {
            "Install update"
        } else {
            "Download"
        }
    }

    /// Above every page while a newer version is available, until closed.
    pub(crate) fn update_banner(&self, cx: &ViewContext<Self>) -> View {
        use crate::gui::ui;
        let Some(release) = &self.update.available else {
            return View::empty();
        };
        if self.update.dismissed {
            return View::empty();
        }
        Border::new()
            .padding(Thickness::xy(14.0, 10.0))
            .border_thickness(Thickness::uniform(1.0))
            .border_brush(ui::card_stroke())
            .background(ui::card_fill())
            .corner_radius(ui::RADIUS_L)
            .automation_name("Update available")
            .content(
                Grid::new()
                    .columns([
                        GridLength::Auto,
                        GridLength::Star(1.0),
                        GridLength::Auto,
                        GridLength::Auto,
                    ])
                    .column_spacing(ui::GAP_M)
                    .children((
                        FontIcon::new()
                            .glyph("\u{e896}")
                            .vertical_alignment(VerticalAlignment::Center),
                        StackPanel::new()
                            .grid_column(1)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                ui::label(format!("Director {} is available", release.version)),
                                ui::caption(format!(
                                    "You have {}. Setup replaces it and keeps your settings.",
                                    update::Version::current()
                                )),
                            )),
                        StackPanel::new()
                            .grid_column(2)
                            .orientation(Orientation::Horizontal)
                            .spacing(ui::GAP_S)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                Button::new()
                                    .style(ButtonStyle::Accent)
                                    .is_enabled(self.ready())
                                    .on_click(cx.message(msg(UpdateMessage::Install)))
                                    .content(self.install_label()),
                                Button::new()
                                    .on_click(cx.message(msg(UpdateMessage::ReleaseNotes)))
                                    .content("What’s new"),
                            )),
                        Button::new()
                            .grid_column(3)
                            .style(ButtonStyle::Subtle)
                            .automation_name("Close update notice")
                            .vertical_alignment(VerticalAlignment::Center)
                            .on_click(cx.message(msg(UpdateMessage::Dismiss)))
                            .content(FontIcon::new().glyph("\u{e711}")),
                    )),
            )
    }

    /// Options → About: the automatic check, a manual check and its result.
    pub(crate) fn update_settings(&self, cx: &ViewContext<Self>, editable: bool) -> View {
        use crate::gui::ui;
        let prefs = self.tip.pending.as_ref().or(self.tip.preferences.as_ref());
        let result: View = match &self.update.result {
            _ if self.update.checking => ui::caption("Checking GitHub…"),
            Some(Ok(text)) => ui::caption(text.clone()),
            Some(Err(e)) => TextBlock::new()
                .text(e.clone())
                .foreground(ui::critical())
                .text_wrapping(TextWrapping::Wrap)
                .into(),
            None => View::empty(),
        };
        StackPanel::new()
            .spacing(ui::GAP_S)
            .children((
                ui::setting(
                    "Check for updates when Director opens",
                    "Asks GitHub for the latest release. Nothing is installed until you choose Install update.",
                    ToggleSwitch::new()
                        .automation_name("Check for updates when Director opens")
                        .is_on(prefs.is_some_and(|p| p.check_for_updates))
                        .is_enabled(editable)
                        .on_toggled(cx.callback(|on| {
                            Message::Tip(crate::gui::tv_tip_ui::TipMessage::CheckUpdates(on))
                        })),
                ),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(ui::GAP_S)
                    .children((
                        Button::new()
                            .is_enabled(!self.update.checking && !self.busy)
                            .on_click(cx.message(msg(UpdateMessage::Check(true))))
                            .content("Check for updates"),
                        if self.update.available.is_some() {
                            Button::new()
                                .style(ButtonStyle::Accent)
                                .is_enabled(self.ready())
                                .on_click(cx.message(msg(UpdateMessage::Install)))
                                .content(self.install_label())
                        } else {
                            View::empty()
                        },
                        if self.update.available.is_some() {
                            Button::new()
                                .on_click(cx.message(msg(UpdateMessage::ReleaseNotes)))
                                .content("What’s new")
                        } else {
                            View::empty()
                        },
                    )),
                result,
            ))
    }
}

fn tick(cx: &ComponentContext<DirectorApp>) {
    work(cx, || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        msg(UpdateMessage::Tick)
    });
}
