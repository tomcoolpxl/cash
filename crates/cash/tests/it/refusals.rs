//! The kinder refusals: `suspend`, `mkfifo` and `stdbuf` are builtins that say what
//! Windows lacks in Bash's and coreutils' shapes, with their statuses, where before
//! each was `command not found`, status 127.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Stdio;

use crate::common::{Output, cash_command};

/// Runs `script` with standard input closed and `PATH` reduced to System32, so that Git
/// for Windows' own `mkfifo`, `stdbuf`, `getconf` and `locale` are not on it.
fn cash(script: &str) -> Output {
    crate::common::output_of(
        cash_command()
            .args(["-c", script])
            .env("PATH", r"C:\Windows\System32")
            .stdin(Stdio::null()),
    )
}

#[test]
fn the_three_are_builtins() {
    let out = cash("type -t suspend mkfifo stdbuf");
    assert_eq!(out.stdout, "builtin\nbuiltin\nbuiltin");
}

#[test]
fn suspend_cannot_and_says_so_in_bashs_shape() {
    let out = cash("suspend; echo \"st=$?\"; suspend -f; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=1\nst=1");
    let lines: Vec<&str> = out.stderr.lines().collect();
    assert_eq!(lines.len(), 2, "{}", out.stderr);
    for line in lines {
        assert!(
            line.ends_with(": line 1: suspend: cannot suspend: Windows has no stop signal"),
            "{line}"
        );
    }
    let out = cash("suspend -x; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=2");
    assert!(
        out.stderr.contains("suspend: -x: invalid option"),
        "{}",
        out.stderr
    );
}

#[test]
fn mkfifo_refuses_each_name_and_points_at_process_substitution() {
    let out = cash("mkfifo a.fifo -m 644 b.fifo; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=1");
    assert_eq!(
        out.stderr,
        "cash: mkfifo: a.fifo: named pipes are not files on Windows; use a process substitution, <(…) or >(…)\n\
         cash: mkfifo: b.fifo: named pipes are not files on Windows; use a process substitution, <(…) or >(…)"
    );
    let out = cash("mkfifo; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=1");
    assert_eq!(
        out.stderr,
        "mkfifo: missing operand\nTry 'mkfifo --help' for more information."
    );
    let out = cash("mkfifo --help | head -2; mkfifo --version");
    assert!(
        out.stdout
            .starts_with("Usage: mkfifo [OPTION]... NAME...\n"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("mkfifo (cash)"), "{}", out.stdout);
}

#[test]
fn stdbuf_runs_cashs_own_as_they_are() {
    let out = cash(
        "printf 'a\\nb\\n' | stdbuf -oL grep a; stdbuf -o0 echo plain; stdbuf -i0 -eL cat <<<here",
    );
    assert_eq!(out.stderr, "");
    assert_eq!(out.stdout, "a\nplain\nhere");
}

#[test]
fn stdbuf_runs_an_external_program_as_it_is_after_one_line() {
    let out = cash("stdbuf -oL cmd.exe /c \"echo from-cmd& exit 7\"; echo \"st=$?\"");
    // cmd.exe ends its line with CRLF.
    assert_eq!(out.stdout.replace("\r\n", "\n"), "from-cmd\nst=7");
    assert_eq!(
        out.stderr,
        "cash: stdbuf: cannot change the buffering of an external program on Windows; running it as is"
    );
}

#[test]
fn stdbuf_errors_are_coreutils() {
    let out = cash("stdbuf -oL nosuchprogram; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=127");
    assert!(
        out.stderr
            .ends_with("stdbuf: failed to run command 'nosuchprogram': No such file or directory"),
        "{}",
        out.stderr
    );
    let out = cash(
        "stdbuf -oX echo; echo \"st=$?\"; stdbuf; echo \"st=$?\"; stdbuf echo; echo \"st=$?\"; stdbuf -iL cat; echo \"st=$?\"",
    );
    assert_eq!(out.stdout, "st=125\nst=125\nst=125\nst=125");
    assert_eq!(
        out.stderr,
        "stdbuf: invalid mode 'X'\n\
         stdbuf: missing operand\nTry 'stdbuf --help' for more information.\n\
         stdbuf: you must specify a buffering mode option\nTry 'stdbuf --help' for more information.\n\
         stdbuf: line buffering stdin is meaningless"
    );
    let out = cash("stdbuf --help | head -1; stdbuf --version");
    assert_eq!(
        out.stdout,
        "Usage: stdbuf OPTION... COMMAND\nstdbuf (cash): coreutils' options; a program's buffering cannot be changed on Windows"
    );
}
