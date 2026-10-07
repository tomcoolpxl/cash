//! `gzip`, `gunzip` and `zcat`: GNU gzip 1.14's interface, messages and exit codes on
//! the pure-Rust deflate of `miniz_oxide`, through `flate2`.
//!
//! Checked against GNU gzip 1.14 (`crates/cash/tests/oracle/gzip_cases.sh`): every
//! message and status, the decompressed bytes, the ten-byte header and the eight-byte
//! trailer of what is compressed, `-l`'s table. The deflate stream between header and
//! trailer is `miniz_oxide`'s, not zlib's: a `.gz` cash makes is not byte for byte GNU's
//! at the same level, decompresses to the same bytes everywhere, and is of a size
//! within a few percent of GNU's (the page says how close).
//!
//! On a clean Windows machine there is no `gzip`, `gunzip` or `zcat` at all, and
//! `tar.exe` reads archives, not a bare `.gz`.
//!
//! Where cash differs, on purpose:
//! - Windows has no mode bits but read-only, which is carried over; no symbolic-link
//!   refusal. Hard links are counted, since cash's own tool links make them common.
//! - The header's OS byte says Unix (3), as Git for Windows' gzip writes it, so a `.gz`
//!   made in cash is the same bytes as one made in Git Bash.
//! - compress (`.Z`), pack, lzh and zip input is refused by name rather than
//!   decompressed: cash carries deflate only, and `tar.exe` reads a `.tar.Z`.
//! - `-r` reads a folder in name order; GNU gzip takes the file system's order.
//! - A stored name is taken without its folders on either separator, `/` or `\`.
//! - The `GZIP` environment variable is read as GNU gzip 1.14 reads it: a compression
//!   level in it is taken, anything else in it is ignored, and nothing is said.
//! - A file replaced in place is written beside it under a temporary name and renamed
//!   over it, so an interrupted run leaves no half-written file.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cash_archive::codec::gzip::{
    Input, Member, Start, Trouble, deflate_parallel, header, inflate_member, read_start,
};
use cash_core::openfiles::{OpenFile, OpenFiles};
use cash_core::{ExecutionResult, ShellFd, builtins};
use cash_getopt::{Arg, Getopt, Item, Long};
use cash_win32::unix::Replacement;
use clap::Parser;

use crate::compress::strerror;

/// The name every message begins with, whichever of the three names ran.
const PROGRAM: &str = "gzip";
/// The longest `-S` suffix GNU gzip takes.
const MAX_SUFFIX: usize = 30;
/// The suffixes decompression recognises after the `-S` one, in GNU's order.
const KNOWN_SUFFIXES: [&str; 7] = [".gz", ".z", ".taz", ".tgz", "-gz", "-z", "_z"];
/// The suffixes `gunzip foo` tries after the `-S` one when `foo` is not there: fewer
/// than it recognises, as GNU gzip has it.
const SEARCHED_SUFFIXES: [&str; 4] = [".gz", ".z", "-z", ".Z"];
/// The `-l` table's column width: the digits of the largest 64-bit file size.
const SIZE_WIDTH: usize = 19;

/// Compress or uncompress files, with GNU gzip's options.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct GzipCommand {
    /// Options and files, parsed here as `getopt_long` parses them.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Uncompress files: `gzip -d`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct GunzipCommand {
    /// Options and files, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Uncompress files to standard output: `gzip -dc`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ZcatCommand {
    /// Options and files, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

macro_rules! command {
    ($type:ty, $name:literal, $preset:expr) => {
        impl builtins::Command for $type {
            type Error = cash_core::Error;

            fn new<I>(args: I) -> Result<Self, clap::Error>
            where
                I: IntoIterator<Item = String>,
            {
                Ok(Self {
                    args: args.into_iter().skip(1).collect(),
                })
            }

            async fn execute<SE: cash_core::ShellExtensions>(
                &self,
                context: cash_core::ExecutionContext<'_, SE>,
            ) -> Result<ExecutionResult, Self::Error> {
                run($name, $preset, &self.args, &context)
            }
        }
    };
}

command!(GzipCommand, "gzip", Preset::Compress);
command!(GunzipCommand, "gunzip", Preset::Decompress);
command!(ZcatCommand, "zcat", Preset::ToStdout);

/// What the invoked name sets before the options are read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Preset {
    /// `gzip`.
    Compress,
    /// `gunzip`: `-d`.
    Decompress,
    /// `zcat`: `-dc`.
    ToStdout,
}

/// GNU gzip's long options in its table's order, which the ambiguity message follows,
/// each known by the short option it stands for.
const LONG_OPTIONS: &[Long<'static, char>] = &[
    Long::new("ascii", Arg::No, 'a'),
    Long::new("to-stdout", Arg::No, 'c'),
    Long::new("stdout", Arg::No, 'c'),
    Long::new("decompress", Arg::No, 'd'),
    Long::new("uncompress", Arg::No, 'd'),
    Long::new("force", Arg::No, 'f'),
    Long::new("help", Arg::No, 'h'),
    Long::new("keep", Arg::No, 'k'),
    Long::new("list", Arg::No, 'l'),
    Long::new("license", Arg::No, 'L'),
    Long::new("no-name", Arg::No, 'n'),
    Long::new("name", Arg::No, 'N'),
    Long::new("quiet", Arg::No, 'q'),
    Long::new("silent", Arg::No, 'q'),
    Long::new("synchronous", Arg::No, 'Y'),
    Long::new("recursive", Arg::No, 'r'),
    Long::new("suffix", Arg::Required, 'S'),
    Long::new("test", Arg::No, 't'),
    Long::new("verbose", Arg::No, 'v'),
    Long::new("version", Arg::No, 'V'),
    Long::new("fast", Arg::No, '1'),
    Long::new("best", Arg::No, '9'),
    Long::new("lzw", Arg::No, 'Z'),
    Long::new("bits", Arg::Required, 'b'),
    Long::new("rsyncable", Arg::No, 'R'),
];

/// GNU gzip's short options; a colon follows one that takes a value.
const SHORT_OPTIONS: &str = "ab:cdfhHklLmMnNqrS:tvVZ123456789";

/// The options, once read.
#[derive(Debug, Clone)]
struct Options {
    to_stdout: bool,
    decompress: bool,
    force: bool,
    keep: bool,
    list: bool,
    test: bool,
    /// Neither store nor restore the original name.
    no_name: bool,
    /// Neither store nor restore the original time.
    no_time: bool,
    quiet: bool,
    recursive: bool,
    synchronous: bool,
    verbose: bool,
    level: u32,
    suffix: String,
    /// Said before anything is done: `--ascii` is ignored.
    notes: Vec<String>,
    files: Vec<String>,
}

