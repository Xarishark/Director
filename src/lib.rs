pub mod audio;
pub mod presets;
pub mod diagnostics;
#[cfg(windows)]
pub mod audio_devices;
#[cfg(windows)]
pub mod windows_audio;
pub mod displays;
#[cfg(windows)]
pub mod windows_hdr;
#[cfg(windows)]
pub mod windows_displays;
pub mod display_apply;
pub mod display_editor;
pub mod display_mirroring;
pub mod display_detection;
pub mod display_capabilities;
#[cfg(windows)]
pub mod windows_display_modes;
#[cfg(windows)]
pub mod windows_display_apply;
pub mod tv_network;
pub mod lg_protocol;
pub mod tv_control;
