//! Where a request came from, and reading it back from there.

use std::path::Path;

use crate::body::BodyKind;
use crate::error::{Error, Result};
use crate::formats::{self, Format, bru, http, openapi, postman};
use crate::request::Request;
use crate::vars::Vars;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A `.http`, `.bru` or `.graphql` file.
    File(std::path::PathBuf),
    /// One request inside a Postman collection, by its position in the tree.
    Postman {
        path: std::path::PathBuf,
        item: Vec<usize>,
    },
    /// One operation in an OpenAPI spec.
    OpenApi {
        path: std::path::PathBuf,
        route: String,
        method: String,
    },
}

/// A request as read, with the variables its collection declares.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub request: Request,
    pub collection_vars: Vars,
    pub title: String,
}

impl Origin {
    /// The file on disk.
    pub fn path(&self) -> &Path {
        match self {
            Origin::File(path) | Origin::Postman { path, .. } | Origin::OpenApi { path, .. } => path,
        }
    }

    /// Where the search for environments starts.
    pub fn dir(&self) -> &Path {
        self.path().parent().unwrap_or(Path::new(""))
    }

    /// Reads the request fresh from disk. `root` bounds the search for
    /// Bruno's `collection.bru` and `folder.bru`.
    pub fn load(&self, root: &Path) -> Result<Loaded> {
        match self {
            Origin::File(path) => {
                let request = formats::load(path)?;
                let collection_vars = if Format::of(path) == Some(Format::Bru) {
                    bru::collection_vars(self.dir(), root)
                } else {
                    Vars::new()
                };
                Ok(Loaded {
                    request,
                    collection_vars,
                    title: file_name(path),
                })
            }
            Origin::Postman { path, item } => {
                let collection = postman::parse(&read(path)?)?;
                let request = collection.request_at(item)?;
                let title = collection
                    .item_at(item)
                    .map(|item| item.name.clone())
                    .unwrap_or_default();
                Ok(Loaded {
                    request,
                    collection_vars: collection.vars(),
                    title,
                })
            }
            Origin::OpenApi {
                path,
                route,
                method,
            } => {
                let spec = openapi::parse(&read(path)?, &file_name(path))?;
                Ok(Loaded {
                    request: openapi::request(&spec, route, method),
                    collection_vars: Vars::new(),
                    title: format!("{method} {route}"),
                })
            }
        }
    }

    /// The format a request saves back to, when it can be saved at all.
    pub fn savable(&self) -> Option<Format> {
        match self {
            Origin::File(path) => Format::of(path).filter(|format| *format != Format::Graphql),
            _ => None,
        }
    }

    /// Writes `request` back to where it came from. A `.http` file is written
    /// whole; a `.bru` file is updated in place, so the blocks binman does not
    /// edit survive.
    pub fn save(&self, request: &Request, kind: BodyKind) -> Result<()> {
        match (self, self.savable()) {
            (Origin::File(path), Some(Format::Http)) => {
                std::fs::write(path, http::format(request))?;
            }
            (Origin::File(path), Some(Format::Bru)) => {
                let text = match std::fs::read_to_string(path) {
                    Ok(original) => bru::update(&original, request, kind),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        bru::format(request, kind)
                    }
                    Err(error) => return Err(error.into()),
                };
                std::fs::write(path, text)?;
            }
            _ => {
                return Err(Error::Invalid(format!(
                    "{} can't be saved back — only .http and .bru files can",
                    self.describe()
                )));
            }
        }
        Ok(())
    }

    fn describe(&self) -> &'static str {
        match self {
            Origin::File(_) => "A .graphql file",
            Origin::Postman { .. } => "A Postman collection",
            Origin::OpenApi { .. } => "An OpenAPI spec",
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|error| Error::parse(path.display(), error))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn a_bru_request_brings_its_collection_vars() {
        let root = testing::scratch("source-bru");
        testing::write(&root.join("collection.bru"), "vars {\n  base: https://api\n}\n");
        let path = root.join("users").join("get.bru");
        testing::write(&path, "get {\n  url: {{base}}/users\n}\n");

        let loaded = Origin::File(path).load(&root).expect("loads");
        assert_eq!(loaded.request.url, "{{base}}/users");
        assert_eq!(loaded.collection_vars["base"], "https://api");
        assert_eq!(loaded.title, "get.bru");
    }

    #[test]
    fn only_http_and_bru_save_back() {
        let root = testing::scratch("source-save");
        let path = root.join("q.graphql");
        testing::write(&path, "query { a }");
        let error = Origin::File(path)
            .save(&Request::default(), BodyKind::Json)
            .unwrap_err()
            .to_string();
        assert!(error.contains(".graphql"), "{error}");

        let http = root.join("new.http");
        let request = Request {
            method: "GET".into(),
            url: "https://x".into(),
            ..Request::default()
        };
        Origin::File(http.clone()).save(&request, BodyKind::None).expect("saves");
        assert_eq!(std::fs::read_to_string(http).unwrap(), "GET https://x\n");
    }
}
