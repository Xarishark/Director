//! Direct Windows Core Audio access. All COM objects stay on their creating worker.
use crate::audio::{AudioBackend, Output, Result, Role};
use crate::audio_devices::{Identity, Inventory};
use std::{ffi::c_void, marker::PhantomData, rc::Rc};
use windows::{
    Win32::{
        Devices::FunctionDiscovery::{PKEY_Device_FriendlyName, PKEY_Device_InstanceId},
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, PROPERTYKEY, WAIT_ABANDONED,
            WAIT_OBJECT_0,
        },
        Media::Audio::{
            DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATEMASK_ALL, EDataFlow, ERole,
            Endpoints::IAudioEndpointVolume, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
            eCapture, eCommunications, eConsole, eMultimedia, eRender,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
                CoUninitialize, STGM_READ, StructuredStorage::PropVariantToStringAlloc,
            },
            Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
        },
    },
    core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface, PCWSTR},
};
fn context(error: impl std::fmt::Display) -> String {
    format!("{error}")
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

struct Apartment(PhantomData<Rc<()>>);
impl Apartment {
    fn new() -> Result<Self> {
        // SAFETY: each worker initializes its own apartment and owns its COM objects.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(context)?;
        }
        Ok(Self(PhantomData))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub struct WindowsAudio {
    enumerator: IMMDeviceEnumerator,
    inventory: Inventory,
    output_ids: Vec<Identity>,
    input_ids: Vec<Identity>,
    // Dropped after the enumerator; this type intentionally cannot move across threads.
    _apartment: Apartment,
}
impl WindowsAudio {
    pub fn new() -> Result<Self> {
        let apartment = Apartment::new()?;
        let enumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(context)?;
        Ok(Self {
            enumerator,
            inventory: Inventory::default(),
            output_ids: Vec::new(),
            input_ids: Vec::new(),
            _apartment: apartment,
        })
    }
    fn device(&self, id: &str) -> Result<IMMDevice> {
        let id = wide(id);
        unsafe { self.enumerator.GetDevice(PCWSTR(id.as_ptr())) }.map_err(context)
    }
    fn device_id(device: &IMMDevice) -> Result<String> {
        unsafe {
            let id = device.GetId().map_err(context)?;
            let result = id.to_string().map_err(context);
            CoTaskMemFree(Some(id.0.cast()));
            result
        }
    }
    fn endpoint_volume(&self, id: &str) -> Result<IAudioEndpointVolume> {
        unsafe { self.device(id)?.Activate(CLSCTX_ALL, None) }.map_err(context)
    }
    fn endpoints(&mut self, flow: EDataFlow) -> Result<Vec<Output>> {
        let console = self.default_for(flow, Role::Console).ok();
        let multimedia = self.default_for(flow, Role::Multimedia).ok();
        let communications = self.default_for(flow, Role::Communications).ok();
        unsafe {
            let devices = self
                .enumerator
                .EnumAudioEndpoints(flow, DEVICE_STATE(DEVICE_STATEMASK_ALL))
                .map_err(context)?;
            let mut output = Vec::new();
            let mut identities = Vec::new();
            for index in 0..devices.GetCount().map_err(context)? {
                let device = devices.Item(index).map_err(context)?;
                let id = Self::device_id(&device)?;
                let state = device.GetState().map_err(context)?;
                let store = device.OpenPropertyStore(STGM_READ).ok();
                let read = |key: &PROPERTYKEY| -> Option<String> {
                    let value = store.as_ref()?.GetValue(key).ok()?;
                    let text = PropVariantToStringAlloc(&value).ok()?;
                    let result = text.to_string().ok().filter(|s| !s.is_empty());
                    CoTaskMemFree(Some(text.0.cast()));
                    result
                };
                let name = read(&PKEY_Device_FriendlyName)
                    .unwrap_or_else(|| "Unnamed audio device".into());
                identities.push(Identity {
                    id: id.clone(),
                    name: name.clone(),
                    stable: read(&STABLE_ID),
                    instance: read(&PKEY_Device_InstanceId),
                });
                let active = state == DEVICE_STATE_ACTIVE;
                let volume_percent = if active {
                    self.volume(&id)
                        .ok()
                        .map(|v| (v * 100.0).round().clamp(0.0, 100.0) as u8)
                } else {
                    None
                };
                let muted = if active { self.mute(&id).ok() } else { None };
                output.push(Output {
                    communications: communications.as_deref() == Some(&id),
                    console: console.as_deref() == Some(&id),
                    multimedia: multimedia.as_deref() == Some(&id),
                    id,
                    name,
                    active,
                    volume_percent,
                    muted,
                    state: match state.0 {
                        1 => "Available",
                        2 => "Disabled",
                        4 => "Not present",
                        8 => "Unplugged",
                        _ => "Unavailable",
                    }
                    .into(),
                });
            }
            output.sort_by(|a, b| {
                b.active
                    .cmp(&a.active)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    .then_with(|| a.id.cmp(&b.id))
            });
            let input = flow == eCapture;
            match Inventory::remember(input, &identities) {
                Ok(inventory) => self.inventory = inventory,
                Err(error) => crate::diagnostics::event("audio.inventory", "error", &error),
            }
            if input {
                self.input_ids = identities;
            } else {
                self.output_ids = identities;
            }
            Ok(output)
        }
    }
    pub fn references_audio_id(&self, id: &str, target: &str, input: bool) -> bool {
        self.inventory.references(id, target, input)
    }

    pub fn picker_devices(&mut self, input: bool) -> Result<Vec<Output>> {
        let devices = if input {
            self.inputs()?
        } else {
            self.outputs()?
        };
        let live = if input {
            &self.input_ids
        } else {
            &self.output_ids
        };
        Ok(self.inventory.picker(input, live, devices))
    }

    pub fn relink_device(
        &mut self,
        input: bool,
        old_id: String,
        old_name: String,
        replacement: &str,
    ) -> Result<()> {
        let live = if input {
            self.inputs()?
        } else {
            self.outputs()?
        };
        if !live.iter().any(|d| d.id == replacement && d.active) {
            return Err("The replacement device is no longer available.".into());
        }
        let identities = if input {
            &self.input_ids
        } else {
            &self.output_ids
        };
        if self.inventory.resolve(&old_id, input, identities).is_some() {
            return Err("The saved device now resolves. Reopen the editor to refresh it.".into());
        }
        Inventory::relink(
            input,
            Identity {
                id: old_id,
                name: old_name,
                stable: None,
                instance: None,
            },
            replacement,
        )
    }

    fn default_for(&self, flow: EDataFlow, role: Role) -> Result<String> {
        let native = match role {
            Role::Console => eConsole,
            Role::Multimedia => eMultimedia,
            Role::Communications => eCommunications,
        };
        let device =
            unsafe { self.enumerator.GetDefaultAudioEndpoint(flow, native) }.map_err(context)?;
        Self::device_id(&device)
    }
    pub fn policy_available(&self) -> Result<()> {
        let _: PolicyConfig = unsafe { CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) }
            .map_err(context)?;
        Ok(())
    }
}
const STABLE_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e),
    pid: 12,
};

