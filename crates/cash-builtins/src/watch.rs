//! `watch`, procps-ng's: run a command every few seconds and show its output full screen.
//!
//! Windows ships nothing like it, and Git for Windows does not carry it either, so the
//! loop every administrator reaches for, `watch -n 1 'ls | wc -l'`, had no answer in a
//! Windows shell. procps-ng 4.0.7's options, keys and exit statuses are followed: the two
//! header lines, `q` and Ctrl-C leaving with 0, `-g` and `-q` leaving with 0 when the
//! visible output changes or stops changing, `-e` freezing on a failure and leaving with
//! the command's status once a key is pressed.
//!
//! The command runs in a copy of this shell, as procps runs it in a child with `sh -c`:
//! functions and aliases are visible here, which a child shell would not have, and
//! nothing it does reaches the caller. `-x` runs the words as one command by cash's rules,
//! a builtin or a program on `PATH`. Standard output and standard error are captured
//! together, as procps joins them.
//!
//! Three things differ, each on purpose. A whole escape sequence is dropped without
//! `-c`, where procps drops only the escape character and shows `[31m`. Each frame is
//! written over the last on the alternate screen, as `top` draws, rather than onto a
//! cleared screen. And standard output must be a terminal: procps draws into a redirected
//! output and then fails reading a key.

use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use cash_core::openfiles::{OpenFile, OpenFiles};
use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Order, Problem, Short};
use clap::Parser;
use unicode_width::UnicodeWidthChar as _;

use crate::top::{FullScreen, RawMode, terminal_size};

/// procps-ng's usage text, which `-h` prints and an option error follows.
const USAGE: &str = "\nUsage:\n watch [options] command\n\nOptions:\n  -b, --beep             beep if command has a non-zero exit\n  -c, --color            interpret ANSI color and style sequences\n  -C, --no-color         do not interpret ANSI color and style sequences\n  -d, --differences[=<permanent>]\n                         highlight changes between updates\n  -e, --errexit          exit if command has a non-zero exit\n  -f, --follow           Follow the output and don't clear screen\n  -g, --chgexit          exit when output from command changes\n  -q, --equexit <cycles>\n                         exit when output from command does not change\n  -n, --interval <secs>  seconds to wait between updates\n  -p, --precise          -n includes command running time\n  -r, --no-rerun         do not rerun program on window resize\n  -s, --shotsdir         directory to store screenshots\n  -t, --no-title         turn off header\n  -w, --no-wrap          turn off line wrapping\n  -x, --exec             pass command to exec instead of \"sh -c\"\n\n -h, --help     display this help and exit\n -v, --version  output version information and exit\n\nFor more details see watch(1).\n";

/// Seconds between runs unless `-n` says otherwise.
const DEFAULT_INTERVAL: f64 = 2.0;
/// The bounds `-n` is kept within, a tenth of a second and 31 days, as procps has them.
const SHORTEST_INTERVAL: f64 = 0.1;
const LONGEST_INTERVAL: f64 = 2_678_400.0;
/// How long the capture is read after the command returned, for a process it left
/// behind that still holds the pipe.
const CAPTURE_GRACE: Duration = Duration::from_millis(200);
/// procps's words on the last line under `-e`.
const ERREXIT_MESSAGE: &str = "command exit with a non-zero status, press a key to exit";
/// The second column of a character two columns wide.
const CONTINUATION: char = '\0';

/// Run a command every few seconds and show its output full screen.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct WatchCommand {
    /// Options and the command, parsed here as getopt parses them.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// `-d`: highlight what changed since the last run, or since the first (`permanent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Differences {
    SinceLast,
    SinceFirst,
}

/// The options, as procps's `watch` takes them.
#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is one of procps's flags, and they are set and read by name"
)]
struct Options {
    beep: bool,
    colour: bool,
    differences: Option<Differences>,
    errexit: bool,
    follow: bool,
    chgexit: bool,
    /// `-q`: leave once the output has not changed for this many runs.
    equexit: Option<u64>,
    interval: f64,
    precise: bool,
    no_rerun: bool,
    shotsdir: Option<String>,
    no_title: bool,
    no_wrap: bool,
    exec: bool,
    help: bool,
    version: bool,
    command: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            beep: false,
            colour: false,
            differences: None,
            errexit: false,
            follow: false,
            chgexit: false,
            equexit: None,
            interval: DEFAULT_INTERVAL,
            precise: false,
            no_rerun: false,
            shotsdir: None,
            no_title: false,
            no_wrap: false,
            exec: false,
            help: false,
            version: false,
            command: Vec::new(),
        }
    }
}

/// Why the options could not be parsed, worded as getopt and procps word it.
#[derive(Debug, PartialEq, Eq)]
enum OptionError {
    Invalid(char),
    /// What `getopt_long` says is wrong with the command line.
    Getopt(Problem),
    BadInterval(String),
    BadCycles(String),
    EnvInterval(String),
}

impl OptionError {
    fn message(&self) -> String {
        match self {
            Self::Invalid(letter) => format!("invalid option -- '{letter}'"),
            Self::Getopt(problem) => problem.to_string(),
            Self::BadInterval(text) => {
                format!("failed to parse argument: '{text}': Invalid argument")
            }
            Self::BadCycles(text) => format!("failed to parse argument: '{text}'"),
            Self::EnvInterval(text) => {
                format!("Could not parse interval from WATCH_INTERVAL: '{text}': Invalid argument")
            }
        }
    }

