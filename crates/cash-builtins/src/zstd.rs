//! `zstd`, `unzstd` and `zstdcat`: zstd 1.5.7's interface, messages and exit codes, on
//! ruzstd.
//!
//! Checked against zstd 1.5.7 (`crates/cash/tests/oracle/zstd_cases.sh`): every
//! message and status, the decompressed bytes, the summaries and `-l`'s tables (of
//! files zstd made). The compressed bytes are ruzstd's, at its fast level: a .zst cash
//! makes is not byte for byte zstd's, and every zstd reads it.
//!
//! On a clean Windows machine there is no `zstd`; `tar.exe` reads a `.tar.zst`, not a
//! bare `.zst`.
//!
//! Where cash differs, on purpose:
//! - The version line, also at the top of `-v` and `-H`, is cash's.
//! - Every level, `-1` to `-22` and `--fast`, compresses at ruzstd's fast level, about
//!   zstd's level 1; the levels are read, checked and reported as zstd does. `--long`,
//!   `--adapt`, `--rsyncable`, `-B`, the memory limit and zstd's other tuning are read
//!   and change nothing. `-T`, `--single-thread` and `ZSTD_NBTHREADS` are zstd's: its
//!   4 MiB frames are compressed that many at once, the same bytes for any number;
//!   without them, every core, where zstd uses one (the user's choice, 2026-10-07).
//! - Dictionaries (`-D`, `--train`, `--patch-from`), the benchmark (`-b`) and lz4 are
//!   refused.
//! - `-vv` says what `-v` says, and no progress counter is drawn at a console.
//! - Windows has no mode bits but read-only, which is carried over; no owner is set.
//! - A file is written beside its target under a temporary name and renamed over it at
//!   the end, so an interrupted run leaves no half-written file.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use cash_archive::codec::xz::{self, Format};
use cash_archive::codec::zstd::{FrameError, decompress_frame};
use cash_archive::codec::{self, Codec, Lookahead};
use cash_core::openfiles::OpenFiles;
use cash_core::{ExecutionResult, builtins};
use cash_win32::unix::Replacement;
use clap::Parser;

use crate::compress::{answer_is_yes, is_terminal, strerror, times_of};

/// The version line, also the banner of `-v` and `-H`.
const VERSION: &str = "*** zstd (cash): zstd v1.5.7's options, on ruzstd ***";

/// The compressed suffixes zstd knows when it decompresses, in its order.
const SUFFIXES: [&str; 9] = [
    ".zst", ".tzst", ".gz", ".tgz", ".lzma", ".xz", ".txz", ".lz4", ".tlz4",
];

/// The suffixes `--exclude-compressed` passes over.
const COMPRESSED: [&str; 12] = [
    ".zst", ".tzst", ".gz", ".tgz", ".lzma", ".xz", ".txz", ".lz4", ".tlz4", ".zstd", ".tlz", ".lz",
];

/// The names zstd shows for standard input and output.
const STDIN_MARK: &str = "/*stdin*\\";
const STDOUT_MARK: &str = "/*stdout*\\";

/// The levels zstd allows without `--ultra`, and with it.
const MAX_LEVEL: i64 = 19;
const ULTRA_LEVEL: i64 = 22;

/// glibc's buffer for standard output on a pipe: what zstd says on standard error comes
/// before the data still in it.
const STDOUT_BUFFER: usize = 4096;

/// Compress or decompress files with zstd's options.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ZstdCommand {
    /// Options and files, read here as zstd reads them.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Decompress files: `zstd -d`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct UnzstdCommand {
    /// Options and files, read here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Decompress files to standard output: `zstd -dcf`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct ZstdcatCommand {
    /// Options and files, read here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

macro_rules! command {
    ($type:ty, $name:literal) => {
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
                run($name, &self.args, &context)
            }
        }
    };
}

command!(ZstdCommand, "zstd");
command!(UnzstdCommand, "unzstd");
command!(ZstdcatCommand, "zstdcat");

/// What is done to the files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Operation {
    Compress,
    Decompress,
    Test,
    List,
    Bench,
    Train,
}

/// The format compressed to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Zstd,
    Gzip,
    Xz,
    Lzma,
    Lz4,
}

impl Kind {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Zstd => ".zst",
            Self::Gzip => ".gz",
            Self::Xz => ".xz",
            Self::Lzma => ".lzma",
            Self::Lz4 => ".lz4",
        }
    }
}

/// Where the output goes, when it is not beside each file.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Out {
    /// Beside each file, its name made from the input's.
    Beside,
    Stdout,
    /// Nowhere: `-t`.
    Null,
    /// One file, `-o`.
    File(String),
}

#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "zstd's own switches, one each"
)]
struct Options {
    operation: Operation,
    kind: Kind,
    out: Out,
    force_stdout: bool,
    force_stdin: bool,
    overwrite: bool,
    follow_links: bool,
    remove: bool,
    display: i32,
    level: i64,
    ultra: bool,
    /// zstd's checksum flag: added and checked, or not.
    check: bool,
    /// `--[no-]pass-through`; unset, it is on for `-c -f`.
    pass_through: Option<bool>,
    recursive: bool,
    exclude_compressed: bool,
    out_dir_flat: Option<String>,
    out_dir_mirror: Option<String>,
    file_lists: Vec<String>,
    files: Vec<String>,
    /// Asked for and refused: dictionaries and `--patch-from`.
    dictionary: Option<&'static str>,
    window_log: Option<u32>,
    /// `-T#`, `--threads=#`, `--single-thread`: the frames compressed at once, 0 for one
    /// a core; unset, `ZSTD_NBTHREADS`, else one.
    threads: Option<u32>,
}

/// What reading the command line came to.
#[derive(Debug)]
enum Parsed {
    /// The command is over: this on standard output, this on standard error, the
    /// status.
    Exit(String, String, u8),
}

fn usage(name: &str) -> String {
    format!(
        "Compress or decompress the INPUT file(s); reads from STDIN if INPUT is `-` or not provided.\n\n\
         Usage: {name} [OPTIONS...] [INPUT... | -] [-o OUTPUT]\n\n\
         Options:\n\
         \x20 -o OUTPUT                     Write output to a single file, OUTPUT.\n\
         \x20 -k, --keep                    Preserve INPUT file(s). [Default] \n\
         \x20 --rm                          Remove INPUT file(s) after successful (de)compression.\n\
         \n\
         \x20 -#                            Desired compression level, where `#` is a number between 1 and 19;\n\
         \x20                               lower numbers provide faster compression, higher numbers yield\n\
         \x20                               better compression ratios. [Default: 3]\n\n\
         \x20 -d, --decompress              Perform decompression.\n\
         \x20 -D DICT                       Use DICT as the dictionary for compression or decompression.\n\n\
         \x20 -f, --force                   Disable input and output checks. Allows overwriting existing files,\n\
         \x20                               receiving input from the console, printing output to STDOUT, and\n\
         \x20                               operating on links, block devices, etc. Unrecognized formats will be\n\
         \x20                               passed-through through as-is.\n\n\
         \x20 -h                            Display short usage and exit.\n\
         \x20 -H, --help                    Display full help and exit.\n\
         \x20 -V, --version                 Display the program version and exit.\n\
         \n"
    )
}