impl AudioBackend for WindowsAudio {
    fn resolve_id(&self, id: &str, input: bool) -> String {
        let live = if input {
            &self.input_ids
        } else {
            &self.output_ids
        };
        self.inventory
            .resolve(id, input, live)
            .unwrap_or_else(|| self.inventory.linked_id(id, input).unwrap_or("").to_owned())
    }
    fn outputs(&mut self) -> Result<Vec<Output>> {
        self.endpoints(eRender)
    }
    fn inputs(&mut self) -> Result<Vec<Output>> {
        self.endpoints(eCapture)
    }
    fn default_input(&mut self, role: Role) -> Result<String> {
        self.default_for(eCapture, role)
    }
    fn set_default_input(&mut self, role: Role, id: &str) -> Result<()> {
        self.set_default(role, id)
    }
    fn default_output(&mut self, role: Role) -> Result<String> {
        self.default_for(eRender, role)
    }
    fn set_default(&mut self, role: Role, id: &str) -> Result<()> {
        let role = match role {
            Role::Console => eConsole,
            Role::Multimedia => eMultimedia,
            Role::Communications => eCommunications,
        };
        let policy: PolicyConfig =
            unsafe { CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) }
                .map_err(context)?;
        let id = wide(id);
        unsafe {
            (policy.vtable().set_default_endpoint)(policy.as_raw(), PCWSTR(id.as_ptr()), role).ok()
        }
        .map_err(|e| format!("Windows could not change the default audio device: {e}"))
    }
    fn volume(&mut self, id: &str) -> Result<f32> {
        unsafe { self.endpoint_volume(id)?.GetMasterVolumeLevelScalar() }.map_err(context)
    }
    fn set_volume(&mut self, id: &str, scalar: f32) -> Result<()> {
        if !scalar.is_finite() || !(0.0..=1.0).contains(&scalar) {
            return Err("Invalid volume scalar.".into());
        }
        unsafe {
            self.endpoint_volume(id)?
                .SetMasterVolumeLevelScalar(scalar, &GUID::zeroed())
        }
        .map_err(context)
    }
    fn mute(&mut self, id: &str) -> Result<bool> {
        unsafe { self.endpoint_volume(id)?.GetMute() }
            .map(|muted| muted.as_bool())
            .map_err(context)
    }
    fn set_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        unsafe { self.endpoint_volume(id)?.SetMute(muted, &GUID::zeroed()) }.map_err(context)
    }
}

// Windows' PolicyConfig COM interface is not a documented public contract.
// Keep its ABI declaration isolated; do not emulate audio switching with helper EXEs.
const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
#[repr(transparent)]
#[derive(Clone)]
struct PolicyConfig(IUnknown);
unsafe impl Interface for PolicyConfig {
    type Vtable = PolicyConfigVtable;
    const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
}
#[repr(C)]
struct PolicyConfigVtable {
    base: IUnknown_Vtbl,
    // GetMixFormat through SetPropertyValue: preserve their slots, never invoke them.
    preceding_methods: [usize; 10],
    set_default_endpoint: unsafe extern "system" fn(*mut c_void, PCWSTR, ERole) -> HRESULT,
    set_endpoint_visibility: usize,
}

pub struct OperationLock(HANDLE);
impl OperationLock {
    pub fn acquire() -> Result<Self> {
        let name = wide("Local\\Director.Audio.Apply.v1");
        unsafe {
            let handle = CreateMutexW(None, false, PCWSTR(name.as_ptr())).map_err(context)?;
            let _already_existed = GetLastError() == ERROR_ALREADY_EXISTS;
            let wait = WaitForSingleObject(handle, 0);
            if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
                Ok(Self(handle))
            } else {
                let _ = CloseHandle(handle);
                Err("Another Director audio change is running. Wait for it to finish.".into())
            }
        }
    }
}
impl Drop for OperationLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OperationLock;

    #[test]
    fn apply_lock_excludes_other_threads_and_releases_afterward() {
        let guard = OperationLock::acquire().expect("first operation owns the lock");
        assert!(
            std::thread::spawn(|| OperationLock::acquire().is_err())
                .join()
                .unwrap()
        );
        drop(guard);
        assert!(
            std::thread::spawn(|| OperationLock::acquire().is_ok())
                .join()
                .unwrap()
        );
    }
}