    /// getopt's errors are followed by the usage; procps's own are not.
    const fn shows_usage(&self) -> bool {
        matches!(self, Self::Invalid(_) | Self::Getopt(_))
    }
}

/// The long options, each known by its letter, which is its short option too.
const LONG_OPTIONS: &[Long<'static, char>] = &[
    Long::new("beep", Arg::No, 'b'),
    Long::new("color", Arg::No, 'c'),
    Long::new("no-color", Arg::No, 'C'),
    Long::new("differences", Arg::Optional, 'd'),
    Long::new("errexit", Arg::No, 'e'),
    Long::new("follow", Arg::No, 'f'),
    Long::new("chgexit", Arg::No, 'g'),
    Long::new("equexit", Arg::Required, 'q'),
    Long::new("interval", Arg::Required, 'n'),
    Long::new("precise", Arg::No, 'p'),
    Long::new("no-rerun", Arg::No, 'r'),
    Long::new("shotsdir", Arg::Required, 's'),
    Long::new("no-title", Arg::No, 't'),
    Long::new("no-wrap", Arg::No, 'w'),
    Long::new("exec", Arg::No, 'x'),
    Long::new("help", Arg::No, 'h'),
    Long::new("version", Arg::No, 'v'),
];

/// Parses the arguments as procps does (`cash-getopt`), with POSIX option order: the
/// first word that is not an option is the command, and everything after it belongs to
/// the command.
fn parse_options(args: &[String], env_interval: Option<&str>) -> Result<Options, OptionError> {
    let mut options = Options::default();
    if let Some(text) = env_interval {
        options.interval =
            parse_interval(text).ok_or_else(|| OptionError::EnvInterval(text.to_owned()))?;
    }
    let shorts: Vec<Short<char>> = LONG_OPTIONS
        .iter()
        .map(|long| Short::new(long.id, long.arg, long.id))
        .collect();
    let getopt = Getopt::new(&shorts, LONG_OPTIONS).order(Order::StopAtOperand);
    for next in getopt.read(args) {
        match next.map_err(OptionError::Getopt)? {
            Item::Option { id: 'd', value, .. } => {
                set_differences(&mut options, value.as_deref());
            }
            Item::Option {
                id: letter @ ('n' | 'q' | 's'),
                value,
                ..
            } => set_valued(&mut options, letter, &value.unwrap_or_default())?,
            Item::Option { id, .. } => set_flag(&mut options, id)?,
            Item::Operand { value, .. } => options.command.push(value),
        }
    }
    Ok(options)
}

const fn set_flag(options: &mut Options, letter: char) -> Result<(), OptionError> {
    match letter {
        'b' => options.beep = true,
        'c' => options.colour = true,
        'C' => options.colour = false,
        'e' => options.errexit = true,
        'f' => options.follow = true,
        'g' => options.chgexit = true,
        'p' => options.precise = true,
        'r' => options.no_rerun = true,
        't' => options.no_title = true,
        'w' => options.no_wrap = true,
        'x' => options.exec = true,
        'h' => options.help = true,
        'v' => options.version = true,
        other => return Err(OptionError::Invalid(other)),
    }
    Ok(())
}

/// `-d` alone highlights against the last run; any argument (`-d1`, `=permanent`) makes
/// the highlights permanent, as procps reads it.
const fn set_differences(options: &mut Options, argument: Option<&str>) {
    options.differences = Some(if argument.is_some() {
        Differences::SinceFirst
    } else {
        Differences::SinceLast
    });
}

fn set_valued(options: &mut Options, letter: char, value: &str) -> Result<(), OptionError> {
    match letter {
        'n' => {
            options.interval =
                parse_interval(value).ok_or_else(|| OptionError::BadInterval(value.to_owned()))?;
        }
        'q' => {
            let cycles: i64 = value
                .trim()
                .parse()
                .map_err(|_| OptionError::BadCycles(value.to_owned()))?;
            options.equexit = Some(u64::try_from(cycles.max(1)).unwrap_or(1));
        }
        's' => options.shotsdir = Some(value.to_owned()),
        other => return Err(OptionError::Invalid(other)),
    }
    Ok(())
}

/// An interval as procps reads one: `.` or `,` as the decimal point, kept between a tenth
/// of a second and 31 days; `None` when it is not a number.
fn parse_interval(text: &str) -> Option<f64> {
    let value: f64 = text.trim().replace(',', ".").parse().ok()?;
    if value.is_nan() {
        return None;
    }
    Some(value.clamp(SHORTEST_INTERVAL, LONGEST_INTERVAL))
}

