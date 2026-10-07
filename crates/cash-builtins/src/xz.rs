//! `xz`, `unxz`, `xzcat`, `lzma`, `unlzma` and `lzcat`: XZ Utils 5.8's interface,
//! messages and exit codes, on lzma-rust2.
//!
//! Checked against XZ Utils 5.8.3 (`crates/cash/tests/oracle/xz_cases.sh`): every
//! message and status, the decompressed bytes, `-v`'s line and `-l`'s tables (of files
//! XZ Utils made). The compressed bytes are lzma-rust2's: a .xz cash makes is not byte
//! for byte liblzma's at the same preset, and every xz reads it.
//!
//! On a clean Windows machine there is no `xz`; `tar.exe` reads a `.tar.xz`, not a bare
//! `.xz`.
//!
//! Where cash differs, on purpose:
//! - The version line is cash's.
//! - Custom filter chains (`--filters`, `--lzma1`, `--lzma2`, the BCJ filters and
//!   `--delta`) are refused; the presets, `-e`, `--check` and `--block-size` are what
//!   cash compresses with. `--block-list`, `--flush-timeout`, the memory limits,
//!   `--no-adjust`, `--no-sync` and `--no-sparse` are read and checked, and change
//!   nothing. `-T` is xz's: by default every core compresses blocks of three
//!   dictionaries (as many cores as a quarter of the memory allows), `-T1` one thread
//!   and one block unless `--block-size`; a file of one stream and several blocks
//!   decompresses on every core too. `--ignore-check` is read, and checks are verified
//!   all the same.
//! - `-vv` says what `-v` says, and `-lvv` what `-lv` says: the filter chains and memory
//!   figures there are liblzma's. At a console, `-v` shows each file's final line, not
//!   one redrawn every second.
//! - Windows has no mode bits but read-only, which is carried over; no owner is set.
//! - A file is written beside its target under a temporary name and renamed over it at
//!   the end, so an interrupted run leaves no half-written file.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::time::Instant;

use cash_archive::codec::xz::{
    self, Broken, Check, FileInfo, Format, InfoProblem, Settings, check_name,
};
use cash_core::openfiles::OpenFiles;
use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Short};
use cash_win32::unix::Replacement;
use clap::Parser;

use crate::compress::{Counted, Output, is_terminal, strerror, times_of};

/// The first line of `--version`.
const VERSION: &str = "xz (cash): XZ Utils 5.8's options, on lzma-rust2";

/// xz's short options; a colon follows one that takes a value.
const SHORTS: &str = "cC:defF:hHlkM:qQS:tT:vVz0123456789";

/// What an option is known by.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Id {
    /// A short option, and the long names that are the same option.
    Letter(char),
    SingleStream,
    /// An option that is read and changes nothing here.
    Accepted,
    Files,
    Files0,
    BlockSize,
    BlockList,
    /// A memory limit, by its option's name.
    Memlimit(&'static str),
    FlushTimeout,
    /// A custom filter chain, by its option's name.
    Filter(&'static str),
    Robot,
    InfoMemory,
}

/// xz 5.8's long options, in its table's order, which its ambiguity messages follow.
const LONGS: &[Long<'static, Id>] = &[
    Long::new("compress", Arg::No, Id::Letter('z')),
    Long::new("decompress", Arg::No, Id::Letter('d')),
    Long::new("uncompress", Arg::No, Id::Letter('d')),
    Long::new("test", Arg::No, Id::Letter('t')),
    Long::new("list", Arg::No, Id::Letter('l')),
    Long::new("keep", Arg::No, Id::Letter('k')),
    Long::new("force", Arg::No, Id::Letter('f')),
    Long::new("stdout", Arg::No, Id::Letter('c')),
    Long::new("to-stdout", Arg::No, Id::Letter('c')),
    Long::new("single-stream", Arg::No, Id::SingleStream),
    Long::new("no-sync", Arg::No, Id::Accepted),
    Long::new("no-sparse", Arg::No, Id::Accepted),
    Long::new("suffix", Arg::Required, Id::Letter('S')),
    Long::new("files", Arg::Optional, Id::Files),
    Long::new("files0", Arg::Optional, Id::Files0),
    Long::new("format", Arg::Required, Id::Letter('F')),
    Long::new("check", Arg::Required, Id::Letter('C')),
    Long::new("ignore-check", Arg::No, Id::Accepted),
    Long::new("block-size", Arg::Required, Id::BlockSize),
    Long::new("block-list", Arg::Required, Id::BlockList),
    Long::new(
        "memlimit-compress",
        Arg::Required,
        Id::Memlimit("memlimit-compress"),
    ),
    Long::new(
        "memlimit-decompress",
        Arg::Required,
        Id::Memlimit("memlimit-decompress"),
    ),
    Long::new(
        "memlimit-mt-decompress",
        Arg::Required,
        Id::Memlimit("memlimit-mt-decompress"),
    ),
    Long::new("memlimit", Arg::Required, Id::Letter('M')),
    Long::new("memory", Arg::Required, Id::Letter('M')),
    Long::new("no-adjust", Arg::No, Id::Accepted),
    Long::new("threads", Arg::Required, Id::Letter('T')),
    Long::new("flush-timeout", Arg::Required, Id::FlushTimeout),
    Long::new("extreme", Arg::No, Id::Letter('e')),
    Long::new("fast", Arg::No, Id::Letter('0')),
    Long::new("best", Arg::No, Id::Letter('9')),
    Long::new("filters", Arg::Required, Id::Filter("filters")),
    Long::new("filters1", Arg::Required, Id::Filter("filters1")),
    Long::new("filters2", Arg::Required, Id::Filter("filters2")),
    Long::new("filters3", Arg::Required, Id::Filter("filters3")),
    Long::new("filters4", Arg::Required, Id::Filter("filters4")),
    Long::new("filters5", Arg::Required, Id::Filter("filters5")),
    Long::new("filters6", Arg::Required, Id::Filter("filters6")),
    Long::new("filters7", Arg::Required, Id::Filter("filters7")),
    Long::new("filters8", Arg::Required, Id::Filter("filters8")),
    Long::new("filters9", Arg::Required, Id::Filter("filters9")),
    Long::new("filters-help", Arg::No, Id::Filter("filters-help")),
    Long::new("lzma1", Arg::Optional, Id::Filter("lzma1")),
    Long::new("lzma2", Arg::Optional, Id::Filter("lzma2")),
    Long::new("x86", Arg::Optional, Id::Filter("x86")),
    Long::new("powerpc", Arg::Optional, Id::Filter("powerpc")),
    Long::new("ia64", Arg::Optional, Id::Filter("ia64")),
    Long::new("arm", Arg::Optional, Id::Filter("arm")),
    Long::new("armthumb", Arg::Optional, Id::Filter("armthumb")),
    Long::new("arm64", Arg::Optional, Id::Filter("arm64")),
    Long::new("sparc", Arg::Optional, Id::Filter("sparc")),
    Long::new("riscv", Arg::Optional, Id::Filter("riscv")),
    Long::new("delta", Arg::Optional, Id::Filter("delta")),
    Long::new("quiet", Arg::No, Id::Letter('q')),
    Long::new("verbose", Arg::No, Id::Letter('v')),
    Long::new("no-warn", Arg::No, Id::Letter('Q')),
    Long::new("robot", Arg::No, Id::Robot),
    Long::new("info-memory", Arg::No, Id::InfoMemory),
    Long::new("help", Arg::No, Id::Letter('h')),
    Long::new("long-help", Arg::No, Id::Letter('H')),
    Long::new("version", Arg::No, Id::Letter('V')),
];

/// Compress or decompress .xz and .lzma files with XZ Utils' options.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct XzCommand {
    /// Options and files, read here as xz reads them.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Decompress files: `xz -d`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct UnxzCommand {
    /// Options and files, read here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Decompress files to standard output: `xz -dc`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct XzcatCommand {
    /// Options and files, read here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Compress files to .lzma: `xz --format=lzma`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct LzmaCommand {
    /// Options and files, read here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Decompress .lzma files: `xz --format=lzma -d`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct UnlzmaCommand {
    /// Options and files, read here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Decompress .lzma files to standard output: `xz --format=lzma -dc`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct LzcatCommand {
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

command!(XzCommand, "xz");
command!(UnxzCommand, "unxz");
command!(XzcatCommand, "xzcat");
command!(LzmaCommand, "lzma");
command!(UnlzmaCommand, "unlzma");
command!(LzcatCommand, "lzcat");

/// What is done to each file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Compress,
    Decompress,
    Test,
    List,
}

