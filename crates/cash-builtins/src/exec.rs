use clap::Parser;
use std::borrow::Cow;
use std::io::Write as _;
use std::path::Path;

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

    /// Command and args. A word before the command that looks like an option is one, as
    /// in Bash: `exec -i` is an invalid option, status 2, and the script goes on; it was
    /// run as a command named `-i`, and the script ended.
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

/// Where the command is among `exec`'s words (D81): the first word after its options,
/// `-a` taking the rest of its word or the next one as the name.
pub(crate) fn command_operand(words: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(word) = words.get(index) {
        index += 1;
        match word.as_str() {
            "--" => return words.get(index).map(|_| index),
            option if option.starts_with('-') && option.len() > 1 => {
                if option.ends_with('a') {
                    index += 1;
                }
            }
            _ => return Some(index - 1),
        }
    }
    None
}

impl builtins::Command for ExecCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.args.is_empty() {
            // When no arguments are present, then there's nothing for us to execute -- but
            // the redirections on `exec` itself are the shell's from now on, and only those.
            let mut context = context;
            context.make_own_redirections_permanent();
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

        let mut cmd = match commands::compose_std_command(
            &context,
            &self.args[0],
            argv0.as_str(),
            &self.args[1..],
            self.empty_environment,
        ) {
            Ok(cmd) => cmd,
            Err(error) => {
                // Said as the shell says it, and ending as a command not found does: it
                // went on (`exec notthere 4>&2` fails here, on the descriptor).
                let status = if matches!(error.kind(), ErrorKind::CommandNotFound(_)) {
                    127
                } else {
                    126
                };
                let _ = context
                    .shell
                    .display_error(&mut context.params.stderr(context.shell), &error);
                return Ok(failed(&context, status));
            }
        };

        // `-a` and `-l` only change the program's `argv[0]`, which std's `Command` cannot
        // set on Windows: another program ran under its own name in silence (the user,
        // 2026-10-04). A cash learns the name from `CASH_ARGV0` (EXE-12).
        if (self.name_for_argv0.is_some() || self.exec_as_login)
            && !commands::is_own_executable(cmd.get_program())
        {
            let option = if self.exec_as_login { "-l" } else { "-a" };
            writeln!(
                context.error_stream(),
                "{}: {option}: not supported on Windows for a program other than cash, whose \
                 argv[0] cannot be set",
                context.command_name
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

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
        let status = match cash_win32::spawn::status(&mut cmd) {
            Ok(status) => status,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // A name is searched for and not found; a path is a file that is not
                // there, said as Bash's `file_error` says it.
                let program = &self.args[0];
                if program.contains(['/', '\\']) {
                    context
                        .error_stream()
                        .write_unnamed(&format!("{program}: No such file or directory"))?;
                } else {
                    writeln!(
                        context.error_stream(),
                        "{}: {program}: not found",
                        context.command_name
                    )?;
                }

                return Ok(failed(&context, 127));
            }
            Err(e) => {
                // A directory, or a file Windows will not start: "cannot execute", status
                // 126, as Bash. It was an i/o error the script went on after.
                let program = &self.args[0];
                let reason = if context.shell.absolute_path(Path::new(program)).is_dir() {
                    String::from("Is a directory")
                } else {
                    cash_core::error::os_error_text(&e)
                };
                writeln!(
                    context.error_stream(),
                    "{}: {program}: cannot execute: {reason}",
                    context.command_name
                )?;
                return Ok(failed(&context, 126));
            }
        };

        let code = status.code().unwrap_or(1);
        #[expect(clippy::cast_sign_loss)]
        let mut result = ExecutionResult::new(cash_win32::exit::from_windows(code as u32));
        result.next_control_flow = cash_core::ExecutionControlFlow::ExitShell;
        Ok(result)
    }
}

/// What a failed `exec` ends with, `status`: POSIX has a non-interactive shell exit, so
/// that `exec missing; echo x` does not print `x`, unless `shopt -s execfail` asks it to
/// go on, as Bash does; that was not asked.
fn failed<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    status: u8,
) -> ExecutionResult {
    let mut result = ExecutionResult::new(status);
    let options = context.shell.options();
    if !options.interactive && !options.exit_on_exec_fail {
        result.next_control_flow = cash_core::ExecutionControlFlow::ExitShell;
    }
    result
}

#[cfg(test)]
mod command_operand_tests {
    use super::command_operand;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_first_word_after_the_options_is_the_command() {
        assert_eq!(command_operand(&words("net user x *")), Some(0));
        assert_eq!(command_operand(&words("-c -l net")), Some(2));
        assert_eq!(command_operand(&words("-cl net")), Some(1));
        assert_eq!(command_operand(&words("-- -net")), Some(1));
        assert_eq!(command_operand(&words("-c")), None);
    }

    #[test]
    fn a_takes_a_name_from_its_word_or_the_next() {
        assert_eq!(command_operand(&words("-a sh net")), Some(2));
        assert_eq!(command_operand(&words("-la sh net")), Some(2));
        assert_eq!(command_operand(&words("-ash net")), Some(1));
        assert_eq!(command_operand(&words("-a")), None);
    }
}
