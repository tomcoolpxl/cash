//! `pgrep` for native Windows process IDs.
//!
//! The parent selector is especially useful with cash's `PID` and `PPID`; keeping the
//! query in-process also guarantees that every printed PID is one `kill` understands.
//!
//! The options are procps-ng 4.0.7's, the table `pgrep` shares with `pkill`, read by
//! cash-getopt with its messages and statuses: 0 when a process matched, 1 when none did,
//! 2 for a usage mistake. Names follow the family's rules (`procmatch`): case-insensitive,
//! `.exe` optional. What needs another process's memory (its command line) or a Linux
//! notion (users, groups, sessions, terminals, namespaces) is refused by name.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Short, Written};
use clap::Parser;

use crate::procmatch::NameMatcher;

/// Find native Windows processes by parent and/or executable name.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PgrepCommand {
    /// Options and the pattern, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// procps-ng 4.0.7's `pgrep` options: its own (`-d`, `-l`, `-a`, `-v`, `-w`) and those it
/// shares with `pkill`.
const SHORTS: &[Short<&str>] = &[
    Short::new('d', Arg::Required, "delimiter"),
    Short::new('l', Arg::No, "list-name"),
    Short::new('a', Arg::No, "list-full"),
    Short::new('v', Arg::No, "inverse"),
    Short::new('w', Arg::No, "lightweight"),
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
const LONGS: &[Long<'static, &str>] = &[
    Long::new("delimiter", Arg::Required, "delimiter"),
    Long::new("list-name", Arg::No, "list-name"),
    Long::new("list-full", Arg::No, "list-full"),
    Long::new("quiet", Arg::No, "quiet"),
    Long::new("inverse", Arg::No, "inverse"),
    Long::new("lightweight", Arg::No, "lightweight"),
    Long::new("count", Arg::No, "count"),
    Long::new("full", Arg::No, "full"),
    Long::new("pgroup", Arg::Required, "pgroup"),
    Long::new("group", Arg::Required, "group"),
    Long::new("ignore-case", Arg::No, "ignore-case"),
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

/// procps-ng 4.0.7's `pgrep --help`, and what Windows changes.
const HELP: &str = "
Usage:
 pgrep [options] <pattern>

Options:
 -d, --delimiter <string>  specify output delimiter
 -l, --list-name           list PID and process name
 -a, --list-full           list PID and full command line
     --quiet               suppress all normal output
 -v, --inverse             negates the matching
 -w, --lightweight         list all TID
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
nothing; -a, -f, -w, -g, -G, -s, -t, -u, -U, -F, -L, -r, -A, -Q,
--cgroup, --ns, --nslist and --env are refused.";

const HINT: &str = "Try `pgrep --help' for more information.";

/// What the command line asked for.
#[derive(Debug)]
struct Options {
    delimiter: String,
    list_name: bool,
    quiet: bool,
    inverse: bool,
    count: bool,
    newest: bool,
    oldest: bool,
    exact: bool,
    /// `-O`: only processes started at least this many seconds ago.
    older: Option<u64>,
    pids: Vec<u32>,
    parents: Vec<u32>,
    pattern: Option<String>,
}

/// Why `pgrep` ends before it looks at a process.
#[derive(Debug, PartialEq, Eq)]
enum Stop {
    Help,
    Version,
    /// A command line `getopt_long` refuses: said with the whole usage, as procps does.
    Usage(String),
    /// Any other mistake: said with the line that points at `--help`.
    Mistake(String),
}

/// A comma-separated list of process IDs.
fn pids(list: &str) -> Result<Vec<u32>, Stop> {
    list.split(',')
        .map(|pid| {
            pid.trim()
                .parse::<u32>()
                .map_err(|_| Stop::Mistake(format!("invalid process ID: {pid}")))
        })
        .collect()
}

/// The message for a procps option that has no Windows answer here, as `written`.
fn refusal(option: &str, written: &Written) -> String {
    let reason = match option {
        "full" | "list-full" => {
            "the command line is in another process's memory (as ps -o args says)"
        }
        "euid" | "uid" | "pgroup" | "group" | "session" | "terminal" => {
            "selecting by user, group, session or terminal is not supported"
        }
        _ => "not supported on Windows",
    };
    format!("{written}: {reason}")
}

/// Reads the command line as procps' `getopt_long` loop does, in order.
fn parse(args: &[String]) -> Result<Options, Stop> {
    let mut options = Options {
        delimiter: "\n".to_owned(),
        list_name: false,
        quiet: false,
        inverse: false,
        count: false,
        newest: false,
        oldest: false,
        exact: false,
        older: None,
        pids: Vec::new(),
        parents: Vec::new(),
        pattern: None,
    };
    for item in Getopt::new(SHORTS, LONGS).read(args) {
        let (id, value, written) = match item {
            Err(problem) => return Err(Stop::Usage(problem.to_string())),
            Ok(Item::Operand { value, .. }) => {
                if options.pattern.replace(value).is_some() {
                    return Err(Stop::Mistake("only one pattern can be provided".to_owned()));
                }
                continue;
            }
            Ok(Item::Option {
                id, value, written, ..
            }) => (id, value.unwrap_or_default(), written),
        };
        match id {
            "delimiter" => options.delimiter = value,
            "list-name" => options.list_name = true,
            "quiet" => options.quiet = true,
            "inverse" => options.inverse = true,
            "count" => options.count = true,
            "newest" => options.newest = true,
            "oldest" => options.oldest = true,
            "exact" => options.exact = true,
            "older" => options.older = Some(crate::procmatch::seconds_as_atoi(&value)),
            "pid" => options.pids.extend(pids(&value)?),
            "parent" => options.parents.extend(pids(&value)?),
            // Case is ignored already; `--signal` means nothing to pgrep.
            "ignore-case" | "signal" => {}
            "help" => return Err(Stop::Help),
            "version" => return Err(Stop::Version),
            refused => return Err(Stop::Mistake(refusal(refused, &written))),
        }
    }
    if options.newest && options.oldest {
        return Err(Stop::Mistake("-n and -o are mutually exclusive".to_owned()));
    }
    if options.pattern.is_none() && options.parents.is_empty() && options.pids.is_empty() {
        return Err(Stop::Mistake("no matching criteria specified".to_owned()));
    }
    Ok(options)
}

impl builtins::Command for PgrepCommand {
    type Error = cash_core::Error;

    /// The words as they are, for procps' option loop.
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
        let options = match crate::options::read_options(&context, || parse(&self.args)) {
            Ok(options) => options,
            Err(Stop::Help) => {
                writeln!(context.stdout(), "{HELP}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Stop::Version) => {
                writeln!(context.stdout(), "pgrep (cash): procps-ng 4.0.7's options")?;
                return Ok(ExecutionResult::success());
            }
            Err(Stop::Usage(message)) => {
                writeln!(context.stderr(), "pgrep: {message}\n{HELP}")?;
                return Ok(ExecutionResult::new(2));
            }
            Err(Stop::Mistake(message)) => {
                writeln!(context.stderr(), "pgrep: {message}\n{HINT}")?;
                return Ok(ExecutionResult::new(2));
            }
        };

        // The family's rules (see procmatch): case-insensitive, `.exe` optional.
        let matcher = match options.pattern.as_deref() {
            Some(pattern) => match NameMatcher::pattern(pattern, options.exact) {
                Ok(matcher) => Some(matcher),
                Err(error) => {
                    writeln!(context.stderr(), "pgrep: invalid pattern: {error}")?;
                    return Ok(ExecutionResult::new(2));
                }
            },
            None => None,
        };

        let found = matching(&options, matcher.as_ref());
        if !options.quiet {
            let mut stdout = context.stdout();
            if options.count {
                writeln!(stdout, "{}", found.len())?;
            } else if !found.is_empty() {
                let shown: Vec<String> = found
                    .iter()
                    .map(|process| {
                        if options.list_name {
                            format!("{} {}", process.pid, process.name)
                        } else {
                            process.pid.to_string()
                        }
                    })
                    .collect();
                writeln!(stdout, "{}", shown.join(&options.delimiter))?;
            }
        }

        Ok(if found.is_empty() {
            ExecutionResult::general_error()
        } else {
            ExecutionResult::success()
        })
    }
}

/// The processes `options` select, in the order Windows lists them; `-n` and `-o` leave
/// the one started last or first.
fn matching(
    options: &Options,
    matcher: Option<&NameMatcher>,
) -> Vec<cash_win32::process::ProcessInfo> {
    // A process whose parent pid names one of these but which started before it is an
    // orphan of an earlier process that had the same pid, not a child.
    let parents: Vec<(u32, Option<u64>)> = options
        .parents
        .iter()
        .map(|&pid| (pid, cash_win32::process::started(pid)))
        .collect();
    let now = cash_win32::process::now_filetime();
    let mut found: Vec<cash_win32::process::ProcessInfo> = cash_win32::process::list()
        .into_iter()
        .filter(|process| {
            parents.is_empty()
                || parents
                    .iter()
                    .any(|&(pid, started)| cash_win32::process::is_child_of(process, pid, started))
        })
        .filter(|process| options.pids.is_empty() || options.pids.contains(&process.pid))
        .filter(|process| {
            // FILETIME counts 100 ns; a start that cannot be read is never old enough.
            options.older.is_none_or(|seconds| {
                cash_win32::process::started(process.pid)
                    .is_some_and(|at| now.saturating_sub(at) / 10_000_000 >= seconds)
            })
        })
        .filter(|process| {
            matcher.is_none_or(|matcher| matcher.matches(&process.name)) != options.inverse
        })
        .collect();
    if options.newest || options.oldest {
        // A process whose start time cannot be read is never the newest or oldest.
        let timed = found
            .iter()
            .filter_map(|p| cash_win32::process::started(p.pid).map(|at| (at, p.pid)));
        let pick = if options.newest {
            timed.max()
        } else {
            timed.min()
        }
        .map(|(_, pid)| pid);
        found.retain(|p| Some(p.pid) == pick);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(line: &str) -> Result<Options, Stop> {
        let words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        parse(&words)
    }

    #[test]
    fn the_command_line_is_read_as_procps_reads_it() {
        let options = parsed("-l -d, --inv -P 4,5 -p 7 -O 30 --exa x").unwrap();
        assert!(options.list_name && options.inverse && options.exact);
        assert_eq!(options.delimiter, ",");
        assert_eq!((options.parents, options.pids), (vec![4, 5], vec![7]));
        assert_eq!(options.older, Some(30));
        assert_eq!(options.pattern.as_deref(), Some("x"));
        assert!(parsed("--quiet -c x").unwrap().quiet);
        // procps reads -O with atoi: the digits it starts with, else 0.
        assert_eq!(parsed("-O x y").unwrap().older, Some(0));
        assert_eq!(parsed("-O 12abc y").unwrap().older, Some(12));
        assert_eq!(parsed("-O -5 y").unwrap().older, Some(0));
        assert_eq!(parsed("-h").err(), Some(Stop::Help));
        assert_eq!(parsed("-V").err(), Some(Stop::Version));
    }

    #[test]
    fn mistakes_are_said_as_procps_says_them() {
        assert_eq!(
            parsed("").err(),
            Some(Stop::Mistake("no matching criteria specified".to_owned()))
        );
        assert_eq!(
            parsed("-Z x").err(),
            Some(Stop::Usage("invalid option -- 'Z'".to_owned()))
        );
        assert_eq!(
            parsed("a b").err(),
            Some(Stop::Mistake("only one pattern can be provided".to_owned()))
        );
        let Some(Stop::Mistake(message)) = parsed("-a x").err() else {
            panic!("-a is refused")
        };
        assert!(message.starts_with("-a:"), "{message}");
        assert_eq!(
            parsed("-n -o x").err(),
            Some(Stop::Mistake("-n and -o are mutually exclusive".to_owned()))
        );
    }
}
