//! The collections tree.
//!
//! Each registered collection is a row of the top level. Directories load as
//! they are opened, a Postman collection or an OpenAPI spec expands into its
//! requests the same way, and a request is a leaf. Nodes carry ids that
//! outlive a reload, as binsql's do, so nothing is keyed on a row that has
//! since moved.

use std::path::{Path, PathBuf};

use binman_core::collection::{self, EntryKind};
use binman_core::formats::{openapi, postman};
use binman_core::workspace::{Collection, Source};
use binman_core::{Format, Origin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u64);

#[derive(Debug, Clone)]
pub enum NodeKind {
    /// A registered collection: a directory of requests, or one file that
    /// holds them.
    Root {
        name: String,
        path: PathBuf,
        source: Source,
    },
    Dir {
        path: PathBuf,
        name: String,
    },
    /// A `.http`, `.bru` or `.graphql` file.
    File {
        path: PathBuf,
        name: String,
        format: Format,
        method: Option<String>,
    },
    /// A Postman collection file.
    Collection {
        path: PathBuf,
        name: String,
    },
    /// A folder inside a Postman collection.
    Folder {
        name: String,
    },
    /// A request inside a Postman collection.
    Item {
        origin: Origin,
        name: String,
        method: String,
    },
    /// An OpenAPI spec file.
    Spec {
        path: PathBuf,
        name: String,
    },
    Tag {
        name: String,
    },
    Operation {
        origin: Origin,
        route: String,
        method: String,
        summary: String,
    },
    /// A leaf that carries a message rather than a request: "(empty)", or why
    /// a file would not read.
    Note {
        text: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadState {
    /// Children not read yet; expanding reads them.
    Pending,
    Loaded,
    /// Nothing to read: a request, or a note.
    Leaf,
}

#[derive(Debug)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub children: Vec<Node>,
    pub expanded: bool,
    pub state: LoadState,
}

impl Node {
    pub fn is_expandable(&self) -> bool {
        self.state != LoadState::Leaf
    }

