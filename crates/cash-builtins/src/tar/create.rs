//! Writing an archive: `-c`, and `-r` and `-u` at the end of one; GNU tar's
//! `dump_file` over the names, with `-C` in order, exclusions, hard links, overrides.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use cash_archive::codec::{self, Codec};
use cash_archive::member::{Kind, Member, Timestamp};
use cash_archive::tar::read::{Event, Reader};
use cash_archive::tar::write::{NameProblem, Options as WriteOptions, WriteError, Writer};
use cash_archive::tar::{BLOCK, Format};
use cash_core::openfiles::OpenFiles;
use cash_win32::unix;

use super::options::{Name, Sort, TRY, TagWhat};
use super::transform::Target;
use super::{Fatal, Tar};
use crate::compress::{is_terminal, strerror};

/// What `--owner`, `--group`, `--mode` and `--mtime` set, read once.
#[derive(Clone, Debug, Default)]
struct Overrides {
    owner: Option<(u64, Vec<u8>)>,
    group: Option<(u64, Vec<u8>)>,
    mode: Vec<ModeChange>,
    mtime: Option<Timestamp>,
}

/// One clause of `--mode`: chmod's.
#[derive(Clone, Debug)]
enum ModeChange {
    Set(u32),
    Symbolic { who: u32, op: char, perms: String },
}

/// `--mode`'s clauses, chmod's syntax.
fn mode_changes(text: &str) -> Option<Vec<ModeChange>> {
    if !text.is_empty() && text.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return u32::from_str_radix(text, 8)
            .ok()
            .map(|m| vec![ModeChange::Set(m)]);
    }
    let mut out = Vec::new();
    for clause in text.split(',') {
        let who_end = clause.find(['+', '-', '=']).unwrap_or(clause.len());
        let (who_text, rest) = clause.split_at(who_end);
        let mut who = 0;
        for c in who_text.chars() {
            who |= match c {
                'u' => 0o4700,
                'g' => 0o2070,
                'o' => 0o1007,
                'a' => 0o7777,
                _ => return None,
            };
        }
        if who == 0 {
            who = 0o7777;
        }
        let mut chars = rest.chars().peekable();
        while let Some(op) = chars.next() {
            if !matches!(op, '+' | '-' | '=') {
                return None;
            }
            let mut perms = String::new();
            while let Some(&c) = chars.peek() {
                if matches!(c, '+' | '-' | '=') {
                    break;
                }
                if !"rwxXst".contains(c) {
                    return None;
                }
                perms.push(c);
                chars.next();
            }
            out.push(ModeChange::Symbolic { who, op, perms });
        }
    }
    Some(out)
}

/// `mode` changed by `changes`; `dir` for `X`.
fn apply_mode(mut mode: u32, changes: &[ModeChange], dir: bool) -> u32 {
    for change in changes {
        match change {
            ModeChange::Set(m) => mode = *m,
            ModeChange::Symbolic { who, op, perms } => {
                let mut bits = 0;
                for c in perms.chars() {
                    bits |= match c {
                        'r' => 0o444,
                        'w' => 0o222,
                        'x' => 0o111,
                        'X' if dir || mode & 0o111 != 0 => 0o111,
                        's' => 0o6000,
                        't' => 0o1000,
                        _ => 0,
                    };
                }
                let bits = bits & who;
                match op {
                    '+' => mode |= bits,
                    '-' => mode &= !bits,
                    _ => mode = (mode & !(who & 0o7777)) | bits,
                }
            }
        }
    }
    mode & 0o7777
}

