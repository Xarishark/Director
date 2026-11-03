use director::audio::Output;
pub(crate) fn device_label(device: &Output) -> String {
    if device.active {
        device.name.clone()
    } else if device.state == "Reconnected" {
        format!("{} · Reconnected", device.name)
    } else {
        format!("{} · Offline", device.name)
    }
}
