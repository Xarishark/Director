//! Ephemeral GUI power transitions, independent of reported device state.
use director::tv_control::{Effect, PowerAction, PowerState};
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Progress {
    #[default]
    Idle,
    Waking(Instant),
    TurningOff,
    AssumedOff,
    WakeExpired,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Next {
    Done,
    Retry(Duration),
    Expired,
}
impl Progress {
    pub fn pending(self) -> bool {
        matches!(self, Self::Waking(_) | Self::TurningOff)
    }
    pub fn begin(&mut self, action: Option<PowerAction>, now: Instant) {
        *self = match action {
            Some(PowerAction::Wake) => Self::Waking(now + Duration::from_secs(30)),
            Some(PowerAction::Off) => Self::TurningOff,
            None => Self::Idle,
        };
    }
    pub fn accepted(&mut self, effect: &Effect, now: Instant) {
        *self = match effect {
            Effect::WakeSent => Self::Waking(now + Duration::from_secs(30)),
            Effect::OffAccepted | Effect::OffSent => Self::AssumedOff,
            _ => Self::Idle,
        };
    }
    pub fn observe(&mut self, power: PowerState, now: Instant) -> Next {
        match *self {
            Self::Waking(deadline) => {
                if power.awake() {
                    *self = Self::Idle;
                    Next::Done
                } else if now >= deadline {
                    *self = Self::WakeExpired;
                    Next::Expired
                } else {
                    Next::Retry(Duration::from_secs(2).min(deadline - now))
                }
            }
            Self::WakeExpired if power.awake() => {
                *self = Self::Idle;
                Next::Done
            }
            Self::AssumedOff if power != PowerState::Unknown => {
                *self = Self::Idle;
                Next::Done
            }
            _ => Next::Done,
        }
    }
    pub fn label(self, reported: PowerState, checking: bool) -> &'static str {
        match self {
            Self::Waking(_) => "Waking…",
            Self::TurningOff => "Turning off…",
            Self::AssumedOff => "Off · assumed",
            Self::WakeExpired if reported == PowerState::Unknown => "Not responding yet",
            Self::WakeExpired => "Not awake yet",
            Self::Idle if checking => "Checking…",
            Self::Idle => reported.label(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wake_waits_for_reported_awake_and_caps_retries() {
        let now = Instant::now();
        let mut state = Progress::Idle;
        state.accepted(&Effect::WakeSent, now);
        assert_eq!(
            state.observe(PowerState::Standby, now),
            Next::Retry(Duration::from_secs(2))
        );
        assert_eq!(state.label(PowerState::Unknown, true), "Waking…");
        assert_eq!(
            state.observe(PowerState::Unknown, now + Duration::from_secs(29)),
            Next::Retry(Duration::from_secs(1))
        );
        assert_eq!(
            state.observe(PowerState::Unknown, now + Duration::from_secs(30)),
            Next::Expired
        );
        assert_eq!(
            state.label(PowerState::Unknown, false),
            "Not responding yet"
        );
        assert_eq!(state.observe(PowerState::On, now), Next::Done);
        assert_eq!(state, Progress::Idle);
        state.accepted(&Effect::WakeSent, now);
        assert_eq!(state.observe(PowerState::On, now), Next::Done);
        assert_eq!(state, Progress::Idle);
    }
    #[test]
    fn only_an_accepted_off_request_is_assumed_off() {
        let now = Instant::now();
        let mut state = Progress::Idle;
        state.begin(Some(PowerAction::Off), now);
        assert_eq!(state.label(PowerState::On, false), "Turning off…");
        state.accepted(&Effect::OffAccepted, now);
        assert_eq!(state.observe(PowerState::Unknown, now), Next::Done);
        assert_eq!(state, Progress::AssumedOff);
        assert_eq!(state.observe(PowerState::On, now), Next::Done);
        assert_eq!(state, Progress::Idle);
        state.accepted(&Effect::AlreadyOff(PowerState::Standby), now);
        assert_eq!(state, Progress::Idle);
    }
}

/// One initial wake plus at most four retransmissions in the existing wake deadline.
#[derive(Default)]
pub(crate) struct WakeRetries {
    attempts: u8,
    next: Option<Instant>,
}
impl WakeRetries {
    pub fn started(&mut self, now: Instant) {
        self.attempts = 1;
        self.next = Some(now + Duration::from_secs(2));
    }
    pub fn take(&mut self, now: Instant) -> bool {
        if self.attempts >= 5 || self.next.is_none_or(|next| now < next) {
            return false;
        }
        self.attempts += 1;
        self.next = Some(now + Duration::from_secs(2));
        true
    }
}

#[test]
fn wake_retransmissions_are_spaced_capped_and_reset_for_a_new_request() {
    let now = Instant::now();
    let mut retry = WakeRetries::default();
    assert!(!retry.take(now));
    retry.started(now);
    assert!(!retry.take(now + Duration::from_millis(1999)));
    for step in 1..=4 {
        assert!(retry.take(now + Duration::from_secs(step * 2)));
    }
    assert!(!retry.take(now + Duration::from_secs(30)));
    retry.started(now + Duration::from_secs(40));
    assert!(!retry.take(now + Duration::from_secs(41)));
    assert!(retry.take(now + Duration::from_secs(42)));
}
