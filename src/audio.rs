//! Device-independent audio transaction logic. GUI and COM types do not belong here.
use serde::Serialize;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Role {
    Console,
    Multimedia,
    Communications,
}
pub const DEFAULT_ROLES: [Role; 3] = [Role::Console, Role::Multimedia, Role::Communications];

#[derive(Clone, Debug, Serialize)]
pub struct Output {
    pub id: String,
    pub name: String,
    pub state: String,
    pub active: bool,
    pub console: bool,
    pub multimedia: bool,
    pub communications: bool,
    pub volume_percent: Option<u8>,
    /// Read for active endpoints only.
    pub muted: Option<bool>,
}

/// A device chosen by endpoint ID, with the name it had when chosen.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSelection {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug)]
pub struct Request {
    /// None leaves the default output as is; volume and mute then apply to it.
    pub output_id: Option<String>,
    pub volume_percent: Option<u8>,
    pub mute: Option<bool>,
    pub communications_output_id: Option<String>,
    pub input: Option<InputRequest>,
}
#[derive(Clone, Debug)]
pub struct InputRequest {
    pub device_id: String,
    pub volume_percent: Option<u8>,
    pub mute: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailurePolicy {
    Revert,
    KeepChanges,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    NoChanges,
    PreflightFailed,
    Reverted,
    RecoveryIncomplete,
    ChangesKept,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub outcome: Outcome,
    pub message: String,
    pub details: Vec<String>,
}
impl Report {
    pub fn succeeded(&self) -> bool {
        matches!(self.outcome, Outcome::Applied | Outcome::NoChanges)
    }
}

pub trait AudioBackend {
    fn outputs(&mut self) -> Result<Vec<Output>>;
    fn inputs(&mut self) -> Result<Vec<Output>> {
        Err("Input device access is unavailable.".into())
    }
    fn resolve_id(&self, id: &str, _input: bool) -> String {
        id.to_owned()
    }
    fn default_input(&mut self, _role: Role) -> Result<String> {
        Err("Input defaults cannot be read.".into())
    }
    fn set_default_input(&mut self, _role: Role, _id: &str) -> Result<()> {
        Err("Input defaults cannot be changed.".into())
    }
    fn default_output(&mut self, role: Role) -> Result<String>;
    fn set_default(&mut self, role: Role, id: &str) -> Result<()>;
    fn volume(&mut self, id: &str) -> Result<f32>;
    fn set_volume(&mut self, id: &str, scalar: f32) -> Result<()>;
    fn mute(&mut self, _id: &str) -> Result<bool> {
        Err("Mute cannot be read.".into())
    }
    fn set_mute(&mut self, _id: &str, _muted: bool) -> Result<()> {
        Err("Mute cannot be changed.".into())
    }
}

/// Changes one output level without calling the routing or mute APIs.
pub fn set_output_volume(
    backend: &mut impl AudioBackend,
    target: &str,
    percent: u8,
) -> Result<String> {
    if percent > 100 {
        return Err("Volume must be between 0 and 100%.".into());
    }
    let outputs = backend.outputs()?;
    let resolved = backend.resolve_id(target, false);
    let device = if let Some(exact) = outputs.iter().find(|d| d.id == resolved) {
        exact
    } else {
        let wanted = crate::cli_name::cli_name(target);
        let mut matches = outputs
            .iter()
            .filter(|d| !wanted.is_empty() && crate::cli_name::cli_name(&d.name) == wanted);
        let first = matches
            .next()
            .ok_or("Output not found. Run director audio outputs.")?;
        if matches.next().is_some() {
            return Err("More than one output has that name. Run director audio outputs and use the endpoint ID.".into());
        }
        first
    };
    if !device.active {
        return Err(format!("{} is offline.", device.name));
    }
    let previous = backend.volume(&device.id)?;
    if !previous.is_finite() || !(0.0..=1.0).contains(&previous) {
        return Err("Windows returned an invalid device volume.".into());
    }
    let scalar = f32::from(percent) / 100.0;
    let verify = |backend: &mut _, expected: f32| -> Result<()> {
        verify_volume(backend, &device.id, expected)
    };
    if (previous - scalar).abs() > 0.005 {
        let result = backend
            .set_volume(&device.id, scalar)
            .and_then(|()| verify(backend, scalar));
        if let Err(error) = result {
            return match backend
                .set_volume(&device.id, previous)
                .and_then(|()| verify(backend, previous))
            {
                Ok(()) => Err(format!("{error} Previous volume restored.")),
                Err(recovery) => Err(format!(
                    "{error} Could not restore the previous volume: {recovery}"
                )),
            };
        }
    }
    Ok(format!("{} volume: {percent}%.", device.name))
}

fn verify_volume(backend: &mut impl AudioBackend, id: &str, expected: f32) -> Result<()> {
    let actual = backend.volume(id)?;
    if actual.is_finite() && (actual - expected).abs() <= 0.01 {
        Ok(())
    } else {
        Err("Windows did not confirm the requested device volume.".into())
    }
}

pub fn apply(
    backend: &mut impl AudioBackend,
    request: &Request,
    policy: FailurePolicy,
    mut progress: impl FnMut(&str),
) -> Report {
    let _operation = crate::diagnostics::Operation::start("audio.apply", None);
    let report = apply_inner(backend, request, policy, |step| {
        crate::diagnostics::event("audio.progress", "started", step);
        progress(step);
    });
    crate::diagnostics::event(
        "audio.result",
        if report.succeeded() { "ok" } else { "error" },
        &format!("{} {}", report.message, report.details.join("; ")),
    );
    report
}
#[derive(Clone)]
struct Route {
    input: bool,
    role: Role,
    desired: String,
    previous: String,
}
struct Level {
    id: String,
    percent: u8,
    previous: f32,
}
struct Mute {
    id: String,
    muted: bool,
    previous: bool,
}
fn default(backend: &mut impl AudioBackend, input: bool, role: Role) -> Result<String> {
    if input {
        backend.default_input(role)
    } else {
        backend.default_output(role)
    }
}
fn assign(backend: &mut impl AudioBackend, input: bool, role: Role, id: &str) -> Result<()> {
    if input {
        backend.set_default_input(role, id)
    } else {
        backend.set_default(role, id)
    }
}
fn verify_mute(backend: &mut impl AudioBackend, id: &str, expected: bool) -> Result<()> {
    if backend.mute(id)? == expected {
        Ok(())
    } else {
        Err("Windows did not confirm the requested mute state.".into())
    }
}
struct Plan {
    routes: Vec<Route>,
    levels: Vec<Level>,
    mutes: Vec<Mute>,
    name: String,
}
fn plan(backend: &mut impl AudioBackend, request: &Request) -> Result<Plan> {
    if request.output_id.as_deref().is_some_and(str::is_empty) {
        return Err("Choose an output device.".into());
    }
    if request.volume_percent.is_some_and(|v| v > 100)
        || request
            .input
            .as_ref()
            .is_some_and(|v| v.volume_percent.is_some_and(|p| p > 100))
    {
        return Err("Volume must be between 0 and 100%.".into());
    }
    if request.output_id.is_none()
        && request.volume_percent.is_none()
        && request.mute.is_none()
        && request.communications_output_id.is_none()
        && request.input.is_none()
    {
        return Err("Choose something to change.".into());
    }
    let outputs = backend.outputs()?;
    // No output means "leave the default as is": volume and mute apply to the current one.
    let output_id = match &request.output_id {
        Some(id) => backend.resolve_id(id, false),
        None => backend.default_output(Role::Console)?,
    };
    let output = outputs
        .iter()
        .find(|o| o.id == output_id && o.active)
        .ok_or("The selected output is no longer available.")?;
    let mut routes = vec![];
    let mut levels = vec![];
    let mut mutes = vec![];
    let communications = request
        .communications_output_id
        .as_deref()
        .map(|id| backend.resolve_id(id, false));
    if let Some(id) = &communications
        && !outputs.iter().any(|o| &o.id == id && o.active)
    {
        return Err("The selected communications output is no longer available.".into());
    }
    for role in DEFAULT_ROLES {
        let desired = match (role, &communications) {
            (Role::Communications, Some(id)) => id.clone(),
            _ if request.output_id.is_some() => output.id.clone(),
            _ => continue,
        };
        routes.push(Route {
            input: false,
            role,
            desired,
            previous: backend.default_output(role)?,
        });
    }
    if let Some(percent) = request.volume_percent {
        levels.push(Level {
            id: output.id.clone(),
            percent,
            previous: backend.volume(&output.id)?,
        });
    }
    if let Some(muted) = request.mute {
        mutes.push(Mute {
            id: output.id.clone(),
            muted,
            previous: backend.mute(&output.id)?,
        });
    }
    if let Some(input) = &request.input {
        let inputs = backend.inputs()?;
        let input_id = backend.resolve_id(&input.device_id, true);
        let target = inputs
            .iter()
            .find(|o| o.id == input_id && o.active)
            .ok_or("The selected microphone is no longer available. No audio settings changed.")?;
        for role in DEFAULT_ROLES {
            routes.push(Route {
                input: true,
                role,
                desired: target.id.clone(),
                previous: backend.default_input(role)?,
            });
        }
        if let Some(percent) = input.volume_percent {
            levels.push(Level {
                id: target.id.clone(),
                percent,
                previous: backend.volume(&target.id)?,
            });
        }
        if let Some(muted) = input.mute {
            mutes.push(Mute {
                id: target.id.clone(),
                muted,
                previous: backend.mute(&target.id)?,
            });
        }
    }
    if levels
        .iter()
        .any(|l| !l.previous.is_finite() || !(0.0..=1.0).contains(&l.previous))
    {
        return Err("Windows returned an invalid device volume.".into());
    }
    Ok(Plan {
        routes,
        levels,
        mutes,
        name: output.name.clone(),
    })
}
fn apply_inner(
    backend: &mut impl AudioBackend,
    request: &Request,
    policy: FailurePolicy,
    mut progress: impl FnMut(&str),
) -> Report {
    let Plan {
        routes,
        levels,
        mutes,
        name,
    } = match plan(backend, request) {
        Ok(v) => v,
        Err(e) => {
            return Report {
                outcome: Outcome::PreflightFailed,
                message: "No changes made.".into(),
                details: vec![e],
            };
        }
    };
    let mut attempted = vec![];
    let mut changed_levels = vec![];
    let mut changed_mutes = vec![];
    let result = (|| -> Result<()> {
        for (i, route) in routes.iter().enumerate() {
            if route.previous != route.desired {
                progress(if route.input {
                    "Changing default microphone…"
                } else {
                    "Changing default audio output…"
                });
                attempted.push(i);
                assign(backend, route.input, route.role, &route.desired)?;
            }
        }
        for (i, level) in levels.iter().enumerate() {
            let scalar = f32::from(level.percent) / 100.0;
            if (level.previous - scalar).abs() > 0.005 {
                progress("Setting device volume…");
                changed_levels.push(i);
                backend.set_volume(&level.id, scalar)?;
            }
        }
        for (i, mute) in mutes.iter().enumerate() {
            if mute.previous != mute.muted {
                progress(if mute.muted {
                    "Muting…"
                } else {
                    "Unmuting…"
                });
                changed_mutes.push(i);
                backend.set_mute(&mute.id, mute.muted)?;
            }
        }
        progress("Verifying audio settings…");
        for route in &routes {
            if default(backend, route.input, route.role)? != route.desired {
                return Err(format!(
                    "Windows did not confirm the {:?} {} device.",
                    route.role,
                    if route.input { "input" } else { "output" }
                ));
            }
        }
        for level in &levels {
            verify_volume(backend, &level.id, f32::from(level.percent) / 100.0)?;
        }
        for mute in &mutes {
            verify_mute(backend, &mute.id, mute.muted)?;
        }
        Ok(())
    })();
    let nothing_changed =
        attempted.is_empty() && changed_levels.is_empty() && changed_mutes.is_empty();
    match result {
        Ok(()) => Report {
            outcome: if nothing_changed {
                Outcome::NoChanges
            } else {
                Outcome::Applied
            },
            message: format!(
                "{}: {name}",
                if nothing_changed {
                    "Already set"
                } else {
                    "Applied"
                }
            ),
            details: vec![format!(
                "{} {}",
                if request.output_id.is_some() {
                    "Output roles verified (Console, Multimedia, Communications)."
                } else {
                    "The default output was left as is."
                },
                if request.input.is_some() {
                    "Selected microphone roles verified."
                } else {
                    "Microphone settings were left unchanged."
                }
            )],
        },
        Err(e) if nothing_changed => Report {
            outcome: Outcome::PreflightFailed,
            message: "No changes made.".into(),
            details: vec![e],
        },
        Err(e) if policy == FailurePolicy::KeepChanges => Report {
            outcome: Outcome::ChangesKept,
            message: "Audio change incomplete. Completed changes were kept.".into(),
            details: vec![e],
        },
        Err(e) => {
            progress("Restoring previous audio settings…");
            let mut errors = vec![e];
            for &i in changed_mutes.iter().rev() {
                let m = &mutes[i];
                if let Err(e) = backend.set_mute(&m.id, m.previous) {
                    errors.push(format!("Restore mute: {e}"));
                }
            }
            for &i in changed_levels.iter().rev() {
                let l = &levels[i];
                if let Err(e) = backend.set_volume(&l.id, l.previous) {
                    errors.push(format!("Restore device volume: {e}"));
                }
            }
            for &i in attempted.iter().rev() {
                let r = &routes[i];
                if let Err(e) = assign(backend, r.input, r.role, &r.previous) {
                    errors.push(format!(
                        "Restore {:?} {}: {e}",
                        r.role,
                        if r.input { "input" } else { "output" }
                    ));
                }
            }
            for &i in &attempted {
                let r = &routes[i];
                match default(backend, r.input, r.role) {
                    Ok(v) if v == r.previous => {}
                    Ok(_) => errors.push("A previous default device could not be verified.".into()),
                    Err(e) => errors.push(format!("Verify restored default: {e}")),
                }
            }
            for &i in &changed_levels {
                let l = &levels[i];
                match backend.volume(&l.id) {
                    Ok(v) if v.is_finite() && (v - l.previous).abs() <= 0.01 => {}
                    Ok(_) => errors.push("Restored device volume could not be verified.".into()),
                    Err(e) => errors.push(format!("Verify restored volume: {e}")),
                }
            }
            for &i in &changed_mutes {
                let m = &mutes[i];
                if let Err(e) = verify_mute(backend, &m.id, m.previous) {
                    errors.push(format!("Verify restored mute: {e}"));
                }
            }
            let complete = errors.len() == 1;
            Report {
                outcome: if complete {
                    Outcome::Reverted
                } else {
                    Outcome::RecoveryIncomplete
                },
                message: if complete {
                    "Audio change failed. Previous settings restored."
                } else {
                    "Audio change failed. Some previous settings could not be restored."
                }
                .into(),
                details: errors,
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        defaults: [String; 3],
        volume: f32,
        writes: Vec<String>,
        fail_role_once: bool,
        fail_volume_once: bool,
        fail_restore: bool,
        available: bool,
    }
    impl Fake {
        fn new() -> Self {
            Self {
                defaults: ["speakers".into(), "music".into(), "calls".into()],
                volume: 0.73,
                writes: vec![],
                fail_role_once: false,
                fail_volume_once: false,
                fail_restore: false,
                available: true,
            }
        }
    }
    impl AudioBackend for Fake {
        fn outputs(&mut self) -> Result<Vec<Output>> {
            Ok(vec![Output {
                communications: false,
                id: "headphones".into(),
                name: "Headphones".into(),
                state: "Available".into(),
                active: self.available,
                console: false,
                multimedia: false,
                volume_percent: Some(73),
                muted: None,
            }])
        }
        fn default_output(&mut self, role: Role) -> Result<String> {
            Ok(self.defaults[role as usize].clone())
        }
        fn set_default(&mut self, role: Role, id: &str) -> Result<()> {
            self.writes.push(format!("{role:?}:{id}"));
            if self.fail_restore && id == "music" {
                return Err("recovery device disappeared".into());
            }
            self.defaults[role as usize] = id.into();
            if role == Role::Multimedia && self.fail_role_once {
                self.fail_role_once = false;
                return Err("failure AFTER role changed".into());
            }
            Ok(())
        }
        fn volume(&mut self, _: &str) -> Result<f32> {
            Ok(self.volume)
        }
        fn set_volume(&mut self, id: &str, scalar: f32) -> Result<()> {
            self.writes.push(format!("volume:{id}"));
            self.volume = scalar;
            if self.fail_volume_once {
                self.fail_volume_once = false;
                return Err("partial volume failure".into());
            }
            Ok(())
        }
    }
    fn request(volume: Option<u8>) -> Request {
        Request {
            communications_output_id: None,
            input: None,
            output_id: Some("headphones".into()),
            volume_percent: volume,
            mute: None,
        }
    }

    #[test]
    fn changes_all_three_default_roles_but_not_volume_when_omitted() {
        let mut b = Fake::new();
        let r = apply(&mut b, &request(None), FailurePolicy::Revert, |_| {});
        assert!(r.succeeded());
        assert_eq!(b.defaults, ["headphones", "headphones", "headphones"]);
        assert_eq!(b.volume, 0.73);
        assert_eq!(b.writes.len(), 3);
    }
    #[test]
    fn restores_distinct_original_defaults_after_partial_role_failure() {
        let mut b = Fake::new();
        b.fail_role_once = true;
        let r = apply(&mut b, &request(Some(40)), FailurePolicy::Revert, |_| {});
        assert_eq!(r.outcome, Outcome::Reverted);
        assert_eq!(b.defaults, ["speakers", "music", "calls"]);
        assert_eq!(b.volume, 0.73);
        assert!(!b.writes.iter().any(|w| w.starts_with("volume")));
    }
    #[test]
    fn restores_target_volume_and_roles_after_partial_volume_failure() {
        let mut b = Fake::new();
        b.fail_volume_once = true;
        let r = apply(&mut b, &request(Some(40)), FailurePolicy::Revert, |_| {});
        assert_eq!(r.outcome, Outcome::Reverted);
        assert_eq!(b.defaults, ["speakers", "music", "calls"]);
        assert_eq!(b.volume, 0.73);
    }
    #[test]
    fn recovery_failure_does_not_prevent_other_roles_being_restored() {
        let mut b = Fake::new();
        b.fail_role_once = true;
        b.fail_restore = true;
        let r = apply(&mut b, &request(None), FailurePolicy::Revert, |_| {});
        assert_eq!(r.outcome, Outcome::RecoveryIncomplete);
        assert_eq!(b.defaults[0], "speakers");
    }
    #[test]
    fn keep_changes_policy_does_not_rollback() {
        let mut b = Fake::new();
        b.fail_role_once = true;
        let r = apply(&mut b, &request(None), FailurePolicy::KeepChanges, |_| {});
        assert_eq!(r.outcome, Outcome::ChangesKept);
        assert_eq!(b.defaults, ["headphones", "headphones", "calls"]);
        assert_eq!(b.writes.len(), 2);
    }
    #[test]
    fn unavailable_target_never_changes_defaults_or_volume() {
        let mut b = Fake::new();
        b.available = false;
        let r = apply(&mut b, &request(Some(100)), FailurePolicy::Revert, |_| {});
        assert_eq!(r.outcome, Outcome::PreflightFailed);
        assert!(b.writes.is_empty());
    }
    #[test]
    fn invalid_volume_never_changes_anything() {
        let mut b = Fake::new();
        assert_eq!(
            apply(&mut b, &request(Some(101)), FailurePolicy::Revert, |_| {}).outcome,
            Outcome::PreflightFailed
        );
        assert!(b.writes.is_empty());
    }
    #[test]
    fn already_matching_settings_do_not_issue_writes() {
        let mut b = Fake::new();
        b.defaults = [
            "headphones".into(),
            "headphones".into(),
            "headphones".into(),
        ];
        b.volume = 0.4;
        assert_eq!(
            apply(&mut b, &request(Some(40)), FailurePolicy::Revert, |_| {}).outcome,
            Outcome::NoChanges
        );
        assert!(b.writes.is_empty());
    }
}

#[cfg(test)]
pub(crate) mod optional_routing_tests {
    use super::*;
    /// Two outputs (speakers, headset) and one mic, with levels and mute; shared with
    /// audio_control's tests.
    pub(crate) struct Backend {
        pub out: [String; 3],
        pub input: [String; 3],
        pub levels: std::collections::BTreeMap<String, f32>,
        pub mutes: std::collections::BTreeMap<String, bool>,
        pub writes: Vec<String>,
        pub mic_available: bool,
        pub input_reads: usize,
        pub fail_mic_volume: bool,
        pub fail_mute: bool,
    }
    impl Backend {
        pub fn new() -> Self {
            Self {
                out: ["old-system".into(), "old-media".into(), "old-calls".into()],
                input: [
                    "old-input-system".into(),
                    "old-input-media".into(),
                    "old-input-calls".into(),
                ],
                levels: std::collections::BTreeMap::from([
                    ("speakers".into(), 0.7),
                    ("mic".into(), 0.8),
                ]),
                mutes: std::collections::BTreeMap::from([
                    ("speakers".into(), false),
                    ("headset".into(), false),
                    ("mic".into(), false),
                ]),
                writes: vec![],
                mic_available: true,
                input_reads: 0,
                fail_mic_volume: false,
                fail_mute: false,
            }
        }
    }
    pub fn device(id: &str, active: bool) -> Output {
        Output {
            id: id.into(),
            name: id.into(),
            active,
            state: "test".into(),
            console: false,
            multimedia: false,
            communications: false,
            volume_percent: None,
            muted: None,
        }
    }
    impl AudioBackend for Backend {
        fn outputs(&mut self) -> Result<Vec<Output>> {
            Ok(vec![device("speakers", true), device("headset", true)])
        }
        fn inputs(&mut self) -> Result<Vec<Output>> {
            self.input_reads += 1;
            Ok(vec![device("mic", self.mic_available)])
        }
        fn default_output(&mut self, r: Role) -> Result<String> {
            Ok(self.out[r as usize].clone())
        }
        fn default_input(&mut self, r: Role) -> Result<String> {
            Ok(self.input[r as usize].clone())
        }
        fn set_default(&mut self, r: Role, id: &str) -> Result<()> {
            self.writes.push(format!("output:{r:?}"));
            self.out[r as usize] = id.into();
            Ok(())
        }
        fn set_default_input(&mut self, r: Role, id: &str) -> Result<()> {
            self.writes.push(format!("input:{r:?}"));
            self.input[r as usize] = id.into();
            Ok(())
        }
        fn volume(&mut self, id: &str) -> Result<f32> {
            self.levels.get(id).copied().ok_or("Missing level".into())
        }
        fn set_volume(&mut self, id: &str, v: f32) -> Result<()> {
            self.writes.push(format!("volume:{id}"));
            self.levels.insert(id.into(), v);
            if id == "mic" && self.fail_mic_volume {
                self.fail_mic_volume = false;
                return Err("Microphone volume changed before failure".into());
            }
            Ok(())
        }
        fn mute(&mut self, id: &str) -> Result<bool> {
            self.mutes.get(id).copied().ok_or("Missing mute".into())
        }
        fn set_mute(&mut self, id: &str, muted: bool) -> Result<()> {
            self.writes.push(format!("mute:{id}"));
            self.mutes.insert(id.into(), muted);
            if self.fail_mute {
                self.fail_mute = false;
                return Err("Mute changed before failure".into());
            }
            Ok(())
        }
    }
    fn request() -> Request {
        Request {
            output_id: Some("speakers".into()),
            volume_percent: Some(40),
            mute: None,
            communications_output_id: Some("headset".into()),
            input: None,
        }
    }
    #[test]
    fn mute_applies_to_the_current_output_when_the_output_is_left_as_is() {
        let mut b = Backend::new();
        b.out = ["speakers".into(), "speakers".into(), "speakers".into()];
        let r = Request {
            output_id: None,
            volume_percent: Some(20),
            mute: Some(true),
            communications_output_id: None,
            input: None,
        };
        let report = apply(&mut b, &r, FailurePolicy::Revert, |_| {});
        assert_eq!(report.outcome, Outcome::Applied, "{:?}", report.details);
        assert_eq!(b.writes, ["volume:speakers", "mute:speakers"]);
        assert!(b.mutes["speakers"]);
        assert!(apply(&mut b, &r, FailurePolicy::Revert, |_| {}).outcome == Outcome::NoChanges);
        let empty = Request {
            output_id: None,
            volume_percent: None,
            mute: None,
            communications_output_id: None,
            input: None,
        };
        assert_eq!(
            apply(&mut b, &empty, FailurePolicy::Revert, |_| {}).outcome,
            Outcome::PreflightFailed
        );
    }
    #[test]
    fn failed_mute_restores_mute_volume_and_routes() {
        let mut b = Backend::new();
        b.fail_mute = true;
        let mut r = request();
        r.mute = Some(true);
        let report = apply(&mut b, &r, FailurePolicy::Revert, |_| {});
        assert_eq!(report.outcome, Outcome::Reverted, "{:?}", report.details);
        assert!(!b.mutes["speakers"]);
        assert_eq!(b.levels["speakers"], 0.7);
        assert_eq!(b.out, ["old-system", "old-media", "old-calls"]);
    }
    #[test]
    fn separate_calls_output_leaves_microphones_untouched() {
        let mut b = Backend::new();
        let old = b.input.clone();
        assert!(apply(&mut b, &request(), FailurePolicy::Revert, |_| {}).succeeded());
        assert_eq!(b.out, ["speakers", "speakers", "headset"]);
        assert_eq!(b.input, old);
        assert_eq!(b.input_reads, 0);
        assert_eq!(b.levels["mic"], 0.8);
    }
    #[test]
    fn unavailable_optional_devices_block_all_changes() {
        let mut b = Backend::new();
        b.mic_available = false;
        let mut r = request();
        r.input = Some(InputRequest {
            device_id: "mic".into(),
            volume_percent: Some(25),
            mute: None,
        });
        assert_eq!(
            apply(&mut b, &r, FailurePolicy::Revert, |_| {}).outcome,
            Outcome::PreflightFailed
        );
        assert!(b.writes.is_empty());
        r.input = None;
        r.communications_output_id = Some("missing-headset".into());
        assert_eq!(
            apply(&mut b, &r, FailurePolicy::Revert, |_| {}).outcome,
            Outcome::PreflightFailed
        );
        assert!(b.writes.is_empty());
    }
    #[test]
    fn partial_microphone_volume_failure_restores_all_six_roles_and_both_levels() {
        let mut b = Backend::new();
        let out = b.out.clone();
        let inputs = b.input.clone();
        let levels = b.levels.clone();
        let mut r = request();
        r.input = Some(InputRequest {
            device_id: "mic".into(),
            volume_percent: Some(25),
            mute: None,
        });
        b.fail_mic_volume = true;
        assert_eq!(
            apply(&mut b, &r, FailurePolicy::Revert, |_| {}).outcome,
            Outcome::Reverted
        );
        assert_eq!(b.out, out);
        assert_eq!(b.input, inputs);
        assert_eq!(b.levels, levels);
    }
    #[test]
    fn input_switch_sets_all_input_roles_only_when_present() {
        let mut b = Backend::new();
        let mut r = request();
        r.input = Some(InputRequest {
            device_id: "mic".into(),
            volume_percent: None,
            mute: None,
        });
        assert!(apply(&mut b, &r, FailurePolicy::Revert, |_| {}).succeeded());
        assert_eq!(b.input, ["mic", "mic", "mic"]);
        assert_eq!(b.levels["mic"], 0.8);
    }
}
