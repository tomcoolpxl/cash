//! Background jobs that run inside the shell: builtins, arithmetic, assignments.
//!
//! A background job is a task of the shell's own process. It ran on the runtime's
//! workers, so a loop of builtins held one for good, and as many such jobs as cores left
//! a foreground `$(…)` waiting for ever (EXE-03). `&` waited for the job to report a pid
//! or end a builtin, which a loop of arithmetic and assignments never does, so the `&`
//! itself hung; and `kill %1` had no process to signal, so such a loop could not be
//! ended (EXE-08). A job started inside a process substitution lost what it wrote after
//! the substitution's own command ended (EXE-06).
//!
//! Such a job had an empty `$!`. It now has a number of its own that no process has, and
//! `kill`, `wait` and `jobs -p` know it; and the limit on subshells running at once,
//! which pipeline stages count against too, is 256 and written down (D70, EXE-09).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{cash_command, output_of, run};

/// Runs each script and checks its standard output and status.
fn check(cases: &[(&str, &str)]) {
    for &(script, expected) in cases {
        let out = run(script);
        assert_eq!(
            (out.stdout.as_str(), out.code),
            (expected, 0),
            "{script}: {}",
            out.stderr
        );
    }
}

#[test]
fn a_loop_of_builtins_starts_in_the_background_and_ends_on_kill() {
    // Bash's statuses: 128 plus the signal.
    check(&[
        (
            r#"{ while ((1)); do x=1; done; } & echo started; kill %1; echo "killed $?"; wait %1; echo "waited $?""#,
            "started\nkilled 0\nwaited 143",
        ),
        (
            r#"while :; do :; done & kill -9 %1; wait %1; echo "waited $?""#,
            "waited 137",
        ),
        (
            r#"while :; do :; done & kill -0 %1; echo "zero $?"; kill %1; wait; echo end"#,
            "zero 0\nend",
        ),
        (
            r#"{ while :; do sleep 0.1; done; } & sleep 0.3; kill %1; wait %1; echo "waited $?""#,
            "waited 143",
        ),
        (
            r#"{ x=1; } & wait %1; echo "w $?"; { exit 3; } & wait %1; echo "w $?""#,
            "w 0\nw 3",
        ),
    ]);
}

#[test]
fn busy_background_jobs_leave_the_foreground_free() {
    // More busy jobs than any machine running the tests has cores.
    check(&[(
        r#"for i in $(seq 1 40); do { while :; do :; done; } & done
           echo "fg $(echo hi)"
           for j in $(seq 1 40); do kill %$j; done; wait; echo done"#,
        "fg hi\ndone",
    )]);
}

#[test]
fn a_job_started_in_a_process_substitution_writes_into_it() {
    check(&[(
        "cat <( { sleep 1; echo late; } & echo early )",
        "early\nlate",
    )]);
}

#[test]
fn a_job_that_starts_no_program_is_known_by_a_number_of_its_own() {
    check(&[
        (
            r#"while :; do :; done & p=$!; [ -n "$p" ] && echo has-pid; kill $p; wait $p; echo "waited $?""#,
            "has-pid\nwaited 143",
        ),
        (
            r#"while :; do :; done & p=$!; kill -0 $p; echo "zero $?"; kill $p; wait; kill -0 $p 2>/dev/null; echo "after $?""#,
            "zero 0\nafter 1",
        ),
        (
            r#"while :; do :; done & p=$!; [ "$(jobs -p)" = "$p" ] && echo match; kill %1; wait"#,
            "match",
        ),
        (r#"{ exit 3; } & p=$!; wait $p; echo "w $?""#, "w 3"),
        // Killed while it waits for a program, the job still ends with 143.
        (
            r#"{ :; sleep 5; } & p=$!; [ "$(jobs -p)" = "$p" ] && echo match; kill $p; wait $p; echo "w $?""#,
            "match\nw 143",
        ),
        // No Windows process id is 4n + 1.
        (r"{ x=1; } & echo $(( $! % 4 )); wait", "1"),
    ]);
}

#[test]
fn subshells_at_once_are_limited_to_256() {
    // 135 compound stages at once: past the old limit of 128, which failed them.
    let stages = "{ sleep 1; } | ".repeat(135);
    check(&[(
        &format!("{stages}{{ true; }}; echo \"stages $?\""),
        "stages 0",
    )]);

    // One more than `CASH_MAX_SUBSHELLS` fails as Bash's fork does.
    let mut command = cash_command();
    command.env("CASH_MAX_SUBSHELLS", "2");
    let out = output_of(command.args([
        "-c",
        "{ sleep 1; } & { sleep 1; } & { echo ran; } & wait; echo end",
    ]));
    assert_eq!(out.stdout, "end", "{}", out.stderr);
    assert!(
        out.stderr
            .contains("fork: retry: Resource temporarily unavailable"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_job_is_written_as_bash_writes_it() {
    // A brace group was written over several lines, and its redirections against the
    // brace; Bash's `jobs` writes one line, and `declare -f` keeps the lines (11.6).
    check(&[
        (
            "{ sleep 1; sleep 0; } & { sleep 1 & sleep 1; } & { sleep 1; } > /dev/null & jobs; wait",
            "[1]   Running                    { sleep 1; sleep 0; } &\n\
             [2]-  Running                    { sleep 1 & sleep 1; } &\n\
             [3]+  Running                    { sleep 1; } > /dev/null &",
        ),
        (
            "f() { { echo a; }; g() { :; }; } > /dev/null; declare -f f",
            "f () \n{ \n    { \n        echo a\n    };\n    function g () \n    { \n        :\n    }\n} > /dev/null",
        ),
    ]);
}
