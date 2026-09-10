//! Bruno's `.bru` files.
//!
//! A file is a run of named blocks. A block opens on a line that starts at the
//! margin and ends in `{`, and closes at a `}` that is also at the margin.
//! Everything between is indented, which is what lets a JSON body hold braces
//! of its own:
//!
//! ```text
//! meta {
//!   name: create user
//!   seq: 1
//! }
//!
//! post {
//!   url: {{BASE}}/users
//!   body: json
//! }
//!
//! headers {
//!   Accept: application/json
//!   ~X-Debug: 1
//! }
//!
//! body:json {
//!   { "name": { "first": "Jane" } }
//! }
//! ```
//!
//! v1 closed a block at any line that trimmed to `}`, so a body with a nested
//! object ran on to the end of the file; and it saved by rewriting the whole
//! file from four fields, which lost `meta`, `vars`, docs and every disabled
//! header. Saving now changes only the blocks that changed.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::body::{self, BodyKind};
use crate::error::Result;
use crate::paths::lineage;
use crate::request::Request;
use crate::vars::Vars;

const METHOD_BLOCKS: [&str; 9] = [
    "get", "post", "put", "patch", "delete", "head", "options", "connect", "trace",
];

/// One top-level block, by line position in the file.
struct Block<'a> {
    name: &'a str,
    /// The line that opens it.
    start: usize,
    /// The line that closes it — the last line of the file for a block that
    /// never closes.
    end: usize,
    /// What sits between the two.
    lines: &'a [&'a str],
}

fn blocks<'a>(lines: &'a [&'a str]) -> Vec<Block<'a>> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let opens = !line.starts_with(char::is_whitespace) && line.trim_end().ends_with('{');
        if !opens {
            index += 1;
            continue;
        }

        let start = index;
        let close = lines[start + 1..]
            .iter()
            .position(|candidate| candidate.trim_end() == "}")
            .map(|offset| start + 1 + offset);
        let inner_end = close.unwrap_or(lines.len());
        out.push(Block {
            name: line.trim_end().trim_end_matches('{').trim(),
            start,
            end: close.unwrap_or(lines.len() - 1),
            lines: &lines[start + 1..inner_end],
        });
        index = inner_end + 1;
    }
    out
}

fn is_method_block(name: &str) -> bool {
    METHOD_BLOCKS.contains(&name.to_ascii_lowercase().as_str())
}

/// `key: value` lines. `~` marks one Bruno has switched off, and `//` a
/// comment; both are passed over.
fn pairs(lines: &[&str]) -> Vec<(String, String)> {
    lines
        .iter()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") || line.starts_with('~') {
                return None;
            }
            let (key, value) = line.split_once(':')?;
            let key = key.trim();
            (!key.is_empty()).then(|| (key.to_string(), value.trim().to_string()))
        })
        .collect()
}

