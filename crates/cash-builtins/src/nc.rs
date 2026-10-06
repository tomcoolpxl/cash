//! `nc`, OpenBSD netcat's (Debian's default `nc`), on Windows sockets: connect to a port
//! and carry standard input and output over it, listen for a connection, scan ports
//! (`-z`), or send and receive datagrams (`-u`).
//!
//! Options, their checks and the messages are OpenBSD netcat's as Debian's netcat-openbsd
//! 1.229 has them: `getopt`'s own messages and the usage on a bad option, `strtonum`'s
//! words for a bad value, `connect to HOST port N (tcp) failed: Connection refused` and
//! `Connection to HOST N port [tcp/http] succeeded!` with `-v`, `Listening on 0.0.0.0 N`
//! and `Connection received on 127.0.0.1 N` with `-lv`, and Debian's `-q` (quit so many
//! seconds after standard input ends; the default waits for the other side). Exit status
//! is 0 when a connection was made and ended, 1 otherwise.
//!
//! A clean Windows machine has no `nc`, and the usual stand-ins miss the common uses:
//! BusyBox's has no `-z`, ncat's messages differ, and `Test-NetConnection` is PowerShell.
//! Scripts write `nc -z host port` to wait for a service and `nc -l` to catch a line.
//!
//! What is not offered: Unix sockets (`-U`), proxies (`-x`, `-X`, `-P`), TCP MD5 (`-S`),
//! DCCP (`-Z`), routing tables (`-V`), passing the socket to another process (`-F`) and a
//! minimum TTL (`-m`), each refused by name. `-D` and `-T` are accepted without effect:
//! Windows has no `SO_DEBUG` to speak of, and ignores a socket's TOS. Netcat-traditional's
//! `-e` and `-c` are no options of OpenBSD's netcat and are refused as it refuses them.
//!
//! Windows differences: a listener names a peer numerically rather than by its reverse
//! lookup; `-u -l` answers the last sender rather than binding itself to the first; a
//! connection that the other side closes ends `nc` at once, where OpenBSD's waits for
//! standard input to end as well; `-v` alone does not probe a UDP port, only `-z` does;
//! `-h` prints to standard output.
//!
//! Standard input is read on a thread of its own: a console a line at a time, with the
//! console's own editing ([`cash_win32::conin::Terminal`]), and anything else in blocks.
//! When the connection ends first, the console thread is woken with a Ctrl-D typed into
//! the console ([`cash_win32::conwake`]) so that the console is handed back whole; a thread
//! still waiting on a pipe is left to end with the shell.

use std::io::Write as _;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cash_core::openfiles::OpenFile;
use cash_core::{ExecutionResult, builtins};
use cash_win32::conin::Line;
use clap::Parser;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpSocket, TcpStream, UdpSocket};
use tokio::sync::mpsc;

/// A command ended by Ctrl-C, as Bash reports one: 128 plus `SIGINT`.
const INTERRUPTED: u8 = 130;

/// How often a wait looks for a Ctrl-C, and the `-q` and `-w` timers.
const POLL: Duration = Duration::from_millis(50);

/// How much is read from the network, or standard input, at a time.
const BUFFER: usize = 16 * 1024;

/// The usage, as netcat prints it on a bad command line.
const USAGE: &str = "usage: nc [-46bCDdFhklNnrStUuvZz] [-I length] [-i interval] [-M ttl]
\t  [-m minttl] [-O length] [-P proxy_username] [-p source_port]
\t  [-q seconds] [-s sourceaddr] [-T keyword] [-V rtable] [-W recvlimit]
\t  [-w timeout] [-X proxy_protocol] [-x proxy_address[:port]]
\t  [destination] [port]
";

/// What `-h` prints after the usage: netcat's command summary.
const SUMMARY: &str = "\tCommand Summary:
\t\t-4\t\tUse IPv4
\t\t-6\t\tUse IPv6
\t\t-b\t\tAllow broadcast
\t\t-C\t\tSend CRLF as line-ending
\t\t-D\t\tEnable the debug socket option
\t\t-d\t\tDetach from stdin
\t\t-F\t\tPass socket fd
\t\t-h\t\tThis help text
\t\t-I length\tTCP receive buffer length
\t\t-i interval\tDelay interval for lines sent, ports scanned
\t\t-k\t\tKeep inbound sockets open for multiple connects
\t\t-l\t\tListen mode, for inbound connects
\t\t-M ttl\t\tOutgoing TTL / Hop Limit
\t\t-m minttl\tMinimum incoming TTL / Hop Limit
\t\t-N\t\tShutdown the network socket after EOF on stdin
\t\t-n\t\tSuppress name/port resolutions
\t\t-O length\tTCP send buffer length
\t\t-P proxyuser\tUsername for proxy authentication
\t\t-p port\t\tSpecify local port for remote connects
\t\t-q secs\t\tquit after EOF on stdin and delay of secs
\t\t-r\t\tRandomize remote ports
\t\t-S\t\tEnable the TCP MD5 signature option
\t\t-s sourceaddr\tLocal source address
\t\t-T keyword\tTOS value
\t\t-t\t\tAnswer TELNET negotiation
\t\t-U\t\tUse UNIX domain socket
\t\t-u\t\tUDP mode
\t\t-V rtable\tSpecify alternate routing table
\t\t-v\t\tVerbose
\t\t-W recvlimit\tTerminate after receiving a number of packets
\t\t-w timeout\tTimeout for connects and final net reads
\t\t-X proto\tProxy protocol: \"4\", \"5\" (SOCKS) or \"connect\"
\t\t-x addr[:port]\tSpecify proxy address and port
\t\t-Z\t\tDCCP mode
\t\t-z\t\tZero-I/O mode [used for scanning]
\tPort numbers can be individual or ranges: lo-hi [inclusive]
";

/// The line `-h` starts with, where Debian's names its patch level.
const VERSION: &str =
    "nc (cash): OpenBSD netcat's options, as in Debian's netcat-openbsd 1.229, on Windows sockets";

/// netcat's `getopt` string: the options, `:` after those that take a value.
const GETOPT: &str = "46bCDdFhI:i:klM:m:NnO:P:p:q:rSs:T:tUuV:vW:w:X:x:Zz";

/// Connect to, listen on or scan TCP and UDP ports.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct NcCommand {
    /// Options, the destination and the port: parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    V4,
    V6,
}

impl Family {
    const fn accepts(self, address: IpAddr) -> bool {
        match self {
            Self::V4 => address.is_ipv4(),
            Self::V6 => address.is_ipv6(),
        }
    }
}

/// What the command line asks for.
#[derive(Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one of netcat's independent switches"
)]
struct Options {
    family: Option<Family>,
    /// `-b`: datagrams may go to a broadcast address.
    broadcast: bool,
    /// `-C`: a newline sent is sent as CRLF.
    crlf: bool,
    /// `-d`: standard input is not read.
    no_stdin: bool,
    /// `-k`: listen again after a connection ends.
    keep: bool,
    listen: bool,
    /// `-N`: the socket's write side is shut when standard input ends.
    shutdown_after_eof: bool,
    /// `-n`: no name or service lookups.
    numeric: bool,
    /// `-p`: the local port, as given.
    source_port: Option<String>,
    /// `-q`: how long after standard input ends to quit; `None` waits for the other side.
    quit_after: Option<Duration>,
    /// `-r`: the ports of a range in random order.
    random: bool,
    /// `-s`: the local address, as given.
    source: Option<String>,
    /// `-t`: telnet negotiations are answered.
    telnet: bool,
    udp: bool,
    verbose: bool,
    /// `-w`: how long a connect may take, and how long a connection may be idle.
    timeout: Option<Duration>,
    /// `-z`: connect, report and close.
    scan: bool,
    /// `-i`: between the lines sent, and the ports scanned.
    interval: Duration,
    /// `-M`: the TTL of what is sent.
    ttl: Option<u32>,
    /// `-W`: end after so many reads from the network.
    recv_limit: Option<u64>,
    /// `-I`: the TCP receive buffer's size.
    recv_buffer: Option<u32>,
    /// `-O`: the TCP send buffer's size.
    send_buffer: Option<u32>,
    /// The destination, or the address to listen on.
    host: Option<String>,
    /// The port, as given: a number, a service name or a range.
    port: String,
}

