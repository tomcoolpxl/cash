//! `watch`: procps-ng's, on a pseudo console.
//!
//! Windows has no `watch` and Git for Windows carries none, so `watch -n 1 'ls | wc -l'`
//! was "command not found". The options, the usage and the exit statuses are checked
//! against procps-ng 4.0.7's without a console; the screen, the keys and the ways a run
//! ends by itself (`-g`, `-q`, `-e`) are driven on a pseudo console, where the script
//! writes `watch`'s status to `out.txt`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::PathBuf;
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

use crate::common::{CASH, Scratch, run, with_isolated_environment};
use crate::read_console::Script;

/// procps's usage text begins so, on standard output for `-h` and on standard error
/// after an option error or without a command.
const USAGE_START: &str = "\nUsage:\n watch [options] command\n\nOptions:\n  -b, --beep             beep if command has a non-zero exit";

#[test]
fn help_and_version_need_no_terminal() {
    for flag in ["--help", "-h"] {
        let out = run(&format!("watch {flag}"));
        assert_eq!(out.code, 0, "{}", out.stderr);
        assert!(out.stdout.starts_with(USAGE_START), "{}", out.stdout);
        assert!(out.stdout.ends_with(
            " -v, --version  output version information and exit\n\nFor more details see watch(1)."
        ));
    }
    for flag in ["--version", "-v"] {
        let out = run(&format!("watch {flag}"));
        assert_eq!(out.code, 0);
        assert_eq!(out.stdout, "watch (cash): procps-ng 4.0.7's options");
    }
}

