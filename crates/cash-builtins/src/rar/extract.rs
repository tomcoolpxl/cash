//! `t`, `x`, `e` and `p`: an archive set's files tested, written out with their folders
//! or without, or printed, as rar does each, with its words a line a file.

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use cash_archive::rar::codec::rar50::Unpack50Decoder;
use cash_archive::rar::crypto::rar13::Rar13Cipher;
use cash_archive::rar::crypto::rar15::Rar15Cipher;
use cash_archive::rar::crypto::rar20::Rar20Cipher;
use cash_archive::rar::crypto::rar30::Rar30Cipher;
use cash_archive::rar::crypto::rar50::{Rar50Cipher, Rar50Keys};
use cash_archive::rar::rar50::blake2sp;
use cash_core::openfiles::{FileKind, OpenFiles};

use super::cmdline::{Arg, Command, FindSpec, Name, Overwrite, Parsed};
use super::entry::{self, Crypto, Entry, Host, Link, Method, Time, Volume};
use super::find;
use super::list::{self, Comment, Masks};
use super::open::{self, Failure, Found};
use super::{Rar, Stop, code};
use crate::rardata::{self, Cipher, DecodeError, Decoder4, DecryptReader, Sink, VolsReader};

/// What the command does with each file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Test,
    /// `x` with `paths`, `e` without.
    Extract {
        paths: bool,
    },
    Print,
    /// `i`: a string looked for in each file.
    Find,
}

impl Mode {
    /// The words that begin an archive's work.
    const fn archive_verb(self) -> &'static str {
        match self {
            Self::Test | Self::Print | Self::Find => "Testing archive",
            Self::Extract { .. } => "Extracting from",
        }
    }

    /// The twelve columns that begin a file's line.
    const fn file_verb(self) -> &'static str {
        match self {
            Self::Test | Self::Print | Self::Find => "Testing     ",
            Self::Extract { .. } => "Extracting  ",
        }
    }

    /// Whether the work has no lines of its own: `p` writes the files, `i` what it
    /// finds.
    const fn quiet(self) -> bool {
        matches!(self, Self::Print | Self::Find)
    }
}

/// The command's choices, the same for every archive.
struct Job {
    mode: Mode,
    masks: Masks,
    /// The folder to extract into, as typed, `/` its separator, none at its end.
    dest: Option<String>,
    /// The same with its separator at its end if typed so, as a temporary file's line
    /// shows it.
    dest_typed: Option<String>,
    /// `i`'s string and how to look for it.
    find: Option<FindSpec>,
}

pub(super) fn run<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    command: &Command,
    parsed: &Parsed,
) -> Result<(), Stop> {
    let mode = match command {
        Command::Test => Mode::Test,
        Command::Extract => Mode::Extract { paths: false },
        Command::ExtractFull => Mode::Extract { paths: true },
        Command::Find(_) => Mode::Find,
        _ => Mode::Print,
    };
    let mut parsed = parsed.clone();
    let mut dest_typed = None;
    // `path_to_extract\`: the last name, when it ends in a separator.
    if matches!(mode, Mode::Extract { .. })
        && let Some(Name::Plain(last)) = parsed.names.last()
        && last.ends_with(['/', '\\'])
    {
        dest_typed = Some(last.replace('\\', "/"));
        parsed.names.pop();
    }
    if let Some(path) = &rar.switches.output_path
        && !path.is_empty()
    {
        dest_typed = Some(path.replace('\\', "/"));
    }
    let dest = dest_typed
        .as_ref()
        .map(|typed| typed.trim_end_matches('/').to_owned());
    let job = Job {
        mode,
        masks: Masks::new(rar, &parsed)?,
        dest,
        dest_typed,
        find: match command {
            Command::Find(spec) => Some(spec.clone()),
            _ => None,
        },
    };
    let Some(archive) = parsed.archive.as_deref() else {
        return Ok(());
    };
    // An archive not there leaves its line ended.
    let mut ended = false;
    // The archives a wildcard matches have one summary, their errors added up.
    let mut tally = Tally::default();
    let mut worked_through = false;
    for found in open::find(rar, archive) {
        ended = !found.path.is_file();
        // The last line of an archive worked through ends before the next begins.
        if worked_through {
            msg(rar, &job, "\n");
        }
        let sets = tally.sets;
        set(rar, &job, &found, &mut tally)?;
        worked_through = tally.sets > sets;
    }
    summary(rar, &job, &tally);
    if mode == Mode::Find && !rar.switches.no_done {
        rar.console.msg(if ended { "Done\n" } else { "\nDone\n" });
    }
    Ok(())
}

/// `a -t`: the archive just written, tested whole.
pub(super) fn test_written<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    display: &str,
    path: PathBuf,
) -> Result<(), Stop> {
    let job = Job {
        mode: Mode::Test,
        masks: Masks::all(),
        dest: None,
        dest_typed: None,
        find: None,
    };
    let found = Found {
        display: display.to_owned(),
        path,
    };
    let mut tally = Tally::default();
    set(rar, &job, &found, &mut tally)?;
    summary(rar, &job, &tally);
    Ok(())
}

/// What the archives of one name came to, for the one summary rar gives them all.
#[derive(Default)]
struct Tally {
    errors: u32,
    done: u32,
    /// The archives worked through, not stopped as they opened, and files that
    /// are no RAR archive.
    sets: u32,
}

/// The end: "All OK", "Total errors", or "No files to extract".
fn summary<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, job: &Job, tally: &Tally) {
    if tally.sets == 0 {
        return;
    }
    if tally.errors > 0 {
        msg(rar, job, &format!("\nTotal errors: {}\n", tally.errors));
    } else if tally.done == 0 {
        msg(rar, job, "\nNo files to extract\n");
        rar.fail(code::NO_FILES);
    } else if rar.switches.no_done {
        msg(rar, job, "\n");
    } else {
        msg(rar, job, "\nAll OK\n");
    }
}

/// The files a file reference asked for copies that are not asked for themselves, by
/// their place: rar unpacks each first, to a temporary file.
fn reference_sources(entries: &[Entry], wanted: &[bool]) -> std::collections::HashSet<usize> {
    entries
        .iter()
        .zip(wanted)
        .filter_map(|(entry, &wanted)| match (&entry.link, wanted) {
            (Some(Link::Copy(target)), true) => Some(target.replace('\\', "/")),
            _ => None,
        })
        .filter_map(|target| {
            entries
                .iter()
                .position(|entry| entry.name.eq_ignore_ascii_case(&target))
        })
        .filter(|&at| !wanted[at])
        .collect()
}

/// The lines rar says for the recovery records of volumes `from` to before `to`, and
/// how many of them failed.
fn recovery_lines(recovery: &[Option<bool>], from: usize, to: usize) -> (String, u32) {
    let mut text = String::new();
    let mut failed = 0;
    for intact in recovery
        .get(from..to.min(recovery.len()))
        .unwrap_or_default()
    {
        match intact {
            Some(true) => text.push_str("\nTesting the recovery record         OK"),
            Some(false) => {
                text.push_str("\nTesting the recovery record        Failed");
                failed += 1;
            }
            None => {}
        }
    }
    (text, failed)
}

/// Whether an archive is solid, its files one stream.
fn archive_solid(archive: &cash_archive::rar::Archive) -> bool {
    use cash_archive::rar::Archive;
    match archive {
        Archive::Rar13(archive) => archive.main.is_solid(),
        Archive::Rar15To40(archive) => archive.main.is_solid(),
        Archive::Rar50Plus(archive) => archive.main.is_solid(),
        _ => false,
    }
}

/// The first volume of the set `display` belongs to, by its naming.
fn first_volume(display: &str, new_numbering: bool) -> String {
    if new_numbering {
        let lower = display.to_ascii_lowercase();
        if let Some(dot) = lower.rfind('.')
            && let Some(stem) = lower.get(..dot)
            && let Some(start) = stem.rfind(|c: char| !c.is_ascii_digit())
            && stem.get(..=start).is_some_and(|s| s.ends_with("part"))
        {
            let digits = dot - start - 1;
            if digits > 0 {
                return format!(
                    "{}{:0digits$}{}",
                    display.get(..=start).unwrap_or_default(),
                    1,
                    display.get(dot..).unwrap_or_default()
                );
            }
        }
    }
    match display.rfind('.') {
        Some(dot) => {
            let ext = display.get(dot + 1..).unwrap_or_default();
            let rar = if ext.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                "RAR"
            } else {
                "rar"
            };
            format!("{}{rar}", display.get(..=dot).unwrap_or_default())
        }
        None => display.to_owned(),
    }
}

