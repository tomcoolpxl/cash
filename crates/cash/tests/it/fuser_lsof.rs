//! `fuser` and the `lsof` subset — ROADMAP item 8.
//!
//! The test process itself holds the files and sockets being looked up, so the expected
//! pid is known exactly. Output shapes follow psmisc 23.7 `fuser` and lsof 4.99.7: bare
//! pids on standard output, names and access letters on standard error, exit 0 when
//! something was found and 1 when nothing was.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::common::{Scratch, cash_command};

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash_in(dir: &Path, script: &str) -> Output {
    let out = cash_command()
        .current_dir(dir)
        .args(["--noprofile", "--norc", "-c", script])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
        code: out.status.code().unwrap_or(-1),
    }
}

fn fixture(name: &str) -> Scratch {
    Scratch::new(&format!("fuser-{name}"))
}

fn me() -> String {
    std::process::id().to_string()
}

#[test]
fn fuser_prints_bare_pids_on_stdout_and_the_name_on_stderr() {
    let dir = fixture("held");
    std::fs::write(dir.join("held.txt"), "x").unwrap();
    std::fs::write(dir.join("free.txt"), "x").unwrap();
    let _held = std::fs::File::open(dir.join("held.txt")).unwrap();

    let out = cash_in(dir.path(), "fuser held.txt");
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout.trim(), me());
    // A space, then the pid right-aligned in five columns: psmisc's `%6d` below 100000,
    // and still a space before a pid of six digits or more.
    assert_eq!(
        out.stdout.trim_end(),
        std::format!(" {:>5}", me()),
        "pids are a space and %5d: {:?}",
        out.stdout
    );
    assert!(out.stderr.contains("/held.txt:"), "{}", out.stderr);

    let free = cash_in(dir.path(), "fuser free.txt");
    assert_eq!((free.code, free.stdout.as_str()), (1, ""));

    let missing = cash_in(dir.path(), "fuser nope.txt");
    assert_eq!(missing.code, 1);
    assert!(
        missing
            .stderr
            .contains("Specified filename nope.txt does not exist."),
        "{}",
        missing.stderr
    );
}

#[test]
fn fuser_verbose_and_user_listings() {
    let dir = fixture("verbose");
    std::fs::write(dir.join("held.txt"), "x").unwrap();
    let _held = std::fs::File::open(dir.join("held.txt")).unwrap();

    let out = cash_in(dir.path(), "fuser -v held.txt");
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.is_empty(),
        "-v writes to stderr: {:?}",
        out.stdout
    );
    assert!(
        out.stderr
            .starts_with(&format!("{:20} USER        PID ACCESS COMMAND\n", "")),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains(&format!("{:>6} f.... ", me())),
        "{}",
        out.stderr
    );

    let user = cash_in(dir.path(), "fuser -u held.txt");
    assert_eq!(user.stdout.trim(), me());
    assert!(
        user.stderr.contains('(') && user.stderr.contains(')'),
        "{}",
        user.stderr
    );

    // -a lists names nobody uses; -s prints nothing but keeps the status.
    std::fs::write(dir.join("free.txt"), "x").unwrap();
    let all = cash_in(dir.path(), "fuser -a held.txt free.txt");
    assert!(all.stderr.contains("/free.txt:"), "{}", all.stderr);
    let silent = cash_in(dir.path(), "fuser -s held.txt");
    assert_eq!(
        (silent.code, silent.stdout.as_str(), silent.stderr.as_str()),
        (0, "", "")
    );
}

#[test]
fn fuser_marks_an_executable_and_a_loaded_module() {
    let dir = fixture("exe");
    let exe = std::env::current_exe().unwrap();
    let out = cash_in(dir.path(), &format!("fuser '{}'", exe.display()));
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.split_whitespace().any(|p| p == me()),
        "{}",
        out.stdout
    );
    assert!(
        out.stderr.contains('e'),
        "executable access: {}",
        out.stderr
    );

    let root = std::env::var("SystemRoot").unwrap();
    let dll = Path::new(&root).join(r"System32\kernel32.dll");
    let dll_out = cash_in(dir.path(), &format!("fuser -v '{}'", dll.display()));
    assert!(
        dll_out.stderr.contains(&format!("{:>6} ....m ", me())),
        "{}",
        dll_out.stderr
    );
}

#[test]
fn fuser_on_a_directory_reports_holders_of_the_files_below_it() {
    let dir = fixture("dir");
    std::fs::create_dir_all(dir.join("sub/deeper")).unwrap();
    std::fs::write(dir.join("sub/deeper/inner.txt"), "x").unwrap();
    let _held = std::fs::File::open(dir.join("sub/deeper/inner.txt")).unwrap();
    let out = cash_in(dir.path(), "fuser sub");
    assert_eq!(
        (out.code, out.stdout.trim()),
        (0, me().as_str()),
        "{}",
        out.stderr
    );
}