    /// The request this node opens, if it is one.
    pub fn origin(&self) -> Option<Origin> {
        match &self.kind {
            NodeKind::File { path, .. } => Some(Origin::File(path.clone())),
            NodeKind::Item { origin, .. } | NodeKind::Operation { origin, .. } => {
                Some(origin.clone())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VisibleNode {
    pub id: NodeId,
    pub depth: usize,
}

#[derive(Debug)]
pub struct Tree {
    pub roots: Vec<Node>,
    next_id: u64,
    pub selected: usize,
    pub offset: usize,
}

impl Tree {
    /// The collections, with the ones you most likely came for already open:
    /// the project's, those named on the command line, or the only one there
    /// is. The rest wait to be opened, so a long list of your own stays a
    /// list.
    pub fn new(collections: &[Collection]) -> Tree {
        let mut tree = Tree {
            roots: Vec::new(),
            next_id: 0,
            selected: 0,
            offset: 0,
        };
        tree.sync(collections);
        let only = collections.len() == 1;
        let open: Vec<NodeId> = tree
            .roots
            .iter()
            .filter(|node| {
                only || matches!(
                    node.kind,
                    NodeKind::Root {
                        source: Source::Project | Source::Argument,
                        ..
                    }
                )
            })
            .map(|node| node.id)
            .collect();
        for id in open {
            tree.toggle(id);
        }
        tree
    }

    /// Brings the top level in line with the collections, keeping each one
    /// still there as it was — open, with what was read under it.
    pub fn sync(&mut self, collections: &[Collection]) {
        let mut kept = std::mem::take(&mut self.roots);
        let roots: Vec<Node> = collections
            .iter()
            .map(|collection| {
                let same = kept.iter().position(|node| {
                    matches!(&node.kind, NodeKind::Root { name, path, .. }
                        if *name == collection.name && *path == collection.path)
                });
                match same {
                    Some(index) => {
                        let mut node = kept.swap_remove(index);
                        if let NodeKind::Root { source, .. } = &mut node.kind {
                            *source = collection.source;
                        }
                        node
                    }
                    None => self.make(
                        NodeKind::Root {
                            name: collection.name.clone(),
                            path: collection.path.clone(),
                            source: collection.source,
                        },
                        LoadState::Pending,
                    ),
                }
            })
            .collect();
        self.roots = roots;
        self.clamp_selection();
    }

    fn make(&mut self, kind: NodeKind, state: LoadState) -> Node {
        self.next_id += 1;
        Node {
            id: NodeId(self.next_id),
            kind,
            children: Vec::new(),
            expanded: false,
            state,
        }
    }

    fn note(&mut self, text: impl Into<String>, is_error: bool) -> Node {
        self.make(
            NodeKind::Note {
                text: text.into(),
                is_error,
            },
            LoadState::Leaf,
        )
    }

    fn list(&mut self, dir: &Path) -> Vec<Node> {
        match collection::list(dir) {
            Ok(entries) => entries.into_iter().map(|entry| self.entry(entry)).collect(),
            Err(error) => vec![self.note(error.to_string(), true)],
        }
    }

    fn entry(&mut self, entry: collection::Entry) -> Node {
        let collection::Entry { name, path, kind } = entry;
        match kind {
            EntryKind::Dir => self.make(NodeKind::Dir { path, name }, LoadState::Pending),
            EntryKind::Request(format) => {
                let method = collection::method_of(&path, format);
                self.make(
                    NodeKind::File {
                        path,
                        name,
                        format,
                        method,
                    },
                    LoadState::Leaf,
                )
            }
            EntryKind::Postman => {
                self.make(NodeKind::Collection { path, name }, LoadState::Pending)
            }
            EntryKind::OpenApi => self.make(NodeKind::Spec { path, name }, LoadState::Pending),
        }
    }

    /// Reads what is under a node. A file that will not parse says why, in
    /// the tree, where the reader is looking.
    fn children_of(&mut self, kind: &NodeKind) -> Vec<Node> {
        match kind {
            NodeKind::Root { path, .. } => self.collection(path),
            NodeKind::Dir { path, .. } => self.list(path),
            NodeKind::Collection { path, .. } => {
                match std::fs::read(path)
                    .map_err(binman_core::Error::from)
                    .and_then(|bytes| postman::parse(&bytes))
                {
                    Ok(collection) => self.postman(path, &collection.items, &mut Vec::new()),
                    Err(error) => vec![self.note(error.to_string(), true)],
                }
            }
            NodeKind::Spec { path, name } => {
                match std::fs::read(path)
                    .map_err(binman_core::Error::from)
                    .and_then(|bytes| openapi::parse(&bytes, name))
                {
                    Ok(spec) => self.spec(path, &spec),
                    Err(error) => vec![self.note(error.to_string(), true)],
                }
            }
            _ => Vec::new(),
        }
    }

    /// What a registered collection holds: a directory's listing, or what its
    /// one file does. One that is not there says so rather than listing as
    /// empty — a path from a teammate's machine, or a drive not mounted.
    fn collection(&mut self, path: &Path) -> Vec<Node> {
        if path.is_dir() {
            return self.list(path);
        }
        if !path.exists() {
            return vec![self.note(format!("{} is not there", path.display()), true)];
        }
        let Some(entry) = collection::entry(path) else {
            return vec![self.note("binman can't open this file", true)];
        };
        let node = self.entry(entry);
        match &node.kind {
            NodeKind::File { .. } => vec![node],
            kind => {
                let kind = kind.clone();
                self.children_of(&kind)
            }
        }
    }

    /// A collection's folders start open, as they did in v1: a collection is
    /// opened to see its requests, not to open it again one folder at a time.
    fn postman(
        &mut self,
        path: &Path,
        items: &[postman::Item],
        trail: &mut Vec<usize>,
    ) -> Vec<Node> {
        let mut out = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            trail.push(index);
            if item.is_request() {
                out.push(self.make(
                    NodeKind::Item {
                        origin: Origin::Postman {
                            path: path.to_path_buf(),
                            item: trail.clone(),
                        },
                        name: item.name.clone(),
                        method: item.method(),
                    },
                    LoadState::Leaf,
                ));
            } else {
                let children = self.postman(path, &item.items, trail);
                let mut folder = self.make(
                    NodeKind::Folder {
                        name: item.name.clone(),
                    },
                    LoadState::Loaded,
                );
                folder.children = self.or_empty(children);
                folder.expanded = true;
                out.push(folder);
            }
            trail.pop();
        }
        out
    }

    fn spec(&mut self, path: &Path, spec: &openapi::Spec) -> Vec<Node> {
        let mut out = Vec::new();
        for group in openapi::groups(spec) {
            let mut children = Vec::with_capacity(group.endpoints.len());
            for endpoint in group.endpoints {
                children.push(self.make(
                    NodeKind::Operation {
                        origin: Origin::OpenApi {
                            path: path.to_path_buf(),
                            route: endpoint.route.clone(),
                            method: endpoint.method.clone(),
                        },
                        route: endpoint.route,
                        method: endpoint.method,
                        summary: endpoint.summary,
                    },
                    LoadState::Leaf,
                ));
            }
            let mut tag = self.make(NodeKind::Tag { name: group.tag }, LoadState::Loaded);
            tag.children = children;
            tag.expanded = true;
            out.push(tag);
        }
        out
    }

    fn or_empty(&mut self, children: Vec<Node>) -> Vec<Node> {
        if children.is_empty() {
            vec![self.note("(empty)", false)]
        } else {
            children
        }
    }

    pub fn find(&self, id: NodeId) -> Option<&Node> {
        fn walk(nodes: &[Node], id: NodeId) -> Option<&Node> {
            for node in nodes {
                if node.id == id {
                    return Some(node);
                }
                if let Some(found) = walk(&node.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.roots, id)
    }

    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        fn walk(nodes: &mut [Node], id: NodeId) -> Option<&mut Node> {
            for node in nodes {
                if node.id == id {
                    return Some(node);
                }
                if let Some(found) = walk(&mut node.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&mut self.roots, id)
    }

    fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        fn walk(nodes: &[Node], id: NodeId, parent: Option<NodeId>) -> Option<NodeId> {
            for node in nodes {
                if node.id == id {
                    return parent;
                }
                if let Some(found) = walk(&node.children, id, Some(node.id)) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.roots, id, None)
    }

    /// The rows the sidebar draws, in order, with their depth.
    pub fn visible(&self) -> Vec<VisibleNode> {
        fn walk(nodes: &[Node], depth: usize, out: &mut Vec<VisibleNode>) {
            for node in nodes {
                out.push(VisibleNode { id: node.id, depth });
                if node.expanded {
                    walk(&node.children, depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.roots, 0, &mut out);
        out
    }

    pub fn selected_id(&self) -> Option<NodeId> {
        self.visible().get(self.selected).map(|node| node.id)
    }

    pub fn selected(&self) -> Option<&Node> {
        self.selected_id().and_then(|id| self.find(id))
    }

    /// The name of the collection the selected row is in.
    pub fn selected_collection(&self) -> Option<&str> {
        let mut id = self.selected_id()?;
        while let Some(parent) = self.parent_of(id) {
            id = parent;
        }
        match &self.find(id)?.kind {
            NodeKind::Root { name, .. } => Some(name),
            _ => None,
        }
    }

    /// The name of the collection whose own row is selected.
    pub fn selected_root(&self) -> Option<&str> {
        match &self.selected()?.kind {
            NodeKind::Root { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Selects a collection's row and opens it.
    pub fn select_root(&mut self, name: &str) {
        let Some(node) = self
            .roots
            .iter()
            .find(|node| matches!(&node.kind, NodeKind::Root { name: held, .. } if held == name))
        else {
            return;
        };
        let (id, expanded) = (node.id, node.expanded);
        if !expanded {
            self.toggle(id);
        }
        self.select_id(id);
    }

    pub fn select_id(&mut self, id: NodeId) {
        if let Some(index) = self.visible().iter().position(|node| node.id == id) {
            self.selected = index;
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.visible().len();
        if count > 0 {
            self.selected = (self.selected as isize + delta).clamp(0, count as isize - 1) as usize;
        }
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self) {
        self.selected = self.visible().len().saturating_sub(1);
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
    }

    /// Opens or closes a node, reading its children the first time.
    pub fn toggle(&mut self, id: NodeId) {
        let Some(node) = self.find(id) else {
            return;
        };
        if !node.is_expandable() {
            return;
        }
        if node.expanded {
            if let Some(node) = self.find_mut(id) {
                node.expanded = false;
            }
            return;
        }
        if node.state == LoadState::Pending {
            let kind = node.kind.clone();
            let children = self.children_of(&kind);
            let children = self.or_empty(children);
            if let Some(node) = self.find_mut(id) {
                node.children = children;
                node.state = LoadState::Loaded;
            }
        }
        if let Some(node) = self.find_mut(id) {
            node.expanded = true;
        }
    }

    /// Collapses the selected node, or steps to its parent when it is already
    /// collapsed — the movement people expect from a file tree.
    pub fn collapse_or_parent(&mut self) {
        let Some(id) = self.selected_id() else {
            return;
        };
        if self.find(id).is_some_and(|node| node.expanded) {
            if let Some(node) = self.find_mut(id) {
                node.expanded = false;
            }
            return;
        }
        if let Some(parent) = self.parent_of(id) {
            self.select_id(parent);
        }
    }

    /// Opens the directories down to a file just written and selects it.
    /// Each is read again on the way, since the file — and maybe the
    /// directory — was not there when it was last read, and what was already
    /// open under each stays open.
    pub fn reveal(&mut self, path: &Path) {
        let Some((root, root_path)) = self
            .roots
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::Root { path: root, .. } if path.starts_with(root) => {
                    Some((node.id, root.clone()))
                }
                _ => None,
            })
            .max_by_key(|(_, root)| root.components().count())
        else {
            return;
        };
        let Some(relative) = path
            .parent()
            .and_then(|parent| parent.strip_prefix(&root_path).ok())
        else {
            return;
        };

        let mut dir = root_path;
        self.refresh(root, &dir);
        let mut parent = root;
        for component in relative.components() {
            dir.push(component);
            let Some(id) = self.find(parent).and_then(|node| {
                node.children
                    .iter()
                    .find(|node| matches!(&node.kind, NodeKind::Dir { path, .. } if *path == dir))
                    .map(|node| node.id)
            }) else {
                return;
            };
            self.refresh(id, &dir);
            parent = id;
        }

        if let Some(id) = self.find(parent).and_then(|node| {
            node.children
                .iter()
                .find(
                    |node| matches!(&node.kind, NodeKind::File { path: file, .. } if file == path),
                )
                .map(|node| node.id)
        }) {
            self.select_id(id);
        }
    }

    /// Reads a directory under `id` again and opens it, keeping what was
    /// open under it.
    fn refresh(&mut self, id: NodeId, dir: &Path) {
        let fresh = self.list(dir);
        if let Some(node) = self.find_mut(id) {
            let kept = std::mem::take(&mut node.children);
            node.children = merge(kept, fresh);
            node.state = LoadState::Loaded;
            node.expanded = true;
        }
    }

    /// Reads the selected node again from disk — or, for a request, what it
    /// sits in, since that is where a new file would appear. Says what it
    /// read, or nothing when there is nothing selected to read.
    pub fn reload_selected(&mut self) -> Option<String> {
        let mut target = self.selected_id();
        while let Some(id) = target {
            match self.find(id).map(|node| &node.kind) {
                Some(
                    NodeKind::Root { .. }
                    | NodeKind::Dir { .. }
                    | NodeKind::Collection { .. }
                    | NodeKind::Spec { .. },
                ) => break,
                _ => target = self.parent_of(id),
            }
        }

        let id = target?;
        let kind = self.find(id)?.kind.clone();
        let children = self.children_of(&kind);
        let children = self.or_empty(children);
        if let Some(node) = self.find_mut(id) {
            node.children = children;
            node.state = LoadState::Loaded;
            node.expanded = true;
        }
        self.clamp_selection();
        match kind {
            NodeKind::Root { name, .. }
            | NodeKind::Dir { name, .. }
            | NodeKind::Collection { name, .. }
            | NodeKind::Spec { name, .. } => Some(name),
            _ => None,
        }
    }
}

/// A directory as just read, with each subdirectory that was already there
/// kept as it was — open, with its children — so reading it again adds what
/// is new without closing everything under it.
fn merge(mut kept: Vec<Node>, fresh: Vec<Node>) -> Vec<Node> {
    fresh
        .into_iter()
        .map(|node| {
            let NodeKind::Dir { path, .. } = &node.kind else {
                return node;
            };
            match kept
                .iter()
                .position(|old| matches!(&old.kind, NodeKind::Dir { path: old, .. } if old == path))
            {
                Some(index) => kept.swap_remove(index),
                None => node,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("binman-tree-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("users")).unwrap();
        std::fs::write(dir.join("users").join("list.http"), "GET https://x/users\n").unwrap();
        std::fs::write(
            dir.join("api.postman_collection.json"),
            r#"{"item":[{"name":"Auth","item":[{"name":"Login","request":{"method":"POST","url":"https://x"}}]}]}"#,
        )
        .unwrap();
        dir
    }

    fn collection_at(name: &str, path: PathBuf, source: Source) -> Collection {
        Collection {
            name: name.into(),
            path,
            source,
        }
    }

    /// A tree of the one collection, which starts open.
    fn tree_of(dir: PathBuf) -> Tree {
        Tree::new(&[collection_at("c", dir, Source::User)])
    }

    fn names(tree: &Tree) -> Vec<String> {
        tree.visible()
            .iter()
            .filter_map(|visible| tree.find(visible.id))
            .map(|node| match &node.kind {
                NodeKind::Root { name, .. }
                | NodeKind::Dir { name, .. }
                | NodeKind::File { name, .. }
                | NodeKind::Collection { name, .. }
                | NodeKind::Folder { name }
                | NodeKind::Item { name, .. }
                | NodeKind::Spec { name, .. }
                | NodeKind::Tag { name } => name.clone(),
                NodeKind::Operation { route, .. } => route.clone(),
                NodeKind::Note { text, .. } => text.clone(),
            })
            .collect()
    }

    #[test]
    fn directories_load_as_they_are_opened() {
        let mut tree = tree_of(scratch("lazy"));
        assert_eq!(
            names(&tree),
            vec!["c", "users", "api.postman_collection.json"]
        );

        let users = tree.visible()[1].id;
        tree.toggle(users);
        assert_eq!(
            names(&tree),
            vec!["c", "users", "list.http", "api.postman_collection.json"]
        );
        assert!(tree.find(tree.visible()[2].id).unwrap().origin().is_some());
    }

    #[test]
    fn a_collection_opens_with_its_folders_open() {
        let mut tree = tree_of(scratch("postman"));
        let collection = tree.visible()[2].id;
        tree.toggle(collection);
        assert_eq!(
            names(&tree),
            vec!["c", "users", "api.postman_collection.json", "Auth", "Login"]
        );
    }

    #[test]
    fn a_new_file_appears_when_its_directory_is_reloaded() {
        let root = scratch("reload");
        let mut tree = tree_of(root.clone());
        let users = tree.visible()[1].id;
        tree.toggle(users);
        std::fs::write(root.join("users").join("create.http"), "POST https://x\n").unwrap();

        tree.selected = 2;
        assert_eq!(tree.reload_selected().as_deref(), Some("users"));
        assert!(names(&tree).contains(&"create.http".to_string()));
    }

    #[test]
    fn only_the_collections_you_came_for_start_open() {
        let dir = scratch("which-open");
        let tree = Tree::new(&[
            collection_at("api", dir.join("users"), Source::Project),
            collection_at("mine", dir.clone(), Source::User),
        ]);
        assert_eq!(names(&tree), vec!["api", "list.http", "mine"]);
    }

    #[test]
    fn a_collection_that_is_one_file_holds_its_requests() {
        let dir = scratch("one-file");
        let tree = tree_of(dir.join("api.postman_collection.json"));
        assert_eq!(names(&tree), vec!["c", "Auth", "Login"]);
    }

    #[test]
    fn a_collection_that_is_not_there_says_so() {
        let dir = scratch("missing");
        let tree = tree_of(dir.join("gone"));
        let note = &names(&tree)[1];
        assert!(note.ends_with("is not there"), "{note}");
    }

    #[test]
    fn a_new_list_of_collections_keeps_the_ones_still_there_as_they_were() {
        let dir = scratch("sync");
        let mine = collection_at("mine", dir.clone(), Source::User);
        let mut tree = tree_of(dir.clone());
        tree.sync(std::slice::from_ref(&mine));
        tree.select_root("mine");
        let users = tree.visible()[1].id;
        tree.toggle(users);

        tree.sync(&[
            collection_at("api", dir.join("users"), Source::Project),
            mine,
        ]);
        assert_eq!(
            names(&tree),
            vec![
                "api",
                "mine",
                "users",
                "list.http",
                "api.postman_collection.json"
            ]
        );
    }
}
