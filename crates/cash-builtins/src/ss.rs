//! `ss`: iproute2's socket statistics, on IP Helper's socket tables.
//!
//! ROADMAP item 9 and research/ss-evaluation.md. Output and option handling follow
//! iproute2 7.2 (checked in WSL and against its `misc/ss.c`): the same columns and
//! widths, `Netid` only when several socket tables are selected, `State` only when the
//! state filter allows more than one state, ports as service names unless `-n`, hosts
//! numeric unless `-r`, `getopt_long`'s option spellings and messages, and exit status 0
//! even when nothing matches.
//!
//! Windows differences, decided in the evaluation:
//!
//! * Recv-Q and Send-Q are not exposed and print as `0`;
//! * `-p` prints `fd=-` (Windows has no descriptor numbers) and, for services hosted
//!   in `svchost.exe`, `service=NAME` from the socket's owning module;
//! * UDP sockets carry no peer, so they are always `UNCONN`;
//! * `-K` closes IPv4 TCP connections only, and only from an elevated shell;
//! * `-B` reads an undocumented table (netstat's `BOUND`);
//! * options with no Windows backing (`-x`, `-e`, `-m`, `-o`, `-i`, other socket
//!   families) are refused by name, and netstat-style flags get a hint.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};
use cash_win32::net::{self, Proto, Socket, TcpState};
use clap::Parser;

use crate::fileuse::{ProcessNames, Services};

const USAGE: &str = "Usage: ss [ OPTIONS ]\n       ss [ OPTIONS ] [ FILTER ]\n   -h, --help          this message\n   -V, --version       output version information\n   -n, --numeric       don't resolve service names\n   -r, --resolve       resolve host names\n   -a, --all           display all sockets\n   -l, --listening     display listening sockets\n   -B, --bound-inactive display TCP bound but inactive sockets\n   -p, --processes     show process using socket\n   -s, --summary       show socket usage summary\n\n   -4, --ipv4          display only IP version 4 sockets\n   -6, --ipv6          display only IP version 6 sockets\n   -t, --tcp           display only TCP sockets\n   -u, --udp           display only UDP sockets\n   -f, --family=FAMILY display sockets of type FAMILY\n       FAMILY := {inet|inet6|help}\n\n   -K, --kill          forcibly close sockets, display what was closed\n   -H, --no-header     Suppress header line\n   -Q, --no-queues     Suppress sending and receiving queue columns\n   -O, --oneline       socket's data printed on a single line\n\n   -A, --query=QUERY, --socket=QUERY\n       QUERY := {all|inet|tcp|udp}[,QUERY]\n\n   -F, --filter=FILE   read filter information from FILE\n       FILTER := [ state STATE-FILTER ] [ EXPRESSION ]\n\ncash's ss is the Windows subset described in ROADMAP item 9.";

/// The iproute2 release whose `ss` this one follows.
const IPROUTE2_VERSION: &str = "7.2.0";

/// How long `-r` waits for reverse lookups, all of them together.
const RESOLVE_LIMIT: Duration = Duration::from_secs(2);

/// Investigate sockets.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct SsCommand {
    /// Options and the filter, parsed here: iproute2's grammar mixes combined short
    /// flags, long options with `=` and a free-form filter expression.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// The states a socket can be in, numbered as iproute2 numbers them, so that a state
/// filter has the same bits, and so decides the State column the same way. UDP sockets
/// are `Close` (shown `UNCONN`), as in Linux.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
enum State {
    Established = 1,
    SynSent = 2,
    SynRecv = 3,
    FinWait1 = 4,
    FinWait2 = 5,
    TimeWait = 6,
    Close = 7,
    CloseWait = 8,
    LastAck = 9,
    Listen = 10,
    Closing = 11,
    /// Bound, neither listening nor connected (`-B`). Linux reports these sockets as
    /// closed, so they print as `UNCONN`.
    BoundInactive = 13,
}

impl State {
    const fn bit(self) -> u16 {
        1 << (self as u16)
    }

    const fn of(socket: &Socket) -> Self {
        match socket.state {
            None => Self::Close,
            Some(state) => match state {
                TcpState::Established => Self::Established,
                TcpState::SynSent => Self::SynSent,
                TcpState::SynReceived => Self::SynRecv,
                TcpState::FinWait1 => Self::FinWait1,
                TcpState::FinWait2 => Self::FinWait2,
                TcpState::TimeWait => Self::TimeWait,
                TcpState::Closed => Self::Close,
                TcpState::CloseWait => Self::CloseWait,
                TcpState::LastAck => Self::LastAck,
                TcpState::Listen => Self::Listen,
                TcpState::Closing => Self::Closing,
            },
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Established => "ESTAB",
            Self::SynSent => "SYN-SENT",
            Self::SynRecv => "SYN-RECV",
            Self::FinWait1 => "FIN-WAIT-1",
            Self::FinWait2 => "FIN-WAIT-2",
            Self::TimeWait => "TIME-WAIT",
            Self::Close | Self::BoundInactive => "UNCONN",
            Self::CloseWait => "CLOSE-WAIT",
            Self::LastAck => "LAST-ACK",
            Self::Listen => "LISTEN",
            Self::Closing => "CLOSING",
        }
    }

    /// Where the socket comes in its family's listing: iproute2 shows what the kernel
    /// dumps, listeners first, then bound-inactive sockets, then the rest.
    const fn rank(self) -> u8 {
        match self {
            Self::Listen => 0,
            Self::BoundInactive => 1,
            _ => 2,
        }
    }
}

const fn mask(states: &[State]) -> u16 {
    let mut bits = 0;
    let mut i = 0;
    while i < states.len() {
        bits |= states[i].bit();
        i += 1;
    }
    bits
}

/// iproute2's `SS_ALL`: every state bit below `SS_MAX` (14), unknown and kernel-only
/// ones included.
const ALL: u16 = (1 << 14) - 1;
/// `SS_CONN`, the default view: no listeners, closed, TIME-WAIT or SYN-RECV sockets.
const CONN: u16 = ALL & !mask(&[State::Listen, State::Close, State::TimeWait, State::SynRecv]);
/// The `connected` group, which unlike the default keeps TIME-WAIT and SYN-RECV.
const CONNECTED: u16 = ALL & !mask(&[State::Close, State::Listen]);
const SYNCHRONIZED: u16 = CONNECTED & !State::SynSent.bit();
const BUCKET: u16 = mask(&[State::SynRecv, State::TimeWait]);
const BIG: u16 = ALL & !BUCKET;
/// What `-l` shows: listeners and unconnected sockets (UDP).
const LISTENING: u16 = mask(&[State::Listen, State::Close]);

/// A state name or group in a `state`/`exclude` clause, as iproute2's `scan_state`
/// takes them (without case).
fn state_bits(name: &str) -> Option<u16> {
    Some(match name.to_ascii_lowercase().as_str() {
        "close" | "closed" | "unconnected" => State::Close.bit(),
        "syn-rcv" | "syn-recv" => State::SynRecv.bit(),
        "established" => State::Established.bit(),
        "all" => ALL,
        "connected" => CONNECTED,
        "synchronized" => SYNCHRONIZED,
        "bucket" => BUCKET,
        "big" => BIG,
        "unknown" => 1,
        "syn-sent" => State::SynSent.bit(),
        "fin-wait-1" => State::FinWait1.bit(),
        "fin-wait-2" => State::FinWait2.bit(),
        "time-wait" => State::TimeWait.bit(),
        "close-wait" => State::CloseWait.bit(),
        "last-ack" => State::LastAck.bit(),
        "listening" => State::Listen.bit(),
        "closing" => State::Closing.bit(),
        "bound-inactive" => State::BoundInactive.bit(),
        // `new-syn-recv` is a kernel detail iproute2 refuses; `listen` is no name.
        _ => return None,
    })
}

