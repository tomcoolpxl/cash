//! `suspend`, Bash's: stop the shell until it is resumed. Windows has no stop signal, so
//! the shell cannot be stopped and says so in Bash's own shape (`bash: suspend: cannot
//! suspend a login shell`, `cannot suspend: no job control`), with status 1. Before this
//! it was `command not found`, status 127, which a script written for Bash reads as the
//! builtin missing rather than refusing.
//!
//! `-f`, which forces a login shell to suspend, is accepted and changes nothing.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Suspend the shell; not possible on Windows.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct SuspendCommand {
    /// Options, read here so that an unknown one is Bash's error.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Why `suspend` cannot: the reason after `cannot suspend:`.
pub(crate) const REASON: &str = "Windows has no stop signal";

impl builtins::Command for SuspendCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut errors = context.error_stream();
        for arg in &self.args {
            match arg.as_str() {
                "-f" | "--" => {}
                "--help" => {
                    writeln!(
                        context.stdout(),
                        "suspend: suspend [-f]\n    Suspend shell execution.\n    \n    \
                         Windows has no stop signal, so cash cannot suspend itself; the\n    \
                         command fails with status 1 and the shell goes on."
                    )?;
                    return Ok(ExecutionResult::success());
                }
                other => {
                    writeln!(errors, "suspend: {other}: invalid option")?;
                    writeln!(errors, "suspend: usage: suspend [-f]")?;
                    return Ok(ExecutionResult::new(2));
                }
            }
        }
        writeln!(errors, "suspend: cannot suspend: {REASON}")?;
        Ok(ExecutionResult::general_error())
    }
}
