//! `xargs` — **D48**, **D32**.
//!
//! Windows has no `xargs` at all: nothing in System32 resembles it, and the only one on a
//! developer's machine comes from Git for Windows' MSYS tree. `find | xargs grep` is in
//! cash's own acceptance corpus, so the gap is not academic.
//!
//! It is cash's business rather than Scoop's because **`xargs` builds command lines**, and
//! on Windows a command line is a single string that the callee re-splits by the CRT's
//! rules (D32). Getting that wrong does not fail — it runs a *different command*. The
//! same reasoning put `-exec` in `find`.
//!
//! Two deliberate choices, both matching GNU:
//!
//! - a backslash escapes the next character in the input, so a Windows-spelled path fed
//!   in raw loses its separators. That is why cash renders every path it prints through
//!   D3 (§4 #1) — `find | xargs` is safe because `find` prints `C:/src`, not `C:\src`;
//! - `-P N` runs up to N commands at once, `-P 0` as many as there are lines; their output
//!   interleaves as it comes, as GNU's does.
//!
//! The options are GNU findutils 4.10's, read by cash-getopt as its
//! `getopt_long("+0a:E:e::i::I:l::L:n:oprs:txP:d:")` reads them, with its messages and
//! statuses: 1 for a usage mistake, 123 when a command failed, 127 when one was not found.

use std::io::{BufRead, Read, Write};
use std::task::Poll;

use cash_core::openfiles::OpenFile;
use cash_core::{ExecutionExitCode, ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Order, Short};
use clap::Parser;
use futures::StreamExt;
use futures::stream::FuturesUnordered;

use crate::options;

/// Windows caps a command line at 32,767 characters; leave room for the program name and
/// the terminator rather than finding out at the boundary.
pub(crate) const MAX_COMMAND_LINE: usize = 30_000;

/// Build and run command lines from standard input.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct XargsCommand {
    /// Options and the command, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// GNU's short options; `--process-slot-var`, `--show-limits`, `--help` and `--version`
/// have no letter.
const SHORTS: &[Short<&str>] = &[
    Short::new('0', Arg::No, "null"),
    Short::new('a', Arg::Required, "arg-file"),
    Short::new('E', Arg::Required, "E"),
    Short::new('e', Arg::Optional, "eof"),
    Short::new('i', Arg::Optional, "replace"),
    Short::new('I', Arg::Required, "replace"),
    Short::new('l', Arg::Optional, "l"),
    Short::new('L', Arg::Required, "max-lines"),
    Short::new('n', Arg::Required, "max-args"),
    Short::new('o', Arg::No, "open-tty"),
    Short::new('p', Arg::No, "interactive"),
    Short::new('r', Arg::No, "no-run-if-empty"),
    Short::new('s', Arg::Required, "max-chars"),
    Short::new('t', Arg::No, "verbose"),
    Short::new('x', Arg::No, "exit"),
    Short::new('P', Arg::Required, "max-procs"),
    Short::new('d', Arg::Required, "delimiter"),
];
const LONGS: &[Long<'static, &str>] = &[
    Long::new("null", Arg::No, "null"),
    Long::new("arg-file", Arg::Required, "arg-file"),
    Long::new("delimiter", Arg::Required, "delimiter"),
    Long::new("eof", Arg::Optional, "eof"),
    Long::new("replace", Arg::Optional, "replace"),
    Long::new("max-lines", Arg::Optional, "l"),
    Long::new("max-args", Arg::Required, "max-args"),
    Long::new("open-tty", Arg::No, "open-tty"),
    Long::new("interactive", Arg::No, "interactive"),
    Long::new("no-run-if-empty", Arg::No, "no-run-if-empty"),
    Long::new("max-chars", Arg::Required, "max-chars"),
    Long::new("verbose", Arg::No, "verbose"),
    Long::new("show-limits", Arg::No, "show-limits"),
    Long::new("exit", Arg::No, "exit"),
    Long::new("max-procs", Arg::Required, "max-procs"),
    Long::new("process-slot-var", Arg::Required, "process-slot-var"),
    Long::new("version", Arg::No, "version"),
    Long::new("help", Arg::No, "help"),
];

/// GNU findutils 4.10's `xargs --help`, and what Windows changes.
const HELP: &str = "\
Usage: xargs [OPTION]... COMMAND [INITIAL-ARGS]...
Run COMMAND with arguments INITIAL-ARGS and more arguments read from input.

