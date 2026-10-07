//! `zipinfo` and `unzip -Z`: `ZipInfo` 3.00's listings, from the names alone to every
//! field of every record.

use std::fmt::Write as _;

use cash_archive::listing::mode_string;
use cash_archive::member::Kind;
use cash_archive::zip::read::{self, Archive, OpenError};
use cash_archive::zip::{
    Entry, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, extra_id, fields, flag, method, owner, times,
    unix_like,
};
use cash_core::ExecutionResult;

use super::help::ZIPINFO_USAGE;
use super::unzip::NO_END;
use super::{Clock, Say, find_archive, matches, name_text, shown_compressed, status};

/// The listing's format.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Format {
    /// `-1`: names only.
    One,
    /// `-2`: names, with `-h`, `-t` and `-z` allowed.
    Two,
    /// `-s`, the default.
    #[default]
    Short,
    /// `-m`.
    Medium,
    /// `-l`.
    Long,
    /// `-v`.
    Verbose,
}

#[derive(Debug, Default)]
struct Options {
    format: Option<Format>,
    header: bool,
    totals: bool,
    comment: bool,
    sortable: bool,
    casefold: bool,
    zipfile: Option<String>,
    names: Vec<String>,
    excludes: Vec<String>,
}

fn parse(words: &[String]) -> Result<Options, u8> {
    let mut options = Options::default();
    let mut i = 0;
    while let Some(word) = words.get(i) {
        let Some(letters) = word.strip_prefix('-') else {
            break;
        };
        if letters.is_empty() {
            return Err(status::PARAM);
        }
        let mut negative = false;
        for c in letters.chars() {
            let on = !negative;
            match c {
                '-' => negative = !negative,
                '1' => options.format = Some(Format::One),
                '2' => options.format = Some(Format::Two),
                's' => options.format = Some(Format::Short),
                'm' => options.format = Some(Format::Medium),
                'l' => options.format = Some(Format::Long),
                'v' => options.format = Some(Format::Verbose),
                'h' => options.header = on,
                't' => options.totals = on,
                'z' => options.comment = on,
                'T' => options.sortable = on,
                'C' => options.casefold = on,
                'M' | 'U' | 'W' | 'x' => {}
                _ => return Err(status::PARAM),
            }
        }
        i += 1;
    }
    options.zipfile = words.get(i).cloned();
    let mut excluding = false;
    for word in words.iter().skip(i + 1) {
        if word == "-x" {
            excluding = true;
        } else if excluding {
            options.excludes.push(word.clone());
        } else {
            options.names.push(word.clone());
        }
    }
    Ok(options)
}

/// `zipinfo`.
pub(super) fn run<SE: cash_core::ShellExtensions>(
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    run_as(args, context, "zipinfo")
}

