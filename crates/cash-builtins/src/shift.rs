use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Shift positional arguments.
#[derive(Parser)]
pub(crate) struct ShiftCommand {
    /// Number of positions to shift the arguments by (defaults to 1); a second is too many.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    n: Vec<String>,
}

impl builtins::Command for ShiftCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        // As Bash's `get_numeric_arg` reads it: a first `--` passed over, a word that is
        // no number said with status 2, a second word too many. Clap's own words were said
        // for `shift x`, and `shift 1 2` was refused as an invalid option.
        let words = match self.n.split_first() {
            Some((first, rest)) if first == "--" => rest,
            _ => &self.n[..],
        };
        let n = match words.first() {
            None => 1,
            Some(word) => {
                let Ok(n) = word.trim().parse::<i64>() else {
                    writeln!(
                        context.error_stream(),
                        "shift: {word}: numeric argument required"
                    )?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                };
                if words.len() > 1 {
                    return Err(cash_core::ErrorKind::TooManyArguments.into());
                }
                n
            }
        };

        // A count out of range fails with 1, as in Bash, so `shift 2 || usage` reads as
        // it does there; a negative one says so.
        if n < 0 {
            writeln!(
                context.error_stream(),
                "shift: {n}: shift count out of range"
            )?;
            return Ok(ExecutionExitCode::GeneralError.into());
        }

        let n = usize::try_from(n).unwrap_or(usize::MAX);

        if n > context.shell.current_shell_args().len() {
            return Ok(ExecutionExitCode::GeneralError.into());
        }

        if n == 0 {
            return Ok(ExecutionResult::success());
        }

        let args = context.shell.current_shell_args_mut();
        args.drain(0..n);

        Ok(ExecutionResult::success())
    }
}
