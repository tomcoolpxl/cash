//! `ss`: a subset of iproute2's socket statistics, on IP Helper's socket tables.
//!
//! ROADMAP item 9 and research/ss-evaluation.md. Output follows iproute2 7.2 (checked in
//! WSL): the same columns and widths, `Netid` only when several protocols are shown,
//! `State` only when the filter allows more than one state, ports as service names
//! unless `-n`, hosts numeric, and exit status 0 even when nothing matches.
//!
//! Windows differences, decided in the evaluation:
//!
//! * Recv-Q and Send-Q are not exposed and print as `0`;
//! * `-p` prints `fd=-` (Windows has no descriptor numbers) and, for services hosted
//!   in `svchost.exe`, `service=NAME` from the socket's owning module;
//! * UDP sockets carry no peer, so they are always `UNCONN`;
//! * options with no Windows backing (`-x`, `-e`, `-m`, `-o`, `-i`, `-K`, `-r`, other
//!   socket families) are refused by name, and netstat-style flags get a hint.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};
use cash_win32::net::{self, Proto, Socket, TcpState};
use clap::Parser;

use crate::fileuse::{ProcessNames, Services};

const USAGE: &str = "Usage: ss [ OPTIONS ]\n       ss [ OPTIONS ] [ FILTER ]\n   -h, --help          this message\n   -V, --version       output version information\n   -n, --numeric       don't resolve service names\n   -a, --all           display all sockets\n   -l, --listening     display listening sockets\n   -p, --processes     show process using socket\n   -s, --summary       show socket usage summary\n   -4, --ipv4          display only IP version 4 sockets\n   -6, --ipv6          display only IP version 6 sockets\n   -t, --tcp           display only TCP sockets\n   -u, --udp           display only UDP sockets\n   -H, --no-header     Suppress header line\n   -O, --oneline       socket's data printed on a single line\n   -Q, --no-queues     Suppress sending and receiving queue columns\n   -f, --family=FAMILY display sockets of type FAMILY (inet, inet6)\n   -A, --query=QUERY   socket tables to show (all, inet, tcp, udp)\n   -F, --filter=FILE   read filter information from FILE\n\n   FILTER := [ state STATE-FILTER ] [ EXPRESSION ]\n\ncash's ss is the Windows subset described in ROADMAP item 9.";

/// Investigate sockets.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct SsCommand {
    /// Options and the filter, parsed here: iproute2's grammar mixes combined short
    /// flags, long options with `=` and a free-form filter expression.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// The states a socket can be in, as ss names them. UDP sockets are `Closed`
/// (shown `UNCONN`), as in Linux.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Established,
    SynSent,
    SynRecv,
    FinWait1,
    FinWait2,
    TimeWait,
    Closed,
    CloseWait,
    LastAck,
    Listen,
    Closing,
}

const ALL_STATES: [State; 11] = [
    State::Established,
    State::SynSent,
    State::SynRecv,
    State::FinWait1,
    State::FinWait2,
    State::TimeWait,
    State::Closed,
    State::CloseWait,
    State::LastAck,
    State::Listen,
    State::Closing,
];

impl State {
    const fn bit(self) -> u16 {
        1 << (self as u16)
    }

