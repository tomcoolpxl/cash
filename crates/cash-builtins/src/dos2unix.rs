//! `dos2unix` and `unix2dos` — line-ending conversion, following the dos2unix 7.x manual.
//!
//! Git for Windows ships the real tools in `usr/bin`, but only when that directory is on
//! `PATH`, and a clean machine has neither. cash carries both so that the one-word fix for
//! a CRLF file works everywhere the shell does. `enable -n dos2unix` restores the `PATH`
//! copy for anyone who needs what is deliberately left out.
//!
//! What is implemented is the ASCII conversion scripts use: in place (old-file mode),
//! `-n INFILE OUTFILE` pairs, standard input to standard output, `-O`, `-k`, `-q`, `-v`,
//! `-f`, `-e`, `-l`, `-7`, the BOM options with each tool's own default, and `-i`/`--info`.
//!
//! Code-page, UTF-16 and Mac (CR-only) conversions, symbolic-link following and ownership
//! control are **refused by name** rather than ignored: a `dos2unix -iso` that quietly did
//! an ASCII conversion would return a file that looks converted and is not.
//!
//! A file is replaced by writing a temporary file in the same directory and renaming it
//! over the original, so a failure part-way through never truncates the input.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Convert DOS (CRLF) line breaks to Unix (LF).
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct Dos2UnixCommand {
    /// Options and files, parsed here rather than by clap: the tool's grammar has
    /// multi-letter single-dash options (`-ascii`, `-iso`) and positional modes (`-n`).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Convert Unix (LF) line breaks to DOS (CRLF).
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct Unix2DosCommand {
    /// Options and files, parsed here rather than by clap.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for Dos2UnixCommand {
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
        run(Direction::ToUnix, &self.args, &context)
    }
}

impl builtins::Command for Unix2DosCommand {
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
        run(Direction::ToDos, &self.args, &context)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Direction {
    ToUnix,
    ToDos,
}

impl Direction {
    const fn tool(self) -> &'static str {
        match self {
            Self::ToUnix => "dos2unix",
            Self::ToDos => "unix2dos",
        }
    }

    const fn format(self) -> &'static str {
        match self {
            Self::ToUnix => "Unix",
            Self::ToDos => "DOS",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bom {
    Keep,
    Add,
    Remove,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

/// The `-i`/`--info` flags.
#[derive(Clone, Copy, Default, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one bool per documented flag"
)]
struct InfoFlags {
    dos: bool,
    unix: bool,
    mac: bool,
    bom: bool,
    text: bool,
    eol: bool,
    convert_only: bool,
    header: bool,
    no_path: bool,
    nul: bool,
}

impl InfoFlags {
    fn parse(tool: &str, letters: &str) -> Result<Self, String> {
        let mut flags = Self::default();
        for letter in letters.chars() {
            match letter {
                '0' => flags.nul = true,
                'd' => flags.dos = true,
                'u' => flags.unix = true,
                'm' => flags.mac = true,
                'b' => flags.bom = true,
                't' => flags.text = true,
                'e' => flags.eol = true,
                'c' => flags.convert_only = true,
                'h' => flags.header = true,
                'p' => flags.no_path = true,
                other => {
                    return Err(format!(
                        "{tool}: wrong flag '{other}' for option -i or --info"
                    ));
                }
            }
        }
        if !(flags.dos || flags.unix || flags.mac || flags.bom || flags.text || flags.eol) {
            flags.dos = true;
            flags.unix = true;
            flags.mac = true;
            flags.bom = true;
            flags.text = true;
        }
        Ok(flags)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Job {
    Stdin,
    Old(String),
    New(String, String),
}

#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one bool per documented option"
)]
struct Options {
    direction: Direction,
    bom: Bom,
    seven_bit: bool,
    add_eol: bool,
    error_binary: bool,
    force: bool,
    keep_date: bool,
    newline: bool,
    to_stdout: bool,
    verbosity: Verbosity,
    info: Option<InfoFlags>,
    jobs: Vec<Job>,
}

#[derive(Debug)]
enum Parsed {
    Run(Options),
    Help,
    Version,
    Fail(String),
}

/// Why an option the real tool has is not carried.
fn refusal(arg: &str) -> Option<&'static str> {
    Some(match arg {
        "-iso" | "-1252" | "-437" | "-850" | "-860" | "-863" | "-865" | "-gb" | "--gb18030" => {
            "code-page conversion"
        }
        "-u" | "--keep-utf16" | "-ul" | "--assume-utf16le" | "-ub" | "--assume-utf16be" => {
            "UTF-16 conversion"
        }
        "-F" | "--follow-symlink" | "-R" | "--replace-symlink" => {
            "converting through symbolic links"
        }
        "--allow-chown" | "--no-allow-chown" => "file ownership control",
        "-L" | "--license" => "the license text",
        _ => return None,
    })
}

