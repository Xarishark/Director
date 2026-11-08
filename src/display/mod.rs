//! Display presets: capture, editing, apply with verification and restore, and detection.
pub mod display_apply;
pub mod display_capabilities;
#[cfg(windows)]
pub mod display_cli;
pub mod display_detection;
pub mod display_editor;
pub mod display_mirroring;
pub mod displays;
#[cfg(windows)]
pub mod windows_display_apply;
#[cfg(windows)]
pub mod windows_display_detection;
#[cfg(windows)]
pub mod windows_display_modes;
#[cfg(windows)]
pub mod windows_displays;
#[cfg(windows)]
pub mod windows_hdr;