impl Options {
    const fn proto(&self) -> &'static str {
        if self.udp { "udp" } else { "tcp" }
    }
}

/// What the command line comes to.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Help,
    Run(Box<Options>),
}

/// A command line refused: its message (after `nc: `), and whether the usage follows, as
/// it does for what `getopt` refuses and for operands missing or too many.
#[derive(Debug, PartialEq, Eq)]
struct Refusal {
    message: String,
    usage: bool,
}

impl Refusal {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            usage: true,
        }
    }

    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            usage: false,
        }
    }
}

/// OpenBSD's `strtonum`: a whole number between `min` and `max`, or why not, in its
/// words.
fn strtonum(text: &str, min: i64, max: i64) -> Result<i64, &'static str> {
    match text.parse::<i64>() {
        Ok(n) if n < min => Err("too small"),
        Ok(n) if n > max => Err("too large"),
        Ok(n) => Ok(n),
        Err(_) => Err("invalid"),
    }
}

/// A value read with `strtonum`, refused with netcat's `WHAT ERR: VALUE` message.
fn number(text: &str, min: i64, max: i64, what: &str) -> Result<i64, Refusal> {
    strtonum(text, min, max).map_err(|why| Refusal::message(format!("{what} {why}: {text}")))
}

/// The `-T` keywords netcat knows.
const TOS_KEYWORDS: &[&str] = &[
    "af11",
    "af12",
    "af13",
    "af21",
    "af22",
    "af23",
    "af31",
    "af32",
    "af33",
    "af41",
    "af42",
    "af43",
    "critical",
    "cs0",
    "cs1",
    "cs2",
    "cs3",
    "cs4",
    "cs5",
    "cs6",
    "cs7",
    "ef",
    "inetcontrol",
    "lowcost",
    "lowdelay",
    "netcontrol",
    "reliability",
    "throughput",
];

/// Whether `text` is a TOS netcat accepts: a keyword, a hexadecimal `0x..` or a number
/// up to 255. Windows ignores a socket's TOS, so the value itself is not kept.
fn valid_tos(text: &str) -> bool {
    if TOS_KEYWORDS.contains(&text) {
        return true;
    }
    if let Some(hex) = text.strip_prefix("0x") {
        return u8::from_str_radix(hex, 16).is_ok();
    }
    strtonum(text, 0, 255).is_ok()
}

/// Why an option Windows cannot back is refused, by its letter.
const fn refusal(letter: char) -> Option<&'static str> {
    Some(match letter {
        'F' => "-F: passing the socket to another process is not possible on Windows",
        'm' => "-m: a minimum TTL cannot be enforced on Windows",
        'P' | 'X' | 'x' => "-x, -X and -P: proxies are not supported; connect directly",
        'S' => "-S: TCP MD5 signatures are not available on Windows",
        'U' => "-U: Unix domain sockets are not supported; nc speaks TCP and UDP",
        'V' => "-V: alternate routing tables do not exist on Windows",
        'Z' => "-Z: DCCP is not available on Windows",
        _ => return None,
    })
}

/// The options as they are read.
struct Reading {
    options: Options,
    help: bool,
}

impl Reading {
    /// Applies one option; `value` is the argument of those that take one.
    fn apply(&mut self, letter: char, value: Option<&str>) -> Result<(), Refusal> {
        let value = value.unwrap_or_default();
        let options = &mut self.options;
        match letter {
            '4' => options.family = Some(Family::V4),
            '6' => options.family = Some(Family::V6),
            'b' => options.broadcast = true,
            'C' => options.crlf = true,
            // SO_DEBUG: nothing to switch on here.
            'D' => {}
            'd' => options.no_stdin = true,
            'h' => self.help = true,
            'I' => {
                let size = number(value, 1, 65536 << 14, "TCP receive window")?;
                options.recv_buffer = Some(u32::try_from(size).unwrap_or(u32::MAX));
            }
            'O' => {
                let size = number(value, 1, 65536 << 14, "TCP send window")?;
                options.send_buffer = Some(u32::try_from(size).unwrap_or(u32::MAX));
            }
            'i' => {
                let seconds = number(value, 0, i64::from(u32::MAX), "interval")?;
                options.interval = Duration::from_secs(seconds.unsigned_abs());
            }
            'k' => options.keep = true,
            'l' => options.listen = true,
            'M' => {
                let ttl = strtonum(value, 0, 255)
                    .map_err(|why| Refusal::message(format!("ttl is {why}")))?;
                options.ttl = Some(u32::try_from(ttl).unwrap_or(255));
            }
            'm' => {
                strtonum(value, 0, 255)
                    .map_err(|why| Refusal::message(format!("minttl is {why}")))?;
                return Err(Refusal::message(refusal('m').unwrap_or_default()));
            }
            'N' => options.shutdown_after_eof = true,
            'n' => options.numeric = true,
            'p' => options.source_port = Some(value.to_owned()),
            'q' => {
                let seconds = number(
                    value,
                    i64::from(i32::MIN),
                    i64::from(i32::MAX),
                    "quit timer",
                )?;
                // A negative value waits forever, as Debian's does; a non-negative one
                // implies -N.
                options.quit_after = u64::try_from(seconds).ok().map(Duration::from_secs);
                if options.quit_after.is_some() {
                    options.shutdown_after_eof = true;
                }
            }
            'r' => options.random = true,
            's' => options.source = Some(value.to_owned()),
            'T' => {
                if !valid_tos(value) {
                    return Err(Refusal::message(format!("illegal tos value {value}")));
                }
            }
            't' => options.telnet = true,
            'u' => options.udp = true,
            'v' => options.verbose = true,
            'W' => {
                let limit = number(value, 1, i64::from(i32::MAX), "receive limit")?;
                options.recv_limit = Some(limit.unsigned_abs());
            }
            'w' => {
                let seconds = number(value, 0, i64::from(i32::MAX / 1000), "timeout")?;
                options.timeout = Some(Duration::from_secs(seconds.unsigned_abs()));
            }
            'z' => options.scan = true,
            other => {
                return Err(Refusal::message(refusal(other).unwrap_or_default()));
            }
        }
        Ok(())
    }
}

/// Whether `letter` is an option, and whether it takes a value.
fn option_kind(letter: char) -> Option<bool> {
    let mut letters = GETOPT.chars().peekable();
    while let Some(known) = letters.next() {
        let takes_value = letters.next_if_eq(&':').is_some();
        if known == letter {
            return Some(takes_value);
        }
    }
    None
}

