//! `nice` and `renice` on Windows' priority classes.
//!
//! `nice` takes GNU coreutils 9.11's options and `renice` util-linux 2.42.3's, with their
//! messages and exit statuses. A niceness picks one of Windows' six priority classes
//! (`cash_win32::priority`): -20..-11 high, -10..-1 above normal, 0 normal, 1..10 below
//! normal, 11..19 idle; a class reads back as the middle of its range, so `nice -n 10
//! nice` prints 5 and `renice -n 7 PID` reports `new priority 5`. Realtime is never set.
//!
//! `nice COMMAND` runs the command as the shell runs one (`run_for_builtin`: a builtin, or
//! a program found on the shell's `PATH`), with the class in its `ExecutionParameters`
//! (`priority_class`), which the spawn path passes to `CreateProcess` as a creation flag:
//! the program is created in the class. Letting the program inherit the class from the
//! shell was tried first and is not enough: Windows has a child inherit only idle and
//! below normal, and creates it normal under a parent of any higher class, so `nice -n
//! -5` did nothing. A builtin named by `nice` runs inside the shell, at the shell's own
//! priority; the page says so.
//!
//! Raising priority (a negative adjustment) needs no privilege on Windows for anything
//! short of realtime, unlike on Unix.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use cash_win32::priority::{self, PriorityClass, clamp_niceness};
use clap::Parser;

/// GNU's exit status for `nice`'s own failures.
const CANCELED: u8 = 125;
/// GNU's exit status for a command found but not runnable.
const CANNOT_INVOKE: u8 = 126;
/// GNU's exit status for a command not found.
const NOT_FOUND: u8 = 127;

/// `nice --help`, GNU's words with the Windows mapping added.
const NICE_USAGE: &str = "\
Usage: nice [OPTION] [COMMAND [ARG]...]
Run COMMAND with an adjusted niceness, which affects process scheduling.
With no COMMAND, print the current niceness.  Niceness values range from
-20 (most favorable to the process) to 19 (least favorable to the process).

Mandatory arguments to long options are mandatory for short options too.
  -n, --adjustment=N   add integer N to the niceness (default 10)
      --help        display this help and exit
      --version     output version information and exit

On Windows a niceness picks one of the six priority classes: -20..-11 high,
-10..-1 above normal, 0 normal, 1..10 below normal, 11..19 idle; a class
reads back as the middle of its range.  Raising priority needs no privilege.

Exit status:
  125  if the nice command itself fails
  126  if COMMAND is found but cannot be invoked
  127  if COMMAND cannot be found
  -    the exit status of COMMAND otherwise
";

/// Run a command with an adjusted niceness.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct NiceCommand {
    /// Options and the command, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// What `nice`'s command line asked for.
#[derive(Debug, PartialEq, Eq)]
enum NiceRequest {
    Help,
    Version,
    /// Print the current niceness.
    Show,
    /// Run `command` with `adjustment` added to the niceness.
    Run {
        adjustment: i32,
        command: Vec<String>,
    },
}

/// Why `nice` refused its command line (exit status 125).
#[derive(Debug, PartialEq, Eq)]
struct NiceRefused {
    message: String,
    /// Whether `Try 'nice --help'` follows.
    try_help: bool,
}

/// The adjustment as GNU reads it: a number, brought to -39..39 silently (`setpriority`
/// clamps the sum further; here the class the sum lands in does).
fn parse_adjustment(text: &str) -> Result<i64, NiceRefused> {
    let trimmed = text.trim();
    let value = trimmed
        .strip_prefix('+')
        .unwrap_or(trimmed)
        .parse::<i64>()
        .map_err(|_| NiceRefused {
            message: std::format!("invalid adjustment '{text}'"),
            try_help: false,
        })?;
    Ok(value.clamp(-39, 39))
}

