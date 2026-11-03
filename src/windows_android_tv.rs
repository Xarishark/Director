//! In-process, mutually authenticated TLS for TCL Android/Google TV Remote v2.
use crate::{
    android_tv as wire,
    tv_control::{Backend, PowerState},
    tvs::{Brand, Tv},
};
use rsa::pkcs8::EncodePrivateKey;
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    net::{SocketAddr, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

fn error(_: impl std::fmt::Display) -> String {
    "The Android TV secure connection failed. Check its address, Android TV Remote Service and saved pairing.".into()
}
#[derive(Debug)]
struct Verify {
    pin: Option<Vec<u8>>,
}
fn public(der: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    use x509_parser::public_key::PublicKey;
    let (_, c) = x509_parser::parse_x509_certificate(der).map_err(error)?;
    match c.public_key().parsed().map_err(error)? {
        PublicKey::RSA(r) => Ok((
            r.modulus.iter().skip_while(|b| **b == 0).copied().collect(),
            r.exponent
                .iter()
                .skip_while(|b| **b == 0)
                .copied()
                .collect(),
        )),
        _ => Err("Android TV pairing requires an RSA certificate.".into()),
    }
}
impl ServerCertVerifier for Verify {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let actual = public(cert.as_ref())
            .map_err(|_| rustls::Error::General("Invalid Android TV certificate".into()))?;
        if let Some(pin) = &self.pin {
            let expected = public(pin)
                .map_err(|_| rustls::Error::General("Invalid saved TV certificate".into()))?;
            if actual != expected {
                return Err(rustls::Error::General(
                    "The saved TV identity does not match. Pair again explicitly.".into(),
                ));
            }
        }
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        sig: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            msg,
            cert,
            sig,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        sig: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            msg,
            cert,
            sig,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    cert: Vec<u8>,
    key: Vec<u8>,
    peer: Vec<u8>,
}
impl Drop for Identity {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.key.zeroize();
    }
}
fn identity() -> Result<Identity, String> {
    let key = rsa::RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048)
        .map_err(error)?
        .to_pkcs8_der()
        .map_err(error)?;
    let pk = PrivatePkcs8KeyDer::from(key.as_bytes().to_vec());
    let pair = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&pk, &rcgen::PKCS_RSA_SHA256)
        .map_err(error)?;
    let mut params = rcgen::CertificateParams::new(vec![]).map_err(error)?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Director");
    let cert = params.self_signed(&pair).map_err(error)?.der().to_vec();
    Ok(Identity {
        cert,
        key: key.as_bytes().to_vec(),
        peer: vec![],
    })
}
// Bound each socket read by the operation deadline, including fragmented TLS records.
struct BoundedTcp {
    tcp: TcpStream,
    deadline: Instant,
}
impl std::io::Read for BoundedTcp {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        self.tcp.set_read_timeout(Some(left))?;
        std::io::Read::read(&mut self.tcp, b)
    }
}
impl std::io::Write for BoundedTcp {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        std::io::Write::write(&mut self.tcp, b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.tcp)
    }
}
type Stream = StreamOwned<ClientConnection, BoundedTcp>;
fn read_frame(s: &mut Stream) -> Result<Vec<u8>, String> {
    s.sock.deadline = Instant::now() + Duration::from_secs(4);
    wire::read(s)
}

