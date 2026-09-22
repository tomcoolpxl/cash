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
    // The default layout is real `ps`'s: pid, terminal, cpu time, command.
    let out = cash("ps");
    let header = out.stdout.lines().next().unwrap_or_default();
    assert!(header.contains("PID"), "no PID column: {header:?}");
    assert!(header.contains("TTY"), "no TTY column: {header:?}");
    assert!(header.contains("TIME"), "no TIME column: {header:?}");
    assert!(header.contains("CMD"), "no CMD column: {header:?}");
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
fn dash_f_is_the_system_v_layout() {
    // `UID PID PPID C STIME TTY TIME CMD`, in that order — a script reading `$2` for the
    // pid is reading real `ps`'s second column, not a homegrown one.
    let out = cash("ps -f");
    let header = out.stdout.lines().next().unwrap_or_default();
    for column in ["UID", "PID", "PPID", "C", "STIME", "TTY", "TIME", "CMD"] {
        assert!(
            header.contains(column),
            "-f is missing {column}: {header:?}"
        );
    }

    let row = out.stdout.lines().nth(1).unwrap_or_default();
    let fields: Vec<&str> = row.split_whitespace().collect();
    assert!(fields.len() >= 8, "a -f row is missing fields: {row:?}");
    assert!(
        fields[0].parse::<u32>().is_err(),
        "the first column should be the account, not a number: {row:?}"
    );
    assert!(
        fields[1].parse::<u32>().is_ok(),
        "PID is not the second column: {row:?}"
    );
    assert!(
        fields[2].parse::<u32>().is_ok(),
        "PPID is not the third column: {row:?}"
    );
}

#[test]
fn aux_is_the_bsd_layout() {
    // `USER PID %CPU %MEM VSZ RSS TTY STAT START TIME COMMAND`.
    let out = cash("ps aux");
    let header = out.stdout.lines().next().unwrap_or_default();
    for column in [
        "USER", "PID", "%CPU", "%MEM", "VSZ", "RSS", "TTY", "STAT", "START", "TIME", "COMMAND",
    ] {
        assert!(
            header.contains(column),
            "aux is missing {column}: {header:?}"
        );
    }

    let row = out.stdout.lines().nth(1).unwrap_or_default();
    let fields: Vec<&str> = row.split_whitespace().collect();
    assert!(fields.len() >= 11, "an aux row is missing fields: {row:?}");
    assert!(
        fields[1].parse::<u32>().is_ok(),
        "PID is not the second column: {row:?}"
    );
    assert!(
        fields[2].parse::<f64>().is_ok(),
        "%CPU is not a number: {row:?}"
    );
    assert!(
        fields[3].parse::<f64>().is_ok(),
        "%MEM is not a number: {row:?}"
    );
    assert!(
        fields[4].parse::<u64>().is_ok() && fields[5].parse::<u64>().is_ok(),
        "VSZ and RSS are not numbers: {row:?}"
    );
}

#[test]
fn ef_and_aux_are_not_the_same_listing() {
    // They were: every spelling printed the same two homegrown columns, so `ps -ef` and
    // `ps aux` were indistinguishable and neither matched what a script expected.
    let ef = cash("ps -ef")
        .stdout
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let aux = cash("ps aux")
        .stdout
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    assert_ne!(ef, aux, "-ef and aux print the same header");
    assert!(ef.contains("PPID"), "-ef lost its PPID column: {ef:?}");
    assert!(aux.contains("%CPU"), "aux lost its %CPU column: {aux:?}");
}

#[test]
fn the_user_column_names_the_account_running_the_shell() {
    // `id -un` answers with the full Windows identity, `DOMAIN\user`; `whoami` and this
    // column give the account name alone, which is what `ps`'s `USER` is on Linux and
    // what fits a column. The two must still be naming the same account, which is the
    // comparison the identity tests already make.
    let out = cash(
        r#"me=$(id -un); mine=$(ps -f | tr -s ' ' | grep -i "cash.exe" | head -1 | cut -d' ' -f1); case "$me" in *"$mine") echo agree ;; *) echo "differ: [$me] [$mine]" ;; esac"#,
    );
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

#[test]
fn the_shell_reports_a_working_set() {
    // An all-zero memory row means the per-process query failed rather than the process
    // being small.
    //
    // Selected by `$$` rather than by name: the tests run in parallel, so several
    // `cash.exe` rows are in the listing and the first one is somebody else's.
    let out = cash(
        r#"ps aux | while read -r user pid cpu mem vsz rss rest; do [ "$pid" = "$$" ] && echo "$rss"; done"#,
    );
    let rss: u64 = out.stdout.trim().parse().unwrap_or(0);
    assert!(
        rss > 0,
        "no resident memory for the shell's own row: [{}] {}",
        out.stdout,
        out.stderr
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
