//! Native discovery adapter. Never commits a temporary topology to Windows' database.
use crate::{
    display::display_apply::Backend as ApplyBackend,
    display::display_detection::{Backend, Learned},
    display::displays::Layout,
    display::windows_display_apply::{Snapshot, WindowsDisplays},
    display::windows_display_modes,
    display::windows_displays,
    tv::tv_control::{self, Backend as TvBackend, PowerState},
    tv::tvs::Tv,
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct WindowsDetection {
    windows: WindowsDisplays,
    pub tv: Option<Tv>,
}
impl Backend for WindowsDetection {
    type Snapshot = Snapshot;
    fn snapshot(&mut self) -> Result<(Layout, Snapshot), String> {
        self.windows.snapshot()
    }
    fn wakes_tv(&self) -> bool {
        self.tv.is_some()
    }
    fn prepare_tv(&mut self, cancel: &AtomicBool, deadline: Instant) -> Result<(), String> {
        let tv = self.tv.as_ref().ok_or("No TV is linked.")?;
        if cancel.load(Ordering::Acquire) {
            return Err("Detection canceled.".into());
        }
        let mut backend = crate::tv::windows_tv::WindowsTvControls::new(tv);
        tv_control::power(&mut backend, tv_control::PowerAction::Wake)?;
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Detection canceled.".into());
            }
            if Instant::now() >= deadline {
                return Err("The TV did not report awake before detection timed out.".into());
            }
            if cancel.load(Ordering::Acquire) {
                return Err("Detection canceled.".into());
            }
            let mut backend = crate::tv::windows_tv::WindowsTvControls::new(tv);
            if backend.power().is_ok_and(PowerState::awake) {
                if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                    return Err("TV preparation stopped before input selection.".into());
                }
                if let Some(input) = &tv.pc_input {
                    tv_control::switch_input(&mut backend, &input.id)?;
                }
                return Ok(());
            }
            for _ in 0..10 {
                if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    fn enable(&mut self, path: &str, before: &Layout, snapshot: &Snapshot) -> Result<(), String> {
        self.windows.enable_for_detection(path, before, snapshot)
    }
    fn current(&mut self) -> Result<Layout, String> {
        windows_displays::capture()
    }
    fn inspect(&mut self, path: &str) -> Result<Learned, String> {
        let layout = windows_displays::capture()?;
        let index = layout
            .monitors
            .iter()
            .position(|m| {
                m.enabled
                    && m.device_path
                        .as_deref()
                        .is_some_and(|p| p.eq_ignore_ascii_case(path))
            })
            .ok_or("The display is not active. Turn it on and select the PC input.")?;
        let options = windows_display_modes::discover_live(&layout)?.remove(index);
        let mut desktop = layout.monitors[index]
            .desktop
            .clone()
            .ok_or("The display did not report a desktop mode.")?;
        if let Some(mode) = crate::display::display_capabilities::recommended_mode(
            &options,
            windows_display_modes::preferred_resolution(path),
        ) {
            desktop.source_width = mode.width;
            desktop.source_height = mode.height;
            desktop.refresh_rate = mode.refresh;
            (desktop.width, desktop.height) = if matches!(desktop.rotation_degrees, 90 | 270) {
                (mode.height, mode.width)
            } else {
                (mode.width, mode.height)
            };
        }
        crate::diagnostics::event(
            "display.detect.inventory",
            "read",
            &format!(
                "{} active=true probe={}x{} recommended={}x{} rate={} hdr={:?} modes={}",
                layout.monitors[index].name,
                layout.monitors[index]
                    .desktop
                    .as_ref()
                    .map_or(0, |d| d.source_width),
                layout.monitors[index]
                    .desktop
                    .as_ref()
                    .map_or(0, |d| d.source_height),
                desktop.source_width,
                desktop.source_height,
                desktop.refresh_rate.label(),
                options.hdr,
                options.modes.len()
            ),
        );
        Ok(Learned {
            device_path: path.into(),
            name: layout.monitors[index].name.clone(),
            options,
            desktop,
        })
    }
    fn restore(&mut self, snapshot: &Snapshot) -> Result<(), String> {
        self.windows.restore_detection(snapshot)
    }
}

impl WindowsDetection {
    pub fn new(tv: Option<Tv>) -> Self {
        Self {
            tv,
            ..Default::default()
        }
    }
}
