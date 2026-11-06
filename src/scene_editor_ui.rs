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
    Sound,
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
            Self::Sound => "Sound",
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
            Self::Sound => "\u{e767}",
            Self::Fps => "\u{e916}",
            Self::After => "\u{e768}",
            Self::Leave => "\u{e7e8}",
        }
    }
    /// What an empty step would do, shown under its title.
    fn hint(self) -> &'static str {
        match self {
            Self::Before => "Run a script or app first",
            Self::Tv => "Wake it and switch to the PC input",
            Self::Displays => "Switch to a saved monitor layout",
            Self::Sound => "Switch output, volume or mic",
            Self::Fps => "Cap the frame rate",
            Self::After => "Open apps or links when it’s done",
            Self::Leave => "Close apps when you switch away",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::Tv => "tv",
            Self::Displays => "displays",
            Self::Sound => "sound",
            Self::Fps => "fps",
            Self::After => "after",
            Self::Leave => "leave",
        }
    }
}

fn msg(m: SceneMessage) -> Message {
    Message::Scene(m)
}
use crate::ui::note;
/// The widest the flow column gets.
const FLOW_WIDTH: f64 = 720.0;
/// Every step card, filled or empty, has this height.
const CARD_HEIGHT: f64 = 64.0;
/// The column that holds a step's icon. A card's content starts this far in: a 2 px inset
/// inside its outline (so the hover highlight stays clear of it), the button's 11 px padding
/// and 6 px of our own.
const ICON_COLUMN: f64 = 32.0;
const CONTENT_INSET: f64 = 19.0;
/// Room kept free on the right of a card's text for the buttons drawn over it.
const ACTIONS_RESERVE: f64 = 100.0;
const ARROW: f64 = 14.0;
const DASH: f64 = 6.0;
const GAP: f64 = 4.0;
/// Translucent, so the dashes read on light and dark backgrounds.
const DASH_COLOR: Color = Color::argb(110, 128, 128, 128);

/// A small down arrow between steps ("then"), centred on the cards' icon column.
fn connector() -> View {
    StackPanel::new()
        .opacity(crate::ui::SECONDARY)
        .margin(Thickness::new(
            CONTENT_INSET + (ICON_COLUMN - ARROW) / 2.0,
            4.0,
            0.0,
            4.0,
        ))
        .horizontal_alignment(HorizontalAlignment::Left)
        .children((Viewbox::new()
            .width(ARROW)
            .height(ARROW)
            .slot(ViewboxSlot::Child, FontIcon::new().glyph("\u{e74b}")),))
}

