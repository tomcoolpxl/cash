//! `grep`, `egrep` and `fgrep`: GNU grep 3.12's options, messages and exit statuses,
//! on ripgrep's engine.
//!
//! Checked against GNU grep 3.12 case by case (`crates/cash/tests/oracle`). The
//! patterns are GNU's basic, extended and fixed syntaxes, translated to the regex
//! crate's (`grep/pattern.rs`); the searching is `grep-searcher`'s line-oriented
//! searcher on `grep-regex`'s matcher, and on `fancy-regex` for a pattern with a
//! backreference, which the regex crate cannot take (`grep/engine.rs`); the printing is
//! GNU's, to the byte, colours included. `-P` is refused by name.
//!
//! Deliberate differences: a line ending in CRLF is matched without its `\r` (D20),
//! so `x$`, `-x` and `-w` work on a Windows text file, and `-o` never prints the `\r`;
//! the line is still printed as it was read, and `-U` keeps the `\r` in the line as
//! GNU does. A pattern file read with `-f` has its lines' `\r` taken off too. Files
//! found under a directory are searched in name order, where GNU takes the file
//! system's. A pattern with a backreference reports the first alternative's match
//! where GNU reports the longest; every other pattern gets GNU's longest match. A
//! bracket range may run between any two characters, where GNU's C.UTF-8 locale
//! refuses one past ASCII. `egrep` and `fgrep` do not print GNU's obsolescence warning.

mod crlf;
mod engine;
mod pattern;

use std::cell::RefCell;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long};
use clap::Parser;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};

use grep_matcher::LineTerminator;

use self::crlf::{CrlfLines, CrlfStripper};
use self::engine::{Engine, Spec};
use self::pattern::{Syntax, translate};

