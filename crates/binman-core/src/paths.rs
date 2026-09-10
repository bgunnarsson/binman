//! Walking from a request's directory up to the collection root.

use std::path::{Path, PathBuf};

/// Every directory from `dir` up to and including `root`, deepest first.
///
/// Stops at the filesystem root when `dir` is not inside `root` at all, rather
/// than looping, so a request opened from outside the collection still finds
/// the environments above it.
pub(crate) fn lineage(dir: &Path, root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut current = Some(dir);
    while let Some(here) = current {
        out.push(here.to_path_buf());
        if here == root {
            break;
        }
        current = here.parent();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_up_to_the_root_and_no_further() {
        let got = lineage(Path::new("/c/a/b"), Path::new("/c"));
        assert_eq!(
            got,
            vec![
                PathBuf::from("/c/a/b"),
                PathBuf::from("/c/a"),
                PathBuf::from("/c")
            ]
        );
    }

    #[test]
    fn a_directory_outside_the_root_stops_at_the_filesystem_root() {
        let got = lineage(Path::new("/x/y"), Path::new("/c"));
        assert_eq!(got.last(), Some(&PathBuf::from("/")));
    }
}
