//! Errors as Bash words them: `script.sh: line 3: …` (the user, 2026-10-03; 13.4).
//!
//! cash wrote `error: command not found: x` in red, into pipes and files too, naming
//! neither the script nor the line. The name is the file the running code came from
//! (`BASH_SOURCE[0]`), else `$0`; the line is `$LINENO`'s, which was 1 in `eval`, in a
//! command substitution and in a trap.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::literal_string_with_formatting_args,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly; `${x:?}` is shell, not a format"
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
            .ends_with(": line 2: nosuch-in-c: command not found"),
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

#[test]
fn errors_are_worded_as_bash_words_them() {
    // Each as Git Bash 5.3 says it, after `bash: line 1: `.
    let scratch = Scratch::new("wording");
    std::fs::create_dir_all(scratch.path().join("folder")).expect("mkdir");
    let cases = [
        ("nosuch", "nosuch: command not found"),
        (
            "./no-such-script",
            "./no-such-script: No such file or directory",
        ),
        ("./folder", "./folder: Is a directory"),
        ("readonly r=1; r=2", "r: readonly variable"),
        (
            "readonly r=1; unset r",
            "unset: r: cannot unset: readonly variable",
        ),
        ("set -u; echo \"$u\"", "u: unbound variable"),
        ("set -u; echo \"$1\"", "$1: unbound variable"),
        (": \"${x:?must be set}\"", "x: must be set"),
        (": \"${x:?}\"", "x: parameter null or not set"),
        (
            ". ./no-such-file",
            "./no-such-file: No such file or directory",
        ),
        (
            "echo hi > /no/such/dir/f",
            "/no/such/dir/f: No such file or directory",
        ),
        ("exec 3<&9", "9: Bad file descriptor"),
        (
            "a=(1); a[0]=(2)",
            "a[0]: cannot assign list to array member",
        ),
        (": ${1:=x}", "$1: cannot assign in this way"),
        ("umask 999", "umask: 999: octal number out of range"),
        ("umask u=q", "umask: `q': invalid symbolic mode character"),
    ];
    for (script, expected) in cases {
        let out = run_in(scratch.path(), script);
        assert!(
            out.stderr.ends_with(&format!("line 1: {expected}")),
            "{script}: {:?}",
            out.stderr
        );
    }
}

#[test]
fn a_builtin_of_bashs_is_located_and_a_tool_is_not() {
    // Bash's builtins report through `builtin_error`, after the location; `chmod` is a
    // program in Bash, and cash's says what a program says. Its `usage:` line is bare.
    for (script, expected) in [
        ("cd /no/such", "cd: /no/such: No such file or directory"),
        ("kill %9", "kill: %9: no such job"),
        ("fg %3", "fg: %3: no such job"),
        (
            "complete -p nosuch",
            "complete: nosuch: no completion specification",
        ),
        ("enable nosuch", "enable: nosuch: not a shell builtin"),
        ("let", "let: expression expected"),
        (
            "read -u 9 x",
            "read: 9: invalid file descriptor: Bad file descriptor",
        ),
        (
            "pushd /no/such",
            "pushd: /no/such: No such file or directory",
        ),
    ] {
        let out = run(script);
        assert!(
            out.stderr.ends_with(&format!(": line 1: {expected}")) && !out.stdout.contains(':'),
            "{script}: {:?} {:?}",
            out.stdout,
            out.stderr
        );
    }

    let out = run("chmod");
    assert!(
        out.stderr.starts_with("chmod: missing operand"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_bad_option_is_bashs_two_lines() {
    // clap printed its own block: `error: unexpected argument '-q' found`, a tip, a usage
    // line of its own and `try '--help'` (BI-16).
    let read_usage = "read: usage: read [-Eers] [-a array] [-d delim] [-i text] [-n nchars] \
                      [-N nchars] [-p prompt] [-t timeout] [-u fd] [name ...]";
    for (script, first) in [
        ("read -q", Some("read: -q: invalid option")),
        ("read --foo", Some("read: --: invalid option")),
        ("read -u", Some("read: -u: option requires an argument")),
    ] {
        let out = run(script);
        let lines: Vec<&str> = out.stderr.lines().collect();
        assert_eq!(lines.len(), 2, "{script}: {}", out.stderr);
        assert!(
            first.is_some_and(|first| lines[0].ends_with(&format!(": line 1: {first}"))),
            "{script}: {}",
            out.stderr
        );
        assert_eq!((lines[1], out.code), (read_usage, 2), "{script}");
    }
    let out = run("getopts");
    assert_eq!(
        (out.stderr.as_str(), out.code),
        ("getopts: usage: getopts optstring name [arg ...]", 2)
    );

    let out = run("read -t 1e1 x < /dev/null");
    assert!(
        out.stderr
            .ends_with("line 1: read: 1e1: invalid timeout specification"),
        "{}",
        out.stderr
    );
    assert_eq!(out.code, 1);
}

#[test]
fn help_goes_to_standard_output() {
    // Bash's builtins give it with status 2; a tool cash carries, as a program does, 0.
    let out =
        run("read --help >/dev/null; echo \"read $?\"; tree --help >/dev/null; echo \"tree $?\"");
    assert_eq!(out.stdout, "read 2\ntree 0", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}
