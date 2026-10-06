//! `flock`, util-linux's: run a command, or hold a shell descriptor, under a lock on a
//! file, so that two scripts take turns.
//!
//! Checked against util-linux 2.42.3 (`crates/cash/tests/oracle`): its options with GNU
//! `getopt_long`'s rules (options end at the first non-option, `--name` by unique prefix,
//! `-c`/`--command` recognised only as the word after the file), its messages and its
//! `sysexits.h` statuses, and `-E`'s code for a lock not got, whether refused (`-n`) or
//! timed out (`-w`).
//!
//! Windows has no `flock(2)`. A lock here is a byte-range lock (`LockFileEx`,
//! [`cash_win32::filelock`]) on one byte far past any content the file could have, so it
//! stops nothing but another `flock`: `cat` and writers of the lock file go on. A lock
//! belongs to the handle it was taken on. In the command form that handle is opened here
//! and closed when the command returns; in the descriptor form it is the shell's own
//! descriptor, and the lock lasts as long as that stays open — through the subshell or
//! `exec N>&-` that closes it — as a Linux lock lives on the open file description.
//!
//! `LockFileEx` cannot be told to wait with a timeout, nor interrupted while it waits, so
//! a wait tries the lock every 25 ms instead, and a Ctrl-C ends it as it ends a program:
//! the trap on `INT` runs, or the script ends with 130.
//!
//! Deliberate differences: a directory cannot be locked (`LockFileEx` takes no directory
//! handle) and is refused with the status of a lock file that could not be opened; `-o`
//! closes nothing, since there is no fork and the command never sees the handle; `-c`
//! runs its string in a copy of this shell, as the `sh -c` it stands for would, rather
//! than in `$SHELL`; `--verbose`'s lines on standard output come before the command's
//! output, where util-linux's buffering puts them after it in a pipe; and a command that
//! cannot be started is reported with util-linux's words and status 69, not Bash's 127.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;
use std::time::{Duration, Instant};

use cash_core::openfiles::OpenFile;
use cash_core::{ExecutionResult, builtins};
use cash_win32::filelock::{self, Range};
use clap::Parser;

/// `sysexits.h`'s `EX_USAGE`: the command line was wrong.
const EX_USAGE: u8 = 64;
/// `EX_DATAERR`: util-linux's for a lock that failed for any reason but a conflict.
const EX_DATAERR: u8 = 65;
/// `EX_NOINPUT`: the lock file could not be opened.
const EX_NOINPUT: u8 = 66;
/// `EX_UNAVAILABLE`: the command could not be started.
const EX_UNAVAILABLE: u8 = 69;
/// `EX_OSERR`: the timer for `-w` could not be set up.
const EX_OSERR: u8 = 71;
/// A command ended by Ctrl-C, as Bash reports one: 128 plus `SIGINT`.
const INTERRUPTED: u8 = 130;

/// How often a wait tries the lock again.
const POLL: Duration = Duration::from_millis(25);

const VERSION: &str = "flock (cash): util-linux 2.42.3's options, on LockFileEx";

/// util-linux 2.42.3's `--help`, which starts with an empty line as it does.
const HELP: &str = "
Usage:
 flock [options] <file>|<directory> <command> [<argument>...]
 flock [options] <file>|<directory> -c <command>
 flock [options] <file descriptor number>

Manage file locks from shell scripts.

Options:
 -s, --shared             get a shared lock
 -x, --exclusive          get an exclusive lock (default)
 -u, --unlock             remove a lock
 -n, --nb, --nonblocking  fail rather than wait
 -w, --timeout <secs>     wait for a limited amount of time
 -E, --conflict-exit-code <number>  exit code after conflict or timeout
 -o, --close              close file descriptor before running command
 -c, --command <command>  run a single command string through the shell
 -F, --no-fork            execute command without forking
     --wait               same as --timeout
     --fcntl              use fcntl(F_OFD_SETLK) rather than flock()
     --start <offset>     starting offset for lock (implies --fcntl)
     --length <number>    number of bytes to lock (implies --fcntl)
     --verbose            increase verbosity

 -h, --help               display this help
 -V, --version            display version

