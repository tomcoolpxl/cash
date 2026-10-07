//! `unzip`: `UnZip` 6.00's options, messages and exit statuses.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use cash_archive::zip::read::{self, Archive, DataError, OpenError};
use cash_archive::zip::{Entry, method, unix_like};
use cash_core::ExecutionResult;
use cash_core::openfiles::OpenFiles;
use cash_win32::unix::{self, Replacement};

use super::help::{UNZIP_USAGE, UNZIP_VERSION};
use super::{
    Clock, Say, find_archive, join, matches, name_text, pad, read_line, shown_compressed, status,
    zipinfo,
};
use crate::compress::{is_terminal, strerror};

/// The command line, read as `UnZip` reads it.
#[derive(Debug, Default)]
struct Options {
    list: bool,
    verbose: bool,
    test: bool,
    pipe: bool,
    cat: bool,
    comment: bool,
    timestamp: bool,
    freshen: bool,
    update: bool,
    never: bool,
    overwrite: bool,
    junk: bool,
    casefold: bool,
    lowercase: bool,
    quiet: u8,
    exdir: Option<String>,
    password: Option<Vec<u8>>,
    stop_at_slash: bool,
    no_times: u8,
    dotdot: bool,
    help: u8,
    zipfile: Option<String>,
    names: Vec<String>,
    excludes: Vec<String>,
}

/// What reading the command line came to, when it is not a run.
enum Stop {
    /// The usage, with this status: 0 asked for, 10 for a mistake.
    Usage(u8),
    /// A message and the status 10.
    Message(String),
}

/// Reads the options, the archive's name, its names and `-x` and `-d`.
fn parse(words: &[String]) -> Result<Options, Stop> {
    let mut options = Options::default();
    let mut i = 0;
    while let Some(word) = words.get(i) {
        let Some(letters) = word.strip_prefix('-') else {
            break;
        };
        if letters.is_empty() {
            return Err(Stop::Usage(status::PARAM));
        }
        let chars: Vec<char> = letters.chars().collect();
        let mut negative = false;
        let mut k = 0;
        while let Some(&c) = chars.get(k) {
            let on = !negative;
            match c {
                '-' => negative = !negative,
                'a' | 'b' | 'B' | 'K' | 'M' | 's' | 'U' | 'V' | 'X' | '^' => {}
                'c' => options.cat = on,
                'C' => options.casefold = on,
                'D' => options.no_times = if on { options.no_times + 1 } else { 0 },
                'f' => options.freshen = on,
                'h' => options.help += 1,
                'j' => options.junk = on,
                'l' => options.list = on,
                'L' => options.lowercase = on,
                'n' => options.never = on,
                'o' => options.overwrite = on,
                'p' => options.pipe = on,
                'q' => options.quiet = if on { options.quiet + 1 } else { 0 },
                't' => options.test = on,
                'T' => options.timestamp = on,
                'u' => options.update = on,
                'v' => options.verbose = on,
                'W' => options.stop_at_slash = on,
                'x' => {}
                'z' => options.comment = on,
                ':' => options.dotdot = on,
                'd' | 'P' | 'I' | 'O' => {
                    let rest: String = chars.get(k + 1..).unwrap_or_default().iter().collect();
                    let value = if rest.is_empty() {
                        i += 1;
                        match words.get(i) {
                            Some(next) => next.clone(),
                            None if c == 'd' => {
                                return Err(Stop::Message(
                                    "error:  must specify directory to which to extract with -d option\n"
                                        .to_owned(),
                                ));
                            }
                            None => return Err(Stop::Usage(status::PARAM)),
                        }
                    } else {
                        rest
                    };
                    match c {
                        'd' => options.exdir = Some(value),
                        'P' => options.password = Some(value.into_bytes()),
                        _ => {}
                    }
                    break;
                }
                _ => return Err(Stop::Usage(status::PARAM)),
            }
            k += 1;
        }
        i += 1;
    }
    options.zipfile = words.get(i).cloned();
    let mut excluding = false;
    let mut rest = words.iter().skip(i + 1);
    while let Some(word) = rest.next() {
        if word == "-x" {
            excluding = true;
        } else if word == "-d" {
            match rest.next() {
                Some(dir) => options.exdir = Some(dir.clone()),
                None => {
                    return Err(Stop::Message(
                        "error:  must specify directory to which to extract with -d option\n"
                            .to_owned(),
                    ));
                }
            }
        } else if let Some(dir) = word.strip_prefix("-d").filter(|d| !d.is_empty()) {
            options.exdir = Some(dir.to_owned());
        } else if excluding {
            options.excludes.push(word.clone());
        } else {
            options.names.push(word.clone());
        }
    }
    Ok(options)
}