/// Search for patterns in files.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true, override_help = HELP)]
pub(crate) struct GrepCommand {
    /// Options, patterns and files, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

const USAGE: &str = "Usage: grep [OPTION]... PATTERNS [FILE]...";
const HINT: &str = "Try 'grep --help' for more information.";

/// GNU grep 3.12's `--help`, without the `-P` it describes and the addresses it ends on.
const HELP: &str = "Usage: grep [OPTION]... PATTERNS [FILE]...
Search for PATTERNS in each FILE.
Example: grep -i 'hello world' menu.h main.c
PATTERNS can contain multiple patterns separated by newlines.

Pattern selection and interpretation:
  -E, --extended-regexp     PATTERNS are extended regular expressions
  -F, --fixed-strings       PATTERNS are strings
  -G, --basic-regexp        PATTERNS are basic regular expressions
  -e, --regexp=PATTERNS     use PATTERNS for matching
  -f, --file=FILE           take PATTERNS from FILE
  -i, --ignore-case         ignore case distinctions in patterns and data
      --no-ignore-case      do not ignore case distinctions (default)
  -w, --word-regexp         match only whole words
  -x, --line-regexp         match only whole lines
  -z, --null-data           a data line ends in 0 byte, not newline

Miscellaneous:
  -s, --no-messages         suppress error messages
  -v, --invert-match        select non-matching lines
  -V, --version             display version information and exit
      --help                display this help text and exit

Output control:
  -m, --max-count=NUM       stop after NUM selected lines
  -b, --byte-offset         print the byte offset with output lines
  -n, --line-number         print line number with output lines
      --line-buffered       flush output on every line
  -H, --with-filename       print file name with output lines
  -h, --no-filename         suppress the file name prefix on output
      --label=LABEL         use LABEL as the standard input file name prefix
  -o, --only-matching       show only nonempty parts of lines that match
  -q, --quiet, --silent     suppress all normal output
      --binary-files=TYPE   assume that binary files are TYPE;
                            TYPE is 'binary', 'text', or 'without-match'
  -a, --text                equivalent to --binary-files=text
  -I                        equivalent to --binary-files=without-match
  -d, --directories=ACTION  how to handle directories;
                            ACTION is 'read', 'recurse', or 'skip'
  -D, --devices=ACTION      how to handle devices, FIFOs and sockets;
                            ACTION is 'read' or 'skip'
  -r, --recursive           like --directories=recurse
  -R, --dereference-recursive  likewise, but follow all symlinks
      --include=GLOB        search only files that match GLOB (a file pattern)
      --exclude=GLOB        skip files that match GLOB
      --exclude-from=FILE   skip files that match any file pattern from FILE
      --exclude-dir=GLOB    skip directories that match GLOB
  -L, --files-without-match  print only names of FILEs with no selected lines
  -l, --files-with-matches  print only names of FILEs with selected lines
  -c, --count               print only a count of selected lines per FILE
  -T, --initial-tab         make tabs line up (if needed)
  -Z, --null                print 0 byte after FILE name

Context control:
  -B, --before-context=NUM  print NUM lines of leading context
  -A, --after-context=NUM   print NUM lines of trailing context
  -C, --context=NUM         print NUM lines of output context
  -NUM                      same as --context=NUM
      --group-separator=SEP  print SEP on line between matches with context
      --no-group-separator  do not print separator for matches with context
      --color[=WHEN],
      --colour[=WHEN]       use markers to highlight the matching strings;
                            WHEN is 'always', 'never', or 'auto'
  -U, --binary              do not strip CR characters at EOL (MSDOS/Windows)

When FILE is '-', read standard input.  If no FILE is given, read standard
input, but with -r, recursively search the working directory instead.  With
fewer than two FILEs, assume -h.  Exit status is 0 if any line is selected,
1 otherwise; if any error occurs and -q is not given, the exit status is 2.
";

const VERSION: &str = "grep (cash): GNU grep 3.12's options, on ripgrep's engine";

/// The width `-T` pads a line number or byte offset to, as GNU grep 3.12 does.
const OFFSET_WIDTH: usize = 19;

// ---------------------------------------------------------------------------------
// Options

/// GNU grep's long options, in its order (which the ambiguity message lists).
const LONG_OPTIONS: &[Long<'static, &str>] = &[
    Long::new("basic-regexp", Arg::No, "G"),
    Long::new("extended-regexp", Arg::No, "E"),
    Long::new("fixed-regexp", Arg::No, "F"),
    Long::new("fixed-strings", Arg::No, "F"),
    Long::new("perl-regexp", Arg::No, "P"),
    Long::new("after-context", Arg::Required, "A"),
    Long::new("before-context", Arg::Required, "B"),
    Long::new("binary-files", Arg::Required, "binary-files"),
    Long::new("byte-offset", Arg::No, "b"),
    Long::new("context", Arg::Required, "C"),
    Long::new("color", Arg::Optional, "color"),
    Long::new("colour", Arg::Optional, "color"),
    Long::new("count", Arg::No, "c"),
    Long::new("devices", Arg::Required, "D"),
    Long::new("directories", Arg::Required, "d"),
    Long::new("dereference-recursive", Arg::No, "R"),
    Long::new("exclude", Arg::Required, "exclude"),
    Long::new("exclude-from", Arg::Required, "exclude-from"),
    Long::new("exclude-dir", Arg::Required, "exclude-dir"),
    Long::new("file", Arg::Required, "f"),
    Long::new("files-with-matches", Arg::No, "l"),
    Long::new("files-without-match", Arg::No, "L"),
    Long::new("group-separator", Arg::Required, "group-separator"),
    Long::new("help", Arg::No, "help"),
    Long::new("include", Arg::Required, "include"),
    Long::new("ignore-case", Arg::No, "i"),
    Long::new("no-ignore-case", Arg::No, "no-ignore-case"),
    Long::new("initial-tab", Arg::No, "T"),
    Long::new("label", Arg::Required, "label"),
    Long::new("line-buffered", Arg::No, "line-buffered"),
    Long::new("line-number", Arg::No, "n"),
    Long::new("line-regexp", Arg::No, "x"),
    Long::new("max-count", Arg::Required, "m"),
    Long::new("no-filename", Arg::No, "h"),
    Long::new("no-group-separator", Arg::No, "no-group-separator"),
    Long::new("no-messages", Arg::No, "s"),
    Long::new("null", Arg::No, "Z"),
    Long::new("null-data", Arg::No, "z"),
    Long::new("only-matching", Arg::No, "o"),
    Long::new("quiet", Arg::No, "q"),
    Long::new("recursive", Arg::No, "r"),
    Long::new("regexp", Arg::Required, "e"),
    Long::new("invert-match", Arg::No, "v"),
    Long::new("silent", Arg::No, "q"),
    Long::new("text", Arg::No, "a"),
    Long::new("binary", Arg::No, "U"),
    Long::new("version", Arg::No, "V"),
    Long::new("with-filename", Arg::No, "H"),
    Long::new("word-regexp", Arg::No, "w"),
];

/// GNU grep's short options; the digits of `-NUM` are read apart.
const SHORT_OPTIONS: &str = "A:B:C:D:EFGHILPRTUVX:abcd:e:f:hilm:noqrsuvwxyZz";

/// The digits of `-NUM`, short options of their own.
const DIGITS: &str = "0123456789";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BinaryFiles {
    Binary,
    Text,
    WithoutMatch,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Directories {
    Read,
    Skip,
    Recurse,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ColorWhen {
    Never,
    Always,
    Auto,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ListFiles {
    No,
    Matching,
    NonMatching,
}

/// Where a pattern comes from, in command-line order.
enum PatternSource {
    Text(String),
    File(String),
}

/// An `--include`, `--exclude` or `--exclude-from`, in command-line order.
enum Filter {
    Include(String),
    Exclude(String),
    ExcludeFrom(String),
}

/// Everything the command line said.
struct Options {
    syntax: Option<Syntax>,
    sources: Vec<PatternSource>,
    ignore_case: bool,
    invert: bool,
    word: bool,
    line: bool,
    count: bool,
    color: ColorWhen,
    list: ListFiles,
    /// `-m`: `None` is no limit.
    max_count: Option<u64>,
    only_matching: bool,
    quiet: bool,
    no_messages: bool,
    byte_offset: bool,
    with_filename: Option<bool>,
    label: Option<String>,
    line_number: bool,
    initial_tab: bool,
    null: bool,
    after: Option<usize>,
    before: Option<usize>,
    default_context: Option<usize>,
    group_separator: Option<String>,
    binary_files: BinaryFiles,
    directories: Directories,
    follow: bool,
    filters: Vec<Filter>,
    exclude_dirs: Vec<String>,
    line_buffered: bool,
    keep_cr: bool,
    null_data: bool,
    files: Vec<String>,
}

/// What parsing the command line ends in, other than options to act on.
enum Early {
    Help,
    Version,
    /// A message for standard error, with the usage lines after it, status 2.
    Usage(String),
    /// A message for standard error, status 2.
    Error(String),
    /// A message for standard error, the usage lines, and the status GNU uses.
    Die(String, u8),
}

impl Options {
    fn new() -> Self {
        Self {
            syntax: None,
            sources: Vec::new(),
            ignore_case: false,
            invert: false,
            word: false,
            line: false,
            count: false,
            color: ColorWhen::Never,
            list: ListFiles::No,
            max_count: None,
            only_matching: false,
            quiet: false,
            no_messages: false,
            byte_offset: false,
            with_filename: None,
            label: None,
            line_number: false,
            initial_tab: false,
            null: false,
            after: None,
            before: None,
            default_context: None,
            group_separator: Some("--".to_owned()),
            binary_files: BinaryFiles::Binary,
            directories: Directories::Read,
            follow: false,
            filters: Vec::new(),
            exclude_dirs: Vec::new(),
            line_buffered: false,
            keep_cr: false,
            null_data: false,
            files: Vec::new(),
        }
    }

    fn set_syntax(&mut self, syntax: Syntax) -> Result<(), Early> {
        if self.syntax.is_some_and(|s| s != syntax) {
            return Err(Early::Error("conflicting matchers specified".to_owned()));
        }
        self.syntax = Some(syntax);
        Ok(())
    }

    /// `-X MATCHER`, GNU's undocumented way of choosing the syntax.
    fn set_matcher(&mut self, name: &str) -> Result<(), Early> {
        match name {
            "grep" => self.set_syntax(Syntax::Basic),
            "egrep" => self.set_syntax(Syntax::Extended),
            "fgrep" => self.set_syntax(Syntax::Fixed),
            "perl" => Err(Early::Error("-P is not supported; use -E".to_owned())),
            other => Err(Early::Error(std::format!("invalid matcher {other}"))),
        }
    }

    /// Applies one option; `value` is its argument when it takes one.
    fn apply(&mut self, id: &str, value: Option<String>) -> Result<(), Early> {
        let value = value.unwrap_or_default();
        match id {
            "help" => return Err(Early::Help),
            "V" => return Err(Early::Version),
            "E" => self.set_syntax(Syntax::Extended)?,
            "F" => self.set_syntax(Syntax::Fixed)?,
            "G" => self.set_syntax(Syntax::Basic)?,
            "P" => return Err(Early::Error("-P is not supported; use -E".to_owned())),
            "X" => self.set_matcher(&value)?,
            "e" => self.sources.push(PatternSource::Text(value)),
            "f" => self.sources.push(PatternSource::File(value)),
            "i" | "y" => self.ignore_case = true,
            "no-ignore-case" => self.ignore_case = false,
            "v" => self.invert = true,
            "w" => self.word = true,
            "x" => self.line = true,
            "z" => self.null_data = true,
            "s" => self.no_messages = true,
            "m" => {
                self.max_count = match count_argument(&value) {
                    Count::Value(n) => Some(n),
                    Count::Negative => None,
                    Count::Bad => return Err(Early::Error("invalid max count".to_owned())),
                };
            }
            "b" => self.byte_offset = true,
            "n" => self.line_number = true,
            "line-buffered" => self.line_buffered = true,
            "H" => self.with_filename = Some(true),
            "h" => self.with_filename = Some(false),
            "label" => self.label = Some(value),
            "o" => self.only_matching = true,
            "q" => self.quiet = true,
            "binary-files" => {
                self.binary_files = match value.as_str() {
                    "binary" => BinaryFiles::Binary,
                    "text" => BinaryFiles::Text,
                    "without-match" => BinaryFiles::WithoutMatch,
                    _ => return Err(Early::Error("unknown binary-files type".to_owned())),
                };
            }
            "a" => self.binary_files = BinaryFiles::Text,
            "I" => self.binary_files = BinaryFiles::WithoutMatch,
            "d" => self.directories = directories_argument(&value)?,
            "D" => {
                if !matches!(value.as_str(), "read" | "skip") {
                    return Err(Early::Error("unknown devices method".to_owned()));
                }
            }
            "r" => self.directories = Directories::Recurse,
            "R" => {
                self.directories = Directories::Recurse;
                self.follow = true;
            }
            "include" => self.filters.push(Filter::Include(value)),
            "exclude" => self.filters.push(Filter::Exclude(value)),
            "exclude-from" => self.filters.push(Filter::ExcludeFrom(value)),
            "exclude-dir" => self.exclude_dirs.push(value),
            "L" => self.list = ListFiles::NonMatching,
            "l" => self.list = ListFiles::Matching,
            "c" => self.count = true,
            "T" => self.initial_tab = true,
            "Z" => self.null = true,
            "A" => self.after = Some(context_argument(&value)?),
            "B" => self.before = Some(context_argument(&value)?),
            "C" => self.default_context = Some(context_argument(&value)?),
            "group-separator" => self.group_separator = Some(value),
            "no-group-separator" => self.group_separator = None,
            "color" => {
                self.color = match value.as_str() {
                    "" | "auto" | "tty" | "if-tty" => ColorWhen::Auto,
                    "always" | "yes" | "force" => ColorWhen::Always,
                    "never" | "no" | "none" => ColorWhen::Never,
                    // GNU: an unknown WHEN shows the help, and ends with 0.
                    _ => return Err(Early::Help),
                };
            }
            "U" => self.keep_cr = true,
            // Removed from GNU grep 3.12: the usage lines, and status 2.
            "u" => return Err(Early::Usage(String::new())),
            _ => return Err(Early::Usage(std::format!("invalid option -- '{id}'"))),
        }
        Ok(())
    }

    /// Parses the command line as `getopt_long` does (`cash-getopt`): options and
    /// operands in any order, clusters, `--name=value`, unique prefixes, `--` ending the
    /// options. Digits in one word make `-NUM`, the context, as GNU grep reads them: a
    /// new word starts a new number.
    fn parse(args: &[String]) -> Result<Self, Early> {
        let mut options = Self::new();
        let mut shorts = cash_getopt::optstring_ids(SHORT_OPTIONS);
        shorts.extend(cash_getopt::optstring_ids(DIGITS));
        let mut number: Option<(usize, usize)> = None;
        for next in Getopt::new(&shorts, LONG_OPTIONS).read(args) {
            match next.map_err(|problem| Early::Usage(problem.to_string()))? {
                Item::Option {
                    id, value, word, ..
                } => {
                    let digit = id.parse::<usize>().ok().filter(|_| DIGITS.contains(id));
                    if let Some(digit) = digit {
                        let context = match number {
                            Some((same, so_far)) if same == word => {
                                so_far.saturating_mul(10).saturating_add(digit)
                            }
                            _ => digit,
                        };
                        number = Some((word, context));
                        options.default_context = Some(context);
                    } else {
                        options.apply(id, value)?;
                    }
                }
                Item::Operand { value, .. } => options.files.push(value),
            }
        }
        Ok(options)
    }
}

/// A number as `strtoimax` reads one for `-A`, `-B`, `-C` and `-m`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Count {
    /// Not a number.
    Bad,
    /// A negative number.
    Negative,
    /// A number; one too large is as large as can be.
    Value(u64),
}

/// A number as `strtoimax` reads one: leading blanks, an optional sign, digits,
/// nothing after.
fn count_argument(text: &str) -> Count {
    let text = text.trim_start();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Count::Bad;
    }
    if negative {
        return Count::Negative;
    }
    Count::Value(digits.parse::<u64>().unwrap_or(u64::MAX))
}

/// `-A`, `-B`, `-C`: a count of lines, which cannot be negative.
fn context_argument(text: &str) -> Result<usize, Early> {
    match count_argument(text) {
        Count::Value(n) => Ok(usize::try_from(n).unwrap_or(usize::MAX)),
        Count::Bad | Count::Negative => Err(Early::Error(std::format!(
            "{text}: invalid context length argument"
        ))),
    }
}

/// `-d ACTION`, matched as `argmatch` does: an unambiguous prefix will do.
fn directories_argument(value: &str) -> Result<Directories, Early> {
    const ACTIONS: [(&str, Directories); 3] = [
        ("read", Directories::Read),
        ("recurse", Directories::Recurse),
        ("skip", Directories::Skip),
    ];
    if let Some((_, action)) = ACTIONS.iter().find(|(name, _)| *name == value) {
        return Ok(*action);
    }
    let candidates: Vec<Directories> = ACTIONS
        .iter()
        .filter(|(name, _)| !value.is_empty() && name.starts_with(value))
        .map(|(_, action)| *action)
        .collect();
    if let [one] = candidates.as_slice() {
        return Ok(*one);
    }
    let kind = if candidates.is_empty() {
        "invalid"
    } else {
        "ambiguous"
    };
    Err(Early::Die(
        std::format!(
            "{kind} argument \u{2018}{value}\u{2019} for \u{2018}--directories\u{2019}\n\
             Valid arguments are:\n  - \u{2018}read\u{2019}\n  - \u{2018}recurse\u{2019}\n  \
             - \u{2018}skip\u{2019}"
        ),
        1,
    ))
}

// ---------------------------------------------------------------------------------
// Colours

/// `GREP_COLORS`: the SGR parameters of each capability, and the two switches.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Colors {
    /// `ms`: a match in a selected line.
    ms: String,
    /// `mc`: a match in a context line.
    mc: String,
    /// `sl`: the rest of a selected line.
    sl: String,
    /// `cx`: the rest of a context line.
    cx: String,
    /// `fn`: a file name.
    file: String,
    /// `ln`: a line number.
    ln: String,
    /// `bn`: a byte offset.
    bn: String,
    /// `se`: a separator.
    se: String,
    /// `rv`: with `-v`, `sl` and `cx` swap.
    rv: bool,
    /// `ne`: no erase-to-end-of-line after each sequence.
    ne: bool,
}

impl Colors {
    /// GNU's defaults, with `GREP_COLOR` (deprecated, taken as `mt=`) and then
    /// `GREP_COLORS` applied as GNU applies them: a malformed entry ends the reading,
    /// keeping what came before it; an unknown capability is passed over.
    fn from_env(grep_color: Option<&str>, grep_colors: Option<&str>) -> Self {
        let mut colors = Self {
            ms: "01;31".to_owned(),
            mc: "01;31".to_owned(),
            sl: String::new(),
            cx: String::new(),
            file: "35".to_owned(),
            ln: "32".to_owned(),
            bn: "32".to_owned(),
            se: "36".to_owned(),
            rv: false,
            ne: false,
        };
        if let Some(value) = grep_color
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit() || b == b';'))
        {
            value.clone_into(&mut colors.ms);
            value.clone_into(&mut colors.mc);
        }
        let Some(spec) = grep_colors else {
            return colors;
        };
        for entry in spec.split(':') {
            let (name, value) = entry
                .split_once('=')
                .map_or((entry, None), |(n, v)| (n, Some(v)));
            if let Some(value) = value
                && !value.bytes().all(|b| b.is_ascii_digit() || b == b';')
            {
                break;
            }
            let value = value.unwrap_or_default().to_owned();
            match name {
                "mt" => {
                    colors.ms.clone_from(&value);
                    colors.mc = value;
                }
                "ms" => colors.ms = value,
                "mc" => colors.mc = value,
                "sl" => colors.sl = value,
                "cx" => colors.cx = value,
                "fn" => colors.file = value,
                "ln" => colors.ln = value,
                "bn" => colors.bn = value,
                "se" => colors.se = value,
                "rv" => colors.rv = true,
                "ne" => colors.ne = true,
                _ => {}
            }
        }
        colors
    }

    /// The sequence that starts `cap`, or nothing when the capability is empty.
    fn start(&self, cap: &str) -> String {
        if cap.is_empty() {
            return String::new();
        }
        let erase = if self.ne { "" } else { "\x1b[K" };
        std::format!("\x1b[{cap}m{erase}")
    }

    /// The sequence that ends a capability, or nothing when it is empty.
    const fn end(&self, cap: &str) -> &'static str {
        if cap.is_empty() {
            ""
        } else if self.ne {
            "\x1b[m"
        } else {
            "\x1b[m\x1b[K"
        }
    }
}