impl builtins::Command for WatchCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let env_interval = context
            .shell
            .env_str("WATCH_INTERVAL")
            .map(|v| v.into_owned());
        let options = match parse_options(&self.args, env_interval.as_deref()) {
            Ok(options) => options,
            Err(error) => {
                let mut stderr = context.stderr();
                writeln!(stderr, "watch: {}", error.message())?;
                if error.shows_usage() {
                    stderr.write_all(USAGE.as_bytes())?;
                }
                return Ok(ExecutionResult::general_error());
            }
        };
        if options.help {
            context.stdout().write_all(USAGE.as_bytes())?;
            return Ok(ExecutionResult::success());
        }
        if options.version {
            writeln!(context.stdout(), "watch (cash): procps-ng 4.0.7's options")?;
            return Ok(ExecutionResult::success());
        }
        if options.command.is_empty() {
            context.stderr().write_all(USAGE.as_bytes())?;
            return Ok(ExecutionResult::general_error());
        }
        // The shell's standard output, which a redirection or a pipe replaces.
        let terminal = std::io::IsTerminal::is_terminal(&std::io::stdout())
            && context
                .try_fd(OpenFiles::STDOUT_FD)
                .is_some_and(|file| file.is_terminal());
        if !terminal {
            writeln!(context.stderr(), "watch: standard output is not a terminal")?;
            return Ok(ExecutionResult::general_error());
        }

        let mut watcher = Watcher::new(&options);
        let mut screen = FullScreen::enter(context.stdout())?;
        let raw = RawMode::enter();
        let outcome = watcher
            .watch(context.shell, &context.params, screen.out())
            .await;
        drop(raw);
        drop(screen);
        match outcome {
            Ok(Ok(result)) => Ok(result),
            // A failure of watch's own, reported once the screen is back.
            Ok(Err(message)) => {
                writeln!(context.stderr(), "watch: {message}")?;
                Ok(ExecutionResult::general_error())
            }
            Err(error) => Err(error),
        }
    }
}

/// What one run of the command produced.
struct Run {
    output: String,
    status: u8,
    took: Duration,
}

/// Runs the command once with its two output streams captured, and waits for it.
async fn run_command<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    params: &cash_core::ExecutionParameters,
    options: &Options,
    command: &str,
) -> Result<Run, cash_core::Error> {
    let started = Instant::now();
    let (reader, writer) = std::io::pipe()?;
    let (sender, receiver) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || read_all(reader, &sender));

    let writer = OpenFile::from(writer);
    let mut params = params.clone();
    params.set_fd(OpenFiles::STDOUT_FD, writer.clone());
    params.set_fd(OpenFiles::STDERR_FD, writer);
    // `params` goes with the call, so the shell's copies of the pipe close with it.
    let (status, failure) = run_in_shell(shell, params, options, command).await?;

    // Until the pipe closes, or nothing more arrives for a while: a process the command
    // left behind may hold its end open.
    let mut bytes = Vec::new();
    while let Ok(chunk) = receiver.recv_timeout(CAPTURE_GRACE) {
        bytes.extend(chunk);
    }
    if let Some(text) = failure {
        bytes.extend_from_slice(text.as_bytes());
    }
    Ok(Run {
        output: String::from_utf8_lossy(&bytes).into_owned(),
        status,
        took: started.elapsed(),
    })
}

/// The command's exit status, and what to show when it could not be started at all.
async fn run_in_shell<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    params: cash_core::ExecutionParameters,
    options: &Options,
    command: &str,
) -> Result<(u8, Option<String>), cash_core::Error> {
    if options.exec {
        // procps's `-x`: the words are one command, not shell source. procps exits its
        // child with 127 and `argv[0]: strerror` when exec fails.
        return Ok(
            match cash_core::commands::run_for_builtin(shell, params, &options.command).await {
                Ok(result) => (u8::from(result.exit_code), None),
                Err(error) => {
                    let program = options.command.first().map_or("", String::as_str);
                    let reason = crate::xargs::start_failure(&error);
                    (127, Some(format!("{program}: {reason}\n")))
                }
            },
        );
    }

    // procps runs `sh -c COMMAND` in a child; a copy of this shell is that child, with its
    // functions and aliases, and whatever the command changes stays in the copy.
    let mut copy = shell.clone();
    let source = shell.call_stack().current_pos_as_source_info();
    match copy.run_string(command.to_owned(), &source, &params).await {
        Ok(result) => Ok((u8::from(result.exit_code), None)),
        // An interrupt ends the shell watch is a part of, too.
        Err(error) if error.is_silent_interrupt() => Err(error),
        Err(error) => {
            let text = format!("{error}\n");
            Ok((u8::from(error.into_result(&copy).exit_code), Some(text)))
        }
    }
}

/// Reads the pipe to its end, or until the receiver is gone.
fn read_all(mut reader: std::io::PipeReader, sender: &mpsc::Sender<Vec<u8>>) {
    use std::io::Read as _;

    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(count) => {
                if sender.send(buffer[..count].to_vec()).is_err() {
                    return;
                }
            }
        }
    }
}

/// One character cell of the output as laid out on the screen, with the SGR parameters
/// in effect where it was written (empty when plain, or without `-c`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    ch: char,
    style: String,
}

impl Cell {
    const fn blank() -> Self {
        Self {
            ch: ' ',
            style: String::new(),
        }
    }
}

/// Lays the output out as the screen shows it, `width` columns wide.
///
/// Tabs go to the next tab stop and escape sequences are dropped, SGR ones remembered
/// as each cell's style when `colour`; other control characters, carriage returns among
/// them, are dropped as procps drops them. A line wider than the screen continues on the
/// next line, or is cut at the edge without `wrap`.
fn layout(text: &str, width: usize, colour: bool, wrap: bool) -> Vec<Vec<Cell>> {
    let mut sheet = Sheet {
        width: width.max(1),
        colour,
        wrap,
        rows: Vec::new(),
        row: Vec::new(),
        column: 0,
        style: String::new(),
        discarding: false,
    };
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\n' => sheet.newline(),
            '\x1b' => sheet.escape(&mut chars),
            '\t' => sheet.tab(),
            ch if ch.is_control() => {}
            ch => sheet.put(ch),
        }
    }
    if !sheet.row.is_empty() {
        sheet.rows.push(sheet.row);
    }
    sheet.rows
}

