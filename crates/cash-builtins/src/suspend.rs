use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Suspend the shell.
#[derive(Parser)]
pub(crate) struct SuspendCommand {
    /// Force suspend login shells.
    #[arg(short = 'f')]
    force: bool,
}

impl builtins::Command for SuspendCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if context.shell.options().login_shell && !self.force {
            writeln!(context.stderr(), "login shell cannot be suspended")?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        #[expect(clippy::cast_possible_wrap)]
        cash_core::sys::signal::kill_process(
            std::process::id() as i32,
            cash_core::traps::TrapSignal::Signal(nix::sys::signal::SIGSTOP),
        )?;

        Ok(ExecutionResult::success())
    }
}