// ---------------------------------------------------------------------------------
// Printing: the Sink

/// What the run needs beyond the options.
struct Run<'a> {
    opts: &'a Options,
    engine: &'a Engine,
    colors: Option<Colors>,
    /// The line terminator in the data.
    eol: u8,
    /// Whether any context option was given, which turns the group separators on.
    context_given: bool,
    /// Trailing context lines, as the searcher is set to give them.
    after: usize,
}

/// Prints one file's results as GNU grep does: the sink the searcher reports to.
struct Printer<'a, W: Write, E: Write> {
    run: &'a Run<'a>,
    out: BufWriter<W>,
    err: E,
    /// Which lines of the file ended in CRLF; filled by the reader as it goes.
    crlf: Rc<RefCell<CrlfLines>>,
    /// Whether any line was printed in this run, which puts a separator before the
    /// next group.
    used: bool,
    /// `-q` found its match: nothing more is read.
    stop_all: bool,
    name: String,
    show_name: bool,
    count: u64,
    matched: bool,
    last_printed: Option<u64>,
    /// A NUL was seen: the file is binary.
    binary: bool,
    /// A line was selected while the file was known to be binary.
    binary_matched: bool,
    /// A line to print was not UTF-8.
    encoding_error: bool,
    /// After `-m` was reached: trailing context lines still to print.
    after_limit: Option<usize>,
    /// The width `-T` pads a line number or byte offset to for this file.
    offset_width: usize,
}

