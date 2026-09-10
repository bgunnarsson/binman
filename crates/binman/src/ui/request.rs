//! The tab strip, the URL, and the request's sections.

use std::ops::Range;

use binman_core::vars::{self, Layer, Vars};
use binman_core::{AuthKind, BodyKind, auth, collection};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::kv::{Column, KvTable};
use crate::app::tab::{Section, Tab};
use crate::app::{App, Pane, host_of};
use crate::theme;
use crate::ui;

pub fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    for (index, tab) in app.tabs.iter().enumerate() {
        let active = index == app.active;
        let marker = if tab.is_sending() { "◐ " } else { "" };
        let dirty = if tab.is_dirty() { " •" } else { "" };
        spans.push(Span::styled(
            format!(" {marker}{}{dirty} ", ui::truncate(&tab.title, 24)),
            theme::tab(active),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(" + ", theme::dim()));

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

/// The method and the URL. A `{{variable}}` in the URL is lit in one colour
/// when something defines it and another when nothing does, so a missing
/// environment shows before anything is sent rather than after.
pub fn draw_url(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Pane::Url;
    let tab = app.tab();
    let block = ui::body_pane("Request", destination(app), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    let method = format!("{} ", tab.method);
    let method_width = method.chars().count() as u16;
    let mut spans = vec![Span::styled(method, theme::method(&tab.method))];

    if tab.url.text().is_empty() && !focused {
        spans.push(Span::styled(
            "Type a URL, or paste a curl command",
            theme::dim(),
        ));
    } else {
        let scope = tab.scope(&app.extracted);
        let lit: Vec<(Range<usize>, Style)> = vars::placeholders(tab.url.text())
            .map(|(range, name)| (range, theme::variable(scope.lookup(name).is_some())))
            .collect();
        let line = ui::input_line(
            &tab.url,
            inner.width.saturating_sub(method_width),
            focused,
            |offset| {
                lit.iter()
                    .find(|(range, _)| range.contains(&offset))
                    .map_or(theme::text(), |(_, style)| *style)
            },
        );
        spans.extend(line.spans);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

/// The scheme and host the URL resolves to, at the end of the URL bar —
/// because two requests both called `list.http` look identical until
/// something spells out which server each one reaches.
fn destination(app: &App) -> Vec<Span<'static>> {
    let tab = app.tab();
    let url = tab.scope(&app.extracted).resolve(tab.url.text().trim());
    if url.is_empty() {
        return Vec::new();
    }
    let host = host_of(&url);
    let style = if vars::scan(&host).is_empty() {
        theme::counter()
    } else {
        theme::warning()
    };
    vec![Span::styled(host, style)]
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Request;
    let describe_source = app.tab().origin.as_ref().map(|_| app.describe(app.tab()));
    let App {
        tabs,
        active,
        extracted,
        root,
        ..
    } = app;
    let tab = &mut tabs[*active];

    let referenced = tab.referenced();
    let sections = Section::ALL
        .iter()
        .map(|section| {
            let label = match section {
                Section::Params => counted("Params", tab.params.rows.len()),
                Section::Headers => counted("Headers", tab.header_rows().len()),
                Section::Vars => counted("Vars", referenced.len()),
                Section::Scripts => counted("Scripts", tab.rules().len()),
                other => other.label().to_string(),
            };
            (label, *section == tab.section)
        })
        .collect();
    let counter = match tab.section {
        Section::Body => vec![Span::styled(tab.body_kind.label(), theme::counter())],
        Section::Auth => vec![Span::styled(tab.auth.kind.label(), theme::counter())],
        _ => Vec::new(),
    };

    let block = ui::tabbed_pane(sections, counter, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    if inner.height == 0 {
        return;
    }

    match tab.section {
        Section::Params => table(frame, &mut tab.params, inner, focused, "a parameter"),
        Section::Headers => table(frame, &mut tab.headers, inner, focused, "a header"),
        Section::Body => body(frame, tab, inner, focused),
        Section::Auth => auth_section(frame, tab, inner, focused),
        Section::Vars => vars_section(frame, tab, extracted, &referenced, inner, focused),
        Section::Scripts => scripts(frame, tab, inner, focused),
        Section::Info => info(frame, tab, extracted, root, describe_source, inner),
    }
}

fn counted(label: &str, count: usize) -> String {
    if count == 0 {
        label.to_string()
    } else {
        format!("{label} {count}")
    }
}

/// Values of secret-bearing names are shown only as far as their first five
/// characters, as v1 did: enough to tell two apart, not enough to lift over a
/// shoulder. Only the display changes.
fn masked(name: &str, value: &str) -> String {
    const SHOWN: usize = 5;
    if !auth::is_secret(name) || value.chars().count() <= SHOWN {
        return value.to_string();
    }
    let head: String = value.chars().take(SHOWN).collect();
    format!("{head}{}", "•".repeat(value.chars().count() - SHOWN))
}

/// A table of name–value rows, with the row that adds one at the end.
fn table(frame: &mut Frame, table: &mut KvTable, area: Rect, focused: bool, noun: &str) {
    let height = area.height as usize;
    let rows = table.rows.len() + 1;
    table.offset = ui::scroll_offset(table.offset, table.selected, height);

    let name_width = table
        .rows
        .iter()
        .map(|(name, _)| unicode_width::UnicodeWidthStr::width(name.as_str()))
        .max()
        .unwrap_or(0)
        .clamp(8, 28);
    let value_width = (area.width as usize).saturating_sub(name_width + 3);

    let mut lines = Vec::with_capacity(height);
    for index in table.offset..(table.offset + height).min(rows) {
        let selected = index == table.selected;
        let mut spans = vec![ui::bar(selected, focused)];

        match table.edit.as_ref().filter(|edit| edit.row == index) {
            Some(edit) => {
                let on_name = edit.column == Column::Name;
                let name = ui::input_line(&edit.name, name_width as u16, on_name, |_| theme::text());
                let used = ui::width_of(&name.spans);
                spans.extend(name.spans);
                spans.push(Span::raw(" ".repeat(name_width.saturating_sub(used) + 2)));
                spans.extend(
                    ui::input_line(&edit.value, value_width as u16, !on_name, |_| theme::text()).spans,
                );
            }
            None if index == table.rows.len() => {
                spans.push(Span::styled(format!("+ add {noun}"), theme::dim()));
            }
            None => {
                let (name, value) = &table.rows[index];
                spans.push(Span::styled(
                    ui::pad(&ui::truncate(name, name_width), name_width),
                    theme::header_name(),
                ));
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    ui::truncate(&masked(name, value), value_width),
                    theme::text(),
                ));
            }
        }

        let line = Line::from(spans);
        lines.push(if selected {
            ui::highlight(line, focused, area.width)
        } else {
            line
        });
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// One line saying what kind something is and how to change it, above what
/// it holds.
fn kind_line(label: &str, focused: bool, extra: Option<&str>) -> Line<'static> {
    let mut spans = vec![
        Span::styled(label.to_string(), theme::title(focused)),
        Span::styled("  ←→ changes it", theme::dim()),
    ];
    if let Some(extra) = extra {
        spans.push(Span::styled(format!(" · {extra}"), theme::dim()));
    }
    Line::from(spans)
}

fn below_first(area: Rect) -> Rect {
    Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    }
}

fn body(frame: &mut Frame, tab: &mut Tab, area: Rect, focused: bool) {
    let editing = tab.editing && focused;
    let extra = match tab.body_kind {
        BodyKind::None => None,
        kind if kind.is_form() => None,
        _ if editing => Some("Esc stops editing"),
        _ => Some("Enter edits"),
    };
    frame.render_widget(
        Paragraph::new(kind_line(tab.body_kind.label(), focused, extra)),
        Rect { height: 1, ..area },
    );
    let content = below_first(area);

    match tab.body_kind {
        BodyKind::None => frame.render_widget(
            Paragraph::new(Span::styled("Nothing is sent.", theme::dim())),
            content,
        ),
        kind if kind.is_form() => table(frame, &mut tab.form, content, focused, "a field"),
        _ => {
            tab.body.set_cursor_style(if editing {
                theme::cursor()
            } else {
                Style::default()
            });
            frame.render_widget(&tab.body, content);
        }
    }
}

fn auth_section(frame: &mut Frame, tab: &mut Tab, area: Rect, focused: bool) {
    frame.render_widget(
        Paragraph::new(kind_line(tab.auth.kind.label(), focused, None)),
        Rect { height: 1, ..area },
    );
    let content = below_first(area);

    let fields = tab.auth.kind.fields();
    if fields.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("Nothing is added to the request.", theme::dim())),
            content,
        );
        return;
    }

    const LABEL: usize = 14;
    let value_width = (content.width as usize).saturating_sub(LABEL + 3);
    let mut lines = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let selected = index == tab.auth.selected;
        let mut spans = vec![
            ui::bar(selected, focused),
            Span::styled(ui::pad(field, LABEL), theme::muted()),
            Span::raw("  "),
        ];
        match (&tab.auth.edit, selected) {
            (Some(input), true) => {
                spans.extend(ui::input_line(input, value_width as u16, true, |_| theme::text()).spans)
            }
            _ => {
                let value = tab.auth.value(index);
                if value.is_empty() {
                    spans.push(Span::styled("—", theme::dim()));
                } else {
                    spans.push(Span::styled(
                        ui::truncate(&masked(field, value), value_width),
                        theme::text(),
                    ));
                }
            }
        }
        let line = Line::from(spans);
        lines.push(if selected {
            ui::highlight(line, focused, content.width)
        } else {
            line
        });
    }
    if tab.auth.kind == AuthKind::ClientCredentials {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "The token is fetched before each send, and reused until it nearly expires.",
            theme::dim(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), content);
}

