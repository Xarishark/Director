//! Remembered TVs are independent of presets and Windows display availability.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Brand {
    #[default]
    Lg,
    Samsung,
    TclAndroid,
}
impl Brand {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lg => "LG webOS",
            Self::Samsung => "Samsung Tizen",
            Self::TclAndroid => "TCL — Android / Google TV",
        }
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Connection {
    #[default]
    Encrypted,
    Legacy,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayLink {
    pub device_path: String,
    pub name: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tv {
    pub id: String,
    pub name: String,
    pub address: String,
    pub brand: Brand,
    #[serde(default)]
    pub samsung: crate::samsung::Metadata,
    #[serde(default)]
    pub android: crate::samsung::Metadata,
    pub connection: Connection,
    pub display: Option<DisplayLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<crate::audio::DeviceSelection>,
    // DPAPI-encrypted key and peer certificate pin; never log or export this field.
    pub credential: Option<Vec<u8>>,
    #[serde(default)]
    pub inputs: Vec<crate::lg_protocol::Input>,
    #[serde(default)]
    pub pc_input: Option<crate::lg_protocol::Input>,
    #[serde(default)]
    pub network: crate::tv_network::NetworkAddresses,
}
impl Tv {
    pub fn compatibility(&self) -> &crate::samsung::Metadata {
        if self.brand == Brand::TclAndroid {
            &self.android
        } else {
            &self.samsung
        }
    }
    pub fn compatibility_mut(&mut self) -> &mut crate::samsung::Metadata {
        if self.brand == Brand::TclAndroid {
            &mut self.android
        } else {
            &mut self.samsung
        }
    }
    pub fn can_wake(&self) -> bool {
        self.brand == Brand::TclAndroid || !self.network.targets().is_empty()
    }
    pub fn control_inputs(&self) -> Vec<crate::lg_protocol::Input> {
        if self.brand != Brand::Lg {
            self.compatibility().inputs()
        } else {
            self.inputs.clone()
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 200
            || self.name.trim().is_empty()
            || self.name.len() > 160
        {
            return Err("Give the TV a name of up to 160 bytes.".into());
        }
        let ip = self.address.parse::<Ipv4Addr>().map_err(
            |_| "Enter the TV’s IPv4 address, for example 192.168.1.50. No URL or port is needed.",
        )?;
        if ip.is_unspecified()
            || ip.is_multicast()
            || ip.is_broadcast()
            || ip.is_loopback()
            || ip.octets()[0] == 0
        {
            return Err("Enter the address of a TV on your network.".into());
        }
        if self
            .display
            .as_ref()
            .is_some_and(|d| d.device_path.trim().is_empty() || d.name.trim().is_empty())
        {
            return Err("Choose a known Windows display or leave the link empty.".into());
        }
        if self
            .credential
            .as_ref()
            .is_some_and(|c| c.is_empty() || c.len() > 65536)
        {
            return Err("The saved pairing data is invalid. The file was left unchanged.".into());
        }
        if self.inputs.len() > 128
            || self.inputs.iter().chain(self.pc_input.iter()).any(|i| {
                i.id.is_empty() || i.id.len() > 256 || i.label.is_empty() || i.label.len() > 256
            })
        {
            return Err("The saved TV input information is invalid.".into());
        }
        if self.audio.as_ref().is_some_and(|a| {
            a.id.trim().is_empty()
                || a.name.trim().is_empty()
                || a.id.len() > 1024
                || a.name.len() > 512
        }) {
            return Err("Choose a remembered audio output or leave the link empty.".into());
        }
        self.samsung.validate()?;
        self.android.validate()?;
        if self.brand == Brand::TclAndroid && self.connection != Connection::Encrypted {
            return Err("Android TV requires an encrypted connection.".into());
        }
        Ok(())
    }
    /// Shared save boundary: preserve pairing for metadata edits, never for a new endpoint.
    pub fn prepare_for_save(&mut self, previous: Option<&Self>) -> Result<(), String> {
        self.name = self.name.trim().into();
        self.address = self.address.trim().into();
        self.validate()?;
        if previous.is_some_and(|old| !old.same_endpoint(self)) {
            self.credential = None;
            self.samsung.tests.clear();
            self.android.tests.clear();
            if self.brand != Brand::Lg {
                self.inputs.clear();
                self.pc_input = None;
            }
        }
        Ok(())
    }
    pub fn same_endpoint(&self, other: &Self) -> bool {
        self.address == other.address
            && self.connection == other.connection
            && self.brand == other.brand
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    tvs: Vec<Tv>,
}
pub fn path() -> Result<PathBuf, String> {
    crate::storage::settings_path("tvs.json")
}
pub fn load(path: &Path) -> Result<Vec<Tv>, String> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(_) => return Err("Could not read saved TVs. The file was left unchanged.".into()),
    };
    if data.len() > 1024 * 1024 {
        return Err("The saved TV file is too large. It was left unchanged.".into());
    }
    let doc: Document = serde_json::from_slice(&data)
        .map_err(|_| "Could not read saved TVs. The file was left unchanged.")?;
    if doc.version != 1 {
        return Err("Unsupported TV file version. The file was left unchanged.".into());
    }
    validate(&doc.tvs)?;
    Ok(doc.tvs)
}
fn validate(tvs: &[Tv]) -> Result<(), String> {
    for (index, tv) in tvs.iter().enumerate() {
        tv.validate()?;
        if tvs[..index].iter().any(|other| {
            other.id == tv.id
                || other.name.to_lowercase() == tv.name.to_lowercase()
                || other.address == tv.address
        }) {
            return Err("Each TV needs a unique name and IP address.".into());
        }
    }
    Ok(())
}
/// Caller holds the shared operation lock, including across pairing.
pub fn save(path: &Path, expected: &[Tv], next: &[Tv]) -> Result<(), String> {
    validate(next)?;
    crate::cli_name::check_new_names(expected, next, |t| (&t.id, &t.name), "TV")?;
    if load(path)? != expected {
        return Err(
            "Saved TVs changed in another window. Reopen the TVs page before saving.".into(),
        );
    }
    crate::storage::write_document(
        path,
        &Document {
            version: 1,
            tvs: next.to_vec(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tv() -> Tv {
        Tv {
            id: "one".into(),
            name: "Living room".into(),
            address: "192.168.1.50".into(),
            brand: Default::default(),
            samsung: Default::default(),
            android: Default::default(),
            connection: Connection::Encrypted,
            display: None,
            audio: None,
            credential: None,
            inputs: vec![],
            pc_input: None,
            network: Default::default(),
        }
    }
    #[test]
    fn validates_network_address_and_independent_display_link() {
        let mut t = tv();
        assert!(t.validate().is_ok());
        for address in [
            "http://192.168.1.50",
            "192.168.1.50:3001",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "127.0.0.1",
        ] {
            t.address = address.into();
            assert!(t.validate().is_err());
        }
        t.address = "192.168.1.50".into();
        t.display = Some(DisplayLink {
            device_path: "remembered-disconnected-target".into(),
            name: "TV".into(),
        });
        assert!(t.validate().is_ok());
    }
    #[test]
    fn metadata_edits_preserve_pairing_and_endpoint_edits_clear_it() {
        let mut original = tv();
        original.credential = Some(vec![1, 2, 3]);
        let mut edit = original.clone();
        edit.name = "  Renamed TV  ".into();
        edit.display = Some(DisplayLink {
            device_path: "offline-display".into(),
            name: "LG".into(),
        });
        edit.prepare_for_save(Some(&original)).unwrap();
        assert_eq!(edit.name, "Renamed TV");
        assert!(edit.credential == original.credential);
        edit.address = " 192.168.1.51 ".into();
        edit.prepare_for_save(Some(&original)).unwrap();
        assert!(edit.credential.is_none());
        assert!(edit.display.is_some());
        let mut legacy = original.clone();
        legacy.connection = Connection::Legacy;
        legacy.prepare_for_save(Some(&original)).unwrap();
        assert!(legacy.credential.is_none());
    }
    #[test]
    fn rejects_duplicate_endpoints_and_names() {
        let a = tv();
        let mut b = a.clone();
        b.id = "two".into();
        b.name = "Other".into();
        assert!(validate(&[a.clone(), b.clone()]).is_err());
        b.address = "192.168.1.51".into();
        b.name = "LIVING ROOM".into();
        assert!(validate(&[a, b]).is_err());
    }
    #[test]
    fn storage_preserves_sealed_credentials_stale_writes_and_corrupt_files() {
        let path = std::env::temp_dir().join(format!(
            "director-tv-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut t = tv();
        t.credential = Some(vec![4, 8, 16]);
        save(&path, &[], &[t.clone()]).unwrap();
        assert!(load(&path).unwrap() == [t]);
        let before = fs::read(&path).unwrap();
        assert!(save(&path, &[], &[]).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&path, b"{broken").unwrap();
        assert!(save(&path, &[], &[]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod tv_file_tests {
    use super::*;
    #[test]
    fn audio_link_roundtrips_without_repairing_or_changing_display() {
        let mut t:Tv=serde_json::from_value(serde_json::json!({"id":"link","name":"TV","brand":"lg","connection":"encrypted","address":"192.0.2.50","display":null,"credential":[4,8]})).unwrap();
        let old = t.clone();
        t.audio = Some(crate::audio::DeviceSelection {
            id: "remembered-hdmi".into(),
            name: "TV audio".into(),
        });
        t.prepare_for_save(Some(&old)).unwrap();
        assert_eq!(t.credential, old.credential);
        assert!(t.display.is_none());
        let data = serde_json::to_vec(&t).unwrap();
        let back: Tv = serde_json::from_slice(&data).unwrap();
        assert!(back == t);
        let mut unlinked = serde_json::to_value(&t).unwrap();
        unlinked.as_object_mut().unwrap().remove("audio");
        let back: Tv = serde_json::from_value(unlinked).unwrap();
        assert!(back.audio.is_none());
        t.audio.as_mut().unwrap().id.clear();
        assert!(t.validate().is_err());
    }
    #[test]
    fn brand_change_invalidates_auth_and_tests() {
        let mut tv:Tv=serde_json::from_value(serde_json::json!({"id":"lg","name":"LG","brand":"lg","connection":"encrypted","address":"192.0.2.50","display":null,"credential":[1,2]})).unwrap();
        let old = tv.clone();
        tv.brand = Brand::Samsung;
        tv.samsung.record(2, true);
        tv.prepare_for_save(Some(&old)).unwrap();
        assert!(tv.credential.is_none() && tv.samsung.tests.is_empty());
    }
    #[test]
    fn android_metadata_is_independent_and_reports_omit_private_identity() {
        let mut tv:Tv=serde_json::from_value(serde_json::json!({"id":"private-tv-id-123","name":"Private living room alias","brand":"lg","connection":"encrypted","address":"192.0.2.50","display":null,"credential":null})).unwrap();
        tv.brand = Brand::TclAndroid;
        tv.credential = Some(vec![21, 42]);
        tv.android.model = Some("TCL fixture".into());
        tv.android.firmware = Some("5.2.0".into());
        let i = tv.android.record(2, true);
        tv.android.tests[i].confirmed = Some(true);
        tv.samsung.record(1, true);
        assert_eq!(tv.control_inputs()[0].id, "HDMI_2");
        assert!(tv.can_wake());
        let report = crate::samsung::report(&tv).unwrap();
        assert_eq!(report.brand, "TCL — Android / Google TV");
        assert_eq!(report.version_kind, "Android TV Remote Service version");
        let data = serde_json::to_string(&report).unwrap();
        for private in [&tv.name, &tv.address, &tv.id] {
            assert!(!data.contains(private))
        }
        let old = tv.clone();
        tv.address = "192.168.1.51".into();
        tv.prepare_for_save(Some(&old)).unwrap();
        assert!(tv.credential.is_none());
        assert!(tv.control_inputs().is_empty());
        tv.connection = Connection::Legacy;
        assert!(tv.validate().is_err());
    }
}
