//! The collection formats binman reads.
//!
//! Three hold one request per file — `.http`, Bruno's `.bru` and `.graphql` —
//! and two hold many: Postman collections and OpenAPI specs. Each is read into
//! the same [`Request`], so nothing above this module knows which one a request
//! came from.

pub mod bru;
pub mod curl;
pub mod graphql;
pub mod http;
pub mod openapi;
pub mod postman;

use std::path::Path;

use crate::error::{Error, Result};
use crate::request::Request;

/// The formats that hold one request per file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Http,
    Bru,
    Graphql,
}

impl Format {
    pub fn of(path: &Path) -> Option<Format> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "http" => Some(Format::Http),
            "bru" => Some(Format::Bru),
            "graphql" => Some(Format::Graphql),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Http => ".http",
            Format::Bru => "Bruno",
            Format::Graphql => "GraphQL",
        }
    }

    pub fn parse(self, text: &str) -> Request {
        match self {
            Format::Http => http::parse(text),
            Format::Bru => bru::parse(text),
            Format::Graphql => graphql::parse(text),
        }
    }
}

pub fn load(path: &Path) -> Result<Request> {
    let format = Format::of(path)
        .ok_or_else(|| Error::parse(path.display(), "not a request file binman reads"))?;
    let text = std::fs::read_to_string(path).map_err(|error| Error::parse(path.display(), error))?;
    Ok(format.parse(&text))
}
