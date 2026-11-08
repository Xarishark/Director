//! Shared headless commands; does not initialize a GUI framework.
#[cfg(windows)]
pub mod cli_list;
pub mod cli_name;
const HELP: &str = "Director — scenes, display presets, TVs, sound and FPS limits

Commands take a CLI name: the item's name in lowercase with dashes, so \"Desk View\" is
desk-view and needs no quotes. The ID from --json works too. director list shows both.

List
  director list [--json] [--all]      Everything below, in sections
  director scene list [--json]
  director display list [--json]      Display presets
  director display screens [--json]   The screens connected now
  director audio list [--json]        Audio outputs and inputs (--all includes ones not present)
  director audio outputs [--json]
  director audio inputs [--json]
  director tv list [--json]

Scenes
  director scene check <scene>
  director scene apply <scene> [--yes | --confirm]
  director scene end

Display presets
  director display capture <name>
  director display check <preset>
  director display apply <preset> [--yes | --confirm]

Sound (changes apply right away; add --mic for the microphone)
  director audio status [--json]
  director audio use <device> [--role console|multimedia|communications]
  director audio volume [<device>] <0..100 | +N | -N>
  director audio mute | unmute | mute toggle [<device>]
  director audio check

TVs
  director tv pair | check | learn | wake | status | off | toggle | report <tv>
  director tv input <tv> <input-id>

FPS
  director fps status
  director fps set <nvidia|rtss> <FPS|vrr|off>

Updates
  director update                     Check GitHub for a newer version
  director update --install           Download it, verify its checksum and start setup

  director --version

Without arguments, opens the window. Leaving out the device in an audio command means the
current default. audio use makes a device the default for all three Windows roles unless
--role picks one. Display capture only saves your layout; it does not change your screens.
Display Apply follows the preset's confirmation setting; --yes skips the prompt and --confirm
requires it. Applying a scene first runs the previous scene's When leaving steps; scene end
runs them without applying another scene.";

/// `director list`, or `<area> list` and the other read-only lists, with an optional
/// `--json`.
fn list_command(
    args: &[String],
) -> Option<(&'static [crate::cli::cli_list::Category], bool, bool)> {
    use crate::cli::cli_list::Category;
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
    let categories: &'static [Category] = match words.as_slice() {
        ["list"] => &Category::ALL,
        ["scene", "list"] => &[Category::Scenes],
        ["display", "list"] => &[Category::DisplayPresets],
        ["display", "screens"] => &[Category::Screens],
        ["audio", "list"] => &[Category::AudioOutputs, Category::AudioInputs],
        ["audio", "outputs"] => &[Category::AudioOutputs],
        ["audio", "inputs"] => &[Category::AudioInputs],
        ["tv", "list"] => &[Category::Tvs],
        _ => return None,
    };
    Some((categories, json, all))
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
        if let Some((categories, json, all)) = list_command(args) {
            return crate::cli::cli_list::run(categories, json, all);
        }
        if args.first().is_some_and(|area| area == "display") {
            return crate::display::display_cli::run(&args[1..]);
        }
        if args.first().is_some_and(|area| area == "tv") {
            return crate::tv::tv_cli::run(&args[1..]);
        }
        if args.first().is_some_and(|area| area == "scene") {
            return crate::scene::windows_scenes::cli(&args[1..]);
        }
        if args.first().is_some_and(|area| area == "fps") {
            return crate::fps::windows_fps::cli(&args[1..]);
        }
        if args.first().is_some_and(|area| area == "update") {
            return crate::update::cli(&args[1..]);
        }
        if args.first().is_some_and(|area| area == "audio") {
            return crate::audio::audio_cli::run(&args[1..]);
        }
        Err("Unknown command. Use director --help.".into())
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
    use crate::cli::cli_list::Category;
    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }
    #[test]
    fn list_commands_and_their_old_spellings() {
        let list = |s: &str| list_command(&args(s));
        assert!(matches!(list("list"), Some((c, false, false)) if c == Category::ALL));
        assert!(matches!(list("list --json"), Some((_, true, false))));
        assert!(matches!(
            list("audio outputs --all --json"),
            Some(([Category::AudioOutputs], true, true))
        ));
        assert!(matches!(
            list("audio list"),
            Some((
                [Category::AudioOutputs, Category::AudioInputs],
                false,
                false
            ))
        ));
        assert!(list("list --json --json").is_none());
        for (command, wanted) in [
            ("scene list", Category::Scenes),
            ("display list", Category::DisplayPresets),
            ("display screens", Category::Screens),
            ("audio outputs", Category::AudioOutputs),
            ("audio inputs --json", Category::AudioInputs),
            ("tv list", Category::Tvs),
        ] {
            assert!(
                matches!(list(command), Some(([c], _, _)) if *c == wanted),
                "{command}"
            );
        }
        assert!(list("display presets").is_none());
        assert!(list("list scenes").is_none());
        assert!(list("audio list extra").is_none());
        assert!(list("scene apply desk").is_none());
    }
}
