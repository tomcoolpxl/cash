//! Readline's init file: `$INPUTRC` or `~/.inputrc` read before the first prompt, and
//! `bind -f FILE`. The listings are read in capitals (`bind -p | tr a-z A-Z`), which
//! the typed lines are not in, so the echo of a command never passes for its output.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed set-up should abort it loudly"
)]

use std::path::Path;
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

use crate::common::{CASH, Scratch};

const WAIT: Duration = Duration::from_secs(10);

/// An inputrc that binds F2 and F3 under tests that hold, F4 under ones that do not, and
/// tries what cash's editor lacks: a function, a variable, a missing include, a vi keymap.
const INPUTRC: &str = r#"# a comment
$if Bash
"\eOQ": cash-help
$else
"\eOS": cash-help
$endif
$if mode=vi
"\eOS": cash-help
$endif
$if version >= 8.0
Control-t: kill-line
$endif
set keymap vi-insert
"\eOR": cash-help
set keymap emacs
Meta-Rubout: no-such-function
set no-such-variable on
$include ~/no-such-file
"#;

/// An interactive cash with `INPUTRC` set to `inputrc`, at its first prompt; `args`
/// come before `-i`.
fn start(inputrc: &Path, args: &[&str]) -> ConPtySession {
    let temp = std::env::temp_dir();
    let temp = temp.to_string_lossy();
    let inputrc = inputrc.to_string_lossy();
    let mut all = vec![
        "--noprofile",
        "--no-config",
        "--disable-color",
        "--input-backend=reedline",
    ];
    all.extend_from_slice(args);
    all.push("-i");
    let mut session = ConPtySession::start_sized(
        Path::new(CASH),
        &all,
        Some(&[
            ("HISTFILE", ""),
            ("PS1", "PROMPT$ "),
            ("CASH_NO_OFFER", "1"),
            ("TEMP", &temp),
            ("INPUTRC", &inputrc),
        ]),
        None,
        120,
        25,
    )
    .expect("start cash on a pseudo console");
    session.expect("PROMPT$", WAIT).expect("the first prompt");
    session
}

fn exits(mut session: ConPtySession) {
    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("cash exits"), 0);
}

#[test]
fn the_startup_file_binds_what_cash_has_and_says_nothing() {
    let dir = Scratch::new("inputrc-startup");
    let file = dir.join("inputrc");
    std::fs::write(&file, INPUTRC).unwrap();
    let mut session = start(&file, &["--norc"]);
    session
        .send("bind -p | tr a-z A-Z; echo listed-do''ne\r")
        .unwrap();
    session.expect("listed-done", WAIT).expect("bind -p ran");
    let output = session.output();
    assert!(output.contains(r#""\EOQ": CASH-HELP"#), "{output}");
    assert!(output.contains(r#""\C-T": KILL-LINE"#), "{output}");
    assert!(
        !output.contains(r#""\EOS""#),
        "a test that fails bound: {output}"
    );
    assert!(
        !output.contains(r#""\EOR""#),
        "the vi keymap bound: {output}"
    );
    // The console's own escape sequences come before the prompt; a message would be words.
    let before_prompt = output.split("PROMPT$").next().unwrap_or_default();
    for word in ["bind", "inputrc", "unknown", "no-such", "cannot"] {
        assert!(
            !before_prompt.contains(word),
            "the start said something: {before_prompt:?}"
        );
    }
    exits(session);
}

#[test]
fn a_bind_in_the_rc_file_wins_over_the_inputrc() {
    let dir = Scratch::new("inputrc-rc");
    let file = dir.join("inputrc");
    std::fs::write(&file, "\"\\eOQ\": cash-help\n").unwrap();
    let rc = dir.join("rc");
    std::fs::write(&rc, "bind '\"\\eOQ\": kill-line'\n").unwrap();
    let rc = rc.to_string_lossy().into_owned();
    let mut session = start(&file, &["--rcfile", &rc]);
    session
        .send("bind -p | grep -i eoq | tr a-z A-Z; echo listed-do''ne\r")
        .unwrap();
    session.expect("listed-done", WAIT).expect("bind -p ran");
    assert!(
        session.output().contains(r#""\EOQ": KILL-LINE"#),
        "{}",
        session.output()
    );
    exits(session);
}

#[test]
fn bind_f_reads_a_file_and_says_when_it_cannot() {
    let dir = Scratch::new("inputrc-bind-f");
    let empty = dir.join("empty");
    std::fs::write(&empty, "").unwrap();
    let file = dir.join("keys");
    std::fs::write(&file, "\"\\eOS\": cash-help\n").unwrap();
    let mut session = start(&empty, &["--norc"]);
    let file = file.to_string_lossy().replace('\\', "/");
    session
        .send(&format!(
            "bind -f '{file}'; bind -p | tr a-z A-Z; echo listed-do''ne\r"
        ))
        .unwrap();
    session.expect("listed-done", WAIT).expect("bind -p ran");
    assert!(
        session.output().contains(r#""\EOS": CASH-HELP"#),
        "{}",
        session.output()
    );
    session
        .send("bind -f no-such-file; echo status-$?-do''ne\r")
        .unwrap();
    session
        .expect("status-1-done", WAIT)
        .expect("a missing file is status 1");
    assert!(
        session.output().contains("cannot read"),
        "{}",
        session.output()
    );
    exits(session);
}
