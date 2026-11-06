use crate::{DirectorApp, Message, work};
use director::{
    audio::Output,
    scenes, tvs,
    windows_audio::{OperationLock, WindowsAudio},
};
use windows_reactor::*;

#[derive(Clone)]
pub(crate) enum RelinkMessage {
    Open {
        input: bool,
        id: String,
        name: String,
    },
    Loaded(Result<RelinkDraft, String>),
    Selected(Option<usize>),
    Closed(ContentDialogResult),
    Saved(Result<(bool, String, String), String>),
}

#[derive(Clone)]
pub(crate) struct RelinkDraft {
    input: bool,
    id: String,
    name: String,
    candidates: Vec<Output>,
    selected: Option<usize>,
    affected: Vec<String>,
}

pub(crate) fn device_label(device: &Output) -> String {
    if device.active {
        device.name.clone()
    } else if device.state == "Reconnected" {
        format!("{} · Reconnected", device.name)
    } else {
        format!("{} · Offline", device.name)
    }
}

impl DirectorApp {
    pub(crate) fn audio_relink_button(
        &self,
        cx: &ViewContext<Self>,
        id: &str,
        fallback: Option<&str>,
        input: bool,
    ) -> View {
        let devices = if input { &self.inputs } else { &self.outputs };
        let device = devices.iter().find(|d| d.id == id);
        if id.is_empty()
            || device.is_some_and(|d| {
                !matches!(
                    d.state.as_str(),
                    "Remembered · offline" | "Remembered · unavailable" | "Unavailable"
                )
            })
        {
            return View::empty();
        }
        let name = device
            .map(|d| d.name.as_str())
            .or(fallback)
            .unwrap_or("Saved audio device");
        Button::new()
            .is_enabled(!self.busy)
            .on_click(cx.message(Message::AudioRelink(RelinkMessage::Open {
                input,
                id: id.into(),
                name: name.into(),
            })))
            .content("Relink device…")
    }

    pub(crate) fn update_audio_relink(
        &mut self,
        message: RelinkMessage,
        cx: &ComponentContext<Self>,
    ) {
        match message {
            RelinkMessage::Open { input, id, name } if !self.busy => {
                self.begin("Looking for replacement audio devices…");
                work(cx, move || {
                    Message::AudioRelink(RelinkMessage::Loaded((|| {
                        let mut audio = WindowsAudio::new()?;
                        let mut candidates = audio.picker_devices(input)?;
                        candidates.retain(|d| d.active && d.id != id);
                        candidates.sort_by_key(|d| d.name.to_lowercase() != name.to_lowercase());
                        if candidates.is_empty() {
                            return Err("Connect a replacement audio device first.".into());
                        }
                        let matches = |saved: &str| audio.references_audio_id(saved, &id, input);
                        let mut affected = Vec::new();
                        for scene in scenes::load(&scenes::path()?)? {
                            let Some(sound) = &scene.sound else { continue };
                            let uses_device = if input {
                                sound.mic.as_ref().is_some_and(|m| matches(&m.device.id))
                            } else {
                                matches!(&sound.output, Some(scenes::SoundOutput::Device { id, .. }) if matches(id))
                                    || sound.calls.as_ref().is_some_and(|d| matches(&d.id))
                            };
                            if uses_device {
                                affected.push(format!("Scene: {}", scene.name));
                            }
                        }
                        if !input {
                            for tv in tvs::load(&tvs::path()?)? {
                                if tv.audio.as_ref().is_some_and(|d| matches(&d.id)) {
                                    affected.push(format!("TV: {}", tv.name));
                                }
                            }
                        }
                        Ok(RelinkDraft {
                            input,
                            id,
                            name,
                            candidates,
                            selected: None,
                            affected,
                        })
                    })()))
                });
            }
            RelinkMessage::Loaded(result) => {
                self.end();
                match result {
                    Ok(draft) => {
                        self.audio_relink = Some(draft);
                        self.status = "Choose a replacement audio device.".into();
                    }
                    Err(error) => self.failure(error),
                }
            }
            RelinkMessage::Selected(index) => {
                if let Some(draft) = &mut self.audio_relink {
                    draft.selected = index;
                }
            }
            RelinkMessage::Closed(result) => {
                if let Some(draft) = self.audio_relink.take()
                    && result == ContentDialogResult::Primary
                    && let Some(replacement) = draft
                        .selected
                        .and_then(|i| draft.candidates.get(i))
                        .cloned()
                {
                    self.begin("Saving the replacement device…");
                    work(cx, move || {
                        Message::AudioRelink(RelinkMessage::Saved((|| {
                            let _lock = OperationLock::acquire()?;
                            WindowsAudio::new()?.relink_device(
                                draft.input,
                                draft.id.clone(),
                                draft.name,
                                &replacement.id,
                            )?;
                            Ok((draft.input, draft.id, replacement.name))
                        })()))
                    });
                }
            }
            RelinkMessage::Saved(result) => {
                self.end();
                match result {
                    Ok((input, id, name)) => {
                        self.status = format!("Relinked to {name}.");
                        self.error = false;
                        self.details.clear();
                        self.announce(cx);
                        let devices = if input {
                            &mut self.inputs
                        } else {
                            &mut self.outputs
                        };
                        if let Some(device) = devices.iter_mut().find(|device| device.id == id) {
                            device.state = "Reconnected".into();
                        }
                    }
                    Err(error) => self.failure(error),
                }
            }
            _ => {}
        }
    }

    pub(crate) fn audio_relink_dialog(&self, cx: &ViewContext<Self>) -> View {
        let Some(draft) = &self.audio_relink else {
            return View::empty();
        };
        let preview = if draft.affected.is_empty() {
            "This saved device reference will use the replacement.".into()
        } else {
            draft.affected.join("\n")
        };
        ContentDialog::new()
            .is_open(true)
            .title(format!("Relink {}", draft.name))
            .primary_button_text("Relink")
            .close_button_text("Cancel")
            .is_primary_button_enabled(draft.selected.is_some())
            .on_closed(cx.callback(|r| Message::AudioRelink(RelinkMessage::Closed(r))))
            .content(
                StackPanel::new().spacing(12.0).children((
                    ComboBox::new()
                        .automation_name("Replacement audio device")
                        .placeholder_text("Choose a replacement")
                        .items_source(draft.candidates.iter().map(|d| {
                            if draft
                                .candidates
                                .iter()
                                .filter(|other| other.name.to_lowercase() == d.name.to_lowercase())
                                .count()
                                > 1
                            {
                                format!("{} · {}", d.name, d.id)
                            } else {
                                d.name.clone()
                            }
                        }))
                        .selected_index(draft.selected)
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .on_selection_changed(
                            cx.callback(|i| Message::AudioRelink(RelinkMessage::Selected(i))),
                        ),
                    TextBlock::new()
                        .text("Saved references affected")
                        .font_weight(FontWeight::SEMI_BOLD),
                    ScrollViewer::new().max_height(160.0).content(
                        TextBlock::new()
                            .text(preview)
                            .text_wrapping(TextWrapping::Wrap),
                    ),
                )),
            )
    }
}
