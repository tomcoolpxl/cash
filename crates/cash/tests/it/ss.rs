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

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::common::{Scratch, cash_command};

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

    let dir = Scratch::new("ss");
    let file = dir.join("filter");
    std::fs::write(&file, format!("sport = :{port}\n")).unwrap();
    let from_file = cash(&format!("ss -Hltn -F '{}'", file.display()));
    assert_eq!(from_file.stdout.lines().count(), 1, "{}", from_file.stdout);
    drop(dir);

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
        ("ss -tm", "socket memory"),
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

/// The lines after the header.
fn rows(out: &Output) -> Vec<&str> {
    out.stdout.lines().skip(1).collect()
}

/// A TIME-WAIT socket: the client end of a loopback connection the client closed
/// first. Returns the client's port once `ss -a` shows it.
fn time_wait() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    let port = client.local_addr().unwrap().port();
    drop(client);
    // The client's FIN arrives as end of file; closing this end then completes it.
    let mut byte = [0u8; 1];
    let _ = accepted.read(&mut byte);
    drop(accepted);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        let out = cash(&format!("ss -Htan state time-wait src 127.0.0.1:{port}"));
        if out.stdout.lines().count() == 1 {
            return port;
        }
    }
    panic!("no TIME-WAIT socket on port {port}");
}

#[test]
fn the_default_view_leaves_out_time_wait_and_connected_keeps_it() {
    let port = time_wait();
    let filter = format!("src 127.0.0.1:{port}");
    let default = cash(&format!("ss -tn {filter}"));
    assert_eq!(rows(&default), Vec::<&str>::new(), "{}", default.stdout);
    let all = cash(&format!("ss -tan {filter}"));
    assert!(all.stdout.contains("TIME-WAIT"), "{}", all.stdout);
    let connected = cash(&format!("ss -tn state connected {filter}"));
    assert!(
        connected.stdout.contains("TIME-WAIT"),
        "{}",
        connected.stdout
    );
}

#[test]
fn state_clauses_follow_iproute2() {
    let sockets = Sockets::open();
    let filter = sockets.filter();

    // `exclude` alone starts from every state, so the listener is there.
    let excluded = cash(&format!("ss -tn exclude established {filter}"));
    let tcp = format!("127.0.0.1:{} ", sockets.tcp_port());
    assert!(
        rows(&excluded)
            .iter()
            .any(|l| l.starts_with("LISTEN") && l.contains(&tcp)),
        "{}",
        excluded.stdout
    );
    // `state X exclude X` leaves nothing, so the tables' own default applies.
    let emptied = cash(&format!(
        "ss -tn state listening exclude listening {filter}"
    ));
    assert!(
        rows(&emptied).iter().all(|l| l.starts_with("ESTAB")),
        "{}",
        emptied.stdout
    );

    // `listening` is LISTEN alone: no UDP socket, and one state means no State column.
    let udp = cash(&format!("ss -uan state listening {filter}"));
    assert_eq!(rows(&udp), Vec::<&str>::new(), "{}", udp.stdout);
    let listening = cash(&format!("ss -tan state listening {filter}"));
    assert!(
        listening.stdout.starts_with("Recv-Q Send-Q"),
        "{}",
        listening.stdout
    );
    assert_eq!(rows(&listening).len(), 2, "{}", listening.stdout);

    for name in ["unconnected", "close", "syn-rcv", "bound-inactive"] {
        let out = cash(&format!("ss -tan state {name}"));
        assert_eq!(out.code, 0, "{name}: {}", out.stderr);
    }
    let listen = cash("ss -tan state listen");
    assert_eq!(
        (listen.code, listen.stderr.trim()),
        (255, "ss: wrong state name: listen")
    );
}

#[test]
fn listeners_come_first_in_each_family() {
    let first = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(first.local_addr().unwrap()).unwrap();
    let (_accepted, _) = first.accept().unwrap();
    let second = TcpListener::bind("127.0.0.1:0").unwrap();
    let ports = [
        first.local_addr().unwrap().port(),
        client.local_addr().unwrap().port(),
        second.local_addr().unwrap().port(),
    ];
    let filter = ports
        .iter()
        .map(|p| format!("src 127.0.0.1:{p}"))
        .collect::<Vec<_>>()
        .join(" or ");
    let out = cash(&format!("ss -Htan '( {filter} )'"));
    let states: Vec<&str> = out
        .stdout
        .lines()
        .map(|l| l.split_whitespace().next().unwrap_or(""))
        .collect();
    assert_eq!(
        states,
        ["LISTEN", "LISTEN", "ESTAB", "ESTAB"],
        "{}",
        out.stdout
    );
}