/// What a file's work came to.
enum Verdict {
    Ok,
    /// The data does not check: "checksum error", or, encrypted, a wrong password too.
    Checksum,
    /// RAR 5's password check says no.
    WrongPassword,
    UnknownMethod,
    /// A volume its data goes on into is not there.
    MissingVolume(String),
    /// What it was written to refused it.
    Write(io::Error),
    /// The file's own checksum is not one rar keeps: "?".
    Unchecked,
}

/// The state of one archive set's work.
struct Work<'r, 'a, SE: cash_core::ShellExtensions> {
    rar: &'r Rar<'a, SE>,
    job: &'r Job,
    volumes: Vec<Volume>,
    files: Vec<Option<File>>,
    /// The set's volumes as paths, for the readers.
    paths: Vec<PathBuf>,
    /// The volume whose "Testing archive" line was the last shown.
    announced: usize,
    decoder5: Option<Box<Unpack50Decoder>>,
    decoders4: Vec<(u8, Decoder4)>,
    /// Whether the last file decoded leaves a solid stream the next can go on with.
    solid_ready: bool,
    keys: Option<([u8; 16], u8, Rar50Keys, bool)>,
    errors: u32,
    done: u32,
    overwrite: Overwrite,
    /// The name of a volume a set goes on into that is not there.
    missing: Option<String>,
    /// The folders extracted, given their times and attributes after their files.
    folders: Vec<(PathBuf, Stamps)>,
    /// Whether the archive is solid, its files one stream.
    solid: bool,
    /// The files not asked for that a file reference asked for copies: by their place.
    sources: std::collections::HashSet<usize>,
    /// Those unpacked, each to a temporary file removed at the end: by their name.
    temps: std::collections::HashMap<String, PathBuf>,
    /// `t`: each volume's recovery record, whether it is intact, if it has one.
    recovery: Vec<Option<bool>>,
    /// The volumes whose recovery record was said, before this one.
    recovery_said: usize,
    /// The password was typed for a file: each encrypted file after asks whether to
    /// use it.
    typed: bool,
    /// "All" answered to that question: no more of it.
    use_all: bool,
    /// The volumes whose damaged service headers were said, before this one.
    services_said: usize,
}

/// What a file keeps of its times and attributes.
#[derive(Clone, Copy)]
struct Stamps {
    modified: Option<Time>,
    created: Option<Time>,
    accessed: Option<Time>,
    attributes: u64,
    host: Host,
}

impl Stamps {
    const fn of(entry: &Entry) -> Self {
        Self {
            modified: entry.modified,
            created: entry.created,
            accessed: entry.accessed,
            attributes: entry.attributes,
            host: entry.host,
        }
    }
}

/// One archive and the volumes after it.
#[expect(
    clippy::too_many_lines,
    reason = "an archive set's work in rar's order: comment, volumes, files, summary"
)]
fn set<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    job: &Job,
    found: &Found,
    tally: &mut Tally,
) -> Result<(), Stop> {
    let opened = match open::open(rar, found)? {
        Ok(opened) => opened,
        Err(failure) => {
            open_failed(rar, job, found, &failure);
            // A file that is no RAR archive counts as one with nothing to extract.
            if matches!(failure, Failure::NotRar) && job.mode != Mode::Find {
                tally.sets += 1;
            }
            return Ok(());
        }
    };
    let mut comment_errors = 0;
    if !job.mode.quiet() && !rar.switches.no_comments {
        match list::comment(rar, found, &opened) {
            Some(Comment::Text(text)) => {
                rar.console.msg(&format!(
                    "\nArchive comment:\n{}\n",
                    text.replace("\r\n", "\n")
                ));
            }
            Some(Comment::Corrupt) => {
                rar.console.err("\nThe archive comment is corrupt");
                rar.fail(code::CRC);
                comment_errors = 1;
            }
            None => {}
        }
    }
    msg(
        rar,
        job,
        &format!("\n{} {}\n", job.mode.archive_verb(), found.display),
    );
    let mut volumes = Vec::new();
    let (first, first_opened) = if opened.facts.volume && !opened.facts.first_volume {
        let display = first_volume(&found.display, opened.facts.new_numbering);
        let first = Found {
            path: rar.path(&display),
            display,
        };
        msg(
            rar,
            job,
            &format!("\n{} {}\n", job.mode.archive_verb(), first.display),
        );
        match open::open(rar, &first)? {
            Ok(opened) => (first, opened),
            Err(failure) => {
                open_failed(rar, job, &first, &failure);
                return Ok(());
            }
        }
    } else {
        (found.clone(), opened)
    };
    let mut next = goes_on(&first_opened)
        .then(|| open::next_volume(&first.display, first_opened.facts.new_numbering));
    let new_numbering = first_opened.facts.new_numbering;
    let mut damage = first_opened.damage;
    volumes.push(Volume {
        display: first.display,
        path: first.path,
        archive: first_opened.archive,
    });
    let mut missing = None;
    while let Some(Some(name)) = next.take() {
        let found = Found {
            path: rar.path(&name),
            display: name.clone(),
        };
        if !found.path.is_file() {
            missing = Some(name);
            break;
        }
        let Ok(opened) = open::open(rar, &found)? else {
            missing = Some(name);
            break;
        };
        if goes_on(&opened) {
            next = Some(open::next_volume(&name, new_numbering));
        }
        damage = damage.or(opened.damage);
        volumes.push(Volume {
            display: name,
            path: found.path,
            archive: opened.archive,
        });
    }
    let paths = volumes.iter().map(|v| v.path.clone()).collect();
    let mut work = Work {
        rar,
        job,
        volumes,
        files: Vec::new(),
        paths,
        announced: 0,
        decoder5: None,
        decoders4: Vec::new(),
        solid_ready: false,
        keys: None,
        errors: comment_errors,
        done: 0,
        overwrite: rar.switches.overwrite.unwrap_or(if rar.switches.yes {
            Overwrite::All
        } else {
            Overwrite::Ask
        }),
        missing,
        folders: Vec::new(),
        solid: false,
        sources: std::collections::HashSet::new(),
        temps: std::collections::HashMap::new(),
        recovery: Vec::new(),
        recovery_said: 0,
        typed: false,
        use_all: false,
        services_said: 0,
    };
    for volume in &work.volumes {
        rar.log_archive(&volume.display);
    }
    let entries = entry::entries(&work.volumes);
    let wanted: Vec<bool> = entries.iter().map(|entry| work.wanted(entry)).collect();
    if job.mode == Mode::Test {
        work.test_recovery();
    }
    work.solid = work
        .volumes
        .first()
        .is_some_and(|volume| archive_solid(&volume.archive));
    if matches!(job.mode, Mode::Extract { .. }) {
        work.sources = reference_sources(&entries, &wanted);
    }
    // rar reads no further than the last file asked for, when one is.
    let last = wanted.iter().rposition(|&wanted| wanted);
    for (index, entry) in entries.iter().enumerate() {
        if last.is_some_and(|last| index > last) {
            break;
        }
        work.entry(index, entry, &entries, wanted[index])?;
    }
    for temp in work.temps.values() {
        let _ = std::fs::remove_file(temp);
    }
    work.finish_folders();
    work.say_damaged_services(work.volumes.len());
    if job.mode == Mode::Test {
        work.test_recovery_records();
        work.test_recovery_volumes(new_numbering);
    }
    if let Some(damage) = damage {
        let end = if job.mode.quiet() { "\n" } else { "" };
        if damage == open::Damage::Unended {
            work.error(&format!("\n{}{end}", damage.words()), code::WARNING);
        } else {
            // Counted twice, as rar counts it: reading, and at the end.
            work.error(&format!("\n{}{end}", damage.words()), code::CRC);
            work.errors += 1;
        }
    }
    tally.errors += work.errors;
    tally.done += work.done;
    tally.sets += 1;
    Ok(())
}

/// Whether a volume's set goes on into another: its end says so, or, with no end to
/// say it, its last file does.
fn goes_on(opened: &open::Opened) -> bool {
    opened.facts.next_volume
        || (opened.facts.volume
            && !opened.facts.end
            && opened
                .items
                .iter()
                .rfind(|item| item.is_file_like())
                .is_some_and(|item| item.split_after))
}

