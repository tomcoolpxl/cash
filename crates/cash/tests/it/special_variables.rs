//! Assignments to the variables the shell keeps itself, as Bash 5.3 takes them.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

/// Runs each script and checks its standard output, standard error and status.
fn check(cases: &[(&str, &str)]) {
    for &(script, expected) in cases {
        let out = run(script);
        assert_eq!(
            (out.stdout.as_str(), out.stderr.as_str(), out.code),
            (expected, "", 0),
            "{script}"
        );
    }
}

#[test]
fn every_way_of_assigning_seeds_random_and_restarts_seconds() {
    // Only `NAME=value` reached them; `read`, `printf -v`, `declare` and `(( ))` dropped
    // the value. The numbers are Git Bash's after `RANDOM=42`.
    check(&[
        ("read RANDOM <<< 42; echo $RANDOM", "17772"),
        ("printf -v RANDOM %s 42; echo $RANDOM", "17772"),
        ("declare RANDOM=42; echo $RANDOM", "17772"),
        ("(( RANDOM = 42 )); echo $RANDOM", "17772"),
        ("SECONDS=50; read SECONDS <<< 100; echo $SECONDS", "100"),
    ]);
}

#[test]
fn bash_argv0_renames_dollar_zero() {
    check(&[("BASH_ARGV0=renamed; echo $0", "renamed")]);
}

#[test]
fn an_element_of_dirstack_and_bash_aliases_takes_effect() {
    // It was "not yet implemented", after a debug line with the getter's address.
    check(&[
        (
            "cd /c/Windows; pushd /c/Users >/dev/null; DIRSTACK[1]=/c/x; dirs -l -p | tail -1",
            "/c/x",
        ),
        (
            "BASH_ALIASES[ll]='echo aliased'; echo \"${BASH_ALIASES[ll]}\"; alias ll",
            "echo aliased\nalias ll='echo aliased'",
        ),
        ("GROUPS[0]=5; EPOCHSECONDS[1]=2; echo ok", "ok"),
    ]);
}

#[test]
fn dirstack_starts_with_the_working_folder() {
    // It was the stack alone, first one pushed first; `dirs` was right.
    check(&[(
        "cd /c/Windows; pushd /c/Users >/dev/null; echo ${#DIRSTACK[@]}; [[ ${DIRSTACK[0]} == \"$PWD\" ]] && echo first; [[ ${DIRSTACK[1]} == */Windows ]] && echo second",
        "2\nfirst\nsecond",
    )]);
}
