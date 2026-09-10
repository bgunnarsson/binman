//! The collections tree.
//!
//! Directories load as they are opened, a Postman collection or an OpenAPI
//! spec expands into its requests the same way, and a request is a leaf. Nodes
//! carry ids that outlive a reload, as binsql's do, so nothing is keyed on a
//! row that has since moved.

use std::path::{Path, PathBuf};

use binman_core::collection::{self, EntryKind};
use binman_core::formats::{openapi, postman};
use binman_core::{Format, Origin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u64);

#[derive(Debug, Clone)]
pub enum NodeKind {
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
    pub root: PathBuf,
    pub roots: Vec<Node>,
    next_id: u64,
    pub selected: usize,
    pub offset: usize,
}

impl Tree {
    pub fn new(root: PathBuf) -> Tree {
        let mut tree = Tree {
            root,
            roots: Vec::new(),
            next_id: 0,
            selected: 0,
            offset: 0,
        };
        let root = tree.root.clone();
        tree.roots = tree.list(&root);
        tree
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

    /// Reads the selected node again from disk — or, for a request, the
    /// directory it sits in, since that is where a new file would appear. The
    /// top level is read again when nothing narrower applies.
    pub fn reload_selected(&mut self) -> String {
        let mut target = self.selected_id();
        while let Some(id) = target {
            match self.find(id).map(|node| &node.kind) {
                Some(
                    NodeKind::Dir { .. } | NodeKind::Collection { .. } | NodeKind::Spec { .. },
                ) => break,
                _ => target = self.parent_of(id),
            }
        }

        let Some(id) = target else {
            let root = self.root.clone();
            self.roots = self.list(&root);
            self.clamp_selection();
            return "the collections".to_string();
        };
        let Some(kind) = self.find(id).map(|node| node.kind.clone()) else {
            return String::new();
        };
        let children = self.children_of(&kind);
        let children = self.or_empty(children);
        if let Some(node) = self.find_mut(id) {
            node.children = children;
            node.state = LoadState::Loaded;
            node.expanded = true;
        }
        self.clamp_selection();
        match kind {
            NodeKind::Dir { name, .. }
            | NodeKind::Collection { name, .. }
            | NodeKind::Spec { name, .. } => name,
            _ => String::new(),
        }
    }
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

    fn names(tree: &Tree) -> Vec<String> {
        tree.visible()
            .iter()
            .filter_map(|visible| tree.find(visible.id))
            .map(|node| match &node.kind {
                NodeKind::Dir { name, .. }
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
        let tree_root = scratch("lazy");
        let mut tree = Tree::new(tree_root);
        assert_eq!(names(&tree), vec!["users", "api.postman_collection.json"]);

        let users = tree.visible()[0].id;
        tree.toggle(users);
        assert_eq!(
            names(&tree),
            vec!["users", "list.http", "api.postman_collection.json"]
        );
        assert!(tree.find(tree.visible()[1].id).unwrap().origin().is_some());
    }

    #[test]
    fn a_collection_opens_with_its_folders_open() {
        let mut tree = Tree::new(scratch("postman"));
        let collection = tree.visible()[1].id;
        tree.toggle(collection);
        assert_eq!(
            names(&tree),
            vec!["users", "api.postman_collection.json", "Auth", "Login"]
        );
    }

    #[test]
    fn a_new_file_appears_when_its_directory_is_reloaded() {
        let root = scratch("reload");
        let mut tree = Tree::new(root.clone());
        let users = tree.visible()[0].id;
        tree.toggle(users);
        std::fs::write(root.join("users").join("create.http"), "POST https://x\n").unwrap();

        tree.selected = 1;
        assert_eq!(tree.reload_selected(), "users");
        assert!(names(&tree).contains(&"create.http".to_string()));
    }
}
