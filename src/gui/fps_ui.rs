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
    /// Use a VRR-safe cap worked out from the primary display's refresh rate.
    VrrSafe(Target, bool),
    Value(Target, Option<f64>),
    Apply,
    Applied(Result<String, String>, Vec<Capability>),
}
#[derive(Default)]
pub(crate) struct FpsState {
    pub caps: Vec<Capability>,
    pub(crate) loaded: bool,
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
use crate::gui::ui::{self, note};
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
                if crate::gui::preset_cards_preview::ENABLED
                    .load(std::sync::atomic::Ordering::Acquire)
                {
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
                        director::fps::windows_fps::survey(),
                    ))
                });
            }
            FpsMessage::Discovered(revision, caps) if revision == self.fps.revision => {
                self.fps.caps = caps;
                self.fps.loaded = true;
                self.fps.checking = false;
            }
            FpsMessage::Enable(target, on) if !self.busy => {
                let request = if on { self.fps_default() } else { None };
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
                    if index == 1 {
                        r.limit = None;
                        r.vrr_safe = false;
                    } else {
                        r.limit = Some(r.limit.unwrap_or(60));
                    }
                    self.fps_edited(target);
                }
            }
            FpsMessage::VrrSafe(target, on) if !self.busy => {
                if let Some(r) = self.fps_request_mut(target) {
                    r.vrr_safe = on;
                    if r.limit.is_none() {
                        r.limit = Some(60);
                    }
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
                    if crate::gui::preset_cards_preview::ENABLED
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
                            let _lock = director::audio::windows_audio::OperationLock::acquire()?;
                            let refresh = if request.vrr_safe {
                                director::display::windows_displays::capture()?.primary_refresh_hz()
                            } else {
                                None
                            };
                            director::fps::windows_fps::apply(&request.resolved(refresh)?, false)
                        })();
                        msg(FpsMessage::Applied(
                            result,
                            director::fps::windows_fps::survey(),
                        ))
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
    /// The refresh rate a VRR-safe preview uses: the scene's display preset, or the live
    /// layout in Now (falling back to the last captured layout).
    fn fps_preview_refresh(&self, target: Target) -> Option<f64> {
        let scene = (target == Target::Scene)
            .then(|| {
                self.scene
                    .draft
                    .as_ref()
                    .and_then(|d| d.display.as_ref())
                    .and_then(|id| self.scene.displays.iter().find(|p| &p.id == id))
                    .and_then(|p| p.layout.primary_refresh_hz())
            })
            .flatten();
        scene
            .or_else(|| {
                self.current
                    .display
                    .draft
                    .as_ref()
                    .and_then(|d| d.layout.primary_refresh_hz())
            })
            .or_else(|| {
                self.display
                    .current
                    .as_ref()
                    .and_then(|l| l.primary_refresh_hz())
            })
    }
    /// Now's staged FPS change can be applied: it's valid and its method is available.
    pub(crate) fn live_fps_ready(&self) -> bool {
        self.current.fps_dirty
            && self.fps.live.as_ref().is_some_and(|r| {
                r.validate().is_ok()
                    && self
                        .fps
                        .caps
                        .iter()
                        .any(|c| c.provider == r.provider && c.available)
            })
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
        // Only the selected method: the others don't matter for this step.
        let current = request
            .and_then(|r| self.fps.caps.iter().find(|c| c.provider == r.provider))
            .and_then(|c| c.current.as_ref().map(|s| (c.provider, s)))
            .map(|(provider, s)| {
                format!(
                    "Currently: {} with {}{}",
                    if s.value == 0 {
                        "no global limit".into()
                    } else {
                        format!("{} FPS", s.value)
                    },
                    provider.label(),
                    if s.limiter_disabled == Some(true) {
                        " (limiter suspended)"
                    } else {
                        ""
                    }
                )
            })
            .unwrap_or_else(|| {
                // On Now with nothing staged, say what every method has set right now.
                if target != Target::Current {
                    return String::new();
                }
                let set = self
                    .fps
                    .caps
                    .iter()
                    .filter_map(|c| c.current.as_ref().map(|s| (c.provider, s.value)))
                    .map(|(provider, value)| {
                        if value == 0 {
                            format!("{} no limit", provider.label())
                        } else {
                            format!("{} {value} FPS", provider.label())
                        }
                    })
                    .collect::<Vec<_>>();
                if set.is_empty() {
                    String::new()
                } else {
                    format!("Currently: {}", set.join(" · "))
                }
            });
        let refresh = self.fps_preview_refresh(target);
        // The controls are always shown, so turning the limit on never changes the page's
        // height. While off they show what turning it on would use, greyed out.
        let on = request.is_some();
        let shown = request.cloned().or_else(|| self.fps_default());
        let editable = available && on;
        let selected = shown
            .as_ref()
            .and_then(|r| methods.iter().position(|c| c.provider == r.provider));
        let cap = shown
            .as_ref()
            .and_then(|r| self.fps.caps.iter().find(|c| c.provider == r.provider));
        let switch = ToggleSwitch::new()
            .automation_name("Change FPS limit")
            .is_on(on)
            .is_enabled(available && (!methods.is_empty() || on))
            .on_toggled(cx.callback(move |v| msg(FpsMessage::Enable(target, v))));
        let header = Grid::new()
            .columns([GridLength::Star(1.0), GridLength::Auto])
            .column_spacing(ui::GAP_L)
            .children((
                StackPanel::new()
                    .spacing(2.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        ui::label(if target == Target::Current {
                            "FPS limit"
                        } else {
                            "Change FPS limit"
                        }),
                        ui::caption(if target == Target::Current && self.current.fps_dirty {
                            "Not applied yet: use Apply changes on the display map.".into()
                        } else if on {
                            current
                        } else if current.is_empty() {
                            "Off leaves FPS settings as they are.".into()
                        } else {
                            format!("{current}. Off leaves it as it is.")
                        }),
                    )),
                Border::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(switch),
            ));
        let status = if !self.fps.loaded || self.fps.checking {
            note("Checking installed FPS methods…")
        } else if methods.is_empty() {
            note(
                self.fps
                    .caps
                    .iter()
                    .map(|c| format!("{}: {}", c.provider.label(), c.detail))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        } else {
            View::empty()
        };
        let check = if self.fps.loaded && self.fps.caps.iter().any(|c| !c.available) {
            Button::new()
                .automation_name("Check FPS methods")
                .is_enabled(!self.fps.checking)
                .on_click(cx.message(msg(FpsMessage::Discover)))
                .content("Check available methods")
        } else {
            View::empty()
        };
        // Two compact rows of labelled controls, like a display card, so the card stays short.
        let field = |title: &str, control: View| -> View {
            StackPanel::new()
                .spacing(4.0)
                .children((TextBlock::new().text(title), control))
        };
        let pair = |left: View, right: View| -> View {
            Grid::new()
                .columns([GridLength::Star(1.0), GridLength::Star(1.0)])
                .column_spacing(ui::GAP_M)
                .children((
                    Border::new().content(left),
                    Border::new().grid_column(1).content(right),
                ))
        };
        let body = match &shown {
            Some(r) => {
                let limited = r.limit.is_some() || r.vrr_safe;
                let rate: View = if r.vrr_safe {
                    field(
                        "Frames per second",
                        ui::note(match refresh {
                            Some(hz) => format!(
                                "{} FPS on a {} Hz display",
                                director::fps::vrr_safe_cap(hz),
                                hz.round()
                            ),
                            None => "Worked out from the refresh rate".into(),
                        }),
                    )
                } else if let Some(n) = r.limit {
                    field(
                        "Frames per second",
                        NumberBox::new()
                            .automation_name("FPS value")
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .value(Some(f64::from(n)))
                            .minimum(1.0)
                            .maximum(f64::from(cap.map_or(1000, |c| c.maximum)))
                            .is_enabled(editable)
                            .on_value_changed(
                                cx.callback(move |v| msg(FpsMessage::Value(target, v))),
                            )
                            .into(),
                    )
                } else {
                    ui::note("Turns off a global cap that’s already set.")
                };
                StackPanel::new()
                    .spacing(ui::GAP_M)
                    .opacity(if on { 1.0 } else { 0.55 })
                    .children((
                        pair(
                            field(
                                "Method",
                                ComboBox::new()
                                    .automation_name("FPS method")
                                    .horizontal_alignment(HorizontalAlignment::Stretch)
                                    .items_source(methods.iter().map(|c| c.provider.label()))
                                    .selected_index(selected)
                                    .placeholder_text(format!(
                                        "{} unavailable",
                                        r.provider.label()
                                    ))
                                    .is_enabled(editable)
                                    .on_selection_changed(
                                        cx.callback(move |i| msg(FpsMessage::Method(target, i))),
                                    )
                                    .into(),
                            ),
                            field(
                                "Limit",
                                ComboBox::new()
                                    .automation_name("FPS limit mode")
                                    .horizontal_alignment(HorizontalAlignment::Stretch)
                                    .items_source(["Set a frame rate limit", "Remove the global limit"])
                                    .selected_index(Some(if limited { 0 } else { 1 }))
                                    .is_enabled(editable)
                                    .on_selection_changed(
                                        cx.callback(move |i| msg(FpsMessage::Mode(target, i))),
                                    )
                                    .into(),
                            ),
                        ),
                        pair(
                            rate,
                            if limited {
                                field(
                                    "VRR-safe cap",
                                    ToggleSwitch::new()
                                        .automation_name("VRR-safe cap")
                                        .is_on(r.vrr_safe)
                                        .is_enabled(editable)
                                        .on_toggled(
                                            cx.callback(move |v| msg(FpsMessage::VrrSafe(target, v))),
                                        )
                                        .into(),
                                )
                            } else {
                                View::empty()
                            },
                        ),
                        match r.validate() {
                            Err(e) if on => ui::caption(e),
                            _ => View::empty(),
                        },
                        ui::caption(match cap {
                            Some(c) => c.detail.clone(),
                            None => "The saved method is unavailable. Applying will fail before any scene changes.".into(),
                        }),
                    ))
            }
            None => View::empty(),
        };
        let content = StackPanel::new()
            .spacing(ui::GAP_M)
            .children((header, status, check, body));
        if target == Target::Current {
            ui::card(content)
        } else {
            content
        }
    }
    /// The request turning the limit on starts from: the first available method, at its
    /// current limit (or 60 when it has none).
    fn fps_default(&self) -> Option<Request> {
        self.fps.caps.iter().find(|c| c.available).map(|c| Request {
            provider: c.provider,
            limit: Some(
                c.current
                    .as_ref()
                    .map_or(60, |s| if s.value == 0 { 60 } else { s.value })
                    .clamp(1, c.maximum),
            ),
            vrr_safe: false,
        })
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
    crate::gui::preset_cards_preview::ENABLED.store(true, std::sync::atomic::Ordering::Release);
    director::diagnostics::init();
    windows_reactor::App::run_component::<DirectorApp>(()).unwrap();
    director::diagnostics::flush();
}
