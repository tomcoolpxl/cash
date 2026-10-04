//! fish's `prevd`, `nextd` and `cdh`: moving through the folders the shell has been in
//! (spec D62). Alt-← and Alt-→ on an empty prompt line run `prevd` and `nextd`.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::Path;

use cash_core::{ExecutionResult, builtins};

/// Go back through the folder history.
#[derive(clap::Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct PrevdCommand {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Go forward through the folder history.
#[derive(clap::Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct NextdCommand {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Choose a recent folder to go to.
#[derive(clap::Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct CdhCommand {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for PrevdCommand {
    type Error = cash_core::Error;

    fn new<I: IntoIterator<Item = String>>(args: I) -> Result<Self, clap::Error> {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        step(&mut context, "prevd", -1, &self.args)
    }
}

impl builtins::Command for NextdCommand {
    type Error = cash_core::Error;

    fn new<I: IntoIterator<Item = String>>(args: I) -> Result<Self, clap::Error> {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        step(&mut context, "nextd", 1, &self.args)
    }
}

/// `prevd`/`nextd [-l] [N]`: move N folders (default 1) in `direction`.
fn step<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    name: &str,
    direction: isize,
    args: &[String],
) -> Result<ExecutionResult, cash_core::Error> {
    let mut list = false;
    let mut count: isize = 1;
    for arg in args {
        match arg.as_str() {
            "--help" => {
                writeln!(context.stdout(), "Usage: {name} [-l] [N]")?;
                return Ok(ExecutionResult::success());
            }
            "-l" | "--list" => list = true,
            number => match number.parse::<isize>() {
                Ok(n) if n > 0 => count = n,
                _ => {
                    writeln!(context.stderr(), "{name}: usage: {name} [-l] [COUNT]")?;
                    return Ok(ExecutionResult::new(2));
                }
            },
        }
    }

    let moved = context.shell.step_directory_history(direction * count)?;
    if moved.is_none() {
        let side = if direction < 0 { "behind" } else { "ahead of" };
        writeln!(context.stderr(), "{name}: no folder {side} this one")?;
        return Ok(ExecutionResult::general_error());
    }
    if list {
        let listing = history_listing(context.shell);
        write!(context.stdout(), "{listing}")?;
    }
    Ok(ExecutionResult::success())
}

/// The history as fish's `dirh` shows it: the folders behind, numbered back from the
/// current one, the current one, and the folders ahead, numbered forward.
fn history_listing(shell: &cash_core::Shell<impl cash_core::ShellExtensions>) -> String {
    let history = shell.directory_history();
    let show = |path: &Path| shell.tilde_shorten(path.to_string_lossy().into_owned());
    let mut out = String::new();
    let back = history.back();
    for (i, dir) in back.iter().enumerate() {
        let _ = writeln!(out, "{:>3}) {}", back.len() - i, show(dir));
    }
    let _ = writeln!(out, "     {}", show(shell.working_dir()));
    for (i, dir) in history.forward().enumerate() {
        let _ = writeln!(out, "{:>3}) {}", i + 1, show(dir));
    }
    out
}

impl builtins::Command for CdhCommand {
    type Error = cash_core::Error;

    fn new<I: IntoIterator<Item = String>>(args: I) -> Result<Self, clap::Error> {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        match self.args.as_slice() {
            [] => {}
            // Taken for a folder named `--help` until `help`'s pages asked for it.
            [help] if help == "--help" => {
                writeln!(context.stdout(), "Usage: cdh [FOLDER]")?;
                return Ok(ExecutionResult::success());
            }
            [dir] => return go(&mut context, Path::new(dir), "cdh"),
            _ => {
                writeln!(context.stderr(), "cdh: usage: cdh [FOLDER]")?;
                return Ok(ExecutionResult::new(2));
            }
        }

        let choices = recent_folders(context.shell);
        if choices.is_empty() {
            writeln!(context.stderr(), "cdh: no other folder in the history")?;
            return Ok(ExecutionResult::general_error());
        }

        // Oldest at the top, so the likeliest choice, 1, sits next to the question.
        let mut listing = String::new();
        for (i, dir) in choices.iter().enumerate().rev() {
            let shown = context
                .shell
                .tilde_shorten(dir.to_string_lossy().into_owned());
            let _ = writeln!(listing, "{:>3}) {shown}", i + 1);
        }
        listing.push_str("Select a folder by number: ");
        write!(context.stdout(), "{listing}")?;
        context.stdout().flush()?;

        let answer = read_line(&context)?;
        let answer = answer.trim();
        if answer.is_empty() {
            return Ok(ExecutionResult::general_error());
        }
        match answer.parse::<usize>() {
            Ok(n) if (1..=choices.len()).contains(&n) => go(&mut context, &choices[n - 1], "cdh"),
            _ => {
                writeln!(context.stderr(), "cdh: no choice '{answer}'")?;
                Ok(ExecutionResult::general_error())
            }
        }
    }
}

/// The folders behind the current one, most recent first, each once, without the current.
fn recent_folders(
    shell: &cash_core::Shell<impl cash_core::ShellExtensions>,
) -> Vec<std::path::PathBuf> {
    let current = shell.working_dir();
    let mut seen = Vec::new();
    for dir in shell.directory_history().back().iter().rev() {
        if dir != current && !seen.contains(dir) {
            seen.push(dir.clone());
        }
    }
    seen
}

/// Changes to `dir` as `cd` would, reporting a failure under `name`.
fn go<SE: cash_core::ShellExtensions>(
    context: &mut cash_core::ExecutionContext<'_, SE>,
    dir: &Path,
    name: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    match context.shell.set_working_dir(dir) {
        Ok(()) => Ok(ExecutionResult::success()),
        Err(error) => {
            let error = error.worded();
            writeln!(context.stderr(), "{name}: {}: {error}", dir.display())?;
            Ok(ExecutionResult::general_error())
        }
    }
}

/// One line of standard input, without waiting past its newline.
fn read_line<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<String, cash_core::Error> {
    let mut stdin = context.stdin();
    let mut line = Vec::new();
    let mut byte = [0_u8];
    while stdin.read(&mut byte)? == 1 && byte[0] != b'\n' {
        line.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}
