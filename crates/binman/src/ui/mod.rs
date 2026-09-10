mod explorer;
mod header;
mod overlays;
mod request;
mod response;
mod status;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::app::line::LineInput;
use crate::app::mouse::Targets;
use crate::theme;

/// v1's sidebar width, held to two fifths of a narrow terminal so the request
/// and the response keep most of it.
const EXPLORER_WIDTH: u16 = 48;

/// v1's arrangement: the header with the environment picker at its right, the
/// URL across the whole width, then the collections beside the request over
/// the response.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme::body()), area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    let explorer_width = EXPLORER_WIDTH.min(rows[3].width * 2 / 5);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(explorer_width), Constraint::Min(20)])
        .split(rows[3]);

    // Filled as each part is drawn, so a click is looked up in exactly what
    // is on screen.
    let mut targets = Targets::default();
    header::draw(frame, app, rows[0], &mut targets);
    request::draw_tabs(frame, app, rows[1], &mut targets);
    request::draw_url(frame, app, rows[2], &mut targets);
    explorer::draw(frame, app, columns[0], &mut targets);
    draw_workspace(frame, app, columns[1], &mut targets);
    status::draw(frame, app, rows[4], &mut targets);

    // Everything recedes behind an open modal, so the modal is plainly the
    // thing being talked to and the layout stays as context rather than as
    // competition.
    if app.overlay.is_some() {
        recede(frame, area, theme::SCRIM);
    }
    overlays::draw(frame, app, area, &mut targets);
    app.targets = targets;
}

/// Blends every cell in `area` toward the background.
///
/// Done to the finished buffer rather than by restyling each widget: the
/// alternative is every draw function taking a "dimmed" flag and remembering to
/// honour it, which is correct on the day it is written and wrong a month
/// later.
fn recede(frame: &mut Frame, area: Rect, amount: f32) {
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            cell.fg = theme::recede(cell.fg, amount);
            cell.bg = theme::recede(cell.bg, amount);
        }
    }
}

/// The response gets the larger share: it is what is read, while the request
/// is mostly a few headers. Two to five, as v1 split them.
fn draw_workspace(frame: &mut Frame, app: &mut App, area: Rect, targets: &mut Targets) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(2), Constraint::Fill(5)])
        .split(area);

    request::draw(frame, app, rows[0], targets);
    response::draw(frame, app, rows[1], targets);
}

/// The bordered frame every pane and overlay shares.
///
/// binvim's popup form: the title sits in the top border after a single dash,
/// and a counter sits at the right end of the same border rather than
/// competing with the content for a line.
pub fn pane(title: &str, focused: bool) -> Block<'static> {
    framed(title, Vec::new(), focused, theme::chrome())
}

/// A pane over the body surface rather than the chrome one: the URL, the
/// request and the response, which are content, not chrome.
pub fn body_pane(title: &str, counter: Vec<Span<'static>>, focused: bool) -> Block<'static> {
    framed(title, counter, focused, theme::body())
}

pub fn counted_pane(title: &str, counter: impl Into<String>, focused: bool) -> Block<'static> {
    let counter = vec![Span::styled(counter.into(), theme::counter())];
    framed(title, counter, focused, theme::chrome())
}

fn framed(title: &str, counter: Vec<Span<'static>>, focused: bool, surface: Style) -> Block<'static> {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title_top(Line::from(vec![
            Span::styled("─", theme::border(focused)),
            Span::styled(format!(" {title} "), theme::title(focused)),
        ]))
        .style(surface);
    with_counter(block, counter, focused)
}

/// Puts styled spans at the right end of a block's top border.
fn with_counter(block: Block<'static>, counter: Vec<Span<'static>>, focused: bool) -> Block<'static> {
    if counter.is_empty() {
        return block;
    }
    let mut spans = vec![Span::raw(" ")];
    spans.extend(counter);
    spans.push(Span::raw(" "));
    spans.push(Span::styled("─", theme::border(focused)));
    block.title_top(Line::from(spans).right_aligned())
}

/// A pane whose sections sit in its top border as tabs, the current one lit,
/// with a counter of styled spans at the right end.
pub fn tabbed_pane(
    sections: Vec<(String, bool)>,
    counter: Vec<Span<'static>>,
    focused: bool,
) -> Block<'static> {
    let mut title = vec![
        Span::styled("─", theme::border(focused)),
        Span::raw(" "),
    ];
    for (index, (label, active)) in sections.into_iter().enumerate() {
        if index > 0 {
            title.push(Span::raw("  "));
        }
        title.push(Span::styled(label, theme::section(active, focused)));
    }
    title.push(Span::raw(" "));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title_top(Line::from(title))
        .style(theme::body());
    with_counter(block, counter, focused)
}

