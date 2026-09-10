//! The header line.
//!
//! v1's: which request this is on the left, and the environment picker on the
//! right, where v1 kept its dropdown. ⌃E opens the list, and it drops from the
//! picker rather than from the middle of the screen.
//!
//! The left is styled after Claude Code: the mark carries the only colour, and
//! everything after it is quiet text separated by `·`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::theme;
use crate::ui;

/// v1's dropdown was sixteen wide; a label past that is cut rather than let
/// push the request's name off the line.
const PICKER_LABEL: usize = 16;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    // The name gives way before the picker does: the picker is the control,
    // the name only context. The picker goes only when it cannot fit at all.
    let mut right = picker(app);
    if ui::width_of(&right) > area.width as usize {
        right.clear();
    }

    let left = identity(app);
    let left_width = ui::width_of(&left);
    let room = (area.width as usize).saturating_sub(ui::width_of(&right));
    let mut spans = if left_width > room {
        truncate_spans(left, room)
    } else {
        let mut spans = left;
        spans.push(Span::styled(" ".repeat(room - left_width), theme::status_bar()));
        spans
    };
    spans.extend(right);

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

/// Which request this is.
fn identity(app: &App) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(format!(" {}  ", theme::MARK), theme::brand())];
    let tab = app.tab();

    if tab.origin.is_none() && tab.url.text().trim().is_empty() {
        // Nothing to name yet, so the header says which program this is.
        spans.push(Span::styled("binman", theme::muted()));
        spans.push(Span::styled("  ·  ", theme::dim()));
        spans.push(Span::styled("new request", theme::dim()));
        return spans;
    }

    spans.push(Span::styled(app.describe(tab), theme::title(true)));
    spans
}

/// The environment the request goes out under, worn as a dropdown, after the
/// key that opens it.
fn picker(app: &App) -> Vec<Span<'static>> {
    let source = app.tab().env_source();
    let label = source.map_or("no environment", |source| source.label.as_str());
    vec![
        Span::styled("⌃E ", theme::key()),
        Span::styled(
            format!(" {} ▾ ", ui::truncate(label, PICKER_LABEL)),
            theme::env_picker(source.is_some()),
        ),
        Span::styled(" ", theme::status_bar()),
    ]
}

/// Cuts a run of styled segments to fit, keeping the leading ones whole: the
/// mark matters more than the end of a long path.
fn truncate_spans(spans: Vec<Span<'static>>, room: usize) -> Vec<Span<'static>> {
    let mut out = Vec::with_capacity(spans.len());
    let mut used = 0;

    for span in spans {
        let width = UnicodeWidthStr::width(span.content.as_ref());
        if used + width <= room {
            used += width;
            out.push(span);
            continue;
        }
        let remaining = room.saturating_sub(used);
        if remaining > 0 {
            out.push(Span::styled(
                ui::truncate(span.content.as_ref(), remaining),
                span.style,
            ));
        }
        break;
    }
    out
}