/// Every variable the request refers to: its value, and which layer gave it.
fn vars_section(
    frame: &mut Frame,
    tab: &mut Tab,
    extracted: &Vars,
    names: &[String],
    area: Rect,
    focused: bool,
) {
    if names.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("No {{variables}} in this request.", theme::muted())),
                Line::from(Span::styled(
                    "Write {{NAME}} in the URL, a header or the body.",
                    theme::dim(),
                )),
            ]),
            area,
        );
        return;
    }

    let rows: Vec<(String, Option<(String, Layer)>)> = {
        let scope = tab.scope(extracted);
        names
            .iter()
            .map(|name| {
                let found = scope.lookup(name).map(|(value, layer)| (value.to_string(), layer));
                (name.clone(), found)
            })
            .collect()
    };

    let height = area.height as usize;
    tab.vars.selected = tab.vars.selected.min(rows.len() - 1);
    tab.vars.offset = ui::scroll_offset(tab.vars.offset, tab.vars.selected, height);
    let name_width = names
        .iter()
        .map(|name| unicode_width::UnicodeWidthStr::width(name.as_str()))
        .max()
        .unwrap_or(0)
        .clamp(6, 24);
    let value_width = (area.width as usize).saturating_sub(name_width + 16);

    let mut lines = Vec::new();
    for (index, (name, found)) in rows.iter().enumerate().skip(tab.vars.offset).take(height) {
        let selected = index == tab.vars.selected;
        let mut spans = vec![
            ui::bar(selected, focused),
            Span::styled(ui::pad(&ui::truncate(name, name_width), name_width), theme::header_name()),
            Span::raw("  "),
        ];
        match (&tab.vars.edit, found) {
            (Some((editing, input)), _) if editing == name => {
                spans.extend(ui::input_line(input, value_width as u16, true, |_| theme::text()).spans);
            }
            (_, Some((value, layer))) => {
                spans.push(Span::styled(
                    ui::truncate(&masked(name, value), value_width),
                    theme::text(),
                ));
                spans.push(Span::styled(format!("  {}", layer.label()), theme::dim()));
            }
            (_, None) => spans.push(Span::styled("not set", theme::warning())),
        }
        let line = Line::from(spans);
        lines.push(if selected {
            ui::highlight(line, focused, area.width)
        } else {
            line
        });
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn scripts(frame: &mut Frame, tab: &mut Tab, area: Rect, focused: bool) {
    let editing = tab.editing && focused;
    let empty = tab.scripts.lines().iter().all(|line| line.trim().is_empty());
    if empty && !editing {
        let example = |rule: &str| Line::from(Span::styled(format!("  {rule}"), theme::muted()));
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Rules pull values out of the response into variables, for the requests after it:",
                    theme::dim(),
                )),
                example("token = json .access_token"),
                example("id    = json data.items.0.id"),
                example("csrf  = header X-CSRF-Token"),
                example("code  = regex /code=(\\d+)/"),
                Line::from(Span::styled("Enter writes one.", theme::dim())),
            ]),
            area,
        );
        return;
    }
    tab.scripts.set_cursor_style(if editing {
        theme::cursor()
    } else {
        Style::default()
    });
    frame.render_widget(&tab.scripts, area);
}

