//! Read-only Windows capability discovery; no driver setters or helper programs.
use crate::{
    display::display_editor::{Mode, Options},
    display::displays::{Layout, RefreshRate},
    display::windows_displays::{query_flags, target_name},
};
use windows::Win32::UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME;
use windows::{
    Win32::{
        Devices::Display::*,
        Graphics::{
            Dxgi::{Common::*, *},
            Gdi::*,
        },
    },
    core::{Interface, PCWSTR},
};
fn text(v: &[u16]) -> String {
    String::from_utf16_lossy(&v[..v.iter().position(|c| *c == 0).unwrap_or(v.len())])
}
fn usable_mode(m: &DXGI_MODE_DESC1) -> bool {
    !m.Stereo.as_bool()
        && matches!(
            m.ScanlineOrdering,
            DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE | DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED
        )
        && m.Width > 0
        && m.Height > 0
        && m.RefreshRate.Numerator > 0
        && m.RefreshRate.Denominator > 0
}
fn output_modes(output: &IDXGIOutput) -> Result<Vec<Mode>, String> {
    let output: IDXGIOutput1 = output.cast().map_err(|e| format!("{e}"))?;
    for _ in 0..3 {
        let mut count = 0;
        unsafe {
            output.GetDisplayModeList1(
                DXGI_FORMAT_R8G8B8A8_UNORM,
                DXGI_ENUM_MODES_SCALING,
                &mut count,
                None,
            )
        }
        .map_err(|e| format!("{e}"))?;
        if count > 32768 {
            return Err("Unexpectedly large mode list.".into());
        }
        let mut modes = vec![DXGI_MODE_DESC1::default(); count as usize];
        match unsafe {
            output.GetDisplayModeList1(
                DXGI_FORMAT_R8G8B8A8_UNORM,
                DXGI_ENUM_MODES_SCALING,
                &mut count,
                Some(modes.as_mut_ptr()),
            )
        } {
            Ok(()) => {
                modes.truncate(count as usize);
                return Ok(modes
                    .into_iter()
                    .filter(usable_mode)
                    .map(|m| Mode {
                        width: m.Width,
                        height: m.Height,
                        refresh: RefreshRate {
                            numerator: m.RefreshRate.Numerator,
                            denominator: m.RefreshRate.Denominator,
                        },
                    })
                    .collect());
            }
            Err(e) if e.code() == DXGI_ERROR_MORE_DATA => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Display modes kept changing. Try again when the connection settles.".into())
}
pub fn discover_live(layout: &Layout) -> Result<Vec<Options>, String> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
        .map_err(|e| format!("Could not enumerate Windows display modes: {e}"))?;
    let mut result = vec![Options::default(); layout.monitors.len()];
    // Match the physical device interface path, never the editable friendly name.
    for ai in 0..64 {
        let adapter = match unsafe { factory.EnumAdapters1(ai) } {
            Ok(a) => a,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => return Err(e.to_string()),
        };
        for oi in 0..64 {
            let output = match unsafe { adapter.EnumOutputs(oi) } {
                Ok(o) => o,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(e.to_string()),
            };
            let desc = unsafe { output.GetDesc() }.map_err(|e| e.to_string())?;
            let mut identities = Vec::new();
            for di in 0..32 {
                let mut device = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                if !unsafe {
                    EnumDisplayDevicesW(
                        PCWSTR(desc.DeviceName.as_ptr()),
                        di,
                        &mut device,
                        EDD_GET_DEVICE_INTERFACE_NAME,
                    )
                }
                .as_bool()
                {
                    break;
                }
                identities.push(text(&device.DeviceID));
            }
            for (i, m) in layout.monitors.iter().enumerate() {
                if m.device_path
                    .as_ref()
                    .is_some_and(|id| identities.iter().any(|v| v.eq_ignore_ascii_case(id)))
                {
                    match output_modes(&output) {
                        Ok(modes) => result[i].modes = modes,
                        Err(e) => result[i].warning = Some(e),
                    }
                }
            }
        }
    }
    // Disabled outputs may not be enumerable by DXGI. Expose only their exact
    // preferred mode reported by CCD, never invented whole-number refresh rates.
    let (paths, _) = query_flags(QDC_ALL_PATHS)?;
    for (i, m) in layout.monitors.iter().enumerate() {
        if let Some(p) = paths.iter().find(|p| {
            m.device_path
                .as_ref()
                .is_some_and(|id| target_name(p).1.is_some_and(|v| v.eq_ignore_ascii_case(id)))
        }) {
            result[i].hdr = crate::display::windows_hdr::read(
                crate::display::windows_displays::luid(p.targetInfo.adapterId),
                p.targetInfo.id,
            );
        }
        if result[i].modes.is_empty() && m.connected {
            if let Some(p) = paths.iter().find(|p| {
                m.device_path
                    .as_ref()
                    .is_some_and(|id| target_name(p).1.is_some_and(|v| v.eq_ignore_ascii_case(id)))
            }) {
                let mut preferred = DISPLAYCONFIG_TARGET_PREFERRED_MODE::default();
                preferred.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE;
                preferred.header.size = std::mem::size_of_val(&preferred) as u32;
                preferred.header.adapterId = p.targetInfo.adapterId;
                preferred.header.id = p.targetInfo.id;
                if unsafe { DisplayConfigGetDeviceInfo(&mut preferred.header) } == 0 {
                    let r = preferred.targetMode.targetVideoSignalInfo.vSyncFreq;
                    if preferred.width > 0
                        && preferred.height > 0
                        && r.Numerator > 0
                        && r.Denominator > 0
                    {
                        result[i].modes.push(Mode {
                            width: preferred.width,
                            height: preferred.height,
                            refresh: RefreshRate {
                                numerator: r.Numerator,
                                denominator: r.Denominator,
                            },
                        });
                    }
                }
            }
            result[i].warning=Some("Display options are limited while inactive. Set up this display to detect its available modes.".into());
        }
        if let Some(d) = &m.desktop {
            result[i].modes.insert(
                0,
                Mode {
                    width: d.source_width,
                    height: d.source_height,
                    refresh: d.refresh_rate.clone(),
                },
            );
        }
        if result[i].modes.is_empty() {
            result[i].warning = Some(
                "Turn on this display and select its PC input, then choose Set up this display."
                    .into(),
            );
        }
    }
    Ok(result)
}

/// Windows' preferred pixel dimensions, queried for this physical target only.
pub fn preferred_resolution(path: &str) -> Option<(u32, u32)> {
    let (paths, _) = query_flags(QDC_ALL_PATHS).ok()?;
    let p = paths.iter().find(|p| {
        target_name(p)
            .1
            .is_some_and(|v| v.eq_ignore_ascii_case(path))
    })?;
    let mut preferred = DISPLAYCONFIG_TARGET_PREFERRED_MODE::default();
    preferred.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_PREFERRED_MODE;
    preferred.header.size = std::mem::size_of_val(&preferred) as u32;
    preferred.header.adapterId = p.targetInfo.adapterId;
    preferred.header.id = p.targetInfo.id;
    (unsafe { DisplayConfigGetDeviceInfo(&mut preferred.header) } == 0
        && preferred.width > 0
        && preferred.height > 0)
        .then_some((preferred.width, preferred.height))
}

pub fn discover(layout: &Layout) -> Result<Vec<Options>, String> {
    let mut options = discover_live(layout)?;
    match crate::display::display_capabilities::path()
        .and_then(|p| crate::display::display_capabilities::load(&p))
    {
        Ok(entries) => crate::display::display_capabilities::merge(layout, &mut options, &entries),
        Err(e) => crate::diagnostics::event("display.capabilities.cache", "error", &e),
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unspecified_scanline_modes_remain_available_but_interlaced_modes_do_not() {
        let mut mode = DXGI_MODE_DESC1 {
            Width: 1920,
            Height: 1080,
            RefreshRate: DXGI_RATIONAL {
                Numerator: 60000,
                Denominator: 1001,
            },
            ..Default::default()
        };
        assert!(usable_mode(&mode));
        mode.ScanlineOrdering = DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST;
        assert!(!usable_mode(&mode));
        mode.ScanlineOrdering = DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE;
        assert!(usable_mode(&mode));
        mode.RefreshRate.Denominator = 0;
        assert!(!usable_mode(&mode));
    }
}