/// One dashed edge of `length`, drawn from short segments. Corners stay open, which keeps
/// the outline looking rounded like the filled cards.
fn dashed_edge(length: f64, horizontal: bool) -> View {
    let inset = 8.0;
    let usable = (length - 2.0 * inset).max(0.0);
    let count = ((usable + GAP) / (DASH + GAP)).floor().max(0.0) as usize;
    let used = count as f64 * (DASH + GAP) - GAP;
    let start = inset + (usable - used.max(0.0)) / 2.0;
    let dashes = (0..count)
        .map(|i| {
            KeyedView::new(
                i.to_string(),
                Border::new()
                    .width(if horizontal { DASH } else { 1.0 })
                    .height(if horizontal { 1.0 } else { DASH })
                    .background(DASH_COLOR),
            )
        })
        .collect::<Vec<_>>();
    let panel = StackPanel::new()
        .orientation(if horizontal {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        })
        .spacing(GAP);
    if horizontal {
        panel
            .margin(Thickness::new(start, 0.0, 0.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Left)
            .keyed_children(dashes)
    } else {
        panel
            .margin(Thickness::new(0.0, start, 0.0, 0.0))
            .vertical_alignment(VerticalAlignment::Top)
            .keyed_children(dashes)
    }
}

/// A dashed rectangle the size of a step card, for empty steps.
fn dashed_outline(width: f64, height: f64) -> View {
    Grid::new().width(width).height(height).keyed_children([
        KeyedView::new(
            "top",
            Border::new()
                .vertical_alignment(VerticalAlignment::Top)
                .content(dashed_edge(width, true)),
        ),
        KeyedView::new(
            "bottom",
            Border::new()
                .vertical_alignment(VerticalAlignment::Bottom)
                .content(dashed_edge(width, true)),
        ),
        KeyedView::new(
            "left",
            Border::new()
                .horizontal_alignment(HorizontalAlignment::Left)
                .content(dashed_edge(height, false)),
        ),
        KeyedView::new(
            "right",
            Border::new()
                .horizontal_alignment(HorizontalAlignment::Right)
                .content(dashed_edge(height, false)),
        ),
    ])
}

/// A step's icon in the shared icon column.
fn step_icon(step: Step, muted: bool) -> View {
    Border::new()
        .width(ICON_COLUMN)
        .opacity(if muted { 0.55 } else { 1.0 })
        .content(
            FontIcon::new()
                .glyph(step.glyph())
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center),
        )
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
            Step::Sound => d.sound.is_some(),
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
            Step::Sound => d
                .sound
                .as_ref()
                .map_or(String::new(), |sound| self.sound_label(sound)),
            Step::Fps => d.fps.as_ref().map_or(String::new(), |r| r.label()),
        }
    }

    /// The width the flow column fills: the page minus the pane, page padding and scroll
    /// bar, up to FLOW_WIDTH.
    fn flow_width(&self) -> f64 {
        // Room for the scroll bar's gutter beside the column.
        (crate::ui::page_width(self.window_width, self.pane_open) - 28.0).clamp(320.0, FLOW_WIDTH)
    }

    /// One step as a fixed-size card. A filled step shows its summary with edit and remove;
    /// an empty one is a dashed outline with a hint and Add. Both open the step's dialog.
    fn step_block(&self, cx: &ViewContext<Self>, d: &Scene, step: Step) -> View {
        let available = !self.busy;
        let used = self.step_used(d, step);
        let text = if used {
            self.step_summary(d, step)
        } else {
            step.hint().to_string()
        };
        let header = Button::new()
            .automation_name(format!(
                "{} {}",
                if used { "Open" } else { "Add" },
                step.title()
            ))
            .style(ButtonStyle::Subtle)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Stretch)
            .is_enabled(available)
            .on_click(cx.message(msg(SceneMessage::Open(Some(step)))))
            .content(
                Border::new()
                    .padding(Thickness::new(6.0, 0.0, ACTIONS_RESERVE, 0.0))
                    .content(
                        Grid::new()
                            .columns([GridLength::Auto, GridLength::Star(1.0)])
                            .column_spacing(10.0)
                            .children((
                                step_icon(step, !used),
                                StackPanel::new()
                                    .grid_column(1)
                                    .spacing(2.0)
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .children((
                                        TextBlock::new()
                                            .text(step.title())
                                            .font_weight(FontWeight::SEMI_BOLD)
                                            .opacity(if used { 1.0 } else { 0.75 }),
                                        TextBlock::new()
                                            .text(text)
                                            .opacity(if used { 0.72 } else { 0.55 })
                                            .text_trimming(TextTrimming::CharacterEllipsis),
                                    )),
                            )),
                    ),
            );
        let actions: View = if used {
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(2.0)
                .margin(Thickness::new(0.0, 0.0, 8.0, 0.0))
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    crate::preset_cards_ui::card_icon_button(Button::new())
                        .automation_name(format!("Edit {}", step.title()))
                        .is_enabled(available)
                        .on_click(cx.message(msg(SceneMessage::Open(Some(step)))))
                        .content(crate::preset_cards_ui::card_edit_icon())
                        .tooltip(format!("Edit {}", step.title())),
                    crate::preset_cards_ui::card_icon_button(Button::new())
                        .automation_name(format!("Remove {}", step.title()))
                        .is_enabled(available)
                        .on_click(cx.message(msg(SceneMessage::ClearStep(step))))
                        .content(crate::icons::delete_small())
                        .tooltip(format!("Remove {} from this scene", step.title())),
                ))
        } else {
            Button::new()
                .automation_name(format!("Add {} step", step.title()))
                .is_enabled(available)
                .margin(Thickness::new(0.0, 0.0, 12.0, 0.0))
                .vertical_alignment(VerticalAlignment::Center)
                .on_click(cx.message(msg(SceneMessage::Open(Some(step)))))
                .content(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Viewbox::new()
                                .width(12.0)
                                .height(12.0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .slot(ViewboxSlot::Child, FontIcon::new().glyph("\u{e710}")),
                            TextBlock::new().text("Add"),
                        )),
                )
        };
        let content = Grid::new()
            .columns([GridLength::Star(1.0), GridLength::Auto])
            .children((
                // The card's main button spans the whole card, so its hover highlight fills it;
                // the step's own buttons sit on top at the right.
                Border::new()
                    .grid_column(0)
                    .grid_column_span(2)
                    .content(header),
                Border::new().grid_column(1).content(actions),
            ));
        if used {
            Border::new()
                .height(CARD_HEIGHT)
                .corner_radius(crate::ui::RADIUS_L)
                .border_thickness(1.0)
                .border_brush(crate::ui::card_stroke())
                .background(crate::ui::card_fill())
                .padding(1.0)
                .content(content)
        } else {
            let width = self.flow_width();
            Grid::new().height(CARD_HEIGHT).keyed_children([
                KeyedView::new("outline", dashed_outline(width, CARD_HEIGHT)),
                KeyedView::new(
                    "content",
                    // The same 2 px inset as filled cards, inside the dashed outline.
                    Border::new()
                        .margin(Thickness::new(2.0, 2.0, 2.0, 2.0))
                        .content(content),
                ),
            ])
        }
    }

    /// The locked "Turn off C1" entry under When leaving, set from the Prepare TV step.
    /// Its pencil opens that step; its bin turns the setting off.
    fn tv_off_card(&self, cx: &ViewContext<Self>, d: &Scene) -> View {
        let Some(prep) = d.tv.as_ref().filter(|t| t.off_when_leaving) else {
            return View::empty();
        };
        let available = !self.busy;
        let name = self
            .scene
            .tvs
            .iter()
            .find(|t| t.id == prep.id)
            .map_or("the TV".to_string(), |t| t.name.clone());
        let header = Button::new()
            .automation_name("Open Prepare TV from Turn off TV")
            .style(ButtonStyle::Subtle)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Stretch)
            .is_enabled(available)
            .on_click(cx.message(msg(SceneMessage::Open(Some(Step::Tv)))))
            .content(
                Border::new()
                    .padding(Thickness::new(6.0, 0.0, ACTIONS_RESERVE, 0.0))
                    .content(
                        Grid::new()
                            .columns([GridLength::Auto, GridLength::Star(1.0)])
                            .column_spacing(10.0)
                            .children((
                                step_icon(Step::Tv, false),
                                StackPanel::new()
                                    .grid_column(1)
                                    .spacing(2.0)
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .children((
                                        TextBlock::new()
                                            .text(format!("Turn off {name}"))
                                            .font_weight(FontWeight::SEMI_BOLD),
                                        TextBlock::new()
                                            .text(
                                                "Set in Prepare TV · runs after the leave actions",
                                            )
                                            .opacity(0.72)
                                            .text_trimming(TextTrimming::CharacterEllipsis),
                                    )),
                            )),
                    ),
            );
        Border::new()
            .height(CARD_HEIGHT)
            .corner_radius(crate::ui::RADIUS_L)
            .border_thickness(1.0)
            .border_brush(crate::ui::card_stroke())
            .background(crate::ui::card_fill())
            .padding(1.0)
            .margin(Thickness::new(0.0, 0.0, 0.0, 8.0))
            .content(
                Grid::new()
                    .columns([GridLength::Star(1.0), GridLength::Auto])
                    .children((
                        Border::new()
                            .grid_column(0)
                            .grid_column_span(2)
                            .content(header),
                        crate::preset_cards_ui::card_icon_button(Button::new())
                            .grid_column(1)
                            .automation_name(format!("Don't turn off {name} when leaving"))
                            .is_enabled(available)
                            .vertical_alignment(VerticalAlignment::Center)
                            .margin(Thickness::new(0.0, 0.0, 8.0, 0.0))
                            .on_click(cx.message(msg(SceneMessage::TvOffWhenLeaving(false))))
                            .content(crate::icons::delete_small())
                            .tooltip("Don't turn the TV off when leaving"),
                    )),
            )
    }

    /// The open step's editor as a dialog. Done keeps the changes; Cancel puts the step back.
    pub(crate) fn step_dialog(&self, cx: &ViewContext<Self>) -> View {
        let (Some(step), Some(d)) = (self.scene.open, &self.scene.draft) else {
            return View::empty();
        };
        ContentDialog::new()
            .is_open(true)
            .title(step.title())
            .primary_button_text("Done")
            .close_button_text("Cancel")
            .on_closed(cx.callback(|r| msg(SceneMessage::StepClosed(r))))
            .content(
                ScrollViewer::new()
                    .max_height(480.0)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                    .content(
                        Border::new()
                            .min_width(440.0)
                            .padding(Thickness::new(0.0, 0.0, 12.0, 0.0))
                            .content(self.step_editor(cx, d, step)),
                    ),
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
                                    ToggleSwitch::new()
                                        .automation_name("Turn off the TV when you leave this scene")
                                        .is_on(prep.off_when_leaving)
                                        .is_enabled(available)
                                        .on_toggled(cx.callback(|v| {
                                            msg(SceneMessage::TvOffWhenLeaving(v))
                                        }))
                                        .slot(
                                            ToggleSwitchSlot::Header,
                                            "Turn off the TV when you leave this scene",
                                        ),
                                    if prep.off_when_leaving {
                                        note("Runs after the When leaving actions, when you apply another scene or end this one.")
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
                            .map(|p| {
                                crate::preset_cards_ui::layout_preview_in_dialog(&p.layout, 124.0)
                            })
                            .unwrap_or_else(View::empty),
                        Button::new()
                            .is_enabled(available)
                            .on_click(cx.message(Message::Go("displays".into())))
                            .content("Manage display presets"),
                    ))
            }
            Step::Sound => self.sound_editor(cx, d),
            Step::Fps => self.fps_controls(cx, crate::fps_ui::Target::Scene),
        }
    }

    /// The Sound step: output (a device or a TV's sound), volume, mute, a separate calls
    /// device and the microphone. Anything left as is stays as it is when the scene runs.
    fn sound_editor(&self, cx: &ViewContext<Self>, d: &Scene) -> View {
        use crate::scene_ui::{SoundEdit, device_choices, mute_choice, sound_device};
        let available = !self.busy;
        let edit = |e: SoundEdit| msg(SceneMessage::Sound(e));
        let empty = scenes::Sound::default();
        let sound = d.sound.as_ref().unwrap_or(&empty);
        let label = |devices: &[director::audio::Output],
                     device: &director::audio::DeviceSelection| {
            devices
                .iter()
                .find(|o| o.id == device.id)
                .map_or(format!("{} — not found", device.name), |o| {
                    crate::audio_relink_ui::device_label(o)
                })
        };
        let outputs = device_choices(&self.outputs, sound_device(sound).as_ref());
        let tvs = self
            .scene
            .tvs
            .iter()
            .filter(|t| t.audio.is_some())
            .collect::<Vec<_>>();
        let output_labels = std::iter::once("Leave as is".to_string())
            .chain(outputs.iter().map(|o| label(&self.outputs, o)))
            .chain(tvs.iter().map(|t| format!("TV's sound · {}", t.name)))
            .collect::<Vec<_>>();
        let output_index = match &sound.output {
            None => 0,
            Some(scenes::SoundOutput::Device { id, .. }) => outputs
                .iter()
                .position(|o| &o.id == id)
                .map_or(0, |i| i + 1),
            Some(scenes::SoundOutput::Tv { id }) => tvs
                .iter()
                .position(|t| &t.id == id)
                .map_or(0, |i| i + 1 + outputs.len()),
        };
        let mute_labels = || ["Leave as is", "Unmute", "Mute"].map(String::from).to_vec();
        let slider = |name: &str, value: u8, on: fn(f64) -> SoundEdit| {
            Slider::new()
                .automation_name(name)
                .minimum(0.0)
                .maximum(100.0)
                .step_frequency(1.0)
                .value(f64::from(value))
                .is_enabled(available)
                .on_value_changed(cx.callback(move |v: f64| edit(on(v))))
        };
        StackPanel::new()
            .spacing(12.0)
            .children((
                ComboBox::new()
                    .automation_name("Scene sound output")
                    .items_source(output_labels)
                    .selected_index(Some(output_index))
                    .is_enabled(available)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .on_selection_changed(cx.callback(move |i| edit(SoundEdit::Output(i))))
                    .slot(ComboBoxSlot::Header, "Output"),
                if sound.output.is_none() && (sound.volume.is_some() || sound.mute.is_some()) {
                    note("Volume and mute apply to whichever output is the default when the scene runs.")
                } else {
                    View::empty()
                },
                Grid::new()
                    .columns([GridLength::Auto, GridLength::Star(1.0), GridLength::Pixel(44.0)])
                    .column_spacing(12.0)
                    .children((
                        ToggleSwitch::new()
                            .automation_name("Set volume")
                            .is_on(sound.volume.is_some())
                            .is_enabled(available)
                            .on_toggled(cx.callback(move |on| edit(SoundEdit::SetVolume(on))))
                            .slot(ToggleSwitchSlot::Header, "Set volume"),
                        if let Some(v) = sound.volume {
                            slider("Scene volume", v, SoundEdit::Volume)
                                .grid_column(1)
                                .vertical_alignment(VerticalAlignment::Bottom)
                                .into()
                        } else {
                            View::empty()
                        },
                        TextBlock::new()
                            .grid_column(2)
                            .text(sound.volume.map_or(String::new(), |v| format!("{v}%")))
                            .vertical_alignment(VerticalAlignment::Bottom)
                            .margin(Thickness::new(0.0, 0.0, 0.0, 8.0)),
                    )),
                ComboBox::new()
                    .automation_name("Scene mute")
                    .items_source(mute_labels())
                    .selected_index(Some(mute_choice(sound.mute)))
                    .is_enabled(available)
                    .on_selection_changed(cx.callback(move |i| edit(SoundEdit::Mute(i))))
                    .slot(ComboBoxSlot::Header, "Mute"),
                CheckBox::new()
                    .automation_name("Use a different device for calls")
                    .is_checked(sound.calls.is_some())
                    .is_enabled(available)
                    .on_is_checked_changed(cx.callback(move |on| edit(SoundEdit::Calls(on))))
                    .content("Use a different device for calls"),
                if let Some(calls) = &sound.calls {
                    let devices = device_choices(&self.outputs, Some(calls));
                    StackPanel::new()
                        .spacing(8.0)
                        .children((
                            ComboBox::new()
                                .automation_name("Calls output")
                                .items_source(
                                    devices
                                        .iter()
                                        .map(|o| label(&self.outputs, o))
                                        .collect::<Vec<_>>(),
                                )
                                .selected_index(devices.iter().position(|o| o.id == calls.id))
                                .is_enabled(available)
                                .horizontal_alignment(HorizontalAlignment::Stretch)
                                .on_selection_changed(
                                    cx.callback(move |i| edit(SoundEdit::CallsDevice(i))),
                                ),
                            self.audio_relink_button(cx, &calls.id, Some(&calls.name), false),
                        ))
                } else {
                    View::empty()
                },
                CheckBox::new()
                    .automation_name("Change microphone")
                    .is_checked(sound.mic.is_some())
                    .is_enabled(available)
                    .on_is_checked_changed(cx.callback(move |on| edit(SoundEdit::Mic(on))))
                    .content("Change microphone"),
                if let Some(mic) = &sound.mic {
                    let devices = device_choices(&self.inputs, Some(&mic.device));
                    StackPanel::new()
                        .spacing(10.0)
                        .children((
                            ComboBox::new()
                                .automation_name("Microphone")
                                .items_source(
                                    devices
                                        .iter()
                                        .map(|o| label(&self.inputs, o))
                                        .collect::<Vec<_>>(),
                                )
                                .selected_index(devices.iter().position(|o| o.id == mic.device.id))
                                .is_enabled(available)
                                .horizontal_alignment(HorizontalAlignment::Stretch)
                                .on_selection_changed(
                                    cx.callback(move |i| edit(SoundEdit::MicDevice(i))),
                                ),
                            self.audio_relink_button(cx, &mic.device.id, Some(&mic.device.name), true),
                            Grid::new()
                                .columns([
                                    GridLength::Auto,
                                    GridLength::Star(1.0),
                                    GridLength::Pixel(44.0),
                                ])
                                .column_spacing(12.0)
                                .children((
                                    ToggleSwitch::new()
                                        .automation_name("Set microphone volume")
                                        .is_on(mic.volume.is_some())
                                        .is_enabled(available)
                                        .on_toggled(
                                            cx.callback(move |on| edit(SoundEdit::SetMicVolume(on))),
                                        )
                                        .slot(ToggleSwitchSlot::Header, "Set mic volume"),
                                    if let Some(v) = mic.volume {
                                        slider("Microphone volume", v, SoundEdit::MicVolume)
                                            .grid_column(1)
                                            .vertical_alignment(VerticalAlignment::Bottom)
                                            .into()
                                    } else {
                                        View::empty()
                                    },
                                    TextBlock::new()
                                        .grid_column(2)
                                        .text(mic.volume.map_or(String::new(), |v| format!("{v}%")))
                                        .vertical_alignment(VerticalAlignment::Bottom)
                                        .margin(Thickness::new(0.0, 0.0, 0.0, 8.0)),
                                )),
                            ComboBox::new()
                                .automation_name("Microphone mute")
                                .items_source(mute_labels())
                                .selected_index(Some(mute_choice(mic.mute)))
                                .is_enabled(available)
                                .on_selection_changed(
                                    cx.callback(move |i| edit(SoundEdit::MicMute(i))),
                                )
                                .slot(ComboBoxSlot::Header, "Microphone mute"),
                        ))
                } else {
                    View::empty()
                },
                if let Some(device) = sound_device(sound) {
                    self.audio_relink_button(cx, &device.id, Some(&device.name), false)
                } else {
                    View::empty()
                },
                note("Sound changes after the displays, so a TV's HDMI audio is ready. The output takes all three Windows roles unless calls use a different device."),
            ))
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
                    } => {
                        let chosen = !path.trim().is_empty();
                        let file = std::path::Path::new(path.trim());
                        let app_name = file
                            .file_name()
                            .map_or(path.clone(), |n| n.to_string_lossy().into_owned());
                        let folder = file
                            .parent()
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let open = self.scene.advanced.contains(&(list, i));
                        let mut extras = vec![];
                        if !args.is_empty() {
                            extras.push(format!(
                                "{} argument{}",
                                args.len(),
                                if args.len() == 1 { "" } else { "s" }
                            ));
                        }
                        if working_directory.is_some() {
                            extras.push("working folder".into());
                        }
                        let advanced_header = if extras.is_empty() {
                            "Advanced options".to_string()
                        } else {
                            format!("Advanced options · {}", extras.join(" · "))
                        };
                        StackPanel::new().spacing(10.0).children((
                            Grid::new()
                                .columns([GridLength::Auto, GridLength::Star(1.0)])
                                .column_spacing(12.0)
                                .children((
                                    Button::new()
                                        .automation_name(format!("Browse action {} executable", i + 1))
                                        .is_enabled(available)
                                        .vertical_alignment(VerticalAlignment::Center)
                                        .on_click(cx.callback(move |()| {
                                            msg(SceneMessage::Browse(list, i, false))
                                        }))
                                        .content(if chosen { "Change…" } else { "Choose app…" }),
                                    if chosen {
                                        StackPanel::new()
                                            .grid_column(1)
                                            .spacing(1.0)
                                            .vertical_alignment(VerticalAlignment::Center)
                                            .children((
                                                TextBlock::new()
                                                    .text(app_name)
                                                    .font_weight(FontWeight::SEMI_BOLD)
                                                    .text_trimming(TextTrimming::CharacterEllipsis),
                                                TextBlock::new()
                                                    .text(folder)
                                                    .font_size(crate::ui::CAPTION)
                                                    .opacity(crate::ui::SECONDARY)
                                                    .text_trimming(TextTrimming::CharacterEllipsis),
                                            ))
                                    } else {
                                        TextBlock::new()
                                            .grid_column(1)
                                            .text("No app chosen yet")
                                            .opacity(crate::ui::SECONDARY)
                                            .vertical_alignment(VerticalAlignment::Center)
                                            .into()
                                    },
                                )),
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
                            HyperlinkButton::new()
                                .automation_name(format!("Action {} advanced options", i + 1))
                                .on_click(cx.callback(move |()| {
                                    msg(SceneMessage::ActionAdvanced(list, i, !open))
                                }))
                                .content(
                                    StackPanel::new()
                                        .orientation(Orientation::Horizontal)
                                        .spacing(8.0)
                                        .children((
                                            TextBlock::new().text(advanced_header),
                                            FontIcon::new()
                                                .glyph(if open { "\u{e70e}" } else { "\u{e70d}" })
                                                .vertical_alignment(VerticalAlignment::Center),
                                        )),
                                ),
                            if open {
                                Border::new()
                                    .padding(Thickness::new(14.0, 2.0, 0.0, 2.0))
                                    .border_thickness(Thickness::new(2.0, 0.0, 0.0, 0.0))
                                    .border_brush(crate::ui::card_stroke())
                                    .content(
                                        StackPanel::new().spacing(10.0).children((
                                            TextBox::new()
                                                .automation_name(format!("Action {} arguments", i + 1))
                                                .placeholder_text("One per line")
                                                .accepts_return(true)
                                                .text(args.join("\n"))
                                                .is_enabled(available)
                                                .on_text_changed(cx.callback(move |s| {
                                                    msg(SceneMessage::ActionText(list, i, 1, s))
                                                }))
                                                .slot(TextBoxSlot::Header, "Arguments"),
                                            Grid::new()
                                                .columns([GridLength::Star(1.0), GridLength::Auto])
                                                .column_spacing(8.0)
                                                .children((
                                                    TextBox::new()
                                                        .automation_name(format!("Action {} working folder", i + 1))
                                                        .placeholder_text("Same as the app")
                                                        .text(working_directory.clone().unwrap_or_default())
                                                        .is_enabled(available)
                                                        .on_text_changed(cx.callback(move |s| {
                                                            msg(SceneMessage::ActionText(list, i, 2, s))
                                                        }))
                                                        .slot(TextBoxSlot::Header, "Working folder"),
                                                    Button::new()
                                                        .grid_column(1)
                                                        .automation_name(format!("Browse action {} folder", i + 1))
                                                        .vertical_alignment(VerticalAlignment::Bottom)
                                                        .is_enabled(available)
                                                        .on_click(cx.callback(move |()| {
                                                            msg(SceneMessage::Browse(list, i, true))
                                                        }))
                                                        .content("Browse…"),
                                                )),
                                            TextBox::new()
                                                .automation_name(format!("Action {} executable", i + 1))
                                                .placeholder_text("Full path to an .exe")
                                                .text(path.clone())
                                                .is_enabled(available)
                                                .on_text_changed(cx.callback(move |s| {
                                                    msg(SceneMessage::ActionText(list, i, 0, s))
                                                }))
                                                .slot(TextBoxSlot::Header, "App path"),
                                        )),
                                    )
                            } else {
                                View::empty()
                            },
                        ))
                    }
                };
                KeyedView::new(
                    format!("action-{i}"),
                    Border::new()
                        .padding(14.0)
                        .corner_radius(crate::ui::RADIUS_L)
                        .border_thickness(1.0)
                        .border_brush(crate::ui::card_stroke())
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
                                                .content(FontIcon::new().glyph("\u{e70e}")),
                                            Button::new()
                                                .automation_name(format!("Move action {} down", i + 1))
                                                .is_enabled(available && i + 1 < actions.len())
                                                .on_click(cx.callback(move |()| {
                                                    msg(SceneMessage::Move(list, i, false))
                                                }))
                                                .content(FontIcon::new().glyph("\u{e70d}")),
                                            Button::new()
                                                .automation_name(format!("Remove action {}", i + 1))
                                                .is_enabled(available)
                                                .on_click(cx.callback(move |()| {
                                                    msg(SceneMessage::Remove(list, i))
                                                }))
                                                .content(crate::icons::delete()),
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
            Step::Sound,
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
        let problem = scenes::resolve(d, &self.scene.displays, &self.scene.tvs).err();
        // Fill the page up to FLOW_WIDTH: the pane, page padding and scroll bar take the rest.
        let column = StackPanel::new()
            .width(self.flow_width())
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
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text("Steps")
                        .font_weight(FontWeight::SEMI_BOLD)
                        .opacity(0.85),
                    StackPanel::new().keyed_children(blocks),
                )),
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text("When you leave this scene")
                        .font_weight(FontWeight::SEMI_BOLD)
                        .opacity(0.85),
                    self.tv_off_card(cx, d),
                    self.step_block(cx, d, Step::Leave),
                )),
            ));
        let changed = self.draft_changed(crate::Page::Scenes);
        let can_save =
            self.scene_ready() && problem.is_none() && !d.name.trim().is_empty() && changed;
        crate::ui::page(
            crate::ui::header(
                if d.name.trim().is_empty() {
                    "New scene"
                } else {
                    d.name.trim()
                },
                "Steps run from top to bottom. Add only what this scene should change.",
                vec![],
            ),
            column,
            Some(crate::ui::footer(
                StackPanel::new()
                    .spacing(2.0)
                    .children((
                        crate::ui::note(match &problem {
                            Some(e) if !d.name.trim().is_empty() => e.clone(),
                            _ if !changed => "No changes to save.".into(),
                            _ => "Saving won’t change your setup.".into(),
                        }),
                        crate::ui::caption(
                            "If a step fails, Director restores your previous settings. Change this in Options.",
                        ),
                    )),
                vec![
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
                ],
            )),
        )
    }
}

