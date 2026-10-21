use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Return from the current function.
#[derive(Parser)]
pub(crate) struct ReturnCommand {
    /// The exit code to return.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    code: Vec<String>,
}

impl builtins::Command for ReturnCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        // The status is read first, as in Bash: `return 1 2` is too many arguments
        // anywhere. A word that is no number returns 2.
        let code = crate::exit_status_word(&context, &self.code)?.unwrap_or_else(Some);

        // `${ cmd; }` returns too, as a function does, as in Bash.
        if !(context.shell.in_function()
            || context.shell.in_sourced_script()
            || context.shell.in_current_shell_substitution())
        {
            let _ = writeln!(
                context.error_stream(),
                "return: can only `return' from a function or sourced script"
            );
            context.shell.note_special_builtin_error();
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        let code_8bit = code.unwrap_or_else(|| context.shell.last_exit_status());

        context.shell.note_status_before_return();
        let mut result = ExecutionResult::new(code_8bit);
        result.next_control_flow = ExecutionControlFlow::ReturnFromFunctionOrScript;
        Ok(result)
    }
}
