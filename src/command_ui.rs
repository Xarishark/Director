//! "Run from a shortcut": the CLI command that applies a scene or preset, ready to copy into a
//! Stream Deck button, a desktop shortcut or a script. Setup puts `director` on PATH.
use crate::{DirectorApp, Message};
use windows_reactor::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Scene,
    Display,
}
impl Kind {
    fn area(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Display => "display",
        }
    }
    fn noun(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Display => "preset",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    pub kind: Kind,
    pub name: String,
    pub id: String,
    /// A scene that does something when left also gets `director scene end`.
    pub can_end: bool,
}

#[derive(Clone)]
pub(crate) enum CommandMessage {
    Show(Target),
    Copy(String),
    Closed,
}

fn commands(t: &Target) -> Vec<(&'static str, String, String)> {
    // The CLI name ("Desk View" is desk-view) never needs quotes. A name made only of
    // symbols has none, so only the ID command is offered.
    let cli = director::cli_name::cli_name(&t.name);
    let mut rows = Vec::new();
    if !cli.is_empty() {
        rows.push((
            "By name",
            format!("director {} apply {cli}", t.kind.area()),
            format!(
                "Easy to read. Copy it again if you rename this {}.",
                t.kind.noun()
            ),
        ));
    }
    rows.push((
        "By ID",
        format!("director {} apply {}", t.kind.area(), t.id),
        "Keeps working after a rename.".into(),
    ));
    if t.kind == Kind::Scene && t.can_end {
        rows.push((
            "End the active scene",
            "director scene end".into(),
            "Runs the active scene’s When leaving steps.".into(),
        ));
    }
    rows
}

/// A quiet ">_" next to a card's name: plain dimmed text rather than a framed icon, so it
/// reads as a hint, not a primary action. The subtle button still highlights on hover.
pub(crate) fn command_button(cx: &ViewContext<DirectorApp>, target: Target) -> View {
    Button::new()
        .automation_name(format!("Command for {}", target.name))
        .style(ButtonStyle::Subtle)
        .vertical_alignment(VerticalAlignment::Center)
        .on_click(cx.callback(move |()| Message::Command(CommandMessage::Show(target.clone()))))
        .content(
            TextBlock::new()
                .text(">_")
                .font_size(13.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .opacity(crate::ui::SECONDARY)
                // Eat into the button's fixed side padding, so the card's name keeps as
                // much room as it can.
                .margin(Thickness::new(-6.0, 0.0, -6.0, 0.0)),
        )
        .tooltip("Command for shortcuts")
}

impl DirectorApp {
    pub(crate) fn update_command(&mut self, m: CommandMessage, cx: &ComponentContext<Self>) {
        match m {
            CommandMessage::Show(target) => self.command = Some(target),
            CommandMessage::Closed => self.command = None,
            CommandMessage::Copy(text) => match crate::clipboard::copy_text(&text) {
                Ok(()) => {
                    self.error = false;
                    self.status = "Command copied.".into();
                    self.details = text;
                    self.announce(cx);
                }
                Err(e) => self.failure(e),
            },
        }
    }
    pub(crate) fn command_dialog(&self, cx: &ViewContext<Self>) -> View {
        let Some(target) = &self.command else {
            return View::empty();
        };
        let rows = commands(target)
            .into_iter()
            .enumerate()
            .map(|(i, (label, command, subtitle))| {
                let copy = command.clone();
                KeyedView::new(
                    i.to_string(),
                    StackPanel::new().spacing(6.0).children((
                        TextBlock::new()
                            .text(label)
                            .font_weight(FontWeight::SEMI_BOLD),
                        Grid::new()
                            .columns([GridLength::Star(1.0), GridLength::Auto])
                            .column_spacing(8.0)
                            .children((
                                Border::new()
                                    .padding(Thickness::xy(10.0, 7.0))
                                    .corner_radius(crate::ui::RADIUS_S)
                                    .background(crate::ui::CODE_FILL)
                                    .content(
                                        TextBlock::new()
                                            .text(command)
                                            .is_text_selection_enabled(true)
                                            .text_wrapping(TextWrapping::Wrap),
                                    ),
                                Button::new()
                                    .grid_column(1)
                                    .automation_name(format!("Copy {}", label.to_lowercase()))
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .on_click(cx.callback(move |()| {
                                        Message::Command(CommandMessage::Copy(copy.clone()))
                                    }))
                                    .content("Copy"),
                            )),
                        TextBlock::new()
                            .text(subtitle)
                            .font_size(crate::ui::CAPTION)
                            .opacity(crate::ui::SECONDARY)
                            .text_wrapping(TextWrapping::Wrap),
                    )),
                )
            })
            .collect::<Vec<_>>();
        ContentDialog::new()
            .is_open(true)
            .title(format!("Run “{}” from a shortcut", target.name))
            .close_button_text("Close")
            .on_closed(cx.callback(|_| Message::Command(CommandMessage::Closed)))
            .content(
                StackPanel::new()
                    .spacing(16.0)
                    .min_width(440.0)
                    .children((
                        StackPanel::new().spacing(14.0).keyed_children(rows),
                        TextBlock::new()
                            .text("Works from Stream Deck, shortcuts and scripts. Director doesn’t need to be open.")
                            .opacity(0.72)
                            .text_wrapping(TextWrapping::Wrap),
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_use_cli_names_and_offer_end_only_when_it_does_something() {
        let mut t = Target {
            kind: Kind::Scene,
            name: "TV \"Gaming\"".into(),
            id: "abc-1".into(),
            can_end: false,
        };
        let rows = commands(&t);
        assert_eq!(rows[0].1, "director scene apply tv-gaming");
        assert_eq!(rows[1].1, "director scene apply abc-1");
        assert_eq!(rows.len(), 2);
        t.can_end = true;
        assert_eq!(commands(&t).len(), 3);
        t.kind = Kind::Display;
        assert_eq!(commands(&t).len(), 2);
        t.name = "★".into();
        let rows = commands(&t);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "director display apply abc-1");
    }
}