For more details see flock(1).
";

/// Manage file locks from shell scripts.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct FlockCommand {
    /// Options, the file or descriptor, and the command: parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// The long options, in util-linux's order (which an ambiguity message follows), each
/// with whether it takes a value.
const LONG_OPTIONS: &[(&str, bool)] = &[
    ("shared", false),
    ("exclusive", false),
    ("unlock", false),
    ("nonblocking", false),
    ("nb", false),
    ("timeout", true),
    ("wait", true),
    ("conflict-exit-code", true),
    ("close", false),
    ("no-fork", false),
    ("help", false),
    ("version", false),
    ("fcntl", false),
    ("start", true),
    ("length", true),
    ("verbose", false),
];

/// How long to wait for the lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wait {
    /// Not at all (`-n`, or `-w 0`).
    Now,
    /// Up to this long (`-w`).
    For(Duration),
    /// Until it is got.
    Forever,
}

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Help,
    Version,
    /// Lock, or unlock, one of the shell's descriptors.
    Descriptor(i32),
    /// Lock a file and run a command.
    Command {
        file: String,
        argv: Vec<String>,
    },
    /// Lock a file and run a string through the shell (`-c`).
    Script {
        file: String,
        script: String,
    },
}

/// The command line, read.
#[derive(Debug, PartialEq, Eq)]
struct Options {
    exclusive: bool,
    unlock: bool,
    wait: Wait,
    /// The status when the lock is not got: `-E`, or 1.
    conflict_code: u8,
    verbose: bool,
    range: Range,
    action: Action,
}

/// A command line refused, with util-linux's message and status, and whether the hint
/// `Try 'flock --help'` follows (it does for what `getopt` refuses, and for too few
/// arguments).
#[derive(Debug, PartialEq, Eq)]
struct Refusal {
    message: String,
    status: u8,
    hint: bool,
}

impl Refusal {
    const fn usage(message: String) -> Self {
        Self {
            message,
            status: EX_USAGE,
            hint: true,
        }
    }

    const fn value(message: String) -> Self {
        Self {
            message,
            status: EX_USAGE,
            hint: false,
        }
    }
}

/// The options as they are read, before `-n` and `-w` are reconciled.
#[derive(Default)]
struct Reading {
    shared: bool,
    unlock: bool,
    nonblock: bool,
    timeout: Option<Duration>,
    conflict_code: Option<u8>,
    verbose: bool,
    start: Option<u64>,
    length: Option<u64>,
    close: bool,
    no_fork: bool,
    help: bool,
    version: bool,
}

impl Reading {
    /// Applies one option by its short letter; `value` is the argument of `w`, `E`,
    /// `start` and `length`.
    fn apply(&mut self, letter: char, value: Option<&str>) -> Result<(), Refusal> {
        match letter {
            's' => self.shared = true,
            'x' | 'e' => self.shared = false,
            'u' => self.unlock = true,
            'n' => self.nonblock = true,
            'w' => self.timeout = Some(parse_timeout(value.unwrap_or_default())?),
            'E' => self.conflict_code = Some(parse_exit_code(value.unwrap_or_default())?),
            'h' => self.help = true,
            'V' => self.version = true,
            // `-o` closes nothing: there is no fork, and the command never sees the
            // handle. `-F` runs the command as every command here runs. Together they
            // are still refused, as util-linux refuses them. `--fcntl` is the only kind
            // of lock there is.
            'o' => self.close = true,
            'F' => self.no_fork = true,
            'f' => {}
            'S' => self.start = Some(parse_offset(value.unwrap_or_default(), "start offset")?),
            'L' => {
                self.length = Some(parse_offset(
                    value.unwrap_or_default(),
                    "length of lock range",
                )?);
            }
            'v' => self.verbose = true,
            _ => unreachable!("every letter the parser dispatches is matched"),
        }
        Ok(())
    }
}