/// The rows being laid out.
struct Sheet {
    width: usize,
    colour: bool,
    wrap: bool,
    rows: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    column: usize,
    style: String,
    /// The rest of a line cut at the edge.
    discarding: bool,
}

impl Sheet {
    fn newline(&mut self) {
        self.rows.push(std::mem::take(&mut self.row));
        self.column = 0;
        self.discarding = false;
    }

    fn put(&mut self, ch: char) {
        if self.discarding {
            return;
        }
        let columns = ch.width().unwrap_or(0);
        if columns == 0 {
            return;
        }
        if self.column + columns > self.width {
            if !self.wrap {
                self.discarding = true;
                return;
            }
            if !self.row.is_empty() {
                self.newline();
            }
            if columns > self.width {
                return;
            }
        }
        self.row.push(Cell {
            ch,
            style: self.style.clone(),
        });
        if columns == 2 {
            self.row.push(Cell {
                ch: CONTINUATION,
                style: self.style.clone(),
            });
        }
        self.column += columns;
    }

    /// To the next multiple of eight columns, as curses expands a tab.
    fn tab(&mut self) {
        if self.discarding {
            return;
        }
        let stop = (self.column / 8 + 1) * 8;
        while self.column < stop && self.column < self.width {
            self.row.push(Cell {
                ch: ' ',
                style: self.style.clone(),
            });
            self.column += 1;
        }
    }

    /// Swallows the sequence an escape starts, keeping an SGR one's parameters as the
    /// style when colour is on.
    fn escape(&mut self, chars: &mut std::str::Chars<'_>) {
        match chars.next() {
            Some('[') => {
                let mut parameters = String::new();
                for ch in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&ch) {
                        if ch == 'm' && self.colour {
                            self.set_style(&parameters);
                        }
                        break;
                    }
                    parameters.push(ch);
                }
            }
            Some(']') => {
                // An operating system command ends with BEL or with ESC \.
                let mut escaped = false;
                for ch in chars.by_ref() {
                    if ch == '\x07' || (escaped && ch == '\\') {
                        break;
                    }
                    escaped = ch == '\x1b';
                }
            }
            Some('(' | ')' | '*' | '+' | '#' | '%') => {
                chars.next();
            }
            _ => {}
        }
    }

    /// `ESC [ 1;31 m` adds to the style; `ESC [ m` and a `0` take it back to plain.
    fn set_style(&mut self, parameters: &str) {
        if parameters.is_empty() {
            self.style.clear();
            return;
        }
        for parameter in parameters.split(';') {
            if parameter.is_empty() || parameter == "0" {
                self.style.clear();
            } else {
                if !self.style.is_empty() {
                    self.style.push(';');
                }
                self.style.push_str(parameter);
            }
        }
    }
}

/// The screen's body: `rows` of `width` cells, blank where the output has none.
fn fit(lines: &[Vec<Cell>], rows: usize, width: usize) -> Vec<Vec<Cell>> {
    (0..rows)
        .map(|index| {
            let mut row = lines.get(index).cloned().unwrap_or_default();
            row.truncate(width);
            row.resize(width, Cell::blank());
            row
        })
        .collect()
}

/// Which cells show another character than before.
fn changes(before: &[Vec<Cell>], after: &[Vec<Cell>]) -> Vec<Vec<bool>> {
    after
        .iter()
        .enumerate()
        .map(|(y, row)| {
            row.iter()
                .enumerate()
                .map(|(x, cell)| {
                    before
                        .get(y)
                        .and_then(|row| row.get(x))
                        .is_none_or(|old| old.ch != cell.ch)
                })
                .collect()
        })
        .collect()
}

/// The header's first line: `Every 2.0s: command` at the left, `host: date` at the right,
/// clipped to fit as procps clips them: the right part first, then the left, then the
/// command cut short with `...`, and nothing at all when even the right part does not fit.
fn title_line(interval: f64, command: &str, host: &str, now: &str, width: usize) -> String {
    let left = format!("Every {interval:.1}s: ");
    let right = format!("{host}: {now}");
    let right_width = right.chars().count();
    if width < right_width {
        return String::new();
    }
    let mut line = String::new();
    if let Some(available) = width.checked_sub(left.chars().count() + right_width) {
        line.push_str(&left);
        if available > command.chars().count() {
            line.push_str(command);
        } else if available > 3 {
            line.extend(command.chars().take(available - 4));
            line.push_str("...");
        }
    }
    let used = line.chars().count();
    line.extend(std::iter::repeat_n(
        ' ',
        (width - right_width).saturating_sub(used),
    ));
    line.push_str(&right);
    line
}

/// The header's second line: how long the run took and its status, at the right.
fn status_line(took: Duration, status: u8, width: usize) -> String {
    let seconds = took.as_secs_f64();
    let text = if seconds > 86_400.0 {
        format!("in >1 day ({status})")
    } else if seconds < 0.001 {
        format!("in <0.001s ({status})")
    } else {
        format!("in {seconds:.3}s ({status})")
    };
    let length = text.chars().count();
    if length > width {
        return String::new();
    }
    format!("{}{text}", " ".repeat(width - length))
}