Mandatory and optional arguments to long options are also
mandatory or optional for the corresponding short option.
  -0, --null                   items are separated by a null, not whitespace;
                                 disables quote and backslash processing and
                                 logical EOF processing
  -a, --arg-file=FILE          read arguments from FILE, not standard input
  -d, --delimiter=CHARACTER    items in input stream are separated by CHARACTER,
                                 not by whitespace; disables quote and backslash
                                 processing and logical EOF processing
  -E END                       set logical EOF string; if END occurs as a line
                                 of input, the rest of the input is ignored
                                 (ignored if -0 or -d was specified)
  -e, --eof[=END]              equivalent to -E END if END is specified;
                                 otherwise, there is no end-of-file string
  -I R                         same as --replace=R
  -i, --replace[=R]            replace R in INITIAL-ARGS with names read
                                 from standard input, split at newlines;
                                 if R is unspecified, assume {}
  -L, --max-lines=MAX-LINES    use at most MAX-LINES non-blank input lines per
                                 command line
  -l[MAX-LINES]                similar to -L but defaults to at most one non-
                                 blank input line if MAX-LINES is not specified
  -n, --max-args=MAX-ARGS      use at most MAX-ARGS arguments per command line
  -o, --open-tty               Reopen stdin as /dev/tty in the child process
                                 before executing the command; useful to run an
                                 interactive application.
  -P, --max-procs=MAX-PROCS    run at most MAX-PROCS processes at a time
  -p, --interactive            prompt before running commands
      --process-slot-var=VAR   set environment variable VAR in child processes
  -r, --no-run-if-empty        if there are no arguments, then do not run COMMAND;
                                 if this option is not given, COMMAND will be
                                 run at least once
  -s, --max-chars=MAX-CHARS    limit length of command line to MAX-CHARS
      --show-limits            show limits on command-line length
  -t, --verbose                print commands before executing them
  -x, --exit                   exit if the size (see -s) is exceeded
      --help                   display this help and exit
      --version                output version information and exit

On Windows a command line holds at most 32,767 characters, its arguments
quoted as the program will read them; xargs keeps each under 30,000.";

/// What the command line asked for.
#[derive(Debug, Default)]
struct Options {
    /// How the input is split; `None` is by blanks, quotes and backslashes honoured.
    delimiter: Option<u8>,
    arg_file: Option<String>,
    eof: Option<String>,
    replace: Option<String>,
    max_lines: Option<usize>,
    max_args: Option<usize>,
    max_chars: Option<usize>,
    max_procs: Option<usize>,
    interactive: bool,
    open_tty: bool,
    skip_if_empty: bool,
    trace: bool,
    exit_if_too_long: bool,
    show_limits: bool,
    slot_variable: Option<String>,
    command: Vec<String>,
}

/// Why `xargs` ends before it reads any input.
#[derive(Debug, PartialEq, Eq)]
enum Stop {
    Help,
    Version,
    /// A usage mistake: said with GNU's Try line, status 1.
    Usage(String),
    /// A mistake said alone, status 1.
    Plain(String),
}

/// The three options that set how much input a command line takes, which GNU lets only
/// the last of stand, warning of the others: what each is called in the warning.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Measure {
    Args,
    Lines,
    Replace,
}

impl Measure {
    /// How GNU names the option that was given, and the one it puts aside.
    const fn given(self) -> &'static str {
        match self {
            Self::Args => "--max-args/-n",
            Self::Lines => "--max-lines/-L",
            Self::Replace => "--replace/-I/-i",
        }
    }

    const fn set_aside(self) -> &'static str {
        match self {
            Self::Args => "--max-args",
            Self::Lines => "--max-lines",
            Self::Replace => "--replace",
        }
    }
}

/// A number option as GNU's `parse_num` reads it: at least `least`; a word that is no
/// number is said as such. A number too large for a count is the largest count.
fn number(text: &str, letter: char, least: i64) -> Result<usize, Stop> {
    let trimmed = text.trim_start();
    let value = match trimmed.parse::<i64>() {
        Ok(value) => value,
        Err(error)
            if matches!(
                error.kind(),
                std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow
            ) =>
        {
            if trimmed.starts_with('-') {
                i64::MIN
            } else {
                i64::MAX
            }
        }
        Err(_) => {
            return Err(Stop::Usage(format!(
                "invalid number \"{text}\" for -{letter} option"
            )));
        }
    };
    if value < least {
        return Err(Stop::Usage(format!(
            "value {text} for -{letter} option should be >= {least}"
        )));
    }
    Ok(usize::try_from(value).unwrap_or(usize::MAX))
}

