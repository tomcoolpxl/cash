//! `fuser`: which processes use a file, a directory's files, or a port.
//!
//! Follows psmisc `fuser` (checked against psmisc 23.7): the name and access letters go
//! to standard error and the bare process ids to standard output, so
//! `pids=$(fuser file 2>/dev/null)` works; the exit status is 0 when anything was found.
//!
//! Windows differences, all documented in ROADMAP item 8:
//!
//! * files come from the Restart Manager, which cannot see a directory used only as a
//!   working directory — `fuser DIR` reports the holders of the files below it;
//! * whether a file is open for writing is not reported, so `f` is never `F` for files
//!   and `-w` is refused; `-m`/`-c`/`-M` (mount points) are refused too;
//! * `e` (executable) and `m` (loaded module) come from each process's image and module
//!   list; sockets are listed with `F`, as psmisc does.

use std::collections::BTreeSet;
use std::io::{BufRead, Write};
use std::net::{IpAddr, ToSocketAddrs};

use cash_core::traps::TrapSignal;
use cash_core::{ExecutionResult, builtins, sys};
use cash_win32::net;
use clap::Parser;

use crate::fileuse::{self, Access, ProcessNames, Services};

/// Width of the name column, as in psmisc.
const NAME_FIELD: usize = 20;

const USAGE: &str = "Usage: fuser [-fIMuvw] [-a|-s] [-4|-6] [-c|-m|-n SPACE]\n             [-k [-i] [-SIGNAL]] NAME...";

/// Identify processes using files or sockets.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct FuserCommand {
    /// Options and names, parsed here: psmisc accepts `-SIGNAL` and combined flags.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Space {
    File,
    Tcp,
    Udp,
}

#[derive(Default)]
struct Options {
    all: bool,
    kill: bool,
    interactive: bool,
    silent: bool,
    user: bool,
    verbose: bool,
    v4_only: bool,
    v6_only: bool,
    signal: Option<TrapSignal>,
}

/// One use of a name by a process.
struct Use {
    pid: u32,
    access: char,
}

enum Parsed {
    Run(Options, Vec<(String, Space)>),
    Exit(ExecutionResult),
}

impl builtins::Command for FuserCommand {
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
        let (options, names) = match parse(&self.args, &context)? {
            Parsed::Run(options, names) => (options, names),
            Parsed::Exit(result) => return Ok(result),
        };
        run(&options, &names, &context)
    }
}

fn usage_error(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    message: &str,
) -> Result<Parsed, cash_core::Error> {
    writeln!(context.stderr(), "{message}\n{USAGE}")?;
    Ok(Parsed::Exit(ExecutionResult::general_error()))
}

fn refuse(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    message: &str,
) -> Result<Parsed, cash_core::Error> {
    writeln!(context.stderr(), "fuser: {message}")?;
    Ok(Parsed::Exit(ExecutionResult::general_error()))
}

/// A `-SIGNAL` argument: a signal name (with or without `SIG`) or number.
fn signal_argument(text: &str) -> Option<TrapSignal> {
    if let Ok(number) = text.parse::<i32>() {
        return TrapSignal::try_from(number).ok();
    }
    if text
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return TrapSignal::try_from(text).ok();
    }
    None
}

fn space_named(name: &str) -> Option<Space> {
    match name {
        "file" => Some(Space::File),
        "tcp" => Some(Space::Tcp),
        "udp" => Some(Space::Udp),
        _ => None,
    }
}

