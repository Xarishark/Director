//! HDR-specific CCD state. Unavailable APIs/unsupported devices stay explicit.
use windows::Win32::Devices::Display::*;
use windows::Win32::Foundation::LUID;

#[repr(C)]
#[derive(Default)]
struct AdvancedColorInfo2 {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    flags: u32,
    color_encoding: i32,
    bits_per_color_channel: u32,
    active_color_mode: i32,
}
#[repr(C)]
#[derive(Default)]
struct SetHdrState {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    flags: u32,
}
fn header(kind: i32, size: usize, adapter: u64, target: u32) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: DISPLAYCONFIG_DEVICE_INFO_TYPE(kind),
        size: size as u32,
        adapterId: LUID {
            LowPart: adapter as u32,
            HighPart: (adapter >> 32) as u32 as i32,
        },
        id: target,
    }
}
fn decode(status: i32, flags: u32) -> Result<Option<bool>, String> {
    if status != 0 {
        return Err(format!("Windows could not read HDR (error {status})."));
    }
    Ok(if flags & (1 << 4) == 0 {
        None
    } else {
        Some(flags & (1 << 5) != 0)
    })
}
pub fn state(adapter: u64, target: u32) -> Result<Option<bool>, String> {
    let mut info = AdvancedColorInfo2::default();
    info.header = header(15, std::mem::size_of_val(&info), adapter, target);
    decode(
        unsafe { DisplayConfigGetDeviceInfo(&mut info.header) },
        info.flags,
    )
}
pub fn read(adapter: u64, target: u32) -> Option<bool> {
    state(adapter, target).ok().flatten()
}
pub fn set(adapter: u64, target: u32, on: bool) -> Result<(), String> {
    let mut state = SetHdrState::default();
    state.header = header(16, std::mem::size_of_val(&state), adapter, target);
    state.flags = u32::from(on);
    let status = unsafe { DisplayConfigSetDeviceInfo(&state.header) };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("Windows could not change HDR (error {status})."))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_sdr_is_distinct_from_a_failed_query() {
        assert_eq!(decode(0, 0), Ok(None));
        assert_eq!(decode(0, 1 << 4), Ok(Some(false)));
        assert_eq!(decode(0, (1 << 4) | (1 << 5)), Ok(Some(true)));
        assert!(decode(87, 0).is_err());
    }
    #[test]
    fn hdr_structs_match_windows_sdk_layout() {
        assert_eq!(std::mem::size_of::<DISPLAYCONFIG_DEVICE_INFO_HEADER>(), 20);
        assert_eq!(std::mem::size_of::<AdvancedColorInfo2>(), 36);
        assert_eq!(std::mem::size_of::<SetHdrState>(), 24);
    }
}
