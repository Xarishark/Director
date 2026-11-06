//! Shared page vocabulary: type ramp, spacing, brushes and the page, section, card and footer
//! builders every page uses, so pages line up and follow the system theme.
use windows_reactor::*;

// Type ramp (Windows 11 / Fluent).
pub(crate) const CAPTION: f64 = 12.0;
pub(crate) const SUBTITLE: f64 = 20.0;
pub(crate) const TITLE: f64 = 28.0;
/// Opacity of secondary text: descriptions, hints, captions.
pub(crate) const SECONDARY: f64 = 0.72;

// Spacing.
pub(crate) const GAP_S: f64 = 8.0;
pub(crate) const GAP_M: f64 = 12.0;
pub(crate) const GAP_L: f64 = 16.0;
pub(crate) const GAP_XXL: f64 = 32.0;

// Corner radii: inner panels and controls, and cards.
pub(crate) const RADIUS_S: f64 = 4.0;
pub(crate) const RADIUS_L: f64 = 8.0;

/// The widest a form column gets (text boxes, pickers and their notes).
pub(crate) const FORM_WIDTH: f64 = 650.0;

// Shell geometry, used by layouts that size themselves from the window.
pub(crate) const PANE_OPEN: f64 = 200.0;
pub(crate) const PANE_COMPACT: f64 = 48.0;
pub(crate) const PAGE_PADDING_X: f64 = 28.0;
pub(crate) const PAGE_PADDING_Y: f64 = 20.0;

/// Green for a positive state marker (the active scene's dot), next to normal text.
pub(crate) const SUCCESS: Color = Color::rgb(108, 203, 95);
/// A faint translucent layer for panels inside a card (previews, hero panels).
pub(crate) const SUBTLE_FILL: Color = Color::argb(18, 128, 128, 128);
/// The background behind commands and other monospace text.
pub(crate) const CODE_FILL: Color = Color::argb(40, 128, 128, 128);

// Screens in layout previews and the arrangement map: a dark panel that reads on both themes,
// outlined in coral for the primary display.
pub(crate) const SCREEN_FILL: Color = Color::rgb(38, 52, 69);
pub(crate) const SCREEN_PRIMARY: Color = Color::rgb(255, 148, 113);
pub(crate) const SCREEN_OTHER: Color = Color::rgb(150, 174, 197);
pub(crate) const SCREEN_TEXT: Color = Color::rgb(240, 244, 249);
/// A visible outline around a work area such as the arrangement map, on either theme.
pub(crate) const OUTLINE: Color = Color::argb(110, 128, 128, 128);

/// The card fill and outline, from the system theme.
pub(crate) fn card_fill() -> Brush {
    ThemeBrush::CardBackground.into()
}
pub(crate) fn card_stroke() -> Brush {
    ThemeBrush::CardStroke.into()
}
pub(crate) fn critical() -> Brush {
    ThemeBrush::SystemCritical.into()
}

/// The content width left for a page in a window of `window_width`.
pub(crate) fn page_width(window_width: f64, pane_open: bool) -> f64 {
    let pane = if pane_open { PANE_OPEN } else { PANE_COMPACT };
    window_width - pane - 2.0 * PAGE_PADDING_X
}

/// Secondary wrapping text. Empty text renders nothing.
pub(crate) fn note(text: impl Into<String>) -> View {
    let text = text.into();
    if text.is_empty() {
        return View::empty();
    }
    TextBlock::new()
        .text(text)
        .text_wrapping(TextWrapping::Wrap)
        .opacity(SECONDARY)
        .into()
}
/// Small secondary text for fine print.
pub(crate) fn caption(text: impl Into<String>) -> View {
    let text = text.into();
    if text.is_empty() {
        return View::empty();
    }
    TextBlock::new()
        .text(text)
        .font_size(CAPTION)
        .text_wrapping(TextWrapping::Wrap)
        .opacity(SECONDARY)
        .into()
}
/// A field label or small heading.
pub(crate) fn label(text: impl Into<String>) -> TextBlock {
    TextBlock::new()
        .text(text.into())
        .font_weight(FontWeight::SEMI_BOLD)
        .text_wrapping(TextWrapping::Wrap)
}
/// A section heading.
pub(crate) fn subtitle(text: impl Into<String>) -> TextBlock {
    TextBlock::new()
        .text(text.into())
        .font_size(SUBTITLE)
        .font_weight(FontWeight::SEMI_BOLD)
        .text_wrapping(TextWrapping::Wrap)
}

