//! `bzip2`, `bunzip2` and `bzcat`: bzip2 1.0.8's interface, messages and exit codes, on
//! libbz2-rs-sys, libbzip2's own code ported to Rust, through the `bzip2` crate.
//!
//! Checked against bzip2 1.0.8 (`crates/cash/tests/oracle/bzip2_cases.sh`): every
//! message and status, the decompressed bytes, and the compressed ones, which are
//! libbzip2's byte for byte.
//!
//! On a clean Windows machine there is no `bzip2`; `tar.exe` reads a `.tar.bz2`, not a
//! bare `.bz2`.
//!
//! Where cash differs, on purpose:
//! - The version line, at the top of the usage and for `-V` and `-L`, is cash's.
//! - `-vv` and more say what `-v` says: the block-by-block trace is printed by libbzip2
//!   itself.
//! - Windows has no mode bits but read-only, which is carried over; the owner is not set.
//! - A file is written beside its target under a temporary name and renamed over it at
//!   the end, so an interrupted run leaves no half-written file.
//!
//! What bzip2 1.0.8 does that looks odd is kept, since scripts see it: flags are read
//! anywhere among the files, `-` is no file unless it comes after `--`, `-V` prints the
//! licence and goes on, `-t -q` on trailing garbage says the file's name and no more,
//! and `-f` decompressing a file with trailing garbage copies the whole file after
//! what it decoded. The reason given when a compressed file ends early is the C
//! library's last error, left from an earlier call; cash gives the one bzip2 1.0.8
//! gives in each case.

use std::fs;
use std::io::{self, BufReader, Read, Seek, Write};
use std::path::PathBuf;

use cash_archive::codec::bzip2::{Broken, Ending, decompress_streams};
use cash_archive::codec::{self, Codec};
use cash_core::openfiles::{OpenFile, OpenFiles};
use cash_core::{ExecutionResult, builtins};
use cash_win32::unix::Replacement;
use clap::Parser;

use crate::compress::{Counted, Output, is_terminal, strerror, times_of};

/// The first line of the usage and of `-V`.
const HEADER: &str = "bzip2 (cash): bzip2 1.0.8's options, on libbz2-rs-sys";

/// The compressed suffixes, and what each becomes when the file is decompressed.
const SUFFIXES: [(&str, &str); 4] = [
    (".bz2", ""),
    (".bz", ""),
    (".tbz2", ".tar"),
    (".tbz", ".tar"),
];

/// What bzip2 reads and writes at a time.
const CHUNK: usize = 5000;

/// C's buffer for standard output on a pipe: what bzip2 says on standard error comes
/// before the data still in it.
const STDOUT_BUFFER: usize = 4096;

/// Compress or decompress files with bzip2 1.0.8's options.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct Bzip2Command {
    /// Flags and files, in any order, read here as bzip2 reads them.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Decompress files: `bzip2 -d`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct Bunzip2Command {
    /// Flags and files, read here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Decompress files to standard output: `bzip2 -dc`.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct BzcatCommand {
    /// Flags and files, read here.
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

command!(Bzip2Command, "bzip2");
command!(Bunzip2Command, "bunzip2");
command!(BzcatCommand, "bzcat");

/// What is done to each file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Op {
    Compress,
    Decompress,
    Test,
}

/// Where the input comes from and the output goes: bzip2's `srcMode`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    /// Standard input to standard output: no file was named.
    Stdin,
    /// Each file to a file beside it.
    Files,
    /// Each file to standard output: `-c`.
    ToStdout,
}

#[derive(Debug)]
struct Options {
    op: Op,
    mode: Mode,
    force: bool,
    keep: bool,
    /// Not `-q`.
    noisy: bool,
    verbosity: u32,
    /// The block size in hundreds of thousands of bytes, 1 to 9.
    block: u32,
    /// The files, in order.
    files: Vec<String>,
    /// The longest file's name, at least 7 bytes: `-v` pads names to it.
    longest: usize,
}

/// Text written while the command line was read, in its order.
#[derive(Debug)]
enum Said {
    Out(String),
    Err(String),
}

#[derive(Debug)]
enum Parsed {
    Run(Options),
    /// The command is over, with this status.
    Exit(u8),
}

fn license() -> String {
    format!("{HEADER}\nMIT licence. libbz2-rs-sys is libbzip2 in Rust (bzip2-1.0.6 licence).\n")
}

