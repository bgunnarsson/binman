//! `.graphql` files, posted as the JSON document GraphQL servers expect.
//!
//! A file may be the operation alone, or the operation between a header and a
//! variables block:
//!
//! ```text
//! # URL: https://api.example.com/graphql
//! # Header: Authorization: Bearer {{TOKEN}}
//! ---
//! query UserById($id: ID!) {
//!   user(id: $id) { name email }
//! }
//! ---
//! { "id": "42" }
//! ```

use serde_json::Value;

use crate::body::BodyKind;
use crate::request::{Request, set_header};

pub fn parse(text: &str) -> Request {
    let text = text.replace("\r\n", "\n");
    let parts: Vec<&str> = text.split("\n---\n").collect();
    let (meta, query, variables) = match parts.as_slice() {
        [] => (None, "", None),
        [query] => (None, *query, None),
        [meta, query] => (Some(*meta), *query, None),
        [meta, query, rest @ ..] => (Some(*meta), *query, Some(rest.join("\n---\n"))),
    };

    let mut request = Request {
        method: "POST".into(),
        headers: vec![("Content-Type".into(), "application/json".into())],
        kind: Some(BodyKind::Json),
        ..Request::default()
    };

    for line in meta.into_iter().flat_map(str::lines) {
        let line = line.trim().trim_start_matches('#').trim();
        if let Some(url) = strip_prefix_ignoring_case(line, "url:") {
            request.url = url.trim().to_string();
        } else if let Some(header) = strip_prefix_ignoring_case(line, "header:")
            && let Some((name, value)) = header.split_once(':')
            && !name.trim().is_empty()
        {
            set_header(&mut request.headers, name.trim(), value.trim());
        }
    }

    let mut payload = serde_json::Map::new();
    payload.insert("query".into(), Value::String(query.trim().to_string()));
    if let Some(variables) = variables
        .as_deref()
        .map(str::trim)
        .filter(|variables| !variables.is_empty())
        .and_then(|variables| serde_json::from_str::<Value>(variables).ok())
    {
        payload.insert("variables".into(), variables);
    }
    request.body = Value::Object(payload).to_string();
    request
}

fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_operation_alone_is_posted_as_json() {
        let request = parse("query { me { id } }");
        assert_eq!(request.method, "POST");
        assert_eq!(request.header("Content-Type"), Some("application/json"));
        let payload: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(payload["query"], "query { me { id } }");
    }

    #[test]
    fn reads_the_header_and_the_variables() {
        let request = parse(
            "# URL: https://api.example.com/graphql\n\
             # Header: Authorization: Bearer abc\n\
             ---\n\
             query UserById($id: ID!) {\n  user(id: $id) { name }\n}\n\
             ---\n\
             {\"id\": \"42\"}\n",
        );
        assert_eq!(request.url, "https://api.example.com/graphql");
        assert_eq!(request.header("Authorization"), Some("Bearer abc"));
        let payload: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(payload["variables"]["id"], "42");
    }
}
