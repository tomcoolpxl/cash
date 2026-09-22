//! `ps` — **D48**, for the reason **D35** exists.
//!
//! uutils has no `ps`; it belongs to procps, a separate project. So on a typical Windows
//! machine the `ps` a user gets is the MSYS one from Git for Windows, and under cash that
//! is worse than nothing: it lists only MSYS processes, so every native program is
//! missing, and the numbers it prints are MSYS pids that `kill` cannot use. It looks like
//! it worked.
//!
//! The assertion that matters most here is not the formatting — it is that a pid `ps`
//! printed can be handed straight to `kill`.

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
// It is ours, not the one on PATH
// ---------------------------------------------------------------------------

#[test]
fn ps_is_a_builtin() {
    // If this resolves to an external, the MSYS one has won and the pids are wrong.
    let out = cash("type ps");
    assert!(
        out.stdout.contains("shell builtin"),
        "ps is not the builtin: {}",
        out.stdout
    );
}

#[test]
fn the_shell_lists_itself() {
    let out = cash("ps");
    assert!(
        out.stdout.to_lowercase().contains("cash.exe"),
        "the shell is missing from its own listing: {}",
        out.stdout
    );
}

#[test]
fn a_header_names_the_columns() {
    let out = cash("ps");
    let header = out.stdout.lines().next().unwrap_or_default();
    assert!(header.contains("PID"), "no PID column: {header:?}");
    assert!(header.contains("COMMAND"), "no COMMAND column: {header:?}");
    assert!(
        !header.contains("PPID"),
        "PPID appears without -f: {header:?}"
    );
}

// ---------------------------------------------------------------------------
// The pids work
// ---------------------------------------------------------------------------

#[test]
fn a_pid_from_ps_can_be_signalled() {
    // The whole reason for carrying this. An MSYS pid here would fail to resolve.
    let out = cash(
        r#"
        ping -n 30 127.0.0.1 > /dev/null &
        sleep 1
        pid=$(ps | grep -i 'PING' | tr -s ' ' | cut -d' ' -f2)
        echo "found=[$pid]"
        kill -KILL "$pid" && echo signalled
        "#,
    );
    assert!(
        out.stdout.contains("signalled"),
        "a pid from ps could not be signalled: {} {}",
        out.stdout,
        out.stderr
    );
}

#[test]
fn the_pid_matches_the_one_the_shell_reports() {
    // `$!` and `ps` must agree, or one of them is lying.
    let out = cash(
        r#"
        ping -n 30 127.0.0.1 > /dev/null &
        pid=$!
        sleep 1
        if ps | tr -s ' ' | cut -d' ' -f2 | grep -qx "$pid"; then echo agree; else echo differ; fi
        kill -KILL "$pid" 2>/dev/null
        "#,
    );
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

#[test]
fn the_default_listing_is_the_shells_own_tree() {
    // Linux's `ps` shows the processes attached to your terminal. Windows has no
    // controlling terminal, so cash shows its own descendants — which is what someone
    // typing a bare `ps` is looking for, and is a small number.
    let out = cash("ps | wc -l");
    let lines: usize = out.stdout.trim().parse().expect("a line count");
    assert!(
        (2..50).contains(&lines),
        "a bare `ps` listed {lines} lines, which is not a shell's own tree"
    );
}

#[test]
fn dash_e_lists_every_process() {
    let own = cash("ps | wc -l").stdout.trim().parse::<usize>().unwrap();
    let all = cash("ps -e | wc -l")
        .stdout
        .trim()
        .parse::<usize>()
        .unwrap();
    assert!(
        all > own,
        "`ps -e` listed no more than the shell's own tree: {all} vs {own}"
    );
    assert!(
        all > 20,
        "`ps -e` listed only {all} processes on a live machine"
    );
}

#[test]
fn the_bsd_spellings_are_accepted() {
    // `ps aux` and `ps ax` are in everyone's fingers, and their options carry no dash.
    for spelling in ["ps aux", "ps ax", "ps -ef", "ps -e -f"] {
        let out = cash(&format!("{spelling} | wc -l"));
        let lines: usize = out
            .stdout
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("{spelling} did not produce a count: {}", out.stderr));
        assert!(lines > 20, "{spelling} listed only {lines} processes");
    }
}

#[test]
fn dash_f_adds_the_parent_column() {
    let out = cash("ps -f");
    let header = out.stdout.lines().next().unwrap_or_default();
    assert!(header.contains("PPID"), "-f did not add PPID: {header:?}");

    let row = out.stdout.lines().nth(1).unwrap_or_default();
    let fields: Vec<&str> = row.split_whitespace().collect();
    assert!(fields.len() >= 3, "a -f row is missing fields: {row:?}");
    assert!(
        fields[0].parse::<u32>().is_ok(),
        "PID is not a number: {row:?}"
    );
    assert!(
        fields[1].parse::<u32>().is_ok(),
        "PPID is not a number: {row:?}"
    );
}

#[test]
fn an_unsupported_option_is_rejected_rather_than_ignored() {
    // Silently ignoring `-o pid,comm` would produce a listing that is not what was asked
    // for, which is the failure mode this project rejects everywhere else.
    let out = cash("ps -o pid 2>&1; echo rc=$?");
    assert!(
        !out.stdout.contains("rc=0"),
        "an unknown option was accepted: {}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[test]
fn every_row_starts_with_a_number() {
    let out = cash("ps -e");
    for row in out.stdout.lines().skip(1) {
        let first = row.split_whitespace().next().unwrap_or_default();
        assert!(
            first.parse::<u32>().is_ok(),
            "a row does not begin with a pid: {row:?}"
        );
    }
}

#[test]
fn the_listing_is_ordered_by_pid() {
    // So two runs are comparable, and `ps | head` means something.
    let out = cash("ps -e");
    let pids: Vec<u32> = out
        .stdout
        .lines()
        .skip(1)
        .filter_map(|row| row.split_whitespace().next()?.parse().ok())
        .collect();
    assert!(pids.len() > 20, "too few rows to judge ordering");
    assert!(
        pids.windows(2).all(|w| w[0] <= w[1]),
        "the listing is not ordered by pid"
    );
}

#[test]
fn ps_can_be_piped_without_a_broken_pipe_diagnostic() {
    // `ps | head` closes the pipe early; that is normal and must be silent.
    let out = cash("ps -e | head -3");
    assert_eq!(out.stdout.lines().count(), 3, "stderr: {}", out.stderr);
    assert!(
        !out.stderr.to_lowercase().contains("broken pipe"),
        "a broken pipe was reported: {}",
        out.stderr
    );
}
