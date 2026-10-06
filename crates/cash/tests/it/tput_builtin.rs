//! `tput`: ncurses 6.6's for `xterm-256color`, without terminfo.
//!
//! The oracle script in `tests/oracle` ran under ncurses' tput in WSL to make the `.out`
//! file, and runs here under cash. Where cash differs on purpose, the expected text is
//! replaced in the test, with the reason beside it, so a difference cannot hide in the
//! golden file.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::PathBuf;
use std::process::Stdio;

use crate::common::{cash_command, run};

fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("oracle")
}

/// Runs an oracle script under cash; standard output and error together, as the golden
/// file was made.
fn run_oracle_script(name: &str) -> String {
    let out = cash_command()
        .arg(format!("{name}.sh"))
        .current_dir(oracle_dir())
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(oracle_dir().join(format!("{name}.out")))
        .expect("read golden output")
        .replace("\r\n", "\n")
}

/// `golden` with `from` replaced by `to`, which must occur exactly once.
fn with_divergence(golden: &str, from: &str, to: &str) -> String {
    assert_eq!(golden.matches(from).count(), 1, "golden text moved: {from}");
    golden.replacen(from, to, 1)
}

#[test]
fn tput_matches_ncurses() {
    let expected = with_divergence(
        &golden("tput_cases"),
        // cash's own version line; the sequences are ncurses 6.6's.
        "== the version\nncurses 6.6.20251230\n",
        "== the version\ntput (cash): VT sequences, as ncurses 6.6 writes them\n",
    );
    let expected = with_divergence(
        &expected,
        // The terminal cash runs in is VT whatever TERM says, so neither an unset TERM
        // nor a non-VT one is refused; only an explicit -T is checked.
        "tput: No value for $TERM and no -T specified\nrc=2\nrc=1\n",
        " 033 [ 1 m\nrc=0\n 033 [ 1 m\nrc=0\n",
    );
    assert_eq!(run_oracle_script("tput_cases"), expected);
}

#[test]
fn tput_cols_is_a_positive_number_into_a_pipe() {
    // `cols=$(tput cols)` is what scripts do: standard output is a pipe, and the number
    // must still be the console's (or 80 without one), never an error.
    let out = run("tput cols; tput lines");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let numbers: Vec<i32> = out
        .stdout
        .lines()
        .map(|line| line.trim().parse().expect("a number"))
        .collect();
    assert_eq!(numbers.len(), 2, "{}", out.stdout);
    assert!(numbers.iter().all(|&n| n > 0), "{}", out.stdout);
    assert_eq!(out.stderr, "");
}

#[test]
fn tput_help_prints_the_usage_and_succeeds() {
    // ncurses treats `--help` as the invalid option `-`; a usage asked for is given.
    let out = run("tput --help");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout.starts_with("Usage: tput [options] [command]"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("-S <<"), "{}", out.stdout);
    assert_eq!(out.stderr, "");
}

#[test]
fn tput_help_page_and_version_name_no_internal_references() {
    for script in [
        "help tput",
        "tput --help",
        "tput -V",
        "tput -T dumb bold",
        "tput nosuch",
    ] {
        let out = run(script);
        let text = format!("{}\n{}", out.stdout, out.stderr);
        for banned in ["D73", "spec.md", "ROADMAP", "§4"] {
            assert!(!text.contains(banned), "{script}: {text}");
        }
    }
}
