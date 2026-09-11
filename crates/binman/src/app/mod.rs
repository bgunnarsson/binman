pub mod keys;
pub mod kv;
pub mod line;
pub mod mouse;
pub mod overlay;
pub mod tab;
pub mod tree;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use binman_core::body::{self, BodyKind};
use binman_core::extract::{self, Rule};
use binman_core::formats::{bru, curl};
use binman_core::history::{self, History};
use binman_core::oauth2::Grant;
use binman_core::request::{header, set_header};
use binman_core::workspace::{self, Scope, Source};
use binman_core::{
    AuthKind, Client, Collection, EnvSource, Error, Format, Origin, Prepared, Request, Vars,
    Workspace, auth, collection, env, vars,
};
use ratatui::style::Style;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;
use tui_textarea::TextArea;

use crate::{pretty, theme};
use line::LineInput;
use overlay::{
    Action, CollectionForm, Command, Entry, EnvEditor, Overlay, Picker, PickerKind, SavePrompt,
    shown,
};
use tab::{Outcome, Received, Section, Tab};
use tree::Tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Collections,
    Url,
    Request,
    Response,
}

impl Pane {
    /// The order Tab walks them in.
    pub const ORDER: [Pane; 4] = [Pane::Collections, Pane::Url, Pane::Request, Pane::Response];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Warning,
    Error,
}

pub struct Status {
    pub text: String,
    pub tone: Tone,
    pub at: Instant,
}

impl Status {
    fn new(text: impl Into<String>, tone: Tone) -> Status {
        Status {
            text: text.into(),
            tone,
            at: Instant::now(),
        }
    }

    /// Transient messages fade, so the bar goes back to showing context.
    pub fn is_stale(&self) -> bool {
        self.tone == Tone::Info && self.at.elapsed() > Duration::from_secs(6)
    }
}

/// Work finished off the UI thread. Each names the tab and the send it
/// belongs to, so a reply that arrives after the user has moved on is dropped
/// rather than applied to the wrong request.
pub enum Message {
    /// A piece of an event stream, as it arrived.
    Streamed {
        tab: u64,
        generation: u64,
        text: String,
    },
    Finished {
        tab: u64,
        generation: u64,
        /// The core's own error rather than a string: a cancelled request is
        /// a different thing on screen from a failed one.
        result: Result<Box<Received>, Error>,
    },
}

/// The environment last picked, carried to every request opened after it, so
/// a session can be spent against staging without choosing it each time.
enum EnvChoice {
    /// Nothing picked yet: the first environment found, as v1 did.
    First,
    Nothing,
    Label(String),
}

/// Said wherever a request needs a collection to go in and there is none.
const NO_COLLECTION: &str = "There is no collection to save it in — a in the collections adds one";

/// Said when e or d is pressed on a row that is not a collection's own.
const ON_A_COLLECTION: &str =
    "e and d work on a collection — select its row at the top of the tree";

pub struct App {
    /// Every collection in play, from every file that lists one.
    pub workspace: Workspace,
    pub client: Arc<Client>,
    pub history: History,
    pub tree: Tree,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub focus: Pane,
    pub overlay: Option<Overlay>,
    pub status: Status,
    /// Values pulled out of responses. Shared by every tab: a token extracted
    /// by a login in one is what the next request in any other sends.
    pub extracted: Vars,
    /// Text for the terminal's clipboard. The loop that owns the terminal
    /// writes it, since nothing else can.
    pub clipboard: Option<String>,
    /// What the last draw put where, for a click to be looked up in.
    pub targets: mouse::Targets,
    /// The sidebar width and request height someone has dragged to, if they
    /// have. Held raw and fitted at draw time, where the terminal's size is
    /// known — so a window resize re-fits them instead of stranding them.
    pub explorer_width: Option<u16>,
    pub request_height: Option<u16>,
    /// The seam being dragged, while one is.
    pub dragging: Option<mouse::Divider>,
    pub should_quit: bool,
    env_choice: EnvChoice,
    next_tab_id: u64,
    tx: UnboundedSender<Message>,
}

impl App {
    pub fn new(
        workspace: Workspace,
        client: Client,
        history: History,
    ) -> (App, UnboundedReceiver<Message>) {
        let (tx, rx) = unbounded_channel();
        let mut app = App {
            tree: Tree::new(workspace.collections()),
            workspace,
            client: Arc::new(client),
            history,
            tabs: Vec::new(),
            active: 0,
            focus: Pane::Collections,
            overlay: None,
            status: Status::new("⌃K for commands, F1 for help", Tone::Info),
            extracted: Vars::new(),
            clipboard: None,
            targets: mouse::Targets::default(),
            explorer_width: None,
            request_height: None,
            dragging: None,
            should_quit: false,
            env_choice: EnvChoice::First,
            next_tab_id: 0,
            tx,
        };
        app.push_tab();
        (app, rx)
    }

    /// Shows the greeting. Called when the app launches rather than when its
    /// state is built, so a test drives the layout without dismissing a
    /// splash first.
    pub fn show_splash(&mut self) {
        self.overlay = Some(Overlay::Splash);
    }

