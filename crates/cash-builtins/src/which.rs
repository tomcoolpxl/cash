//! `which` — **D8**.
//!
//! D8 says cash owns command resolution. `which` did not: it resolved to the MSYS
//! `which.exe` from Git for Windows, which searches `PATH` and knows nothing about the
//! shell asking. So `which cat` answered `/usr/bin/cat` while cash ran its own builtin,
//! and `which ps` answered `/usr/bin/ps` while cash ran its own `ps`. A tool that
//! confidently reports the wrong thing is worse than one that is missing — it is the
//! first thing reached for when something behaves oddly, and it hands back a false lead.
//!
//! This one answers the question the user is actually asking: *what would this shell run
//! if I typed that?* It is the same lookup `type` performs, printed the way `which`
//! prints, so the `p=$(which git)` idiom keeps working.
//!
//! # Why a builtin shows a line rather than nothing
//!
//! GNU `which` is an external program, so a shell builtin is invisible to it and it
//! exits non-zero. zsh's `which` is a builtin and reports them. cash follows zsh,
//! because under cash the builtin *is* the answer — reporting "not found" for a command
//! that demonstrably runs would repeat the original sin in the opposite direction.
//!
//! A script wanting only a filesystem path should ask for one: `which -p` restricts the
//! search to `PATH`, and prints nothing for a builtin.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

use crate::lookup::{self, Resolved};

/// Report what the shell would run for a name.
#[derive(Parser)]
pub(crate) struct WhichCommand {
    /// Print every match, not just the first.
    #[arg(short = 'a', long = "all")]
    all: bool,

    /// Search only `PATH`, ignoring builtins, functions and aliases.
    #[arg(short = 'p', long = "path-only")]
    path_only: bool,

    /// Print nothing; report only through the exit status.
    #[arg(short = 's', long = "silent")]
    silent: bool,

    /// Names to look up.
    names: Vec<String>,
}

impl builtins::Command for WhichCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.names.is_empty() {
            writeln!(
                context.stderr(),
                "{}: usage: which NAME...",
                context.command_name
            )?;
            return Ok(ExecutionResult::from(
                cash_core::ExecutionExitCode::InvalidUsage,
            ));
        }

        let options = lookup::Options {
            force_path_search: self.path_only,
            suppress_func_lookup: self.path_only,
            all_locations: self.all,
            path_dirs: None,
        };

        let mut any_missing = false;

        for name in &self.names {
            let resolved = lookup::resolve(context.shell, name, &options);

            if resolved.is_empty() {
                any_missing = true;
                if !self.silent {
                    writeln!(
                        context.stderr(),
                        "{}: {name}: not found",
                        context.command_name
                    )?;
                }
                continue;
            }

            if self.silent {
                continue;
            }

            let mut stdout = context.stdout();
            for how in &resolved {
                match how {
                    // A path alone, because that is what a script captures.
                    Resolved::File { path, .. } => {
                        writeln!(stdout, "{}", cash_win32::path::render(path))?;
                    }
                    Resolved::Builtin => writeln!(stdout, "{name}: shell builtin")?,
                    Resolved::Keyword => writeln!(stdout, "{name}: shell keyword")?,
                    Resolved::Function(_) => writeln!(stdout, "{name}: shell function")?,
                    Resolved::Alias(target) => writeln!(stdout, "{name}: aliased to {target}")?,
                }
            }
        }

        if any_missing {
            return Ok(ExecutionResult::general_error());
        }
        Ok(ExecutionResult::success())
    }
}