/// An archive that did not open, as `t` and `x` say it.
fn open_failed<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    job: &Job,
    found: &Found,
    failure: &Failure,
) {
    match failure {
        Failure::Missing(error) => {
            rar.console.msg("\n");
            rar.console.err(&format!(
                "\nCannot open {}\n{}",
                found.display,
                open::system_message(error)
            ));
            rar.fail(code::NO_FILES);
        }
        // Its line is ended by what follows: "No files to extract", or `i`'s Done.
        Failure::NotRar => {
            rar.console
                .msg(&format!("\n{} is not RAR archive", found.display));
        }
        Failure::WrongPassword(facts) => {
            if facts.format == "RAR 5" {
                rar.console.msg("\nTotal errors: 1\n");
                rar.console
                    .err(&format!("\nIncorrect password for {}", found.display));
                rar.fail(code::PASSWORD);
            } else {
                rar.console.msg("\nNo files to extract\n");
                rar.console.err(&format!(
                    "\nChecksum error in the encrypted file {}. Corrupt file or wrong password.",
                    found.display
                ));
                rar.fail(code::CRC);
            }
        }
        Failure::Corrupt { truncated, .. } => {
            msg(
                rar,
                job,
                &format!("\n{} {}\n", job.mode.archive_verb(), found.display),
            );
            rar.console.msg("\nTotal errors: 2\n");
            if *truncated {
                rar.console.err("\nUnexpected end of archive");
            } else {
                rar.console.err("\nCorrupt header is found");
            }
            rar.fail(code::CRC);
        }
    }
}

/// A message `p` does not write: its standard output is the files'.
fn msg<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, job: &Job, text: &str) {
    if !job.mode.quiet() {
        rar.console.msg(text);
    }
}

impl<SE: cash_core::ShellExtensions> Work<'_, '_, SE> {
    fn say(&self, text: &str) {
        msg(self.rar, self.job, text);
    }

    /// A file's line: twelve columns of verb, its name in 56, five for the percentage.
    fn file_line(&self, verb: &str, shown: &str) {
        if !self.rar.switches.no_names {
            let area = if self.rar.switches.no_percent {
                ""
            } else {
                "     "
            };
            self.say(&format!("\n{verb}{shown:<56}{area}"));
        }
    }

    /// The line's end: OK, the progress wiped.
    fn ok(&self) {
        if !self.rar.switches.no_names {
            self.say("  OK ");
        }
    }

    /// The archive a file's data goes on into, said as rar says it.
    fn announce(&mut self, volume: usize) {
        if volume > self.announced && volume < self.volumes.len() {
            self.announced = volume;
            self.say_recovery(volume);
            let text = format!(
                "\n\n{} {}\n",
                self.job.mode.archive_verb(),
                self.volumes[volume].display
            );
            self.say(&text);
        }
    }

    fn error(&mut self, text: &str, code: u8) {
        self.rar.console.err(text);
        self.errors += 1;
        self.rar.fail(code);
    }

    fn wanted(&self, entry: &Entry) -> bool {
        let masks = &self.job.masks;
        let by_name = match (&self.rar.switches.versions, entry.version) {
            // The files themselves; an older version only when a name is it exactly.
            (None, None) => masks.wants(&entry.name),
            (None, Some(_)) => masks.names(&entry.name),
            // `-ver`: every version, under its `;N` name.
            (Some(Arg::Bare), _) => masks.wants(&entry.name),
            // `-verN`: version N alone, under its file's name.
            (Some(Arg::Given(number)), Some(version)) => {
                number.parse() == Ok(version) && masks.wants(unversioned(entry))
            }
            (Some(Arg::Given(_)), None) => false,
        };
        by_name && !(self.rar.switches.skip_encrypted && entry.encrypted())
    }

    /// One file of the set, `index` its place, `wanted` whether it was asked for.
    fn entry(
        &mut self,
        index: usize,
        entry: &Entry,
        entries: &[Entry],
        wanted: bool,
    ) -> Result<(), Stop> {
        // A header that failed its checksum is said so where it is, two errors, and
        // used as it stands, as rar does.
        if entry.damaged {
            self.rar.console.err(&format!(
                "\nCorrupt header is found\n{} - the file header is corrupt",
                entry.name
            ));
            self.errors += 2;
            self.rar.fail(code::CRC);
        }
        if !wanted {
            if self.sources.contains(&index) {
                self.unpack_source(entry);
            } else if !self.solid {
                self.solid_ready = false;
            } else if !entry.directory {
                // A solid stream decodes what it does not want, for what follows, and
                // says so when it tests or extracts.
                let said = matches!(self.job.mode, Mode::Test | Mode::Extract { .. });
                if said {
                    self.file_line("Skipping    ", &entry.name);
                }
                if entry.link.is_none() {
                    let _ = self.decode(entry, &mut Discard, None);
                }
                if said {
                    self.wipe_progress();
                }
            }
            return Ok(());
        }
        if entry.volume > self.announced {
            self.announce(entry.volume);
        }
        match self.job.mode {
            Mode::Test => self.test(entry),
            Mode::Print => self.print(entry),
            Mode::Find => self.find(entry),
            Mode::Extract { paths } => self.extract(entry, paths, entries),
        }
    }

    fn test(&mut self, entry: &Entry) -> Result<(), Stop> {
        if entry.directory {
            if !self.rar.switches.no_names {
                self.say(&format!("\nTesting     {:<56}  OK", entry.name));
            }
            self.done += 1;
            return Ok(());
        }
        if !self.ready(entry)? {
            return Ok(());
        }
        self.file_line(self.job.mode.file_verb(), &entry.name);
        let verdict = self.decode(entry, &mut Discard, Some(&entry.name));
        self.verdict(entry, verdict);
        self.rar.log_file(&entry.name);
        Ok(())
    }

    fn print(&mut self, entry: &Entry) -> Result<(), Stop> {
        if entry.directory {
            return Ok(());
        }
        if !self.ready(entry)? {
            return Ok(());
        }
        let mut out = PrintTo { rar: self.rar };
        let verdict = self.decode(entry, &mut out, None);
        self.verdict(entry, verdict);
        Ok(())
    }

    /// `i`: the file read through for the string, and where it is shown when found.
    fn find(&mut self, entry: &Entry) -> Result<(), Stop> {
        if entry.directory || entry.link.is_some() {
            return Ok(());
        }
        let Some(needles) = self.job.find.as_ref().and_then(find::Needles::new) else {
            return Ok(());
        };
        if !self.ready(entry)? {
            return Ok(());
        }
        // A solid stream's next file needs this one decoded to its end.
        let solid = self
            .volumes
            .first()
            .is_some_and(|volume| archive_solid(&volume.archive));
        let mut matcher = find::Matcher::new(&needles, !solid);
        let verdict = self.decode(entry, &mut matcher, None);
        // A found string stops the reading: its file is not checked to the end.
        let stopped = matches!(&verdict, Verdict::Write(error) if find::stopped(error));
        if let Some(shown) = matcher.shown() {
            let archive = self.volumes.first().map_or("", |v| v.display.as_str());
            self.rar
                .console
                .notice(&format!("\nFound  {archive} / {}\n  {shown}", entry.name));
        }
        if stopped {
            self.done += 1;
        } else {
            self.verdict(entry, verdict);
        }
        Ok(())
    }