// iproute2's socket tables. Only TCP and UDP exist on Windows; the others are kept so
// that `-A` and the Netid column work as in iproute2.
const DB_UDP: u32 = 1;
const DB_TCP: u32 = 1 << 1;
const DB_MPTCP: u32 = 1 << 2;
const DB_RAW: u32 = 1 << 3;
const DB_UNIX_STREAM: u32 = 1 << 4;
const DB_UNIX_DGRAM: u32 = 1 << 5;
const DB_UNIX_SEQPACKET: u32 = 1 << 6;
const DB_PACKET_RAW: u32 = 1 << 7;
const DB_PACKET_DGRAM: u32 = 1 << 8;
const DB_NETLINK: u32 = 1 << 9;
const DB_SCTP: u32 = 1 << 10;
const DB_VSOCK_STREAM: u32 = 1 << 11;
const DB_VSOCK_DGRAM: u32 = 1 << 12;
const DB_TIPC: u32 = 1 << 13;
const DB_XDP: u32 = 1 << 14;
/// `-A all`, which leaves out TIPC as iproute2's does.
const DB_ALL: u32 = ((1 << 15) - 1) & !DB_TIPC;
/// The tables of the inet families, what `-4`/`-6` alone select.
const DB_INET: u32 = DB_UDP | DB_TCP | DB_MPTCP | DB_SCTP | DB_RAW;

/// A `-A` table name's tables.
fn table_bits(name: &str) -> Option<u32> {
    Some(match name {
        "all" => DB_ALL,
        "inet" => DB_INET,
        "udp" => DB_UDP,
        "tcp" => DB_TCP,
        "mptcp" => DB_MPTCP,
        "sctp" => DB_SCTP,
        "raw" => DB_RAW,
        "unix" => DB_UNIX_STREAM | DB_UNIX_DGRAM | DB_UNIX_SEQPACKET,
        "unix_stream" | "u_str" => DB_UNIX_STREAM,
        "unix_dgram" | "u_dgr" => DB_UNIX_DGRAM,
        "unix_seqpacket" | "u_seq" => DB_UNIX_SEQPACKET,
        "packet" => DB_PACKET_RAW | DB_PACKET_DGRAM,
        "packet_raw" | "p_raw" => DB_PACKET_RAW,
        "packet_dgram" | "p_dgr" => DB_PACKET_DGRAM,
        "netlink" => DB_NETLINK,
        "tipc" => DB_TIPC,
        "vsock" => DB_VSOCK_STREAM | DB_VSOCK_DGRAM,
        "vsock_stream" | "v_str" => DB_VSOCK_STREAM,
        "vsock_dgram" | "v_dgr" => DB_VSOCK_DGRAM,
        "xdp" => DB_XDP,
        _ => return None,
    })
}

/// The states a table shows when nothing else is said (iproute2's `default_dbs`).
const fn table_default_states(table: u32) -> u16 {
    match table {
        DB_UDP | DB_RAW => State::Established.bit(),
        DB_UNIX_DGRAM | DB_PACKET_RAW | DB_PACKET_DGRAM | DB_NETLINK | DB_XDP => State::Close.bit(),
        _ => CONN,
    }
}

const FAMILY_V4: u8 = 1;
const FAMILY_V6: u8 = 2;

/// A comparison in a filter expression.
#[derive(Clone, Copy)]
enum Compare {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

impl Compare {
    fn parse(token: &str) -> Option<Self> {
        Some(match token {
            "=" | "==" | "eq" => Self::Eq,
            "!=" | "ne" | "neq" => Self::Ne,
            "<" | "lt" => Self::Lt,
            ">" | "gt" => Self::Gt,
            "<=" | "le" | "leq" => Self::Le,
            ">=" | "ge" | "geq" => Self::Ge,
            _ => return None,
        })
    }

    const fn holds(self, left: u16, right: u16) -> bool {
        match self {
            Self::Eq => left == right,
            Self::Ne => left != right,
            Self::Lt => left < right,
            Self::Gt => left > right,
            Self::Le => left <= right,
            Self::Ge => left >= right,
        }
    }
}

/// An address condition: `src`/`dst ADDR[/PREFIX][:PORT]`.
struct AddrMatch {
    hosts: Option<Vec<IpAddr>>,
    prefix: Option<u8>,
    port: Option<u16>,
}

impl AddrMatch {
    fn matches(&self, addr: SocketAddr) -> bool {
        self.port.is_none_or(|p| addr.port() == p)
            && self
                .hosts
                .as_ref()
                .is_none_or(|hosts| hosts.iter().any(|h| in_prefix(*h, addr.ip(), self.prefix)))
    }
}

fn in_prefix(network: IpAddr, ip: IpAddr, prefix: Option<u8>) -> bool {
    match (network, ip) {
        (IpAddr::V4(n), IpAddr::V4(i)) => {
            let bits = u32::from(prefix.unwrap_or(32).min(32));
            let m = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            n.to_bits() & m == i.to_bits() & m
        }
        (IpAddr::V6(n), IpAddr::V6(i)) => {
            let bits = u32::from(prefix.unwrap_or(128).min(128));
            let m = if bits == 0 {
                0
            } else {
                u128::MAX << (128 - bits)
            };
            n.to_bits() & m == i.to_bits() & m
        }
        _ => false,
    }
}

/// The peer a filter sees: a socket without one (a listener, a UDP or bound socket) has
/// the unspecified address and port 0, as the kernel reports to iproute2.
fn peer_of(socket: &Socket) -> SocketAddr {
    socket.remote.unwrap_or_else(|| {
        let ip = if socket.local.is_ipv6() {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        };
        SocketAddr::new(ip, 0)
    })
}

/// A filter expression.
enum Expr {
    Sport(Compare, u16),
    Dport(Compare, u16),
    Src(AddrMatch),
    Dst(AddrMatch),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

impl Expr {
    fn matches(&self, socket: &Socket) -> bool {
        match self {
            Self::Sport(op, port) => op.holds(socket.local.port(), *port),
            Self::Dport(op, port) => op.holds(peer_of(socket).port(), *port),
            Self::Src(addr) => addr.matches(socket.local),
            Self::Dst(addr) => addr.matches(peer_of(socket)),
            Self::Not(inner) => !inner.matches(socket),
            Self::And(a, b) => a.matches(socket) && b.matches(socket),
            Self::Or(a, b) => a.matches(socket) || b.matches(socket),
        }
    }
}

/// Why a filter failed to parse.
enum FilterError {
    /// The grammar does not allow it: iproute2's bison message and the usage, status
    /// 255.
    Syntax,
    /// A message of iproute2's own wording, status 1.
    Message(String),
}

/// iproute2's message for a filter its grammar rejects.
const SYNTAX_ERROR: &str = "ss: bison bellows (while parsing filter): \"syntax error!\" Sorry.";

/// Splits `text` into a host and a port: `[v6]:port`, `v4:port`, `*:port`, `:port`.
fn split_port(text: &str) -> Option<(&str, Option<&str>)> {
    if let Some(rest) = text.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        Some((host, after.strip_prefix(':')))
    } else if text.matches(':').count() == 1 {
        let (host, port) = text.split_once(':')?;
        Some((host, Some(port)))
    } else {
        Some((text, None))
    }
}

/// Whether a bare word reads as a numeric address condition, which iproute2's lexer
/// takes as one and its grammar then rejects on its own.
fn looks_like_address(text: &str) -> bool {
    let Some((host, port)) = split_port(text) else {
        return false;
    };
    let port_ok = port.is_none_or(|p| p == "*" || p.parse::<u16>().is_ok());
    let host = host.split_once('/').map_or(host, |(host, _)| host);
    port_ok && (host.is_empty() || host == "*" || host.parse::<IpAddr>().is_ok())
}

/// Parses the free-form filter expression.
struct FilterParser<'a> {
    tokens: Vec<String>,
    position: usize,
    services: &'a Services,
    protos: &'a [Proto],
}