/// How much is said: xz's verbosity levels.
const V_ERROR: u8 = 1;
const V_WARNING: u8 = 2;
const V_VERBOSE: u8 = 3;

/// Where `--files` and `--files0` read names from.
#[derive(Clone, Debug, Eq, PartialEq)]
struct NameList {
    /// The file, or `None` for standard input.
    file: Option<String>,
    /// The byte that ends each name: `\n`, or `\0` for `--files0`.
    end: u8,
}

#[derive(Debug)]
#[expect(clippy::struct_excessive_bools, reason = "xz's own switches, one each")]
struct Options {
    mode: Mode,
    /// `None` is `--format=auto`.
    format: Option<Format>,
    stdout: bool,
    keep: bool,
    force: bool,
    single_stream: bool,
    verbosity: u8,
    no_warn: bool,
    robot: bool,
    preset: u32,
    extreme: bool,
    check: Check,
    block_size: Option<NonZeroU64>,
    /// `-T`: `None` for one thread (`-T1`), else the threads (0, one a core; `+1`, the
    /// threaded encoder on one).
    threads: Option<u32>,
    suffix: Option<String>,
    names: Option<NameList>,
    files: Vec<String>,
}

/// What reading the command line came to.
#[derive(Debug)]
enum Parsed {
    Run(Options),
    /// The command is over: this is said on standard output, this on standard error,
    /// and the status.
    Exit(String, String, u8),
}

fn help(invoked: &str, long: bool) -> String {
    let mut text = format!(
        "Usage: {invoked} [OPTION]... [FILE]...\n\
         Compress or decompress FILEs in the .xz format.\n\
         \n\
         Mandatory arguments to long options are mandatory for short options too.\n\n"
    );
    if long {
        text.push_str(LONG_HELP);
    } else {
        text.push_str(SHORT_HELP);
    }
    text.push_str("\nWith no FILE, or when FILE is -, read standard input.\n");
    text
}

