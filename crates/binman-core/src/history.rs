//! Every request sent, one JSON object per line.
//!
//! The file and its shape are v1's, so the history written by either version
//! is read by the other.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub timestamp: DateTime<FixedOffset>,
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    /// 0 when no response came back.
    #[serde(default)]
    pub status: u16,
    #[serde(default)]
    pub duration_ns: u64,
}

impl Entry {
    pub fn new(
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
        status: u16,
        duration: Duration,
    ) -> Entry {
        Entry {
            timestamp: chrono::Local::now().fixed_offset(),
            method: method.to_string(),
            url: url.to_string(),
            headers: headers.iter().cloned().collect(),
            body: body.to_string(),
            status,
            duration_ns: u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        }
    }
}

#[derive(Debug, Clone)]
pub struct History {
    path: PathBuf,
}

impl History {
    /// `$XDG_STATE_HOME/binman/history.jsonl`, or `~/.local/state/…`.
    pub fn default_path() -> PathBuf {
        let base = match std::env::var_os("XDG_STATE_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => dirs::home_dir()
                .unwrap_or_default()
                .join(".local")
                .join("state"),
        };
        base.join("binman").join("history.jsonl")
    }

    pub fn at(path: impl Into<PathBuf>) -> History {
        History { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, entry: &Entry) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
        line.push('\n');
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(line.as_bytes())?;
        Ok(())
    }

    /// The last `limit` entries, oldest first. A line that does not parse is
    /// skipped rather than costing the whole history.
    pub fn load(&self, limit: usize) -> Result<Vec<Entry>> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut entries: Vec<Entry> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        entries.sort_by_key(|entry| entry.timestamp);
        let skip = entries.len().saturating_sub(limit);
        Ok(entries.split_off(skip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn appends_and_reads_back_in_order() {
        let history = History::at(testing::scratch("history").join("state").join("history.jsonl"));
        for status in [200, 201, 404] {
            history
                .append(&Entry::new(
                    "GET",
                    "https://example.com",
                    &[],
                    "",
                    status,
                    Duration::from_millis(50),
                ))
                .expect("append");
        }

        let entries = history.load(50).expect("load");
        let statuses: Vec<u16> = entries.iter().map(|entry| entry.status).collect();
        assert_eq!(statuses, vec![200, 201, 404]);
        assert_eq!(history.load(2).unwrap().len(), 2, "limit keeps the newest");
    }

    #[test]
    fn a_missing_file_is_an_empty_history() {
        let history = History::at(testing::scratch("history-empty").join("none.jsonl"));
        assert!(history.load(50).unwrap().is_empty());
    }

    #[test]
    fn reads_a_line_v1_wrote() {
        let line = r#"{"timestamp":"2026-08-26T15:03:04.123456789+02:00","method":"POST","url":"https://api/x","headers":{"Content-Type":"application/json"},"body":"{}","status":201,"duration_ns":50000000}"#;
        let entry: Entry = serde_json::from_str(line).expect("v1 line parses");
        assert_eq!(entry.status, 201);
        assert_eq!(entry.headers["Content-Type"], "application/json");
        assert_eq!(entry.duration_ns, 50_000_000);
    }
}
