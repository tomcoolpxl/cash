//! A process substitution inside a word, as Bash expands it (TODO.md 12.0).
//!
//! `--file=<(cmd)` was split into two arguments, `--file=` and the substitution's path,
//! and `f=<(cmd)` was a syntax error. The substitution is now part of its word: the word
//! expands to the text with the path in it, and the substitution lasts as long as the
//! command the word belongs to.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

#[test]
fn a_substitution_inside_a_word_is_part_of_it() {
    for (script, expected) in [
        ("set -- --file=<(echo hi); echo $#", "1"),
        (r#"f() { cat "${1#--file=}"; }; f --file=<(echo hi)"#, "hi"),
        (r#"g=<(echo there) eval 'cat "$g"'"#, "there"),
        (r#"echo "a<(echo x)""#, "a<(echo x)"),
        ("cat <(echo standalone)", "standalone"),
    ] {
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
fn a_substitution_in_an_assignment_ends_with_the_statement() {
    // As Bash closes its descriptor: the path names nothing by the next command.
    let out =
        run(r#"f=<(echo hi); [ -n "$f" ] && echo set; cat "$f" >/dev/null 2>&1; echo "cat $?""#);
    assert_eq!(out.stdout, "set\ncat 1", "{}", out.stderr);
}