/// The words an exported variable holds, as options read before the command line.
fn environment_words<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    names: &[&str],
) -> Vec<String> {
    names
        .iter()
        .find_map(|name| {
            context
                .shell
                .env()
                .get(name)
                .filter(|(_, var)| var.is_exported())
                .map(|(_, var)| var.value().to_cow_str(context.shell).into_owned())
        })
        .map(|value| value.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// The command, run.
pub(super) fn run<SE: cash_core::ShellExtensions>(
    args: &[String],
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Result<ExecutionResult, cash_core::Error> {
    if let Some(first) = args.first().and_then(|a| a.strip_prefix("-Z")) {
        let mut rest: Vec<String> = Vec::new();
        if !first.is_empty() {
            rest.push(format!("-{first}"));
        }
        rest.extend(args.iter().skip(1).cloned());
        return zipinfo::run_as(&rest, context, "unzip");
    }
    let say = Say { context };
    if args.is_empty() {
        say.out(UNZIP_USAGE)?;
        return Ok(ExecutionResult::success());
    }
    let mut words = environment_words(context, &["UNZIP", "UNZIPOPT"]);
    words.extend(args.iter().cloned());
    let options = match parse(&words) {
        Ok(options) => options,
        Err(Stop::Usage(code)) => {
            if code == 0 {
                say.out(UNZIP_USAGE)?;
            } else {
                say.err(UNZIP_USAGE)?;
            }
            return Ok(ExecutionResult::new(code));
        }
        Err(Stop::Message(text)) => {
            say.err(&text)?;
            return Ok(ExecutionResult::new(status::PARAM));
        }
    };
    let Some(zipfile) = options.zipfile.clone() else {
        if options.verbose {
            say.out(UNZIP_VERSION)?;
            return Ok(ExecutionResult::success());
        }
        if options.help > 0 {
            say.out(UNZIP_USAGE)?;
            return Ok(ExecutionResult::success());
        }
        say.err(UNZIP_USAGE)?;
        return Ok(ExecutionResult::new(status::PARAM));
    };
    let mut unzip = Unzip {
        say,
        context,
        clock: Clock::of(context),
        options,
        status: 0,
        shown: zipfile.clone(),
        remembered_password: None,
        prompt_answers: Answers::Ask,
    };
    unzip.run(&zipfile)
}

/// The answer to the overwrite question that holds for the rest of the run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Answers {
    Ask,
    All,
    None,
}

/// A symbolic link to make once everything else is out.
struct Link {
    shown: String,
    path: PathBuf,
    target: String,
}

/// A run over one archive.
struct Unzip<'a, SE: cash_core::ShellExtensions> {
    say: Say<'a, SE>,
    context: &'a cash_core::ExecutionContext<'a, SE>,
    clock: Clock,
    options: Options,
    status: u8,
    /// The archive's name as found.
    shown: String,
    remembered_password: Option<Vec<u8>>,
    prompt_answers: Answers,
}

/// The message `UnZip` gives for a file that is not a zip archive.
pub(super) const NO_END: &str =
    "  End-of-central-directory signature not found.  Either this file is not
  a zipfile, or it constitutes one disk of a multi-part archive.  In the
  latter case the central directory and zipfile comment will be found on
  the last disk(s) of this archive.
";

/// What a member's data came to.
enum Outcome {
    Ok,
    BadCrc(u32),
    Broken,
    Skipped(&'static str),
    BadLocal,
}

impl<SE: cash_core::ShellExtensions> Unzip<'_, SE> {
    fn raise(&mut self, code: u8) {
        self.status = self.status.max(code);
    }

    fn run(&mut self, zipfile: &str) -> Result<ExecutionResult, cash_core::Error> {
        let Some((shown, path)) = find_archive(self.context, zipfile) else {
            self.say.err(&format!(
                "unzip:  cannot find or open {zipfile}, {zipfile}.zip or {zipfile}.ZIP.\n"
            ))?;
            return Ok(ExecutionResult::new(status::NOZIP));
        };
        self.shown = shown;
        if self.options.never && self.options.overwrite {
            self.say
                .err("caution:  both -n and -o specified; ignoring -o\n")?;
            self.options.overwrite = false;
        }
        let listing =
            self.options.list || self.options.verbose || self.options.test || self.options.comment;
        if listing && self.options.exdir.is_some() {
            self.say.err("caution:  not extracting; -d ignored\n")?;
        }
        let Ok(mut opened) = super::open_archive(&path) else {
            self.say.err(&format!(
                "unzip:  cannot find or open {zipfile}, {zipfile}.zip or {zipfile}.ZIP.\n"
            ))?;
            return Ok(ExecutionResult::new(status::NOZIP));
        };
        let file = &mut opened.file;
        let quiet_archive_line = self.options.pipe
            || (self.options.quiet > 0 && !self.options.comment)
            || (self.options.timestamp && self.options.quiet > 0);
        let archive = match read::open(file) {
            Ok(archive) => archive,
            Err(OpenError::NoEnd) => {
                if !self.options.pipe {
                    self.say.out(&format!("Archive:  {}\n", self.shown))?;
                }
                self.say.err(NO_END)?;
                self.say.err(&format!(
                    "unzip:  cannot find zipfile directory in one of {zipfile} or\n        {zipfile}.zip, and cannot find {zipfile}.ZIP, period.\n"
                ))?;
                return Ok(ExecutionResult::new(status::NOZIP));
            }
            Err(OpenError::BadCentral(number)) => {
                if !quiet_archive_line {
                    self.say.out(&format!("Archive:  {}\n", self.shown))?;
                }
                self.say.err(&format!(
                    "error [{}]:  expected central file header signature not found (file #{number}).\n  (please check that you have transferred or created the zipfile in the\n  appropriate BINARY mode and that you have compiled UnZip properly)\n",
                    self.shown
                ))?;
                return Ok(ExecutionResult::new(status::BADERR));
            }
            Err(OpenError::Io(e)) => return Err(e.into()),
        };
        if self.options.timestamp {
            return self.timestamp(&path, &archive);
        }
        if !quiet_archive_line {
            self.say.out(&format!("Archive:  {}\n", self.shown))?;
            if !archive.end.comment.is_empty() && !self.options.comment {
                self.show_comment(&archive.end.comment)?;
            }
        }
        self.extra_bytes_warning(&archive)?;
        if self.options.comment {
            if !archive.end.comment.is_empty() {
                self.show_comment(&archive.end.comment)?;
            }
            return Ok(ExecutionResult::new(self.status));
        }
        if archive.entries.is_empty() {
            self.say
                .err(&format!("warning [{}]:  zipfile is empty\n", self.shown))?;
            self.raise(status::WARN);
            return Ok(ExecutionResult::new(self.status));
        }
        let (selected, unmatched, unexcluded) = self.select(&archive);
        if self.options.list || self.options.verbose {
            self.list(&archive, &selected)?;
            if !self.options.names.is_empty() && selected.is_empty() {
                self.raise(status::FIND);
            }
            return Ok(ExecutionResult::new(self.status));
        }
        if self.options.test {
            self.test(file, &archive, &selected, &unmatched, &unexcluded)?;
        } else {
            self.extract(file, &archive, &selected)?;
            self.cautions(&unmatched, &unexcluded)?;
        }
        Ok(ExecutionResult::new(self.status))
    }

    /// The names and exclusions that matched nothing.
    fn cautions(&mut self, unmatched: &[String], unexcluded: &[String]) -> io::Result<()> {
        for name in unmatched {
            self.say
                .err(&format!("caution: filename not matched:  {name}\n"))?;
            self.raise(status::FIND);
        }
        for name in unexcluded {
            self.say.err(&format!(
                "caution: excluded filename not matched:  {name}\n"
            ))?;
        }
        Ok(())
    }

    fn show_comment(&self, comment: &[u8]) -> io::Result<()> {
        let mut text = String::from_utf8_lossy(comment).replace("\r\n", "\n");
        if !text.ends_with('\n') {
            text.push('\n');
        }
        self.say.out(&text)
    }

    fn extra_bytes_warning(&mut self, archive: &Archive) -> io::Result<()> {
        if archive.extra_bytes > 0 {
            self.say.err(&format!(
                "warning [{}]:  {} extra bytes at beginning or within zipfile\n  (attempting to process anyway)\n",
                self.shown, archive.extra_bytes
            ))?;
            self.raise(status::WARN);
        } else if archive.extra_bytes < 0 {
            self.say.err(&format!(
                "error [{}]:  missing {} bytes in zipfile\n  (attempting to process anyway)\n",
                self.shown, -archive.extra_bytes
            ))?;
            self.raise(status::ERR);
        }
        Ok(())
    }

    /// The members the names select and the exclusions leave, with the names and
    /// exclusions that matched nothing.
    fn select(&self, archive: &Archive) -> (Vec<usize>, Vec<String>, Vec<String>) {
        let mut name_hits = vec![false; self.options.names.len()];
        let mut exclude_hits = vec![false; self.options.excludes.len()];
        let mut selected = Vec::new();
        for (index, entry) in archive.entries.iter().enumerate() {
            let name = name_text(entry);
            let wanted = if self.options.names.is_empty() {
                true
            } else {
                let mut any = false;
                for (hit, pattern) in name_hits.iter_mut().zip(&self.options.names) {
                    if matches(
                        pattern,
                        &name,
                        self.options.casefold,
                        self.options.stop_at_slash,
                    ) {
                        *hit = true;
                        any = true;
                    }
                }
                any
            };
            if !wanted {
                continue;
            }
            let mut excluded = false;
            for (hit, pattern) in exclude_hits.iter_mut().zip(&self.options.excludes) {
                if matches(
                    pattern,
                    &name,
                    self.options.casefold,
                    self.options.stop_at_slash,
                ) {
                    *hit = true;
                    excluded = true;
                }
            }
            if !excluded {
                selected.push(index);
            }
        }
        let unmatched = name_hits
            .iter()
            .zip(&self.options.names)
            .filter(|(hit, _)| !**hit)
            .map(|(_, name)| name.clone())
            .collect();
        let unexcluded = exclude_hits
            .iter()
            .zip(&self.options.excludes)
            .filter(|(hit, _)| !**hit)
            .map(|(_, name)| name.clone())
            .collect();
        (selected, unmatched, unexcluded)
    }

    fn date_time(&self, entry: &Entry) -> (String, String) {
        let c = self.clock.civil(self.clock.entry_time(entry));
        (
            format!("{:04}-{:02}-{:02}", c.year, c.month, c.day),
            format!("{:02}:{:02}", c.hour, c.minute),
        )
    }

    /// `-l` and `-v`.
    fn list(&self, archive: &Archive, selected: &[usize]) -> io::Result<()> {
        let headers = self.options.quiet < 2;
        let mut total_size = 0_u64;
        let mut total_compressed = 0_u64;
        let mut out = String::new();
        if self.options.verbose {
            if headers {
                out.push_str(" Length   Method    Size  Cmpr    Date    Time   CRC-32   Name\n");
                out.push_str("--------  ------  ------- ---- ---------- ----- --------  ----\n");
            }
        } else if headers {
            out.push_str("  Length      Date    Time    Name\n");
            out.push_str("---------  ---------- -----   ----\n");
        }
        for &index in selected {
            let Some(entry) = archive.entries.get(index) else {
                continue;
            };
            let (date, time) = self.date_time(entry);
            let name = name_text(entry);
            let compressed = shown_compressed(entry);
            total_size += entry.size;
            total_compressed += compressed;
            if self.options.verbose {
                let _ = writeln!(
                    out,
                    "{:>8}  {:<6} {compressed:>8} {:>3}% {date} {time} {:08x}  {name}",
                    entry.size,
                    method_name(entry),
                    percent(entry.size, compressed),
                    entry.crc
                );
            } else {
                let _ = writeln!(out, "{:>9}  {date} {time}   {name}", entry.size);
            }
        }
        if headers {
            let count = selected.len();
            let files = format!("{count} file{}", if count == 1 { "" } else { "s" });
            if self.options.verbose {
                out.push_str("--------          -------  ---                            -------\n");
                let _ = writeln!(
                    out,
                    "{total_size:>8}         {total_compressed:>8} {:>3}%                            {files}",
                    percent(total_size, total_compressed)
                );
            } else {
                out.push_str("---------                     -------\n");
                let _ = writeln!(out, "{total_size:>9}                     {files}");
            }
        }
        self.say.out(&out)
    }

    /// The password for an encrypted member: `-P`'s, the last that worked, or asked for
    /// at the console. `None` when there is none to be had.
    fn password_for(&self, name: &str, wrong_before: bool) -> io::Result<Option<Vec<u8>>> {
        if let Some(password) = &self.options.password {
            return Ok((!wrong_before).then(|| password.clone()));
        }
        if !wrong_before {
            if let Some(password) = &self.remembered_password {
                return Ok(Some(password.clone()));
            }
        }
        let console = is_terminal(self.context, OpenFiles::STDIN_FD);
        if !console {
            return Ok(None);
        }
        let prompt = if wrong_before {
            "password incorrect--reenter: ".to_owned()
        } else {
            format!("[{}] {name} password: ", self.shown)
        };
        self.say.err(&prompt)?;
        let answer = read_line(self.context, false)?;
        self.say.err("\n")?;
        Ok(answer.map(String::into_bytes))
    }

    /// Reads a member's data into `sink`, checking its CRC; `line` is said once the
    /// data can be read.
    fn data(
        &mut self,
        file: &mut fs::File,
        archive: &Archive,
        entry: &Entry,
        sink: &mut dyn Write,
        line: Option<&str>,
    ) -> io::Result<Outcome> {
        let local = match read::local(file, archive, entry) {
            Ok(local) => local,
            Err(DataError::Io(e)) => return Err(e),
            Err(_) => return Ok(Outcome::BadLocal),
        };
        let name = name_text(entry);
        let mut attempt = 0;
        loop {
            let password = if entry.is_encrypted() {
                match self.password_for(&name, attempt > 0)? {
                    Some(password) => Some(password),
                    None if attempt > 0 => return Ok(Outcome::Skipped("incorrect password")),
                    None => return Ok(Outcome::Skipped("unable to get password")),
                }
            } else {
                None
            };
            file.seek(SeekFrom::Start(local.data_offset))?;
            let raw = (&mut *file).take(entry.compressed_size);
            let reader = match read::data(raw, entry, password.as_deref()) {
                Ok(reader) => reader,
                Err(DataError::BadPassword) => {
                    attempt += 1;
                    if attempt >= 2 || self.options.password.is_some() {
                        return Ok(Outcome::Skipped("incorrect password"));
                    }
                    continue;
                }
                Err(DataError::Unsupported(_)) => return Ok(Outcome::Skipped("unsupported")),
                Err(DataError::NeedPassword) => {
                    return Ok(Outcome::Skipped("unable to get password"));
                }
                Err(DataError::BadLocal(_)) => return Ok(Outcome::BadLocal),
                Err(DataError::Io(_)) => return Ok(Outcome::Broken),
            };
            if entry.is_encrypted() {
                self.remembered_password = password;
            }
            if let Some(line) = line {
                self.say.out(line)?;
            }
            let mut crc = crc32fast::Hasher::new();
            let mut reader = reader.take(entry.size);
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                let n = match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => return Ok(Outcome::Broken),
                };
                let chunk = buffer.get(..n).unwrap_or_default();
                crc.update(chunk);
                sink.write_all(chunk)?;
            }
            let crc = crc.finalize();
            return Ok(if crc == entry.crc {
                Outcome::Ok
            } else {
                Outcome::BadCrc(crc)
            });
        }
    }

    /// The words for a member that was skipped.
    fn skipped(&mut self, entry: &Entry, why: &str) -> io::Result<()> {
        let name = name_text(entry);
        let text = if why == "unsupported" {
            match method_word(entry.method) {
                Some(word) => format!("`{word}' method not supported"),
                None => format!("unsupported compression method {}", entry.method),
            }
        } else {
            why.to_owned()
        };
        self.say
            .err(&format!("   skipping: {}  {text}\n", pad(&name, 22)))?;
        self.raise(match why {
            "unsupported" => status::UNSUPPORTED,
            "unable to get password" => 5,
            _ => status::WARN,
        });
        Ok(())
    }

    /// `-t`.
    fn test(
        &mut self,
        file: &mut fs::File,
        archive: &Archive,
        selected: &[usize],
        unmatched: &[String],
        unexcluded: &[String],
    ) -> io::Result<()> {
        let mut errors = false;
        let mut tested = 0;
        let mut bad_passwords = 0;
        for &index in selected {
            let Some(entry) = archive.entries.get(index).cloned() else {
                continue;
            };
            let name = name_text(&entry);
            let line = format!("    testing: {}  ", pad(&name, 22));
            if entry.is_dir() {
                if self.options.quiet == 0 {
                    self.say.out(&format!("{line} OK\n"))?;
                }
                tested += 1;
                continue;
            }
            let shown_line = (self.options.quiet == 0).then_some(line.as_str());
            let outcome = self.data(file, archive, &entry, &mut io::sink(), shown_line)?;
            match outcome {
                Outcome::Ok => {
                    tested += 1;
                    if self.options.quiet == 0 {
                        self.say.out(" OK\n")?;
                    }
                }
                Outcome::BadCrc(crc) => {
                    tested += 1;
                    errors = true;
                    self.raise(status::ERR);
                    if self.options.quiet > 0 {
                        self.say.out(&format!("{} ", pad(&name, 22)))?;
                    }
                    self.say.err(&format!(
                        " bad CRC {crc:08x}  (should be {:08x})\n",
                        entry.crc
                    ))?;
                }
                Outcome::Broken => {
                    tested += 1;
                    errors = true;
                    self.raise(status::ERR);
                    if self.options.quiet == 0 {
                        self.say.out("\n")?;
                    }
                    self.say.err(&format!(
                        "  error:  invalid compressed data to {}\n",
                        method_verb(entry.method)
                    ))?;
                }
                Outcome::BadLocal => {
                    errors = true;
                    self.raise(status::ERR);
                    self.say.err(&format!(
                        "file #{}:  bad zipfile offset (local header sig):  {}\n",
                        index + 1,
                        entry.local_offset
                    ))?;
                }
                Outcome::Skipped(why) => {
                    if why == "incorrect password" {
                        bad_passwords += 1;
                    } else {
                        errors = true;
                    }
                    self.skipped(&entry, why)?;
                }
            }
        }
        self.cautions(unmatched, unexcluded)?;
        if self.options.quiet < 2 {
            if tested == 0 && bad_passwords > 0 {
                self.say
                    .out(&format!("Caution:  zero files tested in {}.\n", self.shown))?;
            } else if errors || self.status >= status::ERR {
                self.say.out(&format!(
                    "At least one error was detected in {}.\n",
                    self.shown
                ))?;
            } else {
                self.say.out(&format!(
                    "No errors detected in compressed data of {}.\n",
                    self.shown
                ))?;
            }
            if bad_passwords > 0 {
                self.say.out(&format!(
                    "{bad_passwords} file{} skipped because of incorrect password.\n",
                    if bad_passwords == 1 { "" } else { "s" }
                ))?;
            }
        }
        if tested == 0 && bad_passwords > 0 {
            self.raise(status::BAD_PASSWORD);
        }
        Ok(())
    }

    /// A member's name made safe to write, with the warnings it took.
    fn safe_name(&mut self, raw: &str) -> io::Result<String> {
        let mut name = raw.replace('\\', "/");
        let mut absolute = false;
        let bytes = name.as_bytes();
        if bytes.len() >= 2
            && bytes.get(1) == Some(&b':')
            && bytes.first().is_some_and(u8::is_ascii_alphabetic)
        {
            name = name.chars().skip(2).collect();
            absolute = true;
        }
        if name.starts_with('/') {
            name = name.trim_start_matches('/').to_owned();
            absolute = true;
        }
        if absolute {
            self.say.err(&format!(
                "warning:  stripped absolute path spec from {raw}\n"
            ))?;
            self.raise(status::WARN);
        }
        let trailing = name.ends_with('/');
        let mut parts: Vec<&str> = Vec::new();
        let mut dotdot = false;
        for part in name.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." && !self.options.dotdot {
                dotdot = true;
                continue;
            }
            parts.push(part);
        }
        if dotdot {
            self.say.err(&format!(
                "warning:  skipped \"../\" path component(s) in {raw}\n"
            ))?;
            self.raise(status::WARN);
        }
        let mut safe = parts.join("/");
        if self.options.junk {
            safe = parts.last().map(|p| (*p).to_owned()).unwrap_or_default();
        } else if trailing && !safe.is_empty() {
            safe.push('/');
        }
        if self.options.lowercase && !safe.chars().any(char::is_lowercase) {
            safe = safe.to_lowercase();
        }
        Ok(safe)
    }

    /// Whether an existing file is to be replaced: `-n`, `-o`, or the question.
    fn may_replace(&mut self, shown: &str) -> io::Result<bool> {
        if self.options.never || self.prompt_answers == Answers::None {
            return Ok(false);
        }
        if self.options.overwrite || self.prompt_answers == Answers::All {
            return Ok(true);
        }
        loop {
            self.say.err(&format!(
                "replace {shown}? [y]es, [n]o, [A]ll, [N]one, [r]ename: "
            ))?;
            let Some(answer) = read_line(self.context, true)? else {
                self.say
                    .err(" NULL\n(EOF or read error, treating as \"[N]one\" ...)\n")?;
                self.prompt_answers = Answers::None;
                self.raise(status::WARN);
                return Ok(false);
            };
            match answer.chars().next() {
                Some('y' | 'Y') => return Ok(true),
                Some('n') => return Ok(false),
                Some('A') => {
                    self.prompt_answers = Answers::All;
                    return Ok(true);
                }
                Some('N') => {
                    self.prompt_answers = Answers::None;
                    return Ok(false);
                }
                Some(other) => self
                    .say
                    .err(&format!("error:  invalid response [{other}]\n"))?,
                None => self.say.err("error:  invalid response []\n")?,
            }
        }
    }

    /// Extraction, `-p` and `-c`.
    #[expect(
        clippy::too_many_lines,
        reason = "UnZip's extract_or_test_member: the name, the checks, the data, the times"
    )]
    fn extract(
        &mut self,
        file: &mut fs::File,
        archive: &Archive,
        selected: &[usize],
    ) -> io::Result<()> {
        let to_stdout = self.options.pipe || self.options.cat;
        let quiet = self.options.quiet > 0 || self.options.pipe;
        let base = self.options.exdir.as_deref().map_or_else(
            || self.context.shell.absolute_path("."),
            |d| self.context.shell.absolute_path(d),
        );
        let prefix = self
            .options
            .exdir
            .as_deref()
            .map(|d| format!("{}/", d.trim_end_matches('/')))
            .unwrap_or_default();
        if let Some(exdir) = &self.options.exdir {
            if !to_stdout && !base.is_dir() {
                if let Err(e) = fs::create_dir(&base) {
                    self.say.err(&format!(
                        "checkdir:  cannot create extraction directory: {exdir}\n           {}\n",
                        strerror(&e)
                    ))?;
                    self.raise(status::ERR);
                    return Ok(());
                }
            }
        }
        let mut folders: Vec<(PathBuf, i64)> = Vec::new();
        let mut links: Vec<Link> = Vec::new();
        let mut bad_passwords = 0;
        let mut done = 0;
        for &index in selected {
            let Some(entry) = archive.entries.get(index).cloned() else {
                continue;
            };
            let raw_name = name_text(&entry);
            if to_stdout {
                if entry.is_dir() {
                    continue;
                }
                let mut data = Vec::new();
                let line = format!("{}: {}  \n", verb(entry.method), pad(&raw_name, 22));
                let shown_line = (self.options.cat && !quiet).then_some(line.as_str());
                let outcome = self.data(file, archive, &entry, &mut data, shown_line)?;
                {
                    let mut out = self.context.stdout();
                    out.write_all(&data)?;
                    if self.options.cat {
                        out.write_all(b"\n")?;
                    }
                    out.flush()?;
                }
                self.report(
                    index,
                    &entry,
                    &raw_name,
                    &outcome,
                    true,
                    &mut bad_passwords,
                    &mut done,
                )?;
                continue;
            }
            let name = self.safe_name(&raw_name)?;
            if name.is_empty() || (self.options.junk && entry.is_dir()) {
                continue;
            }
            let shown = format!("{prefix}{name}");
            let path = join(&base, &name);
            let holdable = name
                .split('/')
                .filter(|p| !p.is_empty())
                .all(|p| unix::check_name(p).is_ok());
            if !holdable {
                self.say.err(&format!(
                    "error:  cannot create {shown}\n        Invalid argument\n"
                ))?;
                self.raise(status::DISK);
                continue;
            }
            let time = self.clock.entry_time(&entry);
            if entry.is_dir() {
                if !path.is_dir() {
                    if let Err(e) = fs::create_dir_all(&path) {
                        self.say.err(&format!(
                            "checkdir error:  cannot create {shown}\n                 {}\n                 unable to process {raw_name}.\n",
                            strerror(&e)
                        ))?;
                        self.raise(status::ERR);
                        continue;
                    }
                    if !quiet {
                        self.say.out(&format!("   creating: {shown}\n"))?;
                    }
                }
                folders.push((path, time));
                done += 1;
                continue;
            }
            let existing = fs::symlink_metadata(&path).ok();
            if let Some(existing) = &existing {
                if self.options.freshen || self.options.update {
                    let on_disk = existing
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
                    if on_disk >= time {
                        continue;
                    }
                }
                if existing.is_dir() {
                    continue;
                }
                if !self.may_replace(&shown)? {
                    continue;
                }
            } else if self.options.freshen {
                continue;
            }
            if let Some(parent) = path.parent() {
                if !parent.is_dir() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            if entry.is_symlink() {
                let mut target = Vec::new();
                let outcome = self.data(file, archive, &entry, &mut target, None)?;
                if !matches!(outcome, Outcome::Ok) {
                    self.report(
                        index,
                        &entry,
                        &raw_name,
                        &outcome,
                        false,
                        &mut bad_passwords,
                        &mut done,
                    )?;
                    continue;
                }
                let target = String::from_utf8_lossy(&target).into_owned();
                if !quiet {
                    self.say
                        .out(&format!("    linking: {}  -> {target} \n", pad(&shown, 22)))?;
                }
                links.push(Link {
                    shown,
                    path,
                    target,
                });
                done += 1;
                continue;
            }
            let line = format!("{}: {}  ", verb(entry.method), pad(&shown, 22));
            let mut output = match Replacement::create(path.clone()) {
                Ok(output) => output,
                Err(e) => {
                    self.say.err(&format!(
                        "error:  cannot create {shown}\n        {}\n",
                        strerror(&e)
                    ))?;
                    self.raise(status::DISK);
                    continue;
                }
            };
            let shown_line = (!quiet).then_some(line.as_str());
            let outcome = self.data(file, archive, &entry, &mut output, shown_line)?;
            let keep = matches!(outcome, Outcome::Ok | Outcome::BadCrc(_) | Outcome::Broken);
            if keep {
                let times = Self::file_times(file, archive, &entry, time);
                let read_only = read_only(&entry);
                if let Err(e) = output.finish(times, read_only, false) {
                    self.say.err(&format!(
                        "error:  cannot create {shown}\n        {}\n",
                        strerror(&e)
                    ))?;
                    self.raise(status::DISK);
                    continue;
                }
            } else {
                output.abandon();
            }
            if !quiet && matches!(outcome, Outcome::Ok) {
                self.say.out("\n")?;
            }
            self.report(
                index,
                &entry,
                &raw_name,
                &outcome,
                quiet,
                &mut bad_passwords,
                &mut done,
            )?;
        }
        if !links.is_empty() {
            if !quiet {
                self.say.out("finishing deferred symbolic links:\n")?;
            }
            for link in links {
                if !quiet {
                    self.say
                        .out(&format!("  {} -> {}\n", pad(&link.shown, 22), link.target))?;
                }
                make_link(&link.path, &link.target);
            }
        }
        if self.options.no_times == 0 {
            for (path, time) in folders.iter().rev() {
                let modified = system_time(*time);
                let _ = unix::set_times(
                    path,
                    &unix::Times {
                        modified,
                        accessed: Some(modified),
                        created: None,
                    },
                );
            }
        }
        if done == 0 && bad_passwords > 0 {
            self.raise(status::BAD_PASSWORD);
        }
        Ok(())
    }

    /// What a member's data came to, said after its line.
    #[expect(
        clippy::too_many_arguments,
        reason = "the member, its outcome and the counts"
    )]
    fn report(
        &mut self,
        index: usize,
        entry: &Entry,
        name: &str,
        outcome: &Outcome,
        quiet: bool,
        bad_passwords: &mut u32,
        done: &mut u32,
    ) -> io::Result<()> {
        match *outcome {
            Outcome::Ok => *done += 1,
            Outcome::BadCrc(crc) => {
                *done += 1;
                self.raise(status::ERR);
                if quiet {
                    self.say.err(&format!("{} ", pad(name, 22)))?;
                }
                self.say.err(&format!(
                    " bad CRC {crc:08x}  (should be {:08x})\n",
                    entry.crc
                ))?;
            }
            Outcome::Broken => {
                self.raise(status::ERR);
                if !quiet {
                    self.say.out("\n")?;
                }
                self.say.err(&format!(
                    "  error:  invalid compressed data to {}\n",
                    method_verb(entry.method)
                ))?;
            }
            Outcome::BadLocal => {
                self.raise(status::ERR);
                self.say.err(&format!(
                    "file #{}:  bad zipfile offset (local header sig):  {}\n",
                    index + 1,
                    entry.local_offset
                ))?;
            }
            Outcome::Skipped(why) => {
                if why == "incorrect password" {
                    *bad_passwords += 1;
                }
                self.skipped(entry, why)?;
            }
        }
        Ok(())
    }

    /// The times to give an extracted file: the local `UT` field's when it has them.
    fn file_times(
        file: &mut fs::File,
        archive: &Archive,
        entry: &Entry,
        time: i64,
    ) -> fs::FileTimes {
        let local_times = read::local(file, archive, entry)
            .ok()
            .and_then(|local| cash_archive::zip::times(&local.extra));
        let modified = local_times.and_then(|t| t.modified).unwrap_or(time);
        let accessed = local_times.and_then(|t| t.accessed).unwrap_or(modified);
        fs::FileTimes::new()
            .set_modified(system_time(modified))
            .set_accessed(system_time(accessed))
    }

    /// `-T`: the archive's time made its newest member's.
    fn timestamp(
        &self,
        path: &Path,
        archive: &Archive,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let newest = archive
            .entries
            .iter()
            .map(|e| self.clock.entry_time(e))
            .max()
            .unwrap_or(0);
        let modified = system_time(newest);
        let _ = unix::set_times(
            path,
            &unix::Times {
                modified,
                accessed: Some(modified),
                created: None,
            },
        );
        self.say
            .out(&format!("Updated time stamp for {}.\n", self.shown))?;
        Ok(ExecutionResult::new(self.status))
    }
}

