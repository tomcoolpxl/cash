use clap::Parser;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Shift positional arguments.
#[derive(Parser)]
pub(crate) struct ShiftCommand {
    /// Number of positions to shift the arguments by (defaults to 1).
    n: Option<i32>,
}

impl builtins::Command for ShiftCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let n = self.n.unwrap_or(1);

        if n < 0 {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        #[expect(clippy::cast_sign_loss)]
        let n = n as usize;

        if n > context.shell.current_shell_args().len() {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        if n == 0 {
            return Ok(ExecutionResult::success());
        }

        let args = context.shell.current_shell_args_mut();
        args.drain(0..n);

        Ok(ExecutionResult::success())
    }
}
