//! Temporary display discovery with mandatory restoration after any attempted change.
use crate::{
    display::display_apply,
    display::display_editor::Options,
    display::displays::{DesktopMode, Layout},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Preparing,
    Waking,
    Enabling,
    Reading,
    Restoring,
    Verifying,
}
impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preparing => "Preparing detection…",
            Self::Waking => "Waking TV and selecting PC input…",
            Self::Enabling => "Enabling display temporarily…",
            Self::Reading => "Reading display options…",
            Self::Restoring => "Restoring your layout…",
            Self::Verifying => "Checking your restored layout…",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Learned {
    pub device_path: String,
    pub name: String,
    pub options: Options,
    pub desktop: DesktopMode,
}
#[derive(Clone)]
pub struct Report<S> {
    pub learned: Option<Learned>,
    pub message: String,
    pub details: Vec<String>,
    pub recovery: Option<(Layout, S)>,
}
pub trait Backend {
    type Snapshot;
    fn snapshot(&mut self) -> Result<(Layout, Self::Snapshot), String>;
    fn wakes_tv(&self) -> bool {
        false
    }
    fn prepare_tv(&mut self, _cancel: &AtomicBool, _deadline: Instant) -> Result<(), String> {
        Ok(())
    }
    fn enable(
        &mut self,
        path: &str,
        before: &Layout,
        snapshot: &Self::Snapshot,
    ) -> Result<(), String>;
    fn current(&mut self) -> Result<Layout, String>;
    fn inspect(&mut self, path: &str) -> Result<Learned, String>;
    fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), String>;
}
fn caught<T>(action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(action))
        .unwrap_or_else(|_| Err("Display detection stopped unexpectedly.".into()))
}
fn check(cancel: &AtomicBool, deadline: Instant) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Detection canceled.".into())
    } else if Instant::now() >= deadline {
        Err("Display detection timed out.".into())
    } else {
        Ok(())
    }
}
/// Verify original enabled displays and allow only the detection target to be added.
pub fn verify_extended(before: &Layout, current: &Layout, path: &str) -> Result<(), String> {
    let target = current
        .monitors
        .iter()
        .find(|m| {
            m.device_path
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(path))
        })
        .filter(|m| m.enabled && m.desktop.is_some())
        .ok_or("The display has not become active. Turn it on and select the PC input.")?;
    let mut expected = before.clone();
    if let Some(original) = expected.monitors.iter_mut().find(|m| {
        m.device_path
            .as_deref()
            .is_some_and(|p| p.eq_ignore_ascii_case(path))
    }) {
        if !original.enabled {
            *original = target.clone();
        }
    } else {
        expected.monitors.push(target.clone());
    }
    display_apply::verify(&expected, current)
        .map_err(|e| format!("Detection would change an existing screen. {e}"))
}
pub fn restore<B: Backend>(
    backend: &mut B,
    before: Layout,
    snapshot: B::Snapshot,
    mut progress: impl FnMut(Stage),
) -> Report<B::Snapshot> {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| progress(Stage::Restoring)));
    let restoration = caught(|| backend.restore(&snapshot));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| progress(Stage::Verifying)));
    let verification = caught(|| {
        backend
            .current()
            .and_then(|layout| display_apply::verify(&before, &layout))
    });
    let mut details = Vec::new();
    if let Err(e) = restoration {
        details.push(e);
    }
    if let Err(e) = verification {
        details.push(e);
    }
    if details.is_empty() {
        Report {
            learned: None,
            message: "Your previous layout is restored. You can retry detection.".into(),
            details,
            recovery: None,
        }
    } else {
        Report {
            learned: None,
            message: "Your layout could not be fully restored. Retry restoration before saving."
                .into(),
            details,
            recovery: Some((before, snapshot)),
        }
    }
}
pub fn detect<B: Backend>(
    backend: &mut B,
    path: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Stage),
) -> Report<B::Snapshot> {
    let _operation = crate::diagnostics::Operation::start("display.detect", Some(path));
    let mut progress = |stage| {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| progress(stage))).is_err() {
            cancel.store(true, Ordering::Release);
        }
    };
    progress(Stage::Preparing);
    let (before, snapshot) = match caught(|| backend.snapshot()) {
        Ok(value) => value,
        Err(e) => {
            return Report {
                learned: None,
                message: "Detection could not start. No display changes were made.".into(),
                details: vec![e],
                recovery: None,
            };
        }
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut touched = false;
    let result = caught(|| {
        check(cancel, deadline)?;
        if backend.wakes_tv() {
            touched = true; // Arrival can change Windows topology even before our setter.
            progress(Stage::Waking);
            check(cancel, deadline)?;
            backend.prepare_tv(cancel, deadline)?;
        }
        check(cancel, deadline)?;
        let already_enabled = before.monitors.iter().any(|m| {
            m.enabled
                && m.device_path
                    .as_deref()
                    .is_some_and(|p| p.eq_ignore_ascii_case(path))
        });
        if !already_enabled {
            progress(Stage::Enabling);
            check(cancel, deadline)?;
            touched = true; // A failing setter may already have changed the desktop.
            backend.enable(path, &before, &snapshot)?;
        }
        loop {
            check(cancel, deadline)?;
            let current = backend.current()?;
            if current.monitors.iter().any(|m| {
                m.enabled
                    && m.device_path
                        .as_deref()
                        .is_some_and(|p| p.eq_ignore_ascii_case(path))
            }) {
                verify_extended(&before, &current, path)?;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        progress(Stage::Reading);
        check(cancel, deadline)?;
        let learned = backend.inspect(path)?;
        check(cancel, deadline)?;
        if learned.options.modes.is_empty() || learned.options.warning.is_some() {
            return Err("Windows returned only partial display information. Check the TV and PC input, then retry.".into());
        }
        Ok(learned)
    });
    let mut recovery = if touched {
        restore(backend, before, snapshot, &mut progress)
    } else {
        Report {
            learned: None,
            message: String::new(),
            details: vec![],
            recovery: None,
        }
    };
    match result {
        Ok(learned) if recovery.recovery.is_none() => {
            if cancel.load(Ordering::Acquire) {
                recovery.message = "Detection canceled. Your previous layout is restored.".into();
            } else {
                recovery.learned = Some(learned);
                recovery.message = if touched {
                    "Display options detected. Your previous layout is restored."
                } else {
                    "Display options refreshed. Your screens are unchanged."
                }
                .into();
            }
        }
        Ok(_) => {}
        Err(error) => {
            if recovery.recovery.is_none() {
                recovery.message = if touched {
                    "Detection stopped. Your previous layout is restored."
                } else {
                    "Detection stopped. Your screens are unchanged."
                }
                .into();
            }
            recovery.details.insert(0, error);
        }
    }
    crate::diagnostics::event(
        "display.detect.result",
        if recovery.learned.is_some() {
            "ok"
        } else {
            "stopped"
        },
        &recovery.details.join("; "),
    );
    recovery
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        display::display_editor::Mode,
        display::displays::{Monitor, RefreshRate},
    };
    fn desktop(x: i32) -> DesktopMode {
        DesktopMode {
            source_adapter_luid: 1,
            source_id: x as u32,
            source_name: "fixture".into(),
            x,
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
        }
    }
    fn layout() -> Layout {
        Layout {
            monitors: vec![
                Monitor {
                    adapter_luid: 1,
                    target_id: 0,
                    device_path: Some("desk".into()),
                    name: "Desk".into(),
                    connected: true,
                    hdr: Some(false),
                    enabled: true,
                    desktop: Some(desktop(0)),
                },
                Monitor {
                    adapter_luid: 1,
                    target_id: 1,
                    device_path: Some("tv".into()),
                    name: "TV".into(),
                    connected: true,
                    hdr: None,
                    enabled: false,
                    desktop: None,
                },
            ],
        }
    }
    struct Fake {
        before: Layout,
        current: Layout,
        events: Vec<&'static str>,
        fail: &'static str,
        wake: bool,
    }
    impl Fake {
        fn new(fail: &'static str) -> Self {
            let before = layout();
            Self {
                current: before.clone(),
                before,
                events: vec![],
                fail,
                wake: false,
            }
        }
    }
    impl Backend for Fake {
        type Snapshot = Layout;
        fn snapshot(&mut self) -> Result<(Layout, Layout), String> {
            self.events.push("snapshot");
            Ok((self.before.clone(), self.before.clone()))
        }
        fn wakes_tv(&self) -> bool {
            self.wake
        }
        fn prepare_tv(&mut self, _: &AtomicBool, _: Instant) -> Result<(), String> {
            self.events.push("wake");
            self.current.monitors[1].enabled = true;
            self.current.monitors[1].desktop = Some(desktop(1920));
            if self.fail == "wake" {
                Err("wake failed".into())
            } else {
                Ok(())
            }
        }
        fn enable(&mut self, _: &str, _: &Layout, _: &Layout) -> Result<(), String> {
            self.events.push("enable");
            self.current.monitors[1].enabled = true;
            self.current.monitors[1].desktop = Some(desktop(1920));
            if self.fail == "enable" {
                Err("partial enable".into())
            } else {
                Ok(())
            }
        }
        fn current(&mut self) -> Result<Layout, String> {
            self.events.push("current");
            Ok(self.current.clone())
        }
        fn inspect(&mut self, path: &str) -> Result<Learned, String> {
            self.events.push("inspect");
            if self.fail == "panic" {
                panic!("mock driver panic")
            }
            if self.fail == "inspect" {
                return Err("read failed".into());
            }
            let desktop = self.current.monitors[1].desktop.clone().unwrap();
            Ok(Learned {
                device_path: path.into(),
                name: "TV".into(),
                options: Options {
                    modes: vec![Mode {
                        width: 1920,
                        height: 1080,
                        refresh: desktop.refresh_rate.clone(),
                    }],
                    warning: None,
                    hdr: Some(false),
                },
                desktop,
            })
        }
        fn restore(&mut self, snapshot: &Layout) -> Result<(), String> {
            self.events.push("restore");
            if self.fail == "restore" {
                return Err("restore failed".into());
            }
            self.current = snapshot.clone();
            Ok(())
        }
    }
    #[test]
    fn success_learns_only_after_restoration_and_keeps_original_screens() {
        let mut backend = Fake::new("");
        let mut stages = vec![];
        let result = detect(&mut backend, "tv", &AtomicBool::new(false), |s| {
            stages.push(s)
        });
        assert!(result.learned.is_some());
        assert!(result.recovery.is_none());
        assert_eq!(backend.current, backend.before);
        assert_eq!(
            stages,
            [
                Stage::Preparing,
                Stage::Enabling,
                Stage::Reading,
                Stage::Restoring,
                Stage::Verifying
            ]
        );
    }
    #[test]
    fn partial_writes_read_failure_panic_and_cancel_all_restore() {
        for failure in ["enable", "inspect", "panic"] {
            let mut backend = Fake::new(failure);
            let report = detect(&mut backend, "tv", &AtomicBool::new(false), |_| {});
            assert!(report.learned.is_none());
            assert!(report.recovery.is_none());
            assert_eq!(backend.current, backend.before);
            assert!(backend.events.contains(&"restore"));
        }
        let mut backend = Fake::new("");
        let cancel = AtomicBool::new(false);
        let report = detect(&mut backend, "tv", &cancel, |stage| {
            if stage == Stage::Reading {
                cancel.store(true, Ordering::Release)
            }
        });
        assert!(report.learned.is_none());
        assert_eq!(backend.current, backend.before);
    }
    #[test]
    fn failed_restoration_retains_snapshot_and_can_be_retried() {
        let mut backend = Fake::new("restore");
        let report = detect(&mut backend, "tv", &AtomicBool::new(false), |_| {});
        assert!(report.learned.is_none());
        let (before, snapshot) = report.recovery.unwrap();
        backend.fail = "";
        let restored = restore(&mut backend, before, snapshot, |_| {});
        assert!(restored.recovery.is_none());
        assert_eq!(backend.current, backend.before);
    }
    #[test]
    fn tv_arrival_is_restored_even_when_wake_preparation_fails() {
        let mut backend = Fake::new("wake");
        backend.wake = true;
        let report = detect(&mut backend, "tv", &AtomicBool::new(false), |_| {});
        assert!(report.learned.is_none());
        assert_eq!(backend.current, backend.before);
        assert!(!backend.events.contains(&"enable"));
        assert!(backend.events.contains(&"restore"));
    }
    #[test]
    fn pre_canceled_or_lost_progress_never_applies_and_deadlines_are_checked() {
        let mut backend = Fake::new("");
        let report = detect(&mut backend, "tv", &AtomicBool::new(true), |_| {});
        assert!(report.learned.is_none());
        assert!(!backend.events.contains(&"enable"));
        let cancel = AtomicBool::new(false);
        let report = detect(&mut backend, "tv", &cancel, |stage| {
            if stage == Stage::Enabling {
                cancel.store(true, Ordering::Release);
            }
        });
        assert!(report.learned.is_none());
        assert!(!backend.events.contains(&"enable"));
        assert!(check(&AtomicBool::new(false), Instant::now()).is_err());
        let cancel = AtomicBool::new(false);
        let report = detect(&mut backend, "tv", &cancel, |_| {
            panic!("progress unavailable")
        });
        assert!(report.learned.is_none());
        assert!(!backend.events.contains(&"enable"));
    }
    #[test]
    fn changed_original_screen_is_rejected() {
        let before = layout();
        let mut current = before.clone();
        current.monitors[1].enabled = true;
        current.monitors[1].desktop = Some(desktop(1920));
        current.monitors[0].desktop.as_mut().unwrap().x = 10;
        assert!(verify_extended(&before, &current, "tv").is_err());
    }
}
