//! Friend-test feedback and an allowlisted diagnostic export, never raw app logs.
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Answer {
    #[default]
    NotTested,
    Yes,
    No,
}
impl Answer {
    pub fn index(self) -> usize {
        match self {
            Self::NotTested => 0,
            Self::Yes => 1,
            Self::No => 2,
        }
    }
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Yes,
            2 => Self::No,
            _ => Self::NotTested,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Feedback {
    pub tv_model: String,
    pub tv_software: String,
    pub network: Network,
    pub power_status_correct: Answer,
    pub turn_off_worked: Answer,
    pub wake_worked: Answer,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Network {
    #[default]
    NotSpecified,
    Wifi,
    Ethernet,
}
impl Network {
    pub fn index(self) -> usize {
        match self {
            Self::NotSpecified => 0,
            Self::Wifi => 1,
            Self::Ethernet => 2,
        }
    }
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Wifi,
            2 => Self::Ethernet,
            _ => Self::NotSpecified,
        }
    }
}
impl Feedback {
    pub fn validate(&self) -> Result<(), String> {
        if self.tv_model.len() > 160 || self.tv_software.len() > 160 {
            return Err("TV model and software version must be at most 160 bytes.".into());
        }
        Ok(())
    }
    pub fn redacted(&self) -> Self {
        let clean =
            |v: &str| crate::tv::samsung::safe_text(&serde_json::json!(v)).unwrap_or_default();
        Self {
            tv_model: clean(&self.tv_model),
            tv_software: clean(&self.tv_software),
            ..self.clone()
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    PairStart,
    PairConfirm,
    Pair,
    Inspect,
    MetadataRead,
    PowerStatus,
    Wake,
    TurnOff,
    HdmiTest,
    HdmiSwitch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Failure {
    Timeout,
    IdentityChanged,
    CodeMismatch,
    AuthorizationDenied,
    PairingRequired,
    ConnectionOrProtocol,
    UnsupportedCommand,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub at_unix_ms: u64,
    pub action: Action,
    pub outcome: Outcome,
    pub elapsed_ms: u64,
    pub failure: Option<Failure>,
    pub system_error_code: Option<String>,
    pub reported_power: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalEvent {
    tv_id: String,
    event: Event,
}
fn failure(s: &str) -> Failure {
    let s = s.to_ascii_lowercase();
    if s.contains("timed out")
        || s.contains("timeout")
        || s.contains("in time")
        || s.contains("80072ee2")
    {
        Failure::Timeout
    } else if s.contains("identity") || s.contains("certificate") && s.contains("match") {
        Failure::IdentityChanged
    } else if s.contains("code does not match") || s.contains("code expired") {
        Failure::CodeMismatch
    } else if s.contains("denied") || s.contains("rejected") || s.contains("unauthorized") {
        Failure::AuthorizationDenied
    } else if s.contains("pair ") || s.contains("pairing data") {
        Failure::PairingRequired
    } else if s.contains("advertise") || s.contains("unsupported") || s.contains("not supported") {
        Failure::UnsupportedCommand
    } else {
        Failure::ConnectionOrProtocol
    }
}
fn error_code(s: &str) -> Option<String> {
    s.as_bytes()
        .windows(10)
        .find(|w| w.starts_with(b"0x") && w[2..].iter().all(u8::is_ascii_hexdigit))
        .map(|w| String::from_utf8_lossy(w).into_owned())
}
pub fn record<T>(
    id: &str,
    action: Action,
    start: Instant,
    result: &Result<T, String>,
    power: Option<crate::tv::tv_control::PowerState>,
) {
    let err = result.as_ref().err();
    let event = Event {
        at_unix_ms: now_ms(),
        action,
        outcome: if err.is_some() {
            Outcome::Error
        } else {
            Outcome::Ok
        },
        elapsed_ms: start.elapsed().as_millis().min(u64::MAX as u128) as u64,
        failure: err.map(|s| failure(s)),
        system_error_code: err.and_then(|s| error_code(s)),
        reported_power: power.map(|p| p.label().to_string()),
    };
    if let Ok(detail) = serde_json::to_string(&LocalEvent {
        tv_id: id.into(),
        event,
    }) {
        crate::diagnostics::event("tv.compatibility", "record", &detail);
    }
}
pub fn measure<T>(
    id: &str,
    action: Action,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let start = Instant::now();
    let result = f();
    record(id, action, start, &result, None);
    result
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
#[derive(Serialize)]
pub struct History {
    pub events: Vec<Event>,
    pub log_files_read: usize,
    pub log_files_unavailable: usize,
    pub limited_to_latest_128: bool,
    pub note: &'static str,
}
fn parse(data: &[u8], id: &str) -> Vec<Event> {
    data.split(|b| *b == b'\n')
        .filter_map(|line| {
            let line: serde_json::Value = serde_json::from_slice(line).ok()?;
            if line["step"] != "tv.compatibility" || line["outcome"] != "record" {
                return None;
            }
            let record: LocalEvent = serde_json::from_str(line["detail"].as_str()?).ok()?;
            if record.tv_id != id {
                return None;
            }
            let e = record.event;
            if e.system_error_code
                .as_ref()
                .is_some_and(|s| s.len() != 10 || error_code(s).as_ref() != Some(s))
                || e.reported_power.as_deref().is_some_and(|s| {
                    ![
                        "On",
                        "Standby",
                        "Off",
                        "Unknown",
                        "Screen off",
                        "Screen saver",
                    ]
                    .contains(&s)
                })
            {
                return None;
            }
            Some(e)
        })
        .collect()
}
pub fn history(id: &str) -> History {
    crate::diagnostics::flush();
    let mut h = History {
        events: vec![],
        log_files_read: 0,
        log_files_unavailable: 0,
        limited_to_latest_128: false,
        note: "Only structured actions recorded by this build for this TV are included. No raw errors, logs or device identities are copied. Empty history means no retained actions were available; repeat the failed action before exporting.",
    };
    let Ok(folder) = crate::diagnostics::folder() else {
        h.log_files_unavailable = 1;
        return h;
    };
    for name in [
        "director.4.log",
        "director.3.log",
        "director.2.log",
        "director.1.log",
        "director.log",
    ] {
        let path = folder.join(name);
        match std::fs::File::open(path) {
            Ok(file) => {
                let mut data = vec![];
                if file.take(1024 * 1024 + 4096).read_to_end(&mut data).is_ok() {
                    h.events.extend(parse(&data, id));
                    h.log_files_read += 1
                } else {
                    h.log_files_unavailable += 1
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => h.log_files_unavailable += 1,
        }
    }
    h.events.sort_by_key(|e| e.at_unix_ms);
    if h.events.len() > 128 {
        h.limited_to_latest_128 = true;
        h.events.drain(..h.events.len() - 128);
    }
    h
}
#[derive(Serialize)]
pub struct Environment {
    pub exported_at_unix_ms: u64,
    pub os: &'static str,
    pub architecture: &'static str,
    pub windows_version: Option<String>,
    pub executable_sha256: Option<String>,
}
pub fn environment() -> Environment {
    static HASH: OnceLock<Option<String>> = OnceLock::new();
    Environment {
        exported_at_unix_ms: now_ms(),
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        windows_version: windows_version(),
        executable_sha256: HASH
            .get_or_init(|| {
                use sha2::{Digest, Sha256};
                let mut file = std::fs::File::open(std::env::current_exe().ok()?).ok()?;
                let mut hash = Sha256::new();
                let mut buf = [0; 65536];
                loop {
                    let n = file.read(&mut buf).ok()?;
                    if n == 0 {
                        break;
                    }
                    hash.update(&buf[..n]);
                }
                Some(format!("{:x}", hash.finalize()))
            })
            .clone(),
    }
}
#[cfg(windows)]
fn windows_version() -> Option<String> {
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut OSVERSIONINFOW) -> i32;
    }
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    if unsafe { RtlGetVersion(&mut info) } >= 0 {
        Some(format!(
            "{}.{}.{}",
            info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber
        ))
    } else {
        None
    }
}
#[cfg(not(windows))]
fn windows_version() -> Option<String> {
    None
}
#[derive(Serialize)]
pub struct WakeConfiguration {
    pub direct_remote_supported: bool,
    pub saved_target_count: usize,
    pub wired_address_saved: bool,
    pub wifi_address_saved: bool,
    pub manual_address_saved: bool,
}
pub fn wake_configuration(tv: &crate::tv::tvs::Tv) -> WakeConfiguration {
    WakeConfiguration {
        direct_remote_supported: tv.brand == crate::tv::tvs::Brand::TclAndroid,
        saved_target_count: tv.network.targets().len(),
        wired_address_saved: tv.network.wired.is_some(),
        wifi_address_saved: tv.network.wifi.is_some(),
        manual_address_saved: tv.network.manual.is_some(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_exports_only_the_selected_tv_and_typed_fields() {
        let e = Event {
            at_unix_ms: 1,
            action: Action::Pair,
            outcome: Outcome::Error,
            elapsed_ms: 200,
            failure: Some(Failure::Timeout),
            system_error_code: Some("0x80072EE2".into()),
            reported_power: None,
        };
        let line = |id: &str| {
            serde_json::json!({"step":"tv.compatibility","outcome":"record","detail":serde_json::to_string(&LocalEvent{tv_id:id.into(),event:e.clone()}).unwrap()}).to_string()
        };
        let input = format!(
            "{}\n{}\n{}\n",
            line("one"),
            line("other"),
            serde_json::json!({"step":"tv.compatibility","outcome":"record","detail":"192.0.2.50 token=secret"})
        );
        let out = parse(input.as_bytes(), "one");
        assert_eq!(out.len(), 1);
        let data = serde_json::to_string(&out).unwrap();
        for private in ["one", "other", "192.0.2.50", "token=secret"] {
            assert!(!data.contains(private))
        }
        assert_eq!(out[0].failure, Some(Failure::Timeout));
        assert_eq!(
            error_code("failed (0x80072EE2), secret"),
            Some("0x80072EE2".into())
        );
        assert_eq!(
            failure("That code does not match this TV"),
            Failure::CodeMismatch
        );
    }
    #[test]
    fn feedback_defaults_and_redaction_do_not_invent_success() {
        let f: Feedback = serde_json::from_str("{}").unwrap();
        assert_eq!(f.wake_worked, Answer::NotTested);
        let mut f = f;
        f.tv_model = "TCL 192.0.2.50".into();
        f.tv_software = "token=secret".into();
        let clean = f.redacted();
        assert!(clean.tv_model.is_empty() && clean.tv_software.is_empty());
        assert_eq!(Answer::from_index(2), Answer::No);
    }
}