/// `-d`'s character: one byte, or an escape GNU knows (`\n`, `\t`, `\x41`, `\101`, …).
fn delimiter(spec: &str) -> Result<u8, Stop> {
    let bytes = spec.as_bytes();
    let invalid = || {
        Stop::Plain(format!(
            "Invalid input delimiter specification {spec}: the delimiter must be either a \
             single character or an escape sequence starting with \\."
        ))
    };
    match bytes {
        [byte] => Ok(*byte),
        [b'\\', rest @ ..] if !rest.is_empty() => {
            let simple = match rest {
                [b'a'] => Some(7),
                [b'b'] => Some(8),
                [b'f'] => Some(12),
                [b'n'] => Some(b'\n'),
                [b'r'] => Some(b'\r'),
                [b't'] => Some(b'\t'),
                [b'v'] => Some(11),
                [b'\\'] => Some(b'\\'),
                _ => None,
            };
            if let Some(byte) = simple {
                return Ok(byte);
            }
            let text = spec.get(2..).unwrap_or_default();
            let parsed = match rest {
                [b'x' | b'X', ..] => u8::from_str_radix(text, 16).ok(),
                [b'0'..=b'7', ..] => u8::from_str_radix(spec.get(1..).unwrap_or_default(), 8).ok(),
                _ => None,
            };
            parsed.ok_or_else(|| {
                Stop::Plain(format!(
                    "Invalid escape sequence {spec} in input delimiter specification."
                ))
            })
        }
        _ => Err(invalid()),
    }
}

/// Makes `chosen` the one of -n, -L and -I that stands, as GNU lets only the last stand,
/// warning in its words of the one it puts aside.
fn choose(
    options: &mut Options,
    measure: &mut Option<Measure>,
    chosen: Measure,
    warn: &mut Vec<String>,
) {
    if let Some(previous) = measure.filter(|previous| *previous != chosen) {
        warn.push(format!(
            "warning: options {} and {} are mutually exclusive, ignoring previous {} value",
            previous.set_aside(),
            chosen.given(),
            previous.set_aside()
        ));
        match previous {
            Measure::Args => options.max_args = None,
            Measure::Lines => options.max_lines = None,
            Measure::Replace => options.replace = None,
        }
    }
    *measure = Some(chosen);
}

/// Reads the command line as GNU's `getopt_long` loop does, in order, the options ending
/// at the command; the warnings GNU gives as it reads go to `warn`.
fn parse(args: &[String], warn: &mut Vec<String>) -> Result<Options, Stop> {
    let mut options = Options::default();
    let mut measure: Option<Measure> = None;
    let mut eof_given = false;
    let getopt = Getopt::new(SHORTS, LONGS).order(Order::StopAtOperand);
    for item in getopt.read(args) {
        let (id, value) = match item {
            Err(problem) => return Err(Stop::Usage(problem.to_string())),
            Ok(Item::Operand { value, .. }) => {
                options.command.push(value);
                continue;
            }
            Ok(Item::Option { id, value, .. }) => (id, value),
        };
        let text = value.clone().unwrap_or_default();
        match id {
            "null" => options.delimiter = Some(0),
            "delimiter" => options.delimiter = Some(delimiter(&text)?),
            "arg-file" => options.arg_file = Some(text),
            "E" => {
                options.eof = Some(text);
                eof_given = true;
            }
            "eof" => {
                options.eof = value.filter(|end| !end.is_empty());
                eof_given = options.eof.is_some();
            }
            "replace" => {
                options.replace = Some(value.unwrap_or_else(|| "{}".to_owned()));
                choose(&mut options, &mut measure, Measure::Replace, warn);
                options.exit_if_too_long = true;
            }
            "max-lines" => {
                options.max_lines = Some(number(&text, 'L', 1)?);
                choose(&mut options, &mut measure, Measure::Lines, warn);
                options.exit_if_too_long = true;
            }
            "l" => {
                options.max_lines = Some(match &value {
                    Some(lines) => number(lines, 'l', 1)?,
                    None => 1,
                });
                choose(&mut options, &mut measure, Measure::Lines, warn);
                options.exit_if_too_long = true;
            }
            "max-args" => {
                let count = number(&text, 'n', 1)?;
                // `-n 1` beside `-I`, which takes one item a line anyway, is no conflict.
                let beside_replace = measure == Some(Measure::Replace) && count == 1;
                options.max_args = Some(count);
                if !beside_replace {
                    choose(&mut options, &mut measure, Measure::Args, warn);
                }
            }
            "max-chars" => {
                let chars = number(&text, 's', 1)?;
                if chars > MAX_COMMAND_LINE {
                    warn.push(format!(
                        "value {text} for -s option should be <= {MAX_COMMAND_LINE}"
                    ));
                }
                options.max_chars = Some(chars.min(MAX_COMMAND_LINE));
            }
            "max-procs" => options.max_procs = Some(number(&text, 'P', 0)?),
            "open-tty" => options.open_tty = true,
            "interactive" => {
                options.interactive = true;
                options.trace = true;
            }
            "no-run-if-empty" => options.skip_if_empty = true,
            "verbose" => options.trace = true,
            "exit" => options.exit_if_too_long = true,
            "show-limits" => options.show_limits = true,
            "process-slot-var" => options.slot_variable = Some(text),
            "version" => return Err(Stop::Version),
            _ => return Err(Stop::Help),
        }
    }
    if eof_given && options.delimiter.is_some() {
        // GNU's message ends with a newline of its own before error()'s.
        warn.push("warning: the -E option has no effect if -0 or -d is used.\n".to_owned());
        options.eof = None;
    }
    if options.command.is_empty() {
        options.command.push("echo".to_owned());
    }
    Ok(options)
}