/// A date of `--mtime`, `-N` and `--newer-mtime`: `@SECONDS`, `YYYY-MM-DD[ HH:MM[:SS]]`
/// in the shell's zone, or a file's own time when it names one.
fn parse_date(text: &str, zone: &cash_core::timefmt::Zone, path: &Path) -> Option<Timestamp> {
    use chrono::{NaiveDate, NaiveDateTime, TimeZone};
    let text = text.trim();
    if text.starts_with(['/', '.']) || (path.exists() && !text.starts_with('@')) {
        return fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .map(Timestamp::from_system_time);
    }
    if let Some(seconds) = text.strip_prefix('@') {
        let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
        let digits: String = fraction.chars().take(9).collect();
        let nanos = if digits.is_empty() {
            0
        } else {
            format!("{digits:0<9}").parse().ok()?
        };
        return Some(Timestamp {
            seconds: whole.parse().ok()?,
            nanos,
        });
    }
    let (text, utc) = match text.strip_suffix(" UTC").or_else(|| text.strip_suffix('Z')) {
        Some(rest) => (rest, true),
        None => (text, false),
    };
    let naive = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
    .or_else(|| {
        NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
    })?;
    let seconds = if utc {
        naive.and_utc().timestamp()
    } else {
        match zone {
            cash_core::timefmt::Zone::Local => chrono::Local
                .from_local_datetime(&naive)
                .earliest()?
                .timestamp(),
            cash_core::timefmt::Zone::Named(tz) => {
                tz.from_local_datetime(&naive).earliest()?.timestamp()
            }
            cash_core::timefmt::Zone::Fixed { offset, .. } => {
                offset.from_local_datetime(&naive).earliest()?.timestamp()
            }
        }
    };
    Some(Timestamp::seconds(seconds))
}

/// `NAME[:ID]` of `--owner` and `--group`.
fn account(text: &str) -> (u64, Vec<u8>) {
    match text.split_once(':') {
        Some((name, id)) => (id.parse().unwrap_or(0), name.as_bytes().to_vec()),
        None => match text.parse::<u64>() {
            Ok(id) => (id, Vec::new()),
            Err(_) => (0, text.as_bytes().to_vec()),
        },
    }
}

/// Where the archive is being written, and what identifies its file.
struct Output {
    writer: Writer<Box<dyn codec::Encoder>>,
    identity: Option<(u32, u64)>,
}

/// An encoder that writes as it is: an uncompressed archive.
struct Plain<W: Write>(W);

impl<W: Write> Write for Plain<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<W: Write> codec::Encoder for Plain<W> {
    fn finish(mut self: Box<Self>) -> io::Result<()> {
        self.0.flush()
    }
}

