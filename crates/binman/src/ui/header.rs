//! The header line.
//!
//! v1's: the program's name on the left — the tab strip already names the
//! request — and the environment picker on the right, where v1 kept its
//! dropdown. ⌃E or a click opens the list, and it drops from the picker rather
//! than from the middle of the screen.
//!
//! The name is styled after Claude Code: the mark carries the only colour.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::app::mouse::{Target, Targets};
use crate::theme;
use crate::ui;

/// v1's dropdown was sixteen wide; a longer label is cut to it.
const PICKER_LABEL: usize = 16;

pub fn draw(frame: &mut Frame, app: &App, area: Rect, targets: &mut Targets) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {}  ", theme::MARK), theme::brand()),
            Span::styled("binman", theme::muted()),
        ]))
        .style(theme::status_bar()),
        area,
    );

    // The picker is the control, so on a short line it keeps its place and
    // the name gives way under it.
    let picker = picker(app);
    let width = u16::try_from(ui::width_of(&picker)).unwrap_or(u16::MAX);
    if width > area.width {
        return;
    }
    let spot = Rect {
        x: area.right() - width,
        width,
        ..area
    };
    frame.render_widget(
        Paragraph::new(Line::from(picker)).style(theme::status_bar()),
        spot,
    );
    targets.add(spot, Target::EnvPicker);
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
