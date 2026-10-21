use clap::Parser;
use std::io::Write;

use cash_core::{ExecutionResult, builtins};

/// Directly invokes a built-in, without going through typical search order.
#[derive(Default, Parser)]
pub(crate) struct BuiltinCommand {
    #[clap(skip)]
    args: Vec<cash_core::CommandArg>,
}

impl builtins::DeclarationCommand for BuiltinCommand {
    fn set_declarations(&mut self, args: Vec<cash_core::CommandArg>) {
        self.args = args;
    }
}

impl builtins::Command for BuiltinCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        mut context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if self.args.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let mut args: Vec<_> = self.args.iter().skip(1).cloned().collect();
        // An option is none it knows, as in Bash: `builtin -z` looked for a builtin `-z`.
        if let Some(first) = args.first().map(ToString::to_string) {
            if first == "--" {
                args.remove(0);
            } else if first.len() > 1 && first.starts_with('-') {
                let option = first.get(..2).unwrap_or(&first);
                writeln!(
                    context.error_stream(),
                    "{}: {option}: invalid option",
                    context.command_name
                )?;
                writeln!(
                    context.error_stream(),
                    "{}: usage: builtin [shell-builtin [arg ...]]",
                    context.command_name
                )?;
                return Ok(cash_core::ExecutionExitCode::InvalidUsage.into());
            }
        }
        if args.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let builtin_name = args[0].to_string();

        if let Some(builtin) = context.shell.builtins().get(&builtin_name)
            && !builtin.disabled
        {
            context.command_name = builtin_name;
            (builtin.execute_func)(context, args).await
        } else {
            Err(cash_core::ErrorKind::BuiltinNotFound(builtin_name).into())
        }
    }
}
