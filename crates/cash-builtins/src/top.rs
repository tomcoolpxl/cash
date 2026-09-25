//! `top` — **D48**, for the same reason as `ps`, one step further.
//!
//! Windows ships no `top`, and the MSYS one reports MSYS pids rather than the native
//! process ids cash's `kill` accepts. This implementation keeps the familiar procps
//! overview and the small set of interactive controls people routinely use. It samples
//! only process CPU/memory, cheap machine-wide Win32 calls, and one local PDH ready-queue
//! counter. It deliberately does not inspect threads, handles, disks, or networks.
//!
//! Windows has no kernel-maintained Unix load average. `top` derives a session-local
//! analogue from busy CPU equivalents plus ready threads, and smooths that sample over
//! 1, 5, and 15 minutes. POSIX nice values and directly comparable swap accounting are
//! omitted. `R`/`S` in the task table says whether the process accrued CPU time during
//! the last sample.

use std::collections::HashMap;
use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

const TICKS_PER_SECOND: f64 = 10_000_000.0;
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

    /// Sort by `cpu` (the default), `mem`, `time`, or `pid`.
    #[arg(short = 'o', value_name = "FIELD")]
    sort: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SortField {
    Cpu,
    Memory,
    Time,
    Pid,
}

impl SortField {
    fn parse(value: Option<&str>) -> Option<Self> {
        match value {
            None | Some("cpu" | "%CPU" | "+%CPU") => Some(Self::Cpu),
            Some("mem" | "%MEM" | "+%MEM") => Some(Self::Memory),
            Some("time" | "TIME+" | "+TIME+") => Some(Self::Time),
            Some("pid" | "PID" | "+PID") => Some(Self::Pid),
            Some(_) => None,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Cpu => "%CPU",
            Self::Memory => "%MEM",
            Self::Time => "TIME+",
            Self::Pid => "PID",
        }
    }
}

struct Row {
    pid: u32,
    user: String,
    name: String,
    priority: i32,
    active: bool,
    cpu_share: f64,
    memory_share: f64,
    resident: u64,
    shared_resident: Option<u64>,
    committed: u64,
    cpu_total: u64,
}

struct Baseline {
    taken_at: u64,
    cpu: HashMap<u32, u64>,
    system_cpu: Option<cash_win32::sysinfo::CpuTimes>,
}

#[derive(Clone, Copy, Default)]
struct CpuSummary {
    user: f64,
    system: f64,
    idle: f64,
}

struct Snapshot {
    rows: Vec<Row>,
    tasks: usize,
    running: usize,
    cpu: CpuSummary,
    load_average: Option<[f64; 3]>,
    memory: Option<cash_win32::sysinfo::MemoryStatus>,
}

#[derive(Clone, Copy, Debug, Default)]
struct LoadAverages {
    values: [f64; 3],
    initialized: bool,
}

impl LoadAverages {
    fn update(&mut self, sample: f64, elapsed: f64) -> [f64; 3] {
        if !self.initialized {
            self.values = [sample; 3];
            self.initialized = true;
            return self.values;
        }

        for (value, period) in self.values.iter_mut().zip([60.0, 300.0, 900.0]) {
            let decay = (-elapsed.max(0.0) / period).exp();
            *value = value.mul_add(decay, sample * (1.0 - decay));
        }
        self.values
    }
}

#[derive(Clone, Copy)]
enum Action {
    Timeout,
    Quit,
    Help,
    Refresh,
    Sort(SortField),
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

impl builtins::Command for TopCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let Some(mut sort) = SortField::parse(self.sort.as_deref()) else {
            let other = self.sort.as_deref().unwrap_or_default();
            writeln!(
                context.stderr(),
                "{}: {other}: unknown sort field; expected cpu, mem, time, or pid",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        };

        let delay = self.delay.unwrap_or(DEFAULT_DELAY).max(0.1);
        let refreshes = self.iterations.unwrap_or(if self.batch { 1 } else { 0 });
        let mut baseline = take_baseline();
        let mut queue = cash_win32::sysinfo::ProcessorQueue::open();
        let mut load_averages = LoadAverages::default();

