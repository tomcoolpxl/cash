//! `zip`: Info-ZIP's zip 3.0's options, messages and exit statuses.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use cash_archive::zip::read::{self, Archive, OpenError};
use cash_archive::zip::write::{Input, NewMember, SplitOutput, Stream, Writer};
use cash_archive::zip::{
    DOS_DIRECTORY, DOS_READ_ONLY, DosTime, Entry, S_IFDIR, S_IFIFO, S_IFLNK, S_IFREG, extra_id,
    field, method, percent,
};
use cash_core::ExecutionResult;
use cash_core::openfiles::OpenFiles;
use cash_win32::unix::{self, Replacement, UnixView};

use super::help::{ZIP_USAGE, ZIP_VERSION};
use super::{Clock, Say, Source as ArchiveSource, matches, name_text, read_line};
use crate::compress::{is_terminal, strerror};

mod fix;

/// zip's exit statuses.
mod code {
    pub(super) const FORMAT: u8 = 3;
    pub(super) const TEST: u8 = 8;
    pub(super) const NONE: u8 = 12;
    pub(super) const CREATE: u8 = 15;
    pub(super) const PARAM: u8 = 16;
    pub(super) const OPEN: u8 = 18;
}

/// What is done to the archive.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Mode {
    /// Add, or replace what is there.
    #[default]
    Add,
    /// `-u`: add, or replace with what is newer.
    Update,
    /// `-f`: replace with what is newer, add nothing.
    Freshen,
    /// `-d`.
    Delete,
    /// `-FS`: the archive made to hold just these files.
    FileSync,
}

#[derive(Debug, Default)]
struct Options {
    mode: Mode,
    level: u32,
    method: Option<u16>,
    recurse: bool,
    junk: bool,
    quiet: bool,
    verbose: u8,
    move_files: bool,
    no_dirs: bool,
    symlinks: bool,
    no_extra: bool,
    names_stdin: bool,
    test: bool,
    latest: bool,
    archive_comment: bool,
    entry_comments: bool,
    must_match: bool,
    show_files: bool,
    encrypt: bool,
    password: Option<Vec<u8>>,
    suffixes: Option<Vec<String>>,
    after: Option<i64>,
    before: Option<i64>,
    excludes: Vec<String>,
    includes: Vec<String>,
    output: Option<String>,
    stop_at_slash: bool,
    help: bool,
    /// `-l` (LF to CR LF, `true`) or `-ll` (CR LF to LF, `false`).
    eol: Option<bool>,
    /// `-s`: the size of each part; 0 makes one archive of a split one.
    split: Option<u64>,
    split_verbose: bool,
    /// `-F` (1) or `-FF` (2).
    fix: u8,
    zipfile: Option<String>,
    files: Vec<String>,
}

/// zip's options of two letters, tried before one.
const TWO_LETTERS: [&str; 31] = [
    "tt", "ws", "MM", "FS", "DF", "sf", "nw", "h2", "AC", "AS", "FF", "ll", "UN", "dd", "db", "dc",
    "dg", "ds", "du", "dv", "lf", "la", "li", "so", "sc", "sd", "sp", "sv", "sb", "fz", "ic",
];

/// The options that take a value, with what zip calls the value.
fn value_of(id: &str) -> Option<&'static str> {
    Some(match id {
        "b" => "dir to use for temp archive",
        "n" => "suffixes to not compress",
        "P" => "password",
        "t" => "exclude before date",
        "tt" => "include before date",
        "O" => "output-file",
        "Z" => "compression method",
        "s" => "split size",
        "ds" => "dot size",
        "lf" => "log file",
        "sp" => "split pause",
        _ => return None,
    })
}

/// The long names, for their letters.
fn long_option(name: &str) -> Option<&'static str> {
    Some(match name {
        "recurse-paths" => "r",
        "recurse-patterns" => "R",
        "junk-paths" => "j",
        "quiet" => "q",
        "verbose" => "v",
        "move" => "m",
        "delete" => "d",
        "update" => "u",
        "freshen" => "f",
        "filesync" => "FS",
        "no-dir-entries" => "D",
        "symlinks" => "y",
        "no-extra" => "X",
        "names-stdin" => "@",
        "test" => "T",
        "latest-time" => "o",
        "archive-comment" => "z",
        "entry-comments" => "c",
        "must-match" => "MM",
        "show-files" => "sf",
        "encrypt" => "e",
        "password" => "P",
        "suffixes" => "n",
        "temp-path" => "b",
        "output-file" | "out" => "O",
        "compression-method" => "Z",
        "wild-stop-dirs" => "ws",
        "no-wild" => "nw",
        "help" => "h",
        "version" => "v",
        "exclude" => "x",
        "include" => "i",
        "grow" => "g",
        "fix" => "F",
        "fixfix" => "FF",
        "split-size" => "s",
        "to-crlf" => "l",
        "from-crlf" => "ll",
        "license" => "L",
        "difference-archive" => "DF",
        "copy-entries" => "U",
        "adjust-sfx" => "A",
        "junk-sfx" => "J",
        "dos-names" => "k",
        "unicode" => "UN",
        _ => return None,
    })
}

