//! Worker-owned native scene execution. Caller holds OperationLock for the entire run.
use crate::{
    audio::{self, AudioBackend, Role},
    display_apply::{self, Backend as DisplayBackend},
    scenes::{self, Action, Plan},
    windows_audio::WindowsAudio,
    windows_display_apply::{Snapshot as DisplaySnapshot, WindowsDisplays},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub type Catalog = (
    Vec<scenes::Scene>,
    Vec<crate::displays::Preset>,
    Vec<crate::tvs::Tv>,
);
pub fn catalog() -> Result<Catalog, String> {
    Ok((
        scenes::load(&scenes::path()?)?,
        crate::displays::load(&crate::displays::path()?)?,
        crate::tvs::load(&crate::tvs::path()?)?,
    ))
}
pub fn load_plan(id: &str) -> Result<Plan, String> {
    let (s, d, t) = catalog()?;
    let scene = s
        .iter()
        .find(|s| s.id == id)
        .ok_or("The scene was removed. Reopen Scenes.")?;
    let mut plan = scenes::resolve(scene, &d, &t)?;
    plan.policy = crate::preferences::current().scene_policy();
    for action in scene.before.iter().chain(&scene.actions) {
        if let Action::App {
            path,
            working_directory,
            ..
        } = action
        {
            if !std::path::Path::new(path).is_file() {
                return Err(format!("Application is unavailable: {path}"));
            }
            if working_directory
                .as_ref()
                .is_some_and(|d| !std::path::Path::new(d).is_dir())
            {
                return Err("An application’s working folder is unavailable.".into());
            }
        }
    }
    Ok(plan)
}
pub fn load_run_plan(id: &str, name: &str, notify: bool) -> Result<Plan, String> {
    load_plan(id).inspect_err(|error| {
        let report = scenes::Report {
            success: false,
            message: "Scene could not start.".into(),
            details: vec![error.clone()],
        };
        save_result(name, "Checking scene", &report, notify);
    })
}

fn save_result(name: &str, stage: &str, report: &scenes::Report, notify: bool) {
    let run = crate::diagnostics::LastRun::new(name, stage, report);
    crate::diagnostics::result("scene.last-run", &run.save());
    if notify
        && !report.success
        && let Err(error) = crate::notifications::report(&run)
    {
        crate::diagnostics::event("scene.notification", "error", &error);
    }
}

pub fn run(
    backend: &mut impl scenes::Backend,
    plan: &Plan,
    cancel: &AtomicBool,
    notify: bool,
    mut progress: impl FnMut(&str),
) -> scenes::Report {
    // Leave the previous scene first. Its leave actions can't block this scene.
    let leaving = crate::scene_session::active()
        .filter(|id| *id != plan.scene.id)
        .and_then(|id| {
            scenes::load(&scenes::path().ok()?)
                .ok()?
                .into_iter()
                .find(|s| s.id == id)
        });
    let tvs = crate::tvs::path()
        .and_then(|p| crate::tvs::load(&p))
        .unwrap_or_default();
    let warnings = leaving.map_or_else(Vec::new, |previous| {
        scenes::leave(backend, &previous, &tvs, plan.policy, &mut progress)
    });
    let mut stage = String::from("Checking current settings");
    let mut report = scenes::apply(backend, plan, cancel, |step| {
        if step.starts_with("Preparing TV")
            || step.starts_with("Applying displays")
            || step.starts_with("Waiting for sound devices")
            || step.starts_with("Applying FPS limit")
            || plan
                .scene
                .before
                .iter()
                .chain(&plan.scene.actions)
                .any(|action| action.label() == step)
        {
            stage = step.to_owned();
        }
        progress(step);
    });
    report.details.splice(0..0, warnings);
    let active = report.success.then_some(plan.scene.id.as_str());
    if let Err(e) = crate::scene_session::set(active) {
        report.details.push(e);
    }
    save_result(&plan.scene.name, &stage, &report, notify);
    report
}

/// End the active scene: run its When leaving actions and forget it. Settings it changed
/// stay as they are.
pub fn end(backend: &mut impl scenes::Backend, mut progress: impl FnMut(&str)) -> scenes::Report {
    let Some(id) = crate::scene_session::active() else {
        return scenes::Report {
            success: true,
            message: "No scene is active.".into(),
            details: vec![],
        };
    };
    let scene = scenes::load(&scenes::path().unwrap_or_default())
        .ok()
        .and_then(|all| all.into_iter().find(|s| s.id == id));
    let policy = crate::preferences::current().scene_policy();
    let (name, mut details) = match &scene {
        Some(scene) => (
            scene.name.clone(),
            scenes::leave(
                backend,
                scene,
                &crate::tvs::path()
                    .and_then(|p| crate::tvs::load(&p))
                    .unwrap_or_default(),
                policy,
                &mut progress,
            ),
        ),
        None => ("The scene".into(), vec![]),
    };
    let success = details.is_empty();
    if let Err(e) = crate::scene_session::set(None) {
        details.push(e);
    }
    scenes::Report {
        success,
        message: if success {
            format!("“{name}” ended.")
        } else {
            format!("“{name}” ended, but some leave actions failed.")
        },
        details,
    }
}

pub struct Snapshot {
    display: Option<(crate::displays::Layout, DisplaySnapshot)>,
    routes: Vec<(bool, Role, String)>,
    fps: Option<crate::fps::Snapshot>,
}
pub struct WindowsScene<F: FnMut(&crate::displays::Preset) -> bool> {
    displays: WindowsDisplays,
    audio: Option<WindowsAudio>,
    levels: Vec<(String, f32)>,
    mutes: Vec<(String, bool)>,
    display_attempted: bool,
    tv_attempted: bool,
    audio_attempted: bool,
    fps: Option<crate::windows_fps::WindowsFps>,
    fps_attempted: bool,
    confirm: F,
}
impl<F: FnMut(&crate::displays::Preset) -> bool> WindowsScene<F> {
    pub fn new(confirm: F) -> Self {
        Self {
            displays: WindowsDisplays::default(),
            audio: None,
            levels: vec![],
            mutes: vec![],
            display_attempted: false,
            tv_attempted: false,
            audio_attempted: false,
            fps: None,
            fps_attempted: false,
            confirm,
        }
    }
}
/// The devices whose volume or mute a Sound step changes, as (input, resolved id, volume,
/// mute). No output means the current default output.
fn level_targets(
    audio: &mut WindowsAudio,
    request: &audio::Request,
) -> Result<Vec<(bool, String, bool, bool)>, String> {
    let mut targets = vec![];
    if request.volume_percent.is_some() || request.mute.is_some() {
        audio.outputs()?;
        let id = match &request.output_id {
            Some(id) => audio.resolve_id(id, false),
            None => audio.default_output(Role::Console)?,
        };
        targets.push((
            false,
            id,
            request.volume_percent.is_some(),
            request.mute.is_some(),
        ));
    }
    if let Some(input) = &request.input
        && (input.volume_percent.is_some() || input.mute.is_some())
    {
        audio.inputs()?;
        targets.push((
            true,
            audio.resolve_id(&input.device_id, true),
            input.volume_percent.is_some(),
            input.mute.is_some(),
        ));
    }
    Ok(targets)
}
impl<F: FnMut(&crate::displays::Preset) -> bool> WindowsScene<F> {
    /// Remember a device's volume and mute once, before the scene changes them. A device
    /// that isn't connected yet is skipped; it is remembered when it appears.
    fn remember(
        &mut self,
        audio: &mut WindowsAudio,
        _input: bool,
        id: &str,
        volume: bool,
        mute: bool,
    ) -> Result<(), String> {
        if id.is_empty() {
            return Ok(());
        }
        if volume
            && !self.levels.iter().any(|(old, _)| old == id)
            && let Ok(v) = audio.volume(id)
        {
            if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                return Err("Could not snapshot an audio volume.".into());
            }
            self.levels.push((id.to_owned(), v));
        }
        if mute
            && !self.mutes.iter().any(|(old, _)| old == id)
            && let Ok(m) = audio.mute(id)
        {
            self.mutes.push((id.to_owned(), m));
        }
        Ok(())
    }
}
fn paused(cancel: &AtomicBool, deadline: Instant) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("Scene canceled.".into())
    } else if Instant::now() >= deadline {
        Err("The selected device did not become ready before the wait expired.".into())
    } else {
        std::thread::sleep(Duration::from_millis(200));
        Ok(())
    }
}
impl<F: FnMut(&crate::displays::Preset) -> bool> scenes::Backend for WindowsScene<F> {
    type Snapshot = Snapshot;
    fn snapshot(&mut self, plan: &Plan) -> Result<Snapshot, String> {
        let fps = if let Some(request) = &plan.scene.fps {
            request.validate()?;
            let mut provider = crate::windows_fps::WindowsFps::open(request.provider)?;
            let snapshot = crate::fps::Backend::snapshot(&mut provider)?;
            self.fps = Some(provider);
            Some(snapshot)
        } else {
            None
        };
        let display = if plan.display.is_some() || plan.tv.is_some() {
            Some(self.displays.snapshot()?)
        } else {
            None
        };
        let mut routes = vec![];
        if plan.sound.is_some() || plan.display.is_some() || plan.tv.is_some() {
            let mut audio = WindowsAudio::new()?;
            for role in audio::DEFAULT_ROLES {
                routes.push((false, role, audio.default_output(role)?));
                if plan.sound.as_ref().is_some_and(|p| p.input.is_some()) {
                    routes.push((true, role, audio.default_input(role)?));
                }
            }
            if let Some(request) = &plan.sound {
                // Devices that are connected now; one that appears later is snapshotted
                // just before the sound stage.
                for (input, id, volume, mute) in level_targets(&mut audio, request)? {
                    self.remember(&mut audio, input, &id, volume, mute)?;
                }
            }
            self.audio = Some(audio);
        }
        Ok(Snapshot {
            display,
            routes,
            fps,
        })
    }
    fn tv(
        &mut self,
        plan: &Plan,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<(), String> {
        use crate::tv_control::Backend;
        self.tv_attempted = true;
        let tv = plan.tv.as_ref().ok_or("Missing TV.")?;
        let prep = plan.scene.tv.as_ref().ok_or("Missing TV preparation.")?;
        let deadline = Instant::now() + Duration::from_secs(u64::from(plan.policy.wait_seconds));
        let mut next_wake = Instant::now();
        let mut wakes = 0;
        loop {
            if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                return Err("TV readiness timed out or was canceled.".into());
            }
            let mut controls = crate::windows_tv::WindowsTvControls::new(tv);
            if controls.power().is_ok_and(|p| p.awake()) {
                if let Some(id) = &prep.input {
                    progress("Selecting the TV input…");
                    crate::tv_control::switch_input(&mut controls, id)?;
                    if !controls.input_readback() {
                        return Ok(());
                    }
                    // Do not continue until the TV reports the requested input. A TV box
                    // may take the input while the TV wakes: ask again, leaving each
                    // request time to land.
                    let mut sent = Instant::now();
                    loop {
                        if cancel.load(Ordering::Acquire) {
                            return Err("Scene canceled.".into());
                        }
                        if controls.current_input()?.as_deref() == Some(id) {
                            break;
                        }
                        if sent.elapsed() >= Duration::from_millis(1500) {
                            controls.switch_input(id)?;
                            sent = Instant::now();
                        }
                        paused(cancel, deadline)?;
                    }
                    // Then keep it there in the background while the scene carries on.
                    let (tv, id) = (tv.clone(), id.clone());
                    crate::tv_input_guard::start(
                        plan.policy.input_guard_seconds,
                        move |stop, until| {
                            crate::tv_input_guard::hold(
                                || crate::windows_tv::WindowsTvControls::new(&tv),
                                &id,
                                || stop.is_set(),
                                || crate::tv_input_guard::sleep_until(stop, until),
                            );
                        },
                    );
                }
                return Ok(());
            }
            if prep.wake && wakes < 5 && Instant::now() >= next_wake {
                progress("Waking TV and waiting for a response…");
                crate::tv_control::Backend::wake(&mut crate::windows_tv::WindowsTvControls::new(
                    tv,
                ))?;
                wakes += 1;
                next_wake = Instant::now() + Duration::from_secs(5);
            }
            progress("Waiting for TV readiness…");
            paused(cancel, deadline)?;
        }
    }
    fn display(&mut self, plan: &Plan) -> Result<(), String> {
        let p = plan.display.as_ref().ok_or("Missing display preset.")?;
        self.display_attempted = true;
        let report = display_apply::apply(&mut self.displays, &p.layout, || {
            !p.confirm_changes || (self.confirm)(p)
        });
        if report.succeeded() {
            Ok(())
        } else {
            Err(format!("{} {}", report.message, report.details.join("; ")))
        }
    }
    fn audio(
        &mut self,
        plan: &Plan,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<(), String> {
        let p = plan.sound.as_ref().ok_or("Missing Sound step.")?;
        let mut audio = self.audio.take().ok_or("Audio snapshot unavailable.")?;
        let result = (|| {
            let deadline =
                Instant::now() + Duration::from_secs(u64::from(plan.policy.wait_seconds));
            loop {
                if cancel.load(Ordering::Acquire) {
                    return Err("Scene canceled.".into());
                }
                let outputs = audio.outputs()?;
                let inputs = if p.input.is_some() {
                    audio.inputs()?
                } else {
                    vec![]
                };
                let available = |id: &str, input, devices: &[audio::Output]| {
                    let resolved = audio.resolve_id(id, input);
                    devices.iter().any(|d| d.id == resolved && d.active)
                };
                if p.output_id
                    .as_deref()
                    .is_none_or(|id| available(id, false, &outputs))
                    && p.communications_output_id
                        .as_deref()
                        .is_none_or(|id| available(id, false, &outputs))
                    && p.input
                        .as_ref()
                        .is_none_or(|d| available(&d.device_id, true, &inputs))
                {
                    break;
                }
                progress("Waiting for the selected sound devices to appear…");
                paused(cancel, deadline)?;
            }
            for (input, id, volume, mute) in level_targets(&mut audio, p)? {
                self.remember(&mut audio, input, &id, volume, mute)?;
            }
            self.audio_attempted = true;
            let report = audio::apply(
                &mut audio,
                p,
                if plan.policy.keep_completed {
                    audio::FailurePolicy::KeepChanges
                } else {
                    audio::FailurePolicy::Revert
                },
                progress,
            );
            if report.succeeded() {
                Ok(())
            } else {
                Err(format!("{} {}", report.message, report.details.join("; ")))
            }
        })();
        self.audio = Some(audio);
        result
    }
    fn fps(&mut self, plan: &Plan) -> Result<(), String> {
        use crate::fps::Backend;
        let wanted = plan.scene.fps.as_ref().ok_or("Missing FPS request.")?;
        // Displays run first, so this reads the refresh rate the scene switched to.
        let refresh = if wanted.vrr_safe {
            crate::windows_displays::capture()?.primary_refresh_hz()
        } else {
            None
        };
        let request = wanted.resolved(refresh)?;
        let provider = self.fps.as_mut().ok_or("Missing FPS provider.")?;
        self.fps_attempted = true;
        provider.set(&request)?;
        crate::fps::verify(&request, &provider.snapshot()?)
    }
    fn tv_off(&mut self, tv: &crate::tvs::Tv) -> Result<(), String> {
        crate::tv_control::power(
            &mut crate::windows_tv::WindowsTvControls::new(tv),
            crate::tv_control::PowerAction::Off,
        )
        .map(|_| ())
    }
    fn launch(&mut self, action: &Action, wait_seconds: u16) -> Result<(), String> {
        match action {
            Action::Close { target, force } => crate::windows_close::close(target, *force),
            Action::App {
                path,
                args,
                working_directory,
                wait,
            } => {
                let mut command = std::process::Command::new(path);
                command
                    .args(args)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                if let Some(dir) = working_directory {
                    command.current_dir(dir);
                }
                let mut child = command
                    .spawn()
                    .map_err(|e| format!("Could not launch application: {e}"))?;
                if !*wait {
                    return Ok(());
                }
                let name = action.label();
                let deadline = Instant::now() + Duration::from_secs(u64::from(wait_seconds));
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) if status.success() => return Ok(()),
                        Ok(Some(status)) => {
                            return Err(format!(
                                "{name} failed (exit code {}).",
                                status.code().map_or("unknown".into(), |c| c.to_string())
                            ));
                        }
                        Ok(None) if Instant::now() >= deadline => {
                            return Err(format!(
                                "{name} didn’t finish within {wait_seconds} seconds. It is still running."
                            ));
                        }
                        Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                        Err(e) => return Err(format!("Could not wait for {name}: {e}")),
                    }
                }
            }
            Action::Uri { target } => {
                use windows::{
                    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
                    core::PCWSTR,
                };
                let target = target.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                let result = unsafe {
                    ShellExecuteW(
                        None,
                        windows::core::w!("open"),
                        PCWSTR(target.as_ptr()),
                        None,
                        None,
                        SW_SHOWNORMAL,
                    )
                };
                if result.0 as isize <= 32 {
                    Err(format!(
                        "Windows could not open the URI ({}).",
                        result.0 as isize
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }
    fn restore(&mut self, snapshot: &Snapshot) -> Result<(), String> {
        // The scene stopped: stop holding the TV on its input too.
        crate::tv_input_guard::stop();
        let mut errors = vec![];
        if self.fps_attempted
            && let (Some(provider), Some(saved)) = (&mut self.fps, &snapshot.fps)
            && let Err(e) = crate::fps::restore_verified(provider, saved)
        {
            errors.push(e);
        }
        // Restore device levels while a newly enabled HDMI endpoint is still available.
        if self.audio_attempted
            && let Some(audio) = &mut self.audio
        {
            for (id, muted) in &self.mutes {
                if audio.set_mute(id, *muted).is_err() || audio.mute(id).ok() != Some(*muted) {
                    errors.push("A previous mute state could not be restored.".into());
                }
            }
            for (id, volume) in &self.levels {
                if let Err(e) = audio.set_volume(id, *volume) {
                    errors.push(e)
                }
                match audio.volume(id) {
                    Ok(actual) if actual.is_finite() && (actual - *volume).abs() <= 0.01 => {}
                    _ => errors.push("A previous audio volume could not be verified.".into()),
                }
            }
        }
        if (self.display_attempted || self.tv_attempted)
            && let Some((before, saved)) = &snapshot.display
        {
            if self
                .displays
                .current()
                .and_then(|l| display_apply::verify(before, &l))
                .is_err()
                && let Err(e) = self.displays.restore(saved)
            {
                errors.push(e)
            }
            if let Err(e) = self
                .displays
                .current()
                .and_then(|l| display_apply::verify(before, &l))
            {
                errors.push(e)
            }
        }
        // Restoring a display can also move Windows audio defaults, even before our audio stage.
        if (self.display_attempted || self.audio_attempted || self.tv_attempted)
            && let Some(audio) = &mut self.audio
        {
            for (input, role, id) in &snapshot.routes {
                let result = if *input {
                    audio.set_default_input(*role, id)
                } else {
                    audio.set_default(*role, id)
                };
                if let Err(e) = result {
                    errors.push(e)
                }
            }
            for (input, role, id) in &snapshot.routes {
                let result = if *input {
                    audio.default_input(*role)
                } else {
                    audio.default_output(*role)
                };
                match result {
                    Ok(actual) if actual == *id => {}
                    _ => errors.push("A previous audio default could not be verified.".into()),
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}
pub fn cli(args: &[String]) -> Result<i32, String> {
    match args {
        [command] if command == "end" => {
            let _lock = crate::windows_audio::OperationLock::acquire()?;
            let mut backend = WindowsScene::new(|_: &crate::displays::Preset| false);
            let report = end(&mut backend, |s| eprintln!("{s}"));
            println!("{}", report.message);
            for detail in report.details {
                println!("{detail}")
            }
            Ok(if report.success { 0 } else { 1 })
        }
        [command, name, rest @ ..]
            if (command == "apply" || command == "check")
                && (rest.is_empty() || rest == ["--yes"] || rest == ["--confirm"]) =>
        {
            let _lock = crate::windows_audio::OperationLock::acquire()?;
            let all = scenes::load(&scenes::path()?)?;
            let scene = crate::cli_name::find(&all, name, |s| (&s.id, &s.name), "scene", "scene")?;
            let mut plan = if command == "check" {
                load_plan(&scene.id)?
            } else {
                load_run_plan(&scene.id, &scene.name, true)?
            };
            if let Some(p) = &mut plan.display {
                if rest == ["--yes"] {
                    p.confirm_changes = false;
                } else if rest == ["--confirm"] {
                    p.confirm_changes = true;
                }
            }
            if command == "check" {
                println!(
                    "Scene references and launch paths are valid. No devices changed; live TV/display/audio readiness is checked during Apply."
                );
                return Ok(0);
            }
            let mut backend = WindowsScene::new(|_: &crate::displays::Preset| {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let mut line = String::new();
                    let keep = std::io::stdin().read_line(&mut line).is_ok()
                        && matches!(line.trim().to_lowercase().as_str(), "yes" | "y");
                    let _ = tx.send(if keep {
                        display_apply::Decision::Keep
                    } else {
                        display_apply::Decision::Revert
                    });
                });
                eprintln!("Keep display changes? Type yes within 15 seconds.");
                display_apply::wait_for_confirmation(&rx, Duration::from_secs(15), |_| true)
            });
            let report = run(&mut backend, &plan, &AtomicBool::new(false), true, |s| {
                eprintln!("{s}")
            });
            println!("{}", report.message);
            for detail in report.details {
                println!("{detail}")
            }
            // The guard runs on its own thread; exiting would end it early.
            if crate::tv_input_guard::active() {
                eprintln!(
                    "Keeping the TV on the scene’s input for a few seconds (TV input protection)…"
                );
            }
            crate::tv_input_guard::wait();
            Ok(if report.success { 0 } else { 1 })
        }
        _ => Err(
            "Use director scene list | end | check <scene> | apply <scene> [--yes | --confirm]."
                .into(),
        ),
    }
}