    /// Whether a file can be worked on: its password asked for, RAR 5's checked, its
    /// method known. What stops it is said.
    fn ready(&mut self, entry: &Entry) -> Result<bool, Stop> {
        if entry.method == Method::Unknown {
            // Said, and the status set, but not one of the errors counted.
            self.rar.console.err(&format!(
                "\nUnknown method in {}\nYou may need a newer version of RAR.",
                entry.name
            ));
            self.rar.fail(code::FATAL);
            return Ok(false);
        }
        if entry.bad_comment {
            // rar 7.23 reads no RAR 1.5 to 2.9 file comment, and counts each.
            self.errors += 1;
            self.rar.fail(code::CRC);
        }
        // A wrong password is asked for again when it was typed for this file, or is
        // the one typed before that rar was told to use; else it is an error.
        let mut retry = false;
        if entry.encrypted() {
            if self.rar.password.borrow().is_none() {
                self.new_password(&entry.name, false)?;
                retry = true;
            } else if self.typed && !self.use_all {
                retry = self.use_current(&entry.name)?;
            }
        }
        if let Some(Crypto::Rar5 {
            salt, count, check, ..
        }) = &entry.crypto
        {
            while !self.rar5_keys(*salt, *count, *check) {
                if retry {
                    self.rar
                        .console
                        .err("\nThe specified password is incorrect.\n");
                    self.new_password(&entry.name, true)?;
                    continue;
                }
                for (index, part) in entry.parts.iter().enumerate() {
                    if index > 0 {
                        self.announce(part.volume);
                    }
                    self.error(
                        &format!("\nIncorrect password for {}", entry.name),
                        code::PASSWORD,
                    );
                }
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// A password typed for the file `name`, the one used from then on; `again` after
    /// rar's "The specified password is incorrect.".
    fn new_password(&mut self, name: &str, again: bool) -> Result<(), Stop> {
        let password = self
            .rar
            .ask_password_for(Some(name), again, self.volumes.len() > 1)?;
        *self.rar.password.borrow_mut() = Some(password);
        self.keys = None;
        self.typed = true;
        Ok(())
    }

    /// rar's question before an encrypted file when the password was typed for
    /// another: No asks for a new one, All stops the question; whether a wrong one
    /// is asked for again (all but All).
    fn use_current(&mut self, name: &str) -> Result<bool, Stop> {
        self.rar.console.err(&format!(
            "\n{name} - use current password? [Y]es, [N]o, [A]ll "
        ));
        let Some(answer) = self.rar.read_answer(true)? else {
            self.rar.console.err("Read error in the file stdin");
            if let Some(extra) = self.rar.stdin_end_words(false) {
                self.rar.console.err(&format!("\n{extra}"));
            }
            return Err(Stop::Aborted(code::READ));
        };
        match answer.trim().chars().next().map(|c| c.to_ascii_lowercase()) {
            Some('n') => {
                self.new_password(name, false)?;
                Ok(true)
            }
            Some('a') => {
                self.use_all = true;
                Ok(false)
            }
            _ => Ok(true),
        }
    }

    /// RAR 5's keys for the password and an encryption record, made once a salt and a
    /// count; whether the record's check says the password is the one.
    fn rar5_keys(&mut self, salt: [u8; 16], count: u8, check: Option<[u8; 12]>) -> bool {
        let Some(password) = self.rar.password.borrow().clone() else {
            return false;
        };
        let fresh = self
            .keys
            .as_ref()
            .is_none_or(|(s, c, _, _)| *s != salt || *c != count);
        if fresh {
            let Some((keys, ok)) = rardata::rar5_derive_keys(check, salt, count, &password) else {
                return false;
            };
            self.keys = Some((salt, count, keys, ok));
        }
        match (&self.keys, check) {
            (Some((_, _, keys, _)), Some(check)) => rardata::rar5_check_matches(keys, check),
            (Some((_, _, _, ok)), None) => *ok,
            (None, _) => false,
        }
    }

    /// What the work on a file came to, said.
    fn verdict(&mut self, entry: &Entry, verdict: Verdict) {
        match verdict {
            Verdict::Ok => {
                self.ok();
                self.done += 1;
            }
            Verdict::Unchecked => {
                if !self.rar.switches.no_names {
                    self.say("   ? ");
                }
                self.done += 1;
            }
            Verdict::Checksum if entry.encrypted() && entry.blake.is_none() => {
                self.error(
                    &format!(
                        "\nChecksum error in the encrypted file {}. Corrupt file or wrong password.",
                        entry.name
                    ),
                    code::CRC,
                );
            }
            Verdict::Checksum => {
                self.error(&format!("\n{:<20} - checksum error", entry.name), code::CRC);
            }
            Verdict::WrongPassword => {
                self.error(
                    &format!("\nIncorrect password for {}", entry.name),
                    code::PASSWORD,
                );
            }
            Verdict::UnknownMethod => {
                self.error(
                    &format!(
                        "\nUnknown method in {}\nYou may need a newer version of RAR.",
                        entry.name
                    ),
                    code::FATAL,
                );
            }
            Verdict::MissingVolume(name) => {
                self.say("     ");
                self.error(&format!("\nCannot find volume {name}"), code::OPEN);
                self.error(
                    &format!("\n{:<20} - checksum error", entry.name),
                    code::OPEN,
                );
            }
            Verdict::Write(error) => {
                self.error(
                    &format!(
                        "\nWrite error in the file {}\n{}",
                        entry.name,
                        open::system_message(&error)
                    ),
                    code::WRITE,
                );
            }
        }
    }

    /// A file's data decoded into `out` and checked; `shown`, the name its line shows,
    /// for the lines between volumes.
    #[expect(
        clippy::too_many_lines,
        reason = "a file's parts read, decrypted, decoded and checked, as one stream"
    )]
    fn decode(&mut self, entry: &Entry, out: &mut dyn Sink, shown: Option<&str>) -> Verdict {
        let mut hashed = Hashed {
            out,
            crc: crc32fast::Hasher::new(),
            blake: entry.blake.map(|_| blake2sp::Hasher::new()),
            sum13: 0,
            written: 0,
            limit: entry.size,
        };
        let cipher = match self.cipher(entry) {
            Ok(cipher) => cipher,
            Err(verdict) => return verdict,
        };
        let split_on = self.split_on(entry);
        let parts: Vec<rardata::Part> = entry
            .parts
            .iter()
            .map(|p| rardata::Part {
                volume: p.volume,
                pos: p.pos,
                size: p.size,
                check: match &p.check {
                    Some(rardata::PartCheck::Crc(crc)) => Some(rardata::PartCheck::Crc(*crc)),
                    Some(rardata::PartCheck::Hashes { crc, blake }) => {
                        Some(rardata::PartCheck::Hashes {
                            crc: *crc,
                            blake: *blake,
                        })
                    }
                    None => None,
                },
            })
            .collect();
        let volume_names: Vec<String> = self.volumes.iter().map(|v| v.display.clone()).collect();
        let verb = self.job.mode.archive_verb();
        let console = self.rar.console;
        let print = !self.job.mode.quiet() && !self.rar.switches.no_names;
        let area = if self.rar.switches.no_percent {
            ""
        } else {
            "     "
        };
        let quiet_lines = !self.job.mode.quiet();
        let mut announced = self.announced;
        let recovery = &self.recovery;
        let mut recovery_said = self.recovery_said;
        let mut recovery_failed = 0;
        let mut hook = |part: usize| -> io::Result<()> {
            let volume = entry.parts.get(part).map_or(0, |p| p.volume);
            if volume > announced {
                announced = volume;
                // The recovery record of a volume left is tested on leaving it.
                let (text, failed) = recovery_lines(recovery, recovery_said, volume);
                recovery_said = recovery_said.max(volume);
                recovery_failed += failed;
                if quiet_lines {
                    console.msg(&text);
                }
                if quiet_lines {
                    console.msg(&format!(
                        "\n\n{verb} {}\n",
                        volume_names.get(volume).map_or("", String::as_str)
                    ));
                }
            }
            if print && shown.is_some() {
                console.msg(&format!("\n...         {:<56}{area}", entry.name));
            }
            Ok(())
        };
        let result = {
            let mut vols = VolsReader::new(&self.paths, &mut self.files, parts).on_next(&mut hook);
            let mut input: Box<dyn Read> = match cipher {
                Some(cipher) => Box::new(DecryptReader::new(&mut vols, cipher)),
                None => Box::new(&mut vols),
            };
            let solid = entry.solid && self.solid_ready;
            let size = entry
                .size
                .and_then(|s| usize::try_from(s).ok())
                .unwrap_or(0);
            let result = match entry.method {
                Method::Stored => rardata::copy(&mut input, &mut hashed),
                Method::Lz5 {
                    version,
                    dictionary,
                } => {
                    let decoder = self
                        .decoder5
                        .get_or_insert_with(|| Box::new(Unpack50Decoder::new()));
                    rardata::decode5(
                        decoder,
                        &mut input,
                        version,
                        size,
                        usize::try_from(dictionary).unwrap_or(usize::MAX),
                        solid,
                        &mut hashed,
                    )
                }
                Method::Lz4 { version } => rardata::decode4(
                    &mut self.decoders4,
                    version,
                    solid,
                    &mut input,
                    size,
                    &mut hashed,
                    true,
                ),
                Method::Unknown => Err(DecodeError::Data),
            };
            drop(input);
            (result, vols.crc_ok)
        };
        self.announced = announced;
        self.recovery_said = recovery_said;
        for _ in 0..recovery_failed {
            self.error("\nRecovery record is corrupt.", code::CRC);
        }
        self.solid_ready = true;
        if let Some(name) = split_on {
            return Verdict::MissingVolume(name);
        }
        let (result, parts_ok) = result;
        if let Err(DecodeError::Sink(error)) = result {
            return Verdict::Write(error);
        }
        // RAR 1.3 keeps its own sum; a file with no checksum at all is unchecked, its
        // data decoded or not.
        if let Some(want) = entry.sum13 {
            return if result.is_ok() && hashed.sum13 == want {
                Verdict::Ok
            } else {
                Verdict::Checksum
            };
        }
        if entry.crc.is_none() && entry.blake.is_none() {
            return Verdict::Unchecked;
        }
        if result.is_err() || !parts_ok {
            return Verdict::Checksum;
        }
        let mac = if entry.mac {
            self.keys.as_ref().map(|(_, _, keys, _)| keys.clone())
        } else {
            None
        };
        let crc = hashed.crc.clone().finalize();
        if let Some(want) = entry.crc
            && entry.blake.is_none()
        {
            let crc = mac.as_ref().map_or(crc, |keys| keys.mac_crc32(crc));
            if crc != want {
                return Verdict::Checksum;
            }
        }
        if let (Some(want), Some(hasher)) = (entry.blake, hashed.blake.take()) {
            let digest = hasher.finalize();
            let digest = mac.as_ref().map_or(digest, |keys| keys.mac_hash32(digest));
            if digest != want {
                return Verdict::Checksum;
            }
        }
        Verdict::Ok
    }

    /// The name of the volume a file's data goes on into that is not there.
    fn split_on(&self, entry: &Entry) -> Option<String> {
        let last = entry.parts.last()?;
        (last.check.is_some() && last.volume + 1 >= self.volumes.len())
            .then(|| self.missing.clone())
            .flatten()
    }

    /// The cipher a file's data needs, keyed by the password.
    fn cipher(&self, entry: &Entry) -> Result<Option<Cipher>, Verdict> {
        let Some(crypto) = &entry.crypto else {
            return Ok(None);
        };
        let password = self.rar.password.borrow().clone().unwrap_or_default();
        let password = rardata::password_utf8(&password);
        Ok(Some(match crypto {
            Crypto::Rar5 { iv, .. } => {
                let Some((_, _, keys, _)) = &self.keys else {
                    return Err(Verdict::WrongPassword);
                };
                Cipher::Rar5(Box::new(Rar50Cipher::new(keys.key, *iv)))
            }
            Crypto::Rar3 { salt } => match Rar30Cipher::new(password.as_bytes(), *salt) {
                Ok(cipher) => Cipher::Rar3(Box::new(cipher)),
                Err(_) => return Err(Verdict::UnknownMethod),
            },
            Crypto::Rar2 => Cipher::Rar2(Box::new(Rar20Cipher::new(password.as_bytes()))),
            Crypto::Rar15 => Cipher::Rar15(Box::new(Rar15Cipher::new(password.as_bytes()))),
            Crypto::Rar13 => Cipher::Rar13(Box::new(Rar13Cipher::new(password.as_bytes()))),
        }))
    }

    fn extract(&mut self, entry: &Entry, paths: bool, entries: &[Entry]) -> Result<(), Stop> {
        let relative = self.target_name(entry, paths);
        let Some(relative) = relative else {
            return Ok(());
        };
        let shown = match self.destination() {
            Some(dest) => format!("{dest}/{relative}"),
            None => relative,
        };
        let path = self.rar.path(&shown);
        // A junction, or a folder's symbolic link, is a link to make, not a folder.
        if entry.directory && entry.link.is_none() {
            if paths && !matches!(self.rar.switches.exclude_paths, Some(0)) {
                // `-f` freshens what is there: a folder that is not stays so.
                if self.rar.switches.freshen && !path.is_dir() {
                    return Ok(());
                }
                self.make_folders(&shown);
                self.folders.push((path, Stamps::of(entry)));
                self.done += 1;
                self.rar.log_file(&entry.name);
            }
            return Ok(());
        }
        if !self.fresher(entry, &path) {
            return Ok(());
        }
        if !self.ready(entry)? {
            return Ok(());
        }
        if let Some(link) = &entry.link {
            return self.extract_link(entry, link, &shown, path, paths, entries);
        }
        self.make_parent(&shown);
        let Some(path) = self.overwrite(entry, &shown, path)? else {
            return Ok(());
        };
        let shown = path_shown(&shown, &path, self.rar);
        self.file_line("Extracting  ", &shown);
        self.write_out(entry, entry, &path, &shown);
        self.rar.log_file(&entry.name);
        Ok(())
    }

    /// The data of `data` written to `path`, checked, with the times and attributes of
    /// `entry`; a file that does not check removed, unless `-kb`.
    fn write_out(&mut self, data: &Entry, entry: &Entry, path: &Path, shown: &str) {
        let file = match File::create(path) {
            Ok(file) => file,
            Err(error) => {
                self.error(
                    &format!("\nCannot create {shown}\n{}", open::system_message(&error)),
                    code::CREATE,
                );
                return;
            }
        };
        let mut out = FileOut {
            file: BufWriter::new(file),
        };
        let verdict = self.decode(data, &mut out, Some(shown));
        let flushed = out.file.flush();
        drop(out);
        let verdict = match (verdict, flushed) {
            (Verdict::Ok, Err(error)) => Verdict::Write(error),
            (verdict, _) => verdict,
        };
        let good = matches!(verdict, Verdict::Ok | Verdict::Unchecked);
        if good {
            self.finish_file(entry, path);
        } else if !self.rar.switches.keep_broken && !matches!(verdict, Verdict::Write(_)) {
            let _ = std::fs::remove_file(path);
        }
        self.verdict(data, verdict);
    }

    /// A link, made as the archive keeps it: a hard link to a file extracted before it,
    /// a copy of one, or a symbolic link, which Windows lets only some make. `-ol-`
    /// leaves symbolic links out; without `-ola` one that could reach outside the
    /// destination is skipped, said.
    fn extract_link(
        &mut self,
        entry: &Entry,
        link: &Link,
        shown: &str,
        path: PathBuf,
        paths: bool,
        entries: &[Entry],
    ) -> Result<(), Stop> {
        match link {
            Link::Hard(_) | Link::Copy(_) => {
                self.extract_file_link(entry, link, shown, path, paths, entries)
            }
            Link::Unix(target) | Link::Windows(target) => {
                self.extract_symlink(entry, target, shown, path, false)
            }
            Link::Junction(target) => self.extract_symlink(entry, target, shown, path, true),
        }
    }

    /// A hard link, to its file extracted before it, or a file reference, a copy of
    /// its file: from the archive when the file is not there.
    fn extract_file_link(
        &mut self,
        entry: &Entry,
        link: &Link,
        shown: &str,
        path: PathBuf,
        paths: bool,
        entries: &[Entry],
    ) -> Result<(), Stop> {
        let (Link::Hard(target) | Link::Copy(target)) = link else {
            return Ok(());
        };
        let hard = matches!(link, Link::Hard(_));
        let target = target.replace('\\', "/");
        let target_path = self.rar.path(&self.beside_destination(&target, paths));
        if hard && !target_path.is_file() {
            self.file_line("Extracting  ", shown);
            self.wipe_progress();
            self.error(
                &format!(
                    "\nCannot create hard link {shown}\nYou need to unpack the link target first"
                ),
                code::CREATE,
            );
            return Ok(());
        }
        self.make_parent(shown);
        let Some(path) = self.overwrite(entry, shown, path)? else {
            return Ok(());
        };
        let shown = path_shown(shown, &path, self.rar);
        self.file_line("Extracting  ", &shown);
        let _ = std::fs::remove_file(&path);
        // A file not asked for was unpacked to a temporary file for its references.
        let target_path = match self.temps.get(&target.to_lowercase()) {
            Some(temp) if !hard && !target_path.is_file() => temp.clone(),
            _ => target_path,
        };
        if !hard && !target_path.is_file() {
            match entries
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(&target))
            {
                Some(data) => self.write_out(data, entry, &path, &shown),
                None => self.error(
                    &format!(
                        "\nCannot create {shown}\n{}",
                        open::system_message(&io::Error::from_raw_os_error(2))
                    ),
                    code::CREATE,
                ),
            }
            return Ok(());
        }
        let made = if hard {
            std::fs::hard_link(&target_path, &path)
        } else {
            std::fs::copy(&target_path, &path).map(|_| ())
        };
        match made {
            Ok(()) => {
                self.finish_file(entry, &path);
                self.ok();
                self.done += 1;
                self.rar.log_file(&entry.name);
            }
            Err(error) => self.error(
                &format!("\nCannot create {shown}\n{}", open::system_message(&error)),
                code::CREATE,
            ),
        }
        Ok(())
    }

