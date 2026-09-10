//! The things that draw on top of the layout: the greeting, help, the pickers,
//! the environment editor, and the prompt for where to save a response.

use std::path::PathBuf;

use binman_core::Origin;
use binman_core::history;
use tui_textarea::TextArea;

use super::line::LineInput;

pub enum Overlay {
    /// Shown once at startup. Any key dismisses it.
    Splash,
    Help,
    Picker(Picker),
    /// Boxed: an editor is far larger than anything else here.
    Env(Box<EnvEditor>),
    SaveResponse(SavePrompt),
    /// The request as a curl command, to read or select.
    Curl(String),
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
    SaveResponse,
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
            Command::SaveResponse => "Save the response…",
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
            Command::Save => "⌃S",
            Command::Reload => "F5",
            Command::Help => "F1",
            Command::Quit => "⌃Q",
            Command::EditEnv | Command::SaveResponse => "",
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