/// `zipinfo`, or `unzip -Z`: `program` is the name its errors carry.
pub(super) fn run_as<SE: cash_core::ShellExtensions>(
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
    program: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    let say = Say { context };
    let mut words: Vec<String> = context
        .shell
        .env()
        .get("ZIPINFO")
        .filter(|(_, var)| var.is_exported())
        .map(|(_, var)| {
            var.value()
                .to_cow_str(context.shell)
                .split_whitespace()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    words.extend(args.iter().cloned());
    let options = match parse(&words) {
        Ok(options) => options,
        Err(code) => {
            say.err(ZIPINFO_USAGE)?;
            return Ok(ExecutionResult::new(code));
        }
    };
    let Some(zipfile) = options.zipfile.clone() else {
        say.out(ZIPINFO_USAGE)?;
        return Ok(ExecutionResult::success());
    };
    let Some((shown, path)) = find_archive(context, &zipfile) else {
        say.err(&format!(
            "{program}:  cannot find or open {zipfile}, {zipfile}.zip or {zipfile}.ZIP.\n"
        ))?;
        return Ok(ExecutionResult::new(status::NOZIP));
    };
    let mut opened = super::open_archive(&path)?;
    let archive = match read::open(&mut opened.file) {
        Ok(archive) => archive,
        Err(OpenError::NoEnd) => {
            say.out(&format!("Archive:  {shown}\n"))?;
            say.err(&format!("[{shown}]\n"))?;
            say.err(NO_END)?;
            let indent = " ".repeat(program.len() + 3);
            say.err(&format!(
                "{program}:  cannot find zipfile directory in one of {zipfile} or\n{indent}{zipfile}.zip, and cannot find {zipfile}.ZIP, period.\n"
            ))?;
            return Ok(ExecutionResult::new(status::NOZIP));
        }
        Err(OpenError::BadCentral(number)) => {
            say.err(&format!(
                "error [{shown}]:  expected central file header signature not found (file #{number}).\n  (please check that you have transferred or created the zipfile in the\n  appropriate BINARY mode and that you have compiled UnZip properly)\n"
            ))?;
            return Ok(ExecutionResult::new(status::BADERR));
        }
        Err(OpenError::Io(e)) => return Err(e.into()),
    };
    let info = Info {
        say,
        clock: Clock::of(context),
        options,
        shown,
    };
    info.list(&archive)
}

struct Info<'a, SE: cash_core::ShellExtensions> {
    say: Say<'a, SE>,
    clock: Clock,
    options: Options,
    shown: String,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `UnZip`'s `ratio`: the saving from `size` to `compressed` in tenths of a percent,
/// rounded.
pub(super) fn ratio(size: u64, compressed: u64) -> i64 {
    if size == 0 {
        return 0;
    }
    let (uc, c) = (i128::from(size), i128::from(compressed));
    let value = if uc > 2_000_000 {
        let denom = uc / 1000;
        if uc >= c {
            (uc - c + (denom >> 1)) / denom
        } else {
            -((c - uc + (denom >> 1)) / denom)
        }
    } else if uc >= c {
        (1000 * (uc - c) + (uc >> 1)) / uc
    } else {
        -((1000 * (c - uc) + (uc >> 1)) / uc)
    };
    i64::try_from(value).unwrap_or(0)
}

/// `ZipInfo`'s three letters for a host.
fn host_short(host: u8) -> &'static str {
    const NAMES: [&str; 20] = [
        "fat", "ami", "vms", "unx", "vm/", "atr", "hpf", "mac", "zzz", "cpm", "t20", "ntf", "qds",
        "aco", "vft", "mvs", "be ", "tan", "the", "osx",
    ];
    NAMES.get(usize::from(host)).copied().unwrap_or("???")
}

/// `ZipInfo`'s words for a host.
fn host_long(host: u8) -> String {
    const NAMES: [&str; 20] = [
        "MS-DOS, OS/2 or NT FAT",
        "Amiga",
        "VMS",
        "Unix",
        "VM/CMS",
        "Atari ST",
        "OS/2 or NT HPFS",
        "Macintosh HFS",
        "Z-System",
        "CP/M",
        "TOPS-20",
        "NTFS",
        "SMS/QDOS",
        "Acorn RISC OS",
        "Win32 VFAT",
        "MVS",
        "BeOS",
        "Tandem NSK",
        "Theos",
        "Mac OS/X (Darwin)",
    ];
    NAMES
        .get(usize::from(host))
        .map_or_else(|| format!("unknown (#{host})"), |n| (*n).to_owned())
}

/// `ZipInfo`'s four letters for a method.
fn method_short(entry: &Entry) -> String {
    let kind = match (entry.flags >> 1) & 3 {
        0 => 'N',
        1 => 'X',
        2 => 'F',
        _ => 'S',
    };
    match entry.method {
        method::STORED => "stor".to_owned(),
        method::SHRUNK => "shrk".to_owned(),
        2..=5 => format!("re:{}", entry.method - 1),
        method::IMPLODED => format!(
            "i{}:{}",
            if entry.flags & 2 != 0 { 8 } else { 4 },
            if entry.flags & 4 != 0 { 3 } else { 2 }
        ),
        7 => "tokn".to_owned(),
        method::DEFLATED => format!("def{kind}"),
        method::DEFLATE64 => format!("d64{kind}"),
        10 => "dcli".to_owned(),
        method::BZIP2 => "bzp2".to_owned(),
        method::LZMA => "lzma".to_owned(),
        18 => "ters".to_owned(),
        19 => "lz77".to_owned(),
        97 => "wavp".to_owned(),
        method::PPMD => "ppmd".to_owned(),
        other => format!("u{other:03}"),
    }
}

/// `ZipInfo`'s words for a method in `-v`.
fn method_long(method_number: u16) -> String {
    match method_number {
        method::STORED => "none (stored)".to_owned(),
        method::SHRUNK => "shrunk".to_owned(),
        2..=5 => format!("reduced (factor {})", method_number - 1),
        method::IMPLODED => "imploded".to_owned(),
        7 => "tokenized".to_owned(),
        method::DEFLATED => "deflated".to_owned(),
        method::DEFLATE64 => "deflated (enhanced-64k)".to_owned(),
        10 => "imploded (PK DCL)".to_owned(),
        method::BZIP2 => "bzipped".to_owned(),
        method::LZMA => "LZMA-ed".to_owned(),
        18 => "Terse-compressed".to_owned(),
        19 => "IBM LZ77-compressed".to_owned(),
        97 => "WavPacked".to_owned(),
        method::PPMD => "PPMd-ed".to_owned(),
        other => format!("unknown ({other})"),
    }
}

/// The attributes column: the Unix mode as `ls -l` shows it, or MS-DOS's letters.
fn attributes(entry: &Entry, name: &str) -> String {
    if let Some(mode) = entry.unix_mode() {
        let kind = match mode & S_IFMT {
            S_IFDIR => Kind::Dir,
            S_IFLNK => Kind::Symlink,
            S_IFIFO => Kind::Fifo,
            0o020_000 => Kind::Char,
            0o060_000 => Kind::Block,
            _ => Kind::File,
        };
        return mode_string(kind, mode & 0o7777);
    }
    let attrs = entry.dos_attributes();
    let lower = name.to_ascii_lowercase();
    let executable = [".com", ".exe", ".btm", ".cmd", ".bat"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    let mut out = String::with_capacity(7);
    out.push(if attrs & 0x10 != 0 { 'd' } else { '-' });
    out.push('r');
    out.push(if attrs & 0x01 != 0 { '-' } else { 'w' });
    out.push(if attrs & 0x10 != 0 || executable {
        'x'
    } else {
        '-'
    });
    out.push(if attrs & 0x20 != 0 { 'a' } else { '-' });
    out.push(if attrs & 0x02 != 0 { 'h' } else { '-' });
    out.push(if attrs & 0x04 != 0 { 's' } else { '-' });
    out
}

impl<SE: cash_core::ShellExtensions> Info<'_, SE> {
    #[expect(
        clippy::too_many_lines,
        reason = "zipinfo's zi_short and friends, one loop for every format"
    )]
    fn list(&self, archive: &Archive) -> Result<ExecutionResult, cash_core::Error> {
        let mut code = 0;
        let explicit = self.options.format;
        let format = explicit.unwrap_or_default();
        let ht = self.options.header || self.options.totals;
        let names_given = !self.options.names.is_empty() || !self.options.excludes.is_empty();
        let defaults = !ht && !names_given && !matches!(explicit, Some(Format::One | Format::Two));
        let header = format != Format::One && (self.options.header || defaults);
        let totals = format != Format::One && (self.options.totals || defaults);
        let listing = format == Format::One || explicit.is_some() || !ht || names_given;
        if format == Format::Verbose {
            return self.verbose(archive);
        }
        if header {
            self.say.out(&format!("Archive:  {}\n", self.shown))?;
            if self.options.comment && !archive.end.comment.is_empty() {
                let mut text = String::from_utf8_lossy(&archive.end.comment).replace("\r\n", "\n");
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                self.say.out(&text)?;
            }
            self.say.out(&format!(
                "Zip file size: {} bytes, number of entries: {}\n",
                archive.file_size, archive.end.entries
            ))?;
        }
        if archive.extra_bytes > 0 {
            self.say.err(&format!(
                "warning [{}]:  {} extra bytes at beginning or within zipfile\n  (attempting to process anyway)\n",
                self.shown, archive.extra_bytes
            ))?;
            code = status::WARN;
        }
        if archive.entries.is_empty() {
            if header {
                self.say.out("Empty zipfile.\n")?;
            }
            return Ok(ExecutionResult::new(status::WARN));
        }
        let mut hits = vec![false; self.options.names.len()];
        let mut count = 0_u64;
        let mut total_size = 0_u64;
        let mut total_compressed = 0_u64;
        let mut out = String::new();
        for entry in &archive.entries {
            let name = name_text(entry);
            if !self.options.names.is_empty() {
                let mut any = false;
                for (hit, pattern) in hits.iter_mut().zip(&self.options.names) {
                    if matches(pattern, &name, self.options.casefold, false) {
                        *hit = true;
                        any = true;
                    }
                }
                if !any {
                    continue;
                }
            }
            if self
                .options
                .excludes
                .iter()
                .any(|p| matches(p, &name, self.options.casefold, false))
            {
                continue;
            }
            count += 1;
            let compressed = shown_compressed(entry);
            total_size += entry.size;
            total_compressed += compressed;
            if !listing {
                continue;
            }
            if matches!(format, Format::One | Format::Two) {
                out.push_str(&name);
                out.push('\n');
                continue;
            }
            let version = format!(
                "{}.{}",
                (entry.version_made_by & 0xff) / 10,
                (entry.version_made_by & 0xff) % 10
            );
            let text = if entry.is_text() { 't' } else { 'b' };
            let text = if entry.is_encrypted() {
                text.to_ascii_uppercase()
            } else {
                text
            };
            let extra = match (!entry.extra.is_empty(), entry.flags & flag::DESCRIPTOR != 0) {
                (false, false) => '-',
                (false, true) => 'l',
                (true, false) => 'x',
                (true, true) => 'X',
            };
            let middle = match format {
                Format::Medium => format!("{text}{extra}{:>3}%", rounded(entry.size, compressed)),
                Format::Long => format!("{text}{extra} {compressed:>8}"),
                _ => format!("{text}{extra}"),
            };
            let _ = writeln!(
                out,
                "{:<10}  {version} {} {:>8} {middle} {} {} {name}",
                attributes(entry, &name),
                host_short(entry.host()),
                entry.size,
                method_short(entry),
                self.date(entry)
            );
        }
        if totals {
            let r = ratio(total_size, total_compressed);
            let sign = if r < 0 { "-" } else { "" };
            let _ = writeln!(
                out,
                "{count} file{}, {total_size} bytes uncompressed, {total_compressed} bytes compressed:  {sign}{}.{}%",
                if count == 1 { "" } else { "s" },
                r.abs() / 10,
                r.abs() % 10
            );
        }
        self.say.out(&out)?;
        for (hit, name) in hits.iter().zip(&self.options.names) {
            if !hit {
                self.say
                    .err(&format!("caution: filename not matched:  {name}\n"))?;
                code = status::FIND;
            }
        }
        Ok(ExecutionResult::new(code))
    }

    fn date(&self, entry: &Entry) -> String {
        let c = self.clock.civil(self.clock.entry_time(entry));
        if self.options.sortable {
            format!(
                "{:04}{:02}{:02}.{:02}{:02}{:02}",
                c.year, c.month, c.day, c.hour, c.minute, c.second
            )
        } else {
            let month = MONTHS
                .get(usize::try_from(c.month.saturating_sub(1)).unwrap_or(0))
                .copied()
                .unwrap_or("???");
            format!(
                "{:02}-{month}-{:02} {:02}:{:02}",
                c.year.rem_euclid(100),
                c.day,
                c.hour,
                c.minute
            )
        }
    }

    /// `-v`.
    #[expect(
        clippy::too_many_lines,
        reason = "zipinfo's zi_long: every field of the end record and of each entry"
    )]
    fn verbose(&self, archive: &Archive) -> Result<ExecutionResult, cash_core::Error> {
        let mut out = format!("Archive:  {}\n", self.shown);
        let comment = &archive.end.comment;
        if comment.is_empty() {
            out.push_str("There is no zipfile comment.\n");
        } else {
            let _ = writeln!(
                out,
                "The zipfile comment is {} bytes long and contains the following text:",
                comment.len()
            );
            out.push_str(
                "======================== zipfile comment begins ==========================\n",
            );
            let mut text = String::from_utf8_lossy(comment).into_owned();
            if !text.ends_with('\n') {
                text.push('\n');
            }
            out.push_str(&text);
            out.push_str(
                "========================= zipfile comment ends ===========================\n",
            );
        }
        let end = &archive.end;
        let expected = end.central_offset + end.central_size;
        let actual = end.zip64_offset.unwrap_or(end.offset);
        let number = |label: &str, value: u64| {
            format!(
                "  {label}{:>w$} ({value:016X}h)\n",
                value,
                w = 48 - label.len()
            )
        };
        out.push_str("\nEnd-of-central-directory record:\n-------------------------------\n\n");
        out.push_str(&number("Zip archive file size:", archive.file_size));
        out.push_str(&number("Actual end-cent-dir record offset:", actual));
        out.push_str(&number("Expected end-cent-dir record offset:", expected));
        out.push_str(
            "  (based on the length of the central directory and its expected offset)\n\n",
        );
        let _ = writeln!(
            out,
            "  This zipfile constitutes the sole disk of a single-part archive; its\n  central directory contains {} {}.",
            end.entries,
            if end.entries == 1 { "entry" } else { "entries" }
        );
        let _ = write!(
            out,
            "  The central directory is {} ({:016X}h) bytes long,\n  and its (expected) offset in bytes from the beginning of the zipfile\n  is {} ({:016X}h).\n\n",
            end.central_size, end.central_size, end.central_offset, end.central_offset
        );
        if archive.extra_bytes > 0 {
            self.say.err(&format!(
                "warning [{}]:  {} extra bytes at beginning or within zipfile\n  (attempting to process anyway)\n",
                self.shown, archive.extra_bytes
            ))?;
        }
        let mut hits = vec![false; self.options.names.len()];
        for (index, entry) in archive.entries.iter().enumerate() {
            let name = name_text(entry);
            if !self.options.names.is_empty() {
                let mut any = false;
                for (hit, pattern) in hits.iter_mut().zip(&self.options.names) {
                    if matches(pattern, &name, self.options.casefold, false) {
                        *hit = true;
                        any = true;
                    }
                }
                if !any {
                    continue;
                }
            }
            if self
                .options
                .excludes
                .iter()
                .any(|p| matches(p, &name, self.options.casefold, false))
            {
                continue;
            }
            let field = |label: &str, value: &str| {
                format!(
                    "  {label}{}{value}\n",
                    " ".repeat(48usize.saturating_sub(label.len()))
                )
            };
            let heading = format!("Central directory entry #{}:", index + 1);
            let _ = write!(
                out,
                "\n{heading}\n{}\n\n  {name}\n\n",
                "-".repeat(heading.len())
            );
            out.push_str(&field(
                "offset of local header from start of archive:",
                &entry.local_offset.to_string(),
            ));
            let _ = writeln!(
                out,
                "{}({:016X}h) bytes",
                " ".repeat(50),
                entry.local_offset
            );
            out.push_str(&field(
                "file system or operating system of origin:",
                &host_long(entry.host()),
            ));
            out.push_str(&field(
                "version of encoding software:",
                &format!(
                    "{}.{}",
                    (entry.version_made_by & 0xff) / 10,
                    (entry.version_made_by & 0xff) % 10
                ),
            ));
            out.push_str(&field(
                "minimum file system compatibility required:",
                &host_long(entry.version_needed.to_be_bytes()[0]),
            ));
            out.push_str(&field(
                "minimum software version required to extract:",
                &format!(
                    "{}.{}",
                    (entry.version_needed & 0xff) / 10,
                    (entry.version_needed & 0xff) % 10
                ),
            ));
            out.push_str(&field("compression method:", &method_long(entry.method)));
            if matches!(entry.method, method::DEFLATED | method::DEFLATE64) {
                let kind = match (entry.flags >> 1) & 3 {
                    0 => "normal",
                    1 => "maximum",
                    2 => "fast",
                    _ => "superfast",
                };
                out.push_str(&field("compression sub-type (deflation):", kind));
            }
            out.push_str(&field(
                "file security status:",
                if entry.is_encrypted() {
                    "encrypted"
                } else {
                    "not encrypted"
                },
            ));
            out.push_str(&field(
                "extended local header:",
                if entry.has_descriptor() { "yes" } else { "no" },
            ));
            let dos = entry.time.civil();
            let month = |m: u32| {
                MONTHS
                    .get(usize::try_from(m.saturating_sub(1)).unwrap_or(0))
                    .copied()
                    .unwrap_or("???")
            };
            out.push_str(&field(
                "file last modified on (DOS date/time):",
                &format!(
                    "{} {} {} {:02}:{:02}:{:02}",
                    dos.year,
                    month(dos.month),
                    dos.day,
                    dos.hour,
                    dos.minute,
                    dos.second
                ),
            ));
            if let Some(modified) = entry.ut_mtime() {
                let local = self.clock.civil(modified);
                let utc = Clock {
                    zone: cash_core::timefmt::Zone::from_tz(Some("UTC0")),
                }
                .civil(modified);
                for (c, word) in [(local, "local"), (utc, "UTC")] {
                    out.push_str(&field(
                        "file last modified on (UT extra field modtime):",
                        &format!(
                            "{} {} {} {:02}:{:02}:{:02} {word}",
                            c.year,
                            month(c.month),
                            c.day,
                            c.hour,
                            c.minute,
                            c.second
                        ),
                    ));
                }
            }
            out.push_str(&field(
                "32-bit CRC value (hex):",
                &format!("{:08x}", entry.crc),
            ));
            out.push_str(&field(
                "compressed size:",
                &format!("{} bytes", entry.compressed_size),
            ));
            out.push_str(&field(
                "uncompressed size:",
                &format!("{} bytes", entry.size),
            ));
            out.push_str(&field(
                "length of filename:",
                &format!("{} characters", entry.name.len()),
            ));
            out.push_str(&field(
                "length of extra field:",
                &format!("{} bytes", entry.extra.len()),
            ));
            out.push_str(&field(
                "length of file comment:",
                &format!("{} characters", entry.comment.len()),
            ));
            out.push_str(&field(
                "disk number on which file begins:",
                &format!("disk {}", entry.disk_start + 1),
            ));
            out.push_str(&field(
                "apparent file type:",
                if entry.is_text() { "text" } else { "binary" },
            ));
            if unix_like(entry.host()) {
                if let Some(mode) = entry.unix_mode() {
                    out.push_str(&field(
                        &format!("Unix file attributes ({mode:06o} octal):"),
                        &attributes(entry, &name),
                    ));
                }
            } else if entry.external_attributes >> 8 != 0 {
                out.push_str(&field(
                    "non-MSDOS external file attributes:",
                    &format!("{:06X} hex", entry.external_attributes >> 8),
                ));
            }
            let attrs = entry.dos_attributes();
            let mut words = String::new();
            for (bit, word) in [
                (1, "rdo "),
                (2, "hid "),
                (4, "sys "),
                (8, "lab "),
                (0x10, "dir "),
                (0x20, "arc "),
            ] {
                if attrs & bit != 0 {
                    words.push_str(word);
                }
            }
            if words.is_empty() {
                words.push_str("none");
            }
            out.push_str(&field(
                &format!("MS-DOS file attributes ({attrs:02X} hex):"),
                &words,
            ));
            if !entry.extra.is_empty() {
                out.push_str("\n  The central-directory extra field contains:");
                for (id, data) in fields(&entry.extra) {
                    out.push_str(&extra_text(id, data));
                }
                out.push('\n');
            }
            if entry.comment.is_empty() {
                out.push_str("\n  There is no file comment.\n");
            } else {
                let _ = writeln!(
                    out,
                    "\n------------------------- file comment begins ----------------------------\n{}\n-------------------------- file comment ends -----------------------------",
                    String::from_utf8_lossy(&entry.comment)
                );
            }
        }
        out.push('\n');
        self.say.out(&out)?;
        let mut code = if archive.extra_bytes > 0 {
            status::WARN
        } else {
            0
        };
        for (hit, name) in hits.iter().zip(&self.options.names) {
            if !hit {
                self.say
                    .err(&format!("caution: filename not matched:  {name}\n"))?;
                code = status::FIND;
            }
        }
        Ok(ExecutionResult::new(code))
    }
}

