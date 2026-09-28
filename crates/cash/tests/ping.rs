//! `ping` with Linux's flags — ROADMAP item 10, part 4 (spec D57).
//!
//! Windows' `ping.exe` reads `-c` as a routing compartment, so `ping -c 1 host && …`
//! failed at the cash prompt for every host. Output and exit statuses here were read off
//! iputils 20250605. Only the loopback addresses are pinged, so the tests need no
//! network; a host that never answers is simulated with an address in TEST-NET-1's
//! neighbourhood that the machine has no route to answer for, and skipped if it does.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction"
)]

use std::os::windows::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::time::Duration;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// `text` with every run of digits (and a decimal point between them) replaced by `N`,
/// so timings and the TTL compare as a shape.
fn shape(text: &str) -> String {
    let mut out = String::new();
    let mut in_number = false;
    for c in text.chars() {
        if c.is_ascii_digit() || (in_number && c == '.') {
            if !in_number {
                out.push('N');
                in_number = true;
            }
        } else {
            in_number = false;
            out.push(c);
        }
    }
    out
}

#[test]
fn ping_is_cash_s_own() {
    let out = cash("type ping");
    assert_eq!(out.stdout, "ping is a shell builtin\n");
}

#[test]
fn a_reply_is_reported_as_iputils_reports_it() {
    let out = cash("ping -c 2 -i 0.2 127.0.0.1");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        shape(&out.stdout),
        "PING N (N) N(N) bytes of data.\n\
         N bytes from N: icmp_seq=N ttl=N time=N ms\n\
         N bytes from N: icmp_seq=N ttl=N time=N ms\n\
         \n\
         --- N ping statistics ---\n\
         N packets transmitted, N received, N% packet loss, time Nms\n\
         rtt min/avg/max/mdev = N/N/N/N ms\n"
    );
    assert!(
        out.stdout
            .starts_with("PING 127.0.0.1 (127.0.0.1) 56(84) bytes of data.\n")
    );
    assert!(
        out.stdout
            .contains("2 packets transmitted, 2 received, 0% packet loss")
    );
}

#[test]
fn ipv6_quiet_size_and_timestamps() {
    let v6 = cash("ping -c 1 -q ::1");
    assert_eq!(v6.code, 0, "{}", v6.stderr);
    assert!(
        v6.stdout
            .starts_with("PING ::1 (::1) 56 data bytes\n\n--- ::1 ping statistics ---\n")
    );

    let sized = cash("ping -c 1 -s 100 127.0.0.1");
    assert!(
        sized.stdout.contains("100(128) bytes of data."),
        "{}",
        sized.stdout
    );
    assert!(
        sized.stdout.contains("\n108 bytes from 127.0.0.1:"),
        "{}",
        sized.stdout
    );

    let stamped = cash("ping -c 1 -D 127.0.0.1");
    let line = stamped.stdout.lines().nth(1).unwrap_or_default();
    assert!(
        line.starts_with('[') && line.contains("] 64 bytes from"),
        "{line}"
    );
}

#[test]
fn a_script_can_test_a_host() {
    // The idiom ping.exe broke: its -c is not a count.
    let out = cash("ping -c 1 -W 2 127.0.0.1 >/dev/null && echo up || echo down");
    assert_eq!(out.stdout, "up\n");
}

#[test]
fn usage_errors_and_names_are_reported_as_iputils_does() {
    let none = cash("ping");
    assert_eq!(
        none.stderr,
        "ping: usage error: Destination address required\n"
    );
    assert_eq!(none.code, 2);

    let unknown = cash("ping -c 1 no-such-host.invalid");
    assert_eq!(
        unknown.stderr,
        "ping: no-such-host.invalid: Name or service not known\n"
    );
    assert_eq!(unknown.code, 2);

    let family = cash("ping -c 1 -4 ::1");
    assert_eq!(
        family.stderr,
        "ping: ::1: Address family for hostname not supported\n"
    );

    let count = cash("ping -c x 127.0.0.1");
    assert_eq!(count.stderr, "ping: invalid argument: 'x'\n");
    assert_eq!(count.code, 1);

    let flood = cash("ping -c 1 -i 0.001 127.0.0.1");
    assert_eq!(
        flood.stderr,
        "ping: cannot flood, minimal interval for user must be >= 2 ms, use -i 0.002 (or higher)\n"
    );
}

#[test]
fn a_windows_count_gets_a_hint_not_an_endless_ping() {
    // iputils would read `-n 3 127.0.0.1` as numeric output via hop 3, and never stop.
    let out = cash("ping -n 3 127.0.0.1");
    assert_eq!(out.code, 2);
    assert!(
        out.stderr
            .contains("hint: ping.exe's `-n 3` is `-c 3` here"),
        "{}",
        out.stderr
    );
}

#[test]
fn raw_socket_options_are_refused_by_name() {
    let out = cash("ping -f 127.0.0.1");
    assert!(
        out.stderr.starts_with("ping: -f is not supported:"),
        "{}",
        out.stderr
    );
    assert_eq!(out.code, 2);
}

#[test]
fn an_unanswered_ping_reports_total_loss() {
    let out = cash("ping -c 2 -i 0.2 -W 1 -O 10.254.254.254");
    if out.stdout.contains("bytes from") {
        eprintln!("skipping: 10.254.254.254 answers on this network");
        return;
    }
    assert_eq!(out.code, 1);
    assert!(
        out.stdout.contains("no answer yet for icmp_seq=1\n"),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("icmp_seq=2"), "{}", out.stdout);
    assert!(
        out.stdout
            .contains("2 packets transmitted, 0 received, 100% packet loss"),
        "{}",
        out.stdout
    );
    // No rtt line: iputils prints an empty line in its place.
    assert!(out.stdout.ends_with("ms\n\n"), "{:?}", out.stdout);
}

#[test]
fn an_interrupt_prints_the_statistics() {
    // The bundled ping in a process group of its own, sharing this console, so a
    // Ctrl-Break can be aimed at it alone.
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    let child = Command::new(CASH)
        .args(["--invoke-bundled", "ping", "-i", "0.2", "127.0.0.1"])
        .creation_flags(CREATE_NEW_PROCESS_GROUP)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run ping");
    std::thread::sleep(Duration::from_millis(1200));
    if cash_win32::console::interrupt_process_group(child.id()).is_err() {
        eprintln!("skipping: this test process has no console to send an interrupt on");
        let mut child = child;
        let _ = child.kill();
        return;
    }
    let out = child.wait_with_output().expect("wait for ping");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--- 127.0.0.1 ping statistics ---"),
        "{stdout}"
    );
    assert!(stdout.contains("rtt min/avg/max/mdev"), "{stdout}");
    assert_eq!(out.status.code(), Some(0), "{stdout}");
}