impl<SE: cash_core::ShellExtensions> Tar<'_, SE> {
    fn write_options(&self) -> WriteOptions {
        WriteOptions {
            format: self.options.format.unwrap_or(Format::Gnu),
            numeric_owner: self.options.numeric_owner,
            record_blocks: self.options.blocking,
        }
    }

    fn overrides(&mut self) -> Result<Overrides, Fatal> {
        let mut overrides = Overrides::default();
        if let Some(owner) = self.options.owner.clone() {
            overrides.owner = Some(account(&owner));
        }
        if let Some(group) = self.options.group.clone() {
            overrides.group = Some(account(&group));
        }
        if let Some(mode) = self.options.mode.clone() {
            let Some(changes) = mode_changes(&mode) else {
                writeln!(
                    self.context.stderr(),
                    "tar: Invalid mode given on option\n{TRY}"
                )?;
                self.status = 2;
                return Err(Fatal::Exit);
            };
            overrides.mode = changes;
        }
        if let Some(mtime) = self.options.mtime.clone() {
            let path = self.path(&mtime);
            if let Some(time) = parse_date(&mtime, &self.zone, &path) {
                overrides.mtime = Some(time);
            } else {
                self.say(&format!(
                    "Substituting -9223372036854775807 for unknown date format '{mtime}'"
                ))?;
                overrides.mtime = Some(Timestamp::seconds(i64::MIN + 1));
            }
        }
        Ok(overrides)
    }

    /// The codec `-z` and its kin, or `-a` by the archive's suffix, ask for.
    fn create_codec(&self, archive: &str) -> Option<Codec> {
        self.options.codec.or_else(|| {
            if self.options.auto_compress {
                Codec::by_suffix(archive).map(|(codec, _)| codec)
            } else {
                None
            }
        })
    }

    /// `-c`.
    pub(super) fn create(&mut self) -> Result<(), Fatal> {
        let names = self.names()?;
        if names.is_empty()
            && !self
                .options
                .names
                .iter()
                .any(|n| matches!(n, Name::FilesFrom(_)))
        {
            writeln!(
                self.context.stderr(),
                "tar: Cowardly refusing to create an empty archive\n{TRY}"
            )?;
            self.status = 2;
            return Err(Fatal::Exit);
        }
        if let Some(program) = self.options.refused_compressor.clone() {
            return Err(self.fatal(&format!("{program}: compressor not carried by cash's tar")));
        }
        let archive = self.archive_name();
        let codec = self.create_codec(&archive);
        let (sink, identity): (Box<dyn Write>, Option<(u32, u64)>) = if archive == "-" {
            if is_terminal(self.context, OpenFiles::STDOUT_FD) {
                return Err(self
                    .fatal("Refusing to write archive contents to terminal (missing -f option?)"));
            }
            (Box::new(self.context.stdout()), None)
        } else if archive == "/dev/null" {
            (Box::new(io::sink()), None)
        } else {
            let path = self.path(&archive);
            match fs::File::create(&path) {
                Ok(file) => {
                    let identity = unix::unix_view(&path, true)
                        .ok()
                        .and_then(|v| v.identity)
                        .map(|i| (i.volume, i.index));
                    (
                        Box::new(io::BufWriter::with_capacity(1 << 16, file)),
                        identity,
                    )
                }
                Err(e) => {
                    return Err(self.fatal(&format!("{archive}: Cannot open: {}", strerror(&e))));
                }
            }
        };
        let encoder: Box<dyn codec::Encoder> = match codec {
            // Each codec's own threads: xz's as many as memory holds, every core for
            // the others.
            Some(codec) => {
                let level = codec.levels().2;
                let threads = match codec {
                    Codec::Xz => {
                        let dict = u64::from(
                            codec::xz::Settings {
                                preset: level,
                                extreme: false,
                                check: codec::xz::Check::Crc64,
                                block_size: None,
                                threads: None,
                            }
                            .dict_size(),
                        );
                        crate::xz::auto_threads(level, codec::xz::default_block(dict).get())
                    }
                    _ => 0,
                };
                codec::writer_on(codec, sink, level, threads)?
            }
            None => Box::new(Plain(sink)),
        };
        let mut output = Output {
            writer: Writer::new(encoder, self.write_options()),
            identity,
        };
        self.dump_names(&mut output, &names, None)?;
        let blocks = output.writer.blocks();
        let encoder = output.writer.finish()?;
        encoder.finish()?;
        let record = self.options.blocking.max(1);
        self.bytes = (blocks + 2).div_ceil(record) * record * BLOCK as u64;
        self.totals(true)
    }

    /// The label of `-V`, then every name; `newer_than` holds `-u`'s archived times.
    fn dump_names(
        &mut self,
        output: &mut Output,
        names: &[(String, PathBuf)],
        newer_than: Option<&HashMap<Vec<u8>, Timestamp>>,
    ) -> Result<(), Fatal> {
        let overrides = self.overrides()?;
        let patterns = self.exclude_patterns()?;
        let newer = match self.options.newer.clone() {
            Some(text) => {
                let path = self.path(&text);
                parse_date(&text, &self.zone, &path)
            }
            None => None,
        };
        if let Some(label) = self.options.label.clone() {
            let member = Member {
                name: label.into_bytes(),
                kind: Some(Kind::Other(b'V')),
                mtime: Timestamp::from_system_time(std::time::SystemTime::now()),
                ..Member::default()
            };
            if let Err(e) = output.writer.write_member(&member, None) {
                return self.write_failed(e, "");
            }
        }
        let mut links = HashMap::new();
        let mut walk = Walk {
            overrides,
            patterns,
            newer,
            newer_than,
            links: &mut links,
        };
        for (name, base) in names {
            let trimmed = if name.len() > 1 {
                name.trim_end_matches(['/', '\\'])
            } else {
                name.as_str()
            };
            let trimmed = if trimmed.is_empty() { "/" } else { trimmed };
            let path = base.join(trimmed);
            self.dump(output, &mut walk, trimmed, &path)?;
        }
        Ok(())
    }

    /// An archive write that failed, or a name the format cannot hold.
    fn write_failed(&mut self, error: WriteError, shown: &str) -> Result<(), Fatal> {
        match error {
            WriteError::Name(NameProblem::TooLong(max)) => self.error(&format!(
                "{shown}: file name is too long (max {max}); not dumped"
            )),
            WriteError::Name(NameProblem::CannotSplit) => self.error(&format!(
                "{shown}: file name is too long (cannot be split); not dumped"
            )),
            WriteError::Name(NameProblem::LinkTooLong) => {
                self.error(&format!("{shown}: link name is too long; not dumped"))
            }
            WriteError::Read(e) => self.error(&format!("{shown}: Read error: {}", strerror(&e))),
            WriteError::Write(e) if e.kind() == io::ErrorKind::BrokenPipe => Err(e.into()),
            WriteError::Write(e) => {
                let archive = self.archive_name();
                Err(self.fatal(&format!("{archive}: Cannot write: {}", strerror(&e))))
            }
        }
    }

    /// One file or folder, and what is in a folder: GNU tar's `dump_file`.
    #[expect(
        clippy::too_many_lines,
        reason = "GNU tar's dump_file0, one pass kept together"
    )]
    fn dump(
        &mut self,
        output: &mut Output,
        walk: &mut Walk<'_>,
        name: &str,
        path: &Path,
    ) -> Result<(), Fatal> {
        let as_given = name.replace('\\', "/");
        if self.excluded(as_given.as_bytes(), &walk.patterns) {
            return Ok(());
        }
        let follow = self.options.dereference;
        let view = match unix::unix_view(path, follow) {
            Ok(view) => view,
            Err(e) => {
                let shown = self.quote_colon(as_given.as_bytes());
                return self.error(&format!("{shown}: Cannot stat: {}", strerror(&e)));
            }
        };
        if output
            .identity
            .zip(view.identity)
            .is_some_and(|(a, b)| a == (b.volume, b.index))
        {
            let shown = self.quote_colon(as_given.as_bytes());
            return self.say(&format!("{shown}: file is the archive; not dumped"));
        }
        let transformed = self.transforms.apply(&as_given, Target::Regular);
        let mut stored = self.safer_name(transformed.as_bytes(), false)?;
        let kind = match view.kind {
            unix::Kind::Dir => Kind::Dir,
            unix::Kind::Symlink => Kind::Symlink,
            unix::Kind::File => Kind::File,
        };
        if kind == Kind::Dir && !stored.ends_with(b"/") {
            stored.push(b'/');
        }
        let tag = if kind == Kind::Dir {
            self.exclusion_tag(path)
        } else {
            None
        };
        if let Some((tag_name, TagWhat::All)) = &tag {
            return self.tag_warning(&as_given, tag_name, "directory not dumped");
        }
        let mtime = Timestamp::from_system_time(view.times.modified);
        let mut skip = false;
        if let Some(newer) = walk.newer {
            if kind != Kind::Dir && mtime < newer {
                skip = true;
            }
        }
        if let Some(archived) = walk.newer_than {
            if kind != Kind::Dir
                && archived
                    .get(&stored)
                    .is_some_and(|t| mtime.seconds <= t.seconds)
            {
                skip = true;
            }
        }
        if !skip {
            let mut member = Member {
                name: stored.clone(),
                kind: Some(kind),
                size: if kind == Kind::File { view.size } else { 0 },
                mode: view.permissions & 0o7777,
                uid: u64::from(view.owner.id),
                gid: u64::from(view.group.id),
                uname: view.owner.name.clone().into_bytes(),
                gname: view.group.name.clone().into_bytes(),
                mtime,
                atime: view.times.accessed.map(Timestamp::from_system_time),
                ctime: Some(mtime),
                link: Vec::new(),
                device: (0, 0),
            };
            if kind == Kind::Symlink {
                let target = view
                    .link_target
                    .as_ref()
                    .map(|t| t.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                member.link = self.transforms.apply(&target, Target::Symlink).into_bytes();
                member.mode = 0o777;
            }
            if kind == Kind::File && !self.options.hard_dereference && view.links > 1 {
                if let Some(identity) = view.identity {
                    let key = (identity.volume, identity.index);
                    if let Some(first) = walk.links.get(&key) {
                        member.kind = Some(Kind::HardLink);
                        member.link = first.clone();
                        member.size = 0;
                    } else {
                        walk.links.insert(key, stored.clone());
                    }
                }
            }
            if let Some((uid, uname)) = &walk.overrides.owner {
                member.uid = *uid;
                member.uname.clone_from(uname);
            }
            if let Some((gid, gname)) = &walk.overrides.group {
                member.gid = *gid;
                member.gname.clone_from(gname);
            }
            if !walk.overrides.mode.is_empty() {
                member.mode = apply_mode(member.mode, &walk.overrides.mode, kind == Kind::Dir);
            }
            if let Some(time) = walk.overrides.mtime {
                if !self.options.clamp_mtime || member.mtime > time {
                    member.mtime = time;
                }
            }
            let shown_name = if self.options.show_transformed || self.options.show_stored {
                stored.clone()
            } else {
                let mut original = as_given.clone().into_bytes();
                if kind == Kind::Dir && !original.ends_with(b"/") {
                    original.push(b'/');
                }
                self.safer_name(&original, false)?
            };
            match self.options.verbose {
                0 => {}
                1 => {
                    let line = self.quote(&shown_name);
                    self.list_line(&line)?;
                }
                _ => {
                    let line = self.long_line(&member, &shown_name);
                    self.list_line(&line)?;
                }
            }
            let shown = self.quote_colon(&stored);
            if member.kind() == Kind::File {
                let mut file = match fs::File::open(path) {
                    Ok(file) => file,
                    Err(e) => {
                        return self.error(&format!("{shown}: Cannot open: {}", strerror(&e)));
                    }
                };
                match output.writer.write_member(&member, Some(&mut file)) {
                    Ok(written) => {
                        if written.shrank > 0 {
                            let shrank = written.shrank;
                            self.error(&format!(
                                "{shown}: File shrank by {shrank} bytes; padding with zeros"
                            ))?;
                        }
                    }
                    Err(e) => return self.write_failed(e, &shown),
                }
            } else if let Err(e) = output.writer.write_member(&member, None) {
                return self.write_failed(e, &shown);
            }
        }
        if let (Some((tag_name, what)), true) = (&tag, kind == Kind::Dir && self.options.recursion)
        {
            self.tag_warning(&as_given, tag_name, "contents not dumped")?;
            if *what == TagWhat::Contents {
                let slash = if as_given.ends_with('/') { "" } else { "/" };
                let dir = format!("{as_given}{slash}");
                self.dump(
                    output,
                    walk,
                    &format!("{dir}{tag_name}"),
                    &path.join(tag_name),
                )?;
            }
        } else if kind == Kind::Dir && self.options.recursion {
            let entries = match fs::read_dir(path) {
                Ok(entries) => entries,
                Err(e) => {
                    let shown = self.quote_colon(as_given.as_bytes());
                    return self.error(&format!("{shown}: Cannot open: {}", strerror(&e)));
                }
            };
            let mut children: Vec<String> = entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            if self.options.sort == Sort::Name {
                children.sort();
            }
            for child in children {
                let child_name = if as_given.ends_with('/') {
                    format!("{as_given}{child}")
                } else {
                    format!("{as_given}/{child}")
                };
                self.dump(output, walk, &child_name, &path.join(&child))?;
            }
        }
        if self.options.remove_files && !skip {
            let removed = if kind == Kind::Dir {
                fs::remove_dir(path)
            } else {
                unix::remove_even_read_only(path)
            };
            if let Err(e) = removed {
                let shown = self.quote_colon(as_given.as_bytes());
                self.error(&format!("{shown}: Cannot remove: {}", strerror(&e)))?;
            }
        }
        Ok(())
    }

    /// The tag that keeps a folder's contents out, the last given first, as GNU's
    /// `check_exclusion_tags` looks.
    fn exclusion_tag(&self, dir: &Path) -> Option<(String, TagWhat)> {
        const SIGNATURE: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";
        self.options.tags.iter().rev().find_map(|tag| {
            let file = dir.join(&tag.file);
            let found = if tag.cachedir {
                fs::File::open(&file).is_ok_and(|mut f| {
                    let mut start = [0_u8; SIGNATURE.len()];
                    io::Read::read_exact(&mut f, &mut start).is_ok() && start == SIGNATURE
                })
            } else {
                fs::metadata(&file).is_ok()
            };
            found.then(|| (tag.file.clone(), tag.what))
        })
    }

    /// GNU's `exclusion_tag_warning`, said with `-v` only.
    fn tag_warning(&self, dir: &str, tag: &str, what: &str) -> Result<(), Fatal> {
        if self.options.verbose == 0 {
            return Ok(());
        }
        let mut dir = dir.to_owned();
        if !dir.ends_with('/') {
            dir.push('/');
        }
        let dir = self.quote_colon(dir.as_bytes());
        let tag = self.quote(tag.as_bytes());
        self.say(&format!(
            "{dir}: contains a cache directory tag {tag}; {what}"
        ))
    }

    /// `-r` and `-u`: the names written where the archive's end begins.
    pub(super) fn append(&mut self, update: bool) -> Result<(), Fatal> {
        let archive = self.archive_name();
        if archive == "-" {
            writeln!(
                self.context.stderr(),
                "tar: Options '-Aru' are incompatible with '-f -'\n{TRY}"
            )?;
            self.status = 2;
            return Err(Fatal::Exit);
        }
        let names = self.names()?;
        let (end, archived) = self.archive_end(&archive, update)?;
        let path = self.path(&archive);
        let mut file = match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) => return Err(self.fatal(&format!("{archive}: Cannot open: {}", strerror(&e)))),
        };
        file.set_len(end * BLOCK as u64)?;
        file.seek(SeekFrom::Start(end * BLOCK as u64))?;
        let identity = unix::unix_view(&path, true)
            .ok()
            .and_then(|v| v.identity)
            .map(|i| (i.volume, i.index));
        let mut writer: Writer<Box<dyn codec::Encoder>> = Writer::new(
            Box::new(Plain(io::BufWriter::new(file))),
            self.write_options(),
        );
        writer.start_at(end);
        let mut output = Output { writer, identity };
        self.dump_names(&mut output, &names, update.then_some(&archived))?;
        let blocks = output.writer.blocks();
        let encoder = output.writer.finish()?;
        encoder.finish()?;
        self.bytes = blocks * BLOCK as u64;
        self.totals(true)
    }

    /// Where an archive's end begins, in blocks, and the latest time of each member:
    /// read through, as `-r`, `-u` and `-A` need it. A compressed archive cannot be
    /// changed.
    pub(super) fn archive_end(
        &mut self,
        archive: &str,
        times: bool,
    ) -> Result<(u64, HashMap<Vec<u8>, Timestamp>), Fatal> {
        let path = self.path(archive);
        let mut map = HashMap::new();
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((0, map)),
            Err(e) => return Err(self.fatal(&format!("{archive}: Cannot open: {}", strerror(&e)))),
        };
        let mut input = codec::Lookahead::new(file);
        let first = input.peek(512)?.to_vec();
        if self.options.codec.is_some() || Codec::sniff(&first).is_some() {
            return Err(self.fatal("Cannot update compressed archives"));
        }
        let mut reader = Reader::new(input, self.options.ignore_zeros);
        loop {
            match reader.next_event() {
                Ok(Event::Member(entry)) => {
                    if times {
                        let time = map
                            .entry(entry.member.name.clone())
                            .or_insert(entry.member.mtime);
                        if entry.member.mtime > *time {
                            *time = entry.member.mtime;
                        }
                    }
                }
                Ok(Event::NotTar) if first.is_empty() => {}
                Ok(Event::NotTar) => {
                    self.error("This does not look like a tar archive")?;
                }
                Ok(Event::Skipping) => self.error("Skipping to next header")?,
                Ok(Event::LoneZero(_) | Event::End) => break,
                Err(_) => {
                    self.say("Unexpected EOF in archive")?;
                    return Err(self.fatal_quiet());
                }
            }
        }
        Ok((reader.end_block().unwrap_or(0), map))
    }
}

