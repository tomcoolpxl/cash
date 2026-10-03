//! Background jobs that run inside the shell: builtins, arithmetic, assignments.
//!
//! A background job is a task of the shell's own process. It ran on the runtime's
//! workers, so a loop of builtins held one for good, and as many such jobs as cores left
//! a foreground `$(…)` waiting for ever (EXE-03). `&` waited for the job to report a pid
//! or end a builtin, which a loop of arithmetic and assignments never does, so the `&`
//! itself hung; and `kill %1` had no process to signal, so such a loop could not be
//! ended (EXE-08). A job started inside a process substitution lost what it wrote after
//! the substitution's own command ended (EXE-06).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

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
