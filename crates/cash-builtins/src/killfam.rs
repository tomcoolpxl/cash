//! `pkill`, `pidof` and `killall` on native Windows process IDs.
//!
//! Output and exit statuses follow procps-ng 4.0.7 (`pkill`, `pidof`) and psmisc 23.7
//! (`killall`), checked against the real tools. Names follow the family's rules in
//! `procmatch`: case-insensitive, `.exe` optional. Signals go through `kill`'s own path,
//! so `TERM`, the default, asks first and escalates (D21), `KILL` terminates at once, and
//! `STOP`/`CONT` suspend and resume (D19); each pid is signalled alone, as `kill PID` is
//! (D22).
//!
//! Where Windows differs (ROADMAP item 10, research/busybox-gap-analysis.md Q2):
//!
//! * the kill family never signals the shell running it, the kernel's pseudo-processes,
//!   images Windows cannot survive losing, or service accounts' processes; `killall -v`
//!   lists what it skipped, as it does processes the user may not open;
//! * names are file names, so `-f` (a command line, which lives in another process's
//!   memory) is refused, as `ps -o args` is; so are the user, group, session and terminal
//!   selectors, which have no Windows meaning here yet.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use cash_win32::process::{Held, ProcessInfo};
use clap::Parser;

use crate::procmatch::{self, Delivery, NameMatcher, Signal};

/// The value for an option that takes one: the rest of its cluster, or the next word.
fn option_value(rest: &str, args: &[String], index: &mut usize) -> Option<String> {
    if rest.is_empty() {
        let value = args.get(*index).cloned();
        *index += 1;
        value
    } else {
        Some(rest.to_owned())
    }
}

/// `--name=value` or `--name value`.
fn long_value(inline: Option<&str>, args: &[String], index: &mut usize) -> Option<String> {
    inline.map_or_else(
        || option_value("", args, index),
        |value| Some(value.to_owned()),
    )
}

/// Whether an option word that is not a known signal was meant as one: two or more
/// capital letters, as `-ZZZ` is. `-P1,2` is an option with its value.
fn looks_like_signal_name(body: &str) -> bool {
    body.len() > 1 && body.chars().all(|c| c.is_ascii_uppercase())
}

/// When a process started, for `-n`/`-o`; `None` when it cannot be opened.
fn started(pid: u32) -> Option<u64> {
    cash_win32::process::details(pid).started
}

// ---------------------------------------------------------------------------
// pkill
// ---------------------------------------------------------------------------

/// Signal processes by name.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PkillCommand {
    /// Options and the pattern, parsed here: procps accepts `-SIGNAL` and clusters.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Default)]
struct PkillOptions {
    signal: Option<Signal>,
    echo: bool,
    count: bool,
    newest: bool,
    oldest: bool,
    exact: bool,
    inverse: bool,
    parents: Vec<u32>,
    pattern: Option<String>,
}

const PKILL_HINT: &str = "Try `pkill --help' for more information.";

impl builtins::Command for PkillCommand {
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
        let options = match parse_pkill(&self.args) {
            Ok(options) => options,
            Err(message) => {
                writeln!(context.stderr(), "pkill: {message}\n{PKILL_HINT}")?;
                return Ok(ExecutionResult::new(2));
            }
        };
        if options.pattern.is_none() && options.parents.is_empty() {
            writeln!(
                context.stderr(),
                "pkill: no matching criteria specified\n{PKILL_HINT}"
            )?;
            return Ok(ExecutionResult::new(2));
        }
        let matcher = match options.pattern.as_deref() {
            Some(pattern) => match NameMatcher::pattern(pattern, options.exact) {
                Ok(matcher) => Some(matcher),
                Err(error) => {
                    writeln!(context.stderr(), "pkill: invalid pattern: {error}")?;
                    return Ok(ExecutionResult::new(2));
                }
            },
            None => None,
        };
        let signal = match options.signal {
            Some(signal) => signal,
            None => Signal::term()?,
        };

        let processes = cash_win32::process::list();
        let listed = cash_win32::process::now_filetime();
        let mut targets: Vec<ProcessInfo> = processes
            .into_iter()
            .filter(|p| options.parents.is_empty() || options.parents.contains(&p.parent_pid))
            .filter(|p| {
                matcher
                    .as_ref()
                    .is_none_or(|matcher| matcher.matches(&p.name))
                    != options.inverse
            })
            .filter(|p| procmatch::protected(p).is_none())
            .collect();

