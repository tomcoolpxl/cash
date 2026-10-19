use clap::Parser;

use cash_core::{ExecutionControlFlow, ExecutionResult, builtins};

/// Exit the shell.
#[derive(Parser)]
pub(crate) struct ExitCommand {
    /// The exit code to return.
    #[arg(allow_hyphen_values = true)]
    code: Option<i64>,
}

impl builtins::Command for ExitCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        #[expect(clippy::cast_sign_loss)]
        let code_8bit = if let Some(code_32bit) = &self.code {
            (code_32bit & 0xFF) as u8
        } else {
            // In the `EXIT` trap, the status the shell was exiting with, as in Bash.
            context
                .shell
                .status_before_exit_trap()
                .unwrap_or_else(|| context.shell.last_exit_status())
        };

        // The `EXIT` trap runs here, where `exit` was called, as in Bash.
        let code_8bit = context
            .shell
            .run_exit_trap_for_exit(code_8bit, &context.params)
            .await;

        let mut result = ExecutionResult::new(code_8bit);
        result.next_control_flow = ExecutionControlFlow::ExitShell;

        Ok(result)
    }
}
