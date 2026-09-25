//! `ulimit` on Windows.
//!
//! Windows has no `setrlimit`/`getrlimit`. The closest mechanism is a job object, whose
//! limits are per-job rather than per-process and cover a different set of resources —
//! there is no per-process descriptor cap at all, and handle counts run into the
//! millions.
//!
//! So this reports `unlimited` and accepts settings without applying them. Two reasons
//! that is the right shape rather than a lie:
//!
//! * `unlimited` is *accurate* for the limits scripts actually ask about on Windows.
//!   There is no cap on open files to report.
//! * Being absent is worse. `ulimit -n 4096` is a commonplace line, and a shell that
//!   answers `command not found` kills any script running under `set -e` — a direct hit
//!   on D2, which promises unmodified POSIX scripts run as-is.
//!
//! The full 504-line Unix implementation is untouched; this is a separate, deliberately
//! small builtin selected only on Windows.

use std::io::Write;
use std::str::FromStr;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Set or report resource limits.
#[derive(Parser)]
pub(crate) struct UlimitCommand {
    /// Report all current limits.
    #[arg(short = 'a')]
    all: bool,

    /// Set a soft limit.
    #[arg(short = 'S')]
    soft: bool,

    /// Set a hard limit.
    #[arg(short = 'H')]
    hard: bool,

    /// Maximum number of open file descriptors.
    #[arg(short = 'n')]
    open_files: bool,

    /// Maximum stack size.
    #[arg(short = 's')]
    stack: bool,

    /// Maximum number of user processes.
    #[arg(short = 'u')]
    processes: bool,

    /// Maximum size of virtual memory.
    #[arg(short = 'v')]
    virtual_memory: bool,

    /// Maximum socket buffer size.
    #[arg(short = 'b')]
    socket_buffer: bool,

    /// Maximum size of core files.
    #[arg(short = 'c')]
    core_size: bool,

    /// Maximum data segment size.
    #[arg(short = 'd')]
    data_size: bool,

    /// Maximum scheduling priority.
    #[arg(short = 'e')]
    scheduling_priority: bool,

    /// Maximum file size.
    #[arg(short = 'f')]
    file_size: bool,

    /// Maximum CPU time in seconds.
    #[arg(short = 't')]
    cpu_time: bool,

    /// Maximum pending signals.
    #[arg(short = 'i')]
    pending_signals: bool,

    /// Maximum kqueues.
    #[arg(short = 'k')]
    kqueues: bool,

    /// Maximum locked memory.
    #[arg(short = 'l')]
    locked_memory: bool,

    /// Maximum resident set size.
    #[arg(short = 'm')]
    resident_memory: bool,

    /// Pipe buffer size.
    #[arg(short = 'p')]
    pipe_size: bool,

    /// Maximum bytes in POSIX message queues.
    #[arg(short = 'q')]
    message_queues: bool,

    /// Maximum realtime priority.
    #[arg(short = 'r')]
    realtime_priority: bool,

    /// Maximum file locks.
    #[arg(short = 'x')]
    file_locks: bool,

    /// Maximum pseudoterminals.
    #[arg(short = 'P')]
    pseudoterminals: bool,

    /// Maximum realtime nonblocking time.
    #[arg(short = 'R')]
    realtime_time: bool,

    /// Maximum threads.
    #[arg(short = 'T')]
    threads: bool,

    /// Limit operands. Bash consumes the first and ignores additional operands.
    #[arg(value_name = "limit", num_args = 0..)]
    limit_operands: Vec<String>,
}

#[derive(Clone, Copy)]
enum LimitValue {
    Unlimited,
    Soft,
    Hard,
    Number,
}

impl FromStr for LimitValue {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unlimited" => Ok(Self::Unlimited),
            "soft" => Ok(Self::Soft),
            "hard" => Ok(Self::Hard),
            _ if value.parse::<u64>().is_ok() => Ok(Self::Number),
            _ => Err(()),
        }
    }
}

/// Every limit reported by `-a`, in bash's order.
const ALL_LIMITS: &[(&str, &str)] = &[
    ("file size", "-f"),
    ("max memory size", "-m"),
    ("open files", "-n"),
    ("stack size", "-s"),
    ("cpu time", "-t"),
    ("max user processes", "-u"),
    ("virtual memory", "-v"),
];

impl builtins::Command for UlimitCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let parsed = Self::try_parse_from(args)?;
        // `-a` reports and ignores operands in Bash. Otherwise the first operand is
        // validated even though Win32 has no corresponding limit to apply.
        if !parsed.all
            && let Some(operand) = parsed.limit_operands.first()
            && LimitValue::from_str(operand).is_err()
        {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::ValueValidation,
                format!("{operand}: invalid number"),
            ));
        }
        Ok(parsed)
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let mut stdout = context.stdout();

        if self.all {
            for (name, flag) in ALL_LIMITS {
                writeln!(stdout, "{name:<24} ({flag})  unlimited")?;
            }
            return Ok(ExecutionResult::success());
        }

        // Setting a limit: accepted, and deliberately not applied. Reporting failure
        // would break scripts for a platform difference they cannot do anything about.
        if !self.limit_operands.is_empty() {
            return Ok(ExecutionResult::success());
        }

        writeln!(stdout, "unlimited")?;
        Ok(ExecutionResult::success())
    }
}
