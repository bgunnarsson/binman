//! One request, however it was written down.

use crate::body::BodyKind;
use crate::vars::Vars;

/// The methods binman offers, in the order ↑ and ↓ step through them.
pub const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub url: String,
    /// In the order the file gives them. A header may repeat; nothing here
    /// collapses duplicates, since a server may read every one it is sent.
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// The body's shape when the file says so outright, as a Bruno
    /// `body:json` block does. `None` leaves it to the `Content-Type`.
    pub kind: Option<BodyKind>,
    /// Variables the file declares for itself: Bruno's `vars` and
    /// `vars:pre-request`. They outrank the environment.
    pub vars: Vars,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        header(&self.headers, name)
    }

    /// The body kind the request declares, or the one its headers imply.
    pub fn body_kind(&self) -> BodyKind {
        self.kind
            .unwrap_or_else(|| BodyKind::detect(self.header("Content-Type"), &self.body))
    }
}

/// Whether `text` names one of the methods binman offers, in any case.
pub fn is_method(text: &str) -> bool {
    METHODS.iter().any(|method| method.eq_ignore_ascii_case(text))
}

/// A header's value. Header names are case-insensitive on the wire, so they
/// are here.
pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Replaces a header wherever it is, whatever case it was written in, or
/// appends it when there is none.
pub fn set_header(headers: &mut Vec<(String, String)>, name: &str, value: impl Into<String>) {
    let value = value.into();
    match headers
        .iter_mut()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
    {
        Some(existing) => existing.1 = value,
        None => headers.push((name.to_string(), value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_are_found_in_any_case() {
        let headers = vec![("content-type".to_string(), "text/plain".to_string())];
        assert_eq!(header(&headers, "Content-Type"), Some("text/plain"));
    }

    #[test]
    fn setting_a_header_replaces_rather_than_duplicates() {
        let mut headers = vec![("authorization".to_string(), "old".to_string())];
        set_header(&mut headers, "Authorization", "new");
        assert_eq!(headers, vec![("authorization".into(), "new".into())]);
    }
}