/// The letter a long option is applied under.
fn long_letter(name: &str) -> char {
    match name {
        "shared" => 's',
        "exclusive" => 'x',
        "unlock" => 'u',
        "nonblocking" | "nb" => 'n',
        "timeout" | "wait" => 'w',
        "conflict-exit-code" => 'E',
        "close" => 'o',
        "no-fork" => 'F',
        "help" => 'h',
        "version" => 'V',
        "fcntl" => 'f',
        "start" => 'S',
        "length" => 'L',
        _ => 'v',
    }
}

/// `-w`'s value: seconds, fractions allowed, as `strtod` reads them. Not a number is
/// refused as util-linux refuses it; a negative or infinite one fails its timer.
fn parse_timeout(text: &str) -> Result<Duration, Refusal> {
    let Ok(seconds) = text.trim_start().parse::<f64>() else {
        return Err(Refusal::value(format!("invalid timeout: '{text}'")));
    };
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(Refusal {
            message: "cannot set up timer: Invalid argument".to_owned(),
            status: EX_OSERR,
            hint: false,
        });
    }
    Ok(Duration::from_secs_f64(seconds))
}

/// `-E`'s value: a status, 0 to 255.
fn parse_exit_code(text: &str) -> Result<u8, Refusal> {
    let Ok(code) = text.trim_start().parse::<i64>() else {
        return Err(Refusal::value(format!("invalid exit code: '{text}'")));
    };
    u8::try_from(code)
        .map_err(|_| Refusal::value("exit code out of range (expected 0 to 255)".to_owned()))
}

/// `--start`'s and `--length`'s values: a byte count, not negative.
fn parse_offset(text: &str, what: &str) -> Result<u64, Refusal> {
    text.trim_start()
        .parse::<u64>()
        .map_err(|_| Refusal::value(format!("invalid as {what}: '{text}': Invalid argument")))
}

/// A descriptor number as `strtos32` reads one: `9`, ` 9`, `+9`, `-1`.
fn parse_descriptor(text: &str) -> Option<i32> {
    text.trim_start().parse::<i32>().ok()
}

/// Reads the command line as util-linux does: `getopt_long` with options ending at the
/// first non-option, then the file or descriptor and the command.
#[expect(
    clippy::too_many_lines,
    reason = "one pass over the command line, long and short options alike"
)]
fn parse(args: &[String]) -> Result<Options, Refusal> {
    let mut reading = Reading::default();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            index += 1;
            break;
        }
        if !arg.starts_with('-') || arg == "-" {
            break;
        }
        index += 1;
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            let (full, takes_value) = find_long(name, arg)?;
            let letter = long_letter(full);
            if takes_value {
                let value = if let Some(value) = value {
                    value
                } else {
                    let Some(next) = args.get(index) else {
                        return Err(Refusal::usage(format!(
                            "option '--{full}' requires an argument"
                        )));
                    };
                    index += 1;
                    next
                };
                reading.apply(letter, Some(value))?;
            } else {
                if value.is_some() {
                    return Err(Refusal::usage(format!(
                        "option '--{full}' doesn't allow an argument"
                    )));
                }
                reading.apply(letter, None)?;
            }
            continue;
        }
        let short = arg.strip_prefix('-').unwrap_or(arg);
        for (at, letter) in short.char_indices() {
            match letter {
                's' | 'x' | 'e' | 'u' | 'n' | 'o' | 'F' | 'h' | 'V' => {
                    reading.apply(letter, None)?;
                }
                'w' | 'E' => {
                    let rest = short.get(at + letter.len_utf8()..).unwrap_or("");
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
                other => {
                    return Err(Refusal::usage(format!("invalid option -- '{other}'")));
                }
            }
        }
    }

    if reading.help {
        return Ok(Options::for_action(&reading, Action::Help));
    }
    if reading.version {
        return Ok(Options::for_action(&reading, Action::Version));
    }
    if reading.close && reading.no_fork {
        return Err(Refusal::value(
            "the --no-fork and --close options are incompatible".to_owned(),
        ));
    }

    let rest = &args[index..];
    let action = match rest {
        [] => return Err(Refusal::usage("not enough arguments".to_owned())),
        [one] => Action::Descriptor(
            parse_descriptor(one)
                .ok_or_else(|| Refusal::value(format!("bad file descriptor: '{one}'")))?,
        ),
        [file, flag, command @ ..] if flag == "-c" || flag == "--command" => match command {
            [script] => Action::Script {
                file: file.clone(),
                script: script.clone(),
            },
            _ => {
                return Err(Refusal::value(
                    "-c requires exactly one command argument".to_owned(),
                ));
            }
        },
        [file, argv @ ..] => Action::Command {
            file: file.clone(),
            argv: argv.to_vec(),
        },
    };
    Ok(Options::for_action(&reading, action))
}

