//! Reading an archive: `-t`, `-x`, `-d` and `--test-label`, member by member as GNU
//! tar's `read_and` goes.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use cash_archive::member::{Kind, Member};
use cash_archive::select;
use cash_archive::tar::read::{CopyError, Entry, Event, ReadError, Reader};
use cash_win32::unix::{self, Replacement, Times};

use super::options::Op;
use super::transform::Target;
use super::{Fatal, Tar, Wanted, is_dir, system_time};
use crate::compress::strerror;

/// A folder made while extracting: its time is set once what is in it is.
struct MadeDir {
    path: PathBuf,
    member: Member,
}

/// `path` joined with a member's `/`-separated name.
fn join(base: &Path, name: &[u8]) -> PathBuf {
    let text = String::from_utf8_lossy(name);
    let mut path = base.to_path_buf();
    for part in text.split('/').filter(|p| !p.is_empty() && *p != ".") {
        path.push(part);
    }
    path
}

/// Whether a part of a member's name is one Windows cannot hold.
fn unholdable(name: &[u8]) -> bool {
    let text = String::from_utf8_lossy(name);
    text.split('/')
        .filter(|p| !p.is_empty())
        .any(|p| unix::check_name(p).is_err())
}

impl<SE: cash_core::ShellExtensions> Tar<'_, SE> {
    /// `-t`, `-x`, `-d` or `--test-label` over the whole archive.
    #[expect(
        clippy::too_many_lines,
        reason = "GNU tar's read_and with its name list, one loop kept together"
    )]
    pub(super) fn read_archive(&mut self, op: Op) -> Result<(), Fatal> {
        let names = self.names()?;
        let mut wanted: Vec<Wanted> = names
            .into_iter()
            .map(|(name, base)| Wanted {
                pattern: name.into_bytes(),
                base,
                found: 0,
            })
            .collect();
        let default_base = self.last_base();
        let patterns = self.exclude_patterns()?;
        let input = self.open_read()?;
        let mut reader = Reader::new(input, self.options.ignore_zeros);
        let flags = Self::member_flags(self.options.matching);
        let mut started = self.options.starting_file.is_none();
        let mut made = Vec::new();
        let mut first_member = true;
        loop {
            let event = match reader.next_event() {
                Ok(event) => event,
                Err(ReadError::UnexpectedEof) => {
                    self.say("Unexpected EOF in archive")?;
                    return Err(self.fatal_quiet());
                }
                Err(ReadError::Io(e)) => {
                    let name = self.archive_name();
                    return Err(self.fatal(&format!("{name}: Cannot read: {}", strerror(&e))));
                }
            };
            let entry = match event {
                Event::NotTar => {
                    self.error("This does not look like a tar archive")?;
                    continue;
                }
                Event::Skipping => {
                    self.error("Skipping to next header")?;
                    continue;
                }
                Event::LoneZero(at) => {
                    self.say(&format!("A lone zero block at {at}"))?;
                    break;
                }
                Event::End => break,
                Event::Member(entry) => entry,
            };
            if op == Op::TestLabel {
                if first_member && entry.member.kind() == cash_archive::member::Kind::Other(b'V') {
                    let label = entry.member.name.clone();
                    if wanted.is_empty() {
                        let shown = self.quote(&label);
                        self.list_line(&shown)?;
                    } else if !wanted.iter().any(|w| w.pattern == label) {
                        self.status = 1;
                    }
                } else if !wanted.is_empty() {
                    self.status = 1;
                }
                break;
            }
            first_member = false;
            if !started {
                let starting = self.options.starting_file.clone().unwrap_or_default();
                if select::matches(starting.as_bytes(), &entry.member.name, flags) {
                    started = true;
                } else {
                    continue;
                }
            }
            if self.excluded(&entry.member.name, &patterns) {
                continue;
            }
            let base = if wanted.is_empty() {
                Some(default_base.clone())
            } else {
                let occurrence = self.options.occurrence;
                let mut chosen = None;
                for want in &mut wanted {
                    if select::matches(&want.pattern, &entry.member.name, flags) {
                        want.found += 1;
                        if occurrence.is_none_or(|n| want.found == n) {
                            chosen = Some(want.base.clone());
                        }
                        break;
                    }
                }
                chosen
            };
            let Some(base) = base else {
                continue;
            };
            match op {
                Op::List => self.list_member(&entry)?,
                Op::Extract => self.extract_member(&mut reader, &entry, &base, &mut made)?,
                Op::Diff => self.diff_member(&mut reader, &entry, &base)?,
                _ => {}
            }
        }
        for dir in made.iter().rev() {
            if !self.options.touch {
                if let Some(modified) = system_time(dir.member.mtime) {
                    let _ = unix::set_times(
                        &dir.path,
                        &Times {
                            modified,
                            accessed: None,
                            created: None,
                        },
                    );
                }
            }
        }
        let wildcards_default = self.options.matching.wildcards.is_none();
        for want in &wanted {
            let occurrence = self.options.occurrence.unwrap_or(1);
            if want.found >= occurrence {
                continue;
            }
            if wildcards_default && select::has_wildcards(&want.pattern) {
                self.say("Pattern matching characters used in file names")?;
                self.say(
                    "Use --wildcards to enable pattern matching, or --no-wildcards to suppress this warning",
                )?;
            }
            let name = self.quote_colon(&want.pattern);
            if want.found == 0 {
                self.error(&format!("{name}: Not found in archive"))?;
            } else {
                self.error(&format!("{name}: Required occurrence not found in archive"))?;
            }
        }
        self.bytes = reader.block() * 512;
        self.totals(false)
    }

    /// `-t`: the name, or with `-v` the long line.
    fn list_member(&mut self, entry: &Entry) -> Result<(), Fatal> {
        let member = &entry.member;
        let shown = if self.options.show_transformed && !self.transforms.is_empty() {
            let name = String::from_utf8_lossy(&member.name).into_owned();
            self.transforms.apply(&name, Target::Regular).into_bytes()
        } else {
            member.name.clone()
        };
        let line = if self.options.verbose >= 1 {
            self.long_line(member, &shown)
        } else {
            self.quote(&shown)
        };
        self.list_line(&line)
    }

    /// The name a member is extracted to: transformed, its leading parts stripped, made
    /// safe; `None` when nothing is left.
    fn extracted_name(&mut self, name: &[u8], target: Target) -> Result<Option<Vec<u8>>, Fatal> {
        let text = String::from_utf8_lossy(name).into_owned();
        let text = self.transforms.apply(&text, target);
        let mut rest = text.as_str();
        let strip = if target == Target::Regular || target == Target::Hardlink {
            self.options.strip_components
        } else {
            0
        };
        for _ in 0..strip {
            let trimmed = rest.trim_start_matches('/');
            match trimmed.find('/') {
                Some(at) => rest = trimmed.get(at + 1..).unwrap_or_default(),
                None => return Ok(None),
            }
        }
        if rest.is_empty() {
            return Ok(None);
        }
        let safe = self.safer_name(rest.as_bytes(), target == Target::Hardlink)?;
        Ok(Some(match &self.top_level {
            Some(dir) if target == Target::Regular => under_top_level(dir, &safe),
            _ => safe,
        }))
    }

    /// What `-v` says of a member being extracted or compared.
    fn verbose_member(&mut self, member: &Member) -> Result<(), Fatal> {
        match self.options.verbose {
            0 => Ok(()),
            1 => {
                let line = self.quote(&member.name);
                self.list_line(&line)
            }
            _ => {
                let line = self.long_line(member, &member.name.clone());
                self.list_line(&line)
            }
        }
    }

    /// Data that ended early, as GNU says it while extracting.
    fn data_failed(&mut self, error: CopyError, shown: &str) -> Result<(), Fatal> {
        match error {
            CopyError::Read(ReadError::UnexpectedEof) => {
                self.say("Unexpected EOF in archive")?;
                self.say("Unexpected EOF in archive")?;
                Err(self.fatal_quiet())
            }
            CopyError::Read(ReadError::Io(e)) => {
                let name = self.archive_name();
                Err(self.fatal(&format!("{name}: Cannot read: {}", strerror(&e))))
            }
            CopyError::Write(e) if e.kind() == io::ErrorKind::BrokenPipe => Err(e.into()),
            CopyError::Write(e) => self.error(&format!("{shown}: Cannot write: {}", strerror(&e))),
        }
    }

    /// `-x` of one member.
    #[expect(
        clippy::too_many_lines,
        reason = "GNU tar's extract_archive, one case per kind"
    )]
    fn extract_member<R: Read>(
        &mut self,
        reader: &mut Reader<R>,
        entry: &Entry,
        base: &Path,
        made: &mut Vec<MadeDir>,
    ) -> Result<(), Fatal> {
        let member = &entry.member;
        let kind = member.kind();
        let Some(name) = self.extracted_name(&member.name, Target::Regular)? else {
            return Ok(());
        };
        self.verbose_member(member)?;
        let shown = self.quote_colon(&name);
        if self.options.to_stdout {
            if matches!(kind, Kind::File | Kind::Other(_)) {
                let mut stdout = self.context.stdout();
                if let Err(error) = reader.copy_data(&mut stdout) {
                    return self.data_failed(error, &shown);
                }
                stdout.flush()?;
            }
            return Ok(());
        }
        if name == b"." && kind == Kind::Dir {
            return Ok(());
        }
        if unholdable(&name) {
            return self.error(&format!("{shown}: Cannot open: Invalid argument"));
        }
        let path = join(base, &name);
        if let Some(parent) = path.parent() {
            if !is_dir(parent) {
                let _ = fs::create_dir_all(parent);
            }
        }
        let existing = fs::symlink_metadata(&path).ok();
        if matches!(kind, Kind::HardLink | Kind::Symlink)
            && existing.is_some()
            && (self.options.keep_old || self.options.skip_old)
        {
            return Ok(());
        }
        if matches!(kind, Kind::File | Kind::Other(_)) {
            if let Some(old) = &existing {
                if self.options.keep_old {
                    return self.error(&format!("{shown}: Cannot open: File exists"));
                }
                if self.options.skip_old {
                    return Ok(());
                }
                if self.options.keep_newer {
                    let old_time = old
                        .modified()
                        .ok()
                        .map(cash_archive::member::Timestamp::from_system_time);
                    if old_time.is_some_and(|t| t.seconds >= member.mtime.seconds) {
                        let quoted = String::from_utf8_lossy(&name).into_owned();
                        return self.say(&format!("Current '{quoted}' is newer or same age"));
                    }
                }
            }
        }
        match kind {
            Kind::Dir => {
                if let Some(old) = &existing {
                    if !old.is_dir() {
                        if self.options.keep_old {
                            return self.error(&format!("{shown}: Cannot mkdir: File exists"));
                        }
                        let _ = unix::remove_even_read_only(&path);
                    }
                }
                if let Err(e) = fs::create_dir_all(&path) {
                    return self.error(&format!("{shown}: Cannot mkdir: {}", strerror(&e)));
                }
                made.push(MadeDir {
                    path,
                    member: member.clone(),
                });
                Ok(())
            }
            Kind::File | Kind::Other(_) => {
                let read_only = member.mode & 0o200 == 0;
                if self.options.overwrite && existing.as_ref().is_some_and(fs::Metadata::is_file) {
                    let file = fs::OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .open(&path);
                    let mut file = match file {
                        Ok(file) => file,
                        Err(e) => {
                            return self.error(&format!("{shown}: Cannot open: {}", strerror(&e)));
                        }
                    };
                    if let Err(error) = reader.copy_data(&mut file) {
                        return self.data_failed(error, &shown);
                    }
                    drop(file);
                    self.set_file_times(&path, member);
                    return Ok(());
                }
                if existing.as_ref().is_some_and(fs::Metadata::is_dir) {
                    let _ = fs::remove_dir(&path);
                }
                let mut output = match Replacement::create(path) {
                    Ok(output) => output,
                    Err(e) => {
                        return self.error(&format!("{shown}: Cannot open: {}", strerror(&e)));
                    }
                };
                if let Err(error) = reader.copy_data(&mut output) {
                    output.abandon();
                    return self.data_failed(error, &shown);
                }
                let mut times = fs::FileTimes::new();
                if !self.options.touch {
                    if let Some(modified) = system_time(member.mtime) {
                        times = times
                            .set_modified(modified)
                            .set_accessed(std::time::SystemTime::now());
                    }
                }
                if let Err(e) = output.finish(times, read_only, false) {
                    return self.error(&format!("{shown}: Cannot open: {}", strerror(&e)));
                }
                Ok(())
            }
            Kind::HardLink => {
                let Some(target) = self.extracted_name(&member.link, Target::Hardlink)? else {
                    return Ok(());
                };
                let target_path = join(base, &target);
                if existing.is_some() {
                    let _ = unix::remove_even_read_only(&path);
                }
                if let Err(e) = unix::hard_link(&target_path, &path) {
                    let quoted = String::from_utf8_lossy(&target).into_owned();
                    return self.error(&format!(
                        "{shown}: Cannot hard link to '{quoted}': {}",
                        strerror(&e)
                    ));
                }
                Ok(())
            }
            Kind::Symlink => {
                let target = String::from_utf8_lossy(&member.link).into_owned();
                let target = self.transforms.apply(&target, Target::Symlink);
                if existing.is_some() {
                    let _ = unix::remove_even_read_only(&path);
                }
                let resolved = path
                    .parent()
                    .map_or_else(|| PathBuf::from(&target), |p| p.join(&target));
                let link_kind = if is_dir(&resolved) {
                    unix::LinkKind::Dir
                } else {
                    unix::LinkKind::File
                };
                let windows_target = target.replace('/', "\\");
                if let Err(e) = unix::symlink(Path::new(&windows_target), &path, link_kind) {
                    return self.error(&format!(
                        "{shown}: Cannot create symlink to '{target}': {}",
                        strerror(&e)
                    ));
                }
                Ok(())
            }
            Kind::Char | Kind::Block | Kind::Fifo => {
                self.error(&format!("{shown}: Cannot mknod: Operation not permitted"))
            }
        }
    }

    fn set_file_times(&self, path: &Path, member: &Member) {
        if self.options.touch {
            return;
        }
        if let Some(modified) = system_time(member.mtime) {
            let _ = unix::set_times(
                path,
                &Times {
                    modified,
                    accessed: Some(std::time::SystemTime::now()),
                    created: None,
                },
            );
        }
    }

    /// A difference `-d` found: said on the listing's stream, the status 1.
    fn differs(&mut self, shown: &str, what: &str) -> Result<(), Fatal> {
        self.list_line(&format!("{shown}: {what}"))?;
        if self.status == 0 {
            self.status = 1;
        }
        Ok(())
    }

    /// `-d` of one member: GNU tar's `diff_archive`.
    fn diff_member<R: Read>(
        &mut self,
        reader: &mut Reader<R>,
        entry: &Entry,
        base: &Path,
    ) -> Result<(), Fatal> {
        let member = &entry.member;
        let Some(name) = self.extracted_name(&member.name, Target::Regular)? else {
            return Ok(());
        };
        self.verbose_member(member)?;
        let shown = self.quote_colon(&name);
        let path = join(base, &name);
        let view = match unix::unix_view(&path, false) {
            Ok(view) => view,
            Err(e) => {
                self.say(&format!("{shown}: Warning: Cannot stat: {}", strerror(&e)))?;
                if self.status == 0 {
                    self.status = 1;
                }
                return Ok(());
            }
        };
        match member.kind() {
            Kind::Dir => {
                if view.kind != unix::Kind::Dir {
                    self.differs(&shown, "File type differs")?;
                } else if view.permissions & 0o200 != member.mode & 0o200 {
                    self.differs(&shown, "Mode differs")?;
                }
                Ok(())
            }
            Kind::HardLink => {
                let Some(target) = self.extracted_name(&member.link, Target::Hardlink)? else {
                    return Ok(());
                };
                let other = unix::unix_view(&join(base, &target), false).ok();
                let same = other
                    .and_then(|o| o.identity)
                    .zip(view.identity)
                    .is_some_and(|(a, b)| a.volume == b.volume && a.index == b.index);
                if !same {
                    let quoted = String::from_utf8_lossy(&target).into_owned();
                    self.differs(&shown, &format!("Not linked to '{quoted}'"))?;
                }
                Ok(())
            }
            Kind::Symlink => {
                let stored = view
                    .link_target
                    .map(|t| t.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                if view.kind != unix::Kind::Symlink || stored.as_bytes() != member.link {
                    self.differs(&shown, "Symlink differs")?;
                }
                Ok(())
            }
            Kind::File | Kind::Other(_) => {
                if view.kind != unix::Kind::File {
                    return self.differs(&shown, "File type differs");
                }
                // Windows keeps one permission, the read-only attribute: the owner's
                // write bit. The others it has none of to compare.
                if view.permissions & 0o200 != member.mode & 0o200 {
                    self.differs(&shown, "Mode differs")?;
                }
                if u64::from(view.owner.id) != member.uid {
                    self.differs(&shown, "Uid differs")?;
                }
                if u64::from(view.group.id) != member.gid {
                    self.differs(&shown, "Gid differs")?;
                }
                let modified =
                    cash_archive::member::Timestamp::from_system_time(view.times.modified);
                if modified.seconds != member.mtime.seconds {
                    self.differs(&shown, "Mod time differs")?;
                }
                if view.size != member.size {
                    return self.differs(&shown, "Size differs");
                }
                let mut archived = Vec::new();
                if let Err(error) = reader.copy_data(&mut archived) {
                    return self.data_failed(error, &shown);
                }
                let on_disk = fs::read(&path).unwrap_or_default();
                if on_disk != archived {
                    self.differs(&shown, "Contents differ")?;
                }
                Ok(())
            }
            Kind::Char | Kind::Block | Kind::Fifo => self.differs(&shown, "File type differs"),
        }
    }
}

/// GNU's `enforce_one_top_level`: a name under `dir`, unless it already is.
fn under_top_level(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let start = name
        .iter()
        .position(|b| *b != b'/' && *b != b'.')
        .unwrap_or(name.len());
    let Some(rest) = name.get(start..).filter(|rest| !rest.is_empty()) else {
        return dir.to_vec();
    };
    if rest.starts_with(dir) && matches!(rest.get(dir.len()), None | Some(b'/')) {
        return name.to_vec();
    }
    let mut joined = dir.to_vec();
    joined.push(b'/');
    joined.extend_from_slice(name);
    normalized(&joined)
}

/// GNU's `normalize_filename_x`: no `.` components, no doubled or trailing `/`.
fn normalized(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if name.first() == Some(&b'/') {
        out.push(b'/');
    }
    for part in name.split(|b| *b == b'/') {
        if part.is_empty() || part == b"." {
            continue;
        }
        if !out.is_empty() && out.last() != Some(&b'/') {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    if out.is_empty() {
        out.push(b'.');
    }
    out
}