impl builtins::Command for XargsCommand {
    type Error = cash_core::Error;

    /// The words as they are, for GNU's option loop.
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    #[expect(clippy::too_many_lines)]
    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let name = context.command_name.clone();
        let mut warnings = Vec::new();
        let parsed = parse(&self.args, &mut warnings);
        for warning in &warnings {
            writeln!(context.stderr(), "{name}: {warning}")?;
        }
        let options = match parsed {
            Ok(options) => options,
            Err(Stop::Help) => {
                writeln!(context.stdout(), "{HELP}")?;
                return Ok(ExecutionResult::success());
            }
            Err(Stop::Version) => {
                writeln!(
                    context.stdout(),
                    "xargs (cash): GNU findutils 4.10's options"
                )?;
                return Ok(ExecutionResult::success());
            }
            Err(Stop::Usage(message)) => {
                return options::gnu_usage_error(&context, &name, message, 1);
            }
            Err(Stop::Plain(message)) => {
                writeln!(context.stderr(), "{name}: {message}")?;
                return Ok(ExecutionResult::general_error());
            }
        };
        if options.show_limits {
            show_limits(&context, &options)?;
        }

        // Read as the commands run, as GNU's does: it read all of its input first, so
        // `slow | xargs -n 1 cmd` started nothing until `slow` ended (BI-18). With `-a`,
        // the items come from the file and the commands keep xargs's own input.
        let input: Box<dyn Read + Send> = match &options.arg_file {
            Some(file) => {
                match std::fs::File::open(context.shell.absolute_path(std::path::Path::new(file))) {
                    Ok(file) => Box::new(file),
                    Err(error) => {
                        writeln!(
                            context.stderr(),
                            "{name}: Cannot open input file '{file}': {}",
                            cash_core::error::os_error_text(&error)
                        )?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
            }
            None => Box::new(context.stdin()),
        };
        let split = match options.delimiter {
            Some(delimiter) => Split::Delimiter(delimiter),
            None if options.replace.is_some() => Split::Lines,
            None => Split::Words,
        };
        let mut items = Items::new(std::io::BufReader::new(input), split, options.eof.clone());

        // A command's standard input: the console's keys under `-o`; xargs's own under
        // `-a`; else nothing, as GNU gives it, since what xargs reads is its own.
        let command_input = if options.open_tty {
            match cash_core::openfiles::tty_input() {
                Ok(keys) => Some(keys),
                Err(error) => {
                    writeln!(context.stderr(), "{name}: '/dev/tty': {error}")?;
                    return Ok(ExecutionResult::general_error());
                }
            }
        } else if options.arg_file.is_some() {
            None
        } else {
            Some(cash_core::openfiles::null()?)
        };
        let mut answers: Option<std::io::BufReader<OpenFile>> = None;
        let mut worst = ExecutionResult::success();

        // The commands running at once, at most `limit` of them (`-P`), each in a slot
        // of its own, which `--process-slot-var` names.
        let limit = match options.max_procs {
            Some(0) => usize::MAX,
            Some(n) => n,
            None => 1,
        };
        let mut free_slots: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        let mut next_slot = 0usize;
        let shared = &context;
        let options = &options;
        let command_input = &command_input;
        let start = move |argv: Vec<String>, slot: usize| async move {
            (
                slot,
                run(shared, options, command_input.as_ref(), &argv, slot).await,
            )
        };
        let mut running = FuturesUnordered::new();
        // Asks under `-p`, then queues a command line and starts it at once, so it runs
        // before more input is read, which may not have been written yet; then, with
        // every slot taken, waits for one to free.
        macro_rules! finished {
            ($done:expr) => {
                let (slot, result) = $done;
                free_slots.insert(slot);
                note(&mut worst, result?);
            };
        }
        macro_rules! admit {
            ($argv:expr) => {
                let argv = $argv;
                if options.interactive {
                    match confirm(&context, &argv, &mut answers) {
                        Ok(true) => {}
                        Ok(false) => continue,
                        Err(error) => {
                            writeln!(
                                context.stderr(),
                                "{name}: failed to open /dev/tty for reading: {error}"
                            )?;
                            return Ok(ExecutionResult::general_error());
                        }
                    }
                }
                let slot = free_slots.pop_first().unwrap_or_else(|| {
                    next_slot += 1;
                    next_slot - 1
                });
                running.push(start(argv, slot));
                if let Poll::Ready(Some(done)) = futures::poll!(running.next()) {
                    finished!(done);
                }
                while running.len() >= limit {
                    if let Some(done) = running.next().await {
                        finished!(done);
                    }
                }
            };
        }

        if let Some(token) = &options.replace {
            // `-I` substitutes rather than appends, and runs once per item.
            while let Some((item, _)) = items.next()? {
                let argv: Vec<String> = options
                    .command
                    .iter()
                    .map(|part| part.replace(token.as_str(), item.as_str()))
                    .collect();
                // -I implies -x: a command line the item makes too long ends the run.
                if argv.iter().map(|part| options.cost(part)).sum::<usize>() > options.budget() {
                    writeln!(context.stderr(), "{name}: argument line too long")?;
                    return Ok(ExecutionResult::general_error());
                }
                admit!(argv);
            }
            while let Some(done) = running.next().await {
                finished!(done);
            }
            return Ok(worst);
        }

        let mut ran = false;
        let mut pending: Option<(String, bool)> = None;
        loop {
            let mut argv = options.command.clone();
            let mut length: usize = argv.iter().map(|part| options.cost(part)).sum();
            let mut taken = 0;
            let mut lines = 0;
            let mut at_end = false;
            let mut too_long = false;

            loop {
                // A full line runs before the next item is read, which may not have been
                // written yet.
                if options.max_args.is_some_and(|max| taken >= max)
                    || options.max_lines.is_some_and(|max| lines >= max)
                {
                    break;
                }
                let next = match pending.take() {
                    Some(item) => Some(item),
                    None => items.next()?,
                };
                let Some((item, ends_line)) = next else {
                    at_end = true;
                    break;
                };
                let cost = options.cost(&item);
                if length + cost > options.budget() {
                    // One that cannot fit even alone is GNU's "argument line too long";
                    // so is a line cut short of `-n` or `-L` under `-x`.
                    let short = options.max_args.is_some() || options.max_lines.is_some();
                    too_long = taken == 0 || (options.exit_if_too_long && short);
                    pending = Some((item, ends_line));
                    break;
                }
                length += cost;
                argv.push(item);
                taken += 1;
                if ends_line {
                    lines += 1;
                }
            }
            if too_long {
                while let Some(done) = running.next().await {
                    finished!(done);
                }
                writeln!(context.stderr(), "{name}: argument line too long")?;
                return Ok(ExecutionResult::general_error());
            }

            // GNU runs the command once with no arguments unless told not to; `-r` is
            // the option every script that pipes a possibly-empty list reaches for. A
            // quote left open stops it, after what came before has run.
            let nothing = taken == 0 && (ran || options.skip_if_empty || items.unmatched.is_some());
            if !nothing {
                ran = true;
                admit!(argv);
            }
            if at_end {
                break;
            }
        }
        while let Some(done) = running.next().await {
            finished!(done);
        }

        if let Some(quote) = items.unmatched {
            let (which, _) = quote_names(quote);
            writeln!(
                context.stderr(),
                "{name}: unmatched {which} quote; by default quotes are special to xargs unless \
                 you use the -0 option"
            )?;
            return Ok(ExecutionResult::general_error());
        }
        Ok(worst)
    }
}

/// `-p`: the command line and `?...` on standard error, and the answer read from the
/// console; yes when it starts with `y` or `Y`, as GNU reads it. The console's keys are
/// opened the first time.
fn confirm<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    argv: &[String],
    answers: &mut Option<std::io::BufReader<OpenFile>>,
) -> Result<bool, cash_core::Error> {
    if answers.is_none() {
        *answers = Some(std::io::BufReader::new(cash_core::openfiles::tty_input()?));
    }
    let mut stderr = context.stderr();
    write!(stderr, "{} ?...", argv.join(" "))?;
    stderr.flush()?;
    let mut answer = String::new();
    if let Some(keys) = answers.as_mut() {
        keys.read_line(&mut answer)?;
    }
    Ok(answer.starts_with(['y', 'Y']))
}

