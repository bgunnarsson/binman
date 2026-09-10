//! The response: its body, its headers, its cookies, what the rules pulled out
//! of it, and how long each part of getting it took.

use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::mouse::{Target, Targets};
use crate::app::tab::{Outcome, Received, Response, View};
use crate::app::{App, Pane, format_elapsed, size};
use crate::{pretty, theme, ui};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect, targets: &mut Targets) {
    let focused = app.focus == Pane::Response;
    let response = &mut app.tab_mut().response;

    let received = response.received();
    let sections: Vec<(String, bool)> = View::ALL
        .iter()
        .map(|view| {
            let count = received.map_or(0, |received| match view {
                View::Headers => received.exchange.headers.len(),
                View::Cookies => received.exchange.set_cookies.len(),
                View::Scripts => received.extracted.len(),
                View::Body | View::Trace => 0,
            });
            let label = if count == 0 {
                view.label().to_string()
            } else {
                format!("{} {count}", view.label())
            };
            (label, *view == response.view)
        })
        .collect();
    let spots = ui::section_spots(area, sections.iter().map(|(label, _)| label.as_str()));

    let counter = match &response.outcome {
        Outcome::Received(received) => {
            let exchange = &received.exchange;
            let mut status = exchange.status.to_string();
            if !exchange.reason.is_empty() {
                status.push(' ');
                status.push_str(&exchange.reason);
            }
            vec![
                Span::styled(status, theme::status(exchange.status)),
                Span::styled(
                    format!(
                        " · {} · {}",
                        format_elapsed(exchange.trace.total),
                        size(exchange.body.len())
                    ),
                    theme::counter(),
                ),
            ]
        }
        Outcome::Sending { started, .. } => vec![Span::styled(
            format!("sending · {}", format_elapsed(started.elapsed())),
            theme::warning(),
        )],
        _ => Vec::new(),
    };

    let block = ui::tabbed_pane(sections, counter, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    targets.add(area, Target::Pane(Pane::Response));
    for (spot, view) in spots.into_iter().zip(View::ALL) {
        targets.add(spot, Target::View(view));
    }
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let message = match &response.outcome {
        Outcome::Idle => Some(vec![Line::from(vec![
            Span::styled("⌃R", theme::key()),
            Span::styled(" sends the request · ", theme::dim()),
            Span::styled("⌃K", theme::key()),
            Span::styled(" for commands", theme::dim()),
        ])]),
        Outcome::Sending { started, streamed } if streamed.is_empty() => {
            Some(vec![Line::from(vec![
                Span::styled(
                    format!("Sending… {} ", format_elapsed(started.elapsed())),
                    theme::warning(),
                ),
                Span::styled("⌃C", theme::key()),
                Span::styled(" cancels", theme::dim()),
            ])])
        }
        Outcome::Cancelled => Some(vec![Line::from(Span::styled("Cancelled", theme::muted()))]),
        Outcome::Failed(error) => Some(ui::wrap_line(
            &Line::from(Span::styled(error.clone(), theme::danger())),
            inner.width as usize,
        )),
        Outcome::Sending { .. } | Outcome::Received(_) => None,
    };
    match message {
        Some(lines) => {
            response.total = 0;
            frame.render_widget(Paragraph::new(lines), inner);
        }
        None => scrolled(frame, response, inner),
    }
}

/// Draws whatever the response pane shows as a window onto its wrapped lines,
/// following the end of a stream while it arrives.
///
/// Only the lines on screen are wrapped; the rest are only measured. A body of
/// a hundred thousand lines costs a pass over their widths, not a hundred
/// thousand wrapped copies every frame.
fn scrolled(frame: &mut Frame, response: &mut Response, area: Rect) {
    let width = area.width as usize;
    let height = area.height as usize;

    let total: usize = lines(response, width)
        .iter()
        .map(|line| ui::rows_for(line.width(), width))
        .sum();
    response.total = total;
    response.max_scroll = total.saturating_sub(height);
    if response.follow {
        response.scroll = response.max_scroll;
    }
    response.scroll = response.scroll.min(response.max_scroll);

    let source = lines(response, width);
    let mut visible = Vec::with_capacity(height);
    let mut row = 0;
    for line in source.iter() {
        let rows = ui::rows_for(line.width(), width);
        if row + rows <= response.scroll {
            row += rows;
            continue;
        }
        for (index, piece) in ui::wrap_line(line, width).into_iter().enumerate() {
            if row + index >= response.scroll && visible.len() < height {
                visible.push(piece);
            }
        }
        row += rows;
        if visible.len() >= height {
            break;
        }
    }
    frame.render_widget(Paragraph::new(visible), area);
}

/// The lines of what the pane is showing: the body as it was rendered when it
/// arrived, borrowed, or a view built fresh from the exchange.
fn lines(response: &Response, width: usize) -> Cow<'_, [Line<'static>]> {
    match &response.outcome {
        Outcome::Sending { streamed, .. } => Cow::Owned(pretty::plain(streamed)),
        Outcome::Received(received) => match response.view {
            View::Body => Cow::Borrowed(received.lines.as_slice()),
            View::Headers => Cow::Owned(headers(received)),
            View::Cookies => Cow::Owned(cookies(received)),
            View::Scripts => Cow::Owned(scripts(received)),
            View::Trace => Cow::Owned(trace(received, width)),
        },
        _ => Cow::Owned(Vec::new()),
    }
}

