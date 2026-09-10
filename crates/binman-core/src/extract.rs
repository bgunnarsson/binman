//! Pulling values out of a response into variables.
//!
//! Rules are written in a request's Scripts tab, one per line:
//!
//! ```text
//! token = json .access_token
//! id    = json data.items.0.id
//! csrf  = header X-CSRF-Token
//! code  = regex /code=(\d+)/
//! ```
//!
//! `#` starts a comment. A rule whose kind is not one of the three is passed
//! over rather than failing the others.

use regex::Regex;
use serde_json::Value;

use crate::request::header;
use crate::vars::Vars;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub name: String,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A dotted path into the JSON body; numeric segments index arrays.
    Json(String),
    /// The first capture group of the pattern, or the whole match without one.
    Regex(String),
    Header(String),
}

pub fn parse(script: &str) -> Vec<Rule> {
    script
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, rest) = line.split_once('=')?;
            let (name, rest) = (name.trim(), rest.trim());
            if name.is_empty() || rest.is_empty() {
                return None;
            }
            let (kind, argument) = rest.split_once(' ')?;
            let argument = argument.trim().to_string();
            let source = match kind.to_ascii_lowercase().as_str() {
                "json" => Source::Json(argument),
                "regex" => Source::Regex(argument),
                "header" => Source::Header(argument),
                _ => return None,
            };
            Some(Rule {
                name: name.to_string(),
                source,
            })
        })
        .collect()
}

/// Runs the rules against a response. A rule that finds nothing is left out
/// rather than setting its variable to an empty string, which would shadow a
/// value from a lower layer.
pub fn apply(rules: &[Rule], body: &str, headers: &[(String, String)]) -> Vars {
    let mut json: Option<Option<Value>> = None;
    let mut out = Vars::new();

    for rule in rules {
        let found = match &rule.source {
            Source::Json(path) => {
                let root = json.get_or_insert_with(|| serde_json::from_str(body).ok());
                root.as_ref().and_then(|root| walk(root, path))
            }
            Source::Regex(pattern) => {
                Regex::new(pattern.trim_matches('/'))
                    .ok()
                    .and_then(|regex| {
                        let captures = regex.captures(body)?;
                        captures
                            .get(1)
                            .or_else(|| captures.get(0))
                            .map(|found| found.as_str().to_string())
                    })
            }
            Source::Header(name) => header(headers, name)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        };
        if let Some(value) = found {
            out.insert(rule.name.clone(), value);
        }
    }
    out
}

fn walk(root: &Value, path: &str) -> Option<String> {
    let mut current = root;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    match current {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        other => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_kinds_and_skips_the_rest() {
        let rules = parse(
            "# header comment\n\
             TOKEN = json data.access_token\n\
             ID = regex /id=(\\d+)/\n\
             SERVER = header Server\n\
             malformed line\n\
             OTHER = xpath //a\n",
        );
        assert_eq!(rules.len(), 3, "{rules:?}");
        assert_eq!(
            rules[0],
            Rule {
                name: "TOKEN".into(),
                source: Source::Json("data.access_token".into())
            }
        );
    }

    #[test]
    fn walks_json_paths() {
        let rules = parse(
            "ID = json data.0.id\n\
             NAME = json data.0.name\n\
             ACTIVE = json data.0.active\n\
             TOKEN = json .token\n\
             MISSING = json data.99.id\n",
        );
        let out = apply(
            &rules,
            r#"{"token":"t","data":[{"id": 42, "name": "alice", "active": true}]}"#,
            &[],
        );
        assert_eq!(out["ID"], "42");
        assert_eq!(out["NAME"], "alice");
        assert_eq!(out["ACTIVE"], "true");
        assert_eq!(out["TOKEN"], "t");
        assert!(!out.contains_key("MISSING"));
    }

    #[test]
    fn a_regex_takes_its_first_group() {
        let out = apply(
            &parse("ID = regex /id=(\\d+)/"),
            "user id=4711 something",
            &[],
        );
        assert_eq!(out["ID"], "4711");
    }

    #[test]
    fn a_header_is_found_in_any_case() {
        let headers = vec![("x-trace-id".to_string(), "abc123".to_string())];
        let out = apply(&parse("TID = header X-Trace-Id"), "", &headers);
        assert_eq!(out["TID"], "abc123");
    }
}