impl FilterParser<'_> {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.position).map(String::as_str)
    }

    fn next(&mut self) -> Option<String> {
        let token = self.tokens.get(self.position).cloned();
        self.position += 1;
        token
    }

    fn port(&self, text: &str) -> Result<u16, FilterError> {
        let bare = text.strip_prefix(':').unwrap_or(text);
        self.protos
            .iter()
            .find_map(|p| self.services.port(bare, p.name()))
            .or_else(|| self.services.port(bare, "tcp"))
            .ok_or_else(|| {
                FilterError::Message(format!(
                    "Error: \"{bare}\" does not look like a port.\nCannot parse dst/src address."
                ))
            })
    }

    fn address(&self, text: &str) -> Result<AddrMatch, FilterError> {
        let bad = || {
            FilterError::Message(format!(
                "Error: an inet prefix is expected rather than \"{text}\".\nCannot parse dst/src address."
            ))
        };
        let (host, port) = split_port(text).ok_or_else(bad)?;
        let port = match port {
            Some("*") | None => None,
            Some(port) => Some(self.port(port)?),
        };
        let (host, prefix) = match host.split_once('/') {
            Some((host, bits)) => (host, Some(bits.parse::<u8>().map_err(|_| bad())?)),
            None => (host, None),
        };
        let hosts = if host.is_empty() || host == "*" {
            None
        } else {
            let resolved: Vec<IpAddr> = (host, 0)
                .to_socket_addrs()
                .map_err(|_| bad())?
                .map(|a| a.ip())
                .collect();
            Some(resolved)
        };
        Ok(AddrMatch {
            hosts,
            prefix,
            port,
        })
    }

    fn primary(&mut self) -> Result<Expr, FilterError> {
        let token = self.next().ok_or(FilterError::Syntax)?;
        match token.as_str() {
            "(" => {
                let inner = self.or()?;
                if self.next().as_deref() != Some(")") {
                    return Err(FilterError::Syntax);
                }
                Ok(inner)
            }
            "not" | "!" => Ok(Expr::Not(Box::new(self.primary()?))),
            "sport" | "dport" => {
                let op = match self.peek().and_then(Compare::parse) {
                    Some(op) => {
                        self.next();
                        op
                    }
                    None => Compare::Eq,
                };
                let value = self.next().ok_or(FilterError::Syntax)?;
                let port = self.port(&value)?;
                Ok(if token == "sport" {
                    Expr::Sport(op, port)
                } else {
                    Expr::Dport(op, port)
                })
            }
            "src" | "dst" => {
                // Addresses are only ever equal: `=`, `==` or `eq`, or nothing.
                if let Some(op) = self.peek().and_then(Compare::parse) {
                    if !matches!(op, Compare::Eq) {
                        return Err(FilterError::Syntax);
                    }
                    self.next();
                }
                let value = self.next().ok_or(FilterError::Syntax)?;
                let addr = self.address(&value)?;
                Ok(if token == "src" {
                    Expr::Src(addr)
                } else {
                    Expr::Dst(addr)
                })
            }
            "dev" | "fwmark" | "cgroup" | "autobound" | "inet-sockopt" => {
                Err(FilterError::Message(format!(
                    "ss: \"{token}\" filters are not supported on Windows"
                )))
            }
            other if Compare::parse(other).is_some() => Err(FilterError::Syntax),
            ")" | "and" | "&&" | "&" | "or" | "||" | "|" => Err(FilterError::Syntax),
            other if looks_like_address(other) => Err(FilterError::Syntax),
            other => Err(self.address(other).err().unwrap_or(FilterError::Syntax)),
        }
    }

    fn and(&mut self) -> Result<Expr, FilterError> {
        let mut left = self.primary()?;
        loop {
            match self.peek() {
                Some("and" | "&&" | "&") => {
                    self.next();
                    left = Expr::And(Box::new(left), Box::new(self.primary()?));
                }
                // Adjacent terms are joined by `and`, as in ss.
                Some(t) if !matches!(t, "or" | "||" | "|" | ")") => {
                    left = Expr::And(Box::new(left), Box::new(self.primary()?));
                }
                _ => return Ok(left),
            }
        }
    }

    fn or(&mut self) -> Result<Expr, FilterError> {
        let mut left = self.and()?;
        while matches!(self.peek(), Some("or" | "||" | "|")) {
            self.next();
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    /// The whole expression; anything left over is a syntax error.
    fn parse(&mut self) -> Result<Expr, FilterError> {
        let expression = self.or()?;
        if self.position < self.tokens.len() {
            return Err(FilterError::Syntax);
        }
        Ok(expression)
    }
}

/// Splits filter arguments into tokens; parentheses stand alone even when attached.
fn tokenize(args: &[String]) -> Vec<String> {
    let mut tokens = Vec::new();
    for arg in args {
        let spaced = arg.replace('(', " ( ").replace(')', " ) ");
        tokens.extend(spaced.split_whitespace().map(str::to_owned));
    }
    tokens
}

/// What the options select, kept as iproute2 keeps it (`current_filter`, `state_filter`,
/// `do_default`), so that combinations come out the same.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one of ss's independent switches"
)]
struct Options {
    numeric: bool,
    resolve: bool,
    processes: bool,
    summary: bool,
    kill: bool,
    no_header: bool,
    no_queues: bool,
    /// The selected socket tables (`DB_*`).
    tables: u32,
    /// The selected families (`FAMILY_*`).
    families: u8,
    /// The states the selected tables and families show by default.
    default_states: u16,
    /// The states `-a`, `-l`, `-B` or `-A` chose; 0 for none.
    state_filter: u16,
    /// No table or family was chosen: all tables, the default states.
    do_default: bool,
    saw_query: bool,
    /// The text of `-F FILE`.
    filter_text: Option<String>,
    filter: Vec<String>,
}

impl Options {
    const fn new() -> Self {
        Self {
            numeric: false,
            resolve: false,
            processes: false,
            summary: false,
            kill: false,
            no_header: false,
            no_queues: false,
            tables: 0,
            families: 0,
            default_states: 0,
            state_filter: 0,
            do_default: true,
            saw_query: false,
            filter_text: None,
            filter: Vec::new(),
        }
    }

    /// iproute2's `filter_db_set`.
    fn set_tables(&mut self, tables: u32, enable: bool) {
        for bit in (0..32).map(|i| 1u32 << i).filter(|bit| tables & bit != 0) {
            if enable {
                self.default_states |= table_default_states(bit);
                self.tables |= bit;
            } else {
                self.tables &= !bit;
            }
        }
        self.do_default = false;
    }

