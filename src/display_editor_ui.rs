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
) -> View {
    let drag = match target {
        EditorTarget::Preset => app.display.drag.as_ref(),
        EditorTarget::Current => app.current.display.drag.as_ref(),
    };
    let map = drag.map_or_else(
        || director::display_editor::MapTransform::new(layout),
        |d| d.map,
    );
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
                    .background(Brush::Solid(Color::rgb(38, 52, 69)))
                    .border_brush(Brush::Solid(if primary {
                        Color::rgb(255, 148, 113)
                    } else {
                        Color::rgb(155, 180, 207)
                    }))
                    .border_thickness(Thickness::uniform(2.0))
                    .corner_radius(CornerRadius::uniform(5.0))
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
                            .font_size(18.0)
                            .foreground(Color::rgb(240, 244, 249))
                            .horizontal_alignment(HorizontalAlignment::Center)
                            .vertical_alignment(VerticalAlignment::Center),
                    )
                    .tooltip(format!(
                        "{}{}
{} × {} · {}
Position: {}, {}
Drag to move this screen.",
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
    Border::new()
        .horizontal_alignment(HorizontalAlignment::Left)
        .border_thickness(Thickness::uniform(1.0))
        .border_brush(Brush::Solid(Color::rgb(100, 110, 125)))
        .corner_radius(CornerRadius::uniform(8.0))
        .padding(Thickness::uniform(10.0))
        .content(
            Grid::new()
                .width(400.0)
                .height(180.0)
                .automation_name("Display arrangement")
                .keyed_children(cards),
        )
}
impl DirectorApp {
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
        let revision = match target {
            EditorTarget::Preset => self.display.orientation_revision,
            EditorTarget::Current => self.current.display.orientation_revision,
        };
        let columns = if matches!(target, EditorTarget::Current) {
            1
        } else {
            (((self.window_width - 380.0) / 430.0).floor() as usize).clamp(1, 3)
        };
        let cards = layout
            .monitors
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let options=director::display_mirroring::common_options(editor,layout,i);
                let group=director::display_mirroring::members(layout,i);
                let mirror_targets=layout.monitors.iter().enumerate().filter(|(j,m)|*j!=i&&m.enabled).map(|(j,_)|j).collect::<Vec<_>>();
                let mirror_selected=group.iter().copied().find(|j|*j!=i).and_then(|j|mirror_targets.iter().position(|v|*v==j)).map_or(Some(0),|j|Some(j+1));
                let mirror_labels=std::iter::once("Extend desktop".to_string()).chain(mirror_targets.iter().map(|j|format!("Mirror with {}. {}",j+1,layout.monitors[*j].name))).collect::<Vec<_>>();
                let mirror_select_targets=mirror_targets.clone();
                let mirror_mode=ComboBox::new().horizontal_alignment(HorizontalAlignment::Stretch)
                    .automation_name(format!("Desktop mode for {}",m.name)).items_source(mirror_labels)
                    .selected_index(mirror_selected).is_enabled(!self.busy&&m.enabled)
                    .on_selection_changed(cx.callback(move|selected|match selected{
                        Some(0)=>msg(target,i,Change::Mirror(None)),
                        Some(n)=>mirror_select_targets.get(n-1).map_or(route(target,DisplayMessage::Noop),|j|msg(target,i,Change::Mirror(Some(*j)))),
                        None=>route(target,DisplayMessage::Noop),
                    })).slot(ComboBoxSlot::Header,"Desktop mode");
                let d = m.desktop.as_ref();
                let resolutions = options.resolutions();
                let resolution = d.and_then(|d| {
                    resolutions
                        .iter()
                        .position(|r| *r == (d.source_width, d.source_height))
                });
                let rates = d
                    .map(|d| options.rates(d.source_width, d.source_height))
                    .unwrap_or_default();
                let rate = d.and_then(|d| rates.iter().position(|r| *r == d.refresh_rate));
                let primary = d.is_some_and(DesktopMode::primary);
                let detecting = matches!(target,EditorTarget::Preset) && self.display.setup.index==Some(i) && self.display.setup.stage.is_some();
                let editable = !self.busy && m.enabled;
                let configured = !options.modes.is_empty() && options.warning.as_deref().is_none_or(|w|w.starts_with("Previously detected"));
                let hdr_toggle=ToggleSwitch::new().automation_name(format!("HDR for {}",m.name))
                    .is_on(m.hdr.or(options.hdr).unwrap_or(false))
                    .is_enabled(editable&&options.hdr.is_some())
                    .on_toggled(cx.callback(move|on|msg(target,i,Change::Hdr(on))))
                    .slot(ToggleSwitchSlot::Header,"HDR");
                let hdr_control:View=if options.hdr.is_none(){
                    Grid::new().children((
                        hdr_toggle,
                        Border::new().background(Color::argb(0,0,0,0))
                            .automation_name(format!("Why HDR is unavailable for {}",m.name))
                            .tooltip("HDR is unavailable for this display or Windows version."),
                    ))
                }else{
                    hdr_toggle.tooltip("Enable or disable HDR for this display.")
                };
                let details: View = if let Some(d) = d.filter(|_|configured) {
                    StackPanel::new().spacing(12.0).children((
                        Grid::new()
                            .columns([GridLength::Star(1.0), GridLength::Star(1.0)])
                            .column_spacing(12.0)
                            .children((
                                ComboBox::new()
                                    .grid_column(0)
                                    .horizontal_alignment(HorizontalAlignment::Stretch)
                                    .automation_name(format!("Resolution for {}", m.name))
                                    .items_source(
                                        resolutions.iter().map(|(w, h)| format!("{w} × {h}")),
                                    )
                                    .selected_index(resolution)
                                    .placeholder_text("Unavailable while inactive")
                                    .is_enabled(editable && !resolutions.is_empty())
                                    .on_selection_changed(cx.callback(move |v| match v {
                                        Some(v) => msg(target,i, Change::Resolution(v)),
                                        None => route(target,DisplayMessage::Noop),
                                    }))
                                    .slot(ComboBoxSlot::Header, "Resolution"),
                                ComboBox::new()
                                    .grid_column(1)
                                    .horizontal_alignment(HorizontalAlignment::Stretch)
                                    .automation_name(format!("Refresh rate for {}", m.name))
                                    .items_source(rates.iter().map(|r| r.label()))
                                    .selected_index(rate)
                                    .placeholder_text("Unavailable while inactive")
                                    .is_enabled(editable && !rates.is_empty())
                                    .on_selection_changed(cx.callback(move |v| match v {
                                        Some(v) => msg(target,i, Change::Refresh(v)),
                                        None => route(target,DisplayMessage::Noop),
                                    }))
                                    .slot(ComboBoxSlot::Header, TextBlock::new().text("Refresh rate").tooltip("Only rates for the selected resolution are shown. Windows marks rates that require a different resolution with an asterisk.")),
                            )),
                        hdr_control,
                        StackPanel::new().spacing(4.0).children((
                            TextBlock::new().text("Orientation"),
                            Grid::new().keyed_children([KeyedView::new(format!("orientation-{i}-{revision}"),
                            Button::new()
                                .automation_name(format!("Orientation for {}", m.name))
                                .is_enabled(editable)
                                .content(
                                    StackPanel::new()
                                        .orientation(Orientation::Horizontal)
                                        .spacing(12.0)
                                        .children((
                                            orientation(d.rotation_degrees),
                                            FontIcon::new()
                                                .glyph("")
                                                .vertical_alignment(VerticalAlignment::Center),
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
                                                                msg(target,i, Change::Rotation(degrees))
                                                            }))
                                                            .content(orientation(degrees)),
                                                    )
                                                })
                                                .collect::<Vec<_>>(),
                                        ),
                                    )
                                    .placement(FlyoutPlacement::Bottom),
                                ))]),
                        )),
                    ))
                } else {
                    View::empty()
                };
                KeyedView::new(
                    format!("editor-{i}"),
                    Border::new()
                        .grid_column((i % columns) as i32).grid_row((i / columns) as i32)
                        .vertical_alignment(VerticalAlignment::Top)
                        .padding(Thickness::uniform(16.0))
                        .border_thickness(Thickness::uniform(1.0))
                        .border_brush(Brush::Solid(Color::rgb(90, 90, 90)))
                        .corner_radius(CornerRadius::uniform(8.0))
                        .content(
                            StackPanel::new().spacing(12.0).children((
                                Grid::new().columns([GridLength::Star(1.0), GridLength::Auto, GridLength::Auto])
                                    .column_spacing(12.0).children((
                                        TextBlock::new().text(format!("{}. {}", i + 1, m.name))
                                            .font_size(18.0).font_weight(FontWeight::SEMI_BOLD)
                                            .vertical_alignment(VerticalAlignment::Center)
                                            .text_wrapping(TextWrapping::Wrap),
                                        if primary {
                                            TextBlock::new().grid_column(1).text(if group.len()>1 {"★ Primary group"}else{"★ Primary"})
                                                .automation_name(format!("{} is the primary display",m.name))
                                                .font_size(12.0).opacity(0.72)
                                                .vertical_alignment(VerticalAlignment::Center).into()
                                        } else if m.enabled && !detecting {
                                            Button::new().grid_column(1).style(ButtonStyle::Subtle)
                                                .vertical_alignment(VerticalAlignment::Center)
                                                .is_enabled(editable)
                                                .automation_name(format!("Make {} primary",m.name))
                                                .on_click(cx.callback(move|()|msg(target,i,Change::Primary)))
                                                .content("Make primary")
                                        } else {View::empty()},
                                        Grid::new().grid_column(2).children((
                                            ToggleSwitch::new()
                                                .automation_name(format!("Enable {}",m.name))
                                                .is_on(m.enabled)
                                                .is_enabled(!self.busy && (!primary || group.len()>1) && (m.enabled || configured && (m.connected || options.warning.as_deref().is_some_and(|w|w.starts_with("Previously detected")))))
                                                .on_toggled(cx.callback(move|v|msg(target,i,Change::Enabled(v))))
                                                .tooltip("Include this display in the preset."),
                                            if primary && group.len()==1 {
                                                // A disabled control does not receive hover input.
                                                // This transparent, enabled layer owns its tooltip.
                                                Border::new().background(Color::argb(0,0,0,0))
                                                    .automation_name(format!("Why {} cannot be disabled",m.name))
                                                    .tooltip("Choose another primary display before disabling this one.")
                                            }else{View::empty()},
                                        )),
                                    )),
                                TextBlock::new().text(if detecting {"Temporary detection · preset unchanged"} else if m.enabled && group.len()>1 { "Mirrored desktop · shared arrangement and resolution" } else if m.enabled { "Active in this preset" } else if m.connected { "Disabled in this preset · connected" } else { "Not detected · turn the display on" }).font_size(12.0).opacity(0.65),
                                self.tip.tvs.iter().find(|tv| tv.display.as_ref().is_some_and(|link| m.device_path.as_ref().is_some_and(|path| link.device_path.eq_ignore_ascii_case(path)))).map(|tv| {
                                    Grid::new().columns([GridLength::Star(1.0), GridLength::Auto]).column_spacing(8.0).children((
                                        TextBlock::new().text(format!("Linked TV: {}", tv.name)).font_size(12.0).opacity(0.72).vertical_alignment(VerticalAlignment::Center).text_wrapping(TextWrapping::Wrap),
                                        Button::new().grid_column(1).style(ButtonStyle::Subtle).automation_name(format!("View linked TV {}", tv.name)).is_enabled(!self.busy)
                                            .on_click(cx.message(Message::Navigate(Some("tvs".into())))).content("View TV"),
                                    ))
                                }).unwrap_or_else(View::empty),
                                options
                                    .warning
                                    .as_ref().filter(|_|!detecting && self.display.setup.review!=Some(i))
                                    .map(|w| {
                                        TextBlock::new()
                                            .text(w)
                                            .opacity(0.72)
                                            .text_wrapping(TextWrapping::Wrap)
                                            .into()
                                    })
                                    .unwrap_or_else(View::empty),
                                if matches!(target,EditorTarget::Preset) { self.display_setup_panel(i,cx) } else {View::empty()},
                                if m.enabled && !detecting {mirror_mode}else{View::empty()},
                                details,
                            )),
                        ),
                )
            })
            .collect::<Vec<_>>();
        StackPanel::new().spacing(16.0).children((
            TextBlock::new()
                .text("Arrange your displays")
                .font_size(20.0)
                .font_weight(FontWeight::SEMI_BOLD),
            TextBlock::new()
                .text("Drag screens to arrange them. Hover for details.")
                .text_wrapping(TextWrapping::Wrap)
                .opacity(0.72),
            preview(self, layout, cx, target),
            Grid::new()
                .columns((0..columns).map(|_| GridLength::Star(1.0)))
                .rows((0..layout.monitors.len().div_ceil(columns)).map(|_| GridLength::Auto))
                .column_spacing(16.0)
                .row_spacing(16.0)
                .keyed_children(cards),
        ))
    }
}