/// `--show-limits`, as GNU words them, with Windows' figures: a command line of 32,767
/// characters, of which xargs uses at most 30,000. The environment is no part of it here.
fn show_limits<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    options: &Options,
) -> Result<(), cash_core::Error> {
    let environment: usize = cash_core::commands::exported_environment(context.shell)
        .iter()
        .map(|(name, value)| name.len() + value.len() + 2)
        .sum();
    writeln!(
        context.stderr(),
        "Your environment variables take up {environment} bytes\n\
         POSIX upper limit on argument length (this system): 32767\n\
         POSIX smallest allowable upper limit on argument length (all systems): 4096\n\
         Maximum length of command we could actually use: {MAX_COMMAND_LINE}\n\
         Size of command buffer we are actually using: {}\n\
         Maximum parallelism (--max-procs must be no greater): 2147483647",
        options.budget()
    )?;
    Ok(())
}

/// Keeps the status of a command that failed, the last one when several do.
const fn note(worst: &mut ExecutionResult, result: ExecutionResult) {
    if !result.is_success() {
        *worst = result;
    }
}

/// How `xargs` splits its input into items.
#[derive(Clone, Copy)]
enum Split {
    /// `-0` and `-d`: by that byte, quotes and backslashes plain, an empty item kept.
    Delimiter(u8),
    /// `-I`: by line, as GNU and POSIX have it, unquoted blanks part of the item.
    Lines,
    /// By blanks and newlines, quotes and backslashes honoured.
    Words,
}