    // --- status ---

    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Status::new(text, Tone::Info);
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.status = Status::new(text, Tone::Success);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.status = Status::new(text, Tone::Warning);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.status = Status::new(text, Tone::Error);
    }

    // --- tabs ---

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    fn push_tab(&mut self) {
        self.next_tab_id += 1;
        let envs = match self.home() {
            Some(home) => env::discover(&home.path, &home.path),
            None => Vec::new(),
        };
        let env = self.choose_env(&envs);
        let mut tab = Tab::blank(self.next_tab_id, envs, env);
        let loaded = tab.reload_env();
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        if let Err(error) = loaded {
            self.warn(error);
        }
    }

    pub fn new_tab(&mut self) {
        self.push_tab();
        self.focus = Pane::Url;
    }

    pub fn close_tab(&mut self) {
        if self.tabs.len() <= 1 {
            self.warn("The last tab stays open");
            return;
        }
        let mut closed = self.tabs.remove(self.active);
        closed.cancel();
        self.active = self.active.min(self.tabs.len() - 1);
    }

    pub fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    pub fn cycle_tab(&mut self, delta: isize) {
        let count = self.tabs.len() as isize;
        self.active = (self.active as isize + delta).rem_euclid(count) as usize;
    }

    /// A tab to put a request in: this one, unless it holds edits or a
    /// request in flight. Those get a new tab, so nothing typed is thrown away.
    fn claim_tab(&mut self) {
        if self.tab().is_dirty() || self.tab().is_sending() {
            self.push_tab();
        }
    }

    fn choose_env(&self, envs: &[EnvSource]) -> Option<usize> {
        let first = (!envs.is_empty()).then_some(0);
        match &self.env_choice {
            EnvChoice::First => first,
            EnvChoice::Nothing => None,
            EnvChoice::Label(label) => envs
                .iter()
                .position(|source| &source.label == label)
                .or(first),
        }
    }

    /// Whether Tab belongs to what is being typed rather than to pane
    /// movement: it indents a body, and moves from a header's name to its
    /// value.
    pub fn wants_tab(&self) -> bool {
        self.focus == Pane::Request && self.tab().is_typing()
    }

    /// Something is in flight, so the elapsed time on screen has to move.
    pub fn is_busy(&self) -> bool {
        self.tabs.iter().any(Tab::is_sending)
    }

    /// The name a request goes by: its path under the collections, and the
    /// request inside the file when the file holds more than one.
    pub fn describe(&self, tab: &Tab) -> String {
        match &tab.origin {
            None => tab.title.clone(),
            Some(origin @ Origin::File(_)) => self.workspace.display(origin.path()),
            Some(origin) => format!("{} › {}", self.workspace.display(origin.path()), tab.title),
        }
    }

    // --- collections ---

    /// The collection being worked in: the one the selected row sits in, or
    /// else the first that is a directory. A new request is saved there, and
    /// a new tab starts with its environments.
    fn home(&self) -> Option<&Collection> {
        let selected = self
            .tree
            .selected_collection()
            .and_then(|name| self.workspace.get(name));
        selected
            .filter(|collection| collection.is_dir())
            .or_else(|| {
                self.workspace
                    .collections()
                    .iter()
                    .find(|collection| collection.is_dir())
            })
    }

    /// Where a relative path is taken from when no request file says
    /// otherwise.
    fn base(&self) -> PathBuf {
        match self.home() {
            Some(home) => home.path.clone(),
            None => std::env::current_dir().unwrap_or_default(),
        }
    }

    /// Enter on a node: open the request, or open the folder.
    pub fn activate_selected(&mut self) {
        let Some(node) = self.tree.selected() else {
            return;
        };
        match node.origin() {
            Some(origin) => self.open(origin),
            None => {
                let id = node.id;
                self.tree.toggle(id);
            }
        }
    }

    pub fn toggle_selected(&mut self) {
        if let Some(id) = self.tree.selected_id() {
            self.tree.toggle(id);
        }
    }

    /// Reads the lists of collections again as well as the files, so a
    /// `.binman.json` that a pull has changed arrives with what it lists.
    pub fn reload_tree(&mut self) {
        if let Err(error) = self.workspace.reload() {
            self.error(error.to_string());
            return;
        }
        self.tree.sync(self.workspace.collections());
        match self.tree.reload_selected() {
            Some(what) => self.info(format!("Reloaded {what}")),
            None => self.info("Reloaded the collections"),
        }
    }

    // --- registering collections ---

    /// The form for a new collection, starting at the directory binman was
    /// started in: usually the repository whose requests are about to be
    /// added.
    pub fn open_add_collection(&mut self) {
        let here = std::env::current_dir().unwrap_or_default();
        self.overlay = Some(Overlay::Collection(CollectionForm::new(
            &self.workspace,
            &here,
        )));
    }

    /// The form over the collection whose row is selected.
    pub fn edit_collection(&mut self) {
        let Some(collection) = self.selected_root().cloned() else {
            self.warn(ON_A_COLLECTION);
            return;
        };
        self.overlay = Some(Overlay::Collection(CollectionForm::editing(
            &self.workspace,
            &collection,
        )));
    }

    fn selected_root(&self) -> Option<&Collection> {
        self.tree
            .selected_root()
            .and_then(|name| self.workspace.get(name))
    }

    /// Writes what the form holds into the file its Saved in names, and
    /// opens the collection in the tree.
    pub fn save_collection(&mut self) {
        let Some(Overlay::Collection(form)) = &self.overlay else {
            return;
        };
        let typed = form.path.text().trim().to_string();
        let path = form.typed_path();
        let named = form.name.text().trim().to_string();
        let editing = form.editing.clone();
        let (scope, file) = (form.scope, form.scope_display());

        match self.register(&typed, &path, &named, editing.as_deref(), scope) {
            Ok(name) => {
                self.overlay = None;
                self.tree.sync(self.workspace.collections());
                self.tree.select_root(&name);
                self.focus = Pane::Collections;
                if editing.is_some() {
                    self.success(format!("Saved {name} in {file}"));
                } else {
                    self.success(format!("Added {name} to {file}"));
                }
            }
            Err(message) => {
                if let Some(Overlay::Collection(form)) = &mut self.overlay {
                    form.error = Some(message);
                }
            }
        }
    }

    /// Checks what the form holds and saves it, answering with the name it
    /// went under. Left empty, that is what the path points at.
    fn register(
        &mut self,
        typed: &str,
        path: &Path,
        named: &str,
        editing: Option<&str>,
        scope: Scope,
    ) -> Result<String, String> {
        if typed.is_empty() {
            return Err("Type where the collection is".into());
        }
        let path = path
            .canonicalize()
            .map_err(|_| format!("{} is not there", path.display()))?;
        if !path.is_dir() && collection::entry(&path).is_none() {
            return Err(format!(
                "binman can't open {} — a collection is a directory of requests, a Postman collection or an OpenAPI spec",
                path.display()
            ));
        }
        let name = if named.is_empty() {
            workspace::name_for(&path)
        } else {
            named.to_string()
        };
        if Some(name.as_str()) != editing && self.workspace.get(&name).is_some() {
            return Err(format!(
                "There is already a collection called {name} — give this one another name"
            ));
        }
        self.workspace
            .set(editing, &name, &path, scope)
            .map_err(|error| error.to_string())?;
        Ok(name)
    }

    /// Takes the selected collection out of the list that keeps it. Its
    /// requests stay where they are.
    pub fn remove_collection(&mut self) {
        let Some(collection) = self.selected_root().cloned() else {
            self.warn(ON_A_COLLECTION);
            return;
        };
        let from = match collection.source {
            Source::Project => self.workspace.project_path().map(shown),
            Source::User => Some(shown(self.workspace.user_path())),
            Source::Config | Source::Argument => None,
        };
        if let Err(error) = self.workspace.remove(&collection.name) {
            self.error(error.to_string());
            return;
        }
        self.tree.sync(self.workspace.collections());
        let name = &collection.name;
        match from {
            Some(file) => self.success(format!(
                "Removed {name} from {file} — its requests are still on disk"
            )),
            None => self.info(format!("Closed {name}, which was open for this run only")),
        }
    }

    /// Opens a request, in the tab that already has it if one does.
    pub fn open(&mut self, origin: Origin) {
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.origin.as_ref() == Some(&origin))
        {
            self.active = index;
            return;
        }
        let root = self.workspace.root_for(origin.path());
        let loaded = match origin.load(&root) {
            Ok(loaded) => loaded,
            Err(error) => {
                self.error(error.to_string());
                return;
            }
        };

        self.claim_tab();
        let envs = env::discover(origin.dir(), &root);
        let env = self.choose_env(&envs);
        let title = loaded.title.clone();
        let tab = self.tab_mut();
        tab.load(loaded, Some(origin), envs, env);
        match tab.reload_env() {
            Ok(()) => self.info(format!("Opened {title}")),
            Err(error) => self.warn(error),
        }
    }

    // --- sending ---

    pub fn send(&mut self) {
        let index = self.active;
        self.tabs[index].commit_edits();
        if let Err(error) = self.tabs[index].reload_env() {
            self.warn(error);
        }

        let base = self.base();
        let (prepared, grant) = match prepare(&self.tabs[index], &self.extracted, &base) {
            Ok(prepared) => prepared,
            Err(message) => {
                self.error(message);
                return;
            }
        };
        let rules = self.tabs[index].rules();
        let announce = format!(
            "Sending {} to {}… ⌃C cancels",
            prepared.method,
            host_of(&prepared.url)
        );

        let tab = &mut self.tabs[index];
        // One request at a time per tab: a second send calls off the first
        // rather than racing it.
        tab.cancel();
        tab.generation += 1;
        let generation = tab.generation;
        let id = tab.id;
        let cancel = CancellationToken::new();
        tab.cancel = Some(cancel.clone());
        tab.response.outcome = Outcome::Sending {
            started: Instant::now(),
            streamed: String::new(),
        };
        tab.response.scroll = 0;
        tab.response.follow = true;
        self.info(announce);

        let client = self.client.clone();
        let history = self.history.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            let attempted = prepared.clone();
            let stream = tx.clone();
            let result = exchange(&client, prepared, grant, &cancel, &rules, move |text| {
                let _ = stream.send(Message::Streamed {
                    tab: id,
                    generation,
                    text,
                });
            })
            .await;

            match &result {
                Ok(received) => record(
                    &history,
                    &received.sent,
                    received.exchange.status,
                    received.exchange.trace.total,
                ),
                Err(Error::Cancelled) => {}
                Err(_) => record(&history, &attempted, 0, started.elapsed()),
            }
            let _ = tx.send(Message::Finished {
                tab: id,
                generation,
                result,
            });
        });
    }

    /// Calls off the active tab's request. The tab is free again at once;
    /// the connection is dropped, which is all a server can be told.
    pub fn cancel_request(&mut self) {
        if self.tab_mut().cancel() {
            self.info("Cancelling…");
        }
    }

    pub fn handle(&mut self, message: Message) {
        match message {
            Message::Streamed {
                tab,
                generation,
                text,
            } => {
                if let Some(tab) = self
                    .tabs
                    .iter_mut()
                    .find(|candidate| candidate.id == tab && candidate.generation == generation)
                    && let Outcome::Sending { streamed, .. } = &mut tab.response.outcome
                {
                    streamed.push_str(&text);
                }
            }
            Message::Finished {
                tab,
                generation,
                result,
            } => self.finish(tab, generation, result),
        }
    }

    fn finish(&mut self, id: u64, generation: u64, result: Result<Box<Received>, Error>) {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
            return;
        };
        if tab.generation != generation {
            return;
        }
        tab.cancel = None;

        let status = match result {
            Ok(received) => {
                let status = Status::new(summary(&received), tone_of(received.exchange.status));
                self.extracted.extend(
                    received
                        .extracted
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone())),
                );
                if !received.exchange.streamed {
                    tab.response.scroll = 0;
                    tab.response.follow = false;
                }
                tab.response.outcome = Outcome::Received(received);
                status
            }
            Err(Error::Cancelled) => {
                tab.response.outcome = Outcome::Cancelled;
                Status::new("Request cancelled", Tone::Warning)
            }
            Err(error) => {
                let text = error.to_string();
                tab.response.outcome = Outcome::Failed(text.clone());
                Status::new(text, Tone::Error)
            }
        };
        self.status = status;
    }

    // --- saving ---

    /// ⌃S: the response, from the response pane; the request, anywhere else.
    pub fn save(&mut self) {
        if self.focus == Pane::Response && self.tab().response.received().is_some() {
            self.open_save_response();
        } else {
            self.save_request();
        }
    }

    /// Writes the request back to its file, or asks where to put it when it
    /// has none.
    pub fn save_request(&mut self) {
        self.tab_mut().commit_edits();
        let Some(origin) = self.tab().origin.clone() else {
            self.open_save_request();
            return;
        };
        match self.write_request(&origin) {
            Ok(()) => {
                self.tab_mut().mark_saved();
                let name = self.workspace.display(origin.path());
                if origin.savable() == Some(Format::Http) && self.tab().auth.kind != AuthKind::None
                {
                    self.warn(format!(
                        "Saved {name} — without the auth, which a .http file has no place for"
                    ));
                } else {
                    self.success(format!("Saved {name}"));
                }
            }
            Err(error) => self.error(error.to_string()),
        }
    }

    fn write_request(&self, origin: &Origin) -> Result<(), Error> {
        let tab = self.tab();
        let kind = tab.body_kind;
        let mut request = tab.request();
        // A .http file says what its body is only through Content-Type, so
        // one is written when the body kind implies it, as v1 did.
        if origin.savable() == Some(Format::Http)
            && header(&request.headers, "Content-Type").is_none()
            && let Some(content_type) = kind.content_type()
        {
            request
                .headers
                .push(("Content-Type".into(), content_type.into()));
        }
        origin.save(&request, kind)
    }

    /// Asks where to write a request that came from nowhere on disk: a new
    /// tab, or one sent again from the history.
    pub fn open_save_request(&mut self) {
        let suggested = match self.home() {
            Some(home) => home.path.join("request.http"),
            None => {
                self.warn(NO_COLLECTION);
                return;
            }
        };
        self.overlay = Some(Overlay::SaveRequest(SavePrompt {
            input: LineInput::new(suggested.display().to_string()),
            error: None,
        }));
    }

    /// Writes the request where the prompt says and ties the tab to that
    /// file, so the next ⌃S writes there without asking.
    pub fn save_request_as(&mut self) {
        let Some(Overlay::SaveRequest(prompt)) = &self.overlay else {
            return;
        };
        let written = self.new_file(prompt.input.text().trim()).and_then(|path| {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|error| format!("{}: {error}", dir.display()))?;
            }
            let origin = Origin::File(path);
            self.write_request(&origin)
                .map_err(|error| error.to_string())?;
            Ok(origin)
        });
        let origin = match written {
            Ok(origin) => origin,
            Err(message) => {
                if let Some(Overlay::SaveRequest(prompt)) = &mut self.overlay {
                    prompt.error = Some(message);
                }
                return;
            }
        };

        let root = self.workspace.root_for(origin.path());
        let envs = env::discover(origin.dir(), &root);
        let env = self.choose_env(&envs);
        let collection_vars = if origin.savable() == Some(Format::Bru) {
            bru::collection_vars(origin.dir(), &root)
        } else {
            Vars::new()
        };
        let name = self.workspace.display(origin.path());
        self.tree.reveal(origin.path());

        let tab = self.tab_mut();
        tab.title = origin
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        tab.collection_vars = collection_vars;
        tab.envs = envs;
        tab.env = env;
        tab.origin = Some(origin);
        tab.mark_saved();
        let loaded = tab.reload_env();
        self.overlay = None;
        match loaded {
            Ok(()) => self.success(format!("Saved {name}")),
            Err(error) => self.warn(error),
        }
    }

    /// Where a typed name puts a new request file: under the collection its
    /// first part names, or the one being worked in. A name that is neither
    /// `.http` nor `.bru` gets `.http` added to it, and nothing is written
    /// over a file that is already there.
    fn new_file(&self, typed: &str) -> Result<PathBuf, String> {
        if typed.is_empty() {
            return Err("Type where to save it".into());
        }
        let typed = PathBuf::from(typed);
        let mut path = if typed.is_absolute() {
            typed
        } else {
            self.workspace
                .locate(&typed, self.home())
                .ok_or(NO_COLLECTION)?
        };
        match Format::of(&path) {
            Some(Format::Http | Format::Bru) => {}
            Some(Format::Graphql) => {
                return Err(
                    "binman reads .graphql files but does not write them — end the name in .http or .bru"
                        .into(),
                );
            }
            None => {
                let mut name = path.into_os_string();
                name.push(".http");
                path = PathBuf::from(name);
            }
        }

        // `..` is refused rather than followed: the file is not there yet, so
        // there is nothing to canonicalize and see where it really lands.
        let inside = self
            .workspace
            .collections()
            .iter()
            .any(|collection| collection.is_dir() && path.starts_with(&collection.path));
        if path.components().any(|part| part == Component::ParentDir) || !inside {
            return Err(format!("{} is outside the collections", path.display()));
        }
        let name = self.workspace.display(&path);
        if path
            .file_name()
            .is_some_and(|file| file.to_string_lossy().starts_with('.'))
        {
            return Err(format!(
                "{name} would be hidden — the tree leaves out names that start with a dot"
            ));
        }
        if path.exists() {
            return Err(format!("{name} is already there"));
        }
        Ok(path)
    }

    pub fn open_save_response(&mut self) {
        if self.tab().response.received().is_none() {
            self.warn("There is no response to save yet");
            return;
        }
        let suggested = self.base().join("response.txt");
        self.overlay = Some(Overlay::SaveResponse(SavePrompt {
            input: LineInput::new(suggested.display().to_string()),
            error: None,
        }));
    }

    pub fn save_response(&mut self) {
        let Some(Overlay::SaveResponse(prompt)) = &self.overlay else {
            return;
        };
        let typed = PathBuf::from(prompt.input.text().trim());
        let path = if typed.is_absolute() {
            typed
        } else {
            self.base().join(typed)
        };
        let Some(received) = self.tab().response.received() else {
            self.overlay = None;
            return;
        };

        match std::fs::write(&path, &received.exchange.body) {
            Ok(()) => {
                self.overlay = None;
                self.success(format!("Saved the response to {}", path.display()));
            }
            Err(error) => {
                if let Some(Overlay::SaveResponse(prompt)) = &mut self.overlay {
                    prompt.error = Some(format!("{}: {error}", path.display()));
                }
            }
        }
    }

    // --- curl ---

    /// Shows the request as a curl command and hands it to the clipboard.
    pub fn copy_curl(&mut self) {
        self.tab_mut().commit_edits();
        let base = self.base();
        let tab = self.tab();
        let (prepared, grant) = match prepare(tab, &self.extracted, &base) {
            Ok(prepared) => prepared,
            Err(message) => {
                self.error(message);
                return;
            }
        };

        let scope = tab.scope(&self.extracted);
        let fields: Option<Vec<(String, String)>> =
            (tab.body_kind == BodyKind::Multipart).then(|| {
                tab.form
                    .rows
                    .iter()
                    .filter(|(name, _)| !name.trim().is_empty())
                    .map(|(name, value)| (scope.resolve(name.trim()), scope.resolve(value)))
                    .collect()
            });
        let request = Request {
            method: prepared.method,
            url: prepared.url,
            headers: prepared.headers,
            body: if fields.is_some() {
                String::new()
            } else {
                String::from_utf8_lossy(&prepared.body).into_owned()
            },
            ..Request::default()
        };
        let command = curl::format(&request, fields.as_deref());

        self.clipboard = Some(command.clone());
        self.overlay = Some(Overlay::Curl(command));
        if grant.is_some() {
            self.warn(
                "Sent to the clipboard — without the OAuth2 token, which is fetched when sending",
            );
        } else {
            self.success("Sent to the clipboard");
        }
    }

    /// Replaces the request with the curl command typed into its URL.
    pub fn import_curl(&mut self) {
        let text = self.tab().url.text().to_string();
        match curl::parse(&text) {
            Ok(request) => {
                self.tab_mut().apply(&request);
                self.success("Imported the curl command");
            }
            Err(error) => self.error(format!("Could not read that curl command: {error}")),
        }
    }

    // --- pickers ---

    /// Builds the palette from what can be done right now: Cancel only while
    /// something is running, Save only for a request that can be saved.
    pub fn open_palette(&mut self) {
        let tab = self.tab();
        let mut commands = vec![Command::Send];
        if tab.is_sending() {
            commands.push(Command::Cancel);
        }
        commands.extend([
            Command::NewTab,
            Command::CloseTab,
            Command::Find,
            Command::History,
            Command::Environments,
        ]);
        if tab.env.is_some() {
            commands.push(Command::EditEnv);
        }
        commands.push(Command::CopyCurl);
        match &tab.origin {
            None => commands.push(Command::SaveAs),
            Some(origin) if origin.savable().is_some() => commands.push(Command::Save),
            Some(_) => {}
        }
        if tab.response.received().is_some() {
            commands.push(Command::SaveResponse);
        }
        commands.push(Command::AddCollection);
        if self.tree.selected_root().is_some() {
            commands.extend([Command::EditCollection, Command::RemoveCollection]);
        }
        commands.extend([Command::Reload, Command::Help, Command::Quit]);

        let entries = commands
            .into_iter()
            .map(|command| Entry {
                method: None,
                label: command.label().to_string(),
                detail: String::new(),
                hint: command.hint(),
                action: Action::Run(command),
            })
            .collect();
        self.overlay = Some(Overlay::Picker(Picker::new(
            PickerKind::Commands,
            entries,
            0,
        )));
    }

    pub fn open_find(&mut self) {
        // Where a request sits starts with which collection, once there is
        // more than one it could be in.
        let qualified = self.workspace.collections().len() > 1;
        let mut found = Vec::new();
        for registered in self.workspace.collections() {
            for mut request in collection::index(&registered.path) {
                if qualified {
                    request.location = if request.location.is_empty() {
                        registered.name.clone()
                    } else {
                        format!("{}/{}", registered.name, request.location)
                    };
                }
                found.push(request);
            }
        }
        if found.is_empty() {
            self.warn("No requests in any collection");
            return;
        }
        let entries = found
            .into_iter()
            .map(|found| Entry {
                method: (!found.method.is_empty()).then_some(found.method),
                label: found.title,
                detail: found.location,
                hint: "",
                action: Action::Open(found.origin),
            })
            .collect();
        self.overlay = Some(Overlay::Picker(Picker::new(PickerKind::Find, entries, 0)));
    }

    pub fn open_history(&mut self) {
        let entries = match self.history.load(50) {
            Ok(entries) => entries,
            Err(error) => {
                self.error(format!("Reading the history: {error}"));
                return;
            }
        };
        if entries.is_empty() {
            self.warn("Nothing has been sent yet");
            return;
        }
        let entries = entries
            .into_iter()
            .rev()
            .map(|entry| {
                let status = if entry.status == 0 {
                    "failed".to_string()
                } else {
                    entry.status.to_string()
                };
                Entry {
                    method: Some(entry.method.clone()),
                    label: entry.url.clone(),
                    detail: format!("{}  {status}", entry.timestamp.format("%H:%M:%S")),
                    hint: "",
                    action: Action::Replay(Box::new(entry)),
                }
            })
            .collect();
        self.overlay = Some(Overlay::Picker(Picker::new(
            PickerKind::History,
            entries,
            0,
        )));
    }

    pub fn open_envs(&mut self) {
        let tab = self.tab();
        if tab.envs.is_empty() {
            self.warn("No environments above this request — a .env file beside it, or at the collection root, would be one");
            return;
        }
        let mut entries = vec![Entry {
            method: None,
            label: "No environment".into(),
            detail: String::new(),
            hint: "",
            action: Action::UseEnv(None),
        }];
        entries.extend(tab.envs.iter().enumerate().map(|(index, source)| Entry {
            method: None,
            label: source.label.clone(),
            detail: format!(
                "{} · {}",
                source.kind.label(),
                self.workspace.display(&source.path)
            ),
            hint: "",
            action: Action::UseEnv(Some(index)),
        }));
        let selected = tab.env.map_or(0, |index| index + 1);
        self.overlay = Some(Overlay::Picker(Picker::new(
            PickerKind::Environments,
            entries,
            selected,
        )));
    }

    pub fn choose(&mut self, action: Action) {
        match action {
            Action::Run(command) => self.execute(command),
            Action::Open(origin) => self.open(origin),
            Action::Replay(entry) => self.replay(*entry),
            Action::UseEnv(index) => self.use_env(index),
        }
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            Command::Send => self.send(),
            Command::Cancel => self.cancel_request(),
            Command::NewTab => self.new_tab(),
            Command::CloseTab => self.close_tab(),
            Command::Find => self.open_find(),
            Command::History => self.open_history(),
            Command::Environments => self.open_envs(),
            Command::EditEnv => self.edit_env(None),
            Command::CopyCurl => self.copy_curl(),
            Command::Save | Command::SaveAs => self.save_request(),
            Command::SaveResponse => self.open_save_response(),
            Command::AddCollection => self.open_add_collection(),
            Command::EditCollection => self.edit_collection(),
            Command::RemoveCollection => self.remove_collection(),
            Command::Reload => self.reload_tree(),
            Command::Help => self.overlay = Some(Overlay::Help),
            Command::Quit => self.should_quit = true,
        }
    }

    pub fn use_env(&mut self, index: Option<usize>) {
        let tab = self.tab_mut();
        tab.env = index;
        let label = tab.env_source().map(|source| source.label.clone());
        let loaded = tab.reload_env();
        self.env_choice = match &label {
            Some(label) => EnvChoice::Label(label.clone()),
            None => EnvChoice::Nothing,
        };
        match (loaded, label) {
            (Err(error), _) => self.error(error),
            (Ok(()), Some(label)) => self.success(format!("Using {label}")),
            (Ok(()), None) => self.info("No environment"),
        }
    }

    /// Opens an environment file to edit: the one given, or the one in use.
    pub fn edit_env(&mut self, index: Option<usize>) {
        let tab = self.tab();
        let Some(source) = index
            .or(tab.env)
            .and_then(|index| tab.envs.get(index))
            .cloned()
        else {
            self.warn("No environment is selected — ⌃E picks one");
            return;
        };
        let text = match std::fs::read_to_string(&source.path) {
            Ok(text) => text,
            Err(error) => {
                self.error(format!("Reading {}: {error}", source.path.display()));
                return;
            }
        };
        let mut editor = TextArea::from(text.lines());
        editor.set_cursor_line_style(Style::default());
        editor.set_cursor_style(theme::cursor());
        editor.set_selection_style(theme::selection(true));
        let label = source
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| source.label.clone());
        self.overlay = Some(Overlay::Env(Box::new(EnvEditor {
            path: source.path,
            label,
            editor,
        })));
    }

    pub fn save_env(&mut self) {
        let Some(Overlay::Env(editor)) = &self.overlay else {
            return;
        };
        let mut text = editor.editor.lines().join("\n");
        text.push('\n');
        let path = editor.path.clone();
        let label = editor.label.clone();

        if let Err(error) = std::fs::write(&path, text) {
            self.error(format!("Saving {label}: {error}"));
            return;
        }
        self.overlay = None;
        match self.tab_mut().reload_env() {
            Ok(()) => self.success(format!("Saved {label}")),
            Err(error) => self.warn(error),
        }
    }

    /// Sends a request from the history again, exactly as it went the first
    /// time.
    pub fn replay(&mut self, entry: history::Entry) {
        self.claim_tab();
        let request = Request {
            method: entry.method,
            url: entry.url,
            headers: entry.headers.into_iter().collect(),
            body: entry.body,
            ..Request::default()
        };
        let tab = self.tab_mut();
        tab.apply(&request);
        tab.origin = None;
        tab.title = "from history".into();
        tab.mark_saved();
        self.send();
    }

    /// Text pasted into the terminal, handed to whatever is taking typing.
    pub fn paste(&mut self, text: &str) {
        match &mut self.overlay {
            Some(Overlay::Picker(picker)) if picker.kind.filters() => {
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    picker.push(ch);
                }
            }
            Some(Overlay::SaveRequest(prompt) | Overlay::SaveResponse(prompt)) => {
                prompt.input.paste(text);
            }
            Some(Overlay::Env(editor)) => {
                editor.editor.insert_str(text);
            }
            Some(Overlay::Collection(form)) => {
                if let Some(input) = form.input() {
                    input.paste(text);
                }
            }
            Some(_) => {}
            None => match self.focus {
                Pane::Url => {
                    let tab = self.tab_mut();
                    tab.url.paste(text);
                    tab.sync_params();
                }
                Pane::Request => self.tab_mut().paste(text),
                Pane::Collections | Pane::Response => {}
            },
        }
    }
}