        if options.newest || options.oldest {
            // A process whose start time cannot be read is never the newest or oldest.
            let timed = targets
                .iter()
                .filter_map(|p| started(p.pid).map(|at| (at, p.pid)));
            let pick = if options.newest {
                timed.max()
            } else {
                timed.min()
            }
            .map(|(_, pid)| pid);
            targets.retain(|p| Some(p.pid) == pick);
        }

        let mut signalled = 0usize;
        for process in &targets {
            match procmatch::deliver_listed(process.pid, listed, signal).0 {
                Delivery::Sent => {
                    signalled += 1;
                    if options.echo {
                        writeln!(
                            context.stdout(),
                            "{} killed (pid {})",
                            process.name,
                            process.pid
                        )?;
                    }
                }
                Delivery::Denied | Delivery::Gone => {}
                Delivery::Failed(error) => {
                    let error = error.worded();
                    writeln!(
                        context.stderr(),
                        "pkill: killing pid {} failed: {error}",
                        process.pid
                    )?;
                }
            }
        }
        if options.count {
            writeln!(context.stdout(), "{signalled}")?;
        }

        Ok(if signalled > 0 {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        })
    }
}

fn parse_pkill(args: &[String]) -> Result<PkillOptions, String> {
    let mut options = PkillOptions::default();
    let mut index = 0;
    let mut only_words = false;
    while let Some(arg) = args.get(index) {
        index += 1;
        if only_words || arg == "-" || !arg.starts_with('-') {
            if options.pattern.replace(arg.clone()).is_some() {
                return Err("only one pattern can be provided".to_owned());
            }
            continue;
        }
        if arg == "--" {
            only_words = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            match name {
                "signal" => {
                    let value = long_value(inline, args, &mut index)
                        .ok_or("option '--signal' requires an argument")?;
                    options.signal = Some(
                        procmatch::parse_signal(&value)
                            .ok_or_else(|| std::format!("Unknown signal \"{value}\"."))?,
                    );
                }
                "parent" => {
                    let value = long_value(inline, args, &mut index)
                        .ok_or("option '--parent' requires an argument")?;
                    options.parents.extend(parse_pids(&value)?);
                }
                "echo" => options.echo = true,
                "count" => options.count = true,
                "newest" => options.newest = true,
                "oldest" => options.oldest = true,
                "exact" => options.exact = true,
                "inverse" => options.inverse = true,
                // Case is ignored already: Windows image names are case-insensitive.
                "ignore-case" => {}
                other => return Err(refusal_or_invalid(other, true)),
            }
            continue;
        }

        let body = arg.get(1..).unwrap_or_default();
        if let Some(signal) = procmatch::parse_signal(body) {
            options.signal = Some(signal);
            continue;
        }
        if looks_like_signal_name(body) {
            return Err(std::format!("Unknown signal \"{body}\"."));
        }
        for (at, flag) in body.char_indices() {
            match flag {
                'e' => options.echo = true,
                'c' => options.count = true,
                'n' => options.newest = true,
                'o' => options.oldest = true,
                'x' => options.exact = true,
                'v' => options.inverse = true,
                'i' => {}
                'P' => {
                    let value = option_value(
                        body.get(at + flag.len_utf8()..).unwrap_or_default(),
                        args,
                        &mut index,
                    )
                    .ok_or("option requires an argument -- 'P'")?;
                    options.parents.extend(parse_pids(&value)?);
                    break;
                }
                other => return Err(refusal_or_invalid(&other.to_string(), false)),
            }
        }
    }
    if options.newest && options.oldest {
        return Err("-n and -o are mutually exclusive".to_owned());
    }
    Ok(options)
}

/// A comma-separated list of process IDs.
fn parse_pids(list: &str) -> Result<Vec<u32>, String> {
    list.split(',')
        .map(|pid| {
            pid.trim()
                .parse::<u32>()
                .map_err(|_| std::format!("invalid process ID: {pid}"))
        })
        .collect()
}

