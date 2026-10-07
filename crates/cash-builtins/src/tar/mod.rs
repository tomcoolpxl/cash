//! `tar`: GNU tar 1.35's interface, messages and exit codes, on cash-archive's tar
//! reader and writer, its codecs and its matching.
//!
//! Checked against GNU tar 1.35 (`crates/cash/tests/oracle/tar_cases.sh`): listings,
//! extraction, every message and status, and the archive's bytes, which with the same
//! options are GNU tar's byte for byte. Windows' own `tar.exe` (bsdtar) stays reachable
//! by its path and by `enable -n tar`.
//!
//! Where cash differs, on purpose:
//! - The version line is cash's.
//! - Compressors cash does not carry (`-Z`, `--lzop`, `-I PROGRAM`) and what needs a
//!   tape, a second volume or a snapshot file (`-M`, `-g`, `-G`, `-L`, `-F`, `-W`,
//!   `--to-command`, `--index-file`, `-w`) are refused, by name.
//! - Owners, groups, ACLs and extended attributes are not restored: Windows' accounts
//!   are not Unix's. Read-only is, from the owner's write bit, and times are.
//! - A member's name Windows cannot hold (`:`, `\`, a trailing dot, `CON`) is refused by
//!   name, not changed; a symbolic link is made where Windows allows it (Developer
//!   Mode, or an elevated shell), and refused as GNU tar refuses one otherwise.
//! - `--sort=none` reads a folder in the order Windows gives, which on NTFS is by name.

mod create;
mod edit;
pub(super) mod help;
mod list;
mod options;
mod transform;

use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use cash_archive::codec::{self, Codec, Lookahead};
use cash_archive::listing::{self, mode_string};
use cash_archive::member::{Kind, Member, Timestamp};
use cash_archive::select::{self, Flags};
use cash_core::openfiles::OpenFiles;
use cash_core::timefmt::Zone;
use cash_core::{ExecutionResult, builtins};
use clap::Parser;

use options::{Matching, Name, Op, Options, Stop, TRY, TopLevel};

use crate::compress::{is_terminal, strerror};

/// The version line.
const VERSION: &str = "tar (cash): GNU tar 1.35's options, in pure Rust";

/// Create, list or extract archives with GNU tar's options.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct TarCommand {
    /// Options and names, read here as GNU tar reads them.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for TarCommand {
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
        run(&self.args, &context)
    }
}

/// Why a run stopped.
#[derive(Debug)]
enum Fatal {
    /// GNU's "Error is not recoverable": said, the status 2.
    Exit,
    /// The shell's own streams failed, or the reader of standard output went away.
    Shell(cash_core::Error),
}

impl From<io::Error> for Fatal {
    fn from(error: io::Error) -> Self {
        Self::Shell(error.into())
    }
}

impl From<cash_core::Error> for Fatal {
    fn from(error: cash_core::Error) -> Self {
        Self::Shell(error)
    }
}

/// A name the command line selects members by, and what it found.
#[derive(Debug)]
struct Wanted {
    pattern: Vec<u8>,
    /// The `-C` in force for it.
    base: PathBuf,
    found: u64,
}

/// One run of the command.
struct Tar<'a, SE: cash_core::ShellExtensions> {
    options: Options,
    context: &'a cash_core::ExecutionContext<'a, SE>,
    /// 0, 1 for differences or a file changed while read, 2 for errors.
    status: u8,
    zone: Zone,
    utf8: bool,
    transforms: transform::Transforms,
    /// Listings go to standard error when the archive or the data goes to standard
    /// output.
    listing_to_stderr: bool,
    /// The prefixes "Removing leading" was said of, for member names and link targets.
    removed: [HashSet<String>; 2],
    /// GNU's width of the owner, group and size columns, which only grows.
    ugs_width: usize,
    /// Bytes of the archive read or written, for `--totals`.
    bytes: u64,
    /// The folder `--one-top-level` puts every member under.
    top_level: Option<Vec<u8>>,
}

