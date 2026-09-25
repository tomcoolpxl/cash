//! `pgrep` for native Windows process IDs.
//!
//! The parent selector is especially useful with cash's `PID` and `PPID`; keeping the
//! query in-process also guarantees that every printed PID is one `kill` understands.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;
use fancy_regex::Regex;

/// Find native Windows processes by parent and/or executable name.
#[derive(Parser)]
pub(crate) struct PgrepCommand {
    /// Match children of these parent process IDs.
    #[arg(short = 'P', long = "parent", value_delimiter = ',')]
    parents: Vec<u32>,

    /// Print the executable name after each PID.
    #[arg(short = 'l', long = "list-name")]
    list_name: bool,

    /// Select names that do not match the pattern.
    #[arg(short = 'v', long = "inverse")]
    inverse: bool,

    /// Require the pattern to match the complete executable name.
    #[arg(short = 'x', long = "exact")]
    exact: bool,

    /// Regular expression matched against the executable file name.
    #[arg(value_name = "PATTERN")]
    pattern: Option<String>,
}

impl builtins::Command for PgrepCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.parents.is_empty() && self.pattern.is_none() {
            writeln!(context.stderr(), "pgrep: a parent or pattern is required")?;
            return Ok(ExecutionResult::general_error());
        }

        let regex = match self.pattern.as_deref() {
            Some(pattern) => {
                let expression = if self.exact {
                    std::format!("^(?:{pattern})$")
                } else {
                    pattern.to_owned()
                };
                match Regex::new(&expression) {
                    Ok(regex) => Some(regex),
                    Err(error) => {
                        writeln!(context.stderr(), "pgrep: invalid pattern: {error}")?;
                        return Ok(ExecutionResult::general_error());
                    }
                }
            }
            None => None,
        };

        let mut matched = false;
        let mut stdout = context.stdout();
        for process in cash_win32::process::list() {
            if !self.parents.is_empty() && !self.parents.contains(&process.parent_pid) {
                continue;
            }
            let name_matches = regex
                .as_ref()
                .is_none_or(|regex| regex.is_match(&process.name).unwrap_or(false));
            if name_matches == self.inverse {
                continue;
            }

            matched = true;
            if self.list_name {
                writeln!(stdout, "{} {}", process.pid, process.name)?;
            } else {
                writeln!(stdout, "{}", process.pid)?;
            }
        }

        Ok(if matched {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        })
    }
}