    const fn of(socket: &Socket) -> Self {
        match socket.state {
            None => Self::Closed,
            Some(state) => match state {
                TcpState::Established => Self::Established,
                TcpState::SynSent => Self::SynSent,
                TcpState::SynReceived => Self::SynRecv,
                TcpState::FinWait1 => Self::FinWait1,
                TcpState::FinWait2 => Self::FinWait2,
                TcpState::TimeWait => Self::TimeWait,
                TcpState::Closed => Self::Closed,
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
            Self::Closed => "UNCONN",
            Self::CloseWait => "CLOSE-WAIT",
            Self::LastAck => "LAST-ACK",
            Self::Listen => "LISTEN",
            Self::Closing => "CLOSING",
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

const ALL: u16 = mask(&ALL_STATES);
const LISTENING: u16 = mask(&[State::Listen, State::Closed]);
const CONNECTED: u16 = ALL & !LISTENING;
const SYNCHRONIZED: u16 = CONNECTED & !State::SynSent.bit();
const BUCKET: u16 = mask(&[State::SynRecv, State::TimeWait]);
const BIG: u16 = ALL & !BUCKET;

/// A state name or group in a `state`/`exclude` clause.
fn state_bits(name: &str) -> Option<u16> {
    Some(match name.to_ascii_lowercase().as_str() {
        "all" => ALL,
        "connected" => CONNECTED,
        "synchronized" => SYNCHRONIZED,
        "bucket" => BUCKET,
        "big" => BIG,
        "listening" => LISTENING,
        "established" => State::Established.bit(),
        "syn-sent" => State::SynSent.bit(),
        "syn-recv" => State::SynRecv.bit(),
        "fin-wait-1" => State::FinWait1.bit(),
        "fin-wait-2" => State::FinWait2.bit(),
        "time-wait" => State::TimeWait.bit(),
        "closed" => State::Closed.bit(),
        "close-wait" => State::CloseWait.bit(),
        "last-ack" => State::LastAck.bit(),
        "listen" => State::Listen.bit(),
        "closing" => State::Closing.bit(),
        _ => return None,
    })
}

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
    fn matches(&self, addr: Option<SocketAddr>) -> bool {
        let Some(addr) = addr else {
            return false;
        };
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
            Self::Dport(op, port) => socket.remote.is_some_and(|r| op.holds(r.port(), *port)),
            Self::Src(addr) => addr.matches(Some(socket.local)),
            Self::Dst(addr) => addr.matches(socket.remote),
            Self::Not(inner) => !inner.matches(socket),
            Self::And(a, b) => a.matches(socket) && b.matches(socket),
            Self::Or(a, b) => a.matches(socket) || b.matches(socket),
        }
    }
}

/// Why a filter failed to parse, with ss's wording.
struct FilterError(String);

/// Parses the free-form filter: `state`/`exclude` clauses and an expression.
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
                FilterError(format!(
                    "Error: \"{bare}\" does not look like a port.\nCannot parse dst/src address."
                ))
            })
    }

    fn address(&self, text: &str) -> Result<AddrMatch, FilterError> {
        let bad = || {
            FilterError(format!(
                "Error: an inet prefix is expected rather than \"{text}\"."
            ))
        };
        // Split off a port: `[v6]:port`, `v4:port`, `*:port`, `:port`.
        let (host, port) = if let Some(rest) = text.strip_prefix('[') {
            let (host, after) = rest.split_once(']').ok_or_else(bad)?;
            (host, after.strip_prefix(':'))
        } else if text.matches(':').count() == 1 {
            let (host, port) = text.split_once(':').ok_or_else(bad)?;
            (host, Some(port))
        } else {
            (text, None)
        };
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
        let Some(token) = self.next() else {
            return Err(FilterError(
                "ss: bad filter expression: unexpected end".to_owned(),
            ));
        };
        match token.as_str() {
            "(" => {
                let inner = self.or()?;
                if self.next().as_deref() != Some(")") {
                    return Err(FilterError(
                        "ss: bad filter expression: missing ')'".to_owned(),
                    ));
                }
                Ok(inner)
            }
            "not" | "!" => Ok(Expr::Not(Box::new(self.primary()?))),
            "sport" | "dport" => {
                let (op, value) = match self.peek().and_then(Compare::parse) {
                    Some(op) => {
                        self.next();
                        (op, self.next().unwrap_or_default())
                    }
                    None => (Compare::Eq, self.next().unwrap_or_default()),
                };
                let port = self.port(&value)?;
                Ok(if token == "sport" {
                    Expr::Sport(op, port)
                } else {
                    Expr::Dport(op, port)
                })
            }
            "src" | "dst" => {
                if self.peek().and_then(Compare::parse).is_some() {
                    self.next();
                }
                let value = self.next().unwrap_or_default();
                let addr = self.address(&value)?;
                Ok(if token == "src" {
                    Expr::Src(addr)
                } else {
                    Expr::Dst(addr)
                })
            }
            "dev" | "fwmark" | "cgroup" | "autobound" | "inet-sockopt" => Err(FilterError(
                format!("ss: \"{token}\" filters are not supported on Windows"),
            )),
            other => Err(FilterError(format!(
                "ss: bad filter expression near \"{other}\""
            ))),
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

#[derive(Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one of ss's independent switches"
)]
struct Options {
    numeric: bool,
    all: bool,
    listening: bool,
    processes: bool,
    summary: bool,
    no_header: bool,
    no_queues: bool,
    v4: bool,
    v6: bool,
    tcp: bool,
    udp: bool,
    filter_file: Option<String>,
    filter: Vec<String>,
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

/// Status 255, which iproute2 uses for option errors.
fn option_error() -> ExecutionResult {
    ExecutionResult::new(255)
}

/// The refusal for an option Windows cannot back, by short name.
const fn refusal(flag: char) -> Option<&'static str> {
    Some(match flag {
        'x' => "-x: Unix domain sockets cannot be listed on Windows",
        'w' => "-w: raw sockets are not listed by Windows' socket tables",
        '0' => "-0: packet sockets do not exist on Windows",
        'd' => "-d: DCCP is not available on Windows",
        'S' => "-S: SCTP is not available on Windows",
        'M' => "-M: MPTCP is not available on Windows",
        'e' => "-e: Windows has no socket uid or inode to show",
        'm' => "-m: Windows does not expose socket memory",
        'o' => "-o: Windows does not expose socket timers",
        'i' => "-i: TCP internals (RTT, cwnd) are not supported by cash's ss",
        'K' => "-K: killing sockets is not supported",
        'r' => "-r: resolving host names is not supported; addresses are numeric",
        'Z' | 'z' => "-Z: SELinux does not exist on Windows",
        'N' => "-N: network namespaces do not exist on Windows",
        'b' => "-b: BPF socket filters do not exist on Windows",
        'E' => "-E: socket events are not supported",
        'D' => "-D: dumping raw socket tables is not supported",
        'T' => "-T: thread information is not supported",
        _ => return None,
    })
}