        if self.batch {
            let mut done = 0usize;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs_f64(delay)).await;
                let (mut snapshot, next) =
                    self.collect(&baseline, queue.as_mut(), &mut load_averages);
                baseline = next;
                sort_rows(&mut snapshot.rows, sort);
                write_screen(&mut context.stdout(), &snapshot, 0, true, sort)?;

                done += 1;
                if refreshes != 0 && done >= refreshes {
                    break;
                }
            }
            return Ok(ExecutionResult::success());
        }

        let mut snapshot: Option<Snapshot> = None;
        let mut top = 0usize;
        let mut done = 0usize;

        loop {
            let action = wait_for_action(delay);
            match action {
                Action::Quit => break,
                Action::Help => {
                    write_help(&mut context.stdout(), delay, sort)?;
                    wait_for_help_key();
                    if let Some(current) = &snapshot {
                        write_screen(&mut context.stdout(), current, top, false, sort)?;
                    }
                    // Do not turn time spent reading help into a giant CPU sample.
                    baseline = take_baseline();
                    continue;
                }
                Action::Sort(field) => {
                    sort = field;
                    top = 0;
                    if let Some(current) = &mut snapshot {
                        sort_rows(&mut current.rows, sort);
                        write_screen(&mut context.stdout(), current, top, false, sort)?;
                    }
                    continue;
                }
                Action::Up => top = top.saturating_sub(1),
                Action::Down => top = top.saturating_add(1),
                Action::PageUp => top = top.saturating_sub(visible_rows()),
                Action::PageDown => top = top.saturating_add(visible_rows()),
                Action::Home => top = 0,
                Action::End => top = usize::MAX,
                Action::Timeout | Action::Refresh => {
                    let (mut current, next) =
                        self.collect(&baseline, queue.as_mut(), &mut load_averages);
                    baseline = next;
                    sort_rows(&mut current.rows, sort);
                    snapshot = Some(current);
                    done += 1;
                }
            }

            if let Some(current) = &snapshot {
                top = clamp_top(top, current.rows.len());
                write_screen(&mut context.stdout(), current, top, false, sort)?;
            }

            if refreshes != 0 && done >= refreshes {
                break;
            }
        }

        Ok(ExecutionResult::success())
    }
}

impl TopCommand {
    fn collect(
        &self,
        baseline: &Baseline,
        queue: Option<&mut cash_win32::sysinfo::ProcessorQueue>,
        load_averages: &mut LoadAverages,
    ) -> (Snapshot, Baseline) {
        let now = cash_win32::process::now_filetime();
        let system_cpu = cash_win32::sysinfo::cpu_times();
        let memory = cash_win32::sysinfo::memory_status();
        let total_memory = memory.map(|status| status.physical_total);

        #[expect(
            clippy::cast_precision_loss,
            reason = "an interval long enough to lose precision here is thousands of years"
        )]
        let elapsed = (now.saturating_sub(baseline.taken_at)) as f64 / TICKS_PER_SECOND;

        let processes = cash_win32::process::list();
        let tasks = processes.len();
        let mut running = 0usize;
        let mut next_cpu = HashMap::with_capacity(tasks);
        let mut rows = Vec::with_capacity(tasks);

        for process in processes {
            let details = cash_win32::process::details(process.pid);
            let user = details.user.clone().unwrap_or_else(|| String::from("?"));
            let cpu_total = details.cpu.unwrap_or_default();
            next_cpu.insert(process.pid, cpu_total);
            let previous = baseline.cpu.get(&process.pid).copied().unwrap_or(cpu_total);
            let cpu_delta = cpu_total.saturating_sub(previous);
            let active = cpu_delta != 0;
            running += usize::from(active);

            // `id -un` may include DOMAIN\ while process tokens here do not.
            if let Some(wanted) = &self.user
                && !user.eq_ignore_ascii_case(account_name(wanted))
            {
                continue;
            }

            rows.push(Row {
                pid: process.pid,
                user,
                name: process.name,
                priority: process.base_priority,
                active,
                cpu_share: share_of_a_processor(cpu_delta, elapsed),
                memory_share: memory_share(details.resident, total_memory),
                resident: details.resident.unwrap_or_default(),
                shared_resident: details.shared_resident,
                committed: details.committed.unwrap_or_default(),
                cpu_total,
            });
        }

        let cpu = cpu_summary(baseline.system_cpu, system_cpu);
        let load_average = queue
            .and_then(cash_win32::sysinfo::ProcessorQueue::sample)
            .map(|ready| {
                let logical_processors =
                    std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "Windows supports far fewer processors than f64 loses integer precision for"
                )]
                let busy = logical_processors as f64 * (100.0 - cpu.idle).max(0.0) / 100.0;
                load_averages.update(busy + ready, elapsed)
            });

        let snapshot = Snapshot {
            rows,
            tasks,
            running,
            cpu,
            load_average,
            memory,
        };
        let next = Baseline {
            taken_at: now,
            cpu: next_cpu,
            system_cpu,
        };
        (snapshot, next)
    }
}

