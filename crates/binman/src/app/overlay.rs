//! The things that draw on top of the layout: the greeting, help, the pickers,
//! the environment editor, the prompts for where to save a request or a
//! response, and the collection form.

use std::path::{Path, PathBuf};

use binman_core::history;
use binman_core::workspace::{self, Collection, Scope};
use binman_core::{Origin, Workspace};
use tui_textarea::TextArea;

use super::line::LineInput;

pub enum Overlay {
    /// Shown once at startup. Any key dismisses it.
    Splash,
    Help,
    Picker(Picker),
    /// Boxed: an editor is far larger than anything else here.
    Env(Box<EnvEditor>),
    /// Where to write a request that has no file yet.
    SaveRequest(SavePrompt),
    SaveResponse(SavePrompt),
    /// The request as a curl command, to read or select.
    Curl(String),
    /// Adding a collection, or changing one.
    Collection(CollectionForm),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Send,
    Cancel,
    NewTab,
    CloseTab,
    Find,
    History,
    Environments,
    EditEnv,
    CopyCurl,
    Save,
    SaveAs,
    SaveResponse,
    AddCollection,
    EditCollection,
    RemoveCollection,
    Reload,
    Help,
    Quit,
}

impl Command {
    pub fn label(self) -> &'static str {
        match self {
            Command::Send => "Send the request",
            Command::Cancel => "Cancel the request in flight",
            Command::NewTab => "New tab",
            Command::CloseTab => "Close tab",
            Command::Find => "Find a request…",
            Command::History => "History…",
            Command::Environments => "Environments…",
            Command::EditEnv => "Edit the environment file…",
            Command::CopyCurl => "Copy as cURL",
            Command::Save => "Save the request",
            Command::SaveAs => "Save the request to a file…",
            Command::SaveResponse => "Save the response…",
            Command::AddCollection => "Add a collection…",
            Command::EditCollection => "Edit this collection…",
            Command::RemoveCollection => "Remove this collection",
            Command::Reload => "Reload the collections",
            Command::Help => "Help",
            Command::Quit => "Quit",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Command::Send => "⌃R",
            Command::Cancel => "⌃C",
            Command::NewTab => "⌃T",
            Command::CloseTab => "⌃W",
            Command::Find => "⌃F",
            Command::History => "⌃H",
            Command::Environments => "⌃E",
            Command::CopyCurl => "⌃Y",
            Command::Save | Command::SaveAs => "⌃S",
            Command::Reload => "F5",
            Command::Help => "F1",
            Command::Quit => "⌃Q",
            // a, e and d are the tree's own keys, and do nothing anywhere else.
            Command::EditEnv
            | Command::SaveResponse
            | Command::AddCollection
            | Command::EditCollection
            | Command::RemoveCollection => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Commands,
    Find,
    History,
    Environments,
}

impl PickerKind {
    pub fn title(self) -> &'static str {
        match self {
            PickerKind::Commands => "Commands",
            PickerKind::Find => "Find a request",
            PickerKind::History => "History",
            PickerKind::Environments => "Environments",
        }
    }

    /// Whether typing narrows the list. The environments are few enough to
    /// read, which leaves their letters free to be commands.
    pub fn filters(self) -> bool {
        self != PickerKind::Environments
    }
}

pub enum Action {
    Run(Command),
    Open(Origin),
    Replay(Box<history::Entry>),
    UseEnv(Option<usize>),
}

pub struct Entry {
    pub method: Option<String>,
    pub label: String,
    pub detail: String,
    pub hint: &'static str,
    pub action: Action,
}

pub struct Picker {
    pub kind: PickerKind,
    pub query: String,
    pub selected: usize,
    pub entries: Vec<Entry>,
}

impl Picker {
    pub fn new(kind: PickerKind, entries: Vec<Entry>, selected: usize) -> Picker {
        Picker {
            kind,
            query: String::new(),
            selected: selected.min(entries.len().saturating_sub(1)),
            entries,
        }
    }

    /// The entries the query leaves, by index. Case-insensitive subsequence,
    /// so "lsusr" finds "list users".
    pub fn matches(&self) -> Vec<usize> {
        let needle = self.query.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                needle.is_empty()
                    || is_subsequence(
                        &needle,
                        &format!(
                            "{} {} {}",
                            entry.method.as_deref().unwrap_or(""),
                            entry.label,
                            entry.detail
                        )
                        .to_lowercase(),
                    )
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The entry under the selection.
    pub fn chosen(&self) -> Option<usize> {
        self.matches().get(self.selected).copied()
    }

    pub fn into_action(mut self) -> Option<Action> {
        let index = self.chosen()?;
        Some(self.entries.swap_remove(index).action)
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.matches().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(count as isize) as usize;
    }

    pub fn push(&mut self, ch: char) {
        self.query.push(ch);
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle
        .chars()
        .all(|wanted| chars.any(|candidate| candidate == wanted))
}

/// An environment file, open to edit in place.
pub struct EnvEditor {
    pub path: PathBuf,
    pub label: String,
    pub editor: TextArea<'static>,
}

pub struct SavePrompt {
    pub input: LineInput,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Path,
    Name,
    /// Which file it is saved in. Only offered when there is a project to
    /// save in — with one file there is nothing to choose.
    Scope,
}

impl FormField {
    pub fn label(self) -> &'static str {
        match self {
            FormField::Path => "Path",
            FormField::Name => "Name",
            FormField::Scope => "Saved in",
        }
    }
}

/// Registering a collection, or changing one: binsql's connection form, for a
/// location rather than a database.
pub struct CollectionForm {
    pub path: LineInput,
    /// Left empty, the collection is named after what the path points at.
    pub name: LineInput,
    pub scope: Scope,
    pub field: FormField,
    /// The name it is saved under, when it is being edited.
    pub editing: Option<String>,
    /// The project file as it is worth showing, when there is one to save
    /// in. `None` leaves Saved in out of the form.
    project: Option<String>,
    user: String,
    pub error: Option<String>,
}

impl CollectionForm {
    /// A new collection, starting at `here` — usually the repository whose
    /// requests are about to be added — and saved, unless you say otherwise,
    /// in the project being worked in.
    pub fn new(workspace: &Workspace, here: &Path) -> CollectionForm {
        CollectionForm::build(
            workspace,
            LineInput::new(workspace::tilde(here)),
            LineInput::new(""),
            workspace.default_scope(None),
            None,
        )
    }

