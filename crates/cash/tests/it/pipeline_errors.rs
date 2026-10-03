//! An error in a stage of a pipeline ends the stage, not the shell (EXE-02).
//!
//! A stage that runs in a shell of its own is a subshell to Bash: whatever ends it, an
//! error, `exit`, `break` or `return`, leaves the pipeline only its status, and the
//! script goes on. In cash such an error ended the whole shell, and `exit` in a stage
//! exited it. The statuses are Bash 5.3's: a fatal expansion error ends a forked simple
//! command or function with 127, as at the top level, and a subshell, a compound stage
//! or a command substitution, which catch it themselves, with 1.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

/// Runs each script and checks its standard output and status against Bash's.
fn check(cases: &[(&str, &str, i32)]) {
    for &(script, expected, code) in cases {
        let out = run(script);
        assert_eq!(
            (out.stdout.as_str(), out.code),
            (expected, code),
            "{script}: {}",
            out.stderr
        );
    }
}

#[test]
fn an_error_in_a_stage_ends_only_the_stage() {
    check(&[
        (
            r#"read -u 99 x | cat; echo "after ${PIPESTATUS[*]}""#,
            "after 1 0",
            0,
        ),
        (
            r#"true | echo ${u:?boom}; echo "after ${PIPESTATUS[*]}""#,
            "after 0 127",
            0,
        ),
        (
            r#"echo ${u:?boom} | cat; echo "after ${PIPESTATUS[*]}""#,
            "after 127 0",
            0,
        ),
        (
            r#"true | { echo ${u:?boom}; echo inner; }; echo "after $?""#,
            "after 1",
            0,
        ),
        (
            r#"set -u; true | echo $nope; echo "after $?""#,
            "after 127",
            0,
        ),
        (
            r#"h() { echo ${u:?b}; }; true | h; echo "fn ${PIPESTATUS[*]}""#,
            "fn 0 127",
            0,
        ),
        (
            r#"x=$(true | echo ${u:?boom}); echo "after $? [$x]""#,
            "after 1 []",
            0,
        ),
        (
            r#"read -u 99 x | read -u 98 y | cat; echo "${PIPESTATUS[*]}""#,
            "1 1 0",
            0,
        ),
    ]);
}

#[test]
fn exit_break_and_return_in_a_stage_end_only_the_stage() {
    check(&[
        (
            r#"true | exit 4; echo "after ${PIPESTATUS[*]}""#,
            "after 0 4",
            0,
        ),
        (
            "for i in 1 2; do true | break; echo \"i $i\"; done",
            "i 1\ni 2",
            0,
        ),
        (
            r#"g() { true | return 3; echo "after return ${PIPESTATUS[*]}"; }; g"#,
            "after return 0 3",
            0,
        ),
        (
            r#"true | (exit 5) | cat; echo "${PIPESTATUS[*]}""#,
            "0 5 0",
            0,
        ),
    ]);
}

#[test]
fn a_bundled_tool_whose_reader_goes_ends_with_141_and_says_nothing() {
    // It said `seq: write error: Broken pipe` and ended with 0 or 1; Bash's ends as
    // SIGPIPE ends it (D71). A builtin already did.
    for (script, expected) in [
        (
            r#"seq 1 1000000 | head -1; echo "${PIPESTATUS[*]}""#,
            "1\n141 0",
        ),
        (
            r#"yes | tr y n | head -1; echo "${PIPESTATUS[*]}""#,
            "n\n141 141 0",
        ),
        (
            r#"seq 1 200000 | sed p | head -1; echo "${PIPESTATUS[*]}""#,
            "1\n141 141 0",
        ),
        (
            r#"while :; do echo y; done | head -1; echo "${PIPESTATUS[*]}""#,
            "y\n141 0",
        ),
        (
            r#"( while :; do echo y; done; echo after >&2 ) | head -1; echo "${PIPESTATUS[*]}""#,
            "y\n141 0",
        ),
        (
            r#"f() { while :; do echo y; done; echo after >&2; }; f | head -1; echo "${PIPESTATUS[*]}""#,
            "y\n141 0",
        ),
        // A reader that stays sees all of it.
        (
            r#"seq 1 5 | cat | wc -l; echo "${PIPESTATUS[*]}""#,
            "5\n0 0 0",
        ),
    ] {
        let out = run(script);
        assert_eq!(out.stdout, expected, "{script}");
        assert!(out.stderr.is_empty(), "{script}: {}", out.stderr);
    }
}

#[test]
fn a_fatal_expansion_error_ends_the_shell_with_127_and_a_subshell_with_1() {
    check(&[
        ("echo ${u:?boom}; echo no", "", 127),
        ("set -u; echo $nope; echo no", "", 127),
        ("x=${u:?boom}; echo no", "", 127),
        (r#"(echo ${u:?boom}); echo "sub $?""#, "sub 1", 0),
        // The last stage of a `lastpipe` pipeline is the shell's own command.
        (
            r#"shopt -s lastpipe; true | echo ${u:?boom}; echo "after $?""#,
            "",
            127,
        ),
    ]);
}
