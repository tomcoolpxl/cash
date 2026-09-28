//! `disown`, the last of the job-control builtins that still refused — **D6**, **D22**,
//! **§4 #23**.
//!
//! `cmd & disown` is how a script hands off a long-lived child and stops the shell
//! accounting for it. cash answered it with "unimplemented built-in" and exit 99, so the
//! idiom failed loudly in the middle of a script that had already started the child.
//!
//! Half of bash's meaning cannot follow to Windows: a disowned job does not outlive the
//! shell, because every process cash spawns is in the session job object and Windows has
//! no way out of one (§4 #23 — `detach` is the way to start a process that survives). The
//! other half — the shell forgetting the job — is exactly what these tests pin down.

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

/// A background job that outlives the script running it, without needing a `sleep`.
const BACKGROUND: &str = "ping.exe -n 20 127.0.0.1 > /dev/null &";

// ---------------------------------------------------------------------------
// Forgetting a job
// ---------------------------------------------------------------------------

#[test]
fn disown_no_longer_refuses() {
    let out = cash(&format!(r#"{BACKGROUND} disown %1"#));
    assert!(
        !out.stderr.contains("unimplemented"),
        "disown still refuses: {}",
        out.stderr
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
}

#[test]
fn the_job_leaves_the_table() {
    let out = cash(&format!(r#"{BACKGROUND} disown %1; jobs; echo "end: $?""#));
    assert_eq!(
        out.stdout, "end: 0",
        "the job is still listed: {}",
        out.stdout
    );
}

#[test]
fn the_other_jobs_stay() {
    let out = cash(&format!(r#"{BACKGROUND} {BACKGROUND} disown %1; jobs"#));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 1, "expected one job left: {lines:?}");
    assert!(
        lines[0].starts_with("[2]"),
        "the wrong job was forgotten: {:?}",
        lines[0]
    );
}

#[test]
fn a_bare_disown_takes_the_current_job() {
    // bash disowns `%+` when given nothing else, which is the newest job.
    let out = cash(&format!(r#"{BACKGROUND} {BACKGROUND} disown; jobs"#));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 1, "expected one job left: {lines:?}");
    assert!(
        lines[0].starts_with("[1]"),
        "the current job was not the one forgotten: {:?}",
        lines[0]
    );
}

#[test]
fn a_pid_names_its_job() {
    let out = cash(&format!(
        r#"{BACKGROUND} disown "$!"; jobs; echo "end: $?""#
    ));
    assert_eq!(
        out.stdout, "end: 0",
        "a pid did not resolve to its job: {} {}",
        out.stdout, out.stderr
    );
}

// ---------------------------------------------------------------------------
// The process itself is left alone
// ---------------------------------------------------------------------------

#[test]
fn the_disowned_process_keeps_running() {
    // Forgetting a job must not be a kill: the whole point of `cmd & disown` is that the
    // command carries on. Dropping the job drops the child handle, which would terminate
    // the process if it had been spawned with `kill_on_drop`.
    let out = cash(&format!(
        r#"{BACKGROUND} p=$!; disown %1; sleep 2; kill -0 "$p" && echo alive || echo gone"#
    ));
    assert_eq!(out.stdout, "alive", "stderr: {}", out.stderr);
}

#[test]
fn wait_no_longer_waits_for_it() {
    // The job is gone from the table, so a bare `wait` has nothing to wait for. Without
    // this, a script ending in `wait` would block for the child's full lifetime — the
    // thing `disown` is used to avoid.
    let started = std::time::Instant::now();
    let out = cash(&format!(r#"{BACKGROUND} disown %1; wait; echo done"#));
    assert_eq!(out.stdout, "done", "stderr: {}", out.stderr);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "wait blocked on a disowned job for {:?}",
        started.elapsed()
    );
}

#[test]
fn a_disowned_job_can_no_longer_be_signalled_by_spec() {
    // bash: the spec is gone with the job, and `kill %1` fails rather than finding
    // something else. The pid still works, which is why `disown "$!"` keeps a handle.
    let out = cash(&format!(
        r#"{BACKGROUND} disown %1; kill %1; echo "code: $?""#
    ));
    assert_eq!(out.stdout, "code: 1", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("no such job"),
        "no diagnostic for the vanished spec: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[test]
fn with_no_jobs_it_names_the_current_job() {
    // bash says `current`, not `%1` — it never saw a spec to echo back.
    let out = cash(r#"disown"#);
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("current: no such job"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn an_unknown_spec_fails() {
    let out = cash(&format!(r#"{BACKGROUND} disown %9"#));
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("%9: no such job"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn an_unknown_pid_fails() {
    let out = cash(&format!(r#"{BACKGROUND} disown 999999"#));
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("999999: no such job"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn a_bare_word_is_told_what_it_is_missing() {
    // bash warns before failing, because the argument could not have been a spec at all.
    let out = cash(&format!(r#"{BACKGROUND} disown foo"#));
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr
            .contains("warning: foo: job specification requires leading `%'"),
        "no warning about the missing %: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("foo: no such job"),
        "no failure after the warning: {}",
        out.stderr
    );
}

#[test]
fn a_good_spec_still_applies_when_another_fails() {
    let out = cash(&format!(
        r#"{BACKGROUND} {BACKGROUND} disown %1 %9; echo "code: $?"; jobs"#
    ));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.first().copied(), Some("code: 1"), "stdout: {lines:?}");
    assert_eq!(lines.len(), 2, "expected one job left: {lines:?}");
    assert!(
        lines[1].starts_with("[2]"),
        "the good spec was not applied: {:?}",
        lines[1]
    );
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[test]
fn dash_a_clears_the_table() {
    let out = cash(&format!(
        r#"{BACKGROUND} {BACKGROUND} disown -a; echo "code: $?"; jobs"#
    ));
    assert_eq!(out.stdout, "code: 0", "jobs survived -a: {}", out.stdout);
}

#[test]
fn dash_a_ignores_a_spec_given_with_it() {
    // bash's `-a` means all, and a spec alongside it does not narrow it.
    let out = cash(&format!(
        r#"{BACKGROUND} {BACKGROUND} disown -a %1; echo "code: $?"; jobs"#
    ));
    assert_eq!(out.stdout, "code: 0", "jobs survived -a %1: {}", out.stdout);
}

#[test]
fn dash_r_clears_the_running_jobs() {
    let out = cash(&format!(
        r#"{BACKGROUND} {BACKGROUND} disown -r; echo "code: $?"; jobs"#
    ));
    assert_eq!(
        out.stdout, "code: 0",
        "running jobs survived -r: {}",
        out.stdout
    );
}

#[test]
fn dash_h_keeps_the_job_listed() {
    // On Windows the mark has nothing to suppress — there is no SIGHUP, and the session
    // job object does not ask (§4 #23). What bash's -h observably does is keep the job,
    // and that part holds.
    let out = cash(&format!(
        r#"{BACKGROUND} disown -h %1; echo "code: $?"; jobs"#
    ));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("code: 0"),
        "stderr: {}",
        out.stderr
    );
    assert_eq!(lines.len(), 2, "-h dropped the job: {lines:?}");
    assert!(lines[1].starts_with("[1]"), "unexpected listing: {lines:?}");
}

#[test]
fn dash_h_still_checks_the_spec() {
    let out = cash(&format!(r#"{BACKGROUND} disown -h %9"#));
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("%9: no such job"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

#[test]
fn an_option_after_the_spec_is_a_spec() {
    // bash reads only the leading words as options, so `disown %1 -h` looks for a job
    // called `-h`. Matching that matters more than being forgiving: a script written
    // against bash gets the same answer either way.
    let out = cash(&format!(r#"{BACKGROUND} disown %1 -h"#));
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("-h: no such job"),
        "-h after the spec was taken as an option: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// The job table around it
// ---------------------------------------------------------------------------

#[test]
fn the_next_job_gets_a_fresh_id() {
    // Job ids used to be `len() + 1`, so disowning %1 of two jobs handed the next job the
    // id 2 — a duplicate, and `%2` then resolved to whichever came first.
    let out = cash(&format!(
        r#"{BACKGROUND} {BACKGROUND} disown %1; {BACKGROUND} jobs"#
    ));
    let ids: Vec<&str> = out
        .stdout
        .lines()
        .filter_map(|line| line.split(']').next())
        .collect();
    assert_eq!(ids, vec!["[2", "[3"], "ids collided: {}", out.stdout);
}

#[test]
fn exactly_one_job_is_marked_previous() {
    // Adding a job demoted the current one to previous without clearing the old previous,
    // so three jobs rendered `[1]- [2]- [3]+` where bash renders `[1] [2]- [3]+`.
    let out = cash(&format!(r#"{BACKGROUND} {BACKGROUND} {BACKGROUND} jobs"#));
    let previous = out.stdout.lines().filter(|l| l.contains("]-")).count();
    let current = out.stdout.lines().filter(|l| l.contains("]+")).count();
    assert_eq!(previous, 1, "not exactly one %-: {}", out.stdout);
    assert_eq!(current, 1, "not exactly one %+: {}", out.stdout);
}

#[test]
fn the_marks_move_when_the_current_job_goes() {
    // bash hands `%+` back to the previous job and `%-` to the one before that.
    let out = cash(&format!(
        r#"{BACKGROUND} {BACKGROUND} {BACKGROUND} disown %3; jobs"#
    ));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "expected two jobs left: {lines:?}");
    assert!(lines[0].starts_with("[1]-"), "unexpected marks: {lines:?}");
    assert!(lines[1].starts_with("[2]+"), "unexpected marks: {lines:?}");
}

#[test]
fn the_current_spec_follows_the_marks() {
    let out = cash(&format!(
        r#"{BACKGROUND} {BACKGROUND} {BACKGROUND} disown %3; jobs %+"#
    ));
    assert!(
        out.stdout.starts_with("[2]"),
        "%+ did not follow: {} {}",
        out.stdout,
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// Subshells
// ---------------------------------------------------------------------------

#[test]
fn a_subshell_disowns_its_own_copy() {
    // bash's subshell is a fork, so it edits a copy of the job table and the parent keeps
    // its job. cash's subshell sees the parent's jobs as read-only snapshots; disowning
    // one has to succeed against that view rather than report a job that plainly is
    // listed.
    let out = cash(&format!(
        r#"{BACKGROUND} said=$(disown %1 2>&1); echo "[$said]"; jobs"#
    ));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.first().copied(), Some("[]"), "stderr: {}", out.stderr);
    assert_eq!(lines.len(), 2, "the parent lost its job: {lines:?}");
    assert!(lines[1].starts_with("[1]"), "unexpected listing: {lines:?}");
}