#[test]
fn filter_grammar_errors_and_peerless_sockets() {
    let sockets = Sockets::open();
    let port = sockets.tcp_port();
    // A listener's peer is 0.0.0.0:0.
    for filter in ["'dport = :0'", "'dport < :100'", "dst 0.0.0.0", "'dst *'"] {
        let out = cash(&format!("ss -Htln src 127.0.0.1:{port} {filter}"));
        assert_eq!(
            out.stdout.lines().count(),
            1,
            "{filter}: {} {}",
            out.stdout,
            out.stderr
        );
    }
    let equal = cash(&format!("ss -Htln src == 127.0.0.1:{port}"));
    assert_eq!(equal.stdout.lines().count(), 1, "{}", equal.stderr);

    for filter in [
        "'src != 127.0.0.1'",
        "'dst > 1.2.3.4'",
        "'( sport = :1'",
        "'sport = :1 or'",
        "0.0.0.0",
    ] {
        let out = cash(&format!("ss -tln {filter}"));
        assert_eq!(out.code, 255, "{filter}");
        assert!(
            out.stderr.starts_with(
                "ss: bison bellows (while parsing filter): \"syntax error!\" Sorry.\nUsage: ss"
            ),
            "{filter}: {}",
            out.stderr
        );
    }
    let bogus = cash("ss -tln bogus");
    assert_eq!(bogus.code, 1);
    assert!(
        bogus
            .stderr
            .starts_with("Error: an inet prefix is expected rather than \"bogus\"."),
        "{}",
        bogus.stderr
    );
}

#[test]
fn a_summary_with_a_selection_counts_everything_then_lists() {
    let sockets = Sockets::open();
    let filter = sockets.filter();
    let udp_row = |out: &Output| -> usize {
        let line = out
            .stdout
            .lines()
            .find(|l| l.starts_with("UDP\t"))
            .unwrap_or_else(|| panic!("{}", out.stdout));
        line.split_whitespace().nth(1).unwrap().parse().unwrap()
    };

    // -t selects what is listed, not what is counted: our UDP socket counts.
    let tcp = cash(&format!("ss -s -tn {filter}"));
    assert!(udp_row(&tcp) >= 1, "{}", tcp.stdout);
    let (summary, list) = tcp.stdout.split_once("\n\n").unwrap();
    assert!(summary.starts_with("Total: "), "{}", tcp.stdout);
    let list: Vec<&str> = list.trim_start_matches(|c| c != 'S').lines().collect();
    assert!(
        list.first().is_some_and(|l| l.starts_with("State ")),
        "{}",
        tcp.stdout
    );
    assert!(
        list.iter().filter(|l| l.starts_with("ESTAB")).count() == 2,
        "{}",
        tcp.stdout
    );

    // Without a selection (and with -a or -l, which select states only), the summary
    // is all.
    for script in ["ss -s", "ss -s -a", "ss -s -l"] {
        let out = cash(script);
        assert!(udp_row(&out) >= 1, "{script}: {}", out.stdout);
        assert!(!out.stdout.contains("Local Address"), "{script}");
    }
}

#[test]
fn options_parse_as_getopt_long_does() {
    let sockets = Sockets::open();
    let filter = sockets.filter();
    let long = cash(&format!("ss --num --listen --tcp {filter}"));
    let short = cash(&format!("ss -nlt {filter}"));
    assert_eq!(
        (long.code, &long.stdout),
        (0, &short.stdout),
        "{}",
        long.stderr
    );

    for (script, message) in [
        (
            "ss --n",
            "ss: option '--n' is ambiguous; possibilities: '--numeric' '--net' '--no-header' '--no-queues'",
        ),
        (
            "ss --listening=x",
            "ss: option '--listening' doesn't allow an argument",
        ),
        ("ss --family", "ss: option '--family' requires an argument"),
        ("ss -f", "ss: option requires an argument -- 'f'"),
        ("ss -A inet6", "ss: \"inet6\" is illegal socket table id"),
        ("ss -f bogus", "ss: \"bogus\" is invalid family"),
        ("ss -d", "ss: invalid option -- 'd'"),
    ] {
        let out = cash(script);
        assert_eq!(out.code, 255, "{script}");
        assert!(
            out.stderr.starts_with(&format!("{message}\nUsage: ss")),
            "{script}: {}",
            out.stderr
        );
    }

    // -A takes tables away with `!`; the Linux-only ones it leaves keep Netid.
    let query = cash(&format!("ss -Hln -A 'all,!udp' {filter}"));
    assert!(
        query.stdout.lines().count() == 2 && query.stdout.lines().all(|l| l.starts_with("tcp ")),
        "{}",
        query.stdout
    );
    let none = cash("ss -A '!udp'");
    assert_eq!(
        (none.code, none.stderr.trim()),
        (0, "ss: no socket tables to show with such filter.")
    );

    for option in ["--tos", "--cgroup", "--inet-sockopt", "--tipcinfo"] {
        let out = cash(&format!("ss {option}"));
        assert_eq!(out.code, 1, "{option}");
        assert!(
            out.stderr.starts_with(&format!("ss: {option}: ")),
            "{option}: {}",
            out.stderr
        );
    }

    let version = cash("ss -V");
    assert!(
        version.stdout.starts_with("ss utility, iproute2-7.2.0"),
        "{}",
        version.stdout
    );
    let missing = cash("ss -F /no/such/file");
    assert_eq!(
        (missing.code, missing.stderr.trim()),
        (255, "fopen filter file: No such file or directory")
    );
    let dir = Scratch::new("ss-twice");
    let file = dir.join("filter");
    std::fs::write(&file, "sport = :1\n").unwrap();
    let twice = cash(&format!("ss -F '{0}' -F '{0}'", file.display()));
    assert_eq!(
        (twice.code, twice.stderr.trim()),
        (255, "More than one filter file")
    );
    drop(dir);
}