/// A body block's text without the two spaces Bruno indents it by.
fn dedent(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|line| {
            line.strip_prefix("  ")
                .or_else(|| line.strip_prefix(' '))
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn body_mode(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::None => "none",
        BodyKind::Json => "json",
        BodyKind::Form => "form-urlencoded",
        BodyKind::Multipart => "multipart-form",
        BodyKind::Xml => "xml",
        BodyKind::Text => "text",
        BodyKind::Sparql => "sparql",
    }
}

fn kind_of_mode(mode: &str) -> Option<BodyKind> {
    Some(match mode {
        "json" => BodyKind::Json,
        "form-urlencoded" => BodyKind::Form,
        "multipart-form" => BodyKind::Multipart,
        "xml" => BodyKind::Xml,
        "text" => BodyKind::Text,
        "sparql" => BodyKind::Sparql,
        _ => return None,
    })
}

/// A form block's fields, with Bruno's `@file(path)` turned into binman's
/// `@path`.
fn form_fields(lines: &[&str]) -> Vec<(String, String)> {
    pairs(lines)
        .into_iter()
        .map(|(name, value)| {
            let value = match value
                .strip_prefix("@file(")
                .and_then(|rest| rest.strip_suffix(')'))
            {
                Some(path) => format!("@{path}"),
                None => value,
            };
            (name, value)
        })
        .collect()
}

pub fn parse(text: &str) -> Request {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut request = Request::default();
    let mut graphql = None;
    let mut graphql_vars = None;

    for block in blocks(&lines) {
        let name = block.name.to_ascii_lowercase();
        if is_method_block(&name) {
            request.method = name.to_ascii_uppercase();
            if let Some((_, url)) = pairs(block.lines).into_iter().find(|(key, _)| key == "url") {
                request.url = url;
            }
            continue;
        }
        match name.as_str() {
            "headers" => request.headers.extend(pairs(block.lines)),
            // `vars:post-response` is set after the request runs, so it has
            // nothing to say about the one going out.
            "vars" | "vars:pre-request" => request.vars.extend(pairs(block.lines)),
            "body:graphql" => graphql = Some(dedent(block.lines)),
            "body:graphql:vars" => graphql_vars = Some(dedent(block.lines)),
            other => {
                if let Some(kind) = other.strip_prefix("body:").and_then(kind_of_mode) {
                    request.kind = Some(kind);
                    request.body = if kind.is_form() {
                        body::encode_form(&form_fields(block.lines))
                    } else {
                        dedent(block.lines)
                    };
                }
            }
        }
    }

    // A GraphQL request goes out as the JSON document every server expects.
    if let Some(query) = graphql {
        let mut payload = serde_json::Map::new();
        payload.insert("query".into(), Value::String(query));
        if let Some(variables) = graphql_vars
            .as_deref()
            .and_then(|vars| serde_json::from_str::<Value>(vars).ok())
        {
            payload.insert("variables".into(), variables);
        }
        request.body = Value::Object(payload).to_string();
        request.kind = Some(BodyKind::Json);
    }
    request
}

/// Every `vars` block merged: `vars`, `vars:pre-request` and the rest. What
/// `collection.bru`, `folder.bru` and an environment file hold.
pub fn vars_blocks(text: &str) -> Vars {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    blocks(&lines)
        .iter()
        .filter(|block| {
            let name = block.name.to_ascii_lowercase();
            name == "vars" || name.starts_with("vars:")
        })
        .flat_map(|block| pairs(block.lines))
        .collect()
}

pub fn load_environment(path: &Path) -> Result<Vars> {
    Ok(vars_blocks(&std::fs::read_to_string(path)?))
}

/// The vars of every `collection.bru` and `folder.bru` from the collection
/// root down to `dir`. A deeper file overrides a shallower one.
pub fn collection_vars(dir: &Path, root: &Path) -> Vars {
    let mut merged = Vars::new();
    for directory in lineage(dir, root).iter().rev() {
        for name in ["collection.bru", "folder.bru"] {
            if let Ok(text) = std::fs::read_to_string(directory.join(name)) {
                merged.extend(vars_blocks(&text));
            }
        }
    }
    merged
}

/// Bruno keeps its environments in one `environments/` directory at the
/// collection root; the nearest one above `dir` is it.
pub fn environments(dir: &Path, root: &Path) -> Vec<(String, PathBuf)> {
    for directory in lineage(dir, root) {
        let Ok(entries) = std::fs::read_dir(directory.join("environments")) else {
            continue;
        };
        let mut found: Vec<(String, PathBuf)> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && super::Format::of(path) == Some(super::Format::Bru))
            .filter_map(|path| {
                let label = path.file_stem()?.to_str()?.to_string();
                Some((label, path))
            })
            .collect();
        if !found.is_empty() {
            found.sort();
            return found;
        }
    }
    Vec::new()
}