/// Whether an extracted file is to be read-only: no write bit for its owner, or MS-DOS's
/// read-only attribute.
fn read_only(entry: &Entry) -> bool {
    if unix_like(entry.host()) {
        if let Some(mode) = entry.unix_mode() {
            return mode & 0o200 == 0;
        }
    }
    entry.dos_attributes() & cash_archive::zip::DOS_READ_ONLY != 0
}

/// A moment as `std` keeps one.
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

/// A symbolic link made where Windows allows one, else a file that holds its target.
fn make_link(path: &Path, target: &str) {
    if fs::symlink_metadata(path).is_ok() {
        let _ = unix::remove_even_read_only(path);
    }
    let resolved = path
        .parent()
        .map_or_else(|| PathBuf::from(target), |p| p.join(target));
    let kind = if resolved.is_dir() {
        unix::LinkKind::Dir
    } else {
        unix::LinkKind::File
    };
    let windows_target = target.replace('/', "\\");
    if unix::symlink(Path::new(&windows_target), path, kind).is_err() {
        let _ = fs::write(path, target.as_bytes());
    }
}

/// `UnZip`'s name for a method in `-v`'s listing.
pub(super) fn method_name(entry: &Entry) -> String {
    let deflate_kind = match (entry.flags >> 1) & 3 {
        0 => 'N',
        1 => 'X',
        2 => 'F',
        _ => 'S',
    };
    match entry.method {
        method::STORED => "Stored".to_owned(),
        method::SHRUNK => "Shrunk".to_owned(),
        2..=5 => format!("Reduce{}", entry.method - 1),
        method::IMPLODED => "Implode".to_owned(),
        7 => "Token".to_owned(),
        method::DEFLATED => format!("Defl:{deflate_kind}"),
        method::DEFLATE64 => format!("Def64{deflate_kind}"),
        10 => "ImplDCL".to_owned(),
        method::BZIP2 => "BZip2".to_owned(),
        method::LZMA => "LZMA".to_owned(),
        18 => "Terse".to_owned(),
        19 => "IBMLZ77".to_owned(),
        97 => "WavPack".to_owned(),
        method::PPMD => "PPMd".to_owned(),
        other => format!("Unk:{other:03}"),
    }
}

