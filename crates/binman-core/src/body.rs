//! What a request body is, and how each kind goes on the wire.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::vars;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyKind {
    None,
    Json,
    Form,
    Multipart,
    Xml,
    Text,
    Sparql,
}

impl BodyKind {
    /// The order ← and → step through them.
    pub const ALL: [BodyKind; 7] = [
        BodyKind::None,
        BodyKind::Json,
        BodyKind::Form,
        BodyKind::Multipart,
        BodyKind::Xml,
        BodyKind::Text,
        BodyKind::Sparql,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BodyKind::None => "No body",
            BodyKind::Json => "JSON",
            BodyKind::Form => "Form URL-encoded",
            BodyKind::Multipart => "Multipart form",
            BodyKind::Xml => "XML",
            BodyKind::Text => "Text",
            BodyKind::Sparql => "SPARQL",
        }
    }

    /// What the body is sent as when the request does not say itself.
    /// Multipart's real value carries a boundary and is only known once the
    /// body is encoded.
    pub fn content_type(self) -> Option<&'static str> {
        match self {
            BodyKind::None => None,
            BodyKind::Json => Some("application/json"),
            BodyKind::Form => Some("application/x-www-form-urlencoded"),
            BodyKind::Multipart => Some("multipart/form-data"),
            BodyKind::Xml => Some("application/xml"),
            BodyKind::Text => Some("text/plain"),
            BodyKind::Sparql => Some("application/sparql-query"),
        }
    }

    /// Edited as a table of fields rather than as text.
    pub fn is_form(self) -> bool {
        matches!(self, BodyKind::Form | BodyKind::Multipart)
    }

    /// v1's rule: the `Content-Type` decides, and a body that arrives without
    /// one is taken for JSON, the commonest thing to send.
    pub fn detect(content_type: Option<&str>, body: &str) -> BodyKind {
        let essence = content_type
            .and_then(|value| value.split(';').next())
            .map(|value| value.trim().to_ascii_lowercase());
        match essence.as_deref() {
            Some("application/json") => BodyKind::Json,
            Some("application/xml" | "text/xml") => BodyKind::Xml,
            Some("text/plain") => BodyKind::Text,
            Some("application/sparql-query") => BodyKind::Sparql,
            Some("application/x-www-form-urlencoded") => BodyKind::Form,
            Some("multipart/form-data") => BodyKind::Multipart,
            _ if !body.is_empty() => BodyKind::Json,
            _ => BodyKind::None,
        }
    }

    pub fn cycle(self, delta: isize) -> BodyKind {
        let position = Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0) as isize;
        let next = (position + delta).rem_euclid(Self::ALL.len() as isize);
        Self::ALL[next as usize]
    }
}

/// Splits `a=1&b=two` into pairs, decoding `%XX` and `+` the way a form does.
pub fn parse_form(body: &str) -> Vec<(String, String)> {
    url::form_urlencoded::parse(body.trim().as_bytes())
        .into_owned()
        .collect()
}

/// Joins pairs back into a form body. `{{placeholders}}` are left as they are
/// rather than percent-encoded, or they would never resolve again.
pub fn encode_form(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .filter(|(name, _)| !name.is_empty())
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// One side of a form pair, percent-encoded except for its placeholders.
fn encode(text: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for (range, _) in vars::placeholders(text) {
        out.extend(url::form_urlencoded::byte_serialize(
            &text.as_bytes()[last..range.start],
        ));
        out.push_str(&text[range.clone()]);
        last = range.end;
    }
    out.extend(url::form_urlencoded::byte_serialize(&text.as_bytes()[last..]));
    out
}

pub struct Multipart {
    pub body: Vec<u8>,
    /// Carries the boundary, so it replaces whatever `Content-Type` the
    /// request had: a multipart body is unreadable without it.
    pub content_type: String,
}

/// Encodes form fields as `multipart/form-data`. A value that starts with `@`
/// names a file to upload; a relative one is read from `base`, the directory
/// the request came from, rather than from wherever binman was started.
pub fn multipart(fields: &[(String, String)], base: Option<&Path>) -> Result<Multipart> {
    let boundary = boundary()?;
    let mut body = Vec::new();

    for (name, value) in fields.iter().filter(|(name, _)| !name.is_empty()) {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match value.strip_prefix('@') {
            Some(file) => {
                let path = match base {
                    Some(base) => base.join(file),
                    None => PathBuf::from(file),
                };
                let contents = std::fs::read(&path).map_err(|error| {
                    Error::Invalid(format!(
                        "reading {} for the {name} field: {error}",
                        path.display()
                    ))
                })?;
                let filename = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n\
                         Content-Type: application/octet-stream\r\n\r\n",
                        quote(name),
                        quote(&filename)
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(&contents);
            }
            None => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                        quote(name)
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(value.as_bytes());
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    Ok(Multipart {
        body,
        content_type: format!("multipart/form-data; boundary={boundary}"),
    })
}

fn quote(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn boundary() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        Error::Invalid(format!("no randomness for a multipart boundary: {error}"))
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn the_content_type_decides_and_a_bare_body_is_json() {
        assert_eq!(
            BodyKind::detect(Some("application/json; charset=utf-8"), ""),
            BodyKind::Json
        );
        assert_eq!(BodyKind::detect(Some("text/xml"), "<a/>"), BodyKind::Xml);
        assert_eq!(BodyKind::detect(None, "{}"), BodyKind::Json);
        assert_eq!(BodyKind::detect(None, ""), BodyKind::None);
    }

    #[test]
    fn kinds_cycle_round() {
        assert_eq!(BodyKind::None.cycle(-1), BodyKind::Sparql);
        assert_eq!(BodyKind::Sparql.cycle(1), BodyKind::None);
    }

    #[test]
    fn a_form_body_decodes_to_pairs() {
        assert_eq!(
            parse_form("name=Jane+Doe&city=Reykjav%C3%ADk"),
            vec![
                ("name".to_string(), "Jane Doe".to_string()),
                ("city".to_string(), "Reykjavík".to_string())
            ]
        );
    }

    #[test]
    fn encoding_a_form_leaves_placeholders_alone() {
        let body = encode_form(&[
            ("token".into(), "{{TOKEN}}".into()),
            ("q".into(), "a b&c".into()),
        ]);
        assert_eq!(body, "token={{TOKEN}}&q=a+b%26c");
    }

    #[test]
    fn multipart_carries_fields_and_files() {
        let dir = testing::scratch("multipart");
        testing::write(&dir.join("data.txt"), "hello world");

        let encoded = multipart(
            &[
                ("name".into(), "alice".into()),
                ("upload".into(), "@data.txt".into()),
            ],
            Some(&dir),
        )
        .expect("encodes");

        let boundary = encoded
            .content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("the content type names its boundary");
        let body = String::from_utf8(encoded.body).expect("text");
        assert!(body.contains("name=\"name\"\r\n\r\nalice\r\n"), "{body}");
        assert!(body.contains("filename=\"data.txt\""), "{body}");
        assert!(body.contains("hello world"), "{body}");
        assert!(body.ends_with(&format!("--{boundary}--\r\n")), "{body}");
    }

    #[test]
    fn a_missing_upload_names_the_file() {
        let error = multipart(&[("upload".into(), "@/no/such/file".into())], None)
            .err()
            .expect("fails")
            .to_string();
        assert!(error.contains("/no/such/file"), "{error}");
    }
}