fn sort_rows(rows: &mut [Row], field: SortField) {
    rows.sort_by(|a, b| {
        let order = match field {
            SortField::Cpu => b
                .cpu_share
                .partial_cmp(&a.cpu_share)
                .unwrap_or(std::cmp::Ordering::Equal),
            SortField::Memory => b
                .memory_share
                .partial_cmp(&a.memory_share)
                .unwrap_or(std::cmp::Ordering::Equal),
            SortField::Time => b.cpu_total.cmp(&a.cpu_total),
            SortField::Pid => a.pid.cmp(&b.pid),
        };
        order.then(a.pid.cmp(&b.pid))
    });
}

fn visible_rows() -> usize {
    /// Five summaries, a spacer, the header, and the blank/status lines below the table.
    const CHROME: usize = 9;

    crossterm::terminal::size()
        .map_or(24, |(_, height)| usize::from(height))
        .saturating_sub(CHROME)
        .max(1)
}

fn clamp_top(top: usize, row_count: usize) -> usize {
    row_count.saturating_sub(visible_rows()).min(top)
}

fn wait_for_action(delay: f64) -> Action {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, poll, read};

    let _raw = RawMode::enter();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(delay);
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        if !poll(left).unwrap_or(false) {
            break;
        }
        let Ok(Event::Key(key)) = read() else {
            continue;
        };
        if matches!(key.kind, KeyEventKind::Release) {
            continue;
        }

        return match key.code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Action::Quit
            }
            KeyCode::Char('?' | 'h') => Action::Help,
            KeyCode::Enter | KeyCode::Char(' ' | 'r') => Action::Refresh,
            KeyCode::Char('P') => Action::Sort(SortField::Cpu),
            KeyCode::Char('M') => Action::Sort(SortField::Memory),
            KeyCode::Char('T') => Action::Sort(SortField::Time),
            KeyCode::Char('N') => Action::Sort(SortField::Pid),
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::PageUp => Action::PageUp,
            KeyCode::PageDown => Action::PageDown,
            KeyCode::Home => Action::Home,
            KeyCode::End => Action::End,
            _ => continue,
        };
    }
    Action::Timeout
}

struct RawMode(bool);