fn refused(tool: &str, what: &str, reason: &str) -> String {
    format!(
        "{tool}: {what}: not supported by cash's builtin {tool} ({reason}); \
         `enable -n {tool}` runs the one on PATH instead"
    )
}

fn set_convmode(tool: &str, mode: &str, seven_bit: &mut bool) -> Result<(), String> {
    match mode {
        "ascii" => *seven_bit = false,
        "7bit" => *seven_bit = true,
        "iso" => return Err(refused(tool, "-c iso", "code-page conversion")),
        "mac" => return Err(refused(tool, "-c mac", "Mac (CR-only) line breaks")),
        other => return Err(format!("{tool}: invalid {other} conversion mode specified")),
    }
    Ok(())
}

#[allow(clippy::too_many_lines, reason = "one arm per documented option")]
fn parse(direction: Direction, args: &[String]) -> Parsed {
    let tool = direction.tool();
    let mut options = Options {
        direction,
        bom: match direction {
            Direction::ToUnix => Bom::Remove,
            Direction::ToDos => Bom::Keep,
        },
        seven_bit: false,
        add_eol: false,
        error_binary: false,
        force: false,
        keep_date: false,
        newline: false,
        to_stdout: false,
        verbosity: Verbosity::Normal,
        info: None,
        jobs: Vec::new(),
    };
    let mut new_mode = false;
    let mut pending: Option<String> = None;
    let mut end_of_options = false;
    let unpaired =
        |file: &str| format!("{tool}: target of file {file} not specified in new-file mode");

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if end_of_options || arg == "-" || !arg.starts_with('-') {
            if new_mode {
                if arg == "-" {
                    return Parsed::Fail(format!(
                        "{tool}: '-' (standard input) cannot be used in new-file mode"
                    ));
                }
                match pending.take() {
                    Some(input) => options.jobs.push(Job::New(input, arg.clone())),
                    None => pending = Some(arg.clone()),
                }
            } else if arg == "-" && !end_of_options {
                options.jobs.push(Job::Stdin);
            } else {
                options.jobs.push(Job::Old(arg.clone()));
            }
            continue;
        }

        if let Some(reason) = refusal(arg) {
            return Parsed::Fail(refused(tool, arg, reason));
        }

        match arg.as_str() {
            "--" => end_of_options = true,
            "-h" | "--help" => return Parsed::Help,
            "-V" | "--version" => return Parsed::Version,
            "-ascii" => options.seven_bit = false,
            "-7" => options.seven_bit = true,
            "-c" | "--convmode" => {
                let Some(mode) = iter.next() else {
                    return Parsed::Fail(format!("{tool}: option '{arg}' requires an argument"));
                };
                if let Err(message) = set_convmode(tool, mode, &mut options.seven_bit) {
                    return Parsed::Fail(message);
                }
            }
            "-b" | "--keep-bom" => options.bom = Bom::Keep,
            "-m" | "--add-bom" => options.bom = Bom::Add,
            "-r" | "--remove-bom" => options.bom = Bom::Remove,
            "-e" | "--add-eol" => options.add_eol = true,
            "--no-add-eol" => options.add_eol = false,
            "--error-binary" => options.error_binary = true,
            "--no-error-binary" => options.error_binary = false,
            "-f" | "--force" => options.force = true,
            "-s" | "--safe" => options.force = false,
            "-k" | "--keepdate" => options.keep_date = true,
            "-l" | "--newline" => options.newline = true,
            "-n" | "--newfile" => new_mode = true,
            "-o" | "--oldfile" => {
                if let Some(input) = pending.take() {
                    return Parsed::Fail(unpaired(&input));
                }
                new_mode = false;
            }
            "-O" | "--to-stdout" => options.to_stdout = true,
            "-q" | "--quiet" => options.verbosity = Verbosity::Quiet,
            "-v" | "--verbose" => options.verbosity = Verbosity::Verbose,
            "-S" | "--skip-symlink" => {}
            other => {
                let info = other
                    .strip_prefix("--info=")
                    .or_else(|| (other == "--info").then_some(""))
                    .or_else(|| other.strip_prefix("-i"));
                if let Some(letters) = info {
                    match InfoFlags::parse(tool, letters) {
                        Ok(flags) => options.info = Some(flags),
                        Err(message) => return Parsed::Fail(message),
                    }
                } else if let Some(mode) = other.strip_prefix("--convmode=") {
                    if let Err(message) = set_convmode(tool, mode, &mut options.seven_bit) {
                        return Parsed::Fail(message);
                    }
                } else {
                    return Parsed::Fail(format!(
                        "{tool}: unrecognized option '{other}'\nTry '{tool} --help' for more information."
                    ));
                }
            }
        }
    }

    if let Some(input) = pending {
        return Parsed::Fail(unpaired(&input));
    }
    if options.jobs.is_empty() {
        options.jobs.push(Job::Stdin);
    }
    Parsed::Run(options)
}

