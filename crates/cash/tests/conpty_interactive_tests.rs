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

/// Like [`start_interactive_cash`], on the reedline backend a user gets by default.
///
/// The session's environment is only what is listed here, so `TEMP` is passed on: without
/// it Windows puts temporary files in the Windows directory, where `fc` may not write.
fn start_reedline_cash() -> ConPtySession {
    let temp = std::env::temp_dir();
    let temp = temp.to_string_lossy();
    ConPtySession::start(
        &PathBuf::from(CASH),
        &[
            "--noprofile",
            "--norc",
            "--no-config",
            "--disable-color",
            "--input-backend=reedline",
            "-i",
        ],
        Some(&[("HISTFILE", ""), ("PS1", "PROMPT$ "), ("TEMP", &temp)]),
    )
    .expect("failed to start cash.exe attached to Win32 ConPTY")
}

/// Keys that arrive after Enter belong to the command Enter starts. The line editor reads
/// the console in batches (vendor/crossterm/CASH-PATCHES.md), and a batch that ran past
/// Enter would keep the answer below from `read`, handing it to the next prompt instead.
#[test]
fn conpty_keys_after_enter_reach_the_command_it_runs() {
    let mut session = start_reedline_cash();
    // ConPTY does not send a trailing blank, so the prompt's space never arrives.
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    // One write, as a paste or a fast typist delivers it: the command, Enter, the answer.
    // The line after it waits in the console for the prompt `read` returns to.
    session.send("read -r answer\rTYPED_AHEAD\r").unwrap();
    session.send("echo \"GOT=[$answer]\"\r").unwrap();
    session
        .expect("GOT=[TYPED_AHEAD]", Duration::from_secs(10))
        .expect("read did not get the keys typed after its Enter");

    session.send("exit 0\r").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
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

/// Readline's `yank-last-arg`: Alt-. inserts the previous command's last word, and a second
/// press replaces it with the last word of the command before that.
#[test]
fn conpty_alt_dot_yanks_the_last_argument() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    session.send("true first OLDER_WORD\r").unwrap();
    session.send("true second NEWER_WORD\r").unwrap();

    // The output joins the words with `-`, which the typed line never contains.
    session.send("printf '%s-%s\\n' X \x1b.\r").unwrap();
    session
        .expect("X-NEWER_WORD", Duration::from_secs(10))
        .expect("Alt-. did not insert the last word of the previous command");

    // Now the printf is the previous command: the third press reaches the first `true`.
    session
        .send("printf '%s+%s\\n' Y \x1b.\x1b.\x1b.\r")
        .unwrap();
    session
        .expect("Y+OLDER_WORD", Duration::from_secs(10))
        .expect("repeated Alt-. did not walk back through history");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// Readline's `edit-and-execute-command`: Ctrl-X Ctrl-E opens the line in `$VISUAL` and
/// runs what the editor saves. The "editor" here is cash's `sed -i`.
#[test]
fn conpty_ctrl_x_ctrl_e_runs_the_edited_line() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    session
        .send("export VISUAL='sed -i s/ORIGINAL/EDITED/'\r")
        .unwrap();
    session.send("echo ORIGINAL$((20 + 3))\x18\x05").unwrap();
    session
        .expect("EDITED23", Duration::from_secs(10))
        .expect("Ctrl-X Ctrl-E did not run the edited line");

    // History holds the line as typed and as edited, as Bash's does, and not the `fc`.
    session
        .send("history 3 | sed 's/^ *[0-9]* *//; s/ /_/g'\r")
        .unwrap();
    session
        .expect("echo_EDITED$((20_+_3))", Duration::from_secs(10))
        .expect("the edited line was not recorded in history");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// fish's abbreviations (D60): the command word expands on Space and on Enter, history
/// keeps the expansion, and an erased abbreviation expands no more.
#[test]
fn conpty_abbr_expands_as_the_command_word() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    // Quoted whole, so the expansion keeps its own quotes.
    session.send("abbr -a pj \"printf '%s-%s\\n'\"\r").unwrap();

    // On Space: the typed line becomes `printf '%s-%s\n' A B`, whose output the typed
    // line never contains. The keys go in one write, so they are read together.
    session.send("pj A B\r").unwrap();
    session
        .expect("\r\nA-B\x1b[K", Duration::from_secs(10))
        .expect("the abbreviation did not expand on Space");

    // Not as an argument: the output line is `pj C` itself.
    session.send("echo pj C\r").unwrap();
    session
        .expect("\r\npj C\x1b[K", Duration::from_secs(10))
        .expect("an argument expanded");

    // On Enter, with nothing after it.
    session
        .send("abbr -a pk \"printf 'K%s%s\\n' x y\"\r")
        .unwrap();
    session.send("pk\r").unwrap();
    session
        .expect("\r\nKxy\x1b[K", Duration::from_secs(10))
        .expect("the abbreviation did not expand on Enter");

    // The session has no PATH, so cash's own awk: history lines whose command is the
    // expanded `printf 'K…`.
    session
        .send("history 5 | awk '$2 == \"printf\" && /K%s/ {n++} END {print \"HIST=\" n}'\r")
        .unwrap();
    session
        .expect("HIST=1", Duration::from_secs(10))
        .expect("history did not keep the expansion");

    session.send("abbr -e pj\r").unwrap();
    session.send("pj D E\r").unwrap();
    session
        .expect("command not found: pj", Duration::from_secs(10))
        .expect("an erased abbreviation still expanded");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// The collapsing prompt (D61): with `CASH_TRANSIENT_PS1` set, an entered line keeps that
/// prompt, expanded like PS1, instead of the full one; unset, prompts stay as they were.
#[test]
fn conpty_transient_prompt_replaces_an_entered_lines_prompt() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    session.send("CASH_TRANSIENT_PS1='T\\$ '\r").unwrap();
    session.send("echo COLLAPSED_$((6 * 7))\r").unwrap();
    session
        .expect("COLLAPSED_42", Duration::from_secs(10))
        .expect("command did not run");
    session.send("unset CASH_TRANSIENT_PS1\r").unwrap();
    session.send("echo FULL_$((6 * 8))\r").unwrap();
    session
        .expect("FULL_48", Duration::from_secs(10))
        .expect("command did not run");
    session
        .settle(Duration::from_millis(300), Duration::from_secs(5))
        .unwrap();

    let screen = session.screen().text();
    let line_of = |needle: &str| {
        screen
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or_default()
            .trim_end()
            .to_owned()
    };
    // The line that set it was entered at the full prompt; the next one collapses.
    assert_eq!(
        line_of("CASH_TRANSIENT_PS1="),
        "PROMPT$ CASH_TRANSIENT_PS1='T\\$ '",
        "{screen}"
    );
    assert_eq!(
        line_of("echo COLLAPSED"),
        "T$ echo COLLAPSED_$((6 * 7))",
        "{screen}"
    );
    assert_eq!(
        line_of("echo FULL"),
        "PROMPT$ echo FULL_$((6 * 8))",
        "{screen}"
    );

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// Keys typed right after a `bind -x` key, read in the same batch, reach the next prompt;
/// the line editor dropped them (vendor/reedline/CASH-PATCHES.md, patch 5).
#[test]
fn conpty_keys_after_a_bound_key_reach_the_next_prompt() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");
    session.send("bind -x '\"\\C-t\": true'\r").unwrap();
    session
        .settle(Duration::from_millis(300), Duration::from_secs(5))
        .unwrap();

    // One write: the bound key, then a whole command typed ahead.
    session.send("\x14echo TYPED_AHEAD_$((2 + 2))\r").unwrap();
    session
        .expect("TYPED_AHEAD_4", Duration::from_secs(10))
        .expect("keys typed after a bound key were lost");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// fish's folder history on the keys (D62): Alt-← and Alt-→ on an empty line go back and
/// forward through the folders visited; on a line with text they move a word.
#[test]
fn conpty_alt_arrows_walk_the_folder_history() {
    const ALT_LEFT: &str = "\x1b[1;3D";
    const ALT_RIGHT: &str = "\x1b[1;3C";

    let root = tempfile::tempdir().unwrap();
    for name in ["first_dir", "second_dir"] {
        std::fs::create_dir(root.path().join(name)).unwrap();
    }
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    let root = root.path().to_string_lossy().replace('\\', "/");
    session
        .send(&format!("cd '{root}/first_dir'; cd ../second_dir\r"))
        .unwrap();
    session
        .send("here() { echo \"AT=${PWD##*/}\"; }\r")
        .unwrap();

    session.send(ALT_LEFT).unwrap();
    session.send("here\r").unwrap();
    session
        .expect("AT=first_dir", Duration::from_secs(10))
        .expect("Alt-Left on an empty line did not go back");

    session.send(ALT_RIGHT).unwrap();
    session.send("here\r").unwrap();
    session
        .expect("AT=second_dir", Duration::from_secs(10))
        .expect("Alt-Right on an empty line did not go forward");

    // With text on the line, Alt-Left moves to the start of the last word.
    session.send("echo abc def").unwrap();
    session.send(ALT_LEFT).unwrap();
    session.send("Z\r").unwrap();
    session
        .expect("abc Zdef", Duration::from_secs(10))
        .expect("Alt-Left on a line with text did not move a word");
    session.send("here\r").unwrap();
    session
        .expect("AT=second_dir\x1b[K\r\nPROMPT$", Duration::from_secs(10))
        .expect("Alt-Left on a line with text changed folder");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}