/// The items of `xargs`'s input, read a line (or a delimited item) at a time, each with
/// whether it ends a line for `-L`: a line that ends with a blank goes on to the next.
struct Items<R> {
    input: R,
    split: Split,
    /// `-E`: the item that ends the input.
    eof: Option<String>,
    /// Items of the line read last, still to hand over.
    queued: std::collections::VecDeque<(String, bool)>,
    /// A quote a line left open, which ends the input: GNU's error.
    unmatched: Option<char>,
    /// The end-of-file item was met: nothing more is read.
    ended: bool,
}

impl<R: std::io::BufRead> Items<R> {
    const fn new(input: R, split: Split, eof: Option<String>) -> Self {
        Self {
            input,
            split,
            eof,
            queued: std::collections::VecDeque::new(),
            unmatched: None,
            ended: false,
        }
    }

    /// Queues `item`, unless it is the end-of-file item, which ends the input: whether it
    /// was queued.
    fn queue(&mut self, item: String, ends_line: bool) -> bool {
        if self.eof.as_deref() == Some(item.as_str()) {
            self.ended = true;
            return false;
        }
        self.queued.push_back((item, ends_line));
        true
    }

    /// The next item, or `None` at the end of the input, of the items before an
    /// unmatched quote, or at the end-of-file item.
    fn next(&mut self) -> std::io::Result<Option<(String, bool)>> {
        loop {
            if let Some(item) = self.queued.pop_front() {
                return Ok(Some(item));
            }
            if self.unmatched.is_some() || self.ended {
                return Ok(None);
            }
            let delimiter = match self.split {
                Split::Delimiter(byte) => byte,
                Split::Lines | Split::Words => b'\n',
            };
            let mut bytes = Vec::new();
            if self.input.read_until(delimiter, &mut bytes)? == 0 {
                return Ok(None);
            }
            match self.split {
                Split::Delimiter(byte) => {
                    // A delimited item is kept even empty, as GNU keeps it; the input's
                    // last one, without its delimiter, only when it is not.
                    let whole = bytes.last() == Some(&byte);
                    if whole {
                        bytes.pop();
                    }
                    if whole || !bytes.is_empty() {
                        let item = String::from_utf8_lossy(&bytes).into_owned();
                        self.queued.push_back((item, true));
                    }
                }
                Split::Lines => {
                    // Leading blanks are ignored, blank lines skipped, and CRLF from a
                    // Windows producer is one line ending, not a carriage return in it.
                    let text = String::from_utf8_lossy(&bytes);
                    let line = text.strip_suffix('\n').unwrap_or(&text);
                    let line = line.strip_suffix('\r').unwrap_or(line);
                    let line = line.trim_start_matches([' ', '\t']);
                    if !line.is_empty() {
                        let line = line.to_owned();
                        self.queue(line, true);
                    }
                }
                Split::Words => {
                    let text = String::from_utf8_lossy(&bytes);
                    let body = text.strip_suffix('\n').unwrap_or(&text);
                    let body = body.strip_suffix('\r').unwrap_or(body);
                    let goes_on = body.ends_with([' ', '\t']);
                    let (found, open) = split_on_whitespace(&text);
                    let count = found.len();
                    for (at, item) in found.into_iter().enumerate() {
                        if !self.queue(item, at + 1 == count && !goes_on) {
                            break;
                        }
                    }
                    if !self.ended {
                        self.unmatched = open;
                    }
                }
            }
        }
    }
}

/// GNU's words for a quote: "single" or "double", and the character.
const fn quote_names(quote: char) -> (&'static str, char) {
    if quote == '\'' {
        ("single", '\'')
    } else {
        ("double", '"')
    }
}

impl Options {
    /// How long a command line may be: `-s`, or what Windows allows.
    fn budget(&self) -> usize {
        self.max_chars.unwrap_or(MAX_COMMAND_LINE)
    }

