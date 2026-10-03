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
        ("set -m; fg %3", "fg: %3: no such job"),
        ("fg", "fg: no job control"),
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

#[test]
fn an_index_before_the_start_is_a_bad_array_subscript() {
    // Reading said nothing, `${#a[-3]}` gave 0, and the rest said `array index out of
    // range: -3`; Bash says `bad array subscript`, naming each as here.
    let scratch = Scratch::new("subscript");
    std::fs::write(
        scratch.path().join("s.sh"),
        "a=(1 2)\necho \"[${a[-3]}] $?\"\necho \"len ${#a[-3]}\"\nunset 'a[-3]'; echo \"unset $?\"\na[-3]=x\necho \"assign $?\"\n",
    )
    .expect("write");
    let out = output_of(cash_command().arg("s.sh").current_dir(scratch.path()));
    assert_eq!(out.stdout, "[] 0\nunset 1\nassign 1", "{}", out.stderr);
    assert_eq!(
        out.stderr,
        "s.sh: line 2: a: bad array subscript\ns.sh: line 3: [-3]: bad array subscript\n\
         s.sh: line 4: unset: [-3]: bad array subscript\ns.sh: line 5: a[-3]: bad array subscript"
    );
}

#[test]
fn a_tool_words_an_io_error_as_the_c_library_does() {
    // Each said Rust's words, "The system cannot find the path specified. (os error 3)",
    // or worded a few kinds of its own; `chmod` named the resolved path.
    let out =
        run("tree /no/such; rev /no/such; chmod +x /no/such; pushd /no/such; install /no/such x");
    assert!(!out.stderr.contains("os error"), "{}", out.stderr);
    for expected in [
        "tree: /no/such: No such file or directory",
        "rev: cannot open /no/such: No such file or directory",
        "chmod: cannot access '/no/such': No such file or directory",
        "pushd: /no/such: No such file or directory",
        "install: cannot stat '/no/such': No such file or directory",
    ] {
        assert!(out.stderr.contains(expected), "{expected}: {}", out.stderr);
    }
}

#[test]
fn an_arithmetic_error_names_the_expression_and_its_token() {
    // cash said `arithmetic evaluation error: division by zero` and `failed to parse
    // expression: 1 + `. Each here is Git Bash 5.3's, after `bash: line 1: `.
    let cases = [
        ("echo $((1/0))", "1/0: division by 0 (error token is \"0\")"),
        (
            "echo $((1/0 + 5))",
            "1/0 + 5: division by 0 (error token is \"0 + 5\")",
        ),
        (
            "x=0; echo $((10 / x))",
            "10 / x: division by 0 (error token is \"x\")",
        ),
        (
            "echo $((2**-1 + 3))",
            "2**-1 + 3: exponent less than 0 (error token is \"+ 3\")",
        ),
        (
            "echo $(( 1 + ))",
            "1 + : arithmetic syntax error: operand expected (error token is \"+ \")",
        ),
        (
            "echo $((a b))",
            "a b: arithmetic syntax error in expression (error token is \"b\")",
        ),
        (
            "echo $((1 ? 2))",
            "1 ? 2: `:' expected for conditional expression (error token is \"2\")",
        ),
        (
            "echo $((08))",
            "08: value too great for base (error token is \"08\")",
        ),
        (
            "x=08; echo $((x))",
            "08: value too great for base (error token is \"08\")",
        ),
        (
            "echo $((65#1))",
            "65#1: invalid arithmetic base (error token is \"65#1\")",
        ),
        ("((1/0))", "((: 1/0: division by 0 (error token is \"0\")"),
        (
            "for ((i = ; i < 2; i++)); do :; done",
            "((: i = : arithmetic syntax error: operand expected (error token is \"= \")",
        ),
        (
            "let 'x='",
            "let: x=: arithmetic syntax error: operand expected (error token is \"=\")",
        ),
    ];
    for (script, expected) in cases {
        let out = run(script);
        assert!(
            out.stderr.ends_with(&format!("line 1: {expected}")),
            "{script}: {:?}",
            out.stderr
        );
    }

    // A blank subscript is 0, as in Bash, where cash failed; an empty one is said, and
    // is 0 itself. `++` at the end is two tokens, as Bash reads it.
    assert_eq!(run("a=(1); echo $((a[ ]))").stdout, "1");
    let out = run("a=(1); echo $((a[] + 1))");
    assert_eq!(out.stdout, "1");
    assert!(
        out.stderr.ends_with("line 1: a[]: bad array subscript"),
        "{}",
        out.stderr
    );
    let out = run("echo $((x++ ++))");
    assert!(
        out.stderr
            .ends_with("operand expected (error token is \"+\")"),
        "{}",
        out.stderr
    );
}

#[test]
fn test_says_what_it_cannot_read_and_returns_2() {
    // `[ 1 -eq x ]` was quietly false, and the rest `invalid test command`.
    for (script, expected) in [
        ("[ 1 -eq x ]", "[: x: integer expected"),
        (
            "[ 1 -gt 99999999999999999999 ]",
            "[: 99999999999999999999: integer expected",
        ),
        ("test 1 -eq", "test: 1: unary operator expected"),
        ("[ -z a b ]", "[: a: binary operator expected"),
        ("[ a = b c ]", "[: too many arguments"),
        ("[ 1 -eq 1 -a ]", "[: argument expected"),
        ("[ ! a = ]", "[: a: unary operator expected"),
        ("[ 1 -eq 1", "[: missing `]'"),
    ] {
        let out = run(&format!("{script}; echo \"rc $?\""));
        assert_eq!(out.stdout, "rc 2", "{script}: {}", out.stderr);
        assert!(
            out.stderr.ends_with(&format!("line 1: {expected}")),
            "{script}: {}",
            out.stderr
        );
    }
    // Blanks around a number are allowed, as in Bash, and `[[` reads arithmetic.
    assert_eq!(
        run("[ ' 3 ' -eq 3 ] && [[ 1 -eq x ]] || echo ok").stdout,
        "ok"
    );
}

#[test]
fn declare_will_not_turn_one_kind_of_array_into_the_other() {
    // `declare -A a; declare -a a` made it indexed with an empty element, and said nothing.
    let out = run("declare -A a; declare -a a; echo \"rc $?\"; declare -p a");
    assert_eq!(out.stdout, "rc 1\ndeclare -A a", "{}", out.stderr);
    assert!(
        out.stderr
            .ends_with("line 1: declare: a: cannot convert associative to indexed array"),
        "{}",
        out.stderr
    );
    let out = run("declare -a b=(1); declare -A b; echo \"rc $?\"");
    assert_eq!(out.stdout, "rc 1", "{}", out.stderr);
    assert!(
        out.stderr
            .ends_with("line 1: declare: b: cannot convert indexed to associative array"),
        "{}",
        out.stderr
    );
}
