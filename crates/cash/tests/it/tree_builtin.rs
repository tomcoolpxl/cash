//! Native Windows `tree` builtin tests.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "integration-test setup should fail loudly"
)]

use crate::common::{Scratch, cash_command};

/// A scratch directory holding a small tree to draw.
fn tree(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    std::fs::create_dir_all(scratch.join("beta/deep")).expect("create directory tree");
    std::fs::write(scratch.join("alpha.txt"), b"alpha").expect("create file");
    std::fs::write(scratch.join("beta/deep/leaf.txt"), b"leaf").expect("create leaf");
    std::fs::write(scratch.join(".hidden"), b"hidden").expect("create hidden file");
    scratch
}

fn cash(script: &str) -> std::process::Output {
    cash_command()
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
    let scratch = tree("shape");
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
    let scratch = tree("options");
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

#[test]
fn a_linked_root_is_listed_through_and_a_link_inside_is_not_gone_into() {
    // tree refused a directory it was given through a junction ("0 directories, 1 file"),
    // where GNU's lists the directory the link names; a junction inside is shown and not
    // gone into. Links are told by the walker's one answer (a name surrogate).
    let scratch = tree("tree-links");
    let base = scratch.as_script_path();
    cash_win32::junction::point(&scratch.join("beta/up"), scratch.path()).unwrap();
    let linked = Scratch::new("tree-links-root");
    cash_win32::junction::point(&linked.join("root"), &scratch.join("beta")).unwrap();
    let output = cash(&format!(
        "tree --noreport '{base}/beta'; tree --noreport '{}/root'",
        linked.as_script_path()
    ));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().skip(1).take(3).collect();
    assert_eq!(
        lines,
        ["├── deep", "│   └── leaf.txt", "└── up"],
        "{stdout}"
    );
    assert!(
        stdout.contains("root\n├── deep\n│   └── leaf.txt"),
        "{stdout}"
    );
}
