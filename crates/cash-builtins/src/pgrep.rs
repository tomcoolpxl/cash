//! `pgrep` for native Windows process IDs.
//!
//! The parent selector is especially useful with cash's `PID` and `PPID`; keeping the
//! query in-process also guarantees that every printed PID is one `kill` understands.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

use crate::procmatch::NameMatcher;

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

    /// Regular expression matched against the executable file name, ignoring case and a
    /// trailing `.exe`.
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

        // The family's rules (see procmatch): case-insensitive, `.exe` optional.
        let matcher = match self.pattern.as_deref() {
            Some(pattern) => match NameMatcher::pattern(pattern, self.exact) {
                Ok(matcher) => Some(matcher),
                Err(error) => {
                    writeln!(context.stderr(), "pgrep: invalid pattern: {error}")?;
                    return Ok(ExecutionResult::general_error());
                }
            },
            None => None,
        };

        // A process whose parent pid names one of these but which started before it is an
        // orphan of an earlier process that had the same pid, not a child.
        let parents: Vec<(u32, Option<u64>)> = self
            .parents
            .iter()
            .map(|&pid| (pid, cash_win32::process::started(pid)))
            .collect();

        let mut matched = false;
        let mut stdout = context.stdout();
        for process in cash_win32::process::list() {
            if !parents.is_empty()
                && !parents
                    .iter()
                    .any(|&(pid, started)| cash_win32::process::is_child_of(&process, pid, started))
            {
                continue;
            }
            let name_matches = matcher
                .as_ref()
                .is_none_or(|matcher| matcher.matches(&process.name));
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