/// A date of `-t` or `-tt`: `mmddyyyy` or `yyyy-mm-dd`, local midnight.
fn parse_date(text: &str, clock: &Clock) -> Option<i64> {
    let (year, month, day) = if let Some((y, rest)) = text.split_once('-') {
        let (m, d) = rest.split_once('-')?;
        (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
    } else {
        if text.len() != 8 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let digits: Vec<u32> = text.bytes().map(|b| u32::from(b - b'0')).collect();
        let number = |range: std::ops::Range<usize>| {
            digits
                .get(range)
                .unwrap_or_default()
                .iter()
                .fold(0, |a, d| a * 10 + d)
        };
        (
            i32::try_from(number(4..8)).ok()?,
            number(0..2),
            number(2..4),
        )
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(clock.seconds(cash_archive::zip::Civil {
        year,
        month,
        day,
        ..cash_archive::zip::Civil::default()
    }))
}

/// `-s`'s size: a number with `k`, `m`, `g` or `t` after it, megabytes without; at
/// least 64 KiB, or 0.
fn split_size(text: &str) -> Result<u64, String> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let unit = text
        .get(digits.len()..)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let number: u64 = digits
        .parse()
        .map_err(|_| format!("invalid size for -s:  '{text}'"))?;
    let scale: u64 = match unit.as_str() {
        "" | "m" => 1 << 20,
        "k" => 1 << 10,
        "g" => 1 << 30,
        "t" => 1 << 40,
        _ => return Err(format!("invalid size for -s:  '{text}'")),
    };
    let size = number.saturating_mul(scale);
    if size != 0 && size < 64 * 1024 {
        return Err(format!("minimum split size is 64 KB:  '{text}'"));
    }
    Ok(size)
}

/// Reads the command line as zip's `get_option` does: options anywhere, letters run
/// together, `-x` and `-i` taking the names after them.
#[expect(
    clippy::too_many_lines,
    reason = "zip's option table, one match for every option"
)]
fn parse(args: &[String], clock: &Clock) -> Result<Options, String> {
    let mut o = Options {
        level: 6,
        ..Options::default()
    };
    let mut list: Option<bool> = None;
    let mut only_names = false;
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        i += 1;
        if only_names || arg == "-" || !arg.starts_with('-') {
            match list {
                Some(true) if arg != "-" => o.excludes.push(arg.clone()),
                Some(false) if arg != "-" => o.includes.push(arg.clone()),
                _ => {
                    if o.zipfile.is_none() {
                        o.zipfile = Some(arg.clone());
                    } else {
                        o.files.push(arg.clone());
                    }
                }
            }
            continue;
        }
        list = None;
        if arg == "--" {
            only_names = true;
            continue;
        }
        let mut words: Vec<(String, Option<String>, bool)> = Vec::new();
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_owned())),
                None => (long, None),
            };
            let (name, negated) = match name.strip_suffix('-') {
                Some(n) => (n, true),
                None => (name, false),
            };
            let Some(id) = long_option(name) else {
                return Err(format!("long option '{name}' not supported"));
            };
            let value = if value_of(id).is_some() && value.is_none() {
                let next = args.get(i).cloned();
                i += 1;
                Some(next.ok_or_else(|| {
                    format!(
                        "option '{id}' ({}) requires a value",
                        value_of(id).unwrap_or_default()
                    )
                })?)
            } else {
                value
            };
            words.push((id.to_owned(), value, negated));
        } else {
            let letters: Vec<char> = arg.chars().skip(1).collect();
            let mut k = 0;
            while k < letters.len() {
                let two: String = letters.iter().skip(k).take(2).collect();
                let id = if two.chars().count() == 2 && TWO_LETTERS.contains(&two.as_str()) {
                    k += 2;
                    two
                } else {
                    k += 1;
                    letters.get(k - 1).map(char::to_string).unwrap_or_default()
                };
                let negated = letters.get(k) == Some(&'-');
                if negated {
                    k += 1;
                }
                if matches!(id.as_str(), "x" | "i") {
                    let rest: String = letters.iter().skip(k).collect();
                    k = letters.len();
                    words.push((id, (!rest.is_empty()).then_some(rest), false));
                    continue;
                }
                let value = if value_of(&id).is_some() {
                    let rest: String = letters.iter().skip(k).collect();
                    k = letters.len();
                    if rest.is_empty() {
                        let next = args.get(i).cloned();
                        i += 1;
                        Some(next.ok_or_else(|| {
                            format!(
                                "option '{id}' ({}) requires a value",
                                value_of(&id).unwrap_or_default()
                            )
                        })?)
                    } else {
                        Some(rest)
                    }
                } else {
                    None
                };
                words.push((id, value, negated));
            }
        }
        for (id, value, negated) in words {
            let on = !negated;
            let value = value.unwrap_or_default();
            match id.as_str() {
                digit if digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit()) => {
                    o.level = digit.parse().unwrap_or(6);
                }
                "r" => o.recurse = on,
                "j" => o.junk = on,
                "q" => o.quiet = on,
                "v" => o.verbose = if on { o.verbose + 1 } else { 0 },
                "m" => o.move_files = on,
                "d" => o.mode = Mode::Delete,
                "u" => o.mode = Mode::Update,
                "f" => o.mode = Mode::Freshen,
                "FS" => o.mode = Mode::FileSync,
                "D" => o.no_dirs = on,
                "y" => o.symlinks = on,
                "X" => o.no_extra = on,
                "@" => o.names_stdin = on,
                "T" => o.test = on,
                "o" => o.latest = on,
                "z" => o.archive_comment = on,
                "c" => o.entry_comments = on,
                "MM" => o.must_match = on,
                "sf" => o.show_files = on,
                "e" => o.encrypt = on,
                "P" => o.password = Some(value.into_bytes()),
                "n" => {
                    o.suffixes = Some(
                        value
                            .split([':', ';'])
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect(),
                    );
                }
                "t" | "tt" => {
                    let date = parse_date(&value, clock).ok_or_else(|| {
                        format!(
                            "invalid date entered for -{id} option - use mmddyyyy or yyyy-mm-dd"
                        )
                    })?;
                    if id == "t" {
                        o.after = Some(date);
                    } else {
                        o.before = Some(date);
                    }
                }
                "O" => o.output = Some(value),
                "Z" => {
                    o.method = Some(match value.to_ascii_lowercase().as_str() {
                        "store" => method::STORED,
                        "deflate" => method::DEFLATED,
                        "bzip2" => method::BZIP2,
                        _ => {
                            return Err(format!("\u{1}{value}"));
                        }
                    });
                }
                "ws" => o.stop_at_slash = on,
                "x" | "i" => {
                    let excluding = id == "x";
                    if value.is_empty() {
                        list = Some(excluding);
                    } else if excluding {
                        o.excludes.push(value);
                    } else {
                        o.includes.push(value);
                    }
                }
                "h" | "?" | "h2" => o.help = true,
                "l" => o.eol = on.then_some(true),
                "ll" => o.eol = on.then_some(false),
                "s" => o.split = Some(split_size(&value)?),
                "sv" => o.split_verbose = on,
                "F" => o.fix = 1,
                "FF" => o.fix = 2,
                "b" | "S" | "k" | "g" | "UN" | "nw" | "dd" | "db" | "dc" | "dg" | "ds" | "du"
                | "dv" | "fz" | "ic" | "so" | "sc" | "sd" | "sb" | "p" => {}
                "R" | "sp" | "U" | "A" | "J" | "L" | "DF" | "AC" | "AS" | "lf" | "la" | "li"
                | "w" | "$" | "!" => {
                    return Err(format!("option -{id} not supported by cash's zip"));
                }
                other => return Err(format!("short option '{other}' not supported")),
            }
        }
    }
    Ok(o)
}

