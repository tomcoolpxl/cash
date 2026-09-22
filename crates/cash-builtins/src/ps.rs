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
//! What this implements is the subset people actually type — `ps`, `ps -e`, `ps -ef`,
//! `ps aux` — in the **layouts those spellings mean**. An earlier version accepted all of
//! them and then printed the same two homegrown columns for each, so `ps -ef` and
//! `ps aux` were indistinguishable and neither was what a script parsing `$2` expected.
//! The column order is the interface here, which is why it is matched rather than
//! improved on.
//!
//! Deliberately absent, because Windows cannot answer honestly: `-o` format strings, and
//! the full command line of another process, which requires reading that process's PEB.
//! `TTY` is always `?` — Windows has no controlling terminal — and `STAT` is always `R`,
//! because Windows does not expose a Linux-style run state and everything in the listing
//! is, by construction, a live process.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// 100-nanosecond intervals per second: the unit every Windows time here is in.
const TICKS_PER_SECOND: u64 = 10_000_000;

/// Report a snapshot of running processes.
#[derive(Parser)]
pub(crate) struct PsCommand {
    /// Select every process.
    #[arg(short = 'e', short_alias = 'A')]
    every: bool,

    /// Full format: the `UID PID PPID C STIME TTY TIME CMD` layout.
    #[arg(short = 'f')]
    full: bool,

    /// BSD-style options, accepted as a group: `aux`, `ax`, `ef`.
    #[arg(allow_hyphen_values = true)]
    bsd_options: Vec<String>,
}

/// Which set of columns to print.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Layout {
    /// `ps`: pid, terminal, cpu time, command.
    Short,
    /// `ps -f`: the System V long format.
    Full,
    /// `ps u`: the BSD user-oriented format, with cpu and memory shares.
    User,
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
        let mut user = false;

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
                    'f' => full = true,
                    'u' => user = true,
                    _ => (),
                }
            }
        }

        // `u` and `-f` ask for different column sets. bash users reach for one or the
        // other; if both arrive, the BSD one wins, as it does in procps.
        let layout = if user {
            Layout::User
        } else if full {
            Layout::Full
        } else {
            Layout::Short
        };

        // With no selection, list the shell's own descendants. Linux's `ps` defaults to
        // "processes attached to this terminal", and Windows has no controlling terminal
        // to filter by — the shell's process tree is the closest honest equivalent, and
        // it is what someone typing `ps` in a shell is looking for.
        let processes = if every {
            cash_win32::process::list()
        } else {
            cash_win32::process::descendants(std::process::id())
        };

        let now = cash_win32::process::now_filetime();
        let total_memory = if matches!(layout, Layout::User) {
            cash_win32::process::total_physical_memory()
        } else {
            None
        };

        let mut stdout = context.stdout();
        write_header(&mut stdout, layout)?;

        for process in processes {
            write_row(&mut stdout, layout, &process, now, total_memory)?;
        }

        Ok(ExecutionResult::success())
    }
}

/// The column names, which are the interface each spelling promises.
fn write_header(out: &mut impl Write, layout: Layout) -> Result<(), cash_core::Error> {
    match layout {
        Layout::Short => writeln!(out, "{:>7} TTY          TIME CMD", "PID")?,
        Layout::Full => writeln!(
            out,
            "{:<12} {:>7} {:>7} {:>2} {:>5} TTY          TIME CMD",
            "UID", "PID", "PPID", "C", "STIME"
        )?,
        Layout::User => writeln!(
            out,
            "{:<12} {:>7} {:>4} {:>4} {:>8} {:>7} TTY      STAT {:>5} {:>6} COMMAND",
            "USER", "PID", "%CPU", "%MEM", "VSZ", "RSS", "START", "TIME"
        )?,
    }

    Ok(())
}