#[test]
fn fuser_finds_tcp_and_udp_owners() {
    let dir = fixture("ports");
    let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp_port = tcp.local_addr().unwrap().port();
    let udp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let udp_port = udp.local_addr().unwrap().port();

    for script in [
        format!("fuser {tcp_port}/tcp"),
        format!("fuser -n tcp {tcp_port}"),
    ] {
        let out = cash_in(dir.path(), &script);
        assert_eq!(
            (out.code, out.stdout.trim()),
            (0, me().as_str()),
            "{script}: {}",
            out.stderr
        );
        assert!(
            out.stderr.starts_with(&format!("{tcp_port}/tcp:")),
            "{}",
            out.stderr
        );
    }
    let out = cash_in(dir.path(), &format!("fuser -v {udp_port}/udp"));
    assert!(
        out.stderr.contains(&format!("{:>6} F.... ", me())),
        "{}",
        out.stderr
    );

    // A port nobody uses: nothing found.
    drop(tcp);
    let gone = cash_in(dir.path(), &format!("fuser {tcp_port}/tcp"));
    assert_eq!(gone.code, 1);
}

#[test]
fn fuser_kill_ends_the_holder() {
    let dir = fixture("kill");
    std::fs::write(dir.join("held.txt"), "x").unwrap();
    // A child cash holds the file on descriptor 3 and spins; D26 keeps fd 3 from being
    // handed to an external `sleep`, so the wait stays inside the shell.
    let mut child = cash_command()
        .current_dir(dir.path())
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "exec 3<held.txt; while :; do :; done",
        ])
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let out = cash_in(dir.path(), "fuser held.txt");
        if out
            .stdout
            .split_whitespace()
            .any(|p| p == child.id().to_string())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the child never showed up: {}",
            out.stderr
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    let out = cash_in(dir.path(), "fuser -k held.txt");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let status = child.wait().unwrap();
    assert!(!status.success(), "the holder was not killed: {status:?}");
}

#[test]
fn fuser_refuses_what_windows_cannot_answer() {
    let dir = fixture("refuse");
    for (script, needle) in [
        ("fuser -m .", "mount points are not supported"),
        ("fuser -w x", "open for writing"),
        ("fuser", "No process specification given"),
        ("fuser -z x", "Invalid option z"),
    ] {
        let out = cash_in(dir.path(), script);
        assert_eq!(out.code, 1, "{script}");
        assert!(out.stderr.contains(needle), "{script}: {}", out.stderr);
    }
}

#[test]
fn lsof_lists_file_holders_with_linux_columns() {
    let dir = fixture("lsof-file");
    std::fs::write(dir.join("held.txt"), "hello").unwrap();
    std::fs::write(dir.join("free.txt"), "x").unwrap();
    let _held = std::fs::File::open(dir.join("held.txt")).unwrap();

    let out = cash_in(dir.path(), "lsof held.txt");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    let header: Vec<&str> = lines[0].split_whitespace().collect();
    assert_eq!(
        header,
        [
            "COMMAND", "PID", "USER", "FD", "TYPE", "DEVICE", "SIZE/OFF", "NODE", "NAME"
        ]
    );
    let row: Vec<&str> = lines[1].split_whitespace().collect();
    assert_eq!(row[1], me());
    assert_eq!(&row[3..], ["-", "REG", "-", "5", "-", "held.txt"]);

    assert_eq!(cash_in(dir.path(), "lsof -t held.txt").stdout.trim(), me());
    assert_eq!(cash_in(dir.path(), "lsof free.txt").code, 1);

    let below = cash_in(dir.path(), "lsof +D .");
    assert!(below.stdout.contains("./held.txt"), "{}", below.stdout);
}

