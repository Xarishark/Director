//! Shared preset-page heading and action placement.
use windows_reactor::*;
pub(crate) fn header(title: &str, subtitle: &str, actions: Vec<View>) -> View {
    Grid::new()
        .grid_row(0)
        .columns([GridLength::Star(1.0), GridLength::Auto])
        .column_spacing(12.0)
        .margin(Thickness::new(0.0, 0.0, 0.0, 16.0))
        .children((
            StackPanel::new().grid_column(0).spacing(2.0).children((
                TextBlock::new()
                    .text(title)
                    .font_size(28.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(subtitle)
                    .opacity(0.72)
                    .text_wrapping(TextWrapping::Wrap),
            )),
            StackPanel::new()
                .grid_column(1)
                .orientation(Orientation::Horizontal)
                .spacing(8.0)
                .vertical_alignment(VerticalAlignment::Top)
                .keyed_children(
                    actions
                        .into_iter()
                        .enumerate()
                        .map(|(i, view)| KeyedView::new(format!("action-{i}"), view))
                        .collect::<Vec<_>>(),
                ),
        ))
}
/// What's active right now, shown in the header row before the page's buttons.
pub(crate) fn current(label: &str, value: String) -> View {
    StackPanel::new()
        .spacing(1.0)
        .max_width(320.0)
        .margin(Thickness::new(0.0, 0.0, 8.0, 0.0))
        .vertical_alignment(VerticalAlignment::Center)
        .children((
            TextBlock::new()
                .text(label)
                .font_size(10.0)
                .opacity(0.65)
                .horizontal_alignment(HorizontalAlignment::Right),
            TextBlock::new()
                .text(value)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .horizontal_alignment(HorizontalAlignment::Right),
        ))
}
