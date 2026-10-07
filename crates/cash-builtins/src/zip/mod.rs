//! `zip`, `unzip` and `zipinfo`: Info-ZIP's zip 3.0 and `UnZip` 6.00 interfaces, messages
//! and exit statuses, on cash-archive's zip reader and writer.
//!
//! Checked against Info-ZIP's own in WSL (`crates/cash/tests/oracle/zip_cases.sh` and
//! `unzip_cases.sh`): listings, extraction, tests, messages and statuses, and the
//! archive's bytes where zip's are the same everywhere (`-0 -X` and fixed times).
//!
//! Where cash differs, on purpose:
//! - The version and usage lines are cash's.
//! - Text is told from binary over the whole file, where zip looks at what deflate's
//!   first block holds; the two differ only for a file whose first control byte is far
//!   in.
//! - A name with characters beyond ASCII is stored as UTF-8 and marked so, as Windows'
//!   own tools and 7-Zip read it.
//! - unzip also reads LZMA, xz and zstd members; it refuses shrunk, reduced, imploded
//!   and `PPMd` ones, as it refuses `WinZip`'s AES.
//! - A member's name Windows cannot hold is refused by name; a symbolic link is made where
//!   Windows allows one, and written as a file holding its target where it does not.

mod help;
mod unzip;
mod zipcmd;
mod zipinfo;

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use cash_archive::select::{self, Flags};
use cash_archive::zip::{Civil, Entry, flag, host};
use cash_core::openfiles::OpenFiles;
use cash_core::timefmt::Zone;
use cash_core::{ExecutionResult, builtins};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use clap::Parser;

macro_rules! command {
    ($type:ident, $name:literal, $run:path, $doc:literal) => {
        #[doc = $doc]
        #[derive(Parser)]
        #[clap(disable_help_flag = true, disable_version_flag = true)]
        pub(crate) struct $type {
            /// Options and names, read here as Info-ZIP reads them.
            #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
            args: Vec<String>,
        }

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
                $run(&self.args, &context)
            }
        }
    };
}

command!(
    ZipCommand,
    "zip",
    zipcmd::run,
    "Package and compress files into a zip archive, with Info-ZIP's options."
);
command!(
    UnzipCommand,
    "unzip",
    unzip::run,
    "List, test and extract a zip archive, with Info-ZIP's options."
);
command!(
    ZipinfoCommand,
    "zipinfo",
    zipinfo::run,
    "List a zip archive in detail, with Info-ZIP's options."
);

/// `UnZip`'s exit statuses.
mod status {
    pub(super) const WARN: u8 = 1;
    pub(super) const ERR: u8 = 2;
    pub(super) const BADERR: u8 = 3;
    pub(super) const NOZIP: u8 = 9;
    pub(super) const PARAM: u8 = 10;
    pub(super) const FIND: u8 = 11;
    pub(super) const DISK: u8 = 50;
    pub(super) const UNSUPPORTED: u8 = 81;
    pub(super) const BAD_PASSWORD: u8 = 82;
}

/// Local times, as the shell's `TZ` gives them.
struct Clock {
    zone: Zone,
}

impl Clock {
    fn of<SE: cash_core::ShellExtensions>(context: &cash_core::ExecutionContext<'_, SE>) -> Self {
        Self {
            zone: Zone::of_shell(context.shell),
        }
    }

    /// The wall time of a moment.
    fn civil(&self, seconds: i64) -> Civil {
        let when = DateTime::<Utc>::from_timestamp(seconds, 0).unwrap_or_default();
        let text = self.zone.format(when, "%Y %m %d %H %M %S");
        let mut parts = text.split(' ').map(|p| p.parse::<i64>().unwrap_or(0));
        let mut next = || parts.next().unwrap_or(0);
        Civil {
            year: i32::try_from(next()).unwrap_or(1980),
            month: u32::try_from(next()).unwrap_or(1),
            day: u32::try_from(next()).unwrap_or(1),
            hour: u32::try_from(next()).unwrap_or(0),
            minute: u32::try_from(next()).unwrap_or(0),
            second: u32::try_from(next()).unwrap_or(0),
        }
    }

    /// The moment a wall time names.
    fn seconds(&self, civil: Civil) -> i64 {
        let Some(naive) = NaiveDate::from_ymd_opt(civil.year, civil.month.max(1), civil.day.max(1))
            .and_then(|d| {
                d.and_hms_opt(
                    civil.hour.min(23),
                    civil.minute.min(59),
                    civil.second.min(59),
                )
            })
        else {
            return 0;
        };
        let local = match &self.zone {
            Zone::Local => chrono::Local
                .from_local_datetime(&naive)
                .earliest()
                .map(|t| t.timestamp()),
            Zone::Named(tz) => tz
                .from_local_datetime(&naive)
                .earliest()
                .map(|t| t.timestamp()),
            Zone::Fixed { offset, .. } => offset
                .from_local_datetime(&naive)
                .earliest()
                .map(|t| t.timestamp()),
        };
        local.unwrap_or_else(|| naive.and_utc().timestamp())
    }

    /// A member's time: its `UT` field's, else its MS-DOS time read as local.
    fn entry_time(&self, entry: &Entry) -> i64 {
        entry
            .ut_mtime()
            .unwrap_or_else(|| self.seconds(entry.time.civil()))
    }
}

/// IBM PC code page 437's upper half, for names from MS-DOS and Windows that are not
/// marked UTF-8.
const CP437: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// A member's name as text: UTF-8 when marked so or made on Unix, code page 437 when
/// made on MS-DOS or Windows, where a `\` between its parts is a `/`.
fn name_text(entry: &Entry) -> String {
    let windows = matches!(entry.host(), host::MSDOS | 6 | host::NTFS | 14);
    let text = name_bytes_text(entry, windows);
    if windows {
        text.replace('\\', "/")
    } else {
        text
    }
}

