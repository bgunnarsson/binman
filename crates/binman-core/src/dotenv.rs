//! `.env` files, and finding the ones above a request.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::paths::lineage;
use crate::vars::Vars;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvFile {
    /// `.env` is "default"; `.env.staging` is "staging".
    pub label: String,
    pub path: PathBuf,
}

/// `KEY=VALUE` lines. `#` starts a comment, and a value wrapped in matching
/// quotes loses them, as every other reader of these files does.
pub fn parse(text: &str) -> Vars {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), unquote(value.trim()).to_string()))
        })
        .collect()
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

pub fn load(path: &Path) -> Result<Vars> {
    Ok(parse(&std::fs::read_to_string(path)?))
}

/// The `.env` and `.env.*` files from `dir` up to `root`. When two levels have
/// the same label the deeper one wins, so a subdirectory can override the
/// collection-wide file.
pub fn find(dir: &Path, root: &Path) -> Vec<EnvFile> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for directory in lineage(dir, root) {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_ok_and(|kind| !kind.is_dir()))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();

        for name in names {
            let label = if name == ".env" {
                "default"
            } else if let Some(label) = name.strip_prefix(".env.") {
                label
            } else {
                continue;
            };
            if label.is_empty() || !seen.insert(label.to_string()) {
                continue;
            }
            out.push(EnvFile {
                label: label.to_string(),
                path: directory.join(&name),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn reads_keys_values_and_comments() {
        let vars = parse("# comment\nFOO=bar\nBAZ = qux\n\nQUOTED=\"a b\"\nSINGLE='c'\n");
        assert_eq!(vars["FOO"], "bar");
        assert_eq!(vars["BAZ"], "qux");
        assert_eq!(vars["QUOTED"], "a b");
        assert_eq!(vars["SINGLE"], "c");
    }

    #[test]
    fn walks_up_and_the_deeper_file_wins() {
        let root = testing::scratch("dotenv-walk");
        let deep = root.join("a").join("b").join("c");
        testing::write(&deep.join(".env"), "X=1");
        testing::write(&root.join(".env"), "X=0");
        testing::write(&root.join(".env.staging"), "Y=2");

        let files = find(&deep, &root);
        let labels: Vec<&str> = files.iter().map(|file| file.label.as_str()).collect();
        assert_eq!(labels, vec!["default", "staging"]);
        assert_eq!(files[0].path, deep.join(".env"));
    }
}
