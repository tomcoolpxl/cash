//! Pipelines that actually run concurrently, and `read -t` — **D11**, **D26**.
//!
//! A compound command on the left of a pipeline was `await`ed inline, so it ran to
//! *completion* before the next member was created. Nothing was draining the pipe, and
//! above the buffer — 4096 bytes on Windows — everything it wrote was lost:
//!
//! ```text
//! { seq 1 5000; } | wc -l     ->  0      (bash: 5000)
//! ```
//!
//! Total, silent data loss in an everyday construct: `for f in *; do ...; done | tee log`
//! is not an exotic shape. Under 4 KiB it worked, which is what let it go unnoticed.
//!
//! The same investigation found `read -t` unimplemented — it failed outright with
//! "poll-based timeout is not supported on this platform", because the implementation was
//! `poll(2)`.
//!
//! The sizes here are deliberate, not arbitrary: each one straddles the pipe buffer.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed, because \
              alternating the two forms by accident of content reads worse."
)]

use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
}

fn cash(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
    }
}

/// Count lines out of a pipeline, trimmed of `wc`'s padding.
fn lines(script: &str) -> String {
    cash(&format!("{script} | wc -l | tr -d ' '")).stdout
}

// ---------------------------------------------------------------------------
// Every compound kind, above and below the pipe buffer
// ---------------------------------------------------------------------------

#[test]
fn a_brace_group_does_not_lose_output_above_the_pipe_buffer() {
    assert_eq!(lines("{ seq 1 5000; }"), "5000");
}

#[test]
fn a_subshell_does_not_lose_output() {
    assert_eq!(lines("( seq 1 5000 )"), "5000");
}

