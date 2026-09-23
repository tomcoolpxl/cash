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

use std::io::{Read, Write};

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};
use clap::Parser;

/// Windows caps a command line at 32,767 characters; leave room for the program name and
/// the terminator rather than finding out at the boundary.
const MAX_COMMAND_LINE: usize = 30_000;

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
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    command: Vec<String>,
}

impl builtins::Command for XargsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut input = String::new();
        context.stdin().read_to_string(&mut input)?;

        let items = if self.null_separated {
            input
                .split('\0')
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect()
        } else {
            split_on_whitespace(input.as_str())
        };

        // GNU runs the command once with no arguments unless told not to; `-r` is the
        // option every script that pipes a possibly-empty list reaches for.
        if items.is_empty() && self.skip_if_empty {
            return Ok(ExecutionResult::success());
        }

        let command: Vec<String> = if self.command.is_empty() {
            vec![String::from("echo")]
        } else {
            self.command.clone()
        };

        let budget = self.max_chars.unwrap_or(MAX_COMMAND_LINE);
        let mut worst = ExecutionResult::success();

        if let Some(token) = &self.replace {
            // `-I` substitutes rather than appends, and runs once per item.
            for item in &items {
                let argv: Vec<String> = command
                    .iter()
                    .map(|part| part.replace(token.as_str(), item.as_str()))
                    .collect();
                let result = self.run(&context, &argv)?;
                if !result.is_success() {
                    worst = result;
                }
            }

            return Ok(worst);
        }

        let mut index = 0;
        loop {
            let mut argv = command.clone();
            let mut length: usize = argv.iter().map(|part| part.len() + 1).sum();
            let mut taken = 0;

            while index < items.len() {
                let item = &items[index];
                if taken > 0 {
                    if length + item.len() + 1 > budget {
                        break;
                    }
                    if self.max_args.is_some_and(|max| taken >= max) {
                        break;
                    }
                }

                length += item.len() + 1;
                argv.push(item.clone());
                taken += 1;
                index += 1;
            }

            let result = self.run(&context, &argv)?;
            if !result.is_success() {
                worst = result;
            }

            if index >= items.len() {
                break;
            }
        }

        Ok(worst)
    }
}

impl XargsCommand {
    /// Runs one command line, reporting it first under `-t`.
    fn run<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        argv: &[String],
    ) -> Result<ExecutionResult, cash_core::Error> {
        let Some((program, rest)) = argv.split_first() else {
            return Ok(ExecutionResult::success());
        };

        if self.trace {
            writeln!(context.stderr(), "{}", argv.join(" "))?;
        }

        // cash: run where the shell believes it is. `cd` moves the shell's own working
        // directory without moving the process (D3/D10), so a builtin that spawns has to
        // ask — otherwise `cd build; ls | xargs rm` reaches into the wrong directory.
        let mut cmd = std::process::Command::new(program);
        cmd.args(rest).current_dir(context.shell.working_dir());

        if let Some(stdout_file) = context.try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD) {
            if let Ok(as_stdio) = std::process::Stdio::try_from(stdout_file) {
                cmd.stdout(as_stdio);
            }
        }
        if let Some(stderr_file) = context.try_fd(cash_core::openfiles::OpenFiles::STDERR_FD) {
            if let Ok(as_stdio) = std::process::Stdio::try_from(stderr_file) {
                cmd.stderr(as_stdio);
            }
        }

        let status = match cmd.status() {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Ok(current_exe) = std::env::current_exe() {
                    let mut fallback = std::process::Command::new(current_exe);
                    fallback
                        .arg("--invoke-bundled")
                        .arg(program)
                        .args(rest)
                        .current_dir(context.shell.working_dir());
                    if let Some(stdout_file) = context.try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD) {
                        if let Ok(as_stdio) = std::process::Stdio::try_from(stdout_file) {
                            fallback.stdout(as_stdio);
                        }
                    }
                    if let Some(stderr_file) = context.try_fd(cash_core::openfiles::OpenFiles::STDERR_FD) {
                        if let Ok(as_stdio) = std::process::Stdio::try_from(stderr_file) {
                            fallback.stderr(as_stdio);
                        }
                    }
                    fallback.status()
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        };

        match status {
            Ok(status) => {
                if status.success() {
                    Ok(ExecutionResult::success())
                } else {
                    // GNU's code for "a command exited non-zero", which is what a script
                    // testing `xargs`'s status is looking for.
                    Ok(ExecutionResult::from(ExecutionExitCode::from(123u8)))
                }
            }
            Err(e) => {
                writeln!(context.stderr(), "{}: {program}: {e}", context.command_name)?;
                Ok(ExecutionResult::from(ExecutionExitCode::from(127u8)))
            }
        }
    }
}

/// Splits input the way `xargs` does: on whitespace, honouring quotes and backslash
/// escapes, so a name with a space in it survives if it was quoted.
///
/// A path spelled the Windows way does not survive, because its separators are escapes —
/// which is the reason cash renders paths with forward slashes everywhere (§4 #1).
fn split_on_whitespace(input: &str) -> Vec<String> {
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
            c if quote.is_none() && c.is_whitespace() => {
                if !current.is_empty() {
                    items.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        items.push(current);
    }

    items
}
