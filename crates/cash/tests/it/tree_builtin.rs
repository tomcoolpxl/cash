//! Native Windows `tree` builtin tests.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "integration-test setup should fail loudly"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cash-tree-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("beta/deep")).expect("create directory tree");
        std::fs::write(path.join("alpha.txt"), b"alpha").expect("create file");
        std::fs::write(path.join("beta/deep/leaf.txt"), b"leaf").expect("create leaf");
        std::fs::write(path.join(".hidden"), b"hidden").expect("create hidden file");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cash(script: &str) -> std::process::Output {
    Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("run cash")
}

#[test]
fn tree_is_a_builtin() {
    let output = cash("type tree");
    assert!(String::from_utf8_lossy(&output.stdout).contains("shell builtin"));
}

#[test]
fn tree_draws_a_sorted_hierarchy_and_counts_it() {
    let scratch = Scratch::new("shape");
    let path = scratch.path().to_string_lossy().replace('\\', "/");
    let output = cash(&format!("tree '{path}'"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("├── alpha.txt"), "{stdout}");
    assert!(stdout.contains("└── beta"), "{stdout}");
    assert!(stdout.contains("    └── deep"), "{stdout}");
    assert!(stdout.contains("        └── leaf.txt"), "{stdout}");
    assert!(!stdout.contains(".hidden"), "{stdout}");
    assert!(stdout.contains("2 directories, 2 files"), "{stdout}");
}

#[test]
fn common_filters_are_supported() {
    let scratch = Scratch::new("options");
    let path = scratch.path().to_string_lossy().replace('\\', "/");

    let shallow = cash(&format!("tree -a -L 1 --noreport '{path}'"));
    let shallow = String::from_utf8_lossy(&shallow.stdout);
    assert!(shallow.contains(".hidden"), "{shallow}");
    assert!(shallow.contains("beta"), "{shallow}");
    assert!(!shallow.contains("deep"), "{shallow}");
    assert!(!shallow.contains("directories"), "{shallow}");

    let directories = cash(&format!("tree -d '{path}'"));
    let directories = String::from_utf8_lossy(&directories.stdout);
    assert!(directories.contains("beta"), "{directories}");
    assert!(directories.contains("deep"), "{directories}");
    assert!(!directories.contains("alpha.txt"), "{directories}");
    assert!(
        directories.contains("2 directories, 0 files"),
        "{directories}"
    );
}
