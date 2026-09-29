//! `top` — **D48**, for the same reason as `ps`, one step further.
//!
//! Windows ships no `top`, and the MSYS one reports MSYS pids rather than the native
//! process ids cash's `kill` accepts. This implementation keeps the familiar procps
//! overview and the interactive controls people routinely use. Each refresh takes one
//! snapshot of every process and thread, the machine's CPU and memory counters, and each
//! process's CPU time and memory; a process's account is looked up once, not at every
//! refresh. It deliberately does not inspect handles, disks, or networks.
//!
//! **Load average.** Windows keeps none. `top` uses Linux's definition, tasks running or
//! waiting for a processor, and measures both parts: the processors busy over the
//! interval, exactly, from the machine's CPU times, and the threads ready to run at the
//! sample, from the snapshot. The sample is smoothed over 1, 5 and 15 minutes as Linux
//! does, from the moment `top` starts. POSIX nice values and directly comparable swap
//! accounting are omitted. `R`/`S` in the task table says whether the process accrued
//! CPU time during the last sample.
//!
//! **The screen.** `top` draws on the terminal's alternate screen with the cursor hidden,
//! and writes each frame in one piece over the last, inside synchronized output. No
//! screen is cleared: clearing flickered, and Windows Terminal scrolls a cleared screen
//! into the scrollback, so every refresh left a copy there. Quitting puts back the screen
//! as it was.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::time::{Duration, Instant};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

const TICKS_PER_SECOND: f64 = 10_000_000.0;
/// Seconds between refreshes, as procps-ng's `top` has it.
const DEFAULT_DELAY: f64 = 3.0;
/// The first sample's length: long enough for a meaningful `%CPU`, short enough that
/// the screen appears at once.
const FIRST_SAMPLE: f64 = 0.3;
/// The shortest interval accepted.
const SHORTEST_DELAY: f64 = 0.1;

/// Display and update a sorted list of the processes using the machine.
#[derive(Parser)]
pub(crate) struct TopCommand {
    /// Batch mode: plain text, no screen control, for a pipe or a log.
    #[arg(short = 'b')]
    batch: bool,

    /// Stop after this many refreshes.
    #[arg(short = 'n', value_name = "COUNT")]
    iterations: Option<usize>,

    /// Seconds between refreshes (3 by default).
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
    parent_pid: u32,
    /// When the process started, a `FILETIME`; 0 where it could not be read.
    started: u64,
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
    processors: Vec<cash_win32::sysinfo::CpuTimes>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
    processors: Vec<CpuSummary>,
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

/// Linux's load sample, tasks running or waiting for a processor: the processors busy
/// over the interval, averaged exactly from the machine's CPU times, plus the threads
/// ready to run at the sample.
fn load_sample(
    before: Option<cash_win32::sysinfo::CpuTimes>,
    after: Option<cash_win32::sysinfo::CpuTimes>,
    elapsed_ticks: u64,
    ready_threads: u32,
) -> f64 {
    let busy = match (before, after) {
        (Some(before), Some(after)) if elapsed_ticks != 0 => {
            let total = after
                .kernel
                .saturating_sub(before.kernel)
                .saturating_add(after.user.saturating_sub(before.user));
            let busy = total.saturating_sub(after.idle.saturating_sub(before.idle));
            #[expect(
                clippy::cast_precision_loss,
                reason = "processor time over a refresh interval is far below f64's integer limit"
            )]
            let busy = busy as f64 / elapsed_ticks as f64;
            busy
        }
        _ => 0.0,
    };
    busy + f64::from(ready_threads)
}

/// Samples the machine, keeping what the next sample is measured against.
struct Sampler {
    query: cash_win32::sysinfo::SystemQuery,
    baseline: Baseline,
    load: LoadAverages,
    /// Each process's account, by pid and start time: a process's account never
    /// changes, and looking it up is a call into Windows' security service.
    owners: HashMap<(u32, u64), String>,
}

impl Sampler {
    fn new() -> Self {
        Self {
            query: cash_win32::sysinfo::SystemQuery::default(),
            baseline: take_baseline(),
            load: LoadAverages::default(),
            owners: HashMap::new(),
        }
    }

    /// Measures from now: time spent elsewhere (reading help) is not one long sample.
    fn rebaseline(&mut self) {
        self.baseline = take_baseline();
    }

    fn sample(&mut self, user: Option<&str>) -> Snapshot {
        let now = cash_win32::process::now_filetime();
        let system_cpu = cash_win32::sysinfo::cpu_times();
        let processors = cash_win32::sysinfo::processor_times().unwrap_or_default();
        let memory = cash_win32::sysinfo::memory_status();
        let total_memory = memory.map(|status| status.physical_total);
        let elapsed_ticks = now.saturating_sub(self.baseline.taken_at);

        #[expect(
            clippy::cast_precision_loss,
            reason = "an interval long enough to lose precision here is thousands of years"
        )]
        let elapsed = elapsed_ticks as f64 / TICKS_PER_SECOND;

        let (processes, ready_threads) = self.processes();
        let tasks = processes.len();
        let mut running = 0usize;
        let mut next_cpu = HashMap::with_capacity(tasks);
        let mut seen = HashSet::with_capacity(tasks);
        let mut rows = Vec::with_capacity(tasks);

        for process in processes {
            let usage = cash_win32::process::usage(process.pid);
            let started = usage.started.unwrap_or_default();
            seen.insert((process.pid, started));
            let owner = self
                .owners
                .entry((process.pid, started))
                .or_insert_with(|| {
                    cash_win32::process::owner(process.pid).unwrap_or_else(|| String::from("?"))
                })
                .clone();

            let cpu_total = usage.cpu.unwrap_or_default();
            next_cpu.insert(process.pid, cpu_total);
            let previous = self
                .baseline
                .cpu
                .get(&process.pid)
                .copied()
                .unwrap_or(cpu_total);
            let cpu_delta = cpu_total.saturating_sub(previous);
            let active = cpu_delta != 0;
            running += usize::from(active);

            // `id -un` may include DOMAIN\ while process tokens here do not.
            if let Some(wanted) = user
                && !owner.eq_ignore_ascii_case(account_name(wanted))
            {
                continue;
            }

            rows.push(Row {
                pid: process.pid,
                parent_pid: process.parent_pid,
                started,
                user: owner,
                name: process.name,
                priority: process.base_priority,
                active,
                cpu_share: share_of_a_processor(cpu_delta, elapsed),
                memory_share: memory_share(usage.resident, total_memory),
                resident: usage.resident.unwrap_or_default(),
                shared_resident: usage.shared_resident,
                committed: usage.committed.unwrap_or_default(),
                cpu_total,
            });
        }
        self.owners.retain(|key, _| seen.contains(key));

        let cpu = cpu_summary(self.baseline.system_cpu, system_cpu);
        let per_processor = per_processor(&self.baseline.processors, &processors);
        let load_average = ready_threads.map(|ready| {
            let sample = load_sample(self.baseline.system_cpu, system_cpu, elapsed_ticks, ready);
            self.load.update(sample, elapsed)
        });

        self.baseline = Baseline {
            taken_at: now,
            cpu: next_cpu,
            system_cpu,
            processors,
        };
        Snapshot {
            rows,
            tasks,
            running,
            cpu,
            processors: per_processor,
            load_average,
            memory,
        }
    }
}

