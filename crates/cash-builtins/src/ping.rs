//! `ping` with Linux's flags, over the Windows ICMP helper API.
//!
//! `ping -c 1 host && …` is common in scripts, and Windows' `ping.exe` reads `-c` as a
//! routing compartment: unelevated it refuses with "Access denied", so the test reports
//! every host as down. This `ping` takes iputils' flags as they are — `-c` count, `-i`
//! interval, `-W` reply timeout, `-w` deadline, `-s` size, `-t` TTL, `-n` numeric output
//! — and prints iputils' lines, checked against iputils 20250605. `ping.exe` is not
//! reached by the name `ping` any more; by its path, or after `enable -n ping`, it is.
//!
//! It runs as a bundled command (`cash --invoke-bundled ping`), a process of its own in
//! the foreground job, so Ctrl-C reaches it and it prints its statistics before exiting,
//! as iputils does. Echoes go through `IcmpSendEcho2`, since Windows gives unprivileged
//! programs no raw sockets, so the options that need one (flood, record route, source
//! routing, a padding pattern, marks) are refused by name.

use std::ffi::OsString;
use std::io::Write as _;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cash_win32::icmp::{Outcome, Pinger};

/// The most data bytes Windows sends in one echo; iputils allows 65507.
const WINDOWS_MAX_SIZE: u32 = 65500;

/// How long the last echo waits for its reply without `-W`, as iputils lingers.
const LINGER: Duration = Duration::from_secs(10);

/// What the command line asked for.
struct Options {
    count: Option<u64>,
    interval: Duration,
    timeout: Option<Duration>,
    deadline: Option<Duration>,
    size: u16,
    ttl: Option<u8>,
    quiet: bool,
    numeric: bool,
    reverse: bool,
    family: Option<Family>,
    timestamps: bool,
    outstanding: bool,
    audible: bool,
    destination: String,
}