/// A `.bru` file for a request that has no file yet.
pub fn format(request: &Request, kind: BodyKind) -> String {
    let mut out = method_block(None, request, kind, true);
    if !request.headers.is_empty() {
        out.push(String::new());
        out.extend(headers_block(&request.headers, &[]));
    }
    if kind != BodyKind::None {
        out.push(String::new());
        out.extend(body_block(request, kind));
    }
    out.join("\n") + "\n"
}

/// `original` with the request's method, URL, headers and body written into
/// it. Every other block — meta, vars, auth, scripts, docs — is kept as it was,
/// and so is a headers or body block whose content has not changed, so saving
/// a request nobody touched leaves the file byte for byte.
pub fn update(original: &str, request: &Request, kind: BodyKind) -> String {
    let original = original.replace("\r\n", "\n");
    let lines: Vec<&str> = original.split('\n').collect();
    let blocks = blocks(&lines);

    let before = parse(&original);
    let headers_changed = before.headers != request.headers;
    let body_changed = before.body_kind() != kind || before.body != request.body;

    let method_index = blocks.iter().position(|block| is_method_block(block.name));
    let has_headers = blocks
        .iter()
        .any(|block| block.name.eq_ignore_ascii_case("headers"));
    let has_body = blocks
        .iter()
        .any(|block| block.name.to_ascii_lowercase().starts_with("body:"));

    let mut out: Vec<String> = Vec::new();
    if method_index.is_none() {
        out.extend(method_block(None, request, kind, true));
        out.push(String::new());
    }

    let mut cursor = 0;
    let mut body_written = false;
    for (index, block) in blocks.iter().enumerate() {
        out.extend(
            lines[cursor..block.start]
                .iter()
                .map(|line| line.to_string()),
        );
        cursor = block.end + 1;
        let verbatim = || {
            lines[block.start..=block.end]
                .iter()
                .map(|line| line.to_string())
        };
        let name = block.name.to_ascii_lowercase();

        if Some(index) == method_index {
            out.extend(method_block(Some(block), request, kind, body_changed));
            if !has_headers && !request.headers.is_empty() {
                out.push(String::new());
                out.extend(headers_block(&request.headers, &[]));
            }
            if !has_body && kind != BodyKind::None {
                out.push(String::new());
                out.extend(body_block(request, kind));
                body_written = true;
            }
        } else if name == "headers" {
            let disabled = block
                .lines
                .iter()
                .any(|line| line.trim_start().starts_with('~'));
            if !headers_changed {
                out.extend(verbatim());
            } else if request.headers.is_empty() && !disabled {
                cursor = skip_blank(&lines, cursor);
            } else {
                out.extend(headers_block(&request.headers, block.lines));
            }
        } else if name.starts_with("body:") {
            if !body_changed {
                out.extend(verbatim());
            } else if !body_written && kind != BodyKind::None {
                out.extend(body_block(request, kind));
                body_written = true;
            } else {
                cursor = skip_blank(&lines, cursor);
            }
        } else {
            out.extend(verbatim());
        }
    }
    out.extend(
        lines[cursor.min(lines.len())..]
            .iter()
            .map(|line| line.to_string()),
    );
    out.join("\n")
}

/// Past one blank line after a removed block, so removing it does not leave
/// two gaps where there was one.
fn skip_blank(lines: &[&str], cursor: usize) -> usize {
    if lines.get(cursor).is_some_and(|line| line.trim().is_empty()) {
        cursor + 1
    } else {
        cursor
    }
}

/// The method block, rewriting the lines binman owns — the URL, and the body
/// mode when the body changed — and keeping every other line Bruno put there.
fn method_block(
    original: Option<&Block>,
    request: &Request,
    kind: BodyKind,
    body_changed: bool,
) -> Vec<String> {
    let method = if request.method.is_empty() {
        "get".to_string()
    } else {
        request.method.to_ascii_lowercase()
    };
    let mut out = vec![format!("{method} {{")];
    let mut wrote_url = false;
    let mut wrote_body = false;

    for line in original.map(|block| block.lines).unwrap_or_default() {
        let key = line.split_once(':').map(|(key, _)| key.trim());
        match key {
            Some("url") => {
                out.push(format!("  url: {}", request.url));
                wrote_url = true;
            }
            Some("body") if body_changed => {
                out.push(format!("  body: {}", body_mode(kind)));
                wrote_body = true;
            }
            Some("body") => {
                out.push(line.to_string());
                wrote_body = true;
            }
            _ => out.push(line.to_string()),
        }
    }
    if !wrote_url {
        out.insert(1, format!("  url: {}", request.url));
    }
    if !wrote_body && (body_changed || original.is_none()) {
        out.insert(2, format!("  body: {}", body_mode(kind)));
    }
    out.push("}".to_string());
    out
}