/// Reads `nice`'s command line as GNU's `getopt_long("+n:")` loop with the old `-N`
/// form does: options stop at the first word that is not one, the last `-n` wins, and
/// an adjustment out of range is brought into it silently.
fn parse_nice(args: &[String]) -> Result<NiceRequest, NiceRefused> {
    let mut given: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let word = args[index].as_str();
        // The old form comes first, as in GNU: `-10`, `--5`, `-+3`.
        if let Some(short) = word.strip_prefix('-') {
            let digits_at = usize::from(short.starts_with(['-', '+']));
            if short
                .get(digits_at..)
                .unwrap_or("")
                .starts_with(|c: char| c.is_ascii_digit())
            {
                given = Some(short.to_owned());
                index += 1;
                continue;
            }
        }
        if word == "--" {
            index += 1;
            break;
        }
        if let Some(long) = word.strip_prefix("--") {
            let (name, value) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            match name {
                "help" if value.is_none() => return Ok(NiceRequest::Help),
                "version" if value.is_none() => return Ok(NiceRequest::Version),
                "adjustment" => {
                    if let Some(value) = value {
                        given = Some(value.to_owned());
                        index += 1;
                    } else {
                        let Some(value) = args.get(index + 1) else {
                            return Err(NiceRefused {
                                message: "option '--adjustment' requires an argument".to_owned(),
                                try_help: true,
                            });
                        };
                        given = Some(value.clone());
                        index += 2;
                    }
                    continue;
                }
                _ => {
                    return Err(NiceRefused {
                        message: std::format!("unrecognized option '{word}'"),
                        try_help: true,
                    });
                }
            }
        }
        let Some(short) = word.strip_prefix('-').filter(|rest| !rest.is_empty()) else {
            break;
        };
        if let Some(rest) = short.strip_prefix('n') {
            if rest.is_empty() {
                let Some(value) = args.get(index + 1) else {
                    return Err(NiceRefused {
                        message: "option requires an argument -- 'n'".to_owned(),
                        try_help: true,
                    });
                };
                given = Some(value.clone());
                index += 2;
            } else {
                given = Some(rest.to_owned());
                index += 1;
            }
            continue;
        }
        let letter = short.chars().next().unwrap_or('-');
        return Err(NiceRefused {
            message: std::format!("invalid option -- '{letter}'"),
            try_help: true,
        });
    }

    let adjustment = given.as_deref().map(parse_adjustment).transpose()?;
    let command: Vec<String> = args[index..].to_vec();
    if command.is_empty() {
        if adjustment.is_some() {
            return Err(NiceRefused {
                message: "a command must be given with an adjustment".to_owned(),
                try_help: true,
            });
        }
        return Ok(NiceRequest::Show);
    }
    Ok(NiceRequest::Run {
        adjustment: i32::try_from(adjustment.unwrap_or(10)).unwrap_or(10),
        command,
    })
}

/// The niceness a program started now would have: the class an enclosing `nice` asked
/// for, else the shell's own, read back. Under `nice -n 10`, a `nice` builtin so prints
/// 5, as GNU's prints 10 in the process the outer one adjusted.
fn current_niceness(asked: Option<PriorityClass>) -> std::io::Result<i32> {
    match asked {
        Some(class) => Ok(class.niceness()),
        None => priority::current().map(PriorityClass::niceness),
    }
}

impl builtins::Command for NiceCommand {
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
        let (adjustment, command) = match parse_nice(&self.args) {
            Ok(NiceRequest::Help) => {
                write!(context.stdout(), "{NICE_USAGE}")?;
                return Ok(ExecutionResult::success());
            }
            Ok(NiceRequest::Version) => {
                writeln!(
                    context.stdout(),
                    "nice (cash): GNU coreutils 9.11's options, on the Windows priority classes"
                )?;
                return Ok(ExecutionResult::success());
            }
            Ok(NiceRequest::Show) => {
                return match current_niceness(context.params.priority_class) {
                    Ok(niceness) => {
                        writeln!(context.stdout(), "{niceness}")?;
                        Ok(ExecutionResult::success())
                    }
                    Err(error) => {
                        writeln!(
                            context.stderr(),
                            "nice: cannot get niceness: {}",
                            cash_core::error::os_error_text(&error)
                        )?;
                        Ok(ExecutionResult::new(CANCELED))
                    }
                };
            }
            Ok(NiceRequest::Run {
                adjustment,
                command,
            }) => (adjustment, command),
            Err(refused) => {
                let mut stderr = context.stderr();
                writeln!(stderr, "nice: {}", refused.message)?;
                if refused.try_help {
                    writeln!(stderr, "Try 'nice --help' for more information.")?;
                }
                return Ok(ExecutionResult::new(CANCELED));
            }
        };

        let niceness = current_niceness(context.params.priority_class).unwrap_or(0);
        let class = PriorityClass::for_niceness(clamp_niceness(
            i64::from(niceness) + i64::from(adjustment),
        ));
        // The program is created in the class (see the module's notes).
        let mut params = context.params.clone();
        params.priority_class = Some(class);
        let result = cash_core::commands::run_for_builtin(context.shell, params, &command).await;
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                let program = command.first().map_or("", String::as_str);
                let status = if matches!(error.kind(), cash_core::ErrorKind::CommandNotFound(_)) {
                    NOT_FOUND
                } else {
                    CANNOT_INVOKE
                };
                writeln!(
                    context.stderr(),
                    "nice: '{program}': {}",
                    crate::xargs::start_failure(&error)
                )?;
                Ok(ExecutionResult::new(status))
            }
        }
    }
}