impl Default for Options {
    /// iputils' defaults: one second apart, 56 data bytes, until interrupted.
    fn default() -> Self {
        Self {
            count: None,
            interval: Duration::from_secs(1),
            timeout: None,
            deadline: None,
            size: 56,
            ttl: None,
            quiet: false,
            numeric: false,
            reverse: false,
            family: None,
            timestamps: false,
            outstanding: false,
            audible: false,
            destination: String::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    V4,
    V6,
}

/// A failure before any echo was sent: its message and exit status.
struct Stop(String, i32);

impl Stop {
    fn usage(message: impl Into<String>) -> Self {
        Self(message.into(), 2)
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self(message.into(), 1)
    }
}

/// Runs `ping` with `args` (the first being the program name) and returns its status.
pub fn run_ping(args: Vec<OsString>) -> i32 {
    let args: Vec<String> = args
        .into_iter()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    match parse(&args).and_then(|options| resolve(&options).map(|target| (options, target))) {
        Ok((options, target)) => ping(&options, &target),
        Err(Stop(message, status)) => {
            if !message.is_empty() {
                eprintln!("{message}");
            }
            status
        }
    }
}

const USAGE: &str = "\
Usage
  ping [options] <destination>

Options:
  <destination>      DNS name or IP address
  -a                 use audible ping
  -c <count>         stop after <count> replies
  -D                 print timestamps
  -h                 print help and exit
  -H                 force reverse DNS name resolution, override -n
  -i <interval>      seconds between sending each packet
  -n                 no reverse DNS name resolution, override -H
  -O                 report outstanding replies
  -q                 quiet output
  -s <size>          use <size> as number of data bytes to be sent
  -t <ttl>           define time to live
  -v                 verbose output
  -V                 print version and exit
  -w <deadline>      reply wait <deadline> in seconds
  -W <timeout>       time to wait for response
  -4                 use IPv4
  -6                 use IPv6

Windows' ping.exe takes other flags (-n is its count); reach it by its path.";

/// Options iputils has that need a raw socket or a Linux socket option.
const fn refusal(flag: char) -> Option<&'static str> {
    Some(match flag {
        'f' | 'l' => "flooding and preloading need a raw socket",
        'I' | 'B' => "choosing the source interface is not supported",
        'p' => "a padding pattern needs a raw socket",
        'R' | 'T' => "IP options (record route, timestamps) need a raw socket",
        'M' | 'Q' | 'S' | 'm' | 'L' | 'b' | 'C' | 'd' | 'e' | 'U' | 'A' | '3' | 'r' => {
            "this option needs a Linux socket option Windows does not have"
        }
        _ => return None,
    })
}

fn parse(args: &[String]) -> Result<Options, Stop> {
    let mut options = Options::default();
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        index += 1;
        let Some(cluster) = arg.strip_prefix('-').filter(|c| !c.is_empty()) else {
            operands.push(arg);
            continue;
        };
        if cluster == "-" {
            operands.extend(args.iter().skip(index).map(String::as_str));
            break;
        }
        for (at, flag) in cluster.char_indices() {
            let rest = cluster.get(at + flag.len_utf8()..).unwrap_or_default();
            let mut value = || -> Result<String, Stop> {
                if rest.is_empty() {
                    index += 1;
                    args.get(index - 1).cloned().ok_or_else(|| {
                        Stop::usage(format!(
                            "ping: option requires an argument -- '{flag}'\n\n{USAGE}"
                        ))
                    })
                } else {
                    Ok(rest.to_owned())
                }
            };
            match flag {
                'c' => {
                    options.count = Some(positive(&value()?)?);
                    break;
                }
                'i' => {
                    options.interval = interval(&value()?)?;
                    break;
                }
                'W' => {
                    options.timeout = Some(seconds(&value()?)?);
                    break;
                }
                'w' => {
                    options.deadline = Some(seconds(&value()?)?);
                    break;
                }
                's' => {
                    options.size = size(&value()?)?;
                    break;
                }
                't' => {
                    options.ttl = Some(ttl(&value()?)?);
                    break;
                }
                'q' => options.quiet = true,
                'n' => {
                    options.numeric = true;
                    options.reverse = false;
                }
                'H' => {
                    options.reverse = true;
                    options.numeric = false;
                }
                '4' => options.family = Some(Family::V4),
                '6' => options.family = Some(Family::V6),
                'D' => options.timestamps = true,
                'O' => options.outstanding = true,
                'a' => options.audible = true,
                'v' => {}
                'h' => {
                    println!("{USAGE}");
                    return Err(Stop(String::new(), 0));
                }
                'V' => {
                    println!("ping (cash), with iputils 20250605's flags, over Windows ICMP");
                    return Err(Stop(String::new(), 0));
                }
                other => {
                    return Err(match refusal(other) {
                        Some(reason) => {
                            Stop::usage(format!("ping: -{other} is not supported: {reason}"))
                        }
                        None => {
                            Stop::usage(format!("ping: invalid option -- '{other}'\n\n{USAGE}"))
                        }
                    });
                }
            }
        }
    }
    options.destination = destination(&operands, options.numeric)?;
    Ok(options)
}

/// The one destination; iputils treats earlier operands as hops to route through.
fn destination(operands: &[&str], numeric: bool) -> Result<String, Stop> {
    match operands {
        [] => Err(Stop::usage(
            "ping: usage error: Destination address required",
        )),
        [destination] => Ok((*destination).to_owned()),
        [first, ..] => {
            let mut message = String::from(
                "ping: more than one destination: routing through intermediate hosts is not supported",
            );
            // `ping -n 3 host` is ping.exe's "three echoes"; here -n is numeric output
            // and 3 would be a hop, so say what was probably meant.
            if numeric && first.parse::<u64>().is_ok() {
                message.push_str("\nping: hint: ping.exe's `-n ");
                message.push_str(first);
                message.push_str("` is `-c ");
                message.push_str(first);
                message.push_str("` here (-n means numeric output)");
            }
            Err(Stop::usage(message))
        }
    }
}

fn positive(text: &str) -> Result<u64, Stop> {
    match text.parse::<i64>() {
        Ok(n) if n >= 1 => Ok(n.unsigned_abs()),
        Ok(_) => Err(Stop::invalid(format!(
            "ping: invalid argument: '{text}': out of range: 1 <= value <= 9223372036854775807"
        ))),
        Err(_) => Err(Stop::invalid(format!("ping: invalid argument: '{text}'"))),
    }
}

fn seconds(text: &str) -> Result<Duration, Stop> {
    text.parse::<f64>()
        .ok()
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(Duration::from_secs_f64)
        .ok_or_else(|| Stop::invalid(format!("ping: invalid argument: '{text}'")))
}

