//! The collections in play, which are rarely just one directory's worth.
//!
//! DataGrip's model, as binsql has it for data sources: one list, stacked from
//! layers that each have their own reader.
//!
//! - `HTTP_FILES` in `~/.config/binman/config` — v1's one location, still read
//!   and listed as a collection of its own, and never written;
//! - **yours**, `~/.config/binman/collections.json`, listed in every project;
//! - a **project's**, the nearest `.binman.json` at or above the working
//!   directory. Its paths are taken from where the file sits, so a repository
//!   commits it and every clone lists the same collections;
//! - anything named on the **command line**, which is for this run and is
//!   never written anywhere.
//!
//! Later layers win per name rather than per file, as binsql's do, so a
//! project's `api` stands in for yours of the same name while the rest of
//! yours stay listed beside it.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::collection;
use crate::config::Config;
use crate::error::{Error, Result};

/// The file a project's collections live in.
pub const PROJECT_FILE: &str = ".binman.json";

/// Yours, beside the config.
const USER_FILE: &str = "collections.json";

/// The file a collection is saved in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The project's `.binman.json`.
    Project,
    /// `~/.config/binman/collections.json`.
    User,
}

/// Where a collection in the list came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `HTTP_FILES` in the config.
    Config,
    User,
    Project,
    /// Named on the command line.
    Argument,
}

impl Source {
    /// The file it is saved in, when it is saved in one of binman's.
    pub fn scope(self) -> Option<Scope> {
        match self {
            Source::User => Some(Scope::User),
            Source::Project => Some(Scope::Project),
            Source::Config | Source::Argument => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub name: String,
    /// Absolute, and canonical when it is there to be canonicalized.
    pub path: PathBuf,
    pub source: Source,
}

impl Collection {
    /// A directory of requests, rather than one file that holds them. Only a
    /// directory can have a new request saved into it.
    pub fn is_dir(&self) -> bool {
        self.path.is_dir()
    }

    /// Where the environments for its requests are looked for up to: the
    /// directory itself, or the one its file sits in.
    pub fn dir(&self) -> &Path {
        if self.is_dir() {
            &self.path
        } else {
            self.path.parent().unwrap_or(&self.path)
        }
    }
}

/// One file's collections: each name, and its path as the file writes it.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Layer {
    #[serde(default)]
    collections: BTreeMap<String, String>,
}

pub struct Workspace {
    user: Layer,
    user_path: PathBuf,
    /// `HTTP_FILES`, as the config gives it.
    legacy: Option<PathBuf>,
    project: Layer,
    /// The project file: the nearest there is, or — inside a repository that
    /// has none — where one would go at its root, so that the first
    /// collection saved to it creates it.
    project_path: Option<PathBuf>,
    arguments: Vec<Collection>,
    /// Every layer flattened, rebuilt on every change. Every read goes
    /// through here.
    merged: Vec<Collection>,
}

impl Workspace {
    /// Reads your collections, the config's `HTTP_FILES`, and whatever
    /// project the working directory sits in.
    pub fn load(config: &Config) -> Result<Workspace> {
        let here = std::env::current_dir()?;
        Workspace::load_at(user_path(), config.root.clone(), project_path(&here))
    }

    /// The same, over paths the caller already knows. A file that is not
    /// there reads as an empty one, so a project file yet to be written can
    /// still be saved into.
    pub fn load_at(
        user_path: PathBuf,
        legacy: Option<PathBuf>,
        project_path: Option<PathBuf>,
    ) -> Result<Workspace> {
        let mut workspace = Workspace {
            user: Layer::default(),
            user_path,
            legacy,
            project: Layer::default(),
            project_path,
            arguments: Vec::new(),
            merged: Vec::new(),
        };
        workspace.reload()?;
        Ok(workspace)
    }

    /// Reads both files again — a `.binman.json` that a pull has changed, say
    /// — and keeps what the command line added.
    pub fn reload(&mut self) -> Result<()> {
        self.user = read(&self.user_path)?;
        self.project = match &self.project_path {
            Some(path) => read(path)?,
            None => Layer::default(),
        };
        self.rebuild();
        Ok(())
    }