impl Sampler {
    /// Every process, and the threads waiting for a processor where Windows says; the
    /// Toolhelp list, with no queue, where it does not.
    fn processes(&mut self) -> (Vec<cash_win32::sysinfo::SnapshotProcess>, Option<u32>) {
        if let Some(snapshot) = self.query.take() {
            return (snapshot.processes, Some(snapshot.ready_threads));
        }
        let listed = cash_win32::process::list()
            .into_iter()
            .map(|process| cash_win32::sysinfo::SnapshotProcess {
                pid: process.pid,
                parent_pid: process.parent_pid,
                name: process.name,
                base_priority: process.base_priority,
            })
            .collect();
        (listed, None)
    }
}

/// Each processor's use between two samples; none when the processors differ.
fn per_processor(
    before: &[cash_win32::sysinfo::CpuTimes],
    after: &[cash_win32::sysinfo::CpuTimes],
) -> Vec<CpuSummary> {
    if before.len() != after.len() {
        return Vec::new();
    }
    before
        .iter()
        .zip(after)
        .map(|(before, after)| cpu_summary(Some(*before), Some(*after)))
        .collect()
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
        processors: cash_win32::sysinfo::processor_times().unwrap_or_default(),
    }
}

/// How the CPU or the memory summary shows: as meters, as procps's text, or not at all.
/// `t` and `m` cycle them, as in procps-ng's `top`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Summary {
    Bars,
    Text,
    Hidden,
}

impl Summary {
    const fn next(self) -> Self {
        match self {
            Self::Bars => Self::Text,
            Self::Text => Self::Hidden,
            Self::Hidden => Self::Bars,
        }
    }
}

/// What the screen shows and how.
struct View {
    sort: SortField,
    /// Index of the first row shown.
    first_row: usize,
    tree: bool,
    per_processor: bool,
    cpu: Summary,
    memory: Summary,
    /// Colour on; `z` toggles it.
    colour: bool,
    /// The account `top` runs as, whose processes are coloured as `ls` colours its files.
    me: String,
    /// Only processes whose name contains this, ignoring case.
    filter: Option<String>,
    delay: f64,
    /// A one-off message for the status line, until the next refresh.
    message: Option<String>,
}

impl View {
    /// Batch mode's view: procps's text, no colour.
    const fn new(sort: SortField, delay: f64) -> Self {
        Self {
            sort,
            first_row: 0,
            tree: false,
            per_processor: false,
            cpu: Summary::Text,
            memory: Summary::Text,
            colour: false,
            me: String::new(),
            filter: None,
            delay,
            message: None,
        }
    }

    /// The interactive screen's: meters, and colour unless `NO_COLOR` asks for none.
    fn screen(sort: SortField, delay: f64, colour: bool) -> Self {
        Self {
            cpu: Summary::Bars,
            memory: Summary::Bars,
            colour,
            me: cash_win32::process::current_process_user().unwrap_or_default(),
            ..Self::new(sort, delay)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Timeout,
    Quit,
    Help,
    Refresh,
    Resize,
    Sort(SortField),
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Tree,
    PerProcessor,
    CpuSummary,
    MemorySummary,
    Colour,
    Filter,
    ClearFilter,
    Delay,
    Kill,
}

impl builtins::Command for TopCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let Some(sort) = SortField::parse(self.sort.as_deref()) else {
            let other = self.sort.as_deref().unwrap_or_default();
            writeln!(
                context.stderr(),
                "{}: {other}: unknown sort field; expected cpu, mem, time, or pid",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        };

        let delay = self.delay.unwrap_or(DEFAULT_DELAY).max(SHORTEST_DELAY);
        let refreshes = self.iterations.unwrap_or_else(|| usize::from(self.batch));
        let mut sampler = Sampler::new();

        if self.batch {
            let view = View::new(sort, delay);
            let mut out = context.stdout();
            for done in 1.. {
                let wait = if done == 1 {
                    FIRST_SAMPLE.min(delay)
                } else {
                    delay
                };
                tokio::time::sleep(Duration::from_secs_f64(wait)).await;
                let snapshot = sampler.sample(self.user.as_deref());
                for line in frame(&snapshot, &view, None).0 {
                    writeln!(out, "{}", line.text())?;
                }
                out.flush()?;
                if refreshes != 0 && done >= refreshes {
                    break;
                }
            }
        } else {
            // https://no-color.org: any value but an empty one asks for no colour.
            let colour = context
                .shell
                .env_str("NO_COLOR")
                .is_none_or(|value| value.is_empty());
            let view = View::screen(sort, delay, colour);
            let mut screen = FullScreen::enter(context.stdout())?;
            let _raw = RawMode::enter();
            interact(
                screen.out(),
                &mut sampler,
                view,
                refreshes,
                self.user.as_deref(),
            )?;
        }
        Ok(ExecutionResult::success())
    }
}

/// The interactive screen: a refresh every interval, and the keys in between.
fn interact(
    out: &mut impl Write,
    sampler: &mut Sampler,
    mut view: View,
    refreshes: usize,
    user: Option<&str>,
) -> Result<(), cash_core::Error> {
    let mut snapshot: Option<Snapshot> = None;
    let mut done = 0usize;
    let mut next_refresh = Instant::now() + Duration::from_secs_f64(FIRST_SAMPLE.min(view.delay));
    // Keys pressed before the first sample wait for it: a sample over a few milliseconds,
    // which a key would otherwise force, says nothing.
    let mut early = Vec::new();

    loop {
        let action = wait_for_action(next_refresh);
        if action == Action::Quit {
            return Ok(());
        }
        if snapshot.is_none() && action != Action::Timeout {
            early.push(action);
            continue;
        }
        if matches!(action, Action::Timeout | Action::Refresh) {
            snapshot = Some(sampler.sample(user));
            view.message = None;
            done += 1;
            next_refresh = Instant::now() + Duration::from_secs_f64(view.delay);
        }
        let Some(current) = &snapshot else {
            continue;
        };

        for action in std::mem::take(&mut early)
            .into_iter()
            .chain(std::iter::once(action))
        {
            act(action, &mut view, current, out, sampler, &mut next_refresh)?;
        }

        let (width, height) = terminal_size();
        let (lines, _) = frame(current, &view, Some((width, height)));
        paint(out, &lines, width, view.colour)?;

        if refreshes != 0 && done >= refreshes {
            return Ok(());
        }
    }
}

/// What a key does to the view; a refresh, a resize and quitting need nothing here.
fn act(
    action: Action,
    view: &mut View,
    current: &Snapshot,
    out: &mut impl Write,
    sampler: &mut Sampler,
    next_refresh: &mut Instant,
) -> Result<(), cash_core::Error> {
    let (width, height) = terminal_size();
    match action {
        Action::Help => {
            show_help(out, view, width, height)?;
            sampler.rebaseline();
            *next_refresh = Instant::now() + Duration::from_secs_f64(FIRST_SAMPLE);
        }
        Action::Sort(field) => {
            view.sort = field;
            view.first_row = 0;
        }
        Action::Up => view.first_row = view.first_row.saturating_sub(1),
        Action::Down => view.first_row = view.first_row.saturating_add(1),
        Action::PageUp => {
            view.first_row = view.first_row.saturating_sub(page(current, view, height));
        }
        Action::PageDown => {
            view.first_row = view.first_row.saturating_add(page(current, view, height));
        }
        Action::Home => view.first_row = 0,
        Action::End => view.first_row = usize::MAX,
        Action::Tree => {
            view.tree = !view.tree;
            view.first_row = 0;
        }
        Action::PerProcessor => view.per_processor = !view.per_processor,
        Action::CpuSummary => view.cpu = view.cpu.next(),
        Action::MemorySummary => view.memory = view.memory.next(),
        Action::Colour => view.colour = !view.colour,
        Action::ClearFilter => {
            view.filter = None;
            view.first_row = 0;
        }
        Action::Filter => {
            let lines = frame(current, view, Some((width, height))).0;
            let question = "Show only names containing (Enter alone shows all): ";
            if let Some(answer) = prompt(out, &lines, question, width, view.colour)? {
                let answer = answer.trim();
                view.filter = (!answer.is_empty()).then(|| answer.to_owned());
                view.first_row = 0;
            }
        }
        Action::Delay => {
            let lines = frame(current, view, Some((width, height))).0;
            let question = format!("Change delay from {:.1} to: ", view.delay);
            if let Some(answer) = prompt(out, &lines, &question, width, view.colour)? {
                match parse_delay(&answer) {
                    Ok(Some(delay)) => {
                        view.delay = delay;
                        *next_refresh = Instant::now() + Duration::from_secs_f64(delay);
                    }
                    Ok(None) => {}
                    Err(message) => view.message = Some(message),
                }
            }
        }
        Action::Kill => {
            let (lines, shown) = frame(current, view, Some((width, height)));
            let default = shown.first().copied();
            view.message = kill_prompt(out, &lines, default, width, view.colour)?;
        }
        Action::Timeout | Action::Refresh | Action::Resize | Action::Quit => {}
    }
    Ok(())
}

/// The alternate screen, with the cursor hidden, until dropped.
struct FullScreen<W: Write> {
    out: W,
}

impl<W: Write> FullScreen<W> {
    fn enter(mut out: W) -> std::io::Result<Self> {
        out.write_all(b"\x1b[?1049h\x1b[?25l")?;
        out.flush()?;
        Ok(Self { out })
    }

