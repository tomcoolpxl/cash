//! `top` — **D48**, for the same reason as `ps`, one step further.
//!
//! Windows ships no `top`, and nothing on a normal machine supplies one: procps has never
//! been ported, and the Cygwin/MSYS build that comes with Git for Windows reports MSYS
//! pids for MSYS processes only — the same trap `ps` fell into, where the numbers on
//! screen cannot be handed to `kill`. So a user who types `top` gets "command not found"
//! at best.
//!
//! What this gives is the part of `top` people actually use: what is running, what it is
//! costing, and being able to watch it change. The one real difference from Linux's is
//! **`%CPU` is measured between two samples**, so the first screen appears after one
//! delay rather than instantly — which is also why `ps` and `top` disagree about `%CPU`
//! and always will: `ps` averages over a process's whole life, `top` reports the moment.
//!
//! Absent because Windows cannot answer: load average (no such counter), `PR`/`NI` (a
//! priority *class* is not a nice value), and the buffer/cache split of memory.

use std::collections::HashMap;
use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// 100-nanosecond intervals per second, the unit Windows reports every time in.
const TICKS_PER_SECOND: f64 = 10_000_000.0;

/// What `top` waits between samples when nothing says otherwise.
const DEFAULT_DELAY: f64 = 2.0;

/// Display and update a sorted list of the processes using the machine.
#[derive(Parser)]
pub(crate) struct TopCommand {
    /// Batch mode: plain text, no screen control, for a pipe or a log.
    #[arg(short = 'b')]
    batch: bool,

    /// Stop after this many refreshes.
    #[arg(short = 'n', value_name = "COUNT")]
    iterations: Option<usize>,

    /// Seconds between refreshes.
    #[arg(short = 'd', value_name = "SECONDS")]
    delay: Option<f64>,

    /// Only show processes owned by this user.
    #[arg(short = 'u', value_name = "USER")]
    user: Option<String>,

    /// Sort by `cpu` (the default) or `mem`.
    #[arg(short = 'o', value_name = "FIELD")]
    sort: Option<String>,
}

/// One process, as a refresh sees it.
struct Row {
    pid: u32,
    user: String,
    name: String,
    cpu_share: f64,
    memory_share: f64,
    resident: u64,
    committed: u64,
    cpu_total: u64,
}

/// What a refresh measures `%CPU` against: the previous refresh.
struct Baseline {
    taken_at: u64,
    cpu: HashMap<u32, u64>,
}

impl builtins::Command for TopCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let sort_by_memory = match self.sort.as_deref() {
            None | Some("cpu" | "%CPU" | "+%CPU") => false,
            Some("mem" | "%MEM" | "+%MEM") => true,
            Some(other) => {
                writeln!(
                    context.stderr(),
                    "{}: {other}: unknown sort field; expected cpu or mem",
                    context.command_name
                )?;
                return Ok(ExecutionResult::general_error());
            }
        };

        let delay = self.delay.unwrap_or(DEFAULT_DELAY).max(0.1);
        let refreshes = self.iterations.unwrap_or(if self.batch { 1 } else { 0 });

        let mut baseline = take_baseline();
        let mut done = 0usize;

        loop {
            // The first screen has to wait a delay, because a share of the processor is a
            // measurement between two points and there is only one so far. On a screen —
            // where there is someone to lose patience — the wait doubles as the chance to
            // press `q`.
            if self.batch {
                tokio::time::sleep(std::time::Duration::from_secs_f64(delay)).await;
            } else if quit_requested(delay) {
                break;
            }

            let rows = self.collect(&baseline, sort_by_memory);
            baseline = take_baseline();

            let mut stdout = context.stdout();
            let shown = if self.batch {
                rows.len()
            } else {
                // cash: a screen holds what it holds. Printing all four hundred processes
                // scrolled the summary and the column header off the top, which is the
                // only part of `top` that is not a list — so the screen showed a list of
                // names and nothing to read it by.
                visible_rows().min(rows.len())
            };

            if !self.batch {
                // Home the cursor and clear, so a refresh replaces the screen rather than
                // scrolling it.
                write!(stdout, "\x1b[H\x1b[2J")?;
            }

            write_screen(&mut stdout, &rows[..shown], self.batch)?;
            stdout.flush()?;

            done += 1;
            if refreshes != 0 && done >= refreshes {
                break;
            }
        }

        Ok(ExecutionResult::success())
    }
}

