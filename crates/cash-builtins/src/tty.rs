//! `tty` builtin — POSIX utility to print the terminal name on stdin.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Print the file name of the terminal connected to standard input.
#[derive(Parser)]
pub(crate) struct TtyCommand {
    /// Print nothing, only return an exit status.
    #[arg(short = 's', long = "silent", alias = "quiet")]
    silent: bool,
}

impl builtins::Command for TtyCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let is_term = context.try_fd(0).is_some_and(|f| f.is_terminal());

        if is_term {
            if !self.silent {
                writeln!(context.stdout(), "/dev/tty")?;
            }
            Ok(ExecutionResult::success())
        } else {
            if !self.silent {
                writeln!(context.stdout(), "not a tty")?;
            }
            // POSIX requires status > 0 (standard: exit 1) when stdin is not a terminal.
            Ok(ExecutionResult::general_error())
        }
    }
}