    const fn out(&mut self) -> &mut W {
        &mut self.out
    }
}

impl<W: Write> Drop for FullScreen<W> {
    fn drop(&mut self) {
        let _ = self.out.write_all(b"\x1b[?25h\x1b[?1049l");
        let _ = self.out.flush();
    }
}

/// Raw keyboard input for as long as it lives: keys arrive one at a time, and a key
/// pressed during a redraw is not echoed onto the screen.
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

fn terminal_size() -> (usize, usize) {
    crossterm::terminal::size().map_or((80, 24), |(width, height)| {
        (usize::from(width).max(1), usize::from(height).max(1))
    })
}

/// Waits for a key, a resize, or `deadline`, whichever comes first.
fn wait_for_action(deadline: Instant) -> Action {
    use crossterm::event::{Event, KeyEventKind, poll, read};

    loop {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return Action::Timeout;
        };
        match poll(left) {
            Ok(false) => return Action::Timeout,
            // Without a console to read, wait out the interval rather than spin.
            Err(_) => {
                std::thread::sleep(left);
                return Action::Timeout;
            }
            Ok(true) => {}
        }
        match read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                if let Some(action) = key_action(key) {
                    return action;
                }
            }
            Ok(Event::Resize(..)) => return Action::Resize,
            _ => {}
        }
    }
}

const fn key_action(key: crossterm::event::KeyEvent) -> Option<Action> {
    use crossterm::event::{KeyCode, KeyModifiers};

    Some(match key.code {
        KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('?' | 'h') => Action::Help,
        KeyCode::Enter | KeyCode::Char(' ' | 'r') => Action::Refresh,
        KeyCode::Char('P') => Action::Sort(SortField::Cpu),
        KeyCode::Char('M') => Action::Sort(SortField::Memory),
        KeyCode::Char('T') => Action::Sort(SortField::Time),
        KeyCode::Char('N') => Action::Sort(SortField::Pid),
        KeyCode::Char('V') => Action::Tree,
        KeyCode::Char('1') => Action::PerProcessor,
        KeyCode::Char('t') => Action::CpuSummary,
        KeyCode::Char('m') => Action::MemorySummary,
        KeyCode::Char('z') => Action::Colour,
        KeyCode::Char('o' | 'O' | '/') => Action::Filter,
        KeyCode::Char('=') => Action::ClearFilter,
        KeyCode::Char('d' | 's') => Action::Delay,
        KeyCode::Char('k') => Action::Kill,
        KeyCode::Up => Action::Up,
        KeyCode::Down => Action::Down,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Home => Action::Home,
        KeyCode::End => Action::End,
        _ => return None,
    })
}

/// Asks `question` on the status line, as procps's `top` does, and returns the answer;
/// `None` when Escape or Ctrl-C cancels it.
fn prompt(
    out: &mut impl Write,
    lines: &[Line],
    question: &str,
    width: usize,
    colour: bool,
) -> Result<Option<String>, cash_core::Error> {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, read};

    let mut answer = String::new();
    let result = loop {
        let mut shown = lines.to_vec();
        if let Some(last) = shown.last_mut() {
            *last = Line::plain(format!("{question}{answer}"));
        }
        paint(out, &shown, width, colour)?;
        write!(out, "\x1b[?25h")?;
        out.flush()?;

        match read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter => break Some(answer),
                KeyCode::Esc => break None,
                KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break None;
                }
                KeyCode::Backspace => {
                    answer.pop();
                }
                KeyCode::Char(character) => answer.push(character),
                _ => {}
            },
            Ok(_) => {}
            Err(_) => break None,
        }
    };
    write!(out, "\x1b[?25l")?;
    out.flush()?;
    Ok(result)
}

/// `d`'s answer: a new interval, `None` to keep the current one, or why it is refused.
fn parse_delay(answer: &str) -> Result<Option<f64>, String> {
    let answer = answer.trim();
    if answer.is_empty() {
        return Ok(None);
    }
    match answer.parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds > 0.0 => {
            Ok(Some(seconds.max(SHORTEST_DELAY)))
        }
        _ => Err(format!("Unacceptable interval: {answer}")),
    }
}

/// `k`: asks for a pid (the first row shown by default) and a signal (`TERM` by
/// default), sends it by the rules of cash's `kill`, and says what happened.
fn kill_prompt(
    out: &mut impl Write,
    lines: &[Line],
    default: Option<u32>,
    width: usize,
    colour: bool,
) -> Result<Option<String>, cash_core::Error> {
    let question = default.map_or_else(
        || String::from("PID to signal/kill: "),
        |pid| format!("PID to signal/kill [default pid = {pid}]: "),
    );
    let Some(answer) = prompt(out, lines, &question, width, colour)? else {
        return Ok(None);
    };
    let pid = match answer.trim() {
        "" => default,
        text => text.parse::<u32>().ok(),
    };
    let Some(pid) = pid.filter(|&pid| pid != 0) else {
        return Ok(Some(format!("Unacceptable pid: {}", answer.trim())));
    };
    // `kill 0` means every process the shell started, and the shell must not go.
    if pid == std::process::id() {
        return Ok(Some(String::from(
            "top does not signal the shell it runs in",
        )));
    }

    let question = format!("Send pid {pid} signal [15/sigterm]: ");
    let Some(answer) = prompt(out, lines, &question, width, colour)? else {
        return Ok(None);
    };
    let answer = answer.trim();
    let name = if answer.is_empty() { "TERM" } else { answer };
    let Ok(signal @ cash_core::traps::TrapSignal::Signal(_)) =
        name.parse::<cash_core::traps::TrapSignal>()
    else {
        return Ok(Some(format!("Unknown signal: {name}")));
    };
    let Ok(target) = i32::try_from(pid) else {
        return Ok(Some(format!("Unacceptable pid: {pid}")));
    };
    Ok(Some(
        match cash_core::sys::signal::kill_process(target, signal) {
            Ok(()) => format!("Sent {} to {pid}", signal.as_str()),
            Err(error) => format!("{pid}: {error}"),
        },
    ))
}