impl<SE: cash_core::ShellExtensions> Tar<'_, SE> {
    /// A message on standard error, `tar: ` before it.
    fn say(&self, text: &str) -> Result<(), Fatal> {
        writeln!(self.context.stderr(), "tar: {text}")?;
        Ok(())
    }

    /// An error: said, the status 2 at the end.
    fn error(&mut self, text: &str) -> Result<(), Fatal> {
        self.say(text)?;
        self.status = 2;
        Ok(())
    }

    /// An error GNU tar stops at.
    fn fatal(&mut self, text: &str) -> Fatal {
        let said = self
            .say(text)
            .and_then(|()| self.say("Error is not recoverable: exiting now"));
        self.status = 2;
        match said {
            Ok(()) => Fatal::Exit,
            Err(stop) => stop,
        }
    }

    /// A line of a listing, where listings go.
    fn list_line(&self, line: &str) -> Result<(), Fatal> {
        if self.listing_to_stderr {
            writeln!(self.context.stderr(), "{line}")?;
        } else {
            let mut stdout = self.context.stdout();
            writeln!(stdout, "{line}")?;
            stdout.flush()?;
        }
        Ok(())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.context.shell.absolute_path(name)
    }

    /// A name quoted as `--quoting-style` says.
    fn quote(&self, name: &[u8]) -> String {
        listing::quote(
            name,
            self.options.quoting,
            self.utf8,
            &self.options.quote_chars,
        )
    }

    /// A name in a message as GNU's `quotearg_colon` gives it: a `:` escaped as well.
    fn quote_colon(&self, name: &[u8]) -> String {
        let mut extra = self.options.quote_chars.clone();
        extra.push(b':');
        listing::quote(name, self.options.quoting, self.utf8, &extra)
    }

    /// The archive's name: `-f`, `TAPE`, or standard input and output.
    fn archive_name(&self) -> String {
        self.options.archive.clone().unwrap_or_else(|| {
            self.context
                .shell
                .env()
                .get("TAPE")
                .filter(|(_, var)| var.is_exported())
                .map_or_else(
                    || "-".to_owned(),
                    |(_, var)| var.value().to_cow_str(self.context.shell).into_owned(),
                )
        })
    }

    /// GNU's `safer_name_suffix`: a name without its leading `/`, and without whatever
    /// leads up to a `..`, said once for each such prefix; `.` for nothing left.
    fn safer_name(&mut self, name: &[u8], link_target: bool) -> Result<Vec<u8>, Fatal> {
        if self.options.absolute_names {
            return Ok(name.to_vec());
        }
        let mut prefix_len = if name.len() >= 2
            && name.get(1) == Some(&b':')
            && name.first().is_some_and(u8::is_ascii_alphabetic)
        {
            2
        } else {
            0
        };
        let mut p = prefix_len;
        while p < name.len() {
            if name.get(p) == Some(&b'.')
                && name.get(p + 1) == Some(&b'.')
                && matches!(name.get(p + 2), Some(b'/' | b'\\') | None)
            {
                prefix_len = p + 2;
            }
            while p < name.len() {
                let c = name.get(p).copied().unwrap_or(0);
                p += 1;
                if c == b'/' || c == b'\\' {
                    break;
                }
            }
        }
        while matches!(name.get(prefix_len), Some(b'/' | b'\\')) {
            prefix_len += 1;
        }
        if prefix_len > 0 {
            let prefix =
                String::from_utf8_lossy(name.get(..prefix_len).unwrap_or_default()).into_owned();
            let index = usize::from(link_target);
            if self
                .removed
                .get_mut(index)
                .is_some_and(|set| set.insert(prefix.clone()))
            {
                let what = if link_target {
                    "hard link targets"
                } else {
                    "member names"
                };
                self.say(&format!("Removing leading `{prefix}' from {what}"))?;
            }
        }
        let rest = name.get(prefix_len..).unwrap_or_default();
        if rest.is_empty() {
            if prefix_len == 0 {
                let what = if link_target {
                    "Substituting `.' for empty hard link target"
                } else {
                    "Substituting `.' for empty member name"
                };
                self.say(what)?;
            }
            return Ok(b".".to_vec());
        }
        Ok(rest.to_vec())
    }

    /// The long line of `-tv`: GNU's `print_header`.
    fn long_line(&mut self, member: &Member, shown: &[u8]) -> String {
        let kind = member.kind();
        let user = if member.uname.is_empty() || self.options.numeric_owner {
            member.uid.to_string()
        } else {
            String::from_utf8_lossy(&member.uname).into_owned()
        };
        let group = if member.gname.is_empty() || self.options.numeric_owner {
            member.gid.to_string()
        } else {
            String::from_utf8_lossy(&member.gname).into_owned()
        };
        let size = match kind {
            Kind::Char | Kind::Block => format!("{},{}", member.device.0, member.device.1),
            _ => member.size.to_string(),
        };
        let pad = user.len() + 1 + group.len() + 1 + size.len();
        if pad > self.ugs_width {
            self.ugs_width = pad;
        }
        let width = self.ugs_width - pad + size.len();
        let format = if self.options.full_time {
            "%Y-%m-%d %H:%M:%S"
        } else {
            "%Y-%m-%d %H:%M"
        };
        let time = member.mtime.system_time().map_or_else(
            || member.mtime.seconds.to_string(),
            |when| {
                if self.options.utc {
                    Zone::from_tz(Some("UTC0")).format_system_time(when, format)
                } else {
                    self.zone.format_system_time(when, format)
                }
            },
        );
        let mut line = format!(
            "{} {user}/{group} {size:>width$} {time} {}",
            mode_string(kind, member.mode),
            self.quote(shown)
        );
        match kind {
            Kind::Symlink => {
                line.push_str(" -> ");
                line.push_str(&self.quote(&member.link));
            }
            Kind::HardLink => {
                line.push_str(" link to ");
                line.push_str(&self.quote(&member.link));
            }
            Kind::Other(b'V') => line.push_str("--Volume Header--"),
            _ => {}
        }
        line
    }

    /// What stops the run before it starts: options cash's tar does not do, and the
    /// folder of `--one-top-level`, given or from the archive's name.
    fn refusals(&mut self) -> Result<(), Fatal> {
        if let Some(name) = self.options.unsupported.clone() {
            writeln!(
                self.context.stderr(),
                "tar: {name}: not supported by cash's tar\n{TRY}"
            )?;
            return Err(Fatal::Exit);
        }
        if self.options.interactive {
            writeln!(
                self.context.stderr(),
                "tar: --interactive: not supported by cash's tar\n{TRY}"
            )?;
            return Err(Fatal::Exit);
        }
        let dir = match &self.options.one_top_level {
            TopLevel::Off => return Ok(()),
            _ if self.options.absolute_names => {
                writeln!(
                    self.context.stderr(),
                    "tar: '--one-top-level' cannot be used with '--absolute-names'\n{TRY}"
                )?;
                return Err(Fatal::Exit);
            }
            TopLevel::Named(dir) => Some(dir.clone()),
            TopLevel::FromArchive => options::top_level_of(&self.archive_name()),
        };
        let Some(dir) = dir else {
            writeln!(
                self.context.stderr(),
                "tar: Cannot deduce top-level directory name; please set it explicitly with --one-top-level=DIR\n{TRY}"
            )?;
            return Err(Fatal::Exit);
        };
        self.top_level = Some(dir.into_bytes());
        Ok(())
    }

    /// How member names are matched: GNU's defaults, literal and anchored, a folder
    /// naming what is under it.
    fn member_flags(matching: Matching) -> Flags {
        Flags {
            wildcards: matching.wildcards.unwrap_or(false),
            pathname: !matching.wildcards_match_slash.unwrap_or(true),
            noescape: false,
            leading_dir: true,
            casefold: matching.ignore_case,
            anchored: matching.anchored.unwrap_or(true),
        }
    }

    /// How exclusions are matched: GNU's defaults, patterns tried after each `/`.
    fn exclude_flags(matching: Matching, recursion: bool) -> Flags {
        Flags {
            wildcards: matching.wildcards.unwrap_or(true),
            pathname: !matching.wildcards_match_slash.unwrap_or(true),
            noescape: false,
            leading_dir: recursion,
            casefold: matching.ignore_case,
            anchored: matching.anchored.unwrap_or(false),
        }
    }

    /// Whether `name` is excluded by `--exclude`, `-X`, `--exclude-vcs` or
    /// `--exclude-backups`.
    fn excluded(&self, name: &[u8], patterns: &[(Vec<u8>, Matching)]) -> bool {
        const VCS: [&str; 21] = [
            "CVS",
            "RCS",
            "SCCS",
            ".git",
            ".gitignore",
            ".gitattributes",
            ".gitmodules",
            ".cvsignore",
            ".svn",
            ".arch-ids",
            "{arch}",
            "=RELEASE-ID",
            "=meta-update",
            "=update",
            ".bzr",
            ".bzrignore",
            ".bzrtags",
            ".hg",
            ".hgignore",
            ".hgtags",
            "_darcs",
        ];
        const BACKUPS: [&str; 7] = [".#*", "*~", "#*#", "*.orig", "*.rej", "*.bak", "*.BAK"];
        let recursion = self.options.recursion;
        if patterns.iter().any(|(pattern, matching)| {
            select::matches(pattern, name, Self::exclude_flags(*matching, recursion))
        }) {
            return true;
        }
        let base_flags = Flags {
            wildcards: true,
            anchored: false,
            leading_dir: recursion,
            ..Flags::default()
        };
        (self.options.exclude_vcs
            && VCS
                .iter()
                .any(|p| select::matches(p.as_bytes(), name, base_flags)))
            || (self.options.exclude_backups
                && BACKUPS
                    .iter()
                    .any(|p| select::matches(p.as_bytes(), name, base_flags)))
    }

    /// The exclusion patterns: `--exclude`, then the lines of each `-X` file.
    fn exclude_patterns(&mut self) -> Result<Vec<(Vec<u8>, Matching)>, Fatal> {
        let mut patterns: Vec<(Vec<u8>, Matching)> = self
            .options
            .excludes
            .iter()
            .map(|(p, m)| (p.as_bytes().to_vec(), *m))
            .collect();
        for (file, matching) in self.options.exclude_from.clone() {
            let base = self.path(".");
            match self.read_named(&base, &file) {
                Ok(bytes) => patterns.extend(
                    bytes
                        .split(|b| *b == b'\n' || (self.options.null && *b == 0))
                        .filter(|line| !line.is_empty())
                        .map(|line| (line.strip_suffix(b"\r").unwrap_or(line).to_vec(), matching)),
                ),
                Err(e) => return Err(self.fatal(&format!("{file}: Cannot open: {}", strerror(&e)))),
            }
        }
        Ok(patterns)
    }

    /// The names of the command line and its `-T` files, with the `-C` in force for
    /// each.
    fn names(&mut self) -> Result<Vec<(String, PathBuf)>, Fatal> {
        let mut base = self.path(".");
        let mut out = Vec::new();
        for name in self.options.names.clone() {
            match name {
                Name::Chdir(dir) => base = base.join(&dir),
                Name::Path(path) => out.push((path, base.clone())),
                Name::FilesFrom(file) => {
                    let bytes = match self.read_named(&base, &file) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            return Err(
                                self.fatal(&format!("{file}: Cannot open: {}", strerror(&e)))
                            );
                        }
                    };
                    let separator = if self.options.null { 0 } else { b'\n' };
                    let mut lines = bytes.split(|b| *b == separator).peekable();
                    while let Some(line) = lines.next() {
                        if lines.peek().is_none() && line.is_empty() {
                            break;
                        }
                        let line = if self.options.null {
                            line
                        } else {
                            line.strip_suffix(b"\r").unwrap_or(line)
                        };
                        let text = String::from_utf8_lossy(line).into_owned();
                        if let Some(dir) = text.strip_prefix("-C").filter(|_| !self.options.null) {
                            base = base.join(dir.trim_start());
                        } else if !text.is_empty() {
                            out.push((text, base.clone()));
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// The bytes of a file a name list or a pattern list is read from: `-` and
    /// `/dev/stdin` are standard input, `/dev/null` is nothing.
    fn read_named(&self, base: &Path, name: &str) -> io::Result<Vec<u8>> {
        match name {
            "-" | "/dev/stdin" => {
                let mut bytes = Vec::new();
                self.context.stdin().read_to_end(&mut bytes)?;
                Ok(bytes)
            }
            "/dev/null" => Ok(Vec::new()),
            _ => fs::read(base.join(name)),
        }
    }

    /// The folder `-C` leaves in force at the end: where members are extracted when no
    /// name selects them.
    fn last_base(&self) -> PathBuf {
        let mut base = self.path(".");
        for name in &self.options.names {
            if let Name::Chdir(dir) = name {
                base = base.join(dir);
            }
        }
        base
    }

    /// The archive read: a file, or standard input, decompressed as its first bytes or
    /// the options say.
    fn open_read(&mut self) -> Result<Box<dyn Read>, Fatal> {
        let name = self.archive_name();
        let input: Box<dyn Read> = if name == "-" {
            if is_terminal(self.context, OpenFiles::STDIN_FD) {
                return Err(self.fatal(
                    "Refusing to read archive contents from terminal (missing -f option?)",
                ));
            }
            Box::new(self.context.stdin())
        } else if name == "/dev/null" {
            Box::new(io::empty())
        } else {
            match fs::File::open(self.path(&name)) {
                Ok(file) => Box::new(file),
                Err(e) => return Err(self.fatal(&format!("{name}: Cannot open: {}", strerror(&e)))),
            }
        };
        self.decompress(input)
    }

    /// `input` decompressed: by `-z` and its kin, or by its first bytes.
    fn decompress(&mut self, input: Box<dyn Read>) -> Result<Box<dyn Read>, Fatal> {
        if let Some(program) = self.options.refused_compressor.clone() {
            return Err(self.fatal(&format!("{program}: compressor not carried by cash's tar")));
        }
        let mut input = Lookahead::new(input);
        let first = input.peek(512)?.to_vec();
        let found = Codec::sniff(&first);
        match self.options.codec {
            Some(asked) if found != Some(asked) && !first.is_empty() => {
                let (program, words) = match asked {
                    Codec::Gzip => ("gzip", "stdin: not in gzip format"),
                    Codec::Bzip2 => ("bzip2", "(stdin) is not a bzip2 file."),
                    Codec::Xz | Codec::Lzma => ("xz", "(stdin): File format not recognized"),
                    Codec::Lzip => (
                        "lzip",
                        "(stdin): Bad magic number (file not in lzip format).",
                    ),
                    Codec::Zstd => ("zstd", "/*stdin*\\: unsupported format"),
                };
                writeln!(self.context.stderr(), "{program}: {words}")?;
                self.say("Child returned status 1")?;
                Err(self.fatal_quiet())
            }
            Some(asked) => Ok(codec::reader(asked, input)),
            None => match found {
                Some(found) => Ok(codec::reader(found, input)),
                None => Ok(Box::new(input)),
            },
        }
    }

    /// "Error is not recoverable" alone.
    fn fatal_quiet(&mut self) -> Fatal {
        self.status = 2;
        match self.say("Error is not recoverable: exiting now") {
            Ok(()) => Fatal::Exit,
            Err(stop) => stop,
        }
    }

    /// `--totals`: bytes read or written, in whole records, with GNU's rate.
    fn totals(&self, written: bool) -> Result<(), Fatal> {
        if !self.options.totals {
            return Ok(());
        }
        let record = 512 * self.options.blocking;
        let bytes = self.bytes.div_ceil(record) * record;
        let what = if written { "written" } else { "read" };
        self.say_plain(&format!(
            "Total bytes {what}: {bytes} ({}, {}/s)",
            human(bytes),
            human(bytes)
        ))
    }

    /// A line on standard error without `tar: `.
    fn say_plain(&self, text: &str) -> Result<(), Fatal> {
        writeln!(self.context.stderr(), "{text}")?;
        Ok(())
    }
}

/// A size as GNU's `human_readable` writes it for `--totals`: `10KiB`, `1.5MiB`.
#[expect(clippy::cast_precision_loss, reason = "a size scaled for people")]
fn human(bytes: u64) -> String {
    let units = ["", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let name = units.get(unit).copied().unwrap_or("TiB");
    if unit == 0 {
        format!("{bytes}B")
    } else if value < 10.0 {
        format!("{value:.1}{name}")
    } else {
        format!("{value:.0}{name}")
    }
}

/// Whether the locale's charset is UTF-8, as `LC_ALL`, `LC_CTYPE` and `LANG` say.
fn utf8_locale<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> bool {
    for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Some(value) = context
            .shell
            .env()
            .get(name)
            .map(|(_, var)| var.value().to_cow_str(context.shell).into_owned())
            .filter(|v| !v.is_empty())
        {
            let upper = value.to_ascii_uppercase();
            return upper.contains("UTF-8") || upper.contains("UTF8");
        }
    }
    true
}

/// A member's time for `std`.
fn system_time(time: Timestamp) -> Option<std::time::SystemTime> {
    time.system_time()
}

/// Whether `path` names a folder, without opening a pipe.
fn is_dir(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_dir())
}

fn run<SE: cash_core::ShellExtensions>(
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    let environment = context
        .shell
        .env()
        .get("TAR_OPTIONS")
        .filter(|(_, var)| var.is_exported())
        .map(|(_, var)| var.value().to_cow_str(context.shell).into_owned());
    let mut options = match options::parse(environment.as_deref(), args) {
        Ok(options) => options,
        Err(Stop::Say(text)) => {
            context.stdout().write_all(text.as_bytes())?;
            return Ok(ExecutionResult::success());
        }
        Err(Stop::Usage(message, status)) => {
            let mut stderr = context.stderr();
            for line in message.lines() {
                if line.starts_with("Try ")
                    || line.starts_with("Valid arguments")
                    || line.starts_with("  ")
                {
                    writeln!(stderr, "{line}")?;
                } else {
                    writeln!(stderr, "tar: {line}")?;
                }
            }
            if !message.contains(TRY) {
                writeln!(stderr, "{TRY}")?;
            }
            return Ok(ExecutionResult::new(status));
        }
    };
    let Some(op) = options.op else {
        writeln!(
            context.stderr(),
            "tar: You must specify one of the '-Acdtrux', '--delete' or '--test-label' options\n{TRY}"
        )?;
        return Ok(ExecutionResult::new(2));
    };
    if options.no_same_owner_letter_o && op == Op::Create {
        options.format = Some(cash_archive::tar::Format::V7);
    }
    let mut transforms = transform::Transforms::default();
    for expression in options.transforms.clone() {
        if let Err(message) = transforms.add(&expression) {
            writeln!(context.stderr(), "tar: {message}\n{TRY}")?;
            return Ok(ExecutionResult::new(2));
        }
    }
    let to_stdout_archive = matches!(op, Op::Create | Op::Delete)
        && options.archive.as_deref().unwrap_or("-") == "-"
        && options.archive.is_some();
    let mut tar = Tar {
        listing_to_stderr: options.to_stdout || to_stdout_archive,
        options,
        context,
        status: 0,
        zone: Zone::of_shell(context.shell),
        utf8: utf8_locale(context),
        transforms,
        removed: [HashSet::new(), HashSet::new()],
        ugs_width: 19,
        bytes: 0,
        top_level: None,
    };
    let outcome = tar.refusals().and_then(|()| match op {
        Op::Create => tar.create(),
        Op::Append | Op::Update => tar.append(op == Op::Update),
        Op::Catenate => tar.catenate(),
        Op::Delete => tar.delete(),
        Op::List | Op::Extract | Op::Diff | Op::TestLabel => tar.read_archive(op),
    });
    match outcome {
        Ok(()) => {
            if tar.status == 2 {
                tar.say("Exiting with failure status due to previous errors")
                    .map_err(|stop| match stop {
                        Fatal::Shell(e) => e,
                        Fatal::Exit => cash_core::Error::from(io::Error::other("tar")),
                    })?;
            }
            Ok(ExecutionResult::new(tar.status))
        }
        Err(Fatal::Exit) => Ok(ExecutionResult::new(2)),
        Err(Fatal::Shell(error)) => Err(error),
    }
}
