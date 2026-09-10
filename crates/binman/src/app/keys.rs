//! Key handling.
//!
//! Four panes, each with its own keys, and a set that works anywhere. As in
//! binsql, nothing shadows typing: pane movement is on Alt, because the Ctrl
//! letters belong to commands.

use binman_core::BodyKind;
use binman_core::formats::curl;
use binman_core::vars::Vars;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tui_textarea::{CursorMove, Input, Key};

use super::kv::KvTable;
use super::line::LineInput;
use super::overlay::{Action, Overlay, PickerKind};
use super::tab::{Section, Tab};
use super::{App, Pane};

/// How far ⌃D and ⌃U move.
const HALF_PAGE: isize = 10;

pub fn handle(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }

    // Quit is handled before anything else can claim it. Raw mode has already
    // taken ⌃C away from the terminal — it arrives here as a key like any
    // other, and cancels a request rather than the program — so a modal that
    // swallowed ⌃Q would leave no way out at all.
    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    if app.overlay.is_some() {
        overlay(app, key);
        return;
    }
    if global(app, key) {
        return;
    }
    match app.focus {
        Pane::Collections => collections(app, key),
        Pane::Url => url(app, key),
        Pane::Request => request(app, key),
        Pane::Response => response(app, key),
    }
}

/// Returns true when the key was consumed.
fn global(app: &mut App, key: KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match (key.code, ctrl, alt) {
        (KeyCode::Char('k'), true, _) => app.open_palette(),
        // ⌃R is binsql's run key and the one the status line names; ⌃J was
        // v1's, and still works.
        (KeyCode::Char('r' | 'j'), true, _) => app.send(),
        (KeyCode::Char('c'), true, _) => app.cancel_request(),
        (KeyCode::Char('t'), true, _) => app.new_tab(),
        (KeyCode::Char('w'), true, _) => app.close_tab(),
        (KeyCode::Char('f'), true, _) => app.open_find(),
        (KeyCode::Char('h'), true, _) => app.open_history(),
        (KeyCode::Char('e'), true, _) => app.open_envs(),
        (KeyCode::Char('s'), true, _) => app.save(),
        (KeyCode::Char('y'), true, _) => app.copy_curl(),
        (KeyCode::F(1), _, _) => app.overlay = Some(Overlay::Help),
        (KeyCode::F(5), _, _) => app.reload_tree(),

        (KeyCode::Char('h'), _, true) => app.focus = Pane::Collections,
        (KeyCode::Char('k'), _, true) => app.focus = Pane::Url,
        (KeyCode::Char('l'), _, true) => app.focus = Pane::Request,
        (KeyCode::Char('j'), _, true) => app.focus = Pane::Response,

        (KeyCode::Char(digit @ '1'..='9'), _, true) => {
            app.select_tab(digit as usize - '1' as usize);
        }
        (KeyCode::PageUp, true, _) => app.cycle_tab(-1),
        (KeyCode::PageDown, true, _) => app.cycle_tab(1),

        // Tab indents a body and moves from a header's name to its value, so
        // it only cycles panes when nothing is being typed.
        (KeyCode::BackTab, _, _) if !app.wants_tab() => cycle_focus(app, -1),
        (KeyCode::Tab, _, _) if !app.wants_tab() => cycle_focus(app, 1),

        _ => return false,
    }
    true
}

pub(super) fn cycle_focus(app: &mut App, delta: isize) {
    let order = Pane::ORDER;
    let position = order.iter().position(|pane| *pane == app.focus).unwrap_or(0) as isize;
    app.focus = order[(position + delta).rem_euclid(order.len() as isize) as usize];
}

fn collections(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.tree.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => app.tree.move_selection(-1),
        KeyCode::Char('d') if ctrl => app.tree.move_selection(HALF_PAGE),
        KeyCode::Char('u') if ctrl => app.tree.move_selection(-HALF_PAGE),
        KeyCode::PageDown => app.tree.move_selection(HALF_PAGE * 2),
        KeyCode::PageUp => app.tree.move_selection(-HALF_PAGE * 2),
        KeyCode::Char('g') | KeyCode::Home => app.tree.select_first(),
        KeyCode::Char('G') | KeyCode::End => app.tree.select_last(),

        KeyCode::Char('h') | KeyCode::Left => app.tree.collapse_or_parent(),
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Char(' ') => app.toggle_selected(),
        KeyCode::Enter => app.activate_selected(),

        KeyCode::Char('r') => app.reload_tree(),
        KeyCode::Char('?') => app.overlay = Some(Overlay::Help),
        _ => {}
    }
}

