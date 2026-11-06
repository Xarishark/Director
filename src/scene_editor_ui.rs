//! The scene editor as a vertical flow of step blocks, in the order a scene runs.
//!
//! Each block shows a one-line summary and expands in place to edit that step. Unused steps
//! are muted "+ Add" slots. When leaving runs at a different time, so it sits apart from the
//! line.
use crate::{DirectorApp, Message, scene_ui::SceneMessage};
use director::scenes::{self, Action, ActionList, Scene};
use windows_reactor::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Before,
    Tv,
    Displays,
    Audio,
    Fps,
    After,
    Leave,
}
impl Step {
    fn title(self) -> &'static str {
        match self {
            Self::Before => "Before",
            Self::Tv => "Prepare TV",
            Self::Displays => "Displays",
            Self::Audio => "Audio",
            Self::Fps => "FPS limit",
            Self::After => "After",
            Self::Leave => "When leaving",
        }
    }
    fn glyph(self) -> &'static str {
        match self {
            Self::Before => "\u{e756}",
            Self::Tv => "\u{e7f4}",
            Self::Displays => "\u{e7f8}",
            Self::Audio => "\u{e767}",
            Self::Fps => "\u{e916}",
            Self::After => "\u{e768}",
            Self::Leave => "\u{e7e8}",
        }
    }
    /// What an unused step offers to add.
    fn add_label(self) -> &'static str {
        match self {
            Self::Before => "Add something to run first",
            Self::Tv => "Prepare a TV",
            Self::Displays => "Switch displays",
            Self::Audio => "Switch audio",
            Self::Fps => "Set an FPS limit",
            Self::After => "Open apps or links afterwards",
            Self::Leave => "Do something when leaving",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::Tv => "tv",
            Self::Displays => "displays",
            Self::Audio => "audio",
            Self::Fps => "fps",
            Self::After => "after",
            Self::Leave => "leave",
        }
    }
}

fn msg(m: SceneMessage) -> Message {
    Message::Scene(m)
}
fn note(s: impl Into<String>) -> View {
    TextBlock::new()
        .text(s)
        .text_wrapping(TextWrapping::Wrap)
        .opacity(0.72)
        .into()
}
const BORDER: Color = Color::rgb(70, 70, 70);
/// The width of the flow column and the left offset of the connector line.
const FLOW_WIDTH: f64 = 720.0;
const LINE_INSET: f64 = 31.0;

fn connector() -> View {
    Border::new()
        .width(2.0)
        .height(18.0)
        .margin(Thickness::new(LINE_INSET, 0.0, 0.0, 0.0))
        .horizontal_alignment(HorizontalAlignment::Left)
        .background(BORDER)
        .into()
}

fn actions_summary(actions: &[Action]) -> String {
    match actions {
        [] => String::new(),
        [one] => one.label(),
        [first, rest @ ..] => format!("{} · and {} more", first.label(), rest.len()),
    }
}

impl DirectorApp {
    fn step_used(&self, d: &Scene, step: Step) -> bool {
        match step {
            Step::Before => !d.before.is_empty(),
            Step::Tv => d.tv.is_some(),
            Step::Displays => d.display.is_some(),
            Step::Audio => d.audio.is_some() || d.tv_audio.is_some(),
            Step::Fps => d.fps.is_some(),
            Step::After => !d.actions.is_empty(),
            Step::Leave => !d.leave.is_empty(),
        }
    }
    fn step_summary(&self, d: &Scene, step: Step) -> String {
        let s = &self.scene;
        match step {
            Step::Before => actions_summary(&d.before),
            Step::After => actions_summary(&d.actions),
            Step::Leave => actions_summary(&d.leave),
            Step::Tv => d.tv.as_ref().map_or(String::new(), |prep| {
                match s.tvs.iter().find(|t| t.id == prep.id) {
                    None => "Missing TV".into(),
                    Some(tv) => {
                        let mut parts = vec![tv.name.clone()];
                        if prep.wake {
                            parts.push("wake if needed".into());
                        }
                        if let Some(input) = prep.input.as_ref().and_then(|id| {
                            tv.control_inputs().iter().find(|i| &i.id == id).cloned()
                        }) {
                            parts.push(input.label);
                        }
                        parts.join(" · ")
                    }
                }
            }),
            Step::Displays => d.display.as_ref().map_or(String::new(), |id| {
                s.displays.iter().find(|p| &p.id == id).map_or(
                    "Missing display preset".into(),
                    |p| {
                        let on = p.layout.monitors.iter().filter(|m| m.enabled).count();
                        format!("{} · {on} screen{}", p.name, if on == 1 { "" } else { "s" })
                    },
                )
            }),
            Step::Audio => {
                if let Some(id) = &d.audio {
                    s.audio
                        .iter()
                        .find(|p| &p.id == id)
                        .map_or("Missing audio preset".into(), |p| {
                            format!("{} · {}", p.name, p.output_name)
                        })
                } else if let Some(id) = &d.tv_audio {
                    s.tvs
                        .iter()
                        .find(|t| &t.id == id)
                        .and_then(|t| t.audio.as_ref().map(|a| (t, a)))
                        .map_or("Missing TV audio".into(), |(t, a)| {
                            format!("{} linked audio · {}", t.name, a.name)
                        })
                } else {
                    String::new()
                }
            }
            Step::Fps => d.fps.as_ref().map_or(String::new(), |r| r.label()),
        }
    }