/// The long option `name` means: itself, or the one it is a unique prefix of. `arg` is
/// the word as typed, which an unrecognized option is reported as.
fn find_long(name: &str, arg: &str) -> Result<(&'static str, bool), Refusal> {
    if let Some(&(full, takes_value)) = LONG_OPTIONS.iter().find(|(full, _)| *full == name) {
        return Ok((full, takes_value));
    }
    let candidates: Vec<&(&str, bool)> = LONG_OPTIONS
        .iter()
        .filter(|(full, _)| !name.is_empty() && full.starts_with(name))
        .collect();
    match candidates.as_slice() {
        [(full, takes_value)] => Ok((full, *takes_value)),
        [] => Err(Refusal::usage(format!("unrecognized option '{arg}'"))),
        many => {
            let possibilities: Vec<String> =
                many.iter().map(|(full, _)| format!("'--{full}'")).collect();
            Err(Refusal::usage(format!(
                "option '--{name}' is ambiguous; possibilities: {}",
                possibilities.join(" ")
            )))
        }
    }
}

impl Options {
    /// The options read, reconciled: `-n`, or a timeout of zero, means no wait at all;
    /// `--start` or `--length` lock those bytes rather than the far one.
    fn for_action(reading: &Reading, action: Action) -> Self {
        let wait = match reading.timeout {
            _ if reading.nonblock => Wait::Now,
            Some(timeout) if timeout.is_zero() => Wait::Now,
            Some(timeout) => Wait::For(timeout),
            None => Wait::Forever,
        };
        let range = match (reading.start, reading.length) {
            (None, None) => Range::FAR_BYTE,
            (start, Some(length)) if length > 0 => Range {
                offset: start.unwrap_or(0),
                length,
            },
            (start, _) => Range::from_offset(start.unwrap_or(0)),
        };
        Self {
            exclusive: !reading.shared,
            unlock: reading.unlock,
            wait,
            conflict_code: reading.conflict_code.unwrap_or(1),
            verbose: reading.verbose,
            range,
            action,
        }
    }
}

/// How a wait for the lock ended.
enum Acquired {
    /// The lock is held.
    Locked,
    /// Another handle holds it, and the wait is over (`-n`, or `-w` ran out).
    Conflict,
    /// Ctrl-C ended the wait.
    Interrupted,
}

impl builtins::Command for FlockCommand {
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
            Ok(options) => options,
            Err(refusal) => {
                let mut stderr = context.stderr();
                writeln!(stderr, "flock: {}", refusal.message)?;
                if refusal.hint {
                    writeln!(stderr, "Try 'flock --help' for more information.")?;
                }
                return Ok(ExecutionResult::new(refusal.status));
            }
        };
        match &options.action {
            Action::Help => {
                write!(context.stdout(), "{HELP}")?;
                Ok(ExecutionResult::success())
            }
            Action::Version => {
                writeln!(context.stdout(), "{VERSION}")?;
                Ok(ExecutionResult::success())
            }
            Action::Descriptor(fd) => lock_descriptor(&mut context, &options, *fd).await,
            Action::Command { file, .. } | Action::Script { file, .. } => {
                lock_file_and_run(&mut context, &options, file).await
            }
        }
    }
}

