//! Mouse handling.
//!
//! Each draw records what it put where — a pane, a tab, a row, the environment
//! picker — and a click is looked up in what was drawn last. A click on a row
//! that is already selected does what Enter does there, so the mouse reaches
//! the same things the keys do and behaves the same once it gets there.
//!
//! The one thing the mouse does that no key does is pull the seams between
//! the panes. How much room the collections, the request and the response
//! each want depends on what is in them, which no layout rule can know — so
//! it is left to the hand.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use tui_textarea::Input;

use super::overlay::Overlay;
use super::tab::{Section, View};
use super::{App, Pane, keys};

/// How far a turn of the wheel moves text. Lists move a row at a time.
const SCROLL_LINES: isize = 3;

/// Something on screen a click can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Anywhere in a pane: a click there gives it focus.
    Pane(Pane),
    EnvPicker,
    Tab(usize),
    NewTab,
    Method,
    /// The URL's text, drawn from its `start`th character.
    Url {
        start: usize,
    },
    /// Send, or Cancel while the request is out.
    Send,
    Section(Section),
    /// What kind of body or auth the section holds.
    Kind,
    /// A row of the request's section: a table row, an auth field, a variable.
    Row(usize),
    /// The body or the rules, as text.
    Editor,
    View(View),
    /// A row of the collections, by its place among the visible rows.
    Node(usize),
    Palette,
    Help,
    CycleFocus,
    /// An entry of the open list, by its place among the matches.
    Entry(usize),
    /// The open list itself, so a click inside it does not count as a click
    /// away from it.
    Overlay,
}

impl Target {
    /// The pane a click on this gives focus to.
    fn pane(self) -> Option<Pane> {
        match self {
            Target::Pane(pane) => Some(pane),
            Target::Method | Target::Url { .. } => Some(Pane::Url),
            Target::Section(_) | Target::Kind | Target::Row(_) | Target::Editor => {
                Some(Pane::Request)
            }
            Target::View(_) => Some(Pane::Response),
            Target::Node(_) => Some(Pane::Collections),
            _ => None,
        }
    }
}

/// A seam between panes that can be dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    /// Between the collections and the request over the response. Moves left
    /// and right.
    Sidebar,
    /// Between the request and the response. Moves up and down.
    Split,
}

/// Where each target was drawn, in the order it was.
#[derive(Debug, Default)]
pub struct Targets(Vec<(Rect, Target)>);

impl Targets {
    pub fn add(&mut self, area: Rect, target: Target) {
        if !area.is_empty() {
            self.0.push((area, target));
        }
    }

    /// What is under a cell: the target drawn last, since it sits on top.
    fn at(&self, column: u16, row: u16) -> Option<(Rect, Target)> {
        self.0
            .iter()
            .rev()
            .find(|(area, _)| area.contains(Position::new(column, row)))
            .copied()
    }

    /// Where a pane was drawn, border and all.
    fn pane(&self, pane: Pane) -> Option<Rect> {
        self.0
            .iter()
            .find(|(_, target)| *target == Target::Pane(pane))
            .map(|(area, _)| *area)
    }
}

pub fn handle(app: &mut App, event: MouseEvent) {
    let under = app.targets.at(event.column, event.row);
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.dragging = divider_at(app, under, event);
            if app.dragging.is_none() {
                click(app, under, event.column);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => drag(app, event),
        MouseEventKind::Up(MouseButton::Left) => app.dragging = None,
        // A middle click closes a tab, as it does in a browser.
        MouseEventKind::Down(MouseButton::Middle) => {
            if app.overlay.is_none()
                && let Some((_, Target::Tab(index))) = under
            {
                app.select_tab(index);
                app.close_tab();
            }
        }
        MouseEventKind::ScrollDown => scroll(app, under, 1, event),
        MouseEventKind::ScrollUp => scroll(app, under, -1, event),
        _ => {}
    }
}

/// Which seam a press landed on, if either.
///
/// Two lines count for each: a pane's own border and the one drawn against
/// it. A single line is a hard thing to hit with a pointer, and the two are
/// touching, so anyone aiming at the seam means either. Only bare border
/// counts — a view drawn in the response's top border is still a view to
/// click — and nothing behind an open modal is within reach.
fn divider_at(app: &App, under: Option<(Rect, Target)>, event: MouseEvent) -> Option<Divider> {
    if app.overlay.is_some() || !matches!(under, Some((_, Target::Pane(_)))) {
        return None;
    }
    let (column, row) = (event.column, event.row);

    if let Some(collections) = app.targets.pane(Pane::Collections) {
        let edge = collections.right().saturating_sub(1);
        if (column == edge || column == edge.saturating_add(1))
            && row >= collections.y
            && row < collections.bottom()
        {
            return Some(Divider::Sidebar);
        }
    }
    if let Some(request) = app.targets.pane(Pane::Request) {
        let edge = request.bottom().saturating_sub(1);
        if (row == edge || row == edge.saturating_add(1))
            && column >= request.x
            && column < request.right()
        {
            return Some(Divider::Split);
        }
    }
    None
}

/// Moves the seam being dragged to the pointer. The border is drawn on the
/// cell under the pointer, so the pane runs one past it.
fn drag(app: &mut App, event: MouseEvent) {
    match app.dragging {
        Some(Divider::Sidebar) => {
            if let Some(collections) = app.targets.pane(Pane::Collections) {
                app.explorer_width =
                    Some(event.column.saturating_add(1).saturating_sub(collections.x));
            }
        }
        Some(Divider::Split) => {
            if let Some(request) = app.targets.pane(Pane::Request) {
                app.request_height = Some(event.row.saturating_add(1).saturating_sub(request.y));
            }
        }
        None => {}
    }
}