impl<'a, W: Write, E: Write> Printer<'a, W, E> {
    fn new(run: &'a Run<'a>, out: W, err: E) -> Self {
        Self {
            run,
            out: BufWriter::with_capacity(64 * 1024, out),
            err,
            crlf: Rc::new(RefCell::new(CrlfLines::default())),
            used: false,
            stop_all: false,
            name: String::new(),
            show_name: false,
            count: 0,
            matched: false,
            last_printed: None,
            binary: false,
            binary_matched: false,
            encoding_error: false,
            after_limit: None,
            offset_width: OFFSET_WIDTH,
        }
    }

    /// Starts a file; `size` is its length when it is a regular file, which `-T` pads
    /// line numbers and offsets to the width of.
    fn begin_file(&mut self, name: String, show_name: bool, size: Option<u64>) {
        self.name = name;
        self.show_name = show_name;
        self.offset_width = size.map_or(OFFSET_WIDTH, |n| n.max(1).to_string().len());
        self.count = 0;
        self.matched = false;
        self.last_printed = None;
        self.binary = false;
        self.binary_matched = false;
        self.encoding_error = false;
        self.after_limit = None;
        *self.crlf.borrow_mut() = CrlfLines::default();
    }

    /// Whether normal output is off: `-q`, `-l`, `-L` and `-c` print no lines.
    fn out_quiet(&self) -> bool {
        let o = self.run.opts;
        o.quiet || o.list != ListFiles::No || o.count
    }

    fn start(&self, cap: fn(&Colors) -> &String) -> String {
        self.run
            .colors
            .as_ref()
            .map_or_else(String::new, |c| c.start(cap(c)))
    }

    fn end(&self, cap: fn(&Colors) -> &String) -> &'static str {
        self.run.colors.as_ref().map_or("", |c| c.end(cap(c)))
    }

    fn write_sep(&mut self, sep: u8) -> std::io::Result<()> {
        let start = self.start(|c| &c.se);
        let end = self.end(|c| &c.se);
        write!(self.out, "{start}{}{end}", char::from(sep))
    }

    fn write_name(&mut self) -> std::io::Result<()> {
        let start = self.start(|c| &c.file);
        let end = self.end(|c| &c.file);
        write!(self.out, "{start}{}{end}", self.name)
    }

    /// Writes a line number or byte offset, padded under `-T`.
    fn write_offset(&mut self, value: u64, cap: fn(&Colors) -> &String) -> std::io::Result<()> {
        let start = self.start(cap);
        let end = self.end(cap);
        if self.run.opts.initial_tab {
            let width = self.offset_width;
            write!(self.out, "{start}{value:>width$}{end}")
        } else {
            write!(self.out, "{start}{value}{end}")
        }
    }

    /// The prefix of an output line: name, line number, byte offset, each with `sep`
    /// after it; a tab under `-T` when there is a prefix and the line has content.
    fn write_head(
        &mut self,
        line: u64,
        offset: u64,
        sep: u8,
        nonempty: bool,
    ) -> std::io::Result<()> {
        let o = self.run.opts;
        if self.show_name {
            self.write_name()?;
            if o.null {
                self.out.write_all(b"\0")?;
            } else {
                self.write_sep(sep)?;
            }
        }
        if o.line_number {
            self.write_offset(line, |c| &c.ln)?;
            self.write_sep(sep)?;
        }
        if o.byte_offset {
            self.write_offset(offset, |c| &c.bn)?;
            self.write_sep(sep)?;
        }
        if o.initial_tab && (self.show_name || o.line_number || o.byte_offset) && nonempty {
            self.out.write_all(b"\t")?;
        }
        Ok(())
    }

    /// The group separator, before a line that does not follow the last one printed.
    fn separate(&mut self, line: u64) -> std::io::Result<()> {
        if !self.run.context_given
            || !self.used
            || self.last_printed == Some(line.saturating_sub(1))
        {
            return Ok(());
        }
        if let Some(sep) = &self.run.opts.group_separator {
            let start = self.start(|c| &c.se);
            let end = self.end(|c| &c.se);
            writeln!(self.out, "{start}{sep}{end}")?;
        }
        Ok(())
    }

    /// Whether `bytes` may be printed as text; a line that is not UTF-8 makes the
    /// file binary unless `-a`.
    fn printable(&mut self, bytes: &[u8]) -> bool {
        if self.run.opts.binary_files == BinaryFiles::Text || std::str::from_utf8(bytes).is_ok() {
            return true;
        }
        self.encoding_error = true;
        false
    }

    /// The line's terminator as it was read: a `\r` if the line had one, then the eol.
    fn write_eol(&mut self, line: u64) -> std::io::Result<()> {
        if self.crlf.borrow().has_cr(line) {
            self.out.write_all(b"\r")?;
        }
        self.out.write_all(&[self.run.eol])
    }

    /// The colours of a line: the rest of it, and a match in it.
    fn line_colors(&self, sep: u8) -> (String, String) {
        let Some(c) = &self.run.colors else {
            return (String::new(), String::new());
        };
        let selected = (sep == b':') != (self.run.opts.invert && c.rv);
        (
            if selected { c.sl.clone() } else { c.cx.clone() },
            if sep == b':' {
                c.ms.clone()
            } else {
                c.mc.clone()
            },
        )
    }

    /// Prints a selected (`sep` `:`) or context (`-`) line, as GNU's `print_line`.
    fn print_line(&mut self, bytes: &[u8], line: u64, offset: u64, sep: u8) -> std::io::Result<()> {
        let o = self.run.opts;
        if !o.only_matching && !self.printable(bytes) {
            return Ok(());
        }
        self.separate(line)?;
        let matching = (sep == b':') != o.invert;
        let (line_color, match_color) = self.line_colors(sep);
        if !o.only_matching {
            self.write_head(line, offset, sep, !bytes.is_empty())?;
        }
        let beg = if matching && (o.only_matching || !match_color.is_empty()) {
            let Some(rest) =
                self.print_middle(bytes, line, offset, sep, &line_color, &match_color)?
            else {
                return Ok(());
            };
            rest
        } else {
            0
        };
        if !o.only_matching {
            let tail = bytes.get(beg..).unwrap_or_default();
            if !tail.is_empty() && !line_color.is_empty() {
                let colors = self.run.colors.as_ref();
                let start = colors.map_or_else(String::new, |c| c.start(&line_color));
                let end = colors.map_or("", |c| c.end(&line_color));
                self.out.write_all(start.as_bytes())?;
                self.out.write_all(tail)?;
                self.out.write_all(end.as_bytes())?;
            } else {
                self.out.write_all(tail)?;
            }
            self.write_eol(line)?;
        }
        self.used = true;
        self.last_printed = Some(line);
        if o.line_buffered {
            self.out.flush()?;
        }
        Ok(())
    }

    /// Prints the matches in a line, with the text between them under `-o`'s rules or
    /// in the line's colour: GNU's `print_line_middle`. Returns where the unprinted
    /// rest begins, or nothing when a match could not be printed.
    fn print_middle(
        &mut self,
        bytes: &[u8],
        line: u64,
        offset: u64,
        sep: u8,
        line_color: &str,
        match_color: &str,
    ) -> std::io::Result<Option<usize>> {
        let o = self.run.opts;
        let colors = self.run.colors.as_ref();
        let line_start = colors.map_or_else(String::new, |c| c.start(line_color));
        let match_start = colors.map_or_else(String::new, |c| c.start(match_color));
        let match_end = colors.map_or("", |c| c.end(match_color));
        let mut cur = 0;
        let mut mid: Option<usize> = None;
        let mut pos = 0;
        while pos <= bytes.len() {
            let Some((start, end)) = self.run.engine.find_at(bytes, pos) else {
                break;
            };
            if start == bytes.len() {
                break;
            }
            if end == start {
                if mid.is_none() {
                    mid = Some(cur);
                }
                cur = start + 1;
                pos = cur;
                continue;
            }
            let found = bytes.get(start..end).unwrap_or_default();
            if o.only_matching {
                if !self.printable(found) {
                    return Ok(None);
                }
                self.separate(line)?;
                let at = offset + u64::try_from(start).unwrap_or(u64::MAX);
                self.write_head(line, at, sep, true)?;
            } else {
                self.out.write_all(line_start.as_bytes())?;
                let from = mid.take().unwrap_or(cur);
                self.out
                    .write_all(bytes.get(from..start).unwrap_or_default())?;
            }
            self.out.write_all(match_start.as_bytes())?;
            self.out.write_all(found)?;
            self.out.write_all(match_end.as_bytes())?;
            if o.only_matching {
                self.out.write_all(&[self.run.eol])?;
                self.used = true;
                self.last_printed = Some(line);
            }
            cur = end;
            pos = end;
        }
        Ok(Some(if o.only_matching {
            bytes.len()
        } else {
            mid.unwrap_or(cur)
        }))
    }

    /// A line after `-m` was reached: trailing context, up to the count asked for.
    fn trailing(&mut self, bytes: &[u8], line: u64, offset: u64) -> std::io::Result<bool> {
        let Some(left) = self.after_limit else {
            return Ok(true);
        };
        if left == 0 {
            return Ok(false);
        }
        if !self.binary {
            self.print_line(bytes, line, offset, b'-')?;
        }
        self.after_limit = Some(left - 1);
        Ok(left > 1)
    }

    /// The original byte offset of a line, with the `\r` bytes taken out before it put
    /// back in the count.
    fn file_offset(&self, line: u64, stripped_offset: u64) -> u64 {
        stripped_offset + self.crlf.borrow().removed_before(line)
    }

    /// Prints what the end of a file calls for: a count, a listed name, the binary
    /// file message. Returns whether the file is a success: a selected line, even
    /// under `-L`, as GNU grep 3.12 has it.
    fn finish_file(&mut self) -> std::io::Result<bool> {
        let o = self.run.opts;
        if o.binary_files == BinaryFiles::WithoutMatch && self.binary {
            self.count = 0;
            self.matched = false;
        }
        // `-q`, `-l` and `-L` take precedence over the count.
        if o.count && !o.quiet && o.list == ListFiles::No {
            if self.show_name {
                self.write_name()?;
                if o.null {
                    self.out.write_all(b"\0")?;
                } else {
                    self.write_sep(b':')?;
                }
            }
            writeln!(self.out, "{}", self.count)?;
        }
        let listed = match o.list {
            ListFiles::No => false,
            ListFiles::Matching => self.matched,
            ListFiles::NonMatching => !self.matched,
        };
        if listed {
            self.write_name()?;
            self.out.write_all(if o.null { b"\0" } else { b"\n" })?;
        }
        if (self.binary_matched || self.encoding_error)
            && o.binary_files == BinaryFiles::Binary
            && !self.out_quiet()
        {
            self.out.flush()?;
            writeln!(self.err, "grep: {}: binary file matches", self.name)?;
        }
        if o.line_buffered {
            self.out.flush()?;
        }
        Ok(self.matched)
    }
}

