use cash_core::{ExecutionExitCode, builtins, trace_categories};

use clap::Parser;

/// (UNIMPLEMENTED COMMAND)
#[derive(Parser)]
pub(crate) struct UnimplementedCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for UnimplementedCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        tracing::warn!(target: trace_categories::UNIMPLEMENTED,
            "unimplemented built-in: {} {}",
            context.command_name,
            self.args.join(" ")
        );
        Ok(ExecutionExitCode::Unimplemented.into())
    }
}