/// A file or folder to add.
#[derive(Debug)]
struct Source {
    /// The name as given or found under a given folder.
    given: String,
    /// Where it is; `None` for standard input.
    path: Option<PathBuf>,
    /// Its name in the archive.
    name: String,
    view: Option<UnixView>,
}

impl Source {
    fn is_dir(&self) -> bool {
        self.view
            .as_ref()
            .is_some_and(|v| v.kind == unix::Kind::Dir)
    }

    fn modified(&self) -> i64 {
        self.view
            .as_ref()
            .map_or_else(now, |v| seconds_of(v.times.modified))
    }
}

fn seconds_of(time: std::time::SystemTime) -> i64 {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

fn now() -> i64 {
    seconds_of(std::time::SystemTime::now())
}

/// zip's `ex2in`: the archive's name for a file's: `/` between its parts, no drive, no
/// leading `/` or `./`, only the last part with `-j`, a folder's ending in `/`.
fn internal_name(given: &str, is_dir: bool, junk: bool) -> String {
    let mut name = given.replace('\\', "/");
    let bytes = name.as_bytes();
    if bytes.len() >= 2
        && bytes.get(1) == Some(&b':')
        && bytes.first().is_some_and(u8::is_ascii_alphabetic)
    {
        name = name.chars().skip(2).collect();
    }
    let mut rest = name.trim_start_matches('/');
    while let Some(after) = rest.strip_prefix("./") {
        rest = after.trim_start_matches('/');
    }
    let mut name = rest.to_owned();
    while name.contains("//") {
        name = name.replace("//", "/");
    }
    if name == "." {
        name.clear();
    }
    if junk {
        name = name
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
    }
    if is_dir && !name.is_empty() && !name.ends_with('/') {
        name.push('/');
    }
    name
}

/// What the archive will hold, in order.
enum Item {
    /// An entry of the old archive, copied as it is.
    Copy(usize),
    /// A file, added or replacing an entry, with the word for it.
    New(Box<Source>, &'static str),
}

/// A run of the command.
struct Zip<'a, SE: cash_core::ShellExtensions> {
    say: Say<'a, SE>,
    context: &'a cash_core::ExecutionContext<'a, SE>,
    clock: Clock,
    options: Options,
    /// Messages go to standard error when the archive goes to standard output.
    to_stdout: bool,
    status: u8,
    skipped: (u64, u64),
    read: (u64, u64),
    /// `--out` with no names: the old archive's members are copied, and said so.
    copying: bool,
}

/// The command, run.
pub(super) fn run<SE: cash_core::ShellExtensions>(
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    let say = Say { context };
    let clock = Clock::of(context);
    let mut args = args.to_vec();
    if args.is_empty() {
        if is_terminal(context, OpenFiles::STDOUT_FD) {
            say.out(ZIP_USAGE)?;
            return Ok(ExecutionResult::success());
        }
        args = vec!["-".to_owned(), "-".to_owned()];
    }
    if args == ["-v"] {
        say.out(ZIP_VERSION)?;
        return Ok(ExecutionResult::success());
    }
    let options = match parse(&args, &clock) {
        Ok(options) => options,
        Err(message) => {
            if let Some(method) = message.strip_prefix('\u{1}') {
                say.err(&format!(
                    "\tzip warning: valid compression methods are:  store, deflate, bzip2\n\tzip warning: unknown compression method found:  {method}\n\nzip error: Invalid command arguments (Option -Z (--compression-method):  unknown method)\n"
                ))?;
            } else {
                say.err(&format!(
                    "\nzip error: Invalid command arguments ({message})\n"
                ))?;
            }
            return Ok(ExecutionResult::new(code::PARAM));
        }
    };
    if options.help {
        say.out(ZIP_USAGE)?;
        return Ok(ExecutionResult::success());
    }
    let to_stdout = options.zipfile.as_deref() == Some("-");
    let mut zip = Zip {
        say,
        context,
        clock,
        options,
        to_stdout,
        status: 0,
        skipped: (0, 0),
        read: (0, 0),
        copying: false,
    };
    zip.run()
}

impl<SE: cash_core::ShellExtensions> Zip<'_, SE> {
    /// A message of the run: on standard output, or standard error when the archive
    /// goes there.
    fn note(&self, text: &str) -> io::Result<()> {
        if self.to_stdout {
            self.say.err(text)
        } else {
            self.say.out(text)
        }
    }

    fn fail(&self, message: &str, status: u8) -> Result<ExecutionResult, cash_core::Error> {
        self.say.err(&format!("\nzip error: {message}\n"))?;
        Ok(ExecutionResult::new(status))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "zip's main: the archive read, the names gathered, the plan, the writing"
    )]
    fn run(&mut self) -> Result<ExecutionResult, cash_core::Error> {
        let Some(mut zipfile) = self.options.zipfile.clone() else {
            return self.fail("Nothing to do! ()", code::NONE);
        };
        if zipfile != "-"
            && !super::is_substitution_file(&self.context.shell.absolute_path(&zipfile))
        {
            let last = zipfile.rsplit(['/', '\\']).next().unwrap_or_default();
            if !last.contains('.') {
                zipfile.push_str(".zip");
            }
        }
        let path = (zipfile != "-").then(|| self.context.shell.absolute_path(&zipfile));
        if self.options.split.is_some_and(|s| s > 0)
            && (zipfile == "-" || !zipfile.to_ascii_lowercase().ends_with(".zip"))
        {
            return self.fail(
                "Invalid command arguments (archive name must end in .zip for splits)",
                code::PARAM,
            );
        }
        if self.options.fix > 0 {
            if let Some(path) = &path {
                return self.fix(&zipfile, path);
            }
        }
        let mut old: Option<(ArchiveSource, Archive)> = None;
        if let Some(path) = &path {
            // An empty file, a `>(...)` among them, is an archive not yet written.
            if fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0) {
                let mut file = fs::File::open(path)?;
                match read::open(&mut file) {
                    Ok(archive) if archive.end.disk > 0 => {
                        if self.options.output.is_none() {
                            self.say.err(
                                "\tzip warning: cannot update a split archive (use --out option)\n",
                            )?;
                            return self.fail(
                                &format!("Invalid command arguments ({zipfile})"),
                                code::PARAM,
                            );
                        }
                        old = Some(
                            super::parts_source(path, &archive)
                                .unwrap_or((ArchiveSource::File(file), archive)),
                        );
                    }
                    Ok(archive) => old = Some((ArchiveSource::File(file), archive)),
                    Err(OpenError::NoEnd | OpenError::BadCentral(..)) => {
                        self.say.err("\tzip warning: missing end signature--probably not a zip file (did you\n\tzip warning: remember to use binary mode when you transferred it?)\n\tzip warning: (if you are trying to read a damaged archive try -F)\n")?;
                        return self.fail(
                            &format!("Zip file structure invalid ({zipfile})"),
                            code::FORMAT,
                        );
                    }
                    Err(OpenError::Io(e)) => return Err(e.into()),
                }
            } else if matches!(
                self.options.mode,
                Mode::Delete | Mode::Update | Mode::Freshen
            ) {
                self.say
                    .err(&format!("\tzip warning: {zipfile} not found or empty\n"))?;
            }
        }
        if self.options.names_stdin {
            while let Some(line) = read_line(self.context, true)? {
                if !line.is_empty() {
                    self.options.files.push(line);
                }
            }
        }
        if self.options.show_files {
            return self.show_files(old.as_ref().map(|(_, a)| a));
        }
        let mut items: Vec<Item> = Vec::new();
        let mut positions: HashMap<String, usize> = HashMap::new();
        if let Some((_, archive)) = &old {
            for (index, entry) in archive.entries.iter().enumerate() {
                positions.insert(name_text(entry), items.len());
                items.push(Item::Copy(index));
            }
        }
        let mut deleted: Vec<String> = Vec::new();
        let mut changes = 0;
        if self.options.mode == Mode::Delete {
            let Some((_, archive)) = &old else {
                for name in &self.options.files {
                    self.say
                        .err(&format!("\tzip warning: name not matched: {name}\n"))?;
                }
                return self.fail(&format!("Nothing to do! ({zipfile})"), code::NONE);
            };
            let patterns = self.options.files.clone();
            let mut hits = vec![false; patterns.len()];
            items.retain(|item| {
                let Item::Copy(index) = item else {
                    return true;
                };
                let Some(entry) = archive.entries.get(*index) else {
                    return true;
                };
                let name = name_text(entry);
                let mut gone = false;
                for (hit, pattern) in hits.iter_mut().zip(&patterns) {
                    if matches(pattern, &name, false, self.options.stop_at_slash) {
                        *hit = true;
                        gone = true;
                    }
                }
                if gone {
                    deleted.push(name);
                }
                !gone
            });
            for (hit, pattern) in hits.iter().zip(&patterns) {
                if !hit {
                    self.say
                        .err(&format!("\tzip warning: name not matched: {pattern}\n"))?;
                }
            }
            if deleted.is_empty() {
                return self.fail(&format!("Nothing to do! ({zipfile})"), code::NONE);
            }
            for name in &deleted {
                self.note(&format!("deleting: {name}\n"))?;
            }
            changes += deleted.len();
        } else {
            let sources = match self.collect()? {
                Ok(sources) => sources,
                Err(result) => return Ok(result),
            };
            let given: std::collections::HashSet<String> =
                sources.iter().map(|s| s.name.clone()).collect();
            for source in sources {
                let existing = positions.get(&source.name).copied();
                match existing {
                    Some(at) => {
                        let Some(Item::Copy(index)) = items.get(at) else {
                            continue;
                        };
                        let entry = old
                            .as_ref()
                            .and_then(|(_, a)| a.entries.get(*index))
                            .cloned()
                            .unwrap_or_default();
                        let newer =
                            self.dos_time(source.modified()) > (entry.time.date, entry.time.time);
                        let replace = match self.options.mode {
                            Mode::Add => true,
                            Mode::Update | Mode::Freshen => newer,
                            Mode::FileSync => {
                                newer
                                    || source.view.as_ref().is_some_and(|v| {
                                        v.kind == unix::Kind::File && v.size != entry.size
                                    })
                            }
                            Mode::Delete => false,
                        };
                        if replace {
                            let word = if self.options.mode == Mode::Freshen {
                                "freshening"
                            } else {
                                "updating"
                            };
                            if let Some(slot) = items.get_mut(at) {
                                *slot = Item::New(Box::new(source), word);
                            }
                            changes += 1;
                        }
                    }
                    None => {
                        if self.options.mode != Mode::Freshen {
                            positions.insert(source.name.clone(), items.len());
                            items.push(Item::New(Box::new(source), "  adding"));
                            changes += 1;
                        }
                    }
                }
            }
            if self.options.mode == Mode::FileSync {
                if let Some((_, archive)) = &old {
                    items.retain(|item| {
                        let Item::Copy(index) = item else {
                            return true;
                        };
                        let name = archive
                            .entries
                            .get(*index)
                            .map(name_text)
                            .unwrap_or_default();
                        if given.contains(&name) {
                            true
                        } else {
                            deleted.push(name);
                            false
                        }
                    });
                }
                for name in &deleted {
                    self.note(&format!("deleting: {name}\n"))?;
                }
                changes += deleted.len();
            }
        }
        // `--out` with no names copies the archive: to one file, or into parts with `-s`.
        if self.options.output.is_some()
            && self.options.files.is_empty()
            && self.options.mode == Mode::Add
            && old.is_some()
        {
            self.copying = true;
            changes += items.len();
        }
        let comment_change = self.options.archive_comment;
        if changes == 0 && !comment_change {
            if old.is_some()
                && matches!(
                    self.options.mode,
                    Mode::Update | Mode::Freshen | Mode::FileSync
                )
            {
                return Ok(ExecutionResult::new(
                    if self.options.mode == Mode::FileSync {
                        0
                    } else {
                        code::NONE
                    },
                ));
            }
            return self.fail(&format!("Nothing to do! ({zipfile})"), code::NONE);
        }
        if self.options.encrypt && self.options.password.is_none() {
            match self.ask_password()? {
                Ok(password) => self.options.password = Some(password),
                Err(message) => {
                    return self.fail(
                        &format!("Invalid command arguments ({message})"),
                        code::PARAM,
                    );
                }
            }
        }
        let mut comment = old
            .as_ref()
            .map(|(_, a)| a.end.comment.clone())
            .unwrap_or_default();
        let target = self
            .options
            .output
            .as_deref()
            .map(|o| self.context.shell.absolute_path(o))
            .or(path);
        let mut written: Vec<(String, PathBuf)> = Vec::new();
        let entries;
        if let Some(target) = target.as_ref().filter(|t| super::is_substitution_file(t)) {
            // The shell reads a `>(...)` back through its own handle: it is written in
            // place, not renamed over.
            let mut output = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .truncate(true)
                .open(target)?;
            let mut writer = Writer::new(&mut output);
            self.write_items(&mut writer, items, &mut old, &mut written)?;
            entries = writer.entries().to_vec();
            if self.options.archive_comment {
                comment = self.read_comment(&comment)?;
            }
            writer.finish(&comment)?;
        } else if let (Some(target), Some(size)) =
            (&target, self.options.split.filter(|size| *size > 0))
        {
            if self.options.split_verbose {
                self.note(&format!("splitsize = {size}\n"))?;
            }
            let output = match SplitOutput::create(target, size) {
                Ok(output) => output,
                Err(e) => {
                    self.say.err(&format!("zip I/O error: {}", strerror(&e)))?;
                    return self.fail(
                        &format!("Could not create output file ({zipfile})"),
                        code::CREATE,
                    );
                }
            };
            let mut writer = Writer::new(output);
            self.write_items(&mut writer, items, &mut old, &mut written)?;
            entries = writer.entries().to_vec();
            let comments = self.entry_comments(&entries)?;
            writer.set_comments(&comments);
            if self.options.archive_comment {
                comment = self.read_comment(&comment)?;
            }
            let output = writer.finish(&comment)?;
            drop(old.take());
            match output.finish(target) {
                Ok(parts) => {
                    if self.options.split_verbose {
                        for part in parts {
                            let name = part
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            self.note(&format!("\tClosing split {name}\n"))?;
                        }
                    }
                }
                Err(e) => {
                    self.say.err(&format!("zip I/O error: {}", strerror(&e)))?;
                    return self.fail(
                        &format!("Could not create output file ({zipfile})"),
                        code::CREATE,
                    );
                }
            }
        } else if let Some(target) = &target {
            let mut replacement = match Replacement::create(target.clone()) {
                Ok(replacement) => replacement,
                Err(e) => {
                    self.say.err(&format!("zip I/O error: {}", strerror(&e)))?;
                    return self.fail(
                        &format!("Could not create output file ({zipfile})"),
                        code::CREATE,
                    );
                }
            };
            let mut writer = Writer::new(replacement.file());
            self.write_items(&mut writer, items, &mut old, &mut written)?;
            entries = writer.entries().to_vec();
            let comments = self.entry_comments(&entries)?;
            writer.set_comments(&comments);
            if self.options.archive_comment {
                comment = self.read_comment(&comment)?;
            }
            writer.finish(&comment)?;
            let modified = if self.options.latest {
                entries
                    .iter()
                    .map(|e| self.clock.seconds(e.time.civil()))
                    .max()
                    .map_or_else(std::time::SystemTime::now, system_time)
            } else {
                std::time::SystemTime::now()
            };
            drop(old.take());
            if let Err(e) =
                replacement.finish(fs::FileTimes::new().set_modified(modified), false, false)
            {
                self.say.err(&format!("zip I/O error: {}", strerror(&e)))?;
                return self.fail(
                    &format!("Could not create output file ({zipfile})"),
                    code::CREATE,
                );
            }
        } else {
            let mut writer = Writer::new(Stream(self.context.stdout()));
            self.write_items(&mut writer, items, &mut old, &mut written)?;
            entries = writer.entries().to_vec();
            if self.options.archive_comment {
                comment = self.read_comment(&comment)?;
            }
            writer.finish(&comment)?;
        }
        if entries.is_empty() {
            self.say.err("\tzip warning: zip file empty\n")?;
        }
        if self.options.verbose > 0 && !self.options.quiet {
            let size: u64 = self.read.1;
            let compressed: u64 = entries
                .iter()
                .filter(|e| {
                    written
                        .iter()
                        .any(|(n, _)| n.as_bytes() == e.name.as_slice())
                })
                .map(|e| e.compressed_size)
                .sum();
            self.note(&format!(
                "total bytes={size}, compressed={compressed} -> {}% savings\n",
                percent(size, compressed)
            ))?;
        }
        if self.skipped.0 > 0 {
            self.say.err(&format!(
                "\nzip warning: Not all files were readable\n  files/entries read:  {} ({} bytes)  skipped:  {} ({} bytes)\n",
                self.read.0, self.read.1, self.skipped.0, self.skipped.1
            ))?;
            self.status = self.status.max(code::OPEN);
        }
        if self.options.test {
            if let Some(target) = &target {
                if test_archive(target) {
                    self.note(&format!("test of {zipfile} OK\n"))?;
                } else {
                    self.note(&format!("test of {zipfile} FAILED\n"))?;
                    return self.fail(
                        "Zip file invalid, could not spawn unzip, or wrong unzip (original files unmodified)",
                        code::TEST,
                    );
                }
            }
        }
        if self.options.move_files {
            for (_, path) in written.iter().rev() {
                if path.is_dir() {
                    let _ = fs::remove_dir(path);
                } else {
                    let _ = unix::remove_even_read_only(path);
                }
            }
        }
        Ok(ExecutionResult::new(self.status))
    }

    /// The MS-DOS time of a moment, as zip rounds it: up to the even second.
    fn dos_time(&self, seconds: i64) -> (u16, u16) {
        let dos = DosTime::from_civil(self.clock.civil((seconds + 1) & !1));
        (dos.date, dos.time)
    }

    /// The names on the command line, folders walked with `-r`, filtered.
    fn collect(&self) -> io::Result<Result<Vec<Source>, ExecutionResult>> {
        let mut out = Vec::new();
        for given in self.options.files.clone() {
            if given == "-" {
                out.push(Source {
                    given: given.clone(),
                    path: None,
                    name: "-".to_owned(),
                    view: None,
                });
                continue;
            }
            let path = self.context.shell.absolute_path(&given);
            if let Ok(view) = unix::unix_view(&path, !self.options.symlinks) {
                self.walk(&given, &path, view, &mut out);
            } else {
                if given.contains(['*', '?', '[']) {
                    continue;
                }
                self.say
                    .err(&format!("\tzip warning: name not matched: {given}\n"))?;
                if self.options.must_match {
                    self.say.err("zip I/O error: No such file or directory")?;
                    self.say.err(&format!(
                        "\nzip error: File not found or no read permission ({given})\n"
                    ))?;
                    return Ok(Err(ExecutionResult::new(code::OPEN)));
                }
            }
        }
        Ok(Ok(out))
    }

    fn walk(&self, given: &str, path: &std::path::Path, view: UnixView, out: &mut Vec<Source>) {
        let is_dir = view.kind == unix::Kind::Dir;
        let name = internal_name(given, is_dir, self.options.junk);
        let modified = seconds_of(view.times.modified);
        let wanted = !name.is_empty()
            && !(is_dir && (self.options.no_dirs || self.options.junk))
            && !self
                .options
                .excludes
                .iter()
                .any(|p| matches(p, &name, false, self.options.stop_at_slash))
            && (self.options.includes.is_empty()
                || self
                    .options
                    .includes
                    .iter()
                    .any(|p| matches(p, &name, false, self.options.stop_at_slash)))
            && self.options.after.is_none_or(|t| modified >= t)
            && self.options.before.is_none_or(|t| modified < t);
        if wanted {
            out.push(Source {
                given: given.to_owned(),
                path: Some(path.to_path_buf()),
                name,
                view: Some(view),
            });
        }
        if is_dir && self.options.recurse {
            let Ok(children) = fs::read_dir(path) else {
                return;
            };
            let names: Vec<String> = children
                .filter_map(Result::ok)
                .map(|c| c.file_name().to_string_lossy().into_owned())
                .collect();
            for child in names {
                let child_given = if given.ends_with('/') || given.ends_with('\\') {
                    format!("{given}{child}")
                } else {
                    format!("{given}/{child}")
                };
                let child_path = path.join(&child);
                if let Ok(view) = unix::unix_view(&child_path, !self.options.symlinks) {
                    self.walk(&child_given, &child_path, view, out);
                }
            }
        }
    }

    /// Writes the plan: old entries copied, files added.
    fn write_items<O: cash_archive::zip::write::Output>(
        &mut self,
        writer: &mut Writer<O>,
        items: Vec<Item>,
        old: &mut Option<(ArchiveSource, Archive)>,
        written: &mut Vec<(String, PathBuf)>,
    ) -> io::Result<()> {
        for item in items {
            match item {
                Item::Copy(index) => {
                    if let Some((file, archive)) = old.as_mut() {
                        if let Some(entry) = archive.entries.get(index).cloned() {
                            if self.copying && !self.options.quiet {
                                self.note(&format!(" copying: {}\n", name_text(&entry)))?;
                            }
                            let _ = writer.copy(file, archive, &entry);
                        }
                    }
                }
                Item::New(source, word) => {
                    self.add(writer, &source, word)?;
                    if let Some(path) = &source.path {
                        written.push((source.name.clone(), path.clone()));
                    }
                }
            }
        }
        Ok(())
    }

    /// Adds one file, saying so.
    #[expect(
        clippy::too_many_lines,
        reason = "zip's zipup: the attributes, the extra fields, the data, the words"
    )]
    fn add<O: cash_archive::zip::write::Output>(
        &mut self,
        writer: &mut Writer<O>,
        source: &Source,
        word: &str,
    ) -> io::Result<()> {
        let speak = !self.options.quiet;
        if speak {
            self.note(&format!("{word}: {}", source.name))?;
        }
        let modified = source.modified();
        let accessed = source
            .view
            .as_ref()
            .and_then(|v| v.times.accessed)
            .map_or(modified, seconds_of);
        let (date, time) = self.dos_time(modified);
        let dos = DosTime { date, time };
        let symlink = source
            .view
            .as_ref()
            .is_some_and(|v| v.kind == unix::Kind::Symlink);
        let is_dir = source.is_dir();
        let mode = match &source.view {
            None => S_IFIFO | 0o600,
            Some(view) => {
                let kind = match view.kind {
                    unix::Kind::Dir => S_IFDIR,
                    unix::Kind::Symlink => S_IFLNK,
                    unix::Kind::File => S_IFREG,
                };
                kind | if symlink {
                    0o777
                } else {
                    view.permissions & 0o7777
                }
            }
        };
        let mut dos_bits = 0_u32;
        if is_dir {
            dos_bits |= u32::from(DOS_DIRECTORY);
        }
        if mode & 0o200 == 0 {
            dos_bits |= u32::from(DOS_READ_ONLY);
        }
        let suffix_stored = self
            .options
            .suffixes
            .clone()
            .unwrap_or_else(|| {
                [".Z", ".zip", ".zoo", ".arc", ".lzh", ".arj"]
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect()
            })
            .iter()
            .any(|s| source.name.ends_with(s.as_str()));
        let method = if self.options.level == 0 || suffix_stored || symlink {
            method::STORED
        } else {
            self.options.method.unwrap_or(method::DEFLATED)
        };
        let (local_extra, central_extra) = if self.options.no_extra || source.view.is_none() {
            (Vec::new(), Vec::new())
        } else {
            let clamp =
                |t: i64| i32::try_from(t).unwrap_or(if t < 0 { i32::MIN } else { i32::MAX });
            let mut local = vec![3_u8];
            local.extend_from_slice(&clamp(modified).to_le_bytes());
            local.extend_from_slice(&clamp(accessed).to_le_bytes());
            let mut central = vec![3_u8];
            central.extend_from_slice(&clamp(modified).to_le_bytes());
            let mut owner = vec![1_u8, 4];
            let (uid, gid) = source
                .view
                .as_ref()
                .map_or((0, 0), |v| (v.owner.id, v.group.id));
            owner.extend_from_slice(&uid.to_le_bytes());
            owner.push(4);
            owner.extend_from_slice(&gid.to_le_bytes());
            let mut l = field(extra_id::TIMES, &local);
            l.extend(field(extra_id::UNIX_OWNER, &owner));
            let mut c = field(extra_id::TIMES, &central);
            c.extend(field(extra_id::UNIX_OWNER, &owner));
            (l, c)
        };
        let member = NewMember {
            name: source.name.clone().into_bytes(),
            method,
            level: self.options.level.max(1),
            time: dos,
            local_extra,
            central_extra,
            external_attributes: mode << 16 | dos_bits,
            password: if is_dir {
                None
            } else {
                self.options.password.clone()
            },
            comment: Vec::new(),
            detect_text: method == method::DEFLATED,
            size_hint: if source.view.is_none() {
                Some(u64::MAX)
            } else {
                source.view.as_ref().map(|v| v.size)
            },
        };
        let added = if is_dir {
            writer.add(&member, Input::None)?
        } else if symlink {
            let target = source
                .view
                .as_ref()
                .and_then(|v| v.link_target.as_ref())
                .map(|t| t.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            writer.add(
                &member,
                Input::File(&mut io::Cursor::new(target.into_bytes())),
            )?
        } else if let Some(path) = &source.path {
            match fs::File::open(path) {
                Ok(mut file) => match self.options.eol {
                    Some(to_crlf) => {
                        let mut head = Vec::new();
                        (&mut file).take(64 * 1024).read_to_end(&mut head)?;
                        file.seek(SeekFrom::Start(0))?;
                        if head.is_empty() || is_text(&head) {
                            let mut converted = Eol::new(file, to_crlf);
                            writer.add(&member, Input::File(&mut converted))?
                        } else {
                            if speak {
                                self.note("\n")?;
                            }
                            let flag = if to_crlf { "-l" } else { "-ll" };
                            self.say
                                .err(&format!("\tzip warning: has binary so {flag} ignored\n"))?;
                            writer.add(&member, Input::File(&mut file))?
                        }
                    }
                    None => writer.add(&member, Input::File(&mut file))?,
                },
                Err(e) => {
                    if speak {
                        self.note("\n")?;
                    }
                    self.say.err(&format!(
                        "zip warning: {}\n\tzip warning: could not open for reading: {}\n",
                        strerror(&e),
                        source.given
                    ))?;
                    self.skipped.0 += 1;
                    self.skipped.1 += source.view.as_ref().map_or(0, |v| v.size);
                    return Ok(());
                }
            }
        } else {
            // What fits in zip's window is kept, so that it can be stored when deflate
            // does not shrink it; the rest is streamed.
            let mut stdin = self.context.stdin();
            let mut head = Vec::new();
            (&mut stdin).take(64 * 1024).read_to_end(&mut head)?;
            if head.len() < 64 * 1024 {
                writer.add(&member, Input::File(&mut io::Cursor::new(head)))?
            } else {
                let mut stream = io::Cursor::new(head).chain(stdin);
                writer.add(&member, Input::Stream(&mut stream))?
            }
        };
        self.read.0 += 1;
        self.read.1 += added.size;
        let data = added
            .compressed_size
            .saturating_sub(if member.password.is_some() && !is_dir {
                12
            } else {
                0
            });
        let how = match added.method {
            method::DEFLATED => "deflated",
            method::BZIP2 => "bzipped",
            _ => "stored",
        };
        let pct = if added.method == method::STORED {
            0
        } else {
            percent(added.size, data)
        };
        if speak {
            if self.options.verbose > 0 {
                // zip shows a space where its progress dots would start: once it has
                // read a second buffer, which storing always does.
                let space = if method == method::STORED || added.size >= 65_536 {
                    " "
                } else {
                    ""
                };
                self.note(&format!(
                    "{space}\t(in={}) (out={}) ({how} {pct}%)\n",
                    added.size, added.compressed_size
                ))?;
            } else {
                self.note(&format!(" ({how} {pct}%)\n"))?;
            }
        }
        Ok(())
    }

    /// `-sf`.
    fn show_files(&self, old: Option<&Archive>) -> Result<ExecutionResult, cash_core::Error> {
        let mut out = String::new();
        if self.options.files.is_empty() {
            let Some(archive) = old else {
                return self.fail(
                    &format!(
                        "Nothing to do! ({})",
                        self.options.zipfile.clone().unwrap_or_default()
                    ),
                    code::NONE,
                );
            };
            out.push_str("Archive contains:\n");
            let mut bytes = 0;
            for entry in &archive.entries {
                let _ = writeln!(out, "  {}", name_text(entry));
                bytes += entry.size;
            }
            let _ = writeln!(
                out,
                "Total {} entries ({bytes} bytes)",
                archive.entries.len()
            );
        } else {
            out.push_str("Would Add/Update:\n");
            let mut bytes = 0;
            let mut count = 0;
            for given in &self.options.files {
                let path = self.context.shell.absolute_path(given);
                if let Ok(view) = unix::unix_view(&path, true) {
                    let _ = writeln!(
                        out,
                        "  {}",
                        internal_name(given, view.kind == unix::Kind::Dir, self.options.junk)
                    );
                    bytes += view.size;
                    count += 1;
                }
            }
            let _ = writeln!(out, "Total {count} entries ({bytes} bytes)");
        }
        self.say.out(&out)?;
        Ok(ExecutionResult::success())
    }

    /// `-z`: the new comment, read up to a line of `.` or the end of the input.
    fn read_comment(&self, current: &[u8]) -> io::Result<Vec<u8>> {
        if !self.options.quiet {
            if !current.is_empty() {
                self.say.err(&format!(
                    "current zip file comment is:\n{}\n",
                    String::from_utf8_lossy(current)
                ))?;
            }
            self.say.err("enter new zip file comment (end with .):\n")?;
        }
        let mut lines: Vec<String> = Vec::new();
        while let Some(line) = read_line(self.context, true)? {
            if line == "." {
                break;
            }
            lines.push(line);
        }
        Ok(lines.join("\r\n").into_bytes())
    }

    /// `-c`: a one-line comment for each member added.
    fn entry_comments(&self, entries: &[Entry]) -> io::Result<Vec<Vec<u8>>> {
        let mut comments = Vec::new();
        for entry in entries {
            if !self.options.entry_comments {
                comments.push(entry.comment.clone());
                continue;
            }
            self.say
                .err(&format!("Enter comment for {}:\n", name_text(entry)))?;
            let line = read_line(self.context, true)?.unwrap_or_default();
            comments.push(line.into_bytes());
        }
        Ok(comments)
    }

    /// `-e`: the password, typed twice at the console.
    fn ask_password(&self) -> io::Result<Result<Vec<u8>, String>> {
        if !is_terminal(self.context, OpenFiles::STDERR_FD)
            || !is_terminal(self.context, OpenFiles::STDIN_FD)
        {
            return Ok(Err("stderr is not a tty".to_owned()));
        }
        self.say.err("Enter password: ")?;
        let first = read_line(self.context, false)?.unwrap_or_default();
        self.say.err("\nVerify password: ")?;
        let second = read_line(self.context, false)?.unwrap_or_default();
        self.say.err("\n")?;
        if first != second {
            return Ok(Err("password verification failed".to_owned()));
        }
        Ok(Ok(first.into_bytes()))
    }
}