/// What the command line asks for.
#[derive(Debug)]
enum Parsed {
    Run(Options),
    Help,
    Version,
    License,
    /// Lines for standard error, status 1.
    Fail(String),
}

/// The options while they are being read: `-n`/`-N` and `-m`/`-M` default by mode.
struct Reading {
    options: Options,
    no_name: Option<bool>,
    no_time: Option<bool>,
}

impl Reading {
    fn apply(&mut self, id: char, value: Option<String>) -> Result<(), Parsed> {
        let options = &mut self.options;
        match id {
            'a' => options
                .notes
                .push(format!("{PROGRAM}: option --ascii ignored on this system")),
            // `-b` sets the bits of `-Z`, which is not carried; the operand is checked.
            'b' => {
                if value.is_some_and(|v| v.trim().parse::<i64>().is_err()) {
                    return Err(usage_failure("-b operand is not an integer"));
                }
            }
            'c' => options.to_stdout = true,
            'd' => options.decompress = true,
            'f' => options.force = true,
            'h' | 'H' => return Err(Parsed::Help),
            'k' => options.keep = true,
            'l' => options.list = true,
            'L' => return Err(Parsed::License),
            'm' => self.no_time = Some(true),
            'M' => self.no_time = Some(false),
            'n' => {
                self.no_name = Some(true);
                self.no_time = Some(true);
            }
            'N' => {
                self.no_name = Some(false);
                self.no_time = Some(false);
            }
            'q' => {
                options.quiet = true;
                options.verbose = false;
            }
            'r' => options.recursive = true,
            'R' => {}
            'S' => options.suffix = value.unwrap_or_default(),
            'Y' => options.synchronous = true,
            't' => options.test = true,
            'v' => {
                options.verbose = true;
                options.quiet = false;
            }
            'V' => return Err(Parsed::Version),
            'Z' => return Err(usage_failure("-Z not supported in this version")),
            '1'..='9' => options.level = id.to_digit(10).unwrap_or(6),
            _ => return Err(usage_failure(&format!("invalid option -- '{id}'"))),
        }
        Ok(())
    }
}

/// A command-line error as GNU gzip reports one: the message, then the hint.
fn usage_failure(message: &str) -> Parsed {
    Parsed::Fail(format!(
        "{PROGRAM}: {message}\nTry `{PROGRAM} --help' for more information."
    ))
}

/// The compression level `GZIP` names, as GNU gzip 1.14 reads the variable: its words
/// are read as options, `-1` to `-9`, `--fast` and `--best` set the level, the last
/// one winning, `--` ends the reading, and every other option or word is passed over
/// without a word.
fn level_from_environment(gzip: &str) -> Option<u32> {
    let shorts = cash_getopt::optstring(SHORT_OPTIONS);
    let getopt = Getopt::new(&shorts, LONG_OPTIONS);
    let mut level = None;
    let mut words = gzip.split_whitespace();
    while let Some(word) = words.next() {
        if word == "--" {
            break;
        }
        if let Some(text) = word.strip_prefix("--") {
            let (name, inline) = text
                .split_once('=')
                .map_or((text, None), |(n, v)| (n, Some(v)));
            if let Ok(found) = getopt.long(name) {
                if let Some(digit) = found.id.to_digit(10) {
                    level = Some(digit);
                }
                if found.arg == Arg::Required && inline.is_none() {
                    words.next();
                }
            }
            continue;
        }
        let Some(cluster) = word.strip_prefix('-').filter(|c| !c.is_empty()) else {
            continue;
        };
        for (at, c) in cluster.char_indices() {
            if let Some(digit) = c.to_digit(10).filter(|d| (1..=9).contains(d)) {
                level = Some(digit);
            } else if matches!(c, 'b' | 'S') {
                if cluster.get(at + 1..).is_none_or(str::is_empty) {
                    words.next();
                }
                break;
            }
        }
    }
    level
}

/// Reads the command line as `getopt_long` reads it (`cash-getopt`): options and
/// operands in any order, clusters, `--name=value`, unique prefixes, `--` ending the
/// options; each option acted on as it comes, so `--help` before a bad option wins.
fn parse(preset: Preset, env_level: Option<u32>, args: &[String]) -> Parsed {
    let mut reading = Reading {
        options: Options {
            to_stdout: preset == Preset::ToStdout,
            decompress: preset != Preset::Compress,
            force: false,
            keep: false,
            list: false,
            test: false,
            no_name: false,
            no_time: false,
            quiet: false,
            recursive: false,
            synchronous: false,
            verbose: false,
            level: env_level.unwrap_or(6),
            suffix: ".gz".to_owned(),
            notes: Vec::new(),
            files: Vec::new(),
        },
        no_name: None,
        no_time: None,
    };
    let shorts = cash_getopt::optstring(SHORT_OPTIONS);
    for next in Getopt::new(&shorts, LONG_OPTIONS).read(args) {
        let step = match next {
            Ok(Item::Option { id, value, .. }) => reading.apply(id, value),
            Ok(Item::Operand { value, .. }) => {
                reading.options.files.push(value);
                Ok(())
            }
            Err(problem) => Err(usage_failure(&problem.to_string())),
        };
        if let Err(early) = step {
            return early;
        }
    }

    let mut options = reading.options;
    if options.list || options.test {
        options.decompress = true;
        options.to_stdout = true;
    }
    options.no_name = reading.no_name.unwrap_or(options.decompress);
    options.no_time = reading.no_time.unwrap_or(options.decompress);
    if options.suffix.is_empty() || options.suffix.len() > MAX_SUFFIX {
        return Parsed::Fail(format!("{PROGRAM}: invalid suffix '{}'", options.suffix));
    }
    if let Some(separator) = options.suffix.chars().find(|c| matches!(c, '/' | '\\')) {
        return Parsed::Fail(format!("{PROGRAM}: suffix contains '{separator}'"));
    }
    Parsed::Run(options)
}

/// GNU gzip 1.14's `--help`, under the invoked name.
fn help(invoked: &str) -> String {
    format!(
        "Usage: {invoked} [OPTION]... [FILE]...\n\
         Compress or uncompress FILEs (by default, compress FILES in-place).\n\
         \n\
         Mandatory arguments to long options are mandatory for short options too.\n\
         \n\
         \x20 -c, --stdout      write on standard output, keep original files unchanged\n\
         \x20 -d, --decompress  decompress\n\
         \x20 -f, --force       force overwrite of output file and compress links\n\
         \x20 -h, --help        give this help\n\
         \x20 -k, --keep        keep (don't delete) input files\n\
         \x20 -l, --list        list compressed file contents\n\
         \x20 -L, --license     display software license\n\
         \x20 -n, --no-name     do not save or restore the original name and timestamp\n\
         \x20 -N, --name        save or restore the original name and timestamp\n\
         \x20 -q, --quiet       suppress all warnings\n\
         \x20 -r, --recursive   operate recursively on directories\n\
         \x20     --rsyncable   make rsync-friendly archive\n\
         \x20 -S, --suffix=SUF  use suffix SUF on compressed files\n\
         \x20     --synchronous synchronous output (safer if system crashes, but slower)\n\
         \x20 -t, --test        test compressed file integrity\n\
         \x20 -v, --verbose     verbose mode\n\
         \x20 -V, --version     display version number\n\
         \x20 -1, --fast        compress faster\n\
         \x20 -9, --best        compress better\n\
         \n\
         With no FILE, or when FILE is -, read standard input.\n"
    )
}

