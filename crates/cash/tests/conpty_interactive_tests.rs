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

use cash_win32::conpty::ConPtySession;
use std::path::PathBuf;
use std::time::Duration;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

fn start_interactive_cash() -> ConPtySession {
    let cash_path = PathBuf::from(CASH);
    assert!(
        cash_path.exists(),
        "cash binary not found: {}",
        cash_path.display()
    );

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

    session
        .send_line("FOO=conpty_value; echo \"VAL=$FOO\"")
        .unwrap();
    session
        .expect("VAL=conpty_value", Duration::from_secs(5))
        .unwrap();

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
    session
        .expect("1  echo cmd_one", Duration::from_secs(5))
        .unwrap();
    session
        .expect("2  echo cmd_two", Duration::from_secs(5))
        .unwrap();

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
    session
        .send_line("for x in alpha beta gamma; do echo \"ITEM:$x\"; done")
        .unwrap();
    session
        .expect("ITEM:alpha", Duration::from_secs(5))
        .unwrap();
    session.expect("ITEM:beta", Duration::from_secs(5)).unwrap();
    session
        .expect("ITEM:gamma", Duration::from_secs(5))
        .unwrap();

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
    session
        .expect("alpha beta_target", Duration::from_secs(5))
        .unwrap();

    // Verify !$ expands to last argument of previous command.
    session.send_line("echo EXP_DOLLAR=!$").unwrap();
    session
        .expect("echo EXP_DOLLAR=beta_target", Duration::from_secs(5))
        .unwrap();
    session
        .expect("EXP_DOLLAR=beta_target", Duration::from_secs(5))
        .unwrap();

    // Verify !^ expands to first argument of previous command (EXP_DOLLAR=beta_target).
    session.send_line("echo EXP_CARET=!^").unwrap();
    session
        .expect(
            "echo EXP_CARET=EXP_DOLLAR=beta_target",
            Duration::from_secs(5),
        )
        .unwrap();

    // Verify quick substitution ^old^new^.
    session.send_line("echo hello world").unwrap();
    session
        .expect("hello world", Duration::from_secs(5))
        .unwrap();
    session.send_line("^world^cash_user^").unwrap();
    session
        .expect("echo hello cash_user", Duration::from_secs(5))
        .unwrap();
    session
        .expect("hello cash_user", Duration::from_secs(5))
        .unwrap();

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
    session
        .expect("cash: !$: event not found", Duration::from_secs(5))
        .unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_read_e_edits_initial_text() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // Send one Enter key. `send_line` writes CRLF, which ConPTY can expose as two
    // key events when the command switches the console into raw mode immediately.
    session
        .send(concat!(
            r#"read -e -p "EDIT> " -i ac value; echo "READ=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("EDIT> ac", Duration::from_secs(5))
        .expect("read did not display its prompt and initial text");

    // Move between 'a' and 'c', insert 'b', and submit the edited buffer.
    session.send("\x1b[Db\r").unwrap();
    session
        .expect("READ=[abc]", Duration::from_secs(5))
        .expect("read -e did not return the edited text");

    session
        .send(concat!(
            r#"read -e -p "EDIT2> " -i axbc value; echo "EDIT2=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("EDIT2> axbc", Duration::from_secs(5))
        .expect("second read did not start");

    // Home, Right, Delete, End, Backspace, then replace the final character.
    session.send("\x1b[H\x1b[C\x1b[3~\x1b[F\x7fc\r").unwrap();
    session
        .expect("EDIT2=[abc]", Duration::from_secs(5))
        .expect("read -e navigation and deletion produced the wrong text");

    session
        .send(concat!(
            r#"read -e -d : -p "DELIM> " value; echo "DELIM=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("DELIM> ", Duration::from_secs(5))
        .expect("delimiter read did not start");
    session.send("a\\:b:").unwrap();
    session
        .expect("DELIM=[a:b]", Duration::from_secs(5))
        .expect("read -e did not preserve an escaped custom delimiter");

    // Bash only uses -i when Readline was requested with -e or -E.
    session
        .send(concat!(
            r#"read -i seed -p "PLAIN> " value; echo "PLAIN=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("PLAIN> ", Duration::from_secs(5))
        .expect("plain read did not start");
    session.send("actual\r").unwrap();
    session
        .expect("PLAIN=[actual]", Duration::from_secs(5))
        .expect("read -i incorrectly applied initial text without -e or -E");

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_read_e_navigates_in_memory_history() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    session.send_line("history -s first-entry").unwrap();
    session.send_line("history -s second-entry").unwrap();
    session
        .send(concat!(
            r#"read -e -p "HIST> " value; echo "HISTORY=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("HIST> ", Duration::from_secs(5))
        .expect("read history prompt did not appear");

    // The command containing `read` is itself the newest history entry. Walk past it
    // to the two entries injected above, then forward once with Ctrl-N.
    session.send("\x10\x10\x10\x0e\r").unwrap();
    session
        .expect("HISTORY=[second-entry]", Duration::from_secs(5))
        .expect("read -e did not navigate the shell's in-memory history");

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_read_capital_e_uses_shell_completion() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    session
        .send(concat!(
            r#"read -E -p "COMPLETE> " -i ech value; printf 'COMPLETED=[%s]\n' "$value""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("COMPLETE> ech", Duration::from_secs(5))
        .expect("read completion prompt did not appear");
    session.send("\t\r").unwrap();
    session
        .expect("COMPLETED=[echo]", Duration::from_secs(5))
        .expect("read -E did not use the shell completion engine");

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}