    /// A symbolic link or, with `junction`, a junction: left out with `-ol-`, skipped as
    /// unsafe without `-ola`, made when Windows allows it. Its folder is made once it is
    /// to be made, its line said first.
    fn extract_symlink(
        &mut self,
        entry: &Entry,
        target: &str,
        shown: &str,
        path: PathBuf,
        junction: bool,
    ) -> Result<(), Stop> {
        if self.rar.switches.links.as_deref() == Some("-") {
            return Ok(());
        }
        let target = if target.is_empty() {
            self.stored_target(entry)
        } else {
            target.to_owned()
        };
        let Some(path) = self.overwrite(entry, shown, path)? else {
            return Ok(());
        };
        let shown = path_shown(shown, &path, self.rar);
        self.file_line("Extracting  ", &shown);
        let target = target.replace('/', "\\");
        if self.rar.switches.links.as_deref() != Some("a") && !safe_link(&entry.name, &target) {
            self.error(
                &format!(
                    "\nSkipping the potentially unsafe {} -> {} link. For archives from a trustworthy source use -ola to extract it anyway.",
                    entry.name,
                    target.replace('\\', "/")
                ),
                code::WARNING,
            );
            return Ok(());
        }
        self.make_parent(&shown);
        if junction {
            self.make_junction(entry, &target, &shown, &path);
            return Ok(());
        }
        let _ = std::fs::remove_file(&path);
        let made = if entry.link_to_folder {
            std::os::windows::fs::symlink_dir(&target, &path)
        } else {
            std::os::windows::fs::symlink_file(&target, &path)
        };
        match made {
            Ok(()) => {
                self.ok();
                self.done += 1;
                self.rar.log_file(&entry.name);
            }
            Err(error) => {
                // rar makes a folder's link on a folder it makes first.
                if entry.link_to_folder {
                    let _ = std::fs::create_dir(&path);
                }
                self.error(
                    &format!(
                        "\nCannot create symbolic link {shown}\nYou may need to run RAR as administrator\n{}",
                        open::system_message(&error)
                    ),
                    code::CREATE,
                );
            }
        }
        Ok(())
    }