impl RawMode {
    fn enter() -> Self {
        Self(crossterm::terminal::enable_raw_mode().is_ok())
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if self.0 {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

fn wait_for_help_key() {
    use crossterm::event::{Event, KeyEventKind, read};

    let _raw = RawMode::enter();
    loop {
        let Ok(event) = read() else {
            return;
        };
        if let Event::Key(key) = event
            && !matches!(key.kind, KeyEventKind::Release)
        {
            return;
        }
    }
}

fn write_help(out: &mut impl Write, delay: f64, sort: SortField) -> Result<(), cash_core::Error> {
    write!(
        out,
        "\x1b[H\x1b[2Jcash top - interactive help\r\n\
         Delay {delay:.1} seconds; current sort: {}\r\n\r\n\
           Enter/Space/r   refresh now\r\n\
           P              sort by processor use\r\n\
           M              sort by memory use\r\n\
           T              sort by accumulated CPU time\r\n\
           N              sort by process id\r\n\
           Up/Down, j/k   scroll one row\r\n\
           PgUp/PgDn      scroll one page\r\n\
           Home/End       first/last page\r\n\
           ? or h         show this help\r\n\
           q or Esc       leave top\r\n\r\n\
         Windows notes:\r\n\
           R/S means CPU-active/idle in the last sample.\r\n\
           PRI is Windows base priority; SHR is shared resident working set.\r\n\
           Load average is a session-local Windows estimate: busy CPUs plus\r\n\
           ready threads, exponentially averaged over 1, 5, and 15 minutes.\r\n\
           Fields with no useful Windows equivalent are omitted.\r\n\r\n\
         Press any key to return",
        sort.label()
    )?;
    out.flush()?;
    Ok(())
}

fn write_screen(
    out: &mut impl Write,
    snapshot: &Snapshot,
    top: usize,
    batch: bool,
    sort: SortField,
) -> Result<(), cash_core::Error> {
    let newline = if batch { "\n" } else { "\r\n" };
    if !batch {
        write!(out, "\x1b[H\x1b[2J")?;
    }

    write_summary(out, snapshot, newline)?;

    let show_shared = snapshot
        .rows
        .iter()
        .any(|row| row.shared_resident.is_some());
    let header = if show_shared {
        format!(
            "{:>7} {:<10} {:>3} {:>7} {:>7} {:>7} S {:>5} {:>5} {:>9} COMMAND",
            "PID", "USER", "PRI", "VIRT", "RES", "SHR", "%CPU", "%MEM", "TIME+"
        )
    } else {
        format!(
            "{:>7} {:<10} {:>3} {:>7} {:>7} S {:>5} {:>5} {:>9} COMMAND",
            "PID", "USER", "PRI", "VIRT", "RES", "%CPU", "%MEM", "TIME+"
        )
    };
    if batch {
        write!(out, "{header}{newline}")?;
    } else {
        let width = terminal_width();
        let header = truncate(&header, width);
        write!(out, "\x1b[7m{header:<width$}\x1b[0m{newline}")?;
    }

    let shown = if batch {
        &snapshot.rows[..]
    } else {
        let end = top.saturating_add(visible_rows()).min(snapshot.rows.len());
        &snapshot.rows[top.min(end)..end]
    };

    for row in shown {
        let line = format_row(row, show_shared);
        if batch {
            write!(out, "{line}{newline}")?;
        } else {
            write!(out, "{}{newline}", truncate(&line, terminal_width()))?;
        }
    }

    if !batch {
        write!(out, "{newline}sort: {}   ? help   q quit", sort.label())?;
    }
    out.flush()?;
    Ok(())
}

fn write_summary(
    out: &mut impl Write,
    snapshot: &Snapshot,
    newline: &str,
) -> Result<(), cash_core::Error> {
    let uptime = format_uptime(cash_win32::sysinfo::uptime());
    if let Some([one, five, fifteen]) = snapshot.load_average {
        write!(
            out,
            "top - {} up {uptime},  load average: {one:.2}, {five:.2}, {fifteen:.2}{newline}",
            chrono::Local::now().format("%H:%M:%S")
        )?;
    } else {
        write!(
            out,
            "top - {} up {uptime}{newline}",
            chrono::Local::now().format("%H:%M:%S")
        )?;
    }
    write!(
        out,
        "Tasks: {:>5} total, {:>3} running, {:>3} sleeping{newline}",
        snapshot.tasks,
        snapshot.running,
        snapshot.tasks.saturating_sub(snapshot.running)
    )?;
    write!(
        out,
        "%Cpu(s): {:>5.1} us, {:>5.1} sy, {:>5.1} id{newline}",
        snapshot.cpu.user, snapshot.cpu.system, snapshot.cpu.idle
    )?;

    if let Some(memory) = snapshot.memory {
        let total = mib(memory.physical_total);
        let free = mib(memory.physical_available);
        let used = mib(memory
            .physical_total
            .saturating_sub(memory.physical_available));
        write!(
            out,
            "MiB Mem : {total:>9.1} total, {free:>9.1} free, {used:>9.1} used{newline}"
        )?;
        write!(
            out,
            "MiB Commit: {used:>8.1} used, {limit:>8.1} limit, {peak:>8.1} peak{newline}",
            used = mib(memory.commit_total),
            limit = mib(memory.commit_limit),
            peak = mib(memory.commit_peak)
        )?;
    }
    write!(out, "{newline}")?;
    Ok(())
}

fn format_row(row: &Row, show_shared: bool) -> String {
    if show_shared {
        format!(
            "{:>7} {:<10} {:>3} {:>7} {:>7} {:>7} {} {:>5.1} {:>5.1} {:>9} {}",
            row.pid,
            truncate(&crate::ps::user_column(Some(&row.user)), 10),
            row.priority,
            mebibytes(row.committed),
            mebibytes(row.resident),
            mebibytes(row.shared_resident.unwrap_or_default()),
            if row.active { 'R' } else { 'S' },
            row.cpu_share,
            row.memory_share,
            cpu_time_plus(row.cpu_total),
            row.name
        )
    } else {
        format!(
            "{:>7} {:<10} {:>3} {:>7} {:>7} {} {:>5.1} {:>5.1} {:>9} {}",
            row.pid,
            truncate(&crate::ps::user_column(Some(&row.user)), 10),
            row.priority,
            mebibytes(row.committed),
            mebibytes(row.resident),
            if row.active { 'R' } else { 'S' },
            row.cpu_share,
            row.memory_share,
            cpu_time_plus(row.cpu_total),
            row.name
        )
    }
}

fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map_or(80, |(width, _)| usize::from(width))
        .max(1)
}

fn cpu_summary(
    before: Option<cash_win32::sysinfo::CpuTimes>,
    after: Option<cash_win32::sysinfo::CpuTimes>,
) -> CpuSummary {
    let (Some(before), Some(after)) = (before, after) else {
        return CpuSummary {
            idle: 100.0,
            ..CpuSummary::default()
        };
    };

    let idle = after.idle.saturating_sub(before.idle);
    let kernel = after.kernel.saturating_sub(before.kernel);
    let user = after.user.saturating_sub(before.user);
    let system = kernel.saturating_sub(idle);
    let total = kernel.saturating_add(user);
    if total == 0 {
        return CpuSummary {
            idle: 100.0,
            ..CpuSummary::default()
        };
    }

    CpuSummary {
        user: percent(user, total),
        system: percent(system, total),
        idle: percent(idle, total),
    }
}

fn percent(part: u64, total: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "CPU and memory ratios need only one decimal place"
    )]
    let value = (part as f64 / total as f64) * 100.0;
    value
}

