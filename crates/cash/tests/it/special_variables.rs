//! Assignments to the variables the shell keeps itself, as Bash 5.3 takes them.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{run, unix_spelling, windows_directory};

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
    let (win, sys32) = windows_and_system32();
    let dirstack =
        format!("cd {win}; pushd {sys32} >/dev/null; DIRSTACK[1]=/c/x; dirs -l -p | tail -1");
    check(&[
        (&dirstack, "/c/x"),
        (
            "BASH_ALIASES[ll]='echo aliased'; echo \"${BASH_ALIASES[ll]}\"; alias ll",
            "echo aliased\nalias ll='echo aliased'",
        ),
        ("GROUPS[0]=5; EPOCHSECONDS[1]=2; echo ok", "ok"),
    ]);
}

#[test]
fn dirstack_starts_with_the_working_directory() {
    // It was the stack alone, first one pushed first; `dirs` was right.
    let (win, sys32) = windows_and_system32();
    let name = win.rsplit('/').next().unwrap_or_default();
    let script = format!(
        "cd {win}; pushd {sys32} >/dev/null; echo ${{#DIRSTACK[@]}}; [[ ${{DIRSTACK[0]}} == \"$PWD\" ]] && echo first; [[ ${{DIRSTACK[1]}} == */{name} ]] && echo second"
    );
    check(&[(&script, "2\nfirst\nsecond")]);
}

/// The Windows directory and its System32, on whichever drive, in the `/c/` spelling.
fn windows_and_system32() -> (String, String) {
    let win = unix_spelling(std::path::Path::new(&windows_directory()));
    let sys32 = format!("{win}/System32");
    (win, sys32)
}