/// The URL is always being typed into, as binsql's editor is: letters are the
/// URL's, so what is not text is on keys a URL has no use for.
fn url(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            if curl::is_curl(app.tab().url.text()) {
                app.import_curl();
            } else {
                app.send();
            }
        }
        KeyCode::Up => app.tab_mut().cycle_method(-1),
        KeyCode::Down => app.tab_mut().cycle_method(1),
        KeyCode::Esc => app.focus = Pane::Collections,
        _ => {
            let tab = app.tab_mut();
            if tab.url.handle(key) {
                tab.sync_params();
            }
        }
    }
}

fn request(app: &mut App, key: KeyEvent) {
    let tab = app.tab_mut();

    // Typing into the body or the scripts: every key is the editor's, bar
    // the one that stops.
    if tab.editing {
        if key.code == KeyCode::Esc {
            tab.editing = false;
            return;
        }
        let input = Input::from(key);
        if input.key == Key::Null {
            return;
        }
        let editor = match tab.section {
            Section::Scripts => &mut tab.scripts,
            _ => &mut tab.body,
        };
        editor.input(input);
        return;
    }

    if tab.is_typing() {
        cell(tab, key);
        return;
    }

    match key.code {
        KeyCode::Char('[') => tab.section = tab.section.cycle(-1),
        KeyCode::Char(']') => tab.section = tab.section.cycle(1),
        KeyCode::Char('?') => app.overlay = Some(Overlay::Help),
        KeyCode::Esc => app.focus = Pane::Collections,
        _ => {
            let App {
                tabs,
                active,
                extracted,
                ..
            } = app;
            section(&mut tabs[*active], extracted, key);
        }
    }
}

/// A key while a cell or field is being typed into.
fn cell(tab: &mut Tab, key: KeyEvent) {
    match tab.section {
        Section::Params | Section::Headers | Section::Body => {
            let is_params = tab.section == Section::Params;
            let table = match tab.section {
                Section::Params => &mut tab.params,
                Section::Headers => &mut tab.headers,
                _ => &mut tab.form,
            };
            let changed = match key.code {
                KeyCode::Esc => {
                    table.cancel();
                    false
                }
                KeyCode::Enter | KeyCode::Tab => table.advance(),
                KeyCode::BackTab => {
                    table.back();
                    false
                }
                _ => {
                    if let Some(input) = table.input() {
                        input.handle(key);
                    }
                    false
                }
            };
            if changed && is_params {
                tab.params_to_url();
            }
        }
        Section::Auth => match key.code {
            KeyCode::Esc => tab.auth.edit = None,
            KeyCode::Enter | KeyCode::Tab => tab.auth.commit(),
            _ => {
                if let Some(input) = &mut tab.auth.edit {
                    input.handle(key);
                }
            }
        },
        Section::Vars => match key.code {
            KeyCode::Esc => tab.vars.edit = None,
            KeyCode::Enter | KeyCode::Tab => {
                if let Some((name, input)) = tab.vars.edit.take() {
                    tab.overrides.insert(name, input.text().to_string());
                }
            }
            _ => {
                if let Some((_, input)) = &mut tab.vars.edit {
                    input.handle(key);
                }
            }
        },
        Section::Scripts | Section::Info => {}
    }
}

/// A key in a section, when nothing is being typed.
fn section(tab: &mut Tab, extracted: &Vars, key: KeyEvent) {
    match tab.section {
        Section::Params => {
            if table(&mut tab.params, key) {
                tab.params_to_url();
            }
        }
        Section::Headers => {
            table(&mut tab.headers, key);
        }
        Section::Body => body(tab, key),
        Section::Auth => match key.code {
            KeyCode::Char('h') | KeyCode::Left => tab.auth.cycle(-1),
            KeyCode::Char('l') | KeyCode::Right => tab.auth.cycle(1),
            KeyCode::Char('j') | KeyCode::Down => tab.auth.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => tab.auth.move_selection(-1),
            KeyCode::Enter | KeyCode::Char('i' | 'e') => tab.auth.begin(),
            KeyCode::Char('d') | KeyCode::Delete => tab.auth.clear(),
            _ => {}
        },
        Section::Vars => vars(tab, extracted, key),
        Section::Scripts => match key.code {
            KeyCode::Enter | KeyCode::Char('i') => tab.editing = true,
            KeyCode::Char('j') | KeyCode::Down => tab.scripts.move_cursor(CursorMove::Down),
            KeyCode::Char('k') | KeyCode::Up => tab.scripts.move_cursor(CursorMove::Up),
            _ => {}
        },
        Section::Info => {}
    }
}

/// Moving and editing in a table. Returns whether its rows changed.
fn table(table: &mut KvTable, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => table.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => table.move_selection(-1),
        KeyCode::Char('g') | KeyCode::Home => table.first(),
        KeyCode::Char('G') | KeyCode::End => table.last(),
        KeyCode::Enter | KeyCode::Char('i' | 'e') => table.begin(),
        KeyCode::Char('a' | 'o') => table.add(),
        KeyCode::Char('d') | KeyCode::Delete => return table.delete(),
        _ => {}
    }
    false
}