/// Reads the command line as netcat does: `getopt`, with GNU's permutation of operands
/// among options, then the destination and the port.
fn parse(args: &[String]) -> Result<Parsed, Refusal> {
    let mut reading = Reading {
        options: Options::default(),
        help: false,
    };
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        index += 1;
        if arg == "--" {
            operands.extend(args.iter().skip(index).map(String::as_str));
            break;
        }
        let Some(cluster) = arg.strip_prefix('-').filter(|rest| !rest.is_empty()) else {
            operands.push(arg);
            continue;
        };
        for (at, letter) in cluster.char_indices() {
            let Some(takes_value) = option_kind(letter) else {
                return Err(Refusal::usage(format!("invalid option -- '{letter}'")));
            };
            if !takes_value {
                reading.apply(letter, None)?;
                continue;
            }
            let rest = cluster.get(at + letter.len_utf8()..).unwrap_or_default();
            let value = if rest.is_empty() {
                let Some(next) = args.get(index) else {
                    return Err(Refusal::usage(format!(
                        "option requires an argument -- '{letter}'"
                    )));
                };
                index += 1;
                next.as_str()
            } else {
                rest
            };
            reading.apply(letter, Some(value))?;
            break;
        }
    }
    if reading.help {
        return Ok(Parsed::Help);
    }
    let read = reading.options;

    // The destination and the port. `nc -l PORT` listens; Debian's also takes the port
    // from -p: `nc -l -p PORT [HOST]`.
    let listen_port = read.listen.then(|| read.source_port.clone()).flatten();
    let (host, port) = match (operands.as_slice(), listen_port) {
        ([host, port], _) => (Some((*host).to_owned()), (*port).to_owned()),
        ([port], None) if read.listen => (None, (*port).to_owned()),
        ([host], Some(port)) => (Some((*host).to_owned()), port),
        ([], Some(port)) => (None, port),
        _ => return Err(Refusal::usage(String::new())),
    };
    let options = Options { host, port, ..read };

    if options.listen && options.scan {
        return Err(Refusal::message("cannot use -z and -l"));
    }
    if options.listen && options.source.is_some() {
        return Err(Refusal::message("cannot use -s and -l"));
    }
    if options.keep && !options.listen {
        return Err(Refusal::message("must use -l with -k"));
    }
    Ok(Parsed::Run(Box::new(options)))
}

/// The service names (`cash_core::net::SERVICES`), shared with the shell's own
/// `/dev/tcp/HOST/SERVICE` redirections, and the two lookups on them.
use cash_core::net::{service_name, service_port};

/// netcat's `build_ports`: a service name, a range `lo-hi` (in random order with `-r`),
/// or one number, each between 1 and 65535.
fn build_ports(text: &str, numeric: bool, random: bool) -> Result<Vec<u16>, Refusal> {
    if !numeric && let Some(port) = service_port(text) {
        return Ok(vec![port]);
    }
    let port = |p: &str| -> Result<u16, Refusal> {
        let n = strtonum(p, 1, 65535)
            .map_err(|why| Refusal::message(format!("port number {why}: {p}")))?;
        Ok(u16::try_from(n).unwrap_or(u16::MAX))
    };
    let Some((lo, hi)) = text.split_once('-') else {
        return Ok(vec![port(text)?]);
    };
    let (lo, hi) = (port(lo)?, port(hi)?);
    let (lo, hi) = if hi < lo { (hi, lo) } else { (lo, hi) };
    let mut ports: Vec<u16> = (lo..=hi).collect();
    if random {
        shuffle(&mut ports);
    }
    Ok(ports)
}

/// Puts `items` in a random order: a Fisher-Yates shuffle on a xorshift generator seeded
/// from the standard library's random hasher. Port order needs no more.
fn shuffle<T>(items: &mut [T]) {
    use std::hash::{BuildHasher as _, Hasher as _};
    let mut state = std::hash::RandomState::new().build_hasher().finish() | 1;
    for end in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let choices = u64::try_from(end).unwrap_or(u64::MAX).saturating_add(1);
        let pick = usize::try_from(state % choices).unwrap_or(0);
        items.swap(end, pick);
    }
}

/// A port given on its own, as `-p` and a listener's take it: a number, or a service
/// name unless `-n`. Refused as `getaddrinfo` refuses a service it does not know.
fn single_port(text: &str, numeric: bool) -> Result<u16, Refusal> {
    if !numeric && let Some(port) = service_port(text) {
        return Ok(port);
    }
    strtonum(text, 1, 65535)
        .ok()
        .and_then(|n| u16::try_from(n).ok())
        .ok_or_else(|| Refusal::message("getaddrinfo: Servname not supported for ai_socktype"))
}

/// A socket error in glibc's words, which netcat's messages are read in; the kinds
/// Windows reports under other names mapped, the rest as the shell reports them.
fn reason(error: &std::io::Error) -> String {
    cash_core::net::connect_failure(error)
}

/// A name lookup's failure in `gai_strerror`'s words (`cash_core::net`).
fn lookup_failure(error: &std::io::Error) -> String {
    cash_core::net::lookup_failure(error)
}

/// The addresses `host` names, in the order they are tried; the message (after `nc: `)
/// when it names none.
async fn resolve(host: &str, numeric: bool, family: Option<Family>) -> Result<Vec<IpAddr>, String> {
    let wanted = |address: &IpAddr| family.is_none_or(|family| family.accepts(*address));
    let unsupported = || "getaddrinfo: Address family for hostname not supported".to_owned();
    if let Ok(address) = host.parse::<IpAddr>() {
        return if wanted(&address) {
            Ok(vec![address])
        } else {
            Err(unsupported())
        };
    }
    if numeric {
        return Err("getaddrinfo: Name or service not known".to_owned());
    }
    let found = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|error| format!("getaddrinfo: {}", lookup_failure(&error)))?;
    let mut addresses: Vec<IpAddr> = Vec::new();
    for address in found.map(|socket| socket.ip()) {
        if wanted(&address) && !addresses.contains(&address) {
            addresses.push(address);
        }
    }
    if addresses.is_empty() {
        return Err(unsupported());
    }
    Ok(addresses)
}

/// The unspecified address of a family, to bind to when none is named.
const fn any_address(v6: bool) -> IpAddr {
    if v6 {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    }
}

/// A received piece of standard input, or how it ended.
enum Input {
    Data(Vec<u8>),
    Eof,
    /// Ctrl-C typed at the console, which the console hands over as a key.
    Interrupted,
}

/// The thread reading a console, and what wakes it.
struct ConsoleReader {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
    input: OpenFile,
}

/// Standard input, read on a thread of its own and handed over in pieces.
struct StdinFeed {
    rx: mpsc::Receiver<Input>,
    console: Option<ConsoleReader>,
}

impl StdinFeed {
    /// Starts reading `input`: a console a line at a time, with its editing, anything
    /// else in blocks as they come. A Ctrl-C typed at the console sets `interrupt` as
    /// well as being handed over, for the waits that do not read standard input.
    fn start(input: OpenFile, interrupt: Arc<AtomicBool>) -> Self {
        let (tx, rx) = mpsc::channel(4);
        let at_console =
            input.is_terminal() && matches!(input, OpenFile::Stdin(_) | OpenFile::File(_));
        if !at_console {
            std::thread::spawn(move || read_blocks(input, &tx));
            return Self { rx, console: None };
        }
        let stop = Arc::new(AtomicBool::new(false));
        let reader_input = input.clone();
        let reader_stop = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            read_console_lines(&reader_input, &reader_stop, &interrupt, &tx);
        });
        Self {
            rx,
            console: Some(ConsoleReader {
                stop,
                thread,
                input,
            }),
        }
    }

    async fn recv(&mut self) -> Option<Input> {
        self.rx.recv().await
    }

    /// Ends the reading. A console thread still waiting for a line is woken with a
    /// Ctrl-D and waited for, so that the console is as it was when the prompt returns;
    /// a thread waiting on a pipe cannot be woken and is left to end with the shell.
    async fn finish(self) {
        let Some(console) = self.console else {
            return;
        };
        console.stop.store(true, Ordering::SeqCst);
        if !console.thread.is_finished() {
            let woken = match &console.input {
                OpenFile::Stdin(stdin) => cash_win32::conwake::end_line_read(stdin),
                OpenFile::File(file) => cash_win32::conwake::end_line_read(file.as_ref()),
                _ => Ok(()),
            };
            if woken.is_err() {
                return;
            }
        }
        let thread = console.thread;
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(move || {
                let _ = thread.join();
            }),
        )
        .await;
    }
}