/// How many process rows fit on the screen, once the summary, the memory line, the blank
/// line and the column header have taken theirs — and one more so the shell's prompt has
/// somewhere to land.
fn visible_rows() -> usize {
    /// The four header lines plus the row the prompt returns to.
    const CHROME: usize = 5;

    crossterm::terminal::size()
        .map_or(24, |(_, height)| usize::from(height))
        .saturating_sub(CHROME)
        .max(1)
}

/// Waits out the refresh interval, returning whether the viewer asked to stop.
///
/// cash: `q` is how everyone leaves `top`, and without it the only way out is Ctrl-C —
/// which in cash means the interrupt escalation (D13), a heavier hammer than the viewer
/// intended. Raw mode is entered for the wait alone and dropped immediately, so the
/// shell's terminal is handed back in the state it was lent in.
fn quit_requested(delay: f64) -> bool {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, poll, read};

    /// Restores the terminal however the wait ends, including on an error path.
    struct RawMode(bool);
    impl Drop for RawMode {
        fn drop(&mut self) {
            if self.0 {
                let _ = crossterm::terminal::disable_raw_mode();
            }
        }
    }

    let _raw = RawMode(crossterm::terminal::enable_raw_mode().is_ok());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(delay);
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        if !poll(left).unwrap_or(false) {
            break;
        }

        let Ok(Event::Key(key)) = read() else {
            continue;
        };

        // Windows reports both the press and the release; acting on the release would
        // quit on the keystroke that started the command.
        if matches!(key.kind, KeyEventKind::Release) {
            continue;
        }

        match key.code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => return true,
            KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return true;
            }
            _ => (),
        }
    }

    false
}

impl TopCommand {
    /// Measures every process against the baseline and orders them the way `top` does.
    fn collect(&self, baseline: &Baseline, sort_by_memory: bool) -> Vec<Row> {
        let now = cash_win32::process::now_filetime();
        let total_memory = cash_win32::process::total_physical_memory();

        #[expect(
            clippy::cast_precision_loss,
            reason = "an interval long enough to lose precision here is thousands of years"
        )]
        let elapsed = (now.saturating_sub(baseline.taken_at)) as f64 / TICKS_PER_SECOND;

        let mut rows: Vec<Row> = cash_win32::process::list()
            .into_iter()
            .filter_map(|process| {
                let details = cash_win32::process::details(process.pid);
                let user = details.user.clone().unwrap_or_else(|| String::from("?"));

                // cash: `-u` takes a login name on Linux, and on Windows there are two
                // spellings of one account — `thraa` and `DESKTOP-TOMC\thraa`. `id -un`
                // gives the second, `whoami` and this listing give the first, so
                // `top -u "$(id -un)"` would match nothing at all. Compare on the account
                // name and let either spelling in.
                if let Some(wanted) = &self.user
                    && !user.eq_ignore_ascii_case(account_name(wanted))
                {
                    return None;
                }

                let cpu_total = details.cpu.unwrap_or_default();
                let previous = baseline.cpu.get(&process.pid).copied().unwrap_or(cpu_total);

                Some(Row {
                    pid: process.pid,
                    user,
                    name: process.name,
                    cpu_share: share_of_a_processor(cpu_total.saturating_sub(previous), elapsed),
                    memory_share: memory_share(details.resident, total_memory),
                    resident: details.resident.unwrap_or_default(),
                    committed: details.committed.unwrap_or_default(),
                    cpu_total,
                })
            })
            .collect();

        if sort_by_memory {
            rows.sort_by(|a, b| {
                b.memory_share
                    .partial_cmp(&a.memory_share)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.pid.cmp(&b.pid))
            });
        } else {
            rows.sort_by(|a, b| {
                b.cpu_share
                    .partial_cmp(&a.cpu_share)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.pid.cmp(&b.pid))
            });
        }

        rows
    }
}

