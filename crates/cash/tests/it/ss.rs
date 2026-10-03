//! `ss` — ROADMAP item 9, research/ss-evaluation.md.
//!
//! The test process owns the sockets being listed, so the expected pid is exact. Layout
//! expectations come from iproute2 7.2 run under WSL: header spelling and the
//! `Port`/`Process` join, `Netid` only for several protocols, `State` only when more
//! than one state is shown, Recv-Q/Send-Q as `0` (Windows does not expose them), and exit
//! status 0 when nothing matches.

#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::net::{TcpListener, TcpStream, UdpSocket};
use std::process::Stdio;

use crate::common::cash_command;

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash(script: &str) -> Output {
    let out = cash_command()
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

/// The sockets one test works with, all owned by this process.
struct Sockets {
    tcp: TcpListener,
    udp: UdpSocket,
    v6: TcpListener,
    _client: TcpStream,
    _accepted: TcpStream,
}

impl Sockets {
    fn open() -> Self {
        let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(tcp.local_addr().unwrap()).unwrap();
        let (accepted, _) = tcp.accept().unwrap();
        Self {
            udp: UdpSocket::bind("127.0.0.1:0").unwrap(),
            v6: TcpListener::bind("[::1]:0").unwrap(),
            tcp,
            _client: client,
            _accepted: accepted,
        }
    }

    fn tcp_port(&self) -> u16 {
        self.tcp.local_addr().unwrap().port()
    }

    fn udp_port(&self) -> u16 {
        self.udp.local_addr().unwrap().port()
    }

    fn v6_port(&self) -> u16 {
        self.v6.local_addr().unwrap().port()
    }

    /// A filter selecting just these sockets: by loopback address as well as port,
    /// since another program can hold the same port number over the other protocol or
    /// on the wildcard address.
    fn filter(&self) -> String {
        format!(
            "'( src 127.0.0.1:{t} or src 127.0.0.1:{u} or src [::1]:{v} or dst 127.0.0.1:{t} )'",
            t = self.tcp_port(),
            u = self.udp_port(),
            v = self.v6_port()
        )
    }
}

fn me() -> String {
    std::process::id().to_string()
}

#[test]
fn tulpn_lists_listening_tcp_and_udp_with_owners() {
    let sockets = Sockets::open();
    let out = cash(&format!("ss -tulpn {}", sockets.filter()));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(
        lines[0].starts_with("Netid State  Recv-Q Send-Q ")
            && lines[0].ends_with("Peer Address:PortProcess"),
        "{}",
        lines[0]
    );
    let users = format!("pid={},fd=-))", me());
    let udp = format!("127.0.0.1:{}", sockets.udp_port());
    let tcp = format!("127.0.0.1:{}", sockets.tcp_port());
    let v6 = format!("[::1]:{}", sockets.v6_port());
    // UDP first, then TCP; -l hides the established pair.
    assert!(
        lines[1].starts_with("udp   UNCONN 0      0") && lines[1].contains(&udp),
        "{}",
        out.stdout
    );
    assert!(
        lines[2].starts_with("tcp   LISTEN") && lines[2].contains(&tcp),
        "{}",
        out.stdout
    );
    assert!(
        lines[3].contains(&v6) && lines[3].contains("[::]:*"),
        "{}",
        out.stdout
    );
    assert_eq!(lines.len(), 4, "{}", out.stdout);
    assert!(
        lines[1..].iter().all(|l| l.contains(&users)),
        "{}",
        out.stdout
    );
}

#[test]
fn state_and_netid_columns_follow_the_selection() {
    let sockets = Sockets::open();
    let filter = sockets.filter();

    // One protocol: no Netid column. -a adds the established pair.
    let all = cash(&format!("ss -tan {filter}"));
    let header = all.stdout.lines().next().unwrap();
    assert_eq!(
        header,
        "State  Recv-Q Send-Q Local Address:Port  Peer Address:Port"
    );
    assert_eq!(
        all.stdout
            .lines()
            .filter(|l| l.starts_with("ESTAB "))
            .count(),
        2,
        "{}",
        all.stdout
    );

    // A single state: no State column.
    let est = cash(&format!("ss -tn state established {filter}"));
    assert!(
        est.stdout
            .lines()
            .next()
            .unwrap()
            .starts_with("Recv-Q Send-Q"),
        "{}",
        est.stdout
    );
    assert_eq!(est.stdout.lines().count(), 3, "{}", est.stdout);

    // Without -a or -l, TCP shows connected sockets only.
    let connected = cash(&format!("ss -tn {filter}"));
    assert!(
        connected
            .stdout
            .lines()
            .skip(1)
            .all(|l| l.starts_with("ESTAB")),
        "{}",
        connected.stdout
    );
}

#[test]
fn no_header_families_and_no_queues() {
    let sockets = Sockets::open();
    let filter = sockets.filter();
    let bare = cash(&format!("ss -H -tln {filter}"));
    assert!(
        bare.stdout
            .lines()
            .all(|l| l.starts_with("LISTEN 0      0 ")),
        "{}",
        bare.stdout
    );

    let v4 = cash(&format!("ss -tln4 {filter}"));
    assert!(!v4.stdout.contains("[::1]"), "{}", v4.stdout);
    let v6 = cash(&format!("ss -tln -6 {filter}"));
    assert!(
        v6.stdout.contains("[::1]") && !v6.stdout.contains("127.0.0.1"),
        "{}",
        v6.stdout
    );
    let family = cash(&format!("ss -tln -f inet6 {filter}"));
    assert_eq!(family.stdout, v6.stdout);

    let no_queues = cash(&format!("ss -tlnQ {filter}"));
    assert!(!no_queues.stdout.contains("Recv-Q"), "{}", no_queues.stdout);
}

#[test]
fn filters_by_port_address_and_file() {
    let sockets = Sockets::open();
    let port = sockets.tcp_port();
    for filter in [
        format!("sport = :{port}"),
        format!("'( sport == :{port} )'"),
        format!("src 127.0.0.1:{port}"),
        format!("src 127.0.0.0/8 sport eq :{port}"),
    ] {
        let out = cash(&format!("ss -Hltn {filter}"));
        assert_eq!(
            out.stdout.lines().count(),
            1,
            "{filter}: {} {}",
            out.stdout,
            out.stderr
        );
    }
    let gt = cash(&format!("ss -Htn 'dport >= :{port} and dport <= :{port}'"));
    assert_eq!(gt.stdout.lines().count(), 1, "{}", gt.stdout);

    let dir = std::env::temp_dir().join(format!("cash-ss-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("filter");
    std::fs::write(&file, format!("sport = :{port}\n")).unwrap();
    let from_file = cash(&format!("ss -Hltn -F '{}'", file.display()));
    assert_eq!(from_file.stdout.lines().count(), 1, "{}", from_file.stdout);
    let _ = std::fs::remove_dir_all(dir);

    // Nothing matches: the header alone and status 0, as in iproute2.
    let none = cash("ss -tn sport = :1");
    assert_eq!((none.code, none.stdout.lines().count()), (0, 1));
}

#[test]
fn port_wait_idiom_works() {
    let sockets = Sockets::open();
    let out = cash(&format!(
        "ss -ltn | grep -q ':{} ' && echo up",
        sockets.tcp_port()
    ));
    assert_eq!(out.stdout.trim(), "up", "{}", out.stderr);
}

#[test]
fn summary_has_iproute2_layout() {
    let _sockets = Sockets::open();
    let out = cash("ss -s");
    assert_eq!(out.code, 0);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines[0].starts_with("Total: "), "{}", out.stdout);
    assert!(
        lines[1].starts_with("TCP:   ") && lines[1].contains("(estab "),
        "{}",
        out.stdout
    );
    assert_eq!(lines[3], "Transport Total     IP        IPv6");
    for (line, name) in lines[4..9]
        .iter()
        .zip(["RAW", "UDP", "TCP", "INET", "FRAG"])
    {
        assert!(line.starts_with(&format!("{name}\t  ")), "{line:?}");
    }
}

#[test]
fn errors_refusals_and_the_netstat_hint() {
    let unknown = cash("ss -y");
    assert_eq!(unknown.code, 255);
    assert!(
        unknown
            .stderr
            .starts_with("ss: invalid option -- 'y'\nUsage: ss"),
        "{}",
        unknown.stderr
    );
    let long = cash("ss --bogus");
    assert_eq!(long.code, 255);
    assert!(
        long.stderr.contains("unrecognized option '--bogus'"),
        "{}",
        long.stderr
    );
    let state = cash("ss -tn state bogus");
    assert_eq!(
        (state.code, state.stderr.trim()),
        (255, "ss: wrong state name: bogus")
    );
    let port = cash("ss -tn 'sport = :abc'");
    assert_eq!(port.code, 1);
    assert!(
        port.stderr.contains("\"abc\" does not look like a port."),
        "{}",
        port.stderr
    );

    let netstat = cash("ss -ano");
    assert_eq!(netstat.code, 1);
    assert!(
        netstat
            .stderr
            .contains("-o: Windows does not expose socket timers"),
        "{}",
        netstat.stderr
    );
    assert!(
        netstat.stderr.contains("the ss spelling is `ss -tuanp`"),
        "{}",
        netstat.stderr
    );
    for (script, needle) in [
        ("ss -x", "Unix domain sockets"),
        ("ss -ti", "TCP internals"),
        ("ss -f unix", "only inet and inet6"),
    ] {
        let out = cash(script);
        assert_eq!(out.code, 1, "{script}");
        assert!(out.stderr.contains(needle), "{script}: {}", out.stderr);
    }
}

#[test]
fn ss_is_a_builtin() {
    let out = cash("type ss");
    assert!(
        out.stdout.contains("ss is a shell builtin"),
        "{}",
        out.stdout
    );
}