fn open(address: &str, port: u16, id: &Identity, pairing: bool) -> Result<Stream, String> {
    let addr: SocketAddr = format!("{address}:{port}").parse().map_err(error)?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(4)).map_err(error)?;
    tcp.set_read_timeout(Some(Duration::from_secs(4)))
        .map_err(error)?;
    tcp.set_write_timeout(Some(Duration::from_secs(4)))
        .map_err(error)?;
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(error)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Verify {
                pin: if pairing { None } else { Some(id.peer.clone()) },
            }))
            .with_client_auth_cert(
                vec![CertificateDer::from(id.cert.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(id.key.clone())),
            )
            .map_err(error)?;
    let name = ServerName::try_from(address.to_string()).map_err(error)?;
    let mut conn = ClientConnection::new(Arc::new(config), name).map_err(error)?;
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut tcp = BoundedTcp { tcp, deadline };
    while conn.is_handshaking() {
        if Instant::now() > deadline {
            return Err("Android TV TLS handshake timed out.".into());
        }
        conn.complete_io(&mut tcp).map_err(error)?;
    }
    Ok(StreamOwned::new(conn, tcp))
}
pub struct Pairing {
    tv_id: String,
    stream: Stream,
    id: Identity,
    deadline: Instant,
}
impl Pairing {
    pub fn start(tv: &Tv) -> Result<Self, String> {
        tv.validate()?;
        if tv.brand != Brand::TclAndroid {
            return Err("Select TCL Android / Google TV.".into());
        }
        let mut pending =
            crate::tv_report::measure(&tv.id, crate::tv_report::Action::PairStart, || {
                Self::start_at(&tv.address, 6467)
            })?;
        pending.tv_id = tv.id.clone();
        Ok(pending)
    }
    fn start_at(address: &str, port: u16) -> Result<Self, String> {
        let mut id = identity()?;
        let mut stream = open(address, port, &id, true)?;
        id.peer = stream
            .conn
            .peer_certificates()
            .and_then(|v| v.first())
            .ok_or("The TV did not provide a certificate.")?
            .as_ref()
            .to_vec();
        let req = [wire::bytes(1, b"atvremote"), wire::bytes(2, b"Director")].concat();
        wire::write(&mut stream, &wire::pairing(10, &req))?;
        wire::pair_reply(&read_frame(&mut stream)?, 11)?;
        let enc = [wire::integer(1, 3), wire::integer(2, 6)].concat();
        wire::write(
            &mut stream,
            &wire::pairing(20, &[wire::bytes(1, &enc), wire::integer(3, 1)].concat()),
        )?;
        wire::pair_reply(&read_frame(&mut stream)?, 20)?;
        wire::write(
            &mut stream,
            &wire::pairing(30, &[wire::bytes(1, &enc), wire::integer(2, 1)].concat()),
        )?;
        wire::pair_reply(&read_frame(&mut stream)?, 31)?;
        Ok(Self {
            tv_id: String::new(),
            stream,
            id,
            deadline: Instant::now() + Duration::from_secs(120),
        })
    }
    pub fn finish(mut self, code: &str) -> Result<Vec<u8>, String> {
        let id = self.tv_id.clone();
        crate::tv_report::measure(&id, crate::tv_report::Action::PairConfirm, || {
            if Instant::now() > self.deadline {
                return Err("The pairing code expired. Start pairing again.".into());
            }
            let code = wire::code(code)?;
            let secret = pair_secret(&self.id.cert, &self.id.peer, code)?;
            wire::write(
                &mut self.stream,
                &wire::pairing(40, &wire::bytes(1, &secret)),
            )?;
            wire::pair_reply(&read_frame(&mut self.stream)?, 41)?;
            crate::windows_lg::protect_document(&self.id)
        })
    }
}
fn pair_secret(client: &[u8], server: &[u8], code: [u8; 3]) -> Result<[u8; 32], String> {
    let (cn, ce) = public(client)?;
    let (sn, se) = public(server)?;
    let mut h = Sha256::new();
    h.update(cn);
    h.update(ce);
    h.update(sn);
    h.update(se);
    h.update(&code[1..]);
    let hash: [u8; 32] = h.finalize().into();
    if hash[0] != code[0] {
        return Err(
            "That code does not match this TV. Start pairing again and enter the new code.".into(),
        );
    }
    Ok(hash)
}
fn load(tv: &Tv) -> Result<Identity, String> {
    tv.validate()?;
    if tv.brand != Brand::TclAndroid {
        return Err("This is not a TCL Android TV entry.".into());
    }
    let id: Identity =
        crate::windows_lg::unprotect_document(tv.credential.as_ref().ok_or("Pair the TV first.")?)?;
    if id.cert.is_empty() || id.key.is_empty() || id.peer.is_empty() {
        return Err("Pair this Android TV again.".into());
    }
    Ok(id)
}
pub struct Remote {
    stream: Stream,
    pub power: PowerState,
    pub model: Option<String>,
    pub version: Option<String>,
    keys: bool,
    features: u64,
}
impl Remote {
    pub fn connect(tv: &Tv) -> Result<Self, String> {
        Self::connect_to(&tv.address, 6466, &load(tv)?)
    }
    fn connect_to(address: &str, port: u16, id: &Identity) -> Result<Self, String> {
        let stream = open(address, port, id, false)?;
        let mut r = Self {
            stream,
            power: PowerState::Unknown,
            model: None,
            version: None,
            keys: false,
            features: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(12);
        for _ in 0..64 {
            if Instant::now() > deadline {
                break;
            }
            if r.receive()? {
                return Ok(r);
            }
        }
        Err("Android TV did not finish its control handshake.".into())
    }
    fn receive(&mut self) -> Result<bool, String> {
        let d = read_frame(&mut self.stream)?;
        if wire::nested(&d, 3)?.is_some() {
            return Err("Android TV rejected the control session.".into());
        }
        if let Some(cfg) = wire::nested(&d, 1)? {
            let offered = wire::int(cfg, 1)?.ok_or("Missing Android TV features.")?;
            let supported = offered & 35;
            self.keys = supported & 2 != 0;
            self.features = supported;
            if let Some(info) = wire::nested(cfg, 2)? {
                self.model = text(info, 1)?;
                self.version = text(info, 6)?;
            }
            let info = [
                wire::integer(3, 1),
                wire::bytes(4, b"1"),
                wire::bytes(5, b"director"),
                wire::bytes(6, env!("CARGO_PKG_VERSION").as_bytes()),
            ]
            .concat();
            wire::write(
                &mut self.stream,
                &wire::bytes(
                    1,
                    &[wire::integer(1, supported), wire::bytes(2, &info)].concat(),
                ),
            )?;
        }
        if let Some(active) = wire::nested(&d, 2)? {
            let _ = wire::int(active, 1)?;
            let flags = self.features;
            wire::write(&mut self.stream, &wire::bytes(2, &wire::integer(1, flags)))?;
        }
        if let Some(ping) = wire::nested(&d, 8)? {
            wire::write(
                &mut self.stream,
                &wire::bytes(9, &wire::integer(1, wire::int(ping, 1)?.unwrap_or(0))),
            )?;
        }
        if let Some(start) = wire::nested(&d, 40)? {
            self.power = if wire::int(start, 1)?.unwrap_or(0) != 0 {
                PowerState::On
            } else {
                PowerState::Standby
            };
            return Ok(true);
        }
        Ok(false)
    }
    fn send(&mut self, frame: &[u8]) -> Result<(), String> {
        if !self.keys {
            return Err("This TV did not advertise direct key support.".into());
        }
        wire::write(&mut self.stream, frame)
    }
}
fn text(data: &[u8], field: u32) -> Result<Option<String>, String> {
    Ok(wire::nested(data, field)?
        .and_then(|v| std::str::from_utf8(v).ok())
        .and_then(|v| crate::samsung::safe_text(&serde_json::json!(v))))
}
pub fn test_hdmi(tv: &Tv, port: u8) -> Result<(), String> {
    crate::tv_report::measure(&tv.id, crate::tv_report::Action::HdmiTest, || {
        let mut r = Remote::connect(tv)?;
        r.send(&wire::hdmi(port)?)?;
        crate::diagnostics::event("android_tv.command", "sent", "direct HDMI test");
        Ok(())
    })
}
pub struct Controls<'a> {
    tv: &'a Tv,
    remote: Option<Remote>,
}
impl<'a> Controls<'a> {
    pub fn new(tv: &'a Tv) -> Self {
        Self { tv, remote: None }
    }
    fn remote(&mut self) -> Result<&mut Remote, String> {
        if self.remote.is_none() {
            self.remote = Some(Remote::connect(self.tv)?)
        }
        self.remote
            .as_mut()
            .ok_or("Android TV session unavailable.".into())
    }
}
impl Backend for Controls<'_> {
    fn power(&mut self) -> Result<PowerState, String> {
        Ok(self.remote()?.power)
    }
    fn inputs(&mut self) -> Result<Vec<crate::lg_protocol::Input>, String> {
        Ok(self.tv.android.inputs())
    }
    fn current_input(&mut self) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn wake(&mut self) -> Result<crate::tv_network::WakeReport, String> {
        match self.remote() {
            Ok(r) => {
                r.send(&wire::key(224))?;
                Ok(crate::tv_network::WakeReport {
                    targets: 1,
                    packets: 1,
                })
            }
            Err(e) => {
                if self.tv.network.targets().is_empty() {
                    Err(format!(
                        "{e} Enable Screenless service / network standby on the TV, or configure its wake MAC address."
                    ))
                } else {
                    crate::tv_network::wake(&self.tv.network)
                }
            }
        }
    }
    fn off(&mut self) -> Result<(), String> {
        self.remote()?.send(&wire::key(223))?;
        crate::diagnostics::event("android_tv.command", "sent", "SLEEP");
        Ok(())
    }
    fn switch_input(&mut self, id: &str) -> Result<(), String> {
        if !self.tv.android.inputs().iter().any(|i| i.id == id) {
            return Err("Confirm this HDMI command in TV setup first.".into());
        }
        let frame = wire::hdmi(crate::samsung::input_port(id)?)?;
        self.remote()?.send(&frame)
    }
    fn input_readback(&self) -> bool {
        false
    }
    fn command_acknowledged(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::{
        DistinguishedName, ServerConfig, ServerConnection,
        server::danger::{ClientCertVerified, ClientCertVerifier},
    };
    use std::{net::TcpListener, sync::mpsc, thread};
    #[derive(Debug)]
    struct Clients;
    impl ClientCertVerifier for Clients {
        fn root_hint_subjects(&self) -> &[DistinguishedName] {
            &[]
        }
        fn verify_client_cert(
            &self,
            c: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: UnixTime,
        ) -> Result<ClientCertVerified, rustls::Error> {
            public(c.as_ref())
                .map_err(|_| rustls::Error::General("invalid fixture client".into()))?;
            Ok(ClientCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            m: &[u8],
            c: &CertificateDer<'_>,
            s: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            ServerCertVerifier::verify_tls12_signature(&Verify { pin: None }, m, c, s)
        }
        fn verify_tls13_signature(
            &self,
            m: &[u8],
            c: &CertificateDer<'_>,
            s: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            ServerCertVerifier::verify_tls13_signature(&Verify { pin: None }, m, c, s)
        }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            ServerCertVerifier::supported_verify_schemes(&Verify { pin: None })
        }
    }
    fn config(id: &Identity) -> Arc<ServerConfig> {
        Arc::new(
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_client_cert_verifier(Arc::new(Clients))
                .with_single_cert(
                    vec![CertificateDer::from(id.cert.clone())],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(id.key.clone())),
                )
                .unwrap(),
        )
    }
    fn listener() -> TcpListener {
        TcpListener::bind("127.0.0.1:0").unwrap()
    }
    fn accept(l: TcpListener, c: Arc<ServerConfig>) -> StreamOwned<ServerConnection, TcpStream> {
        let (tcp, _) = l.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
        tcp.set_write_timeout(Some(Duration::from_secs(8))).unwrap();
        StreamOwned::new(ServerConnection::new(c).unwrap(), tcp)
    }
    #[test]
    fn full_mutual_tls_code_pairing_then_control_negotiation() {
        let server = identity().unwrap();
        let cert = server.cert.clone();
        let cfg = config(&server);
        let l = listener();
        let port = l.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let mut s = accept(l, cfg);
            wire::pair_reply(&wire::read(&mut s).unwrap(), 10).unwrap();
            let client = s.conn.peer_certificates().unwrap()[0].as_ref().to_vec();
            wire::write(&mut s, &wire::pairing(11, &[])).unwrap();
            wire::pair_reply(&wire::read(&mut s).unwrap(), 20).unwrap();
            wire::write(
                &mut s,
                &wire::pairing(
                    20,
                    &wire::bytes(2, &[wire::integer(1, 3), wire::integer(2, 6)].concat()),
                ),
            )
            .unwrap();
            wire::pair_reply(&wire::read(&mut s).unwrap(), 30).unwrap();
            wire::write(&mut s, &wire::pairing(31, &[])).unwrap();
            let (cn, ce) = public(&client).unwrap();
            let (sn, se) = public(&cert).unwrap();
            let mut h = Sha256::new();
            for b in [cn, ce, sn, se, vec![0x12, 0x34]] {
                h.update(b)
            }
            let hash: [u8; 32] = h.finalize().into();
            tx.send(format!("{:02X}1234", hash[0])).unwrap();
            let reply = wire::read(&mut s).unwrap();
            wire::pair_reply(&reply, 40).unwrap();
            assert_eq!(
                wire::nested(wire::nested(&reply, 40).unwrap().unwrap(), 1)
                    .unwrap()
                    .unwrap(),
                &hash
            );
            wire::write(&mut s, &wire::pairing(41, &wire::bytes(1, &hash))).unwrap();
            client
        });
        let pending = Pairing::start_at("127.0.0.1", port).unwrap();
        let code = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let blob = pending.finish(&code).unwrap();
        let id: Identity = crate::windows_lg::unprotect_document(&blob).unwrap();
        assert_eq!(id.cert, handle.join().unwrap());
        assert!(!String::from_utf8_lossy(&blob).contains(&code));
        let l = listener();
        let port = l.local_addr().unwrap().port();
        let cfg = config(&server);
        let expected = id.cert.clone();
        let handle = thread::spawn(move || {
            let mut s = accept(l, cfg);
            let info = [wire::bytes(1, b"TCL fixture"), wire::bytes(6, b"5.2.0")].concat();
            wire::write(
                &mut s,
                &wire::bytes(1, &[wire::integer(1, 35), wire::bytes(2, &info)].concat()),
            )
            .unwrap();
            let d = wire::read(&mut s).unwrap();
            assert_eq!(
                wire::int(wire::nested(&d, 1).unwrap().unwrap(), 1).unwrap(),
                Some(35)
            );
            assert_eq!(s.conn.peer_certificates().unwrap()[0].as_ref(), expected);
            wire::write(&mut s, &wire::bytes(2, &wire::integer(1, 0))).unwrap();
            assert_eq!(
                wire::int(
                    wire::nested(&wire::read(&mut s).unwrap(), 2)
                        .unwrap()
                        .unwrap(),
                    1
                )
                .unwrap(),
                Some(35)
            );
            wire::write(&mut s, &wire::bytes(8, &wire::integer(1, 19))).unwrap();
            assert_eq!(
                wire::int(
                    wire::nested(&wire::read(&mut s).unwrap(), 9)
                        .unwrap()
                        .unwrap(),
                    1
                )
                .unwrap(),
                Some(19)
            );
            wire::write(&mut s, &wire::bytes(40, &wire::integer(1, 1))).unwrap();
            assert_eq!(wire::read(&mut s).unwrap(), wire::hdmi(2).unwrap());
        });
        let mut r = Remote::connect_to("127.0.0.1", port, &id).unwrap();
        assert_eq!(r.power, PowerState::On);
        assert_eq!(r.model.as_deref(), Some("TCL fixture"));
        assert_eq!(r.version.as_deref(), Some("5.2.0"));
        r.send(&wire::hdmi(2).unwrap()).unwrap();
        handle.join().unwrap();
    }
    #[test]
    fn mismatched_server_identity_aborts_before_any_command() {
        let server = identity().unwrap();
        let mut id = identity().unwrap();
        id.peer = id.cert.clone();
        let l = listener();
        let port = l.local_addr().unwrap().port();
        let cfg = config(&server);
        let h = thread::spawn(move || {
            let mut s = accept(l, cfg);
            assert!(wire::read(&mut s).is_err());
        });
        assert!(open("127.0.0.1", port, &id, false).is_err());
        h.join().unwrap();
    }
    #[test]
    fn pin_proof_rejects_wrong_code_and_metadata_is_not_a_pairing_secret() {
        let a = identity().unwrap();
        let b = identity().unwrap();
        let mut h = Sha256::new();
        let (n, e) = public(&a.cert).unwrap();
        h.update(n);
        h.update(e);
        let (n, e) = public(&b.cert).unwrap();
        h.update(n);
        h.update(e);
        h.update([0x12, 0x34]);
        let digest: [u8; 32] = h.finalize().into();
        assert!(pair_secret(&a.cert, &b.cert, [digest[0] ^ 1, 0x12, 0x34]).is_err());
        assert_eq!(
            pair_secret(&a.cert, &b.cert, [digest[0], 0x12, 0x34]).unwrap(),
            digest
        );
    }
    #[test]
    fn a_partial_frame_cannot_keep_a_worker_alive_past_its_deadline() {
        use std::io::Write;
        let l = listener();
        let port = l.local_addr().unwrap().port();
        let h = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.write_all(&[3, 1]).unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });
        let tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let start = Instant::now();
        let mut b = BoundedTcp {
            tcp,
            deadline: start + Duration::from_millis(50),
        };
        assert!(wire::read(&mut b).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        h.join().unwrap();
    }
}