fn show_help(
    out: &mut impl Write,
    view: &View,
    width: usize,
    height: usize,
) -> Result<(), cash_core::Error> {
    use crossterm::event::{Event, KeyEventKind, read};

    let text = format!(
        "cash top - interactive help\n\
         Delay {delay:.1} seconds; sort: {sort}\n\
         \n\
         \x20 Enter/Space/r   refresh now\n\
         \x20 d or s          change the delay\n\
         \x20 P M T N         sort by processor, memory, CPU time, pid\n\
         \x20 V               tree view: children under their parents\n\
         \x20 1               a CPU meter or line per processor\n\
         \x20 t and m         CPU and memory as meters, as text, or hidden\n\
         \x20 z               colour on or off\n\
         \x20 o or /          show only names containing some text; = shows all\n\
         \x20 k               send a signal to a process (TERM asks it to exit)\n\
         \x20 Up/Down         scroll one row\n\
         \x20 PgUp/PgDn       scroll one page\n\
         \x20 Home/End        first/last page\n\
         \x20 ? or h          this help\n\
         \x20 q or Esc        leave top\n\
         \n\
         Windows notes:\n\
         \x20 R/S means CPU-active/idle in the last sample.\n\
         \x20 PRI is Windows base priority; SHR is shared resident working set.\n\
         \x20 Load average: busy processors plus threads ready to run, averaged\n\
         \x20 over 1, 5 and 15 minutes since top started.\n\
         \n\
         Press any key to return",
        delay = view.delay,
        sort = view.sort.label()
    );
    let lines: Vec<Line> = text
        .lines()
        .take(height)
        .map(|line| Line::plain(truncate(line, width)))
        .collect();
    loop {
        let (width, _) = terminal_size();
        paint(out, &lines, width, false)?;
        match read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => return Ok(()),
            Ok(_) => {}
            Err(_) => return Ok(()),
        }
    }
}

/// How a span of text is drawn when colour is on: an SGR colour (empty for the terminal's
/// own) and bold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Style {
    colour: &'static str,
    bold: bool,
}

impl Style {
    const fn colour(colour: &'static str) -> Self {
        Self {
            colour,
            bold: false,
        }
    }

    const fn bold(self, bold: bool) -> Self {
        Self { bold, ..self }
    }

    const fn is_plain(self) -> bool {
        self.colour.is_empty() && !self.bold
    }

    fn sgr(self) -> String {
        match (self.bold, self.colour.is_empty()) {
            (true, true) => String::from("\x1b[1m"),
            (true, false) => format!("\x1b[1;{}m", self.colour),
            (false, _) => format!("\x1b[{}m", self.colour),
        }
    }
}

const PLAIN: Style = Style::colour("");
/// The summary's figures.
const FIGURE: Style = PLAIN.bold(true);
/// Processor time in user mode, and in the kernel: htop's green and red.
const USER_TIME: Style = Style::colour("32");
const SYSTEM_TIME: Style = Style::colour("31");
/// Physical memory in use, and memory committed.
const MEMORY_USED: Style = Style::colour("32");
const COMMIT_USED: Style = Style::colour("33");
/// The column the table is sorted by.
const SORTED: Style = Style::colour("36");

/// A run of text in one style.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Span {
    text: String,
    style: Style,
}

/// One line of a frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Line {
    spans: Vec<Span>,
    /// Shown in reverse video across the whole width, as the column header is.
    inverse: bool,
}

impl Line {
    fn plain(text: impl Into<String>) -> Self {
        let mut line = Self::default();
        line.push(text, PLAIN);
        line
    }

    fn push(&mut self, text: impl Into<String>, style: Style) -> &mut Self {
        self.spans.push(Span {
            text: text.into(),
            style,
        });
        self
    }

    /// The text without its styles, as batch mode prints it.
    fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// Writes `lines` over the screen in one piece, in colour when `colour`.
///
/// From the top-left corner, each line overwrites the one before it and erases what is
/// left of it; below the last, the screen is erased. The whole frame is one write inside
/// synchronized output (mode 2026), so a terminal that supports it shows it at once. A
/// line as wide as the screen gets no erase: the cursor then sits on its last character,
/// which the erase would take.
fn paint(
    out: &mut impl Write,
    lines: &[Line],
    width: usize,
    colour: bool,
) -> Result<(), cash_core::Error> {
    let mut frame = String::with_capacity(width.saturating_add(16) * (lines.len() + 1) + 32);
    frame.push_str("\x1b[?2026h\x1b[H");
    for (index, line) in lines.iter().enumerate() {
        if index != 0 {
            frame.push_str("\r\n");
        }
        let last = index + 1 == lines.len();
        // The last line stops short of the edge, so that the erase below it takes nothing.
        let room = if last { width.saturating_sub(1) } else { width };
        let base = if line.inverse { "\x1b[0;7m" } else { "\x1b[0m" };
        if line.inverse {
            frame.push_str(base);
        }
        let mut length = 0;
        for span in &line.spans {
            let text = truncate(&span.text, room - length);
            length += text.chars().count();
            let styled = colour && !span.style.is_plain() && !text.is_empty();
            if styled {
                frame.push_str(&span.style.sgr());
            }
            frame.push_str(&text);
            if styled {
                frame.push_str(base);
            }
        }
        if line.inverse {
            frame.extend(std::iter::repeat_n(' ', room - length));
            frame.push_str("\x1b[0m");
        } else if length < width {
            frame.push_str("\x1b[K");
        }
    }
    frame.push_str("\x1b[J\x1b[?2026l");
    out.write_all(frame.as_bytes())?;
    out.flush()?;
    Ok(())
}

/// How many rows a page scroll moves.
fn page(snapshot: &Snapshot, view: &View, height: usize) -> usize {
    let (width, _) = terminal_size();
    table_height(summary(snapshot, view, width).len(), height)
}

/// Rows of the table that fit under `summary` lines, a spacer and the header, above a
/// spacer and the status line.
const fn table_height(summary: usize, height: usize) -> usize {
    let height = height.saturating_sub(summary + 4);
    if height == 0 { 1 } else { height }
}

/// The lines of one frame, and the pids of the rows it shows. `screen` is the terminal's
/// size, or `None` for batch mode, which shows every row and no status line.
fn frame(
    snapshot: &Snapshot,
    view: &View,
    screen: Option<(usize, usize)>,
) -> (Vec<Line>, Vec<u32>) {
    let width = screen.map_or(usize::MAX, |(width, _)| width);
    let mut lines = summary(snapshot, view, width);
    let summary_lines = lines.len();
    lines.push(Line::default());

    let show_shared = snapshot
        .rows
        .iter()
        .any(|row| row.shared_resident.is_some());
    lines.push(header_line(show_shared, view.sort, screen.is_some()));

    let arranged = arrange(&snapshot.rows, view);
    let shown: &[(&Row, usize)] = match screen {
        None => &arranged,
        Some((_, height)) => {
            let rows = table_height(summary_lines, height);
            let first = view.first_row.min(arranged.len().saturating_sub(rows));
            &arranged[first..(first + rows).min(arranged.len())]
        }
    };
    for (row, depth) in shown {
        lines.push(row_line(row, show_shared, *depth, view));
    }
    let pids = shown.iter().map(|(row, _)| row.pid).collect();

    if screen.is_some() {
        lines.push(Line::default());
        lines.push(Line::plain(status(view)));
    }
    (lines, pids)
}

/// The column header, in reverse video on the screen, the sort column's name in bold.
fn header_line(show_shared: bool, sort: SortField, inverse: bool) -> Line {
    let text = header(show_shared);
    let mut line = Line {
        inverse,
        ..Line::default()
    };
    match text.find(sort.label()) {
        Some(at) => {
            let (before, rest) = text.split_at(at);
            let (label, after) = rest.split_at(sort.label().len());
            line.push(before, PLAIN)
                .push(label, FIGURE)
                .push(after, PLAIN);
        }
        None => {
            line.push(text, PLAIN);
        }
    }
    line
}

/// A row of the table: the owner coloured as `ls` colours a file's, the sort column
/// picked out, and the whole row bold when the process ran in the last sample.
fn row_line(row: &Row, show_shared: bool, depth: usize, view: &View) -> Line {
    let bold = row.active;
    let base = PLAIN.bold(bold);
    let owner = Style::colour(crate::ls::owner_colour(&row.user, &view.me)).bold(bold);
    let sorted = |field: SortField| {
        if view.sort == field {
            SORTED.bold(bold)
        } else {
            base
        }
    };
    let command = if depth == 0 {
        row.name.clone()
    } else {
        format!("{} `- {}", " ".repeat((depth - 1) * 2), row.name)
    };

    let mut line = Line::default();
    line.push(format!("{:>7}", row.pid), sorted(SortField::Pid))
        .push(" ", base)
        .push(
            format!(
                "{:<10}",
                truncate(&crate::ps::user_column(Some(&row.user)), 10)
            ),
            owner,
        )
        .push(
            format!(
                " {:>3} {:>7} {:>7}",
                row.priority,
                mebibytes(row.committed),
                mebibytes(row.resident)
            ),
            base,
        );
    if show_shared {
        let shared = mebibytes(row.shared_resident.unwrap_or_default());
        line.push(format!(" {shared:>7}"), base);
    }
    line.push(format!(" {} ", if row.active { 'R' } else { 'S' }), base)
        .push(
            format!("{:>5}", cpu_percent(row.cpu_share)),
            sorted(SortField::Cpu),
        )
        .push(" ", base)
        .push(
            format!("{:>5.1}", row.memory_share),
            sorted(SortField::Memory),
        )
        .push(" ", base)
        .push(
            format!("{:>TIME_WIDTH$}", cpu_time_plus(row.cpu_total)),
            sorted(SortField::Time),
        )
        .push(format!(" {command}"), base);
    line
}

fn status(view: &View) -> String {
    if let Some(message) = &view.message {
        return message.clone();
    }
    let mut parts = vec![format!("sort: {}", view.sort.label())];
    if view.tree {
        parts.push(String::from("tree"));
    }
    if let Some(filter) = &view.filter {
        parts.push(format!("names containing {filter:?} (= shows all)"));
    }
    parts.push(String::from("? help"));
    parts.push(String::from("q quit"));
    parts.join("   ")
}

/// The rows to show, in order, with each one's depth in the tree view.
fn arrange<'a>(rows: &'a [Row], view: &View) -> Vec<(&'a Row, usize)> {
    let filter = view.filter.as_deref().map(str::to_lowercase);
    let mut kept: Vec<&Row> = rows
        .iter()
        .filter(|row| {
            filter
                .as_deref()
                .is_none_or(|filter| row.name.to_lowercase().contains(filter))
        })
        .collect();
    if view.tree {
        return tree_order(&kept, view.sort);
    }
    kept.sort_by(|a, b| compare(a, b, view.sort));
    kept.into_iter().map(|row| (row, 0)).collect()
}