/// Reads `input` in blocks until it ends or the receiver is gone.
fn read_blocks(mut input: OpenFile, tx: &mpsc::Sender<Input>) {
    let mut buffer = vec![0u8; BUFFER];
    loop {
        let message = match std::io::Read::read(&mut input, &mut buffer) {
            Ok(0) | Err(_) => Input::Eof,
            Ok(n) => Input::Data(buffer.get(..n).unwrap_or_default().to_vec()),
        };
        let last = !matches!(message, Input::Data(_));
        if tx.blocking_send(message).is_err() || last {
            return;
        }
    }
}

/// Reads the console `input` a line at a time, until it ends, the receiver is gone, or
/// `stop` is set and the thread woken (see [`StdinFeed::finish`]). Ctrl-C sets
/// `interrupt`: the console in this state makes no control event of it.
fn read_console_lines(
    input: &OpenFile,
    stop: &AtomicBool,
    interrupt: &AtomicBool,
    tx: &mpsc::Sender<Input>,
) {
    let Some(mut terminal) = input.console(true, true) else {
        read_blocks(input.clone(), tx);
        return;
    };
    loop {
        let line = terminal.line();
        if stop.load(Ordering::SeqCst) {
            // Woken, or a line came just as the wake was typed: then the Ctrl-D typed to
            // wake this thread is still queued, and is taken out before the prompt sees
            // it. A line Ctrl-D ended has no newline.
            if matches!(&line, Ok(Line::Typed(text)) if text.ends_with('\n')) {
                let _ = terminal.next(Some(Instant::now() + POLL));
            }
            return;
        }
        let message = match line {
            Ok(Line::Typed(text)) => Input::Data(text.into_bytes()),
            Ok(Line::EndOfInput) | Err(_) => Input::Eof,
            Ok(Line::Interrupted) => {
                interrupt.store(true, Ordering::SeqCst);
                Input::Interrupted
            }
        };
        let last = !matches!(message, Input::Data(_));
        if tx.blocking_send(message).is_err() || last {
            return;
        }
    }
}

/// Standard input across the connections of one run: once it has ended, it has ended
/// for the connections that follow (`-k`) as well.
struct Stdin {
    feed: Option<StdinFeed>,
    /// Whether anything more may come: not with `-d`, nor after its end.
    open: bool,
    /// A Ctrl-C typed at the console, not yet acted on.
    interrupt: Arc<AtomicBool>,
}

impl Stdin {
    fn start(options: &Options, input: Option<OpenFile>) -> Self {
        let interrupt = Arc::new(AtomicBool::new(false));
        if options.no_stdin {
            return Self {
                feed: None,
                open: false,
                interrupt,
            };
        }
        let feed = input.map(|input| StdinFeed::start(input, Arc::clone(&interrupt)));
        Self {
            open: feed.is_some(),
            feed,
            interrupt,
        }
    }

    /// Whether a Ctrl-C is waiting to be acted on, from the keyboard through the console's
    /// control event, or typed at a console read as keys; this call takes it. A job in
    /// the background hears no Ctrl-C of its own, as in `interp.rs`.
    fn ctrl_c_pending(&self, params: &cash_core::ExecutionParameters) -> bool {
        (!params.is_asynchronous() && cash_win32::console::take_interrupt())
            || self.interrupt.swap(false, Ordering::SeqCst)
    }

    async fn recv(&mut self) -> Option<Input> {
        match &mut self.feed {
            Some(feed) => feed.recv().await,
            None => None,
        }
    }

    async fn finish(self) {
        if let Some(feed) = self.feed {
            feed.finish().await;
        }
    }
}

/// The side of a connection that is read.
enum NetReader {
    Tcp(OwnedReadHalf),
    Udp(Arc<UdpSocket>),
}

/// The side of a connection that is written.
enum NetWriter {
    Tcp(OwnedWriteHalf),
    Udp(Arc<UdpSocket>),
}

/// What a read from the network produced.
enum Received {
    /// So many bytes, from this sender when the socket is a datagram one.
    Data(usize, Option<SocketAddr>),
    /// The other side closed its write side.
    Closed,
}

async fn receive(reader: &mut NetReader, buffer: &mut [u8]) -> std::io::Result<Received> {
    match reader {
        NetReader::Tcp(stream) => stream.read(buffer).await.map(|n| match n {
            0 => Received::Closed,
            n => Received::Data(n, None),
        }),
        NetReader::Udp(socket) => socket
            .recv_from(buffer)
            .await
            .map(|(n, from)| Received::Data(n, Some(from))),
    }
}

async fn send(
    writer: &mut NetWriter,
    peer: Option<SocketAddr>,
    data: &[u8],
) -> std::io::Result<()> {
    match (writer, peer) {
        (NetWriter::Tcp(stream), _) => stream.write_all(data).await,
        (NetWriter::Udp(socket), Some(peer)) => socket.send_to(data, peer).await.map(|_| ()),
        // No one has sent anything yet to answer.
        (NetWriter::Udp(_), None) => Ok(()),
    }
}

/// A connection, or a datagram socket and whom it talks to.
struct Net {
    reader: NetReader,
    writer: NetWriter,
    /// The other side of a datagram socket: the destination, or the last sender when
    /// listening; a TCP connection knows its own.
    peer: Option<SocketAddr>,
}

impl Net {
    fn tcp(stream: TcpStream) -> Self {
        let (reader, writer) = stream.into_split();
        Self {
            reader: NetReader::Tcp(reader),
            writer: NetWriter::Tcp(writer),
            peer: None,
        }
    }

    fn udp(socket: &Arc<UdpSocket>, peer: Option<SocketAddr>) -> Self {
        Self {
            reader: NetReader::Udp(Arc::clone(socket)),
            writer: NetWriter::Udp(Arc::clone(socket)),
            peer,
        }
    }

    /// Whether anything written has somewhere to go: a TCP connection always, a datagram
    /// socket once it has a peer.
    const fn has_peer(&self) -> bool {
        matches!(self.writer, NetWriter::Tcp(_)) || self.peer.is_some()
    }
}

/// How a connection ended.
enum Ended {
    /// As connections end: the other side closed, a timer ran out, or a limit was met.
    Done,
    /// Ctrl-C, with the result the shell's handling of it came to.
    Interrupted(ExecutionResult),
}

/// Acts on a Ctrl-C as the shell acts on one between commands: the trap on `INT` runs, and
/// its result is the command's if it leaves the normal flow; without a trap the script
/// ends with 130.
async fn interrupted<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    let trap_result = context.shell.interrupt(&context.params).await?;
    if trap_result.is_normal_flow() {
        Ok(ExecutionResult::new(INTERRUPTED))
    } else {
        Ok(trap_result)
    }
}