#[test]
fn a_for_loop_does_not_lose_output() {
    assert_eq!(
        lines(r#"for i in $(seq 1 5000); do echo "$i"; done"#),
        "5000"
    );
}

#[test]
fn a_while_loop_does_not_lose_output() {
    assert_eq!(
        lines(r#"i=0; while [ "$i" -lt 5000 ]; do echo "$i"; i=$((i+1)); done"#),
        "5000"
    );
}

#[test]
fn an_if_block_does_not_lose_output() {
    assert_eq!(lines("if true; then seq 1 5000; fi"), "5000");
}

#[test]
fn a_case_block_does_not_lose_output() {
    assert_eq!(lines("case x in x) seq 1 5000 ;; esac"), "5000");
}

#[test]
fn a_function_does_not_lose_output() {
    assert_eq!(lines("f() { seq 1 5000; }; f"), "5000");
}

#[test]
fn the_boundary_itself_is_covered() {
    // Just under and just over 4096 bytes of output. The old code was correct below and
    // lost everything above, so a fix that only moved the boundary would pass one of
    // these and fail the other.
    for count in [800_usize, 900, 1000, 1100, 1200] {
        assert_eq!(
            lines(&format!("{{ seq 1 {count}; }}")),
            count.to_string(),
            "lost output at {count} lines"
        );
    }
}

#[test]
fn a_very_large_compound_pipeline_is_intact() {
    assert_eq!(lines("{ seq 1 100000; }"), "100000");
}

#[test]
fn the_bytes_are_right_not_just_the_count() {
    // A line count can survive reordering or duplication; this cannot.
    let out = cash(r#"{ seq 1 20000; } | md5sum | cut -d' ' -f1"#);
    let direct = cash(r#"seq 1 20000 | md5sum | cut -d' ' -f1"#);
    assert_eq!(
        out.stdout, direct.stdout,
        "a compound pipeline produced different bytes from a plain one"
    );
    assert!(!out.stdout.is_empty(), "no checksum: {}", out.stderr);
}

#[test]
fn a_small_compound_pipeline_still_works() {
    // The case that always worked, so a fix cannot have broken it.
    let out = cash(r#"{ echo a; echo b; } | wc -l | tr -d ' '"#);
    assert_eq!(out.stdout, "2");
}

#[test]
fn a_compound_member_in_the_middle_of_a_pipeline_works() {
    let out = cash(r#"seq 1 5000 | { cat; } | wc -l | tr -d ' '"#);
    assert_eq!(out.stdout, "5000", "stderr: {}", out.stderr);
}

#[test]
fn two_compound_members_in_a_row_work() {
    let out = cash(r#"{ seq 1 5000; } | { cat; } | wc -l | tr -d ' '"#);
    assert_eq!(out.stdout, "5000", "stderr: {}", out.stderr);
}

#[test]
fn a_compound_producer_stops_when_the_consumer_leaves() {
    // `| head -1` closes the pipe early. The producer must not hang, and the pipeline
    // must still succeed.
    let out = cash(r#"{ seq 1 200000; } | head -1"#);
    assert_eq!(out.stdout, "1", "stderr: {}", out.stderr);
}

#[test]
fn lastpipe_still_sees_its_variables() {
    // The last member runs in the parent shell on purpose, so that assignments survive;
    // spawning it as a task would lose exactly that.
    let out = cash(r#"shopt -s lastpipe; set +m; echo hello | { read -r v; }; echo "v=[$v]""#);
    assert_eq!(out.stdout, "v=[hello]", "stderr: {}", out.stderr);
}

#[test]
fn a_pipelines_exit_status_is_still_the_last_members() {
    let ok = cash(r#"{ seq 1 5000; } | true; echo "rc=$?""#);
    assert_eq!(ok.stdout, "rc=0");

    let bad = cash(r#"{ seq 1 5000; } | false; echo "rc=$?""#);
    assert_eq!(bad.stdout, "rc=1");
}

#[test]
fn pipefail_still_reports_a_failing_producer() {
    let out = cash(r#"set -o pipefail; { seq 1 5000; false; } | cat > /dev/null; echo "rc=$?""#);
    assert_eq!(out.stdout, "rc=1", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// `read -t`
// ---------------------------------------------------------------------------

#[test]
fn read_with_a_timeout_is_supported_at_all() {
    // It used to fail with "poll-based timeout is not supported on this platform".
    let out = cash(r#"printf 'hello\n' | { read -t 5 -r v; echo "rc=$? v=[$v]"; }"#);
    assert_eq!(out.stdout, "rc=0 v=[hello]", "stderr: {}", out.stderr);
    assert!(
        !out.stderr.contains("not supported"),
        "read -t is still unsupported: {}",
        out.stderr
    );
}

#[test]
fn read_with_a_timeout_reads_a_regular_file() {
    // bash's `-t` has no effect on a regular file: it is always ready.
    let out = cash(
        r#"d=$(mktemp -d); printf 'from-file\n' > "$d/f"; read -t 5 -r v < "$d/f"; echo "rc=$? v=[$v]"; cd /; rm -rf "$d""#,
    );
    assert_eq!(out.stdout, "rc=0 v=[from-file]", "stderr: {}", out.stderr);
}

#[test]
fn read_with_a_timeout_reports_end_of_input() {
    let out = cash(r#"read -t 5 -r v < /dev/null; echo "rc=$?""#);
    assert_eq!(out.stdout, "rc=1", "stderr: {}", out.stderr);
}

#[test]
fn a_zero_timeout_is_a_readiness_check() {
    // `read -t 0` asks whether input is waiting, without consuming it.
    let out = cash(r#"printf 'x\n' | { read -t 0 -r v; echo "rc=$?"; }"#);
    assert!(
        out.stdout.starts_with("rc="),
        "read -t 0 produced nothing: {} {}",
        out.stdout,
        out.stderr
    );
}

#[test]
fn a_timeout_does_not_take_longer_than_it_says() {
    // The wait must be bounded even when the input never arrives.
    let started = std::time::Instant::now();
    let out = cash(r#"read -t 1 -r v < /dev/null; echo "rc=$?""#);
    let elapsed = started.elapsed();

    assert!(out.stdout.starts_with("rc="), "no result: {}", out.stderr);
    assert!(
        elapsed < std::time::Duration::from_secs(15),
        "read -t 1 took {elapsed:?}"
    );
}