/// The request as it will go on the wire: every variable resolved, the body
/// encoded, the auth header added. Refuses, sending nothing, when the URL still
/// names a variable nothing defines — it could only go somewhere wrong.
///
/// `base` is where a file to upload is looked for when the request has no
/// file of its own to be beside.
pub(crate) fn prepare(
    tab: &Tab,
    extracted: &Vars,
    base: &Path,
) -> Result<(Prepared, Option<Grant>), String> {
    let scope = tab.scope(extracted);
    let url = scope.resolve(tab.url.text().trim());
    if url.is_empty() {
        return Err("Nothing to send — type a URL first".into());
    }
    let missing = vars::scan(&url);
    if !missing.is_empty() {
        let verb = if missing.len() == 1 { "is" } else { "are" };
        return Err(format!(
            "{} {verb} not set — ⌃E picks an environment, or give it a value under Vars",
            missing.join(", ")
        ));
    }

    let resolve_rows = |rows: &[(String, String)]| -> Vec<(String, String)> {
        rows.iter()
            .filter(|(name, _)| !name.trim().is_empty())
            .map(|(name, value)| (scope.resolve(name.trim()), scope.resolve(value)))
            .collect()
    };
    let mut headers = resolve_rows(&tab.headers.rows);

    let body = match tab.body_kind {
        BodyKind::None => Vec::new(),
        BodyKind::Form => body::encode_form(&resolve_rows(&tab.form.rows)).into_bytes(),
        BodyKind::Multipart => {
            let dir = tab.origin.as_ref().map_or(base, Origin::dir);
            let encoded = body::multipart(&resolve_rows(&tab.form.rows), Some(dir))
                .map_err(|error| error.to_string())?;
            set_header(&mut headers, "Content-Type", encoded.content_type);
            encoded.body
        }
        _ => scope.resolve(&tab.body_text()).into_bytes(),
    };
    if tab.body_kind != BodyKind::Multipart
        && header(&headers, "Content-Type").is_none()
        && let Some(content_type) = tab.body_kind.content_type()
    {
        headers.push(("Content-Type".into(), content_type.into()));
    }

    let values: Vec<String> = tab
        .auth
        .values()
        .iter()
        .map(|value| scope.resolve(value))
        .collect();
    let grant = match tab.auth.kind {
        AuthKind::ClientCredentials => Some(Grant {
            token_url: values[0].clone(),
            client_id: values[1].clone(),
            client_secret: values[2].clone(),
            scope: values[3].clone(),
        }),
        kind => {
            if let Some((name, value)) = auth::header(kind, &values) {
                set_header(&mut headers, &name, value);
            }
            None
        }
    };

    Ok((
        Prepared {
            method: tab.method.clone(),
            url,
            headers,
            body,
        },
        grant,
    ))
}

