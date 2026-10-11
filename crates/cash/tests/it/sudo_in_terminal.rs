//! `sudo` elevating in this terminal by itself (research/sudo-in-terminal-design.md): the
//! elevated cash takes the console of the cash that asked, so the command gets the keys
//! typed at it and the Ctrl-C, as Unix's `sudo` gives them through the terminal.
//!
//! The tests that ask UAC, which someone has to approve, are ignored in the suites and
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

/// `sudo -u USER` asks USER's password at the console, and Ctrl-C there runs nothing, as
/// Unix's `su` ends. The test's own account, whose password is never typed.
#[test]
fn ctrl_c_at_the_password_prompt_runs_nothing() {
    let user = std::env::var("USERNAME").expect("USERNAME is set on Windows");
    let (mut session, dir) = start(
        "password",
        &format!("sudo -u '{user}' cmd /c 'echo ran> ran.txt'\necho \"rc=$?\" >> out.txt\n"),
    );
    session
        .expect(&format!("Password for {user}:"), AFTER)
        .expect("sudo asks the password at the console");
    session.send("\x03").expect("type Ctrl-C");
    let status = ends(&mut session);

    let out = std::fs::read_to_string(dir.join("out.txt")).unwrap_or_default();
    assert!(!dir.join("ran.txt").exists(), "the command ran");
    assert!(
        status == 130 || out.trim_end() == "rc=130",
        "status {status}, out {out:?}:\n{}",
        session.screen().text()
    );
}

/// `sudo -u USER` runs the command as USER, in this terminal, with the redirection the
/// shell made. Needs an account to run as: its name in `CASH_TEST_USER` and its password in
/// `CASH_TEST_PASSWORD`, which the test types at the prompt. That account must be able to
/// read the cash under test, which lies in your profile:
/// `icacls target\debug\cash.exe /grant NAME:RX` lets it. The command runs from the test's
/// own folder, under your profile, which the account cannot enter — so it also exercises
/// the fallback to the system temp. The redirected file is written through the inherited
/// handle regardless of the folder.
#[test]
#[ignore = "needs a second account and its password"]
fn sudo_u_runs_the_command_as_the_account() {
    let (Ok(user), Ok(password)) = (
        std::env::var("CASH_TEST_USER"),
        std::env::var("CASH_TEST_PASSWORD"),
    ) else {
        eprintln!("skipped: CASH_TEST_USER and CASH_TEST_PASSWORD name no account");
        return;
    };
    let (mut session, dir) = start(
        "as-user",
        &format!(
            r#"sudo -u '{user}' cmd /c 'echo %USERNAME%' > who.txt
echo "rc=$?" >> out.txt
"#
        ),
    );
    session
        .expect(&format!("Password for {user}:"), AFTER)
        .expect("sudo asks the password at the console");
    session
        .send(&format!("{password}\r"))
        .expect("type the password");
    let status = ends(&mut session);

    let out = std::fs::read_to_string(dir.join("out.txt")).unwrap_or_default();
    let who = std::fs::read_to_string(dir.join("who.txt")).unwrap_or_default();
    let name = user.rsplit('\\').next().unwrap_or(&user);
    assert_eq!(
        (status, out.trim_end(), who.trim_end().to_lowercase()),
        (0, "rc=0", name.to_lowercase()),
        "{}",
        session.screen().text()
    );
}

/// `sudo -u USER` with a console command writing to the terminal (no redirection): its
/// output shows in this terminal, not a window of its own. Needs the account, as above.
#[test]
#[ignore = "needs a second account and its password"]
fn sudo_u_shows_console_output_in_this_terminal() {
    let (Ok(user), Ok(password)) = (
        std::env::var("CASH_TEST_USER"),
        std::env::var("CASH_TEST_PASSWORD"),
    ) else {
        eprintln!("skipped: CASH_TEST_USER and CASH_TEST_PASSWORD name no account");
        return;
    };
    let (mut session, dir) = start(
        "as-user-console",
        &format!("sudo -u '{user}' cmd /c 'echo CASSMARK-%USERNAME%'\necho \"rc=$?\" >> out.txt\n"),
    );
    session
        .expect(&format!("Password for {user}:"), AFTER)
        .expect("sudo asks the password at the console");
    session
        .send(&format!("{password}\r"))
        .expect("type the password");
    // The marker appears on this console, with the account's name, not in a window of its
    // own; it shows while the command still runs, so wait for the script to finish before
    // the status it then writes is read.
    session
        .expect("CASSMARK-", AFTER)
        .expect("the console command's output shows in this terminal");
    let status = ends(&mut session);
    let out = std::fs::read_to_string(dir.join("out.txt")).unwrap_or_default();
    assert_eq!(
        (status, out.trim_end()),
        (0, "rc=0"),
        "{}",
        session.screen().text()
    );
}

/// How many `PING.EXE` processes `account` owns, asked of Windows.
fn pings_owned_by(account: &str) -> usize {
    let script = format!(
        "@(Get-CimInstance Win32_Process -Filter \"Name='PING.EXE'\" | \
         Where-Object {{ (Invoke-CimMethod -InputObject $_ -MethodName GetOwner).User -eq '{account}' }}).Count"
    );
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .ok()
        .and_then(|out| String::from_utf8_lossy(&out.stdout).trim().parse().ok())
        .unwrap_or(0)
}

/// `sudo -u USER`'s command dies when the shell that started it is killed, not left behind.
/// Needs the account, as above.
#[test]
#[ignore = "needs a second account and its password"]
fn sudo_u_dies_when_the_shell_is_killed() {
    let (Ok(user), Ok(password)) = (
        std::env::var("CASH_TEST_USER"),
        std::env::var("CASH_TEST_PASSWORD"),
    ) else {
        eprintln!("skipped: CASH_TEST_USER and CASH_TEST_PASSWORD name no account");
        return;
    };
    let account = user.rsplit('\\').next().unwrap_or(&user).to_string();
    // Windows' ping: cash's own is Unix's, whose `-t` is the TTL.
    let (mut session, _dir) = start(
        "orphan",
        &format!("sudo -u '{user}' ping.exe -t 127.0.0.1\n"),
    );
    session
        .expect(&format!("Password for {user}:"), AFTER)
        .expect("sudo asks the password at the console");
    session
        .send(&format!("{password}\r"))
        .expect("type the password");
    // The relay shows the account's ping replying in this terminal, so it is running.
    session
        .expect("Reply from 127.0.0.1", AFTER)
        .expect("the account's ping replies in this terminal");
    assert!(
        pings_owned_by(&account) >= 1,
        "no ping is running as {account}"
    );

    // Kill the shell outright, as from Task Manager — not a Ctrl-C.
    session.terminate();

    // The account's ping must die with it, through the kill-on-close job, within a moment.
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(15) {
        if pings_owned_by(&account) == 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("the account's ping outlived the killed shell");
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