impl<W: Write, E: Write> Sink for Printer<'_, W, E> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> std::io::Result<bool> {
        let o = self.run.opts;
        let line = mat.line_number().unwrap_or(0);
        let bytes = strip_eol(mat.bytes(), self.run.eol);
        let offset = self.file_offset(line, mat.absolute_byte_offset());
        if self.after_limit.is_some() {
            return self.trailing(bytes, line, offset);
        }
        self.count += 1;
        self.matched = true;
        if o.quiet {
            self.stop_all = true;
            return Ok(false);
        }
        if o.list != ListFiles::No {
            return Ok(false);
        }
        let limit_reached = o.max_count.is_some_and(|m| self.count >= m);
        if self.binary && o.binary_files != BinaryFiles::Text {
            self.binary_matched = true;
            return Ok(o.count && !limit_reached);
        }
        if o.count {
            return Ok(!limit_reached);
        }
        self.print_line(bytes, line, offset, b':')?;
        if limit_reached {
            if self.run.after > 0 {
                self.after_limit = Some(self.run.after);
                return Ok(true);
            }
            return Ok(false);
        }
        Ok(true)
    }

    fn context(
        &mut self,
        _searcher: &Searcher,
        context: &SinkContext<'_>,
    ) -> std::io::Result<bool> {
        let line = context.line_number().unwrap_or(0);
        let bytes = strip_eol(context.bytes(), self.run.eol);
        let offset = self.file_offset(line, context.absolute_byte_offset());
        if self.after_limit.is_some() {
            return self.trailing(bytes, line, offset);
        }
        if self.binary && self.run.opts.binary_files != BinaryFiles::Text {
            return Ok(true);
        }
        self.print_line(bytes, line, offset, b'-')?;
        Ok(true)
    }

    fn binary_data(&mut self, _searcher: &Searcher, _offset: u64) -> std::io::Result<bool> {
        self.binary = true;
        Ok(self.run.opts.binary_files != BinaryFiles::WithoutMatch)
    }
}

/// A line without its terminator.
fn strip_eol(bytes: &[u8], eol: u8) -> &[u8] {
    bytes.strip_suffix(&[eol]).unwrap_or(bytes)
}

// ---------------------------------------------------------------------------------
// Files

/// A glob of `--include`, `--exclude` or `--exclude-dir`.
struct Glob {
    text: String,
    pattern: Option<glob::Pattern>,
}

impl Glob {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            pattern: glob::Pattern::new(text).ok(),
        }
    }

    /// Whether the glob matches `name` as `fnmatch` without flags does: `*` crosses
    /// `/` and a leading dot. A glob that does not parse matches itself.
    fn matches(&self, name: &str) -> bool {
        let options = glob::MatchOptions {
            case_sensitive: true,
            require_literal_separator: false,
            require_literal_leading_dot: false,
        };
        self.pattern
            .as_ref()
            .map_or(self.text == name, |p| p.matches_with(name, options))
    }

    /// Whether the glob matches a name given on the command line: the whole name, or
    /// what follows any `/` in it.
    fn matches_command_line(&self, name: &str) -> bool {
        self.matches(name)
            || name
                .match_indices('/')
                .any(|(i, _)| self.matches(name.get(i + 1..).unwrap_or_default()))
    }
}

/// The `--include` and `--exclude` globs, in command-line order; the last that matches
/// decides, and when none does, the first option's opposite.
struct Filters {
    list: Vec<(Glob, bool)>,
}

impl Filters {
    fn excludes(&self, name: &str, command_line: bool) -> bool {
        let matches = |g: &Glob| {
            if command_line {
                g.matches_command_line(name)
            } else {
                g.matches(name)
            }
        };
        if let Some((_, include)) = self.list.iter().rev().find(|(g, _)| matches(g)) {
            return !include;
        }
        self.list.first().is_some_and(|(_, include)| *include)
    }
}

