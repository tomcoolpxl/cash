//! `kill` target semantics — **D21**, **D22**.
//!
//! POSIX gives the sign of a kill target a meaning:
//!
//! | Target | POSIX | cash on Windows |
//! |---|---|---|
//! | `pid` | that process | that process |
//! | `0` | every process in my process group | every process tree cash spawned |
//! | `-pid` | every process in that group | the tree rooted at `pid` |
//! | `-1` | every process I may signal | refused |
//!
//! `0` is the one that matters and the one that was wrong. Windows has no "the shell's
//! process group" that excludes the terminal, so cash reached for the console — and
//! `GenerateConsoleCtrlEvent(..., 0)` signals *every process attached to the console*.
//! `trap 'kill 0' EXIT`, an ordinary cleanup idiom, therefore killed the terminal and
//! anything else sharing it. It was found by running the absorbed differential suite on
//! Windows: the suite kept dying, silently, along with the shell that launched it.
//!
//! The fact that these tests complete at all is part of what they assert. A regression
//! here does not produce a failure message; it produces no output and a dead terminal.

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

use crate::common::run as cash;

/// What `wait "$child"; echo "status=$?"` prints for a child that `KILL` ended: 128 + 9.
/// A child nothing ended runs out its ten echoes and reports 0. `wait` is asked, rather
/// than `kill -0` a second later, because the status comes from the handle cash holds;
/// the pid may be another process's by then (see `process_identity.rs`).
const KILLED: &str = "status=137";

// ---------------------------------------------------------------------------
// Target 0 — the one that took the console down
// ---------------------------------------------------------------------------

#[test]
fn kill_zero_does_not_take_down_the_console() {
    // If this regresses, the test binary and the terminal running it both die, and the
    // failure arrives as silence.
    let out = cash("echo before; kill 0; echo after");
    assert_eq!(out.stdout, "before\nafter", "stderr: {}", out.stderr);
    assert_eq!(out.code, 0);
}