/// One process, in the columns the chosen layout asked for.
fn write_row(
    out: &mut impl Write,
    layout: Layout,
    process: &cash_win32::process::ProcessInfo,
    now: u64,
    total_memory: Option<u64>,
) -> Result<(), cash_core::Error> {
    match layout {
        // The short layout needs only the CPU time, and asking for that alone spares the
        // token lookup the other two do per row.
        Layout::Short => {
            let cpu = cash_win32::process::cpu_time(process.pid).unwrap_or_default();
            writeln!(
                out,
                "{:>7} ?        {:>8} {}",
                process.pid,
                hours_minutes_seconds(cpu),
                process.name
            )?;
        }
        Layout::Full => {
            let details = cash_win32::process::details(process.pid);
            writeln!(
                out,
                "{:<12} {:>7} {:>7} {:>2} {:>5} ?        {:>8} {}",
                details.user.as_deref().unwrap_or("?"),
                process.pid,
                process.parent_pid,
                whole_percent(cpu_share(&details, now)),
                start_column(details.started),
                hours_minutes_seconds(details.cpu.unwrap_or_default()),
                process.name
            )?;
        }
        Layout::User => {
            let details = cash_win32::process::details(process.pid);
            writeln!(
                out,
                "{:<12} {:>7} {:>4.1} {:>4.1} {:>8} {:>7} ?        R    {:>5} {:>6} {}",
                details.user.as_deref().unwrap_or("?"),
                process.pid,
                cpu_share(&details, now),
                memory_share(&details, total_memory),
                details.committed.unwrap_or_default() / 1024,
                details.resident.unwrap_or_default() / 1024,
                start_column(details.started),
                minutes_seconds(details.cpu.unwrap_or_default()),
                process.name
            )?;
        }
    }

    Ok(())
}

/// `ps -f`'s `C` column: the processor share as a whole number, in the two characters the
/// column is wide.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=99 immediately before the conversion"
)]
fn whole_percent(share: f64) -> u64 {
    share.round().clamp(0.0, 99.0) as u64
}

/// The share of one processor a process has used over its whole life, as a percentage.
///
/// This is what `ps` reports and it is not what `top` reports: `ps` averages over the
/// process's lifetime, so a program that was busy for a second an hour ago shows near
/// zero. It needs no second sample, which is why `ps` can answer immediately.
fn cpu_share(details: &cash_win32::process::ProcessDetails, now: u64) -> f64 {
    let (Some(cpu), Some(started)) = (details.cpu, details.started) else {
        return 0.0;
    };

    let elapsed = now.saturating_sub(started);
    if elapsed == 0 {
        return 0.0;
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a tick count large enough to lose precision here is centuries of CPU time"
    )]
    let share = (cpu as f64 / elapsed as f64) * 100.0;

    share.min(9999.9)
}

/// The share of physical memory a process's working set holds.
fn memory_share(details: &cash_win32::process::ProcessDetails, total: Option<u64>) -> f64 {
    let (Some(resident), Some(total)) = (details.resident, total) else {
        return 0.0;
    };

    if total == 0 {
        return 0.0;
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "byte counts this large would need exabytes of RAM to lose a digit"
    )]
    let share = (resident as f64 / total as f64) * 100.0;

    share
}

/// `ps`'s `STIME`/`START`: the time of day a process started, or its date once that is no
/// longer today.
fn start_column(started: Option<u64>) -> String {
    let Some(started) = started else {
        return String::from("?");
    };

    let Some(local) = to_local(started) else {
        return String::from("?");
    };

    if local.date_naive() == chrono::Local::now().date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b%d").to_string()
    }
}

/// A Windows `FILETIME` count as local civil time.
fn to_local(filetime: u64) -> Option<chrono::DateTime<chrono::Local>> {
    /// 100ns intervals between 1601-01-01 and the Unix epoch.
    const EPOCH_DIFFERENCE: u64 = 116_444_736_000_000_000;

    let unix = filetime.checked_sub(EPOCH_DIFFERENCE)? / TICKS_PER_SECOND;
    let seconds = i64::try_from(unix).ok()?;

    chrono::DateTime::from_timestamp(seconds, 0).map(|utc| utc.with_timezone(&chrono::Local))
}

/// CPU time as `ps -f` prints it.
fn hours_minutes_seconds(ticks: u64) -> String {
    let seconds = ticks / TICKS_PER_SECOND;
    std::format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// CPU time as `ps u` prints it: minutes and seconds, with the minutes unbounded.
fn minutes_seconds(ticks: u64) -> String {
    let seconds = ticks / TICKS_PER_SECOND;
    std::format!("{}:{:02}", seconds / 60, seconds % 60)
}
