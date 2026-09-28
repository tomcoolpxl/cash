//! `nohup` builtin — run a command immune to hangups.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::windows::process::CommandExt as _;
use std::path::PathBuf;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Run COMMAND, ignoring hangup signals.
#[derive(Parser)]
pub(crate) struct NohupCommand {
    /// Command to run.
    #[arg(required = true)]
    command: String,

    /// Arguments to pass to the command.
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

impl builtins::Command for NohupCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let is_stdin_term = context.try_fd(0).is_some_and(|f| f.is_terminal());
        let is_stdout_term = context.try_fd(1).is_some_and(|f| f.is_terminal());

        let mut out_file = None;
        if is_stdout_term {
            let path = PathBuf::from("nohup.out");
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .or_else(|_| {
                    let home = std::env::var("USERPROFILE")
                        .or_else(|_| std::env::var("HOME"))
                        .unwrap_or_else(|_| ".".to_string());
                    let fallback = PathBuf::from(home).join("nohup.out");
                    OpenOptions::new().create(true).append(true).open(fallback)
                });

            match file {
                Ok(f) => {
                    writeln!(
                        context.stderr(),
                        "nohup: ignoring input and appending output to 'nohup.out'"
                    )?;
                    out_file = Some(f);
                }
                Err(e) => {
                    writeln!(context.stderr(), "nohup: failed to open 'nohup.out': {e}")?;
                    return Ok(ExecutionResult::new(126));
                }
            }
        } else if is_stdin_term {
            writeln!(context.stderr(), "nohup: ignoring input")?;
        }

        // Run via cash executable so shell functions, builtins, and external commands all work
        let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("cash.exe"));
        let mut cmd = std::process::Command::new(current_exe);

        let mut full_args = vec![
            "-c".to_string(),
            "\"$@\"".to_string(),
            self.command.clone(),
            self.command.clone(),
        ];
        full_args.extend(self.args.clone());
        cmd.args(full_args);

        // CREATE_NEW_PROCESS_GROUP = 0x00000200
        cmd.creation_flags(0x0000_0200);

        if is_stdin_term {
            cmd.stdin(std::process::Stdio::null());
        }

        if let Some(out) = out_file {
            let out_clone = out.try_clone().map_err(cash_core::Error::from)?;
            cmd.stdout(out);
            cmd.stderr(out_clone);
        }

        match cmd.status() {
            Ok(status) => {
                let code = status.code().unwrap_or(1);
                Ok(ExecutionResult::new(u8::try_from(code & 0xFF).unwrap_or(1)))
            }
            Err(e) => {
                writeln!(context.stderr(), "nohup: failed to run command: {e}")?;
                Ok(ExecutionResult::new(127))
            }
        }
    }
}