/// The headers block. Headers Bruno has switched off are Bruno's business, not
/// binman's: they are carried over from the original block untouched.
fn headers_block(headers: &[(String, String)], original: &[&str]) -> Vec<String> {
    let mut out = vec!["headers {".to_string()];
    out.extend(
        headers
            .iter()
            .map(|(name, value)| format!("  {name}: {value}")),
    );
    out.extend(
        original
            .iter()
            .filter(|line| line.trim_start().starts_with('~'))
            .map(|line| line.to_string()),
    );
    out.push("}".to_string());
    out
}

fn body_block(request: &Request, kind: BodyKind) -> Vec<String> {
    let mut out = vec![format!("body:{} {{", body_mode(kind))];
    if kind.is_form() {
        for (name, value) in body::parse_form(&request.body) {
            let value = match value.strip_prefix('@') {
                Some(path) if kind == BodyKind::Multipart => format!("@file({path})"),
                _ => value,
            };
            out.push(format!("  {name}: {value}"));
        }
    } else {
        out.extend(request.body.lines().map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("  {line}")
            }
        }));
    }
    out.push("}".to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    const CREATE: &str = "meta {
  name: create user
  seq: 3
}

post {
  url: https://example.com/users
  body: json
  auth: none
}

headers {
  Authorization: Bearer abc
  Accept: application/json
  ~X-Debug: 1
}

vars:pre-request {
  trace: abc
}

body:json {
  {
    \"name\": {
      \"first\": \"Jane\"
    },
    \"age\": 30
  }
}