fn compare(a: &Row, b: &Row, field: SortField) -> std::cmp::Ordering {
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
}

/// Each process under its parent, siblings in `sort` order, with its depth.
///
/// Windows reuses pids, so a parent pid names the parent only when that process started
/// before the child; otherwise the child's parent has exited, and it heads a tree of its
/// own. A process caught in a loop of such pids is shown at the top level rather than
/// lost.
fn tree_order<'a>(rows: &[&'a Row], sort: SortField) -> Vec<(&'a Row, usize)> {
    let by_pid: HashMap<u32, &Row> = rows.iter().map(|row| (row.pid, *row)).collect();
    let mut children: HashMap<u32, Vec<&Row>> = HashMap::new();
    let mut roots = Vec::new();
    for row in rows {
        let parent = by_pid
            .get(&row.parent_pid)
            .filter(|parent| parent.pid != row.pid && parent.started <= row.started);
        match parent {
            Some(parent) => children.entry(parent.pid).or_default().push(row),
            None => roots.push(*row),
        }
    }
    roots.sort_by(|a, b| compare(a, b, sort));
    for siblings in children.values_mut() {
        siblings.sort_by(|a, b| compare(a, b, sort));
    }

    let mut ordered = Vec::with_capacity(rows.len());
    let mut visited = HashSet::with_capacity(rows.len());
    let mut pending: Vec<(&Row, usize)> = Vec::new();
    let mut top_level = roots;
    loop {
        for root in std::mem::take(&mut top_level).into_iter().rev() {
            pending.push((root, 0));
        }
        while let Some((row, depth)) = pending.pop() {
            if !visited.insert(row.pid) {
                continue;
            }
            ordered.push((row, depth));
            if let Some(siblings) = children.get(&row.pid) {
                for child in siblings.iter().rev() {
                    pending.push((child, depth + 1));
                }
            }
        }
        // Rows no root reached are in a loop of parent pids: show them at the top level.
        let mut missed: Vec<&Row> = rows
            .iter()
            .copied()
            .filter(|row| !visited.contains(&row.pid))
            .collect();
        if missed.is_empty() {
            return ordered;
        }
        missed.sort_by(|a, b| compare(a, b, sort));
        top_level = missed.into_iter().take(1).collect();
    }
}

/// A meter's label, padded so that every meter's `[` lines up: `%Cpu(s) `, `Mem     `.
fn meter_label(name: &str) -> String {
    format!("{name:<7} ")
}

/// The summary above the table on a screen `width` wide: the time and load, the tasks,
/// then the processors and the memory, each as meters, as procps's text, or not at all.
///
/// The meters form one block, as wide as the column header up to `COMMAND` (or the
/// screen, if narrower), so that their readings stay by their bars however wide the
/// window is, and their right edges end where the header's text does.
fn summary(snapshot: &Snapshot, view: &View, width: usize) -> Vec<Line> {
    let show_shared = snapshot
        .rows
        .iter()
        .any(|row| row.shared_resident.is_some());
    let width = width.min(header(show_shared).chars().count());
    let uptime = format_uptime(cash_win32::sysinfo::uptime());
    let now = chrono::Local::now().format("%H:%M:%S");
    let mut lines = Vec::with_capacity(6 + snapshot.processors.len());

    let mut top = Line::plain(format!("top - {now} up {uptime}"));
    if let Some([one, five, fifteen]) = snapshot.load_average {
        top.push(",  load average: ", PLAIN)
            .push(format!("{one:.2}"), FIGURE)
            .push(", ", PLAIN)
            .push(format!("{five:.2}"), FIGURE)
            .push(", ", PLAIN)
            .push(format!("{fifteen:.2}"), FIGURE);
    }
    lines.push(top);

    let mut tasks = Line::plain("Tasks: ");
    tasks
        .push(format!("{:>5}", snapshot.tasks), FIGURE)
        .push(" total, ", PLAIN)
        .push(format!("{:>3}", snapshot.running), FIGURE)
        .push(" running, ", PLAIN)
        .push(
            format!("{:>3}", snapshot.tasks.saturating_sub(snapshot.running)),
            FIGURE,
        )
        .push(" sleeping", PLAIN);
    lines.push(tasks);

    cpu_lines(&mut lines, snapshot, view, width);
    if let Some(memory) = snapshot.memory {
        memory_lines(&mut lines, memory, view.memory, width);
    }
    lines
}

