//! `nc`, OpenBSD netcat's: connecting, scanning, listening and the data between the two,
//! against sockets this test process owns, so that no port is guessed at.
//!
//! A Rust `TcpListener` or `UdpSocket` is bound to port 0 and the port it got is passed
//! to `nc`; for `nc -l`, the port is taken from a listener bound to 0 and released first,
//! and the client retries until the listener has come up. The messages and statuses are
//! netcat's as Debian's netcat-openbsd 1.229 prints them.
//!
//! Two tests run `nc` on a pseudo console, where standard input is the console: a line
//! typed reaches the other side, the console is handed back when the connection ends, and
//! Ctrl-C ends a listener with 130.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use crate::common::{Output, cash_command, output_of};
use crate::read_console::Script;

/// Runs `script` with standard input closed.
fn cash(script: &str) -> Output {
    output_of(cash_command().args(["-c", script]).stdin(Stdio::null()))
}

/// Runs `script` with `input` on standard input.
fn cash_with_input(script: &str, input: &[u8]) -> Output {
    let mut child: Child = cash_command()
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start cash");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input)
        .expect("write to cash");
    let out = child.wait_with_output().expect("wait for cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// A port no one listens on: bound and released.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Reads from `stream` until it closes, or `limit` passes.
fn read_all(stream: &mut TcpStream, limit: Duration) -> Vec<u8> {
    stream.set_read_timeout(Some(limit)).unwrap();
    let mut data = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => data.extend_from_slice(&buffer[..n]),
        }
    }
    data
}

#[test]
fn nc_is_cash_s_own() {
    let out = cash("type nc");
    assert_eq!(out.stdout, "nc is a shell builtin");
}

#[test]
fn a_scan_of_an_open_port_is_silent_and_succeeds() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let out = cash(&format!("nc -z 127.0.0.1 {port}"));
    assert_eq!(
        (out.code, out.stdout.as_str(), out.stderr.as_str()),
        (0, "", "")
    );
}

