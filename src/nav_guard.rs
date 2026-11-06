//! Ask before leaving a page with unsaved changes.
//!
//! Each editor's draft is compared with a baseline taken when it opened. Choosing another
//! category from the navigation pane (or Options) with a changed draft opens a dialog:
//! Save, Discard or Keep editing. Links inside an editor that are meant to come back to
//! the draft (Manage presets, View TV, New … preset) navigate without asking.
use crate::{DirectorApp, Draft, Message, Page, display_ui::DisplayMessage};
use director::{displays, scenes::Scene, tvs::Tv};
use windows_reactor::*;

#[derive(Clone)]
pub(crate) enum GuardMessage {
    Closed(ContentDialogResult),
}

#[derive(Default)]
pub(crate) struct NavGuard {
    /// The category the user chose while the current page had unsaved changes.
    pending: Option<String>,
    /// Save was chosen; navigate once it succeeds.
    saving: bool,
    /// Bumped to rebuild the navigation pane after Keep editing, so its selection
    /// returns to the current page.
    pub(crate) revision: u64,
    audio: Option<Draft>,
    display: Option<displays::Preset>,
    tv: Option<Tv>,
    scene: Option<Scene>,
}

/// Keep the baseline in step with the draft: none without a draft, and a fresh copy when a
/// draft opens or a different item is opened.
fn track<T: Clone>(base: &mut Option<T>, draft: Option<&T>, same: impl Fn(&T, &T) -> bool) {
    match (draft, base.as_ref()) {
        (None, _) => *base = None,
        (Some(d), Some(b)) if same(d, b) => {}
        (Some(d), _) => *base = Some(d.clone()),
    }
}

fn changed<T: PartialEq>(base: &Option<T>, draft: Option<&T>) -> bool {
    draft.is_some_and(|d| base.as_ref() != Some(d))
}

fn page_name(page: Page) -> &'static str {
    match page {
        Page::Current => "Now",
        Page::Audio => "this audio preset",
        Page::Displays => "this display preset",
        Page::Tvs => "this TV",
        Page::Scenes => "this scene",
        Page::Options => "Options",
    }
}

impl DirectorApp {
    /// Record baselines for drafts opened by the previous message. Called at the start of
    /// every update, so a draft's first edit is compared with how it opened.
    pub(crate) fn track_unsaved(&mut self) {
        let g = &mut self.nav;
        track(&mut g.audio, self.draft.as_ref(), |a, b| a.id == b.id);
        track(&mut g.display, self.display.draft.as_ref(), |a, b| {
            a.id == b.id
        });
        track(&mut g.tv, self.tv.draft.as_ref(), |a, b| a.id == b.id);
        track(&mut g.scene, self.scene.draft.as_ref(), |a, b| a.id == b.id);
    }
    /// Take a new baseline after data loads into an opened draft (supported display modes,
    /// live TV details), so loading isn't mistaken for an edit.
    pub(crate) fn rebase_display(&mut self) {
        self.nav.display = None;
    }
    pub(crate) fn rebase_tv(&mut self) {
        self.nav.tv = None;
    }