/// What stopped a conversion.
#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    Binary { byte: u8, line: usize },
    Utf16,
}

/// A finished conversion.
#[derive(Debug)]
struct Converted {
    bytes: Vec<u8>,
    had_bom: bool,
    wrote_bom: bool,
    changed: usize,
    total: usize,
}

/// The manual's definition of text: every control character except TAB, LF, FF and CR
/// marks a file as binary.
const fn is_binary(byte: u32) -> bool {
    byte < 0x20 && !matches!(byte, 0x09 | 0x0A | 0x0C | 0x0D)
}

fn convert(options: &Options, input: &[u8]) -> Result<Converted, Refusal> {
    if input.starts_with(b"\xFF\xFE") || input.starts_with(b"\xFE\xFF") {
        return Err(Refusal::Utf16);
    }
    let had_bom = input.starts_with(UTF8_BOM);
    let body = input.strip_prefix(UTF8_BOM).unwrap_or(input);

    if !options.force {
        if let Some(position) = body.iter().position(|&b| is_binary(u32::from(b))) {
            let line = 1 + body.iter().take(position).filter(|&&b| b == b'\n').count();
            return Err(Refusal::Binary {
                byte: body[position],
                line,
            });
        }
    }

    let wrote_bom = match options.bom {
        Bom::Keep => had_bom,
        Bom::Add => true,
        Bom::Remove => false,
    };
    let eol: &[u8] = match options.direction {
        Direction::ToUnix => b"\n",
        Direction::ToDos => b"\r\n",
    };

    let mut bytes = Vec::with_capacity(body.len() + body.len() / 16 + 4);
    if wrote_bom {
        bytes.extend_from_slice(UTF8_BOM);
    }
    let mut converted = 0;
    let mut total = 0;
    let mut previous = None;
    let mut iter = body.iter().copied().peekable();
    while let Some(byte) = iter.next() {
        match (options.direction, byte) {
            (Direction::ToUnix, b'\r') if iter.peek() == Some(&b'\n') => converted += 1,
            (_, b'\n') => {
                total += 1;
                if options.direction == Direction::ToDos && previous != Some(b'\r') {
                    converted += 1;
                    bytes.push(b'\r');
                }
                bytes.push(b'\n');
                if options.newline {
                    bytes.extend_from_slice(eol);
                }
            }
            (_, b) if options.seven_bit && b >= 0x80 => bytes.push(b' '),
            (_, b) => bytes.push(b),
        }
        previous = Some(byte);
    }
    if options.add_eol && !body.is_empty() && body.last() != Some(&b'\n') {
        bytes.extend_from_slice(eol);
    }

    Ok(Converted {
        bytes,
        had_bom,
        wrote_bom,
        changed: converted,
        total,
    })
}

