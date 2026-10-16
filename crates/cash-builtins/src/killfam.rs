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
use cash_getopt::{Arg, Getopt, Item, Long, Problem, Short, Written};
use cash_win32::process::{Held, ProcessInfo};
use clap::Parser;

use crate::procmatch::{self, Delivery, NameMatcher, Signal};

/// Whether an option word that is not a known signal was meant as one: two or more
/// capital letters, as `-ZZZ` is. `-P1,2` is an option with its value.
fn looks_like_signal_name(body: &str) -> bool {
    body.len() > 1 && body.chars().all(|c| c.is_ascii_uppercase())
}

/// A word the kill family takes as a signal before `getopt_long` reads it, as procps'
/// `signal_option` and psmisc's fallback do: `-9`, `-KILL`, `-SIGKILL`, `-kill` for
/// pkill; or one meant as a signal that names none (`-ZZZ`), said as unknown.
fn signal_word(word: &str) -> Option<&'static str> {
    let body = word.strip_prefix('-')?;
    (procmatch::parse_signal(body).is_some() || looks_like_signal_name(body)).then_some("sigword")
}

/// The same for killall, where psmisc takes a word as a signal only when it starts with
/// a capital or a digit: `-kill` is no signal there. Its id is `9`, no option's letter.
fn killall_signal_word(word: &str) -> Option<char> {
    let body = word.strip_prefix('-')?;
    (body.starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit())
        && signal_word(word).is_some())
    .then_some('9')
}

/// The signal a signal word names, or psmisc's and procps' complaint.
fn signal_of_word(word: &str) -> Result<Signal, String> {
    let body = word.get(1..).unwrap_or_default();
    procmatch::parse_signal(body).ok_or_else(|| std::format!("Unknown signal \"{body}\"."))
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
    pids: Vec<u32>,
    /// `-O`: only processes started at least this many seconds ago.
    older: Option<u64>,
    pattern: Option<String>,
}

const PKILL_HINT: &str = "Try `pkill --help' for more information.";

/// procps-ng 4.0.7's `pkill` options: its short ones, and the long names it shares with
/// `pgrep`, `--inverse` among them. Help and version have letters of their own.
const PKILL_SHORTS: &[Short<&str>] = &[
    Short::new('e', Arg::No, "echo"),
    Short::new('H', Arg::No, "require-handler"),
    Short::new('q', Arg::Required, "queue"),
    Short::new('m', Arg::No, "mrelease"),
    Short::new('c', Arg::No, "count"),
    Short::new('f', Arg::No, "full"),
    Short::new('g', Arg::Required, "pgroup"),
    Short::new('G', Arg::Required, "group"),
    Short::new('i', Arg::No, "ignore-case"),
    Short::new('n', Arg::No, "newest"),
    Short::new('o', Arg::No, "oldest"),
    Short::new('O', Arg::Required, "older"),
    Short::new('p', Arg::Required, "pid"),
    Short::new('P', Arg::Required, "parent"),
    Short::new('s', Arg::Required, "session"),
    Short::new('t', Arg::Required, "terminal"),
    Short::new('u', Arg::Required, "euid"),
    Short::new('U', Arg::Required, "uid"),
    Short::new('x', Arg::No, "exact"),
    Short::new('F', Arg::Required, "pidfile"),
    Short::new('L', Arg::No, "logpidfile"),
    Short::new('r', Arg::Required, "runstates"),
    Short::new('A', Arg::No, "ignore-ancestors"),
    Short::new('Q', Arg::No, "shell-quote"),
    Short::new('h', Arg::No, "help"),
    Short::new('V', Arg::No, "version"),
];
const PKILL_LONGS: &[Long<'static, &str>] = &[
    Long::new("require-handler", Arg::No, "require-handler"),
    Long::new("queue", Arg::Required, "queue"),
    Long::new("echo", Arg::No, "echo"),
    Long::new("mrelease", Arg::No, "mrelease"),
    Long::new("count", Arg::No, "count"),
    Long::new("full", Arg::No, "full"),
    Long::new("pgroup", Arg::Required, "pgroup"),
    Long::new("group", Arg::Required, "group"),
    Long::new("ignore-case", Arg::No, "ignore-case"),
    Long::new("inverse", Arg::No, "inverse"),
    Long::new("newest", Arg::No, "newest"),
    Long::new("oldest", Arg::No, "oldest"),
    Long::new("older", Arg::Required, "older"),
    Long::new("pid", Arg::Required, "pid"),
    Long::new("parent", Arg::Required, "parent"),
    Long::new("session", Arg::Required, "session"),
    Long::new("signal", Arg::Required, "signal"),
    Long::new("terminal", Arg::Required, "terminal"),
    Long::new("euid", Arg::Required, "euid"),
    Long::new("uid", Arg::Required, "uid"),
    Long::new("exact", Arg::No, "exact"),
    Long::new("pidfile", Arg::Required, "pidfile"),
    Long::new("logpidfile", Arg::No, "logpidfile"),
    Long::new("runstates", Arg::Required, "runstates"),
    Long::new("ignore-ancestors", Arg::No, "ignore-ancestors"),
    Long::new("shell-quote", Arg::No, "shell-quote"),
    Long::new("cgroup", Arg::Required, "cgroup"),
    Long::new("ns", Arg::Required, "ns"),
    Long::new("nslist", Arg::Required, "nslist"),
    Long::new("env", Arg::Required, "env"),
    Long::new("help", Arg::No, "help"),
    Long::new("version", Arg::No, "version"),
];

