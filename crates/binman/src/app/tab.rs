//! One open request: what it says, where it came from, and what came back.
//!
//! Tabs are the strip across the top of the workspace, as binsql's consoles
//! are. Each keeps its own edits, its own environment and its own last
//! response, so a login and the call it authorises sit side by side.

use std::collections::HashMap;
use std::time::Instant;

use binman_core::body::{self, BodyKind};
use binman_core::extract::{self, Rule};
use binman_core::vars::{self, Scope, Vars};
use binman_core::{
    Auth, AuthKind, EnvSource, Exchange, Loaded, METHODS, Origin, Prepared, Request, query,
};
use ratatui::style::Style;
use ratatui::text::Line;
use tokio_util::sync::CancellationToken;
use tui_textarea::TextArea;

use super::kv::KvTable;
use super::line::LineInput;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Params,
    Headers,
    Body,
    Auth,
    Vars,
    Scripts,
    Info,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::Params,
        Section::Headers,
        Section::Body,
        Section::Auth,
        Section::Vars,
        Section::Scripts,
        Section::Info,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Params => "Params",
            Section::Headers => "Headers",
            Section::Body => "Body",
            Section::Auth => "Auth",
            Section::Vars => "Vars",
            Section::Scripts => "Scripts",
            Section::Info => "Info",
        }
    }

    pub fn cycle(self, delta: isize) -> Section {
        cycle(&Self::ALL, self, delta)
    }
}

/// What the response pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Body,
    Headers,
    Cookies,
    Scripts,
    Trace,
}

impl View {
    pub const ALL: [View; 5] = [
        View::Body,
        View::Headers,
        View::Cookies,
        View::Scripts,
        View::Trace,
    ];

    pub fn label(self) -> &'static str {
        match self {
            View::Body => "Body",
            View::Headers => "Headers",
            View::Cookies => "Cookies",
            View::Scripts => "Scripts",
            View::Trace => "Trace",
        }
    }

    pub fn cycle(self, delta: isize) -> View {
        cycle(&Self::ALL, self, delta)
    }
}

fn cycle<T: Copy + PartialEq>(all: &[T], current: T, delta: isize) -> T {
    let position = all.iter().position(|item| *item == current).unwrap_or(0) as isize;
    all[(position + delta).rem_euclid(all.len() as isize) as usize]
}

/// The Auth section. Values are kept per kind, so stepping past Basic on the
/// way to Bearer does not lose the username.
#[derive(Debug)]
pub struct AuthForm {
    pub kind: AuthKind,
    values: HashMap<(AuthKind, usize), String>,
    pub selected: usize,
    pub edit: Option<LineInput>,
}

impl Default for AuthForm {
    fn default() -> AuthForm {
        AuthForm {
            kind: AuthKind::None,
            values: HashMap::new(),
            selected: 0,
            edit: None,
        }
    }
}

impl AuthForm {
    /// The section filled from what a file says.
    fn loaded(auth: &Auth) -> AuthForm {
        let mut form = AuthForm {
            kind: auth.kind,
            ..AuthForm::default()
        };
        for (index, value) in auth.values.iter().enumerate() {
            form.values.insert((auth.kind, index), value.clone());
        }
        form
    }

    /// The kind in use and its values, as a file holds them.
    pub fn auth(&self) -> Auth {
        Auth {
            kind: self.kind,
            values: self.values(),
        }
    }

    pub fn value(&self, index: usize) -> &str {
        self.values
            .get(&(self.kind, index))
            .map(String::as_str)
            .unwrap_or("")
    }

    /// The current kind's values, in field order.
    pub fn values(&self) -> Vec<String> {
        (0..self.kind.fields().len())
            .map(|index| self.value(index).to_string())
            .collect()
    }

    pub fn cycle(&mut self, delta: isize) {
        self.kind = self.kind.cycle(delta);
        self.selected = 0;
        self.edit = None;
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.kind.fields().len() as isize;
        if count > 0 {
            self.selected = (self.selected as isize + delta).clamp(0, count - 1) as usize;
        }
    }

    pub fn begin(&mut self) {
        if self.selected < self.kind.fields().len() {
            self.edit = Some(LineInput::new(self.value(self.selected)));
        }
    }

    pub fn commit(&mut self) {
        if let Some(edit) = self.edit.take() {
            self.values
                .insert((self.kind, self.selected), edit.text().to_string());
        }
    }

    pub fn clear(&mut self) {
        self.values.remove(&(self.kind, self.selected));
    }
}

/// Where the Vars section is, and the override being typed.
#[derive(Debug, Default)]
pub struct VarsView {
    pub selected: usize,
    pub offset: usize,
    pub edit: Option<(String, LineInput)>,
}

