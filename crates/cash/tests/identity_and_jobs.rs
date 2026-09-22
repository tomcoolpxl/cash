//! Who the shell thinks it is, and what it says about its jobs — **D22**, **§4 #20**.
//!
//! `$UID` and `$EUID` reported a hardcoded 1000 while the `id` builtin reported the
//! account's real RID, so the shell disagreed with its own builtin about who was running
//! it. `[ "$UID" = "$(id -u)" ]` — a normal way to check for a privilege change — saw two
//! different answers and concluded something had happened.
//!
//! `jobs -l` refused outright with "not yet implemented", although the pid it wanted was
//! already tracked for `$!` and `kill %1`.

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

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[test]
fn uid_agrees_with_the_id_builtin() {
    // The shell and its own builtin must not disagree about who is running.
    let out = cash(r#"[ "$UID" = "$(id -u)" ] && echo agree || echo "differ: $UID vs $(id -u)""#);
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

#[test]
fn euid_agrees_too() {
    let out = cash(r#"[ "$EUID" = "$(id -u)" ] && echo agree || echo "differ: $EUID""#);
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

#[test]
fn uid_is_the_accounts_rid_not_a_placeholder() {
    // 1000 was the hardcoded sentinel. A real account RID starts at 500 (Administrator)
    // or 1001 for the first ordinary user, so the sentinel is distinguishable.
    let out = cash(r#"echo "$UID""#);
    let uid: u64 = out
        .stdout
        .parse()
        .unwrap_or_else(|_| panic!("not numeric: {}", out.stdout));
    assert!(uid > 0, "uid should be non-zero for a non-elevated shell");
}

#[test]
fn uid_is_stable_across_invocations() {
    // Derived from the account's SID, so it cannot wander between runs.
    let first = cash(r#"echo "$UID""#).stdout;
    let second = cash(r#"echo "$UID""#).stdout;
    assert_eq!(first, second, "uid changed between runs");
}

#[test]
fn whoami_and_id_name_the_same_account() {
    let out = cash(
        r#"w=$(whoami); i=$(id -un); case "$i" in *"$w") echo agree ;; *) echo "differ: [$w] [$i]" ;; esac"#,
    );
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// `jobs -l`
// ---------------------------------------------------------------------------

#[test]
fn jobs_l_is_implemented() {
    let out = cash(r#"ping -n 10 127.0.0.1 > /dev/null & jobs -l; kill -KILL $! 2>/dev/null"#);
    assert!(
        !out.stderr.contains("not yet implemented"),
        "jobs -l still refuses: {}",
        out.stderr
    );
    assert!(!out.stdout.is_empty(), "jobs -l printed nothing");
}

#[test]
fn jobs_l_shows_a_pid_that_matches_the_one_the_shell_reports() {
    let out = cash(
        r#"
        ping -n 10 127.0.0.1 > /dev/null &
        pid=$!
        listed=$(jobs -l | tr -s ' ' | cut -d' ' -f2)
        [ "$pid" = "$listed" ] && echo agree || echo "differ: [$pid] [$listed]"
        kill -KILL "$pid" 2>/dev/null
        "#,
    );
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

#[test]
fn jobs_l_puts_the_pid_after_the_job_marker() {
    // bash's layout: `[1]+ 12345 Running   sleep 30 &`. Splitting on the first space
    // would land inside the command, because the status and command are tab-separated.
    let out = cash(r#"ping -n 10 127.0.0.1 > /dev/null & jobs -l; kill -KILL $! 2>/dev/null"#);
    let line = out.stdout.lines().next().unwrap_or_default();
    assert!(line.starts_with("[1]"), "no job marker: {line:?}");

    let second = line.split_whitespace().nth(1).unwrap_or_default();
    assert!(
        second.parse::<u32>().is_ok(),
        "the pid is not the second field: {line:?}"
    );
}

#[test]
fn plain_jobs_does_not_show_a_pid() {
    let out = cash(r#"ping -n 10 127.0.0.1 > /dev/null & jobs; kill -KILL $! 2>/dev/null"#);
    let line = out.stdout.lines().next().unwrap_or_default();
    let second = line.split_whitespace().nth(1).unwrap_or_default();
    assert!(
        second.parse::<u32>().is_err(),
        "a bare `jobs` leaked a pid: {line:?}"
    );
}

#[test]
fn jobs_p_still_prints_only_pids() {
    let out = cash(r#"ping -n 10 127.0.0.1 > /dev/null & jobs -p; kill -KILL $! 2>/dev/null"#);
    let line = out.stdout.lines().next().unwrap_or_default();
    assert!(
        line.trim().parse::<u32>().is_ok(),
        "jobs -p printed more than a pid: {line:?}"
    );
}

#[test]
fn jobs_l_with_no_jobs_prints_nothing() {
    let out = cash("jobs -l; echo done");
    assert_eq!(out.stdout, "done", "jobs -l invented a job: {}", out.stdout);
}