    /// A junction to `target`, its `\??\` taken off for the reparse data to put back.
    /// rar ends its line with a question mark over the progress, after the line of the
    /// folder it made for it if any.
    fn make_junction(&mut self, entry: &Entry, target: &str, shown: &str, path: &Path) {
        let target = target.strip_prefix(r"\??\").unwrap_or(target);
        let data = cash_win32::reparse::link_data(target, true);
        match cash_win32::reparse::set(path, true, &data) {
            Ok(()) => {
                if !self.rar.switches.no_names {
                    let back = if self.rar.switches.no_percent {
                        ""
                    } else {
                        "\u{8}\u{8}\u{8}\u{8}\u{8}"
                    };
                    self.say(&format!("{back}   ? "));
                }
                self.done += 1;
                self.rar.log_file(&entry.name);
            }
            Err(error) => self.error(
                &format!("\nCannot create {shown}\n{}", open::system_message(&error)),
                code::CREATE,
            ),
        }
    }

    /// A file not asked for that a reference asked for copies: unpacked first to a
    /// temporary file in the destination, as rar does, said as rar says it.
    fn unpack_source(&mut self, entry: &Entry) {
        let name = format!(
            "__tmp_reference_source_{}.{}.rartemp",
            std::process::id(),
            self.temps.len()
        );
        let shown = match &self.job.dest_typed {
            Some(dest) => format!("{dest}/{name}"),
            None => name,
        };
        let path = self.rar.path(&shown);
        self.file_line("Extracting  ", &shown);
        self.write_out(entry, entry, &path, &shown);
        self.temps.insert(entry.name.to_lowercase(), path);
    }

    /// The progress a file's line shows, wiped: five spaces.
    fn wipe_progress(&self) {
        if !self.rar.switches.no_names && !self.rar.switches.no_percent {
            self.say("     ");
        }
    }

    /// Makes the folders a file's name holds, saying so.
    fn make_parent(&mut self, shown: &str) {
        if let Some(parent) = Path::new(shown).parent()
            && !parent.as_os_str().is_empty()
        {
            let parent = parent.to_string_lossy().replace('\\', "/");
            self.make_folders(&parent);
        }
    }

    /// An archived name where it is extracted: in the destination, its folders kept or
    /// not.
    fn beside_destination(&self, name: &str, paths: bool) -> String {
        let keep_paths = paths && !matches!(self.rar.switches.exclude_paths, Some(0));
        let name = if keep_paths {
            name
        } else {
            name.rsplit('/').next().unwrap_or(name)
        };
        match self.destination() {
            Some(dest) => format!("{dest}/{name}"),
            None => name.to_owned(),
        }
    }

    /// A RAR 1.5 to 4 symbolic link's target, which it keeps as its data.
    fn stored_target(&mut self, entry: &Entry) -> String {
        let mut target = Collect(Vec::new());
        let _ = self.decode(entry, &mut target, None);
        String::from_utf8_lossy(&target.0).into_owned()
    }

    /// Where files go: the destination named; with `-ad1` a folder named after the
    /// archive, beside it; with `-ad2` the archive's own folder.
    fn destination(&self) -> Option<String> {
        let archive = self.volumes.first().map_or("", |v| v.display.as_str());
        let folder = archive
            .rfind('/')
            .map(|at| archive.get(..at).unwrap_or_default().to_owned());
        match self.rar.switches.alt_destination {
            Some(1) => {
                let stem = Path::new(archive)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Some(folder.map_or_else(|| stem.clone(), |folder| format!("{folder}/{stem}")))
            }
            Some(2) => folder,
            _ => self.job.dest.clone(),
        }
    }

    /// `-f` and `-u`: a file that is there is written only when the archive's copy is
    /// newer, one that is not only without `-f`.
    fn fresher(&self, entry: &Entry, path: &Path) -> bool {
        let switches = &self.rar.switches;
        if !switches.freshen && !switches.update {
            return true;
        }
        let Ok(on_disk) = std::fs::metadata(path).and_then(|m| m.modified()) else {
            return !switches.freshen;
        };
        entry
            .modified
            .and_then(|time| self.system_time(time))
            .is_some_and(|archived| archived > on_disk)
    }

    /// The name a file is extracted under, relative to the destination: its archived
    /// name with `-ap`'s or `-ep4`'s prefix taken off, its folders kept or not, in
    /// `-cl`'s or `-cu`'s case. `None` when `-ap` leaves it out.
    fn target_name(&self, entry: &Entry, paths: bool) -> Option<String> {
        let mut name = if matches!(self.rar.switches.versions, Some(Arg::Given(_))) {
            unversioned(entry).to_owned()
        } else {
            entry.name.clone()
        };
        for prefix in [
            &self.rar.switches.archive_path,
            &self.rar.switches.exclude_prefix,
        ]
        .into_iter()
        .flatten()
        {
            let prefix = prefix.replace('\\', "/");
            let prefix = prefix.trim_matches('/');
            if prefix.is_empty() {
                continue;
            }
            let lower = name.to_lowercase();
            let start = format!("{}/", prefix.to_lowercase());
            if lower.starts_with(&start) {
                name = name.get(start.len()..).unwrap_or_default().to_owned();
            } else if lower == prefix.to_lowercase() {
                return None;
            } else if self
                .rar
                .switches
                .archive_path
                .as_deref()
                .is_some_and(|p| p.replace('\\', "/").trim_matches('/') == prefix)
            {
                return None;
            }
        }
        if self.rar.switches.exclude_paths == Some(1) {
            for base in self.job.masks.bases() {
                let lower = name.to_lowercase();
                let start = format!("{}/", base.to_lowercase());
                if lower.starts_with(&start) {
                    name = name.get(start.len()..).unwrap_or_default().to_owned();
                    break;
                }
            }
        }
        let keep_paths = paths && !matches!(self.rar.switches.exclude_paths, Some(0));
        if !keep_paths {
            name = name.rsplit('/').next().unwrap_or(&name).to_owned();
        }
        if let Some(case) = self.rar.switches.case {
            name = if case == 'l' {
                name.to_lowercase()
            } else {
                name.to_uppercase()
            };
        }
        if let Some(alt) = self.rar.switches.alt_destination
            && alt == 0
        {
            let base = Path::new(&self.volumes[0].display)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            name = format!("{base}/{name}");
        }
        Some(name)
    }

    /// Makes each folder of `shown` that is not there, saying so: "Creating".
    fn make_folders(&mut self, shown: &str) {
        let parts: Vec<&str> = shown.split('/').filter(|p| !p.is_empty()).collect();
        let mut so_far = String::new();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 || shown.starts_with('/') {
                so_far.push('/');
            }
            so_far.push_str(part);
            let path = self.rar.path(&so_far);
            if path.exists() {
                continue;
            }
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    if !self.rar.switches.no_names {
                        self.say(&format!("\nCreating    {so_far:<56}  OK"));
                    }
                }
                Err(error) => {
                    self.error(
                        &format!("\nCannot create {so_far}\n{}", open::system_message(&error)),
                        code::CREATE,
                    );
                    return;
                }
            }
        }
    }

    /// What to do with a file that is there: `-o` and `-y`, else the question. The path
    /// to write, or `None` to skip it.
    fn overwrite(
        &mut self,
        entry: &Entry,
        shown: &str,
        path: PathBuf,
    ) -> Result<Option<PathBuf>, Stop> {
        let Ok(metadata) = std::fs::metadata(&path) else {
            return Ok(Some(path));
        };
        match self.overwrite {
            Overwrite::All => return Ok(Some(path)),
            Overwrite::Skip => return Ok(None),
            Overwrite::Rename => return Ok(Some(free_name(&path))),
            Overwrite::Ask => {}
        }
        let existing_time = metadata
            .modified()
            .ok()
            .map(|time| self.rar.zone.format_system_time(time, "%Y-%m-%d %H:%M"))
            .unwrap_or_default();
        let new_time = entry
            .modified
            .and_then(|time| self.wall_time(time))
            .unwrap_or_default();
        let size = entry.size.unwrap_or(0);
        let question = format!(
            "\n\nWould you like to replace the existing file {shown}\n{:>6} bytes, modified on {existing_time}\nwith a new one\n{size:>6} bytes, modified on {new_time}\n\n[Y]es, [N]o, [A]ll, n[E]ver, [R]ename, [Q]uit ",
            metadata.len()
        );
        loop {
            self.rar.console.err(&question);
            let Some(answer) = self.rar.read_answer(true)? else {
                self.rar.console.err("Read error in the file stdin");
                if let Some(extra) = self.rar.stdin_end_words(false) {
                    self.rar.console.err(&format!("\n{extra}"));
                }
                return Err(Stop::Aborted(code::READ));
            };
            match answer.trim().chars().next().map(|c| c.to_ascii_lowercase()) {
                Some('y') => return Ok(Some(path)),
                Some('n') => return Ok(None),
                Some('a') => {
                    self.overwrite = Overwrite::All;
                    return Ok(Some(path));
                }
                Some('e') => {
                    self.overwrite = Overwrite::Skip;
                    return Ok(None);
                }
                Some('r') => {
                    self.rar.console.msg("\nEnter new name: ");
                    let Some(name) = self.rar.read_answer(true)? else {
                        self.rar.console.err("Read error in the file stdin");
                        if let Some(extra) = self.rar.stdin_end_words(false) {
                            self.rar.console.err(&format!("\n{extra}"));
                        }
                        return Err(Stop::Aborted(code::READ));
                    };
                    let renamed = path
                        .parent()
                        .map_or_else(|| PathBuf::from(&name), |dir| dir.join(&name));
                    return Ok(Some(renamed));
                }
                Some('q') => return Err(Stop::Quit),
                _ => {}
            }
        }
    }

    /// A time as the question shows it: `YYYY-MM-DD HH:MM` in the zone.
    fn wall_time(&self, time: Time) -> Option<String> {
        match time {
            Time::Utc(ticks) => {
                let seconds = i64::try_from(ticks / 10_000_000).ok()? - 11_644_473_600;
                let utc = chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)?;
                Some(self.rar.zone.format(utc, "%Y-%m-%d %H:%M"))
            }
            Time::Dos { time, .. } => Some(format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                1980 + (time >> 25),
                (time >> 21) & 0xF,
                (time >> 16) & 0x1F,
                (time >> 11) & 0x1F,
                (time >> 5) & 0x3F
            )),
        }
    }

    /// The folders extracted, given their times and attributes now their files are in.
    fn finish_folders(&mut self) {
        for (path, stamps) in std::mem::take(&mut self.folders).iter().rev() {
            self.finish(stamps, path, true);
        }
    }

    /// A file written: its times and attributes as the archive keeps them.
    fn finish_file(&self, entry: &Entry, path: &Path) {
        self.finish(&Stamps::of(entry), path, false);
    }

    /// A file's or folder's times and attributes, as the archive keeps them. Its
    /// modification time unless `-tsm-`; its creation and access times when `-ts`,
    /// `-tsc` or `-tsa` asks for them; its attributes unless `-ai`.
    fn finish(&self, entry: &Stamps, path: &Path, folder: bool) {
        let asked = self.rar.switches.times;
        let restore = |which: Option<char>, time: Option<Time>| {
            which
                .filter(|&precision| precision != '-')
                .and(time)
                .and_then(|time| self.system_time(time))
        };
        let stored = if asked.modified == Some('-') {
            None
        } else {
            entry.modified.and_then(|time| self.system_time(time))
        };
        let created = restore(asked.created, entry.created);
        let accessed = restore(asked.accessed, entry.accessed);
        let modified = stored.or_else(|| {
            (created.is_some() || accessed.is_some())
                .then(|| std::fs::metadata(path).and_then(|m| m.modified()).ok())
                .flatten()
        });
        if let Some(modified) = modified {
            let times = cash_win32::unix::Times {
                modified,
                accessed,
                created,
            };
            let _ = cash_win32::unix::set_times(path, &times);
        }
        if !self.rar.switches.ignore_attributes && entry.host == Host::Windows {
            let attributes = u32::try_from(entry.attributes & 0x27).unwrap_or(0);
            if attributes != 0x20 && attributes != 0 {
                let _ = cash_win32::unix::set_attributes(path, attributes);
            }
        } else if !self.rar.switches.ignore_attributes
            && !folder
            && entry.host == Host::Unix
            && entry.attributes & 0o200 == 0
        {
            let _ = cash_win32::unix::set_attributes(path, cash_win32::unix::READ_ONLY);
        }
    }

    /// A kept time as the system's.
    fn system_time(&self, time: Time) -> Option<std::time::SystemTime> {
        match time {
            Time::Utc(ticks) => {
                let nanos =
                    (u128::from(ticks) * 100).checked_sub(11_644_473_600 * 1_000_000_000)?;
                std::time::UNIX_EPOCH
                    .checked_add(std::time::Duration::from_nanos(u64::try_from(nanos).ok()?))
            }
            Time::Dos {
                time,
                nanos,
                add_second,
            } => {
                let date = chrono::NaiveDate::from_ymd_opt(
                    i32::try_from(1980 + (time >> 25)).ok()?,
                    (time >> 21) & 0xF,
                    (time >> 16) & 0x1F,
                )?;
                let wall =
                    date.and_hms_opt((time >> 11) & 0x1F, (time >> 5) & 0x3F, (time & 0x1F) * 2)?;
                let seconds = self.rar.zone.local_to_unix(wall)? + i64::from(add_second);
                std::time::UNIX_EPOCH.checked_add(std::time::Duration::new(
                    u64::try_from(seconds).ok()?,
                    nanos,
                ))
            }
        }
    }

    /// `t`'s test of each RAR 5 volume's recovery record: every chunk of it checks,
    /// whether or not the data it protects is damaged.
    fn test_recovery_records(&mut self) {
        self.say_recovery(self.volumes.len());
    }

    /// `t`: each volume's recovery record, tested before the work starts.
    fn test_recovery(&mut self) {
        self.recovery = self
            .volumes
            .iter()
            .map(|volume| {
                let rar5 = volume.archive.as_rar50()?;
                rar5.main
                    .has_recovery_record()
                    .then(|| rar5.recovery_record_intact(None).unwrap_or(false))
            })
            .collect();
    }

    /// The service headers that failed their checksums in the volumes before `until`,
    /// said as a file's are, two errors each.
    fn say_damaged_services(&mut self, until: usize) {
        while self.services_said < until.min(self.volumes.len()) {
            let names = open::damaged_services(&self.volumes[self.services_said].archive);
            self.services_said += 1;
            for name in names {
                self.rar.console.err(&format!(
                    "\nCorrupt header is found\n{name} - the file header is corrupt"
                ));
                self.errors += 2;
                self.rar.fail(code::CRC);
            }
        }
    }

    /// The recovery records of the volumes left before volume `until`, said as rar
    /// says them when it leaves each.
    fn say_recovery(&mut self, until: usize) {
        self.say_damaged_services(until);
        let (text, failed) = recovery_lines(&self.recovery, self.recovery_said, until);
        self.recovery_said = self.recovery_said.max(until);
        self.say(&text);
        for _ in 0..failed {
            self.error("\nRecovery record is corrupt.", code::CRC);
        }
    }

    /// `t`'s test of a volume set's recovery volumes, `NAME.partN.rev`, each by its own
    /// checksum: RAR 5's in its header, RAR 3's at its end.
    fn test_recovery_volumes(&mut self, new_numbering: bool) {
        if self.volumes.len() < 2 || !new_numbering {
            return;
        }
        let first = self.volumes[0].display.clone();
        let Some(names) = rev_names(&first) else {
            return;
        };
        let found: Vec<String> = names
            .into_iter()
            .take_while(|name| self.rar.path(name).is_file())
            .collect();
        if found.is_empty() {
            return;
        }
        self.say("\n");
        for name in found {
            self.file_line("Testing     ", &name);
            let good = std::fs::read(self.rar.path(&name)).is_ok_and(|bytes| {
                if bytes.starts_with(b"Rar!\x1aRev") {
                    cash_archive::rar::rar50::Rev5Volume::parse(&bytes).is_ok()
                } else {
                    super::reconstruct::trailer(&bytes).is_some()
                }
            });
            if good {
                self.ok();
                self.done += 1;
            } else {
                self.error(&format!("\n{name:<20} - checksum error"), code::CRC);
            }
        }
    }
}