/// A long option's short equivalent.
fn long_to_short(name: &str) -> Option<char> {
    Some(match name {
        "numeric" => 'n',
        "resolve" => 'r',
        "all" => 'a',
        "listening" => 'l',
        "options" => 'o',
        "extended" => 'e',
        "memory" => 'm',
        "info" => 'i',
        "processes" => 'p',
        "kill" => 'K',
        "summary" => 's',
        "events" => 'E',
        "context" => 'Z',
        "contexts" => 'z',
        "net" => 'N',
        "bpf" => 'b',
        "ipv4" => '4',
        "ipv6" => '6',
        "packet" => '0',
        "tcp" => 't',
        "udp" => 'u',
        "dccp" => 'd',
        "raw" => 'w',
        "unix" => 'x',
        "sctp" => 'S',
        "mptcp" => 'M',
        "no-header" => 'H',
        "oneline" => 'O',
        "no-queues" => 'Q',
        "threads" => 'T',
        "diag" => 'D',
        "family" => 'f',
        "query" | "socket" => 'A',
        "filter" => 'F',
        _ => return None,
    })
}

/// Whether refused flags look like netstat's `-ano`/`-abno` habit.
fn looks_like_netstat(flags: &str) -> bool {
    flags.contains('o') && flags.contains('n') && !flags.contains('t') && !flags.contains('u')
}

