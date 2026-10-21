use clap::Parser;

use cash_core::{ExecutionControlFlow, builtins};

/// Continue to the next iteration of a control-flow loop.
#[derive(Parser)]
pub(crate) struct ContinueCommand {
    /// If specified, indicates which nested loop to continue to the next iteration of.
    /// A second is too many.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    which_loop: Vec<String>,
}

impl builtins::Command for ContinueCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        crate::leave_loops(&context, &self.which_loop, |levels| {
            ExecutionControlFlow::ContinueLoop { levels }
        })
    }
}
