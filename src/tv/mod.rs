//! TVs: saved definitions, setup, control, Wake-on-LAN and the LG, Samsung and Android TV backends.
pub mod android_tv;
pub mod lg_protocol;
pub mod samsung;
#[cfg(windows)]
pub mod tv_cli;
pub mod tv_control;
pub mod tv_input_guard;
pub mod tv_network;
pub mod tv_report;
pub mod tv_setup;
pub mod tvs;
#[cfg(windows)]
pub mod windows_android_tv;
#[cfg(windows)]
pub mod windows_lg;
#[cfg(windows)]
pub mod windows_samsung;
#[cfg(windows)]
pub mod windows_tv;