    fn rebuild(&mut self) {
        let mut merged: Vec<Collection> = Vec::new();
        let mut put = |collection: Collection| match merged
            .iter_mut()
            .find(|held| held.name == collection.name)
        {
            Some(held) => *held = collection,
            None => merged.push(collection),
        };

        // Relative, it is taken from where binman starts, as v1 took it.
        if let Some(root) = &self.legacy {
            let path = settle(std::path::absolute(root).unwrap_or_else(|_| root.clone()));
            put(Collection {
                name: name_for(&path),
                path,
                source: Source::Config,
            });
        }
        let user_base = base_of(&self.user_path);
        for (name, raw) in &self.user.collections {
            put(Collection {
                name: name.clone(),
                path: resolve(&user_base, raw),
                source: Source::User,
            });
        }
        if let Some(project_path) = &self.project_path {
            let project_base = base_of(project_path);
            for (name, raw) in &self.project.collections {
                put(Collection {
                    name: name.clone(),
                    path: resolve(&project_base, raw),
                    source: Source::Project,
                });
            }
        }
        for argument in &self.arguments {
            put(argument.clone());
        }

        merged.sort_by_key(|collection| collection.name.to_lowercase());
        self.merged = merged;
    }

    pub fn collections(&self) -> &[Collection] {
        &self.merged
    }

    pub fn get(&self, name: &str) -> Option<&Collection> {
        self.merged
            .iter()
            .find(|collection| collection.name == name)
    }

    /// The collection a path is in: the innermost, when one is registered
    /// inside another.
    pub fn containing(&self, path: &Path) -> Option<&Collection> {
        self.merged
            .iter()
            .filter(|collection| path.starts_with(&collection.path))
            .max_by_key(|collection| collection.path.components().count())
    }

    /// How far up from a request file its environments are looked for: to
    /// its collection, or — for a file in none — its own directory.
    pub fn root_for(&self, path: &Path) -> PathBuf {
        match self.containing(path) {
            Some(collection) => collection.dir().to_path_buf(),
            None => path.parent().unwrap_or(path).to_path_buf(),
        }
    }

    /// The name a file goes by: its path under its collection, after the
    /// collection's name once there is more than one for it to be confused
    /// with.
    pub fn display(&self, path: &Path) -> String {
        let Some(collection) = self.containing(path) else {
            return path.display().to_string();
        };
        let rest = path.strip_prefix(&collection.path).unwrap_or(path);
        if rest.as_os_str().is_empty() {
            collection.name.clone()
        } else if self.merged.len() > 1 {
            format!("{}/{}", collection.name, portable(rest))
        } else {
            portable(rest)
        }
    }

    /// Where a path typed relative to the collections lands: under the
    /// collection its first part names, when that is a directory, and under
    /// `fallback` when it names none — which is how one written before there
    /// was more than one collection still lands where it did.
    pub fn locate(&self, typed: &Path, fallback: Option<&Collection>) -> Option<PathBuf> {
        let mut parts = typed.components();
        if let Some(Component::Normal(first)) = parts.next()
            && let Some(named) = self
                .merged
                .iter()
                .find(|collection| collection.is_dir() && first == OsStr::new(&collection.name))
            && !parts.as_path().as_os_str().is_empty()
        {
            return Some(named.path.join(parts.as_path()));
        }
        fallback
            .filter(|collection| collection.is_dir())
            .map(|collection| collection.path.join(typed))
    }

    pub fn user_path(&self) -> &Path {
        &self.user_path
    }

    /// The project file in play, whether or not it has been written yet.
    pub fn project_path(&self) -> Option<&Path> {
        self.project_path.as_deref()
    }

    /// Whether the project file is on disk, rather than only offered.
    pub fn project_exists(&self) -> bool {
        self.project_path.as_deref().is_some_and(Path::is_file)
    }

    /// Where a collection is saved when nothing says otherwise: back where it
    /// came from, and a new one into the project you are standing in.
    pub fn default_scope(&self, name: Option<&str>) -> Scope {
        match name
            .and_then(|name| self.get(name))
            .and_then(|collection| collection.source.scope())
        {
            Some(scope) => scope,
            None if self.project_path.is_some() => Scope::Project,
            None => Scope::User,
        }
    }

