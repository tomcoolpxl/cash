//! `command_not_found_handle`, run as Bash 4 and later run it: with the command name
//! and its arguments, in a subshell, its status the command's; the install hint only
//! when no such function is defined. Git Bash 5.3 is the model, and each script here
//! is run under it too.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

use crate::common::{
    CASH, Output, Scratch, cash_command, git_for_windows, output_of, with_isolated_environment,
};

/// Runs `script` with `PATH` set to an empty folder, so that nothing is found.
fn cash_without_path(dir: &Scratch, script: &str) -> Output {
    output_of(
        cash_command()
            .args(["-c", script])
            .env("PATH", dir.path())
            .stdin(Stdio::null()),
    )
}

/// Runs `script` under Git Bash, with its `PATH` kept (the scripts name no program).
fn git_bash(script: &str) -> Output {
    let bash = format!("{}/bin/bash.exe", git_for_windows());
    output_of(Command::new(bash).args(["-c", script]).stdin(Stdio::null()))
}

#[test]
fn the_handler_gets_the_command_and_its_arguments_and_its_status_is_the_commands() {
    let dir = Scratch::new("cnf-handle");
    let script = "command_not_found_handle() { echo \"no $1 with $# args\"; return 42; }; \
                  nosuch a b; echo \"st=$?\"";
    let out = cash_without_path(&dir, script);
    assert_eq!(out.stderr, "");
    assert_eq!(out.stdout, "no nosuch with 3 args\nst=42");
    assert_eq!(out.stdout, git_bash(script).stdout);

    // Without a `return`, the handler's last command decides; `command` runs it too.
    let script = "command_not_found_handle() { echo \"h:$1\"; }; command nosuch; echo \"st=$?\"";
    let out = cash_without_path(&dir, script);
    assert_eq!(out.stdout, "h:nosuch\nst=0");
    assert_eq!(out.stdout, git_bash(script).stdout);
}

#[test]
fn the_handler_runs_in_a_subshell_so_exit_ends_only_it() {
    let dir = Scratch::new("cnf-exit");
    let script = "command_not_found_handle() { echo handler; exit 3; }; \
                  nosuch a b; echo \"after st=$?\"";
    let out = cash_without_path(&dir, script);
    assert_eq!(out.stdout, "handler\nafter st=3");
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, git_bash(script).stdout);

    let script = "command_not_found_handle() { echo h; x=1; return 0; }; nosuch; echo \"x=$x\"";
    let out = cash_without_path(&dir, script);
    assert_eq!(out.stdout, "h\nx=");
    assert_eq!(out.stdout, git_bash(script).stdout);
}

#[test]
fn a_handler_that_returns_127_is_the_last_word() {
    let dir = Scratch::new("cnf-127");
    let script = "command_not_found_handle() { echo h; return 127; }; nosuch; echo \"st=$?\"";
    let out = cash_without_path(&dir, script);
    assert_eq!(out.stderr, "", "the shell added a line of its own");
    assert_eq!(out.stdout, "h\nst=127");
    assert_eq!(out.stdout, git_bash(script).stdout);
}

#[test]
fn a_command_spelled_as_a_path_does_not_run_the_handler() {
    let dir = Scratch::new("cnf-path");
    let script =
        "command_not_found_handle() { echo \"h:$1\"; return 5; }; ./nosuch; echo \"st=$?\"";
    let out = cash_without_path(&dir, script);
    assert_eq!(out.stdout, "st=127");
    assert!(
        out.stderr.ends_with("./nosuch: No such file or directory"),
        "{}",
        out.stderr
    );
    assert_eq!(out.stdout, git_bash(script).stdout);
}

#[test]
fn the_handler_speaks_in_a_pipeline_and_a_substitution() {
    let dir = Scratch::new("cnf-pipe");
    let script = "command_not_found_handle() { echo \"h:$1\"; }; \
                  nosuch | tr a-z A-Z; echo \"$(nosuch inner)\"";
    let out = cash_without_path(&dir, script);
    assert_eq!(out.stdout, "H:NOSUCH\nh:nosuch");
    assert_eq!(out.stdout, git_bash(script).stdout);
}

/// An interactive cash on a pseudo console, with `PATH` set to `dir` alone and `jq`,
/// which `help tools` knows, not to be found.
#[test]
fn at_the_prompt_the_handler_speaks_and_no_hint_follows() {
    const STUCK: Duration = Duration::from_secs(10);
    let dir = Scratch::new("cnf-prompt");
    let path = dir.path().to_string_lossy().into_owned();
    let mut session = with_isolated_environment(|env| {
        let mut env: Vec<(&str, &str)> = env
            .iter()
            .copied()
            .filter(|(name, _)| !name.eq_ignore_ascii_case("PATH") && *name != "PS1")
            .collect();
        env.push(("PATH", path.as_str()));
        env.push(("PS1", "PROMPT$ "));
        env.push(("HISTFILE", ""));
        ConPtySession::start_in(
            Path::new(CASH),
            &[
                "--noprofile",
                "--norc",
                "--no-config",
                "--disable-color",
                "-i",
            ],
            Some(&env),
            Some(dir.path()),
        )
    })
    .expect("start cash in a pseudo terminal");
    session.expect("PROMPT$", STUCK).expect("the prompt");

    session
        .send("command_not_found_handle() { echo \"handled $1 ($#)\"; return 9; }\r")
        .unwrap();
    session.send("jq --version\r").unwrap();
    session.send("echo rc=$?\r").unwrap();
    session.expect("rc=9", STUCK).expect("the handler's status");
    let screen = session.screen().text();
    assert!(screen.contains("handled jq (2)"), "{screen}");
    assert!(
        !screen.contains("command not found") && !screen.contains("install it"),
        "the shell spoke beside the handler:\n{screen}"
    );

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("cash exits"), 0);
}
