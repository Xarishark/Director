//! Direct local webOS pairing through Windows WinHTTP. No external TV tools.
use crate::{
    tv::lg_protocol::{self, Input, Registration},
    tv::tvs::{Connection, Tv},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::c_void,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Networking::WinHttp::*,
        Security::Cryptography::*,
    },
    core::{PCWSTR, w},
};

struct Handle(*mut c_void, Arc<AtomicBool>);
impl Handle {
    fn new(raw: *mut c_void) -> Result<Self, String> {
        if raw.is_null() {
            Err("Windows could not open the TV connection. Check that the TV is on and its IP address is correct.".into())
        } else {
            Ok(Self(raw, Arc::new(AtomicBool::new(false))))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            if !self.1.swap(true, Ordering::AcqRel) {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}
fn api(result: windows::core::Result<()>) -> Result<(), String> {
    result.map_err(|e| format!("Windows could not communicate with the TV ({}). Check the address, network connection, and the TV’s mobile-app connection setting.", e.code()))
}
fn option(handle: &Handle, name: u32, value: u32) -> Result<(), String> {
    api(unsafe { WinHttpSetOption(Some(handle.0), name, Some(&value.to_ne_bytes())) })
}
// WinHttpCloseHandle must not race synchronous requests. The watchdog instead
// cancels the WebSocket receive using the documented WinHttpWebSocketClose API.
// Join it before releasing any handles; bound its close handshake as well.
struct Deadline {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
    expired: Arc<AtomicBool>,
}
impl Deadline {
    fn new(socket: &Handle, duration: Duration) -> Self {
        Self::with_cancel(socket, duration, Arc::new(AtomicBool::new(false)))
    }
    fn with_cancel(socket: &Handle, duration: Duration, cancel: Arc<AtomicBool>) -> Self {
        let (stop, receiver) = mpsc::channel();
        let expired = Arc::new(AtomicBool::new(false));
        let flag = expired.clone();
        let raw = socket.0 as usize;
        let worker = thread::spawn(move || {
            let until = std::time::Instant::now() + duration;
            loop {
                if cancel.load(Ordering::Acquire) || std::time::Instant::now() >= until {
                    flag.store(true, Ordering::Release);
                    unsafe {
                        WinHttpWebSocketClose(raw as *const c_void, 1001, None, 0);
                    }
                    break;
                }
                let remaining = until.saturating_duration_since(std::time::Instant::now());
                match receiver.recv_timeout(remaining.min(Duration::from_millis(50))) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        Self {
            stop: Some(stop),
            worker: Some(worker),
            expired,
        }
    }
    fn check(&self) -> Result<(), String> {
        if self.expired.load(Ordering::Acquire) {
            Err("The TV did not respond in time. If pairing, accept the prompt on the TV and try again.".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub(crate) struct Socket {
    // Field drop order is significant: stop/join watchdog, then socket, then parents.
    deadline: Deadline,
    socket: Handle,
    _connection: Handle,
    _session: Handle,
    pub(crate) certificate: Option<Vec<u8>>,
}
fn certificate(request: &Handle) -> Result<Vec<u8>, String> {
    certificate_raw(request.0)
}
fn certificate_raw(raw: *mut c_void) -> Result<Vec<u8>, String> {
    let mut context: *const CERT_CONTEXT = ptr::null();
    let mut length = std::mem::size_of_val(&context) as u32;
    api(unsafe {
        WinHttpQueryOption(
            raw,
            WINHTTP_OPTION_SERVER_CERT_CONTEXT,
            Some((&mut context as *mut *const CERT_CONTEXT).cast()),
            &mut length,
        )
    })?;
    if context.is_null() {
        return Err("The TV did not provide a certificate.".into());
    }
    unsafe {
        let certificate = &*context;
        let result = if certificate.cbCertEncoded == 0
            || certificate.cbCertEncoded > 32768
            || certificate.pbCertEncoded.is_null()
        {
            Err("The TV provided an invalid certificate.".into())
        } else {
            Ok(std::slice::from_raw_parts(
                certificate.pbCertEncoded,
                certificate.cbCertEncoded as usize,
            )
            .to_vec())
        };
        let _ = CertFreeCertificateContext(Some(context));
        result
    }
}
impl Socket {
    fn connect(
        tv: &Tv,
        expected_certificate: Option<&[u8]>,
        pairing: bool,
    ) -> Result<Self, String> {
        tv.validate()?;
        let secure = tv.connection == Connection::Encrypted;
        crate::diagnostics::measure("tv.connection", &tv.id, || {
            Self::open(
                &tv.address,
                secure,
                if secure { 3001 } else { 3000 },
                expected_certificate,
                pairing,
                Duration::from_secs(if pairing { 45 } else { 8 }),
            )
        })
    }
    fn open(
        address: &str,
        secure: bool,
        port: u16,
        expected_certificate: Option<&[u8]>,
        pairing: bool,
        duration: Duration,
    ) -> Result<Self, String> {
        Self::open_path(
            address,
            secure,
            port,
            "/",
            expected_certificate,
            pairing,
            duration,
        )
    }
    pub(crate) fn open_path(
        address: &str,
        secure: bool,
        port: u16,
        path: &str,
        expected_certificate: Option<&[u8]>,
        pairing: bool,
        duration: Duration,
    ) -> Result<Self, String> {
        if secure && !pairing && expected_certificate.is_none() {
            return Err("Pair this TV before checking its connection.".into());
        }
        let session = Handle::new(unsafe {
            WinHttpOpen(
                w!("Director/0.1"),
                WINHTTP_ACCESS_TYPE_NO_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        })?;
        api(unsafe { WinHttpSetTimeouts(session.0, 2500, 2500, 2500, 2500) })?;
        let host = address.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let connection =
            Handle::new(unsafe { WinHttpConnect(session.0, PCWSTR(host.as_ptr()), port, 0) })?;
        let request_path = path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let request = Handle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                w!("GET"),
                PCWSTR(request_path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                ptr::null(),
                if secure {
                    WINHTTP_FLAG_SECURE
                } else {
                    WINHTTP_OPEN_REQUEST_FLAGS(0)
                },
            )
        })?;
        option(
            &request,
            WINHTTP_OPTION_REDIRECT_POLICY,
            WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
        )?;
        api(unsafe {
            WinHttpSetOption(Some(request.0), WINHTTP_OPTION_UPGRADE_TO_WEB_SOCKET, None)
        })?;
        if secure {
            // LG presents a local certificate, not one for the numeric address.
            // Permit the TLS handshake only; verify the exact remembered DER
            // before sending a client key. First trust requires on-TV approval.
            option(
                &request,
                WINHTTP_OPTION_SECURITY_FLAGS,
                SECURITY_FLAG_IGNORE_UNKNOWN_CA
                    | SECURITY_FLAG_IGNORE_CERT_CN_INVALID
                    | SECURITY_FLAG_IGNORE_CERT_DATE_INVALID,
            )?;
        }
        // Samsung authenticates in the upgrade URL. Check the TLS pin before headers.
        let pin = if secure && path != "/" {
            expected_certificate.map(|expected| {
                Box::new(PinCheck {
                    expected: expected.to_vec(),
                    closed: request.1.clone(),
                    denied: AtomicBool::new(false),
                    checked: AtomicBool::new(false),
                })
            })
        } else {
            None
        };
        if pin.is_some() {
            unsafe {
                let prior = WinHttpSetStatusCallback(
                    request.0,
                    Some(pin_callback),
                    WINHTTP_CALLBACK_STATUS_SENDING_REQUEST,
                    0,
                );
                if prior.is_some_and(|callback| callback as usize == usize::MAX) {
                    return Err("Could not protect the Samsung authorization request.".into());
                }
            }
        }
        let context = pin
            .as_ref()
            .map_or(0, |p| (&**p as *const PinCheck) as usize);
        let sent = unsafe { WinHttpSendRequest(request.0, None, None, 0, 0, context) };
        if let Some(pin) = &pin {
            if !request.1.load(Ordering::Acquire) {
                unsafe {
                    WinHttpSetStatusCallback(request.0, None, 0, 0);
                }
            }
            if pin.denied.load(Ordering::Acquire) || !pin.checked.load(Ordering::Acquire) {
                return Err("The TV certificate could not be verified. No saved authorization was sent. Pair again after checking its address.".into());
            }
        }
        api(sent)?;
        api(unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) })?;
        let certificate = if secure {
            let actual = certificate(&request)?;
            if let Some(expected) = expected_certificate
                && actual != expected
            {
                return Err("The TV certificate has changed. No pairing key was sent. Check the TV’s address, then choose Pair to approve it again.".into());
            }
            Some(actual)
        } else {
            None
        };
        let mut status = 0u32;
        let mut length = 4;
        api(unsafe {
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some((&mut status as *mut u32).cast()),
                &mut length,
                ptr::null_mut(),
            )
        })?;
        if status != 101 {
            return Err("This address did not offer a TV control connection. Check the TV’s IP address and connection mode.".into());
        }
        option(
            &request,
            WINHTTP_OPTION_RECEIVE_TIMEOUT,
            duration.as_millis() as u32,
        )?;
        let socket = Handle::new(unsafe { WinHttpWebSocketCompleteUpgrade(request.0, None) })?;
        option(&socket, WINHTTP_OPTION_WEB_SOCKET_CLOSE_TIMEOUT, 500)?;
        let deadline = Deadline::new(&socket, duration);
        Ok(Self {
            deadline,
            socket,
            _connection: connection,
            _session: session,
            certificate,
        })
    }
    pub(crate) fn send(&self, value: &Value) -> Result<(), String> {
        self.deadline.check()?;
        let bytes = serde_json::to_vec(value).map_err(|_| "Could not prepare a TV request.")?;
        let result = unsafe {
            WinHttpWebSocketSend(
                self.socket.0,
                WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE,
                Some(&bytes),
            )
        };
        self.deadline.check()?;
        if result != 0 {
            return Err(format!(
                "The TV connection could not send the request (Windows error {result})."
            ));
        }
        Ok(())
    }
    pub(crate) fn receive(&self) -> Result<Value, String> {
        let mut message = vec![];
        loop {
            self.deadline.check()?;
            let mut buffer = [0u8; 4096];
            let mut read = 0;
            let mut kind = WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE;
            let result = unsafe {
                WinHttpWebSocketReceive(
                    self.socket.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut read,
                    &mut kind,
                )
            };
            self.deadline.check()?;
            if result != 0 {
                return Err(format!(
                    "The TV connection ended or timed out (Windows error {result})."
                ));
            }
            if kind != WINHTTP_WEB_SOCKET_UTF8_FRAGMENT_BUFFER_TYPE
                && kind != WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE
            {
                return Err("The TV closed the connection or sent an unexpected message.".into());
            }
            if read as usize > buffer.len() || message.len() + read as usize > 262144 {
                return Err("The TV returned an oversized message.".into());
            }
            message.extend_from_slice(&buffer[..read as usize]);
            if kind == WINHTTP_WEB_SOCKET_UTF8_MESSAGE_BUFFER_TYPE {
                return serde_json::from_slice(&message)
                    .map_err(|_| "The TV returned an unreadable response.".into());
            }
        }
    }
    fn request(&self, id: &str, uri: &str) -> Result<Value, String> {
        self.request_payload(id, uri, json!({}))
    }
    fn request_payload(&self, id: &str, uri: &str, payload: Value) -> Result<Value, String> {
        crate::diagnostics::measure("tv.request", uri, || {
            self.send(&json!({"id":id,"type":"request","uri":uri,"payload":payload}))?;
            crate::diagnostics::event("tv.request.sent", "ok", uri);
            for _ in 0..64 {
                let message = self.receive()?;
                if message["id"] == id {
                    return Ok(message);
                }
            }
            Err("The TV did not return the requested information.".into())
        })
    }

    fn register(&self, key: Option<&str>, pairing: bool) -> Result<String, String> {
        crate::diagnostics::measure(
            "tv.registration",
            if pairing { "pair" } else { "saved_pairing" },
            || {
                self.send(&lg_protocol::registration(key))?;
                for _ in 0..64 {
                    match lg_protocol::registered(&self.receive()?, pairing)? {
                        Registration::Registered(key) => return Ok(key),
                        Registration::Prompt | Registration::Ignore => {}
                    }
                }
                Err("The TV did not complete pairing.".into())
            },
        )
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Secret {
    pub(crate) key: String,
    pub(crate) certificate: Option<Vec<u8>>,
}
pub(crate) fn protect(secret: &Secret) -> Result<Vec<u8>, String> {
    protect_document(secret)
}
pub(crate) fn protect_document<T: serde::Serialize>(secret: &T) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec(secret).map_err(|_| "Could not protect the pairing data.")?;
    if bytes.len() > 60000 {
        bytes.fill(0);
        return Err("Pairing data exceeds its size limit.".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let result = unsafe {
        CryptProtectData(
            &input,
            w!("Director TV pairing"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    bytes.fill(0);
    result.map_err(|_| "Windows could not protect the TV pairing data.")?;
    let encrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(Some(HLOCAL(output.pbData.cast())));
    }
    Ok(encrypted)
}
pub(crate) fn unprotect_document<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, String> {
    if bytes.is_empty() || bytes.len() > 65536 {
        return Err("Invalid protected pairing data.".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output) }.map_err(|_| "Windows could not unlock this TV’s pairing data. Pair again using this Windows account.")?;
    let plain = unsafe { std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize) };
    let result: Result<T, _> = serde_json::from_slice(plain);
    plain.fill(0);
    unsafe {
        LocalFree(Some(HLOCAL(output.pbData.cast())));
    }
    result.map_err(|_| "The TV pairing data is invalid. Pair again.".into())
}
pub(crate) fn unprotect(bytes: &[u8]) -> Result<Secret, String> {
    let secret: Secret = unprotect_document(bytes)?;
    if secret.key.is_empty() || secret.key.len() > 4096 {
        return Err("The TV pairing data is invalid. Pair again.".into());
    }
    Ok(secret)
}
/// Explicit pairing only. Never silently prompt during a connection check.
pub fn pair(tv: &Tv) -> Result<Vec<u8>, String> {
    let socket = Socket::connect(tv, None, true)?;
    let key = socket.register(None, true)?;
    protect(&Secret {
        key,
        certificate: socket.certificate.clone(),
    })
}
pub fn check(tv: &Tv) -> Result<Vec<Input>, String> {
    let secret = unprotect(tv.credential.as_ref().ok_or("Pair this TV first.")?)?;
    let socket = Socket::connect(tv, secret.certificate.as_deref(), false)?;
    let _ = socket.register(Some(&secret.key), false)?;
    socket.send(&json!({"id":"director-inputs", "type":"request", "uri":"ssap://tv/getExternalInputList", "payload":{}}))?;
    for _ in 0..64 {
        let message = socket.receive()?;
        if message["id"] == "director-inputs" {
            return lg_protocol::inputs(&message);
        }
    }
    Err("The TV did not return its inputs.".into())
}
#[derive(Clone)]
pub struct TvDetails {
    pub inputs: Vec<Input>,
    pub network: crate::tv::tv_network::NetworkAddresses,
    pub network_warning: Option<String>,
}
/// Network information is optional: firmware permissions must not block setup.
pub fn inspect(tv: &Tv) -> Result<TvDetails, String> {
    let secret = unprotect(tv.credential.as_ref().ok_or("Pair this TV first.")?)?;
    let socket = Socket::connect(tv, secret.certificate.as_deref(), false)?;
    socket.register(Some(&secret.key), false)?;
    let inputs =
        lg_protocol::inputs(&socket.request("director-inputs", "ssap://tv/getExternalInputList")?)?;
    let network = socket
        .request(
            "director-network",
            "ssap://com.webos.service.connectionmanager/getinfo",
        )
        .and_then(|m| lg_protocol::network(&m));
    let (network, network_warning) = match network {
        Ok(n) => (n, None),
        Err(e) => (Default::default(), Some(e)),
    };
    Ok(TvDetails {
        inputs,
        network,
        network_warning,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_secret_is_protected_and_tampering_fails() {
        let secret = Secret {
            key: "director-test-pairing-key".into(),
            certificate: Some(vec![1, 2, 3]),
        };
        let mut blob = protect(&secret).unwrap();
        assert!(
            !blob
                .windows(secret.key.len())
                .any(|b| b == secret.key.as_bytes())
        );
        let read = unprotect(&blob).unwrap();
        assert_eq!(read.key, secret.key);
        assert_eq!(read.certificate, secret.certificate);
        blob.fill(0);
        assert!(unprotect(&blob).is_err());
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        time::Instant,
    };

    #[test]
    fn subscription_receives_remote_power_changes_and_cancels_an_idle_receive() {
        let (port, server) = server(|mut stream| {
            let request = client_json(&mut stream);
            assert_eq!(request["type"], "subscribe");
            assert_eq!(
                request["uri"],
                "ssap://com.webos.service.tvpower/power/getPowerState"
            );
            control_reply(
                &mut stream,
                &request["id"],
                json!({"returnValue":true,"state":"Active"}),
            );
            control_reply(
                &mut stream,
                &request["id"],
                json!({"returnValue":true,"state":"Suspend"}),
            );
            let mut close = [0; 2];
            stream.read_exact(&mut close).unwrap();
            assert_eq!(close[0] & 15, 8);
        });
        let mut socket =
            Socket::open("127.0.0.1", false, port, None, true, Duration::from_secs(3)).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        socket.deadline =
            Deadline::with_cancel(&socket.socket, Duration::from_secs(30), cancel.clone());
        let states = std::sync::Mutex::new(Vec::new());
        let started = Instant::now();
        let result = listen_power(&socket, &cancel, &|event| {
            if let PowerEvent::Reported(power) = event {
                let mut states = states.lock().unwrap();
                states.push(power);
                if states.len() == 2 {
                    cancel.store(true, Ordering::Release);
                }
            }
        });
        assert!(result.is_ok());
        // A second receive with no incoming data must be interrupted promptly.
        assert!(socket.receive().is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(
            *states.lock().unwrap(),
            vec![
                crate::tv::tv_control::PowerState::On,
                crate::tv::tv_control::PowerState::Off
            ]
        );
        drop(socket);
        server.join().unwrap();
    }
    #[test]
    fn dropping_power_watch_cancels_without_waiting_for_a_network_reply() {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let watch = PowerWatch {
            cancel: cancel.clone(),
            receiver: Arc::new(std::sync::Mutex::new(receiver)),
        };
        drop(watch);
        drop(sender);
        assert!(cancel.load(Ordering::Acquire));
    }

    fn client_json(stream: &mut TcpStream) -> Value {
        let mut header = [0; 2];
        stream.read_exact(&mut header).unwrap();
        assert_eq!(header[0] & 15, 1);
        let mut length = (header[1] & 127) as usize;
        if length == 126 {
            let mut bytes = [0; 2];
            stream.read_exact(&mut bytes).unwrap();
            length = u16::from_be_bytes(bytes) as usize;
        }
        assert!(length < 16384);
        let mut mask = [0; 4];
        let masked = header[1] & 128 != 0;
        if masked {
            stream.read_exact(&mut mask).unwrap();
        }
        let mut data = vec![0; length];
        stream.read_exact(&mut data).unwrap();
        if masked {
            for (i, b) in data.iter_mut().enumerate() {
                *b ^= mask[i % 4];
            }
        }
        serde_json::from_slice(&data).unwrap()
    }
    fn control_reply(stream: &mut TcpStream, id: &Value, payload: Value) {
        let bytes =
            serde_json::to_vec(&json!({"id":id,"type":"response","payload":payload})).unwrap();
        stream.write_all(&[0x81]).unwrap();
        if bytes.len() < 126 {
            stream.write_all(&[bytes.len() as u8]).unwrap();
        } else {
            stream.write_all(&[126]).unwrap();
            stream
                .write_all(&(bytes.len() as u16).to_be_bytes())
                .unwrap();
        }
        stream.write_all(&bytes).unwrap();
    }
    fn control_tv() -> Tv {
        Tv {
            id: "local-control-test".into(),
            name: "Local mock TV".into(),
            address: "192.0.2.50".into(),
            brand: Default::default(),
            samsung: Default::default(),
            android: Default::default(),
            connection: Default::default(),
            display: None,
            audio: None,
            credential: None,
            inputs: vec![],
            pc_input: None,
            network: Default::default(),
        }
    }
    #[test]
    fn native_controls_send_explicit_off_only_after_fresh_on() {
        let (port, server) = server(|mut stream| {
            let request = client_json(&mut stream);
            assert_eq!(
                request["uri"],
                "ssap://com.webos.service.tvpower/power/getPowerState"
            );
            control_reply(
                &mut stream,
                &request["id"],
                json!({"returnValue":true,"state":"Active"}),
            );
            let request = client_json(&mut stream);
            assert_eq!(request["uri"], "ssap://system/turnOff");
            control_reply(&mut stream, &request["id"], json!({"returnValue":true}));
        });
        let socket =
            Socket::open("127.0.0.1", false, port, None, true, Duration::from_secs(3)).unwrap();
        let tv = control_tv();
        let mut backend = WindowsTvControls {
            tv: &tv,
            session: Some(Ok(socket)),
            devices: Value::Null,
        };
        let report =
            crate::tv::tv_control::power(&mut backend, crate::tv::tv_control::PowerAction::Off)
                .unwrap();
        assert!(report.message.contains("accepted"));
        drop(backend);
        server.join().unwrap();
    }
    #[test]
    fn native_controls_validate_input_payload_and_observe_selection() {
        let (port, server) = server(|mut stream| {
            let request = client_json(&mut stream);
            assert_eq!(request["uri"], "ssap://tv/getExternalInputList");
            control_reply(
                &mut stream,
                &request["id"],
                json!({"returnValue":true,"devices":[{"id":"HDMI_1","label":"PC","appId":"com.webos.app.hdmi1"},{"id":"HDMI_2","label":"Console","appId":"com.webos.app.hdmi2"}]}),
            );
            let request = client_json(&mut stream);
            assert_eq!(
                request["uri"],
                "ssap://com.webos.applicationManager/getForegroundAppInfo"
            );
            control_reply(
                &mut stream,
                &request["id"],
                json!({"returnValue":true,"appId":"com.webos.app.hdmi1"}),
            );
            let request = client_json(&mut stream);
            assert_eq!(request["uri"], "ssap://tv/switchInput");
            assert_eq!(request["payload"]["inputId"], "HDMI_2");
            control_reply(&mut stream, &request["id"], json!({"returnValue":true}));
            let request = client_json(&mut stream);
            assert_eq!(
                request["uri"],
                "ssap://com.webos.applicationManager/getForegroundAppInfo"
            );
            control_reply(
                &mut stream,
                &request["id"],
                json!({"returnValue":true,"appId":"com.webos.app.hdmi2"}),
            );
        });
        let socket =
            Socket::open("127.0.0.1", false, port, None, true, Duration::from_secs(3)).unwrap();
        let tv = control_tv();
        let mut backend = WindowsTvControls {
            tv: &tv,
            session: Some(Ok(socket)),
            devices: Value::Null,
        };
        let report = crate::tv::tv_control::switch_input(&mut backend, "HDMI_2").unwrap();
        assert!(report.message.starts_with("TV reports"));
        drop(backend);
        server.join().unwrap();
    }

    fn base64(bytes: &[u8]) -> String {
        const MAP: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::new();
        for chunk in bytes.chunks(3) {
            let n = ((chunk[0] as u32) << 16)
                | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
                | chunk.get(2).copied().unwrap_or(0) as u32;
            result.push(MAP[((n >> 18) & 63) as usize] as char);
            result.push(MAP[((n >> 12) & 63) as usize] as char);
            result.push(if chunk.len() > 1 {
                MAP[((n >> 6) & 63) as usize] as char
            } else {
                '='
            });
            result.push(if chunk.len() > 2 {
                MAP[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        result
    }
    fn handshake(stream: &mut TcpStream) {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        let mut header = vec![];
        while !header.ends_with(b"\r\n\r\n") {
            assert!(header.len() < 16384);
            let mut b = [0];
            stream.read_exact(&mut b).unwrap();
            header.push(b[0]);
        }
        let header = String::from_utf8(header).unwrap();
        let key = header
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("Sec-WebSocket-Key"))
                    .map(|(_, v)| v.trim())
            })
            .unwrap();
        let text = format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
        let mut hash = [0u8; 20];
        let mut len = 20;
        unsafe {
            CryptHashCertificate2(
                w!("SHA1"),
                0,
                None,
                Some(text.as_bytes()),
                Some(hash.as_mut_ptr()),
                &mut len,
            )
        }
        .unwrap();
        write!(stream,"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",base64(&hash)).unwrap();
    }
    fn server(job: impl FnOnce(TcpStream) + Send + 'static) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let worker = thread::spawn(move || {
            let start = Instant::now();
            loop {
                match listener.accept() {
                    Ok((mut s, _)) => {
                        handshake(&mut s);
                        job(s);
                        break;
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && start.elapsed() < Duration::from_secs(5) =>
                    {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(e) => panic!("Local server accept: {e}"),
                }
            }
        });
        (port, worker)
    }
    fn read_frame(stream: &mut TcpStream) {
        let mut header = [0; 2];
        stream.read_exact(&mut header).unwrap();
        let mut length = (header[1] & 127) as usize;
        if length == 126 {
            let mut n = [0; 2];
            stream.read_exact(&mut n).unwrap();
            length = u16::from_be_bytes(n) as usize;
        }
        assert!(length < 16384);
        let mut mask = [0; 4];
        if header[1] & 128 != 0 {
            stream.read_exact(&mut mask).unwrap();
        }
        let mut data = vec![0; length];
        stream.read_exact(&mut data).unwrap();
    }
    fn text(stream: &mut TcpStream, value: &Value) {
        let bytes = serde_json::to_vec(value).unwrap();
        assert!(bytes.len() < 126);
        stream.write_all(&[0x81, bytes.len() as u8]).unwrap();
        stream.write_all(&bytes).unwrap();
    }

    #[test]
    fn samsung_native_socket_sends_one_direct_frame_after_token_handshake() {
        let (port, server) = server(|mut stream| {
            text(
                &mut stream,
                &json!({"event":"ms.channel.connect","data":{"token":"test-token"}}),
            );
            let frame = client_json(&mut stream);
            assert_eq!(frame["method"], "ms.remote.control");
            assert_eq!(frame["params"]["DataOfCmd"], "KEY_HDMI2");
        });
        let socket = Socket::open_path(
            "127.0.0.1",
            false,
            port,
            &crate::tv::samsung::path(Some("test-token")),
            None,
            false,
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(
            crate::tv::samsung::handshake(&socket.receive().unwrap()).unwrap(),
            Some(Some("test-token".into()))
        );
        socket
            .send(&crate::tv::samsung::key_frame("KEY_HDMI2").unwrap())
            .unwrap();
        drop(socket);
        server.join().unwrap();
    }
    #[test]
    fn samsung_denial_is_not_treated_as_pairing_success() {
        let (port, server) = server(|mut stream| {
            text(&mut stream, &json!({"event":"ms.channel.unauthorized"}));
        });
        let socket = Socket::open_path(
            "127.0.0.1",
            false,
            port,
            &crate::tv::samsung::path(None),
            None,
            true,
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(crate::tv::samsung::handshake(&socket.receive().unwrap()).is_err());
        drop(socket);
        server.join().unwrap();
    }
    #[test]
    fn native_websocket_registration_handles_prompt_and_correlated_key() {
        let (port, server) = server(|mut stream| {
            text(
                &mut stream,
                &json!({"id":"director-register","payload":{"pairingType":"PROMPT"}}),
            );
            text(
                &mut stream,
                &json!({"id":"director-register","type":"registered","payload":{"client-key":"local-test"}}),
            );
            let mut b = [0; 2048];
            let _ = stream.read(&mut b);
        });
        let socket =
            Socket::open("127.0.0.1", false, port, None, true, Duration::from_secs(3)).unwrap();
        assert_eq!(socket.register(None, true).unwrap(), "local-test");
        drop(socket);
        server.join().unwrap();
    }
    #[test]
    fn silent_websocket_is_cancelled_without_hanging_worker() {
        let (port, server) = server(|mut stream| {
            read_frame(&mut stream);
            // Wait for the watchdog close frame, then let the connection end.
            read_frame(&mut stream);
        });
        let start = Instant::now();
        let socket = Socket::open(
            "127.0.0.1",
            false,
            port,
            None,
            true,
            Duration::from_millis(350),
        )
        .unwrap();
        let error = socket.register(None, true).err().unwrap();
        assert!(
            error.contains("in time") || error.contains("timed out"),
            "{error}"
        );
        drop(socket);
        server.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(4));
    }
}

impl TvDetails {
    pub fn remember(&self, tv: &mut Tv) {
        tv.inputs = self.inputs.clone();
        tv.network.merge_learned(&self.network);
    }
}
/// Re-pair a remembered TV and learn metadata without discarding a successful
/// pairing if firmware refuses a subsequent optional information request.
pub fn pair_with_details(tv: &Tv) -> Result<(Tv, Option<String>), String> {
    let mut paired = tv.clone();
    paired.credential = Some(pair(tv)?);
    let warning = match inspect(&paired) {
        Ok(details) => {
            details.remember(&mut paired);
            details.network_warning
        }
        Err(e) => Some(format!(
            "Pairing succeeded, but TV details could not be read: {e}"
        )),
    };
    Ok((paired, warning))
}

#[cfg(test)]
mod hardware_read_only {
    use super::*;
    #[test]
    #[ignore = "Requires exactly one paired TV, on and reachable; explicitly authorized read-only hardware check"]
    fn paired_tv_status_and_network() {
        let _lock = crate::audio::windows_audio::OperationLock::acquire().unwrap();
        let tvs = crate::tv::tvs::load(&crate::tv::tvs::path().unwrap()).unwrap();
        let paired = tvs
            .iter()
            .filter(|tv| tv.credential.is_some())
            .collect::<Vec<_>>();
        assert_eq!(
            paired.len(),
            1,
            "This read-only check needs exactly one paired TV; no device is guessed."
        );
        let tv = paired[0];
        let secret = unprotect(tv.credential.as_ref().unwrap()).unwrap();
        let socket = Socket::connect(tv, secret.certificate.as_deref(), false).unwrap();
        socket.register(Some(&secret.key), false).unwrap();
        let state = socket
            .request(
                "director-status-check",
                "ssap://com.webos.service.tvpower/power/getPowerState",
            )
            .unwrap();
        println!(
            "Power response type: {}; state: {}",
            state["type"].as_str().unwrap_or("unknown"),
            state["payload"]["state"].as_str().unwrap_or("unavailable")
        );
        let result = socket
            .request(
                "director-network-check",
                "ssap://com.webos.service.connectionmanager/getinfo",
            )
            .and_then(|m| crate::tv::lg_protocol::network(&m));
        match result {
            Ok(n) => println!(
                "Valid TV-reported wake addresses: {}; Ethernet: {}; Wi-Fi: {}",
                n.targets().len(),
                n.wired.is_some(),
                n.wifi.is_some()
            ),
            Err(e) => println!("Network information unavailable: {e}"),
        }
    }
}

fn authenticated(tv: &Tv) -> Result<Socket, String> {
    let secret = unprotect(
        tv.credential
            .as_ref()
            .ok_or("Pair this TV to read status and control its inputs.")?,
    )?;
    let socket = Socket::connect(tv, secret.certificate.as_deref(), false)?;
    socket.register(Some(&secret.key), false)?;
    Ok(socket)
}
/// Created and used on one background thread; native handles never reach the UI.
pub struct WindowsTvControls<'a> {
    tv: &'a Tv,
    session: Option<Result<Socket, String>>,
    devices: Value,
}
impl<'a> WindowsTvControls<'a> {
    pub fn new(tv: &'a Tv) -> Self {
        Self {
            tv,
            session: None,
            devices: Value::Null,
        }
    }
    fn session(&mut self) -> Result<&Socket, String> {
        if self.session.is_none() {
            self.session = Some(crate::diagnostics::measure(
                "tv.authentication",
                &self.tv.id,
                || authenticated(self.tv),
            ));
        }
        match self.session.as_ref() {
            Some(Ok(s)) => Ok(s),
            Some(Err(e)) => Err(e.clone()),
            None => Err("The TV connection is unavailable.".into()),
        }
    }
}
impl crate::tv::tv_control::Backend for WindowsTvControls<'_> {
    fn power(&mut self) -> Result<crate::tv::tv_control::PowerState, String> {
        let message = self.session()?.request(
            "director-control-power",
            "ssap://com.webos.service.tvpower/power/getPowerState",
        )?;
        let result = lg_protocol::power_state(&message);
        match &result {
            Ok(power) => crate::diagnostics::event("tv.power.reported", "ok", power.label()),
            Err(e) => crate::diagnostics::event("tv.power.reported", "error", e),
        }
        result
    }
    fn inputs(&mut self) -> Result<Vec<Input>, String> {
        let message = self
            .session()?
            .request("director-control-inputs", "ssap://tv/getExternalInputList")?;
        let inputs = lg_protocol::inputs(&message)?;
        self.devices = message;
        Ok(inputs)
    }
    fn current_input(&mut self) -> Result<Option<String>, String> {
        let message = self.session()?.request(
            "director-control-current",
            "ssap://com.webos.applicationManager/getForegroundAppInfo",
        )?;
        if message["type"] == "error" || message["payload"]["returnValue"] == false {
            return Err("The TV did not report its current input.".into());
        }
        Ok(lg_protocol::mapped_input(&self.devices, &message))
    }
    fn wake(&mut self) -> Result<crate::tv::tv_network::WakeReport, String> {
        crate::tv::tv_network::wake(&self.tv.network)
    }
    fn off(&mut self) -> Result<(), String> {
        let message = self
            .session()?
            .request("director-control-off", "ssap://system/turnOff")?;
        let result = lg_protocol::command_accepted(&message);
        crate::diagnostics::result("tv.command.acknowledgment", &result);
        result
    }
    fn switch_input(&mut self, id: &str) -> Result<(), String> {
        let message = self.session()?.request_payload(
            "director-control-switch",
            "ssap://tv/switchInput",
            json!({"inputId":id}),
        )?;
        let result = lg_protocol::command_accepted(&message);
        crate::diagnostics::result("tv.command.acknowledgment", &result);
        result
    }
}

/// Scoped to the controls view. Drop signals cancellation without blocking WinUI.
#[derive(Clone, Debug)]
pub enum PowerEvent {
    Reported(crate::tv::tv_control::PowerState),
    Unavailable(String),
}
#[derive(Clone, Debug)]
pub struct PowerUpdate {
    pub at: std::time::Instant,
    pub event: PowerEvent,
}
pub struct PowerWatch {
    pub cancel: Arc<AtomicBool>,
    pub receiver: Arc<std::sync::Mutex<mpsc::Receiver<PowerUpdate>>>,
}
impl Drop for PowerWatch {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
fn watch_session(
    tv: &Tv,
    cancel: &Arc<AtomicBool>,
    emit: &impl Fn(PowerEvent),
) -> Result<(), String> {
    let secret = unprotect(
        tv.credential
            .as_ref()
            .ok_or("Pair this TV to receive live status.")?,
    )?;
    let mut socket = Socket::connect(tv, secret.certificate.as_deref(), false)?;
    if cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    socket.deadline = Deadline::with_cancel(&socket.socket, Duration::from_secs(8), cancel.clone());
    socket.register(Some(&secret.key), false)?;
    if cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    socket.deadline =
        Deadline::with_cancel(&socket.socket, Duration::from_secs(86400), cancel.clone());
    listen_power(&socket, cancel, emit)
}
fn listen_power(
    socket: &Socket,
    cancel: &AtomicBool,
    emit: &impl Fn(PowerEvent),
) -> Result<(), String> {
    socket.send(&json!({"id":"director-live-power", "type":"subscribe",
        "uri":"ssap://com.webos.service.tvpower/power/getPowerState", "payload":{}}))?;
    crate::diagnostics::event("tv.power.subscription", "started", "");
    while !cancel.load(Ordering::Acquire) {
        let message = socket.receive()?;
        if cancel.load(Ordering::Acquire) {
            break;
        }
        if message["id"] != "director-live-power" {
            continue;
        }
        let power = lg_protocol::power_state(&message)?;
        // An Active packet processing an off request is not a fresh awake confirmation.
        if message["payload"]["processing"].as_str() == Some("Request Power Off") {
            continue;
        }
        crate::diagnostics::event("tv.power.subscription", "reported", power.label());
        emit(PowerEvent::Reported(power));
    }
    Ok(())
}
/// Read-only subscription; commands and their shared operation guard remain separate.
pub fn watch_power(tv: Tv) -> PowerWatch {
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let emit = |event| {
            let _ = sender.send(PowerUpdate {
                at: std::time::Instant::now(),
                event,
            });
        };
        let mut backoff = 2u64;
        while !worker_cancel.load(Ordering::Acquire) {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let saved = crate::tv::tvs::load(&crate::tv::tvs::path()?)?;
                if saved.iter().find(|t| t.id == tv.id) != Some(&tv) {
                    return Err(
                        "The saved TV changed. Close these controls and reopen them.".to_string(),
                    );
                }
                watch_session(&tv, &worker_cancel, &emit)
            }))
            .unwrap_or_else(|_| Err("Live TV status stopped unexpectedly.".into()));
            if worker_cancel.load(Ordering::Acquire) {
                break;
            }
            if let Err(error) = result {
                crate::diagnostics::event("tv.power.subscription", "unavailable", &error);
                emit(PowerEvent::Unavailable(error));
            }
            // No WOL here: reconnecting never powers a TV on by itself.
            for _ in 0..backoff * 20 {
                if worker_cancel.load(Ordering::Acquire) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            backoff = (backoff * 2).min(15);
        }
        crate::diagnostics::event("tv.power.subscription", "stopped", "");
    });
    PowerWatch {
        cancel,
        receiver: Arc::new(std::sync::Mutex::new(receiver)),
    }
}

struct PinCheck {
    expected: Vec<u8>,
    closed: Arc<AtomicBool>,
    denied: AtomicBool,
    checked: AtomicBool,
}
unsafe extern "system" fn pin_callback(
    handle: *mut c_void,
    context: usize,
    status: u32,
    _info: *mut c_void,
    _length: u32,
) {
    if status != WINHTTP_CALLBACK_STATUS_SENDING_REQUEST || context == 0 {
        return;
    }
    let pin = unsafe { &*(context as *const PinCheck) };
    let matches = std::panic::catch_unwind(|| {
        certificate_raw(handle).is_ok_and(|actual| actual == pin.expected)
    })
    .unwrap_or(false);
    pin.checked.store(true, Ordering::Release);
    if !matches {
        pin.denied.store(true, Ordering::Release);
        if !pin.closed.swap(true, Ordering::AcqRel) {
            unsafe {
                let _ = WinHttpCloseHandle(handle);
            }
        }
    }
}
pub(crate) fn http_json(address: &str, port: u16, path: &str) -> Result<Value, String> {
    let session = Handle::new(unsafe {
        WinHttpOpen(
            w!("Director/0.1"),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })?;
    api(unsafe { WinHttpSetTimeouts(session.0, 2000, 2000, 2000, 2000) })?;
    let host = address.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let path = path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let connection =
        Handle::new(unsafe { WinHttpConnect(session.0, PCWSTR(host.as_ptr()), port, 0) })?;
    let request = Handle::new(unsafe {
        WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            ptr::null(),
            WINHTTP_OPEN_REQUEST_FLAGS(0),
        )
    })?;
    option(
        &request,
        WINHTTP_OPTION_REDIRECT_POLICY,
        WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    )?;
    api(unsafe { WinHttpSendRequest(request.0, None, None, 0, 0, 0) })?;
    api(unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) })?;
    let mut status = 0u32;
    let mut length = 4;
    api(unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut length,
            ptr::null_mut(),
        )
    })?;
    if status != 200 {
        return Err("The TV information endpoint did not respond successfully.".into());
    }
    let mut data = vec![];
    let read_deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        if std::time::Instant::now() >= read_deadline {
            return Err("The TV information response timed out.".into());
        }
        let mut block = [0u8; 4096];
        let mut read = 0u32;
        api(unsafe {
            WinHttpReadData(
                request.0,
                block.as_mut_ptr().cast(),
                block.len() as u32,
                &mut read,
            )
        })?;
        if read == 0 {
            break;
        }
        if read as usize > block.len() || data.len() + read as usize > 65536 {
            return Err("The TV information response was too large.".into());
        }
        data.extend_from_slice(&block[..read as usize]);
    }
    serde_json::from_slice(&data).map_err(|_| "The TV information response was unreadable.".into())
}
