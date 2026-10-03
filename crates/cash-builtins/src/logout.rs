use std::io::Write;

use clap::Parser;

use cash_core::{ExecutionControlFlow, ExecutionResult, builtins};

/// Exit a login shell.
#[derive(Parser)]
pub(crate) struct LogoutCommand {
    /// The exit code to return.
    #[arg(allow_hyphen_values = true)]
    code: Option<i64>,
}

impl builtins::Command for LogoutCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if !context.shell.options().login_shell {
            writeln!(
                context.error_stream(),
                "logout: not login shell: use `exit'"
            )?;
            return Ok(ExecutionResult::new(1));
        }

        #[expect(clippy::cast_sign_loss)]
        let code_8bit = if let Some(code_32bit) = &self.code {
            (code_32bit & 0xFF) as u8
        } else {
            context.shell.last_exit_status()
        };

        let mut result = ExecutionResult::new(code_8bit);
        result.next_control_flow = ExecutionControlFlow::ExitShell;

        Ok(result)
    }
}