/// What a wait came to: its value, or the Ctrl-C that ended it.
enum Waited<T> {
    Done(T),
    Interrupted(ExecutionResult),
}

/// Waits for `future`, looking for a Ctrl-C meanwhile.
async fn wait_for<SE: cash_core::ShellExtensions, T>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    stdin: &Stdin,
    future: impl Future<Output = T>,
) -> Result<Waited<T>, cash_core::Error> {
    let mut future = pin!(future);
    let mut tick = tokio::time::interval(POLL);
    loop {
        tokio::select! {
            value = &mut future => return Ok(Waited::Done(value)),
            _ = tick.tick() => {
                if stdin.ctrl_c_pending(&context.params) {
                    return Ok(Waited::Interrupted(interrupted(context).await?));
                }
            }
        }
    }
}

/// `data` as `-C` sends it: every newline not already after a carriage return as CRLF.
fn with_crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    let mut previous = 0u8;
    for &byte in data {
        if byte == b'\n' && previous != b'\r' {
            out.push(b'\r');
        }
        out.push(byte);
        previous = byte;
    }
    out
}

/// The telnet negotiations in `data` answered, as netcat's `-t` answers them: a DO with
/// WONT and a WILL with DONT, each for its option.
fn telnet_answers(data: &[u8]) -> Vec<u8> {
    const IAC: u8 = 255;
    const DO: u8 = 253;
    const WILL: u8 = 251;
    const WONT: u8 = 252;
    const DONT: u8 = 254;
    let mut answers = Vec::new();
    let mut bytes = data.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte != IAC {
            continue;
        }
        let (Some(verb), Some(option)) = (bytes.next(), bytes.next()) else {
            break;
        };
        match verb {
            DO => answers.extend_from_slice(&[IAC, WONT, option]),
            WILL => answers.extend_from_slice(&[IAC, DONT, option]),
            _ => {}
        }
    }
    answers
}

/// Carries standard input to the network and the network to standard output until the
/// connection ends. That is when the other side closes; when standard input has ended
/// and `-q`'s seconds have passed; when nothing has moved for `-w`'s seconds; or after
/// `-W` reads. Ctrl-C ends it as the shell ends a command.
#[expect(
    clippy::too_many_lines,
    reason = "netcat's readwrite loop: one select over the three ends and the timers"
)]
async fn pump<SE: cash_core::ShellExtensions, W: std::io::Write>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    options: &Options,
    stdin: &mut Stdin,
    net: &mut Net,
    stdout: &mut W,
) -> Result<Ended, cash_core::Error> {
    let mut buffer = vec![0u8; BUFFER];
    let mut tick = tokio::time::interval(POLL);
    let mut last_activity = Instant::now();
    let mut quit_at: Option<Instant> = None;
    let mut received: u64 = 0;
    let mut net_open = true;
    while net_open {
        if let Some(at) = quit_at
            && Instant::now() >= at
        {
            break;
        }
        if let Some(idle) = options.timeout
            && last_activity.elapsed() >= idle
        {
            break;
        }
        tokio::select! {
            biased;
            _ = tick.tick() => {
                if stdin.ctrl_c_pending(&context.params) {
                    return Ok(Ended::Interrupted(interrupted(context).await?));
                }
            }
            read = receive(&mut net.reader, &mut buffer) => match read {
                Ok(Received::Closed) | Err(_) => net_open = false,
                Ok(Received::Data(n, from)) => {
                    last_activity = Instant::now();
                    if let Some(from) = from
                        && net.peer != Some(from)
                    {
                        if options.listen && options.verbose {
                            writeln!(
                                context.stderr(),
                                "Connection received on {} {}",
                                from.ip(),
                                from.port()
                            )?;
                        }
                        net.peer = Some(from);
                    }
                    let data = buffer.get(..n).unwrap_or_default();
                    if stdout.write_all(data).and_then(|()| stdout.flush()).is_err() {
                        net_open = false;
                    }
                    if options.telnet {
                        let answers = telnet_answers(data);
                        if !answers.is_empty()
                            && send(&mut net.writer, net.peer, &answers).await.is_err()
                        {
                            net_open = false;
                        }
                    }
                    received += 1;
                    if options.recv_limit.is_some_and(|limit| received >= limit) {
                        net_open = false;
                    }
                }
            },
            input = stdin.recv(), if stdin.open && net.has_peer() => match input {
                Some(Input::Data(data)) => {
                    last_activity = Instant::now();
                    let data = if options.crlf { with_crlf(&data) } else { data };
                    if options.interval.is_zero() {
                        if send(&mut net.writer, net.peer, &data).await.is_err() {
                            net_open = false;
                        }
                    } else {
                        // `-i`: a line at a time, with the interval after each.
                        for line in data.split_inclusive(|&byte| byte == b'\n') {
                            if send(&mut net.writer, net.peer, line).await.is_err() {
                                net_open = false;
                                break;
                            }
                            if let Waited::Interrupted(result) =
                                wait_for(context, stdin, tokio::time::sleep(options.interval))
                                    .await?
                            {
                                return Ok(Ended::Interrupted(result));
                            }
                        }
                    }
                }
                Some(Input::Interrupted) => {
                    return Ok(Ended::Interrupted(interrupted(context).await?));
                }
                Some(Input::Eof) | None => {
                    stdin.open = false;
                    if options.shutdown_after_eof
                        && let NetWriter::Tcp(stream) = &mut net.writer
                    {
                        let _ = stream.shutdown().await;
                    }
                    if let Some(quit_after) = options.quit_after {
                        quit_at = Some(Instant::now() + quit_after);
                    }
                }
            },
        }
    }
    Ok(Ended::Done)
}

/// Why a connection could not be made.
enum Failure {
    /// The destination refused, or could not be reached: the next address is tried, and
    /// `-v` says so.
    Connect(std::io::Error),
    /// Something netcat ends the run on, with this message (after `nc: `): the local
    /// address could not be resolved or bound.
    Fatal(String),
}

/// The local address `-s` and `-p` ask a socket to bind to, for a destination of
/// `v6`'s family; none without either.
async fn local_address(options: &Options, v6: bool) -> Result<Option<SocketAddr>, Failure> {
    let port = match &options.source_port {
        Some(text) => {
            single_port(text, options.numeric).map_err(|refusal| Failure::Fatal(refusal.message))?
        }
        None => 0,
    };
    let address = match &options.source {
        Some(host) => {
            let family = Some(if v6 { Family::V6 } else { Family::V4 });
            resolve(host, options.numeric, family)
                .await
                .map_err(Failure::Fatal)?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    Failure::Fatal("getaddrinfo: Name or service not known".to_owned())
                })?
        }
        None if port == 0 => return Ok(None),
        None => any_address(v6),
    };
    Ok(Some(SocketAddr::new(address, port)))
}

/// netcat's `err(1, "bind failed")`.
fn bind_failed(error: &std::io::Error) -> Failure {
    Failure::Fatal(format!("bind failed: {}", reason(error)))
}

