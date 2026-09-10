//! Every environment a request can be sent with.
//!
//! Three ecosystems' formats share one selector: `.env` files, Bruno's
//! `environments/*.bru`, and Postman's `*.postman_environment.json`. Each is
//! found by walking up from the request to the collection root.

use std::path::{Path, PathBuf};

use crate::dotenv;
use crate::error::Result;
use crate::formats::{bru, postman};
use crate::vars::Vars;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvKind {
    Dotenv,
    Bruno,
    Postman,
}

impl EnvKind {
    pub fn label(self) -> &'static str {
        match self {
            EnvKind::Dotenv => ".env",
            EnvKind::Bruno => "Bruno",
            EnvKind::Postman => "Postman",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvSource {
    pub label: String,
    pub path: PathBuf,
    pub kind: EnvKind,
}

impl EnvSource {
    /// Read fresh from disk each time: the file may have been edited since the
    /// request was opened, in binman or out of it.
    pub fn load(&self) -> Result<Vars> {
        match self.kind {
            EnvKind::Dotenv => dotenv::load(&self.path),
            EnvKind::Bruno => bru::load_environment(&self.path),
            EnvKind::Postman => postman::load_environment(&self.path),
        }
    }
}

/// The environments above a request in `dir`: `.env` files first, then
/// Bruno's, then Postman's — v1's order.
pub fn discover(dir: &Path, root: &Path) -> Vec<EnvSource> {
    let dotenv = dotenv::find(dir, root).into_iter().map(|file| EnvSource {
        label: file.label,
        path: file.path,
        kind: EnvKind::Dotenv,
    });
    let bruno = bru::environments(dir, root)
        .into_iter()
        .map(|(label, path)| EnvSource {
            label,
            path,
            kind: EnvKind::Bruno,
        });
    let postman = postman::find_environments(dir, root)
        .into_iter()
        .map(|(label, path)| EnvSource {
            label,
            path,
            kind: EnvKind::Postman,
        });
    dotenv.chain(bruno).chain(postman).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn three_formats_share_one_list() {
        let root = testing::scratch("env-discover");
        let requests = root.join("api");
        testing::write(&root.join(".env"), "BASE=https://dotenv");
        testing::write(
            &root.join("environments").join("dev.bru"),
            "vars {\n  BASE: https://bruno\n}\n",
        );
        testing::write(
            &root.join("prod.postman_environment.json"),
            r#"{"name":"Prod","values":[{"key":"BASE","value":"https://postman"}]}"#,
        );
        std::fs::create_dir_all(&requests).unwrap();

        let sources = discover(&requests, &root);
        let kinds: Vec<(EnvKind, &str)> = sources
            .iter()
            .map(|source| (source.kind, source.label.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (EnvKind::Dotenv, "default"),
                (EnvKind::Bruno, "dev"),
                (EnvKind::Postman, "prod"),
            ]
        );
        assert_eq!(sources[1].load().unwrap()["BASE"], "https://bruno");
        assert_eq!(sources[2].load().unwrap()["BASE"], "https://postman");
    }
}
