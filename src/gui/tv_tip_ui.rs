//! Optional TV guidance, without changing a display or contacting a TV.
use crate::{DirectorApp, Message, Page, gui::tv_ui::TvMessage, work};
use director::{
    audio::windows_audio::OperationLock,
    preferences::{self, Preferences},
    tv::tvs::{self, Tv},
};
use windows_reactor::*;
#[derive(Default)]
pub(crate) struct TipState {
    pub preferences: Option<Preferences>,
    pub pending: Option<Preferences>,
    pub dismissed: bool,
    pub error: Option<String>,
    pub tvs: Vec<Tv>,
    pub catalog_loaded: bool,
}
#[derive(Clone)]
pub(crate) enum TipMessage {
    Loaded(Result<Preferences, String>, Result<Vec<Tv>, String>),
    Dismiss,
    Show(bool),
    NotifySceneFailures(bool),
    KeepCompleted(bool),
    DeviceWait(Option<f64>),
    InputGuard(Option<f64>),
    CheckUpdates(bool),
    /// Hide the Scenes page's "make your next scene" tip for good.
    HideNextSceneTip,
    Saved(Result<Preferences, String>),
    AddTv,
}
pub(crate) fn load() -> Message {
    Message::Tip(TipMessage::Loaded(
        preferences::path().and_then(|p| preferences::load(&p)),
        tvs::path().and_then(|p| tvs::load(&p)),
    ))
}
fn msg(value: TipMessage) -> Message {
    Message::Tip(value)
}
impl TipState {
    pub fn show_tips(&self) -> bool {
        !self
            .pending
            .as_ref()
            .or(self.preferences.as_ref())
            .is_some_and(|p| p.hide_tv_setup_tip)
    }
}
impl DirectorApp {
    pub(crate) fn update_tip(&mut self, message: TipMessage, cx: &ComponentContext<Self>) {
        match message {
            TipMessage::Loaded(preferences, tvs) => {
                match preferences {
                    Ok(p) => self.tip.preferences = Some(p),
                    Err(e) => self.tip.error = Some(e),
                }
                if let Ok(tvs) = tvs {
                    self.tip.tvs = tvs;
                    self.tip.catalog_loaded = true;
                }
            }
            TipMessage::Dismiss => self.tip.dismissed = true,
            message @ (TipMessage::Show(_)
            | TipMessage::NotifySceneFailures(_)
            | TipMessage::KeepCompleted(_)
            | TipMessage::CheckUpdates(_)
            | TipMessage::HideNextSceneTip
            | TipMessage::DeviceWait(Some(_))
            | TipMessage::InputGuard(Some(_)))
                if !self.busy && self.tip.pending.is_none() =>
            {
                let Some(expected) = self.tip.preferences.clone() else {
                    return;
                };
                let mut next = expected.clone();
                if let TipMessage::Show(show) = message {
                    next.hide_tv_setup_tip = !show;
                    self.tip.dismissed = !show;
                }
                match message {
                    TipMessage::NotifySceneFailures(enabled) => {
                        next.notify_scene_failures = enabled
                    }
                    TipMessage::KeepCompleted(keep) => next.keep_completed_changes = keep,
                    TipMessage::CheckUpdates(on) => next.check_for_updates = on,
                    TipMessage::HideNextSceneTip => next.hide_next_scene_tip = true,
                    TipMessage::DeviceWait(Some(seconds)) => {
                        next.device_wait_seconds = seconds.round().clamp(1.0, 120.0) as u16
                    }
                    TipMessage::InputGuard(Some(seconds)) => {
                        next.tv_input_guard_seconds = seconds
                            .round()
                            .clamp(0.0, f64::from(director::tv::tv_input_guard::MAX_SECONDS))
                            as u16
                    }
                    _ => {}
                }
                if next == expected {
                    return;
                }
                self.tip.pending = Some(next.clone());
                work(cx, move || {
                    msg(TipMessage::Saved((|| {
                        let _lock = OperationLock::acquire()?;
                        preferences::save(&preferences::path()?, &expected, &next)?;
                        Ok(next)
                    })()))
                });
            }
            TipMessage::Saved(result) => {
                self.tip.pending = None;
                match result {
                    Ok(next) => {
                        self.tip.preferences = Some(next);
                        self.tip.error = None;
                    }
                    Err(e) => {
                        director::diagnostics::event("preferences.save", "error", &e);
                        self.tip.error = Some(e);
                    }
                }
            }
            TipMessage::AddTv if !self.busy => {
                self.page = Page::Tvs;
                if self.tv.draft.is_none() {
                    self.tv.add_after_load = true;
                    self.update_tv(TvMessage::Load, cx);
                }
            }
            _ => {}
        }
    }
    pub(crate) fn tv_setup_tip(&self, cx: &ViewContext<Self>, editing: bool) -> View {
        if !self.tip.catalog_loaded
            || self.tip.preferences.is_none()
            || editing
            || self.tip.dismissed
            || !self.tip.show_tips()
            || !self.tip.tvs.is_empty()
        {
            return View::empty();
        }
        Border::new().padding(Thickness::uniform(14.0))
            .border_thickness(Thickness::uniform(1.0)).border_brush(crate::gui::ui::card_stroke())
            .background(crate::gui::ui::card_fill()).corner_radius(crate::gui::ui::RADIUS_L)
            .automation_name("TV setup tip").content(
            Grid::new().columns([GridLength::Auto,GridLength::Star(1.0),GridLength::Auto]).column_spacing(12.0).children((
                FontIcon::new().glyph("\u{e946}").opacity(crate::gui::ui::SECONDARY).vertical_alignment(VerticalAlignment::Top).margin(Thickness::new(0.0,3.0,0.0,0.0)),
                StackPanel::new().grid_column(1).spacing(8.0).children((
                    TextBlock::new().text("Setting up a TV?").font_size(16.0).font_weight(FontWeight::SEMI_BOLD),
                    TextBlock::new().text("Add your TV for wake and PC-input controls. You can also turn it on and select its input yourself.")
                        .text_wrapping(TextWrapping::Wrap).opacity(0.72),
                    StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((
                        Button::new().automation_name("Add TV from display preset").is_enabled(!self.busy).on_click(cx.message(msg(TipMessage::AddTv))).content("Add TV"),
                        Button::new().style(ButtonStyle::Subtle).automation_name("Don't show TV setup tip again")
                            .is_enabled(!self.busy && self.tip.preferences.is_some() && self.tip.pending.is_none())
                            .on_click(cx.message(msg(TipMessage::Show(false)))).content("Don't show again"),
                    )),
                )),
                Button::new().grid_column(2).style(ButtonStyle::Subtle).automation_name("Dismiss TV setup tip")
                    .vertical_alignment(VerticalAlignment::Top).is_enabled(!self.busy)
                    .on_click(cx.message(msg(TipMessage::Dismiss))).content(FontIcon::new().glyph("\u{e711}")),
            )))
    }
}
