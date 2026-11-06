//! The bottom status area. It appears only when something needs the user's eye:
//! work in progress, a result, a warning or an error. Routine hints ("Ready.", "Draft
//! updated…") still go to `status` for diagnostics but are never shown.
use crate::{DirectorApp, Message, work};
use windows_reactor::*;

/// How long a success notice stays before it hides itself.
const SUCCESS_SECONDS: u64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    /// A finished action. Closable, and hides itself after a few seconds.
    Success,
    /// Something the user should read. Stays until closed.
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Notice {
    pub tone: Tone,
    pub title: String,
    pub message: String,
    token: u64,
}

#[derive(Clone)]
pub(crate) enum StatusMessage {
    /// The success notice with this token has been shown long enough.
    Expired(u64),
    /// The user closed the status bar.
    Closed,
}

#[derive(Default)]
pub(crate) struct StatusState {
    notice: Option<Notice>,
    token: u64,
}

impl StatusState {
    fn show(&mut self, tone: Tone, title: String, message: String) -> u64 {
        self.token = self.token.wrapping_add(1);
        self.notice = Some(Notice {
            tone,
            title,
            message,
            token: self.token,
        });
        self.token
    }
    fn expire(&mut self, token: u64) {
        if self
            .notice
            .as_ref()
            .is_some_and(|n| n.token == token && n.tone == Tone::Success)
        {
            self.notice = None;
        }
    }
    pub(crate) fn clear(&mut self) {
        self.notice = None;
    }
}

impl DirectorApp {
    /// Show something the user needs to read; it stays until they close it.
    pub(crate) fn warn(&mut self, title: impl Into<String>, message: impl Into<String>) {
        let (title, message) = (title.into(), message.into());
        self.status = title.clone();
        self.details = message.clone();
        self.notices.show(Tone::Warning, title, message);
    }
    /// Call at the end of a result handler: if it ended without an error, show the
    /// current `status` and `details` as a success notice.
    pub(crate) fn announce(&mut self, cx: &ComponentContext<Self>) {
        if self.error || self.busy {
            return;
        }
        let token = self
            .notices
            .show(Tone::Success, self.status.clone(), self.details.clone());
        Self::expire_later(token, cx);
    }
    fn expire_later(token: u64, cx: &ComponentContext<Self>) {
        work(cx, move || {
            std::thread::sleep(std::time::Duration::from_secs(SUCCESS_SECONDS));
            Message::Status(StatusMessage::Expired(token))
        });
    }
    pub(crate) fn update_status(&mut self, m: StatusMessage) {
        match m {
            StatusMessage::Expired(token) => self.notices.expire(token),
            StatusMessage::Closed => {
                if self.error {
                    self.error = false;
                    self.details.clear();
                }
                self.notices.clear();
            }
        }
    }
    pub(crate) fn status_visible(&self) -> bool {
        self.error || self.busy || self.notices.notice.is_some()
    }
    pub(crate) fn status_view(&self, cx: &ViewContext<Self>) -> View {
        if self.error {
            return InfoBar::new()
                .is_open(true)
                .is_closable(true)
                .severity(InfoBarSeverity::Error)
                .title(self.status.clone())
                .message(self.details.clone())
                .on_closed(cx.message(Message::Status(StatusMessage::Closed)))
                .into();
        }
        if self.busy {
            return StackPanel::new().spacing(6.0).children((
                TextBlock::new()
                    .text(self.status.clone())
                    .opacity(0.72)
                    .text_trimming(TextTrimming::CharacterEllipsis),
                ProgressBar::new().is_indeterminate(true),
            ));
        }
        match &self.notices.notice {
            Some(n) => InfoBar::new()
                .is_open(true)
                .is_closable(true)
                .severity(match n.tone {
                    Tone::Success => InfoBarSeverity::Success,
                    Tone::Warning => InfoBarSeverity::Warning,
                })
                .title(n.title.clone())
                .message(n.message.clone())
                .on_closed(cx.message(Message::Status(StatusMessage::Closed)))
                .into(),
            None => View::empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_current_success_notice_expires() {
        let mut s = StatusState::default();
        let old = s.show(Tone::Success, "Saved".into(), String::new());
        let new = s.show(Tone::Success, "Saved again".into(), String::new());
        s.expire(old);
        assert_eq!(
            s.notice.as_ref().map(|n| n.title.as_str()),
            Some("Saved again")
        );
        s.expire(new);
        assert!(s.notice.is_none());
    }
    #[test]
    fn warnings_do_not_expire() {
        let mut s = StatusState::default();
        let token = s.show(Tone::Warning, "Check this".into(), String::new());
        s.expire(token);
        assert!(s.notice.is_some());
    }
}