    /// iproute2's `filter_af_set`.
    const fn set_family(&mut self, family: u8) {
        self.default_states |= CONN;
        self.families |= family;
        self.do_default = false;
    }
}

enum Parsed {
    Run(Box<Options>),
    Exit(ExecutionResult),
}

impl builtins::Command for SsCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        match parse(&self.args, &context)? {
            Parsed::Run(options) => run(&options, &context),
            Parsed::Exit(result) => Ok(result),
        }
    }
}

/// Status 255, which iproute2 uses for option errors (`exit(-1)`).
fn option_error() -> ExecutionResult {
    ExecutionResult::new(255)
}

/// An option as `getopt_long` knows it: a short letter, or a long-only name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Code {
    Short(char),
    Long(&'static str),
}

/// iproute2 7.2's long options, in its order (which `getopt_long`'s ambiguity message
/// follows), and whether each takes an argument.
const LONG_OPTIONS: &[(&str, bool, Code)] = &[
    ("numeric", false, Code::Short('n')),
    ("resolve", false, Code::Short('r')),
    ("options", false, Code::Short('o')),
    ("extended", false, Code::Short('e')),
    ("memory", false, Code::Short('m')),
    ("info", false, Code::Short('i')),
    ("processes", false, Code::Short('p')),
    ("threads", false, Code::Short('T')),
    ("bpf", false, Code::Short('b')),
    ("events", false, Code::Short('E')),
    ("tcp", false, Code::Short('t')),
    ("sctp", false, Code::Short('S')),
    ("udp", false, Code::Short('u')),
    ("raw", false, Code::Short('w')),
    ("unix", false, Code::Short('x')),
    ("tipc", false, Code::Long("tipc")),
    ("vsock", false, Code::Long("vsock")),
    ("all", false, Code::Short('a')),
    ("listening", false, Code::Short('l')),
    ("bound-inactive", false, Code::Short('B')),
    ("ipv4", false, Code::Short('4')),
    ("ipv6", false, Code::Short('6')),
    ("packet", false, Code::Short('0')),
    ("family", true, Code::Short('f')),
    ("socket", true, Code::Short('A')),
    ("query", true, Code::Short('A')),
    ("summary", false, Code::Short('s')),
    ("diag", true, Code::Short('D')),
    ("filter", true, Code::Short('F')),
    ("version", false, Code::Short('V')),
    ("help", false, Code::Short('h')),
    ("context", false, Code::Short('Z')),
    ("contexts", false, Code::Short('z')),
    ("net", true, Code::Short('N')),
    ("tipcinfo", false, Code::Long("tipcinfo")),
    ("tos", false, Code::Long("tos")),
    ("cgroup", false, Code::Long("cgroup")),
    ("kill", false, Code::Short('K')),
    ("no-header", false, Code::Short('H')),
    ("no-queues", false, Code::Short('Q')),
    ("xdp", false, Code::Long("xdp")),
    ("mptcp", false, Code::Short('M')),
    ("oneline", false, Code::Short('O')),
    ("inet-sockopt", false, Code::Long("inet-sockopt")),
    ("bpf-maps", false, Code::Long("bpf-maps")),
    ("bpf-map-id", true, Code::Long("bpf-map-id")),
];

/// iproute2 7.2's short options (`halBetuwxnro460spTbEf:mMiA:D:F:vVzZN:KHQSO`).
const SHORT_OPTIONS: &str = "halBetuwxnro460spTbEfmMiADFvVzZNKHQSO";

/// Short options that take an argument.
const fn takes_argument(flag: char) -> bool {
    matches!(flag, 'f' | 'A' | 'D' | 'F' | 'N')
}