/// `renice --help`, util-linux's words, with `-g` marked as what Windows lacks.
const RENICE_USAGE: &str = "
Usage:
 renice [-n|--priority|--relative] <priority> [-p|--pid] <pid>...
 renice [-n|--priority|--relative] <priority>  -u|--user <user>...

Alter the priority of running processes.

Options:
 -n <num>               specify the 'absolute' nice value,
                          but 'relative' when POSIXLY_CORRECT is set
 --priority <num>       specify the 'absolute' nice value
 --relative <num>       specify the 'relative' nice value
 -p, --pid              interpret arguments as process ID (default)
 -g, --pgrp             interpret arguments as process group ID (Windows has none)
 -u, --user             interpret arguments as username

 -h, --help             display this help
 -V, --version          display version

A priority picks one of the six Windows priority classes: -20..-11 high,
-10..-1 above normal, 0 normal, 1..10 below normal, 11..19 idle; a class
reads back as the middle of its range.

For more details see renice(1).
";

/// Alter the priority of running processes.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ReniceCommand {
    /// The priority, the selectors and the ids, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// What the ids after the priority name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Pid,
    ProcessGroup,
    User,
}

/// What `renice`'s command line asked for.
#[derive(Debug, PartialEq, Eq)]
enum ReniceRequest {
    Help,
    Version,
    /// `priority`, absolute or `relative`, for the `ids` as the selector before each
    /// says.
    Set {
        priority: i64,
        relative: bool,
        ids: Vec<(Which, String)>,
    },
}

/// Reads `renice`'s command line as util-linux does: the priority first, after an
/// optional `-n`, `--priority` or `--relative`; then selectors and ids in any order.
fn parse_renice(args: &[String], posixly_correct: bool) -> Result<ReniceRequest, String> {
    if let [only] = args {
        match only.as_str() {
            "-h" | "--help" => return Ok(ReniceRequest::Help),
            "-v" | "-V" | "--version" => return Ok(ReniceRequest::Version),
            _ => {}
        }
    }
    let mut rest = args;
    let mut relative = false;
    if let Some(first) = rest.first() {
        match first.as_str() {
            "-n" => {
                relative = posixly_correct;
                rest = &rest[1..];
            }
            "--priority" => rest = &rest[1..],
            "--relative" => {
                relative = true;
                rest = &rest[1..];
            }
            _ => {}
        }
    }
    let [priority, ids @ ..] = rest else {
        return Err("not enough arguments".to_owned());
    };
    if ids.is_empty() {
        return Err("not enough arguments".to_owned());
    }
    let text = priority.trim();
    let priority = text
        .strip_prefix('+')
        .unwrap_or(text)
        .parse::<i64>()
        .map_err(|_| std::format!("invalid priority '{priority}'"))?;
    let mut which = Which::Pid;
    let mut selected = Vec::new();
    for id in ids {
        match id.as_str() {
            "-g" | "--pgrp" => which = Which::ProcessGroup,
            "-u" | "--user" => which = Which::User,
            "-p" | "--pid" => which = Which::Pid,
            _ => selected.push((which, id.clone())),
        }
    }
    Ok(ReniceRequest::Set {
        priority,
        relative,
        ids: selected,
    })
}

/// The C library's words for what Windows said.
fn reason(error: &std::io::Error) -> String {
    match error.raw_os_error() {
        // `OpenProcess` of a pid nothing has.
        Some(87) => "No such process".to_owned(),
        Some(5) => "Permission denied".to_owned(),
        _ => cash_core::error::os_error_text(error),
    }
}

