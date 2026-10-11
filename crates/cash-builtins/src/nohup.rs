//! `nohup` builtin — run a command immune to hangups.
//!
//! What GNU `nohup` does with the streams, it does: input from a terminal is replaced by
//! an empty one, output to a terminal goes to `nohup.out` in the shell's working directory
//! (or the home folder's), and standard error to a terminal follows standard output.
//!
//! The command is run as the shell runs a command that names no function
//! (`run_for_builtin`): a builtin, or a program found on the shell's `PATH`, in the shell's
//! folder and with its exported variables. It used to start a second cash through the
//! standard library, with the process's folder, environment and standard output, so
//! `cd build && nohup ./run.sh > log` ran the wrong path and wrote to the terminal
//! (`REVIEW_REPORT.md` BI-02).
//!
//! On Windows a hangup is the console closing, and what cash started ends with it (D6);
//! a program that is to outlive the console is started with `detach` (D45).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use cash_core::openfiles::{OpenFile, OpenFiles};
use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Run COMMAND, ignoring hangup signals.
#[derive(Parser)]
pub(crate) struct NohupCommand {
    /// The command to run, and its arguments.
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

/// Where the command is among `nohup`'s words (D81): the first, unless it asks for help
/// or the version.
pub(crate) fn command_operand(words: &[String]) -> Option<usize> {
    match words.first()?.as_str() {
        "--" => words.get(1).map(|_| 1),
        word if word.starts_with("--") => None,
        _ => Some(0),
    }
}

impl builtins::Command for NohupCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let is_terminal = |fd| {
            context
                .try_fd(fd)
                .is_some_and(|file: OpenFile| file.is_terminal())
        };
        let stdin_is_terminal = is_terminal(OpenFiles::STDIN_FD);
        let stdout_is_terminal = is_terminal(OpenFiles::STDOUT_FD);
        let stderr_is_terminal = is_terminal(OpenFiles::STDERR_FD);

        let mut params = context.params.clone();
        if stdin_is_terminal {
            params.set_fd(OpenFiles::STDIN_FD, cash_core::openfiles::null()?);
        }

        if stdout_is_terminal {
            let Some(output) = open_nohup_out(&context) else {
                writeln!(context.stderr(), "nohup: failed to open 'nohup.out'")?;
                return Ok(ExecutionResult::new(125));
            };
            writeln!(
                context.stderr(),
                "nohup: {}appending output to 'nohup.out'",
                if stdin_is_terminal {
                    "ignoring input and "
                } else {
                    ""
                }
            )?;
            if stderr_is_terminal {
                params.set_fd(OpenFiles::STDERR_FD, output.clone());
            }
            params.set_fd(OpenFiles::STDOUT_FD, output);
        } else {
            if stdin_is_terminal {
                writeln!(context.stderr(), "nohup: ignoring input")?;
            }
            if stderr_is_terminal && let Some(output) = context.try_fd(OpenFiles::STDOUT_FD) {
                params.set_fd(OpenFiles::STDERR_FD, output);
            }
        }

        match cash_core::commands::run_for_builtin(context.shell, params, &self.command).await {
            Ok(result) => Ok(result),
            Err(e) => {
                let program = self.command.first().map_or("", String::as_str);
                writeln!(
                    context.stderr(),
                    "nohup: {program}: {}",
                    crate::xargs::start_failure(&e)
                )?;
                // GNU's status for a command that could not be found.
                Ok(ExecutionResult::new(127))
            }
        }
    }
}

/// `nohup.out` in the shell's working directory, or in the home folder when that cannot
/// be written, opened to append.
fn open_nohup_out<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Option<OpenFile> {
    let open = |path: PathBuf| OpenOptions::new().create(true).append(true).open(path).ok();
    open(context.shell.absolute_path("nohup.out"))
        .or_else(|| {
            let home = context.shell.env_str("HOME")?;
            open(PathBuf::from(home.as_ref()).join("nohup.out"))
        })
        .map(OpenFile::from)
}

#[cfg(test)]
mod command_operand_tests {
    use super::command_operand;

    #[test]
    fn the_first_word_is_the_command_unless_it_asks_for_help() {
        let words =
            |text: &str| -> Vec<String> { text.split_whitespace().map(str::to_owned).collect() };
        assert_eq!(command_operand(&words("net user x *")), Some(0));
        assert_eq!(command_operand(&words("-x y")), Some(0));
        assert_eq!(command_operand(&words("-- --help")), Some(1));
        assert_eq!(command_operand(&words("--help")), None);
        assert_eq!(command_operand(&words("")), None);
    }
}
