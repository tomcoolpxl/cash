//! What Git for Windows' `/etc/profile` and `/etc/inputrc` give an interactive Git Bash,
//! which cash gives too: `~/bin` first on `PATH`, each `~/bash_completion.d/*.bash`
//! sourced, `no_empty_cmd_completion` on, and `PageUp` and `PageDown` to the oldest history
//! entry and back to the line being typed. Before the startup files, and not with
//! `--norc`.

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
const PAGE_UP: &str = "\x1b[5~";
const PAGE_DOWN: &str = "\x1b[6~";

/// Lets the editor act on a key before the next is typed.
fn pause() {
    std::thread::sleep(Duration::from_millis(400));
}

fn start(home: &Path, args: &[&str]) -> ConPtySession {
    let temp = std::env::temp_dir();
    let temp = temp.to_string_lossy();
    let home = home.to_string_lossy();
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
            ("HOME", &home),
            ("USERPROFILE", &home),
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

/// A home with `bin` and a completion script that leaves a mark.
fn home(name: &str) -> Scratch {
    let home = Scratch::new(name);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::create_dir_all(home.join("bash_completion.d")).unwrap();
    std::fs::write(
        home.join("bash_completion.d/mine.bash"),
        "FROM_COMPLETION_DIR=loaded\n",
    )
    .unwrap();
    std::fs::write(home.join("rc"), "true\n").unwrap();
    home
}

#[test]
fn bin_completion_scripts_and_the_option_are_set_up_before_the_rc_file() {
    let home = home("gitbash-defaults");
    let rc = home.join("rc").to_string_lossy().into_owned();
    let mut session = start(home.path(), &["--rcfile", &rc]);
    session
        .send("echo \"[${PATH%%:*}] [$FROM_COMPLETION_DIR]\" | tr a-z A-Z\r")
        .unwrap();
    session
        .expect("/BIN] [LOADED]", WAIT)
        .expect("~/bin first on PATH, the completion script sourced");
    session
        .send("shopt -q no_empty_cmd_completion && echo option-o''n\r")
        .unwrap();
    session
        .expect("option-on", WAIT)
        .expect("no_empty_cmd_completion on");
    exits(session);
}

#[test]
fn norc_sets_none_of_it_up() {
    let home = home("gitbash-defaults-norc");
    let mut session = start(home.path(), &["--norc"]);
    session
        .send("echo \"[${FROM_COMPLETION_DIR-unset}]\" | tr a-z A-Z\r")
        .unwrap();
    session
        .expect("[UNSET]", WAIT)
        .expect("no completion script");
    exits(session);
}

#[test]
fn page_up_recalls_the_oldest_entry_and_page_down_the_line_being_typed() {
    let home = home("gitbash-defaults-pages");
    let mut session = start(home.path(), &["--norc"]);
    session.send("echo old''est\r").unwrap();
    session
        .expect("oldest", WAIT)
        .expect("the first command ran");
    session.send("echo new''er\r").unwrap();
    session
        .expect("newer", WAIT)
        .expect("the second command ran");
    pause();
    session.send("echo half").unwrap();
    pause();
    session.send(PAGE_UP).unwrap();
    pause();
    session.send(PAGE_DOWN).unwrap();
    pause();
    session.send("-typed\r").unwrap();
    session
        .expect("half-typed", WAIT)
        .expect("PageDown brought back the line being typed");
    pause();
    session.send(PAGE_UP).unwrap();
    pause();
    session.send(" | tr a-z A-Z\r").unwrap();
    session
        .expect("OLDEST", WAIT)
        .expect("PageUp brought back the oldest entry");
    exits(session);
}