fn version_line(invoked: &str) -> String {
    format!("{invoked} (cash): GNU gzip 1.14's options, in pure Rust")
}

/// Why a file was given up on, with everything already said.
#[derive(Debug)]
enum Stop {
    /// An error GNU gzip exits on at once, status 1: the files after it are not done.
    Fatal,
    /// The shell's own streams failed.
    Shell(cash_core::Error),
}

impl From<cash_core::Error> for Stop {
    fn from(error: cash_core::Error) -> Self {
        Self::Shell(error)
    }
}

impl From<io::Error> for Stop {
    fn from(error: io::Error) -> Self {
        Self::Shell(error.into())
    }
}

fn signed(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// `%5.1f%%`: GNU gzip's ratio.
#[expect(
    clippy::cast_precision_loss,
    reason = "a percentage shown to one decimal"
)]
fn ratio(num: i64, den: i64) -> String {
    let pct = if den == 0 {
        0.0
    } else {
        100.0 * num as f64 / den as f64
    };
    format!("{pct:5.1}%")
}

/// The known suffix `name` ends in, if any: the `-S` one first, then GNU's list,
/// matched without regard to case, and never the whole of the name.
fn known_suffix<'a>(name: &'a str, z_suffix: &str) -> Option<&'a str> {
    let lower = name.to_ascii_lowercase();
    let candidates = std::iter::once(z_suffix).chain(KNOWN_SUFFIXES);
    for suffix in candidates {
        let suffix = suffix.to_ascii_lowercase();
        if lower.len() <= suffix.len() || !lower.ends_with(&suffix) {
            continue;
        }
        let cut = name.len() - suffix.len();
        let before = cut
            .checked_sub(1)
            .and_then(|at| name.as_bytes().get(at))
            .copied();
        if matches!(before, Some(b'/' | b'\\')) {
            continue;
        }
        return name.get(cut..);
    }
    None
}

/// The output name of a decompression: the suffix taken off, `.tgz` and `.taz` made
/// `.tar`.
fn decompressed_name(name: &str, suffix: &str) -> String {
    let base = name.get(..name.len() - suffix.len()).unwrap_or_default();
    match suffix.to_ascii_lowercase().as_str() {
        ".tgz" | ".taz" => format!("{base}.tar"),
        _ => base.to_owned(),
    }
}

/// The last component of `name`, on either separator.
fn base_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// `name` with its last component replaced by `base`.
fn with_base_name(name: &str, base: &str) -> String {
    let cut = name.rfind(['/', '\\']).map_or(0, |at| at + 1);
    format!("{}{base}", name.get(..cut).unwrap_or_default())
}

/// A file's modification time as the header stores it; `None` when it does not fit.
fn stamp_of(time: SystemTime) -> Option<u32> {
    let secs = time.duration_since(UNIX_EPOCH).ok()?.as_secs();
    if secs == 0 {
        return None;
    }
    u32::try_from(secs).ok()
}

/// The `-l` table's header line.
fn list_header(verbose: bool) -> String {
    let mut line = String::new();
    if verbose {
        line.push_str("method  crc     date  time  ");
    }
    let _ = writeln!(
        line,
        "{:>SIZE_WIDTH$} {:>SIZE_WIDTH$}  ratio uncompressed_name",
        "compressed", "uncompressed"
    );
    line
}

/// One row of the `-l` table; `verbose` carries the crc and date columns.
fn list_row(
    verbose: Option<(u32, String)>,
    bytes_in: u64,
    bytes_out: u64,
    header_bytes: u64,
    name: &str,
) -> String {
    let mut line = String::new();
    if let Some((crc, date)) = verbose {
        let _ = write!(line, "{:5} {crc:08x} {date} ", "defla");
    }
    let num = signed(bytes_out) - (signed(bytes_in) - signed(header_bytes));
    let _ = writeln!(
        line,
        "{bytes_in:>SIZE_WIDTH$} {bytes_out:>SIZE_WIDTH$} {} {name}",
        ratio(num, signed(bytes_out))
    );
    line
}

/// What became of one input, for the line `-v` ends with.
#[derive(Debug, Default, Clone, Copy)]
struct Done {
    /// Bytes read.
    bytes_in: u64,
    /// Bytes written.
    bytes_out: u64,
    /// The header's bytes (and the trailer's, on decompression) of the first member,
    /// which the ratio leaves out; 0 once a second member was looked for.
    header_bytes: u64,
    /// The last member's CRC.
    crc: u32,
}

/// One run of the command over its files.
struct Run<'a, SE: cash_core::ShellExtensions> {
    options: Options,
    context: &'a cash_core::ExecutionContext<'a, SE>,
    /// The exit status so far: 0, 2 for a warning, 1 for an error.
    status: u8,
    /// `-l`'s table, written at the end as GNU gzip's buffered standard output is.
    listing: Vec<u8>,
    listed_header: bool,
    total_in: u64,
    total_out: u64,
    /// The last parsed first member's header and trailer bytes, for the totals' ratio.
    header_bytes: u64,
}