/// How a long option name failed to match.
enum LongError {
    Unrecognized,
    Ambiguous(Vec<&'static str>),
}

/// `getopt_long`'s lookup: an exact name, or an unambiguous prefix (several prefixes of
/// the same option, `--so` and `--q` for `-A`, are not ambiguous).
fn find_long(name: &str) -> Result<(&'static str, bool, Code), LongError> {
    if let Some(&option) = LONG_OPTIONS.iter().find(|(long, _, _)| *long == name) {
        return Ok(option);
    }
    let candidates: Vec<(&'static str, bool, Code)> = LONG_OPTIONS
        .iter()
        .copied()
        .filter(|(long, _, _)| !name.is_empty() && long.starts_with(name))
        .collect();
    match candidates.first() {
        None => Err(LongError::Unrecognized),
        Some(&first)
            if candidates
                .iter()
                .all(|&(_, value, code)| (value, code) == (first.1, first.2)) =>
        {
            Ok(first)
        }
        Some(_) => Err(LongError::Ambiguous(
            candidates.iter().map(|(long, _, _)| *long).collect(),
        )),
    }
}

/// The refusal for an option Windows cannot back.
fn refusal(code: Code) -> Option<&'static str> {
    Some(match code {
        Code::Short('x') => "-x: Unix domain sockets cannot be listed on Windows",
        Code::Short('w') => "-w: raw sockets are not listed by Windows' socket tables",
        Code::Short('0') => "-0: packet sockets do not exist on Windows",
        Code::Short('S') => "-S: SCTP is not available on Windows",
        Code::Short('M') => "-M: MPTCP is not available on Windows",
        Code::Short('e') => "-e: Windows has no socket uid or inode to show",
        Code::Short('m') => "-m: Windows does not expose socket memory",
        Code::Short('o') => "-o: Windows does not expose socket timers",
        Code::Short('i') => "-i: TCP internals (RTT, cwnd) are not supported by cash's ss",
        Code::Short('Z' | 'z') => "-Z: SELinux does not exist on Windows",
        Code::Short('N') => "-N: network namespaces do not exist on Windows",
        Code::Short('b') => "-b: BPF socket filters do not exist on Windows",
        Code::Short('E') => "-E: socket events are not supported",
        Code::Short('D') => "-D: dumping raw socket tables is not supported",
        Code::Short('T') => "-T: thread information is not supported",
        Code::Long("tipc") => "--tipc: TIPC is not available on Windows",
        Code::Long("tipcinfo") => "--tipcinfo: TIPC is not available on Windows",
        Code::Long("vsock") => "--vsock: vsock is not available on Windows",
        Code::Long("xdp") => "--xdp: XDP sockets do not exist on Windows",
        Code::Long("tos") => "--tos: Windows does not expose a socket's TOS or priority",
        Code::Long("cgroup") => "--cgroup: control groups do not exist on Windows",
        Code::Long("inet-sockopt") => {
            "--inet-sockopt: Windows does not expose the socket options of other processes"
        }
        Code::Long("bpf-maps") => "--bpf-maps: BPF socket storage does not exist on Windows",
        Code::Long("bpf-map-id") => "--bpf-map-id: BPF socket storage does not exist on Windows",
        _ => return None,
    })
}

/// Whether refused flags look like netstat's `-ano`/`-abno` habit.
fn looks_like_netstat(flags: &str) -> bool {
    flags.contains('o') && flags.contains('n') && !flags.contains('t') && !flags.contains('u')
}

/// Writes an option error and the usage, as getopt and iproute2's `usage()` do.
fn usage_error(
    message: &str,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<Parsed, cash_core::Error> {
    writeln!(context.stderr(), "{message}\n{USAGE}")?;
    Ok(Parsed::Exit(option_error()))
}

#[expect(
    clippy::too_many_lines,
    reason = "getopt_long's long and short forms, with their errors, in one loop"
)]
fn parse(
    args: &[String],
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<Parsed, cash_core::Error> {
    let mut options = Options::new();
    let mut args = args.iter();

    while let Some(arg) = args.next() {
        if arg == "--" {
            options.filter.extend(args.by_ref().cloned());
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = long
                .split_once('=')
                .map_or((long, None), |(n, v)| (n, Some(v.to_owned())));
            let (full, takes_value, code) = match find_long(name) {
                Ok(option) => option,
                Err(LongError::Unrecognized) => {
                    return usage_error(&format!("ss: unrecognized option '{arg}'"), context);
                }
                Err(LongError::Ambiguous(names)) => {
                    let names: Vec<String> = names.iter().map(|n| format!("'--{n}'")).collect();
                    return usage_error(
                        &format!(
                            "ss: option '{arg}' is ambiguous; possibilities: {}",
                            names.join(" ")
                        ),
                        context,
                    );
                }
            };
            let value = match (takes_value, value) {
                (false, Some(_)) => {
                    return usage_error(
                        &format!("ss: option '--{full}' doesn't allow an argument"),
                        context,
                    );
                }
                (true, None) => match args.next() {
                    Some(value) => Some(value.clone()),
                    None => {
                        return usage_error(
                            &format!("ss: option '--{full}' requires an argument"),
                            context,
                        );
                    }
                },
                (_, value) => value,
            };
            if let Some(message) = refusal(code) {
                writeln!(context.stderr(), "ss: {message}")?;
                return Ok(Parsed::Exit(ExecutionResult::general_error()));
            }
            if let Code::Short(flag) = code
                && let Some(result) = apply(flag, value, &mut options, context)?
            {
                return Ok(Parsed::Exit(result));
            }
            continue;
        }
        let Some(flags) = arg.strip_prefix('-').filter(|f| !f.is_empty()) else {
            options.filter.push(arg.clone());
            continue;
        };
        // Refuse before applying anything, so a netstat habit gets one clear message.
        // An option's argument (`-finet`) is not more flags.
        let letters = flags
            .char_indices()
            .find(|&(_, f)| takes_argument(f))
            .map_or(flags, |(at, f)| {
                flags.get(..at + f.len_utf8()).unwrap_or(flags)
            });
        if let Some(message) = letters.chars().find_map(|f| refusal(Code::Short(f))) {
            writeln!(context.stderr(), "ss: {message}")?;
            if looks_like_netstat(letters) {
                writeln!(
                    context.stderr(),
                    "ss: -{flags} looks like netstat's flags; the ss spelling is `ss -tuanp` (netstat.exe is still available)"
                )?;
            }
            return Ok(Parsed::Exit(ExecutionResult::general_error()));
        }
        for (index, flag) in flags.char_indices() {
            if !SHORT_OPTIONS.contains(flag) {
                return usage_error(&format!("ss: invalid option -- '{flag}'"), context);
            }
            if takes_argument(flag) {
                let attached = flags.get(index + flag.len_utf8()..).unwrap_or("");
                let value = if attached.is_empty() {
                    match args.next() {
                        Some(value) => value.clone(),
                        None => {
                            return usage_error(
                                &format!("ss: option requires an argument -- '{flag}'"),
                                context,
                            );
                        }
                    }
                } else {
                    attached.to_owned()
                };
                if let Some(result) = apply(flag, Some(value), &mut options, context)? {
                    return Ok(Parsed::Exit(result));
                }
                break;
            }
            if let Some(result) = apply(flag, None, &mut options, context)? {
                return Ok(Parsed::Exit(result));
            }
        }
    }
    Ok(Parsed::Run(Box::new(options)))
}

/// Applies one option; `Some` ends the command with that result.
fn apply(
    flag: char,
    value: Option<String>,
    options: &mut Options,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<Option<ExecutionResult>, cash_core::Error> {
    let value = value.unwrap_or_default();
    match flag {
        'h' => {
            writeln!(context.stdout(), "{USAGE}")?;
            return Ok(Some(ExecutionResult::success()));
        }
        'v' | 'V' => {
            writeln!(
                context.stdout(),
                "ss utility, iproute2-{IPROUTE2_VERSION} (cash {})",
                env!("CARGO_PKG_VERSION")
            )?;
            return Ok(Some(ExecutionResult::success()));
        }
        'n' => options.numeric = true,
        'r' => options.resolve = true,
        'p' => options.processes = true,
        's' => options.summary = true,
        'K' => options.kill = true,
        'H' => options.no_header = true,
        'Q' => options.no_queues = true,
        // One line per socket is all this ss ever prints.
        'O' => {}
        'a' => options.state_filter = ALL,
        'l' => options.state_filter = LISTENING,
        'B' => options.state_filter = State::BoundInactive.bit(),
        't' => options.set_tables(DB_TCP, true),
        'u' => options.set_tables(DB_UDP, true),
        '4' => options.set_family(FAMILY_V4),
        '6' => options.set_family(FAMILY_V6),
        'f' => match value.as_str() {
            "inet" => options.set_family(FAMILY_V4),
            "inet6" => options.set_family(FAMILY_V6),
            "help" => {
                writeln!(context.stdout(), "{USAGE}")?;
                return Ok(Some(ExecutionResult::success()));
            }
            "link" | "unix" | "netlink" | "tipc" | "vsock" | "xdp" => {
                writeln!(
                    context.stderr(),
                    "ss: -f {value}: only inet and inet6 exist on Windows"
                )?;
                return Ok(Some(ExecutionResult::general_error()));
            }
            other => {
                writeln!(
                    context.stderr(),
                    "ss: \"{other}\" is invalid family\n{USAGE}"
                )?;
                return Ok(Some(option_error()));
            }
        },
        'A' => return query(&value, options, context),
        'F' => {
            if options.filter_text.is_some() {
                writeln!(context.stderr(), "More than one filter file")?;
                return Ok(Some(option_error()));
            }
            let mut text = String::new();
            let read = if value.starts_with('-') {
                context.stdin().read_to_string(&mut text).map(|_| ())
            } else {
                std::fs::read_to_string(context.shell.absolute_path(&value)).map(|t| text = t)
            };
            if let Err(error) = read {
                let error = cash_core::error::os_error_text(&error);
                writeln!(context.stderr(), "fopen filter file: {error}")?;
                return Ok(Some(option_error()));
            }
            options.filter_text = Some(text);
        }
        other => {
            writeln!(context.stderr(), "ss: invalid option -- '{other}'\n{USAGE}")?;
            return Ok(Some(option_error()));
        }
    }
    Ok(None)
}

/// `-A QUERY`: socket tables by name, `!` taking one away.
fn query(
    value: &str,
    options: &mut Options,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<Option<ExecutionResult>, cash_core::Error> {
    if !options.saw_query {
        options.tables = 0;
        if options.state_filter == 0 {
            options.state_filter = CONN;
        }
        options.saw_query = true;
        options.do_default = false;
    }
    for item in value.split(',') {
        let (enable, name) = match item.strip_prefix('!') {
            Some(name) => (false, name),
            None => (true, item),
        };
        let Some(tables) = table_bits(name) else {
            writeln!(
                context.stderr(),
                "ss: \"{item}\" is illegal socket table id\n{USAGE}"
            )?;
            return Ok(Some(option_error()));
        };
        if enable && tables & (DB_TCP | DB_UDP) == 0 {
            writeln!(context.stderr(), "ss: -A {name}: not available on Windows")?;
            return Ok(Some(ExecutionResult::general_error()));
        }
        options.set_tables(tables, enable);
    }
    Ok(None)
}

/// One selected socket and the state it is shown in.
#[derive(Clone, Copy)]
struct Entry<'a> {
    socket: &'a Socket,
    state: State,
}

/// One printed socket, its fields already rendered.
struct Line {
    netid: &'static str,
    state: &'static str,
    local_host: String,
    local_port: String,
    peer_host: String,
    peer_port: String,
    process: String,
}

/// Host and interface names for printing, each looked up once.
struct Names {
    /// Reverse-lookup answers (`-r`); empty without it.
    hosts: HashMap<IpAddr, String>,
    interfaces: HashMap<u32, String>,
}

impl Names {
    /// An interface's name for an IPv6 scope id, `if<N>` when Windows has none, as
    /// iproute2 falls back.
    fn interface(&mut self, index: u32) -> &str {
        self.interfaces
            .entry(index)
            .or_insert_with(|| net::interface_name(index).unwrap_or_else(|| format!("if{index}")))
    }

    /// A host as ss prints it: the looked-up name, or numeric with IPv6 in brackets;
    /// a local address bound to a scope gets `%interface`. A peer never shows a scope.
    fn host(&mut self, ip: IpAddr, scope: u32) -> String {
        let base = self.hosts.get(&ip).cloned().unwrap_or_else(|| match ip {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => format!("[{v6}]"),
        });
        if scope == 0 {
            base
        } else {
            format!("{base}%{}", self.interface(scope))
        }
    }
}

const fn scope_of(addr: SocketAddr) -> u32 {
    match addr {
        SocketAddr::V6(v6) => v6.scope_id(),
        SocketAddr::V4(_) => 0,
    }
}

/// What the state words, tables and families come to: iproute2's `main` after its
/// option loop.
struct Selection {
    tables: u32,
    families: u8,
    states: u16,
}

/// Takes the leading `state`/`exclude` clauses off `tokens` and settles the selection,
/// or says why there is nothing to show (`Err` with the status).
fn select(
    options: &Options,
    tokens: &mut Vec<String>,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<Result<Selection, ExecutionResult>, cash_core::Error> {
    let mut state_filter = options.state_filter;
    let mut saw_states = false;
    while let Some(keyword) = tokens.first().map(String::as_str) {
        if !matches!(keyword, "state" | "exclude" | "excl") {
            break;
        }
        let Some(name) = tokens.get(1).cloned() else {
            writeln!(
                context.stderr(),
                "Command line is not complete. Try option \"help\""
            )?;
            return Ok(Err(option_error()));
        };
        let Some(bits) = state_bits(&name) else {
            writeln!(context.stderr(), "ss: wrong state name: {name}")?;
            return Ok(Err(option_error()));
        };
        if keyword == "state" {
            if !saw_states {
                state_filter = 0;
            }
            state_filter |= bits;
        } else {
            if !saw_states {
                state_filter = ALL;
            }
            state_filter &= !bits;
        }
        saw_states = true;
        tokens.drain(..2);
    }

    let mut tables = options.tables;
    let mut families = options.families;
    if options.do_default {
        if state_filter == 0 {
            state_filter = CONN;
        }
        tables = DB_ALL;
    }
    let states = if state_filter != 0 {
        state_filter
    } else {
        options.default_states
    };
    // `filter_merge_defaults`: a table brings its families, a family its tables.
    if tables & DB_INET != 0 && families == 0 {
        families = FAMILY_V4 | FAMILY_V6;
    }
    if families != 0 && tables & DB_INET == 0 {
        tables |= DB_INET;
    }
    if tables == 0 {
        writeln!(
            context.stderr(),
            "ss: no socket tables to show with such filter."
        )?;
        return Ok(Err(ExecutionResult::success()));
    }
    if states == 0 {
        writeln!(
            context.stderr(),
            "ss: no socket states to show with such filter."
        )?;
        return Ok(Err(ExecutionResult::success()));
    }
    Ok(Ok(Selection {
        tables,
        families,
        states,
    }))
}

/// `-K`: closes the selected TCP connections Windows can close, keeping those it closed.
/// IPv6 connections are reported as not closable; listeners, UDP and bound sockets are
/// skipped without a word, as iproute2 skips what the kernel cannot close.
fn kill<'a>(
    entries: Vec<Entry<'a>>,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<(Vec<Entry<'a>>, bool), cash_core::Error> {
    let mut closed = Vec::new();
    let mut all_closed = true;
    for entry in entries {
        let socket = entry.socket;
        if socket.proto != Proto::Tcp || entry.state == State::Listen {
            continue;
        }
        match (socket.local, socket.remote) {
            (SocketAddr::V4(local), Some(SocketAddr::V4(remote))) => {
                match net::close_tcp(local, remote) {
                    Ok(()) => closed.push(entry),
                    Err(error) => {
                        let error = cash_core::error::os_error_text(&error);
                        writeln!(
                            context.stderr(),
                            "ss: cannot close {local} -> {remote}: {error}"
                        )?;
                        all_closed = false;
                    }
                }
            }
            (local, Some(remote)) => {
                writeln!(
                    context.stderr(),
                    "ss: cannot close {local} -> {remote}: Windows closes IPv4 connections only"
                )?;
                all_closed = false;
            }
            (_, None) => {}
        }
    }
    Ok((closed, all_closed))
}

#[expect(
    clippy::too_many_lines,
    reason = "selecting and printing the table is one pipeline"
)]
fn run(
    options: &Options,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<ExecutionResult, cash_core::Error> {
    if options.kill && cash_win32::process::current_process_is_elevated() != Some(true) {
        writeln!(
            context.stderr(),
            "ss: -K: Windows lets only an elevated shell close connections (sudo ss -K ...)"
        )?;
        return Ok(ExecutionResult::general_error());
    }
    if options.summary {
        let result = summary(context)?;
        // As in iproute2: the summary alone, unless something was selected.
        if !result.is_success() || (options.do_default && options.filter.is_empty()) {
            return Ok(result);
        }
    }

    let services = Services::load();
    let mut filter_args = options.filter.clone();
    filter_args.extend(options.filter_text.iter().cloned());
    let mut tokens = tokenize(&filter_args);
    let selection = match select(options, &mut tokens, context)? {
        Ok(selection) => selection,
        Err(result) => return Ok(result),
    };

    let mut protos = Vec::new();
    if selection.tables & DB_UDP != 0 {
        protos.push(Proto::Udp);
    }
    if selection.tables & DB_TCP != 0 {
        protos.push(Proto::Tcp);
    }
    let v4 = selection.families & FAMILY_V4 != 0;
    let v6 = selection.families & FAMILY_V6 != 0;

    let expression = if tokens.is_empty() {
        None
    } else {
        let mut parser = FilterParser {
            tokens,
            position: 0,
            services: &services,
            protos: &protos,
        };
        match parser.parse() {
            Ok(expression) => Some(expression),
            Err(FilterError::Syntax) => {
                writeln!(context.stderr(), "{SYNTAX_ERROR}\n{USAGE}")?;
                return Ok(option_error());
            }
            Err(FilterError::Message(message)) => {
                writeln!(context.stderr(), "{message}")?;
                return Ok(ExecutionResult::general_error());
            }
        }
    };

    let fetched = if options.processes {
        net::sockets_with_owners(&protos, v4, v6)
    } else {
        net::sockets(&protos, v4, v6)
    };
    let sockets = match fetched {
        Ok(sockets) => sockets,
        Err(error) => {
            let error = cash_core::error::os_error_text(&error);
            writeln!(
                context.stderr(),
                "ss: can't read the socket tables: {error}"
            )?;
            return Ok(ExecutionResult::general_error());
        }
    };
    // The bound table is undocumented: if it cannot be read, there is nothing in it.
    let bound =
        if protos.contains(&Proto::Tcp) && selection.states & State::BoundInactive.bit() != 0 {
            net::bound_tcp_sockets(v4, v6).unwrap_or_default()
        } else {
            Vec::new()
        };

    // UDP first, then TCP, each IPv4 then IPv6, listeners first: iproute2's dump order.
    let mut selected: Vec<Entry<'_>> = sockets
        .iter()
        .map(|socket| Entry {
            socket,
            state: State::of(socket),
        })
        .chain(bound.iter().map(|socket| Entry {
            socket,
            state: State::BoundInactive,
        }))
        .filter(|e| selection.states & e.state.bit() != 0)
        .filter(|e| expression.as_ref().is_none_or(|x| x.matches(e.socket)))
        .collect();
    selected.sort_by_key(|e| {
        (
            e.socket.proto == Proto::Tcp,
            e.socket.local.is_ipv6(),
            e.state.rank(),
        )
    });

    let mut status = ExecutionResult::success();
    if options.kill {
        let (closed, all_closed) = kill(selected, context)?;
        selected = closed;
        if !all_closed {
            status = ExecutionResult::general_error();
        }
    }

    let mut names = Names {
        hosts: HashMap::new(),
        interfaces: HashMap::new(),
    };
    if options.resolve {
        let addresses: Vec<IpAddr> = selected
            .iter()
            .flat_map(|e| [Some(e.socket.local), e.socket.remote])
            .flatten()
            .map(|a| a.ip())
            .filter(|ip| !ip.is_unspecified())
            .collect();
        names.hosts = net::host_names(&addresses, RESOLVE_LIMIT);
    }

    let show_netid = selection.tables.count_ones() > 1;
    let show_state = selection.states.count_ones() > 1;
    let show_queues = !options.no_queues;
    let header = !options.no_header;

    let processes = options.processes.then(ProcessNames::new);
    let port_text = |port: u16, proto: Proto| -> String {
        if port == 0 {
            "*".to_owned()
        } else if options.numeric {
            port.to_string()
        } else {
            services
                .name(port, proto.name())
                .map_or_else(|| port.to_string(), str::to_owned)
        }
    };

    let lines: Vec<Line> = selected
        .iter()
        .map(|e| {
            let s = e.socket;
            let peer = peer_of(s);
            let process = match (&processes, s.pid) {
                (Some(processes), pid) if pid != 0 => {
                    let name = processes.name(pid).to_owned();
                    let service = s
                        .owner
                        .as_ref()
                        .filter(|owner| {
                            name.eq_ignore_ascii_case("svchost.exe")
                                && !owner.eq_ignore_ascii_case("svchost.exe")
                        })
                        .map(|owner| format!(",service={owner}"))
                        .unwrap_or_default();
                    format!("users:((\"{name}\",pid={pid},fd=-{service}))")
                }
                _ => String::new(),
            };
            Line {
                netid: s.proto.name(),
                state: e.state.label(),
                local_host: names.host(s.local.ip(), scope_of(s.local)),
                local_port: port_text(s.local.port(), s.proto),
                peer_host: names.host(peer.ip(), 0),
                peer_port: port_text(peer.port(), s.proto),
                process,
            }
        })
        .collect();

    // Widths: Netid, State and the queues keep their header widths even with -H, as in
    // iproute2; the address columns size to the data (and the header when printed).
    let width = |values: &mut dyn Iterator<Item = usize>, title: usize, always: bool| {
        values
            .max()
            .unwrap_or(0)
            .max(if header || always { title } else { 0 })
    };
    let netid_w = width(&mut lines.iter().map(|l| l.netid.len()), 5, true);
    let state_w = width(&mut lines.iter().map(|l| l.state.len()), 5, true);
    let lhost_w = width(&mut lines.iter().map(|l| l.local_host.len()), 13, false);
    let lport_w = width(&mut lines.iter().map(|l| l.local_port.len()), 4, false);
    let phost_w = width(&mut lines.iter().map(|l| l.peer_host.len()), 12, false);
    let pport_w = width(&mut lines.iter().map(|l| l.peer_port.len()), 4, false);
    let show_process = options.processes;

    let render = |netid: &str,
                  state: &str,
                  queues: (&str, &str),
                  local: (&str, &str),
                  peer: (&str, &str),
                  process: &str,
                  is_header: bool|
     -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        if show_netid {
            let _ = write!(out, "{netid:<netid_w$} ");
        }
        if show_state {
            let _ = write!(out, "{state:<state_w$} ");
        }
        if show_queues {
            let _ = write!(out, "{:<6} {:<6} ", queues.0, queues.1);
        }
        let _ = write!(out, "{:>lhost_w$}:{:<lport_w$} ", local.0, local.1);
        let _ = write!(out, "{:>phost_w$}:{:<pport_w$}", peer.0, peer.1);
        if show_process && !process.is_empty() {
            // iproute2 prints the Process title flush against the port column.
            if !is_header {
                out.push(' ');
            }
            out.push_str(process);
        }
        out.trim_end().to_owned()
    };

    let mut stdout = context.stdout();
    if header {
        writeln!(
            stdout,
            "{}",
            render(
                "Netid",
                "State",
                ("Recv-Q", "Send-Q"),
                ("Local Address", "Port"),
                ("Peer Address", "Port"),
                "Process",
                true
            )
        )?;
    }
    for line in &lines {
        writeln!(
            stdout,
            "{}",
            render(
                line.netid,
                line.state,
                ("0", "0"),
                (&line.local_host, &line.local_port),
                (&line.peer_host, &line.peer_port),
                &line.process,
                false
            )
        )?;
    }
    if status.is_success() {
        Ok(ExecutionExitCode::Success.into())
    } else {
        Ok(status)
    }
}