/// zstd 1.5.7's advanced options, as `-H` lists them after the usage.
const ADVANCED_HELP: &str = concat!(
    "Advanced options:\n",
    "  -c, --stdout                  Write to STDOUT (even if it is a console) and keep the INPUT file(s).\n",
    "\n",
    "  -v, --verbose                 Enable verbose output; pass multiple times to increase verbosity.\n",
    "  -q, --quiet                   Suppress warnings; pass twice to suppress errors.\n",
    "  --trace LOG                   Log tracing information to LOG.\n",
    "\n",
    "  --[no-]progress               Forcibly show/hide the progress counter. NOTE: Any (de)compressed\n",
    "                                output to terminal will mix with progress counter text.\n",
    "\n",
    "  -r                            Operate recursively on directories.\n",
    "  --filelist LIST               Read a list of files to operate on from LIST.\n",
    "  --output-dir-flat DIR         Store processed files in DIR.\n",
    "  --output-dir-mirror DIR       Store processed files in DIR, respecting original directory structure.\n",
    "  --[no-]asyncio                Use asynchronous IO. [Default: Enabled]\n",
    "\n",
    "  --[no-]check                  Add XXH64 integrity checksums during compression. [Default: Add, Validate]\n",
    "                                If `-d` is present, ignore/validate checksums during decompression.\n",
    "\n",
    "  --                            Treat remaining arguments after `--` as files.\n",
    "\n",
    "Advanced compression options:\n",
    "  --ultra                       Enable levels beyond 19, up to 22; requires more memory.\n",
    "  --fast[=#]                    Use to very fast compression levels. [Default: 1]\n",
    "  --adapt                       Dynamically adapt compression level to I/O conditions.\n",
    "  --long[=#]                    Enable long distance matching with window log #. [Default: 27]\n",
    "  --patch-from=REF              Use REF as the reference point for Zstandard's diff engine. \n",
    "\n",
    "  -T#                           Spawn # compression threads. [Default: 0, one per core; pass 1 for one.]\n",
    "  --single-thread               Share a single thread for I/O and compression (slightly different than `-T1`).\n",
    "  --auto-threads={physical|logical}\n",
    "                                Use physical/logical cores when using `-T0`. [Default: Physical]\n",
    "\n",
    "  -B#                           Set job size to #. [Default: 0 (automatic)]\n",
    "  --rsyncable                   Compress using a rsync-friendly method (`-B` sets block size). \n",
    "\n",
    "  --exclude-compressed          Only compress files that are not already compressed.\n",
    "\n",
    "  --stream-size=#               Specify size of streaming input from STDIN.\n",
    "  --size-hint=#                 Optimize compression parameters for streaming input of approximately size #.\n",
    "  --target-compressed-block-size=#\n",
    "                                Generate compressed blocks of approximately # size.\n",
    "\n",
    "  --no-dictID                   Don't write `dictID` into the header (dictionary compression only).\n",
    "  --[no-]compress-literals      Force (un)compressed literals.\n",
    "  --[no-]row-match-finder       Explicitly enable/disable the fast, row-based matchfinder for\n",
    "                                the 'greedy', 'lazy', and 'lazy2' strategies.\n",
    "\n",
    "  --format=zstd                 Compress files to the `.zst` format. [Default]\n",
    "  --[no-]mmap-dict              Memory-map dictionary file rather than mallocing and loading all at once\n",
    "  --format=gzip                 Compress files to the `.gz` format.\n",
    "  --format=xz                   Compress files to the `.xz` format.\n",
    "  --format=lzma                 Compress files to the `.lzma` format.\n",
    "  --format=lz4                 Compress files to the `.lz4` format.\n",
    "\n",
    "Advanced decompression options:\n",
    "  -l                            Print information about Zstandard-compressed files.\n",
    "  --test                        Test compressed file integrity.\n",
    "  -M#                           Set the memory usage limit to # megabytes.\n",
    "  --[no-]sparse                 Enable sparse mode. [Default: Enabled for files, disabled for STDOUT.]\n",
    "  --[no-]pass-through           Pass through uncompressed files as-is. [Default: Disabled]\n",
    "\n",
    "Dictionary builder:\n",
    "  --train                       Create a dictionary from a training set of files.\n",
    "\n",
    "  --train-cover[=k=#,d=#,steps=#,split=#,shrink[=#]]\n",
    "                                Use the cover algorithm (with optional arguments).\n",
    "  --train-fastcover[=k=#,d=#,f=#,steps=#,split=#,accel=#,shrink[=#]]\n",
    "                                Use the fast cover algorithm (with optional arguments).\n",
    "\n",
    "  --train-legacy[=s=#]          Use the legacy algorithm with selectivity #. [Default: 9]\n",
    "  -o NAME                       Use NAME as dictionary name. [Default: dictionary]\n",
    "  --maxdict=#                   Limit dictionary to specified size #. [Default: 112640]\n",
    "  --dictID=#                    Force dictionary ID to #. [Default: Random]\n",
    "\n",
    "Benchmark options:\n",
    "  -b#                           Perform benchmarking with compression level #. [Default: 3]\n",
    "  -e#                           Test all compression levels up to #; starting level is `-b#`. [Default: 1]\n",
    "  -i#                           Set the minimum evaluation to time # seconds. [Default: 3]\n",
    "  -B#                           Cut file into independent chunks of size #. [Default: No chunking]\n",
    "  -S                            Output one benchmark result per input file. [Default: Consolidated result]\n",
    "  -D dictionary                 Benchmark using dictionary \n",
    "  --priority=rt                 Set process priority to real-time.\n",
);

/// zstd's `badUsage`: the parameter, and the usage when not `-q`.
fn bad_usage(name: &str, parameter: &str, display: i32) -> Parsed {
    let mut said = String::new();
    if display >= 1 {
        let _ = writeln!(said, "Incorrect parameter: {parameter} ");
    }
    if display >= 2 {
        said.push_str(&usage(name));
    }
    Parsed::Exit(String::new(), said, 1)
}

/// zstd's `errorOut`: the message, status 1.
fn error_out(message: &str, display: i32) -> Parsed {
    let said = if display >= 1 {
        format!("{message} \n")
    } else {
        String::new()
    };
    Parsed::Exit(String::new(), said, 1)
}

/// zstd's `readU32FromChar`: digits with an optional K or M (and `i`, `B`), and what
/// follows them.
fn read_u32(text: &str) -> Result<(u32, &str), ()> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let mut value: u32 = if digits == 0 {
        0
    } else {
        text.get(..digits).and_then(|d| d.parse().ok()).ok_or(())?
    };
    let mut rest = text.get(digits..).unwrap_or_default();
    if let Some(unit) = rest.chars().next().filter(|c| matches!(c, 'K' | 'M')) {
        let shift = if unit == 'K' { 10 } else { 20 };
        value = value.checked_mul(1 << shift).ok_or(())?;
        rest = rest.get(1..).unwrap_or_default();
        rest = rest.strip_prefix('i').unwrap_or(rest);
        rest = rest.strip_prefix('B').unwrap_or(rest);
    }
    Ok((value, rest))
}

/// The words of the command line, read one at a time as zstd's main loop reads them.
struct Words<'w> {
    words: &'w [String],
    at: usize,
}

impl Words<'_> {
    /// zstd's `NEXT_FIELD`: the value after `=`, or the next word, which may not be an
    /// option.
    fn field(&mut self, rest: &str, display: i32) -> Result<String, Parsed> {
        if let Some(value) = rest.strip_prefix('=') {
            return Ok(value.to_owned());
        }
        self.at += 1;
        let Some(next) = self.words.get(self.at) else {
            return Err(error_out("error: missing command argument", display));
        };
        if next.starts_with('-') {
            return Err(error_out(
                "error: command cannot be separated from its argument by another command",
                display,
            ));
        }
        Ok(next.clone())
    }

    /// zstd's `NEXT_UINT32`.
    fn number(&mut self, rest: &str, display: i32) -> Result<u32, Parsed> {
        let field = self.field(rest, display)?;
        match read_u32(&field) {
            Ok((value, "")) => Ok(value),
            Ok(_) => Err(error_out(
                "error: only numeric values with optional suffixes K, KB, KiB, M, MB, MiB are allowed",
                display,
            )),
            Err(()) => Err(error_out(
                "error: numeric value overflows 32-bit unsigned int",
                display,
            )),
        }
    }
}

/// The options zstd's name sets before the command line is read.
fn preset(name: &str) -> Options {
    let mut options = Options {
        operation: Operation::Compress,
        kind: Kind::Zstd,
        out: Out::Beside,
        force_stdout: false,
        force_stdin: false,
        overwrite: false,
        follow_links: false,
        remove: false,
        display: 2,
        level: 3,
        ultra: false,
        check: true,
        pass_through: None,
        recursive: false,
        exclude_compressed: false,
        out_dir_flat: None,
        out_dir_mirror: None,
        file_lists: Vec::new(),
        files: Vec::new(),
        dictionary: None,
        window_log: None,
        threads: None,
    };
    if name == "unzstd" {
        options.operation = Operation::Decompress;
    }
    if name == "zstdcat" {
        options.operation = Operation::Decompress;
        options.overwrite = true;
        options.force_stdout = true;
        options.follow_links = true;
        options.pass_through = Some(true);
        options.out = Out::Stdout;
        options.display = 1;
    }
    options
}

/// One long option of zstd's, matched whole; `None` when it is not one of them.
fn long_switch(options: &mut Options, word: &str) -> Option<Result<(), Parsed>> {
    let o = options;
    match word {
        "--list" => o.operation = Operation::List,
        "--compress" => o.operation = Operation::Compress,
        "--decompress" | "--uncompress" => o.operation = Operation::Decompress,
        "--force" => {
            o.overwrite = true;
            o.force_stdin = true;
            o.force_stdout = true;
            o.follow_links = true;
        }
        "--version" => return Some(Err(Parsed::Exit(format!("{VERSION}\n"), String::new(), 0))),
        "--verbose" => o.display += 1,
        "--quiet" => o.display -= 1,
        "--stdout" => {
            o.force_stdout = true;
            o.out = Out::Stdout;
        }
        "--ultra" => o.ultra = true,
        "--check" => o.check = true,
        "--no-check" => o.check = false,
        "--pass-through" => o.pass_through = Some(true),
        // zstd's single-thread mode: the frames one after the other.
        "--single-thread" => o.threads = Some(1),
        "--no-pass-through" => o.pass_through = Some(false),
        "--test" => o.operation = Operation::Test,
        "--train" => o.operation = Operation::Train,
        "--keep" => o.remove = false,
        "--rm" => o.remove = true,
        "--adapt"
        | "--sparse"
        | "--no-sparse"
        | "--asyncio"
        | "--no-asyncio"
        | "--no-dictID"
        | "--priority=rt"
        | "--show-default-cparams"
        | "--content-size"
        | "--no-content-size"
        | "--no-row-match-finder"
        | "--row-match-finder"
        | "--mmap-dict"
        | "--no-mmap-dict"
        | "--rsyncable"
        | "--compress-literals"
        | "--no-compress-literals"
        | "--no-progress"
        | "--progress"
        | "--max" => {}
        "--format=zstd" => o.kind = Kind::Zstd,
        "--format=gzip" => o.kind = Kind::Gzip,
        "--format=xz" => o.kind = Kind::Xz,
        "--format=lzma" => o.kind = Kind::Lzma,
        "--format=lz4" => o.kind = Kind::Lz4,
        "--exclude-compressed" => o.exclude_compressed = true,
        _ => return None,
    }
    Some(Ok(()))
}