    /// One step: a collapsed summary header, its editor when open, or a muted "+ Add" slot.
    fn step_block(&self, cx: &ViewContext<Self>, d: &Scene, step: Step) -> View {
        let available = !self.busy;
        let open = self.scene.open == Some(step);
        let used = self.step_used(d, step);
        let toggle = if open { None } else { Some(step) };
        if !used && !open {
            return Button::new()
                .automation_name(format!("Add {}", step.title()))
                .style(ButtonStyle::Subtle)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .horizontal_content_alignment(HorizontalAlignment::Left)
                .is_enabled(available)
                .on_click(cx.message(msg(SceneMessage::Open(Some(step)))))
                .content(
                    Border::new().padding(Thickness::xy(6.0, 4.0)).content(
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(12.0)
                            .opacity(0.65)
                            .children((
                                FontIcon::new().glyph("\u{e710}"),
                                TextBlock::new().text(format!(
                                    "{} · {}",
                                    step.title(),
                                    step.add_label()
                                )),
                            )),
                    ),
                );
        }
        let summary = self.step_summary(d, step);
        let header = Button::new()
            .automation_name(format!(
                "{} {}",
                if open { "Collapse" } else { "Edit" },
                step.title()
            ))
            .style(ButtonStyle::Subtle)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Stretch)
            .on_click(cx.message(msg(SceneMessage::Open(toggle))))
            .content(
                Border::new().padding(Thickness::xy(6.0, 4.0)).content(
                    Grid::new()
                        .columns([
                            GridLength::Auto,
                            GridLength::Star(1.0),
                            GridLength::Auto,
                            GridLength::Auto,
                        ])
                        .column_spacing(12.0)
                        .children((
                            FontIcon::new()
                                .grid_column(0)
                                .glyph(step.glyph())
                                .vertical_alignment(VerticalAlignment::Center),
                            StackPanel::new().grid_column(1).spacing(2.0).children((
                                TextBlock::new()
                                    .text(step.title())
                                    .font_weight(FontWeight::SEMI_BOLD),
                                if summary.is_empty() {
                                    View::empty()
                                } else {
                                    TextBlock::new()
                                        .text(summary)
                                        .opacity(0.72)
                                        .text_trimming(TextTrimming::CharacterEllipsis)
                                        .into()
                                },
                            )),
                            Border::new()
                                .grid_column(2)
                                .vertical_alignment(VerticalAlignment::Center)
                                .content(View::empty()),
                            FontIcon::new()
                                .grid_column(3)
                                .glyph(if open { "\u{e70e}" } else { "\u{e70d}" })
                                .opacity(0.72)
                                .vertical_alignment(VerticalAlignment::Center),
                        )),
                ),
            );
        let remove = if used {
            crate::preset_cards_ui::card_icon_button(Button::new())
                .automation_name(format!("Remove {}", step.title()))
                .is_enabled(available)
                .vertical_alignment(VerticalAlignment::Top)
                .margin(Thickness::new(0.0, 6.0, 6.0, 0.0))
                .on_click(cx.message(msg(SceneMessage::ClearStep(step))))
                .content(crate::icons::delete_small())
                .tooltip(format!("Remove {} from this scene", step.title()))
        } else {
            View::empty()
        };
        Border::new()
            .corner_radius(8.0)
            .border_thickness(1.0)
            .border_brush(BORDER)
            .content(
                StackPanel::new().children((
                    Grid::new()
                        .columns([GridLength::Star(1.0), GridLength::Auto])
                        .children((
                            Border::new().grid_column(0).content(header),
                            Border::new().grid_column(1).content(remove),
                        )),
                    if open {
                        Border::new()
                            .padding(Thickness::new(16.0, 4.0, 16.0, 16.0))
                            .content(self.step_editor(cx, d, step))
                    } else {
                        View::empty()
                    },
                )),
            )
    }

    fn step_editor(&self, cx: &ViewContext<Self>, d: &Scene, step: Step) -> View {
        let available = !self.busy;
        match step {
            Step::Before => StackPanel::new()
                .spacing(12.0)
                .children((
                    note("Runs first, before any device changes, for example a setup script. Turn on Wait so the scene continues only after it finishes successfully."),
                    self.action_list(cx, d, ActionList::Before),
                )),
            Step::After => StackPanel::new()
                .spacing(12.0)
                .children((
                    note("Runs in order once every step above has succeeded."),
                    self.action_list(cx, d, ActionList::After),
                )),
            Step::Leave => StackPanel::new()
                .spacing(12.0)
                .children((
                    note("Runs when you apply another scene, or press End scene. For Steam Big Picture, add the link steam://close/bigpicture."),
                    self.action_list(cx, d, ActionList::Leave),
                )),
            Step::Tv => {
                let tv = d
                    .tv
                    .as_ref()
                    .and_then(|p| self.scene.tvs.iter().find(|t| t.id == p.id));
                let labels = std::iter::once("No TV preparation".into())
                    .chain(self.scene.tvs.iter().map(|t| t.name.clone()))
                    .chain(std::iter::once("Add TV…".into()))
                    .collect::<Vec<String>>();
                StackPanel::new()
                    .spacing(10.0)
                    .children((
                        note("Wake your TV and switch it to the PC input before the displays change."),
                        ComboBox::new()
                            .automation_name("Scene TV")
                            .items_source(labels)
                            .selected_index(Some(
                                d.tv.as_ref()
                                    .and_then(|p| self.scene.tvs.iter().position(|t| t.id == p.id))
                                    .map_or(0, |i| i + 1),
                            ))
                            .is_enabled(available)
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .on_selection_changed(cx.callback(|i| msg(SceneMessage::Tv(i)))),
                        if let (Some(tv), Some(prep)) = (tv, &d.tv) {
                            StackPanel::new()
                                .spacing(10.0)
                                .children((
                                    if tv.can_wake() {
                                        ToggleSwitch::new()
                                            .automation_name("Wake TV if needed")
                                            .is_on(prep.wake)
                                            .is_enabled(available)
                                            .on_toggled(cx.callback(|v| msg(SceneMessage::Wake(v))))
                                            .slot(ToggleSwitchSlot::Header, "Wake TV if needed")
                                    } else {
                                        View::empty()
                                    },
                                    if tv.brand == director::tvs::Brand::Lg
                                        || !tv.control_inputs().is_empty()
                                    {
                                        ComboBox::new()
                                            .automation_name("Scene TV input")
                                            .items_source(
                                                std::iter::once("Leave input unchanged".into())
                                                    .chain(
                                                        tv.control_inputs()
                                                            .iter()
                                                            .map(|i| i.label.clone()),
                                                    )
                                                    .collect::<Vec<String>>(),
                                            )
                                            .selected_index(Some(
                                                prep.input
                                                    .as_ref()
                                                    .and_then(|id| {
                                                        tv.control_inputs()
                                                            .iter()
                                                            .position(|i| &i.id == id)
                                                    })
                                                    .map_or(0, |i| i + 1),
                                            ))
                                            .is_enabled(available)
                                            .horizontal_alignment(HorizontalAlignment::Stretch)
                                            .on_selection_changed(
                                                cx.callback(|i| msg(SceneMessage::Input(i))),
                                            )
                                            .into()
                                    } else {
                                        View::empty()
                                    },
                                ))
                        } else {
                            View::empty()
                        },
                    ))
            }
            Step::Displays => {
                let labels = std::iter::once("Leave displays unchanged".into())
                    .chain(self.scene.displays.iter().map(|p| p.name.clone()))
                    .chain(std::iter::once("New display preset…".into()))
                    .collect::<Vec<String>>();
                StackPanel::new()
                    .spacing(10.0)
                    .children((
                        ComboBox::new()
                            .automation_name("Scene display preset")
                            .items_source(labels)
                            .selected_index(Some(
                                d.display
                                    .as_ref()
                                    .and_then(|id| {
                                        self.scene.displays.iter().position(|p| &p.id == id)
                                    })
                                    .map_or(0, |i| i + 1),
                            ))
                            .is_enabled(available)
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .on_selection_changed(cx.callback(|i| msg(SceneMessage::Display(i)))),
                        d.display
                            .as_ref()
                            .and_then(|id| self.scene.displays.iter().find(|p| &p.id == id))
                            .map(|p| crate::preset_cards_ui::layout_preview(&p.layout, 124.0))
                            .unwrap_or_else(View::empty),
                        Button::new()
                            .is_enabled(available)
                            .on_click(cx.message(Message::Navigate(Some("displays".into()))))
                            .content("Manage display presets"),
                    ))
            }
            Step::Audio => {
                let labels = std::iter::once("Leave audio unchanged".into())
                    .chain(
                        self.scene
                            .audio
                            .iter()
                            .map(|p| format!("{} · {}", p.name, p.output_name)),
                    )
                    .chain(self.scene.tvs.iter().filter_map(|t| {
                        t.audio
                            .as_ref()
                            .map(|a| format!("{} linked audio · {}", t.name, a.name))
                    }))
                    .chain(std::iter::once("New audio preset…".into()))
                    .collect::<Vec<String>>();
                let selected = d
                    .audio
                    .as_ref()
                    .and_then(|id| self.scene.audio.iter().position(|p| &p.id == id))
                    .map(|i| i + 1)
                    .or_else(|| {
                        d.tv_audio
                            .as_ref()
                            .and_then(|id| {
                                self.scene
                                    .tvs
                                    .iter()
                                    .filter(|t| t.audio.is_some())
                                    .position(|t| &t.id == id)
                            })
                            .map(|i| i + 1 + self.scene.audio.len())
                    })
                    .unwrap_or(0);
                StackPanel::new()
                    .spacing(10.0)
                    .children((
                        ComboBox::new()
                            .automation_name("Scene audio preset")
                            .items_source(labels)
                            .selected_index(Some(selected))
                            .is_enabled(available)
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .on_selection_changed(cx.callback(|i| msg(SceneMessage::Audio(i)))),
                        note("Audio switches after the displays, so a TV’s HDMI audio is ready. A linked TV output leaves volume and microphone unchanged; choose an audio preset for more control."),
                        Button::new()
                            .is_enabled(available)
                            .on_click(cx.message(Message::Navigate(Some("audio".into()))))
                            .content("Manage audio presets"),
                    ))
            }
            Step::Fps => self.fps_controls(cx, crate::fps_ui::Target::Scene),
        }
    }

    /// The shared editor for Before, After and When leaving actions.
    fn action_list(&self, cx: &ViewContext<Self>, d: &Scene, list: ActionList) -> View {
        let available = !self.busy;
        let actions = d.list(list);
        let rows = actions
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let kind = match a {
                    Action::Uri { .. } => "Open link",
                    Action::App { .. } => "Run app",
                    Action::Close { .. } => "Close app",
                };
                let body: View = match a {
                    Action::Uri { target } => TextBox::new()
                        .automation_name(format!("Action {} link", i + 1))
                        .placeholder_text(if list == ActionList::Leave {
                            "steam://close/bigpicture"
                        } else {
                            "steam://open/bigpicture"
                        })
                        .text(target.clone())
                        .is_enabled(available)
                        .on_text_changed(cx.callback(move |s| {
                            msg(SceneMessage::ActionText(list, i, 0, s))
                        }))
                        .into(),
                    Action::Close { target, force } => StackPanel::new()
                        .spacing(8.0)
                        .children((
                            TextBox::new()
                                .automation_name(format!("Action {} program to close", i + 1))
                                .placeholder_text("steam.exe, or the program’s full path")
                                .text(target.clone())
                                .is_enabled(available)
                                .on_text_changed(cx.callback(move |s| {
                                    msg(SceneMessage::ActionText(list, i, 0, s))
                                })),
                            ToggleSwitch::new()
                                .automation_name(format!("Action {} force close", i + 1))
                                .is_on(*force)
                                .is_enabled(available)
                                .on_toggled(cx.callback(move |v| {
                                    msg(SceneMessage::ActionToggle(list, i, v))
                                }))
                                .slot(
                                    ToggleSwitchSlot::Header,
                                    "End it if it doesn’t close within 10 seconds",
                                ),
                        )),
                    Action::App {
                        path,
                        args,
                        working_directory,
                        wait,
                    } => StackPanel::new()
                        .spacing(8.0)
                        .children((
                            StackPanel::new()
                                .orientation(Orientation::Horizontal)
                                .spacing(8.0)
                                .children((
                                    Button::new()
                                        .automation_name(format!("Browse action {} executable", i + 1))
                                        .is_enabled(available)
                                        .on_click(cx.callback(move |()| {
                                            msg(SceneMessage::Browse(list, i, false))
                                        }))
                                        .content("Choose app…"),
                                    Button::new()
                                        .automation_name(format!("Browse action {} folder", i + 1))
                                        .is_enabled(available)
                                        .on_click(cx.callback(move |()| {
                                            msg(SceneMessage::Browse(list, i, true))
                                        }))
                                        .content("Working folder…"),
                                )),
                            TextBox::new()
                                .automation_name(format!("Action {} executable", i + 1))
                                .placeholder_text("Full path to an .exe")
                                .text(path.clone())
                                .is_enabled(available)
                                .on_text_changed(cx.callback(move |s| {
                                    msg(SceneMessage::ActionText(list, i, 0, s))
                                })),
                            TextBox::new()
                                .automation_name(format!("Action {} arguments", i + 1))
                                .placeholder_text("Optional arguments · one per line")
                                .accepts_return(true)
                                .text(args.join("\n"))
                                .is_enabled(available)
                                .on_text_changed(cx.callback(move |s| {
                                    msg(SceneMessage::ActionText(list, i, 1, s))
                                })),
                            TextBox::new()
                                .automation_name(format!("Action {} working folder", i + 1))
                                .placeholder_text("Optional working folder")
                                .text(working_directory.clone().unwrap_or_default())
                                .is_enabled(available)
                                .on_text_changed(cx.callback(move |s| {
                                    msg(SceneMessage::ActionText(list, i, 2, s))
                                })),
                            ToggleSwitch::new()
                                .automation_name(format!("Action {} wait until it finishes", i + 1))
                                .is_on(*wait)
                                .is_enabled(available)
                                .on_toggled(cx.callback(move |v| {
                                    msg(SceneMessage::ActionToggle(list, i, v))
                                }))
                                .slot(ToggleSwitchSlot::Header, "Wait until it finishes"),
                            if *wait {
                                note("If it fails or takes longer than the wait set in Options, the scene stops.")
                            } else {
                                View::empty()
                            },
                        )),
                };
                KeyedView::new(
                    format!("action-{i}"),
                    Border::new()
                        .padding(14.0)
                        .corner_radius(8.0)
                        .border_thickness(1.0)
                        .border_brush(BORDER)
                        .content(StackPanel::new().spacing(10.0).children((
                            Grid::new()
                                .columns([GridLength::Star(1.0), GridLength::Auto])
                                .children((
                                    TextBlock::new()
                                        .text(format!("{} · {kind}", i + 1))
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .vertical_alignment(VerticalAlignment::Center),
                                    StackPanel::new()
                                        .grid_column(1)
                                        .orientation(Orientation::Horizontal)
                                        .children((
                                            Button::new()
                                                .automation_name(format!("Move action {} up", i + 1))
                                                .is_enabled(available && i > 0)
                                                .on_click(cx.callback(move |()| {
                                                    msg(SceneMessage::Move(list, i, true))
                                                }))
                                                .content(FontIcon::new().glyph("\u{e70e}"))
                                                .tooltip("Move up"),
                                            Button::new()
                                                .automation_name(format!("Move action {} down", i + 1))
                                                .is_enabled(available && i + 1 < actions.len())
                                                .on_click(cx.callback(move |()| {
                                                    msg(SceneMessage::Move(list, i, false))
                                                }))
                                                .content(FontIcon::new().glyph("\u{e70d}"))
                                                .tooltip("Move down"),
                                            Button::new()
                                                .automation_name(format!("Remove action {}", i + 1))
                                                .is_enabled(available)
                                                .on_click(cx.callback(move |()| {
                                                    msg(SceneMessage::Remove(list, i))
                                                }))
                                                .content(crate::icons::delete())
                                                .tooltip("Remove action"),
                                        )),
                                )),
                            body,
                        ))),
                )
            })
            .collect::<Vec<_>>();
        let room = available && actions.len() < 32;
        StackPanel::new().spacing(12.0).children((
            StackPanel::new().spacing(12.0).keyed_children(rows),
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(8.0)
                .children((
                    Button::new()
                        .automation_name("Add app action")
                        .is_enabled(room)
                        .on_click(cx.message(msg(SceneMessage::AddApp(list))))
                        .content("Run app"),
                    Button::new()
                        .automation_name("Add link action")
                        .is_enabled(room)
                        .on_click(cx.message(msg(SceneMessage::AddUri(list))))
                        .content("Open link"),
                    Button::new()
                        .automation_name("Add close action")
                        .is_enabled(room)
                        .on_click(cx.message(msg(SceneMessage::AddClose(list))))
                        .content("Close app"),
                )),
        ))
    }

    pub(crate) fn scene_editor(&self, cx: &ViewContext<Self>, d: &Scene) -> View {
        let available = !self.busy;
        let flow = [
            Step::Before,
            Step::Tv,
            Step::Displays,
            Step::Audio,
            Step::Fps,
            Step::After,
        ];
        let mut blocks = vec![];
        for (i, step) in flow.into_iter().enumerate() {
            if i > 0 {
                blocks.push(KeyedView::new(format!("line-{}", step.key()), connector()));
            }
            blocks.push(KeyedView::new(step.key(), self.step_block(cx, d, step)));
        }
        let problem =
            scenes::resolve(d, &self.scene.displays, &self.scene.audio, &self.scene.tvs).err();
        let column = StackPanel::new()
            .max_width(FLOW_WIDTH)
            .horizontal_alignment(HorizontalAlignment::Left)
            .spacing(20.0)
            .children((
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text("Scene name")
                        .font_weight(FontWeight::SEMI_BOLD),
                    TextBox::new()
                        .automation_name("Scene name")
                        .placeholder_text("For example, Console or Desk")
                        .text(d.name.clone())
                        .is_enabled(available)
                        .on_text_changed(cx.callback(|s| msg(SceneMessage::Name(s)))),
                )),
                StackPanel::new().keyed_children(blocks),
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text("When you leave this scene")
                        .font_weight(FontWeight::SEMI_BOLD)
                        .opacity(0.85),
                    self.step_block(cx, d, Step::Leave),
                )),
                note(
                    "If a step fails, Director restores your previous settings. Change this in Options.",
                ),
            ));
        let can_save = self.scene_ready() && problem.is_none() && !d.name.trim().is_empty();
        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
            .children((
                crate::preset_ui::header(
                    if d.name.trim().is_empty() {
                        "New scene"
                    } else {
                        d.name.trim()
                    },
                    "Steps run from top to bottom. Add only what this scene should change.",
                    vec![],
                ),
                ScrollViewer::new()
                    .grid_row(1)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                    .content(
                        Border::new()
                            .padding(Thickness::new(0.0, 0.0, 16.0, 0.0))
                            .content(column),
                    ),
                Grid::new()
                    .grid_row(2)
                    .columns([GridLength::Star(1.0), GridLength::Auto])
                    .column_spacing(16.0)
                    .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
                    .children((
                        TextBlock::new()
                            .grid_column(0)
                            .text(match &problem {
                                Some(e) if !d.name.trim().is_empty() => e.clone(),
                                _ => "Saving won’t change your setup.".into(),
                            })
                            .opacity(0.72)
                            .text_wrapping(TextWrapping::Wrap)
                            .vertical_alignment(VerticalAlignment::Center),
                        StackPanel::new()
                            .grid_column(1)
                            .orientation(Orientation::Horizontal)
                            .spacing(8.0)
                            .children((
                                Button::new()
                                    .is_enabled(available)
                                    .on_click(cx.message(msg(SceneMessage::Cancel)))
                                    .content("Cancel"),
                                Button::new()
                                    .automation_name("Save scene")
                                    .style(ButtonStyle::Accent)
                                    .is_enabled(can_save)
                                    .on_click(cx.message(msg(SceneMessage::Save)))
                                    .content("Save scene"),
                            )),
                    )),
            ))
    }
}