/// Searches the inputs one by one: the names from the command line, and what is under
/// a directory with `-r`.
struct Walker<'a, W: Write, E: Write> {
    printer: Printer<'a, W, E>,
    searcher: Searcher,
    filters: Filters,
    exclude_dirs: Vec<Glob>,
    operands: usize,
    /// Standard input, until an operand reads it.
    stdin: Option<Box<dyn Read>>,
    trouble: bool,
    success: bool,
}

impl<W: Write, E: Write> Walker<'_, W, E> {
    const fn opts(&self) -> &Options {
        self.printer.run.opts
    }

    /// `grep: NAME: reason`, unless `-s`.
    fn complain(&mut self, name: &str, reason: &str) -> std::io::Result<()> {
        self.trouble = true;
        if self.opts().no_messages {
            return Ok(());
        }
        self.printer.out.flush()?;
        writeln!(self.printer.err, "grep: {name}: {reason}")
    }

    /// Searches one input, already open.
    fn search<R: Read>(
        &mut self,
        name: String,
        reader: R,
        show_name: bool,
        size: Option<u64>,
    ) -> std::io::Result<()> {
        self.printer.begin_file(name, show_name, size);
        let run = self.printer.run;
        let strip = !self.opts().keep_cr && !self.opts().null_data;
        if strip {
            let stripper = CrlfStripper::new(reader, Rc::clone(&self.printer.crlf));
            run.engine
                .search(&mut self.searcher, stripper, &mut self.printer)?;
        } else {
            run.engine
                .search(&mut self.searcher, reader, &mut self.printer)?;
        }
        let success = self.printer.finish_file()?;
        self.success |= success;
        Ok(())
    }

    /// A directory that will not be searched: GNU's message, and what `-c` and `-L`
    /// still print.
    fn refuse_directory(&mut self, name: &str, show_name: bool) -> std::io::Result<()> {
        self.complain(name, "Is a directory")?;
        self.printer.begin_file(name.to_owned(), show_name, None);
        let success = self.printer.finish_file()?;
        self.success |= success;
        Ok(())
    }

    /// Searches a file by its path; `shown` is how its name is printed.
    fn search_file(
        &mut self,
        shown: String,
        actual: &Path,
        show_name: bool,
    ) -> std::io::Result<()> {
        match std::fs::File::open(actual) {
            Ok(file) => {
                let size = file
                    .metadata()
                    .ok()
                    .filter(|m| m.is_file())
                    .map(|m| m.len());
                self.search(shown, file, show_name, size)
            }
            Err(error) => {
                let reason = cash_core::error::os_error_text(&error);
                self.complain(&shown, &reason)
            }
        }
    }

    /// Searches what is under a directory, in name order; `shown` is how the directory
    /// is printed, empty for the working directory searched with no operand.
    fn walk(&mut self, shown: &str, actual: &Path) -> std::io::Result<()> {
        let entries = match std::fs::read_dir(actual) {
            Ok(entries) => entries,
            Err(error) => {
                let reason = cash_core::error::os_error_text(&error);
                return self.complain(shown, &reason);
            }
        };
        let mut names: Vec<std::ffi::OsString> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        names.sort();
        for name in names {
            if self.printer.stop_all {
                return Ok(());
            }
            let name_text = name.to_string_lossy().into_owned();
            let child_shown = if shown.is_empty() {
                name_text.clone()
            } else if shown.ends_with('/') {
                std::format!("{shown}{name_text}")
            } else {
                std::format!("{shown}/{name_text}")
            };
            let child = actual.join(&name);
            let metadata = if self.opts().follow {
                std::fs::metadata(&child)
            } else {
                std::fs::symlink_metadata(&child)
            };
            let Ok(metadata) = metadata else {
                // A dangling link under -R, or something gone since the listing.
                if self.opts().follow {
                    self.complain(&child_shown, "No such file or directory")?;
                }
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if self.exclude_dirs.iter().any(|g| g.matches(&name_text)) {
                    continue;
                }
                self.walk(&child_shown, &child)?;
            } else {
                if self.filters.excludes(&name_text, false) {
                    continue;
                }
                let show_name = self.opts().with_filename.unwrap_or(true);
                self.search_file(child_shown, &child, show_name)?;
            }
        }
        Ok(())
    }

    /// A name from the command line; `actual` is where it is, by the shell's working
    /// directory.
    fn operand(&mut self, name: &str, actual: &Path) -> std::io::Result<()> {
        let show_name = self.opts().with_filename.unwrap_or(self.operands > 1);
        if name == "-" {
            let label = self
                .opts()
                .label
                .clone()
                .unwrap_or_else(|| "(standard input)".to_owned());
            let reader: Box<dyn Read> = self
                .stdin
                .take()
                .unwrap_or_else(|| Box::new(std::io::empty()));
            return self.search(label, reader, show_name, None);
        }
        let shown = cash_win32::path::render(Path::new(name));
        let metadata = match std::fs::metadata(actual) {
            Ok(metadata) => metadata,
            Err(error) => {
                let reason = cash_core::error::os_error_text(&error);
                return self.complain(&shown, &reason);
            }
        };
        if !metadata.is_dir() {
            if self.filters.excludes(&shown, true) {
                return Ok(());
            }
            return self.search_file(shown, actual, show_name);
        }
        if self.opts().directories == Directories::Skip
            || self
                .exclude_dirs
                .iter()
                .any(|g| g.matches_command_line(&shown))
        {
            return Ok(());
        }
        if self.opts().directories == Directories::Recurse {
            self.walk(&shown, actual)
        } else {
            self.refuse_directory(&shown, show_name)
        }
    }
}

// ---------------------------------------------------------------------------------
// The command

/// The patterns, read from their sources in order: `-e` texts and `-f` files, each
/// split at newlines; a pattern file's lines lose a trailing `\r` (D20).
fn collect_patterns<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    sources: &[PatternSource],
) -> Result<Vec<Vec<u8>>, String> {
    let read_error = |name: &str, e: &std::io::Error| {
        std::format!("{name}: {}", cash_core::error::os_error_text(e))
    };
    let mut patterns = Vec::new();
    for source in sources {
        match source {
            PatternSource::Text(text) => {
                patterns.extend(text.split('\n').map(|p| p.as_bytes().to_vec()));
            }
            PatternSource::File(name) => {
                let bytes = if name == "-" {
                    let mut bytes = Vec::new();
                    context
                        .stdin()
                        .read_to_end(&mut bytes)
                        .map_err(|e| read_error(name, &e))?;
                    bytes
                } else {
                    let path = context.shell.absolute_path(Path::new(name));
                    std::fs::read(&path).map_err(|e| read_error(name, &e))?
                };
                let mut lines = bytes.split(|&b| b == b'\n').peekable();
                while let Some(line) = lines.next() {
                    if lines.peek().is_none() && line.is_empty() {
                        break;
                    }
                    let line = line.strip_suffix(b"\r").unwrap_or(line);
                    patterns.push(line.to_vec());
                }
            }
        }
    }
    Ok(patterns)
}

/// The file name globs, with `--exclude-from` files read in their place.
fn collect_filters<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    filters: &[Filter],
) -> Result<Filters, String> {
    let mut list = Vec::new();
    for filter in filters {
        match filter {
            Filter::Include(g) => list.push((Glob::new(g), true)),
            Filter::Exclude(g) => list.push((Glob::new(g), false)),
            Filter::ExcludeFrom(name) => {
                let path = context.shell.absolute_path(Path::new(name));
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| std::format!("{name}: {}", cash_core::error::os_error_text(&e)))?;
                for line in text.lines() {
                    let line = line.strip_suffix('\r').unwrap_or(line);
                    if !line.is_empty() {
                        list.push((Glob::new(line), false));
                    }
                }
            }
        }
    }
    Ok(Filters { list })
}

