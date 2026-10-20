use clap::Parser;

use cash_core::{ExecutionControlFlow, ExecutionResult, builtins};

/// Exit the shell.
#[derive(Parser)]
pub(crate) struct ExitCommand {
    /// The exit code to return.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    code: Vec<String>,
}

impl builtins::Command for ExitCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let code_8bit = match crate::exit_status_word(&context, &self.code)? {
            Ok(Some(code)) => code,
            // In the `EXIT` trap, the status the shell was exiting with, as in Bash.
            Ok(None) => context
                .shell
                .status_before_exit_trap()
                .unwrap_or_else(|| context.shell.last_exit_status()),
            Err(ended) => return Ok(ended),
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
