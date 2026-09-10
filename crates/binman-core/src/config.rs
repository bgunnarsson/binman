//! `~/.config/binman/config`, read exactly as v1 wrote it.
//!
//! ```text
//! HTTP_FILES  = /path/to/collections
//! TIMEOUT     = 30s
//! CLIENT_CERT = /path/to/client.crt
//! CLIENT_KEY  = /path/to/client.key
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{Error, Result};

/// Applied when `TIMEOUT` is not set at all. v1's code did the same, whatever
/// its README said.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `HTTP_FILES`: the directory every collection is read from.
    pub root: PathBuf,
    /// `None` is no timeout at all, which is what a streaming endpoint needs.
    pub timeout: Option<Duration>,
    /// `CLIENT_CERT` and `CLIENT_KEY`. mTLS needs both, so one on its own is
    /// ignored, as it was in v1.
    pub client_identity: Option<(PathBuf, PathBuf)>,
}

impl Config {
    /// The config file, honouring `XDG_CONFIG_HOME`.
    pub fn path() -> PathBuf {
        let base = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => dirs::home_dir().unwrap_or_default().join(".config"),
        };
        base.join("binman").join("config")
    }

    pub fn load() -> Result<Config> {
        Self::load_from(&Self::path())
    }

    /// A missing file reads as an empty one, which then fails for want of
    /// `HTTP_FILES` — with a message that says where to put it.
    pub fn load_from(path: &Path) -> Result<Config> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(Error::Config(format!(
                    "reading {}: {error}",
                    path.display()
                )));
            }
        };
        Self::parse(&text, path)
    }

    /// `origin` is only for the messages: a missing `HTTP_FILES` should say
    /// which file it is missing from.
    pub fn parse(text: &str, origin: &Path) -> Result<Config> {
        let mut root = None;
        let mut timeout = Some(DEFAULT_TIMEOUT);
        let mut cert = None;
        let mut key = None;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match name.trim() {
                "HTTP_FILES" => root = Some(PathBuf::from(value)),
                "TIMEOUT" => {
                    timeout = match parse_duration(value) {
                        Some(duration) if duration.is_zero() => None,
                        Some(duration) => Some(duration),
                        None => {
                            return Err(Error::Config(format!(
                                "TIMEOUT = {value} in {} is not a duration — write it as 30s, 2m or 1m30s",
                                origin.display()
                            )));
                        }
                    }
                }
                "CLIENT_CERT" => cert = Some(PathBuf::from(value)),
                "CLIENT_KEY" => key = Some(PathBuf::from(value)),
                // v1 passed over keys it did not know, and so does this.
                _ => {}
            }
        }

        let root = root.ok_or_else(|| {
            Error::Config(format!(
                "HTTP_FILES is not set — add `HTTP_FILES = /path/to/collections` to {}",
                origin.display()
            ))
        })?;

        Ok(Config {
            root,
            timeout,
            client_identity: cert.zip(key),
        })
    }
}

/// Reads a duration the way Go's `time.ParseDuration` does, since that is what
/// v1 accepted: `30s`, `1m30s`, `1.5s`, `500ms`, `2h`, or a bare `0`.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    if text == "0" {
        return Some(Duration::ZERO);
    }
    if text.is_empty() {
        return None;
    }

    let mut nanos = 0f64;
    let mut rest = text;
    while !rest.is_empty() {
        let number_end = rest
            .find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
            .unwrap_or(rest.len());
        if number_end == 0 {
            return None;
        }
        let number: f64 = rest[..number_end].parse().ok()?;
        rest = &rest[number_end..];

        let unit_end = rest
            .find(|ch: char| ch.is_ascii_digit() || ch == '.')
            .unwrap_or(rest.len());
        let scale = match &rest[..unit_end] {
            "ns" => 1.0,
            "us" | "µs" | "μs" => 1e3,
            "ms" => 1e6,
            "s" => 1e9,
            "m" => 60e9,
            "h" => 3600e9,
            _ => return None,
        };
        nanos += number * scale;
        rest = &rest[unit_end..];
    }
    Some(Duration::from_nanos(nanos.round() as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config> {
        Config::parse(text, Path::new("/tmp/binman/config"))
    }

    #[test]
    fn reads_the_v1_file_as_documented() {
        let config = parse(
            "# Path to the directory containing your HTTP request files (required)\n\
             HTTP_FILES  = /srv/collections\n\
             TIMEOUT     = 45s\n\
             CLIENT_CERT = /etc/client.crt\n\
             CLIENT_KEY  = /etc/client.key\n",
        )
        .expect("parses");
        assert_eq!(config.root, PathBuf::from("/srv/collections"));
        assert_eq!(config.timeout, Some(Duration::from_secs(45)));
        assert_eq!(
            config.client_identity,
            Some((
                PathBuf::from("/etc/client.crt"),
                PathBuf::from("/etc/client.key")
            ))
        );
    }

    #[test]
    fn http_files_is_required_and_the_message_says_where() {
        let error = parse("TIMEOUT = 5s\n").unwrap_err().to_string();
        assert!(error.contains("HTTP_FILES"), "{error}");
        assert!(error.contains("/tmp/binman/config"), "{error}");
    }

    #[test]
    fn an_absent_timeout_is_thirty_seconds_and_zero_is_none() {
        assert_eq!(
            parse("HTTP_FILES = /x").unwrap().timeout,
            Some(DEFAULT_TIMEOUT)
        );
        assert_eq!(parse("HTTP_FILES = /x\nTIMEOUT = 0").unwrap().timeout, None);
        assert_eq!(
            parse("HTTP_FILES = /x\nTIMEOUT = 0s").unwrap().timeout,
            None
        );
    }

    #[test]
    fn a_timeout_that_is_not_a_duration_is_reported() {
        let error = parse("HTTP_FILES = /x\nTIMEOUT = 30")
            .unwrap_err()
            .to_string();
        assert!(error.contains("TIMEOUT = 30"), "{error}");
    }

    #[test]
    fn one_half_of_a_client_identity_is_ignored() {
        let config = parse("HTTP_FILES = /x\nCLIENT_CERT = /c.crt").unwrap();
        assert_eq!(config.client_identity, None);
    }

    #[test]
    fn durations_read_as_go_writes_them() {
        assert_eq!(parse_duration("1m30s"), Some(Duration::from_secs(90)));
        assert_eq!(parse_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("1.5s"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_duration("2h"), Some(Duration::from_secs(7200)));
        assert_eq!(parse_duration("10"), None);
        assert_eq!(parse_duration("ten seconds"), None);
    }
}