pub(crate) async fn exchange(
    client: &Client,
    mut prepared: Prepared,
    grant: Option<Grant>,
    cancel: &CancellationToken,
    rules: &[Rule],
    on_event: impl FnMut(String),
) -> Result<Box<Received>, Error> {
    if let Some(grant) = grant {
        let token = client.client_credentials(&grant, cancel).await?;
        set_header(
            &mut prepared.headers,
            "Authorization",
            format!("Bearer {token}"),
        );
    }
    let exchange = client.send(&prepared, cancel, on_event).await?;
    let text = exchange.text();
    let extracted = extract::apply(rules, &text, &exchange.headers);
    let lines = pretty::render(&text);
    Ok(Box::new(Received {
        exchange,
        extracted,
        lines,
        sent: prepared,
    }))
}

/// Best effort, as in v1: a history that cannot be written must not cost the
/// response.
pub(crate) fn record(history: &History, sent: &Prepared, status: u16, duration: Duration) {
    let _ = history.append(&history::Entry::new(
        &sent.method,
        &sent.url,
        &sent.headers,
        &String::from_utf8_lossy(&sent.body),
        status,
        duration,
    ));
}

/// The scheme and host a URL goes to.
pub fn host_of(url: &str) -> String {
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (Some(scheme), rest),
        None => (None, url),
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    match scheme {
        Some(scheme) => format!("{scheme}://{host}"),
        None => host.to_string(),
    }
}

