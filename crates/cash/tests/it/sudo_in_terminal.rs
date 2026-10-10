//! `sudo` elevating in this terminal by itself (research/sudo-in-terminal-design.md): the
//! elevated cash takes the console of the cash that asked, so the command gets the keys
//! typed at it and the Ctrl-C, as Unix's `sudo` gives them through the terminal.
//!
//! Each test asks UAC, which someone has to approve: they are ignored in the suites and
//! run by hand, `--run-ignored only -E 'test(/^sudo_in_terminal::/)'`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::path::Path;
use std::time::{Duration, Instant};

use cash_win32::conpty::ConPtySession;

use crate::common::{CASH, Scratch, with_isolated_environment};

/// How long UAC's prompt may wait for someone to approve it.
const APPROVAL: Duration = Duration::from_secs(180);

/// How long the script may take once the command has what it waits for.
const AFTER: Duration = Duration::from_secs(20);

/// Runs `body` as `cash script.sh` on a pseudo console of its own, in a scratch folder.
fn start(name: &str, body: &str) -> (ConPtySession, Scratch) {
    let dir = Scratch::new(&format!("sudo-{name}"));
    std::fs::write(dir.join("script.sh"), body).expect("write the script");
    cash_win32::console::enable_ctrl_c();
    let session = with_isolated_environment(|env| {
        ConPtySession::start_in(
            Path::new(CASH),
            &["--no-config", "script.sh"],
            Some(env),
            Some(dir.path()),
        )
    })
    .expect("start cash on a pseudo console");
    (session, dir)
}

/// Waits for cash to end, and gives its status.
fn ends(session: &mut ConPtySession) -> u32 {
    let started = Instant::now();
    loop {
        let _ = session.read_available();
        if let Some(status) = session.try_wait().expect("ask after cash") {
            return status;
        }
        assert!(
            started.elapsed() < AFTER,
            "cash is still running. The console shows:\n{}",
            session.screen().text()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "asks UAC, which someone has to approve"]
fn ctrl_c_reaches_the_elevated_command() {
    let (mut session, dir) = start(
        "ctrl-c",
        // Windows' ping: cash's own is Unix's, whose `-t` is the TTL.
        "sudo ping.exe -n 60 127.0.0.1\necho \"went on: rc=$?\" >> out.txt\n",
    );
    session
        .expect("TTL=", APPROVAL)
        .expect("the elevated ping replies on this console");
    session.send("\x03").expect("type Ctrl-C");
    let status = ends(&mut session);

    // As for any program Ctrl-C ends: the script ends with it, with SIGINT's status.
    let out = std::fs::read_to_string(dir.join("out.txt")).unwrap_or_default();
    assert_eq!(
        (status, out.as_str()),
        (130, ""),
        "{}",
        session.screen().text()
    );
}

#[test]
#[ignore = "asks UAC, which someone has to approve"]
fn the_elevated_command_reads_the_keys_typed() {
    let (mut session, dir) = start(
        "keys",
        r#"sudo bash -c 'read -r -p "name? " x; echo "got $x" > got.txt'
echo "rc=$?" >> out.txt
cat got.txt >> out.txt
"#,
    );
    // The console draws the prompt's trailing space as a cursor move.
    session
        .expect("name?", APPROVAL)
        .expect("the elevated read asks on this console");
    session.send("hello\r").expect("type a line");
    let status = ends(&mut session);

    let out = std::fs::read_to_string(dir.join("out.txt")).unwrap_or_default();
    assert_eq!(
        (status, out.trim_end()),
        (0, "rc=0\ngot hello"),
        "{}",
        session.screen().text()
    );
}