    /// Saves a collection in one file, and writes what changed.
    ///
    /// Whatever it was saved as before goes: a rename takes the old name with
    /// it, and one moved between files leaves no copy behind to shadow it or
    /// be shadowed by it.
    pub fn set(
        &mut self,
        previous: Option<&str>,
        name: &str,
        path: &Path,
        scope: Scope,
    ) -> Result<()> {
        check_name(name)?;
        if scope == Scope::Project && self.project_path.is_none() {
            return Err(Error::Config(format!(
                "there is no {PROJECT_FILE} here to save {name} in"
            )));
        }

        let (mut project_changed, mut user_changed) = (false, false);
        for old in [previous, Some(name)].into_iter().flatten() {
            project_changed |= self.project.collections.remove(old).is_some();
            user_changed |= self.user.collections.remove(old).is_some();
        }
        let written = self.written(path, scope);
        match scope {
            Scope::Project => {
                self.project.collections.insert(name.to_string(), written);
                project_changed = true;
            }
            Scope::User => {
                self.user.collections.insert(name.to_string(), written);
                user_changed = true;
            }
        }
        // Saved for good, it is no longer only for this run.
        self.arguments
            .retain(|argument| argument.name != name && Some(argument.name.as_str()) != previous);

        if project_changed {
            self.write(Scope::Project)?;
        }
        if user_changed {
            self.write(Scope::User)?;
        }
        self.rebuild();
        Ok(())
    }

    /// Takes a collection out of the list: out of the file that holds it, or
    /// out of this run when it came from the command line. The files it
    /// points at are not touched.
    pub fn remove(&mut self, name: &str) -> Result<()> {
        let Some(collection) = self.get(name) else {
            return Ok(());
        };
        match collection.source {
            // The config is v1's, written by hand, and not binman's to
            // rewrite.
            Source::Config => {
                return Err(Error::Config(format!(
                    "{name} is HTTP_FILES in {} — it goes when that line does",
                    Config::path().display()
                )));
            }
            Source::Argument => self.arguments.retain(|argument| argument.name != name),
            Source::Project => {
                self.project.collections.remove(name);
                self.write(Scope::Project)?;
            }
            Source::User => {
                self.user.collections.remove(name);
                self.write(Scope::User)?;
            }
        }
        self.rebuild();
        Ok(())
    }

    /// Lists a directory or a file for this run only: one named on the
    /// command line, which nobody asked to have saved.
    pub fn add_argument(&mut self, path: &Path) -> Result<()> {
        let path = path
            .canonicalize()
            .map_err(|error| Error::Config(format!("{}: {error}", path.display())))?;
        if !path.is_dir() && collection::entry(&path).is_none() {
            return Err(Error::Config(format!(
                "binman opens a directory of requests, a Postman collection, an OpenAPI spec or a request file — {} is none of them",
                path.display()
            )));
        }
        // Already listed, it is already there to open.
        if self.merged.iter().any(|collection| collection.path == path) {
            return Ok(());
        }
        self.arguments.push(Collection {
            name: name_for(&path),
            path,
            source: Source::Argument,
        });
        self.rebuild();
        Ok(())
    }

    /// A path as a file writes it: relative to the project file, so every
    /// clone reads it the same, or from your home in your own.
    fn written(&self, path: &Path, scope: Scope) -> String {
        let (base, prefix) = match scope {
            Scope::Project => (self.project_path.as_deref().map(base_of), ""),
            Scope::User => (dirs::home_dir(), "~/"),
        };
        match base
            .as_deref()
            .and_then(|base| path.strip_prefix(base).ok())
        {
            Some(rest) if rest.as_os_str().is_empty() && prefix.is_empty() => ".".to_string(),
            Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
            Some(rest) => format!("{prefix}{}", portable(rest)),
            None => path.display().to_string(),
        }
    }

    fn write(&self, scope: Scope) -> Result<()> {
        let (layer, path) = match (scope, &self.project_path) {
            (Scope::Project, Some(path)) => (&self.project, path),
            (Scope::Project, None) => return Ok(()),
            (Scope::User, _) => (&self.user, &self.user_path),
        };
        let failed = |error: &dyn std::fmt::Display| {
            Error::Config(format!("writing {}: {error}", path.display()))
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| failed(&error))?;
        }
        let mut json = serde_json::to_string_pretty(layer).map_err(|error| failed(&error))?;
        json.push('\n');
        // Through a temp file, so a failed write cannot leave half a list.
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, json).map_err(|error| failed(&error))?;
        std::fs::rename(&temp, path).map_err(|error| failed(&error))?;
        Ok(())
    }
}

