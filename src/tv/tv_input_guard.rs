//! Keeps a TV on a scene's input for a few seconds after the scene selected it.
//!
//! TV boxes and consoles on HDMI-CEC often take the input over while a TV wakes, even when
//! they didn't wake it. A guard watches the TV on its own thread, so the scene carries on,
//! and switches back whenever the TV reports another input. Only TVs that report their
//! current input get one; without readback a guard couldn't tell a steal from success.
//!
//! A guard must never fight the user. Every other TV power or input request stops it, from
//! any Director process (a Stream Deck `director tv input`, a CLI scene): the running guard
//! writes its token to a marker file, a request deletes the file, and a guard whose token
//! is gone stops at its next poll.
use crate::tv::tv_control::Backend;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// How often a guard reads the TV's input.
pub const POLL: Duration = Duration::from_millis(500);
/// Polls to leave a switch to take effect before sending another one.
const RESEND_POLLS: u32 = 3;
/// The longest guard the preferences allow.
pub const MAX_SECONDS: u16 = 60;
pub const DEFAULT_SECONDS: u16 = 10;

struct Active {
    cancel: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Tells a running guard to end: stopped in this process, or its marker gone or replaced.
pub struct Stop {
    flag: Arc<AtomicBool>,
    /// None when the marker couldn't be written; then only this process can stop it.
    token: Option<String>,
}
impl Stop {
    pub fn is_set(&self) -> bool {
        self.flag.load(Ordering::Acquire)
            || self.token.as_ref().is_some_and(|token| {
                marker()
                    .is_none_or(|path| std::fs::read_to_string(path).ok().as_ref() != Some(token))
            })
    }
}

/// The marker shared by every Director process. Tests use their own, per process.
fn marker() -> Option<PathBuf> {
    #[cfg(test)]
    {
        Some(std::env::temp_dir().join(format!(
            "director-tv-input-guard-{}.txt",
            std::process::id()
        )))
    }
    #[cfg(not(test))]
    {
        crate::storage::settings_path("tv-input-guard.txt").ok()
    }
}
static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);

fn take() -> Option<Active> {
    ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Run `body` as the only guard, stopping one that's running in any process. `body` gets
/// its stop signal and the time the guard ends.
pub fn start(seconds: u16, body: impl FnOnce(&Stop, Instant) + Send + 'static) {
    stop();
    if seconds == 0 {
        return;
    }
    let until = Instant::now() + Duration::from_secs(u64::from(seconds.min(MAX_SECONDS)));
    let cancel = Arc::new(AtomicBool::new(false));
    let token = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    let token = match marker().map(|path| std::fs::write(path, &token)) {
        Some(Ok(())) => Some(token),
        result => {
            let error = result.map_or("no settings folder".into(), |r| format!("{r:?}"));
            crate::diagnostics::event("tv.input.guard.marker", "error", &error);
            None
        }
    };
    let signal = Stop {
        flag: cancel.clone(),
        token,
    };
    let handle = std::thread::spawn(move || {
        body(&signal, until);
        // Clear the marker unless another guard or a stop already replaced it.
        if let (Some(token), Some(path)) = (&signal.token, marker())
            && std::fs::read_to_string(&path).ok().as_ref() == Some(token)
        {
            let _ = std::fs::remove_file(path);
        }
    });
    *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Active { cancel, handle });
}

/// Stop a running guard and wait for it, so it sends nothing after this returns; a guard
/// in another process stops at its next poll. Called before any other TV power or input
/// request, so a guard never fights the user or another scene.
pub fn stop() {
    if let Some(path) = marker() {
        let _ = std::fs::remove_file(path);
    }
    if let Some(active) = take() {
        active.cancel.store(true, Ordering::Release);
        if active.handle.thread().id() != std::thread::current().id() {
            let _ = active.handle.join();
        }
    }
}

/// Wait for a running guard to finish (a CLI run must not exit while its guard runs).
/// Returns whether one was running.
pub fn wait() -> bool {
    take().is_some_and(|active| active.handle.join().is_ok())
}

/// Whether a guard is running.
pub fn active() -> bool {
    ACTIVE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|a| !a.handle.is_finished())
}

/// Sleep for one poll unless stopped first; false when the guard should end.
pub fn sleep_until(stop: &Stop, until: Instant) -> bool {
    let end = (Instant::now() + POLL).min(until);
    while Instant::now() < end {
        if stop.flag.load(Ordering::Acquire) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50).min(end - Instant::now()));
    }
    !stop.is_set() && Instant::now() < until
}