const SHORT_HELP: &str = "  -z, --compress      force compression
  -d, --decompress    force decompression
  -t, --test          test compressed file integrity
  -l, --list          list information about .xz files
  -k, --keep          keep (don't delete) input files
  -f, --force         force overwrite of output file and (de)compress links
  -c, --stdout        write to standard output and don't delete input files
  -0 ... -9           compression preset; default is 6; take compressor *and*
                      decompressor memory usage into account before using 7-9!
  -e, --extreme       try to improve compression ratio by using more CPU time;
                      does not affect decompressor memory requirements
  -T, --threads=NUM   use at most NUM threads; the default is 0 which uses as
                      many threads as there are processor cores
  -q, --quiet         suppress warnings; specify twice to suppress errors too
  -v, --verbose       be verbose; specify twice for even more verbose
  -h, --help          display this short help and exit
  -H, --long-help     display the long help (lists also the advanced options)
  -V, --version       display the version number and exit
";

const LONG_HELP: &str = " Operation mode:

  -z, --compress      force compression
  -d, --decompress    force decompression
  -t, --test          test compressed file integrity
  -l, --list          list information about .xz files

 Operation modifiers:

  -k, --keep          keep (don't delete) input files
  -f, --force         force overwrite of output file and (de)compress links
  -c, --stdout        write to standard output and don't delete input files
      --no-sync       don't synchronize the output file to the storage device
                      before removing the input file
      --single-stream decompress only the first stream, and silently ignore
                      possible remaining input data
      --no-sparse     do not create sparse files when decompressing
  -S, --suffix=.SUF   use the suffix '.SUF' on compressed files
      --files[=FILE]  read filenames to process from FILE; if FILE is omitted,
                      filenames are read from the standard input; filenames
                      must be terminated with the newline character
      --files0[=FILE] like --files but use the null character as terminator

 Basic file format and compression options:

  -F, --format=FORMAT file format to encode or decode; possible values are
                      'auto' (default), 'xz', 'lzma', 'lzip', and 'raw'
  -C, --check=NAME    integrity check type: 'none' (use with caution), 'crc32',
                      'crc64' (default), or 'sha256'
      --ignore-check  don't verify the integrity check when decompressing
  -0 ... -9           compression preset; default is 6; take compressor *and*
                      decompressor memory usage into account before using 7-9!
  -e, --extreme       try to improve compression ratio by using more CPU time;
                      does not affect decompressor memory requirements
  -T, --threads=NUM   use at most NUM threads; the default is 0 which uses as
                      many threads as there are processor cores
      --block-size=SIZE
                      start a new .xz block after every SIZE bytes of input;
                      use this to set the block size for threaded compression
      --block-list=BLOCKS
                      start a new .xz block after the given comma-separated
                      intervals of uncompressed data; optionally, specify a
                      filter chain number (0-9) followed by a ':' before the
                      uncompressed data size
      --flush-timeout=NUM
                      when compressing, if more than NUM milliseconds has
                      passed since the previous flush and reading more input
                      would block, all pending data is flushed out
      --memlimit-compress=LIMIT
      --memlimit-decompress=LIMIT
      --memlimit-mt-decompress=LIMIT
  -M, --memlimit=LIMIT
                      set memory usage limit for compression, decompression,
                      threaded decompression, or all of these; LIMIT is in
                      bytes, % of RAM, or 0 for defaults
      --no-adjust     if compression settings exceed the memory usage limit,
                      give an error instead of adjusting the settings downwards

 Custom filter chain for compression (an alternative to using presets):

  --filters=FILTERS   set the filter chain using the liblzma filter string
                      syntax; use --filters-help for more information
  --filters1=FILTERS ... --filters9=FILTERS
                      set additional filter chains using the liblzma filter
                      string syntax to use with --block-list
  --filters-help      display more information about the liblzma filter string
                      syntax and exit

  --lzma1[=OPTS]
  --lzma2[=OPTS]      LZMA1 or LZMA2; OPTS is a comma-separated list of zero or
                      more of the following options (valid values; default):
                        preset=PRE  reset options to a preset (0-9[e])
                        dict=NUM    dictionary size (4KiB - 1536MiB; 8MiB)
                        lc=NUM      number of literal context bits (0-4; 3)
                        lp=NUM      number of literal position bits (0-4; 0)
                        pb=NUM      number of position bits (0-4; 2)
                        mode=MODE   compression mode (fast, normal; normal)
                        nice=NUM    nice length of a match (2-273; 64)
                        mf=NAME     match finder (hc3, hc4, bt2, bt3, bt4; bt4)
                        depth=NUM   maximum search depth; 0=automatic (default)

  --x86[=OPTS]        x86 BCJ filter (32-bit and 64-bit)
  --arm[=OPTS]        ARM BCJ filter
  --armthumb[=OPTS]   ARM-Thumb BCJ filter
  --arm64[=OPTS]      ARM64 BCJ filter
  --powerpc[=OPTS]    PowerPC BCJ filter (big endian only)
  --ia64[=OPTS]       IA-64 (Itanium) BCJ filter
  --sparc[=OPTS]      SPARC BCJ filter
  --riscv[=OPTS]      RISC-V BCJ filter
                      Valid OPTS for all BCJ filters:
                        start=NUM   start offset for conversions (default=0)

  --delta[=OPTS]      Delta filter; valid OPTS (valid values; default):
                        dist=NUM    distance between bytes being subtracted
                                    from each other (1-256; 1)

 Other options:

  -q, --quiet         suppress warnings; specify twice to suppress errors too
  -v, --verbose       be verbose; specify twice for even more verbose
  -Q, --no-warn       make warnings not affect the exit status
      --robot         use machine-parsable messages (useful for scripts)

      --info-memory   display the total amount of RAM and the currently active
                      memory usage limits, and exit
  -h, --help          display the short help (lists only the basic options)
  -H, --long-help     display this long help and exit
  -V, --version       display the version number and exit
";

/// xz's `str_to_uint64`: a decimal number with an optional KiB, MiB or GiB suffix, or
/// `max`, within `min..=max`; what is wrong is said as xz says it.
fn number(name: &str, value: &str, min: u64, max: u64) -> Result<u64, String> {
    let value = value.trim_start_matches([' ', '\t']);
    if value == "max" {
        return Ok(max);
    }
    let digits = value.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return Err(format!(
            "{value}: Value is not a non-negative decimal integer"
        ));
    }
    let out_of_range =
        || format!("Value of the option '{name}' must be in the range [{min}, {max}]");
    let mut result = value
        .get(..digits)
        .and_then(|d| d.parse::<u64>().ok())
        .ok_or_else(out_of_range)?;
    if let Some(suffix) = value.get(digits..).filter(|s| !s.is_empty()) {
        let shift = match suffix.chars().next() {
            Some('k' | 'K') => 10,
            Some('m' | 'M') => 20,
            Some('g' | 'G') => 30,
            _ => 0,
        };
        let rest = suffix.get(1..).unwrap_or_default();
        if shift == 0 || !matches!(rest, "" | "i" | "iB" | "B") {
            return Err(format!(
                "{suffix}: Invalid multiplier suffix\n\
                 Valid suffixes are 'KiB' (2^10), 'MiB' (2^20), and 'GiB' (2^30)."
            ));
        }
        result = result.checked_mul(1 << shift).ok_or_else(out_of_range)?;
    }
    if result < min || result > max {
        return Err(out_of_range());
    }
    Ok(result)
}

/// A memory limit's value: a size, or a percentage of the RAM.
fn memlimit(name: &str, value: &str) -> Result<(), String> {
    if let Some(percent) = value.strip_suffix('%') {
        number(&format!("{name}%"), percent, 1, 100).map(|_| ())
    } else {
        number(name, value, 0, u64::MAX).map(|_| ())
    }
}

/// Reads one option into `options`; `Err` ends the command with what it says.
fn apply(invoked: &str, options: &mut Options, id: Id, value: Option<&str>) -> Result<(), Parsed> {
    // Each line of a fatal message is the program's.
    let fatal = |text: String| {
        let said = text.lines().fold(String::new(), |mut said, line| {
            let _ = writeln!(said, "{invoked}: {line}");
            said
        });
        Parsed::Exit(String::new(), said, 1)
    };
    let value = value.unwrap_or_default();
    match id {
        Id::Letter('c') => options.stdout = true,
        Id::Letter('C') => {
            options.check = Check::by_name(value)
                .ok_or_else(|| fatal(format!("{value}: Unsupported integrity check type")))?;
        }
        Id::Letter('d') => options.mode = Mode::Decompress,
        Id::Letter('e') => options.extreme = true,
        Id::Letter('f') => options.force = true,
        Id::Letter('F') => {
            options.format = match value {
                "auto" => None,
                "xz" => Some(Format::Xz),
                "lzma" | "alone" => Some(Format::Lzma),
                "lzip" => Some(Format::Lzip),
                "raw" => Some(Format::Raw),
                _ => return Err(fatal(format!("{value}: Unknown file format type"))),
            };
        }
        Id::Letter('h') => return Err(Parsed::Exit(help(invoked, false), String::new(), 0)),
        Id::Letter('H') => return Err(Parsed::Exit(help(invoked, true), String::new(), 0)),
        Id::Letter('k') => options.keep = true,
        Id::Letter('l') => options.mode = Mode::List,
        Id::Letter('M') => memlimit("memlimit", value).map_err(fatal)?,
        Id::Memlimit(name) => memlimit(name, value).map_err(fatal)?,
        Id::Letter('q') => options.verbosity = options.verbosity.saturating_sub(1),
        Id::Letter('Q') => options.no_warn = true,
        Id::Letter('S') => {
            if value.is_empty() || value.contains(['/', '\\']) {
                return Err(fatal(format!("{value}: Invalid filename suffix")));
            }
            options.suffix = Some(value.to_owned());
        }
        Id::Letter('t') => options.mode = Mode::Test,
        Id::Letter('T') => {
            let forced = value.starts_with('+');
            let threads = value.strip_prefix('+').unwrap_or(value);
            let n = number("threads", threads, 0, 16384).map_err(fatal)?;
            let n = u32::try_from(n).unwrap_or(16384);
            options.threads = (n != 1 || forced).then_some(n);
        }
        Id::Letter('v') => options.verbosity = (options.verbosity + 1).min(4),
        Id::Letter('V') => {
            let text = if options.robot {
                "XZ_VERSION=50080032\nLIBLZMA_VERSION=50080032\n".to_owned()
            } else {
                format!("{VERSION}\n")
            };
            return Err(Parsed::Exit(text, String::new(), 0));
        }
        Id::Letter('z') => options.mode = Mode::Compress,
        // A level keeps `-e`, given before it or after.
        Id::Letter(digit @ '0'..='9') => options.preset = digit.to_digit(10).unwrap_or(6),
        Id::SingleStream => options.single_stream = true,
        Id::Files | Id::Files0 => {
            options.names = Some(NameList {
                file: Some(value).filter(|v| !v.is_empty()).map(str::to_owned),
                end: if id == Id::Files { b'\n' } else { 0 },
            });
        }
        Id::BlockSize => {
            let size = number("block-size", value, 0, (1 << 63) - 1).map_err(fatal)?;
            options.block_size = NonZeroU64::new(size);
        }
        Id::FlushTimeout => {
            number("flush-timeout", value, 0, u64::MAX).map_err(fatal)?;
        }
        Id::Filter(name) => {
            return Err(fatal(format!(
                "--{name}: Custom filter chains are not supported by cash's xz"
            )));
        }
        Id::Robot => options.robot = true,
        Id::InfoMemory => return Err(Parsed::Exit(info_memory(), String::new(), 0)),
        Id::BlockList | Id::Accepted | Id::Letter(_) => {}
    }
    Ok(())
}

/// `--info-memory`: the RAM, the processor threads, and the limits, none of them set.
fn info_memory() -> String {
    let ram = cash_win32::sysinfo::memory_status().map_or(0, |m| m.physical_total);
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let quarter = ram / 4;
    format!(
        "Hardware information:\n\
         \x20 Amount of physical memory (RAM):  {} MiB ({ram} B)\n\
         \x20 Number of processor threads:      {threads}\n\
         \n\
         Memory usage limits:\n\
         \x20 Compression:                      Disabled\n\
         \x20 Decompression:                    Disabled\n\
         \x20 Multi-threaded decompression:     {} MiB ({quarter} B)\n\
         \x20 Default for -T0:                  {} MiB ({quarter} B)\n",
        ram.div_ceil(1 << 20),
        quarter.div_ceil(1 << 20),
        quarter.div_ceil(1 << 20),
    )
}

/// Reads `words` with xz's table into `options`; operands go to the files unless
/// `operands` is false, as for the environment's words.
fn read_words(
    invoked: &str,
    options: &mut Options,
    words: &[String],
    operands: bool,
) -> Result<(), Parsed> {
    let shorts: Vec<Short<Id>> = cash_getopt::optstring(SHORTS)
        .into_iter()
        .map(|short| Short::new(short.letter, short.arg, Id::Letter(short.id)))
        .collect();
    for next in Getopt::new(&shorts, LONGS).read(words) {
        match next {
            Ok(Item::Option { id, value, .. }) => apply(invoked, options, id, value.as_deref())?,
            Ok(Item::Operand { value, .. }) => {
                if operands {
                    options.files.push(value);
                }
            }
            Err(problem) => {
                return Err(Parsed::Exit(
                    String::new(),
                    format!(
                        "{invoked}: {problem}\n{invoked}: Try '{invoked} --help' for more information.\n"
                    ),
                    1,
                ));
            }
        }
    }
    Ok(())
}

/// Reads the command line as xz 5.8 does: the invoked name first, then `XZ_DEFAULTS`,
/// `XZ_OPT` (whose operands are passed over) and the words.
fn parse(invoked: &str, environment: &[Option<String>], words: &[String]) -> Parsed {
    let mut options = Options {
        mode: Mode::Compress,
        format: None,
        stdout: false,
        keep: false,
        force: false,
        single_stream: false,
        verbosity: V_WARNING,
        no_warn: false,
        robot: false,
        preset: 6,
        extreme: false,
        check: Check::Crc64,
        block_size: None,
        threads: Some(0),
        suffix: None,
        names: None,
        files: Vec::new(),
    };
    if invoked.contains("xzcat") {
        options.mode = Mode::Decompress;
        options.stdout = true;
    } else if invoked.contains("unxz") {
        options.mode = Mode::Decompress;
    } else if invoked.contains("lzcat") {
        options.format = Some(Format::Lzma);
        options.mode = Mode::Decompress;
        options.stdout = true;
    } else if invoked.contains("unlzma") {
        options.format = Some(Format::Lzma);
        options.mode = Mode::Decompress;
    } else if invoked.contains("lzma") {
        options.format = Some(Format::Lzma);
    }
    for value in environment.iter().flatten() {
        let env_words: Vec<String> = value.split_ascii_whitespace().map(str::to_owned).collect();
        if let Err(stop) = read_words(invoked, &mut options, &env_words, false) {
            return stop;
        }
    }
    if let Err(stop) = read_words(invoked, &mut options, words, true) {
        return stop;
    }
    if options.stdout || options.mode == Mode::Test {
        options.keep = true;
        options.stdout = true;
    }
    if options.mode == Mode::Compress && options.format.is_none() {
        options.format = Some(Format::Xz);
    }
    let fatal = |text: &str| Parsed::Exit(String::new(), format!("{invoked}: {text}\n"), 1);
    if options.mode == Mode::Compress && options.format == Some(Format::Lzip) {
        return fatal("Compression of lzip files (.lz) is not supported");
    }
    if options.mode == Mode::List && options.format.is_some_and(|format| format != Format::Xz) {
        let mut text =
            format!("{invoked}: --list works only on .xz files (--format=xz or --format=auto)\n");
        if options.format == Some(Format::Lzma) {
            let _ = writeln!(text, "{invoked}: Try 'lzmainfo' with .lzma files.");
        }
        return Parsed::Exit(String::new(), text, 1);
    }
    if options.files.is_empty() && options.names.is_none() {
        options.files.push("-".to_owned());
    }
    Parsed::Run(options)
}

/// xz's `uint64_to_nicestr`: bytes below 10000 (or always, with `bytes_only`), else
/// KiB, MiB, GiB or TiB with one decimal; `also_bytes` adds the byte count to a scaled
/// one.
#[expect(clippy::cast_precision_loss, reason = "xz scales sizes in doubles")]
fn nice(value: u64, min_unit: usize, also_bytes: bool) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if min_unit == 0 && value < 10000 {
        return format!("{value} B");
    }
    let mut scaled = value as f64;
    let mut unit = 0;
    loop {
        scaled /= 1024.0;
        unit += 1;
        if !(unit < min_unit || (scaled > 9999.9 && unit < 4)) {
            break;
        }
    }
    let unit_name = UNITS.get(unit).copied().unwrap_or("TiB");
    if also_bytes && value >= 10000 {
        format!("{scaled:.1} {unit_name} ({value} B)")
    } else {
        format!("{scaled:.1} {unit_name}")
    }
}

