use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::mouse::{Target, Targets};
use crate::app::tree::{LoadState, NodeId, NodeKind};
use crate::app::{App, Pane};
use crate::theme;
use crate::ui;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect, targets: &mut Targets) {
    let focused = app.focus == Pane::Collections;
    let root = app
        .root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let block = ui::counted_pane("Collections", root, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    targets.add(area, Target::Pane(Pane::Collections));

    if app.tree.roots.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("Nothing to open here yet.", theme::muted())),
                Line::from(""),
                Line::from(Span::styled(
                    "binman opens .http, .bru and .graphql files, Postman collections and OpenAPI specs.",
                    theme::dim(),
                )),
            ])
            .wrap(ratatui::widgets::Wrap { trim: false }),
            inner,
        );
        return;
    }

    let visible = app.tree.visible();
    let height = inner.height as usize;
    app.tree.offset = ui::scroll_offset(app.tree.offset, app.tree.selected, height);

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .skip(app.tree.offset)
        .take(height)
        .map(|(index, entry)| {
            let selected = index == app.tree.selected;
            let line = render_node(app, entry.id, entry.depth, selected, focused);
            if selected {
                ui::highlight(line, focused, inner.width)
            } else {
                line
            }
        })
        .collect();
    for (row, index) in (app.tree.offset..visible.len()).take(height).enumerate() {
        targets.add(ui::line_at(inner, row), Target::Node(index));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_node(app: &App, id: NodeId, depth: usize, selected: bool, focused: bool) -> Line<'static> {
    let Some(node) = app.tree.find(id) else {
        return Line::default();
    };

    let mut spans = vec![ui::bar(selected, focused), Span::raw("  ".repeat(depth))];
    let expander = match (node.state, node.expanded) {
        (LoadState::Leaf, _) => "  ",
        (_, true) => "▾ ",
        (_, false) => "▸ ",
    };
    spans.push(Span::styled(expander, theme::dim()));

    let count = |spans: &mut Vec<Span<'static>>| {
        let requests = node
            .children
            .iter()
            .filter(|child| !matches!(child.kind, NodeKind::Note { .. }))
            .count();
        if node.state == LoadState::Loaded {
            spans.push(Span::styled(format!("  {requests}"), theme::dim()));
        }
    };

    match &node.kind {
        NodeKind::Dir { name, .. } => {
            let icon = if node.expanded {
                theme::ICON_FOLDER_OPEN
            } else {
                theme::ICON_FOLDER
            };
            spans.push(Span::styled(format!("{icon} "), theme::node_dir()));
            spans.push(Span::styled(name.clone(), theme::node_dir()));
            count(&mut spans);
        }
        NodeKind::File { name, method, .. } => {
            spans.push(badge(method.as_deref()));
            spans.push(Span::styled(name.clone(), theme::node_file()));
        }
        NodeKind::Collection { name, .. } => {
            spans.push(Span::styled(
                format!("{} ", theme::ICON_COLLECTION),
                theme::node_collection(),
            ));
            spans.push(Span::styled(name.clone(), theme::node_collection()));
        }
        NodeKind::Folder { name } => {
            spans.push(Span::styled(format!("{} ", theme::ICON_FOLDER), theme::node_dir()));
            spans.push(Span::styled(name.clone(), theme::node_dir()));
            count(&mut spans);
        }
        NodeKind::Item { name, method, .. } => {
            spans.push(badge(Some(method)));
            spans.push(Span::styled(name.clone(), theme::node_file()));
        }
        NodeKind::Spec { name, .. } => {
            spans.push(Span::styled(format!("{} ", theme::ICON_SPEC), theme::node_spec()));
            spans.push(Span::styled(name.clone(), theme::node_spec()));
        }
        NodeKind::Tag { name } => {
            spans.push(Span::styled(format!("{} ", theme::ICON_TAG), theme::node_tag()));
            spans.push(Span::styled(name.clone(), theme::node_tag()));
            count(&mut spans);
        }
        NodeKind::Operation {
            route,
            method,
            summary,
            ..
        } => {
            spans.push(badge(Some(method)));
            spans.push(Span::styled(route.clone(), theme::node_file()));
            if !summary.is_empty() {
                spans.push(Span::styled(format!("  {summary}"), theme::dim()));
            }
        }
        NodeKind::Note { text, is_error } => {
            let style = if *is_error {
                theme::danger()
            } else {
                theme::dim()
            };
            spans.push(Span::styled(text.clone(), style));
        }
    }
    Line::from(spans)
}

/// The method, in its colour, in a column wide enough for DELETE — so the
/// names after it line up whatever they send.
pub fn badge(method: Option<&str>) -> Span<'static> {
    match method {
        Some(method) => {
            let short = if method == "OPTIONS" { "OPT" } else { method };
            Span::styled(format!("{short:<7}"), theme::method(method))
        }
        None => Span::raw(" ".repeat(7)),
    }
}
