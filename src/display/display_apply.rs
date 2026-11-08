//! Framework-independent display transaction and confirmation deadline.
use crate::display::displays::{Layout, Monitor};
use std::{
    collections::BTreeMap,
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Keep,
    Revert,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Unchanged,
    Kept,
    Reverted,
    Failed,
    RecoveryFailed,
}
#[derive(Clone, Debug)]
pub struct Report {
    pub outcome: Outcome,
    pub message: String,
    pub details: Vec<String>,
}
impl Report {
    pub fn succeeded(&self) -> bool {
        matches!(self.outcome, Outcome::Unchanged | Outcome::Kept)
    }
}
pub trait Backend {
    type Snapshot;
    type Plan;
    fn snapshot(&mut self) -> Result<(Layout, Self::Snapshot), String>;
    fn prepare(
        &mut self,
        wanted: &Layout,
        before: &Layout,
        snapshot: &Self::Snapshot,
    ) -> Result<Self::Plan, String>;
    fn validate(&mut self, plan: &mut Self::Plan) -> Result<(), String>;
    fn apply_temporary(&mut self, plan: &Self::Plan) -> Result<(), String>;
    fn current(&mut self) -> Result<Layout, String>;
    fn commit(&mut self, plan: &Self::Plan) -> Result<(), String>;
    fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), String>;
}
pub fn identity(m: &Monitor) -> Result<String, String> {
    m.device_path
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(|p| p.to_lowercase())
        .ok_or_else(|| {
            format!(
                "“{}” has no Windows monitor identity. Capture it again before applying.",
                m.name
            )
        })
}
pub fn same_rate(
    a: &crate::display::displays::RefreshRate,
    b: &crate::display::displays::RefreshRate,
) -> bool {
    a.denominator != 0
        && b.denominator != 0
        && u64::from(a.numerator) * u64::from(b.denominator)
            == u64::from(b.numerator) * u64::from(a.denominator)
}
fn indexed(layout: &Layout) -> Result<BTreeMap<String, &Monitor>, String> {
    let mut map = BTreeMap::new();
    for m in &layout.monitors {
        let key = match identity(m) {
            Ok(key) => key,
            Err(e) if m.enabled => return Err(e),
            Err(_) => continue,
        };
        if map.insert(key, m).is_some() {
            return Err(format!(
                "Windows monitor identity for “{}” is ambiguous. Reconnect or recapture your monitors.",
                m.name
            ));
        }
    }
    Ok(map)
}
/// Compare user-visible settings, not transient adapter IDs, display numbers, or labels.
pub fn verify(wanted: &Layout, current: &Layout) -> Result<(), String> {
    wanted.validate()?;
    let expected = indexed(wanted)?;
    let actual = indexed(current)?;
    for (key, m) in &expected {
        let found = actual.get(key).copied();
        if !m.enabled {
            if found.is_some_and(|n| n.enabled) {
                return Err(format!("“{}” is still enabled.", m.name));
            }
            continue;
        }
        let found = found
            .filter(|n| n.enabled)
            .ok_or_else(|| format!("“{}” is not enabled.", m.name))?;
        if let Some(hdr) = m.hdr
            && found.hdr != Some(hdr)
        {
            return Err(format!("HDR for “{}” does not match the preset.", m.name));
        }
        let (a, b) = (
            m.desktop.as_ref().ok_or("Saved display mode missing.")?,
            found
                .desktop
                .as_ref()
                .ok_or("Current display mode missing.")?,
        );
        if (a.x, a.y, a.width, a.height, a.rotation_degrees)
            != (b.x, b.y, b.width, b.height, b.rotation_degrees)
            || !same_rate(&a.refresh_rate, &b.refresh_rate)
        {
            return Err(format!(
                "“{}” did not match the saved position, resolution, orientation, or refresh rate.",
                m.name
            ));
        }
    }
    for (key, m) in &actual {
        if m.enabled && !expected.get(key).is_some_and(|p| p.enabled) {
            return Err(format!("Unexpected enabled display “{}”.", m.name));
        }
    }
    let enabled = expected
        .iter()
        .filter(|(_, m)| m.enabled)
        .collect::<Vec<_>>();
    for (i, (key, a)) in enabled.iter().enumerate() {
        for (other_key, b) in &enabled[..i] {
            let ad = a.desktop.as_ref().unwrap();
            let bd = b.desktop.as_ref().unwrap();
            let ac = actual[*key].desktop.as_ref().unwrap();
            let bc = actual[*other_key].desktop.as_ref().unwrap();
            if ((ad.source_adapter_luid, ad.source_id) == (bd.source_adapter_luid, bd.source_id))
                != ((ac.source_adapter_luid, ac.source_id)
                    == (bc.source_adapter_luid, bc.source_id))
            {
                return Err(
                    "Windows did not preserve the saved mirrored/extended display grouping.".into(),
                );
            }
        }
    }
    Ok(())
}
fn report(outcome: Outcome, message: &str, details: Vec<String>) -> Report {
    crate::diagnostics::event(
        "display.result",
        &format!("{outcome:?}"),
        &format!("{message} {}", details.join("; ")),
    );
    Report {
        outcome,
        message: message.into(),
        details,
    }
}
fn recover<B: Backend>(
    backend: &mut B,
    snapshot: &B::Snapshot,
    before: &Layout,
    cause: Option<String>,
) -> Report {
    let restoration = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::diagnostics::measure("display.restore", "", || backend.restore(snapshot))
    }))
    .unwrap_or_else(|_| Err("Display restoration stopped unexpectedly.".into()));
    let verification = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::diagnostics::measure("display.restore.verify", "", || {
            backend.current().and_then(|layout| verify(before, &layout))
        })
    }))
    .unwrap_or_else(|_| Err("Could not verify restored displays.".into()));
    let mut details = cause.clone().into_iter().collect::<Vec<_>>();
    if let Err(e) = restoration {
        details.push(format!("Restore reported: {e}"));
    }
    match verification {
        Ok(()) => report(
            if cause.is_some() {
                Outcome::Failed
            } else {
                Outcome::Reverted
            },
            "Previous display settings restored.",
            details,
        ),
        Err(e) => {
            details.push(e);
            report(
                Outcome::RecoveryFailed,
                "Display recovery is incomplete.",
                details,
            )
        }
    }
}
pub fn apply<B: Backend>(
    backend: &mut B,
    wanted: &Layout,
    mut confirm: impl FnMut() -> bool,
) -> Report {
    let _operation = crate::diagnostics::Operation::start("display.apply", None);
    let preflight = (|| {
        wanted.validate()?;
        indexed(wanted)?;
        let (before, snapshot) =
            crate::diagnostics::measure("display.snapshot", "", || backend.snapshot())?;
        let mut plan = crate::diagnostics::measure("display.prepare", "", || {
            backend.prepare(wanted, &before, &snapshot)
        })?;
        if verify(wanted, &before).is_ok() {
            return Ok((before, snapshot, plan, true));
        }
        crate::diagnostics::measure("display.validate", "", || backend.validate(&mut plan))?;
        Ok((before, snapshot, plan, false))
    })();
    let (before, snapshot, plan, unchanged) = match preflight {
        Ok(p) => p,
        Err(e) => return report(Outcome::Failed, "No display changes made.", vec![e]),
    };
    if unchanged {
        return report(
            Outcome::Unchanged,
            "This display layout is already active.",
            vec![],
        );
    }
    // A failed setter can already have changed the desktop. Every attempted write
    // and any panic after it must go through restoration and readback.
    let changed =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<bool, String> {
            crate::diagnostics::measure("display.apply.temporary", "", || {
                backend.apply_temporary(&plan)
            })?;
            crate::diagnostics::measure("display.verify", "", || {
                verify(wanted, &backend.current()?)
            })?;
            crate::diagnostics::event("display.confirmation", "waiting", "");
            if !confirm() {
                crate::diagnostics::event("display.confirmation", "revert", "");
                return Ok(false);
            }
            crate::diagnostics::measure("display.verify", "", || {
                verify(wanted, &backend.current()?)
            })?;
            crate::diagnostics::event("display.confirmation", "keep", "");
            crate::diagnostics::measure("display.commit", "", || backend.commit(&plan))?;
            crate::diagnostics::measure("display.verify", "", || {
                verify(wanted, &backend.current()?)
            })?;
            Ok(true)
        }))
        .unwrap_or_else(|_| Err("The display operation stopped unexpectedly.".into()));
    match changed {
        Ok(true) => report(Outcome::Kept, "Display preset applied and kept.", vec![]),
        Ok(false) => recover(backend, &snapshot, &before, None),
        Err(e) => recover(backend, &snapshot, &before, Some(e)),
    }
}
/// Worker-owned monotonic deadline. UI delivery failure, dropped channels,
/// cancel, timeout, and late confirmation all choose reversion.
pub fn wait_for_confirmation(
    rx: &Receiver<Decision>,
    timeout: Duration,
    mut tick: impl FnMut(u64) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() != 0);
        if !tick(seconds) {
            return false;
        }
        match rx.recv_timeout(remaining.min(Duration::from_secs(1))) {
            Ok(Decision::Keep) => return Instant::now() < deadline,
            Ok(Decision::Revert) | Err(RecvTimeoutError::Disconnected) => return false,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::display::displays::{DesktopMode, RefreshRate};
    pub fn layout(x: i32) -> Layout {
        Layout {
            monitors: vec![
                Monitor {
                    adapter_luid: 1,
                    target_id: 1,
                    device_path: Some("monitor-a".into()),
                    name: "Main".into(),
                    connected: true,
                    hdr: None,
                    enabled: true,
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
                            numerator: 60000,
                            denominator: 1001,
                        },
                        rotation_degrees: 0,
                    }),
                },
                Monitor {
                    adapter_luid: 1,
                    target_id: 2,
                    device_path: Some("monitor-b".into()),
                    name: "Other".into(),
                    connected: true,
                    hdr: None,
                    enabled: true,
                    desktop: Some(DesktopMode {
                        source_adapter_luid: 1,
                        source_id: 1,
                        source_name: "DISPLAY2".into(),
                        x: 1920,
                        y: x,
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
                },
            ],
        }
    }
    struct Fake {
        current: Layout,
        writes: Vec<&'static str>,
        fault: &'static str,
    }
    impl Backend for Fake {
        type Snapshot = Layout;
        type Plan = Layout;
        fn snapshot(&mut self) -> Result<(Layout, Layout), String> {
            Ok((self.current.clone(), self.current.clone()))
        }
        fn prepare(&mut self, wanted: &Layout, _: &Layout, _: &Layout) -> Result<Layout, String> {
            if self.fault == "prepare" {
                Err("missing monitor".into())
            } else {
                Ok(wanted.clone())
            }
        }
        fn validate(&mut self, _: &mut Layout) -> Result<(), String> {
            if self.fault == "validate" {
                Err("unsupported".into())
            } else {
                Ok(())
            }
        }
        fn apply_temporary(&mut self, p: &Layout) -> Result<(), String> {
            self.writes.push("temporary");
            self.current = p.clone();
            match self.fault {
                "partial" => Err("driver failed after change".into()),
                "panic" => panic!("driver wrapper panic"),
                "mismatch" => {
                    self.current.monitors[1].desktop.as_mut().unwrap().y = 999;
                    Ok(())
                }
                _ => Ok(()),
            }
        }
        fn current(&mut self) -> Result<Layout, String> {
            Ok(self.current.clone())
        }
        fn commit(&mut self, _: &Layout) -> Result<(), String> {
            self.writes.push("commit");
            if self.fault == "commit" {
                Err("database write failed".into())
            } else {
                Ok(())
            }
        }
        fn restore(&mut self, p: &Layout) -> Result<(), String> {
            self.writes.push("restore");
            if self.fault == "restore" {
                Err("unplugged".into())
            } else {
                self.current = p.clone();
                Ok(())
            }
        }
    }
    fn fake(fault: &'static str) -> Fake {
        Fake {
            current: layout(0),
            writes: vec![],
            fault,
        }
    }
    #[test]
    fn hdr_is_verified_and_restored_with_the_transaction() {
        let mut before = layout(0);
        before.monitors[0].hdr = Some(false);
        let mut wanted = before.clone();
        wanted.monitors[0].hdr = Some(true);
        assert!(verify(&wanted, &before).is_err());
        let mut backend = Fake {
            current: before.clone(),
            writes: vec![],
            fault: "",
        };
        assert_eq!(apply(&mut backend, &wanted, || true).outcome, Outcome::Kept);
        assert_eq!(backend.current.monitors[0].hdr, Some(true));
        let mut backend = Fake {
            current: before.clone(),
            writes: vec![],
            fault: "commit",
        };
        let result = apply(&mut backend, &wanted, || true);
        assert_eq!(result.outcome, Outcome::Failed);
        assert_eq!(backend.current.monitors[0].hdr, Some(false));
        let mut unset = wanted.clone();
        unset.monitors[0].hdr = None;
        assert!(verify(&unset, &before).is_ok());
    }
    #[test]
    fn no_op_and_preflight_failures_never_write() {
        for fault in ["prepare", "validate"] {
            let mut b = fake(fault);
            assert_eq!(apply(&mut b, &layout(50), || true).outcome, Outcome::Failed);
            assert!(b.writes.is_empty());
        }
        let mut b = fake("");
        assert_eq!(
            apply(&mut b, &layout(0), || panic!("no confirmation needed")).outcome,
            Outcome::Unchanged
        );
        assert!(b.writes.is_empty());
    }
    #[test]
    fn keep_commits_only_after_verified_confirmation() {
        let mut b = fake("");
        assert_eq!(apply(&mut b, &layout(50), || true).outcome, Outcome::Kept);
        assert_eq!(b.writes, vec!["temporary", "commit"]);
        verify(&layout(50), &b.current).unwrap();
    }
    #[test]
    fn cancel_timeout_or_ui_loss_restores_without_commit() {
        for confirmed in [false, false] {
            let mut b = fake("");
            assert_eq!(
                apply(&mut b, &layout(50), || confirmed).outcome,
                Outcome::Reverted
            );
            assert_eq!(b.writes, vec!["temporary", "restore"]);
            verify(&layout(0), &b.current).unwrap();
        }
    }
    #[test]
    fn partial_set_verify_commit_and_panic_failures_restore() {
        for fault in ["partial", "mismatch", "commit", "panic"] {
            let mut b = fake(fault);
            assert_eq!(apply(&mut b, &layout(50), || true).outcome, Outcome::Failed);
            assert_eq!(b.writes.last(), Some(&"restore"));
            verify(&layout(0), &b.current).unwrap();
        }
    }
    #[test]
    fn recovery_failure_is_not_reported_as_success() {
        let mut b = fake("restore");
        assert_eq!(
            apply(&mut b, &layout(50), || false).outcome,
            Outcome::RecoveryFailed
        );
    }
    #[test]
    fn identity_clone_and_fractional_checks_are_strict() {
        let wanted = layout(0);
        let mut current = wanted.clone();
        current.monitors[0].device_path = Some("MONITOR-A".into());
        current.monitors[0].adapter_luid = 22;
        verify(&wanted, &current).unwrap();
        current.monitors[1].desktop.as_mut().unwrap().source_id = 0;
        assert!(verify(&wanted, &current).is_err());
        let mut current = wanted.clone();
        current.monitors[0].desktop.as_mut().unwrap().refresh_rate = RefreshRate {
            numerator: 60,
            denominator: 1,
        };
        assert!(verify(&wanted, &current).is_err());
        current.monitors[0].device_path = None;
        assert!(verify(&wanted, &current).is_err());
    }
    #[test]
    fn worker_deadline_and_channel_loss_revert() {
        let (tx, rx) = std::sync::mpsc::channel();
        assert!(!wait_for_confirmation(
            &rx,
            Duration::from_millis(20),
            |_| true
        ));
        drop(tx);
        assert!(!wait_for_confirmation(&rx, Duration::from_secs(1), |_| {
            true
        }));
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Decision::Keep).unwrap();
        assert!(!wait_for_confirmation(&rx, Duration::from_secs(1), |_| {
            false
        }));
        assert!(wait_for_confirmation(&rx, Duration::from_secs(1), |_| true));
        assert!(!wait_for_confirmation(&rx, Duration::ZERO, |_| true));
    }
}