/// The message for an option `pkill` does not take: a documented refusal for the procps
/// options that have no Windows answer here, "invalid option" for anything else.
fn refusal_or_invalid(option: &str, long: bool) -> String {
    let refused = match option {
        "f" | "full" => {
            Some("matching the command line needs another process's memory (as ps -o args does)")
        }
        "u" | "euid" | "U" | "uid" | "g" | "pgroup" | "G" | "group" | "s" | "session" | "t"
        | "terminal" => Some("selecting by user, group, session or terminal is not supported"),
        "F" | "pidfile" | "L" | "logpidfile" | "r" | "runstates" | "A" | "ignore-ancestors"
        | "H" | "require-handler" | "q" | "queue" | "m" | "mrelease" | "Q" | "shell-quote"
        | "ns" | "nslist" | "cgroup" | "env" => Some("not supported on Windows"),
        _ => None,
    };
    let spelled = if long {
        std::format!("--{option}")
    } else {
        std::format!("-{option}")
    };
    refused.map_or_else(
        || {
            if long {
                std::format!("unrecognized option '{spelled}'")
            } else {
                std::format!("invalid option -- '{option}'")
            }
        },
        |reason| std::format!("{spelled}: {reason}"),
    )
}

// ---------------------------------------------------------------------------
// pidof
// ---------------------------------------------------------------------------

/// Print the process IDs of programs by name.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PidofCommand {
    /// Options and names, parsed here: `-o` takes `%PPID` and comma lists.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for PidofCommand {
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
        let mut single = false;
        let mut quiet = false;
        let mut omit: Vec<u32> = Vec::new();
        let mut names: Vec<&str> = Vec::new();
        let mut index = 0;
        let mut only_words = false;
        while let Some(arg) = self.args.get(index) {
            index += 1;
            if only_words || arg == "-" || !arg.starts_with('-') {
                names.push(arg);
                continue;
            }
            if arg == "--" {
                only_words = true;
                continue;
            }
            let body = arg.get(1..).unwrap_or_default();
            for (at, flag) in body.char_indices() {
                match flag {
                    's' => single = true,
                    'q' => quiet = true,
                    // -x (scripts are processes too), -c, -n, -z and -w change nothing
                    // on Windows.
                    'x' | 'c' | 'n' | 'z' | 'w' => {}
                    'o' => {
                        let Some(value) = option_value(
                            body.get(at + flag.len_utf8()..).unwrap_or_default(),
                            &self.args,
                            &mut index,
                        ) else {
                            writeln!(
                                context.stderr(),
                                "pidof: option requires an argument -- 'o'"
                            )?;
                            return Ok(ExecutionResult::general_error());
                        };
                        for item in value.split(',') {
                            // `%PPID` is pidof's parent: the shell, which here is the
                            // process running this builtin.
                            if item == "%PPID" {
                                omit.push(std::process::id());
                            } else if let Ok(pid) = item.parse::<u32>() {
                                omit.push(pid);
                            } else {
                                writeln!(
                                    context.stderr(),
                                    "pidof: illegal omit pid value ({item})!"
                                )?;
                                return Ok(ExecutionResult::general_error());
                            }
                        }
                        break;
                    }
                    other => {
                        writeln!(context.stderr(), "pidof: invalid option -- '{other}'")?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
            }
        }

        // A path names its file: `pidof C:/Windows/notepad.exe` means `notepad.exe`.
        let matchers: Vec<NameMatcher> = names
            .iter()
            .map(|name| NameMatcher::exact(name.rsplit(['/', '\\']).next().unwrap_or(name)))
            .collect();
        let mut pids: Vec<u32> = cash_win32::process::list()
            .into_iter()
            .filter(|p| !omit.contains(&p.pid))
            .filter(|p| matchers.iter().any(|m| m.matches(&p.name)))
            .map(|p| p.pid)
            .collect();
        // Highest pid first, the order procps prints (newest first on Linux, where pids
        // count up; not on Windows, which reuses them).
        pids.sort_unstable_by(|a, b| b.cmp(a));
        if single {
            pids.truncate(1);
        }

        if !quiet && !pids.is_empty() {
            let line: Vec<String> = pids.iter().map(u32::to_string).collect();
            writeln!(context.stdout(), "{}", line.join(" "))?;
        }
        Ok(if pids.is_empty() {
            ExecutionResult::general_error()
        } else {
            ExecutionResult::success()
        })
    }
}

// ---------------------------------------------------------------------------
// killall
// ---------------------------------------------------------------------------

/// Signal every process with a given name.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct KillallCommand {
    /// Options and names, parsed here: psmisc accepts `-SIGNAL` and clusters.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Default)]
struct KillallOptions {
    signal: Option<Signal>,
    quiet: bool,
    verbose: bool,
    wait: bool,
    regexp: bool,
    list: bool,
    names: Vec<String>,
}

const KILLALL_USAGE: &str = "Usage: killall [OPTION]... [--] NAME...\n       killall -l, --list";

