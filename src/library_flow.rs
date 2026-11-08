//! Cross-page flows between Now, Scenes and the Library.
//!
//! Now can seed a new display preset from what it has staged, and the scene editor can create a
//! display preset or TV in place and get back to the scene with it selected. Every flow ends in
//! the normal Library editor, so saving still only saves.
use crate::{DirectorApp, Message, Page, display_ui::DisplayMessage, scene_ui::SceneMessage, work};
use director::display::displays::Layout;
use windows_reactor::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    Display,
    Tv,
}

#[derive(Clone)]
pub(crate) enum LibraryMessage {
    /// Open a new display preset, seeded from a staged layout when one is given.
    NewDisplay(Option<Layout>),
    NewTv,
    /// Open a new scene draft, for example one whose Sound step holds what's set in Now.
    NewScene(Box<director::scene::scenes::Scene>),
    /// Create the item for this scene section, then return to the scene with it selected.
    FromScene(Slot),
}

#[derive(Default)]
pub(crate) struct LibraryFlow {
    /// A new display preset to open once the Displays page has loaded.
    display_seed: Option<Option<Layout>>,
    /// A new scene draft to open once Scenes has loaded.
    scene_seed: Option<director::scene::scenes::Scene>,
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
        self.navigate_now(Some(tag.into()), cx);
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
                    self.warn(
                        "Finish or cancel the display preset you’re editing first.",
                        "",
                    );
                    return;
                }
                self.flow.display_seed = Some(seed);
                self.library_go("displays", cx);
            }
            LibraryMessage::NewTv => {
                if self.tv.draft.is_some() {
                    self.library_go("tvs", cx);
                    self.warn("Finish or cancel the TV you’re setting up first.", "");
                    return;
                }
                self.tv.add_after_load = true;
                self.library_go("tvs", cx);
            }
            LibraryMessage::NewScene(scene) => {
                if self.scene.draft.is_some() {
                    self.library_go("scenes", cx);
                    self.warn("Finish or cancel the scene you’re editing first.", "");
                    return;
                }
                self.flow.scene_seed = Some(*scene);
                self.library_go("scenes", cx);
            }
            LibraryMessage::FromScene(slot) => {
                if self.scene.draft.is_none() {
                    return;
                }
                self.update_library(
                    match slot {
                        Slot::Display => LibraryMessage::NewDisplay(None),
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
                            director::display::windows_display_modes::discover(&layout)
                                .map(|options| (layout, options)),
                        ))
                    });
                }
            }
        }
        if self.page == Page::Scenes
            && self.scene_ready()
            && self.scene.draft.is_none()
            && let Some(scene) = self.flow.scene_seed.take()
        {
            // A scene seeded from Now holds only its sound: open that step so it can be
            // checked and adjusted straight away. Cancel keeps the seeded sound.
            let sound = scene.sound.is_some();
            self.scene.snapshot = sound.then(|| scene.clone());
            self.scene.draft = Some(scene);
            self.scene.open = sound.then_some(crate::scene_editor_ui::Step::Sound);
            self.status = "Name this scene to save the sound set in Now.".into();
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
            Slot::Tv => self.scene.tvs.iter().position(|t| t.id == id),
        };
        if let Some(i) = index.map(|i| Some(i + 1)) {
            self.update_scene(
                match slot {
                    Slot::Display => SceneMessage::Display(i),
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
            scene_return: Some(Slot::Tv),
            ..Default::default()
        };
        flow.navigated();
        assert_eq!(flow.scene_return, None);
    }
}