fn interval(text: &str) -> Result<Duration, Stop> {
    let value = seconds(text)?;
    if value < Duration::from_millis(2) {
        return Err(Stop::usage(
            "ping: cannot flood, minimal interval for user must be >= 2 ms, use -i 0.002 (or higher)",
        ));
    }
    Ok(value)
}

fn size(text: &str) -> Result<u16, Stop> {
    let value: u32 = text.parse().ok().filter(|v| *v <= 65507).ok_or_else(|| {
        Stop::invalid(format!(
            "ping: invalid -s value: '{text}': out of range: 0 <= value <= 65507"
        ))
    })?;
    if value > WINDOWS_MAX_SIZE {
        return Err(Stop::usage(format!(
            "ping: -s {value}: Windows sends at most {WINDOWS_MAX_SIZE} data bytes"
        )));
    }
    Ok(u16::try_from(value).unwrap_or(u16::MAX))
}

fn ttl(text: &str) -> Result<u8, Stop> {
    match text.parse::<u32>() {
        Ok(v) if (1..=255).contains(&v) => Ok(u8::try_from(v).unwrap_or(u8::MAX)),
        Ok(_) => Err(Stop::usage(
            "ping: cannot set unicast time-to-live: Invalid argument",
        )),
        Err(_) => Err(Stop::invalid(format!("ping: invalid argument: '{text}'"))),
    }
}

/// The destination and whether it was given as an address.
struct Target {
    address: IpAddr,
    literal: bool,
}

fn resolve(options: &Options) -> Result<Target, Stop> {
    let wanted = |address: &IpAddr| match options.family {
        Some(Family::V4) => address.is_ipv4(),
        Some(Family::V6) => address.is_ipv6(),
        None => true,
    };
    let unsupported = || {
        Stop::usage(format!(
            "ping: {}: Address family for hostname not supported",
            options.destination
        ))
    };
    if let Ok(address) = options.destination.parse::<IpAddr>() {
        return if wanted(&address) {
            Ok(Target {
                address,
                literal: true,
            })
        } else {
            Err(unsupported())
        };
    }
    let found: Vec<IpAddr> = (options.destination.as_str(), 0)
        .to_socket_addrs()
        .map(|addrs| addrs.map(|a| a.ip()).collect())
        .unwrap_or_default();
    if found.is_empty() {
        return Err(Stop::usage(format!(
            "ping: {}: Name or service not known",
            options.destination
        )));
    }
    found
        .into_iter()
        .find(wanted)
        .map(|address| Target {
            address,
            literal: false,
        })
        .ok_or_else(unsupported)
}

/// What the run has seen, for the summary.
#[derive(Default)]
struct Tally {
    transmitted: u64,
    received: u64,
    errors: u64,
    times: Vec<f64>,
}

fn ping(options: &Options, target: &Target) -> i32 {
    let pinger = match Pinger::new(target.address.is_ipv6()) {
        Ok(pinger) => pinger,
        Err(error) => {
            eprintln!("ping: cannot open an ICMP handle: {error}");
            return 2;
        }
    };
    let interrupted = cash_win32::icmp::catch_interrupts();
    let name_replies = options.reverse || (!options.numeric && !target.literal);

    if target.address.is_ipv6() {
        println!(
            "PING {} ({}) {} data bytes",
            options.destination, target.address, options.size
        );
    } else {
        println!(
            "PING {} ({}) {}({}) bytes of data.",
            options.destination,
            target.address,
            options.size,
            u32::from(options.size) + 28
        );
    }

    let mut tally = Tally::default();
    let started = Instant::now();
    let mut last_sent = started;
    let mut seq: u64 = 0;
    while !interrupted.load(Ordering::SeqCst) {
        let deadline_left = options
            .deadline
            .map(|d| d.saturating_sub(started.elapsed()));
        if deadline_left == Some(Duration::ZERO) {
            break;
        }
        if options.count.is_some_and(|c| {
            if options.deadline.is_some() {
                tally.received >= c
            } else {
                tally.transmitted >= c
            }
        }) {
            break;
        }
        seq += 1;
        let last = options.deadline.is_none() && options.count == Some(seq);
        // One echo is in flight at a time, so waiting longer than the interval would
        // push every later send back; iputils sends on schedule. So only the last echo
        // waits the full -W (or iputils' linger); a reply slower than the interval to
        // an earlier one counts as lost.
        let mut wait = if last {
            options.timeout.unwrap_or(LINGER)
        } else {
            options
                .timeout
                .map_or(options.interval, |t| t.min(options.interval))
        };
        if let Some(left) = deadline_left {
            wait = wait.min(left);
        }
        let sent_at = Instant::now();
        last_sent = sent_at;
        tally.transmitted += 1;
        let outcome = pinger.echo(target.address, options.size, options.ttl, wait, interrupted);
        if !report(options, &mut tally, seq, last, name_replies, outcome) {
            break;
        }
        let more = !last && !interrupted.load(Ordering::SeqCst);
        if more {
            pause(
                options.interval.saturating_sub(sent_at.elapsed()),
                interrupted,
            );
        }
    }

    summary(options, &tally, last_sent.duration_since(started));
    let short_of_count =
        options.deadline.is_some() && options.count.is_some_and(|c| tally.received < c);
    if tally.received == 0 || short_of_count {
        1
    } else {
        0
    }
}