fn heading(text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), theme::title(true)))
}

fn headers(received: &Received) -> Vec<Line<'static>> {
    let exchange = &received.exchange;
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{}  ", exchange.version), theme::muted()),
        Span::styled(
            format!("{} {}", exchange.status, exchange.reason)
                .trim_end()
                .to_string(),
            theme::status(exchange.status),
        ),
    ])];
    if exchange.url != received.sent.url {
        lines.push(Line::from(vec![
            Span::styled("redirected to  ", theme::muted()),
            Span::styled(exchange.url.clone(), theme::text()),
        ]));
    }
    lines.push(Line::from(""));

    let name_width = exchange
        .headers
        .iter()
        .map(|(name, _)| UnicodeWidthStr::width(name.as_str()))
        .max()
        .unwrap_or(0)
        .clamp(4, 32);
    lines.extend(exchange.headers.iter().map(|(name, value)| {
        Line::from(vec![
            Span::styled(ui::pad(name, name_width), theme::header_name()),
            Span::raw("  "),
            Span::styled(value.clone(), theme::text()),
        ])
    }));
    lines
}

fn cookies(received: &Received) -> Vec<Line<'static>> {
    let exchange = &received.exchange;
    if exchange.set_cookies.is_empty() && exchange.jar.is_empty() {
        return vec![Line::from(Span::styled("No cookies.", theme::dim()))];
    }

    let mut lines = Vec::new();
    if !exchange.set_cookies.is_empty() {
        lines.push(heading("Set by this response"));
        for cookie in &exchange.set_cookies {
            let mut spans = vec![
                Span::styled(format!("  {}", cookie.name), theme::header_name()),
                Span::styled(" = ", theme::dim()),
                Span::styled(cookie.value.clone(), theme::text()),
            ];
            if !cookie.attributes.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", cookie.attributes.join("; ")),
                    theme::dim(),
                ));
            }
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(""));
    }
    if !exchange.jar.is_empty() {
        lines.push(heading(
            "In the jar — sent with the next request to this host",
        ));
        for (name, value) in &exchange.jar {
            lines.push(Line::from(vec![
                Span::styled(format!("  {name}"), theme::header_name()),
                Span::styled(" = ", theme::dim()),
                Span::styled(value.clone(), theme::text()),
            ]));
        }
    }
    lines
}

fn scripts(received: &Received) -> Vec<Line<'static>> {
    if received.extracted.is_empty() {
        let example = |rule: &str| Line::from(Span::styled(format!("  {rule}"), theme::muted()));
        return vec![
            Line::from(Span::styled("Nothing was extracted.", theme::muted())),
            Line::from(Span::styled(
                "Rules in the request's Scripts section pull values out of a response:",
                theme::dim(),
            )),
            example("token = json .access_token"),
            example("id    = json data.items.0.id"),
            example("csrf  = header X-CSRF-Token"),
        ];
    }

    let mut lines = vec![heading(
        "Extracted — every tab's requests can use these now",
    )];
    lines.extend(received.extracted.iter().map(|(name, value)| {
        Line::from(vec![
            Span::styled(format!("  {name}"), theme::header_name()),
            Span::styled(" = ", theme::dim()),
            Span::styled(value.clone(), theme::text()),
        ])
    }));
    lines
}

/// Each phase, longest bar last. Connect is the TCP connection and the TLS
/// handshake together: the HTTP library makes them one step.
fn trace(received: &Received, width: usize) -> Vec<Line<'static>> {
    let trace = &received.exchange.trace;
    let row = |label: &str, value: Option<std::time::Duration>, note: &str| {
        let mut spans = vec![Span::styled(ui::pad(label, 14), theme::muted())];
        match value {
            Some(duration) => spans.push(Span::styled(format_elapsed(duration), theme::text())),
            None => spans.push(Span::styled("—", theme::dim())),
        }
        if !note.is_empty() && width > 40 {
            spans.push(Span::styled(format!("  {note}"), theme::dim()));
        }
        Line::from(spans)
    };

    let download = trace.total.saturating_sub(trace.first_byte);
    vec![
        row(
            "DNS",
            trace.dns,
            if trace.dns.is_none() {
                "nothing to resolve"
            } else {
                ""
            },
        ),
        row("Connect", trace.connect, "TCP, and TLS for https"),
        row(
            "First byte",
            Some(trace.first_byte),
            "from sending to the headers",
        ),
        row("Download", Some(download), "the body"),
        row("Total", Some(trace.total), ""),
    ]
}
