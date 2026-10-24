//! cash's prompt, the starter's, when the startup files ran and set none: the directory and
//! the branch, then `❯`. With `--norc`, or a prompt of the user's own (`PROMPT_COMMAND`),
//! Bash's `\s-\v\$ ` stays, as in Bash.

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

/// An interactive cash in `dir` with `args` before `-i`, and no `PS1` handed down.
fn start(dir: &Path, args: &[&str]) -> ConPtySession {
    let temp = std::env::temp_dir();
    let temp = temp.to_string_lossy();
    let mut all = vec![
        "--noprofile",
        "--no-config",
        "--disable-color",
        "--input-backend=reedline",
    ];
    all.extend_from_slice(args);
    all.push("-i");
    ConPtySession::start_sized(
        Path::new(CASH),
        &all,
        Some(&[
            ("HISTFILE", ""),
            ("CASH_NO_OFFER", "1"),
            ("TEMP", &temp),
            ("WT_SESSION", ""),
        ]),
        Some(dir),
        100,
        25,
    )
    .expect("start cash on a pseudo console")
}

fn exits(mut session: ConPtySession) {
    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("cash exits"), 0);
}

#[test]
fn an_rc_file_that_sets_no_prompt_gets_cashs_with_the_branch() {
    let dir = Scratch::new("default-prompt");
    let repo = dir.join("project");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/topic\n").unwrap();
    let rc = dir.join("rc");
    std::fs::write(&rc, "alias ll='ls -l'\n").unwrap();
    let rc = rc.to_string_lossy().into_owned();
    let mut session = start(&repo, &["--rcfile", &rc]);
    session
        .expect("(topic)", WAIT)
        .expect("the prompt names the branch");
    session.send("echo prompt-do''ne\r").unwrap();
    session.expect("prompt-done", WAIT).expect("a command ran");
    // The arrow, or a red `#` as administrator, which CI's runners are.
    let output = session.output();
    assert!(
        output.contains('❯') || output.contains("admin: "),
        "{output}"
    );
    exits(session);
}

#[test]
fn norc_keeps_bashs_prompt() {
    let dir = Scratch::new("default-prompt-norc");
    let mut session = start(dir.path(), &["--norc"]);
    // `cash-X.Y$ `, or `#` for `$` as administrator.
    session.expect("cash-", WAIT).expect("Bash's `cash-X.Y$ `");
    session.send("echo prompt-do''ne\r").unwrap();
    session.expect("prompt-done", WAIT).expect("a command ran");
    assert!(!session.output().contains('❯'), "{}", session.output());
    exits(session);
}

#[test]
fn a_prompt_command_of_the_users_own_is_kept() {
    let dir = Scratch::new("default-prompt-own");
    let rc = dir.join("rc");
    std::fs::write(&rc, "PROMPT_COMMAND='PS1=\"MINE> \"'\n").unwrap();
    let rc = rc.to_string_lossy().into_owned();
    let mut session = start(dir.path(), &["--rcfile", &rc]);
    session.expect("MINE> ", WAIT).expect("the user's prompt");
    assert!(!session.output().contains('❯'), "{}", session.output());
    exits(session);
}