/// The descriptor form: the lock lives on the shell's own descriptor, as long as that
/// stays open; `-u` takes it off.
async fn lock_descriptor<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    options: &Options,
    fd: i32,
) -> Result<ExecutionResult, cash_core::Error> {
    let Some(open) = context.try_fd(fd) else {
        writeln!(context.stderr(), "flock: {fd}: Bad file descriptor")?;
        return Ok(ExecutionResult::new(EX_DATAERR));
    };
    let OpenFile::File(file) = open else {
        writeln!(
            context.stderr(),
            "flock: {fd}: not a file; Windows can lock only a file"
        )?;
        return Ok(ExecutionResult::new(EX_DATAERR));
    };
    let name = fd.to_string();
    // A lock this handle already holds would refuse the new one as another handle's
    // would, so it goes first: a second `flock 9` holds the lock as before, and `-s` after
    // `-x` converts it, as `flock(2)` converts — the old lock released, then the new one
    // taken.
    if let Err(error) = filelock::unlock(&file, options.range) {
        return report_lock_error(context, &name, &error);
    }
    if options.unlock {
        return Ok(ExecutionResult::success());
    }
    let started = Instant::now();
    match acquire(context, options, &file, &name).await? {
        Ok(Acquired::Locked) => {
            if options.verbose {
                report_taken(context, started)?;
            }
            Ok(ExecutionResult::success())
        }
        Ok(Acquired::Conflict) => Ok(ExecutionResult::new(options.conflict_code)),
        Ok(Acquired::Interrupted) => Ok(ExecutionResult::new(INTERRUPTED)),
        Err(result) => Ok(result),
    }
}

/// The command form: a handle of its own on the file, the lock on it while the command
/// runs, both gone when the command returns.
async fn lock_file_and_run<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    options: &Options,
    name: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    let path = context.shell.absolute_path(Path::new(name));
    if path.is_dir() {
        // util-linux locks a directory as it locks a file; LockFileEx takes no directory
        // handle. Refused with the status of a lock file that could not be opened.
        writeln!(
            context.stderr(),
            "flock: {name}: cannot lock a directory on Windows; use a file inside it"
        )?;
        return Ok(ExecutionResult::new(EX_NOINPUT));
    }
    let file = match open_lock_file(&path) {
        Ok(file) => file,
        Err(error) => {
            let reason = cash_core::error::os_error_text(&error);
            writeln!(
                context.stderr(),
                "flock: cannot open lock file {name}: {reason}"
            )?;
            return Ok(ExecutionResult::new(EX_NOINPUT));
        }
    };

    let started = Instant::now();
    // `-u` with a command: util-linux unlocks a handle that holds nothing, and runs the
    // command.
    if !options.unlock {
        match acquire(context, options, &file, name).await? {
            Ok(Acquired::Locked) => {}
            Ok(Acquired::Conflict) => return Ok(ExecutionResult::new(options.conflict_code)),
            Ok(Acquired::Interrupted) => return Ok(ExecutionResult::new(INTERRUPTED)),
            Err(result) => return Ok(result),
        }
    }
    if options.verbose {
        report_taken(context, started)?;
    }

    let result = match &options.action {
        Action::Command { argv, .. } => {
            if options.verbose {
                let program = argv.first().map_or("", String::as_str);
                writeln!(context.stdout(), "flock: executing {program}")?;
            }
            let params = context.params.clone();
            match cash_core::commands::run_for_builtin(context.shell, params, argv).await {
                // The command's status, and that alone: util-linux runs it as a process
                // of its own, whose `exit` ends nothing else.
                Ok(result) => ExecutionResult::new(u8::from(result.exit_code)),
                Err(error) => {
                    let program = argv.first().map_or("", String::as_str);
                    writeln!(
                        context.stderr(),
                        "flock: failed to execute {program}: {}",
                        crate::xargs::start_failure(&error)
                    )?;
                    ExecutionResult::new(EX_UNAVAILABLE)
                }
            }
        }
        Action::Script { script, .. } => {
            if options.verbose {
                writeln!(context.stdout(), "flock: executing cash")?;
            }
            // The `sh -c` util-linux starts: a shell of its own, with this one's
            // variables and folder, whose `exit` and assignments stay inside it.
            let source_info = context.shell.call_stack().current_pos_as_source_info();
            let mut subshell = context.shell.clone();
            let result = subshell
                .run_string(script.clone(), &source_info, &context.params)
                .await?;
            ExecutionResult::new(u8::from(result.exit_code))
        }
        Action::Help | Action::Version | Action::Descriptor(_) => ExecutionResult::success(),
    };

    let _ = filelock::unlock(&file, options.range);
    drop(file);
    Ok(result)
}