/// procps-ng 4.0.7's `pkill --help`, and what Windows changes.
const PKILL_HELP: &str = "
Usage:
 pkill [options] <pattern>

Options:
 -<sig>                    signal to send (either number or name)
 -H, --require-handler     match only if signal handler is present
 -q, --queue <value>       integer value to be sent with the signal
 -e, --echo                display what is killed
 -m, --mrelease            release process memory immediately after kill
 -c, --count               count of matching processes
 -f, --full                use full command line to match
 -g, --pgroup <PGID,...>   match listed process group IDs
 -G, --group <GID,...>     match real group IDs
 -i, --ignore-case         match case insensitively
 -n, --newest              select most recently started
 -o, --oldest              select least recently started
 -O, --older <seconds>     select where older than seconds
 -p, --pid <PID,...>       match process PIDs
 -P, --parent <PPID,...>   match only child processes of the given parent
 -s, --session <SID,...>   match session IDs
     --signal <sig>        signal to send (either number or name)
 -t, --terminal <tty,...>  match by controlling terminal
 -u, --euid <ID,...>       match by effective IDs
 -U, --uid <ID,...>        match by real IDs
 -x, --exact               match exactly with the command name
 -F, --pidfile <file>      read PIDs from file
 -L, --logpidfile          fail if PID file is not locked
 -r, --runstates <state>   match runstates [D,S,Z,...]
 -A, --ignore-ancestors    exclude our ancestors from results
 -Q, --shell-quote         output the command line in shell-quoted form
 --cgroup <grp,...>        match by cgroup v2 names
 --ns <PID>                match the processes that belong to the same
                           namespace as <pid>
 --nslist <ns,...>         list which namespaces will be considered for
                           the --ns option.
                           Available namespaces: ipc, mnt, net, pid, user, uts
  --env <name=val,...>     match on environment variable

 -h, --help     display this help and exit
 -V, --version  output version information and exit

On Windows, names are file names matched without case, so -i changes
nothing; -f, -g, -G, -s, -t, -u, -U, -F, -L, -r, -A, -H, -q, -m, -Q,
--cgroup, --ns, --nslist and --env are refused.";

