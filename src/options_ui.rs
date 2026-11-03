//! Native options, separate from device controls.
use crate::{DirectorApp, Message, work};
use windows_reactor::*;
impl DirectorApp {
    pub(crate) fn open_logs(&mut self, cx: &ComponentContext<Self>) {
        work(cx, || {
            Message::LogsOpened((|| {
                let folder = director::diagnostics::folder()?;
                std::fs::create_dir_all(&folder)
                    .map_err(|e| format!("Could not create the logs folder: {e}"))?;
                director::diagnostics::measure("app.logs.shell", "", || open_folder(&folder))?;
                director::diagnostics::flush();
                Ok(format!(
                    "Log location: {}\nLogger initialized: {} · Records written: {}",
                    folder.display(),
                    director::diagnostics::initialized(),
                    director::diagnostics::written()
                ))
            })())
        });
    }
    pub(crate) fn options_view(&self, cx: &ViewContext<Self>) -> View {
        ScrollViewer::new().horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled).content(StackPanel::new().spacing(20.0).children((
            TextBlock::new().text("Options").font_size(28.0).font_weight(FontWeight::SEMI_BOLD),
            TextBlock::new().text("Setup tips").font_size(18.0).font_weight(FontWeight::SEMI_BOLD),
            ToggleSwitch::new().automation_name("Show TV setup tips").is_on(self.tip.show_tips())
                .is_enabled(!self.busy && self.tip.preferences.is_some() && self.tip.pending.is_none())
                .on_toggled(cx.callback(|show| Message::Tip(crate::tv_tip_ui::TipMessage::Show(show))))
                .slot(ToggleSwitchSlot::Header, "Show TV setup tips"),
            if let Some(error) = &self.tip.error { TextBlock::new().text(format!("Could not update setup tips: {error}")).text_wrapping(TextWrapping::Wrap).into() } else { View::empty() },
            TextBlock::new().text("Diagnostics").font_size(18.0).font_weight(FontWeight::SEMI_BOLD),
            TextBlock::new().text("Director keeps local logs to help explain what happened during an operation. Old logs are replaced automatically. Pairing keys and raw TV messages are excluded.")
                .text_wrapping(TextWrapping::Wrap).opacity(0.72),
            StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((
                Button::new().automation_name("Open logs folder").is_enabled(!self.busy)
                .on_click(cx.message(Message::OpenLogs)).content("Open logs folder"),
            )),
            if let Some(error) = director::diagnostics::failure() {
                TextBlock::new().text(format!("Logging is unavailable: {error}")).text_wrapping(TextWrapping::Wrap).into()
            } else { View::empty() },
        )))
    }
}
pub(crate) fn open_folder(folder: &std::path::Path) -> Result<(), String> {
    use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
    let mut buffer = [0u16; 32768];
    let length = unsafe { GetWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err("Windows could not locate File Explorer.".into());
    }
    use std::os::windows::ffi::OsStringExt;
    let explorer = std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]))
        .join("explorer.exe");
    // Invoke Windows' own file manager directly, avoiding a folder association handoff.
    std::process::Command::new(explorer)
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start File Explorer: {e}"))?;
    Ok(())
}
