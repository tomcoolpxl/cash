//! `/dev/tcp/HOST/PORT` and `/dev/udp/HOST/PORT` in redirections, as Bash opens them:
//! a socket in the descriptor table, both directions, Bash's two lines on failure.
//!
//! Every server here is a socket this test owns on 127.0.0.1, bound to port 0, so no
//! port is guessed at. Git Bash 5.3 is the model for the messages: each failing script
//! is run under it too, and the lines compared after the shell's own name.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, UdpSocket};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::common::{Output, cash_command, git_for_windows, output_of};

/// Runs `script` with standard input closed.
fn cash(script: &str) -> Output {
    output_of(cash_command().args(["-c", script]).stdin(Stdio::null()))
}

/// Runs `script` under Git Bash, with standard input closed.
fn git_bash(script: &str) -> Output {
    let bash = format!("{}/bin/bash.exe", git_for_windows());
    output_of(
        Command::new(bash)
            .args(["-c", script])
            .env("MSYS_NO_PATHCONV", "1")
            .stdin(Stdio::null()),
    )
}

/// The lines of standard error with the shell's name taken off the front: Bash's
/// `/usr/bin/bash: line 1: ...` and cash's `C:/.../cash.exe: line 1: ...` both become
/// `line 1: ...`.
fn after_shell_name(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .map(|line| {
            line.split_once(": ")
                .map_or(line, |(_, rest)| rest)
                .to_owned()
        })
        .collect()
}

/// A server that accepts one connection, reads what arrives, writes it back and closes.
fn echo_server() -> (u16, std::thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).unwrap_or(0);
        stream.write_all(&buffer[..n]).unwrap();
        buffer[..n].to_vec()
    });
    (port, server)
}

#[test]
fn exec_opens_a_socket_both_ways_and_closes_it() {
    let (port, server) = echo_server();
    let out = cash(&format!(
        "exec 3<>/dev/tcp/127.0.0.1/{port}; echo \"open=$?\"; echo hello >&3; \
         read -r -t 10 line <&3; echo \"got=$line\"; \
         [ -S /dev/fd/3 ] && echo is-socket; [ -S /dev/fd/0 ] || echo stdin-is-not; \
         exec 3>&-; [ -e /dev/fd/3 ] || echo closed"
    ));
    assert_eq!(out.stderr, "");
    assert_eq!(
        out.stdout,
        "open=0\ngot=hello\nis-socket\nstdin-is-not\nclosed"
    );
    assert_eq!(server.join().unwrap(), b"hello\n");
}

#[test]
fn one_command_can_write_to_or_read_from_the_socket_and_so_can_a_program() {
    // `cat` is a process of its own, and so is cmd.exe: both get the socket as a
    // standard stream they can use.
    let (port, server) = echo_server();
    let out = cash(&format!(
        "exec 3<>/dev/tcp/localhost/{port}; cmd.exe /c \"echo via-cmd\" >&3; cat <&3"
    ));
    assert_eq!(out.stderr, "");
    assert_eq!(out.stdout, "via-cmd");
    assert_eq!(server.join().unwrap(), b"via-cmd\r\n");

    let (port, server) = echo_server();
    let out = cash(&format!(
        "echo one-shot >/dev/tcp/127.0.0.1/{port}; echo \"st=$?\""
    ));
    assert_eq!(out.stdout, "st=0");
    assert_eq!(server.join().unwrap(), b"one-shot\n");
}

#[test]
fn read_t_times_out_on_a_quiet_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let out = cash(&format!(
        "exec 3<>/dev/tcp/127.0.0.1/{port}; read -r -t 0.3 line <&3; echo \"rc=$?\""
    ));
    // Bash's status for a `read -t` that ran out: greater than 128.
    assert_eq!(out.stdout, "rc=142", "{}", out.stderr);
    drop(listener);
}

#[test]
fn dev_udp_sends_a_datagram() {
    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let port = receiver.local_addr().unwrap().port();
    let out = cash(&format!(
        "echo ping >/dev/udp/127.0.0.1/{port}; echo \"st=$?\""
    ));
    assert_eq!(
        (out.code, out.stdout.as_str()),
        (0, "st=0"),
        "{}",
        out.stderr
    );
    let mut buffer = [0u8; 64];
    let (n, _) = receiver.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..n], b"ping\n");
}