/// Where the request came from and where it will go, spelled out.
fn info(
    frame: &mut Frame,
    tab: &Tab,
    extracted: &Vars,
    root: &std::path::Path,
    source: Option<String>,
    area: Rect,
) {
    let scope = tab.scope(extracted);
    let resolved = scope.resolve(tab.url.text().trim());
    let referenced = tab.referenced();
    let unset = referenced
        .iter()
        .filter(|name| scope.lookup(name).is_none())
        .count();

    let environment = match tab.env_source() {
        Some(source) => format!(
            "{} · {} · {}",
            source.label,
            source.kind.label(),
            collection::relative(root, &source.path)
        ),
        None => "none".to_string(),
    };
    let rows = [
        ("Source", source.unwrap_or_else(|| "not saved anywhere".into())),
        ("Environment", environment),
        (
            "Sends",
            if resolved.is_empty() {
                "nothing yet".to_string()
            } else {
                format!("{} {resolved}", tab.method)
            },
        ),
        (
            "Variables",
            format!("{} referred to, {unset} not set", referenced.len()),
        ),
    ];

    let lines: Vec<Line> = rows
        .into_iter()
        .flat_map(|(label, value)| {
            let width = (area.width as usize).saturating_sub(14).max(8);
            ui::wrap_line(&Line::from(Span::styled(value, theme::text())), width)
                .into_iter()
                .enumerate()
                .map(move |(index, piece)| {
                    let label = if index == 0 { label } else { "" };
                    let mut spans = vec![Span::styled(ui::pad(label, 14), theme::muted())];
                    spans.extend(piece.spans);
                    Line::from(spans)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}
