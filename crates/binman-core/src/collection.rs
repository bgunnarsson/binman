//! What sits in the collections directory, for the sidebar and for search.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::formats::{Format, http, openapi, postman};
use crate::request::is_method;
use crate::source::Origin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    Request(Format),
    Postman,
    OpenApi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
}

/// A directory as the sidebar shows it: directories first, then everything
/// binman can open, each alphabetically. Hidden files are left out, and so are
/// the files Bruno keeps beside its requests — `collection.bru`, `folder.bru`,
/// and the `environments/` directory at a collection's root.
pub fn list(dir: &Path) -> Result<Vec<Entry>> {
    let found: Vec<(String, PathBuf)> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| Some((entry.file_name().into_string().ok()?, entry.path())))
        .collect();
    let bruno_root = found
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("collection.bru"));

    let mut entries = Vec::new();
    for (name, path) in found {
        if name.starts_with('.') {
            continue;
        }
        // Follows a symlink, so a collection linked in from elsewhere opens
        // like any other.
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        let kind = if metadata.is_dir() {
            if bruno_root && lower == "environments" {
                continue;
            }
            EntryKind::Dir
        } else if let Some(format) = Format::of(&path) {
            if lower == "collection.bru" || lower == "folder.bru" {
                continue;
            }
            EntryKind::Request(format)
        } else if postman::is_collection(&name) {
            EntryKind::Postman
        } else if is_spec_file(&path, &lower) {
            EntryKind::OpenApi
        } else {
            continue;
        };
        entries.push(Entry { name, path, kind });
    }

    entries.sort_by(|a, b| {
        (a.kind != EntryKind::Dir)
            .cmp(&(b.kind != EntryKind::Dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

fn is_spec_file(path: &Path, lower_name: &str) -> bool {
    let candidate = [".json", ".yaml", ".yml"]
        .iter()
        .any(|extension| lower_name.ends_with(extension));
    candidate && peek(path, 1024).is_some_and(|bytes| openapi::is_spec(&bytes, lower_name))
}

fn peek(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let mut buffer = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut buffer)
        .ok()?;
    Some(buffer)
}

/// The method a request file sends, read from its first lines — enough to
/// badge it in the sidebar without reading the whole file.
pub fn method_of(path: &Path, format: Format) -> Option<String> {
    match format {
        Format::Graphql => Some("POST".to_string()),
        Format::Http => {
            let text = String::from_utf8_lossy(&peek(path, 4096)?).into_owned();
            Some(http::parse(&text).method).filter(|method| is_method(method))
        }
        Format::Bru => {
            let text = String::from_utf8_lossy(&peek(path, 4096)?).into_owned();
            text.lines()
                .filter(|line| !line.starts_with(char::is_whitespace))
                .find_map(|line| {
                    let word = line.split_whitespace().next()?.trim_end_matches('{');
                    is_method(word).then(|| word.to_ascii_uppercase())
                })
        }
    }
}

/// One request anywhere under the root, for the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub method: String,
    pub title: String,
    /// Where it sits: the directory, or the collection and its folders.
    pub location: String,
    pub origin: Origin,
}

/// Every request under `root`, including each one inside a Postman
/// collection or an OpenAPI spec.
pub fn index(root: &Path) -> Vec<Found> {
    let mut out = Vec::new();
    walk(root, root, &mut HashSet::new(), &mut out);
    out.sort_by(|a, b| {
        a.location
            .cmp(&b.location)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    out
}

fn walk(root: &Path, dir: &Path, visited: &mut HashSet<PathBuf>, out: &mut Vec<Found>) {
    // A directory reached twice through symlinks is walked once, so a link
    // back up the tree cannot loop.
    let Ok(canonical) = dir.canonicalize() else {
        return;
    };
    if !visited.insert(canonical) {
        return;
    }
    let Ok(entries) = list(dir) else {
        return;
    };

    for entry in entries {
        match entry.kind {
            EntryKind::Dir => walk(root, &entry.path, visited, out),
            EntryKind::Request(format) => out.push(Found {
                method: method_of(&entry.path, format).unwrap_or_default(),
                title: entry.name.clone(),
                location: relative(root, dir),
                origin: Origin::File(entry.path.clone()),
            }),
            EntryKind::Postman => {
                let Ok(collection) = std::fs::read(&entry.path)
                    .map_err(crate::error::Error::from)
                    .and_then(|bytes| postman::parse(&bytes))
                else {
                    continue;
                };
                let location = relative(root, &entry.path);
                postman_items(
                    &collection.items,
                    &entry.path,
                    &location,
                    &mut Vec::new(),
                    out,
                );
            }
            EntryKind::OpenApi => {
                let Ok(spec) = std::fs::read(&entry.path)
                    .map_err(crate::error::Error::from)
                    .and_then(|bytes| openapi::parse(&bytes, &entry.name))
                else {
                    continue;
                };
                let location = relative(root, &entry.path);
                for group in openapi::groups(&spec) {
                    for endpoint in group.endpoints {
                        out.push(Found {
                            method: endpoint.method.clone(),
                            title: endpoint.route.clone(),
                            location: format!("{location} › {}", group.tag),
                            origin: Origin::OpenApi {
                                path: entry.path.clone(),
                                route: endpoint.route,
                                method: endpoint.method,
                            },
                        });
                    }
                }
            }
        }
    }
}

fn postman_items(
    items: &[postman::Item],
    path: &Path,
    location: &str,
    trail: &mut Vec<usize>,
    out: &mut Vec<Found>,
) {
    for (index, item) in items.iter().enumerate() {
        trail.push(index);
        if item.is_request() {
            out.push(Found {
                method: item.method(),
                title: item.name.clone(),
                location: location.to_string(),
                origin: Origin::Postman {
                    path: path.to_path_buf(),
                    item: trail.clone(),
                },
            });
        } else {
            let nested = format!("{location} › {}", item.name);
            postman_items(&item.items, path, &nested, trail, out);
        }
        trail.pop();
    }
}

/// `path` as it sits under `root`, for display.
pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    fn collection() -> PathBuf {
        let root = testing::scratch("collection");
        testing::write(
            &root.join("users").join("list.http"),
            "# list\nGET https://x/users\n",
        );
        testing::write(
            &root.join("users").join("create.bru"),
            "meta {\n  name: create\n}\n\npost {\n  url: https://x/users\n}\n",
        );
        testing::write(&root.join("query.graphql"), "query { me { id } }");
        testing::write(&root.join("notes.txt"), "not a request");
        testing::write(&root.join(".hidden.http"), "GET https://x");
        testing::write(
            &root.join("openapi.yaml"),
            "openapi: 3.0.0\npaths:\n  /pets:\n    get:\n      tags: [pets]\n",
        );
        testing::write(&root.join("config.json"), "{\"name\": \"not a spec\"}");
        testing::write(
            &root.join("api.postman_collection.json"),
            r#"{"item":[{"name":"Auth","item":[{"name":"Login","request":{"method":"POST","url":"https://x/login"}}]}]}"#,
        );
        testing::write(
            &root.join("bruno").join("collection.bru"),
            "vars {\n  a: 1\n}\n",
        );
        testing::write(
            &root.join("bruno").join("environments").join("dev.bru"),
            "vars {\n  a: 2\n}\n",
        );
        testing::write(
            &root.join("bruno").join("ping.bru"),
            "get {\n  url: https://x/ping\n}\n",
        );
        root
    }

    #[test]
    fn lists_what_binman_can_open_directories_first() {
        let root = collection();
        let names: Vec<String> = list(&root)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(
            names,
            vec![
                "bruno",
                "users",
                "api.postman_collection.json",
                "openapi.yaml",
                "query.graphql"
            ]
        );
    }

    #[test]
    fn hides_what_bruno_keeps_beside_its_requests() {
        let root = collection();
        let names: Vec<String> = list(&root.join("bruno"))
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec!["ping.bru"]);
    }

    #[test]
    fn reads_a_method_without_parsing_the_file() {
        let root = collection();
        assert_eq!(
            method_of(&root.join("users/list.http"), Format::Http).as_deref(),
            Some("GET")
        );
        assert_eq!(
            method_of(&root.join("users/create.bru"), Format::Bru).as_deref(),
            Some("POST")
        );
    }

    #[test]
    fn indexes_every_request_including_those_inside_collections() {
        let root = collection();
        let found: Vec<(String, String, String)> = index(&root)
            .into_iter()
            .map(|found| (found.method, found.title, found.location))
            .collect();
        let expected = [
            ("POST", "query.graphql", ""),
            ("POST", "Login", "api.postman_collection.json › Auth"),
            ("GET", "ping.bru", "bruno"),
            ("GET", "/pets", "openapi.yaml › pets"),
            ("POST", "create.bru", "users"),
            ("GET", "list.http", "users"),
        ]
        .map(|(method, title, location)| {
            (method.to_string(), title.to_string(), location.to_string())
        });
        assert_eq!(found, expected);
    }
}
