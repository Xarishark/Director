//! `director audio …`: live control of Windows audio devices. Saved sound belongs to scenes
//! (their Sound step); these commands change one thing right away.
use crate::{
    audio::{AudioBackend, Output, Role},
    audio_control::{self, Level, MuteChange},
    windows_audio::{OperationLock, WindowsAudio},
};

const USAGE: &str = "Use: director audio list | outputs | inputs | status | check
  director audio use [--mic] <device> [--role console|multimedia|communications]
  director audio volume [--mic] [<device>] <0..100 | +N | -N>
  director audio mute | unmute | mute toggle [--mic] [<device>]";

/// A parsed audio command.
#[derive(Debug, PartialEq)]
enum Command {
    Status {
        json: bool,
    },
    Check,
    Use {
        input: bool,
        device: String,
        roles: Vec<Role>,
    },
    Volume {
        input: bool,
        device: Option<String>,
        level: Level,
    },
    Mute {
        input: bool,
        device: Option<String>,
        change: MuteChange,
    },
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut input = false;
    let mut json = false;
    let mut roles = Vec::new();
    let mut words = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mic" if !input => input = true,
            "--json" if !json => json = true,
            "--role" => {
                i += 1;
                roles.push(match args.get(i).map(String::as_str) {
                    Some("console") => Role::Console,
                    Some("multimedia") => Role::Multimedia,
                    Some("communications") => Role::Communications,
                    _ => {
                        return Err("--role takes console, multimedia or communications.".into());
                    }
                });
            }
            flag if flag.starts_with("--") => {
                return Err(format!("Unknown or repeated option: {flag}"));
            }
            word => words.push(word),
        }
        i += 1;
    }
    let usage = || USAGE.to_string();
    let has_roles = !roles.is_empty();
    let command = match words.as_slice() {
        ["status"] => Command::Status { json },
        ["check"] => Command::Check,
        ["use", device] => Command::Use {
            input,
            device: (*device).into(),
            roles,
        },
        ["volume", level] => Command::Volume {
            input,
            device: None,
            level: Level::parse(level)?,
        },
        ["volume", device, level] => Command::Volume {
            input,
            device: Some((*device).into()),
            level: Level::parse(level)?,
        },
        ["mute", "toggle", rest @ ..] | ["toggle-mute", rest @ ..] if rest.len() <= 1 => {
            Command::Mute {
                input,
                device: rest.first().map(|d| (*d).into()),
                change: MuteChange::Toggle,
            }
        }
        [verb @ ("mute" | "unmute"), rest @ ..] if rest.len() <= 1 => Command::Mute {
            input,
            device: rest.first().map(|d| (*d).into()),
            change: if *verb == "mute" {
                MuteChange::On
            } else {
                MuteChange::Off
            },
        },
        _ => return Err(usage()),
    };
    if has_roles && (input || !matches!(command, Command::Use { .. })) {
        return Err("--role applies to audio use for an output.".into());
    }
    if json && !matches!(command, Command::Status { .. }) {
        return Err("--json applies to audio status and the audio lists.".into());
    }
    Ok(command)
}

fn describe(device: &Output) -> String {
    format!(
        "{} ({}) · {}{}",
        device.name,
        crate::cli_name::cli_name(&device.name),
        device
            .volume_percent
            .map_or("volume unknown".into(), |v| format!("{v}%")),
        if device.muted == Some(true) {
            " · muted"
        } else {
            ""
        }
    )
}

pub fn run(args: &[String]) -> Result<i32, String> {
    let command = parse(args)?;
    let mut audio = WindowsAudio::new()?;
    match command {
        Command::Check => {
            audio.policy_available()?;
            let devices = audio.outputs()?;
            println!(
                "Windows audio access and default-output policy interface available.\n{} audio outputs; {} available.\nRead-only check: no settings changed.",
                devices.len(),
                devices.iter().filter(|d| d.active).count()
            );
        }
        Command::Status { json } => {
            let status = audio_control::status(&mut audio)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&status).map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "Output: {}",
                    status.output.as_ref().map_or("none".into(), describe)
                );
                println!(
                    "Microphone: {}",
                    status.input.as_ref().map_or("none".into(), describe)
                );
            }
        }
        Command::Use {
            input,
            device,
            roles,
        } => {
            let _lock = OperationLock::acquire()?;
            println!(
                "{}",
                audio_control::use_device(&mut audio, &device, input, &roles)?
            );
        }
        Command::Volume {
            input,
            device,
            level,
        } => {
            let _lock = OperationLock::acquire()?;
            println!(
                "{}",
                audio_control::set_volume(&mut audio, device.as_deref(), input, level)?
            );
        }
        Command::Mute {
            input,
            device,
            change,
        } => {
            let _lock = OperationLock::acquire()?;
            println!(
                "{}",
                audio_control::set_mute(&mut audio, device.as_deref(), input, change)?
            );
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }
    #[test]
    fn commands_parse_with_optional_devices_and_mic() {
        assert_eq!(
            parse(&args("volume +5")),
            Ok(Command::Volume {
                input: false,
                device: None,
                level: Level::Relative(5)
            })
        );
        assert_eq!(
            parse(&args("volume --mic astro 80")),
            Ok(Command::Volume {
                input: true,
                device: Some("astro".into()),
                level: Level::Absolute(80)
            })
        );
        assert_eq!(
            parse(&args("mute toggle")),
            Ok(Command::Mute {
                input: false,
                device: None,
                change: MuteChange::Toggle
            })
        );
        assert_eq!(
            parse(&args("unmute --mic")),
            Ok(Command::Mute {
                input: true,
                device: None,
                change: MuteChange::Off
            })
        );
        assert_eq!(
            parse(&args("use soundbar --role communications")),
            Ok(Command::Use {
                input: false,
                device: "soundbar".into(),
                roles: vec![Role::Communications]
            })
        );
        assert_eq!(
            parse(&args("status --json")),
            Ok(Command::Status { json: true })
        );
    }
    #[test]
    fn bad_commands_explain_themselves() {
        assert!(
            parse(&args("apply desk"))
                .unwrap_err()
                .contains("audio use")
        );
        assert!(parse(&args("volume loud")).is_err());
        assert!(parse(&args("use --mic mic --role console")).is_err());
        assert!(parse(&args("volume 5 --json")).is_err());
        assert!(parse(&args("mute a b")).is_err());
    }
}
