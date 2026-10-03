//! A job a signal ended is told of by the signal, as Bash tells of it (TODO.md 11.2).
//!
//! Windows has only exit codes, so `jobs` said `Exit 137` after `kill -9 %1` where Bash
//! says `Killed`, and a script said nothing where Bash prints `script: line 4: 145006
//! Killed  sleep 5`. The job now keeps the signal cash's `kill` sent it. Bash prints the
//! notice for HUP and KILL, before the next line once it has waited for a program in the
//! foreground, or in the `wait` that collects the job; not after builtins alone, nor at
//! the end.

#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use crate::common::{Scratch, run, run_in};

/// The output with the process id in each notice made `PID`, as it differs from run to
/// run: `NAME: line 4: 145006 Killed …` becomes `NAME: line 4: PID Killed …`.
fn without_pids(text: &str) -> String {
    text.lines()
        .map(|line| {
            let Some((head, rest)) = line.split_once(": line ") else {
                return line.to_owned();
            };
            let Some((number, rest)) = rest.split_once(": ") else {
                return line.to_owned();
            };
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ');
            format!("{head}: line {number}: PID {rest}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn jobs_names_the_signal_that_ended_a_job() {
    for (signal, name) in [
        ("TERM", "Terminated"),
        ("HUP", "Hangup"),
        ("INT", "Interrupt"),
        ("KILL", "Killed"),
    ] {
        let out = run(&format!("sleep 5 & kill -{signal} %1; sleep 0.3; jobs"));
        assert_eq!(
            out.stdout,
            format!("[1]+  {name:<26} sleep 5"),
            "{signal}: {}",
            out.stderr
        );
    }
}

#[test]
fn a_script_tells_of_a_job_a_signal_ended_as_bash_does() {
    let scratch = Scratch::new("killed-jobs");
    let script = |name: &str, text: &str| {
        std::fs::write(scratch.path().join(name), text).unwrap();
        let out = run_in(scratch.path(), &format!("source ./{name}"));
        (out.stdout, without_pids(&out.stderr))
    };

    // After a program in the foreground, before the next line, with that line's number.
    let (stdout, stderr) = script(
        "after.sh",
        "sleep 5 &\nkill -9 %1\nsleep 0.5\necho mid\nwait\necho end\n",
    );
    assert_eq!(stdout, "mid\nend");
    assert!(
        stderr.ends_with("line 4: PID Killed                     sleep 5"),
        "{stderr}"
    );

    // In `wait`, for the job it waits for; TERM is not told of.
    let (stdout, stderr) = script(
        "wait.sh",
        "sleep 5 &\nkill -HUP %1\nwait %1\necho \"w $?\"\nsleep 5 &\nkill -TERM %1\nwait\necho end\n",
    );
    assert_eq!(stdout, "w 129\nend");
    assert!(
        stderr.ends_with("line 3: PID Hangup                     sleep 5"),
        "{stderr}"
    );

    // Not after builtins alone, nor at the end.
    let (stdout, stderr) = script(
        "builtins.sh",
        "sleep 5 &\nkill -9 %1\necho a\nx=1\necho b\n",
    );
    assert_eq!((stdout.as_str(), stderr.as_str()), ("a\nb", ""));
}
