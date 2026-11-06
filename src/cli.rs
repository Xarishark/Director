//! Shared headless commands; does not initialize a GUI framework.
use crate::{
    audio::{self, AudioBackend, FailurePolicy, Request},
    presets,
    windows_audio::{OperationLock, WindowsAudio},
};
pub fn run(args: &[String]) -> i32 {
    let _operation = crate::diagnostics::Operation::start("app.cli", None);
    let result: Result<i32, String> = (|| {
        if args == ["--version"] || args == ["-V"] {
            println!("Director {}", env!("CARGO_PKG_VERSION"));
            return Ok(0);
        }
        if args == ["--help"] || args == ["help"] {
            println!(
                "Director — setup presets preview\n\n  director --version\n  director fps status\n  director fps set <nvidia|rtss> <FPS|vrr|off>\n  director tv list\n  director tv pair <name>\n  director tv check <name>\n  director tv learn <name>\n  director tv wake <name>\n  director tv status <name>\n  director tv off <name>\n  director tv input <name> <input-id>\n  director tv report <name>\n  director audio presets\n  director display current [--json]\n  director display presets [--json]\n  director display capture <name>\n  director display check <name-or-id>\n  director display apply <name-or-id> [--yes | --confirm]\n  director audio list [--json]\n  director audio inputs [--json]\n  director audio check\n  director audio volume <output-name-or-endpoint-id> <0..100>\n  director audio apply <preset-name-id-or-endpoint-id> [--volume 0..100] [--keep-on-failure]\n\nWithout arguments, opens the window. Output Apply changes Console, Multimedia and Communications, unless the saved preset selects a separate communications output. Microphones change only when enabled in the preset.\nCreate named audio presets in the window. Display capture only saves your layout; it does not change your screens. Display Apply follows the saved confirmation preference; --yes skips the prompt and --confirm requires it. Verification and recovery always run. Scenes combine independent presets and actions. Use director scene list | end | check <name-or-id> | apply <name-or-id> [--yes | --confirm]. Applying a scene first runs the previous scene's When leaving actions; end runs them without applying another scene."
            );
            return Ok(0);
        }
        if args == ["audio", "presets"] {
            for p in presets::load(&presets::path()?)? {
                println!(
                    "{} — {} · {}",
                    p.name,
                    p.output_name,
                    p.volume_percent
                        .map_or("keep volume".into(), |v| format!("{v}%"))
                );
            }
            return Ok(0);
        }
        if args.first().is_some_and(|area| area == "display") {
            return crate::display_cli::run(&args[1..]);
        }
        if args.first().is_some_and(|area| area == "tv") {
            return crate::tv_cli::run(&args[1..]);
        }
        if args.first().is_some_and(|area| area == "scene") {
            return crate::windows_scenes::cli(&args[1..]);
        }
        if args.first().is_some_and(|area| area == "fps") {
            return crate::windows_fps::cli(&args[1..]);
        }
        let mut backend = WindowsAudio::new()?;
        match args {
            [area, command] if area == "audio" && command == "check" => {
                backend.policy_available()?;
                let devices = backend.outputs()?;
                println!(
                    "Windows audio access and default-output policy interface available.\n{} audio outputs; {} available.\nRead-only check: no settings changed.",
                    devices.len(),
                    devices.iter().filter(|d| d.active).count()
                );
                Ok(0)
            }
            [area, command, rest @ ..]
                if area == "audio"
                    && (command == "list" || command == "inputs")
                    && (rest.is_empty() || rest == ["--json"]) =>
            {
                let outputs = if command == "inputs" {
                    backend.inputs()?
                } else {
                    backend.outputs()?
                };
                if rest == ["--json"] {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&outputs).map_err(|e| e.to_string())?
                    );
                } else {
                    for d in outputs {
                        println!(
                            "{} — {}{}\n  {}",
                            d.name,
                            d.state,
                            if d.console && d.multimedia {
                                " (Default audio)"
                            } else if d.console {
                                " (Console default)"
                            } else if d.multimedia {
                                " (Multimedia default)"
                            } else {
                                ""
                            },
                            d.id
                        );
                    }
                }
                Ok(0)
            }
            [area, command, target, percent] if area == "audio" && command == "volume" => {
                let percent = percent
                    .parse::<u8>()
                    .map_err(|_| "Enter a whole-number percentage from 0 to 100.")?;
                let _lock = OperationLock::acquire()?;
                println!(
                    "{}",
                    audio::set_output_volume(&mut backend, target, percent)?
                );
                Ok(0)
            }
            [area, command, target, rest @ ..] if area == "audio" && command == "apply" => {
                let saved = presets::load(&presets::path()?)?
                    .into_iter()
                    .find(|p| p.id == *target || p.name.to_lowercase() == target.to_lowercase());
                let mut volume_override = None;
                let mut keep_override = false;
                let mut i = 0;
                while i < rest.len() {
                    match rest[i].as_str() {
                        "--volume" if volume_override.is_none() => {
                            i += 1;
                            let value = rest
                                .get(i)
                                .ok_or("--volume needs a percentage.")?
                                .parse::<u8>()
                                .map_err(|_| "Volume must be an integer between 0 and 100.")?;
                            if value > 100 {
                                return Err("Volume must be between 0 and 100.".into());
                            }
                            volume_override = Some(value);
                        }
                        "--keep-on-failure" if !keep_override => keep_override = true,
                        value => return Err(format!("Unknown or repeated option: {value}")),
                    }
                    i += 1;
                }
                let communications_output_id = saved
                    .as_ref()
                    .and_then(|p| p.communications_output.as_ref().map(|d| d.id.clone()));
                let input = saved.as_ref().and_then(|p| {
                    p.input.as_ref().map(|d| audio::InputRequest {
                        device_id: d.id.clone(),
                        volume_percent: d.volume_percent,
                    })
                });
                let (id, volume, keep) = if let Some(p) = saved {
                    (
                        p.output_id,
                        volume_override.or(p.volume_percent),
                        keep_override || crate::preferences::current().keep_completed_changes,
                    )
                } else if target.starts_with('{') {
                    (
                        target.clone(),
                        volume_override,
                        keep_override || crate::preferences::current().keep_completed_changes,
                    )
                } else {
                    return Err(format!(
                        "No audio preset named “{target}”. Create it in the window first."
                    ));
                };
                let _lock = OperationLock::acquire()?;
                let report = audio::apply(
                    &mut backend,
                    &Request {
                        communications_output_id,
                        input,
                        output_id: id,
                        volume_percent: volume,
                    },
                    if keep {
                        FailurePolicy::KeepChanges
                    } else {
                        FailurePolicy::Revert
                    },
                    |message| eprintln!("{message}"),
                );
                println!("{}", report.message);
                for detail in &report.details {
                    println!("{detail}");
                }
                Ok(if report.succeeded() { 0 } else { 1 })
            }
            _ => Err("Unknown command. Use director --help.".into()),
        }
    })();
    match result {
        Ok(code) => code,
        Err(error) => {
            crate::diagnostics::event("app.cli.result", "error", &error);
            eprintln!("{error}");
            2
        }
    }
}
