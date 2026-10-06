//! F1 at the prompt: the one-screen help for Windows users who know Bash, on the
//! alternate screen, closed by F1, Esc or `q`, scrolled when the window is short; and
//! `cash-help`, the Readline function behind it, which `bind` lists and can move.
//!
//! Each test runs cash on a pseudo console with the reedline editor a user gets, types
//! at it, and reads what the console shows. Colour is off, so the words are whole in
//! the output; the styles have unit tests of their own.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::Path;
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

use crate::common::{CASH, run};

/// F1, F2, Esc, Down and End, as a terminal sends them.
const F1: &str = "\x1bOP";
const F2: &str = "\x1bOQ";
const ESC: &str = "\x1b";
const DOWN: &str = "\x1b[B";
const END: &str = "\x1b[F";

const WAIT: Duration = Duration::from_secs(10);

/// An interactive cash on an 80-column console of `rows` rows, at its prompt.
fn start(rows: i16) -> ConPtySession {
    let temp = std::env::temp_dir();
    let temp = temp.to_string_lossy();
    let mut session = ConPtySession::start_sized(
        Path::new(CASH),
        &[
            "--noprofile",
            "--norc",
            "--no-config",
            "--disable-color",
            "--input-backend=reedline",
            "-i",
        ],
        Some(&[
            ("HISTFILE", ""),
            ("PS1", "PROMPT$ "),
            ("CASH_NO_OFFER", "1"),
            ("TEMP", &temp),
        ]),
        None,
        80,
        rows,
    )
    .expect("start cash on a pseudo console");
    session.expect("PROMPT$", WAIT).expect("the first prompt");
    session
}

/// Lets the editor act on a key before the next is typed: a console host merges keys
/// typed at once into one frame.
fn pause() {
    std::thread::sleep(Duration::from_millis(400));
}

/// The title's left half: the version is the running binary's.
fn title() -> String {
    format!("cash {}", env!("CARGO_PKG_VERSION"))
}

fn exits(mut session: ConPtySession) {
    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("cash exits"), 0);
}

/// F1 draws the title with the version, the three sections and the tool count; F1
/// again closes it, and the prompt takes a line as before.
#[test]
fn f1_shows_the_screen_and_f1_closes_it() {
    let mut session = start(25);
    session.send(F1).unwrap();
    for text in [
        title().as_str(),
        "help",
        "F1 or Esc closes",
        "PATHS",
        "cd C:/Users/me/src",
        "KEYS",
        "Alt-E",
        "MORE",
        "0 tools are built in.",
    ] {
        let shown = session.expect(text, WAIT).is_ok();
        assert!(shown, "the screen shows {text:?}:\n{}", session.output());
    }
    session.send(F1).unwrap();
    pause();
    session.send("echo f1-do''ne\r").unwrap();
    session
        .expect("f1-done", WAIT)
        .expect("the prompt is back and runs a line");
    exits(session);
}

/// Esc brings the prompt back with the line as it was: what was typed before F1 runs
/// on Enter.
#[test]
fn esc_puts_the_prompt_back_with_the_line_intact() {
    let mut session = start(25);
    session.send("echo intact-l''ine").unwrap();
    session
        .expect("intact-l''ine", WAIT)
        .expect("the line is typed");
    session.send(F1).unwrap();
    session.expect("PATHS", WAIT).expect("the screen opened");
    session.send(ESC).unwrap();
    pause();
    session.send("\r").unwrap();
    session
        .expect("intact-line", WAIT)
        .expect("the line typed before F1 ran");
    exits(session);
}

#[test]
fn q_closes_the_screen() {
    let mut session = start(25);
    session.send(F1).unwrap();
    session.expect("KEYS", WAIT).expect("the screen opened");
    session.send("q").unwrap();
    pause();
    session.send("echo q-do''ne\r").unwrap();
    session.expect("q-done", WAIT).expect("the prompt is back");
    // The `q` closed the screen rather than going on the line.
    assert!(!session.output().contains("qecho"), "{}", session.output());
    exits(session);
}

/// In a window too short for the text, the title stays and the rest scrolls: Down
/// brings the next line into view, End the last.
#[test]
fn a_short_window_scrolls_on_down_and_end() {
    let mut session = start(15);
    session.send(F1).unwrap();
    session.expect("KEYS", WAIT).expect("the screen opened");
    session
        .settle(Duration::from_millis(200), Duration::from_secs(5))
        .unwrap();
    assert!(
        !session.output().contains("Ctrl-R"),
        "fifteen rows show the whole text:\n{}",
        session.output()
    );
    session.send(DOWN).unwrap();
    session
        .expect("Ctrl-R", WAIT)
        .expect("Down brought the next line into view");
    session.send(END).unwrap();
    session
        .expect("elevates", WAIT)
        .expect("End brought the last line into view");
    session.send(ESC).unwrap();
    pause();
    session.send("echo scroll-do''ne\r").unwrap();
    session
        .expect("scroll-done", WAIT)
        .expect("the prompt is back");
    exits(session);
}

/// `bind -l` lists `cash-help`, `bind -p` shows it on F1, and `bind` puts it on
/// another key. The listings are read in capitals, which the typed lines are not in.
#[test]
fn bind_lists_cash_help_and_can_move_it() {
    let mut session = start(25);
    session.send("bind -l | tr a-z A-Z\r").unwrap();
    session
        .expect("CASH-HELP", WAIT)
        .expect("bind -l lists cash-help");
    session.send("bind -p | tr a-z A-Z\r").unwrap();
    session
        .expect(r#""\EOP": CASH-HELP"#, WAIT)
        .expect("bind -p shows cash-help on F1");

    session.send("bind '\"\\eOQ\": cash-he'lp\r").unwrap();
    pause();
    session.send(F2).unwrap();
    session
        .expect("PATHS", WAIT)
        .expect("F2, bound to cash-help, opened the screen");
    session.send(ESC).unwrap();
    pause();
    session.send("echo bound-do''ne\r").unwrap();
    session
        .expect("bound-done", WAIT)
        .expect("the prompt is back");
    exits(session);
}

/// Without an editor, in a script or `cash -c`, `bind -l` still lists the functions.
#[test]
fn bind_l_lists_cash_help_without_an_editor() {
    let out = run("bind -l");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout.lines().any(|line| line == "cash-help"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.lines().any(|line| line == "cash-picker"));
}