#[allow(
    clippy::too_many_lines,
    reason = "iproute2's option set is one flat list"
)]
fn parse(
    args: &[String],
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<Parsed, cash_core::Error> {
    let mut options = Options::default();
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
            match name {
                "help" => {
                    writeln!(context.stdout(), "{USAGE}")?;
                    return Ok(Parsed::Exit(ExecutionResult::success()));
                }
                "version" => {
                    writeln!(
                        context.stdout(),
                        "ss utility, cash {}",
                        env!("CARGO_PKG_VERSION")
                    )?;
                    return Ok(Parsed::Exit(ExecutionResult::success()));
                }
                "vsock" | "tipc" | "xdp" => {
                    writeln!(context.stderr(), "ss: --{name}: not available on Windows")?;
                    return Ok(Parsed::Exit(ExecutionResult::general_error()));
                }
                _ => {}
            }
            let Some(short) = long_to_short(name) else {
                writeln!(context.stderr(), "ss: unrecognized option '{arg}'\n{USAGE}")?;
                return Ok(Parsed::Exit(option_error()));
            };
            let value = match (short, value) {
                ('f' | 'A' | 'F', None) => args.next().cloned(),
                (_, value) => value,
            };
            if let Some(result) = apply(short, value, &mut options, context)? {
                return Ok(Parsed::Exit(result));
            }
            continue;
        }
        let Some(flags) = arg.strip_prefix('-').filter(|f| !f.is_empty()) else {
            options.filter.push(arg.clone());
            continue;
        };
        // Refuse before applying anything, so a netstat habit gets one clear message.
        if let Some(flag) = flags.chars().find(|&f| refusal(f).is_some()) {
            let message = refusal(flag).unwrap_or_default();
            writeln!(context.stderr(), "ss: {message}")?;
            if looks_like_netstat(flags) {
                writeln!(
                    context.stderr(),
                    "ss: -{flags} looks like netstat's flags; the ss spelling is `ss -tuanp` (netstat.exe is still available)"
                )?;
            }
            return Ok(Parsed::Exit(ExecutionResult::general_error()));
        }
        for (index, flag) in flags.char_indices() {
            if matches!(flag, 'f' | 'A' | 'F') {
                let attached = flags.get(index + 1..).unwrap_or("");
                let value = if attached.is_empty() {
                    args.next().cloned()
                } else {
                    Some(attached.to_owned())
                };
                if let Some(result) = apply(flag, value, &mut options, context)? {
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
    if let Some(message) = refusal(flag) {
        writeln!(context.stderr(), "ss: {message}")?;
        return Ok(Some(ExecutionResult::general_error()));
    }
    match flag {
        'h' => {
            writeln!(context.stdout(), "{USAGE}")?;
            return Ok(Some(ExecutionResult::success()));
        }
        'V' => {
            writeln!(
                context.stdout(),
                "ss utility, cash {}",
                env!("CARGO_PKG_VERSION")
            )?;
            return Ok(Some(ExecutionResult::success()));
        }
        'n' => options.numeric = true,
        'a' => options.all = true,
        'l' => options.listening = true,
        'p' => options.processes = true,
        's' => options.summary = true,
        'H' => options.no_header = true,
        'Q' => options.no_queues = true,
        // One line per socket is all this ss ever prints.
        'O' => {}
        '4' => options.v4 = true,
        '6' => options.v6 = true,
        't' => options.tcp = true,
        'u' => options.udp = true,
        'f' => match value.as_deref() {
            Some("inet") => options.v4 = true,
            Some("inet6") => options.v6 = true,
            Some(other) => {
                writeln!(
                    context.stderr(),
                    "ss: -f {other}: only inet and inet6 exist on Windows"
                )?;
                return Ok(Some(ExecutionResult::general_error()));
            }
            None => {
                writeln!(context.stderr(), "ss: -f requires a family\n{USAGE}")?;
                return Ok(Some(option_error()));
            }
        },
        'A' => {
            for query in value.unwrap_or_default().split(',') {
                match query {
                    "all" | "inet" => {
                        options.tcp = true;
                        options.udp = true;
                    }
                    "tcp" => options.tcp = true,
                    "udp" => options.udp = true,
                    other => {
                        writeln!(context.stderr(), "ss: -A {other}: not available on Windows")?;
                        return Ok(Some(ExecutionResult::general_error()));
                    }
                }
            }
        }
        'F' => options.filter_file = value,
        other => {
            writeln!(context.stderr(), "ss: invalid option -- '{other}'\n{USAGE}")?;
            return Ok(Some(option_error()));
        }
    }
    Ok(None)
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

/// A host for display: numeric, IPv6 in brackets with its scope, as ss prints them.
fn host_text(ip: IpAddr, scope: u32) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) if scope != 0 => format!("[{v6}]%{scope}"),
        IpAddr::V6(v6) => format!("[{v6}]"),
    }
}

const fn scope_of(addr: SocketAddr) -> u32 {
    match addr {
        SocketAddr::V6(v6) => v6.scope_id(),
        SocketAddr::V4(_) => 0,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "selecting and printing the table is one pipeline"
)]
fn run(
    options: &Options,
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<ExecutionResult, cash_core::Error> {
    let services = Services::load();
    let mut protos = Vec::new();
    // Without -t or -u, show both, as ss shows every family it has.
    if options.udp || !options.tcp {
        protos.push(Proto::Udp);
    }
    if options.tcp || !options.udp {
        protos.push(Proto::Tcp);
    }
    let (v4, v6) = if options.v4 || options.v6 {
        (options.v4, options.v6)
    } else {
        (true, true)
    };

    // The filter, from arguments or -F.
    let mut filter_args = options.filter.clone();
    if let Some(file) = &options.filter_file {
        let mut text = String::new();
        let read = if file == "-" {
            context.stdin().read_to_string(&mut text).map(|_| ())
        } else {
            std::fs::read_to_string(context.shell.absolute_path(file)).map(|t| text = t)
        };
        if let Err(error) = read {
            let error = cash_core::error::os_error_text(&error);
            writeln!(
                context.stderr(),
                "ss: can't read filter file {file}: {error}"
            )?;
            return Ok(ExecutionResult::general_error());
        }
        filter_args.push(text);
    }
    let mut tokens = tokenize(&filter_args);

    // `state`/`exclude` clauses lead the filter.
    let mut states: Option<u16> = None;
    let mut excluded: u16 = 0;
    while let Some(keyword) = tokens.first().map(String::as_str) {
        if keyword != "state" && keyword != "exclude" && keyword != "excl" {
            break;
        }
        let Some(name) = tokens.get(1).cloned() else {
            writeln!(context.stderr(), "ss: {keyword} requires a state name")?;
            return Ok(option_error());
        };
        let Some(bits) = state_bits(&name) else {
            writeln!(context.stderr(), "ss: wrong state name: {name}")?;
            return Ok(option_error());
        };
        if keyword == "state" {
            states = Some(states.unwrap_or(0) | bits);
        } else {
            excluded |= bits;
        }
        tokens.drain(..2);
    }
    let default_states = if options.all {
        ALL
    } else if options.listening {
        LISTENING
    } else {
        CONNECTED
    };
    let states = states.unwrap_or(default_states) & !excluded;

    let expression = if tokens.is_empty() {
        None
    } else {
        let mut parser = FilterParser {
            tokens,
            position: 0,
            services: &services,
            protos: &protos,
        };
        match parser.or() {
            Ok(expression) if parser.position >= parser.tokens.len() => Some(expression),
            Ok(_) => {
                writeln!(context.stderr(), "ss: bad filter expression")?;
                return Ok(ExecutionResult::general_error());
            }
            Err(FilterError(message)) => {
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

    if options.summary {
        return summary(&sockets, context);
    }

    // UDP first, then TCP, as iproute2 dumps them.
    let mut selected: Vec<&Socket> = sockets
        .iter()
        .filter(|s| states & State::of(s).bit() != 0)
        .filter(|s| expression.as_ref().is_none_or(|e| e.matches(s)))
        .collect();
    selected.sort_by_key(|s| (s.proto == Proto::Tcp, s.local.is_ipv6()));

    let show_netid = protos.len() > 1;
    let show_state = states.count_ones() > 1;
    let show_queues = !options.no_queues;
    let header = !options.no_header;

    let mut processes = options.processes.then(ProcessNames::new);
    let port_text = |port: u16, proto: Proto| -> String {
        if options.numeric {
            port.to_string()
        } else {
            services
                .name(port, proto.name())
                .map_or_else(|| port.to_string(), str::to_owned)
        }
    };

    let lines: Vec<Line> = selected
        .iter()
        .map(|s| {
            let (peer_host, peer_port) = match s.remote {
                Some(remote) => (
                    host_text(remote.ip(), scope_of(remote)),
                    port_text(remote.port(), s.proto),
                ),
                None => (
                    if s.local.is_ipv6() { "[::]" } else { "0.0.0.0" }.to_owned(),
                    "*".to_owned(),
                ),
            };
            let process = match (&mut processes, s.pid) {
                (Some(names), pid) if pid != 0 => {
                    let name = names.name(pid).to_owned();
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
                state: State::of(s).label(),
                local_host: host_text(s.local.ip(), scope_of(s.local)),
                local_port: port_text(s.local.port(), s.proto),
                peer_host,
                peer_port,
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
    Ok(ExecutionExitCode::Success.into())
}

/// `ss -s`, in iproute2's layout, counted from the socket tables. Windows has no raw or
/// fragment counters here, so those rows are 0, like the queue columns.
fn summary(
    sockets: &[Socket],
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<ExecutionResult, cash_core::Error> {
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

    fn filter(text: &str) -> Expr {
        let services = Services::load();
        let mut parser = FilterParser {
            tokens: tokenize(&[text.to_owned()]),
            position: 0,
            services: &services,
            protos: &[Proto::Tcp],
        };
        let expr = parser.or().ok().unwrap();
        assert_eq!(parser.position, parser.tokens.len(), "{text}");
        expr
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
        let v6 = socket("[::1]:8080", None);
        assert!(filter("src [::1]:8080").matches(&v6));
    }

    #[test]
    fn state_groups_follow_ss() {
        assert_eq!(state_bits("listening"), Some(LISTENING));
        assert_eq!(CONNECTED & State::Listen.bit(), 0);
        assert_ne!(CONNECTED & State::TimeWait.bit(), 0);
        assert_eq!(state_bits("bogus"), None);
    }
}