    /// What `argument` adds to the command line. Under `-s` its bytes and a separator, as
    /// GNU counts; otherwise its length as Windows' command line holds it, quotes and
    /// escapes included, as that is the limit the default budget keeps under: it counted
    /// the bytes alone, so arguments with spaces or quotes could pass it (BI-18).
    fn cost(&self, argument: &str) -> usize {
        if self.max_chars.is_some() {
            argument.len() + 1
        } else {
            cash_win32::cmd::quote_argument(argument).len() + 1
        }
    }
}

/// Runs one command line, reporting it first under `-t` (under `-p` the prompt did), with
/// `input` as its standard input when there is one, and its slot in
/// `--process-slot-var`.
async fn run<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    options: &Options,
    input: Option<&OpenFile>,
    argv: &[String],
    slot: usize,
) -> Result<ExecutionResult, cash_core::Error> {
    let Some(program) = argv.first() else {
        return Ok(ExecutionResult::success());
    };

    if options.trace && !options.interactive {
        writeln!(context.stderr(), "{}", argv.join(" "))?;
    }

    // The shell runs it, as it runs a command that names no function: a builtin by
    // its name (`enable -n` honoured), or a program found on the shell's PATH and
    // started in the shell's directory with its exported variables (D5, D8, D10). A copy
    // of the shell keeps builtins and control flow from changing xargs's caller, and
    // the arguments are already parsed data, never turned back into shell source.
    let mut params = context.params.clone();
    if let Some(input) = input {
        params.set_fd(cash_core::openfiles::OpenFiles::STDIN_FD, input.clone());
    }
    let slot_text = slot.to_string();
    let exported: Vec<(&str, String)> = options
        .slot_variable
        .iter()
        .map(|variable| (variable.as_str(), slot_text.clone()))
        .collect();
    match cash_core::commands::run_for_builtin_exporting(context.shell, params, argv, &exported)
        .await
    {
        Ok(result) if result.is_success() => Ok(ExecutionResult::success()),
        // GNU's code for "a command exited non-zero", which is what a script testing
        // `xargs`'s status is looking for.
        Ok(_) => Ok(ExecutionResult::from(ExecutionExitCode::from(123u8))),
        Err(e) => {
            writeln!(
                context.stderr(),
                "{}: {program}: {}",
                context.command_name,
                start_failure(&e)
            )?;
            Ok(ExecutionResult::from(ExecutionExitCode::from(127u8)))
        }
    }
}

/// Why a command a builtin runs (`xargs`, `find -exec`) could not be started, worded to
/// follow `name: command:` as Bash words it: "command not found", or the system's reason.
pub(crate) fn start_failure(error: &cash_core::Error) -> String {
    match error.kind() {
        // The C library's words, which GNU `xargs` and `find` say.
        cash_core::ErrorKind::CommandNotFound(_) => "No such file or directory".to_owned(),
        _ => error.to_string(),
    }
}