/// `ZipInfo`'s percentage, rounded: `-m`'s column.
fn rounded(size: u64, compressed: u64) -> i64 {
    let r = ratio(size, compressed);
    if r < 0 {
        -((-r + 5) / 10)
    } else {
        (r + 5) / 10
    }
}

/// One subfield of an extra field, as `-v` describes it.
fn extra_text(id: u16, data: &[u8]) -> String {
    let name = match id {
        extra_id::ZIP64 => "PKWARE 64-bit sizes",
        extra_id::NTFS => "PKWARE Win32",
        extra_id::TIMES => "universal time",
        extra_id::UNIX_OLD => "old Info-ZIP Unix/OS2/NT",
        0x7855 => "Unix UID/GID (16-bit)",
        extra_id::UNIX_OWNER => "Unix UID/GID (any size)",
        extra_id::UNICODE_PATH => "Unicode Path",
        0x6375 => "Unicode Comment",
        extra_id::AES => "AES encryption",
        _ => "unknown",
    };
    let mut out = format!(
        "\n  - A subfield with ID 0x{id:04x} ({name}) and {} data bytes",
        data.len()
    );
    if id == extra_id::TIMES {
        out.push('.');
        if let Some(t) = times(&field_bytes(id, data)) {
            let mut kinds = Vec::new();
            if t.flags & 1 != 0 {
                kinds.push("modification");
            }
            if t.flags & 2 != 0 {
                kinds.push("access");
            }
            if t.flags & 4 != 0 {
                kinds.push("creation");
            }
            if !kinds.is_empty() {
                let _ = write!(
                    out,
                    "\n    The local extra field has UTC/GMT {} time{}.",
                    kinds.join("/"),
                    if kinds.len() == 1 { "" } else { "s" }
                );
            }
        }
        return out;
    }
    if id == extra_id::UNIX_OWNER && owner(&field_bytes(id, data)).is_none() {
        out.push('.');
        return out;
    }
    if data.is_empty() {
        out.push('.');
        return out;
    }
    out.push(':');
    for chunk in data.chunks(16) {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        out.push_str("\n    ");
        out.push_str(&hex.join(" "));
    }
    out.push('.');
    out
}

fn field_bytes(id: u16, data: &[u8]) -> Vec<u8> {
    cash_archive::zip::field(id, data)
}
