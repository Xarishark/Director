//! Saved-state previews. Rendering never queries or changes a device.
use crate::{DirectorApp, Message, display_ui::DisplayMessage};
use director::{
    display_mirroring,
    displays::{Layout, Monitor},
};
use windows_reactor::*;
/// Sized to the content so short cards don't end in empty space.
pub(crate) const DISPLAY_CARD_HEIGHT: f64 = 300.0;
pub(crate) const SCENE_CARD_HEIGHT: f64 = 340.0;
pub(crate) const TV_CARD_HEIGHT: f64 = 220.0;
const CARD_WIDTH: f64 = 340.0;
const CARD_GAP: f64 = 8.0;
/// Shared height for a card's Apply, edit and delete buttons so their hover areas line up.
const ACTION_HEIGHT: f64 = 32.0;
fn action_row(views: Vec<View>) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(4.0)
        .vertical_alignment(VerticalAlignment::Center)
        .keyed_children(
            views
                .into_iter()
                .enumerate()
                .map(|(i, v)| KeyedView::new(i.to_string(), v)),
        )
}
/// A wrapping, scrollable layout for cards. Cards are not selectable items, so pointer and keyboard
/// interaction goes straight to their buttons instead of highlighting the whole card.
pub(crate) fn card_grid(row: i32, card_height: f64, cards: Vec<KeyedView>) -> View {
    ScrollViewer::new()
        .grid_row(row)
        .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
        .content(
            VariableSizedWrapGrid::new()
                .orientation(Orientation::Horizontal)
                .item_width(CARD_WIDTH + CARD_GAP)
                .item_height(card_height + CARD_GAP)
                .keyed_children(cards),
        )
}
/// A preset or scene card. `command` sits right after the name (the ">_" button, or empty).
pub(crate) fn card(name: &str, command: View, body: View, actions: Vec<View>, height: f64) -> View {
    Border::new()
        .automation_name(name)
        .horizontal_alignment(HorizontalAlignment::Left)
        .vertical_alignment(VerticalAlignment::Top)
        .width(CARD_WIDTH)
        .height(height)
        .padding(14.0)
        .corner_radius(8.0)
        .background(Color::argb(12, 128, 128, 128))
        .border_brush(Color::rgb(70, 70, 70))
        .border_thickness(1.0)
        .content(
            StackPanel::new().spacing(12.0).children((
                Grid::new()
                    .columns([
                        GridLength::Auto,
                        GridLength::Auto,
                        GridLength::Star(1.0),
                        GridLength::Auto,
                    ])
                    .column_spacing(4.0)
                    .children((
                        TextBlock::new()
                            .text(name)
                            .font_size(18.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .max_width(150.0)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .vertical_alignment(VerticalAlignment::Center)
                            .tooltip(name),
                        Border::new().grid_column(1).content(command),
                        Border::new().grid_column(3).content(action_row(actions)),
                    )),
                body,
            )),
        )
}
/// The Apply button shared by preset and scene cards, around a button that already has its action.
/// It is a standard neutral button so the page's own accent action stays the most prominent.
/// Leave its style unset: windows-reactor's ButtonStyle::Default clears the style to null, which
/// bypasses the rounded Fluent style that unstyled buttons get.
pub(crate) fn apply_button(button: Button, tooltip: &str) -> View {
    button
        .height(ACTION_HEIGHT)
        .content(TextBlock::new().text("Apply"))
        .tooltip(tooltip)
}
/// Edit and delete buttons on cards: Apply's height with a natural width, so 16 px icons are never clipped.
pub(crate) fn card_icon_button(button: Button) -> Button {
    button.style(ButtonStyle::Subtle).height(ACTION_HEIGHT)
}
/// The edit icon at the 16 px size Windows uses inside buttons.
pub(crate) fn card_edit_icon() -> View {
    // FontIcon has no size property here; a Viewbox scales its 20 px default down.
    Viewbox::new()
        .width(16.0)
        .height(16.0)
        .slot(ViewboxSlot::Child, FontIcon::new().glyph("\u{e70f}"))
}
fn legend(m: &Monitor) -> String {
    if let Some(d) = &m.desktop {
        format!(
            "{} × {} · {} · {}",
            d.width,
            d.height,
            d.refresh_rate.label(),
            match m.hdr {
                Some(true) => "HDR on",
                Some(false) => "HDR off",
                None => "HDR unchanged",
            }
        )
    } else {
        "Display off".into()
    }
}
/// One line naming the saved displays that the preset turns off, or None when every display stays on.
pub(crate) fn off_summary(layout: &Layout) -> Option<String> {
    let off = layout
        .monitors
        .iter()
        .enumerate()
        .filter(|(_, m)| m.desktop.is_none())
        .map(|(i, m)| format!("{} {}", i + 1, m.name))
        .collect::<Vec<_>>();
    (!off.is_empty()).then(|| format!("Off: {}", off.join(" · ")))
}
/// The text lines shown inside a preview screen, most important first.
/// Lines that do not fit the screen's size are dropped from the end.
fn screen_lines(layout: &Layout, group: &[usize], width: f64, height: f64) -> Vec<String> {
    let first = &layout.monitors[group[0]];
    let Some(d) = &first.desktop else {
        return vec![];
    };
    let numbers = group
        .iter()
        .map(|j| (j + 1).to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    let star = if d.primary() { " ★" } else { "" };
    if width < 64.0 {
        return vec![format!("{numbers}{star}")];
    }
    let names = group
        .iter()
        .map(|j| layout.monitors[*j].name.as_str())
        .collect::<Vec<_>>()
        .join(" · ");
    let resolution = format!("{} × {}", d.width, d.height);
    let refresh = d.refresh_rate.label();
    let mut lines = vec![format!("{numbers}{star} {names}")];
    if width >= 170.0 {
        lines.push(format!("{resolution} · {refresh}"));
    } else {
        lines.extend([resolution, refresh]);
    }
    if group.iter().any(|j| layout.monitors[*j].hdr == Some(true)) {
        lines.push("HDR".into());
    }
    // 8 px of vertical padding; Segoe UI lines are about 16 px (12 pt name) and 15 px (11 pt details).
    let fits = (((height - 8.0) / 15.0).floor() as usize).max(1);
    lines.truncate(fits);
    lines
}
pub(crate) fn layout_preview(layout: &Layout, height: f64) -> View {
    preview(layout, height, true)
}
/// The same preview without the per-screen tooltips, for use inside a ContentDialog: the
/// pinned Reactor closes the window when a dialog hosts a tooltip.
pub(crate) fn layout_preview_in_dialog(layout: &Layout, height: f64) -> View {
    preview(layout, height, false)
}
fn preview(layout: &Layout, height: f64, tooltips: bool) -> View {
    const WIDTH: f64 = 308.0;
    let mut seen = std::collections::BTreeSet::new();
    let active = layout
        .monitors
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            m.desktop
                .as_ref()
                .filter(|d| seen.insert(display_mirroring::key(d)))
                .map(|d| (i, d))
        })
        .collect::<Vec<_>>();
    let min_x = active
        .iter()
        .map(|(_, d)| f64::from(d.x))
        .fold(f64::INFINITY, f64::min);
    let min_y = active
        .iter()
        .map(|(_, d)| f64::from(d.y))
        .fold(f64::INFINITY, f64::min);
    let max_x = active
        .iter()
        .map(|(_, d)| f64::from(d.x) + f64::from(d.width))
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = active
        .iter()
        .map(|(_, d)| f64::from(d.y) + f64::from(d.height))
        .fold(f64::NEG_INFINITY, f64::max);
    if active.is_empty() {
        return Border::new()
            .background(Color::argb(18, 128, 128, 128))
            .corner_radius(6.0)
            .content(
                Grid::new()
                    .width(WIDTH)
                    .height(height)
                    .children((TextBlock::new()
                        .text("All displays off")
                        .font_size(12.0)
                        .opacity(0.72)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .vertical_alignment(VerticalAlignment::Center),)),
            );
    }
    let scale =
        ((WIDTH - 20.0) / (max_x - min_x).max(1.0)).min((height - 20.0) / (max_y - min_y).max(1.0));
    let left = (WIDTH - (max_x - min_x) * scale) / 2.0;
    let top = (height - (max_y - min_y) * scale) / 2.0;
    let screens = active
        .iter()
        .map(|(i, d)| {
            let group = display_mirroring::members(layout, *i);
            let width = (f64::from(d.width) * scale).max(20.0);
            let screen_height = (f64::from(d.height) * scale).max(18.0);
            let lines = screen_lines(layout, &group, width, screen_height)
                .into_iter()
                .enumerate()
                .map(|(n, line)| {
                    KeyedView::new(
                        format!("line-{n}"),
                        TextBlock::new()
                            .text(line)
                            .font_size(if n == 0 { 12.0 } else { 11.0 })
                            .font_weight(if n == 0 {
                                FontWeight::SEMI_BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .opacity(if n == 0 { 1.0 } else { 0.8 })
                            .horizontal_alignment(HorizontalAlignment::Center)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .text_wrapping(TextWrapping::NoWrap),
                    )
                })
                .collect::<Vec<_>>();
            let details = group
                .iter()
                .map(|j| {
                    format!(
                        "{} · {}",
                        layout.monitors[*j].name,
                        legend(&layout.monitors[*j])
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let screen = Border::new()
                .horizontal_alignment(HorizontalAlignment::Left)
                .vertical_alignment(VerticalAlignment::Top)
                .width(width)
                .height(screen_height)
                .padding(Thickness::new(4.0, 4.0, 4.0, 4.0))
                .margin(Thickness::new(
                    left + (f64::from(d.x) - min_x) * scale,
                    top + (f64::from(d.y) - min_y) * scale,
                    0.0,
                    0.0,
                ))
                .background(Color::rgb(38, 52, 69))
                .border_brush(if d.primary() {
                    Color::rgb(255, 148, 113)
                } else {
                    Color::rgb(145, 167, 187)
                })
                .border_thickness(1.0)
                .corner_radius(4.0)
                .content(
                    StackPanel::new()
                        .vertical_alignment(VerticalAlignment::Center)
                        .keyed_children(lines),
                );
            KeyedView::new(
                format!("preview-screen-{i}"),
                if tooltips {
                    screen.tooltip(details)
                } else {
                    screen
                },
            )
        })
        .collect::<Vec<_>>();
    Border::new()
        .background(Color::argb(18, 128, 128, 128))
        .corner_radius(6.0)
        .content(
            Grid::new()
                .width(WIDTH)
                .height(height)
                .keyed_children(screens),
        )
}
impl DirectorApp {
    pub(crate) fn display_preset_cards(&self, cx: &ViewContext<Self>) -> Vec<KeyedView> {
        self.display
            .presets
            .iter()
            .map(|p| {
                let apply = p.id.clone();
                let edit = p.id.clone();
                let delete = p.id.clone();
                let body = StackPanel::new().spacing(10.0).children((
                    layout_preview(&p.layout, 200.0),
                    TextBlock::new()
                        .text(off_summary(&p.layout).unwrap_or_default())
                        .font_size(12.0)
                        .opacity(0.72)
                        .text_wrapping(TextWrapping::NoWrap)
                        .text_trimming(TextTrimming::CharacterEllipsis)
                        .tooltip(off_summary(&p.layout).unwrap_or_default()),
                ));
                KeyedView::new(
                    p.id.clone(),
                    card(
                        &p.name,
                        crate::command_ui::command_button(
                            cx,
                            crate::command_ui::Target {
                                kind: crate::command_ui::Kind::Display,
                                name: p.name.clone(),
                                id: p.id.clone(),
                                can_end: false,
                            },
                        ),
                        body,
                        vec![
                            apply_button(
                                Button::new()
                                    .automation_name(format!("Apply display preset {}", p.name))
                                    .is_enabled(self.display_ready())
                                    .on_click(cx.callback(move |()| {
                                        Message::Display(DisplayMessage::Apply(apply.clone()))
                                    })),
                                "Apply display preset",
                            ),
                            card_icon_button(Button::new())
                                .automation_name(format!("Edit display preset {}", p.name))
                                .is_enabled(self.display_ready())
                                .on_click(cx.callback(move |()| {
                                    Message::Display(DisplayMessage::Edit(edit.clone()))
                                }))
                                .content(card_edit_icon())
                                .tooltip("Edit display preset"),
                            card_icon_button(Button::new())
                                .automation_name(format!("Delete display preset {}", p.name))
                                .is_enabled(self.display_ready())
                                .on_click(cx.callback(move |()| {
                                    Message::Display(DisplayMessage::Delete(delete.clone()))
                                }))
                                .content(crate::icons::delete_small())
                                .tooltip("Delete display preset"),
                        ],
                        DISPLAY_CARD_HEIGHT,
                    ),
                )
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::{off_summary, screen_lines};
    use director::displays::{DesktopMode, Layout, Monitor, RefreshRate};
    fn monitor(name: &str, mode: Option<(i32, u32, u32, u32)>, hdr: Option<bool>) -> Monitor {
        Monitor {
            adapter_luid: 1,
            target_id: 0,
            device_path: None,
            name: name.into(),
            connected: true,
            hdr,
            enabled: mode.is_some(),
            desktop: mode.map(|(x, width, height, hz)| DesktopMode {
                source_adapter_luid: 1,
                source_id: x as u32,
                source_name: format!("DISPLAY{x}"),
                x,
                y: 0,
                width,
                height,
                source_width: width,
                source_height: height,
                refresh_rate: RefreshRate {
                    numerator: hz,
                    denominator: 1,
                },
                rotation_degrees: 0,
            }),
        }
    }
    fn desk() -> Layout {
        Layout {
            monitors: vec![
                monitor("AW3225QF", Some((0, 3840, 2160, 240)), Some(true)),
                monitor("P27h-20", Some((3840, 1440, 2560, 60)), None),
                monitor("LG TV SSCR2", None, Some(true)),
            ],
        }
    }
    #[test]
    fn wide_screens_show_mode_on_one_line_and_hdr_only_when_on() {
        let layout = desk();
        assert_eq!(
            screen_lines(&layout, &[0], 200.0, 113.0),
            ["1 ★ AW3225QF", "3840 × 2160 · 240.00 Hz", "HDR"]
        );
        assert_eq!(
            screen_lines(&layout, &[1], 80.0, 140.0),
            ["2 P27h-20", "1440 × 2560", "60.00 Hz"]
        );
    }
    #[test]
    fn small_screens_drop_details_but_keep_the_number() {
        let layout = desk();
        assert_eq!(
            screen_lines(&layout, &[1], 80.0, 40.0),
            ["2 P27h-20", "1440 × 2560"]
        );
        assert_eq!(screen_lines(&layout, &[1], 80.0, 18.0), ["2 P27h-20"]);
        assert_eq!(screen_lines(&layout, &[0], 40.0, 60.0), ["1 ★"]);
    }
    #[test]
    fn off_summary_lists_only_turned_off_displays() {
        assert_eq!(off_summary(&desk()).as_deref(), Some("Off: 3 LG TV SSCR2"));
        let mut all_on = desk();
        all_on.monitors.pop();
        assert_eq!(off_summary(&all_on), None);
    }
}