fn usage(invoked: &str) -> String {
    format!(
        "{HEADER}\n\
         \n   usage: {invoked} [flags and input files in any order]\n\n\
         \x20  -h --help           print this message\n\
         \x20  -d --decompress     force decompression\n\
         \x20  -z --compress       force compression\n\
         \x20  -k --keep           keep (don't delete) input files\n\
         \x20  -f --force          overwrite existing output files\n\
         \x20  -t --test           test compressed file integrity\n\
         \x20  -c --stdout         output to standard out\n\
         \x20  -q --quiet          suppress noncritical error messages\n\
         \x20  -v --verbose        be verbose (a 2nd -v gives more)\n\
         \x20  -L --license        display software version & license\n\
         \x20  -V --version        display software version & license\n\
         \x20  -s --small          use less memory (at most 2500k)\n\
         \x20  -1 .. -9            set block size to 100k .. 900k\n\
         \x20  --fast              alias for -1\n\
         \x20  --best              alias for -9\n\
         \n\
         \x20  If invoked as `bzip2', default action is to compress.\n\
         \x20             as `bunzip2',  default action is to decompress.\n\
         \x20             as `bzcat', default action is to decompress to stdout.\n\
         \n\
         \x20  If no file names are given, bzip2 compresses or decompresses\n\
         \x20  from standard input to standard output.  You can combine\n\
         \x20  short flags, so `-v -4' means the same as -v4 or -4v, &c.\n\
         \n"
    )
}

fn bad_flag(invoked: &str, word: &str, said: &mut Vec<Said>) -> Parsed {
    said.push(Said::Err(format!("{invoked}: Bad flag `{word}'\n")));
    said.push(Said::Err(usage(invoked)));
    Parsed::Exit(1)
}

/// The files among `words`: every word not starting with `-`, and every word after `--`
/// but another `--`; and the longest one's length, at least 7.
fn operands(words: &[String]) -> (Vec<String>, usize) {
    let mut files = Vec::new();
    let mut longest = 7;
    let mut flags_end = false;
    for word in words {
        if word == "--" {
            flags_end = true;
        } else if !word.starts_with('-') || flags_end {
            longest = longest.max(word.len());
            files.push(word.clone());
        }
    }
    (files, longest)
}

/// Reads the words as bzip2 1.0.8 does: the files first (every word not starting with
/// `-`, and every word after `--`), then the short flags of every word before `--`,
/// then its long ones, each pass in order.
#[expect(
    clippy::too_many_lines,
    reason = "bzip2's two passes over its flags, kept side by side as its main has them"
)]
fn parse(invoked: &str, words: &[String], said: &mut Vec<Said>) -> Parsed {
    let (files, longest) = operands(words);
    let mut mode = if files.is_empty() {
        Mode::Stdin
    } else {
        Mode::Files
    };
    let mut op = if invoked.contains("unzip") || invoked.contains("UNZIP") {
        Op::Decompress
    } else {
        Op::Compress
    };
    if ["z2cat", "Z2CAT", "zcat", "ZCAT"]
        .iter()
        .any(|name| invoked.contains(name))
    {
        op = Op::Decompress;
        mode = if files.is_empty() {
            Mode::Stdin
        } else {
            Mode::ToStdout
        };
    }
    let (mut force, mut keep, mut small, mut noisy) = (false, false, false, true);
    let mut verbosity = 0_u32;
    let mut block = 9_u32;

    let flag_words = words.iter().take_while(|word| *word != "--");
    for word in flag_words.clone() {
        let Some(letters) = word.strip_prefix('-') else {
            continue;
        };
        if letters.starts_with('-') {
            continue;
        }
        for letter in letters.chars() {
            match letter {
                'c' => mode = Mode::ToStdout,
                'd' => op = Op::Decompress,
                'z' => op = Op::Compress,
                'f' => force = true,
                't' => op = Op::Test,
                'k' => keep = true,
                's' => small = true,
                'q' => noisy = false,
                'V' | 'L' => said.push(Said::Out(license())),
                'v' => verbosity += 1,
                'h' => {
                    said.push(Said::Err(usage(invoked)));
                    return Parsed::Exit(0);
                }
                _ => match letter.to_digit(10).filter(|digit| *digit >= 1) {
                    Some(digit) => block = digit,
                    None => return bad_flag(invoked, word, said),
                },
            }
        }
    }
    for word in flag_words {
        match word.as_str() {
            "--stdout" => mode = Mode::ToStdout,
            "--decompress" => op = Op::Decompress,
            "--compress" => op = Op::Compress,
            "--force" => force = true,
            "--test" => op = Op::Test,
            "--keep" => keep = true,
            "--small" => small = true,
            "--quiet" => noisy = false,
            "--version" | "--license" => said.push(Said::Out(license())),
            // A work factor of 1 changes how blocks are sorted, not what they become.
            "--exponential" => {}
            "--repetitive-best" | "--repetitive-fast" => said.push(Said::Err(format!(
                "{invoked}: {word} is redundant in versions 0.9.5 and above\n"
            ))),
            "--fast" => block = 1,
            "--best" => block = 9,
            "--verbose" => verbosity += 1,
            "--help" => {
                said.push(Said::Err(usage(invoked)));
                return Parsed::Exit(0);
            }
            other if other.starts_with("--") => return bad_flag(invoked, word, said),
            _ => {}
        }
    }
    if op == Op::Compress && small {
        block = block.min(2);
    }
    if op == Op::Test && mode == Mode::ToStdout {
        said.push(Said::Err(format!(
            "{invoked}: -c and -t cannot be used together.\n"
        )));
        return Parsed::Exit(1);
    }
    if mode == Mode::ToStdout && files.is_empty() {
        mode = Mode::Stdin;
    }
    Parsed::Run(Options {
        op,
        mode,
        force,
        keep,
        noisy,
        verbosity: verbosity.min(4),
        block,
        files,
        longest,
    })
}