/// The long options that take a value, matched by their start as zstd matches them.
#[expect(
    clippy::too_many_lines,
    reason = "zstd's long options with values, one case each, in its order"
)]
fn long_with_value(
    name: &str,
    options: &mut Options,
    words: &mut Words<'_>,
    word: &str,
) -> Result<(), Parsed> {
    let display = options.display;
    if let Some(rest) = word.strip_prefix("--threads") {
        options.threads = Some(words.number(rest, display)?);
        return Ok(());
    }
    let numeric = [
        "--memlimit",
        "--memory",
        "--memlimit-decompress",
        "--block-size",
        "--split",
        "--jobsize",
        "--maxdict",
        "--dictID",
    ];
    if word.starts_with("--adapt=") {
        options.display = display;
        return Ok(());
    }
    if let Some(train) = ["--train-cover", "--train-fastcover", "--train-legacy"]
        .iter()
        .find(|prefix| word.starts_with(**prefix))
    {
        let _ = train;
        options.operation = Operation::Train;
        return Ok(());
    }
    if let Some(prefix) = numeric.iter().find(|prefix| word.starts_with(**prefix)) {
        words.number(word.get(prefix.len()..).unwrap_or_default(), display)?;
        return Ok(());
    }
    if word.starts_with("--zstd=") {
        return Ok(());
    }
    for prefix in [
        "--stream-size",
        "--target-compressed-block-size",
        "--size-hint",
    ] {
        if let Some(rest) = word.strip_prefix(prefix) {
            words.number(rest, display)?;
            return Ok(());
        }
    }
    if let Some(rest) = word.strip_prefix("--output-dir-flat") {
        let dir = words.field(rest, display)?;
        if dir.is_empty() {
            return Err(error_out(
                "error: output dir cannot be empty string (did you mean to pass '.' instead?)",
                display,
            ));
        }
        options.out_dir_flat = Some(dir);
        return Ok(());
    }
    if let Some(rest) = word.strip_prefix("--auto-threads") {
        words.field(rest, display)?;
        return Ok(());
    }
    if let Some(rest) = word.strip_prefix("--output-dir-mirror") {
        let dir = words.field(rest, display)?;
        if dir.is_empty() {
            return Err(error_out(
                "error: output dir cannot be empty string (did you mean to pass '.' instead?)",
                display,
            ));
        }
        options.out_dir_mirror = Some(dir);
        return Ok(());
    }
    if let Some(rest) = word.strip_prefix("--trace") {
        words.field(rest, display)?;
        return Ok(());
    }
    if let Some(rest) = word.strip_prefix("--patch-from") {
        words.field(rest, display)?;
        options.dictionary = Some("--patch-from");
        return Ok(());
    }
    if let Some(rest) = word.strip_prefix("--long") {
        options.ultra = true;
        options.window_log = Some(if let Some(value) = rest.strip_prefix('=') {
            read_u32(value).map_or(0, |(value, _)| value)
        } else if rest.is_empty() {
            27
        } else {
            return Err(bad_usage(name, word, display));
        });
        return Ok(());
    }
    if let Some(rest) = word.strip_prefix("--fast") {
        if let Some(value) = rest.strip_prefix('=') {
            let fast = read_u32(value).map_or(0, |(value, _)| value.min(131_072));
            if fast == 0 {
                return Err(bad_usage(name, word, display));
            }
            options.level = -i64::from(fast);
        } else if rest.is_empty() {
            options.level = -1;
        } else {
            return Err(bad_usage(name, word, display));
        }
        return Ok(());
    }
    if let Some(rest) = word.strip_prefix("--filelist") {
        let list = words.field(rest, display)?;
        options.file_lists.push(list);
        return Ok(());
    }
    Err(bad_usage(name, word, display))
}

/// The letters of one `-...` word, read as zstd reads them, aggregated.
fn short_options(
    name: &str,
    options: &mut Options,
    words: &mut Words<'_>,
    letters: &str,
) -> Result<(), Parsed> {
    let mut rest = letters;
    while let Some(letter) = rest.chars().next() {
        let display = options.display;
        if letter.is_ascii_digit() {
            let (level, after) = read_u32(rest).map_err(|()| {
                error_out(
                    "error: numeric value overflows 32-bit unsigned int",
                    display,
                )
            })?;
            options.level = i64::from(level);
            rest = after;
            continue;
        }
        rest = rest.get(letter.len_utf8()..).unwrap_or_default();
        let number = |rest: &mut &str| -> Result<u32, Parsed> {
            let (value, after) = read_u32(rest).map_err(|()| {
                error_out(
                    "error: numeric value overflows 32-bit unsigned int",
                    display,
                )
            })?;
            *rest = after;
            Ok(value)
        };
        match letter {
            'V' => return Err(Parsed::Exit(format!("{VERSION}\n"), String::new(), 0)),
            'H' => {
                return Err(Parsed::Exit(
                    format!("{VERSION}\n\n{}{ADVANCED_HELP}", usage(name)),
                    String::new(),
                    0,
                ));
            }
            'h' => return Err(Parsed::Exit(usage(name), String::new(), 0)),
            'z' => options.operation = Operation::Compress,
            'd' => options.operation = Operation::Decompress,
            'c' => {
                options.force_stdout = true;
                options.out = Out::Stdout;
                options.remove = false;
            }
            'o' => {
                let field = words.field(rest, display)?;
                if rest.starts_with('=') {
                    rest = "";
                }
                options.out = Out::File(field);
            }
            'n' | 'S' => {}
            'D' => {
                words.field(rest, display)?;
                if rest.starts_with('=') {
                    rest = "";
                }
                options.dictionary = Some("-D");
            }
            'f' => {
                options.overwrite = true;
                options.force_stdin = true;
                options.force_stdout = true;
                options.follow_links = true;
            }
            'v' => options.display += 1,
            'q' => options.display -= 1,
            'k' => options.remove = false,
            'C' => options.check = true,
            't' => options.operation = Operation::Test,
            'l' => options.operation = Operation::List,
            'r' => options.recursive = true,
            'b' => options.operation = Operation::Bench,
            'T' => options.threads = Some(number(&mut rest)?),
            'M' | 'e' | 'i' | 'B' | 's' | 'P' => {
                number(&mut rest)?;
            }
            'p' => {
                if rest.starts_with(|c: char| c.is_ascii_digit()) {
                    number(&mut rest)?;
                }
            }
            _ => return Err(bad_usage(name, &format!("-{letter}"), display)),
        }
    }
    Ok(())
}

/// zstd's main loop over the words.
fn parse(name: &str, options: &mut Options, words: &[String]) -> Result<(), Parsed> {
    let mut reader = Words { words, at: 0 };
    let mut files_only = false;
    while let Some(word) = reader.words.get(reader.at).cloned() {
        if files_only {
            options.files.push(word);
        } else if word == "-" {
            options.files.push(STDIN_MARK.to_owned());
        } else if word == "--" {
            files_only = true;
        } else if word.starts_with("--") {
            match long_switch(options, &word) {
                Some(outcome) => outcome?,
                None if word == "--help" => {
                    return Err(Parsed::Exit(
                        format!("{VERSION}\n\n{}{ADVANCED_HELP}", usage(name)),
                        String::new(),
                        0,
                    ));
                }
                None => long_with_value(name, options, &mut reader, &word)?,
            }
        } else if let Some(letters) = word.strip_prefix('-') {
            short_options(name, options, &mut reader, letters)?;
        } else {
            options.files.push(word);
        }
        reader.at += 1;
    }
    Ok(())
}

/// zstd's `init_cLevel`: the environment's level, and what is said of a value it cannot
/// use.
fn environment_level(level: Option<&str>, said: &mut String) -> i64 {
    let mut result = 3;
    if let Some(value) = level {
        let (sign, digits) = match value.strip_prefix('-') {
            Some(rest) => (-1, rest),
            None => (1, value.strip_prefix('+').unwrap_or(value)),
        };
        match read_u32(digits) {
            Ok((number, "")) if digits.starts_with(|c: char| c.is_ascii_digit()) => {
                result = sign * i64::from(number);
            }
            Err(()) if digits.starts_with(|c: char| c.is_ascii_digit()) => {
                let _ = writeln!(
                    said,
                    "Ignore environment variable setting ZSTD_CLEVEL={value}: numeric value too large "
                );
            }
            _ => {
                let _ = writeln!(
                    said,
                    "Ignore environment variable setting ZSTD_CLEVEL={value}: not a valid integer value "
                );
            }
        }
    }
    result
}

/// zstd's `init_nbWorkers`: what is said of a thread count it cannot use.
fn environment_threads(threads: Option<&str>) -> Option<String> {
    let value = threads?;
    (value.is_empty() || !matches!(read_u32(value), Ok((_, "")))).then(|| {
        format!("Ignore environment variable setting ZSTD_NBTHREADS={value}: not a valid unsigned value \n")
    })
}

/// What `-l` reads of a file, as zstd's `fileInfo_t` holds it.
#[derive(Debug, Default, Clone)]
struct Info {
    frames: u64,
    skippable: u64,
    compressed: u64,
    decompressed: u64,
    /// Some frame did not say its size.
    size_unknown: bool,
    uses_check: bool,
    checksum: [u8; 4],
    window: u64,
    dict_id: u32,
    files: u64,
}

/// Why `-l` gave up on a file.
#[derive(Debug, Eq, PartialEq)]
enum InfoError {
    /// Said, and the information so far still shown.
    Frame,
    NotZstd,
    Truncated,
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

fn le_bytes(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .rev()
        .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte))
}

