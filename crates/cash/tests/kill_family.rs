//! `pkill`, `pidof` and `killall` — ROADMAP item 10, the BusyBox-gap kill family.
//!
//! Formats and exit statuses were read off procps-ng 4.0.7 and psmisc 23.7. Names follow
//! the family's rules: case-insensitive, `.exe` optional. Most checks send signal 0,
//! which only asks whether a process could be signalled, so nothing is harmed; the
//! background `ping`s are the targets, and each script kills its own at the end.
//! `kill_term.rs` covers the default `TERM`.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction"
)]

use std::os::windows::process::CommandExt as _;
use std::process::{Command, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// A console of its own, without a window, so no signal can reach the test runner.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Output {
    stdout: String,
    stderr: String,
}

/// Runs `script` after starting two background `ping`s whose pids are `$a` and `$b`
/// (`$a` the older), and kills them afterwards.
fn with_two_pings(script: &str) -> Output {
    let full = format!(
        "ping -n 60 127.0.0.1 >/dev/null & a=$!; sleep 0.5; \
         ping -n 60 127.0.0.1 >/dev/null & b=$!; sleep 0.5; \
         {script}; kill -9 $a 2>/dev/null; kill -9 $b 2>/dev/null"
    );
    let out = Command::new(CASH)
        .args(["-c", &full])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_owned(),
    }
}

#[test]
fn the_family_is_builtin() {
    let out = with_two_pings("type pkill pidof killall");
    for name in ["pkill", "pidof", "killall"] {
        assert!(
            out.stdout.contains(&format!("{name} is a shell builtin")),
            "{}",
            out.stdout
        );
    }
}

#[test]
fn pidof_lists_highest_pid_first_on_one_line() {
    // Other tests run their own pings meanwhile, so check ours are there and the order
    // holds, not the exact list.
    let out = with_two_pings(
        r#"all=$(pidof ping); echo "$all"; case " $all " in *" $a "*) ;; *) echo missing-a ;; esac;            case " $all " in *" $b "*) ;; *) echo missing-b ;; esac;            [ "$(pidof -s PING.EXE | wc -w)" -eq 1 ] && echo single;            case " $(pidof -o $b ping) " in *" $b "*) echo not-omitted ;; *" $a "*) echo omit ;; esac;            pidof no-such-program; echo "none=$?""#,
    );
    let mut lines = out.stdout.lines();
    let pids: Vec<u32> = lines
        .next()
        .unwrap_or_default()
        .split(' ')
        .map(|pid| pid.parse().expect("a pid"))
        .collect();
    let mut sorted = pids.clone();
    sorted.sort_unstable_by(|x, y| y.cmp(x));
    assert_eq!(pids, sorted, "not highest first");
    assert_eq!(
        lines.collect::<Vec<_>>(),
        ["single", "omit", "none=1"],
        "{}",
        out.stderr
    );
}

#[test]
fn pkill_counts_echoes_and_picks_newest_or_oldest() {
    // -P $$ keeps to this shell's own two pings.
    let out = with_two_pings(
        r#"pkill -c -0 -P $$ -x ping; pkill -n -e -0 -P $$ -x ping | grep -q "(pid $b)$" && echo newest;            pkill -o -e -0 -P $$ -x ping | grep -q "(pid $a)$" && echo oldest;            pkill -c -0 -x no-such-program; echo "none=$?""#,
    );
    assert_eq!(out.stdout, "2\nnewest\noldest\n0\nnone=1", "{}", out.stderr);
}

#[test]
fn pkill_echo_names_the_image() {
    let out = with_two_pings("pkill -e -0 -x ping | head -1");
    assert!(
        out.stdout.ends_with(')') && out.stdout.contains(" killed (pid "),
        "{}",
        out.stdout
    );
}

#[test]
fn pkill_usage_errors_exit_2() {
    let out = with_two_pings(
        "pkill; echo none=$?; pkill a b 2>/dev/null; echo two=$?; pkill -f x 2>/dev/null; \
         echo full=$?; pkill -Z x 2>/dev/null; echo bad=$?",
    );
    assert_eq!(out.stdout, "none=2\ntwo=2\nfull=2\nbad=2");
    assert!(
        out.stderr
            .starts_with("pkill: no matching criteria specified"),
        "{}",
        out.stderr
    );
}

#[test]
fn killall_reports_what_it_signals_and_what_it_cannot_find() {
    let out = with_two_pings(
        r#"killall -v -0 ping no-such-program 2>&1 | grep -e "($a)" -e "($b)" -e no-such;            killall -0 ping no-such-program 2>/dev/null; echo rc=$?"#,
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 4, "{}", out.stdout);
    assert!(
        lines[..2]
            .iter()
            .all(|line| line.starts_with("Killed PING.EXE(") && line.ends_with(") with signal 0")),
        "{}",
        out.stdout
    );
    assert_eq!(lines[2..], ["no-such-program: no process found", "rc=1"]);
}

#[test]
fn killall_quiet_usage_signals_and_list() {
    let out = with_two_pings(
        "killall -q no-such-program; echo quiet=$?; killall 2>/dev/null; echo usage=$?; \
         killall -s ZZZ ping; echo bad=$?; killall -l | head -1",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.get(..3), Some(&["quiet=1", "usage=1", "bad=1"][..]));
    let names = lines.get(3).copied().unwrap_or_default();
    assert!(
        names.split(' ').any(|n| n == "TERM") && names.split(' ').any(|n| n == "KILL"),
        "{names}"
    );
    assert_eq!(out.stderr, "ZZZ: unknown signal; killall -l lists signals.");
}

#[test]
fn system_processes_and_the_shell_are_never_signalled() {
    let out = with_two_pings(
        r#"pkill -0 -x csrss; echo csrss=$?; killall -v -0 csrss 2>&1 | head -1; \
           killall -v -0 -r '^cash$' 2>&1 | grep -c "($$): the shell itself""#,
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.first(), Some(&"csrss=1"), "{}", out.stdout);
    assert!(
        lines
            .get(1)
            .is_some_and(|l| l.starts_with("Skipped csrss.exe(") && l.ends_with("a system process")),
        "{}",
        out.stdout
    );
    assert_eq!(lines.get(2), Some(&"1"), "{}", out.stdout);
}

#[test]
fn pgrep_shares_the_rules() {
    let out = with_two_pings(
        r#"[ "$(pgrep -P $$ -x PING | sort -n | tr '\n' ' ')" = "$(printf '%s\n' $a $b | sort -n | tr '\n' ' ')" ] && echo same"#,
    );
    assert_eq!(out.stdout, "same", "{}", out.stderr);
}
