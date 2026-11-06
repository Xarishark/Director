//! Shared headless commands; does not initialize a GUI framework.
use crate::{
    audio::{self, AudioBackend, FailurePolicy, Request},
    presets,
    windows_audio::{OperationLock, WindowsAudio},
};
const HELP: &str = "Director — scenes, display and audio presets, TVs and FPS limits

Commands take a CLI name: the item's name in lowercase with dashes, so \"Desk View\" is
desk-view and needs no quotes. The ID from --json works too. director list shows both.

List
  director list [--json] [--all]      Everything below, in sections
  director scene list [--json]
  director display list [--json]      Display presets
  director display screens [--json]   The screens connected now
  director audio list [--json]        Audio presets
  director audio outputs [--json]     Audio output devices (--all includes ones not present)
  director audio inputs [--json]      Audio input devices
  director tv list [--json]

Scenes
  director scene check <scene>
  director scene apply <scene> [--yes | --confirm]
  director scene end

Display presets
  director display capture <name>
  director display check <preset>
  director display apply <preset> [--yes | --confirm]

Audio
  director audio check
  director audio volume <output> <0..100>
  director audio apply <preset-or-endpoint-id> [--volume 0..100] [--keep-on-failure]

TVs
  director tv pair | check | learn | wake | status | off | report <tv>
  director tv input <tv> <input-id>

FPS
  director fps status
  director fps set <nvidia|rtss> <FPS|vrr|off>

  director --version

Without arguments, opens the window. Display capture only saves your layout; it does not
change your screens. Display Apply follows the preset's confirmation setting; --yes skips the
prompt and --confirm requires it. Audio Apply changes the Console, Multimedia and
Communications outputs unless the preset picks a separate communications output; microphones
change only when the preset includes one. Applying a scene first runs the previous scene's
When leaving steps; scene end runs them without applying another scene.";

/// `director list`, or `<area> list` and the other read-only lists, with an optional
/// `--json`. `Some(None)` is everything; older spellings (`audio presets`, `display presets`,
/// `display current`) still work.
fn list_command(args: &[String]) -> Option<(Option<crate::cli_list::Category>, bool, bool)> {
    use crate::cli_list::Category;
    let mut words: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut flag = |name: &str| {
        let before = words.len();
        words.retain(|w| *w != name);
        match before - words.len() {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    };
    let json = flag("--json")?;
    let all = flag("--all")?;
    let category = match words.as_slice() {
        ["list"] => None,
        ["scene", "list"] => Some(Category::Scenes),
        ["display", "list" | "presets"] => Some(Category::DisplayPresets),
        ["display", "screens" | "current"] => Some(Category::Screens),
        ["audio", "list" | "presets"] => Some(Category::AudioPresets),
        ["audio", "outputs"] => Some(Category::AudioOutputs),
        ["audio", "inputs"] => Some(Category::AudioInputs),
        ["tv", "list"] => Some(Category::Tvs),
        _ => return None,
    };
    Some((category, json, all))
}
pub fn run(args: &[String]) -> i32 {
    let _operation = crate::diagnostics::Operation::start("app.cli", None);
    let result: Result<i32, String> = (|| {
        if args == ["--version"] || args == ["-V"] {
            println!("Director {}", env!("CARGO_PKG_VERSION"));
            return Ok(0);
        }
        if args == ["--help"] || args == ["help"] {
            println!("{HELP}");
            return Ok(0);
        }
        if let Some((category, json, all)) = list_command(args) {
            return match category {
                Some(category) => crate::cli_list::run(category, json, all),
                None => crate::cli_list::everything(json, all),
            };
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
                let all = presets::load(&presets::path()?)?;
                let saved = if target.starts_with('{') && !all.iter().any(|p| p.id == *target) {
                    None
                } else {
                    Some(
                        crate::cli_name::find(
                            &all,
                            target,
                            |p| (&p.id, &p.name),
                            "audio preset",
                            "audio",
                        )?
                        .clone(),
                    )
                };
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
                        mute: None,
                    })
                });
                let (id, volume, keep) = if let Some(p) = saved {
                    (
                        p.output_id,
                        volume_override.or(p.volume_percent),
                        keep_override || crate::preferences::current().keep_completed_changes,
                    )
                } else {
                    (
                        target.clone(),
                        volume_override,
                        keep_override || crate::preferences::current().keep_completed_changes,
                    )
                };
                let _lock = OperationLock::acquire()?;
                let report = audio::apply(
                    &mut backend,
                    &Request {
                        communications_output_id,
                        input,
                        output_id: Some(id),
                        volume_percent: volume,
                        mute: None,
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

#[cfg(test)]
mod tests {
    use super::list_command;
    use crate::cli_list::Category;
    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }
    #[test]
    fn list_commands_and_their_old_spellings() {
        assert!(matches!(
            list_command(&args("list")),
            Some((None, false, false))
        ));
        assert!(matches!(
            list_command(&args("list --json")),
            Some((None, true, false))
        ));
        assert!(matches!(
            list_command(&args("audio outputs --all --json")),
            Some((Some(Category::AudioOutputs), true, true))
        ));
        assert!(list_command(&args("list --json --json")).is_none());
        for (command, wanted) in [
            ("scene list", Category::Scenes),
            ("display list", Category::DisplayPresets),
            ("display presets", Category::DisplayPresets),
            ("display screens", Category::Screens),
            ("display current", Category::Screens),
            ("audio list", Category::AudioPresets),
            ("audio presets", Category::AudioPresets),
            ("audio outputs", Category::AudioOutputs),
            ("audio inputs --json", Category::AudioInputs),
            ("tv list", Category::Tvs),
        ] {
            assert!(
                matches!(list_command(&args(command)), Some((Some(c), _, _)) if c == wanted),
                "{command}"
            );
        }
        assert!(list_command(&args("list scenes")).is_none());
        assert!(list_command(&args("audio list extra")).is_none());
        assert!(list_command(&args("scene apply desk")).is_none());
    }
}