fn account_name(user: &str) -> &str {
    user.rsplit(['\\', '/']).next().unwrap_or(user)
}

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
        system_cpu: cash_win32::sysinfo::cpu_times(),
    }
}

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

fn memory_share(resident: Option<u64>, total: Option<u64>) -> f64 {
    let (Some(resident), Some(total)) = (resident, total) else {
        return 0.0;
    };
    if total == 0 {
        return 0.0;
    }
    percent(resident, total)
}

fn mib(bytes: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "top reports memory to one decimal place"
    )]
    let value = bytes as f64 / (1024.0 * 1024.0);
    value
}

fn mebibytes(bytes: u64) -> String {
    std::format!("{}M", bytes / (1024 * 1024))
}

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

fn format_uptime(uptime: std::time::Duration) -> String {
    let minutes = uptime.as_secs() / 60;
    let days = minutes / (24 * 60);
    let hours = (minutes / 60) % 24;
    let minutes = minutes % 60;
    if days != 0 {
        std::format!(
            "{days} day{}, {hours:>2}:{minutes:02}",
            if days == 1 { "" } else { "s" }
        )
    } else if hours != 0 {
        std::format!("{hours}:{minutes:02}")
    } else {
        std::format!("{minutes} min")
    }
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }
    value.chars().take(width).collect()
}