/// Prints what one echo produced; `false` when the run should stop.
fn report(
    options: &Options,
    tally: &mut Tally,
    seq: u64,
    last: bool,
    name_replies: bool,
    outcome: std::io::Result<Outcome>,
) -> bool {
    let prefix = if options.timestamps {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        format!("[{}.{:06}] ", now.as_secs(), now.subsec_micros())
    } else {
        String::new()
    };
    match outcome {
        Ok(Outcome::Reply {
            from,
            ttl,
            bytes,
            time,
        }) => {
            tally.received += 1;
            let ms = time.as_secs_f64() * 1000.0;
            tally.times.push(ms);
            if !options.quiet {
                let who = if name_replies {
                    let name =
                        cash_win32::icmp::host_name(from).unwrap_or_else(|| from.to_string());
                    format!("{name} ({from})")
                } else {
                    from.to_string()
                };
                let ttl = ttl.map(|t| format!(" ttl={t}")).unwrap_or_default();
                let bell = if options.audible { "\x07" } else { "" };
                println!(
                    "{bell}{prefix}{bytes} bytes from {who}: icmp_seq={seq}{ttl} time={} ms",
                    round_trip(ms)
                );
            }
            true
        }
        Ok(Outcome::TimedOut) => {
            // iputils says this as it sends the next echo, so never for the last one.
            if options.outstanding && !last && !options.quiet {
                println!("{prefix}no answer yet for icmp_seq={seq}");
            }
            true
        }
        Ok(Outcome::Error { from, message }) => {
            tally.errors += 1;
            if !options.quiet {
                println!("{prefix}From {from} icmp_seq={seq} {message}");
            }
            true
        }
        Ok(Outcome::Cancelled) => {
            // The echo was abandoned, so it was never really sent.
            tally.transmitted -= 1;
            false
        }
        Err(error) => {
            eprintln!("ping: sendmsg: {error}");
            false
        }
    }
}

/// Sleeps for `span`, waking early on Ctrl-C.
fn pause(span: Duration, interrupted: &AtomicBool) {
    let until = Instant::now() + span;
    while !interrupted.load(Ordering::SeqCst) {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}

/// A round trip as iputils prints it: three significant digits past the point below
/// 100 ms, fewer above.
fn round_trip(ms: f64) -> String {
    if ms >= 99.95 {
        format!("{ms:.0}")
    } else if ms >= 9.995 {
        format!("{ms:.1}")
    } else if ms >= 0.9995 {
        format!("{ms:.2}")
    } else {
        format!("{ms:.3}")
    }
}

/// A count as a float, for statistics; packet counts stay far below 2^52.
#[expect(
    clippy::cast_precision_loss,
    reason = "packet counts and sample counts never approach 2^52"
)]
const fn as_f64(n: u64) -> f64 {
    n as f64
}