/// An entry's name without the `;N` an older version's ends in.
fn unversioned(entry: &Entry) -> &str {
    entry
        .version
        .and_then(|version| entry.name.strip_suffix(&format!(";{version}")))
        .unwrap_or(&entry.name)
}

/// The recovery volumes' names a set's first volume, `NAME.part1.rar`, gives:
/// `NAME.part1.rev`, `NAME.part2.rev` …, the digits as wide as the volumes'.
fn rev_names(first: &str) -> Option<Vec<String>> {
    let dot = first.rfind('.')?;
    let stem = first.get(..dot)?;
    let digits_end = stem.rfind(|c: char| c.is_ascii_digit())? + 1;
    let digits_start = stem
        .get(..digits_end)?
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |at| at + 1);
    let width = digits_end - digits_start;
    let prefix = first.get(..digits_start)?;
    let between = first.get(digits_end..dot)?;
    Some(
        (1..1000)
            .map(|n| format!("{prefix}{n:0width$}{between}.rev"))
            .collect(),
    )
}

/// The name a renamed file takes under `-or`: `name(1).ext`, `name(2).ext` …
fn free_name(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    (1..u32::MAX)
        .map(|n| dir.join(format!("{stem}({n}){ext}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

/// The name a file's line shows when it is written under another name than its own.
fn path_shown<SE: cash_core::ShellExtensions>(
    shown: &str,
    path: &Path,
    rar: &Rar<'_, SE>,
) -> String {
    if rar.path(shown) == path {
        return shown.to_owned();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match shown.rfind('/') {
        Some(at) => format!("{}/{name}", shown.get(..at).unwrap_or_default()),
        None => name,
    }
}

/// Decoded bytes counted and hashed on their way to the front end's sink.
struct Hashed<'a> {
    out: &'a mut dyn Sink,
    crc: crc32fast::Hasher,
    blake: Option<blake2sp::Hasher>,
    /// RAR 1.3's sum: each byte added, then rotated left one bit.
    sum13: u16,
    written: u64,
    limit: Option<u64>,
}

impl Sink for Hashed<'_> {
    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        let data = match self.limit {
            Some(limit) => {
                let room =
                    usize::try_from(limit.saturating_sub(self.written)).unwrap_or(usize::MAX);
                &data[..data.len().min(room)]
            }
            None => data,
        };
        self.crc.update(data);
        if let Some(blake) = &mut self.blake {
            blake.update(data);
        }
        for &byte in data {
            self.sum13 = self.sum13.wrapping_add(u16::from(byte)).rotate_left(1);
        }
        self.written += data.len() as u64;
        self.out.put(data)
    }
}

/// Bytes kept in memory: a link's target.
struct Collect(Vec<u8>);

impl Sink for Collect {
    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        self.0.extend_from_slice(data);
        Ok(())
    }
}

/// Whether a symbolic link stays inside the destination as rar judges it: a target
/// neither absolute nor going up by `..` more often than the link's own folders.
fn safe_link(name: &str, target: &str) -> bool {
    let absolute = target.starts_with(['\\', '/']) || target.as_bytes().get(1) == Some(&b':');
    let ups = target
        .split(['\\', '/'])
        .filter(|part| *part == "..")
        .count();
    let depth = name.matches('/').count();
    !absolute && ups <= depth
}

/// Bytes tested, and dropped.
struct Discard;

impl Sink for Discard {
    fn put(&mut self, _data: &[u8]) -> io::Result<()> {
        Ok(())
    }
}

/// Bytes written to a file.
struct FileOut {
    file: BufWriter<File>,
}

impl Sink for FileOut {
    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        self.file.write_all(data)
    }
}