/// An account name with any `DOMAIN\` or `DOMAIN/` prefix taken off.
fn account_name(user: &str) -> &str {
    user.rsplit(['\\', '/']).next().unwrap_or(user)
}

/// The CPU totals every process has accrued, and when that was true.
fn take_baseline() -> Baseline {
    let cpu = cash_win32::process::list()
        .into_iter()
        .filter_map(|process| {
            cash_win32::process::cpu_time(process.pid).map(|cpu| (process.pid, cpu))
        })
        .collect();

    Baseline {
        taken_at: cash_win32::process::now_filetime(),
        cpu,
    }
}

/// `%CPU` the way `top` means it: 100% is one processor, so a busy four-core build shows
/// 400%.
fn share_of_a_processor(ticks: u64, elapsed_seconds: f64) -> f64 {
    if elapsed_seconds <= 0.0 {
        return 0.0;
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a tick count that large is centuries of CPU time"
    )]
    let used = ticks as f64 / TICKS_PER_SECOND;

    (used / elapsed_seconds) * 100.0
}

/// A process's working set as a share of physical memory.
fn memory_share(resident: Option<u64>, total: Option<u64>) -> f64 {
    let (Some(resident), Some(total)) = (resident, total) else {
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

/// The header and the table, as one refresh.
fn write_screen(out: &mut impl Write, rows: &[Row], batch: bool) -> Result<(), cash_core::Error> {
    let newline = if batch { "\n" } else { "\r\n" };

    let total = cash_win32::process::total_physical_memory().unwrap_or_default();
    let available = cash_win32::process::available_physical_memory().unwrap_or_default();

    write!(
        out,
        "top - {}   {} processes{newline}",
        chrono::Local::now().format("%H:%M:%S"),
        rows.len()
    )?;
    // Rounded once, then subtracted: dividing each byte count separately leaves a line
    // whose three numbers do not add up, which is the first thing anyone checks.
    let total_mib = total / (1024 * 1024);
    let free_mib = available / (1024 * 1024);
    write!(
        out,
        "Mem: {total_mib} MiB total, {free_mib} MiB free, {} MiB used{newline}",
        total_mib.saturating_sub(free_mib)
    )?;
    write!(out, "{newline}")?;
    write!(
        out,
        "{:>7} {:<12} {:>6} {:>5} {:>9} {:>9} {:>9} COMMAND{newline}",
        "PID", "USER", "%CPU", "%MEM", "RES", "VIRT", "TIME+"
    )?;

    for row in rows {
        write!(
            out,
            "{:>7} {:<12} {:>6.1} {:>5.1} {:>9} {:>9} {:>9} {}{newline}",
            row.pid,
            truncate(&row.user, 12),
            row.cpu_share,
            row.memory_share,
            mebibytes(row.resident),
            mebibytes(row.committed),
            cpu_time_plus(row.cpu_total),
            row.name
        )?;
    }

    Ok(())
}

/// A byte count as `top` shows it.
fn mebibytes(bytes: u64) -> String {
    std::format!("{}M", bytes / (1024 * 1024))
}

/// `top`'s `TIME+` column: minutes, seconds and hundredths of CPU time.
fn cpu_time_plus(ticks: u64) -> String {
    let hundredths = ticks / 100_000;
    let seconds = hundredths / 100;
    std::format!(
        "{}:{:02}.{:02}",
        seconds / 60,
        seconds % 60,
        hundredths % 100
    )
}

/// Keep a column a column, whatever the account is called.
fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }

    value.chars().take(width).collect()
}