/// Reads up to `buffer.len()` bytes, as C's `fread` does.
fn fread(file: &mut fs::File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut got = 0;
    while got < buffer.len() {
        match file.read(buffer.get_mut(got..).unwrap_or_default()) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

/// zstd's `ERROR_IF`: `text` said, `error` returned.
fn fail(said: &mut String, text: &str, error: InfoError) -> InfoError {
    let _ = writeln!(said, "{text} ");
    error
}

/// The blocks of a frame passed over, from their headers.
fn skip_blocks(file: &mut fs::File, said: &mut String) -> Result<(), InfoError> {
    use std::io::{Seek, SeekFrom};
    loop {
        let mut block = [0_u8; 3];
        if fread(file, &mut block).unwrap_or(0) != 3 {
            return Err(fail(
                said,
                "Error while reading block header",
                InfoError::Frame,
            ));
        }
        let value = u32::from(block[0]) | u32::from(block[1]) << 8 | u32::from(block[2]) << 16;
        let kind = (value >> 1) & 3;
        if kind == 3 {
            return Err(fail(
                said,
                "Error: unsupported block type",
                InfoError::Frame,
            ));
        }
        let size = if kind == 1 { 1 } else { i64::from(value >> 3) };
        if file.seek(SeekFrom::Current(size)).is_err() {
            return Err(fail(
                said,
                "Error: could not skip to end of block",
                InfoError::Frame,
            ));
        }
        if value & 1 != 0 {
            return Ok(());
        }
    }
}

/// A zstd frame of `-l`: its header, read from the `read` bytes of `header`, its
/// blocks passed over, its checksum read.
fn zstd_frame(
    file: &mut fs::File,
    header: &[u8],
    read: usize,
    info: &mut Info,
    said: &mut String,
) -> Result<(), InfoError> {
    use std::io::{Seek, SeekFrom};
    let descriptor = header.get(4).copied().unwrap_or(0);
    let single = descriptor & 0x20 != 0;
    let dict_bytes = [0, 1, 2, 4][usize::from(descriptor & 3)];
    let size_bytes = match descriptor >> 6 {
        0 => usize::from(single),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let mut at = 5;
    let mut window = 0_u64;
    if !single {
        let byte = header.get(at).copied().unwrap_or(0);
        let base = 1_u64 << (10 + u64::from(byte >> 3));
        window = base + (base / 8) * u64::from(byte & 7);
        at += 1;
    }
    let dict_id = le_bytes(header.get(at..at + dict_bytes).unwrap_or_default());
    at += dict_bytes;
    let size_field = header.get(at..at + size_bytes).unwrap_or_default();
    let header_size = at + size_bytes;
    if descriptor & 0x08 != 0 || header_size > read {
        return Err(fail(
            said,
            "Error: could not decode frame header",
            InfoError::Frame,
        ));
    }
    if size_bytes == 0 {
        info.size_unknown = true;
    } else {
        let mut size = le_bytes(size_field);
        if size_bytes == 2 {
            size += 256;
        }
        info.decompressed += size;
        if single {
            window = size;
        }
    }
    let dict_id = u32::try_from(dict_id).unwrap_or(u32::MAX);
    if info.dict_id != 0 && info.dict_id != dict_id {
        said.push_str(
            "WARNING: File contains multiple frames with different dictionary IDs. Showing dictID 0 instead",
        );
        info.dict_id = 0;
    } else {
        info.dict_id = dict_id;
    }
    info.window = window;
    let back = i64::try_from(read - header_size).unwrap_or(0);
    if file.seek(SeekFrom::Current(-back)).is_err() {
        return Err(fail(
            said,
            "Error: could not move to end of frame header",
            InfoError::Frame,
        ));
    }
    skip_blocks(file, said)?;
    if descriptor & 0b100 != 0 {
        info.uses_check = true;
        let mut checksum = [0_u8; 4];
        if fread(file, &mut checksum).unwrap_or(0) != 4 {
            return Err(fail(
                said,
                "Error: could not read checksum",
                InfoError::Frame,
            ));
        }
        info.checksum = checksum;
    }
    info.frames += 1;
    Ok(())
}

/// zstd's `FIO_analyzeFrames`: each frame's header, its blocks passed over, its
/// checksum read; what is wrong is said on standard error.
fn analyze_frames(
    file: &mut fs::File,
    info: &mut Info,
    said: &mut String,
) -> Result<(), InfoError> {
    use std::io::{Seek, SeekFrom};
    loop {
        let mut header = [0_u8; 18];
        let read = fread(file, &mut header).unwrap_or(0);
        if read < 6 {
            if read == 0 && info.compressed > 0 {
                let position = file.stream_position().unwrap_or(0);
                if position != info.compressed {
                    return Err(fail(
                        said,
                        &format!(
                            "Error: seeked to position {position}, which is beyond file size of {}\n",
                            info.compressed
                        ),
                        InfoError::Truncated,
                    ));
                }
                return Ok(());
            }
            return Err(fail(
                said,
                "Error: reached end of file with incomplete frame",
                InfoError::NotZstd,
            ));
        }
        let magic = le32(&header, 0);
        if magic == 0xFD2F_B528 {
            zstd_frame(file, &header, read, info, said)?;
        } else if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
            if read < 8 {
                return Err(fail(
                    said,
                    "Error: reached end of file with incomplete frame",
                    InfoError::NotZstd,
                ));
            }
            let size = i64::from(le32(&header, 4));
            let skip = 8 + size - i64::try_from(read).unwrap_or(0);
            if file.seek(SeekFrom::Current(skip)).is_err() {
                return Err(fail(
                    said,
                    "Error: could not find end of skippable frame",
                    InfoError::Frame,
                ));
            }
            info.skippable += 1;
        } else {
            return Err(InfoError::NotZstd);
        }
    }
}

/// zstd's `UTIL_makeHumanReadableSize`: the value, its precision and its suffix.
#[expect(clippy::cast_precision_loss, reason = "zstd scales sizes in doubles")]
fn human(size: u64, verbose: bool) -> (f64, usize, &'static str) {
    if verbose {
        return if size >= 1 << 53 {
            (size as f64 / f64::from(1_u32 << 20), 2, " MiB")
        } else {
            (size as f64, 0, " B")
        };
    }
    let units: [(u32, &str); 6] = [
        (60, " EiB"),
        (50, " PiB"),
        (40, " TiB"),
        (30, " GiB"),
        (20, " MiB"),
        (10, " KiB"),
    ];
    let (value, suffix) = units
        .iter()
        .find(|(shift, _)| size >= 1_u64 << shift)
        .map_or((size as f64, " B"), |(shift, suffix)| {
            (size as f64 / (1_u64 << shift) as f64, *suffix)
        });
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "zstd compares the value, truncated, with the size"
    )]
    let precision = if value >= 100.0 || value as u64 == size {
        0
    } else if value >= 10.0 {
        1
    } else if value > 1.0 {
        2
    } else {
        3
    };
    (value, precision, suffix)
}

/// A size as zstd's `%6.*f%s` prints it: `width` wide, then its suffix as `suffix_width`
/// wide.
fn sized(size: u64, verbose: bool, width: usize, suffix_width: usize) -> String {
    let (value, precision, suffix) = human(size, verbose);
    format!("{value:>width$.precision$}{suffix:>suffix_width$}")
}

/// Why a run stopped before its end.
#[derive(Debug)]
enum Stop {
    /// zstd exits here with this status, everything said.
    Exit(u8),
    /// The shell's own streams failed, or the reader of standard output went away.
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

/// Whether `name` is a device, such as the pipe of a substitution file `<(...)`: its
/// one opening is its data's, so it is not looked at before it is read.
fn is_device(name: &str) -> bool {
    name.starts_with("\\\\.\\") || name.starts_with("//./")
}

/// A file opened to be read, and what it is when it is not standard input.
type Opened = (Box<dyn Read>, Option<fs::Metadata>);

/// Where a file's output is being written.
enum Sink<'s> {
    /// The run's standard output.
    Stdout,
    /// Nowhere.
    Null,
    /// A file of its own, or the one `-o` names.
    File(&'s mut Replacement),
}

/// One run of the command over its files.
struct Run<'a, SE: cash_core::ShellExtensions> {
    options: Options,
    context: &'a cash_core::ExecutionContext<'a, SE>,
    stdout: io::BufWriter<Box<dyn Write>>,
    files_total: usize,
    files_done: usize,
    bytes_in: u64,
    bytes_out: u64,
    has_stdin_input: bool,
    /// The frames compressed at once.
    threads: usize,
}

