//! Native NVIDIA DRS and RTSS SDK providers. No helper executables.
use crate::fps::{self, Backend, Capability, Provider, Request, Snapshot};
use std::{
    ffi::{CStr, c_void},
    path::{Path, PathBuf},
    ptr,
    sync::{Mutex, MutexGuard},
};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
    fn GetSystemDirectoryW(buf: *mut u16, size: u32) -> u32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        key: *mut c_void,
        sub: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn FindWindowW(class: *const u16, name: *const u16) -> *mut c_void;
    fn PostMessageW(window: *mut c_void, message: u32, w: usize, l: isize) -> i32;
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Dll(*mut c_void);
impl Drop for Dll {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}
impl Dll {
    fn load(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("An absolute installed provider DLL is required.".into());
        }
        let raw = unsafe {
            LoadLibraryExW(
                wide(&path.to_string_lossy()).as_ptr(),
                ptr::null_mut(),
                0x900,
            )
        };
        if raw.is_null() {
            Err("The installed provider DLL could not be loaded.".into())
        } else {
            Ok(Self(raw))
        }
    }
    unsafe fn function<T: Copy>(&self, name: &CStr) -> Result<T, String> {
        let p = unsafe { GetProcAddress(self.0, name.as_ptr().cast()) };
        if p.is_null() {
            return Err(format!(
                "Provider interface {} is unavailable.",
                name.to_string_lossy()
            ));
        }
        assert_eq!(std::mem::size_of::<T>(), std::mem::size_of_val(&p));
        Ok(unsafe { std::mem::transmute_copy(&p) })
    }
}
static PROVIDERS: Mutex<()> = Mutex::new(());
// Serializes SDK's process-global loaded profile and NVAPI initialize/unload lifetime.
pub struct WindowsFps {
    inner: Native,
    _guard: MutexGuard<'static, ()>,
}
enum Native {
    Nvidia(Box<Nvidia>),
    Rtss(Rtss),
}
impl WindowsFps {
    pub fn open(provider: Provider) -> Result<Self, String> {
        let guard = PROVIDERS
            .lock()
            .map_err(|_| "FPS provider worker is unavailable.")?;
        let inner = match provider {
            Provider::Nvidia => Native::Nvidia(Box::new(Nvidia::open()?)),
            Provider::Rtss => Native::Rtss(Rtss::open()?),
        };
        Ok(Self {
            inner,
            _guard: guard,
        })
    }
}
impl Backend for WindowsFps {
    fn snapshot(&mut self) -> Result<Snapshot, String> {
        match &mut self.inner {
            Native::Nvidia(p) => p.read(),
            Native::Rtss(p) => p.read(),
        }
    }
    fn set(&mut self, r: &Request) -> Result<(), String> {
        r.validate()?;
        match (&mut self.inner, r.provider) {
            (Native::Nvidia(p), Provider::Nvidia) => p.write(r.limit.unwrap_or(0), false),
            (Native::Rtss(p), Provider::Rtss) => {
                p.write(r.limit.unwrap_or(0), r.limit.map(|_| false))
            }
            _ => Err("FPS provider mismatch.".into()),
        }
    }
    fn restore(&mut self, s: &Snapshot) -> Result<(), String> {
        match (&mut self.inner, s.provider) {
            (Native::Nvidia(p), Provider::Nvidia) => p.write(s.value, s.inherited),
            (Native::Rtss(p), Provider::Rtss) => p.write(s.value, s.limiter_disabled),
            _ => Err("FPS snapshot provider mismatch.".into()),
        }
    }
}
pub fn survey() -> Vec<Capability> {
    [Provider::Nvidia,Provider::Rtss].into_iter().map(|provider|match WindowsFps::open(provider).and_then(|mut p|p.snapshot()){Ok(current)=>Capability{provider,available:true,maximum:if provider==Provider::Nvidia{1023}else{1000},current:Some(current),detail:if provider==Provider::Nvidia{"NVIDIA global/base driver profile. Game-specific settings can override it."}else{"RTSS global profile. RTSS must remain running; game profiles can override it."}.into()},Err(e)=>Capability{provider,available:false,maximum:0,current:None,detail:e}}).collect()
}
pub fn apply(request: &Request, keep: bool) -> Result<String, String> {
    let _operation = crate::diagnostics::Operation::start("fps.apply", None);
    crate::diagnostics::measure("fps.provider.apply", request.provider.label(), || {
        fps::apply(&mut WindowsFps::open(request.provider)?, request, keep)
    })
}
pub fn cli(args: &[String]) -> Result<i32, String> {
    let _lock = crate::windows_audio::OperationLock::acquire()?;
    match args {
        [command] if command == "status" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&survey())
                    .map_err(|_| "Could not format FPS status.")?
            );
            Ok(0)
        }
        [command, provider, limit] if command == "set" => {
            let vrr_safe = limit == "vrr";
            let request = Request {
                provider: Provider::parse(provider)?,
                limit: if limit == "off" || vrr_safe {
                    None
                } else {
                    Some(
                        limit
                            .parse()
                            .map_err(|_| "Use an integer FPS value, vrr or off.")?,
                    )
                },
                vrr_safe,
            };
            let refresh = if vrr_safe {
                crate::windows_displays::capture()?.primary_refresh_hz()
            } else {
                None
            };
            println!("{}", apply(&request.resolved(refresh)?, false)?);
            Ok(0)
        }
        _ => Err("Use: director fps status | director fps set <nvidia|rtss> <FPS|vrr|off>".into()),
    }
}
type Handle = *mut c_void;
type Status = i32;
#[repr(C)]
struct Setting {
    version: u32,
    name: [u16; 2048],
    id: u32,
    kind: u32,
    location: u32,
    predefined: u32,
    valid: u32,
    default_value: [u32; 1025],
    current: [u32; 1025],
}
impl Setting {
    fn new() -> Self {
        let mut s: Self = unsafe { std::mem::zeroed() };
        s.version = std::mem::size_of::<Self>() as u32 | 1 << 16;
        s.id = 0x10835002;
        s
    }
}
type SessionFn = unsafe extern "C" fn(Handle) -> Status;
struct Nvidia {
    _dll: Dll,
    query: unsafe extern "C" fn(u32) -> *mut c_void,
    session: Handle,
    profile: Handle,
    destroy: SessionFn,
    unload: unsafe extern "C" fn() -> Status,
}
impl Drop for Nvidia {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.session);
            (self.unload)();
        }
    }
}
impl Nvidia {
    unsafe fn function<T: Copy>(&self, id: u32) -> Result<T, String> {
        let p = unsafe { (self.query)(id) };
        if p.is_null() {
            return Err("This NVIDIA driver is missing the required NVAPI interface.".into());
        }
        assert_eq!(std::mem::size_of::<T>(), std::mem::size_of_val(&p));
        Ok(unsafe { std::mem::transmute_copy(&p) })
    }
    fn status(&self, s: Status, operation: &str) -> Result<(), String> {
        if s == 0 {
            return Ok(());
        }
        type ErrorFn = unsafe extern "C" fn(Status, *mut u8) -> Status;
        let mut message = [0u8; 64];
        if let Ok(f) = unsafe { self.function::<ErrorFn>(0x6c2d048c) } {
            unsafe {
                f(s, message.as_mut_ptr());
            }
        }
        let end = message.iter().position(|b| *b == 0).unwrap_or(64);
        Err(format!(
            "NVIDIA {operation} failed ({s}): {}",
            String::from_utf8_lossy(&message[..end])
        ))
    }
    fn open() -> Result<Self, String> {
        let mut path = [0u16; 32768];
        let n = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
        if n == 0 || n >= path.len() {
            return Err("Windows system directory is unavailable.".into());
        }
        let dll =
            Dll::load(&PathBuf::from(String::from_utf16_lossy(&path[..n])).join("nvapi64.dll"))
                .map_err(|_| "NVIDIA driver / NVAPI is unavailable.".to_string())?;
        let query = unsafe {
            dll.function::<unsafe extern "C" fn(u32) -> *mut c_void>(c"nvapi_QueryInterface")?
        };
        let get = |id: u32| {
            let p = unsafe { query(id) };
            if p.is_null() {
                Err("NVIDIA driver interface unavailable.".to_string())
            } else {
                Ok(p)
            }
        };
        let initialize: unsafe extern "C" fn() -> Status =
            unsafe { std::mem::transmute(get(0x0150e828)?) };
        let unload: unsafe extern "C" fn() -> Status =
            unsafe { std::mem::transmute(get(0xd22bdd7e)?) };
        let create: unsafe extern "C" fn(*mut Handle) -> Status =
            unsafe { std::mem::transmute(get(0x0694d52e)?) };
        let destroy: SessionFn = unsafe { std::mem::transmute(get(0xdad9cff8)?) };
        let status = unsafe { initialize() };
        if status != 0 {
            return Err(format!("NVIDIA NVAPI initialization failed ({status})."));
        }
        let mut session = ptr::null_mut();
        let status = unsafe { create(&mut session) };
        if status != 0 {
            unsafe {
                unload();
            }
            return Err(format!("NVIDIA driver settings session failed ({status})."));
        }
        let mut n = Self {
            _dll: dll,
            query,
            session,
            profile: ptr::null_mut(),
            destroy,
            unload,
        };
        n.load()?;
        let base: unsafe extern "C" fn(Handle, *mut Handle) -> Status =
            unsafe { n.function(0xda8466a0)? };
        let result = unsafe { base(session, &mut n.profile) };
        n.status(result, "global profile query")?;
        n.read()?;
        Ok(n)
    }
    fn load(&self) -> Result<(), String> {
        let f: SessionFn = unsafe { self.function(0x375dbd6b)? };
        self.status(unsafe { f(self.session) }, "load")
    }
    fn read(&mut self) -> Result<Snapshot, String> {
        self.load()?;
        let mut s = Setting::new();
        let f: unsafe extern "C" fn(Handle, Handle, u32, *mut Setting) -> Status =
            unsafe { self.function(0x73bf8338)? };
        self.status(
            unsafe { f(self.session, self.profile, s.id, &mut s) },
            "frame limiter read",
        )?;
        if s.kind != 0 || s.current[0] > 1023 {
            return Err("This driver returned an unsupported frame limiter representation.".into());
        }
        Ok(Snapshot {
            provider: Provider::Nvidia,
            value: s.current[0],
            inherited: s.predefined != 0 || s.location != 0,
            limiter_disabled: None,
        })
    }
    fn write(&mut self, value: u32, inherited: bool) -> Result<(), String> {
        self.load()?;
        if inherited {
            let remove: unsafe extern "C" fn(Handle, Handle, u32) -> Status =
                unsafe { self.function(0xe4a26362)? };
            self.status(
                unsafe { remove(self.session, self.profile, 0x10835002) },
                "restore inherited limit",
            )?;
        } else {
            let mut s = Setting::new();
            s.current[0] = value;
            let set: unsafe extern "C" fn(Handle, Handle, *mut Setting) -> Status =
                unsafe { self.function(0x577dd202)? };
            self.status(
                unsafe { set(self.session, self.profile, &mut s) },
                "frame limiter write",
            )?;
        }
        let save: SessionFn = unsafe { self.function(0xfcbc7e14)? };
        self.status(unsafe { save(self.session) }, "save")
    }
}
struct Rtss {
    _dll: Dll,
    profile_path: PathBuf,
    window: Handle,
    load: unsafe extern "C" fn(*const u8),
    save: unsafe extern "C" fn(*const u8),
    get: unsafe extern "C" fn(*const u8, *mut u8, u32) -> i32,
    set: unsafe extern "C" fn(*const u8, *mut u8, u32) -> i32,
    update: unsafe extern "C" fn(),
    flags: unsafe extern "C" fn() -> u32,
    set_flags: unsafe extern "C" fn(u32, u32) -> u32,
}
impl Rtss {
    fn open() -> Result<Self, String> {
        let window = unsafe {
            let a = FindWindowW(ptr::null(), wide("RTSS").as_ptr());
            if a.is_null() {
                FindWindowW(ptr::null(), wide("RivaTuner Statistics Server").as_ptr())
            } else {
                a
            }
        };
        if window.is_null() {
            return Err("RTSS is not running. Open RTSS to use its limiter.".into());
        }
        let mut data = [0u16; 32768];
        let mut size = std::mem::size_of_val(&data) as u32;
        let status = unsafe {
            RegGetValueW(
                (0x80000002u32 as i32 as isize) as Handle,
                wide("SOFTWARE\\WOW6432Node\\Unwinder\\RTSS").as_ptr(),
                wide("InstallDir").as_ptr(),
                2,
                ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return Err("The installed RTSS location could not be read.".into());
        }
        let n = data
            .iter()
            .position(|v| *v == 0)
            .ok_or("Invalid RTSS install path.")?;
        let directory = PathBuf::from(String::from_utf16_lossy(&data[..n]));
        let profile_path = directory.join("Profiles").join("Global");
        let dll = Dll::load(&directory.join("RTSSHooks64.dll"))?;
        unsafe {
            Ok(Self {
                profile_path,
                window,
                load: dll.function(c"LoadProfile")?,
                save: dll.function(c"SaveProfile")?,
                get: dll.function(c"GetProfileProperty")?,
                set: dll.function(c"SetProfileProperty")?,
                update: dll.function(c"UpdateProfiles")?,
                flags: dll.function(c"GetFlags")?,
                set_flags: dll.function(c"SetFlags")?,
                _dll: dll,
            })
        }
    }
    fn read(&mut self) -> Result<Snapshot, String> {
        let mut value = 0u32;
        unsafe {
            (self.load)(c"".as_ptr().cast());
            if (self.get)(
                c"FramerateLimit".as_ptr().cast(),
                (&mut value as *mut u32).cast(),
                4,
            ) == 0
            {
                return Err("RTSS could not read its global frame limit.".into());
            }
        }
        let persisted = rtss_profile(
            &std::fs::read(&self.profile_path)
                .map_err(|_| "RTSS global profile could not be read from disk.")?,
        )?;
        if persisted != value {
            return Err("RTSS runtime and saved global limits differ. The profile may not have saved; check privileges and close pending RTSS edits.".into());
        }
        Ok(Snapshot {
            provider: Provider::Rtss,
            value,
            inherited: false,
            limiter_disabled: Some(unsafe { (self.flags)() } & 4 != 0),
        })
    }
    fn write(&mut self, mut value: u32, disabled: Option<bool>) -> Result<(), String> {
        unsafe {
            (self.load)(c"".as_ptr().cast());
            if (self.set)(
                c"FramerateLimit".as_ptr().cast(),
                (&mut value as *mut u32).cast(),
                4,
            ) == 0
            {
                return Err("RTSS rejected the global FPS value.".into());
            }
            (self.save)(c"".as_ptr().cast());
            (self.update)();
            if let Some(disabled) = disabled
                && ((self.flags)() & 4 != 0) != disabled
            {
                (self.set_flags)(!4, if disabled { 4 } else { 0 });
                if PostMessageW(self.window, 0x8000 + 100, 0, 0) == 0 {
                    return Err("RTSS saved the value but Windows blocked its settings notification. Match Director and RTSS privileges.".into());
                }
            }
        }
        Ok(())
    }
}
fn rtss_profile(data: &[u8]) -> Result<u32, String> {
    if data.len() > 1024 * 1024 {
        return Err("RTSS global profile is too large.".into());
    }
    let text = if data.starts_with(&[0xff, 0xfe]) {
        String::from_utf16_lossy(
            &data[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|v| u16::from_le_bytes([v[0], v[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(data).into_owned()
    };
    let mut section = false;
    let (mut limit, mut denominator, mut time) = (None, 1, 0);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line.eq_ignore_ascii_case("[Framerate]");
            continue;
        }
        if !section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "limit" => {
                limit = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "RTSS has an unsupported global limit value.")?,
                )
            }
            "limitdenominator" => {
                denominator = value.parse().map_err(|_| "Invalid RTSS denominator.")?
            }
            "limittime" => {
                time = value
                    .parse()
                    .map_err(|_| "Invalid RTSS time-based limit.")?
            }
            _ => {}
        }
    }
    if denominator != 1 || time != 0 {
        return Err("This preview supports integer RTSS limits. Fractional or time-based RTSS limits are left unchanged.".into());
    }
    limit.ok_or("The RTSS global profile has no readable frame limit.".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn nvidia_abi_matches_version_one_header() {
        assert_eq!(std::mem::size_of::<super::Setting>(), 12320);
        assert_eq!(std::mem::offset_of!(super::Setting, current), 8220);
    }
    #[test]
    #[ignore = "Read-only installed FPS provider inspection; explicit native validation"]
    fn inspect_installed() {
        println!(
            "{}",
            serde_json::to_string_pretty(&super::survey()).unwrap()
        );
    }
    #[test]
    fn rtss_disk_check_rejects_fractional_modes_and_ignores_other_sections() {
        assert_eq!(
            super::rtss_profile(
                b"[Other]\nLimit=999\n[Framerate]\nLimit=115\nLimitDenominator=1\nLimitTime=0"
            )
            .unwrap(),
            115
        );
        assert!(super::rtss_profile(b"[Framerate]\nLimit=60000\nLimitDenominator=1001").is_err());
        assert!(super::rtss_profile(b"[Framerate]\nLimit=120\nLimitTime=12345").is_err());
    }
    #[test]
    #[ignore = "Authorized temporary live cap test; always restores actual pre-test NVIDIA/RTSS configuration"]
    fn native_roundtrip() {
        use crate::fps::Backend;
        for provider in [crate::fps::Provider::Nvidia, crate::fps::Provider::Rtss] {
            let mut backend = super::WindowsFps::open(provider).unwrap();
            let before = backend.snapshot().unwrap();
            let request = crate::fps::Request {
                provider,
                limit: Some(61),
                vrr_safe: false,
            };
            let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                backend.set(&request)?;
                crate::fps::verify(&request, &backend.snapshot()?)
            }));
            let recovery = crate::fps::restore_verified(&mut backend, &before);
            assert!(recovery.is_ok(), "Recovery failed: {recovery:?}");
            assert!(
                attempt.is_ok_and(|r| r.is_ok()),
                "Test cap did not verify; original configuration restored"
            );
            println!(
                "{} test cap verified; exact original state restored: {:?}",
                provider.label(),
                before
            );
        }
    }
}