/// The word of an extraction's line for a method.
const fn verb(method: u16) -> &'static str {
    match method {
        method::DEFLATED | method::DEFLATE64 => "  inflating",
        method::BZIP2 => " bunzipping",
        _ => " extracting",
    }
}

/// `UnZip`'s word for a method it does not have.
const fn method_word(method: u16) -> Option<&'static str> {
    Some(match method {
        1 => "Shrink",
        2..=5 => "Reduce",
        6 => "Implode",
        7 => "Tokenize",
        10 => "ImplDCL",
        18 => "Terse",
        19 => "IBMLZ77",
        97 => "WavPack",
        98 => "PPMd",
        _ => return None,
    })
}

/// The verb of "invalid compressed data to …".
const fn method_verb(method: u16) -> &'static str {
    match method {
        method::DEFLATED | method::DEFLATE64 => "inflate",
        method::BZIP2 => "bunzip",
        _ => "extract",
    }
}

/// `UnZip`'s ratio, rounded to a whole percent: `-v`'s "Cmpr".
pub(super) fn percent(size: u64, compressed: u64) -> i64 {
    let per_mille = super::zipinfo::ratio(size, compressed);
    if per_mille < 0 {
        -((-per_mille + 5) / 10)
    } else {
        (per_mille + 5) / 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(list: &[&str]) -> Option<Options> {
        let words: Vec<String> = list.iter().map(|w| (*w).to_owned()).collect();
        parse(&words).ok()
    }

    #[test]
    fn the_command_line_reads_as_unzip_reads_it() {
        let o = parsed(&["-oq", "a.zip", "x", "-x", "y", "-d", "out", "z"]).unwrap_or_default();
        assert!(o.overwrite);
        assert_eq!(o.quiet, 1);
        assert_eq!(o.zipfile.as_deref(), Some("a.zip"));
        assert_eq!(o.names, ["x"]);
        assert_eq!(o.excludes, ["y", "z"]);
        assert_eq!(o.exdir.as_deref(), Some("out"));
        let o = parsed(&["-Ppw", "-dout", "a"]).unwrap_or_default();
        assert_eq!(o.password.as_deref(), Some(&b"pw"[..]));
        assert_eq!(o.exdir.as_deref(), Some("out"));
        assert!(parsed(&["-Y", "a.zip"]).is_none());
        assert!(parsed(&["-", "a.zip"]).is_none());
    }
}
