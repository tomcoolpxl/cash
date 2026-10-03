//! `coproc`, as Bash 5.3 has it (EXE-04).
//!
//! The coprocess ran in a copy of the shell taken after the shell had opened its ends,
//! so it held the write end of its own input and never saw the end of it: `exec
//! {COPROC[1]}>&-; wait` waited for ever, and so did the shell's exit. `COPROC_PID` was
//! the job's number, `kill %1` found nothing to signal, and the ends were fds 3 and 4.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

/// Runs each script and checks its standard output and status against Bash's.
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
fn a_coprocess_sees_the_end_of_its_input_when_the_shell_closes_it() {
    check(&[
        (
            r#"coproc cat; echo hi >&"${COPROC[1]}"; read -r l <&"${COPROC[0]}"; echo "got $l"; exec {COPROC[1]}>&-; wait; echo "waited $?""#,
            "got hi\nwaited 0",
        ),
        (
            r#"coproc UP { tr a-z A-Z; }; echo abc >&"${UP[1]}"; exec {UP[1]}>&-; read -r l <&"${UP[0]}"; echo "$l"; wait"#,
            "ABC",
        ),
        // The shell ends while the coprocess still runs.
        ("coproc cat; echo bye", "bye"),
    ]);
}

#[test]
fn a_coprocess_is_a_job_with_a_pid() {
    check(&[
        (r#"coproc cat; echo "${COPROC[@]}"; kill %1; wait"#, "63 60"),
        (
            r#"coproc cat; [ "$COPROC_PID" = "$!" ] && echo same-pid; kill $COPROC_PID; wait; echo end"#,
            "same-pid\nend",
        ),
        (
            r#"coproc cat; kill %1; echo "k $?"; wait; echo end"#,
            "k 0\nend",
        ),
        (
            r#"coproc { while :; do :; done; }; kill $COPROC_PID; wait $COPROC_PID; echo "w $?""#,
            "w 143",
        ),
        (
            "coproc cat; jobs; kill %1; wait",
            "[1]+  Running                    coproc cat &",
        ),
    ]);
}

#[test]
fn a_coprocess_that_reads_first_does_not_hold_up_the_shell() {
    // `coproc` waited for the coprocess's first command to end, which waited for the
    // shell to write to it.
    check(&[(
        r#"coproc { read -r x; echo "got $x"; exit 4; }; echo hi >&"${COPROC[1]}"; read -r l <&"${COPROC[0]}"; echo "$l"; wait $COPROC_PID; echo "w $?""#,
        "got hi\nw 4",
    )]);
}