/// Why `pkill` ends before it signals anything.
#[derive(Debug, PartialEq, Eq)]
enum PkillStop {
    Help,
    Version,
    /// A command line `getopt_long` refuses: said with the whole usage, as procps does.
    Usage(String),
    /// Any other mistake: said with the line that points at `--help`.
    Mistake(String),
}

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
            Err(PkillStop::Help) => {
                writeln!(context.stdout(), "{PKILL_HELP}")?;
                return Ok(ExecutionResult::success());
            }
            Err(PkillStop::Version) => {
                writeln!(context.stdout(), "pkill (cash): procps-ng 4.0.7's options")?;
                return Ok(ExecutionResult::success());
            }
            Err(PkillStop::Usage(message)) => {
                writeln!(context.stderr(), "pkill: {message}\n{PKILL_HELP}")?;
                return Ok(ExecutionResult::new(2));
            }
            Err(PkillStop::Mistake(message)) => {
                writeln!(context.stderr(), "pkill: {message}\n{PKILL_HINT}")?;
                return Ok(ExecutionResult::new(2));
            }
        };
        if options.pattern.is_none() && options.parents.is_empty() && options.pids.is_empty() {
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

        let (targets, listed) = pkill_targets(&options, matcher.as_ref());
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

/// The processes `options` select from a listing taken now, the shell's protected ones
/// left out (`-n` and `-o` leave one), and the time the listing was finished by.
fn pkill_targets(options: &PkillOptions, matcher: Option<&NameMatcher>) -> (Vec<ProcessInfo>, u64) {
    let processes = cash_win32::process::list();
    let listed = cash_win32::process::now_filetime();
    let mut targets: Vec<ProcessInfo> = processes
        .into_iter()
        .filter(|p| options.parents.is_empty() || options.parents.contains(&p.parent_pid))
        .filter(|p| options.pids.is_empty() || options.pids.contains(&p.pid))
        .filter(|p| {
            // FILETIME counts 100 ns; a start that cannot be read is never old enough.
            options.older.is_none_or(|seconds| {
                started(p.pid).is_some_and(|at| listed.saturating_sub(at) / 10_000_000 >= seconds)
            })
        })
        .filter(|p| matcher.is_none_or(|matcher| matcher.matches(&p.name)) != options.inverse)
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
    (targets, listed)
}

/// Reads `pkill`'s command line as procps does: its first signal word taken before
/// `getopt_long` (a second is read as options, as there), then procps' table, in order.
fn parse_pkill(args: &[String]) -> Result<PkillOptions, PkillStop> {
    let mut options = PkillOptions::default();
    let mut signal_taken = false;
    let getopt = Getopt::new(PKILL_SHORTS, PKILL_LONGS).claim(signal_word);
    for item in getopt.read(args) {
        let (id, value, written) = match item {
            Err(problem) => return Err(PkillStop::Usage(problem.to_string())),
            Ok(Item::Operand { value, .. }) => {
                if options.pattern.replace(value).is_some() {
                    return Err(PkillStop::Mistake(
                        "only one pattern can be provided".to_owned(),
                    ));
                }
                continue;
            }
            Ok(Item::Option {
                id, value, written, ..
            }) => (id, value.unwrap_or_default(), written),
        };
        match id {
            "sigword" if signal_taken => {
                // procps took its one signal word already: getopt reads this one.
                let letter = value.chars().nth(1).unwrap_or('-');
                return Err(PkillStop::Usage(
                    Problem::UnknownShort { letter }.to_string(),
                ));
            }
            "sigword" => {
                signal_taken = true;
                options.signal = Some(signal_of_word(&value).map_err(PkillStop::Mistake)?);
            }
            "signal" => {
                options.signal = Some(procmatch::parse_signal(&value).ok_or_else(|| {
                    PkillStop::Mistake(std::format!("Unknown signal \"{value}\"."))
                })?);
            }
            "parent" => options
                .parents
                .extend(parse_pids(&value).map_err(PkillStop::Mistake)?),
            "pid" => options
                .pids
                .extend(parse_pids(&value).map_err(PkillStop::Mistake)?),
            "older" => {
                options.older = Some(value.trim().parse().map_err(|_| {
                    PkillStop::Mistake(std::format!("invalid argument for -O: {value}"))
                })?);
            }
            "echo" => options.echo = true,
            "count" => options.count = true,
            "newest" => options.newest = true,
            "oldest" => options.oldest = true,
            "exact" => options.exact = true,
            "inverse" => options.inverse = true,
            // Case is ignored already: Windows image names are case-insensitive.
            "ignore-case" => {}
            "help" => return Err(PkillStop::Help),
            "version" => return Err(PkillStop::Version),
            refused => return Err(PkillStop::Mistake(refusal(refused, &written))),
        }
    }
    if options.newest && options.oldest {
        return Err(PkillStop::Mistake(
            "-n and -o are mutually exclusive".to_owned(),
        ));
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

/// The message for a procps option that has no Windows answer here, as `written`.
fn refusal(option: &str, written: &Written) -> String {
    let reason = match option {
        "full" => "matching the command line needs another process's memory (as ps -o args does)",
        "euid" | "uid" | "pgroup" | "group" | "session" | "terminal" => {
            "selecting by user, group, session or terminal is not supported"
        }
        _ => "not supported on Windows",
    };
    std::format!("{written}: {reason}")
}

// ---------------------------------------------------------------------------
// pidof
// ---------------------------------------------------------------------------

/// procps-ng 4.0.7's `pidof` options; `-d` is `-S`'s old letter.
const PIDOF_SHORTS: &[Short<char>] = &[
    Short::new('q', Arg::No, 'q'),
    Short::new('s', Arg::No, 's'),
    Short::new('c', Arg::No, 'c'),
    Short::new('d', Arg::Required, 'S'),
    Short::new('S', Arg::Required, 'S'),
    Short::new('n', Arg::No, 'n'),
    Short::new('x', Arg::No, 'x'),
    Short::new('m', Arg::No, 'm'),
    Short::new('o', Arg::Required, 'o'),
    Short::new('w', Arg::No, 'w'),
    Short::new('t', Arg::No, 't'),
    Short::new('h', Arg::No, 'h'),
    Short::new('V', Arg::No, 'V'),
];
const PIDOF_LONGS: &[Long<'static, char>] = &[
    Long::new("single-shot", Arg::No, 's'),
    Long::new("check-root", Arg::No, 'c'),
    Long::new("with-workers", Arg::No, 'w'),
    Long::new("omit-pid", Arg::Required, 'o'),
    Long::new("lightweight", Arg::No, 't'),
    Long::new("separator", Arg::Required, 'S'),
    Long::new("help", Arg::No, 'h'),
    Long::new("version", Arg::No, 'V'),
];

/// procps-ng 4.0.7's `pidof --help`, and what Windows changes.
const PIDOF_HELP: &str = "
Usage:
 pidof [options] [program [...]]

Options:
 -s, --single-shot         return one PID only
 -c, --check-root          omit processes with different root
 -q,                       quiet mode, only set the exit code
 -w, --with-workers        show kernel workers too
 -x                        also find shells running the named scripts
 -o, --omit-pid <PID,...>  omit processes with PID
 -t, --lightweight         list threads too
 -S, --separator SEP       use SEP as separator put between PIDs
 -h, --help     display this help and exit
 -V, --version  output version information and exit

On Windows, -c, -w, -x and -t change nothing.";

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
        let mut separator = " ".to_owned();
        let mut omit: Vec<u32> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for item in Getopt::new(PIDOF_SHORTS, PIDOF_LONGS).read(&self.args) {
            let (id, value) = match item {
                Err(problem) => {
                    writeln!(context.stderr(), "pidof: {problem}\n{PIDOF_HELP}")?;
                    return Ok(ExecutionResult::general_error());
                }
                Ok(Item::Operand { value, .. }) => {
                    names.push(value);
                    continue;
                }
                Ok(Item::Option { id, value, .. }) => (id, value.unwrap_or_default()),
            };
            match id {
                's' => single = true,
                'q' => quiet = true,
                'S' => separator = value,
                'h' => {
                    writeln!(context.stdout(), "{PIDOF_HELP}")?;
                    return Ok(ExecutionResult::success());
                }
                'V' => {
                    writeln!(context.stdout(), "pidof (cash): procps-ng 4.0.7's options")?;
                    return Ok(ExecutionResult::success());
                }
                'o' => {
                    for item in value.split(',') {
                        // `%PPID` is pidof's parent: the shell, which here is the process
                        // running this builtin.
                        if item == "%PPID" {
                            omit.push(std::process::id());
                        } else if let Ok(pid) = item.parse::<u32>() {
                            omit.push(pid);
                        } else {
                            writeln!(context.stderr(), "pidof: illegal omit pid value ({item})!")?;
                            return Ok(ExecutionResult::general_error());
                        }
                    }
                }
                // -x (scripts are processes too), -c, -w, -t, -n and -m change nothing on
                // Windows.
                _ => {}
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
            writeln!(context.stdout(), "{}", line.join(&separator))?;
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

/// psmisc 23.7's `killall` usage, and what Windows changes.
const KILLALL_USAGE: &str = "\
Usage: killall [OPTION]... [--] NAME...
       killall -l, --list
       killall -V, --version

  -e,--exact          require exact match for very long names
  -I,--ignore-case    case insensitive process name match
  -g,--process-group  kill process group instead of process
  -y,--younger-than   kill processes younger than TIME
  -o,--older-than     kill processes older than TIME
  -i,--interactive    ask for confirmation before killing
  -l,--list           list all known signal names
  -q,--quiet          don't print complaints
  -r,--regexp         interpret NAME as an extended regular expression
  -s,--signal SIGNAL  send this signal instead of SIGTERM
  -u,--user USER      kill only process(es) running as USER
  -v,--verbose        report if the signal was successfully sent
  -V,--version        display version information
  -w,--wait           wait for processes to die
  -n,--ns PID         match processes that belong to the same namespaces
                      as PID
  -Z,--context REGEXP kill only process(es) having context
                      (must precede other arguments)

On Windows, names are matched whole and without case, so -e and -I change
nothing; -g, -y, -o, -i, -u, -n and -Z are refused.";

/// psmisc 23.7's options, which it reads with `getopt_long_only`: a long name takes one
/// dash too (`-list`).
const KILLALL_SHORTS: &[Short<char>] = &[
    Short::new('e', Arg::No, 'e'),
    Short::new('g', Arg::No, 'g'),
    Short::new('y', Arg::Required, 'y'),
    Short::new('o', Arg::Required, 'o'),
    Short::new('i', Arg::No, 'i'),
    Short::new('l', Arg::No, 'l'),
    Short::new('q', Arg::No, 'q'),
    Short::new('r', Arg::No, 'r'),
    Short::new('s', Arg::Required, 's'),
    Short::new('u', Arg::Required, 'u'),
    Short::new('v', Arg::No, 'v'),
    Short::new('w', Arg::No, 'w'),
    Short::new('Z', Arg::Required, 'Z'),
    Short::new('V', Arg::No, 'V'),
    Short::new('I', Arg::No, 'I'),
    Short::new('h', Arg::No, 'h'),
    Short::new('n', Arg::Required, 'n'),
];
const KILLALL_LONGS: &[Long<'static, char>] = &[
    Long::new("exact", Arg::No, 'e'),
    Long::new("ignore-case", Arg::No, 'I'),
    Long::new("process-group", Arg::No, 'g'),
    Long::new("younger-than", Arg::Required, 'y'),
    Long::new("older-than", Arg::Required, 'o'),
    Long::new("interactive", Arg::No, 'i'),
    Long::new("list", Arg::No, 'l'),
    Long::new("quiet", Arg::No, 'q'),
    Long::new("regexp", Arg::No, 'r'),
    Long::new("signal", Arg::Required, 's'),
    Long::new("user", Arg::Required, 'u'),
    Long::new("verbose", Arg::No, 'v'),
    Long::new("version", Arg::No, 'V'),
    Long::new("wait", Arg::No, 'w'),
    Long::new("ns", Arg::Required, 'n'),
    Long::new("context", Arg::Required, 'Z'),
    Long::new("help", Arg::No, 'h'),
];

/// Why `killall` ends before it signals anything.
#[derive(Debug, PartialEq, Eq)]
enum KillallStop {
    Help,
    Version,
    /// A command line `getopt_long_only` refuses: said with the usage.
    Usage(String),
    /// Any other mistake, said alone.
    Mistake(String),
}

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
        let options = match parse_killall(&self.args) {
            Ok(options) => options,
            // `help killall` shows this, so the usage goes to standard output, status 0,
            // where psmisc's is an error.
            Err(KillallStop::Help) => {
                writeln!(context.stdout(), "{KILLALL_USAGE}")?;
                return Ok(ExecutionResult::success());
            }
            Err(KillallStop::Version) => {
                writeln!(context.stdout(), "killall (cash): psmisc 23.7's options")?;
                return Ok(ExecutionResult::success());
            }
            Err(KillallStop::Usage(message)) => {
                writeln!(context.stderr(), "killall: {message}\n{KILLALL_USAGE}")?;
                return Ok(ExecutionResult::general_error());
            }
            Err(KillallStop::Mistake(message)) => {
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

/// Reads `killall`'s command line as psmisc does: `getopt_long_only` by its table, a
/// word that starts with a capital or a digit taken as a signal (the last one wins).
fn parse_killall(args: &[String]) -> Result<KillallOptions, KillallStop> {
    let mut options = KillallOptions::default();
    let unknown_signal = |value: &str| {
        KillallStop::Mistake(std::format!(
            "{value}: unknown signal; killall -l lists signals."
        ))
    };
    let getopt = Getopt::new(KILLALL_SHORTS, KILLALL_LONGS)
        .long_only(true)
        .claim(killall_signal_word);
    for item in getopt.read(args) {
        let (id, value, written) = match item {
            Err(problem) => return Err(KillallStop::Usage(problem.to_string())),
            Ok(Item::Operand { value, .. }) => {
                options.names.push(value);
                continue;
            }
            Ok(Item::Option {
                id, value, written, ..
            }) => (id, value.unwrap_or_default(), written),
        };
        match id {
            'q' => options.quiet = true,
            'v' => options.verbose = true,
            'w' => options.wait = true,
            'r' => options.regexp = true,
            'l' => options.list = true,
            // Names are compared whole and without case already.
            'e' | 'I' => {}
            'h' => return Err(KillallStop::Help),
            'V' => return Err(KillallStop::Version),
            's' => {
                options.signal =
                    Some(procmatch::parse_signal(&value).ok_or_else(|| unknown_signal(&value))?);
            }
            '9' => {
                let body = value.get(1..).unwrap_or_default();
                options.signal =
                    Some(procmatch::parse_signal(body).ok_or_else(|| unknown_signal(body))?);
            }
            _ => {
                return Err(KillallStop::Mistake(std::format!(
                    "killall: {written} is not supported"
                )));
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
        // procps' signal_option looks past the pattern too, and takes lower case.
        let options = parse_pkill(&args(&["x", "-kill"])).unwrap();
        assert_eq!(options.signal.map(Signal::number), Some(9));
        // Only the first signal word: getopt reads a second one, as in procps.
        assert_eq!(
            parse_pkill(&args(&["-9", "-SIGTERM", "x"])).err(),
            Some(PkillStop::Usage("invalid option -- 'S'".to_owned()))
        );
    }

    #[test]
    fn pkill_reads_procps_table() {
        let options = parse_pkill(&args(&["-P1,2", "-P", "3", "--parent=4"])).unwrap();
        assert_eq!(options.parents, [1, 2, 3, 4]);
        // Long names by any unique prefix; `-p` and `-O`, which procps 4.0.7 has.
        let options = parse_pkill(&args(&[
            "--sig", "KILL", "--exa", "-p", "7,8", "-O", "30", "x",
        ]))
        .unwrap();
        assert_eq!(options.signal.map(Signal::number), Some(9));
        assert!(options.exact);
        assert_eq!(options.pids, [7, 8]);
        assert_eq!(options.older, Some(30));
        // `--inverse` is procps' long name only; `-v` is no pkill option.
        assert!(parse_pkill(&args(&["--inverse", "x"])).unwrap().inverse);
        assert_eq!(
            parse_pkill(&args(&["-v", "x"])).err(),
            Some(PkillStop::Usage("invalid option -- 'v'".to_owned()))
        );
        assert_eq!(parse_pkill(&args(&["-h"])).err(), Some(PkillStop::Help));
        assert_eq!(parse_pkill(&args(&["-V"])).err(), Some(PkillStop::Version));
        assert_eq!(
            parse_pkill(&args(&["--help=x"])).err(),
            Some(PkillStop::Usage(
                "option '--help' doesn't allow an argument".to_owned()
            ))
        );
    }

    #[test]
    fn pkill_refuses_what_windows_cannot_answer() {
        let Some(PkillStop::Mistake(message)) = parse_pkill(&args(&["-f", "x"])).err() else {
            panic!("-f is refused")
        };
        assert!(message.starts_with("-f:"), "{message}");
        let Some(PkillStop::Mistake(message)) = parse_pkill(&args(&["--euid=0", "x"])).err() else {
            panic!("--euid is refused")
        };
        assert!(message.starts_with("--euid:"), "{message}");
        assert_eq!(
            parse_pkill(&args(&["-Z", "x"])).err(),
            Some(PkillStop::Usage("invalid option -- 'Z'".to_owned()))
        );
        assert_eq!(
            parse_pkill(&args(&["a", "b"])).err(),
            Some(PkillStop::Mistake(
                "only one pattern can be provided".to_owned()
            ))
        );
        assert_eq!(
            parse_pkill(&args(&["-ZZZ", "x"])).err(),
            Some(PkillStop::Mistake("Unknown signal \"ZZZ\".".to_owned()))
        );
    }

    #[test]
    fn killall_signals_and_names() {
        let options = parse_killall(&args(&["-v", "-KILL", "a", "-s", "0", "b"])).unwrap();
        assert!(options.verbose);
        assert_eq!(options.signal.map(Signal::number), Some(0));
        assert_eq!(options.names, ["a", "b"]);
        assert_eq!(
            parse_killall(&args(&["-s", "ZZZ", "a"])).err(),
            Some(KillallStop::Mistake(
                "ZZZ: unknown signal; killall -l lists signals.".to_owned()
            ))
        );
        assert_eq!(
            parse_killall(&args(&["-i", "a"])).err(),
            Some(KillallStop::Mistake(
                "killall: -i is not supported".to_owned()
            ))
        );
    }

    #[test]
    fn killall_reads_psmisc_table() {
        // getopt_long_only: a long name with one dash; and by any unique prefix.
        assert!(parse_killall(&args(&["-list"])).unwrap().list);
        let options = parse_killall(&args(&["--sig", "KILL", "--verb", "a"])).unwrap();
        assert_eq!(options.signal.map(Signal::number), Some(9));
        assert!(options.verbose);
        // A signal word starts with a capital or a digit in psmisc: `-kill` is not one.
        assert!(parse_killall(&args(&["-kill", "a"])).is_err());
        assert_eq!(
            parse_killall(&args(&["-V"])).err(),
            Some(KillallStop::Version)
        );
        assert_eq!(
            parse_killall(&args(&["--help"])).err(),
            Some(KillallStop::Help)
        );
        assert_eq!(
            parse_killall(&args(&["--bogus"])).err(),
            Some(KillallStop::Usage(
                "unrecognized option '--bogus'".to_owned()
            ))
        );
    }
}
