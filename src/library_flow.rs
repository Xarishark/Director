//! Cross-page flows between Now, Scenes and the Library.
//!
//! Now can seed a new Library preset from what it has staged, and the scene editor can create a
//! display preset, audio preset or TV in place and get back to the scene with it selected. Every
//! flow ends in the normal Library editor, so saving still only saves.
use crate::{DirectorApp, Message, Page, display_ui::DisplayMessage, scene_ui::SceneMessage, work};
use director::displays::Layout;
use windows_reactor::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    Display,
    Audio,
    Tv,
}

/// Starting values for a new audio preset taken from Now.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AudioSeed {
    pub output_id: String,
    pub volume: Option<f64>,
    /// The user changed the volume in Now, so the preset should set it.
    pub set_volume: bool,
}

#[derive(Clone)]
pub(crate) enum LibraryMessage {
    /// Open a new display preset, seeded from a staged layout when one is given.
    NewDisplay(Option<Layout>),
    NewAudio(Option<AudioSeed>),
    NewTv,
    /// Create the item for this scene section, then return to the scene with it selected.
    FromScene(Slot),
}

#[derive(Default)]
pub(crate) struct LibraryFlow {
    /// A new display preset to open once the Displays page has loaded.
    display_seed: Option<Option<Layout>>,
    audio_seed: Option<Option<AudioSeed>>,
    /// The scene section waiting for the item being created.
    scene_return: Option<Slot>,
    /// The item to select in the scene draft once the scene catalog has reloaded.
    scene_select: Option<(Slot, String)>,
}

impl LibraryFlow {
    /// A user-initiated page change abandons any pending return to the scene.
    pub(crate) fn navigated(&mut self) {
        self.scene_return = None;
    }
}

impl DirectorApp {
    fn library_go(&mut self, tag: &str, cx: &ComponentContext<Self>) {
        let before = self.page;
        <Self as Component>::update(self, Message::Navigate(Some(tag.into())), cx);
        // Entering a page reloads it (sometimes via a queued message) and its load handler
        // resumes the flow. Staying on the same page reloads nothing, so resume now.
        if self.page == before {
            self.library_resume(cx);
        }
    }
    pub(crate) fn update_library(&mut self, m: LibraryMessage, cx: &ComponentContext<Self>) {
        if self.busy {
            return;
        }
        match m {
            LibraryMessage::NewDisplay(seed) => {
                if self.display.draft.is_some() {
                    self.library_go("displays", cx);
                    self.status =
                        "Finish or cancel the display preset you’re editing first.".into();
                    return;
                }
                self.flow.display_seed = Some(seed);
                self.library_go("displays", cx);
            }
            LibraryMessage::NewAudio(seed) => {
                if self.draft.is_some() {
                    self.library_go("audio", cx);
                    self.status = "Finish or cancel the audio preset you’re editing first.".into();
                    return;
                }
                self.flow.audio_seed = Some(seed);
                self.library_go("audio", cx);
            }
            LibraryMessage::NewTv => {
                if self.tv.draft.is_some() {
                    self.library_go("tvs", cx);
                    self.status = "Finish or cancel the TV you’re setting up first.".into();
                    return;
                }
                self.tv.add_after_load = true;
                self.library_go("tvs", cx);
            }
            LibraryMessage::FromScene(slot) => {
                if self.scene.draft.is_none() {
                    return;
                }
                self.update_library(
                    match slot {
                        Slot::Display => LibraryMessage::NewDisplay(None),
                        Slot::Audio => LibraryMessage::NewAudio(None),
                        Slot::Tv => LibraryMessage::NewTv,
                    },
                    cx,
                );
                // Set after navigating: navigation clears any earlier pending return.
                self.flow.scene_return = Some(slot);
            }
        }
    }
    /// Opens a pending new preset once its Library page has finished loading.
    pub(crate) fn library_resume(&mut self, cx: &ComponentContext<Self>) {
        // Keep a pending seed until its page has finished loading; a load may still be queued.
        if self.page == Page::Displays
            && self.display_ready()
            && let Some(seed) = self.flow.display_seed.take()
        {
            match seed {
                None => self.update_display(DisplayMessage::Create, cx),
                Some(layout) => {
                    self.begin("Reading supported modes for your layout…");
                    work(cx, move || {
                        Message::Display(DisplayMessage::EditorLoaded(
                            true,
                            director::windows_display_modes::discover(&layout)
                                .map(|options| (layout, options)),
                        ))
                    });
                }
            }
        }
        if self.page == Page::Audio
            && self.ready()
            && let Some(seed) = self.flow.audio_seed.take()
        {
            <Self as Component>::update(self, Message::Add, cx);
            if let (Some(seed), Some(d)) = (seed, &mut self.draft) {
                if self.outputs.iter().any(|o| o.id == seed.output_id) {
                    d.output_id = seed.output_id;
                }
                if seed.volume.is_some() {
                    d.volume = seed.volume;
                }
                d.set_volume = seed.set_volume;
                self.status = "Name this audio preset to save what’s set in Now.".into();
            }
        }
    }
    /// Call after a Library save succeeds, with the item it added (if any).
    pub(crate) fn library_saved(
        &mut self,
        slot: Slot,
        new_id: Option<String>,
        cx: &ComponentContext<Self>,
    ) {
        if self.flow.scene_return == Some(slot)
            && let Some(id) = new_id
        {
            self.flow.scene_return = None;
            self.flow.scene_select = Some((slot, id));
            self.library_go("scenes", cx);
        }
    }
    pub(crate) fn library_cancelled(&mut self, slot: Slot, cx: &ComponentContext<Self>) {
        if self.flow.scene_return == Some(slot) {
            self.flow.scene_return = None;
            self.library_go("scenes", cx);
        }
    }
    /// Selects a just-created item in the scene draft after the scene catalog reloads.
    pub(crate) fn library_select_in_scene(&mut self, cx: &ComponentContext<Self>) {
        let Some((slot, id)) = self.flow.scene_select.take() else {
            return;
        };
        if self.scene.draft.is_none() {
            return;
        }
        let index = match slot {
            Slot::Display => self.scene.displays.iter().position(|p| p.id == id),
            Slot::Audio => self.scene.audio.iter().position(|p| p.id == id),
            Slot::Tv => self.scene.tvs.iter().position(|t| t.id == id),
        };
        if let Some(i) = index.map(|i| Some(i + 1)) {
            self.update_scene(
                match slot {
                    Slot::Display => SceneMessage::Display(i),
                    Slot::Audio => SceneMessage::Audio(i),
                    Slot::Tv => SceneMessage::Tv(i),
                },
                cx,
            );
        }
    }
}

/// The ID present in `after` but not in `before`, i.e. the item a save just added.
pub(crate) fn added_id<'a>(
    before: impl IntoIterator<Item = &'a str>,
    after: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let before = before.into_iter().collect::<std::collections::HashSet<_>>();
    after
        .into_iter()
        .find(|id| !before.contains(id))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn added_id_finds_only_the_new_item() {
        assert_eq!(added_id(["a", "b"], ["a", "c", "b"]), Some("c".into()));
        assert_eq!(added_id(["a", "b"], ["a", "b"]), None);
        assert_eq!(added_id(["a", "b"], ["a"]), None);
    }
    #[test]
    fn navigating_clears_a_pending_scene_return() {
        let mut flow = LibraryFlow {
            scene_return: Some(Slot::Audio),
            ..Default::default()
        };
        flow.navigated();
        assert_eq!(flow.scene_return, None);
    }
}
