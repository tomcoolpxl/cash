//! Native Win32 ConPTY interactive tests for cash.
//!
//! Spawns real `cash.exe` attached to a genuine Windows Pseudo Console (ConPTY)
//! with real terminal geometry (80x25), real VT100 / ANSI escape sequences,
//! and raw character I/O.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::single_char_pattern,
    clippy::literal_string_with_formatting_args,
    reason = "Integration tests test the compiled binary and assert loudly on failure."
)]

use std::path::PathBuf;
use std::time::Duration;
use cash_win32::conpty::ConPtySession;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

fn start_interactive_cash() -> ConPtySession {
    let cash_path = PathBuf::from(CASH);
    assert!(cash_path.exists(), "cash binary not found: {}", cash_path.display());

    // Start cash with isolated flags and basic backend for automated ConPTY testing.
    ConPtySession::start(
        &cash_path,
        &[
            "--noprofile",
            "--no-config",
            "--disable-bracketed-paste",
            "--disable-color",
            "--input-backend=basic",
            "-i",
        ],
        Some(&[("HISTFILE", "")]),
    )
    .expect("failed to start cash.exe attached to Win32 ConPTY")
}

#[test]
fn conpty_interactive_startup_and_prompt() {
    let mut session = start_interactive_cash();

    // The shell starts and prompts for input.
    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("did not see prompt in ConPTY");

    // Send a simple arithmetic expansion command.
    session.send_line("echo RESULT=$((20 + 22))").unwrap();
    session.expect("RESULT=42", Duration::from_secs(5)).unwrap();

    // Exit cleanly with status 0.
    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_variables_and_arithmetic() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    session.send_line("FOO=conpty_value; echo \"VAL=$FOO\"").unwrap();
    session.expect("VAL=conpty_value", Duration::from_secs(5)).unwrap();

    session.send_line("exit 17").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 17);
}

#[test]
fn conpty_interactive_history_and_pipeline() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // Execute multiple commands.
    session.send_line("echo cmd_one").unwrap();
    session.expect("cmd_one", Duration::from_secs(5)).unwrap();

    session.send_line("echo cmd_two").unwrap();
    session.expect("cmd_two", Duration::from_secs(5)).unwrap();

    // Run history builtin and expect numbered history table entries.
    session.send_line("history").unwrap();
    session.expect("1  echo cmd_one", Duration::from_secs(5)).unwrap();
    session.expect("2  echo cmd_two", Duration::from_secs(5)).unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_multiline_block() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // Multi-line loop.
    session.send_line("for x in alpha beta gamma; do echo \"ITEM:$x\"; done").unwrap();
    session.expect("ITEM:alpha", Duration::from_secs(5)).unwrap();
    session.expect("ITEM:beta", Duration::from_secs(5)).unwrap();
    session.expect("ITEM:gamma", Duration::from_secs(5)).unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_history_expansion_bang_dollar() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // Run initial command with arguments.
    session.send_line("echo alpha beta_target").unwrap();
    session.expect("alpha beta_target", Duration::from_secs(5)).unwrap();

    // Verify !$ expands to last argument of previous command.
    session.send_line("echo EXP_DOLLAR=!$").unwrap();
    session.expect("echo EXP_DOLLAR=beta_target", Duration::from_secs(5)).unwrap();
    session.expect("EXP_DOLLAR=beta_target", Duration::from_secs(5)).unwrap();

    // Verify !^ expands to first argument of previous command (EXP_DOLLAR=beta_target).
    session.send_line("echo EXP_CARET=!^").unwrap();
    session.expect("echo EXP_CARET=EXP_DOLLAR=beta_target", Duration::from_secs(5)).unwrap();

    // Verify quick substitution ^old^new^.
    session.send_line("echo hello world").unwrap();
    session.expect("hello world", Duration::from_secs(5)).unwrap();
    session.send_line("^world^cash_user^").unwrap();
    session.expect("echo hello cash_user", Duration::from_secs(5)).unwrap();
    session.expect("hello cash_user", Duration::from_secs(5)).unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_empty_history_bang_dollar() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // When history is empty, typing `ls !$` must output event not found and NOT pass '!$' as a literal argument to ls.
    session.send_line("ls !$").unwrap();
    session.expect("cash: !$: event not found", Duration::from_secs(5)).unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}