#[test]
fn a_verbose_scan_reports_the_port_and_its_service() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let out = cash(&format!("nc -zv 127.0.0.1 {port}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        format!("Connection to 127.0.0.1 {port} port [tcp/*] succeeded!")
    );
    // A name shows its address, and a known port its service.
    let out = cash(&format!("nc -zv localhost {port}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stderr
            .lines()
            .last()
            .unwrap_or_default()
            .ends_with(&format!("(127.0.0.1) {port} port [tcp/*] succeeded!")),
        "{}",
        out.stderr
    );
}

#[test]
fn a_closed_port_fails_and_says_so_only_with_v() {
    let port = free_port();
    let out = cash(&format!("nc -z 127.0.0.1 {port}"));
    assert_eq!(
        (out.code, out.stdout.as_str(), out.stderr.as_str()),
        (1, "", "")
    );
    let out = cash(&format!("nc -zv 127.0.0.1 {port}"));
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        format!("nc: connect to 127.0.0.1 port {port} (tcp) failed: Connection refused")
    );
}

#[test]
fn a_connect_timeout_runs_out_in_about_a_second() {
    // TEST-NET-1's neighbourhood: nothing routes there, and nothing answers.
    let started = Instant::now();
    let out = cash("nc -zv -w 1 10.255.255.1 80");
    let took = started.elapsed();
    if out.stderr.contains("Network is unreachable") || out.stderr.contains("No route") {
        // A machine without a default route refuses at once; the point is the status.
        assert_eq!(out.code, 1);
        return;
    }
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert_eq!(
        out.stderr,
        "nc: connect to 10.255.255.1 port 80 (tcp) failed: Connection timed out"
    );
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_secs(8),
        "took {took:?}"
    );
}

#[test]
fn a_port_range_reports_each_port() {
    let one = TcpListener::bind("127.0.0.1:0").unwrap();
    let two = TcpListener::bind("127.0.0.1:0").unwrap();
    let (a, b) = (
        one.local_addr().unwrap().port(),
        two.local_addr().unwrap().port(),
    );
    let (lo, hi) = (a.min(b), a.max(b));
    if hi - lo > 50 {
        // Two listeners far apart would scan too many ports in between; the range form
        // is still checked with one port.
        let out = cash(&format!("nc -zv 127.0.0.1 {lo}-{lo}"));
        assert_eq!(out.code, 0, "{}", out.stderr);
        assert_eq!(
            out.stderr,
            format!("Connection to 127.0.0.1 {lo} port [tcp/*] succeeded!")
        );
        return;
    }
    let out = cash(&format!("nc -zv 127.0.0.1 {lo}-{hi}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let succeeded: Vec<&str> = out
        .stderr
        .lines()
        .filter(|line| line.ends_with("succeeded!"))
        .collect();
    assert!(
        succeeded
            .contains(&format!("Connection to 127.0.0.1 {lo} port [tcp/*] succeeded!").as_str())
            && succeeded.contains(
                &format!("Connection to 127.0.0.1 {hi} port [tcp/*] succeeded!").as_str()
            ),
        "{}",
        out.stderr
    );
}

#[test]
fn standard_input_reaches_the_other_side_and_its_close_ends_nc() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut got = [0u8; 6];
        stream.read_exact(&mut got).unwrap();
        stream.write_all(b"back\n").unwrap();
        // Closing ends nc, whose standard input is long gone but was not shut down.
        drop(stream);
        got
    });
    let out = cash(&format!("echo hello | nc 127.0.0.1 {port}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "back");
    assert_eq!(&server.join().unwrap(), b"hello\n");
}

#[test]
fn n_closes_the_sending_side_when_standard_input_ends() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        // Reading to the end works only because -N sent a FIN after the data.
        let got = read_all(&mut stream, Duration::from_secs(5));
        stream.write_all(b"seen\n").unwrap();
        got
    });
    let out = cash(&format!("echo hello | nc -N 127.0.0.1 {port}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "seen");
    assert_eq!(&server.join().unwrap(), b"hello\n");
}

#[test]
fn q_quits_so_long_after_standard_input_ends() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut got = [0u8; 6];
        stream.read_exact(&mut got).unwrap();
        // The server holds the connection open; nc leaves on its own, well before.
        std::thread::sleep(Duration::from_secs(4));
        got
    });
    let started = Instant::now();
    let out = cash(&format!("echo hello | nc -q 1 127.0.0.1 {port}"));
    let took = started.elapsed();
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_millis(3500),
        "took {took:?}"
    );
    assert_eq!(&server.join().unwrap(), b"hello\n");
}

/// Starts `nc -l` with `flags` on a free port and returns the child and the port, once a
/// client can reach it.
fn listener(flags: &str) -> (Child, u16) {
    let port = free_port();
    let child = cash_command()
        .args(["-c", &format!("nc -l {flags} {port}")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start cash");
    (child, port)
}

/// Sends `text` to a listener on `port` with `nc -N`, retrying while the listener is
/// still coming up (a refused connection prints nothing and exits 1).
fn send_to_listener(port: u16, text: &str) -> Output {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let out = cash(&format!("echo {text} | nc -N 127.0.0.1 {port}"));
        if out.code == 0 || Instant::now() >= deadline {
            return out;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_listener_prints_what_a_client_sends_and_ends_with_it() {
    let (child, port) = listener("");
    let client = send_to_listener(port, "hi");
    assert_eq!(client.code, 0, "{}", client.stderr);
    let out = child.wait_with_output().expect("wait for the listener");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), "hi");
}

#[test]
fn a_verbose_listener_names_its_address_and_the_peer() {
    let (child, port) = listener("-v");
    let client = send_to_listener(port, "hi");
    assert_eq!(client.code, 0, "{}", client.stderr);
    let out = child.wait_with_output().expect("wait for the listener");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut lines = stderr.lines();
    assert_eq!(
        lines.next(),
        Some(format!("Listening on 0.0.0.0 {port}").as_str())
    );
    let received = lines.next().unwrap_or_default();
    assert!(
        received.starts_with("Connection received on 127.0.0.1 "),
        "{stderr}"
    );
    assert!(
        received
            .rsplit(' ')
            .next()
            .unwrap_or_default()
            .parse::<u16>()
            .is_ok(),
        "{stderr}"
    );
}

#[test]
fn k_keeps_listening_for_a_second_client() {
    let (mut child, port) = listener("-k");
    let first = send_to_listener(port, "one");
    assert_eq!(first.code, 0, "{}", first.stderr);
    let second = send_to_listener(port, "two");
    assert_eq!(second.code, 0, "{}", second.stderr);
    // Still there: it is killed, and what it printed read.
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        child.try_wait().unwrap().is_none(),
        "the listener left after one client"
    );
    child.kill().unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        "one\ntwo\n"
    );
}

#[test]
fn u_sends_a_datagram() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let out = cash(&format!("echo hi | nc -u -q 0 127.0.0.1 {port}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let mut buffer = [0u8; 64];
    let (n, _) = socket.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..n], b"hi\n");
}

#[test]
fn u_l_prints_datagrams_and_answers_the_sender() {
    let port = free_port();
    let mut child = cash_command()
        .args(["-c", &format!("nc -u -l -q 1 127.0.0.1 {port}")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start cash");
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    // Datagrams until the listener is up and answers: with -q 1 and standard input
    // closed, the answer is what was piped in, sent once a sender is known.
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"pong\n").unwrap();
    drop(stdin);
    let mut buffer = [0u8; 64];
    let deadline = Instant::now() + Duration::from_secs(10);
    let answered = loop {
        socket.send_to(b"ping\n", ("127.0.0.1", port)).unwrap();
        if let Ok((n, _)) = socket.recv_from(&mut buffer) {
            break buffer[..n].to_vec();
        }
        assert!(Instant::now() < deadline, "no answer from nc -u -l");
    };
    assert_eq!(answered, b"pong\n");
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("ping\n"));
}

#[test]
fn a_service_name_is_a_port_and_an_unknown_one_is_refused() {
    let out = cash("nc -zv 127.0.0.1 bogus");
    assert_eq!(out.code, 1);
    assert_eq!(out.stderr, "nc: port number invalid: bogus");
    // A dash makes a range, as in netcat, and the first half is what is refused.
    let out = cash("nc -zv 127.0.0.1 bogus-service");
    assert_eq!(out.stderr, "nc: port number invalid: bogus");
    let out = cash("nc -z 127.0.0.1 0");
    assert_eq!(out.stderr, "nc: port number too small: 0");
    // A name that is not known to the resolver.
    let out = cash("nc -z no-such-host.invalid 80");
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.starts_with("nc: getaddrinfo: "),
        "{}",
        out.stderr
    );
    let out = cash("nc -zn localhost 80");
    assert_eq!(out.stderr, "nc: getaddrinfo: Name or service not known");
}

#[test]
fn e_and_c_are_no_options_and_the_usage_follows() {
    for letter in ["e", "c"] {
        let out = cash(&format!("nc -{letter} cmd.exe 127.0.0.1 80"));
        assert_eq!(out.code, 1);
        assert_eq!(
            out.stderr.lines().next(),
            Some(format!("nc: invalid option -- '{letter}'").as_str())
        );
        assert!(
            out.stderr.contains("usage: nc [-46bCDdFhklNnrStUuvZz]"),
            "{}",
            out.stderr
        );
    }
    let out = cash("nc");
    assert_eq!(out.code, 1);
    assert!(out.stderr.starts_with("usage: nc "), "{}", out.stderr);
}

#[test]
fn what_windows_cannot_do_is_refused_by_name() {
    let out = cash("nc -U /tmp/sock");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "nc: -U: Unix domain sockets are not supported; nc speaks TCP and UDP"
    );
    let out = cash("nc -x proxy:1080 host 80");
    assert_eq!(
        out.stderr,
        "nc: -x, -X and -P: proxies are not supported; connect directly"
    );
    let out = cash("nc -V 1 host 80");
    assert_eq!(
        out.stderr,
        "nc: -V: alternate routing tables do not exist on Windows"
    );
    let out = cash("nc -lz 80");
    assert_eq!(out.stderr, "nc: cannot use -z and -l");
}

#[test]
fn help_cites_nothing_internal() {
    let out = cash("nc -h");
    assert_eq!(out.code, 0);
    assert!(
        out.stdout
            .starts_with("nc (cash): OpenBSD netcat's options"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("usage: nc [-46bCDdFhklNnrStUuvZz]"));
    assert!(
        out.stdout
            .contains("\t\t-z\t\tZero-I/O mode [used for scanning]")
    );
    assert!(
        out.stdout
            .ends_with("Port numbers can be individual or ranges: lo-hi [inclusive]")
    );
    for word in ["D7", "ROADMAP", "spec", "research", "§"] {
        assert!(!out.stdout.contains(word), "{word} in {}", out.stdout);
    }
    // OpenBSD's -V is a routing table, not a version: without one, getopt complains.
    let out = cash("nc -V");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr.lines().next(),
        Some("nc: option requires an argument -- 'V'")
    );
    let out = cash("help nc");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("WINDOWS NOTES"), "{}", out.stdout);
}

#[test]
fn standard_input_from_a_pipe_in_a_script_is_carried_whole() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_all(&mut stream, Duration::from_secs(5))
    });
    let out = cash_with_input(&format!("nc -N 127.0.0.1 {port}"), b"line one\nline two\n");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(&server.join().unwrap(), b"line one\nline two\n");
}

#[test]
fn big_c_sends_crlf_line_endings() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_all(&mut stream, Duration::from_secs(5))
    });
    let out = cash(&format!("printf 'a\\nb\\n' | nc -C -N 127.0.0.1 {port}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(&server.join().unwrap(), b"a\r\nb\r\n");
}

/// At a console, a line typed goes to the other side, and when that side closes, `nc`
/// ends and hands the console back whole: the `read` that follows gets the next line,
/// which a reader of the console left behind would have taken.
#[test]
fn a_line_typed_at_the_console_reaches_the_other_side_and_the_console_is_handed_back() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut got = [0u8; 6];
        stream.read_exact(&mut got).unwrap();
        stream.write_all(b"back\n").unwrap();
        drop(stream);
        got
    });
    let left = Script::start(
        "nc-console",
        &format!(
            "nc 127.0.0.1 {port}; echo \"rc=$?\" >> out.txt\nread -r after; echo \"after=$after\" >> out.txt"
        ),
    )
    .type_keys("hello\r")
    .when_shown("back")
    .type_keys("after\r")
    .finish();
    assert_eq!(left.out, "rc=0\nafter=after", "{}", left.screen);
    assert_eq!(&server.join().unwrap(), b"hello\n");
}

/// Ctrl-C typed at the console ends a waiting listener as it ends a command: the script
/// ends with 130 and its `EXIT` trap run; with a trap on `INT`, the trap runs and the
/// script goes on, `nc` having left with 130.
#[test]
fn ctrl_c_at_the_console_ends_a_listener_with_130() {
    // A test runner may ignore Ctrl-C, and a child inherits that.
    cash_win32::console::enable_ctrl_c();
    let port = free_port();
    let (status, left) = Script::start(
        "nc-ctrl-c",
        &format!(
            "trap 'echo \"exit trap\" >> out.txt' EXIT\nnc -lv {port}\necho \"went on\" >> out.txt"
        ),
    )
    .when_shown(&format!("Listening on 0.0.0.0 {port}"))
    .type_keys("\x03")
    .ends();
    assert_eq!(status, 130, "{}", left.screen);
    assert_eq!(left.out, "exit trap", "{}", left.screen);

    let port = free_port();
    let left = Script::start(
        "nc-ctrl-c-trap",
        &format!("trap 'echo trapped >> out.txt' INT\nnc -lv {port}\necho \"rc=$?\" >> out.txt"),
    )
    .when_shown(&format!("Listening on 0.0.0.0 {port}"))
    .type_keys("\x03")
    .finish();
    assert_eq!(left.out, "trapped\nrc=130", "{}", left.screen);
}
