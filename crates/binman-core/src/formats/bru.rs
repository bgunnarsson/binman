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

use crate::auth::{Auth, AuthKind};
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
    let mut auth = None;
    let mut auth_blocks = Vec::new();

    for block in blocks(&lines) {
        let name = block.name.to_ascii_lowercase();
        if is_method_block(&name) {
            request.method = name.to_ascii_uppercase();
            for (key, value) in pairs(block.lines) {
                match key.as_str() {
                    "url" => request.url = value,
                    "auth" => auth = Some(value),
                    _ => {}
                }
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
                if let Some(mode) = other.strip_prefix("auth:") {
                    auth_blocks.push((mode.to_string(), pairs(block.lines)));
                } else if let Some(kind) = other.strip_prefix("body:").and_then(kind_of_mode) {
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

    // The method block's `auth:` line says which block is in force; the
    // others are what Bruno remembers of kinds switched away from.
    if let Some(mode) = auth
        && let Some((_, block)) = auth_blocks.iter().find(|(name, _)| *name == mode)
    {
        request.auth = read_auth(&mode, block);
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
    let mut out = method_block(None, request, kind, true, true);
    if !request.headers.is_empty() {
        out.push(String::new());
        out.extend(headers_block(&request.headers, &[]));
    }
    if request.auth.kind != AuthKind::None {
        out.push(String::new());
        out.extend(auth_block(&request.auth, &[]));
    }
    if kind != BodyKind::None {
        out.push(String::new());
        out.extend(body_block(request, kind));
    }
    out.join("\n") + "\n"
}

/// `original` with the request's method, URL, headers, auth and body written
/// into it. Every other block — meta, vars, scripts, docs — is kept as it was,
/// and so is a headers, auth or body block whose content has not changed, so
/// saving a request nobody touched leaves the file byte for byte.
pub fn update(original: &str, request: &Request, kind: BodyKind) -> String {
    let original = original.replace("\r\n", "\n");
    let lines: Vec<&str> = original.split('\n').collect();
    let blocks = blocks(&lines);

    let before = parse(&original);
    let headers_changed = before.headers != request.headers;
    let body_changed = before.body_kind() != kind || before.body != request.body;
    let auth_changed = before.auth != request.auth;

    let method_index = blocks.iter().position(|block| is_method_block(block.name));
    let has_headers = blocks
        .iter()
        .any(|block| block.name.eq_ignore_ascii_case("headers"));
    let has_body = blocks
        .iter()
        .any(|block| block.name.to_ascii_lowercase().starts_with("body:"));
    let auth_name = format!("auth:{}", auth_mode(request.auth.kind));
    let add_auth = auth_changed
        && request.auth.kind != AuthKind::None
        && !blocks
            .iter()
            .any(|block| block.name.eq_ignore_ascii_case(&auth_name));

    let mut out: Vec<String> = Vec::new();
    if method_index.is_none() {
        out.extend(method_block(None, request, kind, true, true));
        out.push(String::new());
        if add_auth {
            out.extend(auth_block(&request.auth, &[]));
            out.push(String::new());
        }
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
            out.extend(method_block(
                Some(block),
                request,
                kind,
                body_changed,
                auth_changed,
            ));
            if !has_headers && !request.headers.is_empty() {
                out.push(String::new());
                out.extend(headers_block(&request.headers, &[]));
            }
            if add_auth {
                out.push(String::new());
                out.extend(auth_block(&request.auth, &[]));
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
        } else if let Some(mode) = name.strip_prefix("auth:")
            && auth_changed
        {
            if request.auth.kind != AuthKind::None && mode == auth_mode(request.auth.kind) {
                out.extend(auth_block(&request.auth, block.lines));
            } else if before.auth.kind != AuthKind::None && mode == auth_mode(before.auth.kind) {
                // The block of the kind it was, which the new one replaces.
                cursor = skip_blank(&lines, cursor);
            } else {
                out.extend(verbatim());
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
/// and auth modes when those changed — and keeping every other line Bruno put
/// there.
fn method_block(
    original: Option<&Block>,
    request: &Request,
    kind: BodyKind,
    body_changed: bool,
    auth_changed: bool,
) -> Vec<String> {
    let method = if request.method.is_empty() {
        "get".to_string()
    } else {
        request.method.to_ascii_lowercase()
    };
    let mut out = vec![format!("{method} {{")];
    let mut wrote_url = false;
    let mut wrote_body = false;
    let mut wrote_auth = false;

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
            Some("auth") if auth_changed => {
                out.push(format!("  auth: {}", auth_mode(request.auth.kind)));
                wrote_auth = true;
            }
            Some("auth") => {
                out.push(line.to_string());
                wrote_auth = true;
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
    if !wrote_auth && (auth_changed || original.is_none()) {
        out.push(format!("  auth: {}", auth_mode(request.auth.kind)));
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

/// Bruno's name for an auth kind: the method block's `auth:` line, and the
/// `auth:<mode>` block that holds its values.
fn auth_mode(kind: AuthKind) -> &'static str {
    match kind {
        AuthKind::None => "none",
        AuthKind::Bearer => "bearer",
        AuthKind::Basic => "basic",
        AuthKind::ApiKey => "apikey",
        AuthKind::ClientCredentials => "oauth2",
    }
}

/// The keys of an auth block that hold [`AuthKind::fields`], in that order.
fn auth_keys(kind: AuthKind) -> &'static [&'static str] {
    match kind {
        AuthKind::None => &[],
        AuthKind::Bearer => &["token"],
        AuthKind::Basic => &["username", "password"],
        AuthKind::ApiKey => &["key", "value"],
        AuthKind::ClientCredentials => &["access_token_url", "client_id", "client_secret", "scope"],
    }
}

/// The auth an `auth:<mode>` block describes, when it is one binman can send.
/// An API key Bruno puts in the query string, and an OAuth2 grant other than
/// client credentials, are not: they read as no auth, and since saving only
/// rewrites auth that changed, they stay in the file as they were.
fn read_auth(mode: &str, block: &[(String, String)]) -> Auth {
    let kind = match mode {
        "bearer" => AuthKind::Bearer,
        "basic" => AuthKind::Basic,
        "apikey" => AuthKind::ApiKey,
        "oauth2" => AuthKind::ClientCredentials,
        _ => return Auth::default(),
    };
    let value = |key: &str| {
        block
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    let sendable = match kind {
        AuthKind::ApiKey => value("placement").is_none_or(|placement| placement == "header"),
        AuthKind::ClientCredentials => value("grant_type") == Some("client_credentials"),
        _ => true,
    };
    if !sendable {
        return Auth::default();
    }
    Auth {
        kind,
        values: auth_keys(kind)
            .iter()
            .map(|key| value(key).unwrap_or_default().to_string())
            .collect(),
    }
}

/// What an auth block says, key by key. An API key's placement and an
/// OAuth2 grant are written as well: they are what make the block mean what
/// binman sends.
fn auth_entries(auth: &Auth) -> Vec<(&'static str, String)> {
    let mut entries: Vec<(&'static str, String)> = auth_keys(auth.kind)
        .iter()
        .enumerate()
        .map(|(index, key)| (*key, auth.values.get(index).cloned().unwrap_or_default()))
        .collect();
    match auth.kind {
        AuthKind::ApiKey => entries.push(("placement", "header".into())),
        AuthKind::ClientCredentials => {
            entries.insert(0, ("grant_type", "client_credentials".into()))
        }
        _ => {}
    }
    entries
}

/// The auth block. The keys binman owns are rewritten where they stand and
/// any missing are added after; every other line — Bruno's token placement,
/// its refresh settings — is carried over untouched.
fn auth_block(auth: &Auth, original: &[&str]) -> Vec<String> {
    let mut entries = auth_entries(auth);
    let mut out = vec![format!("auth:{} {{", auth_mode(auth.kind))];
    for line in original {
        let key = line.split_once(':').map(|(key, _)| key.trim());
        match entries.iter().position(|(owned, _)| Some(*owned) == key) {
            Some(index) => {
                let (key, value) = entries.remove(index);
                out.push(format!("  {key}: {value}"));
            }
            None => out.push(line.to_string()),
        }
    }
    out.extend(
        entries
            .into_iter()
            .map(|(key, value)| format!("  {key}: {value}")),
    );
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

    fn auth(kind: AuthKind, values: &[&str]) -> Auth {
        Auth {
            kind,
            values: values.iter().map(|value| value.to_string()).collect(),
        }
    }

    const BEARER: &str = "meta {
  name: me
}

get {
  url: https://example.com/me
  body: none
  auth: bearer
}

auth:bearer {
  token: {{token}}
}

docs {
  Who am I.
}
";

    #[test]
    fn reads_each_auth_kind_binman_sends() {
        assert_eq!(parse(BEARER).auth, auth(AuthKind::Bearer, &["{{token}}"]));
        assert_eq!(
            parse("get {\n  url: https://x\n  auth: basic\n}\n\nauth:basic {\n  username: alice\n  password: {{pw}}\n}\n").auth,
            auth(AuthKind::Basic, &["alice", "{{pw}}"])
        );
        assert_eq!(
            parse("get {\n  url: https://x\n  auth: apikey\n}\n\nauth:apikey {\n  key: X-Api-Key\n  value: k\n  placement: header\n}\n").auth,
            auth(AuthKind::ApiKey, &["X-Api-Key", "k"])
        );
        assert_eq!(
            parse("get {\n  url: https://x\n  auth: oauth2\n}\n\nauth:oauth2 {\n  grant_type: client_credentials\n  access_token_url: https://auth/token\n  client_id: app\n  client_secret: s3cret\n  scope: read\n}\n").auth,
            auth(AuthKind::ClientCredentials, &["https://auth/token", "app", "s3cret", "read"])
        );
    }

    #[test]
    fn the_mode_line_decides_which_block_is_in_force() {
        let request =
            parse("get {\n  url: https://x\n  auth: none\n}\n\nauth:bearer {\n  token: old\n}\n");
        assert_eq!(request.auth, Auth::default());
    }

    #[test]
    fn auth_binman_cannot_send_reads_as_none_and_is_left_alone() {
        for file in [
            "get {\n  url: https://x\n  auth: apikey\n}\n\nauth:apikey {\n  key: api_key\n  value: k\n  placement: queryparams\n}\n",
            "get {\n  url: https://x\n  auth: oauth2\n}\n\nauth:oauth2 {\n  grant_type: authorization_code\n  client_id: app\n}\n",
            "get {\n  url: https://x\n  auth: inherit\n}\n",
        ] {
            let request = parse(file);
            assert_eq!(request.auth, Auth::default(), "{file}");
            assert_eq!(update(file, &request, request.body_kind()), file);
        }
    }

    #[test]
    fn a_new_token_rewrites_only_the_token() {
        let mut request = parse(BEARER);
        request.auth = auth(AuthKind::Bearer, &["abc"]);
        let saved = update(BEARER, &request, BodyKind::None);
        assert_eq!(saved, BEARER.replace("{{token}}", "abc"));
    }

    #[test]
    fn another_kind_replaces_the_block_and_the_mode() {
        let mut request = parse(BEARER);
        request.auth = auth(AuthKind::Basic, &["alice", "pw"]);
        let saved = update(BEARER, &request, BodyKind::None);
        assert!(saved.contains("  auth: basic"), "{saved}");
        assert!(!saved.contains("auth:bearer"), "{saved}");
        assert!(
            saved.contains("name: me") && saved.contains("Who am I."),
            "{saved}"
        );
        assert_eq!(parse(&saved).auth, request.auth);

        request.auth = Auth::default();
        let cleared = update(&saved, &request, BodyKind::None);
        assert!(cleared.contains("  auth: none"), "{cleared}");
        assert!(!cleared.contains("auth:basic"), "{cleared}");
        assert_eq!(parse(&cleared).auth, Auth::default());
    }

    #[test]
    fn bruno_s_own_auth_settings_survive_a_save() {
        let original = "post {\n  url: https://x\n  body: none\n  auth: oauth2\n}\n\nauth:oauth2 {\n  grant_type: client_credentials\n  access_token_url: https://auth/token\n  client_id: app\n  client_secret: old\n  scope: read\n  credentials_placement: body\n  auto_fetch_token: true\n}\n";
        let mut request = parse(original);
        request.auth.values[2] = "new".into();
        let saved = update(original, &request, BodyKind::None);
        assert_eq!(
            saved,
            original.replace("client_secret: old", "client_secret: new")
        );
    }

    #[test]
    fn an_api_key_set_over_a_query_one_goes_in_a_header() {
        let original = "get {\n  url: https://x\n  auth: apikey\n}\n\nauth:apikey {\n  key: api_key\n  value: k\n  placement: queryparams\n}\n";
        let mut request = parse(original);
        request.auth = auth(AuthKind::ApiKey, &["X-Api-Key", "k2"]);
        let saved = update(original, &request, BodyKind::None);
        assert!(saved.contains("  placement: header"), "{saved}");
        assert!(!saved.contains("queryparams"), "{saved}");
        assert_eq!(parse(&saved).auth, request.auth);
    }

    #[test]
    fn a_fresh_file_carries_its_auth() {
        let request = Request {
            method: "GET".into(),
            url: "https://api/x".into(),
            auth: auth(AuthKind::ApiKey, &["X-Api-Key", "{{KEY}}"]),
            ..Request::default()
        };
        let text = format(&request, BodyKind::None);
        assert!(text.contains("  auth: apikey"), "{text}");
        assert_eq!(parse(&text).auth, request.auth);
    }
}
