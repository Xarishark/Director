//! Samsung direct-command capabilities and a deliberately redacted compatibility report.
use crate::{lg_protocol::Input, tv_control::PowerState};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub model: Option<String>,
    pub firmware: Option<String>,
    #[serde(default)]
    pub token_required: Option<bool>,
    #[serde(default)]
    pub tests: Vec<InputTest>,
    #[serde(default)]
    pub feedback: crate::tv_report::Feedback,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputTest {
    pub port: u8,
    pub sent: bool,
    pub confirmed: Option<bool>,
    pub at_unix_seconds: u64,
    pub model: Option<String>,
    pub firmware: Option<String>,
}
impl Metadata {
    pub fn validate(&self) -> Result<(), String> {
        self.feedback.validate()?;
        if self.tests.len() > 64
            || self.model.as_ref().is_some_and(|s| s.len() > 160)
            || self.firmware.as_ref().is_some_and(|s| s.len() > 160)
            || self.tests.iter().any(|t| {
                !(1..=4).contains(&t.port)
                    || (!t.sent && t.confirmed == Some(true))
                    || t.model.as_ref().is_some_and(|s| s.len() > 160)
                    || t.firmware.as_ref().is_some_and(|s| s.len() > 160)
            })
        {
            return Err("Invalid TV compatibility data.".into());
        }
        Ok(())
    }
    pub fn inputs(&self) -> Vec<Input> {
        (1..=4)
            .filter(|port| {
                self.tests
                    .iter()
                    .rev()
                    .find(|t| t.port == *port)
                    .is_some_and(|t| {
                        t.sent
                            && t.confirmed == Some(true)
                            && t.model == self.model
                            && t.firmware == self.firmware
                    })
            })
            .map(|port| Input {
                id: format!("HDMI_{port}"),
                label: format!("HDMI {port}"),
            })
            .collect()
    }
    pub fn record(&mut self, port: u8, sent: bool) -> usize {
        if self.tests.len() >= 64 {
            self.tests.remove(0);
        }
        self.tests.push(InputTest {
            port,
            sent,
            confirmed: None,
            at_unix_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            model: self.model.clone(),
            firmware: self.firmware.clone(),
        });
        self.tests.len() - 1
    }
}
#[derive(Clone, Debug)]
pub struct Info {
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub token_required: Option<bool>,
    pub power: PowerState,
    pub network: crate::tv_network::NetworkAddresses,
}
pub(crate) fn safe_text(value: &serde_json::Value) -> Option<String> {
    let s = value.as_str()?;
    if s.is_empty()
        || s.len() > 160
        || s.split_whitespace()
            .any(|part| part.parse::<crate::tv_network::MacAddress>().is_ok())
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " ._-/()".contains(c))
        || s.split(|c: char| !c.is_ascii_digit() && c != '.')
            .any(|s| s.parse::<std::net::Ipv4Addr>().is_ok())
    {
        None
    } else {
        Some(s.into())
    }
}
pub fn info(value: &serde_json::Value) -> Result<Info, String> {
    let d = value
        .get("device")
        .filter(|d| d.is_object())
        .ok_or("The address did not return Samsung TV information.")?;
    let model = safe_text(&d["modelName"]).or_else(|| safe_text(&d["model"]));
    let firmware = safe_text(&d["firmwareVersion"]).or_else(|| safe_text(&d["firmware"]));
    let token_required = d["TokenAuthSupport"].as_bool().or_else(|| {
        d["TokenAuthSupport"]
            .as_str()
            .and_then(|s| match s.to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            })
    });
    let power = match d["PowerState"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "on" => PowerState::On,
        "standby" => PowerState::Standby,
        "off" => PowerState::Off,
        _ => PowerState::Unknown,
    };
    let network = crate::tv_network::NetworkAddresses {
        wifi: d["wifiMac"].as_str().and_then(|s| s.parse().ok()),
        wired: d["networkMac"]
            .as_str()
            .or_else(|| d["wiredMac"].as_str())
            .and_then(|s| s.parse().ok()),
        ..Default::default()
    };
    Ok(Info {
        model,
        firmware,
        token_required,
        power,
        network,
    })
}
pub fn key(port: u8) -> Result<String, String> {
    if !(1..=4).contains(&port) {
        Err("Choose HDMI 1–4 for a direct-command test.".into())
    } else {
        Ok(format!("KEY_HDMI{port}"))
    }
}
pub fn input_port(id: &str) -> Result<u8, String> {
    let port = id
        .strip_prefix("HDMI_")
        .ok_or("Choose a confirmed HDMI input.")?
        .parse::<u8>()
        .map_err(|_| "Invalid HDMI input.")?;
    key(port)?;
    Ok(port)
}
pub fn key_frame(key: &str) -> Result<serde_json::Value, String> {
    if key != "KEY_POWER" && !(1..=4).any(|p| key == format!("KEY_HDMI{p}")) {
        return Err("Only supported direct commands may be sent.".into());
    }
    Ok(
        serde_json::json!({"method":"ms.remote.control","params":{"Cmd":"Click","DataOfCmd":key,"Option":"false","TypeOfRemote":"SendRemoteKey"}}),
    )
}
pub fn handshake(value: &serde_json::Value) -> Result<Option<Option<String>>, String> {
    match value["event"].as_str(){Some("ms.channel.connect")=>{let token=value["data"]["token"].as_str().or_else(||value["data"]["clients"][0]["attributes"]["token"].as_str());if token.is_some_and(|t|t.is_empty()||t.len()>4096||t.chars().any(char::is_control)){return Err("The TV returned invalid authorization data.".into())}Ok(Some(token.map(str::to_string)))},Some("ms.channel.unauthorized")=>Err("The TV denied access. Remove Director from its device connection settings and pair again.".into()),Some("ms.channel.timeOut")=>Err("Accept the connection prompt on the Samsung TV and retry pairing.".into()),Some("ms.error")=>Err("The Samsung TV rejected the control connection.".into()),_=>Ok(None)}
}
pub fn path(token: Option<&str>) -> String {
    let mut path = "/api/v2/channels/samsung.remote.control?name=RGlyZWN0b3I=".to_string();
    if let Some(token) = token {
        path.push_str("&token=");
        for b in token.bytes() {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                path.push(b as char)
            } else {
                path.push_str(&format!("%{b:02X}"));
            }
        }
    }
    path
}
#[derive(Serialize)]
pub struct CompatibilityReport {
    pub format_version: u32,
    pub director_version: &'static str,
    pub brand: &'static str,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub version_kind: &'static str,
    pub paired: bool,
    pub connection: &'static str,
    pub token_required: Option<bool>,
    pub input_readback: bool,
    pub tests: Vec<InputTest>,
    pub confirmed_inputs: Vec<Input>,
    pub feedback: crate::tv_report::Feedback,
    pub environment: crate::tv_report::Environment,
    pub wake_configuration: crate::tv_report::WakeConfiguration,
    pub diagnostics: crate::tv_report::History,
    pub note: &'static str,
}
pub fn report(tv: &crate::tvs::Tv) -> Result<CompatibilityReport, String> {
    if tv.brand == crate::tvs::Brand::Lg {
        return Err("Compatibility reports are available for Samsung and TCL Android TVs.".into());
    }
    let clean = |s: &Option<String>| {
        s.as_ref()
            .and_then(|s| safe_text(&serde_json::Value::String(s.clone())))
    };
    let mut tests = tv.compatibility().tests.clone();
    for test in &mut tests {
        test.model = clean(&test.model);
        test.firmware = clean(&test.firmware);
    }
    Ok(CompatibilityReport {
        format_version: 1,
        director_version: env!("CARGO_PKG_VERSION"),
        brand: tv.brand.label(),
        model: clean(&tv.compatibility().model),
        firmware: clean(&tv.compatibility().firmware),
        version_kind: if tv.brand == crate::tvs::Brand::TclAndroid {
            "Android TV Remote Service version"
        } else {
            "TV firmware"
        },
        paired: tv.credential.is_some(),
        connection: if tv.brand == crate::tvs::Brand::TclAndroid {
            "Mutual TLS / Android TV Remote v2"
        } else if tv.connection == crate::tvs::Connection::Encrypted {
            "TLS WebSocket"
        } else {
            "Legacy WebSocket"
        },
        token_required: if tv.brand == crate::tvs::Brand::TclAndroid {
            None
        } else {
            tv.samsung.token_required
        },
        input_readback: false,
        tests,
        confirmed_inputs: tv.compatibility().inputs(),
        feedback: tv.compatibility().feedback.redacted(),
        environment: crate::tv_report::environment(),
        wake_configuration: crate::tv_report::wake_configuration(tv),
        diagnostics: crate::tv_report::history(&tv.id),
        note: "Remote commands have no per-key acknowledgment. Confirmed means the user reported a successful switch on this TV with this observed model/firmware. It is not a guarantee for other TVs. No address, MAC, alias, token, certificate, device links or raw responses are included.",
    })
}
pub fn export(tv: &crate::tvs::Tv) -> Result<std::path::PathBuf, String> {
    let data = report(tv)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "The system clock could not timestamp the report.")?
        .as_nanos();
    let path = crate::storage::settings_path("Reports")?.join(format!(
        "{}-compatibility-{now}.json",
        if tv.brand == crate::tvs::Brand::TclAndroid {
            "android-tv"
        } else {
            "samsung"
        }
    ));
    crate::storage::write_document(&path, &data)?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn power_is_reported_not_inferred_from_http_success() {
        assert_eq!(
            info(&serde_json::json!({"device":{}})).unwrap().power,
            PowerState::Unknown
        );
        assert_eq!(
            info(&serde_json::json!({"device":{"PowerState":"standby"}}))
                .unwrap()
                .power,
            PowerState::Standby
        );
        assert!(info(&serde_json::json!({"ok":true})).is_err());
    }
    #[test]
    fn only_latest_confirmed_direct_test_for_current_firmware_enables_input() {
        let mut m = Metadata {
            model: Some("S90C".into()),
            firmware: Some("1234".into()),
            ..Default::default()
        };
        let i = m.record(2, true);
        assert!(m.inputs().is_empty());
        m.tests[i].confirmed = Some(true);
        assert_eq!(m.inputs()[0].id, "HDMI_2");
        m.firmware = Some("9999".into());
        assert!(m.inputs().is_empty());
        m.firmware = Some("1234".into());
        m.record(2, false);
        assert!(m.inputs().is_empty());
    }
    #[test]
    fn frames_and_handshake_are_bounded_and_sequences_rejected() {
        assert!(key_frame("KEY_SOURCE,KEY_RIGHT,KEY_ENTER").is_err());
        assert!(key(5).is_err());
        let frame = key_frame(&key(2).unwrap()).unwrap();
        assert_eq!(frame["params"]["DataOfCmd"], "KEY_HDMI2");
        assert_eq!(
            handshake(&serde_json::json!({"event":"ms.channel.connect","data":{}})).unwrap(),
            Some(None)
        );
        assert!(handshake(&serde_json::json!({"event":"ms.channel.unauthorized"})).is_err());
        assert!(path(Some("a&b")).ends_with("token=a%26b"));
    }
    #[test]
    fn report_is_an_allowlist_and_never_serializes_secrets_or_addresses() {
        let tv:crate::tvs::Tv=serde_json::from_value(serde_json::json!({"id":"private-id","name":"private-alias","brand":"samsung","connection":"encrypted","address":"192.0.2.50","display":null,"credential":[1,2,3],"samsung":{"model":"S90C","firmware":"1234","tests":[]}})).unwrap();
        let data = serde_json::to_string(&report(&tv).unwrap()).unwrap();
        assert!(data.contains("S90C"));
        for value in [
            "private-id",
            "private-alias",
            "192.0.2.50",
            "credential",
            "certificate",
        ] {
            if value == "certificate" {
                continue;
            }
            assert!(!data.contains(value));
        }
        assert!(safe_text(&serde_json::json!("192.0.2.50")).is_none());
        assert!(safe_text(&serde_json::json!("00:11:22:33:44:55")).is_none());
        assert!(safe_text(&serde_json::json!("S90C 00-11-22-33-44-55")).is_none());
    }
}
