//! Native options, separate from device controls.
use crate::{DirectorApp, Message, work};
use director::{backup, diagnostics};
use std::{path::PathBuf, sync::Arc};
use windows_reactor::*;

#[derive(Clone)]
pub(crate) enum OptionsMessage {
    Logs(Option<String>),
    LogsLoaded(String, String, bool),
    Backup(bool),
    Preview(Arc<backup::Import>),
    Done(Result<Option<String>, String>),
    Closed(ContentDialogResult),
}

pub(crate) enum OptionsDialog {
    Logs {
        summary: String,
        details: String,
        selected_run: bool,
    },
    Restore(Arc<backup::Import>),
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
            OptionsMessage::Backup(save) if self.ready() => {
                if !save
                    && (self.draft.is_some()
                        || self.display.draft.is_some()
                        || self.scene.draft.is_some())
                {
                    self.failure(
                        "Save or cancel open preset and scene drafts before restoring a backup."
                            .into(),
                    );
                    return;
                }
                let hwnd =
                    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
                let mut pid = 0;
                unsafe {
                    windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                        hwnd,
                        Some(&mut pid),
                    )
                };
                let owner = if pid == std::process::id() {
                    hwnd.0 as isize
                } else {
                    0
                };
                self.begin(if save {
                    "Choose where to save the backup…"
                } else {
                    "Choose a Director backup…"
                });
                work(cx, move || match choose_backup(save, owner) {
                    Ok(Some(path)) if save => {
                        msg(OptionsMessage::Done(backup::export(&path).map(|()| {
                            Some(format!("Backup saved to {}", path.display()))
                        })))
                    }
                    Ok(Some(path)) => match backup::preview(&path) {
                        Ok(import) => msg(OptionsMessage::Preview(Arc::new(import))),
                        Err(error) => msg(OptionsMessage::Done(Err(error))),
                    },
                    Ok(None) => msg(OptionsMessage::Done(Ok(None))),
                    Err(error) => msg(OptionsMessage::Done(Err(error))),
                });
            }
            OptionsMessage::Preview(import) => {
                self.end();
                self.status = "Review the backup before restoring.".into();
                self.options_dialog = Some(OptionsDialog::Restore(import));
            }
            OptionsMessage::Closed(result) => {
                if let Some(OptionsDialog::Restore(import)) = self.options_dialog.take()
                    && result == ContentDialogResult::Primary
                {
                    self.begin("Restoring backup…");
                    work(cx, move || {
                        let result = backup::restore(&import).map(|()| Some(
                            "Backup restored. Review device links before applying presets or scenes.".into()
                        ));
                        msg(OptionsMessage::Done(result))
                    });
                }
            }
            OptionsMessage::Done(result) => {
                self.end();
                match result {
                    Ok(Some(status)) => {
                        self.status = status;
                        self.announce(cx);
                    }
                    Ok(None) => self.status = "Ready.".into(),
                    Err(error) => self.failure(error),
                }
            }
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
            Some(OptionsDialog::Restore(import)) => dialog
                .title("Restore Director backup?")
                .primary_button_text("Replace scenes and presets")
                .close_button_text("Cancel")
                .content(
                    ScrollViewer::new()
                        .max_height(440.0)
                        .content(diagnostic_text(&import.preview)),
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

fn choose_backup(save: bool, owner: isize) -> Result<Option<PathBuf>, String> {
    use windows::{
        Win32::{
            Foundation::{ERROR_CANCELLED, HWND},
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
            },
            UI::Shell::{
                Common::COMDLG_FILTERSPEC, FOS_FORCEFILESYSTEM, FileOpenDialog, FileSaveDialog,
                IFileDialog, SIGDN_FILESYSPATH,
            },
        },
        core::{HRESULT, w},
    };
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        let _apartment = Apartment;
        let select = || -> windows::core::Result<Option<PathBuf>> {
            let dialog: IFileDialog = CoCreateInstance(
                if save {
                    &FileSaveDialog
                } else {
                    &FileOpenDialog
                },
                None,
                CLSCTX_INPROC_SERVER,
            )?;
            dialog.SetOptions(dialog.GetOptions()? | FOS_FORCEFILESYSTEM)?;
            dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: w!("Director backup (*.director-backup)"),
                pszSpec: w!("*.director-backup"),
            }])?;
            dialog.SetDefaultExtension(w!("director-backup"))?;
            if save {
                dialog.SetTitle(w!("Save Director backup"))?;
                dialog.SetFileName(w!("Director"))?;
            } else {
                dialog.SetTitle(w!("Open Director backup"))?;
            }
            if let Err(error) = dialog.Show(Some(HWND(owner as *mut _))) {
                if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                    return Ok(None);
                }
                return Err(error);
            }
            let value = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            use std::os::windows::ffi::OsStringExt;
            let path = PathBuf::from(std::ffi::OsString::from_wide(value.as_wide()));
            CoTaskMemFree(Some(value.0.cast()));
            Ok(Some(path))
        };
        select().map_err(|e| format!("Could not select a backup file: {e}"))
    }
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
            TextBlock::new().text("Notifications").font_size(18.0).font_weight(FontWeight::SEMI_BOLD),
            ToggleSwitch::new().automation_name("Notify when CLI scenes fail")
                .is_on(self.tip.pending.as_ref().or(self.tip.preferences.as_ref()).is_some_and(|p| p.notify_scene_failures))
                .is_enabled(!self.busy && self.tip.preferences.is_some() && self.tip.pending.is_none())
                .on_toggled(cx.callback(|enabled| Message::Tip(crate::tv_tip_ui::TipMessage::NotifySceneFailures(enabled))))
                .slot(ToggleSwitchSlot::Header, "Notify when CLI scenes fail"),
            TextBlock::new().text("Requires the MSI installation. Click a Windows notification to open that run’s details.")
                .text_wrapping(TextWrapping::Wrap).opacity(0.72),
            TextBlock::new().text("Diagnostics").font_size(18.0).font_weight(FontWeight::SEMI_BOLD),
            TextBlock::new().text("Director keeps local logs to help explain what happened during an operation. Old logs are replaced automatically. Pairing keys and raw TV messages are excluded.")
                .text_wrapping(TextWrapping::Wrap).opacity(0.72),
            StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((
                Button::new().is_enabled(!self.busy).on_click(cx.message(msg(OptionsMessage::Logs(None)))).content("View logs"),
                Button::new().automation_name("Open logs folder").is_enabled(!self.busy)
                .on_click(cx.message(Message::OpenLogs)).content("Open logs folder"),
            )),
            TextBlock::new().text("Backup").font_size(18.0).font_weight(FontWeight::SEMI_BOLD),
            TextBlock::new().text("Save your scenes and presets to one local file. TV pairing stays on this PC.")
                .text_wrapping(TextWrapping::Wrap).opacity(0.72),
            StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((
                Button::new().is_enabled(self.ready()).on_click(cx.message(msg(OptionsMessage::Backup(true)))).content("Export backup"),
                Button::new().is_enabled(self.ready()).on_click(cx.message(msg(OptionsMessage::Backup(false)))).content("Restore backup"),
            )),
            TextBlock::new().text("About Director").font_size(18.0).font_weight(FontWeight::SEMI_BOLD),
            StackPanel::new().spacing(4.0).children((
                TextBlock::new().text(concat!("Version ", env!("CARGO_PKG_VERSION"))),
                TextBlock::new().text(env!("CARGO_PKG_AUTHORS")).text_wrapping(TextWrapping::Wrap),
                TextBlock::new().text(env!("CARGO_PKG_DESCRIPTION")).text_wrapping(TextWrapping::Wrap).opacity(0.72),
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