impl<SE: cash_core::ShellExtensions> Run<'_, SE> {
    /// Said at zstd's `level` of display, or not.
    fn say(&self, level: i32, text: &str) -> Result<(), Stop> {
        if self.options.display >= level {
            self.context.stderr().write_all(text.as_bytes())?;
        }
        Ok(())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.context.shell.absolute_path(name)
    }

    /// The name zstd shows in a frame's error: its last 20 characters, unless `-v`.
    fn short_name<'n>(&self, name: &'n str) -> &'n str {
        if name.len() > 20 && self.options.display < 3 {
            let mut start = name.len() - 20;
            while !name.is_char_boundary(start) {
                start += 1;
            }
            name.get(start..).unwrap_or(name)
        } else {
            name
        }
    }

    /// zstd's `FIO_openSrcFile`: the file, or `None` when it was refused, said.
    fn open_source(&self, name: &str) -> Result<Option<Opened>, Stop> {
        if name == STDIN_MARK {
            return Ok(Some((Box::new(self.context.stdin()), None)));
        }
        if is_device(name) {
            return match fs::File::open(name) {
                Ok(file) => Ok(Some((Box::new(file), None))),
                Err(e) => {
                    self.say(1, &format!("zstd: {name}: {} \n", strerror(&e)))?;
                    Ok(None)
                }
            };
        }
        let metadata = if name.is_empty() {
            Err(io::Error::from(io::ErrorKind::NotFound))
        } else {
            fs::metadata(self.path(name))
        };
        let metadata = match metadata {
            Ok(metadata) => metadata,
            Err(e) => {
                self.say(
                    1,
                    &format!("zstd: can't stat {name} : {} -- ignored \n", strerror(&e)),
                )?;
                return Ok(None);
            }
        };
        match fs::File::open(self.path(name)) {
            Ok(file) => Ok(Some((Box::new(file), Some(metadata)))),
            Err(e) => {
                self.say(1, &format!("zstd: {name}: {} \n", strerror(&e)))?;
                Ok(None)
            }
        }
    }

    /// zstd's `FIO_openDstFile` for a file: the output, or `None` when it was refused,
    /// said. An existing file is asked about, or refused under `-q`, and removed.
    fn open_output(&self, source: Option<&str>, name: &str) -> Result<Option<Replacement>, Stop> {
        let path = self.path(name);
        if let Some(source) = source.filter(|s| *s != STDIN_MARK) {
            let same = fs::canonicalize(self.path(source))
                .ok()
                .zip(fs::canonicalize(&path).ok())
                .is_some_and(|(a, b)| a == b);
            if same {
                self.say(
                    1,
                    "zstd: Refusing to open an output file which will overwrite the input file \n",
                )?;
                return Ok(None);
            }
        }
        if fs::metadata(&path).is_ok_and(|m| m.is_file()) {
            if !self.options.overwrite {
                if self.options.display <= 1 {
                    self.say(
                        1,
                        &format!("zstd: {name} already exists; not overwritten  \n"),
                    )?;
                    return Ok(None);
                }
                self.say(0, &format!("zstd: {name} already exists; "))?;
                if !self.confirm("overwrite (y/n) ? ", "Not overwritten  \n")? {
                    return Ok(None);
                }
            }
            let _ = cash_win32::unix::remove_even_read_only(&path);
        }
        match Replacement::create(path) {
            Ok(output) => Ok(Some(output)),
            Err(e) => {
                self.say(1, &format!("zstd: {name}: {}\n", strerror(&e)))?;
                Ok(None)
            }
        }
    }

    /// zstd's `UTIL_requireUserConfirmation`: whether a line of standard input says yes.
    fn confirm(&self, prompt: &str, abort: &str) -> Result<bool, Stop> {
        if self.has_stdin_input {
            self.say(0, "stdin is an input - not proceeding.\n")?;
            return Ok(false);
        }
        self.say(0, prompt)?;
        if answer_is_yes(self.context)? {
            Ok(true)
        } else {
            self.say(0, &format!("{abort} \n"))?;
            Ok(false)
        }
    }

    /// zstd's `FIO_multiFilesConcatWarning`: whether several inputs may go into the one
    /// output `-o` names.
    fn may_concatenate(&self, out_name: &str) -> Result<bool, Stop> {
        if matches!(self.options.out, Out::Stdout | Out::Null) || self.files_total <= 1 {
            return Ok(true);
        }
        if self.options.display <= 1 {
            if self.options.remove {
                self.say(
                    1,
                    "Aborting. You may not use --rm when concatenating multiple files... \n",
                )?;
            } else {
                self.say(
                    1,
                    "Concatenating multiple processed inputs into a single output loses file metadata. \nAborting. \n",
                )?;
            }
            return Ok(false);
        }
        self.say(
            2,
            &format!(
                "zstd: WARNING: all input files will be processed and concatenated into a single output file: {out_name} \n\
                 The concatenated output CANNOT regenerate original file names nor directory structure. \n"
            ),
        )?;
        if self.options.remove {
            self.say(
                1,
                "Aborting. You may not use --rm when concatenating multiple files... \n",
            )?;
            return Ok(false);
        }
        if self.options.overwrite {
            return Ok(true);
        }
        self.confirm("Proceed? (y/n): ", "Aborting...")
    }

    /// The output name of a file compressed beside itself, or into `--output-dir-*`.
    fn compressed_name(&self, source: &str) -> String {
        let suffix = self.options.kind.suffix();
        self.out_dir_name(source, &format!("{source}{suffix}"))
    }

    /// `name` moved into `--output-dir-flat` or under `--output-dir-mirror`.
    fn out_dir_name(&self, source: &str, name: &str) -> String {
        let base = Path::new(name)
            .file_name()
            .map_or_else(|| name.to_owned(), |b| b.to_string_lossy().into_owned());
        if let Some(dir) = &self.options.out_dir_flat {
            return format!("{}/{base}", dir.trim_end_matches(['/', '\\']));
        }
        if let Some(dir) = &self.options.out_dir_mirror {
            let parent = Path::new(source)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let parent = parent.trim_start_matches("./").trim_start_matches('/');
            let folder = if parent.is_empty() {
                dir.trim_end_matches(['/', '\\']).to_owned()
            } else {
                format!("{}/{parent}", dir.trim_end_matches(['/', '\\']))
            };
            let _ = fs::create_dir_all(self.path(&folder));
            return format!("{folder}/{base}");
        }
        name.to_owned()
    }

    /// zstd's `FIO_determineDstName`: the name a file decompresses to, or `None` when its
    /// suffix is not known, said.
    fn decompressed_name(&self, source: &str) -> Result<Option<String>, Stop> {
        if source == STDIN_MARK {
            return Ok(Some(STDOUT_MARK.to_owned()));
        }
        let suffix = source
            .rfind('.')
            .map(|at| source.get(at..).unwrap_or_default());
        let known = suffix.filter(|s| SUFFIXES.contains(s) && source.len() > s.len());
        let Some(suffix) = known else {
            self.say(
                1,
                &format!(
                    "zstd: {source}: unknown suffix ({}). Can't derive the output file name. Specify it with -o dstFileName. Ignoring.\n",
                    SUFFIXES.join("/") + " expected"
                ),
            )?;
            return Ok(None);
        };
        let stem = source
            .get(..source.len() - suffix.len())
            .unwrap_or_default();
        let tar = if suffix.as_bytes().get(1) == Some(&b't') {
            ".tar"
        } else {
            ""
        };
        Ok(Some(self.out_dir_name(source, &format!("{stem}{tar}"))))
    }

    /// Compresses `input` into `output` in the chosen format: the bytes read and written.
    fn compress_stream(
        &self,
        input: &mut dyn Read,
        output: &mut dyn Write,
    ) -> io::Result<(u64, u64)> {
        let mut written = crate::compress::Counted::new(output);
        let mut read = crate::compress::Counted::new(input);
        {
            let level = u32::try_from(self.options.level.clamp(1, 9)).unwrap_or(6);
            let mut encoder: Box<dyn codec::Encoder + '_> = match self.options.kind {
                Kind::Gzip => codec::writer(Codec::Gzip, &mut written, level)?,
                Kind::Xz | Kind::Lzma => {
                    let format = if self.options.kind == Kind::Xz {
                        Format::Xz
                    } else {
                        Format::Lzma
                    };
                    let settings = xz::Settings {
                        preset: level,
                        extreme: false,
                        check: xz::Check::Crc64,
                        block_size: None,
                        threads: None,
                    };
                    xz::compressor(format, &settings, &mut written)?
                }
                Kind::Zstd | Kind::Lz4 => Box::new(
                    cash_archive::codec::zstd::Writer::with_check(&mut written, self.options.check)
                        .with_threads(self.threads),
                ),
            };
            io::copy(&mut read, &mut encoder)?;
            encoder.finish()?;
        }
        Ok((read.count, written.count))
    }

    /// One file compressed: zstd's `FIO_compressFilename_srcFile`; `shared` is the one
    /// output of `-o` or standard output, `None` for an output beside the file.
    fn compress_file(
        &mut self,
        source: &str,
        out_name: &str,
        shared: Option<&mut Sink<'_>>,
    ) -> Result<bool, Stop> {
        let path = self.path(source);
        if source != STDIN_MARK && !is_device(source) && path.is_dir() {
            self.say(1, &format!("zstd: {source} is a directory -- ignored \n"))?;
            return Ok(false);
        }
        let Some((mut input, metadata)) = self.open_source(source)? else {
            return Ok(false);
        };
        let transfer = metadata
            .as_ref()
            .filter(|m| m.is_file())
            .filter(|_| shared.is_none());
        let times = transfer.map(times_of);
        let read_only = transfer.is_some_and(|m| m.permissions().readonly());
        let result = if let Some(sink) = shared {
            match sink {
                Sink::Stdout => {
                    let mut out = std::mem::replace(
                        &mut self.stdout,
                        io::BufWriter::new(Box::new(io::sink())),
                    );
                    let counts = self.compress_stream(&mut input, &mut out);
                    self.stdout = out;
                    counts
                }
                Sink::Null => self.compress_stream(&mut input, &mut io::sink()),
                Sink::File(file) => self.compress_stream(&mut input, &mut **file),
            }
        } else {
            let Some(mut output) = self.open_output(Some(source), out_name)? else {
                return Ok(false);
            };
            match self.compress_stream(&mut input, &mut output) {
                Ok(counts) => match output.finish(times.unwrap_or_default(), read_only, false) {
                    Ok(()) => Ok(counts),
                    Err(e) => {
                        self.say(1, &format!("zstd: {out_name}: {} \n", strerror(&e)))?;
                        return Ok(false);
                    }
                },
                Err(e) => {
                    output.abandon();
                    Err(e)
                }
            }
        };
        let (bytes_in, bytes_out) = match result {
            Ok(counts) => counts,
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => return Err(e.into()),
            Err(e) => {
                self.say(1, &format!("zstd: {source}: {} \n", strerror(&e)))?;
                return Ok(false);
            }
        };
        drop(input);
        self.bytes_in += bytes_in;
        self.bytes_out += bytes_out;
        if self.files_total <= 1 || self.options.display >= 3 {
            let verbose = self.options.display > 3;
            let shown = if out_name == STDOUT_MARK || matches!(self.options.out, Out::Stdout) {
                STDOUT_MARK
            } else {
                out_name
            };
            let line = if bytes_in == 0 {
                format!(
                    "{source:<20} :  ({} => {}, {shown}) \n",
                    sized(bytes_in, verbose, 6, 0),
                    sized(bytes_out, verbose, 6, 0)
                )
            } else {
                #[expect(clippy::cast_precision_loss, reason = "zstd's ratio is a double")]
                let percent = bytes_out as f64 / bytes_in as f64 * 100.0;
                format!(
                    "{source:<20} :{percent:6.2}%   ({} => {}, {shown}) \n",
                    sized(bytes_in, verbose, 6, 0),
                    sized(bytes_out, verbose, 6, 0)
                )
            };
            self.say(2, &line)?;
        }
        if self.options.remove && source != STDIN_MARK {
            if let Err(e) = cash_win32::unix::remove_even_read_only(&path) {
                self.say(1, &format!("zstd: {source}: {}\n", strerror(&e)))?;
                return Err(Stop::Exit(1));
            }
        }
        Ok(true)
    }

    /// zstd's `FIO_decompressFrames`: every frame of `input`, each by its magic; the
    /// bytes written, or `None` when the file failed, said.
    #[expect(
        clippy::too_many_lines,
        reason = "zstd's decompressFrames, one case per magic"
    )]
    fn decompress_frames(
        &self,
        source: &str,
        input: Box<dyn Read>,
        output: &mut dyn Write,
        to_stdout: bool,
    ) -> Result<Option<u64>, Stop> {
        let pass_through = self
            .options
            .pass_through
            .unwrap_or(self.options.overwrite && to_stdout);
        let mut input = Lookahead::new(input);
        let mut size = 0_u64;
        let mut read_something = false;
        loop {
            let first = match input.peek(4) {
                Ok(first) => first.to_vec(),
                Err(e) => {
                    self.say(1, &format!("zstd: {source}: {} \n", strerror(&e)))?;
                    return Ok(None);
                }
            };
            if first.is_empty() {
                if !read_something {
                    self.say(1, &format!("zstd: {source}: unexpected end of file \n"))?;
                    return Ok(None);
                }
                break;
            }
            read_something = true;
            if first.len() < 4 {
                if pass_through {
                    return self.pass_through(&mut input, output, size);
                }
                self.say(1, &format!("zstd: {source}: unknown header \n"))?;
                return Ok(None);
            }
            let magic = le32(&first, 0);
            if magic == 0xFD2F_B528 || magic & 0xFFFF_FFF0 == 0x184D_2A50 {
                match decompress_frame(&mut input, output, self.options.check) {
                    Ok(written) => size += written,
                    Err(FrameError::PrematureEnd) => {
                        self.say(
                            1,
                            &format!(
                                "{} : Read error (39) : premature end \n",
                                self.short_name(source)
                            ),
                        )?;
                        return Ok(None);
                    }
                    Err(FrameError::Decoding(what)) => {
                        self.say(
                            1,
                            &format!(
                                "{} : Decoding error (36) : {what} \n",
                                self.short_name(source)
                            ),
                        )?;
                        return Ok(None);
                    }
                    Err(FrameError::Read(e)) => {
                        self.say(1, &format!("zstd: {source}: {} \n", strerror(&e)))?;
                        return Ok(None);
                    }
                    Err(FrameError::Write(e)) => return self.write_failed(e),
                }
            } else if first.starts_with(&[0x1f, 0x8b]) {
                let mut counted = crate::compress::Counted::new(&mut *output);
                let mut decoder = codec::reader(Codec::Gzip, &mut input);
                match io::copy(&mut decoder, &mut counted) {
                    Ok(_) => size += counted.count,
                    Err(e) => {
                        return match codec::codec_error(&e) {
                            Some(codec::CodecError::Truncated) => {
                                self.say(1, &format!("zstd: {source}: premature gz end \n"))?;
                                Ok(None)
                            }
                            Some(_) => {
                                self.say(1, &format!("zstd: {source}: inflate error -3 \n"))?;
                                Ok(None)
                            }
                            None if e.kind() == io::ErrorKind::BrokenPipe => self.write_failed(e),
                            None => {
                                self.say(1, &format!("zstd: {source}: {} \n", strerror(&e)))?;
                                Ok(None)
                            }
                        };
                    }
                }
            } else if first.starts_with(&[0xfd, 0x37]) || first.starts_with(&[0x5d, 0x00]) {
                let (format, single) = if first.first() == Some(&0xfd) {
                    (Format::Xz, false)
                } else {
                    (Format::Lzma, true)
                };
                let mut counted = crate::compress::Counted::new(&mut *output);
                match xz::decompress(format, &mut input, &mut counted, single, 8 << 20) {
                    Ok(_) => size += counted.count,
                    Err(xz::Broken::Truncated) => {
                        self.say(1, &format!("zstd: {source}: premature lzma end \n"))?;
                        return Ok(None);
                    }
                    Err(xz::Broken::Write(e)) => return self.write_failed(e),
                    Err(xz::Broken::Read(e)) => {
                        self.say(1, &format!("zstd: {source}: {} \n", strerror(&e)))?;
                        return Ok(None);
                    }
                    Err(xz::Broken::Corrupt | xz::Broken::Unsupported) => {
                        self.say(1, &format!("zstd: {source}: lzma_code decoding error 9 \n"))?;
                        return Ok(None);
                    }
                }
            } else if magic == 0x184D_2204 {
                self.say(
                    1,
                    &format!(
                        "zstd: {source}: lz4 file cannot be uncompressed (zstd compiled without HAVE_LZ4) -- ignored \n"
                    ),
                )?;
                return Ok(None);
            } else if pass_through {
                return self.pass_through(&mut input, output, size);
            } else {
                self.say(1, &format!("zstd: {source}: unsupported format \n"))?;
                return Ok(None);
            }
        }
        Ok(Some(size))
    }

    fn pass_through(
        &self,
        input: &mut dyn Read,
        output: &mut dyn Write,
        size: u64,
    ) -> Result<Option<u64>, Stop> {
        match io::copy(input, output) {
            Ok(copied) => Ok(Some(size + copied)),
            Err(e) => self.write_failed(e),
        }
    }

    fn write_failed(&self, error: io::Error) -> Result<Option<u64>, Stop> {
        if error.kind() == io::ErrorKind::BrokenPipe {
            return Err(error.into());
        }
        self.say(1, &format!("zstd: {}\n", strerror(&error)))?;
        Ok(None)
    }

    /// One file decompressed or tested: zstd's `FIO_decompressSrcFile`.
    fn decompress_file(
        &mut self,
        source: &str,
        out_name: &str,
        shared: Option<&mut Sink<'_>>,
    ) -> Result<bool, Stop> {
        let path = self.path(source);
        if source != STDIN_MARK && !is_device(source) && path.is_dir() {
            self.say(1, &format!("zstd: {source} is a directory -- ignored \n"))?;
            return Ok(false);
        }
        let Some((input, metadata)) = self.open_source(source)? else {
            return Ok(false);
        };
        let size = if let Some(sink) = shared {
            match sink {
                Sink::Stdout => {
                    let mut out = std::mem::replace(
                        &mut self.stdout,
                        io::BufWriter::new(Box::new(io::sink())),
                    );
                    let size = self.decompress_frames(source, input, &mut out, true);
                    self.stdout = out;
                    size?
                }
                Sink::Null => self.decompress_frames(source, input, &mut io::sink(), false)?,
                Sink::File(file) => self.decompress_frames(source, input, &mut **file, false)?,
            }
        } else {
            let Some(mut output) = self.open_output(Some(source), out_name)? else {
                return Ok(false);
            };
            let size = self.decompress_frames(source, input, &mut output, false)?;
            let transfer = metadata.as_ref().filter(|m| m.is_file());
            if let Some(size) = size {
                let times = transfer.map(times_of).unwrap_or_default();
                let read_only = transfer.is_some_and(|m| m.permissions().readonly());
                if let Err(e) = output.finish(times, read_only, false) {
                    self.say(1, &format!("zstd: {out_name}: {} \n", strerror(&e)))?;
                    return Ok(false);
                }
                Some(size)
            } else {
                output.abandon();
                None
            }
        };
        let Some(size) = size else {
            return Ok(false);
        };
        self.bytes_out += size;
        if self.files_total <= 1 || self.options.display >= 3 {
            self.say(2, &format!("{source:<20}: {size} bytes \n"))?;
        }
        if self.options.remove && source != STDIN_MARK {
            if let Err(e) = cash_win32::unix::remove_even_read_only(&path) {
                self.say(1, &format!("zstd: {source}: {} \n", strerror(&e)))?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Every file, compressed or decompressed: zstd's `FIO_*MultipleFilenames`.
    #[expect(
        clippy::too_many_lines,
        reason = "zstd's compressMultipleFilenames and decompressMultipleFilenames, together"
    )]
    fn process(&mut self, files: &[String]) -> Result<bool, Stop> {
        let compress = self.options.operation == Operation::Compress;
        let mut error = false;
        let out = self.options.out.clone();
        let mut shared_file = if let Out::File(name) = &out {
            if files.len() > 1 && !self.may_concatenate(name)? {
                return Ok(true);
            }
            if let [source] = files {
                let source = source.clone();
                let ok = if compress {
                    self.compress_file(&source, name, None)?
                } else {
                    self.decompress_file(&source, name, None)?
                };
                if ok {
                    self.files_done += 1;
                }
                return Ok(!ok);
            }
            let Some(output) = self.open_output(None, name)? else {
                return Ok(true);
            };
            Some(output)
        } else {
            None
        };
        for source in files {
            let ok = match (&out, shared_file.as_mut()) {
                (Out::File(name), Some(file)) => {
                    let mut sink = Sink::File(file);
                    if compress {
                        self.compress_file(source, name, Some(&mut sink))?
                    } else {
                        self.decompress_file(source, name, Some(&mut sink))?
                    }
                }
                (Out::Stdout, _) => {
                    let mut sink = Sink::Stdout;
                    if compress {
                        self.compress_file(source, STDOUT_MARK, Some(&mut sink))?
                    } else {
                        self.decompress_file(source, STDOUT_MARK, Some(&mut sink))?
                    }
                }
                (Out::Null, _) => {
                    let mut sink = Sink::Null;
                    if compress {
                        self.compress_file(source, "/dev/null", Some(&mut sink))?
                    } else {
                        self.decompress_file(source, "/dev/null", Some(&mut sink))?
                    }
                }
                _ => {
                    if source == STDIN_MARK {
                        let mut sink = Sink::Stdout;
                        if compress {
                            self.compress_file(source, STDOUT_MARK, Some(&mut sink))?
                        } else {
                            self.decompress_file(source, STDOUT_MARK, Some(&mut sink))?
                        }
                    } else if compress {
                        if self.options.exclude_compressed
                            && COMPRESSED.iter().any(|s| source.ends_with(s))
                        {
                            continue;
                        }
                        let out_name = self.compressed_name(source);
                        self.compress_file(source, &out_name, None)?
                    } else {
                        match self.decompressed_name(source)? {
                            Some(out_name) => self.decompress_file(source, &out_name, None)?,
                            None => false,
                        }
                    }
                }
            };
            if ok {
                self.files_done += 1;
            } else {
                error = true;
            }
        }
        if let Some(output) = shared_file {
            if let Err(e) = output.finish(fs::FileTimes::new(), false, false) {
                self.say(1, &format!("zstd: {}\n", strerror(&e)))?;
                error = true;
            }
        }
        if self.files_done >= 1 && self.files_total > 1 {
            let verbose = self.options.display > 3;
            if compress {
                let line = if self.bytes_in == 0 {
                    format!(
                        "{:3} files compressed : ({} => {})\n",
                        self.files_done,
                        sized(self.bytes_in, verbose, 6, 4),
                        sized(self.bytes_out, verbose, 6, 4)
                    )
                } else {
                    #[expect(clippy::cast_precision_loss, reason = "zstd's ratio is a double")]
                    let percent = self.bytes_out as f64 / self.bytes_in as f64 * 100.0;
                    format!(
                        "{:3} files compressed : {percent:.2}% ({} => {})\n",
                        self.files_done,
                        sized(self.bytes_in, verbose, 6, 4),
                        sized(self.bytes_out, verbose, 6, 4)
                    )
                };
                self.say(2, &line)?;
            } else {
                self.say(
                    2,
                    &format!(
                        "{} files decompressed : {:6} bytes total \n",
                        self.files_done, self.bytes_out
                    ),
                )?;
            }
        }
        Ok(error)
    }

    /// `-l`: zstd's `FIO_listMultipleFiles`.
    #[expect(
        clippy::too_many_lines,
        reason = "zstd's listMultipleFiles and listFile, one pass kept together"
    )]
    fn list(&mut self, files: &[String]) -> Result<bool, Stop> {
        if files.iter().any(|f| f == STDIN_MARK) {
            self.say(
                1,
                "zstd: --list does not support reading from standard input \n",
            )?;
            return Ok(true);
        }
        let display = self.options.display;
        if display <= 2 {
            self.stdout
                .write_all(b"Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename\n")?;
        }
        let mut total = Info {
            uses_check: true,
            ..Info::default()
        };
        let mut error = false;
        for name in files {
            let mut said = String::new();
            let mut info = Info::default();
            let outcome = match fs::metadata(self.path(name)) {
                Ok(m) if m.is_file() => {
                    if let Ok(mut file) = fs::File::open(self.path(name)) {
                        info.compressed = m.len();
                        info.files = 1;
                        Ok(analyze_frames(&mut file, &mut info, &mut said))
                    } else {
                        let _ = writeln!(said, "Error: could not open source file {name} ");
                        Err(())
                    }
                }
                _ => {
                    let _ = writeln!(said, "Error : {name} is not a file ");
                    Err(())
                }
            };
            self.say(1, &said)?;
            match outcome {
                Err(()) => {
                    if display > 2 {
                        self.stdout.write_all(b"\n")?;
                    }
                    error = true;
                    continue;
                }
                Ok(Err(InfoError::NotZstd)) => {
                    writeln!(self.stdout, "File \"{name}\" not compressed by zstd ")?;
                    if display > 2 {
                        self.stdout.write_all(b"\n")?;
                    }
                    error = true;
                    continue;
                }
                Ok(Err(InfoError::Truncated)) => {
                    writeln!(self.stdout, "File \"{name}\" is truncated ")?;
                    if display > 2 {
                        self.stdout.write_all(b"\n")?;
                    }
                    error = true;
                    continue;
                }
                Ok(Err(InfoError::Frame)) => {
                    self.say(1, &format!("Error while parsing \"{name}\" \n"))?;
                    error = true;
                }
                Ok(Ok(())) => {}
            }
            self.display_info(name, &info)?;
            total.frames += info.frames;
            total.skippable += info.skippable;
            total.compressed += info.compressed;
            total.decompressed += info.decompressed;
            total.size_unknown |= info.size_unknown;
            total.uses_check &= info.uses_check;
            total.files += info.files;
        }
        if files.len() > 1 && display <= 2 {
            let check = if total.uses_check { "XXH64" } else { "" };
            #[expect(clippy::cast_precision_loss, reason = "zstd's ratio is a double")]
            let ratio = if total.compressed == 0 {
                0.0
            } else {
                total.decompressed as f64 / total.compressed as f64
            };
            self.stdout.write_all(
                b"----------------------------------------------------------------- \n",
            )?;
            if total.size_unknown {
                writeln!(
                    self.stdout,
                    "{:6}  {:5}  {}                       {check:>5}  {} files",
                    total.frames + total.skippable,
                    total.skippable,
                    sized(total.compressed, false, 6, 4),
                    total.files
                )?;
            } else {
                writeln!(
                    self.stdout,
                    "{:6}  {:5}  {}  {}  {ratio:5.3}  {check:>5}  {} files",
                    total.frames + total.skippable,
                    total.skippable,
                    sized(total.compressed, false, 6, 4),
                    sized(total.decompressed, false, 8, 4),
                    total.files
                )?;
            }
        }
        Ok(error)
    }

    /// zstd's `displayInfo`.
    fn display_info(&mut self, name: &str, info: &Info) -> Result<(), Stop> {
        let check = if info.uses_check { "XXH64" } else { "None" };
        #[expect(clippy::cast_precision_loss, reason = "zstd's ratio is a double")]
        let ratio = if info.compressed == 0 {
            0.0
        } else {
            info.decompressed as f64 / info.compressed as f64
        };
        if self.options.display <= 2 {
            if info.size_unknown {
                writeln!(
                    self.stdout,
                    "{:6}  {:5}  {}                       {check:>5}  {name}",
                    info.frames + info.skippable,
                    info.skippable,
                    sized(info.compressed, false, 6, 4)
                )?;
            } else {
                writeln!(
                    self.stdout,
                    "{:6}  {:5}  {}  {}  {ratio:5.3}  {check:>5}  {name}",
                    info.frames + info.skippable,
                    info.skippable,
                    sized(info.compressed, false, 6, 4),
                    sized(info.decompressed, false, 8, 4)
                )?;
            }
            return Ok(());
        }
        let verbose = self.options.display > 3;
        let mut text = format!("{name} \n# Zstandard Frames: {}\n", info.frames);
        if info.skippable > 0 {
            let _ = writeln!(text, "# Skippable Frames: {}", info.skippable);
        }
        let _ = writeln!(text, "DictID: {}", info.dict_id);
        let _ = writeln!(
            text,
            "Window Size: {} ({} B)",
            sized(info.window, verbose, 0, 0),
            info.window
        );
        let _ = writeln!(
            text,
            "Compressed Size: {} ({} B)",
            sized(info.compressed, verbose, 0, 0),
            info.compressed
        );
        if !info.size_unknown {
            let _ = writeln!(
                text,
                "Decompressed Size: {} ({} B)",
                sized(info.decompressed, verbose, 0, 0),
                info.decompressed
            );
            let _ = writeln!(text, "Ratio: {ratio:.4}");
        }
        if info.uses_check && info.frames == 1 {
            let [a, b, c, d] = info.checksum;
            let _ = writeln!(text, "Check: {check} {d:02x}{c:02x}{b:02x}{a:02x}");
        } else {
            let _ = writeln!(text, "Check: {check}");
        }
        text.push('\n');
        self.stdout.write_all(text.as_bytes())?;
        Ok(())
    }

    /// The files of the command line, `--filelist` and `-r`, links passed over without
    /// `-f`: zstd's file table.
    fn file_table(&mut self) -> Result<Option<Vec<String>>, Stop> {
        let mut files = std::mem::take(&mut self.options.files);
        if !self.options.follow_links {
            let mut kept = Vec::new();
            let had = files.len();
            for name in files {
                let link = !is_device(&name)
                    && name != STDIN_MARK
                    && fs::symlink_metadata(self.path(&name))
                        .is_ok_and(|m| m.file_type().is_symlink());
                if link {
                    self.say(
                        2,
                        &format!("Warning : {name} is a symbolic link, ignoring \n"),
                    )?;
                } else {
                    kept.push(name);
                }
            }
            if kept.is_empty() && had > 0 {
                return Ok(None);
            }
            files = kept;
        }
        for list in self.options.file_lists.clone() {
            let Ok(bytes) = fs::read(self.path(&list)) else {
                self.say(1, &format!("zstd: error reading {list} \n"))?;
                return Err(Stop::Exit(1));
            };
            files.extend(
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned),
            );
        }
        let named = files.len();
        if self.options.operation == Operation::List && files.is_empty() {
            if !is_terminal(self.context, OpenFiles::STDIN_FD) {
                self.say(
                    1,
                    "zstd: --list does not support reading from standard input \n",
                )?;
            }
            self.say(1, "No files given \n")?;
            return Err(Stop::Exit(1));
        }
        if self.options.recursive {
            let mut expanded = Vec::new();
            for name in files {
                self.expand(&name, &mut expanded);
            }
            files = expanded;
        }
        if files.is_empty() {
            if named > 0 {
                self.say(
                    1,
                    "please provide correct input file(s) or non-empty directories -- ignored \n",
                )?;
                return Err(Stop::Exit(0));
            }
            files.push(STDIN_MARK.to_owned());
        }
        Ok(Some(files))
    }

    /// `name` for `-r`: itself, or every file under it, in name order.
    fn expand(&self, name: &str, into: &mut Vec<String>) {
        let path = self.path(name);
        if name == STDIN_MARK || !path.is_dir() {
            into.push(name.to_owned());
            return;
        }
        let Ok(entries) = fs::read_dir(&path) else {
            return;
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for entry in names {
            let child = format!("{}/{entry}", name.trim_end_matches(['/', '\\']));
            self.expand(&child, into);
        }
    }

    /// The whole run, from the checks zstd makes after reading its command line.
    #[expect(
        clippy::too_many_lines,
        reason = "zstd's main after its argument loop, its checks in its order"
    )]
    fn run_all(&mut self) -> Result<u8, Stop> {
        self.say(3, &format!("{VERSION}\n"))?;
        let Some(files) = self.file_table()? else {
            return Ok(1);
        };
        match self.options.operation {
            Operation::Bench => {
                self.say(1, "zstd: benchmark mode is not supported by cash's zstd \n")?;
                return Ok(1);
            }
            Operation::Train => {
                self.say(
                    1,
                    "zstd: dictionary training is not supported by cash's zstd \n",
                )?;
                return Ok(1);
            }
            Operation::Test => {
                self.options.out = Out::Null;
                self.options.remove = false;
            }
            _ => {}
        }
        if files.len() == 1
            && files.first().is_some_and(|f| f == STDIN_MARK)
            && self.options.out == Out::Beside
        {
            self.options.out = Out::Stdout;
        }
        let stdin_used = files.iter().any(|f| f == STDIN_MARK);
        self.has_stdin_input = stdin_used;
        if !self.options.force_stdin && stdin_used && is_terminal(self.context, OpenFiles::STDIN_FD)
        {
            self.say(1, "stdin is a console, aborting\n")?;
            return Ok(1);
        }
        if matches!(self.options.out, Out::Beside | Out::Stdout)
            && stdin_used
            && is_terminal(self.context, OpenFiles::STDOUT_FD)
            && !self.options.force_stdout
            && self.options.operation != Operation::Decompress
        {
            self.say(1, "stdout is a console, aborting\n")?;
            return Ok(1);
        }
        if let Some(what) = self.options.dictionary {
            self.say(
                1,
                &format!("zstd: {what}: dictionaries are not supported by cash's zstd \n"),
            )?;
            return Ok(1);
        }
        let max = if self.options.ultra {
            ULTRA_LEVEL
        } else {
            MAX_LEVEL
        };
        if self.options.level > max {
            self.say(
                2,
                &format!("Warning : compression level higher than max, reduced to {max} \n"),
            )?;
            self.options.level = max;
        }
        let has_stdout = self.options.out == Out::Stdout;
        if has_stdout && self.options.display == 2 {
            self.options.display = 1;
        }
        if self.options.remove && has_stdout {
            self.say(
                3,
                "Note: src files are not removed when output is stdout \n",
            )?;
            self.options.remove = false;
        }
        self.files_total = files.len();
        let failed = match self.options.operation {
            Operation::List => self.list(&files)?,
            Operation::Compress => {
                if self.options.kind == Kind::Lz4 {
                    self.say(
                        1,
                        "zstd: lz4 compression is not supported by cash's zstd \n",
                    )?;
                    return Ok(1);
                }
                if let Some(log) = self
                    .options
                    .window_log
                    .filter(|log| !(10..=31).contains(log))
                {
                    let _ = log;
                    self.say(1, "zstd: error 11 : Parameter is out of bound \n")?;
                    return Ok(11);
                }
                self.process(&files)?
            }
            Operation::Decompress | Operation::Test | Operation::Bench | Operation::Train => {
                self.process(&files)?
            }
        };
        Ok(u8::from(failed))
    }
}

