//! `logger`, util-linux's, writing to the Application event log: a message read back with
//! `wevtutil`, and the rest with `--no-act`, which writes nothing, as util-linux 2.42
//! prints it with `-s`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::{Command, Stdio};

use crate::common::{cash_command, output_of, run};

/// The latest entries the source `cash` wrote, as `wevtutil` prints them in XML.
fn latest_cash_entries() -> String {
    let out = Command::new("wevtutil")
        .args([
            "qe",
            "Application",
            "/q:*[System[Provider[@Name='cash']]]",
            "/c:20",
            "/rd:true",
            "/f:xml",
        ])
        .output()
        .expect("run wevtutil");
    String::from_utf8_lossy(&out.stdout).replace('\0', "")
}

#[test]
fn a_message_reaches_the_application_log_as_tag_and_text() {
    let marker = std::format!("cash-logger-test-{}-{}", std::process::id(), line!());
    let out = run(&std::format!("logger -p user.err -t cashtest {marker}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let entries = latest_cash_entries();
    let at = entries
        .find(&std::format!("<Data>cashtest: {marker}</Data>"))
        .unwrap_or_else(|| panic!("{marker} not in:\n{entries}"));
    // The entry's own <Event>: an error, Event Viewer's level 2.
    let before = entries.get(..at).unwrap_or_default();
    let start = before.rfind("<Event ").expect("the entry's start");
    let event = before.get(start..).unwrap_or_default();
    assert!(event.contains("<Level>2</Level>"), "{event}");
}

#[test]
fn s_prints_the_message_as_util_linux_does() {
    let out = run("logger --no-act -s -p user.err -t tag hello world");
    assert_eq!(out.code, 0);
    // `<11>Oct 11 21:44:17 tag: hello world`: user (1) times 8, plus err (3).
    let line = out.stderr;
    assert!(line.starts_with("<11>"), "{line}");
    assert!(line.ends_with(" tag: hello world"), "{line}");
    assert_eq!(
        line.len(),
        "<11>Oct 11 21:44:17 tag: hello world".len(),
        "{line}"
    );
}

#[test]
fn input_lines_are_logged_one_by_one_and_e_skips_empty_ones() {
    let mut child = cash_command()
        .args([
            "-c",
            "logger --no-act -s -e -t t; logger --no-act -s -i -t u x",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start cash");
    std::io::Write::write_all(child.stdin.as_mut().expect("stdin"), b"one\r\n\ntwo\n")
        .expect("write the input");
    drop(child.stdin.take());
    let out = output_of_child(child);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert!(lines[0].ends_with(" t: one"), "{out}");
    assert!(lines[1].ends_with(" t: two"), "{out}");
    // `-i`: the process id, in brackets after the tag.
    let pid = lines[2]
        .split_once("u[")
        .and_then(|(_, rest)| rest.split_once("]: x"));
    assert!(
        pid.is_some_and(|(pid, _)| pid.parse::<u32>().is_ok()),
        "{out}"
    );
}

/// What a child wrote on standard error, once it has ended.
fn output_of_child(child: std::process::Child) -> String {
    let out = child.wait_with_output().expect("wait for cash");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn unknown_names_and_syslog_daemons_are_refused() {
    let out = run("logger -p foo.info x");
    assert_eq!(
        (out.code, out.stderr.as_str()),
        (1, "logger: unknown facility name: foo")
    );
    let out = run("logger -p user.bogus x");
    assert_eq!(
        (out.code, out.stderr.as_str()),
        (1, "logger: unknown priority name: bogus")
    );
    for option in ["-n host", "-T", "-u /dev/log", "--journald"] {
        let out = output_of(cash_command().args(["-c", &std::format!("logger {option} x")]));
        assert_eq!(out.code, 1, "{option}");
        assert!(
            out.stderr
                .contains("not supported: messages go to the Application event log"),
            "{option}: {}",
            out.stderr
        );
    }
}
