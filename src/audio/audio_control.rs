//! Live audio controls: switch the default device, change volume or mute, one change at a
//! time. Each change is verified and the previous value is put back if Windows does not
//! confirm it. Scenes use `audio::apply`, which changes several things as one transaction.
use crate::{
    audio::{AudioBackend, DEFAULT_ROLES, Output, Result, Role},
    cli::cli_name::cli_name,
};

/// A volume change: an absolute percentage, or a step up or down from the current level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Absolute(u8),
    Relative(i16),
}
impl Level {
    /// "35", "+5" or "-10".
    pub fn parse(text: &str) -> Result<Self> {
        let invalid = || "Use a volume from 0 to 100, or a step such as +5 or -5.".to_string();
        if let Some(step) = text.strip_prefix('+') {
            step.parse::<u8>()
                .map(|s| Self::Relative(i16::from(s)))
                .map_err(|_| invalid())
        } else if let Some(step) = text.strip_prefix('-') {
            step.parse::<u8>()
                .map(|s| Self::Relative(-i16::from(s)))
                .map_err(|_| invalid())
        } else {
            match text.parse::<u8>() {
                Ok(v) if v <= 100 => Ok(Self::Absolute(v)),
                _ => Err(invalid()),
            }
        }
    }
    fn target(self, current: u8) -> u8 {
        match self {
            Self::Absolute(v) => v.min(100),
            Self::Relative(step) => (i16::from(current) + step).clamp(0, 100) as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuteChange {
    On,
    Off,
    Toggle,
}

/// The default output and microphone right now.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Status {
    pub output: Option<Output>,
    pub input: Option<Output>,
}

fn kind(input: bool) -> &'static str {
    if input { "microphone" } else { "output" }
}

fn devices(b: &mut impl AudioBackend, input: bool) -> Result<Vec<Output>> {
    if input { b.inputs() } else { b.outputs() }
}

fn default_id(b: &mut impl AudioBackend, input: bool) -> Result<String> {
    if input {
        b.default_input(Role::Console)
    } else {
        b.default_output(Role::Console)
    }
}

/// A device by endpoint ID or CLI name; no target means the current default. The device
/// must be connected.
pub fn find_device(b: &mut impl AudioBackend, target: Option<&str>, input: bool) -> Result<Output> {
    let list = devices(b, input)?;
    let area = if input { "inputs" } else { "outputs" };
    let device = match target {
        None => {
            let id = default_id(b, input)?;
            list.into_iter()
                .find(|d| d.id == id)
                .ok_or(format!("There is no default {}.", kind(input)))?
        }
        Some(target) => {
            let resolved = b.resolve_id(target, input);
            if let Some(exact) = list
                .iter()
                .find(|d| !resolved.is_empty() && d.id == resolved)
            {
                exact.clone()
            } else {
                let wanted = cli_name(target);
                let mut matches = list
                    .into_iter()
                    .filter(|d| !wanted.is_empty() && cli_name(&d.name) == wanted)
                    .filter(|d| d.active)
                    .collect::<Vec<_>>();
                match matches.len() {
                    0 => {
                        return Err(format!(
                            "No connected {} named {}. Run director audio {area}.",
                            kind(input),
                            if wanted.is_empty() { target } else { &wanted }
                        ));
                    }
                    1 => matches.remove(0),
                    _ => {
                        return Err(format!(
                            "More than one {} is named {wanted}. Run director audio {area} and use the endpoint ID.",
                            kind(input)
                        ));
                    }
                }
            }
        }
    };
    if !device.active {
        return Err(format!("{} is not connected.", device.name));
    }
    Ok(device)
}

/// Make a device the default for the given roles (all three when `roles` is empty).
/// Restores the previous defaults if Windows does not confirm the change.
pub fn use_device(
    b: &mut impl AudioBackend,
    target: &str,
    input: bool,
    roles: &[Role],
) -> Result<String> {
    let device = find_device(b, Some(target), input)?;
    let roles = if roles.is_empty() {
        &DEFAULT_ROLES[..]
    } else {
        roles
    };
    let read = |b: &mut _, role| {
        if input {
            AudioBackend::default_input(b, role)
        } else {
            AudioBackend::default_output(b, role)
        }
    };
    let write = |b: &mut _, role, id: &str| {
        if input {
            AudioBackend::set_default_input(b, role, id)
        } else {
            AudioBackend::set_default(b, role, id)
        }
    };
    let mut previous = Vec::new();
    for &role in roles {
        previous.push((role, read(b, role)?));
    }
    let mut changed = Vec::new();
    let result = (|| {
        for (role, old) in &previous {
            if *old != device.id {
                changed.push((*role, old.clone()));
                write(b, *role, &device.id)?;
            }
        }
        for &role in roles {
            if read(b, role)? != device.id {
                return Err(format!(
                    "Windows did not confirm {} as the {role:?} default.",
                    device.name
                ));
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) if changed.is_empty() => Ok(format!(
            "{} is already the default {}.",
            device.name,
            kind(input)
        )),
        Ok(()) => Ok(format!(
            "{} is now the default {}.",
            device.name,
            kind(input)
        )),
        Err(error) => {
            let mut failed = Vec::new();
            for (role, old) in changed.iter().rev() {
                if write(b, *role, old).is_err() || read(b, *role).ok().as_ref() != Some(old) {
                    failed.push(format!("{role:?}"));
                }
            }
            Err(if failed.is_empty() {
                format!("{error} Previous defaults restored.")
            } else {
                format!(
                    "{error} Could not restore the previous {} default.",
                    failed.join(", ")
                )
            })
        }
    }
}

/// Set or step a device's volume; no target means the current default.
pub fn set_volume(
    b: &mut impl AudioBackend,
    target: Option<&str>,
    input: bool,
    level: Level,
) -> Result<String> {
    let device = find_device(b, target, input)?;
    let previous = b.volume(&device.id)?;
    if !previous.is_finite() || !(0.0..=1.0).contains(&previous) {
        return Err("Windows returned an invalid device volume.".into());
    }
    let percent = level.target((previous * 100.0).round() as u8);
    let scalar = f32::from(percent) / 100.0;
    if (previous - scalar).abs() > 0.005 {
        let result = b
            .set_volume(&device.id, scalar)
            .and_then(|()| crate::audio::verify_volume(b, &device.id, scalar));
        if let Err(error) = result {
            return match b
                .set_volume(&device.id, previous)
                .and_then(|()| crate::audio::verify_volume(b, &device.id, previous))
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

/// Mute, unmute or toggle a device; no target means the current default.
pub fn set_mute(
    b: &mut impl AudioBackend,
    target: Option<&str>,
    input: bool,
    change: MuteChange,
) -> Result<String> {
    let device = find_device(b, target, input)?;
    let previous = b.mute(&device.id)?;
    let muted = match change {
        MuteChange::On => true,
        MuteChange::Off => false,
        MuteChange::Toggle => !previous,
    };
    if muted != previous {
        let result = b
            .set_mute(&device.id, muted)
            .and_then(|()| match b.mute(&device.id)? {
                v if v == muted => Ok(()),
                _ => Err("Windows did not confirm the mute change.".to_string()),
            });
        if let Err(error) = result {
            let restored = b.set_mute(&device.id, previous).is_ok()
                && b.mute(&device.id).ok() == Some(previous);
            return Err(if restored {
                format!("{error} Previous mute state restored.")
            } else {
                format!("{error} Could not restore the previous mute state.")
            });
        }
    }
    Ok(format!(
        "{} {}.",
        device.name,
        if muted { "muted" } else { "unmuted" }
    ))
}

/// The default output and microphone, with volume and mute.
pub fn status(b: &mut impl AudioBackend) -> Result<Status> {
    let output = find_device(b, None, false).ok();
    let input = find_device(b, None, true).ok();
    Ok(Status { output, input })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::optional_routing_tests::Backend;

    fn defaults_to(b: &mut Backend, id: &str) {
        b.out = [id.into(), id.into(), id.into()];
    }

    #[test]
    fn levels_parse_and_clamp() {
        assert_eq!(Level::parse("35"), Ok(Level::Absolute(35)));
        assert_eq!(Level::parse("+5"), Ok(Level::Relative(5)));
        assert_eq!(Level::parse("-10"), Ok(Level::Relative(-10)));
        assert!(Level::parse("101").is_err());
        assert!(Level::parse("loud").is_err());
        assert_eq!(Level::Relative(10).target(95), 100);
        assert_eq!(Level::Relative(-10).target(5), 0);
        assert_eq!(Level::Absolute(40).target(90), 40);
    }

    #[test]
    fn no_target_means_the_default_device() {
        let mut b = Backend::new();
        defaults_to(&mut b, "speakers");
        let message = set_volume(&mut b, None, false, Level::Relative(5)).unwrap();
        assert_eq!(message, "speakers volume: 75%.");
        assert!((b.levels["speakers"] - 0.75).abs() < 0.001);
        assert_eq!(b.writes, ["volume:speakers"]);
    }

    #[test]
    fn mute_toggles_and_already_set_writes_nothing() {
        let mut b = Backend::new();
        defaults_to(&mut b, "speakers");
        assert_eq!(
            set_mute(&mut b, Some("speakers"), false, MuteChange::Toggle).unwrap(),
            "speakers muted."
        );
        assert!(b.mutes["speakers"]);
        set_mute(&mut b, None, false, MuteChange::On).unwrap();
        assert_eq!(b.writes, ["mute:speakers"]);
        set_mute(&mut b, None, false, MuteChange::Toggle).unwrap();
        assert!(!b.mutes["speakers"]);
    }

    #[test]
    fn failed_mute_restores_the_previous_state() {
        let mut b = Backend::new();
        b.fail_mute = true;
        let error = set_mute(&mut b, Some("headset"), false, MuteChange::On).unwrap_err();
        assert!(error.contains("restored"), "{error}");
        assert!(!b.mutes["headset"]);
    }

    #[test]
    fn use_device_switches_only_the_requested_roles() {
        let mut b = Backend::new();
        use_device(&mut b, "headset", false, &[Role::Communications]).unwrap();
        assert_eq!(b.out, ["old-system", "old-media", "headset"]);
        let message = use_device(&mut b, "speakers", false, &[]).unwrap();
        assert!(message.contains("now the default"), "{message}");
        assert_eq!(b.out, ["speakers", "speakers", "speakers"]);
        assert!(
            use_device(&mut b, "speakers", false, &[])
                .unwrap()
                .contains("already")
        );
        use_device(&mut b, "mic", true, &[]).unwrap();
        assert_eq!(b.input, ["mic", "mic", "mic"]);
    }

    #[test]
    fn unknown_and_disconnected_devices_are_refused() {
        let mut b = Backend::new();
        assert!(
            find_device(&mut b, Some("tv"), false)
                .unwrap_err()
                .contains("director audio outputs")
        );
        b.mic_available = false;
        assert!(find_device(&mut b, Some("mic"), true).is_err());
        assert!(set_volume(&mut b, Some("mic"), true, Level::Absolute(10)).is_err());
        assert_eq!(b.levels["mic"], 0.8);
    }
}
