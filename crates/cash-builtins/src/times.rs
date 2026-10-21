use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins, timing};

/// Report on usage time.
#[derive(Parser)]
pub(crate) struct TimesCommand {
    /// Words after it, which it does not read, as Bash does not; they were an error.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    _operands: Vec<String>,
}

impl builtins::Command for TimesCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let (self_user, self_system) = cash_core::sys::resource::get_self_user_and_system_time()?;
        writeln!(
            context.stdout(),
            "{} {}",
            timing::format_duration_non_posixly(&self_user),
            timing::format_duration_non_posixly(&self_system),
        )?;

        let (children_user, children_system) =
            cash_core::sys::resource::get_children_user_and_system_time()?;
        writeln!(
            context.stdout(),
            "{} {}",
            timing::format_duration_non_posixly(&children_user),
            timing::format_duration_non_posixly(&children_system),
        )?;

        Ok(ExecutionResult::success())
    }
}