/// The processors' part of the summary.
fn cpu_lines(lines: &mut Vec<Line>, snapshot: &Snapshot, view: &View, width: usize) {
    let processors = view.per_processor && !snapshot.processors.is_empty();
    match (view.cpu, processors) {
        (Summary::Hidden, _) => {}
        (Summary::Text, false) => lines.push(cpu_text("%Cpu(s):", snapshot.cpu)),
        (Summary::Text, true) => {
            for (index, cpu) in snapshot.processors.iter().enumerate() {
                lines.push(cpu_text(&format!("%Cpu{index:<3}:"), *cpu));
            }
        }
        (Summary::Bars, false) => {
            let mut line = Line::default();
            cpu_meter(&mut line, &meter_label("%Cpu(s)"), snapshot.cpu, width);
            lines.push(line);
        }
        (Summary::Bars, true) => {
            // Two to a row, as htop lays out many processors, filling the same width as
            // the other meters: the right one takes what an odd width leaves over.
            let left = width.saturating_sub(1) / 2;
            let right = width.saturating_sub(left + 1);
            for (pair, cpus) in snapshot.processors.chunks(2).enumerate() {
                let mut line = Line::default();
                for (offset, cpu) in cpus.iter().enumerate() {
                    let label = meter_label(&format!("%Cpu{}", pair * 2 + offset));
                    if offset == 0 {
                        cpu_meter(&mut line, &label, *cpu, left);
                    } else {
                        line.push(" ", PLAIN);
                        cpu_meter(&mut line, &label, *cpu, right);
                    }
                }
                lines.push(line);
            }
        }
    }
}

/// The memory part of the summary.
fn memory_lines(
    lines: &mut Vec<Line>,
    memory: cash_win32::sysinfo::MemoryStatus,
    shown: Summary,
    width: usize,
) {
    let used = memory
        .physical_total
        .saturating_sub(memory.physical_available);
    match shown {
        Summary::Hidden => {}
        Summary::Text => {
            let mut physical = Line::plain("MiB Mem : ");
            physical
                .push(format!("{:>9.1}", mib(memory.physical_total)), FIGURE)
                .push(" total, ", PLAIN)
                .push(format!("{:>9.1}", mib(memory.physical_available)), FIGURE)
                .push(" free, ", PLAIN)
                .push(format!("{:>9.1}", mib(used)), FIGURE)
                .push(" used", PLAIN);
            lines.push(physical);
            let mut commit = Line::plain("MiB Commit: ");
            commit
                .push(format!("{:>8.1}", mib(memory.commit_total)), FIGURE)
                .push(" used, ", PLAIN)
                .push(format!("{:>8.1}", mib(memory.commit_limit)), FIGURE)
                .push(" limit, ", PLAIN)
                .push(format!("{:>8.1}", mib(memory.commit_peak)), FIGURE)
                .push(" peak", PLAIN);
            lines.push(commit);
        }
        Summary::Bars => {
            let mut physical = Line::default();
            let total = memory.physical_total;
            memory_meter(
                &mut physical,
                &meter_label("Mem"),
                used,
                total,
                MEMORY_USED,
                width,
            );
            lines.push(physical);
            let mut commit = Line::default();
            let (used, limit) = (memory.commit_total, memory.commit_limit);
            memory_meter(
                &mut commit,
                &meter_label("Commit"),
                used,
                limit,
                COMMIT_USED,
                width,
            );
            lines.push(commit);
        }
    }
}

/// procps's `%Cpu` line: user, system and idle percentages.
fn cpu_text(label: &str, cpu: CpuSummary) -> Line {
    let mut line = Line::plain(format!("{label} "));
    line.push(format!("{:>5.1}", cpu.user), FIGURE)
        .push(" us, ", PLAIN)
        .push(format!("{:>5.1}", cpu.system), FIGURE)
        .push(" sy, ", PLAIN)
        .push(format!("{:>5.1}", cpu.idle), FIGURE)
        .push(" id", PLAIN);
    line
}

/// A processor meter: user time green, kernel time red, and the share busy.
fn cpu_meter(line: &mut Line, label: &str, cpu: CpuSummary, width: usize) {
    let reading = format!("{:.1}%", (100.0 - cpu.idle).max(0.0));
    meter(
        line,
        label,
        &[
            (cpu.user / 100.0, USER_TIME),
            (cpu.system / 100.0, SYSTEM_TIME),
        ],
        &reading,
        width,
    );
}

/// A memory meter: `used` of `total`, in gibibytes.
fn memory_meter(line: &mut Line, label: &str, used: u64, total: u64, style: Style, width: usize) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a memory meter needs only one decimal of a gibibyte"
    )]
    let (used, total) = (used as f64, total as f64);
    let gib = 1024.0 * 1024.0 * 1024.0;
    let reading = format!("{:.1}G/{:.1}G", used / gib, total / gib);
    let fraction = if total > 0.0 { used / total } else { 0.0 };
    meter(line, label, &[(fraction, style)], &reading, width);
}

/// A meter as `htop` and `NTop` draw one, `width` wide: `label[|||||||      23.4%]`, each
/// part of the bar a fraction of it in its own style, the reading at its right end.
fn meter(line: &mut Line, label: &str, parts: &[(f64, Style)], reading: &str, width: usize) {
    let inside = width.saturating_sub(label.chars().count() + 2);
    let reading_width = reading.chars().count();
    let (room, reading) = if inside > reading_width {
        (inside - reading_width - 1, reading)
    } else {
        (inside, "")
    };

    line.push(label, PLAIN).push("[", FIGURE);
    let mut drawn = 0;
    let mut reached = 0.0;
    for &(fraction, style) in parts {
        reached = (reached + fraction.max(0.0)).min(1.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a fraction of a meter a few hundred cells wide, clamped to 0..=1"
        )]
        let end = ((reached * room as f64).round() as usize).min(room);
        if end > drawn {
            line.push("|".repeat(end - drawn), style);
            drawn = end;
        }
    }
    let gap = room - drawn + usize::from(!reading.is_empty());
    line.push(" ".repeat(gap), PLAIN)
        .push(reading, PLAIN)
        .push("]", FIGURE);
}

