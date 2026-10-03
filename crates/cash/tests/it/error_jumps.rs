//! Errors that abandon the whole top-level command, as Bash's `jump_to_top_level`.

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
fn an_assignment_error_in_a_function_abandons_the_callers_command() {
    // The command the function's error came out of went on with status 1:
    // `f; echo same` echoed, and so did the rest of a calling function.
    check(&[
        (
            "readonly r=1\nf() { r=3; echo in; }\nf; echo same\necho \"next $?\"\ntrue",
            "next 1",
        ),
        (
            "readonly r=1\nf() { r=3; }\ng() { f; echo in-g; }\ng; echo same\necho \"next $?\"\ntrue",
            "next 1",
        ),
        (
            "declare -n p=q; declare -n q=p\nf() { p=z; echo in; }\nf; echo same\necho next\ntrue",
            "next",
        ),
    ]);
}

#[test]
fn an_arithmetic_error_in_a_function_abandons_the_callers_command() {
    check(&[(
        "f() { local x=$((1/0)); echo in; }\nf || echo or; echo same\necho \"next $?\"\ntrue",
        "next 1",
    )]);
}

#[test]
fn a_call_past_the_nesting_limit_abandons_the_command() {
    // It went on with status 0 and said "maximum function call depth exceeded"; Bash
    // names the function and the limit.
    let out = run(
        "FUNCNEST=3\nn=0\ng() { n=$((n+1)); g; echo back; }\ng; echo same\necho \"next $? $n\"",
    );
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("next 1 3", 0),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("g: maximum function nesting level exceeded (3)"),
        "{}",
        out.stderr
    );
}

#[test]
fn eval_source_a_subshell_and_a_builtin_still_stop_it() {
    // As in Bash: they catch it, and the line goes on.
    check(&[(
        "readonly r=1\nf() { r=3; }\neval f; echo \"eval $?\"\n(f); echo \"sub $?\"\nread r <<< x; echo \"read $?\"",
        "eval 1\nsub 1\nread 1",
    )]);
}
