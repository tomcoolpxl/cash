//! Errors as Bash words them: `script.sh: line 3: …` (the user, 2026-10-03; 13.4).
//!
//! cash wrote `error: command not found: x` in red, into pipes and files too, naming
//! neither the script nor the line. The name is the file the running code came from
//! (`BASH_SOURCE[0]`), else `$0`; the line is `$LINENO`'s, which was 1 in `eval`, in a
//! command substitution and in a trap.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly"
)]

use crate::common::{Scratch, cash_command, output_of, run, run_in};

#[test]
fn an_error_names_the_file_and_line_it_came_from() {
    let scratch = Scratch::new("error-location");
    std::fs::write(
        scratch.path().join("lib.sh"),
        "nosuch-in-lib\nlibf() {\n  nosuch-in-libf\n}\n",
    )
    .expect("write");
    std::fs::write(
        scratch.path().join("main.sh"),
        ". ./lib.sh\nlibf\nmainf() { nosuch-in-mainf; }\nmainf\neval 'nosuch-in-eval'\nx=$(nosuch-in-subst)\ntrap 'nosuch-in-trap' EXIT\n",
    )
    .expect("write");
    let out = output_of(cash_command().arg("main.sh").current_dir(scratch.path()));
    let expected = [
        "./lib.sh: line 1: ",
        "./lib.sh: line 3: ",
        "main.sh: line 3: ",
        "main.sh: line 5: ",
        "main.sh: line 6: ",
        "main.sh: line 1: ",
    ];
    let lines: Vec<&str> = out.stderr.lines().collect();
    assert_eq!(lines.len(), expected.len(), "{}", out.stderr);
    for (line, prefix) in lines.iter().zip(expected) {
        assert!(line.starts_with(prefix), "{line:?} should start {prefix:?}");
    }

    let out = run_in(scratch.path(), "true\nnosuch-in-c");
    assert!(
        out.stderr
            .ends_with(": line 2: command not found: nosuch-in-c"),
        "{}",
        out.stderr
    );
}

#[test]
fn an_error_into_a_pipe_is_not_coloured() {
    // The test's pipe is no terminal, so no escape sequence may reach it.
    let out = run("nosuch-command; cd /no/such/dir");
    assert!(!out.stderr.contains('\u{1b}'), "{:?}", out.stderr);
    assert!(out.stderr.contains("line 1: "), "{}", out.stderr);
}

#[test]
fn lineno_counts_from_the_file_in_eval_substitutions_and_traps() {
    let scratch = Scratch::new("lineno");
    std::fs::write(
        scratch.path().join("lines.sh"),
        "trap 'echo \"err $LINENO\"' ERR\ntrap 'echo \"exit $LINENO ${0##*/}\"' EXIT\neval 'echo \"eval $LINENO\"'\nx=$(echo \"subst $LINENO\"); echo \"$x\"\nfalse\n",
    )
    .expect("write");
    let out = output_of(cash_command().arg("lines.sh").current_dir(scratch.path()));
    assert_eq!(
        out.stdout, "eval 3\nsubst 4\nerr 5\nexit 1 lines.sh",
        "{}",
        out.stderr
    );
}

#[test]
fn a_syntax_error_is_reported_on_two_lines() {
    let scratch = Scratch::new("syntax-lines");
    std::fs::write(scratch.path().join("bad.sh"), "echo a\nfi\n").expect("write");
    let out = output_of(cash_command().arg("bad.sh").current_dir(scratch.path()));
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        (
            "a",
            "bad.sh: line 2: syntax error near unexpected token `fi'\nbad.sh: line 2: `fi'",
            2
        )
    );

    let out = run("\n\neval 'if true; then'");
    assert!(
        out.stderr.ends_with(
            ": eval: line 4: syntax error: unexpected end of file from `if' command on line 3"
        ),
        "{}",
        out.stderr
    );
}