fn header(show_shared: bool) -> String {
    if show_shared {
        format!(
            "{:>7} {:<10} {:>3} {:>7} {:>7} {:>7} S {:>5} {:>5} {:>9} COMMAND",
            "PID", "USER", "PRI", "VIRT", "RES", "SHR", "%CPU", "%MEM", "TIME+"
        )
    } else {
        format!(
            "{:>7} {:<10} {:>3} {:>7} {:>7} S {:>5} {:>5} {:>9} COMMAND",
            "PID", "USER", "PRI", "VIRT", "RES", "%CPU", "%MEM", "TIME+"
        )
    }
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

/// The width of the `TIME+` column.
const TIME_WIDTH: usize = 9;

/// CPU time as `TIME+` shows it, in its nine columns: minutes, seconds and hundredths
/// while they fit, and coarser as the time grows, as procps's `top` scales it: minutes
/// and seconds, then hours and minutes, then days and hours, then weeks and days.
/// `1423:07.21` would push the command a column right; it shows as `1423:07`.
fn cpu_time_plus(ticks: u64) -> String {
    let hundredths = ticks / 100_000;
    let seconds = hundredths / 100;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    let weeks = days / 7;
    [
        format!("{minutes}:{:02}.{:02}", seconds % 60, hundredths % 100),
        format!("{minutes}:{:02}", seconds % 60),
        format!("{hours},{:02}", minutes % 60),
        format!("{days}d+{:02}h", hours % 24),
        format!("{weeks}w+{}d", days % 7),
    ]
    .into_iter()
    .find(|shown| shown.len() <= TIME_WIDTH)
    .unwrap_or_else(|| format!("{weeks}w"))
}

/// `%CPU` in its five columns: one decimal until a process busy on ten processors or
/// more needs the room.
fn cpu_percent(share: f64) -> String {
    if share < 999.95 {
        format!("{share:.1}")
    } else {
        format!("{share:.0}")
    }
}

fn format_uptime(uptime: Duration) -> String {
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

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests assert loudly on failure")]
mod tests {
    use super::*;
    use cash_win32::sysinfo::CpuTimes;

    fn row(pid: u32, parent_pid: u32, started: u64, name: &str, cpu_share: f64) -> Row {
        Row {
            pid,
            parent_pid,
            started,
            user: String::from("me"),
            name: name.to_owned(),
            priority: 8,
            active: cpu_share > 0.0,
            cpu_share,
            memory_share: 0.0,
            resident: 0,
            shared_resident: None,
            committed: 0,
            cpu_total: 0,
        }
    }

    fn snapshot(rows: Vec<Row>) -> Snapshot {
        Snapshot {
            tasks: rows.len(),
            running: 0,
            rows,
            cpu: CpuSummary::default(),
            processors: vec![CpuSummary::default(); 4],
            load_average: Some([1.0, 0.5, 0.25]),
            memory: None,
        }
    }

    fn names(arranged: &[(&Row, usize)]) -> Vec<(String, usize)> {
        arranged
            .iter()
            .map(|(row, depth)| (row.name.clone(), *depth))
            .collect()
    }

    #[test]
    fn the_load_sample_is_busy_processors_plus_ready_threads() {
        let before = CpuTimes {
            idle: 1_000,
            kernel: 3_000,
            user: 1_000,
        };
        // Over 1,000 ticks of wall time, 4,000 ticks of processor time of which 1,000 idle:
        // three processors busy on average. Two threads waited for one at the sample.
        let after = CpuTimes {
            idle: 2_000,
            kernel: 5_000,
            user: 3_000,
        };
        let sample = load_sample(Some(before), Some(after), 1_000, 2);
        assert!((sample - 5.0).abs() < 1e-9, "{sample}");
        assert!((load_sample(None, Some(after), 1_000, 2) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn load_averages_decay_towards_the_sample_by_elapsed_time() {
        let mut load = LoadAverages::default();
        assert!(
            load.update(4.0, 3.0)
                .iter()
                .all(|value| (value - 4.0).abs() < 1e-12)
        );
        let [one, five, fifteen] = load.update(0.0, 60.0);
        assert!((one - 4.0 / std::f64::consts::E).abs() < 1e-9, "{one}");
        assert!(one < five && five < fifteen && fifteen < 4.0);
    }

    #[test]
    fn a_tree_puts_children_under_parents_in_sort_order() {
        let rows = [
            row(4, 0, 1, "System", 0.0),
            row(100, 4, 2, "smss.exe", 0.0),
            row(200, 999, 3, "orphan.exe", 5.0),
            row(300, 100, 4, "busy.exe", 9.0),
            row(301, 100, 5, "idle.exe", 1.0),
        ];
        let refs: Vec<&Row> = rows.iter().collect();
        assert_eq!(
            names(&tree_order(&refs, SortField::Cpu)),
            [
                ("orphan.exe".to_owned(), 0),
                ("System".to_owned(), 0),
                ("smss.exe".to_owned(), 1),
                ("busy.exe".to_owned(), 2),
                ("idle.exe".to_owned(), 2),
            ]
        );
    }

    #[test]
    fn a_reused_parent_pid_does_not_adopt_an_older_process() {
        // Process 50 started after 60 did: 60's real parent exited, and 50 is a stranger.
        let rows = [
            row(50, 0, 20, "newer.exe", 0.0),
            row(60, 50, 10, "older.exe", 0.0),
        ];
        let refs: Vec<&Row> = rows.iter().collect();
        assert!(
            tree_order(&refs, SortField::Pid)
                .iter()
                .all(|(_, depth)| *depth == 0)
        );
    }

    #[test]
    fn a_loop_of_parent_pids_loses_no_process() {
        let rows = [row(1, 2, 0, "a.exe", 0.0), row(2, 1, 0, "b.exe", 0.0)];
        let refs: Vec<&Row> = rows.iter().collect();
        let ordered = tree_order(&refs, SortField::Pid);
        assert_eq!(
            names(&ordered),
            [("a.exe".to_owned(), 0), ("b.exe".to_owned(), 1)]
        );
    }

    #[test]
    fn a_filter_keeps_names_containing_it_in_any_case() {
        let rows = [
            row(1, 0, 0, "Cash.exe", 0.0),
            row(2, 0, 0, "ping.exe", 0.0),
            row(3, 0, 0, "cashctl.exe", 0.0),
        ];
        let mut view = View::new(SortField::Pid, 3.0);
        view.filter = Some(String::from("CASH"));
        assert_eq!(
            names(&arrange(&rows, &view)),
            [("Cash.exe".to_owned(), 0), ("cashctl.exe".to_owned(), 0)]
        );
    }

    #[test]
    fn a_frame_fits_the_screen_with_the_status_line_last() {
        let rows = (1..=100).map(|pid| row(pid, 0, 0, "p.exe", 0.0)).collect();
        let view = View::new(SortField::Pid, 3.0);
        let (lines, pids) = frame(&snapshot(rows), &view, Some((80, 24)));
        assert_eq!(lines.len(), 24);
        assert!(lines.last().unwrap().text().starts_with("sort: PID"));
        assert_eq!(pids.first(), Some(&1));
        // Three summary lines, a spacer and the header above; a spacer and status below.
        assert_eq!(pids.len(), 24 - 3 - 4);
    }

    #[test]
    fn one_shows_a_text_line_per_processor() {
        let mut view = View::new(SortField::Pid, 3.0);
        view.per_processor = true;
        let lines: Vec<String> = summary(&snapshot(Vec::new()), &view, 80)
            .iter()
            .map(Line::text)
            .collect();
        assert_eq!(
            lines.iter().filter(|line| line.starts_with("%Cpu")).count(),
            4
        );
        assert!(lines.iter().any(|line| line.starts_with("%Cpu3  :")));
    }

    #[test]
    fn as_meters_processors_come_two_to_a_row() {
        let mut view = View::screen(SortField::Pid, 3.0, true);
        view.per_processor = true;
        let lines: Vec<String> = summary(&snapshot(Vec::new()), &view, 81)
            .iter()
            .map(Line::text)
            .collect();
        let meters: Vec<&String> = lines
            .iter()
            .filter(|line| line.starts_with("%Cpu"))
            .collect();
        assert_eq!(meters.len(), 2, "{lines:#?}");
        assert!(meters[1].contains("] %Cpu3   ["), "{}", meters[1]);
    }

    #[test]
    fn the_meters_form_one_aligned_block() {
        let mut memory = snapshot(Vec::new());
        memory.memory = Some(cash_win32::sysinfo::MemoryStatus {
            physical_total: 32 << 30,
            physical_available: 13 << 30,
            commit_total: 33 << 30,
            commit_limit: 54 << 30,
            commit_peak: 40 << 30,
        });
        // As wide as the column header up to COMMAND, however wide the screen.
        let block = header(false).chars().count();
        for per_processor in [false, true] {
            let mut view = View::screen(SortField::Cpu, 3.0, true);
            view.per_processor = per_processor;
            let meters: Vec<String> = summary(&memory, &view, 200)
                .iter()
                .map(Line::text)
                .filter(|line| line.contains('['))
                .collect();
            assert_eq!(
                meters.len(),
                if per_processor { 4 } else { 3 },
                "{meters:#?}"
            );
            for line in &meters {
                assert_eq!(line.chars().count(), block, "{line:?}");
                assert_eq!(line.find('['), Some(8), "{line:?}");
            }
            if per_processor {
                let right: Vec<Option<usize>> =
                    meters[..2].iter().map(|line| line.rfind('[')).collect();
                assert_eq!(right[0], right[1], "{meters:#?}");
            }
        }
    }

    #[test]
    fn a_meter_fills_its_share_and_ends_with_its_reading() {
        let mut line = Line::default();
        meter(&mut line, "Mem ", &[(0.5, MEMORY_USED)], "8.0G/16.0G", 36);
        let text = line.text();
        // 30 inside the brackets: the reading, a space, and 19 cells, half of them bar.
        assert_eq!(text, "Mem [||||||||||          8.0G/16.0G]");
        assert_eq!(text.chars().count(), 36);
        // Too narrow for the reading: the bar alone.
        let mut line = Line::default();
        meter(&mut line, "M", &[(1.0, MEMORY_USED)], "8.0G/16.0G", 8);
        assert_eq!(line.text(), "M[|||||]");
    }

    #[test]
    fn t_and_m_cycle_meters_text_and_nothing() {
        let mut view = View::screen(SortField::Pid, 3.0, true);
        let mut memory = snapshot(Vec::new());
        memory.memory = Some(cash_win32::sysinfo::MemoryStatus {
            physical_total: 16 << 30,
            physical_available: 8 << 30,
            commit_total: 4 << 30,
            commit_limit: 32 << 30,
            commit_peak: 5 << 30,
        });
        let texts = |view: &View| -> Vec<String> {
            summary(&memory, view, 80).iter().map(Line::text).collect()
        };
        assert!(texts(&view)[2].starts_with("%Cpu(s) ["));
        assert!(texts(&view)[3].starts_with("Mem     ["));
        assert!(texts(&view)[4].starts_with("Commit  ["));
        view.cpu = view.cpu.next();
        view.memory = view.memory.next();
        assert!(texts(&view)[2].starts_with("%Cpu(s):"));
        assert!(texts(&view)[3].starts_with("MiB Mem :"));
        view.cpu = view.cpu.next();
        view.memory = view.memory.next();
        assert_eq!(texts(&view).len(), 2);
    }

    #[test]
    fn colour_is_written_only_when_on() {
        let mut line = Line::plain("load ");
        line.push("0.47", FIGURE);
        let lines = [line, Line::plain("status")];
        let mut plain = Vec::new();
        paint(&mut plain, &lines, 20, false).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        assert!(plain.contains("load 0.47\x1b[K"), "{plain:?}");
        let mut coloured = Vec::new();
        paint(&mut coloured, &lines, 20, true).unwrap();
        let coloured = String::from_utf8(coloured).unwrap();
        assert!(
            coloured.contains("load \x1b[1m0.47\x1b[0m\x1b[K"),
            "{coloured:?}"
        );
    }

    #[test]
    fn the_header_picks_out_the_sort_column() {
        let header = header_line(false, SortField::Memory, true);
        let bold: Vec<&str> = header
            .spans
            .iter()
            .filter(|span| span.style == FIGURE)
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(bold, ["%MEM"]);
        assert_eq!(header.text(), super::header(false));
    }

    #[test]
    fn a_painted_frame_overwrites_rather_than_clears() {
        let header = Line {
            inverse: true,
            ..Line::plain("HEADER")
        };
        let lines = vec![
            Line::plain("short"),
            header,
            Line::plain("x".repeat(10)),
            Line::plain("status"),
        ];
        let mut out = Vec::new();
        paint(&mut out, &lines, 10, false).unwrap();
        let painted = String::from_utf8(out).unwrap();
        assert!(!painted.contains("\x1b[2J"), "{painted:?}");
        assert!(painted.starts_with("\x1b[?2026h\x1b[H"));
        assert!(painted.ends_with("\x1b[J\x1b[?2026l"));
        assert!(painted.contains("short\x1b[K\r\n"));
        assert!(painted.contains("\x1b[0;7mHEADER    \x1b[0m\r\n"));
        // A full-width line gets no erase, which would take its last character.
        assert!(painted.contains("xxxxxxxxxx\r\n"));
    }

    #[test]
    fn k_asks_to_kill_and_the_old_scroll_keys_are_gone() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let key = |code| key_action(KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(key(KeyCode::Char('k')), Some(Action::Kill));
        assert_eq!(key(KeyCode::Char('V')), Some(Action::Tree));
        assert_eq!(key(KeyCode::Char('1')), Some(Action::PerProcessor));
        assert_eq!(key(KeyCode::Char('/')), Some(Action::Filter));
        assert_eq!(key(KeyCode::Char('d')), Some(Action::Delay));
        assert_eq!(key(KeyCode::Char('j')), None);
    }

    #[test]
    fn cpu_time_grows_coarser_to_stay_in_its_column() {
        const SECOND: u64 = 10_000_000;
        let minutes = |m: u64| m * 60 * SECOND;
        assert_eq!(cpu_time_plus(0), "0:00.00");
        assert_eq!(
            cpu_time_plus(minutes(44) + 51 * SECOND + 3_200_000),
            "44:51.32"
        );
        // Firefox after six days: 1423:07.21 is ten characters.
        assert_eq!(
            cpu_time_plus(minutes(1423) + 7 * SECOND + 2_100_000),
            "1423:07"
        );
        assert_eq!(cpu_time_plus(minutes(1_000_000)), "16666,40");
        assert_eq!(cpu_time_plus(minutes(60 * 24_000)), "24000,00");
        // 41666d+16h would be ten characters.
        assert_eq!(cpu_time_plus(minutes(60 * 1_000_000)), "5952w+2d");
        assert!(cpu_time_plus(u64::MAX).len() <= TIME_WIDTH);
    }

    #[test]
    fn extreme_values_keep_the_command_under_its_header() {
        let mut busy = row(1_048_576, 0, 0, "busy.exe", 1600.0);
        busy.cpu_total = u64::MAX / 2;
        busy.memory_share = 100.0;
        busy.committed = 999_999 * 1024 * 1024;
        busy.resident = 999_999 * 1024 * 1024;
        busy.shared_resident = Some(999_999 * 1024 * 1024);
        for show_shared in [false, true] {
            let column = header(show_shared).find("COMMAND").unwrap();
            let line = row_line(&busy, show_shared, 0, &View::new(SortField::Cpu, 3.0)).text();
            assert_eq!(line.find("busy.exe"), Some(column), "{line}");
        }
    }

    #[test]
    fn an_interval_is_a_positive_number_of_seconds() {
        assert_eq!(parse_delay(" 1.5 "), Ok(Some(1.5)));
        assert_eq!(parse_delay(""), Ok(None));
        assert_eq!(parse_delay("0.01"), Ok(Some(SHORTEST_DELAY)));
        assert!(parse_delay("-1").is_err());
        assert!(parse_delay("soon").is_err());
    }
}