/// The lock file, open to read and write, created if it was not there, or read-only
/// when that is all this account may do with it; shared with every reader and writer,
/// so the open itself keeps nobody out.
fn open_lock_file(path: &Path) -> std::io::Result<File> {
    // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE.
    const SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
    let read_write = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(SHARE_ALL)
        .open(path);
    match read_write {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => OpenOptions::new()
            .read(true)
            .share_mode(SHARE_ALL)
            .open(path),
        Err(error) => Err(error),
    }
}

/// Takes the lock on `file`, waiting as `options.wait` says, 25 ms between tries. A
/// Ctrl-C that arrives meanwhile is acted on as the shell acts on one between commands:
/// the trap on `INT` runs, and its result is the command's if it leaves the normal flow;
/// without a trap the shell's error ends the script with 130.
///
/// `Err(result)` is a failure that is not a conflict, reported, with its status.
async fn acquire<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    options: &Options,
    file: &File,
    name: &str,
) -> Result<Result<Acquired, ExecutionResult>, cash_core::Error> {
    let deadline = match options.wait {
        Wait::Now => None,
        Wait::For(timeout) => Some(Instant::now() + timeout),
        Wait::Forever => Some(Instant::now() + Duration::from_secs(u32::MAX.into())),
    };
    loop {
        match filelock::try_lock(file, options.exclusive, options.range) {
            Ok(true) => return Ok(Ok(Acquired::Locked)),
            Ok(false) => {}
            Err(error) => return report_lock_error(context, name, &error).map(Err),
        }
        let Some(deadline) = deadline else {
            if options.verbose {
                writeln!(context.stderr(), "flock: failed to get lock")?;
            }
            return Ok(Ok(Acquired::Conflict));
        };
        if Instant::now() >= deadline {
            if options.verbose {
                writeln!(context.stderr(), "flock: timeout while waiting to get lock")?;
            }
            return Ok(Ok(Acquired::Conflict));
        }
        // A job in the background hears no Ctrl-C of its own, as in `interp.rs`.
        if !context.params.is_asynchronous() && cash_win32::console::take_interrupt() {
            let trap_result = context.shell.interrupt(&context.params).await?;
            if !trap_result.is_normal_flow() {
                return Ok(Err(trap_result));
            }
            return Ok(Ok(Acquired::Interrupted));
        }
        tokio::time::sleep(POLL).await;
    }
}

/// A lock that failed for a reason other than a conflict: util-linux's `flock: NAME:
/// reason`, status 65.
fn report_lock_error<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    name: &str,
    error: &std::io::Error,
) -> Result<ExecutionResult, cash_core::Error> {
    let reason = cash_core::error::os_error_text(error);
    writeln!(context.stderr(), "flock: {name}: {reason}")?;
    Ok(ExecutionResult::new(EX_DATAERR))
}

/// `--verbose`'s first line, with util-linux's six decimals, on standard output as
/// util-linux writes it (its `failed to get lock` and `timeout` lines go to standard
/// error). Written here and now: util-linux's is block-buffered into a pipe, so it comes
/// out after the command's output there, and with `-F` not at all.
fn report_taken<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    started: Instant,
) -> Result<(), cash_core::Error> {
    writeln!(
        context.stdout(),
        "flock: getting lock took {:.6} seconds",
        started.elapsed().as_secs_f64()
    )?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::panic, reason = "tests assert loudly on failure")]
mod tests {
    use std::time::Duration;

    use super::{Action, EX_OSERR, EX_USAGE, Options, Range, Refusal, Wait, parse};

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn parsed(line: &str) -> Options {
        parse(&args(line)).unwrap_or_else(|refusal| panic!("{line}: {refusal:?}"))
    }

