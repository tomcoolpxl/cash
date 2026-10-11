//! `shopt winglob` — **D81**: a program of Windows' own globs for itself.
//!
//! cmd never expands a wildcard, so every program in `System32` reads `*` and `?` itself
//! when it wants them, or means something else by them: `net user NAME *` asks for the
//! password, and bash's globbing hands `net` the folder's file names instead. With
//! `winglob`, the words after such a program are not pathname-expanded. It is on by
//! default at the interactive prompt and off in scripts, which keep bash's globbing.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Stdio;

use crate::common::{Scratch, cash_command, git_for_windows};

/// What `cash -c script` prints, run in a folder of its own holding `a.txt` and `b.txt`;
/// `interactive` makes it the shell of a prompt (`-i`), reading no startup file. cmd's
/// `echo` ends its line with CRLF; the lines are joined with LF here.
fn among_two_files(interactive: bool, script: &str) -> String {
    let scratch = Scratch::new("winglob");
    std::fs::write(scratch.join("a.txt"), "a").unwrap();
    std::fs::write(scratch.join("b.txt"), "b").unwrap();
    let mut command = cash_command();
    command.env("CASH_NO_OFFER", "1");
    if interactive {
        command.args(["--norc", "-i"]);
    }
    let out = command
        .args(["-c", script])
        .current_dir(scratch.path())
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    String::from_utf8_lossy(&out.stdout)
        .replace("\r\n", "\n")
        .trim_end()
        .to_owned()
}

#[test]
fn scripts_keep_bash_globbing() {
    assert_eq!(
        among_two_files(false, "shopt -q winglob || echo off"),
        "off"
    );
    assert_eq!(among_two_files(false, "cmd /c echo *"), "a.txt b.txt");
}

#[test]
fn the_interactive_prompt_has_it_on() {
    assert_eq!(
        among_two_files(true, "shopt -q winglob && cmd /c echo *"),
        "*"
    );
    assert_eq!(
        among_two_files(true, "shopt -u winglob; cmd /c echo *"),
        "a.txt b.txt"
    );
}

#[test]
fn a_program_of_windows_own_gets_its_wildcards_as_typed() {
    assert_eq!(
        among_two_files(false, "shopt -s winglob; cmd /c echo * ?.txt '*' [ab].txt"),
        "* ?.txt * [ab].txt"
    );
    // By its path as well as by its name; `$SystemRoot` is `C:\WINDOWS`.
    assert_eq!(
        among_two_files(
            false,
            r#"shopt -s winglob; "$SystemRoot/System32/cmd.exe" /c echo *"#
        ),
        "*"
    );
}

#[test]
fn the_other_expansions_still_happen() {
    assert_eq!(
        among_two_files(
            false,
            "shopt -s winglob; x=a; cmd /c echo $x.txt {a,b}.txt \"$x\"*.txt"
        ),
        "a.txt a.txt b.txt a*.txt"
    );
}

#[test]
fn nullglob_and_failglob_have_nothing_to_act_on() {
    assert_eq!(
        among_two_files(
            false,
            "shopt -s winglob failglob; cmd /c echo *.none; echo \"status $?\""
        ),
        "*.none\nstatus 0"
    );
    assert_eq!(
        among_two_files(false, "shopt -s winglob nullglob; cmd /c echo *.none"),
        "*.none"
    );
}

#[test]
fn a_program_elsewhere_gets_the_shells_globbing() {
    let printf = format!("{}/usr/bin/printf.exe", git_for_windows());
    assert_eq!(
        among_two_files(false, &format!("shopt -s winglob; '{printf}' '[%s]' *")),
        "[a.txt][b.txt]"
    );
}

#[test]
fn a_builtin_that_runs_the_program_is_looked_through() {
    assert_eq!(
        among_two_files(
            false,
            "shopt -s winglob
             command cmd /c echo *
             nohup cmd /c echo *
             nice cmd /c echo *
             nice -n 5 cmd /c echo *
             env X=1 cmd /c echo *
             timeout 10 cmd /c echo *
             exec cmd /c echo *"
        ),
        "*\n*\n*\n*\n*\n*\n*"
    );
}

#[test]
fn a_function_and_a_builtin_are_the_shells() {
    assert_eq!(
        among_two_files(
            false,
            "shopt -s winglob; cmd() { printf '[%s]' \"$@\"; }; cmd /c echo *"
        ),
        "[/c][echo][a.txt][b.txt]"
    );
    assert_eq!(
        among_two_files(false, "shopt -s winglob; echo *"),
        "a.txt b.txt"
    );
    assert_eq!(
        among_two_files(false, "shopt -s winglob; command -v cmd >/dev/null; echo *"),
        "a.txt b.txt"
    );
}
