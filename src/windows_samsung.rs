//! Samsung Tizen LAN control via native WinHTTP; no sequences or cloud.
use crate::{
    samsung,
    tv_control::{Backend, PowerState},
    tvs::{Brand, Connection, Tv},
    windows_lg::{Secret, Socket, protect, unprotect},
};
use std::time::Duration;
fn connect(tv: &Tv) -> Result<Socket, String> {
    tv.validate()?;
    if tv.brand != Brand::Samsung {
        return Err("Select Samsung for this TV.".into());
    }
    let secret = unprotect(tv.credential.as_ref().ok_or("Pair this TV first.")?)?;
    let token = secret.key.strip_prefix("samsung-token:");
    if token.is_none() && secret.key != "samsung-name-authorized" {
        return Err("Pair this Samsung TV again.".into());
    }
    let secure = tv.connection == Connection::Encrypted;
    let socket = Socket::open_path(
        &tv.address,
        secure,
        if secure { 8002 } else { 8001 },
        &samsung::path(token),
        secret.certificate.as_deref(),
        false,
        Duration::from_secs(6),
    )?;
    for _ in 0..64 {
        if samsung::handshake(&socket.receive()?)?.is_some() {
            return Ok(socket);
        }
    }
    Err("The Samsung TV did not complete authorization.".into())
}
pub fn pair(tv: &Tv) -> Result<Vec<u8>, String> {
    tv.validate()?;
    if tv.brand != Brand::Samsung {
        return Err("Samsung pairing requires a Samsung entry.".into());
    }
    let secure = tv.connection == Connection::Encrypted;
    let socket = Socket::open_path(
        &tv.address,
        secure,
        if secure { 8002 } else { 8001 },
        &samsung::path(None),
        None,
        true,
        Duration::from_secs(30),
    )?;
    for _ in 0..64 {
        if let Some(token) = samsung::handshake(&socket.receive()?)? {
            let key = token.map_or("samsung-name-authorized".into(), |t| {
                format!("samsung-token:{t}")
            });
            return protect(&Secret {
                key,
                certificate: socket.certificate.clone(),
            });
        }
    }
    Err("The Samsung TV did not complete pairing. Accept its connection prompt and retry.".into())
}
pub fn info(tv: &Tv) -> Result<samsung::Info, String> {
    tv.validate()?;
    samsung::info(&crate::windows_lg::http_json(
        &tv.address,
        8001,
        "/api/v2/",
    )?)
}
pub fn test_hdmi(tv: &Tv, port: u8) -> Result<(), String> {
    crate::tv_report::measure(&tv.id, crate::tv_report::Action::HdmiTest, || {
        let key = samsung::key(port)?;
        let frame = samsung::key_frame(&key)?;
        let socket = connect(tv)?;
        socket.send(&frame)?;
        crate::diagnostics::event("samsung.command", "sent", &key);
        Ok(())
    })
}
pub struct Controls<'a> {
    tv: &'a Tv,
}
impl<'a> Controls<'a> {
    pub fn new(tv: &'a Tv) -> Self {
        Self { tv }
    }
}
impl Backend for Controls<'_> {
    fn power(&mut self) -> Result<PowerState, String> {
        let power = info(self.tv)?.power;
        crate::diagnostics::event("samsung.power", "reported", power.label());
        Ok(power)
    }
    fn inputs(&mut self) -> Result<Vec<crate::lg_protocol::Input>, String> {
        Ok(self.tv.samsung.inputs())
    }
    fn current_input(&mut self) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn wake(&mut self) -> Result<crate::tv_network::WakeReport, String> {
        crate::tv_network::wake(&self.tv.network)
    }
    fn off(&mut self) -> Result<(), String> {
        let socket = connect(self.tv)?;
        if !info(self.tv)?.power.awake() {
            return Err("The TV did not freshly report On. No power toggle was sent.".into());
        }
        socket.send(&samsung::key_frame("KEY_POWER")?)?;
        crate::diagnostics::event("samsung.command", "sent", "KEY_POWER");
        Ok(())
    }
    fn switch_input(&mut self, id: &str) -> Result<(), String> {
        if !self.tv.samsung.inputs().iter().any(|i| i.id == id) {
            return Err("Confirm this direct HDMI command in TV setup first.".into());
        }
        test_hdmi(self.tv, samsung::input_port(id)?)
    }
    fn input_readback(&self) -> bool {
        false
    }
    fn command_acknowledged(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[test]
#[ignore = "Manual: needs a hand-started local TLS server that writes target/preview/samsung-tls-case.json; no real TV or settings"]
fn samsung_tls_fixture() {
    let value: serde_json::Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/preview/samsung-tls-case.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let port = value["port"].as_u64().unwrap() as u16;
    let certificate = serde_json::from_value::<Vec<u8>>(value["certificate"].clone()).unwrap();
    let result = Socket::open_path(
        "127.0.0.1",
        true,
        port,
        &samsung::path(Some("synthetic-test-token-only")),
        Some(&certificate),
        false,
        Duration::from_secs(3),
    );
    if value["success"] == true {
        let socket = result.unwrap();
        assert!(
            samsung::handshake(&socket.receive().unwrap())
                .unwrap()
                .is_some()
        );
        socket
            .send(&samsung::key_frame("KEY_HDMI2").unwrap())
            .unwrap();
    } else {
        let error = result
            .err()
            .expect("Mismatched TLS certificate was accepted");
        assert!(error.contains("certificate"), "{error}");
    }
}
