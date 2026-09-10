//! Helpers the unit tests share.

use std::path::{Path, PathBuf};

/// A fresh, empty directory for one test. Tests run concurrently in one
/// process, so the name carries the test's own label as well as the pid.
pub fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binman-core-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create a scratch directory");
    dir
}

pub fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create the parent directory");
    }
    std::fs::write(path, contents).expect("write a fixture");
}