/// Line-break counts for `--info`.
#[derive(Debug, PartialEq, Eq)]
struct Info {
    dos: usize,
    unix: usize,
    mac: usize,
    bom: &'static str,
    binary: bool,
    last: &'static str,
}

fn analyse(input: &[u8]) -> Info {
    let (bom, units): (&'static str, Vec<u32>) = if let Some(rest) = input.strip_prefix(UTF8_BOM) {
        ("UTF-8", rest.iter().map(|&b| u32::from(b)).collect())
    } else if let Some(rest) = input.strip_prefix(b"\xFF\xFE") {
        let units = rest
            .chunks(2)
            .map(|c| u32::from(c[0]) | (u32::from(c.get(1).copied().unwrap_or(0)) << 8))
            .collect();
        ("UTF-16LE", units)
    } else if let Some(rest) = input.strip_prefix(b"\xFE\xFF") {
        let units = rest
            .chunks(2)
            .map(|c| (u32::from(c[0]) << 8) | u32::from(c.get(1).copied().unwrap_or(0)))
            .collect();
        ("UTF-16BE", units)
    } else {
        ("no_bom", input.iter().map(|&b| u32::from(b)).collect())
    };

    let (mut dos, mut unix, mut mac) = (0, 0, 0);
    let mut i = 0;
    while i < units.len() {
        match units[i] {
            0x0D if units.get(i + 1) == Some(&0x0A) => {
                dos += 1;
                i += 1;
            }
            0x0D => mac += 1,
            0x0A => unix += 1,
            _ => {}
        }
        i += 1;
    }
    let last = match units.as_slice() {
        [.., 0x0D, 0x0A] => "dos",
        [.., 0x0A] => "unix",
        [.., 0x0D] => "mac",
        _ => "noeol",
    };
    Info {
        dos,
        unix,
        mac,
        bom,
        binary: units.iter().any(|&u| is_binary(u)),
        last,
    }
}

fn info_header(flags: InfoFlags) -> String {
    let mut line = String::new();
    push_columns(
        &mut line,
        flags,
        ["DOS", "UNIX", "MAC"].map(ToString::to_string),
        "BOM",
        "TXTBIN",
        "LASTLN",
    );
    line.push_str("  FILE");
    line
}

fn push_columns(
    line: &mut String,
    flags: InfoFlags,
    counts: [String; 3],
    bom: &str,
    text: &str,
    last: &str,
) {
    use std::fmt::Write as _;
    let [dos, unix, mac] = counts;
    if flags.dos {
        let _ = write!(line, "{dos:>8}");
    }
    if flags.unix {
        let _ = write!(line, "{unix:>8}");
    }
    if flags.mac {
        let _ = write!(line, "{mac:>8}");
    }
    if flags.bom {
        let _ = write!(line, "  {bom:<8}");
    }
    if flags.text {
        let _ = write!(line, "  {text:<6}");
    }
    if flags.eol {
        let _ = write!(line, " {last:<6}");
    }
}

/// The `--info` line for one input, or `None` when `c` filters it out.
fn info_line(
    options: &Options,
    flags: InfoFlags,
    name: Option<&str>,
    input: &[u8],
) -> Option<String> {
    let info = analyse(input);
    let shown = name.map(|name| {
        if flags.no_path {
            Path::new(name)
                .file_name()
                .map_or_else(|| name.to_string(), |n| n.to_string_lossy().into_owned())
        } else {
            name.to_string()
        }
    });

    if flags.convert_only {
        let needs = match options.direction {
            Direction::ToUnix => info.dos > 0,
            Direction::ToDos => info.unix > 0,
        };
        return (needs && (options.force || !info.binary)).then(|| shown.unwrap_or_default());
    }

    let mut line = String::new();
    push_columns(
        &mut line,
        flags,
        [info.dos, info.unix, info.mac].map(|n| n.to_string()),
        info.bom,
        if info.binary { "binary" } else { "text" },
        info.last,
    );
    if let Some(shown) = shown {
        line.push_str("  ");
        line.push_str(&shown);
    }
    Some(line)
}