/// getopt's errors are followed by the usage and fail with 1, as procps has them.
#[test]
fn a_bad_option_and_a_missing_command_print_the_usage() {
    let out = run("watch -z true; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert!(
        out.stderr
            .starts_with(&format!("watch: invalid option -- 'z'\n{USAGE_START}")),
        "{}",
        out.stderr
    );

    let out = run("watch --bogus true; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert!(
        out.stderr.starts_with(&format!(
            "watch: unrecognized option '--bogus'\n{USAGE_START}"
        )),
        "{}",
        out.stderr
    );

    let out = run("watch -n; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert!(
        out.stderr
            .starts_with("watch: option requires an argument -- 'n'\n"),
        "{}",
        out.stderr
    );

    let out = run("watch; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert!(out.stderr.starts_with(USAGE_START), "{}", out.stderr);
}

/// procps's own errors, without the usage.
#[test]
fn a_bad_interval_or_cycle_count_is_refused_in_procps_words() {
    let out = run("watch -n abc true; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(
        out.stderr,
        "watch: failed to parse argument: 'abc': Invalid argument"
    );

    let out = run("watch -q soon true; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(out.stderr, "watch: failed to parse argument: 'soon'");

    let out = run("WATCH_INTERVAL=later watch true; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(
        out.stderr,
        "watch: Could not parse interval from WATCH_INTERVAL: 'later': Invalid argument"
    );
}

/// `cash -c` under `output()` writes to a pipe: nothing to draw on.
#[test]
fn without_a_terminal_to_draw_on_watch_fails() {
    let out = run("watch true; echo rc=$?");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(out.stderr, "watch: standard output is not a terminal");

    let out = run("watch -n 0.2 -g true | cat; echo rc=${PIPESTATUS[0]}");
    assert_eq!(out.stdout, "rc=1");
    assert_eq!(out.stderr, "watch: standard output is not a terminal");
}

/// The header, the command's output, and `q`: the script goes on with status 0, and the
/// main screen comes back with nothing of the frame on it.
#[test]
fn the_screen_shows_the_header_and_the_output_until_q() {
    let left = Script::start(
        "quit",
        r#"watch -n 5 'echo hello from watch'; echo "rc=$?" >> out.txt"#,
    )
    .when_shown("Every 5.0s: echo hello from watch")
    .when_shown("hello from watch")
    .type_keys("q")
    .finish();

    assert_eq!(left.out, "rc=0");
    assert_eq!(left.screen, "", "the frame was left on the main screen");
}

/// Ctrl-C leaves as `q` does, with 0, as procps's handler does; the script goes on.
#[test]
fn ctrl_c_leaves_with_status_0() {
    let left = Script::start(
        "ctrl-c",
        r#"watch -n 5 echo running; echo "rc=$?" >> out.txt"#,
    )
    .when_shown("running")
    .type_keys("\x03")
    .finish();

    assert_eq!(left.out, "rc=0");
}

/// Standard error joins standard output, and a command the shell cannot find shows the
/// shell's message with its status in the header.
#[test]
fn standard_error_and_a_missing_command_are_shown() {
    let left = Script::start(
        "stderr",
        r#"watch -n 5 'echo out; echo err >&2; nosuchcommand'; echo "rc=$?" >> out.txt"#,
    )
    .when_shown("err")
    .when_shown("nosuchcommand: command not found")
    .when_shown("(127)")
    .type_keys("q")
    .finish();

    assert_eq!(left.out, "rc=0");
}

/// `-x`: the words are one command, not shell source; a program that is not there is
/// reported as procps reports a failed exec.
#[test]
fn x_runs_the_words_as_one_command() {
    let left = Script::start(
        "exec",
        r#"watch -n 5 -x echo one 'two words'; echo "rc=$?" >> out.txt
watch -n 5 -x nosuchprogram; echo "rc=$?" >> out.txt"#,
    )
    .when_shown("Every 5.0s: echo one two words")
    .when_shown("one two words")
    .type_keys("q")
    .when_shown("nosuchprogram: No such file or directory")
    .when_shown("(127)")
    .type_keys("q")
    .finish();

    assert_eq!(left.out, "rc=0\nrc=0");
}

/// `-g` ends with 0 as soon as the visible output differs from the first run's.
#[test]
fn g_ends_when_the_output_changes() {
    let left = Script::start(
        "chgexit",
        r#"watch -n 0.2 -g 'echo x >> n; cat n'; echo "rc=$?" >> out.txt
wc -l < n | tr -d ' ' >> out.txt"#,
    )
    .finish();

    // Two runs: the first draws, the second differs.
    assert_eq!(left.out, "rc=0\n2");
}

/// `-q N` ends with 0 once the output has stayed the same for N runs after the first.
#[test]
fn q_ends_when_the_output_stops_changing() {
    let left = Script::start(
        "equexit",
        r#"watch -n 0.2 -q 2 'echo x >> n; echo same'; echo "rc=$?" >> out.txt
wc -l < n | tr -d ' ' >> out.txt"#,
    )
    .finish();

    assert_eq!(left.out, "rc=0\n3");
}

/// `-e`: a failing command freezes the screen with procps's message; a key ends `watch`
/// with the command's status.
#[test]
fn e_freezes_on_a_failure_and_leaves_with_its_status_after_a_key() {
    let left = Script::start(
        "errexit",
        r#"watch -n 0.2 -e 'echo failing; exit 3'; echo "rc=$?" >> out.txt"#,
    )
    .when_shown("command exit with a non-zero status, press a key to exit")
    .type_keys("x")
    .finish();

    assert_eq!(left.out, "rc=3");
}

/// `-d`: the cells that changed since the last run are drawn in reverse video.
///
/// The pseudo console passes on only what changed on its screen, so the second frame
/// arrives as the one new cell: the screen is read as text, the stream for the attribute.
#[test]
fn d_marks_what_changed_in_reverse_video() {
    let dir = Scratch::new("watch-differences");
    std::fs::write(dir.join("n"), "aaa").expect("write the file watched");
    let mut session = with_isolated_environment(|env| {
        ConPtySession::start_in(
            &PathBuf::from(CASH),
            &[
                "--no-config",
                "-c",
                "watch -n 0.2 -d 'cat n; printf b >> n'",
            ],
            Some(env),
            Some(dir.path()),
        )
    })
    .expect("start cash in a pseudo terminal");

    let start = std::time::Instant::now();
    while !session.screen().text().contains("aaab") {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the second run was not shown:\n{}",
            session.screen().text()
        );
        session.read_available().expect("read the console");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        session.output().contains("\x1b[7m"),
        "no reverse video: {:?}",
        session.output()
    );
    session.send("q").expect("quit watch");
    assert_eq!(session.wait().expect("watch exits"), 0);
    drop(session);
}

/// `-t` leaves the header out; the frame is drawn on the alternate screen, which is left
/// on quitting.
#[test]
fn t_leaves_the_header_out_and_the_alternate_screen_is_put_back() {
    let mut session = with_isolated_environment(|env| {
        ConPtySession::start(
            &PathBuf::from(CASH),
            &["--no-config", "-c", "watch -t -n 5 echo untitled"],
            Some(env),
        )
    })
    .expect("start cash in a pseudo terminal");
    session
        .expect("untitled", Duration::from_secs(10))
        .expect("the command's output");
    session.send("q").expect("quit watch");
    assert_eq!(session.wait().expect("watch exits"), 0);

    let output = session.output();
    assert!(!output.contains("Every "), "{output:?}");
    let entered = output
        .find("\x1b[?1049h")
        .expect("watch draws on the alternate screen");
    let left = output
        .rfind("\x1b[?1049l")
        .expect("and leaves it when it quits");
    // The pseudo console clears its own screen as it starts; what watch drew is between
    // entering the alternate screen and leaving it.
    let drawn = output.get(entered..left).unwrap_or_default();
    assert!(drawn.contains("untitled"), "{drawn:?}");
    assert!(
        !drawn.contains("\x1b[2J"),
        "watch cleared the screen: {drawn:?}"
    );
}
