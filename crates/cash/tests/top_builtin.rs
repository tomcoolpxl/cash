//! `top` — **D48**, one step past `ps`.
//!
//! Windows ships no `top` and nothing on a normal machine supplies one: procps was never
//! ported, and the Cygwin build that comes with Git for Windows reports Cygwin pids for
//! Cygwin processes only — the same trap that made carrying `ps` necessary. So `top` was
//! simply "command not found".
//!
//! Every test here drives batch mode, which is what makes them repeatable: a fixed number
//! of refreshes, a short delay, plain text, no screen control.

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

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// One refresh, as short as `top` allows, so the tests stay quick and deterministic.
const ONCE: &str = "top -b -n 1 -d 0.1";

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

/// The rows of a refresh: everything after the header block.
fn rows(stdout: &str) -> Vec<Vec<String>> {
    stdout
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("PID"))
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_whitespace()
                .map(std::string::ToString::to_string)
                .collect()
        })
        .collect()
}

fn column_index(stdout: &str, name: &str) -> usize {
    stdout
        .lines()
        .find(|line| line.trim_start().starts_with("PID"))
        .expect("process header")
        .split_whitespace()
        .position(|field| field == name)
        .unwrap_or_else(|| panic!("missing {name} column"))
}

// ---------------------------------------------------------------------------
// It exists, and it is ours
// ---------------------------------------------------------------------------

#[test]
fn top_is_a_builtin() {
    let out = cash("type top");
    assert!(
        out.stdout.contains("shell builtin"),
        "top is not the builtin: {} {}",
        out.stdout,
        out.stderr
    );
}

#[test]
fn a_refresh_has_a_header_and_a_table() {
    let out = cash(ONCE);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(
        lines.first().unwrap_or(&"").starts_with("top - "),
        "no summary line: {lines:?}"
    );
    assert!(
        lines.get(1).unwrap_or(&"").starts_with("Tasks:"),
        "no task summary: {lines:?}"
    );
    assert!(
        lines.get(2).unwrap_or(&"").starts_with("%Cpu(s):"),
        "no CPU summary: {lines:?}"
    );
    assert!(
        lines.get(3).unwrap_or(&"").starts_with("MiB Mem :"),
        "no memory summary: {lines:?}"
    );
    assert!(
        lines.get(4).unwrap_or(&"").starts_with("MiB Commit:"),
        "no commit summary: {lines:?}"
    );
    assert!(
        out.stdout.contains("PID")
            && out.stdout.contains(" PRI")
            && out.stdout.contains(" VIRT")
            && out.stdout.contains(" RES")
            && out.stdout.contains(" S ")
            && out.stdout.contains("%CPU")
            && out.stdout.contains("%MEM")
            && out.stdout.contains("TIME+")
            && out.stdout.contains("COMMAND"),
        "no column header: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("n/a"),
        "placeholder leaked: {}",
        out.stdout
    );
}

#[test]
fn the_shell_appears_in_its_own_listing() {
    let out = cash(ONCE);
    assert!(
        out.stdout.to_lowercase().contains("cash.exe"),
        "the shell is missing from the listing: {}",
        out.stdout
    );
}

#[test]
fn a_pid_in_the_listing_is_a_real_windows_pid() {
    // The whole reason for carrying this, as with `ps`: the numbers have to be ones
    // `kill` can use.
    let out = cash(&format!(
        r#"{ONCE} | tr -s ' ' | cut -d' ' -f2 | tail -n +1 > /dev/null; {ONCE}"#
    ));
    let first = rows(&out.stdout);
    let row = first.first().expect("at least one process");
    assert!(
        row[0].parse::<u32>().is_ok(),
        "the first column is not a pid: {row:?}"
    );
}