/// Keep `id` selected until `next` says stop (it waits one poll and returns whether to go
/// on). `connect` opens a fresh connection, used again after a failed request; each one
/// reads the input list first, which a TV may need to name its current input (LG maps the
/// app in front to an input through it). Returns how many times the input was switched back.
pub fn hold<B: Backend>(
    mut connect: impl FnMut() -> B,
    id: &str,
    stopped: impl Fn() -> bool,
    mut next: impl FnMut() -> bool,
) -> usize {
    let mut backend = connect();
    let mut listed = false;
    let mut since_switch = RESEND_POLLS;
    let mut switches = 0;
    loop {
        if stopped() {
            break;
        }
        if !listed {
            match backend.inputs() {
                Ok(_) => listed = true,
                Err(e) => {
                    crate::diagnostics::event("tv.input.guard", "unavailable", &e);
                    backend = connect();
                    since_switch = since_switch.saturating_add(1);
                    if !next() {
                        break;
                    }
                    continue;
                }
            }
        }
        match backend.current_input() {
            Ok(current) if current.as_deref() == Some(id) => {}
            Ok(current) => {
                if since_switch >= RESEND_POLLS {
                    let from = current.unwrap_or_else(|| "an app".into());
                    // Checked again right before sending: a stop may have come meanwhile.
                    if stopped() {
                        break;
                    }
                    match backend.switch_input(id) {
                        Ok(()) => {
                            switches += 1;
                            crate::diagnostics::event(
                                "tv.input.guard",
                                "reclaimed",
                                &format!("{from} → {id}"),
                            );
                        }
                        Err(e) => {
                            crate::diagnostics::event("tv.input.guard", "error", &e);
                            backend = connect();
                            listed = false;
                        }
                    }
                    since_switch = 0;
                }
            }
            Err(e) => {
                crate::diagnostics::event("tv.input.guard", "unavailable", &e);
                backend = connect();
                listed = false;
            }
        }
        since_switch = since_switch.saturating_add(1);
        if !next() {
            break;
        }
    }
    switches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tv::lg_protocol::Input, tv::tv_control::PowerState, tv::tv_network::WakeReport};
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct State {
        /// What the TV reports on each read; the last entry repeats.
        reads: Vec<Result<Option<String>, String>>,
        switches: Vec<String>,
        connects: usize,
    }
    /// Like LG, it can't name its input until this connection read the input list.
    struct Fake(Rc<RefCell<State>>, bool);
    impl Backend for Fake {
        fn power(&mut self) -> Result<PowerState, String> {
            Ok(PowerState::On)
        }
        fn inputs(&mut self) -> Result<Vec<Input>, String> {
            self.1 = true;
            Ok(vec![])
        }
        fn current_input(&mut self) -> Result<Option<String>, String> {
            let mut s = self.0.borrow_mut();
            let read = if s.reads.len() > 1 {
                s.reads.remove(0)
            } else {
                s.reads[0].clone()
            };
            read.map(|input| input.filter(|_| self.1))
        }
        fn wake(&mut self) -> Result<WakeReport, String> {
            unreachable!()
        }
        fn off(&mut self) -> Result<(), String> {
            unreachable!()
        }
        fn switch_input(&mut self, id: &str) -> Result<(), String> {
            self.0.borrow_mut().switches.push(id.into());
            Ok(())
        }
    }
    fn run(reads: Vec<Result<Option<String>, String>>, polls: usize) -> (usize, State) {
        let state = Rc::new(RefCell::new(State {
            reads,
            ..Default::default()
        }));
        let mut left = polls;
        let switched = hold(
            || {
                state.borrow_mut().connects += 1;
                Fake(state.clone(), false)
            },
            "HDMI_1",
            || false,
            || {
                left -= 1;
                left > 0
            },
        );
        (switched, Rc::try_unwrap(state).ok().unwrap().into_inner())
    }
    fn on(id: &str) -> Result<Option<String>, String> {
        Ok(Some(id.into()))
    }

    #[test]
    fn a_steady_input_is_left_alone() {
        let (switched, state) = run(vec![on("HDMI_1")], 20);
        assert_eq!(switched, 0);
        assert!(state.switches.is_empty());
        assert_eq!(state.connects, 1);
    }

    #[test]
    fn a_stolen_input_is_switched_back_once_per_resend_window() {
        // The box takes over and keeps it for 5 polls, then the TV settles on the PC.
        let mut reads = vec![on("HDMI_1"), on("HDMI_1")];
        reads.extend((0..5).map(|_| on("HDMI_2")));
        reads.push(on("HDMI_1"));
        let (switched, state) = run(reads, 20);
        // Switched on the first steal and again 3 polls later; not on every poll.
        assert_eq!(switched, 2);
        assert_eq!(state.switches, ["HDMI_1", "HDMI_1"]);
    }

    #[test]
    fn an_app_in_front_counts_as_a_steal() {
        let (switched, _) = run(vec![Ok(None), on("HDMI_1")], 5);
        assert_eq!(switched, 1);
    }

    #[test]
    fn a_failed_read_reconnects_without_switching() {
        let (switched, state) = run(vec![Err("closed".into()), on("HDMI_1")], 5);
        assert_eq!(switched, 0);
        assert_eq!(state.connects, 2);
    }

    #[test]
    fn a_canceled_guard_sends_nothing() {
        let state = Rc::new(RefCell::new(State {
            reads: vec![on("HDMI_2")],
            ..Default::default()
        }));
        let switched = hold(|| Fake(state.clone(), false), "HDMI_1", || true, || true);
        assert_eq!(switched, 0);
        assert!(state.borrow().switches.is_empty());
    }

    #[test]
    fn start_replaces_and_stop_joins() {
        use std::sync::atomic::AtomicUsize;
        static RUNS: AtomicUsize = AtomicUsize::new(0);
        start(0, |_, _| unreachable!("a zero-second guard never starts"));
        assert!(!active());
        start(30, |cancel, until| {
            while sleep_until(cancel, until) {}
            RUNS.fetch_add(1, Ordering::SeqCst);
        });
        assert!(active());
        stop();
        assert!(!active());
        assert_eq!(RUNS.load(Ordering::SeqCst), 1);
        start(1, |cancel, until| while sleep_until(cancel, until) {});
        assert!(wait());
        assert!(!wait());
        let path = marker().unwrap();
        assert!(!path.exists(), "a finished guard clears its marker");
        // Another process's TV request deletes the marker: the guard ends at its next poll.
        let begun = Instant::now();
        start(30, |stop, until| while sleep_until(stop, until) {});
        assert!(path.exists());
        std::fs::remove_file(&path).unwrap();
        assert!(wait());
        assert!(begun.elapsed() < Duration::from_secs(5));
        // So does a newer guard's token.
        start(30, |stop, until| while sleep_until(stop, until) {});
        std::fs::write(&path, "another-guard").unwrap();
        assert!(wait());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "another-guard");
        stop();
        assert!(!path.exists());
    }
}