fn body(tab: &mut Tab, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => tab.body_kind = tab.body_kind.cycle(-1),
        KeyCode::Char('l') | KeyCode::Right => tab.body_kind = tab.body_kind.cycle(1),
        _ if tab.body_kind.is_form() => {
            table(&mut tab.form, key);
        }
        KeyCode::Enter | KeyCode::Char('i') if tab.body_kind != BodyKind::None => {
            tab.editing = true;
        }
        KeyCode::Char('j') | KeyCode::Down => tab.body.move_cursor(CursorMove::Down),
        KeyCode::Char('k') | KeyCode::Up => tab.body.move_cursor(CursorMove::Up),
        KeyCode::Char('g') | KeyCode::Home => tab.body.move_cursor(CursorMove::Top),
        KeyCode::Char('G') | KeyCode::End => tab.body.move_cursor(CursorMove::Bottom),
        _ => {}
    }
}

fn vars(tab: &mut Tab, extracted: &Vars, key: KeyEvent) {
    let names = tab.referenced();
    let last = names.len().saturating_sub(1);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => tab.vars.selected = (tab.vars.selected + 1).min(last),
        KeyCode::Char('k') | KeyCode::Up => tab.vars.selected = tab.vars.selected.saturating_sub(1),
        KeyCode::Enter | KeyCode::Char('i' | 'e') => {
            if let Some(name) = names.get(tab.vars.selected) {
                let value = tab
                    .scope(extracted)
                    .lookup(name)
                    .map(|(value, _)| value.to_string())
                    .unwrap_or_default();
                tab.vars.edit = Some((name.clone(), LineInput::new(value)));
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(name) = names.get(tab.vars.selected) {
                tab.overrides.remove(name);
            }
        }
        _ => {}
    }
}

fn response(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let response = &mut app.tab_mut().response;
    match key.code {
        KeyCode::Char('[') => response.show(response.view.cycle(-1)),
        KeyCode::Char(']') => response.show(response.view.cycle(1)),
        KeyCode::Char('j') | KeyCode::Down => response.scroll_by(1),
        KeyCode::Char('k') | KeyCode::Up => response.scroll_by(-1),
        KeyCode::Char('d') if ctrl => response.scroll_by(HALF_PAGE),
        KeyCode::Char('u') if ctrl => response.scroll_by(-HALF_PAGE),
        KeyCode::PageDown => response.scroll_by(HALF_PAGE * 2),
        KeyCode::PageUp => response.scroll_by(-HALF_PAGE * 2),
        KeyCode::Char('g') | KeyCode::Home => response.to_top(),
        KeyCode::Char('G') | KeyCode::End => response.to_bottom(),
        KeyCode::Char('?') => app.overlay = Some(Overlay::Help),
        KeyCode::Esc => app.focus = Pane::Collections,
        _ => {}
    }
}

fn overlay(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match app.overlay.as_mut() {
        // Help says "any key closes this" and it has to be true, or the way
        // out is a guess. The splash is a greeting, not a prompt, and the
        // curl view is there to be read.
        Some(Overlay::Help | Overlay::Splash | Overlay::Curl(_)) => app.overlay = None,

        Some(Overlay::Picker(picker)) => match key.code {
            KeyCode::Esc => app.overlay = None,
            KeyCode::Up => picker.move_selection(-1),
            KeyCode::Down => picker.move_selection(1),
            KeyCode::Char('k') if !picker.kind.filters() => picker.move_selection(-1),
            KeyCode::Char('j') if !picker.kind.filters() => picker.move_selection(1),
            KeyCode::Char('e') if picker.kind == PickerKind::Environments => {
                let index = picker.chosen().and_then(|index| match picker.entries[index].action {
                    Action::UseEnv(env) => env,
                    _ => None,
                });
                app.overlay = None;
                app.edit_env(index);
            }
            KeyCode::Backspace => picker.backspace(),
            KeyCode::Enter => {
                if let Some(Overlay::Picker(picker)) = app.overlay.take()
                    && let Some(action) = picker.into_action()
                {
                    app.choose(action);
                }
            }
            KeyCode::Char(ch) if picker.kind.filters() && !ctrl => picker.push(ch),
            _ => {}
        },

        Some(Overlay::Env(editor)) => match key.code {
            KeyCode::Char('s') if ctrl => app.save_env(),
            KeyCode::Esc => app.overlay = None,
            _ => {
                let input = Input::from(key);
                if input.key != Key::Null {
                    editor.editor.input(input);
                }
            }
        },

        Some(Overlay::SaveResponse(prompt)) => match key.code {
            KeyCode::Esc => app.overlay = None,
            KeyCode::Enter => app.save_response(),
            _ => {
                prompt.input.handle(key);
                prompt.error = None;
            }
        },

        None => {}
    }
}
