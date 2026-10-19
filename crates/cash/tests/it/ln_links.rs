//! `ln -s` as cash's own `ln` makes it (`crates/cash-uutils`): a symbolic link when Windows
//! grants one, a junction for a directory and a copy for a file when it does not (D27).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, cash_command};

/// `ln -s` to a directory makes a link whether Windows grants symbolic links or not: a
/// junction when it does not (D27), which needs no privilege. A symbolic link to a file
/// that Windows refuses is a copy, with a warning that says how to get the link, as Git
/// Bash's `ln` copies; bats links each test file. A link to nothing is refused. uutils'
/// `ln` failed for all of them.
#[test]
fn ln_s_links_a_directory_without_the_privilege_and_copies_a_file() {
    let scratch = Scratch::new("ln-s-junction");
    let script = r#"mkdir d && touch d/inside
ln -s d dl; echo "dir $?"
[ -L dl ] && echo link
ls dl
rm dl; ls d
ln -s d/inside fl 2>err; echo "file $?"
if [ -L fl ]; then echo link; else grep -c 'Developer Mode.*copied' err; fi
ln -s nosuch nl 2>err; s=$?; if [ -L nl ]; then echo dangling; else echo "nothing $s"; fi"#;
    let out = cash_command()
        .current_dir(scratch.path())
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.get(..4),
        Some(&["dir 0", "link", "inside", "inside"][..]),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // A link with the privilege; without it a copy, and no link to nothing.
    assert_eq!(lines.get(4), Some(&"file 0"), "{stdout}");
    assert!(
        matches!(
            (lines.get(5), lines.get(6)),
            (Some(&"link"), Some(&"dangling")) | (Some(&"1"), Some(&"nothing 1"))
        ),
        "{stdout}"
    );
}