/// Writes one frame over the screen in one piece, inside synchronized output: the header
/// lines, the body with its marked cells in reverse video, and `message` on the last line.
fn paint(
    out: &mut impl std::io::Write,
    header: &[String],
    body: &[Vec<Cell>],
    marks: Option<&[Vec<bool>]>,
    size: (usize, usize),
) -> std::io::Result<()> {
    let (width, height) = size;
    let mut frame = String::from("\x1b[?2026h\x1b[0m");
    let mut row = 0;
    for line in header {
        if row >= height {
            break;
        }
        let _ = write!(frame, "\x1b[{};1H{line}", row + 1);
        if line.chars().count() < width {
            frame.push_str("\x1b[K");
        }
        row += 1;
    }
    for (index, cells) in body.iter().enumerate() {
        if row >= height {
            break;
        }
        // The last line stops short of the edge: a cell written there leaves the cursor
        // waiting to wrap, and the next erase would take it.
        let room = if row + 1 == height {
            width.saturating_sub(1)
        } else {
            width
        };
        let marked = marks.and_then(|marks| marks.get(index).map(Vec::as_slice));
        render_row(&mut frame, row, cells, marked, room);
        row += 1;
    }
    for blank in row..height {
        let _ = write!(frame, "\x1b[{};1H\x1b[K", blank + 1);
    }
    frame.push_str("\x1b[?2026l");
    out.write_all(frame.as_bytes())?;
    out.flush()
}

/// One body row: runs of cells in one style, marked ones in reverse video.
fn render_row(frame: &mut String, row: usize, cells: &[Cell], marks: Option<&[bool]>, room: usize) {
    let _ = write!(frame, "\x1b[{};1H", row + 1);
    let mut current: Option<(&str, bool)> = None;
    let mut column = 0;
    for (x, cell) in cells.iter().enumerate() {
        if cell.ch == CONTINUATION {
            continue;
        }
        let columns = cell.ch.width().unwrap_or(1);
        if column + columns > room {
            break;
        }
        let marked = marks.is_some_and(|marks| {
            (0..columns).any(|offset| marks.get(x + offset).copied().unwrap_or(false))
        });
        let state = (cell.style.as_str(), marked);
        if current != Some(state) {
            frame.push_str("\x1b[0m");
            if !cell.style.is_empty() {
                let _ = write!(frame, "\x1b[{}m", cell.style);
            }
            if marked {
                frame.push_str("\x1b[7m");
            }
            current = Some(state);
        }
        frame.push(cell.ch);
        column += columns;
    }
    frame.push_str("\x1b[0m");
    if column < room {
        frame.push_str("\x1b[K");
    }
}

/// What ended a wait between runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Timeout,
    Quit,
    RunNow,
    Resize,
    Screenshot,
}

/// Waits for `deadline`, a key or a resize, whichever comes first. Keys: `q` and Ctrl-C
/// leave, Space runs the command now, `s` takes a screenshot, as in procps.
fn wait_for(deadline: Instant) -> Outcome {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, poll, read};

    loop {
        // A Ctrl-C the console made an event of, where it was not a key.
        if cash_win32::console::take_interrupt() {
            return Outcome::Quit;
        }
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return Outcome::Timeout;
        };
        match poll(left.min(Duration::from_millis(250))) {
            Ok(false) => continue,
            // Without a console to read, wait out the interval rather than spin.
            Err(_) => {
                std::thread::sleep(left);
                return Outcome::Timeout;
            }
            Ok(true) => {}
        }
        match read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Outcome::Quit;
                }
                KeyCode::Char('q') => return Outcome::Quit,
                KeyCode::Char(' ') => return Outcome::RunNow,
                KeyCode::Char('s') => return Outcome::Screenshot,
                _ => {}
            },
            Ok(Event::Resize(..)) => return Outcome::Resize,
            _ => {}
        }
    }
}

/// Drops the keys typed so far and waits for the next one, as procps does under `-e`.
fn wait_for_a_key() {
    use crossterm::event::{Event, KeyEventKind, poll, read};

    while matches!(poll(Duration::ZERO), Ok(true)) {
        if read().is_err() {
            return;
        }
    }
    loop {
        if cash_win32::console::take_interrupt() {
            return;
        }
        match poll(Duration::from_millis(250)) {
            Ok(false) => continue,
            Err(_) => return,
            Ok(true) => {}
        }
        match read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => return,
            Ok(_) => {}
            Err(_) => return,
        }
    }
}

/// The screen between runs: the last body shown, and what the comparisons need.
struct Watcher<'a> {
    options: &'a Options,
    /// The command as the header shows it, and as the shell runs it.
    command: String,
    host: String,
    /// The body of the last frame, as compared against the next; `None` for the first
    /// frame and after a resize, which procps also treats as a first screen.
    previous: Option<Vec<Vec<Cell>>>,
    /// `-d=permanent`: every cell that has changed since the first frame.
    sticky: Vec<Vec<bool>>,
    /// `-f`: every line so far, the last ones shown.
    history: Vec<Vec<Cell>>,
    /// `-q`: runs in a row with the same output, counting as procps counts.
    cycles: u64,
    /// The last screenshot's second, and how many were taken in it.
    last_shot: (String, u32),
}

