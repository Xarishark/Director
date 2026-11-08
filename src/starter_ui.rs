//! The Scenes page's first run: one card that saves how the PC is set up right now as the
//! everyday scene, and afterwards a tip about making the next one. See `director::scene::starter`.
use crate::{DirectorApp, Message, scene_ui::SceneMessage, work};
use director::{
    audio::{AudioBackend, Output},
    display::displays::Layout,
    scene::starter,
};
use windows_reactor::*;

const DEFAULT_NAME: &str = "Default";

#[derive(Clone)]
pub(crate) enum StarterMessage {
    /// Read the current screens and sound (FPS comes from the Scenes page's FPS survey).
    Probe,
    Probed(Result<Layout, String>, Result<Option<Output>, String>),
    Name(String),
    Screens(bool),
    Sound(bool),
    Fps(bool),
    Save,
    /// The saved scene's name.
    Saved(Result<String, String>),
}

pub(crate) struct StarterState {
    probing: bool,
    layout: Option<Result<Layout, String>>,
    output: Option<Result<Option<Output>, String>>,
    name: String,
    screens: bool,
    sound: bool,
    fps: bool,
    /// The scene just saved, announced once the scene list has reloaded.
    pub(crate) saved: Option<String>,
}
impl Default for StarterState {
    fn default() -> Self {
        Self {
            probing: false,
            layout: None,
            output: None,
            name: DEFAULT_NAME.into(),
            screens: true,
            sound: true,
            fps: true,
            saved: None,
        }
    }
}

fn msg(m: StarterMessage) -> Message {
    Message::Scene(SceneMessage::Starter(m))
}
use crate::ui::note;