/// `ss -s`, in iproute2's layout, counted from the whole socket tables whatever else is
/// selected, as iproute2 counts the whole system. Windows has no raw or fragment
/// counters here, so those rows are 0, like the queue columns.
fn summary(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<ExecutionResult, cash_core::Error> {
    let sockets = match net::sockets(&[Proto::Udp, Proto::Tcp], true, true) {
        Ok(sockets) => sockets,
        Err(error) => {
            let error = cash_core::error::os_error_text(&error);
            writeln!(
                context.stderr(),
                "ss: can't read the socket tables: {error}"
            )?;
            return Ok(ExecutionResult::general_error());
        }
    };
    let count = |proto: Proto, v6: Option<bool>| {
        sockets
            .iter()
            .filter(|s| s.proto == proto && v6.is_none_or(|v6| s.local.is_ipv6() == v6))
            .count()
    };
    let tcp: Vec<&Socket> = sockets.iter().filter(|s| s.proto == Proto::Tcp).collect();
    let in_state = |state: TcpState| tcp.iter().filter(|s| s.state == Some(state)).count();
    let (udp4, udp6) = (
        count(Proto::Udp, Some(false)),
        count(Proto::Udp, Some(true)),
    );
    let (tcp4, tcp6) = (
        count(Proto::Tcp, Some(false)),
        count(Proto::Tcp, Some(true)),
    );
    let mut out = context.stdout();
    writeln!(out, "Total: {}", sockets.len())?;
    writeln!(
        out,
        "TCP:   {} (estab {}, closed {}, orphaned 0, timewait {})",
        tcp.len(),
        in_state(TcpState::Established),
        in_state(TcpState::Closed),
        in_state(TcpState::TimeWait)
    )?;
    writeln!(out)?;
    writeln!(out, "Transport Total     IP        IPv6")?;
    for (name, total, ip, ipv6) in [
        ("RAW", 0, 0, 0),
        ("UDP", udp4 + udp6, udp4, udp6),
        ("TCP", tcp4 + tcp6, tcp4, tcp6),
        ("INET", udp4 + udp6 + tcp4 + tcp6, udp4 + tcp4, udp6 + tcp6),
        ("FRAG", 0, 0, 0),
    ] {
        writeln!(out, "{name}\t  {total:<9} {ip:<9} {ipv6:<9}")?;
    }
    writeln!(out)?;
    Ok(ExecutionResult::success())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser_for(text: &str, services: &Services) -> Result<Expr, FilterError> {
        let mut parser = FilterParser {
            tokens: tokenize(&[text.to_owned()]),
            position: 0,
            services,
            protos: &[Proto::Tcp],
        };
        parser.parse()
    }

    fn filter(text: &str) -> Expr {
        let services = Services::load();
        let parsed = parser_for(text, &services).ok();
        assert!(parsed.is_some(), "{text} does not parse");
        parsed.unwrap()
    }

    fn is_syntax_error(text: &str) -> bool {
        let services = Services::load();
        matches!(parser_for(text, &services), Err(FilterError::Syntax))
    }

    fn socket(local: &str, remote: Option<&str>) -> Socket {
        Socket {
            proto: Proto::Tcp,
            local: local.parse().unwrap(),
            remote: remote.map(|r| r.parse().unwrap()),
            state: Some(if remote.is_some() {
                TcpState::Established
            } else {
                TcpState::Listen
            }),
            pid: 1,
            owner: None,
        }
    }

    #[test]
    fn port_comparisons_and_boolean_operators() {
        let listen = socket("127.0.0.1:8080", None);
        let conn = socket("127.0.0.1:50000", Some("10.0.0.1:443"));
        assert!(filter("sport = :8080").matches(&listen));
        assert!(!filter("sport = :8080").matches(&conn));
        assert!(filter("dport = :https").matches(&conn));
        assert!(filter("( sport = :8080 or dport gt :400 )").matches(&conn));
        assert!(filter("not sport = :8080").matches(&conn));
        assert!(filter("sport >= :8000 sport <= :9000").matches(&listen));
        assert!(!filter("(sport = :1)").matches(&listen));
    }

    #[test]
    fn address_conditions_with_prefix_and_port() {
        let conn = socket("192.168.1.5:50000", Some("10.1.2.3:22"));
        assert!(filter("dst 10.0.0.0/8").matches(&conn));
        assert!(filter("dst 10.1.2.3:22").matches(&conn));
        assert!(!filter("dst 10.1.2.3:23").matches(&conn));
        assert!(filter("src 192.168.1.0/24").matches(&conn));
        assert!(filter("dst == 10.1.2.3").matches(&conn));
        assert!(filter("dst eq 10.1.2.3").matches(&conn));
        let v6 = socket("[::1]:8080", None);
        assert!(filter("src [::1]:8080").matches(&v6));
    }

    #[test]
    fn a_socket_without_a_peer_has_peer_port_zero() {
        let listen = socket("127.0.0.1:8080", None);
        assert!(filter("dport = :0").matches(&listen));
        assert!(filter("dport < :100").matches(&listen));
        assert!(filter("dst 0.0.0.0").matches(&listen));
        assert!(filter("dst *").matches(&listen));
        let v6 = socket("[::1]:8080", None);
        assert!(filter("dst [::]:0").matches(&v6));
        assert!(!filter("dst 0.0.0.0").matches(&v6));
    }

    #[test]
    fn operators_other_than_equal_after_an_address_are_syntax_errors() {
        for text in [
            "src != 127.0.0.1",
            "dst > 1.2.3.4",
            "dst < 1.2.3.4",
            "dst",
            "sport = :1 or",
            "( sport = :1",
            "sport = :1 )",
            "0.0.0.0",
            ":5355",
        ] {
            assert!(is_syntax_error(text), "{text}");
        }
        let services = Services::load();
        assert!(matches!(
            parser_for("bogus", &services),
            Err(FilterError::Message(m)) if m.starts_with("Error: an inet prefix is expected rather than \"bogus\".")
        ));
    }

    #[test]
    fn state_groups_follow_ss() {
        // The default leaves out TIME-WAIT and SYN-RECV; `connected` keeps them.
        assert_eq!(CONN & State::TimeWait.bit(), 0);
        assert_eq!(CONN & State::SynRecv.bit(), 0);
        assert_ne!(CONNECTED & State::TimeWait.bit(), 0);
        assert_eq!(CONNECTED & State::Listen.bit(), 0);
        assert_eq!(state_bits("listening"), Some(State::Listen.bit()));
        for (name, state) in [
            ("unconnected", State::Close),
            ("close", State::Close),
            ("CLOSED", State::Close),
            ("syn-rcv", State::SynRecv),
            ("bound-inactive", State::BoundInactive),
        ] {
            assert_eq!(state_bits(name), Some(state.bit()), "{name}");
        }
        for name in ["listen", "new-syn-recv", "bogus"] {
            assert_eq!(state_bits(name), None, "{name}");
        }
    }

    #[test]
    fn long_options_match_as_getopt_long_does() {
        assert_eq!(find_long("num").ok().map(|o| o.2), Some(Code::Short('n')));
        assert_eq!(find_long("so").ok().map(|o| o.2), Some(Code::Short('A')));
        assert_eq!(
            find_long("context").ok().map(|o| o.2),
            Some(Code::Short('Z'))
        );
        assert!(
            matches!(find_long("n"), Err(LongError::Ambiguous(names)) if names == ["numeric", "net", "no-header", "no-queues"])
        );
        assert!(matches!(find_long("bogus"), Err(LongError::Unrecognized)));
    }

    #[test]
    fn a_scope_is_an_interface_name_and_a_peer_has_none() {
        let mut names = Names {
            hosts: HashMap::new(),
            interfaces: HashMap::from([(17, "ethernet_32769".to_owned())]),
        };
        let link_local: IpAddr = "fe80::1".parse().unwrap();
        assert_eq!(names.host(link_local, 17), "[fe80::1]%ethernet_32769");
        assert_eq!(names.host(link_local, 0), "[fe80::1]");
        // Windows' own name for the loopback pseudo-interface, index 1.
        assert!(!names.host(link_local, 1).contains(' '));
        names.hosts.insert(link_local, "router".to_owned());
        assert_eq!(names.host(link_local, 17), "router%ethernet_32769");
    }
}