#[test]
fn ipv6_scope_is_an_interface_name() {
    let out = cash("ss -uan6");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let numeric = regex::Regex::new(r"\]%\d+:").unwrap();
    assert!(!numeric.is_match(&out.stdout), "{}", out.stdout);
}

#[test]
fn resolve_names_hosts() {
    let sockets = Sockets::open();
    let port = sockets.tcp_port();
    let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let expected = cash_win32::net::host_names(&[loopback], Duration::from_secs(10))
        .remove(&loopback)
        .unwrap_or_else(|| "127.0.0.1".to_owned());
    let out = cash(&format!("ss -Htlnr src 127.0.0.1:{port}"));
    let line = out
        .stdout
        .lines()
        .next()
        .unwrap_or_else(|| panic!("{}", out.stderr));
    assert!(line.contains(&format!(" {expected}:{port} ")), "{line}");
    // Without -r the host stays numeric.
    let numeric = cash(&format!("ss -Htln src 127.0.0.1:{port}"));
    assert!(numeric.stdout.contains(&format!(" 127.0.0.1:{port} ")));
}

#[test]
fn bound_inactive_sockets_are_listed() {
    // The standard library cannot bind without listening or connecting; PowerShell can.
    // It holds the socket until its input ends, which dropping `child` brings.
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$s = [Net.Sockets.Socket]::new('InterNetwork', 'Stream', 'Tcp'); \
             $s.Bind([Net.IPEndPoint]::new([Net.IPAddress]::Loopback, 0)); \
             [Console]::Out.WriteLine($s.LocalEndPoint.Port); [Console]::Out.Flush(); \
             [void][Console]::In.ReadLine(); $s.Close()",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let filter = format!("src 127.0.0.1:{port}");

    // -B alone: one state, so no State column.
    let bound = cash(&format!("ss -tnB {filter}"));
    assert!(
        bound.stdout.starts_with("Recv-Q Send-Q"),
        "{}",
        bound.stdout
    );
    assert_eq!(
        rows(&bound),
        [format!("0      0          127.0.0.1:{port}      0.0.0.0:*").as_str()],
        "{}",
        bound.stdout
    );
    let all = cash(&format!("ss -Htan {filter}"));
    assert!(all.stdout.starts_with("UNCONN"), "{}", all.stdout);
    let listening = cash(&format!("ss -Htln {filter}"));
    assert_eq!(listening.stdout, "", "{}", listening.stdout);

    child.stdin.take().unwrap().write_all(b"\n").unwrap();
    child.wait().unwrap();
}

#[test]
fn kill_refuses_unelevated_and_closes_only_ipv4_elevated() {
    let elevated = cash_win32::process::current_process_is_elevated() == Some(true);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = listener.local_addr().unwrap();
    let mut client = TcpStream::connect(server).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    let filter = format!(
        "src 127.0.0.1:{} dst 127.0.0.1:{}",
        client.local_addr().unwrap().port(),
        server.port()
    );
    let out = cash(&format!("ss -tnK {filter}"));
    if !elevated {
        assert_eq!(out.code, 1);
        assert!(
            out.stderr
                .starts_with("ss: -K: Windows lets only an elevated shell close connections"),
            "{}",
            out.stderr
        );
        assert_eq!(out.stdout, "");
        // Nothing was closed.
        client.write_all(b"x").unwrap();
        let mut byte = [0u8; 1];
        accepted.read_exact(&mut byte).unwrap();
        return;
    }
    // Elevated: the test's own connection, and nothing else, is closed and listed.
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(rows(&out).len(), 1, "{}", out.stdout);
    let mut byte = [0u8; 1];
    assert!(client.read(&mut byte).is_err() || accepted.read(&mut byte).is_err());

    let v6 = TcpListener::bind("[::1]:0").unwrap();
    let v6_client = TcpStream::connect(v6.local_addr().unwrap()).unwrap();
    let (_v6_accepted, _) = v6.accept().unwrap();
    let port = v6_client.local_addr().unwrap().port();
    let refused = cash(&format!("ss -tnK src [::1]:{port}"));
    assert_eq!(refused.code, 1);
    assert!(
        refused
            .stderr
            .contains("Windows closes IPv4 connections only"),
        "{}",
        refused.stderr
    );
    assert_eq!(rows(&refused), Vec::<&str>::new());
}