/// Info-ZIP's `is_text_buf`: no byte it black-lists, and one it white-lists.
fn is_text(bytes: &[u8]) -> bool {
    let mut white = false;
    for b in bytes {
        match b {
            0..=6 | 14..=25 | 28..=31 => return false,
            9 | 10 | 13 | 32.. => white = true,
            _ => {}
        }
    }
    white
}

/// A file read with its line ends changed: `-l`'s LF to CR LF, or `-ll`'s CR LF to LF.
struct Eol<R> {
    inner: R,
    to_crlf: bool,
    /// A CR at the end of the last chunk, which a LF may follow.
    pending_cr: bool,
    out: Vec<u8>,
    at: usize,
    done: bool,
}

impl<R: Read + Seek> Eol<R> {
    const fn new(inner: R, to_crlf: bool) -> Self {
        Self {
            inner,
            to_crlf,
            pending_cr: false,
            out: Vec::new(),
            at: 0,
            done: false,
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        let mut chunk = vec![0_u8; 32 * 1024];
        let n = self.inner.read(&mut chunk)?;
        self.out.clear();
        self.at = 0;
        if n == 0 {
            if self.pending_cr {
                self.out.push(b'\r');
                self.pending_cr = false;
            }
            self.done = true;
            return Ok(());
        }
        for &b in chunk.get(..n).unwrap_or_default() {
            if self.to_crlf {
                if b == b'\n' {
                    self.out.push(b'\r');
                }
                self.out.push(b);
            } else if self.pending_cr {
                self.pending_cr = false;
                if b != b'\n' {
                    self.out.push(b'\r');
                }
                if b == b'\r' {
                    self.pending_cr = true;
                } else {
                    self.out.push(b);
                }
            } else if b == b'\r' {
                self.pending_cr = true;
            } else {
                self.out.push(b);
            }
        }
        Ok(())
    }
}

impl<R: Read + Seek> Read for Eol<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.at >= self.out.len() {
            if self.done {
                return Ok(0);
            }
            self.fill()?;
        }
        let left = self.out.get(self.at..).unwrap_or_default();
        let n = left.len().min(buf.len());
        buf.get_mut(..n)
            .unwrap_or_default()
            .copy_from_slice(left.get(..n).unwrap_or_default());
        self.at += n;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for Eol<R> {
    /// Only back to the start, which the retry as stored needs.
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        if to != SeekFrom::Start(0) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "only back to the start",
            ));
        }
        self.inner.seek(SeekFrom::Start(0))?;
        self.pending_cr = false;
        self.out.clear();
        self.at = 0;
        self.done = false;
        Ok(0)
    }
}