pub enum Outcome {
    /// Nothing has been sent from this tab yet.
    Idle,
    Sending {
        started: Instant,
        /// A stream as far as it has arrived.
        streamed: String,
    },
    Received(Box<Received>),
    /// Called off. Not an error: nobody wanted the answer.
    Cancelled,
    Failed(String),
}

/// A response and what was made of it.
pub struct Received {
    pub exchange: Exchange,
    /// What the Scripts rules pulled out of it.
    pub extracted: Vars,
    /// The body as it is shown: re-indented and coloured when it is JSON.
    pub lines: Vec<Line<'static>>,
    /// What actually went out, token and all.
    pub sent: Prepared,
}

pub struct Response {
    pub view: View,
    pub scroll: usize,
    /// How far down it is worth scrolling, written by the renderer once it
    /// knows how many lines there are.
    pub max_scroll: usize,
    /// How many lines there are, wrapped to the pane, for the status line.
    pub total: usize,
    /// Keeps the end of a stream in view while it arrives, until the reader
    /// scrolls away from it.
    pub follow: bool,
    pub outcome: Outcome,
}

impl Response {
    fn new() -> Response {
        Response {
            view: View::Body,
            scroll: 0,
            max_scroll: 0,
            total: 0,
            follow: false,
            outcome: Outcome::Idle,
        }
    }

    pub fn received(&self) -> Option<&Received> {
        match &self.outcome {
            Outcome::Received(received) => Some(received),
            _ => None,
        }
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let next = (self.scroll as isize + delta).clamp(0, self.max_scroll as isize) as usize;
        self.scroll = next;
        self.follow = next >= self.max_scroll;
    }

    pub fn to_top(&mut self) {
        self.scroll = 0;
        self.follow = false;
    }

    pub fn to_bottom(&mut self) {
        self.scroll = self.max_scroll;
        self.follow = true;
    }

    pub fn show(&mut self, view: View) {
        self.view = view;
        self.scroll = 0;
        self.follow = false;
    }
}

/// What was loaded or last saved, to tell edits from what is on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    kind: BodyKind,
    body: String,
    auth: Auth,
}

pub struct Tab {
    /// Stable across tab closes, so a response can only land in the tab that
    /// asked for it.
    pub id: u64,
    pub title: String,
    /// `None` for a tab that came from nowhere on disk: a new tab, a replay
    /// from history.
    pub origin: Option<Origin>,
    pub method: String,
    pub url: LineInput,
    /// The URL's query string as rows. The URL is the one source of truth;
    /// these are rebuilt from it whenever it changes.
    pub params: KvTable,
    pub headers: KvTable,
    pub body_kind: BodyKind,
    pub body: TextArea<'static>,
    pub form: KvTable,
    pub auth: AuthForm,
    pub scripts: TextArea<'static>,
    pub vars: VarsView,
    /// Values typed into the Vars section, and only those. v1 filled the tab
    /// with every variable's value when a request opened and let all of them
    /// override, so a token extracted afterwards was hidden behind the empty
    /// one filled in before it existed.
    pub overrides: Vars,
    /// The request's own vars: Bruno's `vars:pre-request`.
    pub file_vars: Vars,
    pub collection_vars: Vars,
    pub envs: Vec<EnvSource>,
    pub env: Option<usize>,
    /// The selected environment, read when it is chosen and again before
    /// every send, so an edit made outside binman is picked up.
    pub env_vars: Vars,
    pub section: Section,
    /// Typing into the body or the scripts, rather than moving past them.
    pub editing: bool,
    pub response: Response,
    /// Calls off the request in flight. Held only while one is, so
    /// `is_sending` and this stay one fact rather than two.
    pub cancel: Option<CancellationToken>,
    /// Bumped on every send, so a response that arrives after a newer send
    /// has started is dropped instead of overwriting it.
    pub generation: u64,
    pristine: Snapshot,
}

impl Tab {
    pub fn blank(id: u64, envs: Vec<EnvSource>, env: Option<usize>) -> Tab {
        let mut tab = Tab {
            id,
            title: "new request".into(),
            origin: None,
            method: "GET".into(),
            url: LineInput::default(),
            params: KvTable::default(),
            headers: KvTable::default(),
            body_kind: BodyKind::None,
            body: body_editor(""),
            form: KvTable::default(),
            auth: AuthForm::default(),
            scripts: scripts_editor(),
            vars: VarsView::default(),
            overrides: Vars::new(),
            file_vars: Vars::new(),
            collection_vars: Vars::new(),
            envs,
            env,
            env_vars: Vars::new(),
            section: Section::Params,
            editing: false,
            response: Response::new(),
            cancel: None,
            generation: 0,
            pristine: Snapshot {
                method: String::new(),
                url: String::new(),
                headers: Vec::new(),
                kind: BodyKind::None,
                body: String::new(),
                auth: Auth::default(),
            },
        };
        tab.mark_saved();
        tab
    }