/// Connects to `address`, within `-w` if given, from the address `-s`/`-p` name.
async fn connect_tcp(options: &Options, address: SocketAddr) -> Result<TcpStream, Failure> {
    let socket = if address.is_ipv6() {
        TcpSocket::new_v6()
    } else {
        TcpSocket::new_v4()
    }
    .map_err(Failure::Connect)?;
    if let Some(size) = options.recv_buffer {
        socket
            .set_recv_buffer_size(size)
            .map_err(Failure::Connect)?;
    }
    if let Some(size) = options.send_buffer {
        socket
            .set_send_buffer_size(size)
            .map_err(Failure::Connect)?;
    }
    if let Some(local) = local_address(options, address.is_ipv6()).await? {
        socket.bind(local).map_err(|error| bind_failed(&error))?;
    }
    let connecting = socket.connect(address);
    let stream = match options.timeout {
        Some(limit) => tokio::time::timeout(limit, connecting)
            .await
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))
            .and_then(|connected| connected),
        None => connecting.await,
    }
    .map_err(Failure::Connect)?;
    if let Some(ttl) = options.ttl {
        stream.set_ttl(ttl).map_err(Failure::Connect)?;
    }
    Ok(stream)
}

/// A datagram socket for `address`, bound as `-s`/`-p` say, connected to it.
async fn connect_udp(options: &Options, address: SocketAddr) -> Result<UdpSocket, Failure> {
    let local = local_address(options, address.is_ipv6())
        .await?
        .unwrap_or_else(|| SocketAddr::new(any_address(address.is_ipv6()), 0));
    let socket = UdpSocket::bind(local)
        .await
        .map_err(|error| bind_failed(&error))?;
    if options.broadcast {
        socket.set_broadcast(true).map_err(Failure::Connect)?;
    }
    if let Some(ttl) = options.ttl {
        socket.set_ttl(ttl).map_err(Failure::Connect)?;
    }
    socket.connect(address).await.map_err(Failure::Connect)?;
    Ok(socket)
}

/// One connection made, to this address, or the failure.
async fn open_connection(options: &Options, address: SocketAddr) -> Result<Net, Failure> {
    if options.udp {
        let socket = Arc::new(connect_udp(options, address).await?);
        Ok(Net::udp(&socket, Some(address)))
    } else {
        connect_tcp(options, address).await.map(Net::tcp)
    }
}

/// netcat's `udptest`: a datagram port cannot be connected to, so a few bytes are sent
/// and the socket asked whether the other side refused them (an ICMP port unreachable
/// surfaces on the next receive). `false` when it did.
async fn udp_answers(socket: &UdpSocket) -> bool {
    for _ in 0..2 {
        if socket.send(b"X").await.is_err() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut byte = [0u8; 1];
    match tokio::time::timeout(Duration::from_millis(100), socket.recv(&mut byte)).await {
        Ok(Err(_)) => false,
        Ok(Ok(_)) | Err(_) => true,
    }
}

/// `Connection to HOST [(IP)] PORT port [PROTO/SERVICE] succeeded!`, netcat's `-v` line;
/// the address in parentheses when the destination was given as a name.
fn succeeded(options: &Options, host: &str, address: IpAddr, port: u16) -> String {
    let where_ = if !options.numeric && host != address.to_string() {
        format!("{host} ({address})")
    } else {
        host.to_owned()
    };
    let service = if options.numeric {
        "*"
    } else {
        service_name(port)
    };
    format!(
        "Connection to {where_} {port} port [{}/{service}] succeeded!",
        options.proto()
    )
}

/// The connecting form: each port of the list in turn, each address of the destination
/// until one answers; `-z` only reports. Status 0 when the last port tried was reached.
async fn connect<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    options: &Options,
    stdin: &mut Stdin,
) -> Result<ExecutionResult, cash_core::Error> {
    let host = options.host.clone().unwrap_or_default();
    let ports = match build_ports(&options.port, options.numeric, options.random) {
        Ok(ports) => ports,
        Err(refusal) => {
            writeln!(context.stderr(), "nc: {}", refusal.message)?;
            return Ok(ExecutionResult::general_error());
        }
    };
    let addresses = match resolve(&host, options.numeric, options.family).await {
        Ok(addresses) => addresses,
        Err(message) => {
            writeln!(context.stderr(), "nc: {message}")?;
            return Ok(ExecutionResult::general_error());
        }
    };
    let mut stdout = context.stdout();
    let mut status = ExecutionResult::general_error();
    for (index, &port) in ports.iter().enumerate() {
        if index > 0
            && !options.interval.is_zero()
            && let Waited::Interrupted(result) =
                wait_for(context, stdin, tokio::time::sleep(options.interval)).await?
        {
            return Ok(result);
        }
        let mut connected: Option<(Net, IpAddr)> = None;
        for &address in &addresses {
            let target = SocketAddr::new(address, port);
            match wait_for(context, stdin, open_connection(options, target)).await? {
                Waited::Interrupted(result) => return Ok(result),
                Waited::Done(Ok(net)) => {
                    connected = Some((net, address));
                    break;
                }
                Waited::Done(Err(Failure::Fatal(message))) => {
                    writeln!(context.stderr(), "nc: {message}")?;
                    return Ok(ExecutionResult::general_error());
                }
                Waited::Done(Err(Failure::Connect(error))) => {
                    if options.verbose {
                        writeln!(
                            context.stderr(),
                            "nc: connect to {host} port {port} ({}) failed: {}",
                            options.proto(),
                            reason(&error)
                        )?;
                    }
                }
            }
        }
        let Some((mut net, address)) = connected else {
            continue;
        };
        status = ExecutionResult::success();
        if options.scan
            && options.udp
            && let NetReader::Udp(socket) = &net.reader
            && !udp_answers(socket).await
        {
            status = ExecutionResult::general_error();
            continue;
        }
        if options.verbose {
            writeln!(
                context.stderr(),
                "{}",
                succeeded(options, &host, address, port)
            )?;
        }
        if options.scan {
            continue;
        }
        if let Ended::Interrupted(result) =
            pump(context, options, stdin, &mut net, &mut stdout).await?
        {
            return Ok(result);
        }
    }
    Ok(status)
}

/// The listening form: one connection, or one after another with `-k`, each carried to
/// its end; with `-u`, datagrams from anyone, answered to the last sender.
async fn listen<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    options: &Options,
    stdin: &mut Stdin,
) -> Result<ExecutionResult, cash_core::Error> {
    let port = match single_port(&options.port, options.numeric) {
        Ok(port) => port,
        Err(refusal) => {
            writeln!(context.stderr(), "nc: {}", refusal.message)?;
            return Ok(ExecutionResult::general_error());
        }
    };
    let address = match &options.host {
        Some(host) => match resolve(host, options.numeric, options.family).await {
            Ok(addresses) => addresses
                .first()
                .copied()
                .unwrap_or_else(|| any_address(options.family == Some(Family::V6))),
            Err(message) => {
                writeln!(context.stderr(), "nc: {message}")?;
                return Ok(ExecutionResult::general_error());
            }
        },
        None => any_address(options.family == Some(Family::V6)),
    };
    let local = SocketAddr::new(address, port);
    if options.udp {
        return listen_udp(context, options, stdin, local).await;
    }
    let mut stdout = context.stdout();
    let listener = match TcpListener::bind(local).await {
        Ok(listener) => listener,
        Err(error) => {
            writeln!(context.stderr(), "nc: {}", reason(&error))?;
            return Ok(ExecutionResult::general_error());
        }
    };
    if let Some(ttl) = options.ttl {
        let _ = listener.set_ttl(ttl);
    }
    if options.verbose {
        let bound = listener.local_addr().unwrap_or(local);
        writeln!(
            context.stderr(),
            "Listening on {} {}",
            bound.ip(),
            bound.port()
        )?;
    }
    loop {
        let (stream, peer) = match wait_for(context, stdin, listener.accept()).await? {
            Waited::Interrupted(result) => return Ok(result),
            Waited::Done(Ok(accepted)) => accepted,
            Waited::Done(Err(error)) => {
                writeln!(context.stderr(), "nc: accept: {}", reason(&error))?;
                return Ok(ExecutionResult::general_error());
            }
        };
        if options.verbose {
            writeln!(
                context.stderr(),
                "Connection received on {} {}",
                peer.ip(),
                peer.port()
            )?;
        }
        let mut net = Net::tcp(stream);
        if let Ended::Interrupted(result) =
            pump(context, options, stdin, &mut net, &mut stdout).await?
        {
            return Ok(result);
        }
        if !options.keep {
            return Ok(ExecutionResult::success());
        }
    }
}