impl<SE: cash_core::ShellExtensions> Run<'_, SE> {
    fn stderr(&self) -> impl Write + 'static {
        self.context.stderr()
    }

    /// A warning: `line` said unless `-q`, status 2 either way.
    fn warn_line(&mut self, line: &str) -> Result<(), Stop> {
        if !self.options.quiet {
            writeln!(self.stderr(), "{line}")?;
        }
        if self.status == 0 {
            self.status = 2;
        }
        Ok(())
    }

    /// A warning about a file.
    fn warn(&mut self, text: &str) -> Result<(), Stop> {
        self.warn_line(&format!("{PROGRAM}: {text}"))
    }

    /// An error that lets the next file go on: status 1.
    fn error(&mut self, text: &str) -> Result<(), Stop> {
        writeln!(self.stderr(), "{PROGRAM}: {text}")?;
        self.status = 1;
        Ok(())
    }

    /// `not in gzip format`: an error with the newline that ends a `-v` line begun,
    /// after which the next file goes on.
    fn not_gzip(&mut self, name: &str) -> Result<(), Stop> {
        writeln!(self.stderr(), "\n{PROGRAM}: {name}: not in gzip format")?;
        self.status = 1;
        Ok(())
    }

    /// An error GNU gzip exits on: said with the newline that ends a `-v` line begun.
    fn fatal(&mut self, name: &str, text: &str) -> Stop {
        self.status = 1;
        match writeln!(self.stderr(), "\n{PROGRAM}: {name}: {text}") {
            Ok(()) => Stop::Fatal,
            Err(e) => Stop::Shell(e.into()),
        }
    }

    /// `trouble` inside `iname`'s stream, written to `oname`: always fatal.
    fn stream_failed(&mut self, trouble: Trouble, iname: &str, oname: &str) -> Stop {
        match trouble {
            Trouble::Read(e) => self.fatal(iname, &strerror(&e)),
            Trouble::Write(e) => self.fatal(oname, &strerror(&e)),
            Trouble::Eof => self.fatal(iname, "unexpected end of file"),
            Trouble::Format => self.fatal(iname, "invalid compressed data--format violated"),
            Trouble::Crc => self.fatal(iname, "invalid compressed data--crc error"),
            Trouble::Length => self.fatal(iname, "invalid compressed data--length error"),
        }
    }

    /// What a member's start that is not a member means for `name`: `Ok(None)` when the
    /// file is done with (said, status set), `Ok(Some(..))` for passthrough bytes.
    fn unusable_start(&mut self, start: Start, name: &str) -> Result<Option<Vec<u8>>, Stop> {
        match start {
            Start::Member(_) => Ok(None),
            Start::Passthrough(taken) => Ok(Some(taken)),
            Start::NotGzip => self.not_gzip(name).map(|()| None),
            Start::Refused(format) => self
                .error(&format!("{name}: {format} is not supported by cash's gzip"))
                .map(|()| None),
            Start::Problem(problem) => self.error(&problem.shown(name)).map(|()| None),
            Start::End | Start::Zeros | Start::Garbage => {
                Err(self.fatal(name, "unexpected end of file"))
            }
        }
    }

    fn is_terminal(&self, fd: ShellFd) -> bool {
        crate::compress::is_terminal(self.context, fd)
    }

    /// Standard input's modification time when it is a file, as GNU gzip stores it for
    /// `gzip < file`; 0 for a pipe or a terminal.
    fn stdin_stamp(&self) -> u32 {
        match self.context.try_fd(OpenFiles::STDIN_FD) {
            Some(OpenFile::File(file)) => file
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(stamp_of)
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// The date column of `-lv`, in the zone `TZ` names.
    fn list_date(&self, secs: u32) -> String {
        let zone = cash_core::timefmt::Zone::of_shell(self.context.shell);
        zone.format_system_time(
            UNIX_EPOCH + Duration::from_secs(u64::from(secs)),
            "%b %e %H:%M",
        )
    }

    /// The whole run: every file, then the totals and the table.
    fn run_all(&mut self) -> Result<(), Stop> {
        for note in self.options.notes.clone() {
            writeln!(self.stderr(), "{note}")?;
        }
        let files = if self.options.files.is_empty() {
            vec!["-".to_owned()]
        } else {
            self.options.files.clone()
        };
        let mut outcome = Ok(());
        for file in &files {
            outcome = self.treat_file(file);
            if outcome.is_err() {
                break;
            }
        }
        if outcome.is_ok()
            && self.options.list
            && !self.options.quiet
            && files.len() > 1
            && self.total_in > 0
            && self.total_out > 0
        {
            if self.options.verbose {
                self.listing
                    .extend_from_slice(b"                            ");
            }
            let row = list_row(
                None,
                self.total_in,
                self.total_out,
                self.header_bytes,
                "(totals)",
            );
            self.listing.extend_from_slice(row.as_bytes());
        }
        if !self.listing.is_empty() {
            let mut stdout = self.context.stdout();
            stdout.write_all(&self.listing)?;
            stdout.flush()?;
        }
        outcome
    }

    /// Standard input to standard output.
    fn treat_stdin(&mut self) -> Result<(), Stop> {
        let options = self.options.clone();
        let refused = !options.force
            && !options.list
            && self.is_terminal(if options.decompress {
                OpenFiles::STDIN_FD
            } else {
                OpenFiles::STDOUT_FD
            });
        if refused {
            let (way, what) = if options.decompress {
                ("read from", "de")
            } else {
                ("written to", "")
            };
            writeln!(
                self.stderr(),
                "{PROGRAM}: compressed data not {way} a terminal. Use -f to force {what}compression.\n\
                 For help, type: {PROGRAM} -h"
            )?;
            self.status = 1;
            return Err(Stop::Fatal);
        }
        let mut input = Input::new(Box::new(self.context.stdin()));
        let stamp = if options.no_time {
            0
        } else {
            self.stdin_stamp()
        };
        if options.list {
            return self.list_one(&mut input, "stdin", "stdout", stamp, None);
        }
        let mut stdout: Box<dyn Write> = if options.test {
            Box::new(io::sink())
        } else {
            Box::new(self.context.stdout())
        };
        if options.decompress {
            let passthrough = options.force && options.to_stdout;
            let start = match read_start(&mut input, 1, false, passthrough) {
                Ok(start) => start,
                Err(trouble) => return Err(self.stream_failed(trouble, "stdin", "stdout")),
            };
            match start {
                Start::Member(member) => {
                    self.note_extra(&member, "stdin")?;
                    self.decompress_rest(&mut input, &mut stdout, "stdin", "stdout", &member)?;
                }
                other => {
                    if let Some(taken) = self.unusable_start(other, "stdin")? {
                        self.copy_through(&mut input, &mut stdout, "stdin", "stdout", &taken)?;
                    } else {
                        return Ok(());
                    }
                }
            }
            if options.verbose && options.test {
                writeln!(self.stderr(), " OK")?;
            }
            return Ok(());
        }
        let done = self.compress_into(&mut input, &mut stdout, "stdin", "stdout", stamp, None)?;
        if options.verbose {
            let num = signed(done.bytes_in) - (signed(done.bytes_out) - signed(done.header_bytes));
            writeln!(self.stderr(), "{}", ratio(num, signed(done.bytes_in)))?;
        }
        Ok(())
    }

    /// Every entry of the folder `name`, in name order.
    fn treat_dir(&mut self, name: &str, path: &Path) -> Result<(), Stop> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) => return self.error(&format!("{name}: {}", strerror(&e))),
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for entry in names {
            let child = if name.ends_with(['/', '\\']) {
                format!("{name}{entry}")
            } else {
                format!("{name}/{entry}")
            };
            self.treat_file(&child)?;
        }
        Ok(())
    }

    /// Finds the file to read for `name`: when decompressing, a name without a known
    /// suffix is tried with each, as `gunzip foo` finds `foo.gz`.
    fn locate(&self, name: &str) -> Result<(String, PathBuf, fs::Metadata), io::Error> {
        let path = self.context.shell.absolute_path(Path::new(name));
        match fs::metadata(&path) {
            Ok(meta) => return Ok((name.to_owned(), path, meta)),
            Err(e)
                if e.kind() != io::ErrorKind::NotFound
                    || !self.options.decompress
                    || known_suffix(name, &self.options.suffix).is_some() =>
            {
                return Err(e);
            }
            Err(_) => {}
        }
        let suffixes = std::iter::once(self.options.suffix.as_str()).chain(SEARCHED_SUFFIXES);
        let mut first_error = None;
        for suffix in suffixes {
            let candidate = format!("{name}{suffix}");
            let path = self.context.shell.absolute_path(Path::new(&candidate));
            match fs::metadata(&path) {
                Ok(meta) => return Ok((candidate, path, meta)),
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        Err(first_error.unwrap_or_else(|| io::Error::from(io::ErrorKind::NotFound)))
    }

    /// Whether `target` may be written: it does not exist, `-f` was given, or the user
    /// said yes at the console.
    fn may_overwrite(&mut self, oname: &str, target: &Path) -> Result<bool, Stop> {
        if fs::symlink_metadata(target).is_err() || self.options.force {
            return Ok(true);
        }
        let mut stderr = self.stderr();
        write!(stderr, "{PROGRAM}: {oname} already exists;")?;
        let ok = if self.is_terminal(OpenFiles::STDIN_FD) {
            write!(stderr, " do you wish to overwrite (y or n)? ")?;
            stderr.flush()?;
            self.answer_is_yes()?
        } else {
            false
        };
        if !ok {
            writeln!(stderr, "\tnot overwritten")?;
            if self.status == 0 {
                self.status = 2;
            }
        }
        Ok(ok)
    }

    /// Reads one line of standard input: `y` or `Y` first is yes.
    fn answer_is_yes(&self) -> Result<bool, Stop> {
        Ok(crate::compress::answer_is_yes(self.context)?)
    }

    /// `-v` says of an extra field that it is ignored.
    fn note_extra(&self, member: &Member, name: &str) -> Result<(), Stop> {
        if let Some(extra) = member.extra.filter(|_| self.options.verbose) {
            writeln!(
                self.stderr(),
                "{PROGRAM}: {name}: extra field of {extra} bytes ignored"
            )?;
        }
        Ok(())
    }

    /// The name a named file is written to, or `None` when it is to be skipped: GNU's
    /// `make_ofname`.
    fn output_name(&mut self, name: &str) -> Result<Option<String>, Stop> {
        let options = self.options.clone();
        if options.to_stdout && !options.list && !options.test {
            return Ok(Some("stdout".to_owned()));
        }
        let suffix = known_suffix(name, &options.suffix);
        if options.decompress {
            return Ok(Some(match suffix {
                Some(suffix) => decompressed_name(name, suffix),
                None if !options.recursive && (options.list || options.test) => name.to_owned(),
                None => {
                    if options.verbose || (!options.recursive && !options.quiet) {
                        self.warn(&format!("{name}: unknown suffix -- ignored"))?;
                    }
                    return Ok(None);
                }
            }));
        }
        if let Some(suffix) = suffix.filter(|_| !options.force) {
            if options.verbose || (!options.recursive && !options.quiet) {
                writeln!(
                    self.stderr(),
                    "{PROGRAM}: {name} already has {suffix} suffix -- unchanged"
                )?;
            }
            return Ok(None);
        }
        Ok(Some(format!("{name}{}", options.suffix)))
    }

    /// One named file: compressed, decompressed, tested or listed as the options say.
    #[expect(
        clippy::too_many_lines,
        reason = "GNU gzip's treat_file, one pass kept together"
    )]
    fn treat_file(&mut self, name: &str) -> Result<(), Stop> {
        if name == "-" {
            return self.treat_stdin();
        }
        let options = self.options.clone();
        let (name, path, meta) = match self.locate(name) {
            Ok(found) => found,
            Err(e) => {
                let shown = if options.decompress
                    && e.kind() == io::ErrorKind::NotFound
                    && known_suffix(name, &options.suffix).is_none()
                {
                    format!("{name}{}", options.suffix)
                } else {
                    name.to_owned()
                };
                return self.error(&format!("{shown}: {}", strerror(&e)));
            }
        };
        let name = name.as_str();
        if meta.is_dir() {
            if options.recursive {
                return self.treat_dir(name, &path);
            }
            return self.warn(&format!("{name} is a directory -- ignored"));
        }
        if !options.to_stdout {
            if !meta.is_file() {
                return self.warn(&format!(
                    "{name} is not a directory or a regular file - ignored"
                ));
            }
            if !options.force {
                let links = cash_win32::fs::file_link_count(&path, &meta);
                if links >= 2 {
                    let others = links - 1;
                    let plural = if others == 1 { "" } else { "s" };
                    return self.warn(&format!(
                        "{name} has {others} other link{plural} -- file ignored"
                    ));
                }
            }
        }
        let Some(mut oname) = self.output_name(name)? else {
            return Ok(());
        };

        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(e) => return self.error(&format!("{name}: {}", strerror(&e))),
        };
        let mut input = Input::new(Box::new(file));
        let read_only = meta.permissions().readonly();
        let input_times = fs::FileTimes::new()
            .set_modified(meta.modified().unwrap_or(UNIX_EPOCH))
            .set_accessed(meta.accessed().unwrap_or(UNIX_EPOCH));

        if !options.decompress {
            let stamp = meta.modified().ok().and_then(stamp_of);
            let out_of_range = stamp.is_none() && !options.no_time;
            let stamp = if options.no_time {
                0
            } else {
                stamp.unwrap_or(0)
            };
            let stored_name = (!options.no_name).then(|| base_name(name).as_bytes().to_vec());
            let range_warning =
                format!("{name}: warning: file timestamp out of range for gzip format");
            if options.to_stdout {
                let mut stdout = self.context.stdout();
                if options.verbose {
                    write!(self.stderr(), "{name}:\t")?;
                }
                if out_of_range {
                    self.warn(&range_warning)?;
                }
                let done = self.compress_into(
                    &mut input,
                    &mut stdout,
                    name,
                    "stdout",
                    stamp,
                    stored_name.as_deref(),
                )?;
                return self.report(done, &oname, false);
            }
            let target_path = self.context.shell.absolute_path(Path::new(&oname));
            if !self.may_overwrite(&oname, &target_path)? {
                return Ok(());
            }
            let mut target = match Replacement::create(target_path) {
                Ok(target) => target,
                Err(e) => return self.error(&format!("{oname}: {}", strerror(&e))),
            };
            if options.verbose {
                write!(self.stderr(), "{name}:\t")?;
            }
            if out_of_range {
                self.warn(&range_warning)?;
            }
            let done = match self.compress_into(
                &mut input,
                target.file(),
                name,
                &oname,
                stamp,
                stored_name.as_deref(),
            ) {
                Ok(done) => done,
                Err(stop) => {
                    target.abandon();
                    return Err(stop);
                }
            };
            drop(input);
            if let Err(e) = target.finish(input_times, read_only, options.synchronous) {
                return self.error(&format!("{oname}: {}", strerror(&e)));
            }
            if !options.keep {
                if let Err(e) = cash_win32::unix::remove_even_read_only(&path) {
                    self.error(&format!("{name}: {}", strerror(&e)))?;
                }
            }
            return self.report(done, &oname, false);
        }

        // Decompression: the first member's header first, since `-N` takes the output
        // name from it, and a file that is not gzip's is left alone.
        let name_wanted = (options.list || !options.to_stdout) && !options.no_name;
        let passthrough = options.force && options.to_stdout && !options.list;
        self.header_bytes = 0;
        let start = match read_start(&mut input, 1, name_wanted, passthrough) {
            Ok(start) => start,
            Err(trouble) => return Err(self.stream_failed(trouble, name, &oname)),
        };
        let (first, taken) = match start {
            Start::Member(member) => (Some(member), Vec::new()),
            other => match self.unusable_start(other, name)? {
                Some(taken) => (None, taken),
                None => return Ok(()),
            },
        };
        let stored_stamp = first.as_ref().map_or(0, |m| m.mtime);
        if let Some(stored) = first.as_ref().and_then(|m| m.name.as_deref()) {
            let base = String::from_utf8_lossy(stored).into_owned();
            oname = with_base_name(&oname, base_name(&base));
        }
        if let Some(member) = &first {
            self.note_extra(member, name)?;
        }
        let restore_time = (!options.no_time && stored_stamp != 0)
            .then(|| UNIX_EPOCH + Duration::from_secs(u64::from(stored_stamp)));

        if options.list {
            let stamp = meta.modified().ok().and_then(stamp_of).unwrap_or(0);
            return self.list_one(&mut input, name, &oname, stamp, first);
        }

        if options.to_stdout {
            let mut stdout: Box<dyn Write> = if options.test {
                Box::new(io::sink())
            } else {
                Box::new(self.context.stdout())
            };
            if options.verbose {
                write!(self.stderr(), "{name}:\t")?;
            }
            let done = match first {
                Some(member) => {
                    let Some(done) =
                        self.decompress_rest(&mut input, &mut stdout, name, &oname, &member)?
                    else {
                        return Ok(());
                    };
                    done
                }
                None => self.copy_through(&mut input, &mut stdout, name, &oname, &taken)?,
            };
            return self.report(done, &oname, true);
        }

        let target_path = self.context.shell.absolute_path(Path::new(&oname));
        if !self.may_overwrite(&oname, &target_path)? {
            return Ok(());
        }
        let mut target = match Replacement::create(target_path) {
            Ok(target) => target,
            Err(e) => return self.error(&format!("{oname}: {}", strerror(&e))),
        };
        if options.verbose {
            write!(self.stderr(), "{name}:\t")?;
        }
        let member = first.unwrap_or_default();
        let done = match self.decompress_rest(&mut input, target.file(), name, &oname, &member) {
            Ok(Some(done)) => done,
            Ok(None) => {
                target.abandon();
                return Ok(());
            }
            Err(stop) => {
                target.abandon();
                return Err(stop);
            }
        };
        drop(input);
        let times = restore_time.map_or(input_times, |time| input_times.set_modified(time));
        if let Err(e) = target.finish(times, read_only, options.synchronous) {
            return self.error(&format!("{oname}: {}", strerror(&e)));
        }
        if !options.keep {
            if let Err(e) = cash_win32::unix::remove_even_read_only(&path) {
                self.error(&format!("{name}: {}", strerror(&e)))?;
            }
        }
        self.report(done, &oname, true)
    }

    /// The line `-v` ends with, after the `name:\t` begun before the work.
    fn report(&self, done: Done, oname: &str, decompress: bool) -> Result<(), Stop> {
        let options = &self.options;
        if !options.verbose {
            return Ok(());
        }
        let mut stderr = self.stderr();
        if options.test {
            writeln!(stderr, " OK")?;
            return Ok(());
        }
        let (bytes_in, bytes_out) = (signed(done.bytes_in), signed(done.bytes_out));
        let header = signed(done.header_bytes);
        let shown = if decompress {
            ratio(bytes_out - (bytes_in - header), bytes_out)
        } else {
            ratio(bytes_in - (bytes_out - header), bytes_in)
        };
        let how = if options.keep {
            "created"
        } else {
            "replaced with"
        };
        writeln!(stderr, "{shown} -- {how} {oname}")?;
        Ok(())
    }

    /// Compresses all of `input` into `out`, header and trailer included.
    fn compress_into(
        &mut self,
        input: &mut Input,
        out: &mut dyn Write,
        iname: &str,
        oname: &str,
        stamp: u32,
        stored_name: Option<&[u8]>,
    ) -> Result<Done, Stop> {
        let header = header(stamp, self.options.level, stored_name);
        if let Err(e) = out.write_all(&header) {
            return Err(self.stream_failed(Trouble::Write(e), iname, oname));
        }
        let threads = cash_archive::codec::parallel::threads(0);
        match deflate_parallel(input, out, self.options.level, threads) {
            // GNU gzip counts the trailer with the header when it shows the ratio.
            Ok((counts, written)) => Ok(Done {
                bytes_in: counts.size,
                bytes_out: written + header.len() as u64,
                header_bytes: header.len() as u64 + 8,
                crc: counts.crc,
            }),
            Err(trouble) => Err(self.stream_failed(trouble, iname, oname)),
        }
    }

    /// `-f -c` on what is not gzip's: `taken` and the rest of `input`, unchanged.
    fn copy_through(
        &mut self,
        input: &mut Input,
        out: &mut dyn Write,
        iname: &str,
        oname: &str,
        taken: &[u8],
    ) -> Result<Done, Stop> {
        let mut written = taken.len() as u64;
        let result = out.write_all(taken).map_err(Trouble::Write).and_then(|()| {
            loop {
                if !input.fill().map_err(Trouble::Read)? {
                    break Ok(());
                }
                let chunk = input.available().to_vec();
                out.write_all(&chunk).map_err(Trouble::Write)?;
                written += chunk.len() as u64;
                input.advance(chunk.len());
            }
        });
        match result {
            Ok(()) => Ok(Done {
                bytes_in: input.read_total,
                bytes_out: written,
                header_bytes: 0,
                crc: 0,
            }),
            Err(trouble) => Err(self.stream_failed(trouble, iname, oname)),
        }
    }

    /// The members of `input` from the first one's data on, `first` being its header.
    /// `None` when a later member's header was unusable: the output is to be dropped.
    fn decompress_rest(
        &mut self,
        input: &mut Input,
        out: &mut dyn Write,
        iname: &str,
        oname: &str,
        first: &Member,
    ) -> Result<Option<Done>, Stop> {
        let mut written = 0u64;
        let mut header_bytes = first.header_len + 8;
        let mut part = 1u32;
        let mut crc;
        loop {
            crc = match inflate_member(input, out, &mut written) {
                Ok(counts) => counts.crc,
                Err(trouble) => return Err(self.stream_failed(trouble, iname, oname)),
            };
            match input.at_end() {
                Ok(true) => break,
                Ok(false) => {}
                Err(e) => return Err(self.stream_failed(Trouble::Read(e), iname, oname)),
            }
            part += 1;
            header_bytes = 0;
            let start = match read_start(input, part, false, false) {
                Ok(start) => start,
                Err(trouble) => return Err(self.stream_failed(trouble, iname, oname)),
            };
            match start {
                Start::Member(member) => self.note_extra(&member, iname)?,
                Start::End => break,
                Start::Zeros => {
                    if self.options.verbose {
                        self.warn_line(&format!(
                            "\n{PROGRAM}: {iname}: decompression OK, trailing zero bytes ignored"
                        ))?;
                    }
                    break;
                }
                Start::Garbage | Start::NotGzip | Start::Refused(_) | Start::Passthrough(_) => {
                    self.warn_line(&format!(
                        "\n{PROGRAM}: {iname}: decompression OK, trailing garbage ignored"
                    ))?;
                    break;
                }
                Start::Problem(problem) => {
                    self.error(&problem.shown(iname))?;
                    return Ok(None);
                }
            }
        }
        self.header_bytes = header_bytes;
        Ok(Some(Done {
            bytes_in: input.read_total,
            bytes_out: written,
            header_bytes,
            crc,
        }))
    }

    /// One row of `-l`: the whole file is read, as GNU gzip 1.14 reads it, so a file
    /// of several members is listed by all of them. `stamp` is the file's time; the
    /// stored one replaces it with `-N`.
    fn list_one(
        &mut self,
        input: &mut Input,
        iname: &str,
        oname: &str,
        stamp: u32,
        first: Option<Member>,
    ) -> Result<(), Stop> {
        self.header_bytes = 0;
        let first = if let Some(member) = first {
            member
        } else {
            let start = match read_start(input, 1, !self.options.no_name, false) {
                Ok(start) => start,
                Err(trouble) => return Err(self.stream_failed(trouble, iname, oname)),
            };
            match start {
                Start::Member(member) => {
                    self.note_extra(&member, iname)?;
                    member
                }
                other => {
                    self.unusable_start(other, iname)?;
                    return Ok(());
                }
            }
        };
        let stamp = if !self.options.no_time && first.mtime != 0 {
            first.mtime
        } else {
            stamp
        };
        let oname = match first.name.as_deref() {
            Some(stored) => with_base_name(oname, base_name(&String::from_utf8_lossy(stored))),
            None => oname.to_owned(),
        };
        let mut sink = io::sink();
        let Some(done) = self.decompress_rest(input, &mut sink, iname, &oname, &first)? else {
            return Ok(());
        };
        if !self.options.quiet && !self.listed_header {
            self.listed_header = true;
            let header = list_header(self.options.verbose);
            self.listing.extend_from_slice(header.as_bytes());
        }
        let verbose = self
            .options
            .verbose
            .then(|| (done.crc, self.list_date(stamp)));
        let row = list_row(
            verbose,
            done.bytes_in,
            done.bytes_out,
            done.header_bytes,
            &oname,
        );
        self.listing.extend_from_slice(row.as_bytes());
        self.total_in += done.bytes_in;
        self.total_out += done.bytes_out;
        Ok(())
    }
}