docs {
  Creates a user.

  Returns 201.
}
";

    #[test]
    fn reads_method_url_headers_vars_and_body() {
        let request = parse(CREATE);
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://example.com/users");
        assert_eq!(
            request.headers,
            vec![
                ("Authorization".to_string(), "Bearer abc".to_string()),
                ("Accept".to_string(), "application/json".to_string()),
            ],
            "the disabled header is not sent"
        );
        assert_eq!(request.vars["trace"], "abc");
        assert_eq!(request.kind, Some(BodyKind::Json));
        assert!(
            request.body.starts_with("{\n  \"name\": {"),
            "{}",
            request.body
        );
    }

    #[test]
    fn a_nested_json_body_does_not_swallow_the_rest_of_the_file() {
        let request = parse(CREATE);
        let parsed: Value = serde_json::from_str(&request.body).expect("the body is whole JSON");
        assert_eq!(parsed["name"]["first"], "Jane");
        assert!(!request.body.contains("docs"), "{}", request.body);
    }

    #[test]
    fn form_bodies_are_read_as_fields() {
        let request = parse(
            "post {\n  url: https://x\n  body: multipart-form\n}\n\nbody:multipart-form {\n  name: Jane Doe\n  avatar: @file(/tmp/a.png)\n}\n",
        );
        assert_eq!(request.kind, Some(BodyKind::Multipart));
        assert_eq!(
            body::parse_form(&request.body),
            vec![
                ("name".to_string(), "Jane Doe".to_string()),
                ("avatar".to_string(), "@/tmp/a.png".to_string()),
            ]
        );
    }

    #[test]
    fn a_graphql_body_becomes_the_json_a_server_expects() {
        let request = parse(
            "post {\n  url: https://x/graphql\n  body: graphql\n}\n\nbody:graphql {\n  query { me { id } }\n}\n\nbody:graphql:vars {\n  {\"id\": 1}\n}\n",
        );
        let payload: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(payload["query"], "query { me { id } }");
        assert_eq!(payload["variables"]["id"], 1);
    }

    #[test]
    fn saving_an_untouched_request_leaves_the_file_alone() {
        let request = parse(CREATE);
        let kind = request.body_kind();
        assert_eq!(update(CREATE, &request, kind), CREATE);
    }

    #[test]
    fn saving_changes_only_what_changed() {
        let mut request = parse(CREATE);
        request.url = "https://example.com/v2/users".into();
        request.headers.retain(|(name, _)| name != "Authorization");
        let saved = update(CREATE, &request, BodyKind::Json);

        assert!(
            saved.contains("  url: https://example.com/v2/users"),
            "{saved}"
        );
        assert!(!saved.contains("Bearer abc"), "{saved}");
        for kept in [
            "name: create user",
            "~X-Debug: 1",
            "trace: abc",
            "Returns 201.",
            "auth: none",
            "\"first\": \"Jane\"",
        ] {
            assert!(saved.contains(kept), "{kept} was lost:\n{saved}");
        }
        let again = parse(&saved);
        assert_eq!(again.url, request.url);
        assert_eq!(again.headers, request.headers);
        assert_eq!(again.body, request.body);
    }

    #[test]
    fn removing_the_body_removes_its_block_and_mode() {
        let mut request = parse(CREATE);
        request.body.clear();
        let saved = update(CREATE, &request, BodyKind::None);
        assert!(!saved.contains("body:json"), "{saved}");
        assert!(saved.contains("  body: none"), "{saved}");
        assert!(saved.contains("Returns 201."), "{saved}");
    }

    #[test]
    fn a_fresh_file_round_trips() {
        let request = Request {
            method: "PATCH".into(),
            url: "https://api/x".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: "{\"a\":1}".into(),
            ..Request::default()
        };
        let again = parse(&format(&request, BodyKind::Json));
        assert_eq!(again.method, "PATCH");
        assert_eq!(again.url, request.url);
        assert_eq!(again.headers, request.headers);
        assert_eq!(again.body, request.body);
    }

    #[test]
    fn vars_blocks_merge_and_skip_disabled_entries() {
        let vars = vars_blocks(
            "vars {\n  a: 1\n  b: 2\n  ~c: 3\n}\n\nvars:secret [\n  token\n]\n\nvars:pre-request {\n  d: 4\n}\n",
        );
        assert_eq!(vars.len(), 3, "{vars:?}");
        assert_eq!(vars["d"], "4");
    }

    #[test]
    fn collection_vars_walk_down_and_the_folder_wins() {
        let root = testing::scratch("bru-collection");
        let collection = root.join("coll");
        let folder = collection.join("users");
        testing::write(
            &collection.join("collection.bru"),
            "vars {\n  base: https://api.example.com\n  shared: c\n}\n",
        );
        testing::write(&folder.join("folder.bru"), "vars {\n  shared: f\n}\n");

        let vars = collection_vars(&folder, &root);
        assert_eq!(vars["base"], "https://api.example.com");
        assert_eq!(vars["shared"], "f");
    }

    #[test]
    fn finds_the_environments_directory_above() {
        let root = testing::scratch("bru-environments");
        let collection = root.join("coll");
        testing::write(
            &collection.join("environments").join("prod.bru"),
            "vars {\n  host: prod\n}\n",
        );
        testing::write(
            &collection.join("environments").join("dev.bru"),
            "vars {\n  host: dev\n}\n",
        );
        std::fs::create_dir_all(collection.join("auth")).unwrap();

        let found = environments(&collection.join("auth"), &root);
        let labels: Vec<&str> = found.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(labels, vec!["dev", "prod"]);
        assert_eq!(load_environment(&found[0].1).unwrap()["host"], "dev");
    }
}
