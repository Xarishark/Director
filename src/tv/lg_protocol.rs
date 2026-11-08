//! Minimal SSAP messages, independent of Windows transport and UI.
use serde_json::{Value, json};

pub fn registration(key: Option<&str>) -> Value {
    let mut payload = json!({
        "forcePairing": false, "pairingType": "PROMPT",
        "manifest": {"manifestVersion": 1, "appVersion": "1.0",
            "permissions": ["READ_NETWORK_STATE", "READ_INPUT_DEVICE_LIST", "READ_POWER_STATE", "READ_RUNNING_APPS", "CONTROL_INPUT_TV", "CONTROL_POWER", "LAUNCH"]}
    });
    if let Some(key) = key {
        payload["client-key"] = key.into();
    }
    json!({"id": "director-register", "type": "register", "payload": payload})
}
pub enum Registration {
    Prompt,
    Registered(String),
    Ignore,
}
pub fn registered(message: &Value, allow_prompt: bool) -> Result<Registration, String> {
    if message["id"] != "director-register" {
        return Ok(Registration::Ignore);
    }
    if message["type"] == "error" {
        return Err("The TV rejected pairing. Allow Director on the TV, or pair again if access was removed.".into());
    }
    if message["type"] == "registered" {
        let key = message["payload"]["client-key"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or("The TV did not return valid pairing data.")?;
        return Ok(Registration::Registered(key.into()));
    }
    if message["payload"]["pairingType"] == "PROMPT" {
        return if allow_prompt {
            Ok(Registration::Prompt)
        } else {
            Err(
                "The TV needs permission again. Choose Pair and accept the prompt on the TV."
                    .into(),
            )
        };
    }
    Ok(Registration::Ignore)
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub label: String,
}
pub fn inputs(message: &Value) -> Result<Vec<Input>, String> {
    if message["type"] == "error" || message["payload"]["returnValue"] == false {
        return Err("The TV refused the input-list request. Try pairing again.".into());
    }
    let devices = message["payload"]["devices"]
        .as_array()
        .ok_or("The TV returned an unexpected input list.")?;
    if devices.len() > 128 {
        return Err("The TV returned too many inputs.".into());
    }
    devices
        .iter()
        .map(|d| {
            let id = d["id"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .ok_or("The TV returned an invalid input.")?;
            let label = d["label"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256)
                .unwrap_or(id);
            Ok(Input {
                id: id.into(),
                label: label.into(),
            })
        })
        .collect()
}
/// Read only TV-reported adapter addresses; never infer a gateway/neighbor MAC.
pub fn network(message: &Value) -> Result<crate::tv::tv_network::NetworkAddresses, String> {
    if message["type"] == "error" || message["payload"]["returnValue"] == false {
        return Err("The TV did not allow reading its network addresses. You can enter a MAC address manually.".into());
    }
    let read = |kind: &str| {
        message["payload"][kind]["macAddress"]
            .as_str()
            .and_then(|s| s.parse().ok())
    };
    let result = crate::tv::tv_network::NetworkAddresses {
        wired: read("wiredInfo"),
        wifi: read("wifiInfo"),
        manual: None,
    };
    if result.targets().is_empty() {
        return Err(
            "The TV did not report a usable network address. You can enter a MAC address manually."
                .into(),
        );
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_requires_correlated_response_and_explicit_prompt_permission() {
        assert!(matches!(
            registered(&json!({"id":"other","type":"registered"}), true).unwrap(),
            Registration::Ignore
        ));
        let prompt = json!({"id":"director-register","payload":{"pairingType":"PROMPT"}});
        assert!(registered(&prompt, false).is_err());
        assert!(matches!(
            registered(&prompt, true).unwrap(),
            Registration::Prompt
        ));
        assert!(
            registered(
                &json!({"id":"director-register","type":"registered","payload":{}}),
                true
            )
            .is_err()
        );
        let reg = json!({"id":"director-register","type":"registered","payload":{"client-key":"test-secret"}});
        assert!(
            matches!(registered(&reg,true).unwrap(), Registration::Registered(k) if k=="test-secret")
        );
        assert!(registration(None)["payload"].get("client-key").is_none());
    }
    #[test]
    fn errors_never_echo_raw_tv_messages_or_keys() {
        let m =
            json!({"id":"director-register","type":"error","error":"private-key-must-not-escape"});
        assert!(!registered(&m, true).err().unwrap().contains("private-key"));
        assert!(inputs(&json!({"payload":{"returnValue":false,"devices":[]}})).is_err());
        assert!(
            inputs(&json!({"payload":{"devices":[{"id":"HDMI_1","label":"PC"}]}})).unwrap()[0]
                .label
                == "PC"
        );
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reads_only_valid_tv_reported_addresses_and_handles_denial() {
        let m = json!({"payload":{"returnValue":true,"wiredInfo":{"macAddress":"00:11:22:33:44:55"},"wifiInfo":{"macAddress":"02-aa-bb-cc-dd-ee"}}});
        let n = network(&m).unwrap();
        assert_eq!(n.targets().len(), 2);
        assert_eq!(n.wifi.unwrap().to_string(), "02:AA:BB:CC:DD:EE");
        assert!(
            network(&json!({"type":"error","error":"sensitive text"}))
                .err()
                .unwrap()
                .contains("manually")
        );
        assert!(
            network(&json!({"payload":{"wiredInfo":{"macAddress":"00:00:00:00:00:00"}}})).is_err()
        );
        let partial=network(&json!({"payload":{"wiredInfo":{"macAddress":"bad"},"wifiInfo":{"macAddress":"00:11:22:33:44:55"}}})).unwrap();
        assert!(partial.wired.is_none());
        assert!(partial.wifi.is_some());
    }
}

pub fn power_state(message: &Value) -> Result<crate::tv::tv_control::PowerState, String> {
    use crate::tv::tv_control::PowerState;
    if message["type"] == "error" || message["payload"]["returnValue"] == false {
        return Err("The TV refused the power-status request.".into());
    }
    Ok(match message["payload"]["state"].as_str() {
        Some("Active") => PowerState::On,
        Some("Active Standby" | "Standby") => PowerState::Standby,
        Some("Screen Off") => PowerState::ScreenOff,
        Some("Screen Saver") => PowerState::ScreenSaver,
        Some("Suspend" | "Power Off") => PowerState::Off,
        _ => PowerState::Unknown,
    })
}
pub fn command_accepted(message: &Value) -> Result<(), String> {
    if message["type"] == "response" && message["payload"]["returnValue"] == true {
        Ok(())
    } else {
        Err("The TV did not confirm the control request.".into())
    }
}
pub fn mapped_input(devices: &Value, foreground: &Value) -> Option<String> {
    let app = foreground["payload"]["appId"].as_str()?;
    devices["payload"]["devices"]
        .as_array()?
        .iter()
        .find(|d| d["appId"].as_str() == Some(app))
        .and_then(|d| d["id"].as_str())
        .map(str::to_owned)
}
#[cfg(test)]
mod control_protocol_tests {
    use super::*;
    #[test]
    fn status_and_acknowledgement_are_not_inferred_from_connectivity() {
        assert_eq!(
            power_state(&json!({"payload":{"state":"Active"}})).unwrap(),
            crate::tv::tv_control::PowerState::On
        );
        assert_eq!(
            power_state(&json!({"payload":{"state":"future-state"}})).unwrap(),
            crate::tv::tv_control::PowerState::Unknown
        );
        assert!(power_state(&json!({"type":"error"})).is_err());
        assert!(
            command_accepted(&json!({"type":"response","payload":{"returnValue":true}})).is_ok()
        );
        assert!(command_accepted(&json!({"type":"response","payload":{}})).is_err());
        let devices =
            json!({"payload":{"devices":[{"id":"HDMI_1","appId":"com.webos.app.hdmi1"}]}});
        assert_eq!(
            mapped_input(
                &devices,
                &json!({"payload":{"appId":"com.webos.app.hdmi1"}})
            )
            .as_deref(),
            Some("HDMI_1")
        );
        assert!(mapped_input(&devices, &json!({"payload":{"appId":"netflix"}})).is_none());
    }
}