    fn refused(line: &str) -> Refusal {
        parse(&args(line))
            .err()
            .unwrap_or_else(|| panic!("{line} was accepted"))
    }

    fn usage(message: &str) -> Refusal {
        Refusal {
            message: message.to_owned(),
            status: EX_USAGE,
            hint: true,
        }
    }

    fn value(message: &str) -> Refusal {
        Refusal {
            message: message.to_owned(),
            status: EX_USAGE,
            hint: false,
        }
    }

    #[test]
    fn the_three_forms() {
        assert_eq!(parsed("9").action, Action::Descriptor(9));
        assert_eq!(parsed(" +9").action, Action::Descriptor(9));
        assert_eq!(
            parsed("f echo -n hi").action,
            Action::Command {
                file: "f".into(),
                argv: args("echo -n hi"),
            }
        );
        assert_eq!(
            parsed("f -c true").action,
            Action::Script {
                file: "f".into(),
                script: "true".into(),
            }
        );
        assert_eq!(
            parsed("f --command true").action,
            Action::Script {
                file: "f".into(),
                script: "true".into(),
            }
        );
        // `--command=…` after the file is a command, as in util-linux.
        assert_eq!(
            parsed("f --command=true").action,
            Action::Command {
                file: "f".into(),
                argv: args("--command=true"),
            }
        );
    }

    #[test]
    fn options_end_at_the_first_non_option() {
        let options = parsed("f -n true");
        assert_eq!(options.wait, Wait::Forever);
        assert_eq!(
            options.action,
            Action::Command {
                file: "f".into(),
                argv: args("-n true"),
            }
        );
        assert_eq!(
            parsed("-- -n true").action,
            Action::Command {
                file: "-n".into(),
                argv: args("true"),
            }
        );
        assert_eq!(
            parsed("- true").action,
            Action::Command {
                file: "-".into(),
                argv: args("true"),
            }
        );
    }

    #[test]
    fn waiting() {
        assert_eq!(parsed("-n f true").wait, Wait::Now);
        assert_eq!(parsed("--nb f true").wait, Wait::Now);
        assert_eq!(parsed("--nonblock f true").wait, Wait::Now);
        assert_eq!(parsed("-w 0 f true").wait, Wait::Now);
        assert_eq!(parsed("-nw 1 f true").wait, Wait::Now);
        assert_eq!(parsed("-w 1 -n f true").wait, Wait::Now);
        assert_eq!(
            parsed("-w1.5 f true").wait,
            Wait::For(Duration::from_millis(1500))
        );
        assert_eq!(
            parsed("--timeout=2 f true").wait,
            Wait::For(Duration::from_secs(2))
        );
        assert_eq!(
            parsed("--wait .5 f true").wait,
            Wait::For(Duration::from_millis(500))
        );
        assert_eq!(parsed("f true").wait, Wait::Forever);
    }

    #[test]
    fn the_lock_kind_and_the_conflict_code() {
        assert!(parsed("f true").exclusive);
        assert!(!parsed("-s f true").exclusive);
        assert!(parsed("-s -x f true").exclusive);
        assert!(parsed("-s -e f true").exclusive);
        assert!(!parsed("-x -s f true").exclusive);
        assert!(parsed("-u 9").unlock);
        assert!(parsed("--unlock 9").unlock);
        assert_eq!(parsed("f true").conflict_code, 1);
        assert_eq!(parsed("-E 7 f true").conflict_code, 7);
        assert_eq!(parsed("-E +3 f true").conflict_code, 3);
        assert_eq!(parsed("--conflict-exit-code 0 f true").conflict_code, 0);
        assert_eq!(parsed("-E 255 f true").conflict_code, 255);
        assert!(parsed("--verbose f true").verbose);
        assert!(parsed("--verb f true").verbose);
        // Accepted and without effect, except together.
        let _ = parsed("-o --fcntl --close f true");
        let _ = parsed("-F --no-fork f true");
        assert_eq!(
            refused("-o -F f true"),
            value("the --no-fork and --close options are incompatible")
        );
        assert_eq!(refused("-F -o 9").status, EX_USAGE);
        assert_eq!(parsed("-o -F -h").action, Action::Help);
    }

