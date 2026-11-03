pub mod audio;
pub mod presets;
#[cfg(windows)]
pub mod windows_audio;

#[cfg(windows)]
pub mod cli;

pub mod displays;
#[cfg(windows)]
pub mod windows_displays;

#[cfg(windows)]
pub mod display_cli;

pub mod display_apply;

pub mod display_editor;
#[cfg(windows)]
pub mod windows_display_apply;
#[cfg(windows)]
pub mod windows_display_modes;
#[cfg(windows)]
pub mod windows_hdr;

pub mod lg_protocol;
pub mod tvs;
#[cfg(windows)]
pub mod windows_lg;

#[cfg(windows)]
pub mod tv_cli;

pub mod diagnostics;
pub mod display_capabilities;
pub mod display_detection;
#[cfg(windows)]
pub mod notifications;
pub mod preferences;
pub mod tv_control;
pub mod tv_network;
pub mod tv_setup;
#[cfg(windows)]
pub mod windows_display_detection;

pub mod display_mirroring;

pub mod scenes;
#[cfg(windows)]
pub mod windows_scenes;

pub mod samsung;
#[cfg(windows)]
pub mod windows_samsung;
#[cfg(windows)]
pub mod windows_tv;

pub mod android_tv;
#[cfg(windows)]
pub mod windows_android_tv;

pub mod tv_report;

pub mod fps;
#[cfg(windows)]
pub mod windows_fps;

#[cfg(windows)]
mod audio_devices;
