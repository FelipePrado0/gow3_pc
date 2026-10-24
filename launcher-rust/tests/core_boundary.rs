#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

fn walk(root: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[test]
fn core_and_runner_never_mention_iced() {
    for root in [
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/core"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/runner"),
    ] {
        for entry in walk(root) {
            // `iced_x86` is the x86 decoder of `runner/isa_rewrite.rs`, not the UI toolkit; only
            // that exact token is allowed.
            let text = std::fs::read_to_string(&entry)
                .unwrap()
                .replace("iced_x86", "");
            assert!(!text.contains("iced"), "{entry:?} imports iced");
        }
    }
}

#[test]
fn version_is_the_crates() {
    // The crate's own version (the stamp and the exe's metadata) is the one the window shows.
    assert_eq!(env!("CARGO_PKG_VERSION"), spartanport_launcher::VERSION);
}
