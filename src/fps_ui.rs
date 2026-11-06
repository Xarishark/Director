//! Shared native FPS controls for scenes and direct current-setup changes.
use crate::{DirectorApp, Message, work};
#[cfg(test)]
use director::fps::Provider;
use director::fps::{Capability, Request};
use windows_reactor::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Scene,
    Current,
}
#[derive(Clone)]
pub(crate) enum FpsMessage {
    Discover,
    Discovered(u64, Vec<Capability>),
    Enable(Target, bool),
    Method(Target, Option<usize>),
    Mode(Target, Option<usize>),
    Value(Target, Option<f64>),
    Apply,
    Applied(Result<String, String>, Vec<Capability>),
}
#[derive(Default)]
pub(crate) struct FpsState {
    pub caps: Vec<Capability>,
    loaded: bool,
    checking: bool,
    revision: u64,
    pub live: Option<Request>,
}
#[cfg(test)]
impl FpsState {
    pub(crate) fn fixture() -> Self {
        Self {
            caps: [Provider::Nvidia, Provider::Rtss]
                .into_iter()
                .map(|provider| Capability {
                    provider,
                    available: true,
                    maximum: if provider == Provider::Nvidia {
                        1023
                    } else {
                        1000
                    },
                    current: Some(director::fps::Snapshot {
                        provider,
                        value: 120,
                        inherited: false,
                        limiter_disabled: if provider == Provider::Rtss {
                            Some(false)
                        } else {
                            None
                        },
                    }),
                    detail: "Fixture only; no real FPS changes.".into(),
                })
                .collect(),
            loaded: true,
            checking: false,
            revision: 0,
            live: None,
        }
    }
}
fn msg(m: FpsMessage) -> Message {
    Message::Fps(m)
}
fn note(text: impl Into<String>) -> View {
    TextBlock::new()
        .text(text)
        .text_wrapping(TextWrapping::Wrap)
        .opacity(0.72)
        .into()
}
impl DirectorApp {
    fn fps_request(&self, target: Target) -> Option<&Request> {
        match target {
            Target::Scene => self.scene.draft.as_ref().and_then(|d| d.fps.as_ref()),
            Target::Current => self.fps.live.as_ref(),
        }
    }
    fn fps_request_mut(&mut self, target: Target) -> Option<&mut Request> {
        match target {
            Target::Scene => self.scene.draft.as_mut().and_then(|d| d.fps.as_mut()),
            Target::Current => self.fps.live.as_mut(),
        }
    }
    fn fps_edited(&mut self, target: Target) {
        if target == Target::Current {
            self.current.fps_dirty = self.fps.live.is_some();
        }
    }
    pub(crate) fn update_fps(&mut self, m: FpsMessage, cx: &ComponentContext<Self>) {
        match m {
            FpsMessage::Discover if !self.fps.checking => {
                #[cfg(test)]
                if crate::preset_cards_preview::ENABLED.load(std::sync::atomic::Ordering::Acquire) {
                    self.fps.caps = [Provider::Nvidia, Provider::Rtss]
                        .into_iter()
                        .map(|provider| Capability {
                            provider,
                            available: true,
                            maximum: if provider == Provider::Nvidia {
                                1023
                            } else {
                                1000
                            },
                            current: Some(director::fps::Snapshot {
                                provider,
                                value: 120,
                                inherited: false,
                                limiter_disabled: if provider == Provider::Rtss {
                                    Some(false)
                                } else {
                                    None
                                },
                            }),
                            detail: "Fixture only; no real FPS changes.".into(),
                        })
                        .collect();
                    self.fps.loaded = true;
                    return;
                }
                self.fps.checking = true;
                self.fps.revision = self.fps.revision.wrapping_add(1);
                let revision = self.fps.revision;
                work(cx, move || {
                    msg(FpsMessage::Discovered(
                        revision,
                        director::windows_fps::survey(),
                    ))
                });
            }
            FpsMessage::Discovered(revision, caps) if revision == self.fps.revision => {
                self.fps.caps = caps;
                self.fps.loaded = true;
                self.fps.checking = false;
            }
            FpsMessage::Enable(target, on) if !self.busy => {
                let request = if on {
                    self.fps.caps.iter().find(|c| c.available).map(|c| Request {
                        provider: c.provider,
                        limit: Some(
                            c.current
                                .as_ref()
                                .map_or(60, |s| if s.value == 0 { 60 } else { s.value })
                                .clamp(1, c.maximum),
                        ),
                    })
                } else {
                    None
                };
                match target {
                    Target::Current => self.fps.live = request,
                    Target::Scene => {
                        if let Some(d) = &mut self.scene.draft {
                            d.fps = request
                        }
                    }
                }
                self.fps_edited(target);
            }
            FpsMessage::Method(target, Some(index)) if !self.busy => {
                if let Some(cap) = self
                    .fps
                    .caps
                    .iter()
                    .filter(|c| c.available)
                    .nth(index)
                    .cloned()
                    && let Some(r) = self.fps_request_mut(target)
                {
                    r.provider = cap.provider;
                    if let Some(n) = r.limit {
                        r.limit = Some(n.clamp(1, cap.maximum));
                    }
                    self.fps_edited(target);
                }
            }
            FpsMessage::Mode(target, Some(index)) if !self.busy => {
                if let Some(r) = self.fps_request_mut(target) {
                    r.limit = if index == 1 {
                        None
                    } else {
                        Some(r.limit.unwrap_or(60))
                    };
                    self.fps_edited(target);
                }
            }
            FpsMessage::Value(target, value) if !self.busy => {
                if let Some(r) = self.fps_request_mut(target)
                    && r.limit.is_some()
                {
                    let value = value.unwrap_or(f64::NAN);
                    r.limit = if value.is_finite()
                        && value >= 1.0
                        && value.fract() == 0.0
                        && value <= u32::MAX as f64
                    {
                        Some(value as u32)
                    } else {
                        Some(0)
                    };
                    self.fps_edited(target);
                }
            }
            FpsMessage::Apply if !self.busy && self.guard_ready => {
                if let Some(request) = self.fps.live.clone() {
                    if let Err(e) = request.validate() {
                        self.failure(e);
                        return;
                    }
                    #[cfg(test)]
                    if crate::preset_cards_preview::ENABLED
                        .load(std::sync::atomic::Ordering::Acquire)
                    {
                        for cap in &mut self.fps.caps {
                            if cap.provider == request.provider {
                                cap.current = Some(director::fps::Snapshot {
                                    provider: request.provider,
                                    value: request.limit.unwrap_or(0),
                                    inherited: false,
                                    limiter_disabled: if request.provider == Provider::Rtss {
                                        Some(false)
                                    } else {
                                        None
                                    },
                                })
                            }
                        }
                        self.current.fps_dirty = false;
                        self.status = "Fixture FPS applied. No real settings changed.".into();
                        self.announce(cx);
                        return;
                    }
                    self.fps.revision = self.fps.revision.wrapping_add(1);
                    self.fps.checking = false;
                    self.begin("Applying FPS limit…");
                    work(cx, move || {
                        let result = (|| {
                            let _lock = director::windows_audio::OperationLock::acquire()?;
                            director::windows_fps::apply(&request, false)
                        })();
                        msg(FpsMessage::Applied(result, director::windows_fps::survey()))
                    });
                }
            }
            FpsMessage::Applied(result, caps) => {
                self.end();
                self.fps.caps = caps;
                match result {
                    Ok(text) => {
                        self.current.fps_dirty = false;
                        self.status = "FPS configuration applied.".into();
                        self.details = text;
                        self.error = false;
                        self.announce(cx);
                    }
                    Err(e) => self.failure(e),
                }
            }
            _ => {}
        }
    }
    pub(crate) fn fps_controls(&self, cx: &ViewContext<Self>, target: Target) -> View {
        let request = self.fps_request(target);
        let methods = self
            .fps
            .caps
            .iter()
            .filter(|c| c.available)
            .collect::<Vec<_>>();
        let available = !self.busy && self.guard_ready;
        let selected = request.and_then(|r| methods.iter().position(|c| c.provider == r.provider));
        let cap = request.and_then(|r| self.fps.caps.iter().find(|c| c.provider == r.provider));
        let current = self
            .fps
            .caps
            .iter()
            .filter_map(|c| {
                c.current.as_ref().map(|s| {
                    format!(
                        "{}: {}{}",
                        c.provider.label(),
                        if s.value == 0 {
                            "global limit off".into()
                        } else {
                            format!("{} FPS", s.value)
                        },
                        if s.limiter_disabled == Some(true) {
                            " (limiter suspended)"
                        } else {
                            ""
                        }
                    )
                })
            })
            .collect::<Vec<_>>()
            .join(" · ");
        StackPanel::new().spacing(12.0).children((
TextBlock::new().text(if target==Target::Scene{"4 · FPS limit"}else{"FPS limit"}).font_size(if target==Target::Scene{20.0}else{24.0}).font_weight(FontWeight::SEMI_BOLD),
if current.is_empty(){View::empty()}else{note(current)},
ToggleSwitch::new().automation_name("Change FPS limit").is_on(request.is_some()).is_enabled(available&&(!methods.is_empty()||request.is_some())).on_toggled(cx.callback(move|v|msg(FpsMessage::Enable(target,v)))).slot(ToggleSwitchSlot::Header,"Change FPS limit"),
if !self.fps.loaded||self.fps.checking{note("Checking installed FPS methods…")}else if methods.is_empty(){note(self.fps.caps.iter().map(|c|format!("{}: {}",c.provider.label(),c.detail)).collect::<Vec<_>>().join("\n"))}else{View::empty()},
if self.fps.loaded&&self.fps.caps.iter().any(|c|!c.available){Button::new().automation_name("Check FPS methods").is_enabled(!self.fps.checking).on_click(cx.message(msg(FpsMessage::Discover))).content("Check available methods")}else{View::empty()},
if let Some(r)=request{StackPanel::new().spacing(12.0).children((
ComboBox::new().automation_name("FPS method").items_source(methods.iter().map(|c|c.provider.label())).selected_index(selected).placeholder_text(format!("{} unavailable",r.provider.label())).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(available).on_selection_changed(cx.callback(move|i|msg(FpsMessage::Method(target,i)))).slot(ComboBoxSlot::Header,"Method"),
ComboBox::new().automation_name("FPS limit mode").items_source(["Cap frame rate","Disable global limit"]).selected_index(Some(if r.limit.is_some(){0}else{1})).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(available).on_selection_changed(cx.callback(move|i|msg(FpsMessage::Mode(target,i)))).slot(ComboBoxSlot::Header,"Limit"),
if let Some(n)=r.limit{NumberBox::new().automation_name("FPS value").value(Some(f64::from(n))).minimum(1.0).maximum(f64::from(cap.map_or(1000,|c|c.maximum))).is_enabled(available).on_value_changed(cx.callback(move|v|msg(FpsMessage::Value(target,v)))).slot(NumberBoxSlot::Header,"Frames per second")}else{View::empty()},
if let Some(c)=cap{note(&c.detail)}else{note("The saved method is unavailable. Applying will fail before any scene changes.")},
if let Err(e)=r.validate(){note(e)}else{View::empty()},
note("Global setting. Game-specific profiles and other limiters can still apply. RTSS must stay running when selected. A running game may need restarting."),
))}else{note("Off leaves all existing FPS settings unchanged.")},
if target==Target::Current{Button::new().automation_name("Apply current FPS changes").style(ButtonStyle::Accent).horizontal_alignment(HorizontalAlignment::Left).is_enabled(available&&self.current.fps_dirty&&request.is_some_and(|r|r.validate().is_ok())&&cap.is_some_and(|c|c.available)).on_click(cx.message(msg(FpsMessage::Apply))).content("Apply FPS changes")}else{View::empty()},
))
    }
}
#[cfg(test)]
pub(crate) static PREVIEW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
#[test]
#[ignore = "Owned native FPS UI fixture; no device writes"]
fn fps_fixture() {
    PREVIEW.store(true, std::sync::atomic::Ordering::Release);
    crate::preset_cards_preview::ENABLED.store(true, std::sync::atomic::Ordering::Release);
    director::diagnostics::init();
    windows_reactor::App::run_component::<DirectorApp>(()).unwrap();
    director::diagnostics::flush();
}