fn exported<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    name: &str,
) -> Option<String> {
    context
        .shell
        .env()
        .get(name)
        .filter(|(_, var)| var.is_exported())
        .map(|(_, var)| var.value().to_cow_str(context.shell).into_owned())
}

fn run<SE: cash_core::ShellExtensions>(
    name: &'static str,
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    let mut options = preset(name);
    let mut said = String::new();
    let level = environment_level(exported(context, "ZSTD_CLEVEL").as_deref(), &mut said);
    if options.display >= 2 {
        context.stderr().write_all(said.as_bytes())?;
    }
    options.level = level;
    if let Err(Parsed::Exit(out, err, status)) = parse(name, &mut options, args) {
        context.stdout().write_all(out.as_bytes())?;
        context.stderr().write_all(err.as_bytes())?;
        return Ok(ExecutionResult::new(status));
    }
    if let Some(said) = environment_threads(exported(context, "ZSTD_NBTHREADS").as_deref()) {
        if options.display >= 2 {
            context.stderr().write_all(said.as_bytes())?;
        }
    }
    let stdout: io::BufWriter<Box<dyn Write>> =
        io::BufWriter::with_capacity(STDOUT_BUFFER, Box::new(context.stdout()));
    // init_nbWorkers: -T, else ZSTD_NBTHREADS when it is a number, else every core
    // (zstd's own default is one; the bytes are the same either way); 0, a core each.
    let threads = options
        .threads
        .or_else(|| {
            let value = exported(context, "ZSTD_NBTHREADS")?;
            match read_u32(&value) {
                Ok((n, "")) => Some(n),
                _ => None,
            }
        })
        .unwrap_or(0);
    let mut run = Run {
        options,
        context,
        stdout,
        files_total: 0,
        files_done: 0,
        bytes_in: 0,
        bytes_out: 0,
        has_stdin_input: false,
        threads: cash_archive::codec::parallel::threads(threads),
    };
    let outcome = run.run_all();
    let flushed = run.stdout.flush();
    match outcome {
        Ok(status) | Err(Stop::Exit(status)) => {
            flushed?;
            Ok(ExecutionResult::new(status))
        }
        Err(Stop::Shell(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(name: &str, words: &[&str]) -> Result<Options, (String, String, u8)> {
        let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
        let mut options = preset(name);
        match parse(name, &mut options, &words) {
            Ok(()) => Ok(options),
            Err(Parsed::Exit(out, err, status)) => Err((out, err, status)),
        }
    }

    #[test]
    fn options_are_read_as_zstd_reads_them() {
        let o = parsed("zstd", &["-19dvf", "a", "-o", "b", "--", "-c"]).unwrap();
        assert_eq!(
            (o.level, o.operation, o.display),
            (19, Operation::Decompress, 3)
        );
        assert_eq!(o.out, Out::File("b".into()));
        assert_eq!(o.files, ["a", "-c"]);
        let o = parsed("zstd", &["--fast=3", "-T4", "-M32MB", "-"]).unwrap();
        assert_eq!((o.level, o.files.len()), (-3, 1));
        assert_eq!(parsed("zstdcat", &[]).unwrap().out, Out::Stdout);
        let (_, err, status) = parsed("zstd", &["-Y"]).unwrap_err();
        assert!(
            err.starts_with("Incorrect parameter: -Y \nCompress or decompress"),
            "{err}"
        );
        assert_eq!(status, 1);
        let (_, err, _) = parsed("zstd", &["-q", "-o"]).unwrap_err();
        assert_eq!(err, "error: missing command argument \n");
        let (_, err, _) = parsed("zstd", &["-T4x"]).unwrap_err();
        assert!(err.starts_with("Incorrect parameter: -x "), "{err}");
        assert!(parsed("zstd", &["--decom"]).is_err());
    }

    #[test]
    fn sizes_read_as_zstd_prints_them() {
        assert_eq!(sized(6, false, 6, 0), "     6 B");
        assert_eq!(sized(19, false, 6, 4), "    19   B");
        assert_eq!(sized(2 << 20, false, 0, 0), "2.00 MiB");
        assert_eq!(read_u32("32MB"), Ok((32 << 20, "")));
        let mut said = String::new();
        assert_eq!(environment_level(Some("x"), &mut said), 3);
        assert_eq!(
            said,
            "Ignore environment variable setting ZSTD_CLEVEL=x: not a valid integer value \n"
        );
    }
}
