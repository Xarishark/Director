//! Native TV controls. Status reads never hold the mutation guard.
use crate::gui::tv_power_state::{Next, Progress, WakeRetries};
use crate::{DirectorApp, Message, gui::tv_ui::TvMessage};
use director::{
    audio::windows_audio::OperationLock,
    tv::tv_control::{self, Effect, PowerAction, PowerState, Report, Snapshot},
    tv::tvs::{self, Tv},
    tv::windows_tv::WindowsTvControls,
};
use windows_reactor::*;
#[derive(Clone)]
pub(crate) enum ControlMessage {
    Open(String),
    Close,
    Refresh,
    Loaded(u64, u64, Result<Snapshot, String>),
    Poll(u64, u64),
    Watch(u64, Option<director::tv::windows_lg::PowerUpdate>),
    Details(bool),
    Input(Option<usize>),
    Power(PowerAction),
    Switch,
    Completed(u64, Result<Report, String>),
}
pub(crate) struct Controls {
    pub tv_id: String,
    pub token: u64,
    pub snapshot: Option<Snapshot>,
    pub target: String,
    revision: u64,
    checking: bool,
    notice: String,
    details: String,
    status_details: String,
    show_details: bool,
    progress: Progress,
    retries: WakeRetries,
    watch: Option<director::tv::windows_lg::PowerWatch>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    wake_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ignore_before: std::time::Instant,
    live_details: String,
}
impl Drop for Controls {
    fn drop(&mut self) {
        self.wake_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        self.cancel
            .store(true, std::sync::atomic::Ordering::Release);
    }
}
impl Controls {
    fn live_update(&mut self, update: director::tv::windows_lg::PowerUpdate) -> bool {
        use director::tv::windows_lg::PowerEvent;
        if update.at < self.ignore_before {
            return false;
        }
        match update.event {
            PowerEvent::Unavailable(error) => {
                self.live_details = format!(
                    "Live power status unavailable. Reconnecting while these controls are open. {error}"
                );
                if !matches!(self.progress, Progress::AssumedOff | Progress::Waking(_)) {
                    self.invalidate_read();
                }
                if self.progress != Progress::AssumedOff {
                    let snapshot = self.snapshot.get_or_insert_with(Snapshot::default);
                    snapshot.power = PowerState::Unknown;
                    snapshot.current_input = None;
                }
                false
            }
            PowerEvent::Reported(power) => {
                self.live_details.clear();
                let old = self
                    .snapshot
                    .as_ref()
                    .map_or(PowerState::Unknown, |s| s.power);
                if self.progress == Progress::AssumedOff
                    && power.awake()
                    && update.at.saturating_duration_since(self.ignore_before)
                        < std::time::Duration::from_millis(500)
                {
                    return false;
                }
                let was_waking = matches!(self.progress, Progress::Waking(_));
                if power != old && (!was_waking || power.awake()) {
                    self.invalidate_read();
                }
                let snapshot = self.snapshot.get_or_insert_with(Snapshot::default);
                snapshot.power = power;
                if !power.awake() {
                    snapshot.current_input = None;
                }
                if power.awake() {
                    self.wake_cancel
                        .store(true, std::sync::atomic::Ordering::Release);
                }
                let next = self.progress.observe(power, update.at);
                if was_waking && next == Next::Done {
                    self.notice = "The TV reports awake.".into();
                }
                if !power.awake() && power != PowerState::Unknown && self.progress == Progress::Idle
                {
                    self.notice = "The TV reports off or standby.".into();
                }
                power.awake() && !old.awake()
            }
        }
    }
    fn apply_report(&mut self, report: Report) -> bool {
        self.ignore_before = std::time::Instant::now();
        self.progress.accepted(&report.effect, self.ignore_before);
        if report.effect == Effect::WakeSent {
            self.retries.started(self.ignore_before);
        }
        #[cfg(test)]
        if TRANSITIONS_PREVIEW.load(std::sync::atomic::Ordering::Acquire)
            && matches!(self.progress, Progress::Waking(_))
        {
            self.progress =
                Progress::Waking(std::time::Instant::now() + std::time::Duration::from_secs(4));
        }
        self.notice = report.message;
        self.details = report.details;
        match report.effect {
            Effect::WakeSent => {
                self.notice = "Waiting for the TV to report awake…".into();
                true
            }
            Effect::OffAccepted | Effect::OffSent => {
                let snapshot = self.snapshot.get_or_insert_with(Snapshot::default);
                snapshot.power = PowerState::Off;
                snapshot.current_input = None;
                false
            }
            Effect::AlreadyOff(power) => {
                let snapshot = self.snapshot.get_or_insert_with(Snapshot::default);
                snapshot.power = power;
                snapshot.current_input = None;
                false
            }
            Effect::InputConfirmed(id) => {
                if let Some(snapshot) = &mut self.snapshot {
                    snapshot.current_input = Some(id);
                }
                false
            }
            Effect::InputAccepted(_) => true,
            Effect::InputSent(_) => false,
        }
    }
    fn invalidate_read(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.checking = false;
    }
    fn accepts(&self, token: u64, revision: u64) -> bool {
        self.token == token && self.revision == revision
    }
}
fn msg(message: ControlMessage) -> Message {
    Message::Tv(TvMessage::Control(message))
}
fn current_definition(tv: &Tv) -> Result<(), String> {
    let current = tvs::load(&tvs::path()?)?;
    if current.iter().find(|t| t.id == tv.id) != Some(tv) {
        return Err(
            "This TV changed in another window. Close these controls and reopen the TVs page."
                .into(),
        );
    }
    Ok(())
}
fn selected_target(state: &mut Controls, tv: &Tv, snapshot: Snapshot) {
    if !snapshot.inputs.iter().any(|i| i.id == state.target) {
        state.target = snapshot
            .current_input
            .as_ref()
            .filter(|id| snapshot.inputs.iter().any(|i| &i.id == *id))
            .cloned()
            .or_else(|| {
                tv.pc_input
                    .as_ref()
                    .filter(|pc| snapshot.inputs.iter().any(|i| i.id == pc.id))
                    .map(|pc| pc.id.clone())
            })
            .unwrap_or_default();
    }
    state.snapshot = Some(snapshot);
}
impl DirectorApp {
    fn start_power_watch(&mut self, cx: &ComponentContext<Self>) {
        let Some(state) = &mut self.tv.controls else {
            return;
        };
        let Some(tv) = self.tv.tvs.iter().find(|t| t.id == state.tv_id).cloned() else {
            return;
        };
        let watch = director::tv::windows_tv::watch_power(tv);
        state.cancel = watch.cancel.clone();
        state.watch = Some(watch);
        self.queue_power_watch(cx);
    }
    fn queue_power_watch(&self, cx: &ComponentContext<Self>) {
        let Some(state) = &self.tv.controls else {
            return;
        };
        let Some(watch) = &state.watch else {
            return;
        };
        let token = state.token;
        let receiver = watch.receiver.clone();
        let _ = cx.spawn_background_with_rejection(
            move |_| {
                msg(ControlMessage::Watch(
                    token,
                    receiver.lock().ok().and_then(|r| r.recv().ok()),
                ))
            },
            msg(ControlMessage::Watch(token, None)),
        );
    }
    fn read_tv_controls(&mut self, cx: &ComponentContext<Self>) {
        let Some(state) = &mut self.tv.controls else {
            return;
        };
        let waking = matches!(state.progress, Progress::Waking(_));
        let wake_deadline = if let Progress::Waking(deadline) = state.progress {
            Some(deadline)
        } else {
            None
        };
        let retry_wake = waking && state.retries.take(std::time::Instant::now());
        let cancel = state.cancel.clone();
        let wake_cancel = state.wake_cancel.clone();
        state.invalidate_read();
        state.checking = true;
        let token = state.token;
        let revision = state.revision;
        let Some(tv) = self.tv.tvs.iter().find(|t| t.id == state.tv_id).cloned() else {
            return;
        };
        let _ = cx.spawn_background_with_rejection(
            move |_| {
                let _operation =
                    director::diagnostics::Operation::start("tv.controls.status", Some(&tv.id));
                director::diagnostics::event(
                    "tv.dialog.context",
                    "read",
                    &format!("token={token} revision={revision} waking={waking}"),
                );
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    #[cfg(test)]
                    if CONTROL_PREVIEW.load(std::sync::atomic::Ordering::Acquire) {
                        return Ok(preview_snapshot(waking));
                    }
                    if cancel.load(std::sync::atomic::Ordering::Acquire) {
                        return Err("TV controls closed.".into());
                    }
                    current_definition(&tv)?;
                    let mut backend = WindowsTvControls::new(&tv);
                    if wake_deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                        return Err("The wake readiness deadline expired.".into());
                    }
                    if retry_wake && !wake_cancel.load(std::sync::atomic::Ordering::Acquire) {
                        let _lock = OperationLock::acquire()?;
                        if cancel.load(std::sync::atomic::Ordering::Acquire) {
                            return Err("TV controls closed.".into());
                        }
                        if wake_deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                            return Err("The wake readiness deadline expired.".into());
                        }
                        use director::tv::tv_control::Backend;
                        director::diagnostics::measure(
                            "tv.wake.retry",
                            "bounded retransmission",
                            || {
                                if wake_cancel.load(std::sync::atomic::Ordering::Acquire)
                                    || cancel.load(std::sync::atomic::Ordering::Acquire)
                                {
                                    return Err("Wake retry canceled.".into());
                                }
                                backend.wake()
                            },
                        )?;
                    }
                    if cancel.load(std::sync::atomic::Ordering::Acquire) {
                        return Err("TV controls closed.".into());
                    }
                    Ok(if waking {
                        tv_control::wake_snapshot(&mut backend).unwrap_or_else(|error| Snapshot {
                            notes: vec![error],
                            ..Default::default()
                        })
                    } else {
                        tv_control::snapshot(&mut backend)
                    })
                }))
                .unwrap_or_else(|_| {
                    Err("The TV status check stopped unexpectedly. Try refreshing.".into())
                });
                director::diagnostics::result("tv.controls.status.result", &result);
                if let Ok(snapshot) = &result {
                    director::diagnostics::event(
                        "tv.snapshot.power",
                        "reported",
                        snapshot.power.label(),
                    );
                    for note in &snapshot.notes {
                        director::diagnostics::event("tv.snapshot.note", "unavailable", note);
                    }
                }
                msg(ControlMessage::Loaded(token, revision, result))
            },
            msg(ControlMessage::Loaded(
                token,
                revision,
                Err("The TV status check could not complete. Try refreshing.".into()),
            )),
        );
    }
    fn queue_wake_check(
        &self,
        token: u64,
        revision: u64,
        delay: std::time::Duration,
        cx: &ComponentContext<Self>,
    ) {
        let _ = cx.spawn_background_with_rejection(
            move |_| {
                std::thread::sleep(delay);
                msg(ControlMessage::Poll(token, revision))
            },
            msg(ControlMessage::Loaded(
                token,
                revision,
                Err("Could not schedule the wake check. Try again.".into()),
            )),
        );
    }

    pub(crate) fn update_tv_controls(
        &mut self,
        message: ControlMessage,
        cx: &ComponentContext<Self>,
    ) {
        let ready = self.tv_ready();
        match message {
            ControlMessage::Open(id) if ready && self.tv.tvs.iter().any(|t| t.id == id) => {
                self.tv.control_revision = self.tv.control_revision.wrapping_add(1);
                self.tv.controls = Some(Controls {
                    tv_id: id,
                    token: self.tv.control_revision,
                    snapshot: None,
                    target: String::new(),
                    revision: 0,
                    checking: false,
                    notice: String::new(),
                    details: String::new(),
                    status_details: String::new(),
                    show_details: false,
                    progress: Progress::Idle,
                    retries: WakeRetries::default(),
                    watch: None,
                    cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    wake_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    ignore_before: std::time::Instant::now(),
                    live_details: String::new(),
                });
                #[cfg(test)]
                if crate::gui::samsung_ui::PREVIEW.load(std::sync::atomic::Ordering::Acquire) {
                    let tv = self
                        .tv
                        .tvs
                        .iter()
                        .find(|t| self.tv.controls.as_ref().is_some_and(|c| c.tv_id == t.id))
                        .unwrap();
                    let inputs = tv.control_inputs();
                    if let Some(state) = &mut self.tv.controls {
                        state.target = inputs.first().map_or(String::new(), |i| i.id.clone());
                        state.snapshot = Some(Snapshot {
                            power: PowerState::On,
                            inputs,
                            current_input: None,
                            notes: vec![],
                        });
                    }
                    return;
                }
                #[cfg(test)]
                if CONTROL_PREVIEW.load(std::sync::atomic::Ordering::Acquire) {
                    if let Some(state) = &mut self.tv.controls {
                        state.snapshot = Some(Snapshot {
                            power: PowerState::On,
                            inputs: vec![
                                director::tv::lg_protocol::Input {
                                    id: "HDMI_1".into(),
                                    label: "PC".into(),
                                },
                                director::tv::lg_protocol::Input {
                                    id: "HDMI_2".into(),
                                    label: "Console".into(),
                                },
                            ],
                            current_input: Some("HDMI_1".into()),
                            notes: vec![],
                        });
                        state.target = "HDMI_1".into();
                    }
                    if LIVE_PREVIEW.load(std::sync::atomic::Ordering::Acquire) {
                        self.start_live_preview(cx);
                    }
                    return;
                }
                self.start_power_watch(cx);
                self.read_tv_controls(cx);
            }
            ControlMessage::Watch(token, update) => {
                let Some(state) = &mut self.tv.controls else {
                    return;
                };
                if state.token != token {
                    return;
                }
                let Some(update) = update else {
                    return;
                };
                let refresh = if !self.busy {
                    state.live_update(update)
                } else {
                    false
                };
                let checking = state.checking;
                self.queue_power_watch(cx);
                if refresh && !checking {
                    self.read_tv_controls(cx);
                }
            }
            ControlMessage::Close => self.tv.controls = None,
            ControlMessage::Details(value) => {
                if let Some(state) = &mut self.tv.controls {
                    state.show_details = value;
                }
            }
            ControlMessage::Refresh
                if ready
                    && self
                        .tv
                        .controls
                        .as_ref()
                        .is_some_and(|s| !s.checking && !s.progress.pending()) =>
            {
                if let Some(state) = &mut self.tv.controls {
                    state.notice.clear();
                    state.details.clear();
                    state.status_details.clear();
                    state.show_details = false;
                }
                self.read_tv_controls(cx)
            }
            ControlMessage::Poll(token, revision) => {
                let Some(state) = &mut self.tv.controls else {
                    return;
                };
                if !state.accepts(token, revision) || !matches!(state.progress, Progress::Waking(_))
                {
                    return;
                }
                let power = state
                    .snapshot
                    .as_ref()
                    .map_or(PowerState::Unknown, |s| s.power);
                if state.progress.observe(power, std::time::Instant::now()) == Next::Expired {
                    state.notice = "The TV has not reported awake. You can retry Wake.".into();
                    director::diagnostics::event("tv.wake.wait", "expired", "");
                } else {
                    self.read_tv_controls(cx);
                }
            }
            ControlMessage::Loaded(token, revision, result) => {
                let Some(state) = &mut self.tv.controls else {
                    return;
                };
                if !state.accepts(token, revision) {
                    return;
                }
                state.checking = false;
                let mut next = Next::Done;
                match result {
                    Ok(snapshot) => {
                        let was_waking = matches!(state.progress, Progress::Waking(_));
                        next = state
                            .progress
                            .observe(snapshot.power, std::time::Instant::now());
                        if snapshot.power.awake() {
                            state
                                .wake_cancel
                                .store(true, std::sync::atomic::Ordering::Release);
                        }
                        state.status_details = snapshot.notes.join("\n");
                        if was_waking {
                            state.notice = match next {
                                Next::Done => "The TV reports awake.".into(),
                                Next::Retry(_) => "Waiting for the TV to report awake…".into(),
                                Next::Expired => {
                                    "The TV has not reported awake. You can retry Wake.".into()
                                }
                            };
                            director::diagnostics::event(
                                "tv.wake.wait",
                                if next == Next::Done {
                                    "awake"
                                } else if next == Next::Expired {
                                    "expired"
                                } else {
                                    "waiting"
                                },
                                "",
                            );
                        } else if state.notice.is_empty() {
                            state.notice = if snapshot.power == PowerState::Unknown
                                && state.progress != Progress::AssumedOff
                            {
                                "TV status unavailable. It may be off or unreachable.".into()
                            } else if !snapshot.notes.is_empty()
                                && state.progress != Progress::AssumedOff
                            {
                                "Some TV details are unavailable. Try refreshing.".into()
                            } else {
                                String::new()
                            };
                        }
                        if let Some(tv) = self.tv.tvs.iter().find(|t| t.id == state.tv_id) {
                            selected_target(state, tv, snapshot);
                        }
                    }
                    Err(e) => {
                        state.snapshot = None;
                        if matches!(state.progress, Progress::Waking(_)) {
                            next = state
                                .progress
                                .observe(PowerState::Unknown, std::time::Instant::now());
                            if next == Next::Expired {
                                state.notice =
                                    "The TV has not reported awake. You can retry Wake.".into();
                            }
                        } else {
                            state.progress = Progress::Idle;
                        }
                        if state.notice.is_empty() {
                            state.notice =
                                "Could not check this TV. Close the controls and try again.".into();
                        }
                        state.status_details = e;
                    }
                }
                if let Next::Retry(delay) = next {
                    self.queue_wake_check(token, revision, delay, cx);
                }
            }

            ControlMessage::Input(index)
                if !self.busy
                    && self
                        .tv
                        .controls
                        .as_ref()
                        .is_some_and(|s| !s.progress.pending() && !s.checking) =>
            {
                if let Some(state) = &mut self.tv.controls {
                    state.target = index
                        .and_then(|i| state.snapshot.as_ref()?.inputs.get(i))
                        .map_or(String::new(), |i| i.id.clone());
                }
            }
            ControlMessage::Power(action)
                if ready
                    && self
                        .tv
                        .controls
                        .as_ref()
                        .is_some_and(|s| !s.progress.pending()) =>
            {
                self.run_tv_control(Some(action), cx)
            }
            ControlMessage::Switch
                if ready
                    && self.tv.controls.as_ref().is_some_and(|s| {
                        !s.progress.pending() && !s.checking && s.progress != Progress::AssumedOff
                    }) =>
            {
                self.run_tv_control(None, cx)
            }
            ControlMessage::Completed(token, report) => {
                self.end();
                if let Some(state) = &mut self.tv.controls
                    && state.token == token
                {
                    let reconcile = match report {
                        Ok(report) => state.apply_report(report),
                        Err(e) => {
                            state.progress = Progress::Idle;
                            state.notice =
                                "Could not confirm the request. Check the TV before retrying."
                                    .into();
                            state.details = e;
                            true
                        }
                    };
                    if reconcile {
                        self.read_tv_controls(cx);
                    }
                } else {
                    self.status = match report {
                        Ok(r) => r.message,
                        Err(_) => {
                            "Could not confirm the TV request. Check the TV before retrying.".into()
                        }
                    };
                    self.details.clear();
                }
            }

            _ => {}
        }
    }
    fn run_tv_control(&mut self, action: Option<PowerAction>, cx: &ComponentContext<Self>) {
        let Some(state) = &mut self.tv.controls else {
            return;
        };
        let token = state.token;
        let target = state.target.clone();
        let Some(tv) = self.tv.tvs.iter().find(|t| t.id == state.tv_id).cloned() else {
            return;
        };
        if action.is_none()
            && !state
                .snapshot
                .as_ref()
                .is_some_and(|s| s.inputs.iter().any(|i| i.id == target))
        {
            return;
        }
        state
            .wake_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        if action == Some(PowerAction::Wake) {
            state.wake_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        }
        state.ignore_before = std::time::Instant::now();
        state.invalidate_read();
        state.progress.begin(action, std::time::Instant::now());
        state.notice = match action {
            Some(PowerAction::Wake) => "Waiting for the TV to report awake…",
            Some(PowerAction::Off) => "Turning off…",
            None => "Switching input…",
        }
        .into();
        state.details.clear();
        state.status_details.clear();
        // Mutations retain the shared guard, without leaking modal progress into the page.
        self.busy = true;
        crate::gui::close_guard::set_busy(true);
        let _ = cx.spawn_background_with_rejection(
            move |_| {
                let report = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _operation =
                        director::diagnostics::Operation::start("tv.controls.action", Some(&tv.id));
                    director::diagnostics::event(
                        "tv.dialog.context",
                        "action",
                        &format!("token={token}"),
                    );
                    director::diagnostics::event(
                        "tv.action.intent",
                        "selected",
                        match action {
                            Some(PowerAction::Wake) => "wake",
                            Some(PowerAction::Off) => "off",
                            None => "switch_input",
                        },
                    );
                    #[cfg(test)]
                    if TRANSITIONS_PREVIEW.load(std::sync::atomic::Ordering::Acquire) {
                        return Ok(preview_action(action, &target));
                    }
                    let lock = OperationLock::acquire();
                    director::diagnostics::result("tv.action.lock", &lock);
                    let _lock = lock?;
                    let definition = current_definition(&tv);
                    director::diagnostics::result("tv.action.definition", &definition);
                    definition?;
                    let mut backend = WindowsTvControls::new(&tv);
                    let result = match action {
                        Some(action) => tv_control::power(&mut backend, action),
                        None => tv_control::switch_input(&mut backend, &target),
                    };
                    director::diagnostics::result("tv.action.result", &result);
                    result
                }))
                .unwrap_or_else(|_| {
                    Err("The TV command stopped unexpectedly. Check the TV before retrying.".into())
                });
                msg(ControlMessage::Completed(token, report))
            },
            msg(ControlMessage::Completed(
                token,
                Err(
                    "The TV command result could not be delivered. Check the TV before retrying."
                        .into(),
                ),
            )),
        );
    }
    pub(crate) fn tv_controls_dialog(&self, cx: &ViewContext<Self>) -> View {
        let Some(state) = &self.tv.controls else {
            return View::empty();
        };
        let Some(tv) = self.tv.tvs.iter().find(|t| t.id == state.tv_id) else {
            return View::empty();
        };
        let snapshot = state.snapshot.as_ref();
        let power = snapshot.map_or(PowerState::Unknown, |s| s.power);
        let action = if state.progress == Progress::AssumedOff
            || state.progress == Progress::WakeExpired
            || matches!(state.progress, Progress::Waking(_))
        {
            PowerAction::Wake
        } else {
            power.button_action()
        };
        let enabled = self.tv_ready()
            && !state.progress.pending()
            && (action == PowerAction::Off || tv.can_wake());
        let inputs = if state.progress.pending()
            || state.progress == Progress::AssumedOff
            || matches!(power, PowerState::Off | PowerState::Standby)
        {
            &[][..]
        } else {
            snapshot.map(|s| s.inputs.as_slice()).unwrap_or(&[])
        };
        let index = inputs.iter().position(|i| i.id == state.target);
        let current = snapshot
            .and_then(|s| s.current_input.as_ref())
            .and_then(|id| inputs.iter().find(|i| &i.id == id))
            .map_or("Unknown", |i| i.label.as_str());
        let switch = self.tv_ready()
            && !state.checking
            && !state.progress.pending()
            && state.progress != Progress::AssumedOff
            && index.is_some()
            && snapshot.is_some_and(|s| s.current_input.as_deref() != Some(state.target.as_str()));
        let status = state.progress.label(power, state.checking);
        ContentDialog::new()
            .is_open(true)
            .title(tv.name.clone())
            .close_button_text("Close controls")
            .on_closed(cx.callback(|_| msg(ControlMessage::Close)))
            .content(
                StackPanel::new().width(340.0).spacing(16.0).children((
                    Grid::new()
                        .columns([GridLength::Star(1.0), GridLength::Auto])
                        .children((
                            StackPanel::new().spacing(4.0).children((
                                TextBlock::new()
                                    .text("Power")
                                    .opacity(crate::gui::ui::SECONDARY),
                                TextBlock::new()
                                    .automation_name(format!("Power status: {status}"))
                                    .text(status)
                                    .font_size(22.0)
                                    .font_weight(FontWeight::SEMI_BOLD),
                            )),
                            Button::new()
                                .grid_column(1)
                                .style(ButtonStyle::Subtle)
                                .automation_name("Refresh TV status")
                                .is_enabled(
                                    self.tv_ready() && !state.checking && !state.progress.pending(),
                                )
                                .on_click(cx.message(msg(ControlMessage::Refresh)))
                                .content(FontIcon::new().glyph("\u{e72c}")),
                        )),
                    if action == PowerAction::Off || tv.can_wake() {
                        Button::new()
                            .automation_name(if action == PowerAction::Off {
                                "Turn off TV"
                            } else {
                                "Wake TV"
                            })
                            .style(ButtonStyle::Accent)
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .is_enabled(enabled)
                            .on_click(cx.message(msg(ControlMessage::Power(action))))
                            .content(
                                StackPanel::new()
                                    .orientation(Orientation::Horizontal)
                                    .spacing(8.0)
                                    .children((
                                        FontIcon::new().glyph("\u{e7e8}"),
                                        TextBlock::new().text(
                                            if matches!(state.progress, Progress::Waking(_)) {
                                                "Waking…"
                                            } else if state.progress == Progress::TurningOff {
                                                "Turning off…"
                                            } else if state.progress == Progress::WakeExpired {
                                                "Retry wake"
                                            } else if action == PowerAction::Off {
                                                "Turn off"
                                            } else {
                                                "Wake"
                                            },
                                        ),
                                    )),
                            )
                    } else {
                        View::empty()
                    },
                    if action == PowerAction::Wake && !tv.can_wake() {
                        TextBlock::new()
                            .text("No wake address saved. Complete TV setup to enable Wake.")
                            .font_size(crate::gui::ui::CAPTION)
                            .opacity(crate::gui::ui::SECONDARY)
                            .text_wrapping(TextWrapping::Wrap)
                            .into()
                    } else {
                        View::empty()
                    },
                    if tv.brand != director::tv::tvs::Brand::Lg && tv.control_inputs().is_empty() {
                        View::empty()
                    } else {
                        StackPanel::new().spacing(8.0).children((
                            TextBlock::new()
                                .text("Input")
                                .font_weight(FontWeight::SEMI_BOLD),
                            {
                                let view: View = if inputs.is_empty() {
                                    TextBlock::new()
                                        .text(if matches!(state.progress, Progress::Waking(_)) {
                                            "Inputs will be available when the TV wakes."
                                        } else if state.progress == Progress::AssumedOff {
                                            "Wake the TV to use its inputs."
                                        } else if state.checking {
                                            "Looking for available inputs…"
                                        } else {
                                            "Inputs appear when the TV is connected."
                                        })
                                        .text_wrapping(TextWrapping::Wrap)
                                        .opacity(crate::gui::ui::SECONDARY)
                                        .into()
                                } else {
                                    StackPanel::new().spacing(8.0).children((
                                        if tv.brand == director::tv::tvs::Brand::Lg {
                                            TextBlock::new()
                                                .automation_name(format!(
                                                    "Current TV input: {current}"
                                                ))
                                                .text(format!("Current: {current}"))
                                                .opacity(crate::gui::ui::SECONDARY)
                                                .text_wrapping(TextWrapping::Wrap)
                                                .into()
                                        } else {
                                            View::empty()
                                        },
                                        Grid::new()
                                            .columns([GridLength::Star(1.0), GridLength::Auto])
                                            .column_spacing(8.0)
                                            .children((
                                                ComboBox::new()
                                                    .automation_name("TV control input")
                                                    .horizontal_alignment(
                                                        HorizontalAlignment::Stretch,
                                                    )
                                                    .items_source(
                                                        inputs.iter().map(|i| i.label.clone()),
                                                    )
                                                    .selected_index(index)
                                                    .is_enabled(
                                                        self.tv_ready()
                                                            && !state.checking
                                                            && !state.progress.pending(),
                                                    )
                                                    .on_selection_changed(cx.callback(|i| {
                                                        msg(ControlMessage::Input(i))
                                                    })),
                                                Button::new()
                                                    .grid_column(1)
                                                    .automation_name("Switch TV input")
                                                    .is_enabled(switch)
                                                    .on_click(
                                                        cx.message(msg(ControlMessage::Switch)),
                                                    )
                                                    .content("Switch"),
                                            )),
                                    ))
                                };
                                view
                            },
                        ))
                    },
                    if !state.notice.is_empty() {
                        TextBlock::new()
                            .text(state.notice.clone())
                            .text_wrapping(TextWrapping::Wrap)
                            .font_size(crate::gui::ui::CAPTION)
                            .opacity(crate::gui::ui::SECONDARY)
                            .into()
                    } else {
                        View::empty()
                    },
                    if !state.details.is_empty()
                        || !state.status_details.is_empty()
                        || !state.live_details.is_empty()
                    {
                        Expander::new()
                            .is_expanded(state.show_details)
                            .on_is_expanded_changed(
                                cx.callback(|v| msg(ControlMessage::Details(v))),
                            )
                            .slots([
                                SlotView::new(ExpanderSlot::Header, "Technical details"),
                                SlotView::new(
                                    ExpanderSlot::Content,
                                    ScrollViewer::new()
                                        .max_height(90.0)
                                        .horizontal_scroll_bar_visibility(
                                            ScrollBarVisibility::Disabled,
                                        )
                                        .content(
                                            TextBlock::new()
                                                .max_width(280.0)
                                                .text(
                                                    [
                                                        state.details.as_str(),
                                                        state.status_details.as_str(),
                                                        state.live_details.as_str(),
                                                    ]
                                                    .into_iter()
                                                    .filter(|s| !s.is_empty())
                                                    .collect::<Vec<_>>()
                                                    .join("\n\n"),
                                                )
                                                .font_size(crate::gui::ui::CAPTION)
                                                .text_wrapping(TextWrapping::Wrap),
                                        ),
                                ),
                            ])
                    } else {
                        View::empty()
                    },
                )),
            )
    }
}
#[cfg(test)]
static LIVE_PREVIEW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
pub(crate) static CONTROL_PREVIEW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
impl DirectorApp {
    fn start_live_preview(&mut self, cx: &ComponentContext<Self>) {
        use director::tv::windows_lg::{PowerEvent, PowerUpdate, PowerWatch};
        use std::sync::{Arc, Mutex, atomic::Ordering, mpsc};
        let Some(state) = &mut self.tv.controls else {
            return;
        };
        let cancel = state.cancel.clone();
        let (sender, receiver) = mpsc::channel();
        state.watch = Some(PowerWatch {
            cancel: cancel.clone(),
            receiver: Arc::new(Mutex::new(receiver)),
        });
        std::thread::spawn(move || {
            for event in [
                PowerEvent::Reported(PowerState::Off),
                PowerEvent::Unavailable("Simulated connection loss".into()),
                PowerEvent::Reported(PowerState::On),
            ] {
                for _ in 0..25 {
                    if cancel.load(Ordering::Acquire) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                if cancel.load(Ordering::Acquire) {
                    return;
                }
                if sender
                    .send(PowerUpdate {
                        at: std::time::Instant::now(),
                        event,
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
        self.queue_power_watch(cx);
    }
    pub(crate) fn install_tv_controls_preview(&mut self) {
        self.page = crate::Page::Tvs;
        self.tv.draft = None;
        self.tv.ready_for_controls_preview();
        self.tv.tvs = vec![Tv {
            id: "test-controls-preview-missing-record".into(),
            name: "Living room TV".into(),
            address: "192.0.2.50".into(),
            brand: Default::default(),
            samsung: Default::default(),
            android: Default::default(),
            connection: Default::default(),
            display: None,
            audio: None,
            credential: Some(vec![1]),
            inputs: vec![],
            pc_input: None,
            network: director::tv::tv_network::NetworkAddresses {
                wired: Some("00:11:22:33:44:55".parse().unwrap()),
                ..Default::default()
            },
        }];
        self.tv.controls = None;
    }
}
#[cfg(test)]
mod native_preview {
    use super::*;
    #[test]
    #[ignore = "Owned-window live-event fixture; no TV, networking or settings writes"]
    fn live_power_fixture() {
        director::diagnostics::init();
        CONTROL_PREVIEW.store(true, std::sync::atomic::Ordering::Release);
        LIVE_PREVIEW.store(true, std::sync::atomic::Ordering::Release);
        App::run_component::<DirectorApp>(()).unwrap();
        director::diagnostics::flush();
    }
    #[test]
    #[ignore = "Owned-window control fixture; never send commands to a real TV"]
    fn controls_fixture() {
        director::diagnostics::init();
        CONTROL_PREVIEW.store(true, std::sync::atomic::Ordering::Release);
        App::run_component::<DirectorApp>(()).unwrap();
        director::diagnostics::flush();
    }
}

#[cfg(test)]
mod read_tests {
    use super::*;
    fn state() -> Controls {
        Controls {
            tv_id: "test".into(),
            token: 3,
            snapshot: None,
            target: String::new(),
            revision: 1,
            checking: true,
            notice: String::new(),
            details: String::new(),
            status_details: String::new(),
            show_details: false,
            progress: Progress::Idle,
            retries: WakeRetries::default(),
            watch: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            wake_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            ignore_before: std::time::Instant::now(),
            live_details: String::new(),
        }
    }
    #[test]
    fn live_events_update_remote_changes_without_turning_disconnect_into_off() {
        use director::tv::windows_lg::{PowerEvent, PowerUpdate};
        use std::time::Duration;
        let mut s = state();
        let at = s.ignore_before + Duration::from_secs(1);
        assert!(s.live_update(PowerUpdate {
            at,
            event: PowerEvent::Reported(PowerState::On)
        }));
        let revision = s.revision;
        assert!(!s.live_update(PowerUpdate {
            at: at + Duration::from_secs(1),
            event: PowerEvent::Reported(PowerState::Off)
        }));
        assert!(s.revision > revision);
        assert_eq!(s.snapshot.as_ref().unwrap().power, PowerState::Off);
        s.live_update(PowerUpdate {
            at: at + Duration::from_secs(2),
            event: PowerEvent::Unavailable("network lost".into()),
        });
        assert_eq!(s.snapshot.as_ref().unwrap().power, PowerState::Unknown);
        s.progress = Progress::AssumedOff;
        s.snapshot.as_mut().unwrap().power = PowerState::Off;
        s.live_update(PowerUpdate {
            at: at + Duration::from_secs(3),
            event: PowerEvent::Unavailable("network lost".into()),
        });
        assert_eq!(s.progress, Progress::AssumedOff);
        assert_eq!(s.snapshot.as_ref().unwrap().power, PowerState::Off);
        s.live_update(PowerUpdate {
            at: at + Duration::from_secs(4),
            event: PowerEvent::Reported(PowerState::Standby),
        });
        assert_eq!(s.progress, Progress::Idle);
    }
    #[test]
    fn old_events_cannot_replace_actions_and_closing_cancels_wake_and_listener() {
        use director::tv::windows_lg::{PowerEvent, PowerUpdate};
        let mut s = state();
        let old = s.ignore_before;
        s.ignore_before += std::time::Duration::from_secs(1);
        assert!(!s.live_update(PowerUpdate {
            at: old,
            event: PowerEvent::Reported(PowerState::On)
        }));
        assert!(s.snapshot.is_none());
        s.progress = Progress::Waking(s.ignore_before + std::time::Duration::from_secs(30));
        s.live_update(PowerUpdate {
            at: s.ignore_before,
            event: PowerEvent::Reported(PowerState::On),
        });
        assert_eq!(s.progress, Progress::Idle);
        assert!(s.wake_cancel.load(std::sync::atomic::Ordering::Acquire));
        let cancel = s.cancel.clone();
        let wake = s.wake_cancel.clone();
        drop(s);
        assert!(cancel.load(std::sync::atomic::Ordering::Acquire));
        assert!(wake.load(std::sync::atomic::Ordering::Acquire));
    }
    #[test]
    fn stale_reads_cannot_replace_an_action_or_reopened_dialog() {
        let mut state = state();
        assert!(state.accepts(3, 1));
        state.invalidate_read();
        assert!(!state.checking);
        assert!(!state.accepts(3, 1));
        assert!(!state.accepts(2, 2));
        assert!(state.accepts(3, 2));
    }
}

#[cfg(test)]
static TRANSITIONS_PREVIEW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static WAKE_COMMANDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static WAKE_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
fn preview_snapshot(waking: bool) -> Snapshot {
    if LIVE_PREVIEW.load(std::sync::atomic::Ordering::Acquire) {
        return Snapshot {
            power: PowerState::On,
            inputs: vec![director::tv::lg_protocol::Input {
                id: "HDMI_1".into(),
                label: "PC".into(),
            }],
            current_input: Some("HDMI_1".into()),
            notes: vec![],
        };
    }
    use std::{sync::atomic::Ordering, time::Duration};
    if TRANSITIONS_PREVIEW.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(500));
        if waking
            && WAKE_COMMANDS.load(Ordering::Acquire) != 2
            && WAKE_READS.fetch_add(1, Ordering::AcqRel) > 0
        {
            return Snapshot {
                power: PowerState::On,
                inputs: vec![
                    director::tv::lg_protocol::Input {
                        id: "HDMI_1".into(),
                        label: "PC".into(),
                    },
                    director::tv::lg_protocol::Input {
                        id: "HDMI_2".into(),
                        label: "Console".into(),
                    },
                ],
                current_input: Some("HDMI_1".into()),
                notes: vec![],
            };
        }
    } else {
        std::thread::sleep(Duration::from_secs(2));
    }
    Snapshot {
        notes: vec!["Power status unavailable: Windows could not communicate with the TV (0x80072EE2). Check the address, network connection, and the TV mobile-app connection setting.".into(), "Inputs unavailable: The TV did not respond before the connection timeout.".into()],
        ..Default::default()
    }
}
#[cfg(test)]
fn preview_action(action: Option<PowerAction>, target: &str) -> Report {
    use std::sync::atomic::Ordering;
    std::thread::sleep(std::time::Duration::from_millis(700));
    let effect = match action {
        Some(PowerAction::Wake) => {
            WAKE_COMMANDS.fetch_add(1, Ordering::AcqRel);
            WAKE_READS.store(0, Ordering::Release);
            Effect::WakeSent
        }
        Some(PowerAction::Off) => Effect::OffAccepted,
        None => Effect::InputConfirmed(target.into()),
    };
    Report {
        effect,
        message: "Mock command accepted.".into(),
        details: "In-memory fixture; no TV command sent.".into(),
    }
}
#[cfg(test)]
mod power_preview {
    use super::*;
    #[test]
    #[ignore = "In-memory power transitions; no TV/storage commands are sent"]
    fn transitions_fixture() {
        use std::sync::atomic::Ordering;
        director::diagnostics::init();
        CONTROL_PREVIEW.store(true, Ordering::Release);
        TRANSITIONS_PREVIEW.store(true, Ordering::Release);
        App::run_component::<DirectorApp>(()).unwrap();
        director::diagnostics::flush();
    }
}