fn click(app: &mut App, under: Option<(Rect, Target)>, column: u16) {
    if app.overlay.is_some() {
        click_overlay(app, under.map(|(_, target)| target));
        return;
    }
    let Some((area, target)) = under else {
        return;
    };

    // Whatever was being typed into a cell is kept, as Enter would keep it,
    // and typing into the body stops unless the click is on the body.
    let tab = app.tab_mut();
    tab.commit_edits();
    if target != Target::Editor {
        tab.editing = false;
    }
    let focused = target.pane() == Some(app.focus);
    if let Some(pane) = target.pane() {
        app.focus = pane;
    }

    match target {
        Target::Pane(_) | Target::Entry(_) | Target::Overlay => {}
        Target::EnvPicker => app.open_envs(),
        Target::Tab(index) => app.select_tab(index),
        Target::NewTab => app.new_tab(),
        Target::Method => app.tab_mut().cycle_method(1),
        Target::Url { start } => {
            let offset = usize::from(column - area.x);
            app.tab_mut().url.place_cursor(start, offset);
        }
        Target::Send => {
            if app.tab().is_sending() {
                app.cancel_request();
            } else {
                app.send();
            }
        }
        Target::Section(section) => app.tab_mut().section = section,
        Target::Kind => {
            let tab = app.tab_mut();
            match tab.section {
                Section::Body => tab.body_kind = tab.body_kind.cycle(1),
                Section::Auth => tab.auth.cycle(1),
                _ => {}
            }
        }
        Target::Row(index) => row(app, index, focused),
        Target::Editor => app.tab_mut().editing = true,
        Target::View(view) => app.tab_mut().response.show(view),
        Target::Node(index) => {
            app.tree.selected = index;
            app.activate_selected();
        }
        Target::Palette => app.open_palette(),
        Target::Help => app.overlay = Some(Overlay::Help),
        Target::CycleFocus => keys::cycle_focus(app, 1),
    }
}

fn click_overlay(app: &mut App, target: Option<Target>) {
    match app.overlay.as_mut() {
        // These close on any key, so on any click too.
        Some(Overlay::Help | Overlay::Splash | Overlay::Curl(_)) => app.overlay = None,
        Some(Overlay::Picker(picker)) => match target {
            Some(Target::Entry(position)) => {
                picker.selected = position;
                if let Some(Overlay::Picker(picker)) = app.overlay.take()
                    && let Some(action) = picker.into_action()
                {
                    app.choose(action);
                }
            }
            Some(Target::Overlay) => {}
            // A click away from a list closes it, as Esc does — the picker in
            // the header included, so it closes its own dropdown.
            _ => app.overlay = None,
        },
        // An editor or a prompt holds typing that a stray click must not
        // throw away.
        Some(
            Overlay::Env(_)
            | Overlay::SaveRequest(_)
            | Overlay::SaveResponse(_)
            | Overlay::Collection(_),
        )
        | None => {}
    }
}

/// A row of the request's section. The first click selects it; a click on the
/// selected row of the pane in focus does what Enter does there, and the row
/// that adds one adds at once.
fn row(app: &mut App, index: usize, focused: bool) {
    let tab = app.tab_mut();
    let enter = match tab.section {
        Section::Params | Section::Headers | Section::Body => {
            let table = match tab.section {
                Section::Params => &mut tab.params,
                Section::Headers => &mut tab.headers,
                _ => &mut tab.form,
            };
            let enter = (focused && table.selected == index) || index == table.rows.len();
            table.selected = index;
            enter
        }
        Section::Auth => {
            let enter = focused && tab.auth.selected == index;
            tab.auth.selected = index;
            enter
        }
        Section::Vars => {
            let enter = focused && tab.vars.selected == index;
            tab.vars.selected = index;
            enter
        }
        Section::Scripts | Section::Info => false,
    };
    if enter {
        keys::handle(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }
}

/// The wheel moves whatever is under the pointer, focused or not.
fn scroll(app: &mut App, under: Option<(Rect, Target)>, delta: isize, event: MouseEvent) {
    match app.overlay.as_mut() {
        Some(Overlay::Picker(picker)) => {
            let last = picker.matches().len().saturating_sub(1);
            picker.selected = picker.selected.saturating_add_signed(delta).min(last);
            return;
        }
        Some(Overlay::Env(editor)) => {
            editor.editor.input(Input::from(event));
            return;
        }
        Some(_) => return,
        None => {}
    }

    let Some(pane) = under.and_then(|(_, target)| target.pane()) else {
        return;
    };
    match pane {
        Pane::Collections => app.tree.move_selection(delta),
        Pane::Url => {}
        Pane::Request => {
            let tab = app.tab_mut();
            // A cell being typed into stays where it is.
            if tab.is_typing() && !tab.editing {
                return;
            }
            match tab.section {
                Section::Params => tab.params.move_selection(delta),
                Section::Headers => tab.headers.move_selection(delta),
                Section::Body if tab.body_kind.is_form() => tab.form.move_selection(delta),
                Section::Body => {
                    tab.body.input(Input::from(event));
                }
                Section::Auth => tab.auth.move_selection(delta),
                Section::Vars => {
                    let last = tab.referenced().len().saturating_sub(1);
                    tab.vars.selected = tab.vars.selected.saturating_add_signed(delta).min(last);
                }
                Section::Scripts => {
                    tab.scripts.input(Input::from(event));
                }
                Section::Info => {}
            }
        }
        Pane::Response => app.tab_mut().response.scroll_by(delta * SCROLL_LINES),
    }
}