/// What a collection is called when nobody has named it: its directory's
/// name, or its file's without the extension a Postman export or a spec
/// carries.
pub fn name_for(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    if path.is_dir() {
        return name;
    }
    let lower = name.to_ascii_lowercase();
    [".postman_collection.json", ".json", ".yaml", ".yml"]
        .iter()
        .find(|suffix| lower.ends_with(*suffix))
        .map_or_else(
            || name.clone(),
            |suffix| name[..name.len() - suffix.len()].to_string(),
        )
}

/// The project file for work in `from`: the nearest `.binman.json` at or
/// above it, looked for no higher than the repository it is in — one above
/// that belongs to something else — and, when there is none, where one would
/// go at that repository's root.
pub fn find_project(from: &Path) -> Option<PathBuf> {
    for dir in from.ancestors() {
        let candidate = dir.join(PROJECT_FILE);
        if candidate.is_file() || dir.join(".git").exists() {
            return Some(candidate);
        }
    }
    None
}

/// `BINMAN_PROJECT` if it is set, otherwise what the working directory finds.
fn project_path(here: &Path) -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("BINMAN_PROJECT") {
        // An explicit empty value turns discovery off, which is what a test or
        // a CI job wants when the checkout happens to hold one.
        return (!raw.is_empty()).then(|| PathBuf::from(raw));
    }
    find_project(here)
}

fn user_path() -> PathBuf {
    Config::path().with_file_name(USER_FILE)
}

fn read(path: &Path) -> Result<Layer> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Layer::default()),
        Err(error) => {
            return Err(Error::Config(format!(
                "reading {}: {error}",
                path.display()
            )));
        }
    };
    if text.trim().is_empty() {
        return Ok(Layer::default());
    }
    serde_json::from_str(&text).map_err(|error| Error::parse(path.display(), error))
}

fn check_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Config("A collection needs a name".into()));
    }
    // A request's name starts with its collection's, then a slash.
    if name.contains(['/', '\\']) {
        return Err(Error::Config(format!(
            "{name}: a collection's name can't hold a slash — a request's name starts with it"
        )));
    }
    Ok(())
}

/// The directory a file's relative paths are taken from.
fn base_of(file: &Path) -> PathBuf {
    settle(file.parent().unwrap_or(Path::new("")).to_path_buf())
}

/// A path as a file writes it, taken from `base`: `~` is your home, and a
/// relative path sits beside the file.
fn resolve(base: &Path, raw: &str) -> PathBuf {
    let path = match raw.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            let home = dirs::home_dir().unwrap_or_default();
            let rest = rest.trim_start_matches(['/', '\\']);
            if rest.is_empty() {
                home
            } else {
                home.join(rest)
            }
        }
        _ => PathBuf::from(raw),
    };
    settle(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}

