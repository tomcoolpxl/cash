//! `$BASH_SUBSHELL` and the `set -x` prefix count what Bash counts.
//!
//! Both followed the number of copies of the shell: `true | echo $BASH_SUBSHELL` was 1
//! where Bash's is 0 (it forks a simple command without entering a subshell), `true |
//! (echo $BASH_SUBSHELL)` 2 where it is 1, and a subshell, a pipeline stage and a
//! background job traced with `++`, which Bash keeps for command and process
//! substitutions, `eval` and sourced files.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::run;

#[test]
fn bash_subshell_counts_the_subshells_bash_counts() {
    let out = run(r#"echo "top $BASH_SUBSHELL"
        (echo "paren $BASH_SUBSHELL")
        echo "$(echo "cmd $BASH_SUBSHELL")"
        true | echo "simple $BASH_SUBSHELL"
        true | { echo "brace $BASH_SUBSHELL"; }
        true | (echo "stage-paren $BASH_SUBSHELL")
        f() { echo "func $BASH_SUBSHELL"; }; true | f
        { echo "bg $BASH_SUBSHELL"; } & wait
        cat <(echo "procsub $BASH_SUBSHELL")
        (echo a; (echo "nested $BASH_SUBSHELL"))"#);
    assert_eq!(
        out.stdout,
        "top 0\nparen 1\ncmd 1\nsimple 0\nbrace 1\nstage-paren 1\nfunc 1\nbg 1\nprocsub 1\na\nnested 2",
        "{}",
        out.stderr
    );
}

#[test]
fn the_trace_prefix_grows_only_in_substitutions_and_eval() {
    let out = run(
        r#"set -x; (echo paren); true | echo stage; { echo bg; } & wait; eval "echo ev"; x=$(echo sub)"#,
    );
    // Sorted: the stages and the background job trace from threads of their own, in no
    // fixed order.
    let mut trace: Vec<&str> = out
        .stderr
        .lines()
        .filter(|line| line.starts_with('+'))
        .collect();
    trace.sort_unstable();
    let mut expected = [
        "+ echo paren",
        "+ true",
        "+ echo stage",
        "+ wait",
        "+ echo bg",
        "+ eval 'echo ev'",
        "++ echo ev",
        "++ echo sub",
        "+ x=sub",
    ];
    expected.sort_unstable();
    assert_eq!(trace, expected, "{}", out.stderr);
}