#[test]
fn kill_zero_with_no_children_is_a_quiet_no_op() {
    // bash signals its own process group, which in a shell that has spawned nothing is
    // just itself and its own handler; the script continues. Silence and success.
    let out = cash("kill 0; echo rc=$?");
    assert_eq!(out.stdout, "rc=0");
    assert!(
        out.stderr.is_empty(),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn kill_zero_reaps_the_shells_own_children() {
    // The useful half. `kill 0` has to *mean* something, or the fix is just a mute.
    let out = cash(
        r#"
        ping.exe -n 10 127.0.0.1 > /dev/null &
        child=$!
        sleep 1
        kill -KILL 0
        wait "$child"; echo "status=$?"
        "#,
    );
    assert_eq!(out.stdout, KILLED, "stderr: {}", out.stderr);
}

#[test]
fn kill_zero_leaves_unrelated_processes_alone() {
    // The other half of "not the console": a sibling process cash did not spawn must
    // survive. Started by this test, so it shares a console with the cash under test.
    let mut bystander = Command::new("ping")
        .args(["-n", "20", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn bystander");

    let out = cash("kill -KILL 0; echo done");
    assert_eq!(out.stdout, "done", "stderr: {}", out.stderr);

    let still_running = bystander.try_wait().expect("try_wait").is_none();
    let _ = bystander.kill();
    let _ = bystander.wait();

    assert!(
        still_running,
        "a process cash never spawned was killed by `kill 0`"
    );
}

#[test]
fn an_exit_trap_may_use_kill_zero() {
    // The idiom this all exists for: clean up whatever the script started, on the way
    // out, without naming every pid.
    let out = cash(
        r#"
        trap 'kill 0 2>/dev/null' EXIT
        ping.exe -n 30 127.0.0.1 > /dev/null &
        sleep 1
        echo finished
        "#,
    );
    assert_eq!(out.stdout, "finished", "stderr: {}", out.stderr);
    assert_eq!(out.code, 0);
}

// ---------------------------------------------------------------------------
// Ordinary targets still work
// ---------------------------------------------------------------------------

#[test]
fn a_bare_pid_still_targets_one_process() {
    let out = cash(
        r#"
        ping.exe -n 10 127.0.0.1 > /dev/null &
        child=$!
        sleep 1
        kill -KILL "$child"
        wait "$child"; echo "status=$?"
        "#,
    );
    assert_eq!(out.stdout, KILLED, "stderr: {}", out.stderr);
}

#[test]
fn kill_9_on_a_bare_pid_leaves_its_children_running() {
    // `kill -9 $pid` reaped the whole tree the pid roots, as `kill %1` does; a bare pid
    // is that process (D22, W32-06). The orphaned ping still finishes its four echoes
    // into the file and writes the summary; a job spec takes it with the shell.
    let out = cash(
        r#"
        cd "$(mktemp -d)" || exit
        finished() { for _ in $(seq 25); do grep -q 'Sent = 4' "$1" && return; sleep 0.2; done; return 1; }
        cmd.exe /d /s /c "ping.exe -n 4 127.0.0.1 > pid.txt" &
        sleep 1
        kill -KILL $!
        finished pid.txt && echo "pid: ping finished" || echo "pid: ping killed"
        cmd.exe /d /s /c "ping.exe -n 4 127.0.0.1 > job.txt" &
        sleep 1
        kill -KILL %%
        finished job.txt && echo "job: ping finished" || echo "job: ping killed"
        "#,
    );
    assert_eq!(
        out.stdout, "pid: ping finished\njob: ping killed",
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn a_negative_pid_reaps_that_tree() {
    // POSIX spells "the process group led by N" as `-N`. D22 already reaps trees for job
    // specs; this is the same scope, named by pid.
    let out = cash(
        r#"
        cmd.exe /d /s /c "ping.exe -n 10 127.0.0.1 > nul" &
        child=$!
        sleep 1
        kill -KILL -"$child"
        wait "$child"; echo "status=$?"
        "#,
    );
    assert_eq!(out.stdout, KILLED, "stderr: {}", out.stderr);
}

#[test]
fn kill_0_reports_whether_a_process_exists() {
    // `kill -0` is a liveness probe, not a signal, and scripts branch on it.
    let alive = cash(
        r#"sleep 5 & child=$!; kill -0 "$child" && echo yes || echo no; kill -KILL "$child" 2>/dev/null"#,
    );
    assert_eq!(alive.stdout, "yes", "stderr: {}", alive.stderr);

    let dead = cash(r#"kill -0 999999 2>/dev/null && echo yes || echo no"#);
    assert_eq!(dead.stdout, "no");
}

#[test]
fn signalling_a_nonexistent_pid_fails_without_side_effects() {
    // It must not fall back to a broadcast, and it must say so.
    let out = cash(r#"kill -TERM 999999; echo "rc=$?"; echo alive"#);
    assert!(
        out.stdout.contains("alive"),
        "the shell did not survive: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("rc=") && !out.stdout.contains("rc=0"),
        "a signal to a nonexistent pid reported success: {}",
        out.stdout
    );
}

#[test]
fn every_target_is_signalled() {
    // A second one was "too many jobs or processes specified" (BI-03). The status is 0
    // when one of them was signalled, as in Bash.
    let out = cash(
        r#"sleep 30 & a=$!; sleep 30 & b=$!; kill $a $b; echo "rc $?"; wait $a; echo "a $?"; wait $b; echo "b $?"; sleep 30 & c=$!; kill abc $c 2>/dev/null; echo "rc2 $?"; kill 999998 999999 2>/dev/null; echo "rc3 $?""#,
    );
    assert_eq!(
        out.stdout, "rc 0\na 143\nb 143\nrc2 0\nrc3 1",
        "{}",
        out.stderr
    );
}