impl<'a> Watcher<'a> {
    fn new(options: &'a Options) -> Self {
        Self {
            options,
            command: options.command.join(" "),
            host: cash_win32::process::computer_name().unwrap_or_default(),
            previous: None,
            sticky: Vec::new(),
            history: Vec::new(),
            cycles: 1,
            last_shot: (String::new(), 0),
        }
    }

    /// Runs, draws and waits until something ends it. The outer error is the shell's;
    /// the inner one is watch's own, to report once the screen is back.
    async fn watch<SE: cash_core::ShellExtensions>(
        &mut self,
        shell: &cash_core::Shell<SE>,
        params: &cash_core::ExecutionParameters,
        out: &mut impl std::io::Write,
    ) -> Result<Result<ExecutionResult, String>, cash_core::Error> {
        let interval = Duration::from_secs_f64(self.options.interval);
        let mut rerun = true;
        let mut last: Option<Run> = None;
        let mut deadline = Instant::now();
        let mut drawn_for = (0, 0);
        loop {
            if rerun {
                let started = Instant::now();
                let run = run_command(shell, params, self.options, &self.command).await?;
                drawn_for = terminal_size();
                let changed = self.show(out, &run, drawn_for, true)?;
                if let Some(status) = self.after_run(out, &run, changed)? {
                    return Ok(Ok(ExecutionResult::new(status)));
                }
                let from = if self.options.precise {
                    started
                } else {
                    Instant::now()
                };
                deadline = from + interval;
                last = Some(run);
                rerun = false;
            }
            match wait_for(deadline) {
                Outcome::Quit => return Ok(Ok(ExecutionResult::success())),
                Outcome::Timeout | Outcome::RunNow => rerun = true,
                // The console reports a size event as watch starts, and for a change of
                // buffer that leaves the window as it was: the frame fits already.
                Outcome::Resize if terminal_size() == drawn_for => {}
                Outcome::Resize if self.options.no_rerun => {
                    // The last output again, fitted to the new size; the differences
                    // start over, as procps's do after a resize.
                    self.previous = None;
                    drawn_for = terminal_size();
                    if let Some(run) = &last {
                        self.show(out, run, drawn_for, false)?;
                    }
                }
                Outcome::Resize => {
                    self.previous = None;
                    rerun = true;
                }
                Outcome::Screenshot => {
                    if let Err(message) = self.screenshot(shell) {
                        return Ok(Err(message));
                    }
                }
            }
        }
    }

    /// Draws one run's output, and says whether the visible part differs from the last
    /// frame's; `None` when there was no last frame to compare with.
    fn show(
        &mut self,
        out: &mut impl std::io::Write,
        run: &Run,
        size: (usize, usize),
        append: bool,
    ) -> std::io::Result<Option<bool>> {
        let (width, height) = size;
        let header_rows = if self.options.no_title { 0 } else { 2 };
        let rows = height.saturating_sub(header_rows);
        let lines = layout(
            &run.output,
            width,
            self.options.colour,
            !self.options.no_wrap,
        );
        let visible = if self.options.follow {
            if append {
                self.history.extend(lines);
            }
            let start = self.history.len().saturating_sub(rows);
            fit(self.history.get(start..).unwrap_or_default(), rows, width)
        } else {
            fit(&lines, rows, width)
        };

        let changed = self
            .previous
            .as_deref()
            .map(|before| changes(before, &visible));
        let screen_changed = changed
            .as_ref()
            .map(|marks| marks.iter().flatten().any(|&changed| changed));
        let marks = match (self.options.differences, changed) {
            (None, _) | (_, None) => None,
            (Some(Differences::SinceLast), Some(marks)) => Some(marks),
            (Some(Differences::SinceFirst), Some(marks)) => {
                if self.sticky.len() != marks.len()
                    || self.sticky.first().map(Vec::len) != marks.first().map(Vec::len)
                {
                    self.sticky = marks;
                } else {
                    for (kept, fresh) in self.sticky.iter_mut().zip(&marks) {
                        for (kept, fresh) in kept.iter_mut().zip(fresh) {
                            *kept |= fresh;
                        }
                    }
                }
                Some(self.sticky.clone())
            }
        };

        let header = if self.options.no_title {
            Vec::new()
        } else {
            let now = chrono::Local::now()
                .format("%a %b %e %H:%M:%S %Y")
                .to_string();
            vec![
                title_line(
                    self.options.interval,
                    &self.command,
                    &self.host,
                    &now,
                    width,
                ),
                status_line(run.took, run.status, width),
            ]
        };
        paint(out, &header, &visible, marks.as_deref(), size)?;
        self.previous = Some(visible);
        Ok(screen_changed)
    }

    /// What a run's status and a change in the output decide: `Some(status)` ends watch.
    fn after_run(
        &mut self,
        out: &mut impl std::io::Write,
        run: &Run,
        changed: Option<bool>,
    ) -> std::io::Result<Option<u8>> {
        if run.status != 0 {
            if self.options.beep {
                out.write_all(b"\x07")?;
                out.flush()?;
            }
            if self.options.errexit {
                let (width, height) = terminal_size();
                let message: String = ERREXIT_MESSAGE
                    .chars()
                    .take(width.saturating_sub(1))
                    .collect();
                write!(out, "\x1b[{height};1H\x1b[0m{message}\x1b[K")?;
                out.flush()?;
                wait_for_a_key();
                return Ok(Some(run.status));
            }
        }
        // The first screen, and the first after a resize, decide nothing.
        let Some(changed) = changed else {
            return Ok(None);
        };
        if self.options.chgexit && changed {
            return Ok(Some(0));
        }
        if let Some(cycles) = self.options.equexit {
            if changed {
                self.cycles = 1;
            } else if self.cycles >= cycles {
                return Ok(Some(0));
            } else {
                self.cycles += 1;
            }
        }
        Ok(None)
    }