#[test]
fn the_shells_own_pid_is_listed() {
    let out = cash(&format!(r#"echo "me=$$"; {ONCE}"#));
    let me = out
        .stdout
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("me="))
        .unwrap_or_default()
        .to_string();
    assert!(
        rows(&out.stdout).iter().any(|row| row[0] == me),
        "the shell's own pid {me} is not in the listing"
    );
}

// ---------------------------------------------------------------------------
// Refreshing
// ---------------------------------------------------------------------------

#[test]
fn n_controls_the_number_of_refreshes() {
    let out = cash("top -b -n 2 -d 0.1");
    let refreshes = out
        .stdout
        .lines()
        .filter(|line| line.starts_with("top - "))
        .count();
    assert_eq!(refreshes, 2, "expected two refreshes: {}", out.stdout);
}

#[test]
fn batch_mode_lists_every_process() {
    // The refreshing display trims itself to the window — printing four hundred rows
    // scrolled the summary and the column header off the top, leaving a list with
    // nothing to read it by. A batch run is for a pipe or a log, so it keeps everything,
    // and this is the half of that rule a test can hold: a screen's worth is about 20
    // rows, and a live Windows machine runs many times that.
    let listed = rows(&cash(ONCE).stdout).len();
    assert!(
        listed > 50,
        "batch mode listed only {listed} processes, which looks like a screen rather than a machine"
    );
}

#[test]
fn batch_mode_writes_plain_lines() {
    // No screen control, so the output can go in a pipe or a log.
    let out = cash(ONCE);
    assert!(
        !out.stdout.contains('\x1b'),
        "batch mode emitted an escape sequence"
    );
    assert!(
        !out.stdout.contains('\r'),
        "batch mode emitted a carriage return"
    );
}

// ---------------------------------------------------------------------------
// Ordering and filtering
// ---------------------------------------------------------------------------

#[test]
fn the_listing_is_ordered_by_processor_share() {
    let out = cash(ONCE);
    let cpu = column_index(&out.stdout, "%CPU");
    let shares: Vec<f64> = rows(&out.stdout)
        .iter()
        .filter_map(|row| row.get(cpu).and_then(|field| field.parse::<f64>().ok()))
        .collect();
    assert!(shares.len() > 1, "too few rows to check ordering");
    assert!(
        shares.windows(2).all(|pair| pair[0] >= pair[1]),
        "not ordered by %CPU: {shares:?}"
    );
}

#[test]
fn o_mem_orders_by_memory_instead() {
    let out = cash("top -b -n 1 -d 0.1 -o mem");
    let memory = column_index(&out.stdout, "%MEM");
    let shares: Vec<f64> = rows(&out.stdout)
        .iter()
        .filter_map(|row| row.get(memory).and_then(|field| field.parse::<f64>().ok()))
        .collect();
    assert!(shares.len() > 1, "too few rows to check ordering");
    assert!(
        shares.windows(2).all(|pair| pair[0] >= pair[1]),
        "not ordered by %MEM: {shares:?}"
    );
}

#[test]
fn u_keeps_only_one_users_processes() {
    // `id -un` gives the full Windows identity, `DOMAIN\user`; the listing's column gives
    // the account name alone. `-u` takes either, which is the point — a user reaching for
    // `top -u "$(id -un)"` should not have to know the difference.
    let out = cash(&format!(r#"me=$(id -un); {ONCE} -u "$me""#));
    let listed = rows(&out.stdout);
    assert!(!listed.is_empty(), "no rows: {}", out.stdout);

    let full = cash("id -un").stdout;
    let account = full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string();
    // The USER column is ten characters wide, so a longer name (GitHub's `runneradmin`)
    // is listed cut short.
    let column: String = account.chars().take(10).collect();
    assert!(
        listed
            .iter()
            .all(|row| row[1].eq_ignore_ascii_case(&column)),
        "another account's process survived the filter: {listed:?}"
    );
}

#[test]
fn an_unknown_sort_field_is_refused_rather_than_ignored() {
    // Silently sorting by something else would be a listing that is not what was asked
    // for, which is the failure mode this project rejects everywhere.
    let out = cash("top -b -n 1 -d 0.1 -o nonsense");
    assert_eq!(out.code, 1, "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("nonsense"),
        "unexpected diagnostic: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// The numbers
// ---------------------------------------------------------------------------

#[test]
fn the_memory_line_adds_up() {
    let out = cash(ONCE);
    let line = out
        .stdout
        .lines()
        .nth(3)
        .expect("a memory line")
        .to_string();

    let numbers: Vec<f64> = line
        .split_whitespace()
        .filter_map(|field| field.parse::<f64>().ok())
        .collect();
    assert_eq!(numbers.len(), 3, "expected total, free and used: {line:?}");
    assert!(numbers[0] > 0.0, "no total memory: {line:?}");
    // Each figure is rounded to 0.1 on its own, so the three can disagree by up to
    // 3 × 0.05 (8186.7 total, 5658.6 free, 2528.2 used on GitHub's runner).
    assert!(
        (numbers[0] - numbers[1] - numbers[2]).abs() <= 0.151,
        "free and used do not add to total: {line:?}"
    );
}

#[test]
fn the_commit_line_reports_windows_commit_accounting() {
    let out = cash(ONCE);
    let line = out.stdout.lines().nth(4).expect("a commit line");
    let numbers: Vec<f64> = line
        .split_whitespace()
        .filter_map(|field| field.parse::<f64>().ok())
        .collect();
    assert_eq!(numbers.len(), 3, "expected used, limit and peak: {line:?}");
    assert!(numbers[0] > 0.0, "no committed memory: {line:?}");
    assert!(numbers[0] <= numbers[1], "commit exceeds limit: {line:?}");
    assert!(
        numbers[0] <= numbers[2],
        "commit exceeds boot peak: {line:?}"
    );
}

#[test]
fn the_shell_has_a_working_set() {
    // A row whose memory is all zeroes means the per-process query failed.
    let out = cash(&format!(r#"echo "me=$$"; {ONCE}"#));
    let me = out
        .stdout
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("me="))
        .unwrap_or_default()
        .to_string();

    let row = rows(&out.stdout)
        .into_iter()
        .find(|row| row[0] == me)
        .expect("the shell's own row");

    let resident = row[column_index(&out.stdout, "RES")]
        .trim_end_matches('M')
        .parse::<u64>()
        .unwrap_or(0);
    assert!(
        resident > 0,
        "the shell reports no resident memory: {row:?}"
    );
}

// ---------------------------------------------------------------------------
// Interactive controls
// ---------------------------------------------------------------------------

#[test]
fn question_mark_opens_help_and_q_exits() {
    let mut session = ConPtySession::start(&PathBuf::from(CASH), &["-c", "top -d 5"], None)
        .expect("start cash in a pseudo terminal");

    session.send("?").expect("send help key");
    session
        .expect("cash top - interactive help", Duration::from_secs(5))
        .expect("help screen");
    session.send("x").expect("leave help");
    session.send("r").expect("request a refresh");
    session
        .expect("Tasks:", Duration::from_secs(5))
        .expect("dashboard after help");
    session
        .expect("\x1b[7m", Duration::from_secs(2))
        .expect("highlighted process header");
    session.send("q").expect("quit top");
    assert_eq!(session.wait().expect("top exits"), 0);
}