fn run<SE: cash_core::ShellExtensions>(
    invoked: &'static str,
    preset: Preset,
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    let env_level = context
        .shell
        .env()
        .get("GZIP")
        .filter(|(_, var)| var.is_exported())
        .map(|(_, var)| var.value().to_cow_str(context.shell).into_owned())
        .and_then(|gzip| level_from_environment(&gzip));
    let options = match parse(preset, env_level, args) {
        Parsed::Run(options) => options,
        Parsed::Help => {
            context.stdout().write_all(help(invoked).as_bytes())?;
            return Ok(ExecutionResult::success());
        }
        Parsed::Version => {
            writeln!(context.stdout(), "{}", version_line(invoked))?;
            return Ok(ExecutionResult::success());
        }
        Parsed::License => {
            writeln!(
                context.stdout(),
                "{}\nMIT licence. Deflate by miniz_oxide (MIT, Zlib or Apache-2.0).",
                version_line(invoked)
            )?;
            return Ok(ExecutionResult::success());
        }
        Parsed::Fail(message) => {
            writeln!(context.stderr(), "{message}")?;
            return Ok(ExecutionResult::general_error());
        }
    };
    let mut run = Run {
        options,
        context,
        status: 0,
        listing: Vec::new(),
        listed_header: false,
        total_in: 0,
        total_out: 0,
        header_bytes: 0,
    };
    match run.run_all() {
        Ok(()) => Ok(ExecutionResult::new(run.status)),
        Err(Stop::Fatal) => Ok(ExecutionResult::new(1)),
        Err(Stop::Shell(error)) => Err(error),
    }
}