    /// `s`: the body as text into `watch_YYYYmmdd-HHMMSS` in the screenshots folder (`-s`)
    /// or the working folder, numbered when the second already has one.
    fn screenshot<SE: cash_core::ShellExtensions>(
        &mut self,
        shell: &cash_core::Shell<SE>,
    ) -> Result<(), String> {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let name = if self.last_shot.0 == stamp {
            self.last_shot.1 += 1;
            format!("watch_{stamp}-{:03}", self.last_shot.1)
        } else {
            self.last_shot = (stamp.clone(), 0);
            format!("watch_{stamp}")
        };
        let path = match self.options.shotsdir.as_deref() {
            Some(dir) if !dir.is_empty() => shell.absolute_path(dir).join(name),
            _ => shell.absolute_path(&name),
        };
        let mut text = String::new();
        for row in self.previous.as_deref().unwrap_or_default() {
            let line: String = row
                .iter()
                .filter(|cell| cell.ch != CONTINUATION)
                .map(|cell| cell.ch)
                .collect();
            text.push_str(line.trim_end());
            text.push('\n');
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|error| {
                format!(
                    "open({}): {}",
                    path.display(),
                    cash_core::error::os_error_text(&error)
                )
            })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "tests assert loudly on failure, and compare the exact intervals they gave"
)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, OptionError> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        parse_options(&args, None)
    }

    fn text(rows: &[Vec<Cell>]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                row.iter()
                    .filter(|cell| cell.ch != CONTINUATION)
                    .map(|cell| cell.ch)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_first_word_that_is_not_an_option_starts_the_command() {
        let options = parse(&["-n", "0.5", "-d", "ls", "-l", "-n"]).unwrap();
        assert_eq!(options.interval, 0.5);
        assert_eq!(options.differences, Some(Differences::SinceLast));
        assert_eq!(options.command, ["ls", "-l", "-n"]);

        let options = parse(&["--", "-x"]).unwrap();
        assert_eq!(options.command, ["-x"]);
        assert!(!options.exec);
    }

    #[test]
    fn flags_cluster_and_values_attach_or_follow() {
        let options = parse(&["-betx", "-n1", "-q", "3", "cmd"]).unwrap();
        assert!(options.beep && options.errexit && options.no_title && options.exec);
        assert_eq!(options.interval, 1.0);
        assert_eq!(options.equexit, Some(3));
        assert_eq!(options.command, ["cmd"]);

        let options = parse(&["-d1", "cmd"]).unwrap();
        assert_eq!(options.differences, Some(Differences::SinceFirst));
        let options = parse(&["--differences=permanent", "cmd"]).unwrap();
        assert_eq!(options.differences, Some(Differences::SinceFirst));
        let options = parse(&["--differences", "cmd"]).unwrap();
        assert_eq!(options.differences, Some(Differences::SinceLast));
    }

    #[test]
    fn long_options_take_their_argument_either_way_and_a_prefix() {
        let options = parse(&["--interval=0,2", "--equexit", "2", "--no-ti", "cmd"]).unwrap();
        assert_eq!(options.interval, 0.2);
        assert_eq!(options.equexit, Some(2));
        assert!(options.no_title);
        let said = |args: &[&str]| parse(args).unwrap_err().message();
        assert_eq!(
            said(&["--no-", "cmd"]),
            "option '--no-' is ambiguous; possibilities: '--no-color' '--no-rerun' \
             '--no-title' '--no-wrap'"
        );
        assert_eq!(
            said(&["--beep=1", "cmd"]),
            "option '--beep' doesn't allow an argument"
        );
        assert_eq!(
            said(&["--interval"]),
            "option '--interval' requires an argument"
        );
        assert_eq!(said(&["--bogus", "cmd"]), "unrecognized option '--bogus'");
    }

    #[test]
    fn errors_are_worded_as_getopt_and_procps_word_them() {
        let error = parse(&["-z", "cmd"]).unwrap_err();
        assert_eq!(error.message(), "invalid option -- 'z'");
        assert!(error.shows_usage());

        let error = parse(&["-n"]).unwrap_err();
        assert_eq!(error.message(), "option requires an argument -- 'n'");

        let error = parse(&["-n", "abc", "cmd"]).unwrap_err();
        assert_eq!(
            error.message(),
            "failed to parse argument: 'abc': Invalid argument"
        );
        assert!(!error.shows_usage());

        let error = parse(&["-q", "abc", "cmd"]).unwrap_err();
        assert_eq!(error.message(), "failed to parse argument: 'abc'");

        let args = vec![String::from("cmd")];
        let error = parse_options(&args, Some("soon")).unwrap_err();
        assert_eq!(
            error.message(),
            "Could not parse interval from WATCH_INTERVAL: 'soon': Invalid argument"
        );
    }

    #[test]
    fn the_interval_is_kept_between_a_tenth_of_a_second_and_a_month() {
        assert_eq!(parse_interval("0.05"), Some(0.1));
        assert_eq!(parse_interval("-3"), Some(0.1));
        assert_eq!(parse_interval("1,5"), Some(1.5));
        assert_eq!(parse_interval(" 2 "), Some(2.0));
        assert_eq!(parse_interval("1e9"), Some(LONGEST_INTERVAL));
        assert_eq!(parse_interval("nan"), None);
        assert_eq!(parse_interval("1s"), None);
        let args = vec![String::from("cmd")];
        assert_eq!(parse_options(&args, Some("5")).unwrap().interval, 5.0);
        assert_eq!(parse(&["-q", "0", "cmd"]).unwrap().equexit, Some(1));
    }

    #[test]
    fn the_title_line_is_clipped_as_procps_clips_it() {
        const RIGHT: &str = "host: Tue Oct  6 10:41:06 2026";
        let title = |width| title_line(2.0, "echo hi", "host", "Tue Oct  6 10:41:06 2026", width);
        // Everything fits: the command at the left, the host and time at the right.
        assert_eq!(
            title(60),
            format!("Every 2.0s: echo hi{}{RIGHT}", " ".repeat(11))
        );
        // Room for the header but not the whole command: cut short with dots.
        assert_eq!(title(48), format!("Every 2.0s: ec... {RIGHT}"));
        assert_eq!(title(46), format!("Every 2.0s: ... {RIGHT}"));
        // Room for the right part only.
        assert_eq!(title(40), format!("{}{RIGHT}", " ".repeat(10)));
        // Not even that.
        assert_eq!(title(20), "");
    }

    #[test]
    fn the_status_line_is_right_aligned() {
        assert_eq!(
            status_line(Duration::from_millis(4), 0, 20),
            "       in 0.004s (0)"
        );
        assert_eq!(
            status_line(Duration::from_micros(300), 1, 16),
            "  in <0.001s (1)"
        );
        assert_eq!(
            status_line(Duration::from_secs(100_000), 127, 20),
            "     in >1 day (127)"
        );
        assert_eq!(status_line(Duration::from_millis(4), 0, 5), "");
    }

    #[test]
    fn escape_sequences_are_dropped_and_remembered_as_styles_with_colour() {
        let output = "\x1b[31mred\x1b[0m plain \x1b[1mbold\x1b[m\n\x1b]0;title\x07x\x1b(By\n";
        let plain = layout(output, 80, false, true);
        assert_eq!(text(&plain), ["red plain bold", "xy"]);
        assert!(plain.iter().flatten().all(|cell| cell.style.is_empty()));

        let coloured = layout(output, 80, true, true);
        assert_eq!(text(&coloured), ["red plain bold", "xy"]);
        let styles: Vec<&str> = coloured[0].iter().map(|cell| cell.style.as_str()).collect();
        assert_eq!(styles[0], "31");
        assert_eq!(styles[3], "");
        assert_eq!(styles[10], "1");
        assert_eq!(coloured[1][0].style, "");
    }

    #[test]
    fn tabs_stop_every_eight_columns_and_other_controls_vanish() {
        let rows = layout("a\tb\r\nx\x08y\x7fz\n", 80, false, true);
        assert_eq!(text(&rows), ["a       b", "xyz"]);
    }

    #[test]
    fn a_long_line_wraps_or_is_cut_at_the_edge() {
        let rows = layout("0123456789ABCDEF\nshort\n", 10, false, true);
        assert_eq!(text(&rows), ["0123456789", "ABCDEF", "short"]);
        let rows = layout("0123456789ABCDEF\nshort\n", 10, false, false);
        assert_eq!(text(&rows), ["0123456789", "short"]);
        // A wide character takes two cells and does not straddle the edge.
        let rows = layout("abc日本", 4, false, true);
        assert_eq!(text(&rows), ["abc", "日本"]);
        assert_eq!(rows[1].len(), 4);
        assert_eq!(rows[1][1].ch, CONTINUATION);
    }

    #[test]
    fn the_body_is_fitted_and_its_changes_found_cell_by_cell() {
        let before = fit(&layout("abc\n", 80, false, true), 2, 5);
        let after = fit(&layout("axc\nd", 80, false, true), 2, 5);
        assert_eq!(before.len(), 2);
        assert_eq!(before[0].len(), 5);
        let marks = changes(&before, &after);
        assert_eq!(marks[0], [false, true, false, false, false]);
        assert_eq!(marks[1], [true, false, false, false, false]);
    }

    #[test]
    fn a_frame_marks_changed_cells_in_reverse_video() {
        let body = fit(&layout("ab", 80, true, true), 1, 4);
        let marks = vec![vec![false, true, false, false]];
        let mut out = Vec::new();
        paint(
            &mut out,
            &[String::from("Every 2.0s: x")],
            &body,
            Some(&marks),
            (4, 3),
        )
        .unwrap();
        let frame = String::from_utf8(out).unwrap();
        assert!(frame.starts_with("\x1b[?2026h"), "{frame:?}");
        assert!(frame.contains("\x1b[1;1HEvery 2.0s: x"), "{frame:?}");
        assert!(
            frame.contains("\x1b[2;1H\x1b[0ma\x1b[0m\x1b[7mb\x1b[0m "),
            "{frame:?}"
        );
        assert!(frame.contains("\x1b[3;1H\x1b[K"), "{frame:?}");
        assert!(frame.ends_with("\x1b[?2026l"), "{frame:?}");
    }
}
