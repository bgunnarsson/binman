//! The header line.
//!
//! One question, as in binsql: where does ⌃R send this. The left says which
//! request it is and the environment it goes out under; the right, the host
//! its URL resolves to — because two requests both called `list.http` look
//! identical until something spells out which server each one reaches.
//!
//! Styled after Claude Code rather than binvim: the mark carries the only
//! colour, everything after it is quiet text separated by `·`, and there are
//! no chips or arrows.

use binman_core::vars;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, host_of};
use crate::theme;
use crate::ui;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let left = identity(app);
    let left_width = ui::width_of(&left);

    // The host gives way before the identity does: which request this is
    // matters more than where it goes, once the two cannot both fit.
    let mut right = destination(app);
    if left_width + ui::width_of(&right) > area.width as usize {
        right.clear();
    }

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

/// Which request this is, and the environment it goes out under.
fn identity(app: &App) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(format!(" {}  ", theme::MARK), theme::brand())];
    let tab = app.tab();

    if tab.origin.is_none() && tab.url.text().trim().is_empty() {
        // Nothing to name yet, so the header says which program this is.
        spans.push(Span::styled("binman", theme::muted()));
        spans.push(separator());
        spans.push(Span::styled("new request", theme::dim()));
        return spans;
    }

    spans.push(Span::styled(app.describe(tab), theme::title(true)));
    if let Some(source) = tab.env_source() {
        spans.push(separator());
        spans.push(Span::styled(source.label.clone(), theme::muted()));
    }
    spans
}

/// The scheme and host the URL resolves to, or what is missing for it to.
fn destination(app: &App) -> Vec<Span<'static>> {
    let tab = app.tab();
    let url = tab.scope(&app.extracted).resolve(tab.url.text().trim());
    if url.is_empty() {
        return Vec::new();
    }
    let host = host_of(&url);
    let style: Style = if vars::scan(&host).is_empty() {
        theme::muted()
    } else {
        theme::warning()
    };
    vec![Span::styled(host, style), Span::raw(" ")]
}

fn separator() -> Span<'static> {
    Span::styled("  ·  ", theme::dim())
}

/// Cuts a run of styled segments to fit, keeping the leading ones whole: the
/// mark and the request matter more than the environment after them.
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
