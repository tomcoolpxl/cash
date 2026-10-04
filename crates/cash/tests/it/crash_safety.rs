//! Inputs that used to take the whole shell down.
//!
//! The panic recovery README's "Shell robustness" describes catches an unwinding panic
//! and goes back to the prompt. A stack overflow is not a panic: it aborts the process,
//! and at the prompt that is the session gone, with every program the job object holds.
//! Function recursion overflowed at depth 200, about 45 inside `$(...)` and about 20 in a
//! pipeline stage, where Bash runs 500 (`REVIEW_REPORT.md` BIN-01). The other cases here
//! were panics: in a `peg!` rule, in the `-c` attribute, in a prompt's `\D{}`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed."
)]

use crate::common::{Output, run as cash};

/// The script ran to its end: no overflow, no panic report, the last line printed.
fn assert_survived(out: &Output, last_line: &str) {
    assert!(
        !out.stderr.contains("overflowed its stack")
            && !out.stderr.contains("panicked")
            && !out.stderr.contains("internal error"),
        "cash crashed: {}",
        out.stderr
    );
    assert_eq!(out.stdout.lines().last(), Some(last_line), "{}", out.stderr);
    assert_eq!(out.code, 0, "{}", out.stderr);
}

const COUNTDOWN: &str = r#"f() { (( $1 > 0 )) && f $(( $1 - 1 )); return 0; }"#;

#[test]
fn a_function_recurses_two_hundred_deep() {
    let out = cash(&format!("{COUNTDOWN}; f 200; echo done"));
    assert_survived(&out, "done");
}

#[test]
fn a_function_recurses_two_hundred_deep_inside_a_command_substitution() {
    let out = cash(&format!(
        r#"{COUNTDOWN}; x=$(f 200; echo inner); echo "$x""#
    ));
    assert_survived(&out, "inner");
}

#[test]
fn a_function_recurses_two_hundred_deep_in_a_pipeline_stage() {
    // The shape of a directory walker: a loop around the recursive call, piped on.
    let out = cash(
        r#"f() { local i=$1; if [ "$i" -gt 0 ]; then for x in a; do f $((i-1)); done; else echo bottom; fi; }
           f 200 | cat"#,
    );
    assert_survived(&out, "bottom");
}

#[test]
fn the_depth_guard_fires_before_the_stack_runs_out() {
    let out = cash(&format!("{COUNTDOWN}; f 600"));
    assert!(
        out.stderr
            .contains("maximum function nesting level exceeded (500)"),
        "{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("overflowed its stack"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_brace_sequence_step_beyond_i64_stays_literal() {
    let out = cash("echo {1..3..99999999999999999999}; echo after");
    assert_survived(&out, "after");
    assert_eq!(
        out.stdout.lines().next(),
        Some("{1..3..99999999999999999999}")
    );
}

#[test]
fn capitalize_attribute_takes_a_multibyte_first_letter() {
    let out = cash("declare -c c; c=éCOLE; echo \"$c\"");
    assert_survived(&out, "École");
}

#[test]
fn an_unknown_date_specifier_in_a_prompt_gives_nothing() {
    let out = cash(r#"x='[\D{%Q}]'; echo "${x@P}"; echo after"#);
    assert_survived(&out, "after");
    assert_eq!(out.stdout.lines().next(), Some("[]"));
}

#[test]
fn a_prompt_counts_commands_and_history() {
    // A script reads no command at a prompt and keeps no history: both are 1, as in Bash.
    let out = cash(r#"x='[\!|\#]'; echo "${x@P}""#);
    assert_survived(&out, "[1|1]");
}