/// Centres a box of an explicit size, clamped to fit `area`.
pub fn centered_size(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Centres a box horizontally but pins its top, so a box whose height changes
/// — a filtered list — grows downwards instead of shifting under the cursor.
pub fn anchored_size(area: Rect, width: u16, height: u16, from_top: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let top = from_top.min(area.height.saturating_sub(height));
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + top,
        width,
        height,
    }
}

/// One row of `area` from column `x`, cut to fit: where something drawn on
/// that row can be clicked.
pub fn spot(area: Rect, x: u16, width: u16) -> Rect {
    Rect {
        x,
        y: area.y,
        width,
        height: 1,
    }
    .intersection(area)
}

/// The `row`th line of `area`, or nothing when it falls outside.
pub fn line_at(area: Rect, row: usize) -> Rect {
    let y = area.y.saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
    Rect { y, height: 1, ..area }.intersection(area)
}

/// Where each section's name sits in a tabbed pane's top border, as
/// `tabbed_pane` draws them: after the corner, a dash and a space, two apart.
pub fn section_spots<'a>(area: Rect, labels: impl IntoIterator<Item = &'a str>) -> Vec<Rect> {
    let border = border_row(area);
    let mut x = area.x.saturating_add(3);
    labels
        .into_iter()
        .map(|label| {
            let width = u16::try_from(UnicodeWidthStr::width(label)).unwrap_or(u16::MAX);
            let at = spot(border, x, width);
            x = x.saturating_add(width).saturating_add(2);
            at
        })
        .collect()
}

/// Where a counter `width` columns wide sits at the right end of a pane's top
/// border, as `with_counter` draws it.
pub fn counter_spot(area: Rect, width: u16) -> Rect {
    spot(
        border_row(area),
        area.right().saturating_sub(width.saturating_add(3)),
        width,
    )
}

/// A pane's top border, between its corners.
fn border_row(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.min(1),
        ..area
    }
}

/// Hangs a box from the right end of the header, the way a dropdown opens
/// under the control that opened it.
pub fn dropdown_size(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height.saturating_sub(1));
    Rect {
        x: area.right().saturating_sub(width + 1).max(area.x),
        y: area.y + 1,
        width,
        height,
    }
}

/// Cuts a string to `width` display columns, marking that it was cut.
pub fn truncate(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }

    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width - 1 {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

pub fn pad(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    format!("{}{}", text, " ".repeat(width.saturating_sub(used)))
}

pub fn width_of(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

/// Keeps `selected` inside a window of `height` rows, adjusting `offset` by
/// the least amount that works.
pub fn scroll_offset(offset: usize, selected: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    if selected < offset {
        selected
    } else if selected >= offset + height {
        selected + 1 - height
    } else {
        offset
    }
}

/// Column zero of a row: the selection bar when the row is selected, a space
/// when it is not, so marking a row never shifts its contents sideways.
pub fn bar(selected: bool, focused: bool) -> Span<'static> {
    if selected {
        Span::styled(
            theme::SELECTION_BAR.to_string(),
            theme::selection_bar(focused),
        )
    } else {
        Span::raw(" ")
    }
}

/// The bar for a row being typed into, so the row says it is taking typing
/// before the field in it is read.
pub fn typing_bar() -> Span<'static> {
    Span::styled(theme::SELECTION_BAR.to_string(), theme::typing_bar())
}