fn name_bytes_text(entry: &Entry, windows: bool) -> String {
    let oem = entry.flags & flag::UTF8 == 0 && windows && !entry.name.is_ascii();
    if oem {
        entry
            .name
            .iter()
            .map(|b| {
                if b.is_ascii() {
                    char::from(*b)
                } else {
                    CP437.get(usize::from(b - 0x80)).copied().unwrap_or('?')
                }
            })
            .collect()
    } else {
        String::from_utf8_lossy(&entry.name).into_owned()
    }
}

/// `text` padded with spaces to `width` characters, as `%-Ns` pads it.
fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        text.to_owned()
    } else {
        format!("{text}{}", " ".repeat(width - len))
    }
}

/// Whether `pattern` names `name`, as `UnZip`'s `match` decides: wildcards, `*` crossing
/// `/` unless `-W` (where `**` still does), case folded with `-C`.
fn matches(pattern: &str, name: &str, casefold: bool, stop_at_slash: bool) -> bool {
    let flags = Flags {
        wildcards: true,
        pathname: stop_at_slash && !pattern.contains("**"),
        noescape: false,
        leading_dir: false,
        casefold,
        anchored: true,
    };
    select::fnmatch(pattern.as_bytes(), name.as_bytes(), flags)
}

/// The archive a name gives: itself, or with `.zip` or `.ZIP` added.
fn find_archive<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    name: &str,
) -> Option<(String, PathBuf)> {
    [
        name.to_owned(),
        format!("{name}.zip"),
        format!("{name}.ZIP"),
    ]
    .into_iter()
    .map(|candidate| {
        let path = context.shell.absolute_path(&candidate);
        (candidate, path)
    })
    .find(|(_, path)| is_pipe(path) || std::fs::metadata(path).is_ok_and(|m| m.is_file()))
}

/// Whether a path is a named pipe: a `<(...)`, which may be opened once and cannot seek.
fn is_pipe(path: &Path) -> bool {
    // The shell renders it with either slash.
    path.to_string_lossy()
        .replace('/', "\\")
        .starts_with(r"\\.\pipe\")
}

/// An archive opened to be read: the file itself, or for a pipe a copy of what it held,
/// since a zip archive is read from its end.
struct Opened {
    file: std::fs::File,
    spool: Option<PathBuf>,
}

impl Drop for Opened {
    fn drop(&mut self) {
        if let Some(spool) = self.spool.take() {
            let _ = std::fs::remove_file(spool);
        }
    }
}

/// Opens an archive, a pipe's copied to a temporary file first.
fn open_archive(path: &Path) -> io::Result<Opened> {
    if !is_pipe(path) {
        return Ok(Opened {
            file: std::fs::File::open(path)?,
            spool: None,
        });
    }
    let mut pipe = std::fs::File::open(path)?;
    let mut attempt = 0_u32;
    let (spool, mut file) = loop {
        let candidate =
            std::env::temp_dir().join(format!("cash-zip-{}-{attempt}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 100 => attempt += 1,
            Err(e) => return Err(e),
        }
    };
    let opened = Opened {
        file: file.try_clone()?,
        spool: Some(spool),
    };
    io::copy(&mut pipe, &mut file)?;
    Ok(opened)
}

/// Whether a path is one of the shell's substitution files, `<(...)` or `>(...)`, which
/// it hands a builtin as a temporary file in a folder of its own: a name zip adds no
/// `.zip` to.
fn is_substitution_file(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n.to_string_lossy().starts_with("cash-psub-"))
}

/// Reads a line of standard input, the console's as a line is typed there, shown or
/// not; `None` at the end of the input.
fn read_line<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    shown: bool,
) -> io::Result<Option<String>> {
    let console = context
        .try_fd(OpenFiles::STDIN_FD)
        .and_then(|file| file.console(true, shown));
    if let Some(mut console) = console {
        return Ok(match console.line()? {
            cash_win32::conin::Line::Typed(text) => Some(text.trim_end_matches('\n').to_owned()),
            cash_win32::conin::Line::EndOfInput | cash_win32::conin::Line::Interrupted => None,
        });
    }
    let mut stdin = context.stdin();
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        if stdin.read(&mut byte)? == 0 {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        bytes.push(byte[0]);
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Where the shell's streams go, written and flushed line by line so that standard
/// output and standard error come out in the order they are written.
struct Say<'a, SE: cash_core::ShellExtensions> {
    context: &'a cash_core::ExecutionContext<'a, SE>,
}

impl<SE: cash_core::ShellExtensions> Say<'_, SE> {
    fn out(&self, text: &str) -> io::Result<()> {
        let mut out = self.context.stdout();
        out.write_all(text.as_bytes())?;
        out.flush()
    }

    fn err(&self, text: &str) -> io::Result<()> {
        let mut err = self.context.stderr();
        err.write_all(text.as_bytes())?;
        err.flush()
    }
}

/// The compressed size a listing shows: the data's, without the encryption header.
const fn shown_compressed(entry: &Entry) -> u64 {
    if entry.is_encrypted() {
        entry.compressed_size.saturating_sub(12)
    } else {
        entry.compressed_size
    }
}

/// The path a member is written to, its parts joined to `base`.
fn join(base: &Path, name: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    for part in name.split('/').filter(|p| !p.is_empty() && *p != ".") {
        path.push(part);
    }
    path
}