/// What the walk carries from member to member.
struct Walk<'l> {
    overrides: Overrides,
    patterns: Vec<(Vec<u8>, super::options::Matching)>,
    newer: Option<Timestamp>,
    newer_than: Option<&'l HashMap<Vec<u8>, Timestamp>>,
    /// The first name each file with other links was stored under.
    links: &'l mut HashMap<(u32, u64), Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_change_as_chmod_changes_them() {
        let changes = mode_changes("a+x").unwrap();
        assert_eq!(apply_mode(0o644, &changes, false), 0o755);
        assert_eq!(
            apply_mode(0o644, &mode_changes("600").unwrap(), false),
            0o600
        );
        assert_eq!(
            apply_mode(0o640, &mode_changes("go=rX").unwrap(), true),
            0o655
        );
        assert_eq!(
            apply_mode(0o755, &mode_changes("u-w,o-x").unwrap(), false),
            0o554
        );
    }

    #[test]
    fn dates_read_as_gnu_tar_reads_them() {
        let zone = cash_core::timefmt::Zone::from_tz(Some("UTC0"));
        let none = Path::new("/no/such/file");
        assert_eq!(parse_date("@0", &zone, none), Some(Timestamp::seconds(0)));
        assert_eq!(
            parse_date("2021-02-03 04:05:06", &zone, none),
            Some(Timestamp::seconds(1_612_325_106))
        );
        assert_eq!(
            parse_date("2021-01-01", &zone, none),
            Some(Timestamp::seconds(1_609_459_200))
        );
    }
}