impl builtins::Command for KillallCommand {
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
        // `help killall` shows this; it was an unrecognized option.
        if self
            .args
            .iter()
            .take_while(|arg| arg.as_str() != "--")
            .any(|arg| arg == "--help")
        {
            writeln!(context.stdout(), "{KILLALL_USAGE}")?;
            return Ok(ExecutionResult::success());
        }
        let options = match parse_killall(&self.args) {
            Ok(options) => options,
            Err(message) => {
                writeln!(context.stderr(), "{message}")?;
                return Ok(ExecutionResult::general_error());
            }
        };
        if options.list {
            let names = procmatch::signal_names();
            for line in names.chunks(16) {
                writeln!(context.stdout(), "{}", line.join(" "))?;
            }
            return Ok(ExecutionResult::success());
        }
        if options.names.is_empty() {
            writeln!(context.stderr(), "{KILLALL_USAGE}")?;
            return Ok(ExecutionResult::general_error());
        }
        let signal = match options.signal {
            Some(signal) => signal,
            None => Signal::term()?,
        };

        let processes = cash_win32::process::list();
        let listed = cash_win32::process::now_filetime();
        let mut stderr = context.stderr();
        let mut all_found = true;
        let mut signalled = Vec::new();
        for name in &options.names {
            let matcher = if options.regexp {
                match NameMatcher::pattern(name, false) {
                    Ok(matcher) => matcher,
                    Err(error) => {
                        writeln!(stderr, "killall: Bad regular expression: {error}")?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
            } else {
                NameMatcher::exact(name.rsplit(['/', '\\']).next().unwrap_or(name))
            };

            let (matched, sent) = signal_name_matches(
                &processes,
                listed,
                &matcher,
                signal,
                &options,
                &mut signalled,
                &mut stderr,
            )?;

            if sent == 0 {
                all_found = false;
                if !options.quiet {
                    if matched == 0 {
                        writeln!(stderr, "{name}: no process found")?;
                    } else {
                        writeln!(
                            stderr,
                            "{name}: no process killed ({matched} skipped: system, the shell, \
                             or access denied; -v lists them)"
                        )?;
                    }
                }
            }
        }

        if options.wait && !matches!(signal, Signal::Probe) {
            // Each was held open before it was signalled, so it is that process this
            // waits for. Asked by pid, the wait went on for as long as any process had
            // the number: the next one Windows gave it to, for its whole life.
            while signalled.iter().any(Held::is_running) {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        Ok(if all_found {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        })
    }
}

/// Signals every process `matcher` names in a listing finished by `listed`, reporting as
/// psmisc does under `-v`; returns how many matched and how many were signalled. Those
/// signalled are added to `signalled`, held open, for `-w` to wait for.
fn signal_name_matches(
    processes: &[ProcessInfo],
    listed: u64,
    matcher: &NameMatcher,
    signal: Signal,
    options: &KillallOptions,
    signalled: &mut Vec<Held>,
    stderr: &mut impl Write,
) -> Result<(usize, usize), cash_core::Error> {
    let mut matched = 0usize;
    let mut sent = 0usize;
    for process in processes.iter().filter(|p| matcher.matches(&p.name)) {
        matched += 1;
        if let Some(why) = procmatch::protected(process) {
            if options.verbose {
                writeln!(
                    stderr,
                    "Skipped {}({}): {}",
                    process.name,
                    process.pid,
                    why.reason()
                )?;
            }
            continue;
        }
        let (delivery, held) = procmatch::deliver_listed(process.pid, listed, signal);
        match delivery {
            Delivery::Sent => {
                sent += 1;
                signalled.extend(held);
                if options.verbose {
                    writeln!(
                        stderr,
                        "Killed {}({}) with signal {}",
                        process.name,
                        process.pid,
                        signal.number()
                    )?;
                }
            }
            Delivery::Denied => {
                if options.verbose {
                    writeln!(
                        stderr,
                        "Skipped {}({}): access denied",
                        process.name, process.pid
                    )?;
                }
            }
            Delivery::Gone => {}
            Delivery::Failed(error) => {
                let error = error.worded();
                writeln!(stderr, "{}({}): {error}", process.name, process.pid)?;
            }
        }
    }

    Ok((matched, sent))
}

fn parse_killall(args: &[String]) -> Result<KillallOptions, String> {
    let mut options = KillallOptions::default();
    let mut index = 0;
    let mut only_words = false;
    let unknown_signal =
        |value: &str| std::format!("{value}: unknown signal; killall -l lists signals.");
    while let Some(arg) = args.get(index) {
        index += 1;
        if only_words || arg == "-" || !arg.starts_with('-') {
            options.names.push(arg.clone());
            continue;
        }
        if arg == "--" {
            only_words = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            match name {
                "signal" => {
                    let value = long_value(inline, args, &mut index)
                        .ok_or("killall: option '--signal' requires an argument")?;
                    options.signal = Some(
                        procmatch::parse_signal(&value).ok_or_else(|| unknown_signal(&value))?,
                    );
                }
                "quiet" => options.quiet = true,
                "verbose" => options.verbose = true,
                "wait" => options.wait = true,
                "regexp" => options.regexp = true,
                "list" => options.list = true,
                "exact" | "ignore-case" => {}
                "interactive" | "user" | "process-group" | "younger-than" | "older-than" | "ns"
                | "context" => {
                    return Err(std::format!("killall: --{name} is not supported"));
                }
                other => {
                    return Err(std::format!(
                        "killall: unrecognized option '--{other}'\n{KILLALL_USAGE}"
                    ));
                }
            }
            continue;
        }

        let body = arg.get(1..).unwrap_or_default();
        if let Some(signal) = procmatch::parse_signal(body) {
            options.signal = Some(signal);
            continue;
        }
        if looks_like_signal_name(body) {
            return Err(unknown_signal(body));
        }
        for (at, flag) in body.char_indices() {
            match flag {
                'q' => options.quiet = true,
                'v' => options.verbose = true,
                'w' => options.wait = true,
                'r' => options.regexp = true,
                'l' => options.list = true,
                // Names are compared whole and without case already.
                'e' | 'I' => {}
                's' => {
                    let value = option_value(
                        body.get(at + flag.len_utf8()..).unwrap_or_default(),
                        args,
                        &mut index,
                    )
                    .ok_or("killall: option requires an argument -- 's'")?;
                    options.signal = Some(
                        procmatch::parse_signal(&value).ok_or_else(|| unknown_signal(&value))?,
                    );
                    break;
                }
                'i' | 'u' | 'g' | 'y' | 'o' | 'n' | 'Z' => {
                    return Err(std::format!("killall: -{flag} is not supported"));
                }
                other => {
                    return Err(std::format!(
                        "killall: invalid option -- '{other}'\n{KILLALL_USAGE}"
                    ));
                }
            }
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn pkill_takes_a_signal_first_or_later() {
        let options = parse_pkill(&args(&["-9", "-e", "notepad"])).unwrap();
        assert_eq!(options.signal.map(Signal::number), Some(9));
        assert!(options.echo);
        assert_eq!(options.pattern.as_deref(), Some("notepad"));

        let options = parse_pkill(&args(&["-c", "--signal=term", "x"])).unwrap();
        assert_eq!(options.signal.map(Signal::number), Some(15));
        let options = parse_pkill(&args(&["-ce", "-SIGKILL", "x"])).unwrap();
        assert!(options.count && options.echo);
        assert_eq!(options.signal.map(Signal::number), Some(9));
    }

    #[test]
    fn pkill_parent_lists_and_clusters() {
        let options = parse_pkill(&args(&["-P1,2", "-P", "3", "--parent=4"])).unwrap();
        assert_eq!(options.parents, [1, 2, 3, 4]);
    }

    #[test]
    fn pkill_refuses_what_windows_cannot_answer() {
        let message = parse_pkill(&args(&["-f", "x"])).err().unwrap();
        assert!(message.starts_with("-f:"), "{message}");
        assert_eq!(
            parse_pkill(&args(&["-Z", "x"])).err().unwrap(),
            "invalid option -- 'Z'"
        );
        assert_eq!(
            parse_pkill(&args(&["a", "b"])).err().unwrap(),
            "only one pattern can be provided"
        );
        assert!(parse_pkill(&args(&["-ZZZ", "x"])).is_err());
    }

    #[test]
    fn killall_signals_and_names() {
        let options = parse_killall(&args(&["-v", "-KILL", "a", "-s", "0", "b"])).unwrap();
        assert!(options.verbose);
        assert_eq!(options.signal.map(Signal::number), Some(0));
        assert_eq!(options.names, ["a", "b"]);
        assert_eq!(
            parse_killall(&args(&["-s", "ZZZ", "a"])).err().unwrap(),
            "ZZZ: unknown signal; killall -l lists signals."
        );
        assert!(parse_killall(&args(&["-i", "a"])).is_err());
    }
}