/// xz's ratio of compressed to uncompressed, or `---` when it is past 9.999.
#[expect(
    clippy::cast_precision_loss,
    reason = "xz works the ratio out in doubles"
)]
fn ratio(compressed: u64, uncompressed: u64) -> String {
    if uncompressed == 0 {
        return "---".to_owned();
    }
    let ratio = compressed as f64 / uncompressed as f64;
    if ratio > 9.999 {
        "---".to_owned()
    } else {
        format!("{ratio:.3}")
    }
}

/// `-v`'s sizes: compressed, uncompressed, and their ratio.
#[expect(
    clippy::cast_precision_loss,
    reason = "xz works the ratio out in doubles"
)]
fn progress_sizes(compressed: u64, uncompressed: u64) -> String {
    let ratio = if uncompressed > 0 {
        compressed as f64 / uncompressed as f64
    } else {
        16.0
    };
    let ratio = if ratio > 9.999 {
        " > 9.999".to_owned()
    } else {
        format!(" = {ratio:.3}")
    };
    format!(
        "{} / {}{ratio}",
        nice(compressed, 0, false),
        nice(uncompressed, 0, false)
    )
}

/// `-v`'s speed, once three seconds have gone by.
#[expect(
    clippy::cast_precision_loss,
    reason = "xz works the speed out in doubles"
)]
fn progress_speed(uncompressed: u64, millis: u128) -> String {
    if millis < 3000 {
        return String::new();
    }
    let mut speed = uncompressed as f64 / (millis as f64 * (1024.0 / 1000.0));
    let mut unit = 0;
    for _ in 0..3 {
        if speed <= 999.0 {
            break;
        }
        speed /= 1024.0;
        unit += 1;
    }
    if unit == 3 {
        return String::new();
    }
    let name = ["KiB/s", "MiB/s", "GiB/s"]
        .get(unit)
        .copied()
        .unwrap_or("GiB/s");
    if speed > 9.9 {
        format!("{speed:.0} {name}")
    } else {
        format!("{speed:.1} {name}")
    }
}

