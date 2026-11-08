//! Windows display capture and direct validated application adapter.
use crate::display::displays::{DesktopMode, Layout, Monitor, RefreshRate};
use std::collections::BTreeMap;
use windows::Win32::{
    Devices::Display::*,
    Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID},
    Graphics::Gdi::{
        DEVMODEW, DISPLAYCONFIG_PATH_ACTIVE, DM_PELSHEIGHT, DM_PELSWIDTH, DM_POSITION,
        ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_FLAGS, EnumDisplaySettingsExW,
    },
};
pub(super) fn luid(value: LUID) -> u64 {
    ((value.HighPart as u32 as u64) << 32) | value.LowPart as u64
}
fn text(value: &[u16]) -> String {
    String::from_utf16_lossy(&value[..value.iter().position(|v| *v == 0).unwrap_or(value.len())])
}
fn query() -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), String> {
    query_flags(QDC_ALL_PATHS)
}
pub(super) fn query_flags(
    flags: QUERY_DISPLAY_CONFIG_FLAGS,
) -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), String> {
    // The topology may grow between sizing and querying (hot-plug). Retry, bounded.
    for _ in 0..4 {
        let (mut paths_len, mut modes_len) = (0, 0);
        let status = unsafe { GetDisplayConfigBufferSizes(flags, &mut paths_len, &mut modes_len) };
        if status != ERROR_SUCCESS {
            return Err(format!(
                "Could not size Windows display configuration (Windows error {}). Use a local interactive Windows session.",
                status.0
            ));
        }
        if paths_len > 65536 || modes_len > 65536 {
            return Err("Windows returned an unexpectedly large display configuration.".into());
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); paths_len.max(1) as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); modes_len.max(1) as usize];
        let status = unsafe {
            QueryDisplayConfig(
                flags,
                &mut paths_len,
                paths.as_mut_ptr(),
                &mut modes_len,
                modes.as_mut_ptr(),
                None,
            )
        };
        if status == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if status != ERROR_SUCCESS {
            return Err(format!(
                "Could not read Windows display configuration (Windows error {}).",
                status.0
            ));
        }
        paths.truncate(paths_len as usize);
        modes.truncate(modes_len as usize);
        return Ok((paths, modes));
    }
    Err(
        "The display setup kept changing. Wait for your monitors to settle, then capture again."
            .into(),
    )
}
fn source_name(path: &DISPLAYCONFIG_PATH_INFO) -> Result<String, String> {
    let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
    name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
    name.header.size = std::mem::size_of_val(&name) as u32;
    name.header.adapterId = path.sourceInfo.adapterId;
    name.header.id = path.sourceInfo.id;
    let status = unsafe { DisplayConfigGetDeviceInfo(&mut name.header) };
    if status != 0 {
        return Err(format!(
            "Could not identify an active desktop source (Windows error {status}). Refresh and capture again."
        ));
    }
    Ok(text(&name.viewGdiDeviceName))
}
pub(super) fn target_name(path: &DISPLAYCONFIG_PATH_INFO) -> (String, Option<String>) {
    let mut name = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
    name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
    name.header.size = std::mem::size_of_val(&name) as u32;
    name.header.adapterId = path.targetInfo.adapterId;
    name.header.id = path.targetInfo.id;
    if unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } != 0 {
        return (String::new(), None);
    }
    let device = text(&name.monitorDevicePath);
    (
        text(&name.monitorFriendlyDeviceName),
        (!device.is_empty()).then_some(device),
    )
}
fn desktop(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
    source_name: String,
) -> Result<DesktopMode, String> {
    // No QDC_VIRTUAL_MODE_AWARE flag: these are the regular modeInfoIdx unions.
    let mode = modes
        .get(unsafe { path.sourceInfo.Anonymous.modeInfoIdx } as usize)
        .filter(|m| {
            m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
                && m.id == path.sourceInfo.id
                && luid(m.adapterId) == luid(path.sourceInfo.adapterId)
        })
        .ok_or("An active display mode became unavailable. Refresh and capture again.")?;
    let source = unsafe { mode.Anonymous.sourceMode };
    let mut refresh = path.targetInfo.refreshRate;
    if refresh.Numerator == 0 || refresh.Denominator == 0 {
        let target = modes
            .get(unsafe { path.targetInfo.Anonymous.modeInfoIdx } as usize)
            .filter(|m| {
                m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET
                    && m.id == path.targetInfo.id
                    && luid(m.adapterId) == luid(path.targetInfo.adapterId)
            })
            .ok_or("An active display refresh rate is unavailable. Refresh and capture again.")?;
        refresh = unsafe { target.Anonymous.targetMode.targetVideoSignalInfo.vSyncFreq };
    }
    let rotation_degrees = match path.targetInfo.rotation.0 {
        1 => 0,
        2 => 90,
        3 => 180,
        4 => 270,
        _ => return Err("An active display has an unsupported orientation.".into()),
    };
    Ok(DesktopMode {
        source_adapter_luid: luid(path.sourceInfo.adapterId),
        source_id: path.sourceInfo.id,
        source_name,
        x: source.position.x,
        y: source.position.y,
        width: source.width,
        height: source.height,
        source_width: source.width,
        source_height: source.height,
        refresh_rate: RefreshRate {
            numerator: refresh.Numerator,
            denominator: refresh.Denominator,
        },
        rotation_degrees,
    })
}
fn reconcile_desktop_extent(mode: &mut DesktopMode, settings: &DEVMODEW) -> Result<(), String> {
    let required = DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT;
    if settings.dmFields & required != required
        || settings.dmPelsWidth == 0
        || settings.dmPelsHeight == 0
    {
        return Err("Windows did not provide complete current desktop dimensions. Refresh and capture again.".into());
    }
    let position = unsafe { settings.Anonymous1.Anonymous2.dmPosition };
    if (position.x, position.y) != (mode.x, mode.y) {
        return Err("The desktop layout changed during capture. Refresh and capture again.".into());
    }
    // Some drivers expose unrotated CCD dimensions. Keep those separately and
    // use the current GDI desktop extent, rather than guessing from rotation.
    mode.width = settings.dmPelsWidth;
    mode.height = settings.dmPelsHeight;
    Ok(())
}
fn read_desktop_extent(mode: &mut DesktopMode) -> Result<(), String> {
    let mut settings = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    let name = windows::core::HSTRING::from(&mode.source_name);
    if !unsafe {
        EnumDisplaySettingsExW(
            windows::core::PCWSTR(name.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut settings,
            ENUM_DISPLAY_SETTINGS_FLAGS(0),
        )
    }
    .as_bool()
    {
        return Err(
            "Could not read the current desktop dimensions. Refresh and capture again.".into(),
        );
    }
    reconcile_desktop_extent(mode, &settings)
}
pub fn capture() -> Result<Layout, String> {
    let (paths, modes) = query()?;
    let mut targets = BTreeMap::new();
    for path in paths {
        let enabled = path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0;
        let key = (luid(path.targetInfo.adapterId), path.targetInfo.id);
        // ALL_PATHS contains possible routes, not one record per physical monitor.
        if targets
            .get(&key)
            .is_some_and(|m: &Monitor| m.enabled || !enabled)
        {
            continue;
        }
        let (mut name, device_path) = target_name(&path);
        // Omit unidentifiable unused ports; retain named disconnected targets.
        if !enabled && device_path.is_none() && name.is_empty() {
            continue;
        }
        let desktop = if enabled {
            {
                let mut mode = desktop(&path, &modes, source_name(&path)?)?;
                read_desktop_extent(&mut mode)?;
                Some(mode)
            }
        } else {
            None
        };
        if name.is_empty() {
            name = desktop.as_ref().map_or_else(
                || format!("Display {}", path.targetInfo.id),
                |d| d.source_name.clone(),
            );
        }
        targets.insert(
            key,
            Monitor {
                adapter_luid: key.0,
                target_id: key.1,
                device_path,
                name,
                connected: path.targetInfo.targetAvailable.as_bool(),
                hdr: crate::display::windows_hdr::read(key.0, key.1),
                enabled,
                desktop,
            },
        );
    }
    let mut monitors = targets.into_values().collect::<Vec<_>>();
    monitors.sort_by_key(|m| {
        (
            !m.enabled,
            !m.desktop.as_ref().is_some_and(DesktopMode::primary),
            m.name.clone(),
            m.adapter_luid,
            m.target_id,
        )
    });
    let layout = Layout { monitors };
    layout.validate()?;
    Ok(layout)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> (DISPLAYCONFIG_PATH_INFO, Vec<DISPLAYCONFIG_MODE_INFO>) {
        let mut path = DISPLAYCONFIG_PATH_INFO {
            flags: DISPLAYCONFIG_PATH_ACTIVE,
            ..Default::default()
        };
        path.sourceInfo.id = 2;
        path.sourceInfo.Anonymous.modeInfoIdx = 0;
        path.targetInfo.rotation = DISPLAYCONFIG_ROTATION_IDENTITY;
        path.targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
            Numerator: 60000,
            Denominator: 1001,
        };
        let mut mode = DISPLAYCONFIG_MODE_INFO {
            infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
            id: 2,
            ..Default::default()
        };
        let mut source = DISPLAYCONFIG_SOURCE_MODE {
            width: 2560,
            height: 1440,
            ..Default::default()
        };
        source.position.x = -1440;
        source.position.y = -200;
        mode.Anonymous.sourceMode = source;
        (path, vec![mode])
    }
    #[test]
    fn reads_negative_positions_rotation_and_exact_fractional_rate() {
        let (mut path, modes) = sample();
        path.targetInfo.rotation = DISPLAYCONFIG_ROTATION_ROTATE90;
        let got = desktop(&path, &modes, "DISPLAY2".into()).unwrap();
        assert_eq!((got.x, got.y, got.rotation_degrees), (-1440, -200, 90));
        assert_eq!(
            got.refresh_rate,
            RefreshRate {
                numerator: 60000,
                denominator: 1001
            }
        );
    }
    #[test]
    fn rejects_bad_mode_indices_types_and_identities() {
        let (mut path, mut modes) = sample();
        path.sourceInfo.Anonymous.modeInfoIdx = u32::MAX;
        assert!(desktop(&path, &modes, "display".into()).is_err());
        path.sourceInfo.Anonymous.modeInfoIdx = 0;
        modes[0].id = 9;
        assert!(desktop(&path, &modes, "display".into()).is_err());
        modes[0].id = 2;
        modes[0].infoType = DISPLAYCONFIG_MODE_INFO_TYPE_TARGET;
        assert!(desktop(&path, &modes, "display".into()).is_err());
    }
    #[test]
    fn portrait_desktop_extent_preserves_raw_ccd_mode_and_rejects_races() {
        let (path, modes) = sample();
        let mut mode = desktop(&path, &modes, "DISPLAY2".into()).unwrap();
        let mut settings = DEVMODEW {
            dmFields: DM_POSITION | DM_PELSWIDTH | DM_PELSHEIGHT,
            dmPelsWidth: 1440,
            dmPelsHeight: 2560,
            ..Default::default()
        };
        settings.Anonymous1.Anonymous2.dmPosition =
            windows::Win32::Foundation::POINTL { x: -1440, y: -200 };
        let raw = (
            mode.source_width,
            mode.source_height,
            mode.refresh_rate.clone(),
        );
        reconcile_desktop_extent(&mut mode, &settings).unwrap();
        assert_eq!((mode.width, mode.height), (1440, 2560));
        assert_eq!(
            (
                mode.source_width,
                mode.source_height,
                mode.refresh_rate.clone()
            ),
            raw
        );
        settings.Anonymous1.Anonymous2.dmPosition.x = 25;
        assert!(reconcile_desktop_extent(&mut mode, &settings).is_err());
        settings.dmFields = DM_PELSWIDTH;
        assert!(reconcile_desktop_extent(&mut mode, &settings).is_err());
    }
}