/// Bytes printed: `p`.
struct PrintTo<'r, 'a, SE: cash_core::ShellExtensions> {
    rar: &'r Rar<'a, SE>,
}

impl<SE: cash_core::ShellExtensions> Sink for PrintTo<'_, '_, SE> {
    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        self.rar.console.data(data)
    }
}

impl<SE: cash_core::ShellExtensions> Rar<'_, SE> {
    /// A password typed: a line at the console, else all one read of standard input
    /// gives, its line ends trimmed, as rar takes it whole; `None` at the end of the
    /// input.
    pub(super) fn read_password(&self) -> Result<Option<String>, Stop> {
        let stdin = self.context.try_fd(OpenFiles::STDIN_FD);
        if stdin.is_some_and(|file| file.is_terminal()) {
            return self.read_line(false);
        }
        let mut buffer = vec![0u8; 4096];
        let got = self.context.stdin().read(&mut buffer).unwrap_or(0);
        if got == 0 {
            return Ok(None);
        }
        buffer.truncate(got);
        let text = String::from_utf8_lossy(&buffer);
        Ok(Some(text.trim_end_matches(['\r', '\n']).to_owned()))
    }

    /// An answer to a question: a line typed at the console, else the first line of what
    /// one read of standard input gives, as rar reads a pipe a buffer at a time; `None` at
    /// the end of the input.
    pub(super) fn read_answer(&self, shown: bool) -> Result<Option<String>, Stop> {
        let stdin = self.context.try_fd(OpenFiles::STDIN_FD);
        if stdin.is_some_and(|file| file.is_terminal()) {
            return self.read_line(shown);
        }
        let mut buffer = vec![0u8; 4096];
        let got = self.context.stdin().read(&mut buffer).unwrap_or(0);
        if got == 0 {
            return Ok(None);
        }
        buffer.truncate(got);
        let text = String::from_utf8_lossy(&buffer);
        let line = text
            .lines()
            .next()
            .unwrap_or_default()
            .trim_end_matches('\r');
        Ok(Some(line.to_owned()))
    }

    /// What Windows says after rar's "Read error in the file stdin": a pipe's end, or
    /// the error left by a file rar looked for and did not find.
    pub(super) fn stdin_end_words(&self, after_a_search: bool) -> Option<&'static str> {
        let kind = self.context.try_fd(OpenFiles::STDIN_FD).map(|f| f.kind());
        if matches!(kind, Some(FileKind::Pipe { .. })) {
            Some("The pipe has been ended.")
        } else if after_a_search {
            Some("The system cannot find the file specified.")
        } else {
            None
        }
    }
}
