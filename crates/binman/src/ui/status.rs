//! The status line.
//!
//! What just happened on the left, the keys on the right, in Claude Code's
//! register: quiet text, `·` between things, no chips and no arrows.

use binman_core::BodyKind;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::mouse::{Target, Targets};
use crate::app::tab::{Section, Tab};
use crate::app::{App, Pane, Tone};
use crate::theme;
use crate::ui;

/// Key, then what it does, so the key can carry the accent and the label a
/// foreground bright enough to read — and what a click on it does.
const HINTS: [(&str, &str, Option<Target>); 5] = [
    ("⇥", "panes", Some(Target::CycleFocus)),
    ("⌃R", "send", Some(Target::Send)),
    ("⌃K", "commands", Some(Target::Palette)),
    ("F1", "help", Some(Target::Help)),
    // Not clickable: a stray click in the corner would end the session.
    ("⌃Q", "quit", None),
];

pub fn draw(frame: &mut Frame, app: &App, area: Rect, targets: &mut Targets) {
    // The transient message, or the context it falls back to once the
    // message has aged out.
    // While something is being typed, what the keys do there matters more
    // than a passing remark about what happened before it.
    let typing = app.focus == Pane::Request && app.tab().is_typing();
    let (message, message_style) =
        if app.status.is_stale() || (typing && app.status.tone == Tone::Info) {
            (context(app), theme::muted())
        } else {
            (app.status.text.clone(), tone_style(app.status.tone))
        };

    let hints = hint_spans();
    let hints_width = ui::width_of(&hints);
    let room = (area.width as usize).saturating_sub(hints_width + 1);

    let message = ui::truncate(&message, room.saturating_sub(1));
    let mut spans = vec![Span::styled(format!(" {message}"), message_style)];

    let used = ui::width_of(&spans);
    if room > used {
        spans.push(Span::styled(" ".repeat(room - used), theme::status_bar()));
        spans.extend(hints);
        spans.push(Span::styled(" ", theme::status_bar()));

        let mut x = area.x.saturating_add(room as u16);
        for (binding, what, target) in HINTS {
            let width = UnicodeWidthStr::width(binding) + 1 + UnicodeWidthStr::width(what);
            let width = width as u16;
            if let Some(target) = target {
                targets.add(ui::spot(area, x, width), target);
            }
            x = x.saturating_add(width + 3);
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

fn hint_spans() -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(HINTS.len() * 3);
    for (index, (binding, what, _)) in HINTS.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", theme::dim()));
        }
        spans.push(Span::styled(*binding, theme::key()));
        spans.push(Span::styled(format!(" {what}"), theme::muted()));
    }
    spans
}

/// What the bar says once a message has aged out: what the keys do where the
/// focus is, rather than what just happened.
fn context(app: &App) -> String {
    let tab = app.tab();
    match app.focus {
        Pane::Collections => "Enter opens a request · ⌃F finds one anywhere".into(),
        Pane::Url => {
            "↑↓ changes the method · Enter sends · a pasted curl command is imported on Enter"
                .into()
        }
        Pane::Request => request_context(tab),
        Pane::Response => match tab.response.received() {
            Some(_) => format!(
                "line {} of {} · [ ] changes what is shown",
                (tab.response.scroll + 1).min(tab.response.total.max(1)),
                tab.response.total
            ),
            None => "⌃R sends the request".into(),
        },
    }
}

fn request_context(tab: &Tab) -> String {
    if tab.editing {
        return "Esc stops editing".into();
    }
    if tab.is_typing() {
        return match tab.section {
            Section::Vars => {
                "Enter keeps this request's own value · Esc leaves it as it was".into()
            }
            Section::Auth => "Enter keeps it · Esc leaves it as it was".into(),
            _ => "Enter keeps it · Tab goes to the value · Esc leaves it as it was".into(),
        };
    }
    let table = "Enter edits · a adds · d deletes · [ ] changes section";
    match tab.section {
        Section::Params | Section::Headers => table.into(),
        Section::Body if tab.body_kind.is_form() => format!("{table} · ←→ changes the kind"),
        Section::Body if tab.body_kind == BodyKind::None => {
            "←→ picks what kind of body to send".into()
        }
        Section::Body => "Enter edits the body · ←→ changes the kind".into(),
        Section::Auth => "←→ picks the auth · Enter edits a field · d clears it".into(),
        Section::Vars => {
            "Enter gives a variable a value of its own · d goes back to the resolved one".into()
        }
        Section::Scripts => "Enter edits the rules".into(),
        Section::Info => "[ ] changes section".into(),
    }
}

fn tone_style(tone: Tone) -> ratatui::style::Style {
    match tone {
        Tone::Info => theme::muted(),
        Tone::Success => theme::success(),
        Tone::Warning => theme::warning(),
        Tone::Error => theme::danger(),
    }
}