fn summary(received: &Received) -> String {
    let exchange = &received.exchange;
    let mut text = exchange.status.to_string();
    if !exchange.reason.is_empty() {
        text.push(' ');
        text.push_str(&exchange.reason);
    }
    text.push_str(&format!(
        " in {} · {}",
        format_elapsed(exchange.trace.total),
        size(exchange.body.len())
    ));
    if !received.extracted.is_empty() {
        let names: Vec<&str> = received.extracted.keys().map(String::as_str).collect();
        text.push_str(&format!(" · set {}", names.join(", ")));
    }
    text
}

fn tone_of(status: u16) -> Tone {
    match status {
        200..=299 => Tone::Success,
        100..=199 | 300..=399 => Tone::Info,
        400..=499 => Tone::Warning,
        _ => Tone::Error,
    }
}

pub fn format_elapsed(elapsed: Duration) -> String {
    let millis = elapsed.as_secs_f64() * 1000.0;
    if millis < 1.0 {
        format!("{millis:.2}ms")
    } else if millis < 1000.0 {
        format!("{millis:.0}ms")
    } else {
        format!("{:.2}s", elapsed.as_secs_f64())
    }
}

pub fn size(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f < KIB {
        format!("{bytes} B")
    } else if bytes_f < KIB * KIB {
        format!("{:.1} KB", bytes_f / KIB)
    } else {
        format!("{:.1} MB", bytes_f / (KIB * KIB))
    }
}

impl Tab {
    /// Pasted text for whatever in this tab is taking typing.
    pub fn paste(&mut self, text: &str) {
        if self.editing {
            let editor = match self.section {
                Section::Scripts => &mut self.scripts,
                _ => &mut self.body,
            };
            editor.insert_str(text);
            return;
        }
        let input = match self.section {
            Section::Params => self.params.input(),
            Section::Headers => self.headers.input(),
            Section::Body => self.form.input(),
            Section::Auth => self.auth.edit.as_mut(),
            Section::Vars => self.vars.edit.as_mut().map(|(_, input)| input),
            Section::Scripts | Section::Info => None,
        };
        if let Some(input) = input {
            input.paste(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_what_a_url_reaches() {
        assert_eq!(
            host_of("https://api.example.com/v1/users?x=1"),
            "https://api.example.com"
        );
        assert_eq!(host_of("http://localhost:8080"), "http://localhost:8080");
        assert_eq!(host_of("{{BASE}}/users"), "{{BASE}}");
    }

    #[test]
    fn sizes_read_as_people_say_them() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(3277), "3.2 KB");
        assert_eq!(size(3 * 1024 * 1024), "3.0 MB");
    }
}
