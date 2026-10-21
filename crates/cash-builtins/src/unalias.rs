use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins};

/// Unset a shell alias.
#[derive(Parser)]
pub(crate) struct UnaliasCommand {
    /// Remove all aliases.
    #[arg(short = 'a')]
    remove_all: bool,

    /// Names of aliases to operate on.
    aliases: Vec<String>,
}

impl builtins::Command for UnaliasCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let mut exit_code = ExecutionResult::success();

        // No name is a usage error, as in Bash; it did nothing, status 0.
        if !self.remove_all && self.aliases.is_empty() {
            writeln!(
                context.stderr(),
                "{}: usage: unalias [-a] name [name ...]",
                context.command_name
            )?;
            return Ok(cash_core::ExecutionExitCode::InvalidUsage.into());
        }

        if self.remove_all {
            context.shell.aliases_mut().clear();
        } else {
            for alias in &self.aliases {
                if context.shell.aliases_mut().remove(alias).is_none() {
                    writeln!(
                        context.error_stream(),
                        "{}: {}: not found",
                        context.command_name,
                        alias
                    )?;
                    exit_code = ExecutionResult::general_error();
                }
            }
        }

        Ok(exit_code)
    }
}
