//! Provider-independent global FPS configuration and verified recovery.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Nvidia,
    Rtss,
}
impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Nvidia => "NVIDIA",
            Self::Rtss => "RTSS",
        }
    }
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "nvidia" => Ok(Self::Nvidia),
            "rtss" => Ok(Self::Rtss),
            _ => Err("Choose nvidia or rtss.".into()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub provider: Provider,
    pub limit: Option<u32>,
}
impl Request {
    pub fn validate(&self) -> Result<(), String> {
        let max = match self.provider {
            Provider::Nvidia => 1023,
            Provider::Rtss => 1000,
        };
        if self.limit.is_some_and(|n| n == 0 || n > max) {
            return Err(format!(
                "{} global FPS limit must be 1–{max}, or choose Disable limit.",
                self.provider.label()
            ));
        }
        Ok(())
    }
    pub fn label(&self) -> String {
        format!(
            "{} · {}",
            self.provider.label(),
            self.limit
                .map_or("global limit disabled".into(), |v| format!(
                    "{v} FPS, global"
                ))
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub provider: Provider,
    pub value: u32,
    pub inherited: bool,
    pub limiter_disabled: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Capability {
    pub provider: Provider,
    pub available: bool,
    pub maximum: u32,
    pub current: Option<Snapshot>,
    pub detail: String,
}
pub trait Backend {
    fn snapshot(&mut self) -> Result<Snapshot, String>;
    fn set(&mut self, request: &Request) -> Result<(), String>;
    fn restore(&mut self, snapshot: &Snapshot) -> Result<(), String>;
}
pub fn verify(request: &Request, actual: &Snapshot) -> Result<(), String> {
    if actual.provider != request.provider
        || actual.value != request.limit.unwrap_or(0)
        || request.limit.is_some() && actual.limiter_disabled == Some(true)
    {
        Err("The FPS setting did not read back as requested.".into())
    } else {
        Ok(())
    }
}
pub fn apply(
    backend: &mut impl Backend,
    request: &Request,
    keep_on_failure: bool,
) -> Result<String, String> {
    request.validate()?;
    let before = backend.snapshot()?;
    if before.provider != request.provider {
        return Err("The selected FPS provider changed.".into());
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        backend.set(request)?;
        verify(request, &backend.snapshot()?)
    }))
    .unwrap_or_else(|_| Err("The FPS operation stopped unexpectedly.".into()));
    match result {
        Ok(()) => Ok(format!(
            "{} saved and read back. Check an actual game to confirm the effective cap.",
            request.label()
        )),
        Err(e) if keep_on_failure => Err(format!("{e} Completed FPS changes were kept.")),
        Err(e) => match restore_verified(backend, &before) {
            Ok(()) => Err(format!("{e} Previous FPS configuration restored.")),
            Err(recovery) => Err(format!("{e} Recovery failed: {recovery}")),
        },
    }
}
pub fn restore_verified(backend: &mut impl Backend, before: &Snapshot) -> Result<(), String> {
    if backend.snapshot().is_ok_and(|s| s == *before) {
        return Ok(());
    }
    backend.restore(before)?;
    if backend.snapshot()? != *before {
        return Err("The previous FPS configuration could not be verified.".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        value: Snapshot,
        fail: bool,
        restores: usize,
    }
    impl Backend for Fake {
        fn snapshot(&mut self) -> Result<Snapshot, String> {
            Ok(self.value.clone())
        }
        fn set(&mut self, r: &Request) -> Result<(), String> {
            self.value.value = r.limit.unwrap_or(0);
            self.value.inherited = false;
            self.value.limiter_disabled = Some(false);
            if self.fail {
                Err("save failed".into())
            } else {
                Ok(())
            }
        }
        fn restore(&mut self, s: &Snapshot) -> Result<(), String> {
            self.value = s.clone();
            self.restores += 1;
            Ok(())
        }
    }
    #[test]
    fn failed_partial_changes_restore_full_state_and_keep_policy_is_explicit() {
        let before = Snapshot {
            provider: Provider::Rtss,
            value: 75,
            inherited: false,
            limiter_disabled: Some(true),
        };
        let mut b = Fake {
            value: before.clone(),
            fail: true,
            restores: 0,
        };
        let r = Request {
            provider: Provider::Rtss,
            limit: Some(60),
        };
        assert!(apply(&mut b, &r, false).unwrap_err().contains("restored"));
        assert_eq!(b.value, before);
        assert_eq!(b.restores, 1);
        assert!(apply(&mut b, &r, true).is_err());
        assert_eq!(b.value.value, 60);
        assert_eq!(b.restores, 1);
    }
    #[test]
    fn no_limit_and_disabled_provider_flags_are_distinct() {
        let r = Request {
            provider: Provider::Rtss,
            limit: None,
        };
        let mut s = Snapshot {
            provider: Provider::Rtss,
            value: 0,
            inherited: false,
            limiter_disabled: Some(true),
        };
        assert!(verify(&r, &s).is_ok());
        s.value = 60;
        assert!(
            verify(
                &Request {
                    limit: Some(60),
                    ..r
                },
                &s
            )
            .is_err()
        );
        assert!(
            Request {
                provider: Provider::Nvidia,
                limit: Some(1024)
            }
            .validate()
            .is_err()
        );
        assert!(
            Request {
                provider: Provider::Rtss,
                limit: Some(0)
            }
            .validate()
            .is_err()
        );
    }
}