/// `lost` of `sent` as a percentage, as C's `%g` prints it: six significant digits,
/// trailing zeros dropped (`33.3333`, `50`).
fn percent(lost: u64, sent: u64) -> String {
    if sent == 0 || (lost * 100).is_multiple_of(sent) {
        return (lost * 100).checked_div(sent).unwrap_or(0).to_string();
    }
    let value = as_f64(lost) * 100.0 / as_f64(sent);
    let decimals = if value >= 10.0 {
        4
    } else if value >= 1.0 {
        5
    } else {
        // Below 1, the leading zeros after the point are not significant.
        let mut zeros = 0;
        let mut scaled = lost * 1000;
        while scaled < sent {
            zeros += 1;
            scaled *= 10;
        }
        6 + zeros
    };
    let text = format!("{value:.decimals$}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn summary(options: &Options, tally: &Tally, elapsed: Duration) {
    println!();
    println!("--- {} ping statistics ---", options.destination);
    let lost = tally.transmitted - tally.received.min(tally.transmitted);
    let errors = if tally.errors > 0 {
        format!(", +{} errors", tally.errors)
    } else {
        String::new()
    };
    println!(
        "{} packets transmitted, {} received{errors}, {}% packet loss, time {}ms",
        tally.transmitted,
        tally.received,
        percent(lost, tally.transmitted),
        elapsed.as_millis()
    );
    if tally.times.is_empty() {
        println!();
    } else {
        let n = as_f64(u64::try_from(tally.times.len()).unwrap_or(u64::MAX));
        let min = tally.times.iter().copied().fold(f64::INFINITY, f64::min);
        let max = tally.times.iter().copied().fold(0.0, f64::max);
        let avg = tally.times.iter().sum::<f64>() / n;
        let mean_square = tally.times.iter().map(|t| t * t).sum::<f64>() / n;
        let mdev = (-avg).mul_add(avg, mean_square).max(0.0).sqrt();
        println!("rtt min/avg/max/mdev = {min:.3}/{avg:.3}/{max:.3}/{mdev:.3} ms");
    }
    let _ = std::io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn error(list: &[&str]) -> (String, i32) {
        // A parse that succeeds reads as an empty message and status -1, which fails
        // whatever the caller expected.
        parse(&args(list)).map_or_else(
            |Stop(message, status)| (message, status),
            |_| (String::new(), -1),
        )
    }

    #[test]
    fn linux_flags_mean_what_they_mean_on_linux() {
        let options = parse(&args(&[
            "-c", "3", "-i0.2", "-W", "1", "-s", "100", "-t", "5", "-qn", "host",
        ]))
        .map_err(|Stop(message, _)| message)
        .unwrap();
        assert_eq!(options.count, Some(3));
        assert_eq!(options.interval, Duration::from_millis(200));
        assert_eq!(options.timeout, Some(Duration::from_secs(1)));
        assert_eq!(options.size, 100);
        assert_eq!(options.ttl, Some(5));
        assert!(options.quiet && options.numeric);
        assert_eq!(options.destination, "host");
    }

    #[test]
    fn errors_match_iputils() {
        assert_eq!(
            error(&[]),
            (
                "ping: usage error: Destination address required".to_owned(),
                2
            )
        );
        assert_eq!(error(&["-c", "x", "h"]).0, "ping: invalid argument: 'x'");
        assert_eq!(error(&["-c", "0", "h"]).1, 1);
        assert_eq!(
            error(&["-i", "0.001", "h"]).0,
            "ping: cannot flood, minimal interval for user must be >= 2 ms, use -i 0.002 (or higher)"
        );
        assert_eq!(
            error(&["-t", "0", "h"]).0,
            "ping: cannot set unicast time-to-live: Invalid argument"
        );
        assert!(
            error(&["-s", "70000", "h"])
                .0
                .contains("out of range: 0 <= value <= 65507")
        );
        assert!(
            error(&["-s", "65507", "h"])
                .0
                .contains("Windows sends at most 65500")
        );
        assert!(
            error(&["-f", "h"])
                .0
                .starts_with("ping: -f is not supported")
        );
        assert!(
            error(&["-Z", "h"])
                .0
                .starts_with("ping: invalid option -- 'Z'")
        );
    }

    #[test]
    fn a_windows_count_gets_a_hint() {
        let (message, status) = error(&["-n", "3", "host"]);
        assert!(message.contains("`-n 3` is `-c 3` here"), "{message}");
        assert_eq!(status, 2);
    }

    #[test]
    fn round_trips_and_percentages_print_as_iputils_does() {
        assert_eq!(round_trip(0.0414), "0.041");
        assert_eq!(round_trip(1.234), "1.23");
        assert_eq!(round_trip(21.16), "21.2");
        assert_eq!(round_trip(123.4), "123");
        assert_eq!(percent(0, 3), "0");
        assert_eq!(percent(2, 2), "100");
        assert_eq!(percent(1, 3), "33.3333");
        assert_eq!(percent(1, 11), "9.09091");
        assert_eq!(percent(1, 1001), "0.0999001");
        assert_eq!(percent(1, 2), "50");
    }
}