/// A shell-style errno for an I/O failure, which is what the real tool exits with.
fn errno(error: &std::io::Error) -> u8 {
    match error.kind() {
        std::io::ErrorKind::NotFound => 2,
        std::io::ErrorKind::PermissionDenied => 13,
        _ => 1,
    }
}

fn describe(error: &std::io::Error) -> String {
    cash_core::error::os_error_text(error)
}

/// Replace `target` with `bytes` via a temporary file in the same directory.
fn write_replacing(
    target: &Path,
    bytes: &[u8],
    modified: Option<SystemTime>,
) -> std::io::Result<()> {
    let directory = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let base = target
        .file_name()
        .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());

    let mut attempt = 0_u32;
    let (temporary, mut file) = loop {
        let candidate =
            directory.join(format!(".{base}.cash-{}-{attempt}.tmp", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 100 => {
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    };

    let written = file
        .write_all(bytes)
        .and_then(|()| modified.map_or(Ok(()), |time| file.set_modified(time)));
    drop(file);
    let result = written.and_then(|()| fs::rename(&temporary, target));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Standard error, silenced by `-q`.
struct Diagnostics<W> {
    writer: W,
    tool: &'static str,
    verbosity: Verbosity,
}

impl<W: Write> Diagnostics<W> {
    fn warn(&mut self, message: &str) -> std::io::Result<()> {
        if self.verbosity == Verbosity::Quiet {
            return Ok(());
        }
        writeln!(self.writer, "{}: {message}", self.tool)
    }

    fn verbose(&mut self, message: &str) -> std::io::Result<()> {
        if self.verbosity != Verbosity::Verbose {
            return Ok(());
        }
        writeln!(self.writer, "{}: {message}", self.tool)
    }
}

/// An input file that could be read, or the status its failure earned.
enum Loaded {
    Bytes(Vec<u8>, Option<SystemTime>),
    Skipped,
    Failed(u8),
}

fn load<W: Write>(
    path: &Path,
    name: &str,
    skip_symlinks: bool,
    diagnostics: &mut Diagnostics<W>,
) -> std::io::Result<Loaded> {
    let not_regular =
        |d: &mut Diagnostics<W>| d.warn(&format!("Skipping {name}, not a regular file."));
    match fs::symlink_metadata(path) {
        Err(e) => {
            diagnostics.warn(&format!("{name}: {}", describe(&e)))?;
            not_regular(diagnostics)?;
            return Ok(Loaded::Failed(errno(&e)));
        }
        Ok(meta) if skip_symlinks && meta.file_type().is_symlink() => {
            diagnostics.warn(&format!("Skipping symbolic link {name}."))?;
            return Ok(Loaded::Skipped);
        }
        Ok(_) => {}
    }
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) => {
            diagnostics.warn(&format!("{name}: {}", describe(&e)))?;
            return Ok(Loaded::Failed(errno(&e)));
        }
    };
    if !meta.is_file() {
        not_regular(diagnostics)?;
        return Ok(Loaded::Skipped);
    }
    match fs::read(path) {
        Ok(bytes) => Ok(Loaded::Bytes(bytes, meta.modified().ok())),
        Err(e) => {
            diagnostics.warn(&format!("{name}: {}", describe(&e)))?;
            Ok(Loaded::Failed(errno(&e)))
        }
    }
}

fn help(direction: Direction) -> String {
    let tool = direction.tool();
    let (bom_default_keep, bom_default_remove) = match direction {
        Direction::ToUnix => ("", " (default)"),
        Direction::ToDos => (" (default)", ""),
    };
    format!(
        "Usage: {tool} [options] [file ...] [-n infile outfile ...]\n\
         cash builtin, following the dos2unix 7.x manual.\n\
         \x20-ascii                default conversion mode\n\
         \x20-7                    convert 8 bit characters to 7 bit space\n\
         \x20-b, --keep-bom        keep Byte Order Mark{bom_default_keep}\n\
         \x20-c, --convmode MODE   conversion mode: ascii (default) or 7bit\n\
         \x20-e, --add-eol         add a line break to the last line if there isn't one\n\
         \x20--error-binary        return an error when a binary file is skipped\n\
         \x20-f, --force           force conversion of binary files\n\
         \x20-h, --help            display this help text\n\
         \x20-i, --info[=FLAGS]    display file information (flags: 0 d u m b t e c h p)\n\
         \x20-k, --keepdate        keep output file date\n\
         \x20-l, --newline         add additional newline\n\
         \x20-m, --add-bom         add UTF-8 Byte Order Mark\n\
         \x20-n, --newfile         write to new file: infile outfile ...\n\
         \x20-O, --to-stdout       write to standard output\n\
         \x20-o, --oldfile         write to old file (default)\n\
         \x20-q, --quiet           quiet mode, suppress all warnings\n\
         \x20-r, --remove-bom      remove Byte Order Mark{bom_default_remove}\n\
         \x20-s, --safe            skip binary files (default)\n\
         \x20-S, --skip-symlink    keep symbolic links and targets unchanged (default)\n\
         \x20-v, --verbose         verbose operation\n\
         \x20-V, --version         display version number\n\
         Not supported (refused): code pages (-iso, -437, ...), UTF-16 (-u, -ul, -ub),\n\
         -c iso, -c mac, -F, -R, --allow-chown, -L. `enable -n {tool}` runs the one on PATH.\n"
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "one pass over the jobs, kept together"
)]
fn run<SE: cash_core::ShellExtensions>(
    direction: Direction,
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    let tool = direction.tool();
    let options = match parse(direction, args) {
        Parsed::Run(options) => options,
        Parsed::Help => {
            context.stdout().write_all(help(direction).as_bytes())?;
            return Ok(ExecutionResult::success());
        }
        Parsed::Version => {
            writeln!(
                context.stdout(),
                "{tool} (cash builtin), following the dos2unix 7.x manual"
            )?;
            return Ok(ExecutionResult::success());
        }
        Parsed::Fail(message) => {
            writeln!(context.stderr(), "{message}")?;
            return Ok(ExecutionResult::general_error());
        }
    };

    let mut stdout = context.stdout();
    let mut diagnostics = Diagnostics {
        writer: context.stderr(),
        tool,
        verbosity: options.verbosity,
    };
    let mut status = 0_u8;
    // A refusal of unsupported input is reported even under `-q` (it is not a warning).
    let mut refused_input = false;

    if let Some(flags) = options.info {
        if flags.header && !flags.convert_only {
            writeln!(stdout, "{}", info_header(flags))?;
        }
        let terminator: &[u8] = if flags.nul { b"\0" } else { b"\n" };
        for job in &options.jobs {
            let (name, bytes) = match job {
                Job::Stdin => {
                    let mut bytes = Vec::new();
                    context.stdin().read_to_end(&mut bytes)?;
                    (None, bytes)
                }
                Job::Old(name) | Job::New(name, _) => {
                    let path = context.shell.absolute_path(Path::new(name));
                    match load(&path, name, false, &mut diagnostics)? {
                        Loaded::Bytes(bytes, _) => (Some(name.as_str()), bytes),
                        Loaded::Skipped => continue,
                        Loaded::Failed(code) => {
                            status = code;
                            continue;
                        }
                    }
                }
            };
            if let Some(line) = info_line(&options, flags, name, &bytes) {
                stdout.write_all(line.as_bytes())?;
                stdout.write_all(terminator)?;
            }
        }
        return Ok(finish(&options, status, refused_input));
    }

    for job in &options.jobs {
        let (name, input, modified) = match job {
            Job::Stdin => {
                let mut bytes = Vec::new();
                context.stdin().read_to_end(&mut bytes)?;
                ("stdin".to_string(), bytes, None)
            }
            Job::Old(name) | Job::New(name, _) => {
                let path = context.shell.absolute_path(Path::new(name));
                let skip_symlinks = matches!(job, Job::Old(_)) && !options.to_stdout;
                match load(&path, name, skip_symlinks, &mut diagnostics)? {
                    Loaded::Bytes(bytes, modified) => (name.clone(), bytes, modified),
                    Loaded::Skipped => continue,
                    Loaded::Failed(code) => {
                        status = code;
                        continue;
                    }
                }
            }
        };

        let converted = match convert(&options, &input) {
            Ok(converted) => converted,
            Err(Refusal::Binary { byte, line }) => {
                diagnostics.warn(&format!("Binary symbol 0x{byte:02X} found at line {line}"))?;
                diagnostics.warn(&format!("Skipping binary file {name}"))?;
                if *job == Job::Stdin || options.error_binary {
                    status = 1;
                }
                continue;
            }
            Err(Refusal::Utf16) => {
                writeln!(
                    diagnostics.writer,
                    "{}",
                    refused(tool, &format!("{name} is UTF-16"), "UTF-16 conversion")
                )?;
                refused_input = true;
                continue;
            }
        };

        if converted.had_bom {
            diagnostics.verbose(&format!("Input file {name} has UTF-8 BOM."))?;
        }
        if converted.wrote_bom {
            diagnostics.verbose("Writing UTF-8 BOM.")?;
        }
        diagnostics.verbose(&format!(
            "Converted {} out of {} line breaks.",
            converted.changed, converted.total
        ))?;

        let format = direction.format();
        let written = match job {
            Job::Stdin => {
                stdout.write_all(&converted.bytes)?;
                Ok(())
            }
            Job::Old(_) if options.to_stdout => {
                diagnostics.warn(&format!("converting file {name} to {format} format..."))?;
                stdout.write_all(&converted.bytes)?;
                Ok(())
            }
            Job::Old(_) => {
                diagnostics.warn(&format!("converting file {name} to {format} format..."))?;
                let path = context.shell.absolute_path(Path::new(&name));
                write_replacing(
                    &path,
                    &converted.bytes,
                    modified.filter(|_| options.keep_date),
                )
                .map_err(|e| (name.clone(), e))
            }
            Job::New(_, output) => {
                diagnostics.warn(&format!(
                    "converting file {name} to file {output} in {format} format..."
                ))?;
                let path = context.shell.absolute_path(Path::new(output));
                write_replacing(
                    &path,
                    &converted.bytes,
                    modified.filter(|_| options.keep_date),
                )
                .map_err(|e| (output.clone(), e))
            }
        };
        if let Err((target, e)) = written {
            diagnostics.warn(&format!("Failed to write {target}: {}", describe(&e)))?;
            status = errno(&e);
        }
    }

    Ok(finish(&options, status, refused_input))
}

/// The manual: "The return value is always zero in quiet mode, except when wrong
/// command-line options are used." A refusal of unsupported input is not quieted.
fn finish(options: &Options, status: u8, refused_input: bool) -> ExecutionResult {
    if refused_input {
        ExecutionResult::general_error()
    } else if options.verbosity == Verbosity::Quiet {
        ExecutionResult::success()
    } else {
        ExecutionResult::new(status)
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod tests {
    use super::*;

    fn options(direction: Direction, args: &[&str]) -> Options {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        match parse(direction, &args) {
            Parsed::Run(options) => options,
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    fn to_unix(args: &[&str], input: &[u8]) -> Vec<u8> {
        convert(&options(Direction::ToUnix, args), input)
            .map(|c| c.bytes)
            .unwrap_or_default()
    }

    fn to_dos(args: &[&str], input: &[u8]) -> Vec<u8> {
        convert(&options(Direction::ToDos, args), input)
            .map(|c| c.bytes)
            .unwrap_or_default()
    }

    #[test]
    fn dos2unix_converts_only_crlf() {
        assert_eq!(to_unix(&[], b"a\r\nb\nc\rd\r\r\n"), b"a\nb\nc\rd\r\n");
    }

    #[test]
    fn unix2dos_does_not_double_an_existing_cr() {
        assert_eq!(to_dos(&[], b"a\r\nb\nc\rd\r\r\n"), b"a\r\nb\r\nc\rd\r\r\n");
    }

    #[test]
    fn bom_defaults_differ_per_tool() {
        assert_eq!(to_unix(&[], b"\xEF\xBB\xBFa\r\n"), b"a\n");
        assert_eq!(to_unix(&["-b"], b"\xEF\xBB\xBFa\r\n"), b"\xEF\xBB\xBFa\n");
        assert_eq!(to_dos(&[], b"\xEF\xBB\xBFa\n"), b"\xEF\xBB\xBFa\r\n");
        assert_eq!(to_dos(&["-r"], b"\xEF\xBB\xBFa\n"), b"a\r\n");
        assert_eq!(to_dos(&["-m"], b"a\n"), b"\xEF\xBB\xBFa\r\n");
        assert_eq!(to_unix(&["-b", "-r"], b"\xEF\xBB\xBFa\r\n"), b"a\n");
    }

    #[test]
    fn eol_newline_and_seven_bit() {
        assert_eq!(to_unix(&["-e"], b"a\r\nb"), b"a\nb\n");
        assert_eq!(to_dos(&["-e"], b"a\r\nb"), b"a\r\nb\r\n");
        assert_eq!(to_unix(&["-l"], b"a\r\nb\r\n"), b"a\n\nb\n\n");
        assert_eq!(to_unix(&["-7"], b"a\xC3\xA9\r\n"), b"a  \n");
    }

    #[test]
    fn binary_is_refused_unless_forced() {
        let refusal = convert(&options(Direction::ToUnix, &[]), b"a\r\n\x1B\r\n");
        assert_eq!(
            refusal.err(),
            Some(Refusal::Binary {
                byte: 0x1B,
                line: 2
            })
        );
        assert_eq!(to_unix(&["-f"], b"a\r\n\0\r\n"), b"a\n\0\n");
        assert_eq!(to_unix(&[], b"a\t\x0C\r\n"), b"a\t\x0C\n");
    }

    #[test]
    fn utf16_input_is_refused() {
        let refusal = convert(&options(Direction::ToUnix, &["-f"]), b"\xFF\xFEa\0");
        assert_eq!(refusal.err(), Some(Refusal::Utf16));
    }

    #[test]
    fn operands_follow_the_mode_they_appear_in() {
        let parsed = options(
            Direction::ToUnix,
            &["-n", "a", "b", "-o", "c", "-", "--", "-x"],
        );
        assert_eq!(
            parsed.jobs,
            vec![
                Job::New("a".into(), "b".into()),
                Job::Old("c".into()),
                Job::Stdin,
                Job::Old("-x".into()),
            ]
        );
        assert_eq!(options(Direction::ToUnix, &["-q"]).jobs, vec![Job::Stdin]);
    }

    #[test]
    fn unsupported_options_are_refused() {
        for args in [
            &["-iso"][..],
            &["-437"],
            &["-ul"],
            &["-c", "mac"],
            &["-kq"],
            &["-n", "a"],
        ] {
            let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            assert!(
                matches!(parse(Direction::ToUnix, &args), Parsed::Fail(_)),
                "{args:?} was accepted"
            );
        }
    }

    #[test]
    fn info_counts_match_the_real_tool() {
        let info = analyse(b"\xEF\xBB\xBFa\r\nb\nc\rd");
        assert_eq!(
            info,
            Info {
                dos: 1,
                unix: 1,
                mac: 1,
                bom: "UTF-8",
                binary: false,
                last: "noeol",
            }
        );
        let flags = InfoFlags::parse("dos2unix", "").unwrap_or_default();
        let options = options(Direction::ToUnix, &["-i"]);
        assert_eq!(
            info_line(&options, flags, Some("i1"), b"\xEF\xBB\xBFa\r\nb\nc\rd").as_deref(),
            Some("       1       1       1  UTF-8     text    i1")
        );
        assert_eq!(
            info_header(flags),
            "     DOS    UNIX     MAC  BOM       TXTBIN  FILE"
        );
    }
}