#[test]
fn dev_filters_by_scope_and_ipv4_has_no_device() {
    let sockets = Sockets::open();
    let port = sockets.tcp_port();
    let count = |filter: &str| {
        let out = cash(&format!("ss -Htln src 127.0.0.1:{port} {filter}"));
        assert_eq!(out.code, 0, "{filter}: {}", out.stderr);
        out.stdout.lines().count()
    };
    // An IPv4 socket is bound to no device, like a Linux socket without one.
    assert_eq!(count("dev 0"), 1);
    assert_eq!(count("'dev = 1'"), 0);
    assert_eq!(count("'dev != 1'"), 1);
    assert_eq!(count("'not dev 1'"), 1);
    assert_eq!(count(&format!("'( dev 1 or sport = :{port} )'")), 1);

    let unknown = cash("ss -tln dev no-such-device");
    assert_eq!(
        (unknown.code, unknown.stderr.trim()),
        (1, "Cannot parse device.")
    );
    for filter in ["dev", "'dev < 1'"] {
        let out = cash(&format!("ss -tln {filter}"));
        assert_eq!(out.code, 255, "{filter}");
    }

    // A link-local socket, where the machine has one, is on its scope's device.
    let all = cash("ss -Huan6");
    let scoped = regex::Regex::new(r"\]%([^ :]+):").unwrap();
    if let Some(name) = scoped
        .captures(&all.stdout)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
    {
        let on = cash(&format!("ss -Huan6 dev {name}"));
        assert!(
            !on.stdout.is_empty() && on.stdout.lines().all(|l| l.contains(&format!("]%{name}:"))),
            "{name}: {}",
            on.stdout
        );
        let off = cash(&format!("ss -Huan6 dev != {name}"));
        assert!(
            !off.stdout.contains(&format!("]%{name}:")),
            "{}",
            off.stdout
        );
    }
}

#[test]
fn info_shows_what_windows_keeps_and_says_what_it_does_not() {
    let elevated = cash_win32::process::current_process_is_elevated() == Some(true);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = listener.local_addr().unwrap();
    let mut client = TcpStream::connect(server).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    client.write_all(&vec![b'x'; 100_000]).unwrap();
    let mut read = vec![0u8; 100_000];
    accepted.read_exact(&mut read).unwrap();
    let filter = format!(
        "src 127.0.0.1:{} dst 127.0.0.1:{}",
        client.local_addr().unwrap().port(),
        server.port()
    );

    let out = cash(&format!("ss -tni {filter}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{}", out.stdout);
    assert!(lines[1].starts_with("ESTAB"), "{}", out.stdout);
    let info = lines[2];
    if !elevated {
        // A new connection: only the SYN's MSS values, nothing uncollected.
        let syn_only = regex::Regex::new(r"^\t mss:\d+ advmss:\d+$").unwrap();
        assert!(syn_only.is_match(info), "{info:?}");
        assert!(
            out.stderr
                .contains("ss: -i: Windows collects the other statistics only once an elevated shell switches them on"),
            "{}",
            out.stderr
        );
    } else {
        assert!(
            out.stderr
                .contains("statistics collection switched on for 1 connection;"),
            "{}",
            out.stderr
        );
        client.write_all(&[b'y'; 5000]).unwrap();
        accepted.read_exact(&mut read[..5000]).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let again = cash(&format!("ss -tni {filter}"));
        let info = again.stdout.lines().nth(2).unwrap_or_default();
        assert!(
            info.contains(" bytes_sent:") && info.contains(" mss:"),
            "{}",
            again.stdout
        );
        assert!(!again.stderr.contains("switched on"), "{}", again.stderr);
    }

    // -O keeps it on the socket's line; a listener has nothing to add.
    let oneline = cash(&format!("ss -tniO {filter}"));
    assert_eq!(oneline.stdout.lines().count(), 2, "{}", oneline.stdout);
    assert!(oneline.stdout.contains(" advmss:"), "{}", oneline.stdout);
    let listening = cash(&format!("ss -tlni src 127.0.0.1:{}", server.port()));
    assert_eq!(listening.stdout.lines().count(), 2, "{}", listening.stdout);
    assert_eq!(listening.stderr, "");
}
