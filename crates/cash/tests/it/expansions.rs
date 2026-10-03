//! Expansions as Bash 5.3 performs them (TODO.md 12.3).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

/// Runs each script and checks its standard output and status against Bash's.
fn check(cases: &[(&str, &str)]) {
    for &(script, expected) in cases {
        let out = run(script);
        assert_eq!(
            (out.stdout.as_str(), out.code),
            (expected, 0),
            "{script}: {}",
            out.stderr
        );
    }
}

#[test]
fn lengths_and_offsets_count_characters() {
    // They counted bytes: `${#x}` of `café` was 5 and `${x: -1}` empty (LANG-09).
    check(&[
        ("x=café; echo ${#x} \"${x: -1}\" \"${x: -2:1}\"", "4 é f"),
        (
            "a=(café ü); echo ${#a[0]} ${#a[1]} \"${a[0]: -1}\"",
            "4 1 é",
        ),
        ("y=日本語; echo ${#y} \"${y: -1}\"", "3 語"),
        ("set -- héllo; echo ${#1}", "5"),
    ]);
}
