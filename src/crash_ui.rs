//! The "Director closed unexpectedly" dialog shown at startup after a crash, and Options'
//! "Report a problem". Reporting only opens a GitHub issue in the browser; the user reviews
//! and submits it there.
use crate::{DirectorApp, Message};
use windows_reactor::*;

#[derive(Clone)]
pub(crate) enum CrashMessage {
    /// The startup dialog closed: Primary reports, Secondary opens the saved report.
    Closed(ContentDialogResult),
    /// Options: open a new GitHub issue, with the latest crash report's facts if there is one.
    ReportProblem,
}

impl DirectorApp {
    pub(crate) fn update_crash(&mut self, m: CrashMessage, _: &ComponentContext<Self>) {
        match m {
            CrashMessage::Closed(result) => {
                let Some(report) = self.crash.take() else {
                    return;
                };
                director::crash::dismiss(&report);
                let opened = match result {
                    ContentDialogResult::Primary => {
                        director::crash::open(&director::crash::issue_url(Some(&report)))
                    }
                    ContentDialogResult::Secondary => director::crash::open(
                        &report.path.with_extension("seen.txt").to_string_lossy(),
                    ),
                    _ => Ok(()),
                };
                if let Err(e) = opened {
                    self.failure(e);
                }
            }
            CrashMessage::ReportProblem => {
                if let Err(e) = director::crash::open(&director::crash::issue_url(None)) {
                    self.failure(e);
                }
            }
        }
    }

    /// Shown once, at startup, when the last run left a crash report. It waits for any other
    /// dialog, because Windows allows only one open at a time. No tooltips in here: Reactor
    /// can't host them inside a ContentDialog.
    pub(crate) fn crash_dialog(&self, cx: &ViewContext<Self>) -> View {
        let Some(report) = &self.crash else {
            return View::empty();
        };
        if self.options_dialog.is_some() || self.command.is_some() {
            return View::empty();
        }
        ContentDialog::new()
            .is_open(true)
            .title("Director closed unexpectedly")
            .primary_button_text("Report on GitHub")
            .secondary_button_text("Open report")
            .close_button_text("Not now")
            .on_closed(cx.callback(|r| Message::Crash(CrashMessage::Closed(r))))
            .content(
                StackPanel::new()
                    .spacing(12.0)
                    .max_width(480.0)
                    .children((
                        TextBlock::new()
                            .text("Last time, Director stopped because of an error. Sorry about that.")
                            .text_wrapping(TextWrapping::Wrap),
                        Border::new()
                            .padding(Thickness::xy(10.0, 7.0))
                            .corner_radius(crate::ui::RADIUS_S)
                            .background(crate::ui::CODE_FILL)
                            .content(
                                TextBlock::new()
                                    .text(report.summary.clone())
                                    .is_text_selection_enabled(true)
                                    .text_wrapping(TextWrapping::Wrap),
                            ),
                        TextBlock::new()
                            .text("Report on GitHub opens a new issue in your browser with the error filled in. Nothing is sent until you submit it there.")
                            .opacity(0.72)
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text("The full report, with recent log lines, is saved in your Director logs folder. Attach it to the issue if you’re happy to share it.")
                            .font_size(crate::ui::CAPTION)
                            .opacity(crate::ui::SECONDARY)
                            .text_wrapping(TextWrapping::Wrap),
                    )),
            )
    }
}
