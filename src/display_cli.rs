//! Headless display capture, validation, and confirmed application.
use crate::{
    displays::{self, Preset},
    windows_audio::OperationLock,
    windows_displays,
};
pub fn run(args: &[String]) -> Result<i32, String> {
    if let [command, name, rest @ ..] = args
        && (command == "apply" || command == "check")
    {
        use crate::{
            display_apply::{self, Backend, Decision},
            windows_display_apply::WindowsDisplays,
        };
        if (command == "check" && !rest.is_empty())
            || (command == "apply"
                && !rest.is_empty()
                && rest != ["--yes"]
                && rest != ["--confirm"])
        {
            return Err(
                "Use display check <preset> or display apply <preset> [--yes | --confirm].".into(),
            );
        }
        let presets = displays::load(&displays::path()?)?;
        let preset = crate::cli_name::find(
            &presets,
            name,
            |p| (&p.id, &p.name),
            "display preset",
            "display",
        )?;
        let _lock = OperationLock::acquire()?;
        let mut backend = WindowsDisplays::default();
        if command == "check" {
            let (before, snapshot) = backend.snapshot()?;
            let mut plan = backend.prepare(&preset.layout, &before, &snapshot)?;
            backend.validate(&mut plan)?;
            println!(
                "Windows validated “{}”. No display settings changed.",
                preset.name
            );
            return Ok(0);
        }
        let report = display_apply::apply(&mut backend, &preset.layout, || {
            if !asks_for_confirmation(preset.confirm_changes, rest) {
                return true;
            }
            eprintln!(
                "Keep these display settings? Type yes within 15 seconds; otherwise the previous layout is restored."
            );
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut line = String::new();
                let keep = std::io::stdin().read_line(&mut line).is_ok()
                    && matches!(line.trim().to_lowercase().as_str(), "y" | "yes");
                let _ = tx.send(if keep {
                    Decision::Keep
                } else {
                    Decision::Revert
                });
            });
            display_apply::wait_for_confirmation(
                &rx,
                std::time::Duration::from_secs(15),
                |seconds| {
                    eprintln!("Reverting in {seconds} seconds unless confirmed.");
                    true
                },
            )
        });
        println!("{}", report.message);
        for detail in &report.details {
            eprintln!("{detail}");
        }
        return Ok(if report.succeeded() { 0 } else { 1 });
    }
    match args {
        [command, name] if command == "capture" => {
            let name = name.trim();
            if name.is_empty() || name.len() > 160 {
                return Err("Display presets need a name of up to 160 bytes.".into());
            }
            let _lock = OperationLock::acquire()?;
            let path = displays::path()?;
            let old = displays::load(&path)?;
            let layout = windows_displays::capture()?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?;
            let preset = Preset {
                id: format!("{}-{}", std::process::id(), now.as_nanos()),
                name: name.into(),
                captured_at_unix_seconds: now.as_secs(),
                confirm_changes: false,
                layout,
            };
            let mut next = old.clone();
            next.push(preset);
            next.sort_by_key(|p| p.name.to_lowercase());
            displays::save(&path, &old, &next)?;
            println!(
                "Captured display preset “{name}” ({}). Your screens are unchanged.",
                crate::cli_name::cli_name(name)
            );
        }
        _ => return Err("Unknown display command. Use director --help.".into()),
    }
    Ok(0)
}

fn asks_for_confirmation(saved: bool, flags: &[String]) -> bool {
    match flags {
        [flag] if flag == "--yes" => false,
        [flag] if flag == "--confirm" => true,
        _ => saved,
    }
}
#[cfg(test)]
mod tests {
    use super::asks_for_confirmation;
    #[test]
    fn confirmation_policy_honors_preset_and_explicit_overrides() {
        for saved in [true, false] {
            assert_eq!(asks_for_confirmation(saved, &[]), saved);
            assert!(!asks_for_confirmation(saved, &["--yes".into()]));
            assert!(asks_for_confirmation(saved, &["--confirm".into()]));
        }
    }
}