impl DirectorApp {
    /// The current FPS limit to keep, from the FPS survey the Scenes page already runs.
    fn starter_fps(&self) -> Option<director::fps::Request> {
        let current = self
            .fps
            .caps
            .iter()
            .filter(|c| c.available)
            .filter_map(|c| c.current.clone())
            .collect::<Vec<_>>();
        starter::fps_request(&current)
    }
    fn starter_choice(&self) -> starter::Choice {
        let s = &self.scene.starter;
        starter::Choice {
            name: s.name.trim().to_string(),
            layout: s
                .layout
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .filter(|_| s.screens)
                .cloned(),
            output: s
                .output
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .and_then(Option::as_ref)
                .filter(|_| s.sound)
                .cloned(),
            fps: self.starter_fps().filter(|_| s.fps),
        }
    }
    fn starter_name_problem(&self) -> Option<&'static str> {
        let name = self.scene.starter.name.trim();
        if name.is_empty() {
            Some("Give the scene a name.")
        } else if name.chars().count() > 160 {
            Some("Use a shorter name.")
        } else if self
            .scene
            .scenes
            .iter()
            .any(|s| s.name.trim().eq_ignore_ascii_case(name))
        {
            Some("A scene with this name already exists.")
        } else {
            None
        }
    }

    pub(crate) fn update_starter(&mut self, m: StarterMessage, cx: &ComponentContext<Self>) {
        match m {
            StarterMessage::Probe if !self.scene.starter.probing => {
                self.scene.starter.probing = true;
                work(cx, || {
                    let layout = director::display::windows_displays::capture();
                    let output = director::audio::windows_audio::WindowsAudio::new()
                        .and_then(|mut a| a.outputs())
                        .map(|list| starter::default_output(&list).cloned());
                    msg(StarterMessage::Probed(layout, output))
                });
            }
            StarterMessage::Probed(layout, output) => {
                let s = &mut self.scene.starter;
                s.probing = false;
                s.screens = layout.is_ok();
                s.sound = matches!(output, Ok(Some(_)));
                s.layout = Some(layout);
                s.output = Some(output);
            }
            StarterMessage::Name(name) => self.scene.starter.name = name,
            StarterMessage::Screens(on) => self.scene.starter.screens = on,
            StarterMessage::Sound(on) => self.scene.starter.sound = on,
            StarterMessage::Fps(on) => self.scene.starter.fps = on,
            StarterMessage::Save if self.scene_ready() && self.starter_name_problem().is_none() => {
                let choice = self.starter_choice();
                self.begin("Saving your scene…");
                work(cx, move || {
                    msg(StarterMessage::Saved(
                        director::scene::starter::save(&choice).map(|scene| scene.name),
                    ))
                });
            }
            StarterMessage::Saved(result) => {
                self.end();
                match result {
                    Ok(name) => {
                        self.scene.starter = StarterState {
                            saved: Some(name),
                            ..Default::default()
                        };
                        // Reload scenes and presets; the notice follows once they're in.
                        self.update_scene(SceneMessage::Load, cx);
                    }
                    Err(e) => self.failure(e),
                }
            }
            _ => {}
        }
    }

    /// After a reload: announce a just-saved starter scene, or start probing for the card.
    pub(crate) fn starter_loaded(&mut self, cx: &ComponentContext<Self>) {
        if let Some(name) = self.scene.starter.saved.take() {
            self.error = false;
            self.status = format!("Saved “{name}”. Apply it any time to come back to this setup.");
            self.details.clear();
            self.announce(cx);
        }
        self.starter_probe_if_needed(cx);
    }
    /// Read the current setup when the card is about to show: the list is empty (also after
    /// deleting the last scene) and nothing has been read yet.
    pub(crate) fn starter_probe_if_needed(&mut self, cx: &ComponentContext<Self>) {
        if self.scene.ready && self.scene.scenes.is_empty() && self.scene.starter.layout.is_none() {
            self.update_starter(StarterMessage::Probe, cx);
        }
    }

    /// One row of "What to save": a checkbox and what it would save.
    fn starter_row(
        &self,
        cx: &ViewContext<Self>,
        label: &str,
        checked: bool,
        detail: Result<String, String>,
        toggle: fn(bool) -> StarterMessage,
    ) -> View {
        let available = detail.is_ok();
        let text = detail.unwrap_or_else(|e| e);
        Grid::new()
            .columns([GridLength::Pixel(140.0), GridLength::Star(1.0)])
            .column_spacing(8.0)
            .children((
                CheckBox::new()
                    .automation_name(format!("Save {}", label.to_lowercase()))
                    .is_checked(Some(checked && available))
                    .is_enabled(available && !self.busy)
                    .on_is_checked_changed(cx.callback(move |v| msg(toggle(v))))
                    .content(label),
                TextBlock::new()
                    .grid_column(1)
                    .text(text)
                    .opacity(if checked && available { 0.85 } else { 0.5 })
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    .vertical_alignment(VerticalAlignment::Center),
            ))
    }

    /// The empty Scenes page: save the setup as it is now as the everyday scene.
    pub(crate) fn starter_card(&self, cx: &ViewContext<Self>) -> View {
        let s = &self.scene.starter;
        let checking = |what: &str| Err(format!("Checking your {what}…"));
        let screens = match &s.layout {
            None => checking("screens"),
            Some(Ok(layout)) => Ok(starter::screens_summary(layout)),
            Some(Err(_)) => Err("Couldn’t read your screens".into()),
        };
        let sound = match &s.output {
            None => checking("sound"),
            Some(Ok(Some(o))) => Ok(match o.volume_percent {
                Some(v) => format!("{} · {v}%", o.name),
                None => o.name.clone(),
            }),
            Some(Ok(None)) => Err("No active output".into()),
            Some(Err(_)) => Err("Couldn’t read your sound".into()),
        };
        let fps = match self.starter_fps() {
            Some(r) => Ok(match r.limit {
                Some(n) => format!("{} · {n} FPS", r.provider.label()),
                None => format!("{} · no global limit", r.provider.label()),
            }),
            None if !self.fps.loaded => checking("FPS limit"),
            None => Err("No NVIDIA or RTSS limit found".into()),
        };
        let choice = self.starter_choice();
        let something = choice.layout.is_some() || choice.output.is_some() || choice.fps.is_some();
        let problem = self.starter_name_problem();
        let name = s.name.trim();
        let label = if name.is_empty() || problem.is_some() {
            "Save as my everyday scene".to_string()
        } else {
            format!("Save as my {name} scene")
        };
        Border::new()
            .max_width(650.0)
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Top)
            .padding(24.0)
            .corner_radius(crate::ui::RADIUS_L)
            .border_thickness(1.0)
            .border_brush(crate::ui::card_stroke())
            .background(crate::ui::card_fill())
            .automation_name("Start with your everyday setup")
            .content(
                StackPanel::new().spacing(18.0).children((
                    StackPanel::new().spacing(6.0).children((
                        TextBlock::new()
                            .text("Start with your everyday setup")
                            .font_size(crate::ui::SUBTITLE)
                            .font_weight(FontWeight::SEMI_BOLD),
                        note("Save how your PC is set up right now as a scene. Whenever another scene changes your screens or sound, this one brings everything back in one click."),
                    )),
                    StackPanel::new().spacing(4.0).children((
                        TextBox::new()
                            .automation_name("Everyday scene name")
                            .max_width(320.0)
                            .horizontal_alignment(HorizontalAlignment::Left)
                            .text(s.name.clone())
                            .is_enabled(!self.busy)
                            .on_text_changed(cx.callback(|t| msg(StarterMessage::Name(t))))
                            .slot(TextBoxSlot::Header, "Scene name"),
                        match problem {
                            Some(p) if !name.is_empty() || !s.name.is_empty() => TextBlock::new()
                                .text(p)
                                .font_size(crate::ui::CAPTION)
                                .foreground(crate::ui::critical())
                                .into(),
                            _ => View::empty(),
                        },
                    )),
                    StackPanel::new().spacing(6.0).children((
                        TextBlock::new()
                            .text("What to save")
                            .font_weight(FontWeight::SEMI_BOLD),
                        self.starter_row(cx, "Screens", s.screens, screens, StarterMessage::Screens),
                        self.starter_row(cx, "Sound", s.sound, sound, StarterMessage::Sound),
                        self.starter_row(cx, "FPS limit", s.fps, fps, StarterMessage::Fps),
                    )),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(16.0)
                        .children((
                            Button::new()
                                .style(ButtonStyle::Accent)
                                .automation_name("Save everyday scene")
                                .is_enabled(
                                    self.scene_ready()
                                        && !s.probing
                                        && s.layout.is_some()
                                        && s.output.is_some()
                                        && problem.is_none()
                                        && something,
                                )
                                .on_click(cx.message(msg(StarterMessage::Save)))
                                .content(label),
                            TextBlock::new()
                                .text("Saving doesn’t change anything on your PC.")
                                .font_size(crate::ui::CAPTION)
                                .opacity(crate::ui::SECONDARY)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                    HyperlinkButton::new()
                        .automation_name("Build a scene from scratch instead")
                        .is_enabled(self.scene_ready())
                        .on_click(cx.message(Message::Scene(SceneMessage::Add)))
                        .content("Build a scene from scratch instead"),
                )),
            )
    }

    /// Above the cards while there's only one scene: how to make the next.
    pub(crate) fn next_scene_tip(&self, cx: &ViewContext<Self>) -> View {
        if self.scene.scenes.len() != 1
            || self
                .tip
                .pending
                .as_ref()
                .or(self.tip.preferences.as_ref())
                .is_none_or(|p| p.hide_next_scene_tip)
        {
            return View::empty();
        }
        Border::new()
            .padding(Thickness::xy(16.0, 12.0))
            .margin(Thickness::new(0.0, 0.0, 0.0, 16.0))
            .corner_radius(crate::ui::RADIUS_L)
            .border_thickness(1.0)
            .border_brush(crate::ui::card_stroke())
            .background(crate::ui::card_fill())
            .automation_name("Next scene tip")
            .content(
                Grid::new()
                    .columns([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
                    .column_spacing(12.0)
                    .children((
                        FontIcon::new()
                            .glyph("\u{e946}")
                            .opacity(crate::ui::SECONDARY)
                            .vertical_alignment(VerticalAlignment::Center),
                        TextBlock::new()
                            .grid_column(1)
                            .text("Next, make a scene for something else, like gaming on the TV. Press + and add steps: each step can save a new screen layout, sound or TV right there.")
                            .text_wrapping(TextWrapping::Wrap)
                            .opacity(0.85)
                            .vertical_alignment(VerticalAlignment::Center),
                        Button::new()
                            .grid_column(2)
                            .automation_name("Hide next scene tip")
                            .is_enabled(!self.busy && self.tip.pending.is_none())
                            .vertical_alignment(VerticalAlignment::Center)
                            .on_click(cx.message(Message::Tip(
                                crate::tv_tip_ui::TipMessage::HideNextSceneTip,
                            )))
                            .content("Got it"),
                    )),
            )
    }
}