/// Sets one process's class, reporting as util-linux does; whether it worked.
fn renice_process(
    pid: u32,
    priority: i64,
    relative: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<bool> {
    let old = match priority::of_process(pid) {
        Ok(class) => class.niceness(),
        Err(error) => {
            writeln!(
                err,
                "renice: failed to get priority for {pid} (process ID): {}",
                reason(&error)
            )?;
            return Ok(false);
        }
    };
    let wanted = if relative {
        i64::from(old) + priority
    } else {
        priority
    };
    let class = PriorityClass::for_niceness(clamp_niceness(wanted));
    match priority::set_process(pid, class) {
        Ok(()) => {
            writeln!(
                out,
                "{pid} (process ID) old priority {old}, new priority {}",
                class.niceness()
            )?;
            Ok(true)
        }
        Err(error) => {
            writeln!(
                err,
                "renice: failed to set priority for {pid} (process ID): {}",
                reason(&error)
            )?;
            Ok(false)
        }
    }
}

/// The pids of the processes the account `name` (`tom`, `PC\tom`) runs, or `None` when
/// there is no such user.
fn processes_of(name: &str) -> Option<Vec<u32>> {
    let sid = cash_win32::account::lookup_user(name)?;
    let account = cash_win32::account::account_name(&sid)?;
    let wanted = account.rsplit('\\').next().unwrap_or(&account).to_owned();
    Some(
        cash_win32::process::list()
            .into_iter()
            .filter(|process| {
                cash_win32::process::owner(process.pid)
                    .is_some_and(|owner| owner.eq_ignore_ascii_case(&wanted))
            })
            .map(|process| process.pid)
            .collect(),
    )
}

impl builtins::Command for ReniceCommand {
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
        let posixly_correct = context.shell.env_str("POSIXLY_CORRECT").is_some();
        let (priority, relative, ids) = match parse_renice(&self.args, posixly_correct) {
            Ok(ReniceRequest::Help) => {
                write!(context.stdout(), "{RENICE_USAGE}")?;
                return Ok(ExecutionResult::success());
            }
            Ok(ReniceRequest::Version) => {
                writeln!(
                    context.stdout(),
                    "renice (cash): util-linux 2.42.3's options, on the Windows priority classes"
                )?;
                return Ok(ExecutionResult::success());
            }
            Ok(ReniceRequest::Set {
                priority,
                relative,
                ids,
            }) => (priority, relative, ids),
            Err(message) => {
                writeln!(
                    context.stderr(),
                    "renice: {message}\nTry 'renice --help' for more information."
                )?;
                return Ok(ExecutionResult::general_error());
            }
        };

        let mut out = context.stdout();
        let mut err = context.stderr();
        let mut failed = false;
        for (which, id) in ids {
            match which {
                Which::Pid => {
                    if let Ok(pid) = id.trim().parse::<u32>() {
                        failed |= !renice_process(pid, priority, relative, &mut out, &mut err)?;
                    } else {
                        writeln!(err, "renice: bad process ID value: {id}")?;
                        failed = true;
                    }
                }
                Which::ProcessGroup => {
                    writeln!(
                        err,
                        "renice: -g {id}: Windows has no process groups; use -p for a process or -u for a user's processes"
                    )?;
                    failed = true;
                }
                Which::User => {
                    if let Some(pids) = processes_of(&id) {
                        for pid in pids {
                            failed |= !renice_process(pid, priority, relative, &mut out, &mut err)?;
                        }
                    } else {
                        writeln!(err, "renice: unknown user {id}")?;
                        failed = true;
                    }
                }
            }
        }
        Ok(if failed {
            ExecutionResult::general_error()
        } else {
            ExecutionResult::success()
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests assert loudly on failure"
)]
mod tests {
    use super::*;

    fn words(args: &[&str]) -> Vec<String> {
        args.iter().map(|&a| a.to_owned()).collect()
    }

    fn nice(args: &[&str]) -> NiceRequest {
        parse_nice(&words(args)).unwrap_or_else(|refused| panic!("{refused:?} for {args:?}"))
    }

    fn run(adjustment: i32, command: &[&str]) -> NiceRequest {
        NiceRequest::Run {
            adjustment,
            command: words(command),
        }
    }

    #[test]
    fn nice_reads_gnus_forms() {
        assert_eq!(nice(&[]), NiceRequest::Show);
        assert_eq!(nice(&["--help"]), NiceRequest::Help);
        assert_eq!(nice(&["--version"]), NiceRequest::Version);
        assert_eq!(nice(&["cmd", "-n", "5"]), run(10, &["cmd", "-n", "5"]));
        assert_eq!(nice(&["-n", "5", "cmd", "a"]), run(5, &["cmd", "a"]));
        assert_eq!(nice(&["-n5", "cmd"]), run(5, &["cmd"]));
        assert_eq!(nice(&["-n", "-5", "cmd"]), run(-5, &["cmd"]));
        assert_eq!(nice(&["--adjustment=-20", "cmd"]), run(-20, &["cmd"]));
        assert_eq!(nice(&["--adjustment", "+3", "cmd"]), run(3, &["cmd"]));
        // The old form: `-10` is an adjustment of 10, `--5` of -5.
        assert_eq!(nice(&["-10", "cmd"]), run(10, &["cmd"]));
        assert_eq!(nice(&["--5", "cmd"]), run(-5, &["cmd"]));
        assert_eq!(nice(&["-+7", "cmd"]), run(7, &["cmd"]));
        // The last adjustment wins, and out of range is brought into GNU's -39..39.
        assert_eq!(nice(&["-n", "5", "-n", "3", "cmd"]), run(3, &["cmd"]));
        assert_eq!(nice(&["-n", "100", "cmd"]), run(39, &["cmd"]));
        assert_eq!(nice(&["-n", "-100", "cmd"]), run(-39, &["cmd"]));
        // `--` ends the options: `-5` is then the command.
        assert_eq!(nice(&["--", "-5"]), run(10, &["-5"]));
    }

    #[test]
    fn nice_refuses_as_gnu_does() {
        let refused = |args: &[&str]| parse_nice(&words(args)).unwrap_err();
        assert_eq!(
            refused(&["-n", "abc", "cmd"]),
            NiceRefused {
                message: "invalid adjustment 'abc'".to_owned(),
                try_help: false
            }
        );
        assert_eq!(
            refused(&["-n"]),
            NiceRefused {
                message: "option requires an argument -- 'n'".to_owned(),
                try_help: true
            }
        );
        assert_eq!(
            refused(&["-n", "3"]),
            NiceRefused {
                message: "a command must be given with an adjustment".to_owned(),
                try_help: true
            }
        );
        assert_eq!(
            refused(&["-x", "cmd"]),
            NiceRefused {
                message: "invalid option -- 'x'".to_owned(),
                try_help: true
            }
        );
        assert_eq!(
            refused(&["--bogus"]),
            NiceRefused {
                message: "unrecognized option '--bogus'".to_owned(),
                try_help: true
            }
        );
    }

    #[test]
    fn a_niceness_and_its_class_agree_both_ways() {
        // The mapping `nice` and `renice` share: what `nice -n N nice` prints.
        let read_back = |n: i64| PriorityClass::for_niceness(clamp_niceness(n)).niceness();
        assert_eq!(read_back(10), 5);
        assert_eq!(read_back(19), 15);
        assert_eq!(read_back(-5), -5);
        assert_eq!(read_back(-20), -15);
        assert_eq!(read_back(0), 0);
        assert_eq!(read_back(100), 15);
        assert_eq!(read_back(5 + 10), 15);
    }

    fn renice(args: &[&str]) -> ReniceRequest {
        parse_renice(&words(args), false).unwrap_or_else(|message| panic!("{message} for {args:?}"))
    }

    #[test]
    fn renice_reads_util_linuxs_forms() {
        let set = |priority, relative, ids: &[(Which, &str)]| ReniceRequest::Set {
            priority,
            relative,
            ids: ids.iter().map(|&(w, id)| (w, id.to_owned())).collect(),
        };
        assert_eq!(renice(&["-h"]), ReniceRequest::Help);
        assert_eq!(renice(&["--version"]), ReniceRequest::Version);
        assert_eq!(
            renice(&["-n", "5", "-p", "100"]),
            set(5, false, &[(Which::Pid, "100")])
        );
        assert_eq!(
            renice(&["+7", "100"]),
            set(7, false, &[(Which::Pid, "100")])
        );
        assert_eq!(
            renice(&["--relative", "2", "100", "200"]),
            set(2, true, &[(Which::Pid, "100"), (Which::Pid, "200")])
        );
        assert_eq!(
            renice(&["--priority", "-3", "-u", "tom", "-p", "8"]),
            set(-3, false, &[(Which::User, "tom"), (Which::Pid, "8")])
        );
        assert_eq!(
            renice(&["5", "-g", "1"]),
            set(5, false, &[(Which::ProcessGroup, "1")])
        );
        // `-n` is relative only under POSIXLY_CORRECT.
        assert_eq!(
            parse_renice(&words(&["-n", "5", "100"]), true).unwrap(),
            set(5, true, &[(Which::Pid, "100")])
        );
    }

    #[test]
    fn renice_refuses_as_util_linux_does() {
        let refused = |args: &[&str]| parse_renice(&words(args), false).unwrap_err();
        assert_eq!(refused(&[]), "not enough arguments");
        assert_eq!(refused(&["5"]), "not enough arguments");
        assert_eq!(refused(&["-n", "7"]), "not enough arguments");
        assert_eq!(refused(&["abc", "5"]), "invalid priority 'abc'");
        assert_eq!(refused(&["-g", "1"]), "invalid priority '-g'");
        assert_eq!(refused(&["-p", "5", "100"]), "invalid priority '-p'");
    }

    #[test]
    fn windows_errors_get_the_c_librarys_words() {
        assert_eq!(
            reason(&std::io::Error::from_raw_os_error(87)),
            "No such process"
        );
        assert_eq!(
            reason(&std::io::Error::from_raw_os_error(5)),
            "Permission denied"
        );
    }
}