/// The page heading: title, one-line subtitle, and the page's buttons at the top right.
pub(crate) fn header(title: &str, subtitle: &str, actions: Vec<View>) -> View {
    Grid::new()
        .grid_row(0)
        .columns([GridLength::Star(1.0), GridLength::Auto])
        .column_spacing(GAP_M)
        .margin(Thickness::new(0.0, 0.0, 0.0, GAP_L))
        .children((
            StackPanel::new().grid_column(0).spacing(2.0).children((
                TextBlock::new()
                    .text(title)
                    .font_size(TITLE)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_wrapping(TextWrapping::Wrap),
                note(subtitle),
            )),
            StackPanel::new()
                .grid_column(1)
                .orientation(Orientation::Horizontal)
                .spacing(GAP_S)
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
        .margin(Thickness::new(0.0, 0.0, GAP_S, 0.0))
        .vertical_alignment(VerticalAlignment::Center)
        .children((
            TextBlock::new()
                .text(label)
                .font_size(CAPTION)
                .opacity(SECONDARY)
                .horizontal_alignment(HorizontalAlignment::Right),
            TextBlock::new()
                .text(value)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .horizontal_alignment(HorizontalAlignment::Right),
        ))
}

/// A page: fixed header, scrolling body, and an optional fixed footer.
pub(crate) fn page(header: View, body: View, footer: Option<View>) -> View {
    Grid::new()
        .rows([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
        .children((
            Border::new().grid_row(0).content(header),
            ScrollViewer::new()
                .grid_row(1)
                .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                .content(
                    // Room for the vertical scroll bar so it never covers controls.
                    Border::new()
                        .padding(Thickness::new(0.0, 0.0, GAP_L, GAP_S))
                        .content(body),
                ),
            match footer {
                Some(footer) => Border::new().grid_row(2).content(footer),
                None => View::empty(),
            },
        ))
}
/// A left-aligned column no wider than `width`.
pub(crate) fn column(width: f64, spacing: f64, children: impl IntoViews) -> View {
    StackPanel::new()
        .max_width(width)
        .horizontal_alignment(HorizontalAlignment::Left)
        .spacing(spacing)
        .children(children)
}

/// A titled section: subtitle with an optional action at its right, a one-line description,
/// then the body.
pub(crate) fn section(title: &str, description: &str, action: Option<View>, body: View) -> View {
    StackPanel::new().spacing(GAP_M).children((
        StackPanel::new().spacing(2.0).children((
            Grid::new()
                .columns([GridLength::Star(1.0), GridLength::Auto])
                .column_spacing(GAP_M)
                .children((
                    subtitle(title).vertical_alignment(VerticalAlignment::Center),
                    match action {
                        Some(action) => Border::new()
                            .grid_column(1)
                            .vertical_alignment(VerticalAlignment::Center)
                            .content(action),
                        None => View::empty(),
                    },
                )),
            note(description),
        )),
        body,
    ))
}

/// Work in progress inside a section: a small spinning ring and a line of text.
pub(crate) fn loading(text: &str) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(GAP_M)
        .children((
            ProgressRing::new().is_active(true).width(20.0).height(20.0),
            note(text),
        ))
}

/// A bordered card in the system card colours.
pub(crate) fn card_padded(padding: Thickness, body: impl Into<View>) -> View {
    Border::new()
        .padding(padding)
        .corner_radius(RADIUS_L)
        .background(card_fill())
        .border_brush(card_stroke())
        .border_thickness(1.0)
        .content(body)
}

/// A settings row in a card, as in Windows Settings: title and description on the left,
/// the control on the right.
pub(crate) fn setting(title: &str, description: &str, control: impl Into<View>) -> View {
    card_padded(
        Thickness::new(GAP_L, GAP_M, GAP_L, GAP_M),
        Grid::new()
            .columns([GridLength::Star(1.0), GridLength::Auto])
            .column_spacing(GAP_L)
            .children((
                StackPanel::new()
                    .spacing(2.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((label(title), caption(description))),
                Border::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(control),
            )),
    )
}

/// A full-width rule with a caption under it, separating parts of a page that behave
/// differently.
pub(crate) fn divider(caption_text: &str) -> View {
    StackPanel::new().spacing(GAP_S).children((
        Border::new()
            .height(1.0)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .background(OUTLINE),
        caption(caption_text),
    ))
}

/// An editor footer: a hint on the left, the buttons (Cancel, then the accent action) on the right.
pub(crate) fn footer(hint: View, actions: Vec<View>) -> View {
    Grid::new()
        .columns([GridLength::Star(1.0), GridLength::Auto])
        .column_spacing(GAP_L)
        .margin(Thickness::new(0.0, GAP_L, 0.0, 0.0))
        .children((
            Border::new()
                .grid_column(0)
                .vertical_alignment(VerticalAlignment::Center)
                .content(hint),
            StackPanel::new()
                .grid_column(1)
                .orientation(Orientation::Horizontal)
                .spacing(GAP_S)
                .vertical_alignment(VerticalAlignment::Center)
                .keyed_children(
                    actions
                        .into_iter()
                        .enumerate()
                        .map(|(i, view)| KeyedView::new(format!("footer-{i}"), view))
                        .collect::<Vec<_>>(),
                ),
        ))
}

/// An empty page: a heading, a line of explanation and an optional button.
pub(crate) fn empty_state(title: &str, text: &str, action: Option<View>) -> View {
    StackPanel::new()
        .spacing(GAP_S)
        .max_width(FORM_WIDTH)
        .horizontal_alignment(HorizontalAlignment::Left)
        .margin(Thickness::new(0.0, GAP_S, 0.0, 0.0))
        .children((
            subtitle(title),
            note(text),
            match action {
                Some(action) => Border::new()
                    .margin(Thickness::new(0.0, GAP_S, 0.0, 0.0))
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .content(action),
                None => View::empty(),
            },
        ))
}