/// Canonical when it can be, so it compares with the paths the tree lists; as
/// given when it is not there to canonicalize.
fn settle(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

/// Forward slashes whatever the platform, so a committed file reads the same
/// on every machine.
fn portable(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    struct Fixture {
        dir: PathBuf,
        user: PathBuf,
        project: PathBuf,
    }

    /// A home for the user file and a repository beside it, each with a
    /// directory of requests in it.
    fn fixture(label: &str) -> Fixture {
        let dir = testing::scratch(label).canonicalize().unwrap();
        let user = dir.join("config").join(USER_FILE);
        let project = dir.join("repo").join(PROJECT_FILE);
        testing::write(&dir.join("mine").join("ping.http"), "GET https://x\n");
        testing::write(
            &dir.join("repo").join("requests").join("list.http"),
            "GET https://x\n",
        );
        Fixture { dir, user, project }
    }

    fn names(workspace: &Workspace) -> Vec<(&str, Source)> {
        workspace
            .collections()
            .iter()
            .map(|collection| (collection.name.as_str(), collection.source))
            .collect()
    }

    #[test]
    fn a_project_joins_your_collections_rather_than_replacing_them() {
        let f = fixture("workspace-joins");
        testing::write(
            &f.user,
            &format!(
                r#"{{ "collections": {{ "mine": "{}" }} }}"#,
                f.dir.join("mine").display()
            ),
        );
        testing::write(&f.project, r#"{ "collections": { "api": "requests" } }"#);

        let workspace = Workspace::load_at(f.user, None, Some(f.project)).unwrap();
        assert_eq!(
            names(&workspace),
            vec![("api", Source::Project), ("mine", Source::User)]
        );
        assert_eq!(
            workspace.get("api").unwrap().path,
            f.dir.join("repo").join("requests"),
            "a project's paths are taken from where its file sits"
        );
    }

    #[test]
    fn the_project_wins_a_name_both_use() {
        let f = fixture("workspace-collision");
        testing::write(&f.user, r#"{ "collections": { "api": "/elsewhere" } }"#);
        testing::write(&f.project, r#"{ "collections": { "api": "requests" } }"#);

        let workspace = Workspace::load_at(f.user, None, Some(f.project)).unwrap();
        assert_eq!(names(&workspace), vec![("api", Source::Project)]);
    }

    #[test]
    fn http_files_is_still_a_collection_and_is_not_binmans_to_remove() {
        let f = fixture("workspace-legacy");
        let mut workspace = Workspace::load_at(f.user, Some(f.dir.join("mine")), None).unwrap();
        assert_eq!(names(&workspace), vec![("mine", Source::Config)]);

        let error = workspace.remove("mine").unwrap_err().to_string();
        assert!(error.contains("HTTP_FILES"), "{error}");
        assert_eq!(workspace.collections().len(), 1);
    }

    #[test]
    fn saving_writes_paths_each_file_can_read_back() {
        let f = fixture("workspace-writes");
        let mut workspace =
            Workspace::load_at(f.user.clone(), None, Some(f.project.clone())).unwrap();

        workspace
            .set(
                None,
                "api",
                &f.dir.join("repo").join("requests"),
                Scope::Project,
            )
            .unwrap();
        workspace
            .set(None, "mine", &f.dir.join("mine"), Scope::User)
            .unwrap();

        let project = std::fs::read_to_string(&f.project).unwrap();
        assert!(project.contains(r#""api": "requests""#), "{project}");

        let reread = Workspace::load_at(f.user, None, Some(f.project)).unwrap();
        assert_eq!(
            names(&reread),
            vec![("api", Source::Project), ("mine", Source::User)]
        );
        assert_eq!(reread.get("mine").unwrap().path, f.dir.join("mine"));
    }

    #[test]
    fn saving_to_the_other_file_moves_it_and_a_rename_takes_the_old_name() {
        let f = fixture("workspace-moves");
        let mut workspace =
            Workspace::load_at(f.user.clone(), None, Some(f.project.clone())).unwrap();
        let requests = f.dir.join("repo").join("requests");
        workspace
            .set(None, "api", &requests, Scope::Project)
            .unwrap();

        workspace
            .set(Some("api"), "backend", &requests, Scope::User)
            .unwrap();
        assert_eq!(names(&workspace), vec![("backend", Source::User)]);

        let project = std::fs::read_to_string(&f.project).unwrap();
        assert!(!project.contains("api"), "moved, not copied: {project}");
    }

    #[test]
    fn removing_takes_it_out_of_the_file_that_held_it() {
        let f = fixture("workspace-removes");
        testing::write(&f.project, r#"{ "collections": { "api": "requests" } }"#);
        let mut workspace = Workspace::load_at(f.user, None, Some(f.project.clone())).unwrap();

        workspace.remove("api").unwrap();
        assert!(workspace.collections().is_empty());
        let project = std::fs::read_to_string(&f.project).unwrap();
        assert!(!project.contains("api"), "{project}");
        assert!(
            f.dir
                .join("repo")
                .join("requests")
                .join("list.http")
                .exists(),
            "the requests themselves stay"
        );
    }

    #[test]
    fn a_path_on_the_command_line_is_listed_and_never_written() {
        let f = fixture("workspace-argument");
        let mut workspace = Workspace::load_at(f.user.clone(), None, None).unwrap();
        workspace.add_argument(&f.dir.join("mine")).unwrap();
        assert_eq!(names(&workspace), vec![("mine", Source::Argument)]);

        workspace
            .set(None, "other", &f.dir.join("repo"), Scope::User)
            .unwrap();
        let written = std::fs::read_to_string(&f.user).unwrap();
        assert!(!written.contains("mine"), "{written}");

        assert!(workspace.add_argument(&f.dir.join("nothing-here")).is_err());
    }

    #[test]
    fn a_name_is_qualified_once_there_is_more_than_one_collection() {
        let f = fixture("workspace-display");
        let mut workspace = Workspace::load_at(f.user, None, None).unwrap();
        let mine = f.dir.join("mine");
        workspace.add_argument(&mine).unwrap();
        assert_eq!(workspace.display(&mine.join("ping.http")), "ping.http");

        workspace.add_argument(&f.dir.join("repo")).unwrap();
        assert_eq!(workspace.display(&mine.join("ping.http")), "mine/ping.http");
        assert_eq!(
            workspace.display(&f.dir.join("repo").join("requests").join("list.http")),
            "repo/requests/list.http"
        );
    }

    #[test]
    fn a_typed_path_lands_in_the_collection_it_names() {
        let f = fixture("workspace-locate");
        let mut workspace = Workspace::load_at(f.user, None, None).unwrap();
        workspace.add_argument(&f.dir.join("mine")).unwrap();
        workspace.add_argument(&f.dir.join("repo")).unwrap();
        let mine = workspace.get("mine").cloned();

        assert_eq!(
            workspace.locate(Path::new("repo/new.http"), mine.as_ref()),
            Some(f.dir.join("repo").join("new.http"))
        );
        assert_eq!(
            workspace.locate(Path::new("users/new.http"), mine.as_ref()),
            Some(f.dir.join("mine").join("users").join("new.http")),
            "a first part that names nothing is a directory in the fallback"
        );
        assert_eq!(workspace.locate(Path::new("users/new.http"), None), None);
    }

    #[test]
    fn a_project_file_is_found_no_higher_than_its_repository() {
        let f = fixture("workspace-find");
        let repo = f.dir.join("repo");
        let deep = repo.join("src").join("deep");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        // One above the repository is somebody else's.
        testing::write(&f.dir.join(PROJECT_FILE), "{}");

        assert_eq!(
            find_project(&deep),
            Some(repo.join(PROJECT_FILE)),
            "offered at the repository's root, where it would go"
        );
        assert!(!repo.join(PROJECT_FILE).exists(), "offered, not written");

        testing::write(&repo.join("src").join(PROJECT_FILE), "{}");
        assert_eq!(
            find_project(&deep),
            Some(repo.join("src").join(PROJECT_FILE))
        );
    }

    #[test]
    fn the_first_collection_saved_to_an_offered_project_file_creates_it() {
        let f = fixture("workspace-creates");
        let mut workspace = Workspace::load_at(f.user, None, Some(f.project.clone())).unwrap();
        assert!(!workspace.project_exists());
        assert_eq!(workspace.default_scope(None), Scope::Project);

        workspace
            .set(
                None,
                "api",
                &f.dir.join("repo").join("requests"),
                Scope::Project,
            )
            .unwrap();
        assert!(workspace.project_exists());
    }

    #[test]
    fn a_name_that_would_read_as_a_path_is_refused() {
        let f = fixture("workspace-names");
        let mut workspace = Workspace::load_at(f.user, None, None).unwrap();
        assert!(
            workspace
                .set(None, "a/b", &f.dir.join("mine"), Scope::User)
                .is_err()
        );
        assert!(
            workspace
                .set(None, " ", &f.dir.join("mine"), Scope::User)
                .is_err()
        );
    }

    #[test]
    fn an_unnamed_collection_is_called_what_its_file_is() {
        let f = fixture("workspace-name-for");
        testing::write(&f.dir.join("stripe.postman_collection.json"), "{}");
        assert_eq!(
            name_for(&f.dir.join("stripe.postman_collection.json")),
            "stripe"
        );
        assert_eq!(name_for(&f.dir.join("mine")), "mine");
    }
}