/// Splits a line of input the way `xargs` does: on whitespace, honouring quotes and
/// backslash escapes, so a name with a space in it survives if it was quoted. With the
/// quote the line left open, if any: as in GNU's, a quote does not reach past the line,
/// and the items before it stand.
///
/// A path spelled the Windows way does not survive, because its separators are escapes —
/// which is the reason cash renders paths with forward slashes everywhere (§4 #1).
fn split_on_whitespace(input: &str) -> (Vec<String>, Option<char>) {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars();
    let mut quote: Option<char> = None;

    while let Some(c) = chars.next() {
        match c {
            '\\' if quote.is_none() => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '\'' | '"' if quote.is_none() => quote = Some(c),
            c if Some(c) == quote => quote = None,
            '\n' if quote.is_some() => return (items, quote),
            c if quote.is_none() && c.is_whitespace() => {
                if !current.is_empty() {
                    items.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if quote.is_some() {
        return (items, quote);
    }
    if !current.is_empty() {
        items.push(current);
    }
    (items, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn parsed(line: &str) -> (Result<Options, Stop>, Vec<String>) {
        let mut warnings = Vec::new();
        let result = parse(&words(line), &mut warnings);
        (result, warnings)
    }

    fn items(input: &str, split: Split, eof: Option<&str>) -> Vec<(String, bool)> {
        let mut items = Items::new(input.as_bytes(), split, eof.map(str::to_owned));
        let mut all = Vec::new();
        while let Some(item) = items.next().unwrap() {
            all.push(item);
        }
        all
    }

    #[test]
    fn options_are_gnus() {
        let options = parsed("-0 -n 2 -P 3 -r -t -s 100 --max-a=4 cmd -x")
            .0
            .unwrap();
        assert_eq!(options.delimiter, Some(0));
        assert_eq!(options.max_args, Some(4));
        assert_eq!((options.max_procs, options.max_chars), (Some(3), Some(100)));
        assert!(options.skip_if_empty && options.trace);
        // The options end at the command.
        assert_eq!(options.command, ["cmd", "-x"]);
        assert_eq!(parsed("").0.unwrap().command, ["echo"]);
        assert_eq!(parsed("-i").0.unwrap().replace.as_deref(), Some("{}"));
        assert_eq!(parsed("-iZ").0.unwrap().replace.as_deref(), Some("Z"));
        assert_eq!(parsed("-l").0.unwrap().max_lines, Some(1));
        assert_eq!(parsed("-E END").0.unwrap().eof.as_deref(), Some("END"));
        assert_eq!(parsed("-E END -e").0.unwrap().eof, None);
        assert_eq!(parsed("-d \\t").0.unwrap().delimiter, Some(b'\t'));
        assert_eq!(parsed("-d \\x41").0.unwrap().delimiter, Some(b'A'));
        assert_eq!(parsed("-d \\101").0.unwrap().delimiter, Some(b'A'));
        assert_eq!(parsed("--help").0.err(), Some(Stop::Help));
        assert_eq!(parsed("--version").0.err(), Some(Stop::Version));
    }

    #[test]
    fn mistakes_are_said_as_gnu_says_them() {
        let usage = |line: &str| match parsed(line).0 {
            Err(Stop::Usage(message) | Stop::Plain(message)) => message,
            other => panic!("{other:?}"),
        };
        assert_eq!(usage("-q"), "invalid option -- 'q'");
        assert_eq!(usage("-n"), "option requires an argument -- 'n'");
        assert_eq!(usage("-n 0"), "value 0 for -n option should be >= 1");
        assert_eq!(usage("-L 0"), "value 0 for -L option should be >= 1");
        assert_eq!(usage("-P x"), "invalid number \"x\" for -P option");
        assert_eq!(usage("-P -1"), "value -1 for -P option should be >= 0");
        assert_eq!(
            usage("-d ab"),
            "Invalid input delimiter specification ab: the delimiter must be either a \
             single character or an escape sequence starting with \\."
        );
        assert_eq!(
            usage("-d \\q"),
            "Invalid escape sequence \\q in input delimiter specification."
        );
    }

    #[test]
    fn the_last_of_n_l_and_i_stands_with_gnus_warning() {
        let (options, warnings) = parsed("-I {} -n 2");
        let options = options.unwrap();
        assert_eq!((options.replace, options.max_args), (None, Some(2)));
        assert_eq!(
            warnings,
            [
                "warning: options --replace and --max-args/-n are mutually exclusive, \
              ignoring previous --replace value"
            ]
        );
        let (options, warnings) = parsed("-n2 -I {}");
        let options = options.unwrap();
        assert_eq!(
            (options.replace.as_deref(), options.max_args),
            (Some("{}"), None)
        );
        assert_eq!(
            warnings,
            [
                "warning: options --max-args and --replace/-I/-i are mutually exclusive, \
              ignoring previous --max-args value"
            ]
        );
        let (options, warnings) = parsed("-L1 -n2");
        assert_eq!(options.unwrap().max_lines, None);
        assert_eq!(warnings.len(), 1);
        // `-n 1` beside `-I` is no conflict.
        let (options, warnings) = parsed("-I {} -n1");
        assert_eq!(options.unwrap().replace.as_deref(), Some("{}"));
        assert_eq!(warnings, Vec::<String>::new());
        // -E beside -0 or -d has no effect, said.
        let (options, warnings) = parsed("-0 -E END");
        assert_eq!(options.unwrap().eof, None);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn items_are_split_as_gnu_splits_them() {
        let owned = |list: &[(&str, bool)]| -> Vec<(String, bool)> {
            list.iter()
                .map(|(item, end)| ((*item).to_owned(), *end))
                .collect()
        };
        // A line that ends with a blank goes on; a blank line is no line.
        assert_eq!(
            items("a b \nc d\n\ne\n", Split::Words, None),
            owned(&[
                ("a", false),
                ("b", false),
                ("c", false),
                ("d", true),
                ("e", true)
            ])
        );
        // Delimited items are kept empty, the last one without its delimiter only when
        // there is something.
        assert_eq!(
            items("a\0\0b", Split::Delimiter(0), None),
            owned(&[("a", true), ("", true), ("b", true)])
        );
        assert_eq!(
            items("a,", Split::Delimiter(b','), None),
            owned(&[("a", true)])
        );
        // The end-of-file item ends the input, with -I's lines too.
        assert_eq!(
            items("a EOF b\nc\n", Split::Words, Some("EOF")),
            owned(&[("a", false)])
        );
        assert_eq!(
            items("x\nEOF\ny\n", Split::Lines, Some("EOF")),
            owned(&[("x", true)])
        );
    }
}
