//! `abbr`, fish's abbreviations (spec D60): the builtin's options, run as a script. How
//! the line editor expands them is in `conpty_interactive_tests.rs`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Stdio;

use crate::common::cash_command;

/// Runs `script` under cash; standard output, standard error and the exit status.
fn run(script: &str) -> (String, String, i32) {
    let out = cash_command()
        .args(["--norc", "--noprofile", "-c", script])
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    (
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn show_prints_lines_that_define_them_again_in_definition_order() {
    let (out, err, status) = run(concat!(
        "abbr -a gco git checkout\n",
        "abbr gst git status\n",
        "abbr -a --position anywhere L '| less'\n",
        "abbr -a gco git switch\n",
        "abbr\n",
        "echo ---\n",
        "abbr --list\n",
    ));
    assert_eq!(err, "");
    assert_eq!(status, 0);
    assert_eq!(
        out,
        concat!(
            "abbr -a -- gco 'git switch'\n",
            "abbr -a -- gst 'git status'\n",
            "abbr -a --position anywhere -- L '| less'\n",
            "---\n",
            "gco\ngst\nL\n",
        )
    );
}

#[test]
fn a_shown_line_defines_the_same_abbreviation() {
    let (out, err, _) = run(concat!(
        "abbr -a --position=anywhere -- \"it's\" \"echo it's\"\n",
        "saved=$(abbr --show)\n",
        "abbr -e \"it's\"\n",
        "eval \"$saved\"\n",
        "abbr -s\n",
    ));
    assert_eq!(err, "");
    assert_eq!(
        out,
        "abbr -a --position anywhere -- 'it'\\''s' 'echo it'\\''s'\n"
    );
}

#[test]
fn query_erase_and_rename() {
    let (out, err, status) = run(concat!(
        "abbr -a gco git checkout\n",
        "abbr -q gco && echo has-gco\n",
        "abbr -q nope || echo no-nope\n",
        "abbr -r gco co; abbr -l\n",
        "abbr -e co; echo erased=$?\n",
        "abbr -e co; echo again=$?\n",
    ));
    assert_eq!(out, "has-gco\nno-nope\nco\nerased=0\nagain=1\n");
    assert_eq!(err, "abbr: no such abbreviation 'co'\n");
    assert_eq!(status, 0);
}

#[test]
fn fish_scope_flags_are_accepted_and_what_cash_lacks_is_refused() {
    let (out, _, _) = run("abbr -a -g gco git checkout; abbr -U -a gst git status; abbr -l");
    assert_eq!(out, "gco\ngst\n");

    let (_, err, status) = run("abbr -a --regex 'g.*' --function f");
    assert_eq!(err, "abbr: --regex is not supported by cash\n");
    assert_eq!(status, 2);
}

#[test]
fn bad_definitions_are_usage_errors() {
    for script in [
        "abbr -a gco",
        "abbr -a 'two words' x",
        "abbr -e",
        "abbr -a -e gco x",
    ] {
        let (_, err, status) = run(script);
        assert_eq!(status, 2, "{script}: {err}");
        assert!(err.starts_with("abbr: "), "{script}: {err}");
    }
}