    fn unsaved(&self) -> bool {
        let g = &self.nav;
        match self.page {
            Page::Audio => changed(&g.audio, self.draft.as_ref()),
            Page::Displays => changed(&g.display, self.display.draft.as_ref()),
            Page::Tvs => changed(&g.tv, self.tv.draft.as_ref()),
            Page::Scenes => changed(&g.scene, self.scene.draft.as_ref()),
            Page::Current => self.current.dirty(),
            Page::Options => false,
        }
    }
    fn can_save(&self) -> bool {
        match self.page {
            Page::Audio => self.draft_valid(),
            Page::Displays => {
                self.display_ready()
                    && self.display.drag.is_none()
                    && self
                        .display
                        .draft
                        .as_ref()
                        .is_some_and(|d| !d.name.trim().is_empty() && d.name.trim().len() <= 160)
            }
            // The setup guide saves only at its last step.
            Page::Tvs => !self.busy && self.tv.setup.is_none() && self.tv.draft.is_some(),
            Page::Scenes => {
                self.scene_ready()
                    && self.scene.draft.as_ref().is_some_and(|d| {
                        !d.name.trim().is_empty()
                            && director::scenes::resolve(
                                d,
                                &self.scene.displays,
                                &self.scene.audio,
                                &self.scene.tvs,
                            )
                            .is_ok()
                    })
            }
            // Now applies changes rather than saving them.
            Page::Current | Page::Options => false,
        }
    }
    fn draft_name(&self) -> Option<String> {
        let name = match self.page {
            Page::Audio => self.draft.as_ref().map(|d| d.name.clone()),
            Page::Displays => self.display.draft.as_ref().map(|d| d.name.clone()),
            Page::Tvs => self.tv.draft.as_ref().map(|d| d.name.clone()),
            Page::Scenes => self.scene.draft.as_ref().map(|d| d.name.clone()),
            _ => None,
        }?;
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// A category chosen in the navigation pane. Asks first when the page has unsaved
    /// changes; otherwise navigates.
    pub(crate) fn navigate_guarded(&mut self, tag: Option<String>, cx: &ComponentContext<Self>) {
        let Some(next) = tag.as_deref().and_then(crate::page_for) else {
            return;
        };
        if next == self.page || self.nav.pending.is_some() {
            return;
        }
        if self.unsaved() {
            self.nav.pending = tag;
            self.nav.saving = false;
            return;
        }
        self.navigate_now(tag, cx);
    }

    pub(crate) fn update_guard(&mut self, m: GuardMessage, cx: &ComponentContext<Self>) {
        let GuardMessage::Closed(result) = m;
        let Some(tag) = self.nav.pending.clone() else {
            return;
        };
        match result {
            ContentDialogResult::Primary if self.can_save() => {
                self.nav.saving = true;
                let save = match self.page {
                    Page::Audio => Message::Save,
                    Page::Displays => Message::Display(DisplayMessage::Save),
                    Page::Tvs => Message::Tv(crate::tv_ui::TvMessage::Save),
                    _ => Message::Scene(crate::scene_ui::SceneMessage::Save),
                };
                <Self as Component>::update(self, save, cx);
                // A save that never started (validation failed) leaves the user here.
                if !self.busy && self.nav.saving {
                    self.nav.saving = false;
                    self.nav.pending = None;
                    self.nav.revision += 1;
                }
            }
            ContentDialogResult::Secondary => {
                self.nav.pending = None;
                let discard = match self.page {
                    Page::Audio => Message::Cancel,
                    Page::Displays => Message::Display(DisplayMessage::Cancel),
                    Page::Tvs => Message::Tv(crate::tv_ui::TvMessage::Cancel),
                    Page::Scenes => Message::Scene(crate::scene_ui::SceneMessage::Cancel),
                    _ => Message::Current(crate::current_ui::CurrentMessage::Load),
                };
                <Self as Component>::update(self, discard, cx);
                // Discarding Now rereads Windows in the background; navigate right away.
                self.navigate_now(Some(tag), cx);
            }
            _ => {
                self.nav.pending = None;
                self.nav.revision += 1;
            }
        }
    }
    /// Call when a page's save finished. After Save in the dialog, continue to the chosen
    /// category, or stay here when the save failed.
    pub(crate) fn guard_saved(&mut self, ok: bool, cx: &ComponentContext<Self>) {
        if !self.nav.saving {
            return;
        }
        self.nav.saving = false;
        let tag = self.nav.pending.take();
        if ok {
            self.navigate_now(tag, cx);
        } else {
            self.nav.revision += 1;
        }
    }

    /// A save that failed keeps the user on the page with its error.
    pub(crate) fn guard_save_failed(&mut self) {
        if self.nav.saving {
            self.nav.saving = false;
            self.nav.pending = None;
            self.nav.revision += 1;
        }
    }
    pub(crate) fn guard_dialog(&self, cx: &ViewContext<Self>) -> View {
        if self.nav.pending.is_none() || self.nav.saving {
            return View::empty();
        }
        let can_save = self.can_save();
        let title = match self.draft_name() {
            Some(name) => format!("Save changes to “{name}”?"),
            None if self.page == Page::Current => "Discard your Now edits?".into(),
            None => "Save your changes?".into(),
        };
        let body = if self.page == Page::Current {
            "You changed settings in Now without applying them. Discard them, or keep editing and apply them first.".to_string()
        } else if can_save {
            format!(
                "You have unsaved changes to {}. Save them before you go, or discard them.",
                page_name(self.page)
            )
        } else {
            format!(
                "You have unsaved changes to {} that can’t be saved yet. Discard them, or keep editing.",
                page_name(self.page)
            )
        };
        ContentDialog::new()
            .is_open(true)
            .title(title)
            .primary_button_text_optional(can_save.then_some("Save"))
            .secondary_button_text("Discard")
            .close_button_text("Keep editing")
            .on_closed(cx.callback(|r| Message::Guard(GuardMessage::Closed(r))))
            .content(
                TextBlock::new()
                    .text(body)
                    .text_wrapping(TextWrapping::Wrap),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baselines_follow_the_open_draft() {
        let mut base = None;
        track(&mut base, Some(&(1, "a")), |x, y| x.0 == y.0);
        assert!(!changed(&base, Some(&(1, "a"))));
        assert!(changed(&base, Some(&(1, "b"))));
        // Opening another item takes a new baseline.
        track(&mut base, Some(&(2, "z")), |x, y| x.0 == y.0);
        assert!(!changed(&base, Some(&(2, "z"))));
        // Closing the draft clears it.
        track(&mut base, None, |x, y| x.0 == y.0);
        assert_eq!(base, None);
        assert!(!changed(&base, None::<&(i32, &str)>));
    }
}
