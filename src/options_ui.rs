//! Native options, separate from device controls.
use crate::{DirectorApp, Message, work};
use director::diagnostics;
use windows_reactor::*;

#[derive(Clone)]
pub(crate) enum OptionsMessage {
    Logs(Option<String>),
    LogsLoaded(String, String, bool),
    Closed(ContentDialogResult),
}

pub(crate) enum OptionsDialog {
    Logs {
        summary: String,
        details: String,
        selected_run: bool,
    },
}

fn msg(message: OptionsMessage) -> Message {
    Message::Options(message)
}

impl DirectorApp {
    pub(crate) fn update_options(&mut self, message: OptionsMessage, cx: &ComponentContext<Self>) {
        match message {
            OptionsMessage::Logs(id) if !self.busy => {
                self.begin("Reading logs…");
                work(cx, || {
                    let selected_run = id.is_some();
                    let run = match id {
                        Some(id) => director::notifications::read_run(&id),
                        None => diagnostics::LastRun::read(),
                    };
                    let summary = match run {
                        Ok(Some(run)) => {
                            let stage = run
                                .failed_stage
                                .map_or(String::new(), |s| format!("\nStep: {s}"));
                            format!(
                                "{} · {}\n{}{stage}\n{}",
                                run.name,
                                run.completed_at,
                                run.message,
                                run.details.join("\n")
                            )
                        }
                        Ok(None) if selected_run => {
                            "This run has expired. View logs from Options to see the latest run."
                                .into()
                        }
                        Ok(None) => "No scene runs recorded yet.".into(),
                        Err(error) => format!("Could not read the last run: {error}"),
                    };
                    let details = match diagnostics::recent_text() {
                        Ok(text) if text.is_empty() => "No log entries yet.".into(),
                        Ok(text) => text,
                        Err(error) => format!("Could not read recent logs: {error}"),
                    };
                    msg(OptionsMessage::LogsLoaded(summary, details, selected_run))
                });
            }
            OptionsMessage::LogsLoaded(summary, details, selected_run) => {
                self.end();
                self.status = "Ready.".into();
                if selected_run && !self.store_ready {
                    let _ = cx.sender().send(Message::Refresh);
                }
                self.options_dialog = Some(OptionsDialog::Logs {
                    summary,
                    details,
                    selected_run,
                });
            }
            OptionsMessage::Closed(_) => { self.options_dialog = None; }
            _ => {}
        }
    }

    pub(crate) fn options_dialog(&self, cx: &ViewContext<Self>) -> View {
        let dialog = ContentDialog::new()
            .is_open(true)
            .on_closed(cx.callback(|result| msg(OptionsMessage::Closed(result))));
        match &self.options_dialog {
            Some(OptionsDialog::Logs {
                summary,
                details,
                selected_run,
            }) => dialog
                .title("Director logs")
                .close_button_text("Close")
                .content(
                    ScrollViewer::new().max_height(520.0).content(
                        StackPanel::new().spacing(12.0).children((
                            TextBlock::new()
                                .text(if *selected_run {
                                    "Scene result"
                                } else {
                                    "Last run"
                                })
                                .font_weight(FontWeight::SEMI_BOLD),
                            diagnostic_text(summary),
                            TextBlock::new()
                                .text("Recent log entries")
                                .font_weight(FontWeight::SEMI_BOLD),
                            diagnostic_text(details),
                        )),
                    ),
                ),
            None => View::empty(),
        }
    }
}

fn diagnostic_text(text: &str) -> TextBlock {
    TextBlock::new()
        .text(text)
        .is_text_selection_enabled(true)
        .text_wrapping(TextWrapping::Wrap)
}

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
                Button::new().is_enabled(!self.busy).on_click(cx.message(msg(OptionsMessage::Logs(None)))).content("View logs"),
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