/// `-v`'s numbers for a compressed file.
#[expect(
    clippy::cast_precision_loss,
    reason = "bzip2 works the ratios out in doubles"
)]
fn stats(bytes_in: u64, bytes_out: u64) -> String {
    let (read, written) = (bytes_in as f64, bytes_out as f64);
    format!(
        "{:6.3}:1, {:6.3} bits/byte, {:5.2}% saved, {bytes_in} in, {bytes_out} out.\n",
        read / written,
        8.0 * written / read,
        100.0 * (1.0 - written / read)
    )
}

/// Compresses all of `input` into `output` as one bzip2 stream with blocks of `block`
/// hundred thousand bytes: the bytes read and written.
fn compress_stream(
    input: &mut dyn Read,
    output: &mut dyn Write,
    block: u32,
) -> io::Result<(u64, u64)> {
    let mut written = Counted::new(output);
    // pbzip2's layout on every core: a stream of each block's input.
    let mut encoder = codec::writer_on(Codec::Bzip2, &mut written, block, 0)?;
    let mut buffer = vec![0_u8; CHUNK];
    let mut read = 0_u64;
    loop {
        let n = match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        read += n as u64;
        encoder.write_all(buffer.get(..n).unwrap_or_default())?;
    }
    encoder.finish()?;
    Ok((read, written.count))
}

/// The output being written to a file, taken back from `output`.
fn into_file(output: Output<'_>) -> Option<Replacement> {
    match output {
        Output::File(file) => Some(file),
        Output::Stdout(_) | Output::Sink => None,
    }
}

/// `input` from its first byte again, as bzip2's `rewind` has it: a file is opened
/// again, standard input read from its start when it is a file. A pipe cannot go back,
/// and what libbzip2 had read of it is lost, as it is in bzip2.
fn rewound<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    input: BufReader<Box<dyn Read>>,
    path: Option<&PathBuf>,
) -> io::Result<Box<dyn Read>> {
    if let Some(path) = path {
        return Ok(Box::new(fs::File::open(path)?));
    }
    if let Some(OpenFile::File(file)) = context.try_fd(OpenFiles::STDIN_FD) {
        (&*file).seek(io::SeekFrom::Start(0))?;
        return Ok(Box::new(context.stdin()));
    }
    Ok(input.into_inner())
}

/// Why a run stopped before its last file.
#[derive(Debug)]
enum Stop {
    /// bzip2 exits here: everything is said and the status set.
    Exit,
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

/// One run of the command over its files.
struct Run<'a, SE: cash_core::ShellExtensions> {
    invoked: &'static str,
    options: Options,
    context: &'a cash_core::ExecutionContext<'a, SE>,
    stdout: io::BufWriter<Box<dyn Write>>,
    /// The exit status: the highest set so far.
    status: u8,
    /// The names bzip2's long messages show, its `inName` and `outName`.
    in_name: String,
    out_name: String,
    /// The files begun so far.
    begun: usize,
    /// A file was not bzip2's (decompressing) or failed its test.
    failed: bool,
}