    /// Fills the tab from a request read from disk, auth included. What
    /// belonged to the request it replaces — its rules, its overrides, its
    /// response — goes with it.
    pub fn load(
        &mut self,
        loaded: Loaded,
        origin: Option<Origin>,
        envs: Vec<EnvSource>,
        env: Option<usize>,
    ) {
        self.title = loaded.title;
        self.origin = origin;
        self.apply(&loaded.request);
        self.file_vars = loaded.request.vars;
        self.collection_vars = loaded.collection_vars;
        self.envs = envs;
        self.env = env;
        self.auth = AuthForm::loaded(&loaded.request.auth);
        self.scripts = scripts_editor();
        self.overrides.clear();
        self.vars = VarsView::default();
        self.editing = false;
        self.response = Response::new();
        self.mark_saved();
    }

    /// Takes a request's method, URL, headers and body, and leaves where the
    /// tab points alone — so a curl command pasted into a tab that came from
    /// a file can be saved back to that file, as it could in v1.
    pub fn apply(&mut self, request: &Request) {
        self.method = if request.method.is_empty() {
            "GET".to_string()
        } else {
            request.method.to_ascii_uppercase()
        };
        self.url.set(request.url.clone());
        self.sync_params();
        self.headers.set_rows(request.headers.clone());
        self.body_kind = request.body_kind();
        if self.body_kind.is_form() {
            self.form.set_rows(body::parse_form(&request.body));
            self.body = body_editor("");
        } else {
            self.form.set_rows(Vec::new());
            self.body = body_editor(&request.body);
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            method: self.method.clone(),
            url: self.url.text().to_string(),
            headers: self.header_rows(),
            kind: self.body_kind,
            body: self.body_text(),
            auth: self.auth.auth(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.snapshot() != self.pristine
    }

    pub fn mark_saved(&mut self) {
        self.pristine = self.snapshot();
    }

    /// Headers as typed, less any row left without a name.
    pub fn header_rows(&self) -> Vec<(String, String)> {
        self.headers
            .rows
            .iter()
            .filter(|(name, _)| !name.trim().is_empty())
            .cloned()
            .collect()
    }

    /// The body as text: the editor's, or the form's fields encoded.
    pub fn body_text(&self) -> String {
        match self.body_kind {
            BodyKind::None => String::new(),
            kind if kind.is_form() => body::encode_form(&self.form.rows),
            _ => self.body.lines().join("\n"),
        }
    }

    /// The request as typed, variables unresolved: what is saved to disk.
    pub fn request(&self) -> Request {
        Request {
            method: self.method.clone(),
            url: self.url.text().trim().to_string(),
            headers: self.header_rows(),
            body: self.body_text(),
            kind: Some(self.body_kind),
            vars: self.file_vars.clone(),
            auth: self.auth.auth(),
        }
    }

    pub fn scope<'a>(&'a self, extracted: &'a Vars) -> Scope<'a> {
        Scope {
            collection: &self.collection_vars,
            environment: &self.env_vars,
            request: &self.file_vars,
            extracted,
            overrides: &self.overrides,
        }
    }

    /// Every variable the request refers to, in the order it first does, and
    /// then any override no longer referred to — so it can still be seen, and
    /// removed.
    pub fn referenced(&self) -> Vec<String> {
        let mut text = String::new();
        text.push_str(self.url.text());
        for (name, value) in &self.headers.rows {
            text.push('\n');
            text.push_str(name);
            text.push(':');
            text.push_str(value);
        }
        text.push('\n');
        text.push_str(&self.body_text());
        for value in self.auth.values() {
            text.push('\n');
            text.push_str(&value);
        }
        let mut names = vars::scan(&text);
        for name in self.overrides.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
    }

    /// Rebuilds the Params rows from the URL, unless one is being typed.
    pub fn sync_params(&mut self) {
        if self.params.edit.is_none() {
            let selected = self.params.selected;
            self.params.set_rows(query::pairs(self.url.text()));
            self.params.selected = selected.min(self.params.rows.len());
        }
    }

    /// Writes the Params rows back into the URL.
    pub fn params_to_url(&mut self) {
        let url = query::with_pairs(self.url.text(), &self.params.rows);
        self.url.set(url);
    }

    pub fn cycle_method(&mut self, delta: isize) {
        let next = match METHODS.iter().position(|method| *method == self.method) {
            Some(position) => {
                (position as isize + delta).rem_euclid(METHODS.len() as isize) as usize
            }
            None => 0,
        };
        self.method = METHODS[next].to_string();
    }

    pub fn env_source(&self) -> Option<&EnvSource> {
        self.envs.get(self.env?)
    }

    /// Reads the selected environment from disk.
    pub fn reload_env(&mut self) -> Result<(), String> {
        self.env_vars = match self.env_source() {
            Some(source) => source
                .load()
                .map_err(|error| format!("{}: {error}", source.path.display()))?,
            None => Vars::new(),
        };
        Ok(())
    }

    pub fn rules(&self) -> Vec<Rule> {
        extract::parse(&self.scripts.lines().join("\n"))
    }

    pub fn is_sending(&self) -> bool {
        matches!(self.response.outcome, Outcome::Sending { .. })
    }

    /// Calls off the request in flight. Returns whether there was one: saying
    /// "cancelled" when nothing was running is worse than saying nothing.
    pub fn cancel(&mut self) -> bool {
        match self.cancel.take() {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    /// Keeps whatever is being typed into a cell, as Enter would, so sending
    /// or saving mid-edit uses what is on screen.
    pub fn commit_edits(&mut self) {
        if self.params.commit() {
            self.params_to_url();
        }
        self.headers.commit();
        self.form.commit();
        self.auth.commit();
        if let Some((name, input)) = self.vars.edit.take() {
            self.overrides.insert(name, input.text().to_string());
        }
    }

    /// Whether a key pressed now is text rather than a command.
    pub fn is_typing(&self) -> bool {
        self.editing
            || self.params.edit.is_some()
            || self.headers.edit.is_some()
            || self.form.edit.is_some()
            || self.auth.edit.is_some()
            || self.vars.edit.is_some()
    }
}

/// Variables light up in the body the way binsql lights up SQL keywords:
/// tui-textarea colours every match of its search pattern, so the pattern is
/// the placeholder syntax. Nothing binds interactive search, so the trade
/// costs nothing.
fn body_editor(text: &str) -> TextArea<'static> {
    let mut editor = TextArea::from(text.lines());
    editor.set_cursor_line_style(Style::default());
    editor.set_selection_style(theme::selection(true));
    editor.set_placeholder_text("Enter to write the body");
    editor.set_placeholder_style(theme::dim());
    editor.set_search_style(theme::variable(true));
    let _ = editor.set_search_pattern(r"\{\{[^}]+\}\}");
    editor.set_tab_length(2);
    editor.set_hard_tab_indent(false);
    editor
}

fn scripts_editor() -> TextArea<'static> {
    let mut editor = TextArea::default();
    editor.set_cursor_line_style(Style::default());
    editor.set_selection_style(theme::selection(true));
    editor.set_placeholder_style(theme::dim());
    editor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab() -> Tab {
        Tab::blank(1, Vec::new(), None)
    }

    #[test]
    fn a_new_tab_is_not_dirty_until_something_is_typed() {
        let mut tab = tab();
        assert!(!tab.is_dirty());
        tab.url.set("https://x");
        assert!(tab.is_dirty());
        tab.mark_saved();
        assert!(!tab.is_dirty());
    }

    #[test]
    fn params_follow_the_url_and_write_back_to_it() {
        let mut tab = tab();
        tab.url.set("{{BASE}}/users?page=1");
        tab.sync_params();
        assert_eq!(tab.params.rows, vec![("page".to_string(), "1".to_string())]);

        tab.params.add();
        tab.params.input().unwrap().set("id");
        tab.params.advance();
        tab.params.input().unwrap().set("{{ID}}");
        tab.params.advance();
        tab.params_to_url();
        assert_eq!(tab.url.text(), "{{BASE}}/users?page=1&id={{ID}}");
    }

    #[test]
    fn an_override_is_only_what_was_typed() {
        let mut tab = tab();
        tab.url.set("{{BASE}}/users/{{ID}}");
        tab.env_vars.insert("BASE".into(), "https://env".into());
        let extracted: Vars = [("ID".to_string(), "42".to_string())].into();

        let scope = tab.scope(&extracted);
        assert_eq!(scope.resolve(tab.url.text()), "https://env/users/42");
        assert!(
            tab.overrides.is_empty(),
            "nothing is pinned by looking at it"
        );

        tab.overrides.insert("ID".into(), "7".into());
        assert_eq!(
            tab.scope(&extracted).resolve(tab.url.text()),
            "https://env/users/7"
        );
    }

    #[test]
    fn methods_cycle_both_ways() {
        let mut tab = tab();
        tab.cycle_method(-1);
        assert_eq!(tab.method, "OPTIONS");
        tab.cycle_method(1);
        assert_eq!(tab.method, "GET");
    }
}