/// Marks the selected row the way binvim's picker does: an accent bar down the
/// left, then a surface background under the rest. The bar carries the
/// emphasis, so each span keeps its own foreground and the colours of what
/// the row says survive being selected — and a span with a background of its
/// own, a field being typed into, keeps that too.
pub fn highlight(line: Line<'static>, focused: bool, width: u16) -> Line<'static> {
    let background = theme::selection(focused);
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(line.spans.len() + 1);

    let mut remaining = width as usize;
    for (index, span) in line.spans.into_iter().enumerate() {
        let content: String = span.content.into_owned();
        let trimmed = if UnicodeWidthStr::width(content.as_str()) > remaining {
            truncate(&content, remaining)
        } else {
            content
        };
        remaining = remaining.saturating_sub(UnicodeWidthStr::width(trimmed.as_str()));
        let style = if index == 0 || span.style.bg.is_some() {
            span.style
        } else {
            span.style.patch(background)
        };
        spans.push(Span::styled(trimmed, style));
    }

    if remaining > 0 {
        spans.push(Span::styled(" ".repeat(remaining), background));
    }
    Line::from(spans)
}

/// How many rows a line of `line_width` columns takes when wrapped to `width`.
pub fn rows_for(line_width: usize, width: usize) -> usize {
    if width == 0 || line_width == 0 {
        1
    } else {
        line_width.div_ceil(width)
    }
}

/// Wraps a styled line to `width` columns, a character at a time, each piece
/// keeping the style it had.
pub fn wrap_line(line: &Line<'_>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::default()];
    }
    let mut out = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut used = 0;

    for span in &line.spans {
        let mut piece = String::new();
        for ch in span.content.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + ch_width > width && used > 0 {
                if !piece.is_empty() {
                    current.push(Span::styled(std::mem::take(&mut piece), span.style));
                }
                out.push(Line::from(std::mem::take(&mut current)));
                used = 0;
            }
            piece.push(ch);
            used += ch_width;
        }
        if !piece.is_empty() {
            current.push(Span::styled(piece, span.style));
        }
    }
    out.push(Line::from(current));
    out
}

/// A line input as spans: the window of its text that keeps the cursor on
/// screen, each character styled by `style_of` (given its byte offset), and
/// the cursor cell marked when the input has focus.
pub fn input_line(
    input: &LineInput,
    width: u16,
    focused: bool,
    style_of: impl Fn(usize) -> Style,
) -> Line<'static> {
    Line::from(input_spans(input, width, focused, style_of, theme::cursor()))
}

/// A line input being typed into inside a row of a list: a well of its own
/// colour across the width it may fill, and a caret that shows whatever colour
/// the row is.
pub fn field(input: &LineInput, width: u16) -> Vec<Span<'static>> {
    let mut spans = input_spans(input, width, true, |_| theme::field(), theme::caret());
    let used = width_of(&spans);
    spans.push(Span::styled(
        " ".repeat((width as usize).saturating_sub(used)),
        theme::field(),
    ));
    spans
}

fn input_spans(
    input: &LineInput,
    width: u16,
    focused: bool,
    style_of: impl Fn(usize) -> Style,
    cursor: Style,
) -> Vec<Span<'static>> {
    let width = width as usize;
    let start = if focused { input.first_visible(width) } else { 0 };
    let mut runs = Runs::default();
    let mut used = 0;

    for (index, (offset, ch)) in input.text().char_indices().enumerate().skip(start) {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        let mut style = style_of(offset);
        if focused && index == input.cursor() {
            style = style.patch(cursor);
        }
        runs.push(ch, style);
        used += ch_width;
    }
    if focused && input.cursor() >= input.text().chars().count() && used < width {
        runs.push(' ', style_of(input.text().len()).patch(cursor));
    }
    runs.finish()
}

/// Characters gathered into spans, one span per run of the same style.
#[derive(Default)]
struct Runs {
    spans: Vec<Span<'static>>,
    run: String,
    style: Style,
}

impl Runs {
    fn push(&mut self, ch: char, style: Style) {
        if style != self.style && !self.run.is_empty() {
            self.spans
                .push(Span::styled(std::mem::take(&mut self.run), self.style));
        }
        self.style = style;
        self.run.push(ch);
    }

    fn finish(mut self) -> Vec<Span<'static>> {
        if !self.run.is_empty() {
            self.spans.push(Span::styled(self.run, self.style));
        }
        self.spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_marks_what_it_cut() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello w…");
        assert_eq!(truncate("hello", 1), "…");
    }

    #[test]
    fn scrolling_moves_the_minimum() {
        assert_eq!(scroll_offset(0, 5, 10), 0);
        assert_eq!(scroll_offset(0, 12, 10), 3);
        assert_eq!(scroll_offset(10, 4, 10), 4);
    }

    #[test]
    fn a_wrapped_line_keeps_every_character_and_its_style() {
        let line = Line::from(vec![
            Span::styled("abcd", theme::json_key()),
            Span::styled("efgh", theme::json_string()),
        ]);
        let wrapped = wrap_line(&line, 3);
        let text: Vec<String> = wrapped
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
            .collect();
        assert_eq!(text, vec!["abc", "def", "gh"]);
        assert_eq!(wrapped[1].spans[0].style, theme::json_key());
        assert_eq!(wrapped[1].spans[1].style, theme::json_string());
    }
}
