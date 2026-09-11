use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::app::mouse::{Target, Targets};
use crate::app::overlay::{
    CollectionForm, EnvEditor, FormField, Overlay, Picker, PickerKind, SavePrompt,
};
use crate::theme;
use crate::ui;
use crate::ui::explorer::badge;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect, targets: &mut Targets) {
    let collections: Vec<String> = app
        .workspace
        .collections()
        .iter()
        .map(|collection| collection.name.clone())
        .collect();
    match &mut app.overlay {
        None => {}
        Some(Overlay::Splash) => splash(frame, &collections, area),
        Some(Overlay::Help) => help(frame, area),
        Some(Overlay::Picker(picker)) => picker_box(frame, picker, area, targets),
        Some(Overlay::Env(editor)) => env_editor(frame, editor, area),
        Some(Overlay::SaveRequest(prompt)) => {
            save_prompt(frame, prompt, area, "Save the request to");
        }
        Some(Overlay::SaveResponse(prompt)) => {
            save_prompt(frame, prompt, area, "Save the response to");
        }
        Some(Overlay::Curl(command)) => curl(frame, command, area),
        Some(Overlay::Collection(form)) => collection_form(frame, form, area),
    }
}

/// Breathing room either side of an overlay's contents.
const PADDING: usize = 2;
/// The two edges of a bordered box, in either direction.
const BORDERS: u16 = 2;

fn frame_for(frame: &mut Frame, area: Rect, title: &str) -> Rect {
    frame.render_widget(Clear, area);
    let block = ui::pane(title, true).style(theme::overlay());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn frame_for_counted(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    counter: impl Into<String>,
) -> Rect {
    frame.render_widget(Clear, area);
    let block = ui::counted_pane(title, counter, true).style(theme::overlay());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn padded(inner: Rect) -> Rect {
    Rect {
        x: inner.x + PADDING as u16,
        width: inner.width.saturating_sub(PADDING as u16 * 2),
        ..inner
    }
}

fn footer_row(inner: Rect) -> Rect {
    Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: 1.min(inner.height),
        ..padded(inner)
    }
}

/// A very long value would overflow a `u16`; it is going to be clamped to the
/// screen anyway.
fn saturating_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// The widest line in a block, in display columns.
fn block_width(lines: &[Line<'_>]) -> usize {
    lines
        .iter()
        .map(|line| ui::width_of(&line.spans))
        .max()
        .unwrap_or(0)
}

fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (key, what)) in pairs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", theme::dim()));
        }
        spans.push(Span::styled(key.to_string(), theme::key()));
        spans.push(Span::styled(format!(" {what}"), theme::dim()));
    }
    Line::from(spans)
}

// ── Splash ──────────────────────────────────────────────────────────

/// The wordmark's letters in the ANSI-shadow shape a terminal splash is
/// expected to wear. Every glyph is single-width, so the block is exactly as
/// wide as it looks.
const B: [&str; 6] = [
    "██████╗ ",
    "██╔══██╗",
    "██████╔╝",
    "██╔══██╗",
    "██████╔╝",
    "╚═════╝ ",
];
const I: [&str; 6] = ["██╗", "██║", "██║", "██║", "██║", "╚═╝"];
const N: [&str; 6] = [
    "███╗   ██╗",
    "████╗  ██║",
    "██╔██╗ ██║",
    "██║╚██╗██║",
    "██║ ╚████║",
    "╚═╝  ╚═══╝",
];
const M: [&str; 6] = [
    "███╗   ███╗",
    "████╗ ████║",
    "██╔████╔██║",
    "██║╚██╔╝██║",
    "██║ ╚═╝ ██║",
    "╚═╝     ╚═╝",
];
const A: [&str; 6] = [
    " █████╗ ",
    "██╔══██╗",
    "███████║",
    "██╔══██║",
    "██║  ██║",
    "╚═╝  ╚═╝",
];

