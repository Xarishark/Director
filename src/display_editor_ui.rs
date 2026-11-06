//! Native per-monitor draft controls. All changes remain in memory until Save.
use crate::{DirectorApp, Message, display_ui::DisplayMessage};
use director::{
    display_editor::{Change, Editor},
    displays::{DesktopMode, Layout},
};
use windows_reactor::*;
#[derive(Clone, Copy)]
pub(crate) enum EditorTarget {
    Preset,
    Current,
}
/// How many monitor cards fit side by side in a column `width` wide (at least 1, at most 3).
pub(crate) fn monitor_columns(width: f64) -> usize {
    // Cards need about 400 DIP each, with 16 between them; n cards plus their gaps fit in n × (CARD + GAP) − GAP.
    const CARD: f64 = 400.0;
    const GAP: f64 = 16.0;
    (((width + GAP) / (CARD + GAP)).floor() as usize).clamp(1, 3)
}
fn route(target: EditorTarget, value: DisplayMessage) -> Message {
    match target {
        EditorTarget::Preset => Message::Display(value),
        EditorTarget::Current => {
            Message::Current(crate::current_ui::CurrentMessage::Display(value))
        }
    }
}
fn msg(target: EditorTarget, index: usize, change: Change) -> Message {
    route(target, DisplayMessage::Change(index, change))
}
const ORIENTATIONS: [(u16, &str); 4] = [
    (0, "Landscape"),
    (90, "Portrait"),
    (180, "Landscape flipped"),
    (270, "Portrait flipped"),
];
fn orientation(degrees: u16) -> View {
    let path = std::env::current_exe()
        .unwrap_or_default()
        .with_file_name("orientations")
        .join(format!("{degrees}.svg"));
    let image: View = Image::new()
        .source_file(path)
        .map(|v| v.width(24.0).height(24.0).into())
        .unwrap_or_else(|_| View::empty());
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .horizontal_alignment(HorizontalAlignment::Left)
        .width(212.0)
        .spacing(8.0)
        .children((
            image,
            TextBlock::new()
                .text(format!(
                    "{} ({degrees}°)",
                    ORIENTATIONS
                        .iter()
                        .find(|(d, _)| *d == degrees)
                        .map_or("Orientation", |(_, label)| *label)
                ))
                .vertical_alignment(VerticalAlignment::Center),
        ))
}
fn preview(
    app: &DirectorApp,
    layout: &Layout,
    cx: &ViewContext<DirectorApp>,
    target: EditorTarget,
    actions: Vec<View>,
) -> View {
    let drag = match target {
        EditorTarget::Preset => app.display.drag.as_ref(),
        EditorTarget::Current => app.current.display.drag.as_ref(),
    };
    let (width, height) = app.map_size();
    let map = drag.map_or_else(
        || director::display_editor::MapTransform::new(layout, width, height),
        |d| d.map,
    );
    let selected = match target {
        EditorTarget::Preset => app.display.selected,
        EditorTarget::Current => app.current.display.selected,
    };
    let cards = layout
        .monitors
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let d = m.desktop.as_ref()?;
            let group = director::display_mirroring::members(layout, i);
            if group.first() != Some(&i) {
                return None;
            }
            let primary = drag.map_or(d.primary(), |drag| {
                drag.before.monitors[i]
                    .desktop
                    .as_ref()
                    .is_some_and(DesktopMode::primary)
            });
            Some(KeyedView::new(
                format!("screen-{i}"),
                Border::new()
                    .automation_name(format!("Move display {}", m.name))
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .vertical_alignment(VerticalAlignment::Top)
                    .width((d.width as f64 * map.scale).max(8.0))
                    .height((d.height as f64 * map.scale).max(8.0))
                    .margin(Thickness::new(
                        map.left + d.x as f64 * map.scale,
                        map.top + d.y as f64 * map.scale,
                        0.0,
                        0.0,
                    ))
                    .background(if group.contains(&selected) {
                        Brush::from(ThemeBrush::Accent)
                    } else {
                        Brush::from(crate::ui::SCREEN_FILL)
                    })
                    .border_brush(Brush::Solid(if primary {
                        crate::ui::SCREEN_PRIMARY
                    } else {
                        crate::ui::SCREEN_OTHER
                    }))
                    .border_thickness(Thickness::uniform(2.0))
                    .corner_radius(crate::ui::RADIUS_S)
                    .capture_pointer_on_press(!app.busy)
                    .on_pointer_pressed(cx.callback(move |point| {
                        route(target, DisplayMessage::PointerDown(i, point))
                    }))
                    .on_pointer_moved(
                        cx.callback(move |point| route(target, DisplayMessage::PointerMove(point))),
                    )
                    .on_pointer_released(
                        cx.callback(move |point| route(target, DisplayMessage::PointerUp(point))),
                    )
                    .on_pointer_capture_lost(cx.message(route(target, DisplayMessage::CaptureLost)))
                    .on_pointer_canceled(cx.message(route(target, DisplayMessage::CancelDrag)))
                    .content(
                        TextBlock::new()
                            .text(format!(
                                "{}{}",
                                group
                                    .iter()
                                    .map(|j| (j + 1).to_string())
                                    .collect::<Vec<_>>()
                                    .join(" | "),
                                if primary { " ★" } else { "" }
                            ))
                            .font_size(
                                (d.height.min(d.width) as f64 * map.scale * 0.3).clamp(12.0, 28.0),
                            )
                            .foreground(crate::ui::SCREEN_TEXT)
                            .horizontal_alignment(HorizontalAlignment::Center)
                            .vertical_alignment(VerticalAlignment::Center),
                    )
                    .tooltip(format!(
                        "{}{}
{} × {} · {}
Position: {}, {}
Select to edit it; drag to move it.",
                        m.name,
                        if primary { " · Primary" } else { "" },
                        d.width,
                        d.height,
                        d.refresh_rate.label(),
                        d.x,
                        d.y
                    )),
            ))
        })
        .collect::<Vec<_>>();
    let has_actions = !actions.is_empty();
    let mut children = cards;
    if has_actions {
        // Windows keeps the display page's actions in the map's bottom-right corner.
        children.push(KeyedView::new(
            "map-actions",
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(crate::ui::GAP_S)
                .horizontal_alignment(HorizontalAlignment::Right)
                .vertical_alignment(VerticalAlignment::Bottom)
                .keyed_children(
                    actions
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| {
                            // Buttons are translucent; a solid backing keeps them readable.
                            KeyedView::new(
                                format!("action-{i}"),
                                Border::new()
                                    .corner_radius(crate::ui::RADIUS_S)
                                    .background(ThemeBrush::SolidBackground)
                                    .content(v),
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
        ));
    }
    Border::new()
        .horizontal_alignment(HorizontalAlignment::Center)
        .border_thickness(Thickness::uniform(1.0))
        .border_brush(crate::ui::OUTLINE)
        .corner_radius(crate::ui::RADIUS_L)
        .padding(Thickness::uniform(10.0))
        .content(
            Grid::new()
                .width(width)
                .height(height + if has_actions { MAP_ACTIONS_HEIGHT } else { 0.0 })
                .automation_name("Display arrangement")
                .keyed_children(children),
        )
}
/// The arrangement map's height, as in Windows Settings › Display.
const MAP_HEIGHT: f64 = 280.0;
/// A strip under the screens for the map's buttons, so screens never sit behind them.
const MAP_ACTIONS_HEIGHT: f64 = 52.0;
/// The widest the map gets on a large window.
const MAP_MAX_WIDTH: f64 = 1000.0;
impl DirectorApp {
    /// The arrangement map's box: the editor's width (less the scroll gutter, the map's
    /// padding and border), capped, and a fixed height.
    pub(crate) fn map_size(&self) -> (f64, f64) {
        let width =
            crate::ui::page_width(self.window_width, self.pane_open) - crate::ui::GAP_L - 22.0;
        (width.clamp(320.0, MAP_MAX_WIDTH), MAP_HEIGHT)
    }
    pub(crate) fn monitor_editor(
        &self,
        layout: &Layout,
        editor: &Editor,
        cx: &ViewContext<Self>,
    ) -> View {
        self.monitor_editor_for(layout, editor, cx, EditorTarget::Preset)
    }
    pub(crate) fn monitor_editor_for(
        &self,
        layout: &Layout,
        editor: &Editor,
        cx: &ViewContext<Self>,
        target: EditorTarget,
    ) -> View {
        self.monitor_editor_with(layout, editor, cx, target, vec![])
    }
    /// The editor with buttons in the map's bottom-right corner, where Windows puts its
    /// display actions.
    pub(crate) fn monitor_editor_with(
        &self,
        layout: &Layout,
        editor: &Editor,
        cx: &ViewContext<Self>,
        target: EditorTarget,
        map_actions: Vec<View>,
    ) -> View {
        // Now's Displays section has its own heading and hint.
        let heading = matches!(target, EditorTarget::Preset);
        StackPanel::new().spacing(16.0).children((
            if heading {
                crate::ui::subtitle("Arrange your displays").into()
            } else {
                View::empty()
            },
            if heading {
                crate::ui::note(
                    "Select a display to find its settings. Drag displays to rearrange them.",
                )
            } else {
                View::empty()
            },
            preview(self, layout, cx, target, map_actions),
            self.all_displays(layout, editor, cx, target),
        ))
    }
    /// A card for every display, side by side when there's room.
    fn all_displays(
        &self,
        layout: &Layout,
        editor: &Editor,
        cx: &ViewContext<Self>,
        target: EditorTarget,
    ) -> View {
        let columns = monitor_columns(crate::ui::page_width(self.window_width, self.pane_open));
        let labelled =
            |title: View, control: View| StackPanel::new().spacing(4.0).children((title, control));
        let pair = |left: View, right: View| -> View {
            Grid::new()
                .columns([GridLength::Star(1.0), GridLength::Star(1.0)])
                .column_spacing(12.0)
                .children((
                    Border::new().content(left),
                    Border::new().grid_column(1).content(right),
                ))
        };
        let selected = match target {
            EditorTarget::Preset => self.display.selected,
            EditorTarget::Current => self.current.display.selected,
        };
        let cards = (0..layout.monitors.len())
            .map(|i| {
                let p = self.monitor_parts(layout, editor, cx, target, i);
                let text = |s: &str| -> View { TextBlock::new().text(s).into() };
                let mode = p.mode.map(|mode| labelled(text("Desktop mode"), mode));
                let body: View = match p.details {
                    Some(d) => StackPanel::new()
                        .spacing(12.0)
                        .children((
                            pair(
                                labelled(text("Resolution"), d.resolution),
                                labelled(
                                    TextBlock::new()
                                        .text("Refresh rate")
                                        .tooltip("Only rates for the selected resolution are shown. Windows marks rates that require a different resolution with an asterisk."),
                                    d.rate,
                                ),
                            ),
                            // Desktop mode sits beside Orientation instead of taking a row.
                            pair(
                                mode.unwrap_or_else(View::empty),
                                labelled(text("Orientation"), d.orientation),
                            ),
                            labelled(text("HDR"), d.hdr),
                        )),
                    None => mode.unwrap_or_else(View::empty),
                };
                KeyedView::new(
                    format!("editor-{i}"),
                    Border::new()
                        .grid_column((i % columns) as i32)
                        .grid_row((i / columns) as i32)
                        .vertical_alignment(VerticalAlignment::Top)
                        .content(
                            // The selected display's card is outlined like its screen on the map.
                            Border::new()
                                .padding(crate::ui::GAP_L)
                                .corner_radius(crate::ui::RADIUS_L)
                                .background(crate::ui::card_fill())
                                .border_brush(if i == selected {
                                    Brush::from(ThemeBrush::Accent)
                                } else {
                                    crate::ui::card_stroke()
                                })
                                .border_thickness(if i == selected { 2.0 } else { 1.0 })
                                .content(StackPanel::new().spacing(12.0).children((
                                    p.header, p.status, p.linked, p.warning, p.setup, body,
                                ))),
                        ),
                )
            })
            .collect::<Vec<_>>();
        Grid::new()
            .columns((0..columns).map(|_| GridLength::Star(1.0)))
            .rows((0..layout.monitors.len().div_ceil(columns)).map(|_| GridLength::Auto))
            .column_spacing(16.0)
            .row_spacing(16.0)
            .keyed_children(cards)
    }
    /// The pieces of one display's editor, shared by both views. Controls carry no header;
    /// each view labels them its own way.
    fn monitor_parts(
        &self,
        layout: &Layout,
        editor: &Editor,
        cx: &ViewContext<Self>,
        target: EditorTarget,
        i: usize,
    ) -> MonitorParts {
        let revision = match target {
            EditorTarget::Preset => self.display.orientation_revision,
            EditorTarget::Current => self.current.display.orientation_revision,
        };
        let m = &layout.monitors[i];
        let options = director::display_mirroring::common_options(editor, layout, i);
        let group = director::display_mirroring::members(layout, i);
        let d = m.desktop.as_ref();
        let primary = d.is_some_and(DesktopMode::primary);
        let detecting = matches!(target, EditorTarget::Preset)
            && self.display.setup.index == Some(i)
            && self.display.setup.stage.is_some();
        let editable = !self.busy && m.enabled;
        let configured = !options.modes.is_empty()
            && options
                .warning
                .as_deref()
                .is_none_or(|w| w.starts_with("Previously detected"));

        let mirror_targets = layout
            .monitors
            .iter()
            .enumerate()
            .filter(|(j, m)| *j != i && m.enabled)
            .map(|(j, _)| j)
            .collect::<Vec<_>>();
        let mirror_selected = group
            .iter()
            .copied()
            .find(|j| *j != i)
            .and_then(|j| mirror_targets.iter().position(|v| *v == j))
            .map_or(Some(0), |j| Some(j + 1));
        let mirror_labels = std::iter::once("Extend desktop".to_string())
            .chain(
                mirror_targets
                    .iter()
                    .map(|j| format!("Mirror with {}. {}", j + 1, layout.monitors[*j].name)),
            )
            .collect::<Vec<_>>();
        let mode: View = ComboBox::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .automation_name(format!("Desktop mode for {}", m.name))
            .items_source(mirror_labels)
            .selected_index(mirror_selected)
            .is_enabled(!self.busy && m.enabled)
            .on_selection_changed(cx.callback(move |selected| {
                match selected {
                    Some(0) => msg(target, i, Change::Mirror(None)),
                    Some(n) => mirror_targets
                        .get(n - 1)
                        .map_or(route(target, DisplayMessage::Noop), |j| {
                            msg(target, i, Change::Mirror(Some(*j)))
                        }),
                    None => route(target, DisplayMessage::Noop),
                }
            }))
            .into();

        let details = d.filter(|_| configured).map(|d| {
            let resolutions = options.resolutions();
            let resolution = resolutions
                .iter()
                .position(|r| *r == (d.source_width, d.source_height));
            let rates = options.rates(d.source_width, d.source_height);
            let rate = rates.iter().position(|r| *r == d.refresh_rate);
            let hdr_toggle = ToggleSwitch::new()
                .automation_name(format!("HDR for {}", m.name))
                .is_on(m.hdr.or(options.hdr).unwrap_or(false))
                .is_enabled(editable && options.hdr.is_some())
                .on_toggled(cx.callback(move |on| msg(target, i, Change::Hdr(on))));
            let hdr: View = if options.hdr.is_none() {
                Grid::new().children((
                    hdr_toggle,
                    // A disabled control gets no hover; this enabled layer explains it.
                    Border::new()
                        .background(Color::argb(0, 0, 0, 0))
                        .automation_name(format!("Why HDR is unavailable for {}", m.name))
                        .tooltip("HDR is unavailable for this display or Windows version."),
                ))
            } else {
                hdr_toggle.tooltip("Enable or disable HDR for this display.")
            };
            Details {
                resolution: ComboBox::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .automation_name(format!("Resolution for {}", m.name))
                    .items_source(resolutions.iter().map(|(w, h)| format!("{w} × {h}")))
                    .selected_index(resolution)
                    .placeholder_text("Unavailable while inactive")
                    .is_enabled(editable && !resolutions.is_empty())
                    .on_selection_changed(cx.callback(move |v| match v {
                        Some(v) => msg(target, i, Change::Resolution(v)),
                        None => route(target, DisplayMessage::Noop),
                    }))
                    .into(),
                rate: ComboBox::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .automation_name(format!("Refresh rate for {}", m.name))
                    .items_source(rates.iter().map(|r| r.label()))
                    .selected_index(rate)
                    .placeholder_text("Unavailable while inactive")
                    .is_enabled(editable && !rates.is_empty())
                    .on_selection_changed(cx.callback(move |v| match v {
                        Some(v) => msg(target, i, Change::Refresh(v)),
                        None => route(target, DisplayMessage::Noop),
                    }))
                    .into(),
                hdr,
                orientation: Grid::new().keyed_children([KeyedView::new(
                    format!("orientation-{i}-{revision}"),
                    Button::new()
                        .automation_name(format!("Orientation for {}", m.name))
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .horizontal_content_alignment(HorizontalAlignment::Stretch)
                        .is_enabled(editable)
                        .content(
                            Grid::new()
                                .columns([GridLength::Star(1.0), GridLength::Auto])
                                .column_spacing(12.0)
                                .children((
                                    Border::new()
                                        .horizontal_alignment(HorizontalAlignment::Left)
                                        .content(orientation(d.rotation_degrees)),
                                    // The 12 px chevron ComboBoxes use.
                                    Viewbox::new()
                                        .grid_column(1)
                                        .width(12.0)
                                        .height(12.0)
                                        .vertical_alignment(VerticalAlignment::Center)
                                        .slot(
                                            ViewboxSlot::Child,
                                            FontIcon::new().glyph("\u{e70d}"),
                                        ),
                                )),
                        )
                        .flyout_with(
                            Flyout::rich(
                                StackPanel::new().width(236.0).spacing(0.0).keyed_children(
                                    ORIENTATIONS
                                        .iter()
                                        .map(|(degrees, label)| {
                                            let degrees = *degrees;
                                            KeyedView::new(
                                                format!("rotation-{degrees}"),
                                                Button::new()
                                                    .height(36.0)
                                                    .style(ButtonStyle::Subtle)
                                                    .horizontal_alignment(
                                                        HorizontalAlignment::Stretch,
                                                    )
                                                    .automation_name(format!(
                                                        "{label} for {}",
                                                        m.name
                                                    ))
                                                    .on_click(cx.callback(move |()| {
                                                        msg(target, i, Change::Rotation(degrees))
                                                    }))
                                                    .content(orientation(degrees)),
                                            )
                                        })
                                        .collect::<Vec<_>>(),
                                ),
                            )
                            .placement(FlyoutPlacement::Bottom),
                        ),
                )]),
            }
        });

        let header: View = Grid::new()
            .columns([GridLength::Star(1.0), GridLength::Auto, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                TextBlock::new()
                    .text(format!("{}. {}", i + 1, m.name))
                    .font_size(18.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text_wrapping(TextWrapping::Wrap),
                if primary {
                    TextBlock::new()
                        .grid_column(1)
                        .text(if group.len() > 1 {
                            "★ Primary group"
                        } else {
                            "★ Primary"
                        })
                        .automation_name(format!("{} is the primary display", m.name))
                        .font_size(crate::ui::CAPTION)
                        .opacity(crate::ui::SECONDARY)
                        .vertical_alignment(VerticalAlignment::Center)
                        .into()
                } else if m.enabled && !detecting {
                    Button::new()
                        .grid_column(1)
                        .style(ButtonStyle::Subtle)
                        .vertical_alignment(VerticalAlignment::Center)
                        .is_enabled(editable)
                        .automation_name(format!("Make {} primary", m.name))
                        .on_click(cx.callback(move |()| msg(target, i, Change::Primary)))
                        .content("Make primary")
                } else {
                    View::empty()
                },
                Grid::new().grid_column(2).children((
                    ToggleSwitch::new()
                        .automation_name(format!("Enable {}", m.name))
                        .is_on(m.enabled)
                        .is_enabled(
                            !self.busy
                                && (!primary || group.len() > 1)
                                && (m.enabled
                                    || configured
                                        && (m.connected
                                            || options.warning.as_deref().is_some_and(|w| {
                                                w.starts_with("Previously detected")
                                            }))),
                        )
                        .on_toggled(cx.callback(move |v| msg(target, i, Change::Enabled(v))))
                        .tooltip("Include this display in the preset."),
                    if primary && group.len() == 1 {
                        // A disabled control does not receive hover input.
                        // This transparent, enabled layer owns its tooltip.
                        Border::new()
                            .background(Color::argb(0, 0, 0, 0))
                            .automation_name(format!("Why {} cannot be disabled", m.name))
                            .tooltip("Choose another primary display before disabling this one.")
                    } else {
                        View::empty()
                    },
                )),
            ));
        let preset = matches!(target, EditorTarget::Preset);
        let status = crate::ui::caption(if detecting {
            "Temporary detection · preset unchanged"
        } else if m.enabled && group.len() > 1 {
            "Mirrored desktop · shared arrangement and resolution"
        } else if m.enabled {
            if preset { "Active in this preset" } else { "" }
        } else if m.connected {
            if preset {
                "Disabled in this preset · connected"
            } else {
                "Off · connected"
            }
        } else {
            "Not detected · turn the display on"
        });
        let linked = self
            .tip
            .tvs
            .iter()
            .find(|tv| {
                tv.display.as_ref().is_some_and(|link| {
                    m.device_path
                        .as_ref()
                        .is_some_and(|path| link.device_path.eq_ignore_ascii_case(path))
                })
            })
            .map(|tv| {
                Grid::new()
                    .columns([GridLength::Star(1.0), GridLength::Auto])
                    .column_spacing(8.0)
                    .children((
                        crate::ui::caption(format!("Linked TV: {}", tv.name)),
                        Button::new()
                            .grid_column(1)
                            .style(ButtonStyle::Subtle)
                            .automation_name(format!("View linked TV {}", tv.name))
                            .is_enabled(!self.busy)
                            .on_click(cx.message(Message::Go("tvs".into())))
                            .content("View TV"),
                    ))
            })
            .unwrap_or_else(View::empty);
        let warning = options
            .warning
            .as_ref()
            .filter(|_| !detecting && self.display.setup.review != Some(i))
            .map(crate::ui::note)
            .unwrap_or_else(View::empty);
        MonitorParts {
            header,
            status,
            linked,
            warning,
            setup: if preset {
                self.display_setup_panel(i, cx)
            } else {
                View::empty()
            },
            mode: (m.enabled && !detecting).then_some(mode),
            details,
        }
    }
}
/// One display's editor pieces; see `monitor_parts`.
struct MonitorParts {
    header: View,
    status: View,
    linked: View,
    warning: View,
    setup: View,
    mode: Option<View>,
    details: Option<Details>,
}
/// The mode controls, present once the display's modes are known.
struct Details {
    resolution: View,
    rate: View,
    hdr: View,
    orientation: View,
}