    #[test]
    fn the_range() {
        assert_eq!(parsed("f true").range, Range::FAR_BYTE);
        assert_eq!(
            parsed("--start 10 --length 5 f true").range,
            Range {
                offset: 10,
                length: 5,
            }
        );
        assert_eq!(parsed("--start 10 f true").range, Range::from_offset(10));
        assert_eq!(parsed("--length 0 f true").range, Range::from_offset(0));
        assert_eq!(
            parsed("--length 3 f true").range,
            Range {
                offset: 0,
                length: 3,
            }
        );
    }

    #[test]
    fn help_and_version_win() {
        assert_eq!(parsed("-h").action, Action::Help);
        assert_eq!(parsed("-h extra").action, Action::Help);
        assert_eq!(parsed("--help").action, Action::Help);
        assert_eq!(parsed("-V").action, Action::Version);
        assert_eq!(parsed("-nV f").action, Action::Version);
    }

    #[test]
    fn what_getopt_refuses() {
        assert_eq!(refused("-Z f true"), usage("invalid option -- 'Z'"));
        assert_eq!(refused("-c echo f"), usage("invalid option -- 'c'"));
        assert_eq!(
            refused("--zzz f true"),
            usage("unrecognized option '--zzz'")
        );
        assert_eq!(
            refused("--command echo f"),
            usage("unrecognized option '--command'")
        );
        assert_eq!(
            refused("--no f true"),
            usage("option '--no' is ambiguous; possibilities: '--nonblocking' '--no-fork'")
        );
        assert_eq!(
            refused("--c f true"),
            usage("option '--c' is ambiguous; possibilities: '--conflict-exit-code' '--close'")
        );
        assert_eq!(
            refused("--shared=x f true"),
            usage("option '--shared' doesn't allow an argument")
        );
        assert_eq!(
            refused("--timeout"),
            usage("option '--timeout' requires an argument")
        );
        assert_eq!(refused("-w"), usage("option requires an argument -- 'w'"));
        assert_eq!(refused(""), usage("not enough arguments"));
        assert_eq!(refused("-n"), usage("not enough arguments"));
    }

    #[test]
    fn what_the_values_refuse() {
        assert_eq!(refused("f"), value("bad file descriptor: 'f'"));
        assert_eq!(refused("9abc"), value("bad file descriptor: '9abc'"));
        assert_eq!(refused("-w abc f true"), value("invalid timeout: 'abc'"));
        assert_eq!(
            refused("-w 1.5.2 f true"),
            value("invalid timeout: '1.5.2'")
        );
        assert_eq!(
            refused("-w -1 f true"),
            Refusal {
                message: "cannot set up timer: Invalid argument".into(),
                status: EX_OSERR,
                hint: false,
            }
        );
        assert_eq!(refused("-w inf f true").status, EX_OSERR);
        assert_eq!(refused("-E abc f true"), value("invalid exit code: 'abc'"));
        assert_eq!(refused("-E 1.5 f true"), value("invalid exit code: '1.5'"));
        assert_eq!(
            refused("-E 256 f true"),
            value("exit code out of range (expected 0 to 255)")
        );
        assert_eq!(
            refused("-E -1 f true"),
            value("exit code out of range (expected 0 to 255)")
        );
        assert_eq!(
            refused("--start x f true"),
            value("invalid as start offset: 'x': Invalid argument")
        );
        assert_eq!(
            refused("--start -5 f true"),
            value("invalid as start offset: '-5': Invalid argument")
        );
        assert_eq!(
            refused("--length 1.5 f true"),
            value("invalid as length of lock range: '1.5': Invalid argument")
        );
        assert_eq!(
            refused("f -c echo extra"),
            value("-c requires exactly one command argument")
        );
        assert_eq!(
            refused("f -c"),
            value("-c requires exactly one command argument")
        );
    }
}
