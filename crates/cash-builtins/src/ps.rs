//! `ps` — **D48**, and the same argument as D35.
//!
//! uutils does not implement `ps`: it belongs to procps, a separate project, so the
//! bundle D48 ships has no process listing in it. What a user gets instead, on a typical
//! Windows machine, is the MSYS `ps` from Git for Windows — and that one is actively
//! misleading under cash:
//!
//! - it lists only MSYS processes, so every native program is missing;
//! - the numbers it prints are **MSYS** pids, which `kill` cannot use.
//!
//! A `ps` whose pids do not work with `kill` is worse than no `ps`, because it looks like
//! it worked. D35 exists to name exactly this class of problem, and the resolution here
//! is the one D48 prefers: carry the tool.
//!
//! What this implements is the subset that people actually type — `ps`, `ps -e`,
//! `ps -ef`, `ps aux` — and nothing more. Deliberately absent: `-o` format strings, CPU
//! and memory columns, and the full command line of another process, which requires
//! reading that process's PEB and is not worth the fragility.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Report a snapshot of running processes.
#[derive(Parser)]
pub(crate) struct PsCommand {
    /// Select every process.
    #[arg(short = 'e', short_alias = 'A')]
    every: bool,

    /// Full format: include the parent process id.
    #[arg(short = 'f')]
    full: bool,

    /// BSD-style options, accepted as a group: `aux`, `ax`, `ef`.
    #[arg(allow_hyphen_values = true)]
    bsd_options: Vec<String>,
}

impl builtins::Command for PsCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // `ps aux` and `ps ax` pass their options without a leading dash, which clap
        // cannot express alongside the dashed forms. Read them here instead of rejecting
        // a spelling every Linux user has in their fingers.
        let mut every = self.every;
        let mut full = self.full;

        for option in &self.bsd_options {
            let letters = option.strip_prefix('-').unwrap_or(option);
            if letters.is_empty() || !letters.chars().all(|c| "aefuxA".contains(c)) {
                writeln!(
                    context.stderr(),
                    "{}: unsupported option: {option}",
                    context.command_name
                )?;
                return Ok(ExecutionResult::general_error());
            }
            for letter in letters.chars() {
                match letter {
                    'a' | 'x' | 'e' | 'A' => every = true,
                    'f' | 'u' => full = true,
                    _ => (),
                }
            }
        }

        // With no selection, list the shell's own descendants. Linux's `ps` defaults to
        // "processes attached to this terminal", and Windows has no controlling terminal
        // to filter by — the shell's process tree is the closest honest equivalent, and
        // it is what someone typing `ps` in a shell is looking for.
        let processes = if every {
            cash_win32::process::list()
        } else {
            cash_win32::process::descendants(std::process::id())
        };

        let mut stdout = context.stdout();

        if full {
            writeln!(stdout, "{:>8} {:>8} COMMAND", "PID", "PPID")?;
            for process in processes {
                writeln!(
                    stdout,
                    "{:>8} {:>8} {}",
                    process.pid, process.parent_pid, process.name
                )?;
            }
        } else {
            writeln!(stdout, "{:>8} COMMAND", "PID")?;
            for process in processes {
                writeln!(stdout, "{:>8} {}", process.pid, process.name)?;
            }
        }

        Ok(ExecutionResult::success())
    }
}
