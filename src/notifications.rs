use crate::{
    diagnostics::{self, LastRun},
    preferences,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use windows::{
    Data::Xml::Dom::XmlDocument,
    UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager},
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::HSTRING,
};

const APP_ID: &str = "Director.Desktop";
const PREFIX: &str = "director-run://result/";
const RETAINED: usize = 16;

fn valid_id(id: &str) -> bool {
    (1..=39).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn activation_id(uri: &str) -> Option<&str> {
    uri.strip_prefix(PREFIX).filter(|id| valid_id(id))
}

fn directory() -> Result<PathBuf, String> {
    Ok(diagnostics::folder()?.join("NotifiedRuns"))
}

pub fn read_run(id: &str) -> Result<Option<LastRun>, String> {
    if !valid_id(id) {
        return Err("Invalid run identifier.".into());
    }
    LastRun::read_file(&directory()?.join(format!("{id}.json")))
}

pub(crate) fn report(run: &LastRun) -> Result<(), String> {
    if run.failed_stage.is_none()
        || !preferences::load(&preferences::path()?)?.notify_scene_failures
    {
        return Ok(());
    }
    // CLI scene application holds the cross-process operation lock here.
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos()
        .to_string();
    let directory = directory()?;
    crate::presets::write_document(&directory.join(format!("{id}.json")), run)?;
    let mut records = fs::read_dir(&directory)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| {
            if path.extension()?.to_str()? != "json" {
                return None;
            }
            let id = path.file_stem()?.to_str()?;
            valid_id(id).then(|| (id.parse::<u128>().unwrap_or_default(), path.clone()))
        })
        .collect::<Vec<_>>();
    records.sort_unstable_by_key(|(id, _)| *id);
    let excess = records.len().saturating_sub(RETAINED);
    for (_, file) in records.into_iter().take(excess) {
        fs::remove_file(file).map_err(|e| e.to_string())?;
    }
    show(run, &id).map_err(|e| e.to_string())
}

fn escape(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(180)
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn show(run: &LastRun, id: &str) -> windows::core::Result<()> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    let _apartment = Apartment;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?;
    if notifier.Setting()? != NotificationSetting::Enabled {
        return Ok(());
    }
    let stage = run.failed_stage.as_deref().unwrap_or_default();
    let xml = XmlDocument::new()?;
    xml.LoadXml(&HSTRING::from(format!(
        "<toast activationType=\"protocol\" launch=\"{PREFIX}{id}\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text><text>{}</text></binding></visual></toast>",
        escape(&run.name), escape(&format!("Step: {stage}")), escape(&run.message)
    )))?;
    notifier.Show(&ToastNotification::CreateToastNotification(&xml)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_accepts_only_local_numeric_result_ids() {
        assert_eq!(activation_id("director-run://result/123"), Some("123"));
        for uri in [
            "director-run://result/../last-run",
            "director-run://result/",
            "director-run://result/1?arg=x",
            "https://result/1",
        ] {
            assert_eq!(activation_id(uri), None);
        }
        assert_eq!(escape("A & <B>"), "A &amp; &lt;B&gt;");
    }
}