#[cfg(test)]
mod tests {
    /// The source between two markers.
    fn region<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let from = source.find(start).expect(start);
        let to = source[from..].find(end).expect(end) + from;
        &source[from..to]
    }
    /// The pinned Reactor closes the whole window (StructureUnsupported) when a tooltip sits
    /// inside a ContentDialog, so nothing a dialog hosts may use one.
    #[test]
    fn dialog_content_has_no_tooltips() {
        let editor = include_str!("scene_editor_ui.rs");
        let fps = include_str!("fps_ui.rs");
        let tip = concat!(".tool", "tip(");
        for (name, code) in [
            (
                "step dialogs",
                region(
                    editor,
                    "pub(crate) fn step_dialog",
                    "pub(crate) fn scene_editor(",
                ),
            ),
            (
                "FPS controls",
                region(
                    fps,
                    "pub(crate) fn fps_controls",
                    "pub(crate) static PREVIEW",
                ),
            ),
            (
                "relink button (Sound step)",
                region(
                    include_str!("audio_relink_ui.rs"),
                    "pub(crate) fn audio_relink_button",
                    "pub(crate) fn update_audio_relink",
                ),
            ),
            ("crash dialog", include_str!("crash_ui.rs")),
            ("shared page helpers", include_str!("ui.rs")),
            (
                "command dialog",
                region(
                    include_str!("command_ui.rs"),
                    "pub(crate) fn command_dialog",
                    "#[cfg(test)]",
                ),
            ),
        ] {
            assert!(!code.contains(tip), "{name} use a tooltip inside a dialog");
        }
        // Shared helpers can hide a tooltip (the display preview did), so the step dialogs
        // may only call helpers checked to be tooltip-free. Check a new one, then add it here.
        let safe = [
            "crate::icons::delete(",
            "crate::preset_cards_ui::layout_preview_in_dialog(",
            // Returns a String label.
            "crate::audio_relink_ui::device_label(",
        ];
        let steps = region(
            editor,
            "pub(crate) fn step_dialog",
            "pub(crate) fn scene_editor(",
        );
        for (at, _) in steps.match_indices("crate::") {
            let rest = &steps[at..];
            let path = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
                .unwrap_or(rest.len());
            let call = &rest[..(path + 1).min(rest.len())];
            // A call to a function in another UI module (not a type or enum path).
            let last = call.trim_end_matches('(').rsplit("::").next().unwrap_or("");
            // crate::ui is checked whole above, so its helpers are all tooltip-free.
            if call.ends_with('(')
                && last.starts_with(|c: char| c.is_lowercase())
                && !call.starts_with("crate::ui::")
            {
                assert!(
                    safe.contains(&call),
                    "{call} is used inside a step dialog; check it has no tooltip"
                );
            }
        }
        let preview = region(
            include_str!("preset_cards_ui.rs"),
            "fn preview(",
            "impl DirectorApp",
        );
        assert!(
            preview.contains("if tooltips"),
            "the dialog preview must skip tooltips"
        );
    }
}