fn wordmark() -> Vec<String> {
    (0..6)
        .map(|row| {
            [B, I, N, M, A, N]
                .iter()
                .map(|letter| letter[row])
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// The greeting, shown once at startup. Falls back to plain text in a terminal
/// too narrow for the wordmark — a splash that overflows its own box is worse
/// than no splash.
fn splash(frame: &mut Frame, collections: &[String], area: Rect) {
    let wordmark = wordmark();
    let wordmark_width = wordmark
        .iter()
        .map(|row| UnicodeWidthStr::width(row.as_str()))
        .max()
        .unwrap_or(0);
    let roomy = area.width as usize >= wordmark_width + 8;

    let mut lines: Vec<Line<'static>> = Vec::new();
    if roomy {
        lines.extend(
            wordmark
                .into_iter()
                .map(|row| Line::from(Span::styled(row, theme::brand()))),
        );
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", theme::MARK), theme::brand()),
            Span::styled("binman", theme::title(true)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "an HTTP client for the terminal",
        theme::muted(),
    )));
    lines.push(Line::from(""));
    if collections.is_empty() {
        lines.push(Line::from(Span::styled(
            "No collections yet — a in the sidebar adds one",
            theme::dim(),
        )));
    } else {
        // Held to the wordmark's width, so a long list does not widen the box.
        let names = ui::truncate(&collections.join(", "), wordmark_width.saturating_sub(13));
        lines.push(Line::from(vec![
            Span::styled("Collections  ", theme::dim()),
            Span::styled(names, theme::muted()),
        ]));
    }
    lines.push(Line::from(""));
    for (binding, what) in [("⌃F", "find a request"), ("⌃K", "commands"), ("F1", "help")] {
        lines.push(Line::from(vec![
            Span::styled(format!("{binding:<4}"), theme::key()),
            Span::styled(what, theme::muted()),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Any key to begin.", theme::dim())));

    let width = saturating_u16(block_width(&lines) + BORDERS as usize + PADDING * 2);
    let height = saturating_u16(lines.len()) + BORDERS;
    let inner = frame_for_counted(
        frame,
        ui::centered_size(area, width, height),
        "binman",
        format!("v{}", env!("CARGO_PKG_VERSION")),
    );
    frame.render_widget(Paragraph::new(lines), padded(inner));
}

// ── Help ────────────────────────────────────────────────────────────

type Section = (&'static str, &'static [(&'static str, &'static str)]);

/// How many sections the left-hand help column takes.
const LEFT_COLUMN: usize = 2;

const SECTIONS: &[Section] = &[
    (
        "Anywhere",
        &[
            ("⌃R / ⌃J", "Send the request"),
            ("⌃C", "Cancel the request in flight"),
            ("⌃K", "Command palette"),
            ("⌃F", "Find a request"),
            ("⌃H", "History"),
            ("⌃E", "Environments"),
            ("⌃S", "Save the request, or the response"),
            ("⌃Y", "Copy as cURL"),
            ("⌃T / ⌃W", "New tab / close tab"),
            ("⌥1…9", "Jump to a tab"),
            ("Tab / ⇧Tab", "Cycle panes"),
            ("⌥h ⌥k ⌥l ⌥j", "Collections / URL / request / response"),
            ("F1 or ?", "This help"),
            ("F5", "Reload the selected folder"),
            ("drag an edge", "Resize the panes"),
            ("⌃Q", "Quit"),
        ],
    ),
    (
        "URL",
        &[
            ("↑ / ↓", "Change the method"),
            ("Enter", "Send, or import a pasted curl"),
            ("Esc", "Back to the collections"),
        ],
    ),
    (
        "Collections",
        &[
            ("j / k", "Move"),
            ("l / Space", "Expand"),
            ("h", "Collapse, or go to parent"),
            ("Enter", "Open the request"),
            ("g / G", "First / last"),
            ("r", "Reload from disk"),
            ("a", "Add a collection"),
            ("e / d", "Edit / remove a collection"),
        ],
    ),
    (
        "Request",
        &[
            ("[ / ]", "Previous / next section"),
            ("j / k", "Move"),
            ("Enter", "Edit"),
            ("a / d", "Add / delete"),
            ("← / →", "Body or auth kind"),
            ("Esc", "Stop editing"),
        ],
    ),
    (
        "Response",
        &[
            ("[ / ]", "Previous / next view"),
            ("j / k", "Scroll"),
            ("⌃D / ⌃U", "Half page"),
            ("g / G", "Top / bottom"),
        ],
    ),
];

/// Renders one help section per heading, blank-separated, with no trailing gap.
fn section_lines(sections: &[Section]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (heading, bindings) in sections {
        lines.push(Line::from(Span::styled(
            (*heading).to_string(),
            theme::title(true),
        )));
        for (keys, description) in *bindings {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{keys:<13}"), theme::key()),
                Span::styled(*description, theme::muted()),
            ]));
        }
        lines.push(Line::from(""));
    }
    lines.pop();
    lines
}

fn help(frame: &mut Frame, area: Rect) {
    // Two columns: the bindings do not fit down one, and a help screen that
    // scrolls is a help screen nobody reads to the end of.
    let left = section_lines(&SECTIONS[..LEFT_COLUMN]);
    let right = section_lines(&SECTIONS[LEFT_COLUMN..]);

    const GUTTER: u16 = 4;
    let left_width = saturating_u16(block_width(&left));
    let right_width = saturating_u16(block_width(&right));
    let width = left_width + GUTTER + right_width + BORDERS + PADDING as u16 * 2;
    let height = saturating_u16(left.len().max(right.len()) + 2) + BORDERS;

    let inner = frame_for(frame, ui::centered_size(area, width, height), "Help");
    if inner.height < 3 {
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(GUTTER),
            Constraint::Min(0),
        ])
        .split(Rect {
            height: inner.height - 2,
            ..padded(inner)
        });
    frame.render_widget(Paragraph::new(left), columns[0]);
    frame.render_widget(Paragraph::new(right), columns[2]);
    frame.render_widget(
        Paragraph::new(Span::styled("Any key closes this.", theme::dim())),
        footer_row(inner),
    );
}

// ── Pickers ─────────────────────────────────────────────────────────

/// Wide enough that the box does not jump about as the query narrows the list.
const MIN_PICKER_WIDTH: u16 = 56;

/// A list to choose from: the command palette, the request search, the
/// history, the environments. Pinned near the top rather than centred, so a
/// list that shrinks as the query narrows it grows and shrinks downwards
/// instead of sliding the prompt under the cursor. The environments drop from
/// the picker in the header instead, as v1's dropdown did.
fn picker_box(frame: &mut Frame, picker: &Picker, area: Rect, targets: &mut Targets) {
    const FROM_TOP: u16 = 3;
    /// Enough to choose from without the box owning the screen.
    const MAX_ROWS: usize = 14;

    let matches = picker.matches();
    let rows = matches.len().clamp(1, MAX_ROWS);

    let mut lines = Vec::new();
    if picker.kind.filters() {
        lines.push(Line::from(vec![
            Span::styled("› ", theme::accent()),
            Span::styled(picker.query.clone(), theme::text()),
            Span::styled("▏", theme::accent()),
        ]));
        lines.push(Line::from(""));
    }

    let ceiling = (area.width as usize * 90 / 100).saturating_sub(BORDERS as usize + PADDING * 2);
    let label_width = matches
        .iter()
        .map(|&index| UnicodeWidthStr::width(picker.entries[index].label.as_str()))
        .max()
        .unwrap_or(0)
        .min(ceiling.saturating_sub(12).max(12));
    let offset = ui::scroll_offset(0, picker.selected, rows);

    let mut entries = Vec::with_capacity(rows);
    let mut positions = Vec::with_capacity(rows);
    let mut selected_row = None;
    for (position, &index) in matches.iter().enumerate().skip(offset).take(rows) {
        positions.push(position);
        let entry = &picker.entries[index];
        let selected = position == picker.selected;
        if selected {
            selected_row = Some(entries.len());
        }
        let mut spans = vec![ui::bar(selected, true), Span::raw(" ")];
        if picker.kind != PickerKind::Commands && picker.kind != PickerKind::Environments {
            spans.push(badge(entry.method.as_deref()));
        }
        spans.push(Span::styled(
            ui::pad(&ui::truncate(&entry.label, label_width), label_width),
            theme::text(),
        ));
        if !entry.detail.is_empty() {
            spans.push(Span::styled(format!("  {}", entry.detail), theme::dim()));
        }
        if !entry.hint.is_empty() {
            spans.push(Span::styled(format!("  {}", entry.hint), theme::dim()));
        }
        entries.push(Line::from(spans));
    }

    // A list that filters keeps its width as the query narrows it; the
    // environments do not filter, so their dropdown hugs what it holds.
    let row_width = if picker.kind.filters() {
        ceiling
    } else {
        block_width(&entries).min(ceiling)
    };
    if let Some(row) = selected_row {
        let line = std::mem::take(&mut entries[row]);
        entries[row] = ui::highlight(line, true, saturating_u16(row_width));
    }
    lines.extend(entries);

    if matches.is_empty() {
        lines.push(Line::from(Span::styled("  No match", theme::dim())));
    }
    if picker.kind == PickerKind::Environments {
        lines.push(Line::from(""));
        lines.push(hints(&[
            ("↵", "use"),
            ("e", "edit the file"),
            ("Esc", "close"),
        ]));
    }

    let mut width = saturating_u16(
        block_width(&lines)
            .min(ceiling)
            .saturating_add(BORDERS as usize + PADDING * 2),
    );
    if picker.kind.filters() {
        width = width.max(MIN_PICKER_WIDTH);
    }
    let height = saturating_u16(lines.len()) + BORDERS;
    let position = if matches.is_empty() {
        0
    } else {
        picker.selected + 1
    };
    let placed = if picker.kind == PickerKind::Environments {
        ui::dropdown_size(area, width, height)
    } else {
        ui::anchored_size(area, width, height, FROM_TOP)
    };
    let inner = frame_for_counted(
        frame,
        placed,
        picker.kind.title(),
        format!("{position}/{}", matches.len()),
    );
    let list = padded(inner);
    targets.add(placed, Target::Overlay);
    let first = if picker.kind.filters() { 2 } else { 0 };
    for (row, position) in positions.into_iter().enumerate() {
        targets.add(ui::line_at(list, first + row), Target::Entry(position));
    }
    frame.render_widget(Paragraph::new(lines), list);
}

// ── Environment editor ──────────────────────────────────────────────

const MIN_EDITOR_WIDTH: u16 = 56;

fn env_editor(frame: &mut Frame, editor: &mut EnvEditor, area: Rect) {
    let widest = editor
        .editor
        .lines()
        .iter()
        .map(|line| UnicodeWidthStr::width(line.as_str()))
        .max()
        .unwrap_or(0);
    let ceiling_width = (area.width * 88 / 100).max(1);
    let width = saturating_u16(widest + 2 + BORDERS as usize + PADDING * 2)
        .clamp(MIN_EDITOR_WIDTH.min(ceiling_width), ceiling_width);
    let ceiling_height = (area.height * 80 / 100).max(BORDERS + 3);
    let height = saturating_u16(editor.editor.lines().len() + 2)
        .saturating_add(BORDERS)
        .clamp((BORDERS + 3).min(ceiling_height), ceiling_height);

    let inner = frame_for_counted(
        frame,
        ui::centered_size(area, width, height),
        &format!("Edit {}", editor.label),
        "⌃S saves",
    );
    if inner.height < 3 {
        return;
    }
    frame.render_widget(
        &editor.editor,
        Rect {
            height: inner.height - 2,
            ..padded(inner)
        },
    );
    frame.render_widget(
        Paragraph::new(hints(&[("⌃S", "save"), ("Esc", "cancel")])),
        footer_row(inner),
    );
}

// ── Save prompt ─────────────────────────────────────────────────────

const MIN_PROMPT_WIDTH: u16 = 64;

fn save_prompt(frame: &mut Frame, prompt: &SavePrompt, area: Rect, title: &str) {
    let width = (area.width * 70 / 100).clamp(MIN_PROMPT_WIDTH.min(area.width), 110);
    let field_width = width.saturating_sub(BORDERS + PADDING as u16 * 2);

    let mut lines = vec![
        ui::input_line(&prompt.input, field_width, true, |_| theme::text()),
        Line::from(""),
    ];
    if let Some(error) = &prompt.error {
        lines.extend(ui::wrap_line(
            &Line::from(Span::styled(error.clone(), theme::danger())),
            field_width as usize,
        ));
        lines.push(Line::from(""));
    }
    lines.push(hints(&[("↵", "save"), ("Esc", "cancel")]));

    let height = saturating_u16(lines.len()) + BORDERS;
    let inner = frame_for(frame, ui::centered_size(area, width, height), title);
    frame.render_widget(Paragraph::new(lines), padded(inner));
}

// ── Collection form ─────────────────────────────────────────────────

/// Enough for the labels and a path worth reading.
const MIN_FORM_WIDTH: u16 = 64;
const LABEL_WIDTH: usize = 10;

/// binsql's connection form: a label column, the field in focus marked, and
/// the value scrolling inside its field rather than stretching the box.
fn collection_form(frame: &mut Frame, form: &CollectionForm, area: Rect) {
    let title = if form.editing.is_some() {
        "Edit the collection"
    } else {
        "Add a collection"
    };
    let width = (area.width * 70 / 100).clamp(MIN_FORM_WIDTH.min(area.width), 110);
    let inner_width = width.saturating_sub(BORDERS + PADDING as u16 * 2);
    let field_width = inner_width.saturating_sub(LABEL_WIDTH as u16 + 2);

    let mut lines = vec![
        Line::from(Span::styled(
            "A directory of requests, a Postman collection or an OpenAPI spec.",
            theme::dim(),
        )),
        Line::from(""),
    ];
    for field in form.fields() {
        let active = field == form.field;
        let mut spans = vec![
            Span::styled(if active { "▸ " } else { "  " }, theme::accent()),
            Span::styled(format!("{:<LABEL_WIDTH$}", field.label()), theme::muted()),
        ];
        match field {
            FormField::Path => {
                spans.extend(
                    ui::input_line(&form.path, field_width, active, |_| theme::text()).spans,
                );
            }
            // Left empty, it shows what it will be called.
            FormField::Name if form.name.text().is_empty() => {
                if active {
                    spans.push(Span::styled(" ", theme::cursor()));
                }
                spans.push(Span::styled(form.placeholder(), theme::dim()));
            }
            FormField::Name => {
                spans.extend(
                    ui::input_line(&form.name, field_width, active, |_| theme::text()).spans,
                );
            }
            FormField::Scope => {
                let arrows = if active {
                    theme::accent()
                } else {
                    theme::dim()
                };
                // Cut from the front, so the file's name and whether saving
                // creates it stay on screen.
                let file = ui::truncate_start(
                    &form.scope_display(),
                    usize::from(field_width.saturating_sub(4)),
                );
                spans.push(Span::styled("‹ ", arrows));
                spans.push(Span::styled(file, theme::text()));
                spans.push(Span::styled(" ›", arrows));
            }
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));
    if let Some(error) = &form.error {
        lines.extend(ui::wrap_line(
            &Line::from(Span::styled(error.clone(), theme::danger())),
            inner_width as usize,
        ));
        lines.push(Line::from(""));
    }
    let mut keys = vec![("Tab", "next")];
    if form.fields().contains(&FormField::Scope) {
        keys.push(("← →", "saved in"));
    }
    keys.extend([("↵", "save"), ("Esc", "cancel")]);
    lines.push(hints(&keys));

    let height = saturating_u16(lines.len()) + BORDERS;
    let inner = frame_for(frame, ui::centered_size(area, width, height), title);
    frame.render_widget(Paragraph::new(lines), padded(inner));
}

// ── cURL ────────────────────────────────────────────────────────────

fn curl(frame: &mut Frame, command: &str, area: Rect) {
    let ceiling = (area.width * 88 / 100).max(1);
    let wanted = saturating_u16(UnicodeWidthStr::width(command) + BORDERS as usize + PADDING * 2);
    let width = wanted.clamp(48.min(ceiling), ceiling);
    let body_width = width.saturating_sub(BORDERS + PADDING as u16 * 2) as usize;

    let mut lines = ui::wrap_line(
        &Line::from(Span::styled(command.to_string(), theme::text())),
        body_width,
    );
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Any key closes this.",
        theme::dim(),
    )));

    let height = saturating_u16(lines.len())
        .saturating_add(BORDERS)
        .min(area.height);
    let inner = frame_for_counted(
        frame,
        ui::centered_size(area, width, height),
        "cURL",
        "sent to the clipboard",
    );
    frame.render_widget(Paragraph::new(lines), padded(inner));
}
