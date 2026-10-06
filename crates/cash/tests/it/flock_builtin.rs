//! `flock`, util-linux's, against its golden output, and the lock itself: held across
//! processes, invisible to readers, and living as long as a descriptor does.
//!
//! The oracle script ran under util-linux 2.42.3 in WSL to make `flock_cases.out`, and
//! runs here under cash. Where cash differs on purpose, the expected text is replaced in
//! the test, with the reason beside it, so a difference cannot hide in the golden file.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::needless_raw_string_hashes,
    clippy::literal_string_with_formatting_args,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::common::{Scratch, cash_command, run_in};

fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("oracle")
}

/// Runs the oracle script under cash; standard output and error together, as the golden
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
fn flock_matches_util_linux() {
    let expected = with_divergence(
        &golden("flock_cases"),
        // cash's version line names what the lock is made of.
        "flock from util-linux 2.42.3\n",
        "flock (cash): util-linux 2.42.3's options, on LockFileEx\n",
    );
    // util-linux's `--verbose` lines are block-buffered into a pipe, so they come out
    // after the command's output there; cash writes them before it runs the command, as
    // util-linux's appear on a terminal. And `-c` runs its string in a copy of this
    // shell, not in `$SHELL`.
    let expected = with_divergence(
        &expected,
        "== --verbose\nhi\nflock: getting lock took N seconds\nflock: executing echo\n",
        "== --verbose\nflock: getting lock took N seconds\nflock: executing echo\nhi\n",
    );
    let expected = with_divergence(
        &expected,
        "== --verbose -c\nhi\nflock: getting lock took N seconds\nflock: executing /usr/bin/bash\n",
        "== --verbose -c\nflock: getting lock took N seconds\nflock: executing cash\nhi\n",
    );
    let expected = with_divergence(
        &expected,
        // A pipe is not a file; LockFileEx takes only a file's handle.
        "== a descriptor that is a pipe\nrc=0\n",
        "== a descriptor that is a pipe\nflock: 0: not a file; Windows can lock only a file\nrc=65\n",
    );
    let expected = with_divergence(
        &expected,
        // A directory cannot be locked on Windows: refused with the status of a lock
        // file that could not be opened.
        "== a directory\nrc=0\n",
        "== a directory\nflock: d: cannot lock a directory on Windows; use a file inside it\nrc=66\n",
    );
    assert_eq!(run_oracle_script("flock_cases"), expected);
}

#[test]
fn two_processes_contend_for_the_lock() {
    let scratch = Scratch::new("flock-processes");
    let lock = scratch.as_script_path() + "/f";
    let mut holder = cash_command()
        .args(["-c", &format!("flock '{lock}' sleep 2")])
        .stdin(Stdio::null())
        .spawn()
        .expect("start the holder");
    // The holder has the lock once the file exists; give it a moment past that.
    let started = Instant::now();
    while !scratch.join("f").exists() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));

    let refused = run_in(scratch.path(), "flock -n f true; echo \"rc=$?\"");
    assert_eq!(refused.stdout, "rc=1", "{}", refused.stderr);
    let coded = run_in(scratch.path(), "flock -n -E 7 f true; echo \"rc=$?\"");
    assert_eq!(coded.stdout, "rc=7", "{}", coded.stderr);
    let timed_out = run_in(scratch.path(), "flock -w 0.3 f true; echo \"rc=$?\"");
    assert_eq!(timed_out.stdout, "rc=1", "{}", timed_out.stderr);

    // The holder's `sleep 2` ends, and the lock with it: a wait long enough gets it.
    let waited = run_in(scratch.path(), "flock -w 5 f echo got; echo \"rc=$?\"");
    assert_eq!(waited.stdout, "got\nrc=0", "{}", waited.stderr);
    let status = holder.wait().expect("the holder ends");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn a_held_lock_keeps_no_reader_out() {
    let scratch = Scratch::new("flock-reader");
    std::fs::write(scratch.join("f"), "the content\n").expect("write the lock file");
    let out = run_in(
        scratch.path(),
        "flock f sleep 1 & sleep 0.3; cat f; echo appended >> f; wait; cat f",
    );
    assert_eq!(
        out.stdout, "the content\nthe content\nappended",
        "{}",
        out.stderr
    );
}

#[test]
fn the_descriptor_form_holds_the_lock_while_the_subshell_does() {
    let scratch = Scratch::new("flock-descriptor");
    let out = run_in(
        scratch.path(),
        r#"( flock -n 9 || exit 99; echo got; flock -n f true; echo "inside rc=$?" ) 9>f
echo "subshell rc=$?"
flock -n f echo free; echo "after rc=$?""#,
    );
    assert_eq!(
        out.stdout, "got\ninside rc=1\nsubshell rc=0\nfree\nafter rc=0",
        "{}",
        out.stderr
    );
}

#[test]
fn the_descriptor_form_locks_for_another_process_too() {
    let scratch = Scratch::new("flock-descriptor-process");
    let lock = scratch.as_script_path() + "/f";
    let out = run_in(
        scratch.path(),
        &format!(
            r#"exec 9>f
flock 9
cash --no-config -c "flock -n '{lock}' true"; echo "other rc=$?"
flock -u 9
cash --no-config -c "flock -n '{lock}' true"; echo "unlocked rc=$?"
flock 9
exec 9>&-
cash --no-config -c "flock -n '{lock}' true"; echo "closed rc=$?""#
        ),
    );
    assert_eq!(
        out.stdout, "other rc=1\nunlocked rc=0\nclosed rc=0",
        "{}",
        out.stderr
    );
}

#[test]
fn a_timeout_waits_no_longer_than_asked() {
    let scratch = Scratch::new("flock-timeout");
    let started = Instant::now();
    let out = run_in(
        scratch.path(),
        "flock f sleep 3 & sleep 0.3; flock -w 0.5 -E 7 f true; echo \"rc=$?\"; kill %1 2>/dev/null; wait",
    );
    assert_eq!(out.stdout, "rc=7", "{}", out.stderr);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn the_command_runs_in_the_shell_with_its_folder_and_variables() {
    let scratch = Scratch::new("flock-shell");
    let out = run_in(
        scratch.path(),
        r#"mkdir sub; export FROM_SHELL=yes
cd sub && flock ../f sh -c 'basename "$PWD"; echo "$FROM_SHELL"'
flock ../f -c 'x=1; echo "in -c: $x"'; echo "outside: ${x:-unset}""#,
    );
    assert_eq!(
        out.stdout, "sub\nyes\nin -c: 1\noutside: unset",
        "{}",
        out.stderr
    );
}

#[test]
fn help_names_no_decision_numbers() {
    let scratch = Scratch::new("flock-help");
    let out = run_in(scratch.path(), "flock --help; flock -V; help flock");
    assert_eq!(out.code, 0, "{}", out.stderr);
    for word in ["D7", "D26", "D73", "ROADMAP", "spec.md", "§4"] {
        assert!(!out.stdout.contains(word), "{word} in:\n{}", out.stdout);
    }
    assert!(out.stdout.contains("Manage file locks from shell scripts."));
}