/// `-u -l`: datagrams from anyone on `local`, answered to the last sender; `-k` starts
/// over once a run ends (on `-q` or `-w`, since a datagram socket is never closed by the
/// other side).
async fn listen_udp<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    options: &Options,
    stdin: &mut Stdin,
    local: SocketAddr,
) -> Result<ExecutionResult, cash_core::Error> {
    let socket = match UdpSocket::bind(local).await {
        Ok(socket) => Arc::new(socket),
        Err(error) => {
            writeln!(context.stderr(), "nc: {}", reason(&error))?;
            return Ok(ExecutionResult::general_error());
        }
    };
    if options.broadcast {
        let _ = socket.set_broadcast(true);
    }
    if let Some(ttl) = options.ttl {
        let _ = socket.set_ttl(ttl);
    }
    if options.verbose {
        let bound = socket.local_addr().unwrap_or(local);
        writeln!(context.stderr(), "Bound on {} {}", bound.ip(), bound.port())?;
    }
    let mut stdout = context.stdout();
    loop {
        let mut net = Net::udp(&socket, None);
        if let Ended::Interrupted(result) =
            pump(context, options, stdin, &mut net, &mut stdout).await?
        {
            return Ok(result);
        }
        if !options.keep {
            return Ok(ExecutionResult::success());
        }
    }
}

impl builtins::Command for NcCommand {
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
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let options = match parse(&self.args) {
            Ok(Parsed::Help) => {
                write!(context.stdout(), "{VERSION}\n{USAGE}{SUMMARY}")?;
                return Ok(ExecutionResult::success());
            }
            Ok(Parsed::Run(options)) => options,
            Err(refusal) => {
                let mut stderr = context.stderr();
                if !refusal.message.is_empty() {
                    writeln!(stderr, "nc: {}", refusal.message)?;
                }
                if refusal.usage {
                    write!(stderr, "{USAGE}")?;
                }
                return Ok(ExecutionResult::general_error());
            }
        };
        let input = context.params.try_stdin(context.shell);
        let mut stdin = Stdin::start(&options, input);
        let result = if options.listen {
            listen(&mut context, &options, &mut stdin).await
        } else {
            connect(&mut context, &options, &mut stdin).await
        };
        stdin.finish().await;
        result
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::unwrap_used,
    reason = "a test that meets what it did not expect should stop loudly"
)]
mod tests {
    use std::time::Duration;

    use cash_core::net::SERVICES;

    use super::{
        Family, Options, Parsed, Refusal, build_ports, option_kind, parse, service_name,
        service_port, single_port, strtonum, succeeded, telnet_answers, valid_tos, with_crlf,
    };

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn parsed(line: &str) -> Options {
        match parse(&args(line)) {
            Ok(Parsed::Run(options)) => *options,
            other => panic!("{line}: {other:?}"),
        }
    }

    fn refused(line: &str) -> Refusal {
        parse(&args(line))
            .err()
            .unwrap_or_else(|| panic!("{line} was accepted"))
    }

    fn usage(message: &str) -> Refusal {
        Refusal {
            message: message.to_owned(),
            usage: true,
        }
    }

    fn message(message: &str) -> Refusal {
        Refusal {
            message: message.to_owned(),
            usage: false,
        }
    }