fn parse(
    args: &[String],
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<Parsed, cash_core::Error> {
    let mut options = Options::default();
    let mut names = Vec::new();
    let mut space = Space::File;
    let mut only_names = false;
    let mut args = args.iter();

    while let Some(arg) = args.next() {
        if only_names || !arg.starts_with('-') || arg == "-" {
            names.push((arg.clone(), space));
            continue;
        }
        if arg == "--" {
            only_names = true;
            continue;
        }
        match arg.as_str() {
            "--help" => {
                writeln!(context.stdout(), "{USAGE}")?;
                return Ok(Parsed::Exit(ExecutionResult::success()));
            }
            "-V" | "--version" => {
                writeln!(
                    context.stdout(),
                    "fuser (cash) {}\nRestart Manager and IP Helper backed; see ROADMAP item 8.",
                    env!("CARGO_PKG_VERSION")
                )?;
                return Ok(Parsed::Exit(ExecutionResult::success()));
            }
            "-l" | "--list-signals" => {
                let names: Vec<&str> = TrapSignal::iterator()
                    .filter_map(|s| match s {
                        TrapSignal::Signal(_) => Some(s.as_str().trim_start_matches("SIG")),
                        _ => None,
                    })
                    .collect();
                writeln!(context.stdout(), "{}", names.join(" "))?;
                return Ok(Parsed::Exit(ExecutionResult::success()));
            }
            _ => {}
        }
        let body = arg.get(1..).unwrap_or("");
        if let Some(signal) = signal_argument(body) {
            options.signal = Some(signal);
            continue;
        }
        let mut flags = body.chars();
        while let Some(flag) = flags.next() {
            match flag {
                'a' => options.all = true,
                's' => options.silent = true,
                'k' => options.kill = true,
                'i' => options.interactive = true,
                'u' => options.user = true,
                'v' => options.verbose = true,
                '4' => options.v4_only = true,
                '6' => options.v6_only = true,
                // POSIX compatibility; psmisc ignores it too.
                'f' => {}
                'n' => {
                    let rest: String = flags.by_ref().collect();
                    let value = if rest.is_empty() {
                        args.next().cloned().unwrap_or_default()
                    } else {
                        rest
                    };
                    let Some(named) = space_named(&value) else {
                        return usage_error(context, &format!("Invalid namespace name: {value}"));
                    };
                    space = named;
                }
                'm' | 'c' | 'M' => {
                    return refuse(
                        context,
                        &format!(
                            "-{flag}: mount points are not supported on Windows; `fuser DIR` reports the processes holding files below a directory"
                        ),
                    );
                }
                'w' => {
                    return refuse(
                        context,
                        "-w: Windows does not report whether a file is open for writing",
                    );
                }
                'I' => {
                    return refuse(context, "-I: inode comparison does not apply on Windows");
                }
                other => return usage_error(context, &format!("fuser: Invalid option {other}")),
            }
        }
    }

    if names.is_empty() {
        return usage_error(context, "No process specification given");
    }
    if options.all && options.silent {
        return usage_error(context, "fuser: -a and -s cannot be used together");
    }
    Ok(Parsed::Run(options, names))
}

/// The processes using one file or directory.
fn file_uses(path: &std::path::Path) -> std::io::Result<Vec<Use>> {
    let holders = if path.is_dir() {
        fileuse::holders_below(path)?
    } else {
        fileuse::file_holders(path)?
    };
    let mut seen = BTreeSet::new();
    Ok(holders
        .into_iter()
        .filter(|h| seen.insert(h.pid))
        .map(|h| Use {
            pid: h.pid,
            access: match h.access {
                Access::Executable => 'e',
                Access::Mapped => 'm',
                Access::Open => 'f',
            },
        })
        .collect())
}

/// A socket specification: `[local_port][,[remote_host][,[remote_port]]]`.
struct PortSpec {
    proto: net::Proto,
    local: Option<u16>,
    remote_host: Option<Vec<IpAddr>>,
    remote_port: Option<u16>,
}

fn parse_port_spec(text: &str, proto: net::Proto, services: &Services) -> Option<PortSpec> {
    let mut parts = text.splitn(3, ',');
    let local = parts.next().unwrap_or("");
    let host = parts.next().unwrap_or("");
    let remote = parts.next().unwrap_or("");
    let port = |text: &str| -> Option<Option<u16>> {
        if text.is_empty() {
            Some(None)
        } else {
            services.port(text, proto.name()).map(Some)
        }
    };
    Some(PortSpec {
        proto,
        local: port(local)?,
        remote_host: if host.is_empty() {
            None
        } else {
            let addresses: Vec<IpAddr> =
                (host, 0).to_socket_addrs().ok()?.map(|a| a.ip()).collect();
            Some(addresses)
        },
        remote_port: port(remote)?,
    })
}

fn socket_uses(spec: &PortSpec, options: &Options) -> std::io::Result<Vec<Use>> {
    let v4 = !options.v6_only;
    let v6 = !options.v4_only;
    let mut seen = BTreeSet::new();
    Ok(net::sockets(&[spec.proto], v4, v6)?
        .into_iter()
        .filter(|s| s.pid != 0)
        .filter(|s| spec.local.is_none_or(|port| s.local.port() == port))
        .filter(|s| {
            spec.remote_port
                .is_none_or(|port| s.remote.is_some_and(|r| r.port() == port))
        })
        .filter(|s| {
            spec.remote_host
                .as_ref()
                .is_none_or(|hosts| s.remote.is_some_and(|r| hosts.iter().any(|h| *h == r.ip())))
        })
        .filter(|s| seen.insert(s.pid))
        .map(|s| Use {
            pid: s.pid,
            access: 'F',
        })
        .collect())
}

fn access_field(access: char) -> String {
    // psmisc's five columns: f/F, r(oot), c(wd), e(xecutable), m(apped).
    let mut field = ['.'; 5];
    match access {
        'f' | 'F' => field[0] = access,
        'e' => field[3] = 'e',
        'm' => field[4] = 'm',
        _ => {}
    }
    field.iter().collect()
}

fn padded_name(name: &str) -> String {
    let label = format!("{name}:");
    format!("{label:<NAME_FIELD$}")
}

#[allow(
    clippy::too_many_lines,
    reason = "one pass over the names, mirroring psmisc's output order"
)]
fn run(
    options: &Options,
    names: &[(String, Space)],
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
) -> Result<ExecutionResult, cash_core::Error> {
    let services = Services::load();
    let mut processes = ProcessNames::new();
    let mut found_any = false;
    let mut to_kill: BTreeSet<u32> = BTreeSet::new();
    let mut header_written = false;
    let mut stdout = context.stdout();
    let mut stderr = context.stderr();

    for (raw, space) in names {
        // `NAME/tcp` selects a socket space as psmisc's short form of `-n tcp NAME`.
        let (spec_text, space) = match raw.rsplit_once('/') {
            Some((spec, "tcp")) if *space == Space::File => (spec.to_owned(), Space::Tcp),
            Some((spec, "udp")) if *space == Space::File => (spec.to_owned(), Space::Udp),
            _ => (raw.clone(), *space),
        };

        let (display, uses) = match space {
            Space::File => {
                let path = context.shell.absolute_path(&spec_text);
                if !path.exists() {
                    writeln!(stderr, "Specified filename {raw} does not exist.")?;
                    continue;
                }
                let display = cash_win32::path::render(&path);
                match file_uses(&path) {
                    Ok(uses) => (display, uses),
                    Err(error) => {
                        writeln!(stderr, "fuser: {display}: {error}")?;
                        continue;
                    }
                }
            }
            Space::Tcp | Space::Udp => {
                let proto = if space == Space::Tcp {
                    net::Proto::Tcp
                } else {
                    net::Proto::Udp
                };
                let Some(spec) = parse_port_spec(&spec_text, proto, &services) else {
                    writeln!(stderr, "fuser: Cannot resolve {raw}")?;
                    continue;
                };
                let display = format!("{spec_text}/{}", proto.name());
                match socket_uses(&spec, options) {
                    Ok(uses) => (display, uses),
                    Err(error) => {
                        writeln!(stderr, "fuser: {display}: {error}")?;
                        continue;
                    }
                }
            }
        };

        found_any |= !uses.is_empty();
        to_kill.extend(uses.iter().map(|u| u.pid));
        if options.silent || (uses.is_empty() && !options.all) {
            continue;
        }

        if options.verbose {
            if !header_written {
                writeln!(stderr, "{:NAME_FIELD$} USER        PID ACCESS COMMAND", "")?;
                header_written = true;
            }
            if uses.is_empty() {
                writeln!(stderr, "{}", padded_name(&display).trim_end())?;
            }
            for (index, used) in uses.iter().enumerate() {
                let lead = if index == 0 {
                    padded_name(&display)
                } else {
                    " ".repeat(NAME_FIELD)
                };
                let user = processes.user(used.pid);
                writeln!(
                    stderr,
                    "{lead} {user:<8} {:>6} {} {}",
                    used.pid,
                    access_field(used.access),
                    processes.name(used.pid)
                )?;
            }
        } else {
            write!(stderr, "{}", padded_name(&display))?;
            stderr.flush()?;
            for used in &uses {
                write!(stdout, "{:>6}", used.pid)?;
                stdout.flush()?;
                if options.user {
                    write!(stderr, "({})", processes.user(used.pid))?;
                }
                if matches!(used.access, 'e' | 'm') {
                    write!(stderr, "{}", used.access)?;
                }
                stderr.flush()?;
            }
            writeln!(stderr)?;
        }
    }

    if options.kill {
        let signal = match options.signal {
            Some(signal) => signal,
            None => TrapSignal::try_from("KILL")?,
        };
        let me = std::process::id();
        let mut input = std::io::BufReader::new(context.stdin());
        for pid in to_kill.into_iter().filter(|&pid| pid != me && pid > 4) {
            if options.interactive {
                write!(
                    stderr,
                    "Kill process {pid} ({}) ? (y/N) ",
                    processes.name(pid)
                )?;
                stderr.flush()?;
                let mut answer = String::new();
                input.read_line(&mut answer)?;
                if !answer.trim_start().starts_with(['y', 'Y']) {
                    continue;
                }
            }
            if let Ok(target) = i32::try_from(pid)
                && let Err(error) = sys::signal::kill_process(target, signal)
            {
                writeln!(stderr, "Could not kill process {pid}: {error}")?;
            }
        }
    }

    Ok(if found_any {
        ExecutionResult::success()
    } else {
        ExecutionResult::general_error()
    })
}
