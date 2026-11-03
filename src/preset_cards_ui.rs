//! Saved-state previews. Rendering never queries or changes a device.
use crate::{DirectorApp, Message, display_ui::DisplayMessage};
use director::{
    display_mirroring,
    displays::{Layout, Monitor},
};
use windows_reactor::*;
fn action_row(views: Vec<View>) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(2.0)
        .vertical_alignment(VerticalAlignment::Top)
        .keyed_children(
            views
                .into_iter()
                .enumerate()
                .map(|(i, v)| KeyedView::new(i.to_string(), v)),
        )
}
pub(crate) fn card(name: &str, body: View, actions: Vec<View>, height: f64) -> View {
    Border::new()
        .width(340.0)
        .height(height)
        .padding(14.0)
        .corner_radius(8.0)
        .background(Color::argb(12, 128, 128, 128))
        .border_brush(Color::rgb(70, 70, 70))
        .border_thickness(1.0)
        .content(
            StackPanel::new().spacing(12.0).children((
                Grid::new()
                    .columns([GridLength::Star(1.0), GridLength::Auto])
                    .column_spacing(8.0)
                    .children((
                        TextBlock::new()
                            .text(name)
                            .font_size(18.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .vertical_alignment(VerticalAlignment::Center)
                            .tooltip(name),
                        Border::new().grid_column(1).content(action_row(actions)),
                    )),
                body,
            )),
        )
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
pub(crate) fn layout_preview(layout: &Layout) -> View {
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
    let scale = (284.0 / (max_x - min_x).max(1.0)).min(100.0 / (max_y - min_y).max(1.0));
    let left = (308.0 - (max_x - min_x) * scale) / 2.0;
    let top = (124.0 - (max_y - min_y) * scale) / 2.0;
    let screens = active
        .iter()
        .map(|(i, d)| {
            let group = display_mirroring::members(layout, *i);
            let numbers = group
                .iter()
                .map(|j| (j + 1).to_string())
                .collect::<Vec<_>>()
                .join(" | ");
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
            KeyedView::new(
                format!("preview-screen-{i}"),
                Border::new()
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .vertical_alignment(VerticalAlignment::Top)
                    .width((f64::from(d.width) * scale).max(20.0))
                    .height((f64::from(d.height) * scale).max(18.0))
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
                        TextBlock::new()
                            .text(format!("{numbers}{}", if d.primary() { " ★" } else { "" }))
                            .font_size(12.0)
                            .horizontal_alignment(HorizontalAlignment::Center)
                            .vertical_alignment(VerticalAlignment::Center),
                    )
                    .tooltip(details),
            )
        })
        .collect::<Vec<_>>();
    Border::new()
        .background(Color::argb(18, 128, 128, 128))
        .corner_radius(6.0)
        .content(
            Grid::new()
                .width(308.0)
                .height(124.0)
                .keyed_children(screens),
        )
}
impl DirectorApp {
    pub(crate) fn audio_preset_cards(&self, cx: &ViewContext<Self>) -> Vec<KeyedView> {
        self.presets
            .iter()
            .map(|p| {
                let apply = p.id.clone();
                let edit = p.id.clone();
                let delete = p.id.clone();
                let output = self.outputs.iter().find(|o| o.id == p.output_id);
                let body = StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text("OUTPUT DEVICE")
                        .font_size(10.0)
                        .opacity(0.65),
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::Star(1.0)])
                        .column_spacing(12.0)
                        .children((
                            FontIcon::new()
                                .glyph("\u{e767}")
                                .vertical_alignment(VerticalAlignment::Center),
                            TextBlock::new()
                                .grid_column(1)
                                .text(output.map_or(p.output_name.as_str(), |o| o.name.as_str()))
                                .font_size(17.0)
                                .text_wrapping(TextWrapping::Wrap),
                        )),
                    TextBlock::new()
                        .text(
                            p.volume_percent
                                .map_or("Keep current device volume".into(), |v| {
                                    format!("Set volume to {v}%")
                                }),
                        )
                        .font_size(13.0)
                        .opacity(0.8),
                    TextBlock::new()
                        .text(
                            p.communications_output
                                .as_ref()
                                .map_or("Calls: main output".into(), |d| {
                                    format!("Calls: {}", d.name)
                                }),
                        )
                        .font_size(12.0)
                        .opacity(0.8)
                        .max_height(32.0)
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text(p.input.as_ref().map_or("Microphone unchanged".into(), |d| {
                            format!(
                                "Microphone: {}{}",
                                d.name,
                                d.volume_percent
                                    .map_or(String::new(), |v| format!(" · {v}%"))
                            )
                        }))
                        .font_size(12.0)
                        .opacity(0.8)
                        .max_height(32.0)
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text(if output.is_some_and(|o| o.active) {
                            ""
                        } else {
                            "Currently unavailable · remembered device"
                        })
                        .font_size(12.0)
                        .opacity(0.65)
                        .text_wrapping(TextWrapping::Wrap),
                ));
                KeyedView::new(
                    p.id.clone(),
                    GridViewItem::new()
                        .tag(p.id.clone())
                        .automation_name(p.name.clone())
                        .margin(4.0)
                        .content(card(
                            &p.name,
                            body,
                            vec![
                                Button::new()
                                    .style(ButtonStyle::Subtle)
                                    .automation_name(format!("Apply {}", p.name))
                                    .is_enabled(self.ready())
                                    .on_click(cx.callback(move |()| Message::Apply(apply.clone())))
                                    .content(
                                        TextBlock::new()
                                            .text("✓")
                                            .font_size(20.0)
                                            .foreground(Color::rgb(75, 190, 120)),
                                    )
                                    .tooltip("Apply audio preset"),
                                Button::new()
                                    .style(ButtonStyle::Subtle)
                                    .automation_name(format!("Edit {}", p.name))
                                    .is_enabled(self.ready())
                                    .on_click(cx.callback(move |()| Message::Edit(edit.clone())))
                                    .content(FontIcon::new().glyph("\u{e70f}"))
                                    .tooltip("Edit audio preset"),
                                Button::new()
                                    .style(ButtonStyle::Subtle)
                                    .automation_name(format!("Delete {}", p.name))
                                    .is_enabled(self.ready())
                                    .on_click(
                                        cx.callback(move |()| Message::Delete(delete.clone())),
                                    )
                                    .content(crate::icons::delete())
                                    .tooltip("Delete audio preset"),
                            ],
                            300.0,
                        )),
                )
            })
            .collect()
    }
    pub(crate) fn display_preset_cards(&self, cx: &ViewContext<Self>) -> Vec<KeyedView> {
        self.display
            .presets
            .iter()
            .map(|p| {
                let apply = p.id.clone();
                let edit = p.id.clone();
                let delete = p.id.clone();
                let descriptions = p
                    .layout
                    .monitors
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        KeyedView::new(
                            format!("legend-{i}"),
                            StackPanel::new().spacing(2.0).children((
                                TextBlock::new()
                                    .text(format!("{}. {}", i + 1, m.name))
                                    .font_size(12.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .text_trimming(TextTrimming::CharacterEllipsis)
                                    .tooltip(&m.name),
                                TextBlock::new()
                                    .text(legend(m))
                                    .font_size(12.0)
                                    .opacity(0.72)
                                    .text_wrapping(TextWrapping::Wrap),
                            )),
                        )
                    })
                    .collect::<Vec<_>>();
                let body = StackPanel::new().spacing(12.0).children((
                    layout_preview(&p.layout),
                    ScrollViewer::new()
                        .height(150.0)
                        .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                        .content(StackPanel::new().spacing(10.0).keyed_children(descriptions)),
                ));
                KeyedView::new(
                    p.id.clone(),
                    GridViewItem::new()
                        .tag(p.id.clone())
                        .automation_name(p.name.clone())
                        .margin(4.0)
                        .content(card(
                            &p.name,
                            body,
                            vec![
                                Button::new()
                                    .style(ButtonStyle::Subtle)
                                    .automation_name(format!("Apply display preset {}", p.name))
                                    .is_enabled(self.display_ready())
                                    .on_click(cx.callback(move |()| {
                                        Message::Display(DisplayMessage::Apply(apply.clone()))
                                    }))
                                    .content(
                                        TextBlock::new()
                                            .text("✓")
                                            .font_size(20.0)
                                            .foreground(Color::rgb(75, 190, 120)),
                                    )
                                    .tooltip("Apply display preset"),
                                Button::new()
                                    .style(ButtonStyle::Subtle)
                                    .automation_name(format!("Edit display preset {}", p.name))
                                    .is_enabled(self.display_ready())
                                    .on_click(cx.callback(move |()| {
                                        Message::Display(DisplayMessage::Edit(edit.clone()))
                                    }))
                                    .content(FontIcon::new().glyph("\u{e70f}"))
                                    .tooltip("Edit display preset"),
                                Button::new()
                                    .style(ButtonStyle::Subtle)
                                    .automation_name(format!("Delete display preset {}", p.name))
                                    .is_enabled(self.display_ready())
                                    .on_click(cx.callback(move |()| {
                                        Message::Display(DisplayMessage::Delete(delete.clone()))
                                    }))
                                    .content(crate::icons::delete())
                                    .tooltip("Delete display preset"),
                            ],
                            370.0,
                        )),
                )
            })
            .collect()
    }
}
