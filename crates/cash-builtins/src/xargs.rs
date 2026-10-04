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
//! - `-P N` is a *maximum*, so running the commands one at a time honours it. Parallelism
//!   is not implemented, and nothing has to be refused to say so.

use std::io::Write;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};
use clap::Parser;

/// Windows caps a command line at 32,767 characters; leave room for the program name and
/// the terminator rather than finding out at the boundary.
pub(crate) const MAX_COMMAND_LINE: usize = 30_000;

/// Build and run command lines from standard input.
#[derive(Parser)]
pub(crate) struct XargsCommand {
    /// Items are separated by NUL rather than whitespace.
    #[arg(short = '0', long = "null")]
    null_separated: bool,

    /// Run at most this many arguments per command.
    #[arg(short = 'n', long = "max-args", value_name = "COUNT")]
    max_args: Option<usize>,

    /// Replace this token in the command with each item, one item per run.
    #[arg(short = 'I', value_name = "TOKEN")]
    replace: Option<String>,

    /// Do not run the command at all if the input is empty.
    #[arg(short = 'r', long = "no-run-if-empty")]
    skip_if_empty: bool,

    /// Print each command line to standard error before running it.
    #[arg(short = 't', long = "verbose")]
    trace: bool,

    /// Use at most this many characters per command line.
    #[arg(short = 's', long = "max-chars", value_name = "COUNT")]
    max_chars: Option<usize>,

    /// Run at most this many commands at once. cash runs them one at a time, which the
    /// maximum allows.
    #[arg(short = 'P', long = "max-procs", value_name = "COUNT")]
    max_procs: Option<usize>,

    /// The command to run, and its leading arguments. Defaults to `echo`.
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

impl builtins::Command for XargsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // Read as the commands run, as GNU's does: it read all of its input first, so
        // `slow | xargs -n 1 cmd` started nothing until `slow` ended (BI-18).
        let mode = if self.null_separated {
            Split::Null
        } else if self.replace.is_some() {
            Split::Lines
        } else {
            Split::Words
        };
        let mut items = Items::new(std::io::BufReader::new(context.stdin()), mode);

        let command: Vec<String> = if self.command.is_empty() {
            vec![String::from("echo")]
        } else {
            self.command.clone()
        };
        let mut worst = ExecutionResult::success();

        if let Some(token) = &self.replace {
            // `-I` substitutes rather than appends, and runs once per item.
            while let Some(item) = items.next()? {
                let argv: Vec<String> = command
                    .iter()
                    .map(|part| part.replace(token.as_str(), item.as_str()))
                    .collect();
                let result = self.run(&context, &argv).await?;
                if !result.is_success() {
                    worst = result;
                }
            }
            return Ok(worst);
        }

        let mut ran = false;
        let mut pending: Option<String> = None;
        loop {
            let mut argv = command.clone();
            let mut length: usize = argv.iter().map(|part| self.cost(part)).sum();
            let mut taken = 0;
            let mut at_end = false;

            loop {
                // A full line runs before the next item is read, which may not have been
                // written yet.
                if self.max_args.is_some_and(|max| taken >= max) {
                    break;
                }
                let next = match pending.take() {
                    Some(item) => Some(item),
                    None => items.next()?,
                };
                let Some(item) = next else {
                    at_end = true;
                    break;
                };
                let cost = self.cost(&item);
                if taken > 0 && length + cost > self.budget() {
                    pending = Some(item);
                    break;
                }
                length += cost;
                argv.push(item);
                taken += 1;
            }

            // GNU runs the command once with no arguments unless told not to; `-r` is
            // the option every script that pipes a possibly-empty list reaches for. A
            // quote left open stops it, after what came before has run.
            let nothing = taken == 0 && (ran || self.skip_if_empty || items.unmatched.is_some());
            if !nothing {
                let result = self.run(&context, &argv).await?;
                ran = true;
                if !result.is_success() {
                    worst = result;
                }
            }
            if at_end {
                break;
            }
        }

        if let Some(quote) = items.unmatched {
            let (which, _) = quote_names(quote);
            writeln!(
                context.stderr(),
                "{}: unmatched {which} quote; by default quotes are special to xargs unless \
                 you use the -0 option",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }
        Ok(worst)
    }
}

/// How `xargs` splits its input into items.
#[derive(Clone, Copy)]
enum Split {
    /// `-0`: by NUL.
    Null,
    /// `-I`: by line, as GNU and POSIX have it, unquoted blanks part of the item.
    Lines,
    /// By blanks and newlines, quotes and backslashes honoured.
    Words,
}

/// The items of `xargs`'s input, read a line (or a NUL-ended item) at a time.
struct Items<R> {
    input: R,
    split: Split,
    /// Items of the line read last, still to hand over.
    queued: std::collections::VecDeque<String>,
    /// A quote a line left open, which ends the input: GNU's error.
    unmatched: Option<char>,
}

impl<R: std::io::BufRead> Items<R> {
    const fn new(input: R, split: Split) -> Self {
        Self {
            input,
            split,
            queued: std::collections::VecDeque::new(),
            unmatched: None,
        }
    }

    /// The next item, or `None` at the end of the input, or of the items before an
    /// unmatched quote.
    fn next(&mut self) -> std::io::Result<Option<String>> {
        loop {
            if let Some(item) = self.queued.pop_front() {
                return Ok(Some(item));
            }
            if self.unmatched.is_some() {
                return Ok(None);
            }
            let delimiter = if matches!(self.split, Split::Null) {
                b'\0'
            } else {
                b'\n'
            };
            let mut bytes = Vec::new();
            if self.input.read_until(delimiter, &mut bytes)? == 0 {
                return Ok(None);
            }
            let text = String::from_utf8_lossy(&bytes);
            match self.split {
                Split::Null => {
                    let item = text.strip_suffix('\0').unwrap_or(&text);
                    if !item.is_empty() {
                        self.queued.push_back(item.to_owned());
                    }
                }
                Split::Lines => {
                    // Leading blanks are ignored, blank lines skipped, and CRLF from a
                    // Windows producer is one line ending, not a carriage return in it.
                    let line = text.strip_suffix('\n').unwrap_or(&text);
                    let line = line.strip_suffix('\r').unwrap_or(line);
                    let line = line.trim_start_matches([' ', '\t']);
                    if !line.is_empty() {
                        self.queued.push_back(line.to_owned());
                    }
                }
                Split::Words => {
                    let (items, open) = split_on_whitespace(&text);
                    self.queued.extend(items);
                    self.unmatched = open;
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

impl XargsCommand {
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

    /// Runs one command line, reporting it first under `-t`.
    async fn run<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        argv: &[String],
    ) -> Result<ExecutionResult, cash_core::Error> {
        let Some(program) = argv.first() else {
            return Ok(ExecutionResult::success());
        };

        if self.trace {
            writeln!(context.stderr(), "{}", argv.join(" "))?;
        }

        // The shell runs it, as it runs a command that names no function: a builtin by
        // its name (`enable -n` honoured), or a program found on the shell's PATH and
        // started in the shell's folder with its exported variables (D5, D8, D10). A copy
        // of the shell keeps builtins and control flow from changing xargs's caller, and
        // the arguments are already parsed data, never turned back into shell source.
        //
        // As GNU xargs does, the command reads an empty input: what xargs reads is its own.
        let mut params = context.params.clone();
        params.set_fd(
            cash_core::openfiles::OpenFiles::STDIN_FD,
            cash_core::openfiles::null()?,
        );
        match cash_core::commands::run_for_builtin(context.shell, params, argv).await {
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
