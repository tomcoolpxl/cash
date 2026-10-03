//! Errors that abandon the whole top-level command, as Bash's `jump_to_top_level`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly"
)]

use crate::common::{Scratch, run, run_in};

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
fn a_syntax_error_in_eval_fails_eval_and_the_script_goes_on() {
    // Bash reports it, `eval` returns 2, and the next command runs; cash ended the
    // script. In POSIX mode, as for any special builtin, it ends a script.
    check(&[(
        "eval 'if then'; echo \"after $?\"\nf() { eval 'done'; echo \"in f $?\"; }; f; echo \"after f $?\"\nx=$(eval 'fi'); echo \"sub $?\"",
        "after 2\nin f 2\nafter f 0\nsub 2",
    )]);
    let out = run("set -e; eval 'case'; echo not reached");
    assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{}", out.stderr);
    let out = run("set -o posix; eval 'if then'; echo not reached");
    assert_eq!((out.stdout.as_str(), out.code), ("", 2), "{}", out.stderr);
}

#[test]
fn a_sourced_file_that_does_not_parse_fails_source_and_the_script_goes_on() {
    // As `eval`, and in POSIX mode too, as Bash 5.3 has it.
    let scratch = Scratch::new("source-syntax");
    std::fs::write(scratch.path().join("bad.inc"), "if then\n").expect("write");
    for script in [
        ". ./bad.inc; echo \"after $?\"",
        "set -o posix; . ./bad.inc; echo \"after $?\"",
    ] {
        let out = run_in(scratch.path(), script);
        assert_eq!(
            (out.stdout.as_str(), out.code),
            ("after 2", 0),
            "{script}: {}",
            out.stderr
        );
    }
}

#[test]
fn eval_source_a_subshell_and_a_builtin_still_stop_it() {
    // As in Bash: they catch it, and the line goes on.
    check(&[(
        "readonly r=1\nf() { r=3; }\neval f; echo \"eval $?\"\n(f); echo \"sub $?\"\nread r <<< x; echo \"read $?\"",
        "eval 1\nsub 1\nread 1",
    )]);
}
