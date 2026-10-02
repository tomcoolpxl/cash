//! The bundled awk, sed and bc, run as a script runs them (D48, D49, D56).
//!
//! Their own crates test what each computes; these tests drive them through cash, for
//! what only shows in a pipeline: how a tool ends when its reader goes away, and the
//! order its output takes beside another program's.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use crate::common::run;

#[test]
fn awk_stops_with_a_write_error_when_its_reader_goes() {
    // `print` went through Rust's `print!`, which panics when the pipe is closed:
    // "failed printing to stdout: The pipe is being closed" (REVIEW_REPORT.md TXT-08).
    let out = run(r#"seq 1 200000 | awk '{print}' | head -1; echo "status ${PIPESTATUS[1]}""#);
    assert_eq!(out.stdout, "1\nstatus 2", "{}", out.stderr);
    assert!(!out.stderr.contains("panicked"), "{}", out.stderr);
    assert!(
        out.stderr.contains("awk: write error: Broken pipe"),
        "{}",
        out.stderr
    );
}

#[test]
fn awk_output_comes_before_what_system_prints() {
    // gawk, mawk and BWK awk flush before `system()`; without it, `b` overtook `a`
    // (TXT-14).
    let out = run(r#"awk 'BEGIN { printf "a"; system("echo b"); print "c" }' | cat"#);
    assert_eq!(out.stdout, "ab\nc", "{}", out.stderr);
}
