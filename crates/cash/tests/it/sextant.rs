//! `sextant`, the shooter drawn with text: the parts that need no player.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run as cash;

#[test]
fn it_is_a_builtin_with_a_help_page() {
    let out = cash("type sextant; help sextant | head -3");
    assert!(out.stdout.contains("sextant is a shell builtin"), "{out:?}");
    assert!(out.stdout.contains("shooter"), "{out:?}");
}

#[test]
fn glyphs_sends_the_soft_font_and_shows_every_pattern() {
    let out = cash("sextant --glyphs");
    assert_eq!(out.code, 0, "{out:?}");
    assert!(
        out.stdout.starts_with("\x1bP0;1;0;10;0;2;16;0{ @"),
        "{out:?}"
    );
    assert!(
        out.stdout.contains('\u{1FB00}') && out.stdout.contains("63 "),
        "{out:?}"
    );
}

#[test]
fn without_a_terminal_it_plays_nothing() {
    let out = cash("sextant | cat");
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr
            .contains("sextant: standard output is not a terminal"),
        "{out:?}"
    );
    let out = cash("sextant > /dev/null; echo $?");
    assert_eq!(out.stdout.trim(), "1", "{out:?}");
}