/// `-T`: every member of the written archive read back and its CRC checked.
fn test_archive(path: &std::path::Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let Ok(archive) = read::open(&mut file) else {
        return false;
    };
    for entry in &archive.entries {
        if entry.is_dir() {
            continue;
        }
        let Ok(local) = read::local(&mut file, &archive, entry) else {
            return false;
        };
        if file.seek(SeekFrom::Start(local.data_offset)).is_err() {
            return false;
        }
        if entry.is_encrypted() {
            continue;
        }
        let raw = (&mut file).take(entry.compressed_size);
        let Ok(mut reader) = read::data(raw, entry, None) else {
            return false;
        };
        let mut crc = crc32fast::Hasher::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => crc.update(buffer.get(..n).unwrap_or_default()),
                Err(_) => return false,
            }
        }
        if crc.finalize() != entry.crc {
            return false;
        }
    }
    true
}

fn system_time(seconds: i64) -> std::time::SystemTime {
    let magnitude = std::time::Duration::from_secs(seconds.unsigned_abs());
    if seconds >= 0 {
        std::time::UNIX_EPOCH + magnitude
    } else {
        std::time::UNIX_EPOCH
            .checked_sub(magnitude)
            .unwrap_or(std::time::UNIX_EPOCH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn the_command_line_reads_as_zip_reads_it() {
        let clock = Clock {
            zone: cash_core::timefmt::Zone::from_tz(Some("UTC0")),
        };
        let o = parse(
            &words(&["-rq9", "a.zip", "src", "-x", "*.o", "*.a", "-D"]),
            &clock,
        )
        .unwrap_or_default();
        assert!(o.recurse && o.quiet && o.no_dirs);
        assert_eq!(o.level, 9);
        assert_eq!(o.zipfile.as_deref(), Some("a.zip"));
        assert_eq!(o.files, ["src"]);
        assert_eq!(o.excludes, ["*.o", "*.a"]);
        let o = parse(
            &words(&[
                "-tt",
                "2021-01-01",
                "-X-",
                "--recurse-paths",
                "-Zbzip2",
                "b",
                "c",
            ]),
            &clock,
        )
        .unwrap_or_default();
        assert_eq!(o.before, Some(1_609_459_200));
        assert!(o.recurse && !o.no_extra);
        assert_eq!(o.method, Some(method::BZIP2));
        assert_eq!(
            parse(&words(&["-Y", "a"]), &clock).err().as_deref(),
            Some("short option 'Y' not supported")
        );
        assert_eq!(
            parse(&words(&["-b"]), &clock).err().as_deref(),
            Some("option 'b' (dir to use for temp archive) requires a value")
        );
    }

    #[test]
    fn names_go_into_the_archive_as_zip_puts_them() {
        assert_eq!(internal_name("./src/a.txt", false, false), "src/a.txt");
        assert_eq!(internal_name("/tmp/x/a.txt", false, false), "tmp/x/a.txt");
        assert_eq!(internal_name("C:\\data\\a.txt", false, false), "data/a.txt");
        assert_eq!(internal_name("../x", false, false), "../x");
        assert_eq!(internal_name("src", true, false), "src/");
        assert_eq!(internal_name("src/sub/b.txt", false, true), "b.txt");
        assert_eq!(internal_name(".", true, false), "");
    }
}