    #[test]
    fn the_destination_and_the_port() {
        let options = parsed("example.org 80");
        assert_eq!(options.host.as_deref(), Some("example.org"));
        assert_eq!(options.port, "80");
        assert!(!options.listen);
        // GNU getopt permutes: options after the operands count.
        assert!(parsed("example.org 80 -v").verbose);
        assert_eq!(parsed("-- -host 80").host.as_deref(), Some("-host"));
        // One operand is a port only when listening.
        assert_eq!(refused("80"), usage(""));
        assert_eq!(refused(""), usage(""));
        assert_eq!(refused("a b c"), usage(""));
        let listening = parsed("-l 8080");
        assert!(listening.listen && listening.host.is_none());
        assert_eq!(listening.port, "8080");
        assert_eq!(
            parsed("-l 127.0.0.1 8080").host.as_deref(),
            Some("127.0.0.1")
        );
        // Debian's `-l -p PORT`.
        assert_eq!(parsed("-l -p 8080").port, "8080");
        let with_host = parsed("-lp 8080 127.0.0.1");
        assert_eq!(with_host.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(with_host.port, "8080");
    }

    #[test]
    fn the_flags() {
        let options = parsed("-46bCDdkNnrtuv -l 1");
        assert_eq!(options.family, Some(Family::V6));
        assert!(options.broadcast && options.crlf && options.no_stdin && options.keep);
        assert!(options.shutdown_after_eof && options.numeric && options.random);
        assert!(options.telnet && options.udp && options.verbose && !options.scan);
        assert!(parsed("-z h 1").scan);
        assert_eq!(parsed("-4 h 1").family, Some(Family::V4));
        assert_eq!(parsed("h 1").family, None);
    }

    #[test]
    fn the_values() {
        let options = parsed("-w 3 -i 2 -M 64 -W 5 -I 1024 -O 2048 -p 4000 -s 127.0.0.1 -q 1 h 1");
        assert_eq!(options.timeout, Some(Duration::from_secs(3)));
        assert_eq!(options.interval, Duration::from_secs(2));
        assert_eq!(options.ttl, Some(64));
        assert_eq!(options.recv_limit, Some(5));
        assert_eq!(options.recv_buffer, Some(1024));
        assert_eq!(options.send_buffer, Some(2048));
        assert_eq!(options.source_port.as_deref(), Some("4000"));
        assert_eq!(options.source.as_deref(), Some("127.0.0.1"));
        assert_eq!(options.quit_after, Some(Duration::from_secs(1)));
        // A non-negative -q implies -N; a negative one waits forever.
        assert!(options.shutdown_after_eof);
        let forever = parsed("-q -1 h 1");
        assert_eq!(forever.quit_after, None);
        assert!(!forever.shutdown_after_eof);
        assert_eq!(parsed("-w3 h 1").timeout, Some(Duration::from_secs(3)));
        assert_eq!(parsed("-vw 3 h 1").timeout, Some(Duration::from_secs(3)));
        assert_eq!(parsed("-T lowdelay -D h 1").port, "1");
        assert_eq!(parsed("-T 0x10 h 1").port, "1");
        assert_eq!(parsed("-T 7 h 1").port, "1");
    }

    #[test]
    fn what_getopt_refuses() {
        assert_eq!(refused("-e cmd h 1"), usage("invalid option -- 'e'"));
        assert_eq!(refused("-c cmd h 1"), usage("invalid option -- 'c'"));
        assert_eq!(refused("-Q h 1"), usage("invalid option -- 'Q'"));
        assert_eq!(
            refused("h 1 -w"),
            usage("option requires an argument -- 'w'")
        );
        assert_eq!(refused("-V"), usage("option requires an argument -- 'V'"));
        assert_eq!(parse(&args("-h")), Ok(Parsed::Help));
        assert_eq!(parse(&args("-lh 1 extra junk")), Ok(Parsed::Help));
    }

    #[test]
    fn what_the_values_refuse() {
        assert_eq!(refused("-w abc h 1"), message("timeout invalid: abc"));
        assert_eq!(refused("-w -1 h 1"), message("timeout too small: -1"));
        assert_eq!(
            refused("-w 99999999 h 1"),
            message("timeout too large: 99999999")
        );
        assert_eq!(refused("-i x h 1"), message("interval invalid: x"));
        assert_eq!(refused("-M 256 h 1"), message("ttl is too large"));
        assert_eq!(refused("-M x h 1"), message("ttl is invalid"));
        assert_eq!(refused("-m 300 h 1"), message("minttl is too large"));
        assert_eq!(refused("-q x h 1"), message("quit timer invalid: x"));
        assert_eq!(refused("-W 0 h 1"), message("receive limit too small: 0"));
        assert_eq!(
            refused("-I 0 h 1"),
            message("TCP receive window too small: 0")
        );
        assert_eq!(refused("-O x h 1"), message("TCP send window invalid: x"));
        assert_eq!(refused("-T bogus h 1"), message("illegal tos value bogus"));
        assert_eq!(refused("-T 256 h 1"), message("illegal tos value 256"));
    }

    #[test]
    fn what_windows_refuses() {
        for (line, letter) in [
            ("-F h 1", "-F"),
            ("-m 5 h 1", "-m"),
            ("-P user h 1", "-x, -X and -P"),
            ("-X 5 h 1", "-x, -X and -P"),
            ("-x proxy:1080 h 1", "-x, -X and -P"),
            ("-S h 1", "-S"),
            ("-U /tmp/sock", "-U"),
            ("-V 1 h 1", "-V"),
            ("-Z h 1", "-Z"),
        ] {
            let refusal = refused(line);
            assert!(!refusal.usage, "{line}");
            assert!(
                refusal.message.starts_with(letter),
                "{line}: {}",
                refusal.message
            );
        }
    }

    #[test]
    fn what_the_combinations_refuse() {
        assert_eq!(refused("-lz 1"), message("cannot use -z and -l"));
        assert_eq!(
            refused("-l -s 127.0.0.1 1"),
            message("cannot use -s and -l")
        );
        assert_eq!(refused("-k h 1"), message("must use -l with -k"));
        let _ = parsed("-lk 1");
        let _ = parsed("-l -p 1 -N 2");
    }

    #[test]
    fn every_getopt_letter_is_known() {
        for letter in "46bCDdFhIiklMmNnOPpqrSsTtUuVvWwXxZz".chars() {
            assert!(option_kind(letter).is_some(), "{letter}");
        }
        assert_eq!(option_kind('w'), Some(true));
        assert_eq!(option_kind('v'), Some(false));
        assert_eq!(option_kind('e'), None);
    }

    #[test]
    fn strtonum_speaks_as_openbsd_does() {
        assert_eq!(strtonum("5", 1, 10), Ok(5));
        assert_eq!(strtonum("+5", 1, 10), Ok(5));
        assert_eq!(strtonum("0", 1, 10), Err("too small"));
        assert_eq!(strtonum("11", 1, 10), Err("too large"));
        assert_eq!(strtonum("x", 1, 10), Err("invalid"));
        assert_eq!(strtonum(" 5", 1, 10), Err("invalid"));
        assert_eq!(strtonum("", 1, 10), Err("invalid"));
        assert!(valid_tos("af11") && valid_tos("cs7") && valid_tos("0xff") && valid_tos("255"));
        assert!(!valid_tos("0x100") && !valid_tos("-1") && !valid_tos("fast"));
    }

    #[test]
    fn ports_and_ranges() {
        assert_eq!(build_ports("80", false, false), Ok(vec![80]));
        assert_eq!(build_ports("http", false, false), Ok(vec![80]));
        assert_eq!(build_ports("80-83", false, false), Ok(vec![80, 81, 82, 83]));
        assert_eq!(build_ports("83-80", false, false), Ok(vec![80, 81, 82, 83]));
        let mut random = build_ports("1-50", false, true).unwrap_or_default();
        random.sort_unstable();
        assert_eq!(random, (1..=50).collect::<Vec<u16>>());
        assert_eq!(
            build_ports("http", true, false),
            Err(message("port number invalid: http"))
        );
        assert_eq!(
            build_ports("0", false, false),
            Err(message("port number too small: 0"))
        );
        assert_eq!(
            build_ports("70000", false, false),
            Err(message("port number too large: 70000"))
        );
        assert_eq!(
            build_ports("80-x", false, false),
            Err(message("port number invalid: x"))
        );
        assert_eq!(single_port("ssh", false), Ok(22));
        assert_eq!(single_port("22", true), Ok(22));
        assert_eq!(
            single_port("80-81", false),
            Err(message(
                "getaddrinfo: Servname not supported for ai_socktype"
            ))
        );
        assert_eq!(
            single_port("ssh", true),
            Err(message(
                "getaddrinfo: Servname not supported for ai_socktype"
            ))
        );
    }

    #[test]
    fn the_service_table() {
        assert_eq!(service_port("https"), Some(443));
        assert_eq!(service_port("nothing"), None);
        assert_eq!(service_name(22), "ssh");
        assert_eq!(service_name(53), "domain");
        assert_eq!(service_name(49152), "*");
        // Every entry names one port, and no name twice.
        let mut names: Vec<&str> = SERVICES.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SERVICES.len());
    }

    #[test]
    fn the_succeeded_line() {
        let options = parsed("localhost 80");
        assert_eq!(
            succeeded(
                &options,
                "localhost",
                "127.0.0.1".parse().unwrap_or_else(|_| unreachable!()),
                80
            ),
            "Connection to localhost (127.0.0.1) 80 port [tcp/http] succeeded!"
        );
        let numeric = parsed("-n 127.0.0.1 80");
        assert_eq!(
            succeeded(
                &numeric,
                "127.0.0.1",
                "127.0.0.1".parse().unwrap_or_else(|_| unreachable!()),
                80
            ),
            "Connection to 127.0.0.1 80 port [tcp/*] succeeded!"
        );
        let udp = parsed("-u 127.0.0.1 53");
        assert_eq!(
            succeeded(
                &udp,
                "127.0.0.1",
                "127.0.0.1".parse().unwrap_or_else(|_| unreachable!()),
                53
            ),
            "Connection to 127.0.0.1 53 port [udp/domain] succeeded!"
        );
    }

    #[test]
    fn crlf_and_telnet() {
        assert_eq!(with_crlf(b"a\nb\r\nc\n"), b"a\r\nb\r\nc\r\n");
        assert_eq!(with_crlf(b"no newline"), b"no newline");
        // DO echo (1) is answered WONT echo; WILL suppress-go-ahead (3) DONT.
        assert_eq!(
            telnet_answers(&[b'x', 255, 253, 1, b'y', 255, 251, 3]),
            vec![255, 252, 1, 255, 254, 3]
        );
        assert_eq!(telnet_answers(b"plain"), Vec::<u8>::new());
        assert_eq!(telnet_answers(&[255, 253]), Vec::<u8>::new());
    }
}
