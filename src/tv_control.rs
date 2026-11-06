//! Explicit TV operations shared by the window and CLI.
use crate::{lg_protocol::Input, tv_network::WakeReport};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PowerState {
    On,
    Standby,
    ScreenOff,
    ScreenSaver,
    Off,
    #[default]
    Unknown,
}
impl PowerState {
    pub fn label(self) -> &'static str {
        match self {
            Self::On => "On",
            Self::Standby => "Standby",
            Self::ScreenOff => "Screen off",
            Self::ScreenSaver => "Screen saver",
            Self::Off => "Off",
            Self::Unknown => "Unknown",
        }
    }
    pub fn awake(self) -> bool {
        matches!(self, Self::On | Self::ScreenOff | Self::ScreenSaver)
    }
    pub fn button_action(self) -> PowerAction {
        if self.awake() {
            PowerAction::Off
        } else {
            PowerAction::Wake
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    Wake,
    Off,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub power: PowerState,
    pub inputs: Vec<Input>,
    pub current_input: Option<String>,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    WakeSent,
    OffAccepted,
    OffSent,
    InputSent(String),
    AlreadyOff(PowerState),
    InputConfirmed(String),
    InputAccepted(String),
}
#[derive(Clone, Debug)]
pub struct Report {
    pub effect: Effect,
    pub message: String,
    pub details: String,
}
pub trait Backend {
    fn input_readback(&self) -> bool {
        true
    }
    fn command_acknowledged(&self) -> bool {
        true
    }
    fn power(&mut self) -> Result<PowerState, String>;
    fn inputs(&mut self) -> Result<Vec<Input>, String>;
    fn current_input(&mut self) -> Result<Option<String>, String>;
    fn wake(&mut self) -> Result<WakeReport, String>;
    fn off(&mut self) -> Result<(), String>;
    fn switch_input(&mut self, id: &str) -> Result<(), String>;
}
pub fn snapshot(backend: &mut impl Backend) -> Snapshot {
    let mut result = Snapshot::default();
    match backend.power() {
        Ok(power) => {
            result.power = power;
            if power == PowerState::Unknown {
                result
                    .notes
                    .push("The TV did not report a recognized power state.".into());
            }
        }
        Err(e) => result.notes.push(format!("Power status unavailable: {e}")),
    }
    complete_snapshot(backend, result)
}
/// During wake, do not query inputs until the TV actually reports awake.
pub fn wake_snapshot(backend: &mut impl Backend) -> Result<Snapshot, String> {
    let power = backend.power()?;
    let result = Snapshot {
        power,
        ..Default::default()
    };
    Ok(if power.awake() {
        complete_snapshot(backend, result)
    } else {
        result
    })
}
fn complete_snapshot(backend: &mut impl Backend, mut result: Snapshot) -> Snapshot {
    match backend.inputs() {
        Ok(inputs) => result.inputs = inputs,
        Err(e) => result.notes.push(format!("Inputs unavailable: {e}")),
    }
    if !result.inputs.is_empty() {
        match backend.current_input() {
            Ok(id) => result.current_input = id,
            Err(e) => result.notes.push(format!("Current input unavailable: {e}")),
        }
    }
    result
}
/// The displayed action is an explicit intent. Fresh state may make it a no-op;
/// never reverse it if the TV changed state after the button was displayed.
pub fn power(backend: &mut impl Backend, action: PowerAction) -> Result<Report, String> {
    // Not in unit tests: they run in parallel with the guard's own tests.
    #[cfg(not(test))]
    crate::tv_input_guard::stop();
    match action {
        PowerAction::Wake => {
            // Providers use an explicit wake intent: direct wake or bounded Wake-on-LAN.
            let sent = crate::diagnostics::measure("tv.wake.dispatch", "", || backend.wake())?;
            Ok(Report {
                effect: Effect::WakeSent,
                message: "Wake request sent.".into(),
                details: format!(
                    "{} wake request(s) sent for {} target(s). Physical wake has not been verified.",
                    sent.packets, sent.targets
                ),
            })
        }
        PowerAction::Off => {
            let state =
                crate::diagnostics::measure("tv.off.preflight", "fresh power state", || {
                    backend.power()
                })
                .map_err(|e| {
                    format!("Could not read fresh power status. No turn-off request was sent. {e}")
                })?;
            crate::diagnostics::event("tv.off.preflight.state", "reported", state.label());
            if state == PowerState::Unknown {
                return Err(
                    "The TV did not report a recognized power state. No turn-off request was sent."
                        .into(),
                );
            }
            if !state.awake() {
                return Ok(Report {
                    effect: Effect::AlreadyOff(state),
                    message: "The TV already reports off or standby.".into(),
                    details: String::new(),
                });
            }
            crate::diagnostics::measure("tv.off.command", "", || backend.off()).map_err(|e|format!("Could not confirm the turn-off request. The TV may already have turned off. {e}"))?;
            Ok(Report {
                effect: if backend.command_acknowledged() {
                    Effect::OffAccepted
                } else {
                    Effect::OffSent
                },
                message: if backend.command_acknowledged() {
                    "Turn-off request accepted by the TV."
                } else {
                    "Turn-off request sent; shutdown is not confirmed."
                }
                .into(),
                details: "Physical power-off has not been independently verified.".into(),
            })
        }
    }
}
/// Select an input. Stops a running input guard first, so it can't switch back.
pub fn switch_input(backend: &mut impl Backend, id: &str) -> Result<Report, String> {
    // Not in unit tests: they run in parallel with the guard's own tests.
    #[cfg(not(test))]
    crate::tv_input_guard::stop();
    let inputs = crate::diagnostics::measure("tv.input.preflight", id, || backend.inputs())?;
    let target = inputs
        .iter()
        .find(|i| i.id == id)
        .ok_or("The TV no longer reports that input. Refresh its controls and choose again.")?;
    let label = target.label.clone();
    if backend.current_input().ok().flatten().as_deref() == Some(id) {
        return Ok(Report {
            effect: Effect::InputConfirmed(id.into()),
            message: format!("“{label}” is already the active input."),
            details: String::new(),
        });
    }
    crate::diagnostics::measure("tv.input.command", id, || backend.switch_input(id)).map_err(
        |e| {
            format!(
                "Could not confirm input selection. The TV may already have changed inputs. {e}"
            )
        },
    )?;
    if !backend.input_readback() {
        return Ok(Report{effect:Effect::InputSent(id.into()),message:format!("Direct input command sent for “{label}”."),details:"Samsung does not acknowledge remote-key commands or report its selected input through this provider. Windows verification is separate.".into()});
    }
    for attempt in 0..4 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        match backend.current_input(){
   Ok(Some(current)) if current==id=>return Ok(Report{effect:Effect::InputConfirmed(id.into()),message:format!("TV reports “{label}” as the active input."),details:"This confirms the TV’s selected app/input, not HDMI signal or Windows display readiness.".into()}),
   Err(_)=>break,
   _=>{},
  }
    }
    Ok(Report {
        effect: Effect::InputAccepted(id.into()),
        message: format!("Input change accepted for “{label}”."),
        details:
            "The TV has not yet confirmed the selected input. Refresh its status after it settles."
                .into(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        state: Result<PowerState, String>,
        inputs: Vec<Input>,
        current: Option<String>,
        events: Vec<String>,
        confirm: bool,
        input_reads: usize,
        acknowledged: bool,
    }
    impl Fake {
        fn new(state: PowerState) -> Self {
            Self {
                state: Ok(state),
                inputs: vec![
                    Input {
                        id: "HDMI_1".into(),
                        label: "PC".into(),
                    },
                    Input {
                        id: "HDMI_2".into(),
                        label: "Console".into(),
                    },
                ],
                current: Some("HDMI_1".into()),
                events: vec![],
                confirm: true,
                input_reads: 0,
                acknowledged: true,
            }
        }
    }
    impl Backend for Fake {
        fn command_acknowledged(&self) -> bool {
            self.acknowledged
        }
        fn input_readback(&self) -> bool {
            self.acknowledged
        }
        fn power(&mut self) -> Result<PowerState, String> {
            self.events.push("read-power".into());
            self.state.clone()
        }
        fn inputs(&mut self) -> Result<Vec<Input>, String> {
            self.input_reads += 1;
            Ok(self.inputs.clone())
        }
        fn current_input(&mut self) -> Result<Option<String>, String> {
            Ok(self.current.clone())
        }
        fn wake(&mut self) -> Result<WakeReport, String> {
            self.events.push("wake".into());
            Ok(WakeReport {
                targets: 1,
                packets: 3,
            })
        }
        fn off(&mut self) -> Result<(), String> {
            self.events.push("off".into());
            Ok(())
        }
        fn switch_input(&mut self, id: &str) -> Result<(), String> {
            self.events.push(format!("input:{id}"));
            if self.confirm {
                self.current = Some(id.into());
            }
            Ok(())
        }
    }

    #[test]
    fn wake_read_avoids_inputs_until_awake() {
        let mut backend = Fake::new(PowerState::Standby);
        assert_eq!(
            wake_snapshot(&mut backend).unwrap().power,
            PowerState::Standby
        );
        assert_eq!(backend.input_reads, 0);
        backend.state = Err("offline".into());
        assert!(wake_snapshot(&mut backend).is_err());
        assert_eq!(backend.input_reads, 0);
        backend.state = Ok(PowerState::On);
        assert_eq!(wake_snapshot(&mut backend).unwrap().inputs.len(), 2);
        assert_eq!(backend.input_reads, 1);
    }
    #[test]
    fn power_honors_displayed_intent_and_rechecks_fresh_state() {
        let mut b = Fake::new(PowerState::On);
        power(&mut b, PowerAction::Wake).unwrap();
        assert!(!b.events.contains(&"off".into()));
        assert_eq!(b.events, ["wake"]);
        b.events.clear();
        b.state = Ok(PowerState::Off);
        power(&mut b, PowerAction::Off).unwrap();
        assert!(!b.events.contains(&"wake".into()));
        assert!(!b.events.contains(&"off".into()));
        b.state = Ok(PowerState::On);
        power(&mut b, PowerAction::Off).unwrap();
        assert_eq!(b.events.last().unwrap(), "off");
    }
    #[test]
    fn unknown_is_never_off_and_wake_does_not_require_connectivity() {
        let mut b = Fake::new(PowerState::Unknown);
        assert!(power(&mut b, PowerAction::Off).is_err());
        assert_eq!(snapshot(&mut b).power, PowerState::Unknown);
        b.state = Err("offline".into());
        assert!(power(&mut b, PowerAction::Off).is_err());
        power(&mut b, PowerAction::Wake).unwrap();
        assert_eq!(b.events.last().unwrap(), "wake");
        assert_eq!(PowerState::ScreenOff.button_action(), PowerAction::Off);
    }
    #[test]
    fn inputs_are_freshly_validated_and_read_back() {
        let mut b = Fake::new(PowerState::On);
        assert!(switch_input(&mut b, "HDMI_missing").is_err());
        assert!(b.events.is_empty());
        switch_input(&mut b, "HDMI_1").unwrap();
        assert!(b.events.is_empty());
        let report = switch_input(&mut b, "HDMI_2").unwrap();
        assert_eq!(b.events, ["input:HDMI_2"]);
        assert!(report.message.starts_with("TV reports"));
        b.confirm = false;
        b.current = None;
        let report = switch_input(&mut b, "HDMI_1").unwrap();
        assert!(report.message.starts_with("Input change accepted"));
        assert!(report.details.contains("not yet confirmed"));
    }
    #[test]
    fn unacknowledged_provider_reports_sent_instead_of_accepted_or_confirmed() {
        let mut b = Fake::new(PowerState::On);
        b.acknowledged = false;
        b.confirm = false;
        let report = power(&mut b, PowerAction::Off).unwrap();
        assert_eq!(report.effect, Effect::OffSent);
        assert!(report.message.contains("sent") && !report.message.contains("accepted"));
        let report = switch_input(&mut b, "HDMI_2").unwrap();
        assert_eq!(report.effect, Effect::InputSent("HDMI_2".into()));
    }
}