    /// An existing collection, saved back where it came from unless Saved in
    /// is changed — which moves it.
    pub fn editing(workspace: &Workspace, collection: &Collection) -> CollectionForm {
        CollectionForm::build(
            workspace,
            LineInput::new(workspace::tilde(&collection.path)),
            LineInput::new(collection.name.clone()),
            workspace.default_scope(Some(&collection.name)),
            Some(collection.name.clone()),
        )
    }

    fn build(
        workspace: &Workspace,
        path: LineInput,
        name: LineInput,
        scope: Scope,
        editing: Option<String>,
    ) -> CollectionForm {
        // One to be written says so, since saving here is what creates it.
        let project = workspace.project_path().map(|path| {
            if workspace.project_exists() {
                shown(path)
            } else {
                format!("{} (new)", shown(path))
            }
        });
        CollectionForm {
            path,
            name,
            scope,
            field: FormField::Path,
            editing,
            project,
            user: shown(workspace.user_path()),
            error: None,
        }
    }

    /// The fields the form shows, in order.
    pub fn fields(&self) -> Vec<FormField> {
        let mut fields = vec![FormField::Path, FormField::Name];
        if self.project.is_some() {
            fields.push(FormField::Scope);
        }
        fields
    }

    pub fn next_field(&mut self, delta: isize) {
        let fields = self.fields();
        let position = fields
            .iter()
            .position(|field| *field == self.field)
            .unwrap_or(0) as isize;
        self.field = fields[(position + delta).rem_euclid(fields.len() as isize) as usize];
    }

    pub fn toggle_scope(&mut self) {
        if self.project.is_some() {
            self.scope = match self.scope {
                Scope::Project => Scope::User,
                Scope::User => Scope::Project,
            };
        }
    }

    /// The text field in focus, when the focus is on one.
    pub fn input(&mut self) -> Option<&mut LineInput> {
        match self.field {
            FormField::Path => Some(&mut self.path),
            FormField::Name => Some(&mut self.name),
            FormField::Scope => None,
        }
    }

    /// The file it will be written to, named by its path: "project" and
    /// "yours" mean nothing until you know which files they are, and this
    /// form is where you find out.
    pub fn scope_display(&self) -> String {
        match self.scope {
            Scope::Project => self.project.clone().unwrap_or_default(),
            Scope::User => self.user.clone(),
        }
    }

    /// The path as typed, taken as a shell would take it: `~` is your home,
    /// and a relative path starts where binman was started.
    pub fn typed_path(&self) -> PathBuf {
        let here = std::env::current_dir().unwrap_or_default();
        workspace::resolve(&here, self.path.text().trim())
    }

    /// What the collection will be called if Name is left empty.
    pub fn placeholder(&self) -> String {
        if self.path.text().trim().is_empty() {
            return String::new();
        }
        workspace::name_for(&self.typed_path())
    }
}

/// A file as it is worth showing: from where binman was started when it is
/// under there, since `./.binman.json` says all an absolute path does, and
/// from your home otherwise.
pub fn shown(path: &Path) -> String {
    let here = std::env::current_dir().ok();
    match here
        .as_deref()
        .and_then(|here| path.strip_prefix(here).ok())
    {
        Some(rest) if !rest.as_os_str().is_empty() => format!("./{}", rest.display()),
        _ => workspace::tilde(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str) -> Entry {
        Entry {
            method: Some("GET".into()),
            label: label.into(),
            detail: "users".into(),
            hint: "",
            action: Action::Run(Command::Help),
        }
    }

    #[test]
    fn typing_narrows_by_subsequence() {
        let mut picker = Picker::new(
            PickerKind::Find,
            vec![entry("list.http"), entry("create.http")],
            0,
        );
        for ch in "crt".chars() {
            picker.push(ch);
        }
        assert_eq!(picker.matches(), vec![1]);
        assert_eq!(picker.chosen(), Some(1));
    }

    #[test]
    fn the_selection_wraps() {
        let mut picker = Picker::new(PickerKind::Find, vec![entry("a"), entry("b")], 0);
        picker.move_selection(-1);
        assert_eq!(picker.selected, 1);
    }
}