/// `-v`'s elapsed time, once a second has gone by.
fn progress_time(millis: u128) -> String {
    let seconds = millis / 1000;
    if seconds == 0 || seconds > ((9999 * 60) + 59) * 60 + 59 {
        return String::new();
    }
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// The names xz gives the checks in a set, joined by commas.
fn check_names(set: u32, space: bool) -> String {
    let names: Vec<String> = (0..16_u8)
        .filter(|id| set & (1 << id) != 0)
        .map(check_name)
        .collect();
    names.join(if space { ", " } else { "," })
}

/// What `-l` adds up over the files.
#[derive(Debug, Default)]
struct Totals {
    files: u64,
    streams: u64,
    blocks: u64,
    compressed: u64,
    uncompressed: u64,
    padding: u64,
    checks: u32,
}

/// Why a run stopped before its last file.
#[derive(Debug)]
enum Stop {
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

/// Where one file's data comes from.
enum Source {
    Stdin,
    File(PathBuf, fs::Metadata),
}

/// One run of the command over its files.
struct Run<'a, SE: cash_core::ShellExtensions> {
    invoked: &'static str,
    options: Options,
    context: &'a cash_core::ExecutionContext<'a, SE>,
    /// 0, 2 for a warning, 1 for an error.
    status: u8,
    /// `-l`'s output, written at the end as xz's buffered standard output is.
    listing: String,
    listed_header: bool,
    totals: Totals,
    /// The number of the file being worked on, and of all named on the command line.
    file_number: usize,
    files_total: usize,
}

impl<SE: cash_core::ShellExtensions> Run<'_, SE> {
    /// The threads compressing: as `-T` says, and with `-T0` one a core, as many as a
    /// quarter of the memory holds, each with its encoder and two blocks (xz's default
    /// limit for threads it chooses itself).
    fn threads(&self) -> Option<u32> {
        let asked = self.options.threads?;
        if asked != 0 {
            return Some(asked);
        }
        let cores = u32::try_from(cash_archive::codec::parallel::threads(0)).unwrap_or(1);
        let preset = self.options.preset.min(9);
        let dict = u64::from(
            Settings {
                preset,
                extreme: self.options.extreme,
                check: self.options.check,
                block_size: None,
                threads: None,
            }
            .dict_size(),
        );
        let block = self
            .options
            .block_size
            .unwrap_or_else(|| xz::default_block(dict))
            .get();
        let each = xz::encoder_memory(preset).saturating_add(block.saturating_mul(2));
        let budget = cash_win32::process::total_physical_memory().map_or(u64::MAX, |m| m / 4);
        let fit = u32::try_from(budget / each.max(1)).unwrap_or(u32::MAX);
        Some(cores.min(fit).max(1))
    }

    fn say(&self, text: &str) -> Result<(), Stop> {
        self.context.stderr().write_all(text.as_bytes())?;
        Ok(())
    }

    /// An error: said unless `-qq`, status 1.
    fn error(&mut self, text: &str) -> Result<(), Stop> {
        if self.options.verbosity >= V_ERROR {
            self.say(&format!("{}: {text}\n", self.invoked))?;
        }
        self.status = 1;
        Ok(())
    }

    /// A warning: said unless `-q`, status 2 unless there was an error.
    fn warn(&mut self, text: &str) -> Result<(), Stop> {
        if self.options.verbosity >= V_WARNING {
            self.say(&format!("{}: {text}\n", self.invoked))?;
        }
        if self.status == 0 {
            self.status = 2;
        }
        Ok(())
    }

    /// A message at warning level that leaves the status alone.
    fn note(&self, text: &str) -> Result<(), Stop> {
        if self.options.verbosity >= V_WARNING {
            self.say(&format!("{}: {text}\n", self.invoked))?;
        }
        Ok(())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.context.shell.absolute_path(name)
    }

    /// xz's `io_open_src`: what is said of a named file that cannot be worked on, as
    /// xz checks it.
    fn open_source(&mut self, name: &str) -> Result<Option<Source>, Stop> {
        let follow = self.options.stdout || self.options.force || self.options.keep;
        let path = self.path(name);
        // Looked at only when a link would be refused: a substitution file's pipe takes
        // one opening, and its data needs that one.
        if !follow && fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            self.warn(&format!("{name}: Is a symbolic link, skipping"))?;
            return Ok(None);
        }
        let metadata = if name.is_empty() {
            Err(io::Error::from(io::ErrorKind::NotFound))
        } else {
            fs::metadata(&path)
        };
        let metadata = match metadata {
            Ok(metadata) => metadata,
            Err(e) => {
                self.error(&format!("{name}: {}", strerror(&e)))?;
                return Ok(None);
            }
        };
        if metadata.is_dir() {
            self.warn(&format!("{name}: Is a directory, skipping"))?;
            return Ok(None);
        }
        let regular_only = !self.options.stdout || self.options.mode == Mode::List;
        if regular_only && !metadata.is_file() {
            self.warn(&format!("{name}: Not a regular file, skipping"))?;
            return Ok(None);
        }
        if regular_only && !self.options.force && !self.options.keep {
            let links = cash_win32::fs::file_link_count(&path, &metadata);
            if links > 1 {
                self.warn(&format!(
                    "{name}: Input file has more than one hard link, skipping"
                ))?;
                return Ok(None);
            }
        }
        Ok(Some(Source::File(path, metadata)))
    }

    /// xz's `suffix_get_dest_name`: the output's name, or `None` when the file is
    /// skipped, said.
    fn output_name(&mut self, name: &str) -> Result<Option<String>, Stop> {
        let format = self.options.format;
        let suffix = self.options.suffix.clone();
        let tested = |known: &str| name.len() > known.len() && name.ends_with(known);
        if self.options.mode == Mode::Compress {
            let known: &[&str] = match format {
                Some(Format::Xz) => &[".xz", ".txz"],
                Some(Format::Lzma) => &[".lzma", ".tlz"],
                _ => &[],
            };
            let refused = known
                .iter()
                .copied()
                .chain(suffix.as_deref())
                .find(|known| tested(known))
                .map(str::to_owned);
            if let Some(known) = refused {
                self.warn(&format!(
                    "{name}: File already has '{known}' suffix, skipping"
                ))?;
                return Ok(None);
            }
            if format == Some(Format::Raw) && suffix.is_none() {
                self.error(&format!(
                    "{name}: With --format=raw, --suffix=.SUF is required unless writing to stdout"
                ))?;
                return Ok(None);
            }
            let added = suffix.unwrap_or_else(|| {
                if format == Some(Format::Lzma) {
                    ".lzma".to_owned()
                } else {
                    ".xz".to_owned()
                }
            });
            return Ok(Some(format!("{name}{added}")));
        }
        let mut found = None;
        if format == Some(Format::Raw) {
            if suffix.is_none() {
                self.error(&format!(
                    "{name}: With --format=raw, --suffix=.SUF is required unless writing to stdout"
                ))?;
                return Ok(None);
            }
        } else {
            const KNOWN: [(&str, &str); 5] = [
                (".xz", ""),
                (".txz", ".tar"),
                (".lzma", ""),
                (".tlz", ".tar"),
                (".lz", ""),
            ];
            found = KNOWN
                .iter()
                .find(|(known, _)| tested(known))
                .map(|(known, new)| {
                    format!(
                        "{}{new}",
                        name.get(..name.len() - known.len()).unwrap_or_default()
                    )
                });
        }
        if found.is_none() {
            if let Some(custom) = suffix.as_deref().filter(|custom| tested(custom)) {
                found = name.get(..name.len() - custom.len()).map(str::to_owned);
            }
        }
        if found.is_none() {
            self.warn(&format!("{name}: Filename has an unknown suffix, skipping"))?;
        }
        Ok(found)
    }

    /// xz's `io_open_dest` for a named file: the output file, or `None` when it was
    /// refused, said.
    fn open_output(&mut self, out_name: &str) -> Result<Option<Replacement>, Stop> {
        let path = self.path(out_name);
        let exists = fs::symlink_metadata(&path).is_ok();
        if exists && self.options.force {
            if let Err(e) = cash_win32::unix::remove_even_read_only(&path) {
                self.error(&format!("{out_name}: Cannot remove: {}", strerror(&e)))?;
                return Ok(None);
            }
        } else if exists {
            self.error(&format!("{out_name}: File exists"))?;
            return Ok(None);
        }
        match Replacement::create(path) {
            Ok(output) => Ok(Some(output)),
            Err(e) => {
                self.error(&format!("{out_name}: {}", strerror(&e)))?;
                Ok(None)
            }
        }
    }

    /// `-v`'s line about a file, at its start (a console's) or end.
    fn announce(&self, name: &str) -> Result<(), Stop> {
        if self.options.verbosity >= V_VERBOSE && is_terminal(self.context, OpenFiles::STDERR_FD) {
            let number = if self.files_total > 0 {
                format!("{}/{}", self.file_number, self.files_total)
            } else {
                self.file_number.to_string()
            };
            self.say(&format!("{name} ({number})\n"))?;
        }
        Ok(())
    }

    fn report(
        &self,
        name: &str,
        compressed: u64,
        uncompressed: u64,
        start: Instant,
    ) -> Result<(), Stop> {
        if self.options.verbosity < V_VERBOSE {
            return Ok(());
        }
        let millis = start.elapsed().as_millis();
        let sizes = progress_sizes(compressed, uncompressed);
        let speed = progress_speed(uncompressed, millis);
        let time = progress_time(millis);
        if is_terminal(self.context, OpenFiles::STDERR_FD) {
            return self.say(&format!(
                "\r {:>6} {sizes:>35}   {speed:>9} {time:>10}   {:>10}\r\n",
                "100 %", ""
            ));
        }
        let mut line = format!("{name}: {sizes}");
        if !speed.is_empty() {
            let _ = write!(line, ", {speed}");
        }
        if !time.is_empty() {
            let _ = write!(line, ", {time}");
        }
        self.say(&format!("{line}\n"))
    }

    /// A failure while coding `name` into `out_name`: said, and the run goes on.
    fn coding_failed(&mut self, failure: Broken, name: &str, out_name: &str) -> Result<(), Stop> {
        match failure {
            Broken::Truncated => self.error(&format!("{name}: Unexpected end of input")),
            Broken::Corrupt => self.error(&format!("{name}: Compressed data is corrupt")),
            Broken::Unsupported => self.error(&format!("{name}: Unsupported options")),
            Broken::Read(e) => self.error(&format!("{name}: Read error: {}", strerror(&e))),
            Broken::Write(e) if e.kind() == io::ErrorKind::BrokenPipe => Err(e.into()),
            Broken::Write(e) => self.error(&format!("{out_name}: Write error: {}", strerror(&e))),
        }
    }

    /// One file, `-` for standard input: xz's `coder_run`.
    #[expect(
        clippy::too_many_lines,
        reason = "xz's coder_run with its io_open_src and io_open_dest, one pass kept together"
    )]
    fn treat(&mut self, name: &str) -> Result<(), Stop> {
        let mode = self.options.mode;
        if mode == Mode::List {
            return self.list(name);
        }
        let is_stdin = name == "-";
        let shown = if is_stdin { "(stdin)" } else { name };
        if is_stdin {
            if mode == Mode::Compress {
                if is_terminal(self.context, OpenFiles::STDOUT_FD) {
                    return self.error("Compressed data cannot be written to a terminal");
                }
            } else if is_terminal(self.context, OpenFiles::STDIN_FD) {
                return self.error("Compressed data cannot be read from a terminal");
            }
            if self
                .options
                .names
                .as_ref()
                .is_some_and(|names| names.file.is_none())
            {
                return self.error(
                    "Cannot read data from standard input when reading filenames from standard input",
                );
            }
        }
        let source = if is_stdin {
            Source::Stdin
        } else {
            match self.open_source(name)? {
                Some(source) => source,
                None => return Ok(()),
            }
        };
        let (mut input, times, read_only): (Box<dyn Read>, _, _) = match &source {
            Source::Stdin => (Box::new(self.context.stdin()), None, false),
            Source::File(path, metadata) => match fs::File::open(path) {
                Ok(file) => (
                    Box::new(file),
                    Some(times_of(metadata)),
                    metadata.permissions().readonly(),
                ),
                Err(e) => return self.error(&format!("{name}: {}", strerror(&e))),
            },
        };

        // Decompressing or testing: the first bytes tell the format.
        let mut first = Vec::new();
        let mut format = self.options.format.unwrap_or(Format::Xz);
        let mut passthrough = false;
        if mode != Mode::Compress {
            first = match xz::sniff(&mut input) {
                Ok(first) => first,
                Err(e) => return self.error(&format!("{shown}: Read error: {}", strerror(&e))),
            };
            let found = match self.options.format {
                None => xz::detect(&first),
                Some(named) => xz::is_format(named, &first).then_some(named),
            };
            match found {
                Some(found) => format = found,
                None if mode == Mode::Decompress && self.options.stdout && self.options.force => {
                    passthrough = true;
                }
                None => return self.error(&format!("{shown}: File format not recognized")),
            }
        }

        // The output.
        let to_stdout = self.options.stdout || is_stdin;
        let (out_name, file_output) = if mode == Mode::Test {
            ("(stdout)".to_owned(), None)
        } else if to_stdout {
            if mode == Mode::Compress && is_terminal(self.context, OpenFiles::STDOUT_FD) {
                return self.error("Compressed data cannot be written to a terminal");
            }
            ("(stdout)".to_owned(), None)
        } else {
            let Some(out_name) = self.output_name(name)? else {
                return Ok(());
            };
            let Some(output) = self.open_output(&out_name)? else {
                return Ok(());
            };
            (out_name, Some(output))
        };
        self.announce(shown)?;
        let start = Instant::now();
        let mut output = match file_output {
            Some(file) => Output::File(file),
            None if mode == Mode::Test => Output::Sink,
            None => Output::Stdout(Box::new(self.context.stdout())),
        };

        let outcome = if passthrough {
            let mut all = io::Cursor::new(first).chain(input);
            let mut written = Counted::new(&mut output);
            io::copy(&mut all, &mut written)
                .map(|n| (n, written.count))
                .map_err(Broken::Write)
        } else if mode == Mode::Compress {
            let settings = Settings {
                preset: self.options.preset,
                extreme: self.options.extreme,
                check: self.options.check,
                block_size: self.options.block_size,
                threads: self.threads(),
            };
            let mut written = Counted::new(&mut output);
            let mut read = Counted::new(&mut input);
            let coded = xz::compressor(format, &settings, &mut written)
                .map_err(Broken::Write)
                .and_then(|mut encoder| {
                    copy_into(&mut read, &mut encoder)?;
                    encoder.finish().map_err(Broken::Write)
                });
            coded.map(|()| (written.count, read.count))
        } else {
            let settings = Settings {
                preset: self.options.preset,
                extreme: self.options.extreme,
                check: self.options.check,
                block_size: None,
                threads: None,
            };
            let mut written = Counted::new(&mut output);
            // A file of several blocks, on every core.
            let threads = self
                .options
                .threads
                .map_or(1, cash_archive::codec::parallel::threads);
            let threaded = match &source {
                Source::File(path, _) if format == Format::Xz && !self.options.single_stream => {
                    fs::File::open(path).ok().and_then(|mut file| {
                        xz::decompress_file_mt(&mut file, &mut written, threads)
                    })
                }
                _ => None,
            };
            if let Some(threaded) = threaded {
                threaded.map(|read| (read, written.count))
            } else {
                let mut all =
                    BufReader::with_capacity(64 << 10, io::Cursor::new(first).chain(input));
                xz::decompress(
                    format,
                    &mut all,
                    &mut written,
                    self.options.single_stream,
                    settings.dict_size(),
                )
                .map(|read| (read, written.count))
            }
        };
        match outcome {
            Ok((compressed, uncompressed)) => {
                if let Err(e) = output.finish(times, read_only) {
                    return self.error(&format!("{out_name}: {}", strerror(&e)));
                }
                self.report(shown, compressed, uncompressed, start)?;
                if let Source::File(path, _) = &source {
                    if !self.options.keep {
                        if let Err(e) = cash_win32::unix::remove_even_read_only(path) {
                            self.error(&format!("{name}: Cannot remove: {}", strerror(&e)))?;
                        }
                    }
                }
                Ok(())
            }
            Err(failure) => {
                output.abandon();
                self.coding_failed(failure, shown, &out_name)
            }
        }
    }

    /// `-l` for one file: xz's `list_file`.
    fn list(&mut self, name: &str) -> Result<(), Stop> {
        if name == "-" {
            return self.error("--list does not support reading from standard input");
        }
        let Some(Source::File(path, _)) = self.open_source(name)? else {
            return Ok(());
        };
        let info = fs::File::open(&path)
            .map_err(InfoProblem::Read)
            .and_then(|file| xz::file_info(&mut BufReader::new(file)));
        let info = match info {
            Ok(info) => info,
            Err(problem) => {
                let text = match problem {
                    InfoProblem::Empty => "File is empty".to_owned(),
                    InfoProblem::TooSmall => "Too small to be a valid .xz file".to_owned(),
                    InfoProblem::NotRecognized => "File format not recognized".to_owned(),
                    InfoProblem::Corrupt => "Compressed data is corrupt".to_owned(),
                    InfoProblem::Unsupported => "Unsupported options".to_owned(),
                    InfoProblem::Read(e) => strerror(&e),
                };
                return self.error(&format!("{name}: {text}"));
            }
        };
        let totals = &mut self.totals;
        totals.files += 1;
        totals.streams += info.streams.len() as u64;
        totals.blocks += info.block_count();
        totals.compressed += info.file_size();
        totals.uncompressed += info.uncomp_size();
        totals.padding += info.padding();
        totals.checks |= info.checks();
        if self.options.robot {
            self.list_robot(name, &info);
        } else if self.options.verbosity >= V_VERBOSE {
            self.list_detailed(name, &info);
        } else {
            self.list_basic(name, &info);
        }
        Ok(())
    }

    fn list_basic(&mut self, name: &str, info: &FileInfo) {
        if !self.listed_header {
            self.listed_header = true;
            self.listing
                .push_str("Strms  Blocks   Compressed Uncompressed  Ratio  Check   Filename\n");
        }
        let _ = writeln!(
            self.listing,
            "{:>5} {:>7}  {:>11}  {:>11}  {:>5}  {:<7} {name}",
            info.streams.len(),
            info.block_count(),
            nice(info.file_size(), 0, false),
            nice(info.uncomp_size(), 0, false),
            ratio(info.file_size(), info.uncomp_size()),
            check_names(info.checks(), false),
        );
    }

    /// The summary lines of `-lv`, for a file or for the totals.
    fn summary(
        &mut self,
        streams: u64,
        blocks: u64,
        compressed: u64,
        uncompressed: u64,
        checks: u32,
        padding: u64,
    ) {
        let _ = writeln!(
            self.listing,
            "  Streams:           {streams}\n\
             \x20 Blocks:            {blocks}\n\
             \x20 Compressed size:   {}\n\
             \x20 Uncompressed size: {}\n\
             \x20 Ratio:             {}\n\
             \x20 Check:             {}\n\
             \x20 Stream Padding:    {}",
            nice(compressed, 0, true),
            nice(uncompressed, 0, true),
            ratio(compressed, uncompressed),
            check_names(checks, true),
            nice(padding, 0, true),
        );
    }

    fn list_detailed(&mut self, name: &str, info: &FileInfo) {
        if self.totals.files > 1 {
            self.listing.push('\n');
        }
        let _ = writeln!(
            self.listing,
            "{name} ({}/{})",
            self.file_number, self.files_total
        );
        self.summary(
            info.streams.len() as u64,
            info.block_count(),
            info.file_size(),
            info.uncomp_size(),
            info.checks(),
            info.padding(),
        );
        self.listing.push_str(
            "  Streams:\n    Stream    Blocks      CompOffset    UncompOffset        \
             CompSize      UncompSize  Ratio  Check      Padding\n",
        );
        for (number, stream) in info.streams.iter().enumerate() {
            let _ = writeln!(
                self.listing,
                "    {:>6} {:>9} {:>15} {:>15} {:>15} {:>15}  {:>5}  {:<10} {:>7}",
                number + 1,
                stream.blocks.len(),
                stream.comp_offset,
                stream.uncomp_offset,
                stream.comp_size,
                stream.uncomp_size,
                ratio(stream.comp_size, stream.uncomp_size),
                check_name(stream.check),
                stream.padding,
            );
        }
        if info.block_count() == 0 {
            return;
        }
        self.listing.push_str(
            "  Blocks:\n    Stream     Block      CompOffset    UncompOffset       \
             TotalSize      UncompSize  Ratio  Check\n",
        );
        for (number, stream) in info.streams.iter().enumerate() {
            for (block_number, block) in stream.blocks.iter().enumerate() {
                let _ = writeln!(
                    self.listing,
                    "    {:>6} {:>9} {:>15} {:>15} {:>15} {:>15}  {:>5}  {}",
                    number + 1,
                    block_number + 1,
                    block.comp_offset,
                    block.uncomp_offset,
                    block.total_size(),
                    block.uncomp_size,
                    ratio(block.total_size(), block.uncomp_size),
                    check_name(stream.check),
                );
            }
        }
    }

    fn list_robot(&mut self, name: &str, info: &FileInfo) {
        let _ = writeln!(
            self.listing,
            "name\t{name}\nfile\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            info.streams.len(),
            info.block_count(),
            info.file_size(),
            info.uncomp_size(),
            ratio(info.file_size(), info.uncomp_size()),
            check_names(info.checks(), false),
            info.padding(),
        );
        if self.options.verbosity < V_VERBOSE {
            return;
        }
        for (number, stream) in info.streams.iter().enumerate() {
            let _ = writeln!(
                self.listing,
                "stream\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                number + 1,
                stream.blocks.len(),
                stream.comp_offset,
                stream.uncomp_offset,
                stream.comp_size,
                stream.uncomp_size,
                ratio(stream.comp_size, stream.uncomp_size),
                check_name(stream.check),
                stream.padding,
            );
        }
        let mut in_file = 0;
        for (number, stream) in info.streams.iter().enumerate() {
            for (block_number, block) in stream.blocks.iter().enumerate() {
                in_file += 1;
                let _ = writeln!(
                    self.listing,
                    "block\t{}\t{}\t{in_file}\t{}\t{}\t{}\t{}\t{}\t{}",
                    number + 1,
                    block_number + 1,
                    block.comp_offset,
                    block.uncomp_offset,
                    block.total_size(),
                    block.uncomp_size,
                    ratio(block.total_size(), block.uncomp_size),
                    check_name(stream.check),
                );
            }
        }
    }

    /// `-l`'s totals: xz's `list_totals`.
    fn list_totals(&mut self) {
        let t = std::mem::take(&mut self.totals);
        if self.options.robot {
            let _ = writeln!(
                self.listing,
                "totals\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                t.streams,
                t.blocks,
                t.compressed,
                t.uncompressed,
                ratio(t.compressed, t.uncompressed),
                check_names(t.checks, false),
                t.padding,
                t.files,
            );
        } else if t.files > 1 {
            if self.options.verbosity >= V_VERBOSE {
                let _ = writeln!(self.listing, "\nTotals:\n  Number of files:   {}", t.files);
                self.summary(
                    t.streams,
                    t.blocks,
                    t.compressed,
                    t.uncompressed,
                    t.checks,
                    t.padding,
                );
            } else {
                let _ = writeln!(self.listing, "{}", "-".repeat(79));
                let _ = writeln!(
                    self.listing,
                    "{:>5} {:>7}  {:>11}  {:>11}  {:>5}  {:<7} {} files",
                    t.streams,
                    t.blocks,
                    nice(t.compressed, 0, false),
                    nice(t.uncompressed, 0, false),
                    ratio(t.compressed, t.uncompressed),
                    check_names(t.checks, false),
                    t.files,
                );
            }
        }
    }

    /// The names `--files` or `--files0` gives, each in turn, and what is wrong after
    /// the last of them: xz says it once it gets there.
    fn read_names(&self, list: &NameList) -> (Vec<String>, Option<String>) {
        let mut bytes = Vec::new();
        let shown = list.file.clone().unwrap_or_else(|| "(stdin)".to_owned());
        let read = match &list.file {
            Some(file) => fs::read(self.path(file)).map(|read| bytes = read),
            None => self.context.stdin().read_to_end(&mut bytes).map(|_| ()),
        };
        if let Err(e) = read {
            return (Vec::new(), Some(format!("{shown}: {}", strerror(&e))));
        }
        let mut names = Vec::new();
        let mut pieces = bytes.split(|byte| *byte == list.end).peekable();
        while let Some(piece) = pieces.next() {
            if pieces.peek().is_none() {
                let trouble = (!piece.is_empty())
                    .then(|| format!("{shown}: Unexpected end of input when reading filenames"));
                return (names, trouble);
            }
            if list.end == b'\n' && piece.contains(&0) {
                return (
                    names,
                    Some(format!(
                        "{shown}: Null character found when reading filenames; maybe you \
                         meant to use '--files0' instead of '--files'?"
                    )),
                );
            }
            if !piece.is_empty() {
                names.push(String::from_utf8_lossy(piece).into_owned());
            }
        }
        (names, None)
    }

    fn run_all(&mut self) -> Result<(), Stop> {
        if self.options.format == Some(Format::Raw) && self.options.mode != Mode::List {
            self.note("Using a preset in raw mode is discouraged.")?;
            self.note("The exact options of the presets may vary between software versions.")?;
        }
        let files = self.options.files.clone();
        self.files_total = if self.options.names.is_some() {
            0
        } else {
            files.len()
        };
        for file in &files {
            self.file_number += 1;
            self.treat(file)?;
        }
        if let Some(list) = self.options.names.clone() {
            let (names, trouble) = self.read_names(&list);
            for name in names {
                self.file_number += 1;
                self.treat(&name)?;
            }
            if let Some(trouble) = trouble {
                self.error(&trouble)?;
            }
        }
        if self.options.mode == Mode::List {
            self.list_totals();
        }
        Ok(())
    }
}

