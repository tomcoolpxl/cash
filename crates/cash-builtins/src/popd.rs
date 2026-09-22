use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};

use crate::dirs;

/// Pop a path from the current directory stack.
#[derive(Parser)]
pub(crate) struct PopdCommand {
    /// Pop the path without changing the current working directory.
    #[clap(short = 'n')]
    no_directory_change: bool,

    /// `+N` or `-N`: which entry to drop, instead of the one on top.
    #[arg(allow_hyphen_values = true)]
    selector: Option<String>,
}

impl builtins::Command for PopdCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let listing = dirs::listing(context.shell);

        // The listing always holds the current directory, so a stack worth popping has
        // at least two entries. bash's wording, which cash reported as "directory stack
        // is empty".
        if listing.len() < 2 {
            writeln!(
                context.stderr(),
                "{}: directory stack empty",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }

        // cash: `+N` was rejected by the parser as an unexpected argument, so the way to
        // drop one entry from the middle of the stack — the reason `dirs -v` prints
        // indices at all — was not available.
        let index = match self.selector.as_deref() {
            Some(arg) => {
                let Some(selector) = dirs::Selector::parse(arg) else {
                    let complaint = if arg.starts_with(['+', '-']) {
                        "invalid number"
                    } else {
                        "invalid argument"
                    };
                    writeln!(
                        context.stderr(),
                        "{}: {arg}: {complaint}",
                        context.command_name
                    )?;
                    writeln!(
                        context.stderr(),
                        "{}: usage: popd [-n] [+N | -N]",
                        context.command_name
                    )?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                };

                let Some(index) = selector.index_in(listing.len()) else {
                    return dirs::report_bad_index(&context, listing.len(), arg);
                };

                index
            }
            // Told not to move, bash drops the entry above the current directory rather
            // than the current directory itself.
            None if self.no_directory_change => 1,
            None => 0,
        };

        let mut remaining = listing;
        remaining.remove(index);

        // Only dropping the current directory moves the shell; dropping anything else
        // leaves it where it is.
        let change_dir = index == 0 && !self.no_directory_change;
        dirs::store(context.shell, remaining, change_dir)?;

        let dirs_cmd = dirs::DirsCommand::default();
        dirs_cmd.execute(context).await?;

        Ok(ExecutionResult::success())
    }
}