#[test]
fn lsof_selects_sockets_by_address_state_and_process() {
    let dir = fixture("lsof-net");
    let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = tcp.local_addr().unwrap().port();

    let out = cash_in(dir.path(), &format!("lsof -nP -iTCP:{port} -sTCP:LISTEN"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout
            .contains(&format!("TCP 127.0.0.1:{port} (LISTEN)")),
        "{}",
        out.stdout
    );
    // By address and protocol: another program may hold the same port number over UDP
    // or on another address.
    let terse = cash_in(dir.path(), &format!("lsof -t -iTCP@127.0.0.1:{port}"));
    assert_eq!(terse.stdout.trim(), me());
    let named = cash_in(dir.path(), &format!("lsof -i :{port}"));
    assert!(
        named.stdout.contains(&format!("localhost:{port}")),
        "{}",
        named.stdout
    );

    // -a narrows -p to that process's sockets; OR without it.
    let anded = cash_in(dir.path(), &format!("lsof -a -p {} -i", me()));
    assert!(
        anded
            .stdout
            .lines()
            .skip(1)
            .all(|l| l.contains("TCP") || l.contains("UDP")),
        "{}",
        anded.stdout
    );
    assert!(
        anded.stdout.contains(&format!(":{port} (LISTEN)")),
        "{}",
        anded.stdout
    );

    // The host forms, with and without brackets, and a port range.
    for spec in [
        format!("tcp@127.0.0.1:{port}"),
        format!("@127.0.0.1:{port}"),
        format!("4tcp:{}-{}", port.saturating_sub(1), port.saturating_add(1)),
    ] {
        // A range can also catch a neighbouring port another process holds, so only
        // require this process to be among the results.
        let out = cash_in(dir.path(), &format!("lsof -t -i {spec}"));
        assert!(
            out.stdout.lines().any(|pid| pid == me()),
            "{spec}: {} {}",
            out.stdout,
            out.stderr
        );
    }
    let v6 = std::net::TcpListener::bind("[::1]:0").unwrap();
    let v6_port = v6.local_addr().unwrap().port();
    let bracketed = cash_in(dir.path(), &format!("lsof -t -i 6tcp@[::1]:{v6_port}"));
    assert_eq!(bracketed.stdout.trim(), me(), "{}", bracketed.stderr);

    // Not listening on that port: nothing.
    let other = cash_in(
        dir.path(),
        &format!("lsof -nP -iTCP:{port} -sTCP:ESTABLISHED"),
    );
    assert_eq!(other.code, 1);
}

#[test]
fn lsof_p_shows_executable_modules_and_open_files() {
    let dir = fixture("lsof-p");
    let held_path = dir.path().join("held-by-the-test.txt");
    std::fs::write(&held_path, "x").unwrap();
    let held = std::fs::OpenOptions::new()
        .append(true)
        .open(&held_path)
        .unwrap();
    let out = cash_in(dir.path(), &format!("lsof -p {}", me()));
    drop(held);
    assert_eq!(out.code, 0, "{}", out.stderr);
    // The handle walk names the file, its handle as the descriptor with `w`, and its size.
    let row = out
        .stdout
        .lines()
        .find(|l| l.contains("held-by-the-test.txt"))
        .unwrap_or_else(|| panic!("{}", out.stdout));
    let columns: Vec<&str> = row.split_whitespace().collect();
    assert!(
        columns[3].ends_with('w') && columns[3].trim_end_matches('w').parse::<u64>().is_ok(),
        "{row}"
    );
    assert_eq!((columns[4], columns[6]), ("REG", "1"), "{row}");
    let first_row = out.stdout.lines().nth(1).unwrap();
    assert!(first_row.contains(" txt "), "{}", out.stdout);
    assert!(
        out.stdout
            .lines()
            .any(|l| l.contains(" mem ") && l.to_ascii_lowercase().contains("kernel32.dll"))
    );
    // -t prints the process alone.
    let terse = cash_in(dir.path(), &format!("lsof -t -p {}", me()));
    assert_eq!(
        (terse.stdout.trim(), terse.stderr.as_str()),
        (me().as_str(), "")
    );
}

#[test]
fn lsof_refuses_what_windows_cannot_answer() {
    let dir = fixture("lsof-refuse");
    for (script, needle) in [
        ("lsof -U", "Unix domain sockets"),
        ("lsof -Z", "illegal option character: Z"),
        ("lsof -F p -i", "not supported"),
    ] {
        let out = cash_in(dir.path(), script);
        assert_eq!(out.code, 1, "{script}");
        assert!(out.stderr.contains(needle), "{script}: {}", out.stderr);
    }
}

#[test]
fn lsof_plus_d_is_one_level_and_plus_capital_d_all_below() {
    let dir = fixture("lsof-dirs");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("top.txt"), "t").unwrap();
    std::fs::write(dir.join("sub").join("deep.txt"), "d").unwrap();
    let _top = std::fs::File::open(dir.join("top.txt")).unwrap();
    let _deep = std::fs::File::open(dir.join("sub").join("deep.txt")).unwrap();

    let all = cash_in(dir.path(), "lsof +D .");
    assert!(all.stdout.contains("./top.txt"), "{}", all.stdout);
    assert!(all.stdout.contains("./sub/deep.txt"), "{}", all.stdout);
    let one = cash_in(dir.path(), "lsof +d .");
    assert!(one.stdout.contains("./top.txt"), "{}", one.stdout);
    assert!(!one.stdout.contains("deep.txt"), "{}", one.stdout);
}

#[test]
fn bare_lsof_lists_every_process_it_can_open() {
    let dir = fixture("lsof-all");
    let out = cash_in(dir.path(), "lsof -t");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let pids: Vec<&str> = out.stdout.lines().collect();
    assert!(pids.contains(&me().as_str()), "{}", out.stdout);
    assert!(pids.len() > 10, "{}", out.stdout);
}

#[test]
fn both_are_builtins() {
    let out = cash_in(&std::env::temp_dir(), "type fuser lsof");
    assert!(
        out.stdout.contains("fuser is a shell builtin"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("lsof is a shell builtin"),
        "{}",
        out.stdout
    );
}