/// Copies `input` into `encoder`, telling reading trouble from writing trouble.
fn copy_into(
    input: &mut dyn Read,
    encoder: &mut Box<dyn cash_archive::codec::Encoder + '_>,
) -> Result<(), Broken> {
    let mut buffer = vec![0_u8; 64 << 10];
    loop {
        let n = match input.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Broken::Read(e)),
        };
        encoder
            .write_all(buffer.get(..n).unwrap_or_default())
            .map_err(Broken::Write)?;
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
    invoked: &'static str,
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    let environment = [
        exported(context, "XZ_DEFAULTS"),
        exported(context, "XZ_OPT"),
    ];
    let options = match parse(invoked, &environment, args) {
        Parsed::Run(options) => options,
        Parsed::Exit(out, err, status) => {
            context.stdout().write_all(out.as_bytes())?;
            context.stderr().write_all(err.as_bytes())?;
            return Ok(ExecutionResult::new(status));
        }
    };
    let no_warn = options.no_warn;
    let mut run = Run {
        invoked,
        options,
        context,
        status: 0,
        listing: String::new(),
        listed_header: false,
        totals: Totals::default(),
        file_number: 0,
        files_total: 0,
    };
    let outcome = run.run_all();
    if !run.listing.is_empty() {
        let mut stdout = context.stdout();
        stdout.write_all(run.listing.as_bytes())?;
        stdout.flush()?;
    }
    match outcome {
        Ok(()) => {
            let status = if run.status == 2 && no_warn {
                0
            } else {
                run.status
            };
            Ok(ExecutionResult::new(status))
        }
        Err(Stop::Shell(error)) => Err(error),
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test stops on what it did not expect")]
mod tests {
    use super::*;

    fn options(invoked: &str, words: &[&str]) -> Options {
        let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
        match parse(invoked, &[None, None], &words) {
            Parsed::Run(options) => options,
            Parsed::Exit(out, err, status) => panic!("exit {status}: {out}{err}"),
        }
    }

    fn refusal(invoked: &str, words: &[&str]) -> String {
        let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
        match parse(invoked, &[None, None], &words) {
            Parsed::Run(_) => panic!("ran"),
            Parsed::Exit(_, err, _) => err,
        }
    }

    #[test]
    fn the_name_sets_the_mode_and_the_format() {
        let o = options("unlzma", &["f"]);
        assert_eq!(
            (o.mode, o.format, o.stdout),
            (Mode::Decompress, Some(Format::Lzma), false)
        );
        let o = options("xzcat", &[]);
        assert_eq!(
            (o.mode, o.stdout, o.keep, o.files.len()),
            (Mode::Decompress, true, true, 1)
        );
        assert_eq!(options("xz", &[]).format, Some(Format::Xz));
        assert_eq!(options("xz", &["-t"]).format, None);
    }

    #[test]
    fn values_are_checked_as_xz_checks_them() {
        assert_eq!(number("threads", "4", 0, 16384), Ok(4));
        assert_eq!(number("block-size", "2MiB", 0, u64::MAX), Ok(2 << 20));
        assert_eq!(number("threads", "max", 0, 16384), Ok(16384));
        assert_eq!(
            number("threads", "x", 0, 16384),
            Err("x: Value is not a non-negative decimal integer".to_owned())
        );
        assert_eq!(
            number("threads", "99999", 0, 16384),
            Err("Value of the option 'threads' must be in the range [0, 16384]".to_owned())
        );
        assert!(refusal("xz", &["--de"]).contains("possibilities: '--decompress' '--delta'"));
        assert_eq!(
            refusal("xz", &["-S", ""]),
            "xz: : Invalid filename suffix\n"
        );
        assert_eq!(
            refusal("xz", &["-F", "lzip"]),
            "xz: Compression of lzip files (.lz) is not supported\n"
        );
    }

    #[test]
    fn sizes_and_ratios_read_as_xz_prints_them() {
        assert_eq!(nice(72, 0, false), "72 B");
        assert_eq!(nice(300_000, 0, false), "293.0 KiB");
        assert_eq!(nice(3_000_213, 0, true), "2929.9 KiB (3000213 B)");
        assert_eq!(progress_sizes(72, 6), "72 B / 6 B > 9.999");
        assert_eq!(ratio(40, 6), "6.667");
        assert_eq!(ratio(72, 0), "---");
        assert_eq!(check_names(0b100_0000_0010, true), "CRC32, SHA-256");
    }
}
