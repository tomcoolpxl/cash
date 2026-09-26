//! `$!` and background job identity — **D11**, **D22**.
//!
//! `cmd & pid=$!` is the standard way a script keeps hold of something it started, and it
//! was a race that always lost. A background job in cash is a tokio task executing a
//! whole and-or list, so `&` returned before the task had spawned anything and `$!`
//! expanded to the empty string — reliably, not intermittently, which is worse: a script
//! then runs `kill ""` or `wait ""` and gets a parse error instead of the thing it asked
//! for.
//!
//! It surfaced while checking something else entirely: a test that claimed a child had
//! been reaped was really observing `kill -0 ""` failing to parse.
//!
//! `&` now waits until the task has either published a pid or reached a point where it
//! never will. Both arms resolve promptly, so a background job that runs forever does
//! not stall the shell — which is the failure mode any naive "wait for the pid" would
//! have.

#![cfg(windows)]
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
    code: i32,
}

fn cash(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

// ---------------------------------------------------------------------------
// The race
// ---------------------------------------------------------------------------

#[test]
fn an_external_command_sets_the_pid_immediately() {
    // No `sleep` before reading `$!`. That sleep is exactly what used to make this look
    // as though it worked.
    let out = cash(
        r#"ping.exe -n 5 127.0.0.1 > /dev/null & pid=$!; echo "[$pid]"; kill -KILL "$pid" 2>/dev/null; wait 2>/dev/null"#,
    );
    assert_ne!(out.stdout, "[]", "`$!` was empty: {}", out.stderr);
    assert!(
        out.stdout.trim_matches(['[', ']']).parse::<u32>().is_ok(),
        "`$!` was not a number: {}",
        out.stdout
    );
}

#[test]
fn a_bundled_utility_sets_the_pid_too() {
    // D48 dispatches a bundled utility as a builtin that re-enters the binary as a child
    // process, so its pid appears partway through the builtin rather than before it.
    // Releasing the waiter on builtin *entry* would leave this empty.
    let out =
        cash(r#"sleep 5 & pid=$!; echo "[$pid]"; kill -KILL "$pid" 2>/dev/null; wait 2>/dev/null"#);
    assert_ne!(
        out.stdout, "[]",
        "`$!` was empty for a bundled utility: {}",
        out.stderr
    );
}

#[test]
fn the_pid_is_the_one_that_can_be_signalled() {
    // A pid that cannot be used is no better than an empty one.
    let out = cash(
        r#"
        ping.exe -n 30 127.0.0.1 > /dev/null &
        pid=$!
        kill -0 "$pid" && echo alive
        kill -KILL "$pid"
        sleep 1
        kill -0 "$pid" 2>/dev/null && echo "still alive" || echo gone
        "#,
    );
    assert_eq!(out.stdout, "alive\ngone", "stderr: {}", out.stderr);
}

#[test]
fn wait_accepts_the_pid() {
    // `pid=$!; wait "$pid"` is the other half of the idiom.
    let out =
        cash(r#"ping.exe -n 2 127.0.0.1 > /dev/null & pid=$!; wait "$pid"; echo "waited rc=$?""#);
    assert_eq!(out.stdout, "waited rc=0", "stderr: {}", out.stderr);
}

#[test]
fn several_background_jobs_report_their_own_pids() {
    let out = cash(
        r#"
        ping.exe -n 20 127.0.0.1 > /dev/null & first=$!
        ping.exe -n 20 127.0.0.1 > /dev/null & second=$!
        if [ "$first" != "$second" ]; then echo distinct; else echo "same: $first"; fi
        kill -KILL "$first" "$second" 2>/dev/null
        kill -KILL "$first" 2>/dev/null; kill -KILL "$second" 2>/dev/null
        "#,
    );
    assert_eq!(out.stdout, "distinct", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// What the wait must never do
// ---------------------------------------------------------------------------

#[test]
fn a_background_job_that_never_ends_does_not_stall_the_shell() {
    // The trap in any "wait for the pid" scheme. This job spawns nothing and never
    // finishes, so a naive wait hangs the shell for good.
    let out = cash(r#"while true; do :; done & echo survived"#);
    assert_eq!(out.stdout, "survived", "the shell stalled: {}", out.stderr);
    assert_eq!(out.code, 0);
}

#[test]
fn a_background_job_keeps_running_after_the_ampersand_returns() {
    // Waiting for the *pid* must not become waiting for the *job*.
    let out = cash(
        r#"
        ping.exe -n 20 127.0.0.1 > /dev/null &
        pid=$!
        echo "returned"
        kill -0 "$pid" && echo "still running"
        kill -KILL "$pid" 2>/dev/null
        "#,
    );
    assert_eq!(
        out.stdout, "returned\nstill running",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn a_long_running_background_job_does_not_delay_the_next_command() {
    // If `&` waited for completion rather than for the pid, this would take 20 seconds.
    let started = std::time::Instant::now();
    let out =
        cash(r#"ping.exe -n 20 127.0.0.1 > /dev/null & echo immediate; kill -KILL $! 2>/dev/null"#);
    let elapsed = started.elapsed();

    assert_eq!(out.stdout, "immediate", "stderr: {}", out.stderr);
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "`&` waited for the job to finish: {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// The job table agrees
// ---------------------------------------------------------------------------

#[test]
fn the_job_table_lists_the_background_job() {
    let out = cash(r#"ping.exe -n 20 127.0.0.1 > /dev/null & jobs; kill -KILL $! 2>/dev/null"#);
    assert!(
        out.stdout.contains("[1]") && out.stdout.contains("Running"),
        "the job was not listed: {}",
        out.stdout
    );
}

#[test]
fn a_job_spec_reaps_the_same_process_the_pid_names() {
    // D22: `kill %1` and `kill $!` must agree about what they are aiming at.
    let out = cash(
        r#"
        ping.exe -n 30 127.0.0.1 > /dev/null &
        pid=$!
        kill -KILL %1
        sleep 1
        kill -0 "$pid" 2>/dev/null && echo "still alive" || echo reaped
        "#,
    );
    assert_eq!(out.stdout, "reaped", "stderr: {}", out.stderr);
}
