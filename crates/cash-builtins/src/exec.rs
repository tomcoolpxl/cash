use clap::Parser;
use std::borrow::Cow;
use std::io::Write as _;

use cash_core::{ErrorKind, ExecutionExitCode, ExecutionResult, builtins, commands};

/// Exec the provided command.
#[derive(Parser)]
pub(crate) struct ExecCommand {
    /// Pass given name as zeroth argument to command.
    #[arg(short = 'a', value_name = "NAME")]
    name_for_argv0: Option<String>,

    /// Exec command with an empty environment.
    #[arg(short = 'c')]
    empty_environment: bool,

    /// Exec command as a login shell.
    #[arg(short = 'l')]
    exec_as_login: bool,

    /// Command and args.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for ExecCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.args.is_empty() {
            // When no arguments are present, then there's nothing for us to execute -- but we need
            // to ensure that any redirections setup for this builtin get applied to the calling
            // shell instance.
            #[expect(clippy::needless_collect)]
            let fds: Vec<_> = context.iter_fds().collect();

            context.shell.replace_open_files(fds.into_iter());
            return Ok(ExecutionResult::success());
        }

        // A subshell takes the same emulation below: the command runs, and the shell it
        // ends is the subshell, which is in this process. It ran the command through
        // `command`, which took no option and returned, so `(exec true; echo x)` printed
        // `x`, and `$(exec cmd)` lost the command's status.
        let mut argv0 = Cow::Borrowed(self.name_for_argv0.as_ref().unwrap_or(&self.args[0]));

        if self.exec_as_login {
            argv0 = Cow::Owned(std::format!("-{argv0}"));
        }

        let mut cmd = commands::compose_std_command(
            &context,
            &self.args[0],
            argv0.as_str(),
            &self.args[1..],
            self.empty_environment,
        )?;

        // cash: Windows has no `execve`, so the process image cannot be replaced.
        //
        // The standard emulation — run the command, then exit the shell with its
        // status — is observably the same for a script: nothing runs after the `exec`,
        // and `$?` propagates to whoever invoked cash. What differs is that the pid
        // changes and the shell lingers as a parent while the command runs, so anything
        // watching the pid sees two processes rather than one.
        //
        // Note the no-argument form above (`exec 3>&1`, `exec > log`) needs none of
        // this: it only replaces the shell's own open files, exactly as in bash.
        let status = match cmd.status() {
            Ok(status) => status,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                writeln!(
                    context.error_stream(),
                    "{}: {}: not found",
                    context.command_name,
                    self.args[0]
                )?;

                // POSIX: when `exec` cannot run the command, a non-interactive shell
                // exits. Without this, `exec missing; echo x` would print `x` — the
                // script carrying on past a line that was meant to replace it.
                let mut result: ExecutionResult = ExecutionExitCode::NotFound.into();
                if !context.shell.options().interactive {
                    result.next_control_flow = cash_core::ExecutionControlFlow::ExitShell;
                }
                return Ok(result);
            }
            Err(e) => return Err(ErrorKind::from(e).into()),
        };

        let code = status.code().unwrap_or(1);
        #[expect(clippy::cast_sign_loss)]
        let mut result = ExecutionResult::new(cash_win32::exit::from_windows(code as u32));
        result.next_control_flow = cash_core::ExecutionControlFlow::ExitShell;
        Ok(result)
    }
}