/// The patterns as one alternation in the regex crate's syntax, with whether any has
/// a backreference; the warnings are GNU's, without their prefix. The error is GNU's
/// message.
fn join_patterns(
    patterns: &[Vec<u8>],
    syntax: Syntax,
) -> Result<(String, bool, Vec<String>), String> {
    let mut joined = String::new();
    let mut groups = 0;
    let mut fancy = false;
    let mut warnings = Vec::new();
    for (i, pattern) in patterns.iter().enumerate() {
        let translated = translate(pattern, syntax, groups)?;
        warnings.extend(translated.warnings);
        if i > 0 {
            joined.push('|');
        }
        joined.push_str("(?:");
        joined.push_str(&translated.regex);
        joined.push(')');
        groups += translated.groups;
        fancy |= translated.fancy;
    }
    if patterns.is_empty() {
        // No pattern at all (`-f` of an empty file) matches nothing.
        joined.push_str(r"\b\B");
    }
    Ok((joined, fancy, warnings))
}

/// The searcher, set up for the options.
fn searcher(opts: &Options, before: usize, after: usize, eol: u8) -> Searcher {
    let mut builder = SearcherBuilder::new();
    builder
        .line_number(true)
        .invert_match(opts.invert)
        .before_context(before)
        .after_context(after)
        .multi_line(false)
        .bom_sniffing(false)
        .line_terminator(LineTerminator::byte(eol));
    if opts.binary_files == BinaryFiles::Text || opts.null_data {
        builder.binary_detection(BinaryDetection::none());
    } else {
        builder.binary_detection(BinaryDetection::convert(0));
    }
    builder.build()
}

/// Reports how the command line was refused, with the status GNU gives.
fn report_early<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    early: Early,
) -> Result<ExecutionResult, cash_core::Error> {
    Ok(match early {
        Early::Help => {
            write!(context.stdout(), "{HELP}")?;
            ExecutionResult::success()
        }
        Early::Version => {
            writeln!(context.stdout(), "{VERSION}")?;
            ExecutionResult::success()
        }
        Early::Usage(message) => {
            writeln!(context.stderr(), "grep: {message}\n{USAGE}\n{HINT}")?;
            ExecutionResult::new(2)
        }
        Early::Error(message) => {
            writeln!(context.stderr(), "grep: {message}")?;
            ExecutionResult::new(2)
        }
        Early::Die(message, status) => {
            writeln!(context.stderr(), "grep: {message}\n{USAGE}\n{HINT}")?;
            ExecutionResult::new(status)
        }
    })
}

/// Parses the command line, with the syntax `egrep` or `fgrep` presets; the first
/// operand is the pattern when no `-e` or `-f` gave one.
fn settle(args: &[String], preset: Option<Syntax>) -> Result<Options, Early> {
    let mut opts = Options::parse(args)?;
    if let Some(syntax) = preset {
        // As if `-E` or `-F` came first: a later `-F` or `-E` conflicts.
        let later = opts.syntax;
        opts.syntax = Some(syntax);
        if let Some(later) = later {
            opts.set_syntax(later)?;
        }
    }
    if opts.sources.is_empty() {
        if opts.files.is_empty() {
            return Err(Early::Usage(String::new()));
        }
        let text = opts.files.remove(0);
        opts.sources.push(PatternSource::Text(text));
    }
    Ok(opts)
}

/// The colours, when `--color` asks for them: `GREP_COLOR` is taken with GNU's warning.
fn colors<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    when: ColorWhen,
) -> Result<Option<Colors>, cash_core::Error> {
    let on = match when {
        ColorWhen::Never => false,
        ColorWhen::Always => true,
        ColorWhen::Auto => context
            .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
            .is_some_and(|fd| fd.is_terminal()),
    };
    if !on {
        return Ok(None);
    }
    let grep_color = context.shell.env_str("GREP_COLOR").map(|v| v.into_owned());
    let grep_colors = context.shell.env_str("GREP_COLORS").map(|v| v.into_owned());
    let colors = Colors::from_env(grep_color.as_deref(), grep_colors.as_deref());
    // GNU warns when the deprecated variable still has an effect after `GREP_COLORS`.
    if let Some(value) = grep_color.as_deref().filter(|v| !v.is_empty())
        && (colors.ms == value || colors.mc == value)
    {
        writeln!(
            context.stderr(),
            "grep: warning: GREP_COLOR='{value}' is deprecated; use GREP_COLORS='mt={value}'"
        )?;
    }
    Ok(Some(colors))
}

impl builtins::Command for GrepCommand {
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
        let preset = match context.command_name.as_str() {
            "egrep" => Some(Syntax::Extended),
            "fgrep" => Some(Syntax::Fixed),
            _ => None,
        };
        let opts = match settle(&self.args, preset) {
            Ok(opts) => opts,
            Err(Early::Usage(message)) if message.is_empty() => {
                writeln!(context.stderr(), "{USAGE}\n{HINT}")?;
                return Ok(ExecutionResult::new(2));
            }
            Err(early) => return report_early(&context, early),
        };
        let patterns = match collect_patterns(&context, &opts.sources) {
            Ok(patterns) => patterns,
            Err(message) => return report_early(&context, Early::Error(message)),
        };
        // GNU: `-v ''` selects nothing, so nothing is read and the status is 1.
        if opts.invert
            && !opts.line
            && !opts.word
            && patterns.len() == 1
            && patterns.first().is_some_and(|p| p.is_empty())
        {
            return Ok(ExecutionResult::general_error());
        }
        let prepared = join_patterns(&patterns, opts.syntax.unwrap_or(Syntax::Basic))
            .and_then(|joined| collect_filters(&context, &opts.filters).map(|f| (joined, f)));
        let ((joined, fancy, warnings), filters) = match prepared {
            Ok(prepared) => prepared,
            Err(message) => return report_early(&context, Early::Error(message)),
        };
        for warning in &warnings {
            writeln!(context.stderr(), "grep: warning: {warning}")?;
        }
        let colors = colors(&context, opts.color)?;
        let engine = match Engine::build(&Spec {
            pattern: &joined,
            case_insensitive: opts.ignore_case,
            word: opts.word,
            whole_line: opts.line,
            fancy,
            null_data: opts.null_data,
            spans: opts.only_matching || colors.is_some(),
        }) {
            Ok(engine) => engine,
            Err(message) => return report_early(&context, Early::Error(message)),
        };
        if opts.max_count == Some(0) {
            return Ok(ExecutionResult::general_error());
        }

        search_inputs(&context, &opts, &engine, colors, filters)
    }
}

