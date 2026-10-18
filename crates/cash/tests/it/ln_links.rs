//! `ln -s` as cash's own `ln` makes it (`crates/cash-uutils`): a symbolic link when Windows
//! grants one, a junction for a directory when it does not (D27).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, cash_command};

/// `ln -s` to a directory makes a link whether Windows grants symbolic links or not: a
/// junction when it does not (D27), which needs no privilege. A symbolic link to a file
/// that Windows refuses says how to get one. uutils' `ln` failed for both.
#[test]
fn ln_s_links_a_directory_without_the_privilege_and_names_it_for_a_file() {
    let scratch = Scratch::new("ln-s-junction");
    let script = r#"mkdir d && touch d/inside
ln -s d dl; echo "dir $?"
[ -L dl ] && echo link
ls dl
rm dl; ls d
ln -s d/inside fl 2>err; s=$?
if [ $s -ne 0 ]; then grep -c 'Developer Mode' err; else echo made; fi"#;
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
    // Made with the privilege, refused naming Developer Mode without it.
    assert!(matches!(lines.get(4), Some(&"made" | &"1")), "{stdout}");
}