impl<SE: cash_core::ShellExtensions> Run<'_, SE> {
    fn set_exit(&mut self, status: u8) {
        self.status = self.status.max(status);
    }

    fn say(&self, text: &str) -> Result<(), Stop> {
        self.context.stderr().write_all(text.as_bytes())?;
        Ok(())
    }

    /// `text` said about a file that is then left alone: status 1, the run goes on.
    fn refuse(&mut self, text: &str) -> Result<(), Stop> {
        self.say(text)?;
        self.set_exit(1);
        Ok(())
    }

    /// The refusal to read compressed data from a console or write it to one.
    fn refuse_terminal(&mut self, way: &str) -> Result<(), Stop> {
        let p = self.invoked;
        self.refuse(&format!(
            "{p}: I won't {way} a terminal.\n{p}: For help, type: `{p} --help'.\n"
        ))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.context.shell.absolute_path(name)
    }

    /// What `name` is, as bzip2 finds out whether it can open it; the empty name is no
    /// file.
    fn metadata(&self, name: &str) -> io::Result<fs::Metadata> {
        if name.is_empty() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        fs::metadata(self.path(name))
    }

    /// `-v`'s start of the line about the file being worked on, padded.
    fn announce(&self) -> Result<(), Stop> {
        if self.options.verbosity >= 1 {
            let pad = " ".repeat(self.options.longest.saturating_sub(self.in_name.len()));
            self.say(&format!("  {}: {pad}", self.in_name))?;
        }
        Ok(())
    }

    /// Whether the output may be written: it is not there, or `-f` removes it now, as
    /// bzip2 does.
    fn clear_output(&mut self) -> Result<bool, Stop> {
        let out_name = self.out_name.clone();
        if self.metadata(&out_name).is_err() {
            return Ok(true);
        }
        if self.options.force {
            let path = self.path(&out_name);
            let _ = if path.is_dir() {
                fs::remove_dir(&path)
            } else {
                cash_win32::unix::remove_even_read_only(&path)
            };
            return Ok(true);
        }
        let p = self.invoked;
        self.refuse(&format!("{p}: Output file {out_name} already exists.\n"))?;
        Ok(false)
    }

    /// Whether the input may be replaced: with `-f`, or when it has no other hard link.
    fn alone(&mut self) -> Result<bool, Stop> {
        if self.options.force {
            return Ok(true);
        }
        let path = self.path(&self.in_name);
        let others = fs::metadata(&path).map_or(0, |meta| {
            cash_win32::fs::file_link_count(&path, &meta).saturating_sub(1)
        });
        if others == 0 {
            return Ok(true);
        }
        let (p, name) = (self.invoked, self.in_name.clone());
        let plural = if others > 1 { "s" } else { "" };
        self.refuse(&format!(
            "{p}: Input file {name} has {others} other link{plural}.\n"
        ))?;
        Ok(false)
    }

    /// Whether the input is a regular file, not a link, a device or anything else.
    fn normal_file(&self) -> bool {
        fs::symlink_metadata(self.path(&self.in_name)).is_ok_and(|m| m.file_type().is_file())
    }

    /// The input removed once its output is whole, unless `-k`.
    fn remove_input(&mut self) -> Result<(), Stop> {
        if self.options.mode == Mode::Files && !self.options.keep {
            if let Err(e) = cash_win32::unix::remove_even_read_only(&self.path(&self.in_name)) {
                return Err(self.io_failed(e, None));
            }
        }
        Ok(())
    }

    fn show_file_names(&self) -> Result<(), Stop> {
        if self.options.noisy {
            self.say(&format!(
                "\tInput file = {}, output file = {}\n",
                self.in_name, self.out_name
            ))?;
        }
        Ok(())
    }

    fn advise(&self) -> Result<(), Stop> {
        if self.options.noisy {
            self.say(
                "\nIt is possible that the compressed file(s) have become corrupted.\n\
                 You can use the -tvv option to test integrity of such files.\n\n\
                 You can use the `bzip2recover' program to attempt to recover\n\
                 data from undamaged sections of corrupted files.\n\n",
            )?;
        }
        Ok(())
    }

    /// bzip2's `cleanUpAndFail`: the output being written is removed (or kept, when
    /// the input has gone), the files not reached are counted, and the run ends.
    fn give_up(&mut self, status: u8, output: Option<Replacement>) -> Stop {
        match self.say_give_up(output) {
            Ok(()) => {
                self.set_exit(status);
                Stop::Exit
            }
            Err(stop) => stop,
        }
    }

    fn say_give_up(&self, output: Option<Replacement>) -> Result<(), Stop> {
        let p = self.invoked;
        if let Some(output) = output {
            if fs::metadata(self.path(&self.in_name)).is_ok() {
                output.abandon();
                if self.options.noisy {
                    self.say(&format!(
                        "{p}: Deleting output file {}, if it exists.\n",
                        self.out_name
                    ))?;
                }
            } else {
                let _ = output.finish(fs::FileTimes::new(), false, false);
                self.say(&format!(
                    "{p}: WARNING: deletion of output file suppressed\n\
                     {p}:    since input file no longer exists.  Output file\n\
                     {p}:    `{}' may be incomplete.\n\
                     {p}:    I suggest doing an integrity test (bzip2 -tv) of it.\n",
                    self.out_name
                ))?;
            }
        }
        let total = self.options.files.len();
        if self.options.noisy && total > 0 && self.begun < total {
            self.say(&format!(
                "{p}: WARNING: some files have not been processed:\n\
                 {p}:    {total} specified on command line, {} not processed yet.\n\n",
                total - self.begun
            ))?;
        }
        Ok(())
    }

    /// bzip2's `ioError`: reading or writing failed. A reader of standard output that
    /// went away ends the run in silence, as `SIGPIPE` ends bzip2.
    fn io_failed(&mut self, error: io::Error, output: Option<Replacement>) -> Stop {
        if error.kind() == io::ErrorKind::BrokenPipe {
            if let Some(output) = output {
                output.abandon();
            }
            return Stop::Shell(error.into());
        }
        let p = self.invoked;
        let said = self
            .say(&format!(
                "\n{p}: I/O or other error, bailing out.  Possible reason follows.\n\
                 {p}: {}\n",
                strerror(&error)
            ))
            .and_then(|()| self.show_file_names());
        match said {
            Ok(()) => self.give_up(1, output),
            Err(stop) => stop,
        }
    }

    /// bzip2's `crcError`: the data inside a stream is damaged.
    fn data_failed(&mut self, output: Option<Replacement>) -> Stop {
        let p = self.invoked;
        let said = self
            .say(&format!(
                "\n{p}: Data integrity error when decompressing.\n"
            ))
            .and_then(|()| self.show_file_names())
            .and_then(|()| self.advise());
        match said {
            Ok(()) => self.give_up(2, output),
            Err(stop) => stop,
        }
    }

    /// bzip2's `compressedStreamEOF`: the input ends inside a stream. The reason is the
    /// C library's last error, which bzip2 1.0.8 leaves as it is here for each mode.
    fn ended_early(&mut self, output: Option<Replacement>) -> Stop {
        let p = self.invoked;
        let stale = match self.options.mode {
            Mode::Files => "No such file or directory",
            Mode::ToStdout => "Success",
            Mode::Stdin => "Inappropriate ioctl for device",
        };
        let said = if self.options.noisy {
            self.say(&format!(
                "\n{p}: Compressed file ends unexpectedly;\n\t\
                 perhaps it is corrupted?  *Possible* reason follows.\n\
                 {p}: {stale}\n"
            ))
            .and_then(|()| self.show_file_names())
            .and_then(|()| self.advise())
        } else {
            Ok(())
        };
        match said {
            Ok(()) => self.give_up(2, output),
            Err(stop) => stop,
        }
    }

    /// Sets the names bzip2's messages show: `(stdin)` and `(stdout)` without a file.
    fn name(&mut self, name: Option<&str>, out_name: impl FnOnce(&str) -> String) {
        if let Some(name) = name.filter(|_| self.options.mode != Mode::Stdin) {
            name.clone_into(&mut self.in_name);
            self.out_name = if self.options.mode == Mode::Files {
                out_name(name)
            } else {
                "(stdout)".to_owned()
            };
        } else {
            "(stdin)".clone_into(&mut self.in_name);
            "(stdout)".clone_into(&mut self.out_name);
        }
    }

    /// Opens the output file beside the input; the empty name is no file.
    fn create_output(&self) -> io::Result<Replacement> {
        if self.out_name.is_empty() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Replacement::create(self.path(&self.out_name))
    }

    /// The input and the output file of a named file; `None` when it was refused, said.
    fn open_pair(&mut self) -> Result<Option<(fs::File, Option<Replacement>)>, Stop> {
        let p = self.invoked;
        let (in_name, out_name) = (self.in_name.clone(), self.out_name.clone());
        let opened = fs::File::open(self.path(&in_name));
        let output = if self.options.mode == Mode::Files {
            match self.create_output() {
                Ok(output) => Some(output),
                Err(e) => {
                    self.refuse(&format!(
                        "{p}: Can't create output file {out_name}: {}.\n",
                        strerror(&e)
                    ))?;
                    return Ok(None);
                }
            }
        } else {
            None
        };
        match opened {
            Ok(file) => Ok(Some((file, output))),
            Err(e) => {
                if let Some(output) = output {
                    output.abandon();
                }
                let space =
                    if self.options.mode == Mode::ToStdout && self.options.op == Op::Decompress {
                        ""
                    } else {
                        " "
                    };
                self.refuse(&format!(
                    "{p}: Can't open input file {in_name}:{space}{}.\n",
                    strerror(&e)
                ))?;
                Ok(None)
            }
        }
    }

    /// bzip2's `compress`.
    fn compress(&mut self, name: Option<&str>) -> Result<(), Stop> {
        self.name(name, |name| format!("{name}.bz2"));
        let (p, mode, noisy) = (self.invoked, self.options.mode, self.options.noisy);
        let in_name = self.in_name.clone();
        let mut metadata = None;
        if mode != Mode::Stdin {
            match self.metadata(&in_name) {
                Ok(meta) => metadata = Some(meta),
                Err(e) => {
                    return self.refuse(&format!(
                        "{p}: Can't open input file {in_name}: {}.\n",
                        strerror(&e)
                    ));
                }
            }
        }
        if let Some((suffix, _)) = SUFFIXES
            .iter()
            .find(|(suffix, _)| in_name.ends_with(suffix))
        {
            if noisy {
                self.say(&format!(
                    "{p}: Input file {in_name} already has {suffix} suffix.\n"
                ))?;
            }
            self.set_exit(1);
            return Ok(());
        }
        if metadata.as_ref().is_some_and(fs::Metadata::is_dir) {
            return self.refuse(&format!("{p}: Input file {in_name} is a directory.\n"));
        }
        if mode == Mode::Files && !self.options.force && !self.normal_file() {
            if noisy {
                self.say(&format!(
                    "{p}: Input file {in_name} is not a normal file.\n"
                ))?;
            }
            self.set_exit(1);
            return Ok(());
        }
        if mode == Mode::Files && !(self.clear_output()? && self.alone()?) {
            return Ok(());
        }
        let times = metadata.as_ref().map(times_of);
        let read_only = metadata
            .as_ref()
            .is_some_and(|m| m.permissions().readonly());

        let (mut input, file_output): (Box<dyn Read>, Option<Replacement>) = match mode {
            Mode::Stdin => {
                if is_terminal(self.context, OpenFiles::STDOUT_FD) {
                    return self.refuse_terminal("write compressed data to");
                }
                (Box::new(self.context.stdin()), None)
            }
            Mode::ToStdout if is_terminal(self.context, OpenFiles::STDOUT_FD) => {
                return self.refuse_terminal("write compressed data to");
            }
            Mode::ToStdout | Mode::Files => match self.open_pair()? {
                Some((file, output)) => (Box::new(file), output),
                None => return Ok(()),
            },
        };
        if let Err(stop) = self.announce() {
            if let Some(output) = file_output {
                output.abandon();
            }
            return Err(stop);
        }
        let mut output = match file_output {
            Some(file) => Output::File(file),
            None => Output::Stdout(Box::new(&mut self.stdout)),
        };
        let (bytes_in, bytes_out) =
            match compress_stream(&mut input, &mut output, self.options.block) {
                Ok(counts) => counts,
                Err(e) => {
                    let file = into_file(output);
                    return Err(self.io_failed(e, file));
                }
            };
        drop(input);
        if let Err(e) = output.finish(times, read_only) {
            return Err(self.io_failed(e, None));
        }
        if self.options.verbosity >= 1 {
            if bytes_in == 0 {
                self.say(" no data compressed.\n")?;
            } else {
                self.say(&stats(bytes_in, bytes_out))?;
            }
        }
        self.remove_input()
    }

    /// bzip2's `uncompress`.
    #[expect(
        clippy::too_many_lines,
        reason = "bzip2's uncompress and uncompressStream, one pass kept together"
    )]
    fn uncompress(&mut self, name: Option<&str>) -> Result<(), Stop> {
        let mut cant_guess = false;
        self.name(name, |name| {
            SUFFIXES
                .iter()
                .find_map(|(suffix, new)| {
                    name.strip_suffix(suffix).map(|base| format!("{base}{new}"))
                })
                .unwrap_or_else(|| {
                    cant_guess = true;
                    format!("{name}.out")
                })
        });
        let (p, mode, noisy) = (self.invoked, self.options.mode, self.options.noisy);
        let (in_name, out_name) = (self.in_name.clone(), self.out_name.clone());
        let mut metadata = None;
        if mode != Mode::Stdin {
            match self.metadata(&in_name) {
                Ok(meta) if meta.is_dir() => {
                    return self.refuse(&format!("{p}: Input file {in_name} is a directory.\n"));
                }
                Ok(meta) => metadata = Some(meta),
                Err(e) => {
                    return self.refuse(&format!(
                        "{p}: Can't open input file {in_name}: {}.\n",
                        strerror(&e)
                    ));
                }
            }
        }
        if mode == Mode::Files && !self.options.force && !self.normal_file() {
            if noisy {
                self.say(&format!(
                    "{p}: Input file {in_name} is not a normal file.\n"
                ))?;
            }
            self.set_exit(1);
            return Ok(());
        }
        if cant_guess && noisy {
            self.say(&format!(
                "{p}: Can't guess original name for {in_name} -- using {out_name}\n"
            ))?;
        }
        if mode == Mode::Files && !(self.clear_output()? && self.alone()?) {
            return Ok(());
        }
        let times = metadata.as_ref().map(times_of);
        let read_only = metadata
            .as_ref()
            .is_some_and(|m| m.permissions().readonly());

        let path = (mode != Mode::Stdin).then(|| self.path(&in_name));
        let (input, file_output): (Box<dyn Read>, Option<Replacement>) = if mode == Mode::Stdin {
            if is_terminal(self.context, OpenFiles::STDIN_FD) {
                return self.refuse_terminal("read compressed data from");
            }
            (Box::new(self.context.stdin()), None)
        } else {
            match self.open_pair()? {
                Some((file, output)) => (Box::new(file), output),
                None => return Ok(()),
            }
        };
        let mut input = BufReader::with_capacity(CHUNK, input);
        if let Err(stop) = self.announce() {
            if let Some(output) = file_output {
                output.abandon();
            }
            return Err(stop);
        }
        let mut output = match file_output {
            Some(file) => Output::File(file),
            None => Output::Stdout(Box::new(&mut self.stdout)),
        };
        let ending = match decompress_streams(&mut input, &mut output) {
            // `-f` copies what is not bzip2's as it is, all of it from the start.
            Ok(Ending::NoStream { .. }) if self.options.force => {
                match rewound(self.context, input, path.as_ref())
                    .and_then(|mut rest| io::copy(&mut rest, &mut output))
                {
                    Ok(_) => Ok(Ending::Whole),
                    Err(e) => Err(Broken::Read(e)),
                }
            }
            other => other,
        };
        let whole = match ending {
            Ok(Ending::Whole) => {
                if let Err(e) = output.finish(times, read_only) {
                    return Err(self.io_failed(e, None));
                }
                true
            }
            Ok(Ending::NoStream { stream: 1 }) => {
                output.abandon();
                false
            }
            // Trailing garbage: the output is closed without its mode, and standard
            // output is not flushed yet, as in bzip2.
            Ok(Ending::NoStream { .. }) => {
                if let Some(file) = into_file(output) {
                    if let Err(e) = file.finish(times.unwrap_or_default(), false, false) {
                        return Err(self.io_failed(e, None));
                    }
                }
                if noisy {
                    self.say(&format!(
                        "\n{p}: {in_name}: trailing garbage after EOF ignored\n"
                    ))?;
                }
                true
            }
            Err(Broken::Data) => {
                let file = into_file(output);
                return Err(self.data_failed(file));
            }
            Err(Broken::Truncated) => {
                let file = into_file(output);
                return Err(self.ended_early(file));
            }
            Err(Broken::Read(e) | Broken::Write(e)) => {
                let file = into_file(output);
                return Err(self.io_failed(e, file));
            }
        };
        if whole {
            self.remove_input()?;
            if self.options.verbosity >= 1 {
                self.say("done\n")?;
            }
        } else {
            self.failed = true;
            self.set_exit(2);
            if self.options.verbosity >= 1 {
                self.say("not a bzip2 file.\n")?;
            } else {
                self.say(&format!("{p}: {in_name} is not a bzip2 file.\n"))?;
            }
        }
        Ok(())
    }

    /// bzip2's `testf`.
    fn test(&mut self, name: Option<&str>) -> Result<(), Stop> {
        self.name(name, str::to_owned);
        "(none)".clone_into(&mut self.out_name);
        let (p, mode) = (self.invoked, self.options.mode);
        let in_name = self.in_name.clone();
        if mode != Mode::Stdin {
            match self.metadata(&in_name) {
                Ok(meta) if meta.is_dir() => {
                    return self.refuse(&format!("{p}: Input file {in_name} is a directory.\n"));
                }
                Ok(_) => {}
                Err(e) => {
                    return self.refuse(&format!(
                        "{p}: Can't open input {in_name}: {}.\n",
                        strerror(&e)
                    ));
                }
            }
        }
        let input: Box<dyn Read> = if mode == Mode::Stdin {
            if is_terminal(self.context, OpenFiles::STDIN_FD) {
                return self.refuse_terminal("read compressed data from");
            }
            Box::new(self.context.stdin())
        } else {
            match fs::File::open(self.path(&in_name)) {
                Ok(file) => Box::new(file),
                Err(e) => {
                    return self.refuse(&format!(
                        "{p}: Can't open input file {in_name}:{}.\n",
                        strerror(&e)
                    ));
                }
            }
        };
        self.announce()?;
        let ending = decompress_streams(
            &mut BufReader::with_capacity(CHUNK, input),
            &mut Output::Sink,
        );
        let prefix = if self.options.verbosity == 0 {
            format!("{p}: {in_name}: ")
        } else {
            String::new()
        };
        let whole = match ending {
            Ok(Ending::Whole) => true,
            Ok(Ending::NoStream { stream: 1 }) => {
                self.say(&format!(
                    "{prefix}bad magic number (file not created by bzip2)\n"
                ))?;
                false
            }
            Ok(Ending::NoStream { .. }) => {
                self.say(&prefix)?;
                if self.options.noisy {
                    self.say("trailing garbage after EOF ignored\n")?;
                }
                true
            }
            Err(Broken::Data) => {
                self.say(&format!("{prefix}data integrity (CRC) error in data\n"))?;
                false
            }
            Err(Broken::Truncated) => {
                self.say(&format!("{prefix}file ends unexpectedly\n"))?;
                false
            }
            Err(Broken::Read(e) | Broken::Write(e)) => {
                self.say(&prefix)?;
                return Err(self.io_failed(e, None));
            }
        };
        if whole && self.options.verbosity >= 1 {
            self.say("ok\n")?;
        }
        if !whole {
            self.failed = true;
        }
        Ok(())
    }

    fn treat(&mut self, name: Option<&str>) -> Result<(), Stop> {
        match self.options.op {
            Op::Compress => self.compress(name),
            Op::Decompress => self.uncompress(name),
            Op::Test => self.test(name),
        }
    }

    /// Every file in turn, then what the run as a whole says.
    fn run_all(&mut self) -> Result<(), Stop> {
        if self.options.mode == Mode::Stdin {
            self.treat(None)?;
        } else {
            for file in self.options.files.clone() {
                self.begun += 1;
                self.treat(Some(&file))?;
            }
        }
        if self.failed {
            if self.options.op == Op::Test && self.options.noisy {
                self.say(
                    "\nYou can use the `bzip2recover' program to attempt to recover\n\
                     data from undamaged sections of corrupted files.\n\n",
                )?;
            }
            self.set_exit(2);
        }
        Ok(())
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
    // The words of `BZIP2`, then of `BZIP`, come before the command line's.
    let mut words: Vec<String> = ["BZIP2", "BZIP"]
        .iter()
        .filter_map(|name| exported(context, name))
        .flat_map(|value| {
            value
                .split_ascii_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    words.extend(args.iter().cloned());
    let mut said = Vec::new();
    let parsed = parse(invoked, &words, &mut said);
    let mut stdout: io::BufWriter<Box<dyn Write>> =
        io::BufWriter::with_capacity(STDOUT_BUFFER, Box::new(context.stdout()));
    for text in said {
        match text {
            Said::Out(text) => stdout.write_all(text.as_bytes())?,
            Said::Err(text) => context.stderr().write_all(text.as_bytes())?,
        }
    }
    let options = match parsed {
        Parsed::Run(options) => options,
        Parsed::Exit(status) => {
            stdout.flush()?;
            return Ok(ExecutionResult::new(status));
        }
    };
    let mut run = Run {
        invoked,
        options,
        context,
        stdout,
        status: 0,
        in_name: String::new(),
        out_name: String::new(),
        begun: 0,
        failed: false,
    };
    let outcome = run.run_all();
    let flushed = run.stdout.flush();
    match outcome {
        Ok(()) | Err(Stop::Exit) => {
            flushed?;
            Ok(ExecutionResult::new(run.status))
        }
        Err(Stop::Shell(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(invoked: &str, words: &[&str]) -> (Parsed, Vec<Said>) {
        let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
        let mut said = Vec::new();
        (parse(invoked, &words, &mut said), said)
    }

    fn options(invoked: &str, words: &[&str]) -> Options {
        match parsed(invoked, words).0 {
            Parsed::Run(options) => options,
            Parsed::Exit(status) => panic!("exit {status}"),
        }
    }

    #[test]
    fn flags_are_read_anywhere_and_a_dash_is_no_file() {
        let o = options("bzip2", &["a", "-d", "b", "-", "-v4k"]);
        assert_eq!(
            (o.op, o.mode, o.keep, o.block),
            (Op::Decompress, Mode::Files, true, 4)
        );
        assert_eq!(o.files, ["a", "b"]);
        let o = options("bzip2", &["--", "-", "-d"]);
        assert_eq!((o.op, o.files.len()), (Op::Compress, 2));
        assert_eq!(options("bzip2", &["-c"]).mode, Mode::Stdin);
        assert_eq!(options("bzcat", &["x"]).mode, Mode::ToStdout);
        assert_eq!(options("bunzip2", &[]).op, Op::Decompress);
        assert_eq!(options("bzip2", &["-s", "-c"]).block, 2);
        assert_eq!(
            options("bzip2", &["--fast", "-9"]).block,
            1,
            "long flags come after"
        );
        assert_eq!(options("bzip2", &["longername"]).longest, 10);
    }

    #[test]
    fn a_bad_flag_is_quoted_whole_and_the_usage_follows() {
        let (outcome, said) = parsed("bzip2", &["--foo", "-xY"]);
        assert!(matches!(outcome, Parsed::Exit(1)));
        assert!(matches!(said.first(), Some(Said::Err(t)) if t == "bzip2: Bad flag `-xY'\n"));
        let (outcome, _) = parsed("bzcat", &["-t", "f"]);
        assert!(matches!(outcome, Parsed::Exit(1)));
        let (outcome, said) = parsed("bzip2", &["-V", "-h"]);
        assert!(matches!(outcome, Parsed::Exit(0)));
        assert!(matches!(said.first(), Some(Said::Out(t)) if t.starts_with(HEADER)));
    }

    #[test]
    fn the_numbers_are_bzip2s() {
        assert_eq!(
            stats(6, 42),
            " 0.143:1, 56.000 bits/byte, -600.00% saved, 6 in, 42 out.\n"
        );
    }
}