/// Searches the inputs the command line names, or standard input, or the working
/// directory under `-r`, and settles the exit status.
fn search_inputs<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    opts: &Options,
    engine: &Engine,
    colors: Option<Colors>,
    filters: Filters,
) -> Result<ExecutionResult, cash_core::Error> {
    let out_quiet = opts.quiet || opts.list != ListFiles::No || opts.count;
    let context_given =
        opts.after.is_some() || opts.before.is_some() || opts.default_context.is_some();
    let (before, after) = if out_quiet {
        (0, 0)
    } else {
        (
            opts.before.or(opts.default_context).unwrap_or(0),
            opts.after.or(opts.default_context).unwrap_or(0),
        )
    };
    let eol = if opts.null_data { 0 } else { b'\n' };
    let run = Run {
        opts,
        engine,
        colors,
        eol,
        context_given,
        after,
    };
    let mut walker = Walker {
        printer: Printer::new(&run, context.stdout(), context.stderr()),
        searcher: searcher(opts, before, after, eol),
        filters,
        exclude_dirs: opts.exclude_dirs.iter().map(|g| Glob::new(g)).collect(),
        operands: opts.files.len(),
        stdin: Some(Box::new(context.stdin())),
        trouble: false,
        success: false,
    };
    if opts.files.is_empty() {
        if opts.directories == Directories::Recurse {
            let here = context.shell.working_dir().to_path_buf();
            walker.walk("", &here)?;
        } else {
            walker.operand("-", &PathBuf::new())?;
        }
    } else {
        for name in &opts.files {
            if walker.printer.stop_all {
                break;
            }
            let actual = context.shell.absolute_path(Path::new(name));
            walker.operand(name, &actual)?;
        }
    }
    walker.printer.out.flush()?;

    if opts.quiet && walker.success {
        return Ok(ExecutionResult::success());
    }
    if walker.trouble {
        return Ok(ExecutionResult::new(2));
    }
    Ok(if walker.success {
        ExecutionResult::success()
    } else {
        ExecutionResult::general_error()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ColorWhen, Colors, Count, Directories, Early, Filters, Glob, ListFiles, Options,
        count_argument,
    };

    fn parse(args: &[&str]) -> Result<Options, Early> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        Options::parse(&args)
    }

    fn parsed(args: &[&str]) -> Options {
        parse(args).unwrap_or_else(|_| Options::new())
    }

    fn error_text(result: Result<Options, Early>) -> String {
        match result {
            Ok(_) => "ok".to_owned(),
            Err(Early::Help) => "help".to_owned(),
            Err(Early::Version) => "version".to_owned(),
            Err(Early::Usage(m)) => std::format!("usage: {m}"),
            Err(Early::Error(m)) => std::format!("error: {m}"),
            Err(Early::Die(m, s)) => std::format!("die {s}: {m}"),
        }
    }

    #[test]
    fn clusters_attached_values_and_long_prefixes() {
        let o = parsed(&["-rni", "-e", "foo", "-A1", "--max=2", "dir"]);
        assert_eq!(o.directories, Directories::Recurse);
        assert!(o.line_number && o.ignore_case);
        assert_eq!(o.after, Some(1));
        assert_eq!(o.max_count, Some(2));
        assert_eq!(o.files, vec!["dir"]);
        let o = parsed(&["-efoo", "-C", "3", "-12", "--colo=always"]);
        assert_eq!(o.default_context, Some(12));
        assert_eq!(o.sources.len(), 1);
        assert_eq!(o.color, ColorWhen::Always);
        let o = parsed(&["-1n2", "x"]);
        assert_eq!(o.default_context, Some(12));
        assert!(o.line_number);
        assert_eq!(parsed(&["--color", "x"]).color, ColorWhen::Auto);
    }

    #[test]
    fn gnu_errors_for_bad_options() {
        assert_eq!(error_text(parse(&["-k"])), "usage: invalid option -- 'k'");
        assert_eq!(
            error_text(parse(&["-e"])),
            "usage: option requires an argument -- 'e'"
        );
        assert_eq!(
            error_text(parse(&["--foo"])),
            "usage: unrecognized option '--foo'"
        );
        assert_eq!(
            error_text(parse(&["--co"])),
            "usage: option '--co' is ambiguous; possibilities: '--context' '--color' '--colour' '--count'"
        );
        assert_eq!(
            error_text(parse(&["--help=x"])),
            "usage: option '--help' doesn't allow an argument"
        );
        assert_eq!(
            error_text(parse(&["-E", "-F", "x"])),
            "error: conflicting matchers specified"
        );
        assert_eq!(
            error_text(parse(&["-P", "x"])),
            "error: -P is not supported; use -E"
        );
        assert_eq!(error_text(parse(&["-m", "x"])), "error: invalid max count");
        assert_eq!(
            error_text(parse(&["-A", "-1"])),
            "error: -1: invalid context length argument"
        );
        assert_eq!(
            error_text(parse(&["--binary-files=foo"])),
            "error: unknown binary-files type"
        );
        assert_eq!(error_text(parse(&["--color=foo"])), "help");
        assert!(
            error_text(parse(&["-d", "foo"]))
                .starts_with("die 1: invalid argument \u{2018}foo\u{2019}")
        );
        assert_eq!(
            parsed(&["-d", "rec", "x"]).directories,
            Directories::Recurse
        );
    }

    #[test]
    fn numbers_as_strtoimax_reads_them() {
        assert_eq!(count_argument("12"), Count::Value(12));
        assert_eq!(count_argument(" +3"), Count::Value(3));
        assert_eq!(count_argument("-1"), Count::Negative);
        assert_eq!(
            count_argument("99999999999999999999999"),
            Count::Value(u64::MAX)
        );
        assert_eq!(count_argument("1k"), Count::Bad);
        assert_eq!(count_argument(""), Count::Bad);
        assert_eq!(count_argument("0x1"), Count::Bad);
    }

    #[test]
    fn the_last_of_l_and_capital_l_wins_as_does_the_last_directories_option() {
        assert_eq!(parsed(&["-lL", "x"]).list, ListFiles::NonMatching);
        assert_eq!(parsed(&["-Ll", "x"]).list, ListFiles::Matching);
        assert_eq!(
            parsed(&["-r", "-d", "skip", "x"]).directories,
            Directories::Skip
        );
    }

    #[test]
    fn grep_colors_as_gnu_reads_them() {
        let c = Colors::from_env(None, None);
        assert_eq!(
            (c.ms.as_str(), c.file.as_str(), c.se.as_str()),
            ("01;31", "35", "36")
        );
        let c = Colors::from_env(None, Some("ms=1:bogus=2:ln=3"));
        assert_eq!((c.ms.as_str(), c.ln.as_str()), ("1", "3"));
        // A malformed entry ends the reading; what came before stays.
        let c = Colors::from_env(None, Some("sl=1:ms=1x:cx=2"));
        assert_eq!(
            (c.sl.as_str(), c.ms.as_str(), c.cx.as_str()),
            ("1", "01;31", "")
        );
        let c = Colors::from_env(Some("1;32"), Some("sl=4:rv=1:ne"));
        assert_eq!(
            (c.ms.as_str(), c.mc.as_str(), c.sl.as_str()),
            ("1;32", "1;32", "4")
        );
        assert!(c.rv && c.ne);
        assert_eq!(c.start("1"), "\x1b[1m");
        assert_eq!(c.end("1"), "\x1b[m");
        let c = Colors::from_env(Some("1;32"), Some("ms=4"));
        assert_eq!(c.ms, "4");
        assert_eq!(c.start("4"), "\x1b[4m\x1b[K");
        assert_eq!(c.start(""), "");
    }

    #[test]
    fn include_and_exclude_as_gnulib_decides_them() {
        let filters = |specs: &[(&str, bool)]| Filters {
            list: specs.iter().map(|(g, i)| (Glob::new(g), *i)).collect(),
        };
        let f = filters(&[("f", true), ("f", false)]);
        assert!(f.excludes("f", false));
        assert!(f.excludes("g", false));
        let f = filters(&[("f", false), ("f", true)]);
        assert!(!f.excludes("f", false));
        assert!(!f.excludes("g", false));
        let f = filters(&[("*", false), ("g", true)]);
        assert!(!f.excludes("g", false));
        assert!(f.excludes("h", false));
        let f = filters(&[("*.txt", true)]);
        assert!(!f.excludes("a.txt", false));
        assert!(f.excludes("a.c", false));
        assert!(f.excludes("A.TXT", false));
        // A command-line name matches whole or after any `/`.
        let f = filters(&[("*/a.txt", false)]);
        assert!(f.excludes("r/a.txt", true));
        assert!(!f.excludes("a.txt", true));
        let f = filters(&[("a.txt", false)]);
        assert!(f.excludes("r/a.txt", true));
        assert!(!f.excludes("r/a.txt", false));
        assert!(Glob::new("[!g]").matches("h"));
        assert!(!Glob::new("[!g]").matches("g"));
        assert!(!filters(&[]).excludes("x", true));
    }
}