#[test]
fn a_refused_port_is_bashs_two_lines_and_status_1() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    for script in [
        format!("exec 3<>/dev/tcp/127.0.0.1/{port}; echo \"status=$?\""),
        format!("cat </dev/tcp/127.0.0.1/{port}; echo \"status=$?\""),
        format!("echo hi >/dev/tcp/127.0.0.1/{port}; echo \"status=$?\""),
    ] {
        let ours = cash(&script);
        let model = git_bash(&script);
        assert_eq!(ours.stdout, "status=1", "{script}: {}", ours.stderr);
        assert_eq!(
            after_shell_name(&ours.stderr),
            [
                "connect: Connection refused".to_owned(),
                format!("line 1: /dev/tcp/127.0.0.1/{port}: Connection refused"),
            ],
            "{script}"
        );
        assert_eq!(
            after_shell_name(&ours.stderr),
            after_shell_name(&model.stderr),
            "{script}: Git Bash differs"
        );
    }
}

#[test]
fn an_unknown_host_or_service_is_a_lookup_failure_then_invalid_argument() {
    let script = "exec 3<>/dev/tcp/nosuchhost.invalid/80; echo \"status=$?\"";
    let ours = cash(script);
    assert_eq!(ours.stdout, "status=1");
    assert_eq!(
        after_shell_name(&ours.stderr),
        [
            "line 1: nosuchhost.invalid: Name or service not known",
            "line 1: /dev/tcp/nosuchhost.invalid/80: Invalid argument",
        ]
    );
    assert_eq!(
        after_shell_name(&ours.stderr),
        after_shell_name(&git_bash(script).stderr)
    );

    let script = "exec 3<>/dev/tcp/127.0.0.1/nosvc; echo \"status=$?\"";
    let ours = cash(script);
    assert_eq!(ours.stdout, "status=1");
    assert_eq!(
        after_shell_name(&ours.stderr),
        [
            "line 1: nosvc: Servname not supported for ai_socktype",
            "line 1: /dev/tcp/127.0.0.1/nosvc: Invalid argument",
        ]
    );
    assert_eq!(
        after_shell_name(&ours.stderr),
        after_shell_name(&git_bash(script).stderr)
    );
}

#[test]
fn a_service_name_stands_for_its_port() {
    // No server listens on port 1 (tcpmux is not in the table), but `http` is: the
    // refusal names the service as written, after resolving it to a port to connect.
    let out = cash("exec 3<>/dev/tcp/127.0.0.1/discard; echo \"status=$?\"");
    assert_eq!(out.stdout, "status=1");
    assert!(
        out.stderr
            .ends_with("discard: Servname not supported for ai_socktype\n")
            || out
                .stderr
                .contains("/dev/tcp/127.0.0.1/discard: Invalid argument"),
        "{}",
        out.stderr
    );
    let out = cash("exec 3<>/dev/tcp/127.0.0.1/http; echo \"status=$?\"");
    // Either a web server answers on this machine, or the port is refused: a
    // resolution failure would be `Invalid argument`.
    assert!(
        !out.stderr.contains("Invalid argument"),
        "http was not taken as port 80:\n{}",
        out.stderr
    );
}

#[test]
fn as_an_argument_the_name_is_a_file_that_does_not_exist() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let out = cash(&format!(
        "cat /dev/tcp/127.0.0.1/{port}; echo \"status=$?\""
    ));
    assert_eq!(out.stdout, "status=1");
    assert!(
        out.stderr
            .starts_with(&format!("cat: /dev/tcp/127.0.0.1/{port}: ")),
        "{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("connect"),
        "a connection was tried:\n{}",
        out.stderr
    );
    assert!(
        listener.accept().is_err(),
        "cat connected to the port it was given as an argument"
    );
    // Without a port the name is no socket name at all.
    let out = cash("exec 3<>/dev/tcp/127.0.0.1; echo \"status=$?\"");
    assert_eq!(out.stdout, "status=1");
    assert!(
        out.stderr
            .ends_with("/dev/tcp/127.0.0.1: No such file or directory"),
        "{}",
        out.stderr
    );
}