#[cfg(test)]
#[expect(
    clippy::panic,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod tests {
    use super::*;

    fn options(preset: Preset, args: &[&str]) -> Options {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        match parse(preset, None, &args) {
            Parsed::Run(options) => options,
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    fn failure(args: &[&str]) -> String {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        match parse(Preset::Compress, None, &args) {
            Parsed::Fail(message) => message,
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn the_names_preset_their_modes() {
        let gzip = options(Preset::Compress, &[]);
        assert!(!gzip.decompress && !gzip.to_stdout && !gzip.no_name);
        let gunzip = options(Preset::Decompress, &[]);
        assert!(gunzip.decompress && !gunzip.to_stdout && gunzip.no_name && gunzip.no_time);
        let zcat = options(Preset::ToStdout, &["-l"]);
        assert!(zcat.decompress && zcat.to_stdout && zcat.list);
        let named = options(Preset::Decompress, &["-N"]);
        assert!(!named.no_name && !named.no_time);
    }

    #[test]
    fn clusters_values_prefixes_and_operands() {
        let parsed = options(
            Preset::Compress,
            &["-cn9", "a", "-S.x", "--rs", "b", "--", "-c"],
        );
        assert!(parsed.to_stdout && parsed.no_name && parsed.no_time);
        assert_eq!(parsed.level, 9);
        assert_eq!(parsed.suffix, ".x");
        assert_eq!(parsed.files, vec!["a", "b", "-c"]);
        assert_eq!(options(Preset::Compress, &["--suffix", ".z"]).suffix, ".z");
        assert_eq!(options(Preset::Compress, &["--suf=.w"]).suffix, ".w");
        assert_eq!(options(Preset::Compress, &["-1", "-9"]).level, 9);
        assert_eq!(options(Preset::Compress, &["--best", "--fast"]).level, 1);
        assert!(options(Preset::Compress, &["-t"]).decompress);
        assert!(options(Preset::Compress, &["-vq"]).quiet);
        assert!(options(Preset::Compress, &["-qv"]).verbose);
    }

    #[test]
    fn errors_are_worded_as_getopt_words_them() {
        assert_eq!(
            failure(&["-Y"]),
            "gzip: invalid option -- 'Y'\nTry `gzip --help' for more information."
        );
        assert_eq!(
            failure(&["--foo"]),
            "gzip: unrecognized option '--foo'\nTry `gzip --help' for more information."
        );
        assert_eq!(
            failure(&["--s"]),
            "gzip: option '--s' is ambiguous; possibilities: '--stdout' '--silent' \
             '--synchronous' '--suffix'\nTry `gzip --help' for more information."
        );
        assert_eq!(
            failure(&["-S"]),
            "gzip: option requires an argument -- 'S'\nTry `gzip --help' for more information."
        );
        assert_eq!(
            failure(&["--best=3"]),
            "gzip: option '--best' doesn't allow an argument\nTry `gzip --help' for more information."
        );
        assert_eq!(failure(&["-S", ""]), "gzip: invalid suffix ''");
        assert_eq!(failure(&["-S", "/x"]), "gzip: suffix contains '/'");
        assert_eq!(
            failure(&["-Z"]),
            "gzip: -Z not supported in this version\nTry `gzip --help' for more information."
        );
        assert_eq!(
            failure(&["--bits", "x"]),
            "gzip: -b operand is not an integer\nTry `gzip --help' for more information."
        );
    }

    #[test]
    fn the_environment_names_only_a_level() {
        assert_eq!(level_from_environment("-9"), Some(9));
        assert_eq!(level_from_environment("-9 -1"), Some(1));
        assert_eq!(level_from_environment("x -9"), Some(9));
        assert_eq!(level_from_environment("-9 -- -1"), Some(9));
        assert_eq!(level_from_environment("-nq"), None);
        assert_eq!(level_from_environment(""), None);
        assert_eq!(level_from_environment("--best --no-name"), Some(9));
        assert_eq!(level_from_environment("--fa"), Some(1));
        assert_eq!(level_from_environment("-S 9"), None);
        assert_eq!(level_from_environment("-S9 -2"), Some(2));
        assert_eq!(level_from_environment("--suffix=9 -3"), Some(3));
        assert_eq!(level_from_environment("-x9"), Some(9));
        assert_eq!(level_from_environment("--b"), None);
    }

    #[test]
    fn suffixes_as_gnu_knows_them() {
        assert_eq!(known_suffix("a.gz", ".gz"), Some(".gz"));
        assert_eq!(known_suffix("a.GZ", ".gz"), Some(".GZ"));
        assert_eq!(known_suffix("a.tgz", ".gz"), Some(".tgz"));
        assert_eq!(known_suffix("a.Z", ".gz"), Some(".Z"));
        assert_eq!(known_suffix("a-gz", ".gz"), Some("-gz"));
        assert_eq!(known_suffix("a_z", ".gz"), Some("_z"));
        assert_eq!(known_suffix("a.suf", ".suf"), Some(".suf"));
        assert_eq!(known_suffix("asuf", "suf"), Some("suf"));
        assert_eq!(known_suffix(".gz", ".gz"), None);
        assert_eq!(known_suffix("d/.gz", ".gz"), None);
        assert_eq!(known_suffix("a.txt", ".gz"), None);
        assert_eq!(decompressed_name("a.tar.gz", ".gz"), "a.tar");
        assert_eq!(decompressed_name("a.TGZ", ".TGZ"), "a.tar");
        assert_eq!(decompressed_name("a.taz", ".taz"), "a.tar");
        assert_eq!(decompressed_name("x.tar.tgz", ".tgz"), "x.tar.tar");
        assert_eq!(with_base_name("sub/x.gz", "h"), "sub/h");
        assert_eq!(with_base_name("x", "h"), "h");
        assert_eq!(base_name("a/b\\c"), "c");
    }

    #[test]
    fn a_time_fits_the_header_or_is_none() {
        assert_eq!(stamp_of(UNIX_EPOCH + Duration::from_secs(1)), Some(1));
        assert_eq!(stamp_of(UNIX_EPOCH), None);
        assert_eq!(
            stamp_of(UNIX_EPOCH + Duration::from_secs(0x1_0000_0000)),
            None
        );
        assert_eq!(
            stamp_of(UNIX_EPOCH + Duration::from_secs(0xffff_ffff)),
            Some(0xffff_ffff)
        );
    }

    #[test]
    fn the_list_table_has_gnu_s_columns() {
        assert_eq!(
            list_header(false),
            "         compressed        uncompressed  ratio uncompressed_name\n"
        );
        assert_eq!(
            list_header(true),
            "method  crc     date  time           compressed        uncompressed  ratio uncompressed_name\n"
        );
        assert_eq!(
            list_row(None, 28, 6, 20, "l"),
            "                 28                   6 -33.3% l\n"
        );
        assert_eq!(
            list_row(
                Some((0x363a_3020, "Jan  2 03:04".to_owned())),
                28,
                6,
                20,
                "h"
            ),
            "defla 363a3020 Jan  2 03:04                  28                   6 -33.3% h\n"
        );
        assert_eq!(
            list_row(None, 54, 12, 18, "(totals)"),
            "                 54                  12 -200.0% (totals)\n"
        );
        assert_eq!(
            list_row(None, 23, 0, 18, "em"),
            "                 23                   0   0.0% em\n"
        );
        assert_eq!(ratio(999, 1000), " 99.9%");
    }
}
