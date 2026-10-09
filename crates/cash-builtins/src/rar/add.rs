//! `a`, `u`, `f`, `m` and `mf`: files put into an archive, made or changed, as rar does
//! it. A folder is walked in the file system's own order, its files and folders as they
//! come; folders are written after every file, deepest first. An update keeps the
//! archive's files where they are, puts new files after them and the folders after
//! those. Each file put in has its line, then "Done".

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::SystemTime;

use cash_archive::rar::{
    self, ArchiveFamily, ArchiveMemberDetail, ArchiveReadOptions, ArchiveReader, ArchiveVersion,
    AttrSource, Builder, EntrySource, FileTimes, FileTimestamp, RewriteStaging, WriteOperation,
    WriteProgressEvent, WriterResources,
};

use super::cmdline::{Command, Name, Parsed};
use super::open::{self, Found};
use super::{Rar, Stop, code};
use super::{identical, list};
use crate::rardata;

/// Which files are put in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// `a`: every file found, one of the same name replaced.
    Add,
    /// `u`, `-u`: new files, and those newer than their archived copy.
    Update,
    /// `f`, `-f`: only files in the archive, when newer.
    Freshen,
}

/// What is deleted once archived: nothing, the files, or files and folders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delete {
    None,
    Files,
    All,
}

/// A file or folder found to put in.
#[derive(Clone, Debug)]
struct Source {
    path: PathBuf,
    /// As its line shows it: the name typed with its folders, `/` between.
    shown: String,
    /// Its name in the archive, `/` between folders.
    name: String,
    is_dir: bool,
    /// Its size in bytes; a folder's is 0.
    size: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
    attributes: u32,
    /// Its place in the walk, each folder before what it holds: rar deletes in the
    /// opposite order.
    walk: usize,
    /// With `-oh`, a file that has other names.
    hard: Option<HardLinked>,
    /// With `-ol`, the symbolic link or junction it is, stored as such.
    link: Option<rar::rar50::FileRedirection>,
}

/// A file with other names: which file it is, and the times its folder's entry gives,
/// which a hard link stored keeps.
#[derive(Clone, Copy, Debug)]
struct HardLinked {
    /// Its volume's serial number and its file index there.
    identity: (u32, u64),
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
}

/// Whether a source is new to the archive or takes an archived member's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Add,
    Update,
}

/// A member of the archive written, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// An archived member kept as it is: its index among the archive's members.
    Kept(usize),
    /// A file or folder put in: its index among the sources.
    Put(usize),
}

#[expect(
    clippy::too_many_lines,
    reason = "a command's work in rar's order: archive, names, plan, write, then -t and -df"
)]
pub(super) fn run<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    command: &Command,
    parsed: &Parsed,
) -> Result<(), Stop> {
    let switches = &rar.switches;
    let mode = match command {
        Command::Update => Mode::Update,
        Command::Freshen => Mode::Freshen,
        _ if switches.freshen => Mode::Freshen,
        _ if switches.update => Mode::Update,
        _ => Mode::Add,
    };
    let delete = match command {
        Command::Move { files_only: true } => Delete::Files,
        Command::Move { files_only: false } => Delete::All,
        _ if switches.delete_files || switches.recycle || switches.wipe => Delete::All,
        _ => Delete::None,
    };
    let Some(archive) = parsed.archive.as_deref() else {
        return Ok(());
    };
    adjusted_recovery(rar, switches.recovery_record.as_deref());
    let display = super::cmdline::with_default_extension(archive).replace('\\', "/");
    let path = rar.path(&display);
    let found = Found {
        display: display.clone(),
        path: path.clone(),
    };

    // An archive there is read first: a locked one, or a volume, is not changed.
    let old = if path.is_file() {
        let opened = match open::open(rar, &found)? {
            Ok(opened) => opened,
            Err(failure) => {
                open::report(rar, &found, &failure, false);
                return Ok(());
            }
        };
        if opened.facts.locked {
            rar.console.err("\n\nERROR: Locked archive");
            return Err(Stop::Refused(code::LOCKED));
        }
        if opened.facts.volume {
            rar.console.err("\n\nERROR: Cannot modify volume");
            return Err(Stop::Refused(code::LOCKED));
        }
        Some(opened)
    } else {
        None
    };

    // Stored files are not solid: rar leaves `-s` out with `-m0`.
    let compressing = switches.method != Some(0);
    let solid = (compressing && switches.solid.as_deref().is_some_and(|s| s != "-"))
        || old.as_ref().is_some_and(|o| o.facts.solid);
    let identical = switches.identical.as_deref().and_then(identical::parse);
    if let Some(identical) = identical.filter(|identical| identical.level >= 3) {
        return list_identical(rar, parsed, identical);
    }
    let words = match (&old, solid) {
        (Some(old), _) if old.facts.solid => "Updating solid archive",
        (Some(_), _) => "Updating archive",
        (None, true) => "Creating solid archive",
        (None, false) => "Creating archive",
    };
    rar.console.msg(&format!("\n{words} {display}\n"));

    let comment = read_comment(rar, &display)?;

    let Collected {
        mut sources,
        missing,
        held,
    } = collect(rar, parsed)?;
    for (shown, error) in missing.iter().chain(&held) {
        rar.console.err(&format!(
            "\nCannot open {shown}\n{}",
            open::system_message(error)
        ));
    }
    if !missing.is_empty() {
        rar.fail(code::NO_FILES);
    }
    if solid && compressing && !switches.no_sort {
        sort_solid(rar, &mut sources);
    }

    // What each source does, against what the archive holds.
    let members: Vec<rar::ArchiveMember> = old
        .as_ref()
        .map(|o| o.archive.members().collect())
        .unwrap_or_default();
    let names: Vec<String> = old
        .as_ref()
        .map(|o| member_names(&o.archive, &members))
        .unwrap_or_default();
    let mut actions: Vec<Option<(Action, Option<usize>)>> = Vec::new();
    for source in &sources {
        let there = names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&source.name));
        let action = match (there, mode) {
            (None, Mode::Freshen) => None,
            (None, _) => Some((Action::Add, None)),
            (Some(index), Mode::Add) => Some((Action::Update, Some(index))),
            (Some(index), _) => {
                let archived = member_seconds(&members[index], &rar.zone);
                let newer = match (source.modified.map(unix_seconds), archived) {
                    (Some(new), Some(old)) => new > old,
                    _ => true,
                };
                newer.then_some((Action::Update, Some(index)))
            }
        };
        actions.push(action);
    }
    // `-as`: the members no name gave are taken out.
    let synced: Vec<usize> = if switches.synchronize {
        (0..members.len())
            .filter(|&index| {
                let name = member_name(&members[index]);
                !sources
                    .iter()
                    .any(|source| source.name.eq_ignore_ascii_case(&name))
            })
            .collect()
    } else {
        Vec::new()
    };
    if actions.iter().all(Option::is_none) && synced.is_empty() {
        rar.console.msg("\nWARNING: No files\n");
        rar.fail(code::NO_FILES);
        return Ok(());
    }
    // `-ol`: the symbolic links and junctions met, stored as such.
    let mut links: Vec<Option<rar::rar50::FileRedirection>> =
        sources.iter().map(|source| source.link.clone()).collect();
    // `-oh`: a file's other names put in after it are hard links to it, with the times
    // their folders' entries give.
    let mut named: std::collections::HashMap<(u32, u64), usize> = std::collections::HashMap::new();
    for index in 0..sources.len() {
        let Some(hard) = sources[index].hard.filter(|_| actions[index].is_some()) else {
            continue;
        };
        let Some(&first) = named.get(&hard.identity) else {
            named.insert(hard.identity, index);
            continue;
        };
        links[index] = Some(rar::rar50::FileRedirection::new(
            4,
            0,
            sources[first].name.clone().into_bytes(),
        ));
        let source = &mut sources[index];
        source.modified = hard.modified;
        source.created = hard.created;
        source.accessed = hard.accessed;
    }
    // `-oi`: each set of identical files put in, its first kept, the rest references.
    if let Some(identical) = identical {
        let files: Vec<(usize, u64, &Path)> = sources
            .iter()
            .enumerate()
            .filter(|(index, source)| !source.is_dir && actions[*index].is_some())
            .map(|(index, source)| (index, source.size, source.path.as_path()))
            .collect();
        let (looked, sets) = identical::sets(&files, identical.least);
        let line = |index: usize| format!("{:>12}  {}", sources[index].size, sources[index].shown);
        rar.console
            .msg(&identical::searched(identical.level, looked, &sets, &line));
        for set in &sets {
            // Hard links stay hard links, counted all the same.
            for &index in &set[1..] {
                if links[index].is_none() {
                    links[index] = Some(rar::rar50::FileRedirection::new(
                        5,
                        0,
                        sources[set[0]].name.clone().into_bytes(),
                    ));
                }
            }
        }
    }

    let versions = version_files(rar, old.as_ref(), &members, &names, &mut actions);
    // The version each member already is.
    let had: Vec<Option<u64>> = match &old {
        Some(opened) => member_versions(&opened.archive, members.len()),
        None => Vec::new(),
    };
    let mut dropped = versions.dropped.clone();
    dropped.extend(&synced);
    let slots = plan(&sources, &actions, &members, &dropped);
    let notes = deletions_among(&slots, &actions, &synced, &names);
    let password = data_password(rar)?;
    let mut builder = match &old {
        Some(opened) => match rewriting_builder(opened, password.as_deref()) {
            Ok(builder) => builder,
            Err(error) => {
                rar.console.err(&format!("\n{error}"));
                return Err(Stop::Aborted(code::FATAL));
            }
        },
        None => Builder::new(ArchiveVersion::Rar50),
    };
    // The newest file's time among those written: `-tl` and `-ams` take it.
    let newest = slots
        .iter()
        .filter_map(|slot| match *slot {
            Slot::Kept(index) if !members[index].meta.is_directory => {
                member_time(&members[index], &rar.zone)
            }
            Slot::Put(index) if !sources[index].is_dir => sources[index].modified,
            _ => None,
        })
        .max();
    builder = settings(
        rar,
        builder,
        solid,
        password.as_deref(),
        comment,
        old.as_ref(),
        saved_metadata(rar, &path, newest),
    )?;
    // WinRAR's bound on an archive it updates counts the old members and every file
    // named, whether written or not.
    if let Some(opened) = &old {
        let bound = sources
            .iter()
            .fold(old_bound(&opened.archive), |total, source| {
                total.saturating_add(rar::rar50::Layout::member_bound(
                    source.size,
                    source.name.as_bytes(),
                ))
            });
        builder = builder.layout(layout(rar, Some(bound)));
    }
    let legacy = builder.format().family() != ArchiveFamily::Rar50Plus;
    if !legacy && compressing {
        builder =
            builder.rar50_dictionary_size(Some(dictionary(switches.dictionary, &sources, solid)));
    }

    // The archived members kept, carried as they are or read back to be written again.
    let kept: Vec<usize> = slots
        .iter()
        .filter_map(|slot| match *slot {
            Slot::Kept(index) => Some(index),
            Slot::Put(_) => None,
        })
        .collect();
    let mut keeper = match &old {
        Some(opened) => Some(Keeper::new(rar, opened, &path, &kept, password.as_deref())?),
        None => None,
    };

    // Each line, by the member's place in what is written: the name shown and whether
    // it is new or replaces one.
    let mut lines: Vec<Option<(String, Action)>> = Vec::new();
    for slot in &slots {
        let added = match (*slot, keeper.as_mut()) {
            // An older version keeps its number, or gets its new one: a member carried
            // has its own, one written again in a solid archive has it set here.
            (Slot::Kept(index), Some(keeper)) => keeper
                .keep(&mut builder, index, password.is_some())
                .and_then(
                    |()| match versions.numbers.get(&index).copied().or(had[index]) {
                        Some(number) => builder.set_version(Some(number)),
                        None => Ok(()),
                    },
                )
                .map(|()| None),
            (Slot::Kept(_), None) => Ok(None),
            (Slot::Put(index), _) => {
                let source = &sources[index];
                let action = actions[index].map_or(Action::Add, |(action, _)| action);
                put(&mut builder, rar, source, legacy, links[index].clone())
                    .map(|()| Some((source.shown.clone(), action)))
            }
        };
        match added {
            Ok(line) => lines.push(line),
            Err(error) => {
                let shown = match *slot {
                    Slot::Kept(index) => names[index].clone(),
                    Slot::Put(index) => sources[index].shown.clone(),
                };
                rar.console.err(&format!("\nCannot add {shown}\n{error}"));
                rar.fail(code::WARNING);
            }
        }
    }

    let resources = WriterResources::default().with_temp_dir(work_dir(rar, &path));
    let written = match volume_size(rar) {
        Some(size) => write_volumes(
            rar,
            &display,
            &builder.volume_size(Some(size)),
            &path,
            &resources,
            &lines,
            volume_digits(&sources, u64::try_from(size).unwrap_or(u64::MAX)),
        ),
        None => write_single(rar, &builder, &path, &resources, &lines, &notes)
            .map(|()| vec![path.clone()]),
    };
    let written = match written {
        Ok(written) => written,
        Err(error) => {
            rar.console
                .err(&format!("\nCannot create {display}\n{error}"));
            rar.fail(code::CREATE);
            return Ok(());
        }
    };
    if switches.latest_time {
        for path in &written {
            stamp_time(path, newest);
        }
    }
    // `-log`: the archive, each volume of it, and the files put in, in their order.
    for path in &written {
        rar.log_archive(&shown_beside(&display, path));
    }
    for slot in &slots {
        if let Slot::Put(index) = *slot {
            rar.log_file(&sources[index].name);
        }
    }

    if asked_recovery_percent(rar).is_some() {
        rar.console.msg("\nAdding the data recovery record     ");
    }
    if switches.lock {
        rar.console.msg("\nLocking archive");
    }
    let tested = if switches.test_after {
        let first = written.first().cloned().unwrap_or_else(|| path.clone());
        let first_display = shown_beside(&display, &first);
        super::extract::test_written(rar, &first_display, first)?;
        true
    } else {
        false
    };
    if delete != Delete::None {
        let archived: Vec<&Source> = sources
            .iter()
            .zip(&actions)
            .filter(|(_, action)| action.is_some())
            .map(|(source, _)| source)
            .collect();
        delete_sources(rar, &archived, delete);
    }
    if !held.is_empty() {
        let files = if held.len() == 1 { "file" } else { "files" };
        rar.console
            .msg(&format!("\nWARNING: Cannot open {} {files}", held.len()));
        rar.fail(code::OPEN);
    }
    if !switches.no_done {
        rar.console.msg(if tested { "Done\n" } else { "\nDone\n" });
    }
    Ok(())
}

/// `-oi3` and `-oi4`: the identical files among those named, listed; no archive is
/// written.
fn list_identical<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    parsed: &Parsed,
    identical: identical::Identical,
) -> Result<(), Stop> {
    let Collected { sources, .. } = collect(rar, parsed)?;
    let files: Vec<(usize, u64, &Path)> = sources
        .iter()
        .enumerate()
        .filter(|(_, source)| !source.is_dir)
        .map(|(index, source)| (index, source.size, source.path.as_path()))
        .collect();
    let (looked, sets) = identical::sets(&files, identical.least);
    let line = |index: usize| format!("{:>12}  {}", sources[index].size, sources[index].shown);
    let name = |index: usize| sources[index].shown.clone();
    rar.console.msg(&identical::listed(
        identical.level,
        looked,
        &sets,
        &line,
        &name,
    ));
    Ok(())
}

/// `-ams`: the archive's own name and a time saved in its main header, as rar saves
/// them: the time it is written, or with `-tl` its newest file's.
pub(super) fn saved_metadata<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    path: &Path,
    newest: Option<SystemTime>,
) -> Option<rar::rar50::ArchiveMetadataRecord> {
    if rar.switches.archive_metadata != Some('s') {
        return None;
    }
    let name = path
        .file_name()?
        .to_string_lossy()
        .into_owned()
        .into_bytes();
    let time = if rar.switches.latest_time {
        newest
    } else {
        Some(SystemTime::now())
    };
    let ticks = time.map(|time| match filetime(time) {
        FileTimestamp::WindowsFiletime(ticks) => ticks,
        _ => 0,
    });
    Some(rar::rar50::ArchiveMetadataRecord::new(Some(name), ticks))
}

/// `-tl`: an archive's time set to `time`, its newest file's, as rar sets it.
pub(super) fn stamp_time(path: &Path, time: Option<SystemTime>) {
    if let Some(time) = time
        && let Ok(file) = std::fs::File::options().write(true).open(path)
    {
        let _ = file.set_modified(time);
    }
}

/// A member's modification time as the system's, to the second.
pub(super) fn member_time(
    member: &rar::ArchiveMember,
    zone: &cash_core::timefmt::Zone,
) -> Option<SystemTime> {
    let seconds = member_seconds(member, zone)?;
    let since = std::time::Duration::from_secs(u64::try_from(seconds).ok()?);
    SystemTime::UNIX_EPOCH.checked_add(since)
}

/// An archived member's name as rar compares it: `/` between folders.
pub(super) fn member_name(member: &rar::ArchiveMember) -> String {
    String::from_utf8_lossy(&member.meta.name).replace('\\', "/")
}

/// Each member's version, when it is an older version of its file (`-ver`).
pub(super) fn member_versions(archive: &rar::Archive, count: usize) -> Vec<Option<u64>> {
    let mut versions: Vec<Option<u64>> = archive
        .as_rar50()
        .map(|archive| archive.files().map(|file| file.version).collect())
        .unwrap_or_default();
    versions.resize(count, None);
    versions
}

/// The members' names as rar compares them, an older version's with its `;N`.
pub(super) fn member_names(archive: &rar::Archive, members: &[rar::ArchiveMember]) -> Vec<String> {
    members
        .iter()
        .zip(member_versions(archive, members.len()))
        .map(|(member, version)| match version {
            Some(version) => format!("{};{version}", member_name(member)),
            None => member_name(member),
        })
        .collect()
}

/// What `-ver` makes of the members sources replace.
#[derive(Default)]
struct Versions {
    /// The number each member kept is to have, by its index.
    numbers: std::collections::HashMap<usize, u64>,
    /// The members `-verN`'s limit drops.
    dropped: Vec<usize>,
}

/// `-ver`: a file a source replaces keeps its place as an older version, one past its
/// file's last, and the source is added after the rest; with `-verN` the oldest beyond
/// N go, said, and those left are numbered from 1 again. Only a RAR 5 archive keeps
/// versions.
fn version_files<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    old: Option<&open::Opened>,
    members: &[rar::ArchiveMember],
    names: &[String],
    actions: &mut [Option<(Action, Option<usize>)>],
) -> Versions {
    let mut versions = Versions::default();
    let Some(asked) = &rar.switches.versions else {
        return versions;
    };
    let Some(archive) = old
        .map(|old| &old.archive)
        .filter(|a| a.as_rar50().is_some())
    else {
        return versions;
    };
    let had = member_versions(archive, members.len());
    let bases: Vec<String> = members
        .iter()
        .map(|member| member_name(member).to_lowercase())
        .collect();
    let number =
        |versions: &Versions, index: usize| versions.numbers.get(&index).copied().or(had[index]);
    let mut retired = Vec::new();
    for action in actions.iter_mut() {
        let Some((Action::Update, Some(member))) = *action else {
            continue;
        };
        if members[member].meta.is_directory {
            continue;
        }
        let last = (0..members.len())
            .filter(|&index| bases[index] == bases[member])
            .filter_map(|index| number(&versions, index))
            .max()
            .unwrap_or(0);
        versions.numbers.insert(member, last + 1);
        *action = Some((Action::Add, None));
        retired.push(member);
    }
    let Some(limit) = asked.text().and_then(|text| text.parse::<usize>().ok()) else {
        return versions;
    };
    let mut seen = std::collections::HashSet::new();
    for member in retired {
        if !seen.insert(bases[member].clone()) {
            continue;
        }
        let mut older: Vec<(u64, usize)> = (0..members.len())
            .filter(|&index| bases[index] == bases[member])
            .filter_map(|index| number(&versions, index).map(|n| (n, index)))
            .collect();
        older.sort_unstable();
        let excess = older.len().saturating_sub(limit);
        for &(_, index) in older.get(..excess).unwrap_or_default() {
            rar.console.msg(&format!("\nDeleting {}", names[index]));
            versions.dropped.push(index);
        }
        for (n, &(_, index)) in older.get(excess..).unwrap_or_default().iter().enumerate() {
            versions.numbers.insert(index, n as u64 + 1);
        }
    }
    versions
}

/// The order rar writes in: the archive's files where they were, a replaced one in its
/// place; then the new files; then every folder, the new before the old, deepest first.
fn plan(
    sources: &[Source],
    actions: &[Option<(Action, Option<usize>)>],
    members: &[rar::ArchiveMember],
    dropped: &[usize],
) -> Vec<Slot> {
    let replacing = |member: usize| {
        actions
            .iter()
            .position(|action| matches!(action, Some((Action::Update, Some(m))) if *m == member))
    };
    let mut slots = Vec::new();
    for (index, member) in members.iter().enumerate() {
        if !member.meta.is_directory && !dropped.contains(&index) {
            slots.push(replacing(index).map_or(Slot::Kept(index), Slot::Put));
        }
    }
    for (index, (source, action)) in sources.iter().zip(actions).enumerate() {
        if !source.is_dir && matches!(action, Some((Action::Add, _))) {
            slots.push(Slot::Put(index));
        }
    }
    let mut folders: Vec<(usize, Slot)> = Vec::new();
    for (index, (source, action)) in sources.iter().zip(actions).enumerate() {
        if source.is_dir && matches!(action, Some((Action::Add, _))) {
            folders.push((depth(&source.name), Slot::Put(index)));
        }
    }
    for (index, member) in members.iter().enumerate() {
        if member.meta.is_directory && !dropped.contains(&index) {
            let slot = replacing(index).map_or(Slot::Kept(index), Slot::Put);
            folders.push((depth(&member_name(member)), slot));
        }
    }
    folders.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
    slots.extend(folders.into_iter().map(|(_, slot)| slot));
    slots
}

/// `-as`'s "Deleting" lines, by the member written they come before: each before the
/// first that was after it in the archive, the rest after the last.
fn deletions_among(
    slots: &[Slot],
    actions: &[Option<(Action, Option<usize>)>],
    synced: &[usize],
    names: &[String],
) -> Vec<Vec<String>> {
    let place = |slot: &Slot| match *slot {
        Slot::Kept(index) => Some(index),
        Slot::Put(source) => actions[source].and_then(|(_, member)| member),
    };
    let mut notes = vec![Vec::new(); slots.len() + 1];
    for &member in synced {
        let at = slots
            .iter()
            .position(|slot| place(slot).is_some_and(|index| index > member))
            .unwrap_or(slots.len());
        notes[at].push(format!("\nDeleting {}", names[member]));
    }
    notes
}

fn depth(name: &str) -> usize {
    name.matches('/').count()
}

/// The password the new files are encrypted with: `-p`'s or `-hp`'s, asked for when
/// given alone.
pub(super) fn data_password<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
) -> Result<Option<Vec<u8>>, Stop> {
    Ok(rar
        .given_password()?
        .map(|password| rardata::password_utf8(&password).into_bytes()))
}

/// The folder temporary files go in: `-w`'s, else the archive's own.
pub(super) fn work_dir<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    archive: &Path,
) -> PathBuf {
    match rar.switches.work_dir.as_deref().filter(|w| !w.is_empty()) {
        Some(dir) => rar.path(dir),
        None => archive
            .parent()
            .map_or_else(std::env::temp_dir, Path::to_path_buf),
    }
}

/// `-v<size>`'s size, the first given; `-v` alone sizes volumes by the disk, which cash
/// does not, and makes one archive.
fn volume_size<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) -> Option<usize> {
    let sizes = rar.switches.volumes.as_ref()?;
    sizes
        .iter()
        .flatten()
        .next()
        .and_then(|&size| usize::try_from(size).ok())
}

/// The archive's comment from `-z`: the file named, or standard input.
fn read_comment<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    display: &str,
) -> Result<Option<Vec<u8>>, Stop> {
    let Some(arg) = &rar.switches.comment_file else {
        return Ok(None);
    };
    comment_from(rar, display, arg.text()).map(Some)
}

/// A comment read from `file`, else from standard input, with rar's lines on the way.
pub(super) fn comment_from<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    display: &str,
    file: Option<&str>,
) -> Result<Vec<u8>, Stop> {
    let bytes = if let Some(file) = file {
        rar.console.msg(&format!("\nReading comment from {file}"));
        match std::fs::read(rar.path(file)) {
            Ok(bytes) => bytes,
            Err(error) => {
                rar.console.err(&format!(
                    "\nCannot open {file}\n{}",
                    open::system_message(&error)
                ));
                return Err(Stop::Aborted(code::OPEN));
            }
        }
    } else {
        rar.console.msg("\nReading comment from stdin\n");
        let mut bytes = Vec::new();
        let _ = io::Read::read_to_end(&mut rar.context.stdin(), &mut bytes);
        bytes
    };
    rar.console
        .msg(&format!("\nAdding a comment to {display}\n"));
    // RAR 5 keeps a comment in UTF-8: a text file is read in its own encoding.
    Ok(super::decode_text(&bytes).into_bytes())
}

/// The builder's settings from the switches, over those of the archive updated.
fn settings<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    builder: Builder,
    solid: bool,
    password: Option<&[u8]>,
    comment: Option<Vec<u8>>,
    old: Option<&open::Opened>,
    metadata: Option<rar::rar50::ArchiveMetadataRecord>,
) -> Result<Builder, Stop> {
    let switches = &rar.switches;
    let level = switches.method.unwrap_or(3);
    let mut builder = builder
        .compression_level(Some(level.max(1)))
        .store(level == 0)
        .solid(solid);
    if let Some(password) = password {
        builder = builder.password(Some(password.to_vec())).header_encryption(
            switches.header_password.is_some() || old.is_some_and(|o| o.facts.encrypted_headers),
        );
    }
    let comment = match comment {
        Some(comment) => Some(comment),
        None => old.and_then(|o| o.archive.comment(password).ok().flatten()),
    };
    builder = builder.comment(comment);
    if let Some(percent) = asked_recovery_percent(rar) {
        builder = builder.recovery_percent(Some(percent));
    }
    builder = builder.layout(layout(rar, None));
    if builder.format().family() == ArchiveFamily::Rar50Plus {
        let quick_open = quick_open_on(rar);
        builder = builder
            .archive_metadata(metadata, switches.lock, quick_open)
            .map_err(|error| {
                rar.console.err(&format!("\n{error}"));
                Stop::Aborted(code::FATAL)
            })?;
    }
    Ok(builder)
}

/// `WinRAR`'s layout: CRC32, or BLAKE2 with `-htb`; quick open for members stored in
/// more than 4,096 bytes, or for all with `-qo+`.
pub(super) fn layout<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    bound: Option<u64>,
) -> rar::rar50::Layout {
    let checksums = if rar.switches.hash == Some('b') {
        rar::rar50::Checksums::Blake2
    } else {
        rar::rar50::Checksums::Crc32
    };
    let over = if rar.switches.quick_open == Some('+') {
        None
    } else {
        Some(4096)
    };
    rar::rar50::Layout::winrar()
        .with_checksums(checksums)
        .with_quick_open_over(over)
        .with_offset_bound(bound)
}

/// The dictionary rar packs with, as Rar.txt says and `Rar.exe` records: `-md`'s, 32 MB
/// without it, halved while the largest file (all of them, in a solid archive) would
/// fit in it twice, to 128 KB at the least, or 1 MB in a solid archive.
fn dictionary(asked: Option<u64>, sources: &[Source], solid: bool) -> u64 {
    let files = sources.iter().filter(|source| !source.is_dir);
    let size = if solid {
        files.map(|source| source.size).sum()
    } else {
        files.map(|source| source.size).max().unwrap_or(0)
    };
    let mut dictionary = asked.unwrap_or(32 << 20);
    let least = if solid {
        dictionary.min(1 << 20)
    } else {
        128 << 10
    };
    while dictionary > least && size.saturating_mul(2) <= dictionary {
        dictionary /= 2;
    }
    dictionary
}

/// The builder a rewrite of `opened` starts from. A non-solid RAR 5 archive's members
/// are carried as they are, so their encrypted data needs no password, as rar asks none
/// for it; what is written anew still does.
pub(super) fn rewriting_builder(
    opened: &open::Opened,
    password: Option<&[u8]>,
) -> rar::Result<Builder> {
    let builder = if opened.facts.volume {
        // A volume is written again by itself, as rar writes one it changes alone.
        opened.archive.volume_builder(password)?
    } else if opened.archive.as_rar50().is_some() && !opened.facts.solid {
        opened.archive.carrying_builder(password)?
    } else {
        opened.archive.preserving_builder(password)?
    };
    // WinRAR drops the recovery record of an archive it changes, unless `-rr` asks for
    // one again.
    Ok(builder.recovery_percent(None))
}

/// The recovery record `-rr` asks for, as a percentage of the archive.
pub(super) fn asked_recovery_percent<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
) -> Option<u64> {
    rar.switches
        .recovery_record
        .as_deref()
        .and_then(|text| recovery_percent(Some(text)))
}

/// The bound `WinRAR` gives the locator's offsets when it changes an archive: from the
/// old archive's members, the ones dropped too, each counted as a member written is but
/// by its packed size (seen with compressed members near a width's edge).
pub(super) fn old_bound(archive: &rar::Archive) -> u64 {
    let members = archive.members().fold(1u64, |total, member| {
        total.saturating_add(rar::rar50::Layout::member_bound(
            member.meta.packed_size,
            &member.meta.name,
        ))
    });
    // Its recovery record counts too, as a member would.
    archive.as_rar50().map_or(members, |archive| {
        archive.services().fold(members, |total, service| {
            total.saturating_add(rar::rar50::Layout::member_bound(
                service.unpacked_size,
                &service.name,
            ))
        })
    })
}

/// Whether an archive written gets quick-open information: unless `-qo-`. Under
/// encrypted headers it is encrypted too, as `WinRAR`'s is.
pub(super) fn quick_open_on<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) -> bool {
    rar.switches.quick_open != Some('-')
}

/// `-rr[N]`'s and `rr[N]`'s size as a percentage of the archive, `N`, `N%` or `Np`: 3
/// when left out, none for 0, up to 1000; above that rar says it adjusts it to 1000
/// ([`adjusted_recovery`]) and writes 200, as `Rar.exe` 7.23 was seen to.
pub(super) fn recovery_percent(text: Option<&str>) -> Option<u64> {
    let digits = text.map_or("", |text| text.trim_end_matches(['%', 'p', 'P']));
    match digits.parse::<u64>() {
        Ok(0) => None,
        Ok(percent @ 1..=1000) => Some(percent),
        Ok(_) => Some(200),
        Err(_) => Some(3),
    }
}

/// rar's word on a recovery record asked above 1000%, said once by the command.
pub(super) fn adjusted_recovery<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    text: Option<&str>,
) {
    let digits = text.map_or("", |text| text.trim_end_matches(['%', 'p', 'P']));
    if digits.parse::<u64>().is_ok_and(|percent| percent > 1000) {
        rar.console
            .err(&format!("\nAdjusting -rr{digits} value to 1000."));
    }
}

/// A source queued in the builder, with its times and attributes; with `link`, as that
/// link, its data not read.
fn put<SE: cash_core::ShellExtensions>(
    builder: &mut Builder,
    rar: &Rar<'_, SE>,
    source: &Source,
    legacy: bool,
    link: Option<rar::rar50::FileRedirection>,
) -> rar::Result<()> {
    if legacy {
        // RAR 1.5 to 4 names take Windows' separator, times its local wall clock.
        let name = source.name.replace('/', "\\").into_bytes();
        let dos = source.modified.map(|time| dos_time(&rar.zone, time));
        if source.is_dir {
            builder.add_directory(name.clone(), dos, None)?;
        } else {
            builder.add_source(
                name.clone(),
                EntrySource::from_path(&source.path),
                dos,
                None,
            )?;
        }
        builder.set_dos_attributes(&name, u64::from(source.attributes))?;
        return Ok(());
    }
    let name = source.name.clone().into_bytes();
    let (unix, times) = stamped(
        rar.switches.times,
        source.modified,
        source.created,
        source.accessed,
    );
    if let Some(link) = link {
        builder.add_link(name.clone(), link, source.is_dir, source.size, unix)?;
    } else if source.is_dir {
        builder.add_directory(name.clone(), unix, None)?;
    } else {
        builder.add_source(
            name.clone(),
            EntrySource::from_path(&source.path),
            unix,
            None,
        )?;
    }
    if unix.is_none() && times != FileTimes::default() {
        builder.set_file_times(&name, Some(times))?;
    }
    builder.set_dos_attributes(&name, u64::from(source.attributes))?;
    Ok(())
}

/// The times a file is stored with under `-ts`: with `-tsm1` alone, whole seconds in
/// the header's own Unix time, as `WinRAR` keeps them; else those kept, at one
/// precision for the file: whole seconds, when every time kept is asked at one second,
/// as Unix seconds, else in full.
fn stamped(
    store: super::cmdline::TimeStore,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
) -> (Option<u32>, FileTimes) {
    if store.modified == Some('1') && store.created.is_none() && store.accessed.is_none() {
        let unix = modified
            .map(unix_seconds)
            .and_then(|seconds| u32::try_from(seconds).ok());
        return (unix, FileTimes::default());
    }
    let kept = [
        Some(store.modified.unwrap_or('+')),
        store.created,
        store.accessed,
    ];
    let whole = kept
        .iter()
        .flatten()
        .filter(|&&precision| precision != '-')
        .all(|&precision| precision == '1');
    let stamp = |time: Option<SystemTime>, precision: Option<char>| match precision {
        Some('-') | None => None,
        Some(_) if whole => time.and_then(|time| {
            u32::try_from(unix_seconds(time))
                .ok()
                .map(FileTimestamp::UnixSeconds)
        }),
        Some(_) => time.map(filetime),
    };
    let times = FileTimes {
        modified: stamp(modified, Some(store.modified.unwrap_or('+'))),
        created: stamp(created, store.created),
        accessed: stamp(accessed, store.accessed),
    };
    (None, times)
}

/// A kept member's times stored anew by `-ts`, as rar stores them when it writes an
/// archive again (`rn` aside): by default its modification time alone, its creation
/// and access times gone. RAR 5 output only.
fn restamp(
    builder: &mut Builder,
    store: super::cmdline::TimeStore,
    member: &rar::ArchiveMember,
) -> rar::Result<()> {
    if builder.format().family() != ArchiveFamily::Rar50Plus
        || !matches!(member.detail, rar::ArchiveMemberDetail::Rar50Plus { .. })
    {
        return Ok(());
    }
    let times = member.file_times()?.unwrap_or_default();
    let modified = times.modified.map(system_time).or_else(|| {
        member.meta.file_time.map(|seconds| {
            let nanos = member
                .meta
                .mtime_refinement
                .map_or(0, |time| time.nanoseconds);
            SystemTime::UNIX_EPOCH + std::time::Duration::new(u64::from(seconds), nanos)
        })
    });
    let (unix, stored) = stamped(
        store,
        modified,
        times.created.map(system_time),
        times.accessed.map(system_time),
    );
    // The member just added: an older version shares its name with the file.
    let Some(id) = builder.last_entry_id() else {
        return Ok(());
    };
    builder.set_mtime_by_id(id, unix)?;
    builder.set_file_times_by_id(id, (stored != FileTimes::default()).then_some(stored))
}

/// A stored time as a system time.
fn system_time(stamp: FileTimestamp) -> SystemTime {
    let since = |seconds: i64, nanos: u32| {
        let at = std::time::Duration::new(seconds.unsigned_abs(), nanos);
        if seconds >= 0 {
            SystemTime::UNIX_EPOCH + at
        } else {
            SystemTime::UNIX_EPOCH - at
        }
    };
    match stamp {
        FileTimestamp::WindowsFiletime(ticks) => {
            let ticks = i128::from(ticks) - 116_444_736_000_000_000;
            let seconds = i64::try_from(ticks.div_euclid(10_000_000)).unwrap_or(0);
            let nanos = u32::try_from(ticks.rem_euclid(10_000_000) * 100).unwrap_or(0);
            since(seconds, nanos)
        }
        FileTimestamp::Unix {
            seconds,
            nanoseconds,
        } => since(i64::from(seconds), nanoseconds),
        FileTimestamp::UnixSeconds(seconds) => since(i64::from(seconds), 0),
    }
}

/// The archived members a rewrite keeps. An archive's files are carried as they are,
/// packed and encrypted bytes and all, as rar copies them; a solid archive's, and RAR
/// 1.3's, are read back and written again.
pub(super) struct Keeper<'o> {
    opened: &'o open::Opened,
    path: PathBuf,
    carry: bool,
    members: Vec<rar::ArchiveMember>,
    staged: std::collections::HashMap<usize, EntrySource>,
    comments: Vec<Option<Vec<u8>>>,
    /// `-ts`, to store each member's times again by, as rar does when it writes an
    /// archive anew; `None` keeps them as they are, as `rn` does.
    times: Option<super::cmdline::TimeStore>,
}

impl<'o> Keeper<'o> {
    /// Ready to keep the members `kept` (indices among the archive's members): those
    /// that are not carried are read back now.
    pub(super) fn new<SE: cash_core::ShellExtensions>(
        rar: &Rar<'_, SE>,
        opened: &'o open::Opened,
        path: &Path,
        kept: &[usize],
        password: Option<&[u8]>,
    ) -> Result<Self, Stop> {
        let members: Vec<rar::ArchiveMember> = opened.archive.members().collect();
        let carry = opened.archive.as_rar13().is_none() && !opened.facts.solid;
        let payload: Vec<usize> = kept
            .iter()
            .copied()
            .filter(|&index| {
                let meta = &members[index].meta;
                !carry && !meta.is_directory && !meta.is_redirection
            })
            .collect();
        let mut staged = std::collections::HashMap::new();
        if !payload.is_empty() {
            let staging = RewriteStaging {
                directory: work_dir(rar, path),
                max_staged_bytes: u64::MAX,
            };
            let options = ArchiveReadOptions::with_optional_password(password);
            match opened
                .archive
                .stage_rewrite_sources(&payload, options, &staging)
            {
                Ok(sources) => staged.extend(payload.into_iter().zip(sources)),
                Err(error) => {
                    rar.console.err(&format!("\n{error}"));
                    return Err(Stop::Aborted(code::FATAL));
                }
            }
        }
        let comments = opened.archive.member_comments(password).unwrap_or_default();
        Ok(Self {
            opened,
            path: path.to_path_buf(),
            carry,
            members,
            staged,
            comments,
            times: Some(rar.switches.times),
        })
    }

    /// Keeps each member's times as they are, as `rn` does.
    pub(super) const fn keeping_times(mut self) -> Self {
        self.times = None;
        self
    }

    /// Queues member `index` in `builder`; `encrypting` as [`keep`]'s.
    pub(super) fn keep(
        &mut self,
        builder: &mut Builder,
        index: usize,
        encrypting: bool,
    ) -> rar::Result<()> {
        let member = &self.members[index];
        let comment = self.comments.get(index).cloned().flatten();
        if self.carry && !member.meta.is_directory && !member.meta.is_redirection {
            builder.carry(&self.opened.archive, index, &self.path)?;
            if comment.is_some() {
                let _ = builder.set_file_comment(&member.meta.name, comment);
            }
        } else {
            let source = self.staged.remove(&index);
            keep(builder, member, source, comment, encrypting)?;
        }
        match self.times {
            Some(store) => restamp(builder, store, member),
            None => Ok(()),
        }
    }
}

/// An archived member queued as it was: its name, times, attributes and comment, its
/// data from `source`; plain when it was, though the new files are encrypted.
pub(super) fn keep(
    builder: &mut Builder,
    member: &rar::ArchiveMember,
    source: Option<EntrySource>,
    comment: Option<Vec<u8>>,
    encrypting: bool,
) -> rar::Result<()> {
    let meta = &member.meta;
    let name = meta.name.clone();
    let legacy = builder.format().family() != ArchiveFamily::Rar50Plus;
    let unix = meta.attr_source() == AttrSource::Unix;
    let mode = unix.then(|| u32::try_from(meta.file_attr).unwrap_or(0o644));
    let dos_time = if legacy { meta.file_time } else { None };
    if meta.is_directory {
        builder.add_directory(name.clone(), dos_time, mode.map(|m| m & 0o7777))?;
    } else {
        let source = source.unwrap_or_else(|| EntrySource::from_bytes(Vec::new()));
        builder.add_source(name.clone(), source, dos_time, mode)?;
    }
    if !unix {
        builder.set_dos_attributes(&name, meta.file_attr)?;
    }
    match &member.detail {
        ArchiveMemberDetail::Rar50Plus { .. } if !legacy => {
            if let Ok(Some(times)) = member.file_times() {
                builder.set_file_times(&name, Some(times))?;
            }
        }
        ArchiveMemberDetail::Rar15To40 {
            extended_times,
            unicode_name,
            ..
        } if legacy => {
            if !extended_times.is_empty() {
                let _ = builder.set_legacy_extended_times(&name, Some(extended_times.clone()));
            }
            if let Some(raw) = unicode_name {
                let _ = builder.set_legacy_unicode_name(&name, raw.clone());
            }
        }
        _ => {}
    }
    if comment.is_some() {
        let _ = builder.set_file_comment(&name, comment);
    }
    if encrypting && !meta.is_encrypted {
        builder.set_entry_encryption(&name, None, None)?;
    }
    Ok(())
}

/// A system time as a FILETIME stamp, whole seconds only with `seconds`.
fn filetime(time: SystemTime) -> FileTimestamp {
    let nanos: i128 = match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(since) => i128::try_from(since.as_nanos()).unwrap_or(i128::MAX),
        Err(before) => -i128::try_from(before.duration().as_nanos()).unwrap_or(i128::MAX),
    };
    let ticks = nanos / 100 + 116_444_736_000_000_000;
    FileTimestamp::WindowsFiletime(u64::try_from(ticks.max(0)).unwrap_or(0))
}

/// A system time as whole seconds since 1970.
fn unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        Err(before) => -i64::try_from(before.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

/// An archived member's modification time as whole seconds since 1970: RAR 5's own,
/// RAR 1.5 to 4's MS-DOS time on the zone's wall clock.
pub(super) fn member_seconds(
    member: &rar::ArchiveMember,
    zone: &cash_core::timefmt::Zone,
) -> Option<i64> {
    if let Ok(Some(FileTimes {
        modified: Some(stamp),
        ..
    })) = member.file_times()
    {
        return i64::try_from(stamp.unix_nanoseconds().div_euclid(1_000_000_000)).ok();
    }
    let time = member.meta.file_time?;
    if member.meta.family == ArchiveFamily::Rar50Plus {
        return Some(i64::from(time));
    }
    let date = chrono::NaiveDate::from_ymd_opt(
        i32::try_from((time >> 25) + 1980).ok()?,
        (time >> 21) & 0xF,
        (time >> 16) & 0x1F,
    )?;
    let wall = date.and_hms_opt((time >> 11) & 0x1F, (time >> 5) & 0x3F, (time & 0x1F) * 2)?;
    zone.local_to_unix(wall)
}

/// A system time as MS-DOS's, on the zone's wall clock.
fn dos_time(zone: &cash_core::timefmt::Zone, time: SystemTime) -> u32 {
    use chrono::{Datelike, Timelike};
    let local = zone.to_local(chrono::DateTime::<chrono::Utc>::from(time));
    let year = u32::try_from(local.year() - 1980).unwrap_or(0).min(127);
    (year << 25)
        | (local.month() << 21)
        | (local.day() << 16)
        | (local.hour() << 11)
        | (local.minute() << 5)
        | (local.second() / 2)
}

/// What the writer reported, as the lines need it.
#[derive(Clone, Copy, Debug)]
enum Event {
    Started(usize),
    Finished(usize),
}

/// Writes one archive, the lines shown as the writer gets to each member: "Adding" or
/// "Updating" as it starts, "OK" when it is done, in the archive's order.
fn write_single<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    builder: &Builder,
    path: &Path,
    resources: &WriterResources,
    lines: &[Option<(String, Action)>],
    notes: &[Vec<String>],
) -> rar::Result<()> {
    let (sender, events) = mpsc::channel();
    let sender = Mutex::new(sender);
    let progress = move |event: WriteProgressEvent<'_>| {
        let event = match event {
            WriteProgressEvent::EntryStarted {
                operation: WriteOperation::Compression,
                index,
                ..
            } => Event::Started(index),
            WriteProgressEvent::EntryFinished {
                operation: WriteOperation::Compression,
                index,
                ..
            } => Event::Finished(index),
            _ => return,
        };
        if let Ok(sender) = sender.lock() {
            let _ = sender.send(event);
        }
    };
    let mut shown = Shown::new(rar, lines, notes);
    std::thread::scope(|scope| {
        let writer = scope.spawn(move || {
            let result = builder.write_to_path_with_resources(path, resources, Some(&progress));
            drop(progress);
            result
        });
        for event in events {
            shown.take(event);
        }
        let result = writer.join().unwrap_or(Err(rar::Error::WriterFailure(
            "the archive writer panicked",
        )));
        if result.is_ok() {
            shown.finish();
        }
        result
    })
}

/// The lines shown so far, in the archive's order whatever order the writer reports in.
struct Shown<'r, 'a, SE: cash_core::ShellExtensions> {
    rar: &'r Rar<'a, SE>,
    lines: &'r [Option<(String, Action)>],
    /// Lines said before a member's, by its place, and after the last: `-as`'s.
    notes: &'r [Vec<String>],
    started: Vec<bool>,
    finished: Vec<bool>,
    /// The first member whose line is not ended.
    next: usize,
    /// Whether `next`'s line is begun.
    begun: bool,
}

impl<'r, 'a, SE: cash_core::ShellExtensions> Shown<'r, 'a, SE> {
    fn new(
        rar: &'r Rar<'a, SE>,
        lines: &'r [Option<(String, Action)>],
        notes: &'r [Vec<String>],
    ) -> Self {
        Self {
            rar,
            lines,
            notes,
            started: vec![false; lines.len()],
            finished: vec![false; lines.len()],
            next: 0,
            begun: false,
        }
    }

    fn take(&mut self, event: Event) {
        match event {
            Event::Started(index) => {
                if let Some(started) = self.started.get_mut(index) {
                    *started = true;
                }
            }
            Event::Finished(index) => {
                if let Some(finished) = self.finished.get_mut(index) {
                    *finished = true;
                }
            }
        }
        while self.next < self.lines.len() {
            let index = self.next;
            if !self.begun && (self.started[index] || self.finished[index]) {
                self.begin(index);
            }
            if !self.finished[index] {
                break;
            }
            self.end(index);
        }
    }

    /// Ends every line: those of members the writer said nothing of too.
    fn finish(&mut self) {
        while self.next < self.lines.len() {
            let index = self.next;
            if !self.begun {
                self.begin(index);
            }
            self.end(index);
        }
        self.note(self.lines.len());
    }

    /// The lines said before member `index`'s.
    fn note(&self, index: usize) {
        if !self.rar.switches.no_names {
            for note in self.notes.get(index).into_iter().flatten() {
                self.rar.console.msg(note);
            }
        }
    }

    fn begin(&mut self, index: usize) {
        self.note(index);
        if let Some((shown, action)) = &self.lines[index] {
            self.rar.console.msg(&line_start(self.rar, *action, shown));
        }
        self.begun = true;
    }

    fn end(&mut self, index: usize) {
        if self.lines[index].is_some() && !self.rar.switches.no_names {
            self.rar.console.msg("  OK ");
        }
        self.next += 1;
        self.begun = false;
    }
}

/// A file's line up to its "OK": the verb, the name, the percentage's place.
fn line_start<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    action: Action,
    shown: &str,
) -> String {
    if rar.switches.no_names {
        return String::new();
    }
    let verb = match action {
        Action::Add => "Adding    ",
        Action::Update => "Updating  ",
    };
    format!("\n{verb}{shown:<58}{}", area(rar))
}

/// The percentage's place after a name: none with `-idp`.
const fn area<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) -> &'static str {
    if rar.switches.no_percent { "" } else { "     " }
}

/// The digits rar writes a volume's number in: as many as the volumes it foresees, one
/// for each seven eighths of a volume the archive's bound counts, and one more.
fn volume_digits(sources: &[Source], volume_size: u64) -> usize {
    let bound = sources.iter().fold(1u64, |total, source| {
        total.saturating_add(rar::rar50::Layout::member_bound(
            source.size,
            source.name.as_bytes(),
        ))
    });
    let foreseen = (bound - 1) / (volume_size * 7 / 8).max(1) + 1;
    foreseen.to_string().len()
}

/// Writes a volume set, `NAME.part1.rar` on, then shows the lines: a member cut across
/// volumes gets a line in each, after the volume's "Creating archive".
fn write_volumes<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    display: &str,
    builder: &Builder,
    path: &Path,
    resources: &WriterResources,
    lines: &[Option<(String, Action)>],
    width: usize,
) -> rar::Result<Vec<PathBuf>> {
    let mut sink = VolumeFiles {
        first: path.to_path_buf(),
        written: Vec::new(),
        width,
    };
    builder.write_volumes_to(&mut sink, resources, None)?;
    let written = sink.written;
    // Where each member's parts went: the volume each part is in.
    let mut parts: Vec<Vec<usize>> = Vec::new();
    for (volume, file) in written.iter().enumerate() {
        let Ok(archive) = ArchiveReader::read_path(file) else {
            continue;
        };
        for member in archive.members() {
            if member.meta.is_split_before {
                if let Some(last) = parts.last_mut() {
                    last.push(volume);
                }
            } else {
                parts.push(vec![volume]);
            }
        }
    }
    let folder = match display.rfind('/') {
        Some(at) => display.get(..=at).unwrap_or_default(),
        None => "",
    };
    let mut volume = 0;
    for (index, line) in lines.iter().enumerate() {
        let spans = parts.get(index).cloned().unwrap_or_default();
        let mut spans = spans.into_iter();
        let first = spans.next().unwrap_or(volume);
        while volume < first {
            volume += 1;
            announce_volume(rar, folder, &written, volume);
        }
        let Some((shown, action)) = line else {
            for next in spans {
                volume = next;
            }
            continue;
        };
        rar.console.msg(&line_start(rar, *action, shown));
        for next in spans {
            // The percentage's last place, cleared with spaces, stays in rar's line.
            if !rar.switches.no_names {
                rar.console.msg("    ");
            }
            volume = next;
            announce_volume(rar, folder, &written, volume);
            if !rar.switches.no_names {
                rar.console
                    .msg(&format!("\n...       {shown:<58}{}", area(rar)));
            }
        }
        if !rar.switches.no_names {
            rar.console.msg("  OK ");
        }
    }
    Ok(written)
}

fn announce_volume<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    folder: &str,
    written: &[PathBuf],
    volume: usize,
) {
    let name = written
        .get(volume)
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    rar.console
        .msg(&format!("\n\nCreating archive {folder}{name}\n"));
}

/// A file written beside the archive, shown as the archive's name is.
fn shown_beside(display: &str, path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    match display.rfind('/') {
        Some(at) => format!("{}{name}", display.get(..=at).unwrap_or_default()),
        None => name,
    }
}

/// Writes each volume to its file: `NAME.part1.rar`, `NAME.part2.rar` …
struct VolumeFiles {
    first: PathBuf,
    written: Vec<PathBuf>,
    /// The digits a volume's number is written in.
    width: usize,
}

impl VolumeFiles {
    fn name(&self, index: u64) -> PathBuf {
        let stem = self
            .first
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = self
            .first
            .extension()
            .map_or_else(|| "rar".to_owned(), |e| e.to_string_lossy().into_owned());
        let dir = self
            .first
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        dir.join(format!(
            "{stem}.part{:0width$}.{ext}",
            index + 1,
            width = self.width
        ))
    }
}

impl rar::rar50::VolumeSink for VolumeFiles {
    fn start_volume(&mut self, index: u64) -> rar::Result<Box<dyn io::Write + Send>> {
        let path = self.name(index);
        let file = std::fs::File::create(&path)?;
        self.written.push(path);
        Ok(Box::new(io::BufWriter::new(file)))
    }
}

/// The order rar puts a solid archive's new files in: by the line of `rarfiles.lst`
/// each matches, `$default`'s place for the others; then by extension and name.
fn sort_solid<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, sources: &mut [Source]) {
    let order = order_list(rar);
    let default = order
        .iter()
        .position(|mask| mask == "$default")
        .unwrap_or(order.len());
    let key = |source: &Source| {
        let file = source.name.rsplit('/').next().unwrap_or(&source.name);
        let file = file.to_lowercase();
        let ext = match file.rfind('.') {
            Some(at) => file.get(at + 1..).unwrap_or_default().to_owned(),
            None => String::new(),
        };
        (
            place(&order, &source.name, &file).unwrap_or(default),
            ext,
            file,
        )
    };
    // Folders keep their places: they are written after the files anyway.
    let mut files: Vec<Source> = sources.iter().filter(|s| !s.is_dir).cloned().collect();
    files.sort_by_cached_key(key);
    let mut files = files.into_iter();
    for slot in sources.iter_mut().filter(|s| !s.is_dir) {
        if let Some(file) = files.next() {
            *slot = file;
        }
    }
}

/// `rarfiles.lst`'s masks from `%APPDATA%\WinRAR`, comments and blank lines left out.
fn order_list<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) -> Vec<String> {
    let Some(appdata) = super::env_var(rar.context, "APPDATA") else {
        return Vec::new();
    };
    let path = PathBuf::from(appdata).join("WinRAR").join("rarfiles.lst");
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    super::decode_text(&bytes)
        .lines()
        .map(|line| line.trim().replace('\\', "/"))
        .filter(|line| !line.is_empty() && !line.starts_with(';'))
        .map(|line| {
            if line.eq_ignore_ascii_case("$default") {
                "$default".to_owned()
            } else {
                line
            }
        })
        .collect()
}

/// The line of the order list a file takes: the first that matches it, unless a later
/// one matches only some of what that one does.
fn place(order: &[String], name: &str, file: &str) -> Option<usize> {
    let matches = |mask: &str| {
        if mask.contains('/') {
            open::mask_matches(mask, name)
        } else {
            open::mask_matches(mask, file)
        }
    };
    let mut chosen: Option<usize> = None;
    for (index, mask) in order.iter().enumerate() {
        if mask == "$default" || !matches(mask) {
            continue;
        }
        chosen = match chosen {
            // A mask the chosen one takes all of, and not the other way, is narrower.
            Some(at)
                if open::mask_matches(&order[at], mask)
                    && !open::mask_matches(mask, &order[at]) =>
            {
                Some(index)
            }
            Some(at) => Some(at),
            None => Some(index),
        };
    }
    chosen
}

/// What the names give: the sources, the names not there with Windows' errors, and the
/// files another program holds open for writing, with theirs.
struct Collected {
    sources: Vec<Source>,
    missing: Vec<(String, io::Error)>,
    held: Vec<(String, io::Error)>,
}

/// The files and folders the names give, by rar's rules: a folder named alone is put in
/// whole (only itself with `-r-`); a mask takes the files of its folder, and with `-r`
/// those of every folder below and the folders it matches. Names that are not there
/// come back with their errors.
fn collect<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    parsed: &Parsed,
) -> Result<Collected, Stop> {
    let mut names: Vec<String> = Vec::new();
    for name in &parsed.names {
        match name {
            Name::Plain(text) => names.push(text.replace('\\', "/")),
            Name::List(file) => names.extend(list::read_list(rar, file)?),
        }
    }
    if names.is_empty() {
        names.push("*".to_owned());
    }
    let switches = &rar.switches;
    let mut exclude: Vec<String> = switches
        .exclude
        .iter()
        .map(|m| m.replace('\\', "/"))
        .collect();
    for file in &switches.exclude_lists {
        exclude.extend(list::read_list(rar, file.as_deref().unwrap_or_default())?);
    }
    let mut include: Vec<String> = switches
        .include
        .iter()
        .map(|m| m.replace('\\', "/"))
        .collect();
    for file in &switches.include_lists {
        include.extend(list::read_list(rar, file.as_deref().unwrap_or_default())?);
    }
    exclude.retain(|m| !m.is_empty());
    include.retain(|m| !m.is_empty());
    let mut walker = Walker {
        rar,
        exclude,
        include,
        found: Vec::new(),
        missing: Vec::new(),
        held: Vec::new(),
        walk: 0,
    };
    for name in &names {
        walker.name(name);
    }
    let Walker {
        found,
        missing,
        held,
        ..
    } = walker;
    // Each once, where it was first found; files first, then the folders deepest first.
    let mut seen = std::collections::HashSet::new();
    let mut files = Vec::new();
    let mut folders = Vec::new();
    for source in found {
        if seen.insert(source.name.to_lowercase()) {
            if source.is_dir {
                folders.push(source);
            } else {
                files.push(source);
            }
        }
    }
    folders.sort_by_key(|folder| std::cmp::Reverse(depth(&folder.name)));
    files.extend(folders);
    Ok(Collected {
        sources: files,
        missing,
        held,
    })
}

/// Walks the names given, keeping what the switches let in.
struct Walker<'r, 'a, SE: cash_core::ShellExtensions> {
    rar: &'r Rar<'a, SE>,
    exclude: Vec<String>,
    include: Vec<String>,
    found: Vec<Source>,
    missing: Vec<(String, io::Error)>,
    held: Vec<(String, io::Error)>,
    walk: usize,
}

impl<SE: cash_core::ShellExtensions> Walker<'_, '_, SE> {
    fn name(&mut self, arg: &str) {
        let recurse = self.rar.switches.recurse;
        let deep = matches!(recurse, Some('r' | '0'));
        // With `-r` (not `-r0`) a file's name is looked for in every folder below its
        // own, as a mask is; a folder's is not.
        let searched = recurse == Some('r') && !arg.ends_with('/') && !self.rar.path(arg).is_dir();
        if open::has_wildcard(arg) || arg.ends_with('/') || searched {
            let (folder, mask) = if arg.ends_with('/') {
                (arg.trim_end_matches('/').to_owned(), "*".to_owned())
            } else {
                match arg.rfind('/') {
                    Some(at) => (
                        arg.get(..at).unwrap_or_default().to_owned(),
                        arg.get(at + 1..).unwrap_or_default().to_owned(),
                    ),
                    None => (String::new(), arg.to_owned()),
                }
            };
            let base = if folder.is_empty() {
                String::new()
            } else {
                format!("{folder}/")
            };
            self.masked(&folder, &mask, &base, deep);
            return;
        }
        let arg = arg.trim_end_matches('/');
        let path = self.rar.path(arg);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.missing.push((arg.to_owned(), error));
                return;
            }
        };
        let base = match arg.rfind('/') {
            Some(at) => arg.get(..=at).unwrap_or_default().to_owned(),
            None => String::new(),
        };
        match self.seen(&path, &metadata) {
            Kind::Folder if recurse != Some('-') => self.folder(&path, arg, &base, &metadata),
            Kind::Skip => {}
            kind => self.keep(&path, arg, &base, &metadata, kind),
        }
    }

    /// What the walk makes of what it meets, `metadata` its own: a symbolic link or a
    /// junction is followed as rar follows it without `-ol`, left out with `-ol-`, and
    /// stored as the link with `-ol`.
    fn seen(&self, path: &Path, metadata: &std::fs::Metadata) -> Kind {
        if !metadata.file_type().is_symlink() {
            return if metadata.is_dir() {
                Kind::Folder
            } else {
                Kind::File
            };
        }
        match self.rar.switches.links.as_deref() {
            Some("-") => Kind::Skip,
            Some(_) => stored_link(path, metadata)
                .map_or(Kind::Skip, |(link, folder)| Kind::Link(link, folder)),
            None => match std::fs::metadata(path) {
                Ok(followed) if followed.is_dir() => Kind::Folder,
                Ok(_) => Kind::File,
                Err(_) => Kind::Skip,
            },
        }
    }

    /// A folder, then what it holds in the file system's order, each folder in turn.
    fn folder(&mut self, path: &Path, shown: &str, base: &str, metadata: &std::fs::Metadata) {
        self.keep(path, shown, base, metadata, Kind::Folder);
        for (name, metadata) in listing(path) {
            let child = path.join(&name);
            let shown = format!("{shown}/{name}");
            match self.seen(&child, &metadata) {
                Kind::Folder => self.folder(&child, &shown, base, &metadata),
                Kind::Skip => {}
                kind => self.keep(&child, &shown, base, &metadata, kind),
            }
        }
    }

    /// A mask's files in a folder; with `deep`, the folders below too, and those of them
    /// it matches.
    fn masked(&mut self, folder: &str, mask: &str, base: &str, deep: bool) {
        let path = self.rar.path(if folder.is_empty() { "." } else { folder });
        let prefix = if folder.is_empty() {
            String::new()
        } else {
            format!("{folder}/")
        };
        for (name, metadata) in listing(&path) {
            let shown = format!("{prefix}{name}");
            let child = path.join(&name);
            match self.seen(&child, &metadata) {
                Kind::Folder => {
                    if deep {
                        if open::mask_matches(mask, &name) {
                            self.keep(&child, &shown, base, &metadata, Kind::Folder);
                        }
                        self.masked(&shown, mask, base, deep);
                    }
                }
                Kind::Skip => {}
                // A link stored, a folder's too, is taken as a file is.
                kind => {
                    if open::mask_matches(mask, &name) {
                        self.keep(&child, &shown, base, &metadata, kind);
                    }
                }
            }
        }
    }

    /// Whether the switches let a file, folder or link in: `-ed`, `-x` and `-n`, the
    /// sizes, the attributes and the times asked.
    fn admits(&self, shown: &str, metadata: &std::fs::Metadata, is_dir: bool) -> bool {
        use std::os::windows::fs::MetadataExt;
        let switches = &self.rar.switches;
        if is_dir && (switches.no_empty_dirs || switches.exclude_paths == Some(0)) {
            return false;
        }
        let last = shown.rsplit('/').next().unwrap_or(shown);
        if self
            .exclude
            .iter()
            .any(|mask| mask_match(mask, shown, last))
        {
            return false;
        }
        if !self.include.is_empty()
            && !self
                .include
                .iter()
                .any(|mask| mask_match(mask, shown, last))
        {
            return false;
        }
        if !is_dir {
            let size = metadata.len();
            if switches.size_less.is_some_and(|limit| size >= limit)
                || switches.size_more.is_some_and(|limit| size <= limit)
            {
                return false;
            }
        }
        let attributes = metadata.file_attributes();
        if switches
            .exclude_attr
            .is_some_and(|mask| attributes & mask != 0)
            || switches
                .include_attr
                .is_some_and(|mask| attributes & mask == 0)
        {
            return false;
        }
        time_filters(self.rar, metadata.modified().ok())
    }

    /// One file, folder or link, if the switches let it in.
    fn keep(
        &mut self,
        path: &Path,
        shown: &str,
        base: &str,
        metadata: &std::fs::Metadata,
        kind: Kind,
    ) {
        use std::os::windows::fs::MetadataExt;
        let walk = self.walk;
        self.walk += 1;
        let switches = &self.rar.switches;
        let (is_dir, link) = match kind {
            Kind::Folder => (true, None),
            Kind::Link(link, folder) => (folder, Some(link)),
            Kind::File | Kind::Skip => (false, None),
        };
        if !self.admits(shown, metadata, is_dir) {
            return;
        }
        let attributes = metadata.file_attributes();
        let modified = metadata.modified().ok();
        // A link stored is not read: its own times and attributes, the reparse point's
        // among them, are what is kept.
        if let Some(link) = link {
            self.found.push(Source {
                path: path.to_path_buf(),
                shown: shown.to_owned(),
                name: archive_name(self.rar, shown, base, path),
                is_dir,
                size: 0,
                modified,
                created: metadata.created().ok(),
                accessed: metadata.accessed().ok(),
                attributes: attributes & (0x2837 | REPARSE_POINT),
                walk,
                hard: None,
                link: Some(link),
            });
            return;
        }
        // A file another program is writing is not read, as rar reads none, unless `-dh`.
        let mut opened = None;
        if !is_dir && !switches.shared {
            match open_unshared(path) {
                Err(error) if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => {
                    self.held.push((shown.to_owned(), error));
                    return;
                }
                Ok(file) => opened = Some(file),
                Err(_) => {}
            }
        }
        // A file's size and times are its own: its folder's entry for a hard link's
        // other name may not have caught up with them.
        let own = if is_dir {
            None
        } else {
            match &opened {
                Some(file) => file.metadata().ok(),
                None => std::fs::metadata(path).ok(),
            }
        };
        let times = own.as_ref().unwrap_or(metadata);
        let hard = if !is_dir && switches.hard_links {
            cash_win32::fs::file_info(path)
                .ok()
                .filter(|info| info.links > 1)
                .map(|info| HardLinked {
                    identity: (info.volume, info.index),
                    modified,
                    created: metadata.created().ok(),
                    accessed: metadata.accessed().ok(),
                })
        } else {
            None
        };
        self.found.push(Source {
            path: path.to_path_buf(),
            shown: shown.to_owned(),
            name: archive_name(self.rar, shown, base, path),
            is_dir,
            size: if is_dir { 0 } else { times.len() },
            modified: times.modified().ok(),
            created: times.created().ok(),
            accessed: times.accessed().ok(),
            attributes: attributes & 0x2837,
            walk,
            hard,
            link: None,
        });
    }
}

/// What the walk makes of a file system entry.
enum Kind {
    File,
    Folder,
    /// A symbolic link or junction stored as one (`-ol`), and whether it stands for a
    /// folder.
    Link(rar::rar50::FileRedirection, bool),
    /// Left out: a link with `-ol-`, or one that cannot be followed or read.
    Skip,
}

/// Windows' attribute of a symbolic link or junction (`FILE_ATTRIBUTE_REPARSE_POINT`).
const REPARSE_POINT: u32 = 0x400;

/// `-ol`'s record of a symbolic link or junction, as rar stores it: type 2 or 3, the
/// substitute name its reparse data holds, `\??\` and all, with `/` between folders; and
/// whether it stands for a folder.
fn stored_link(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Option<(rar::rar50::FileRedirection, bool)> {
    use std::os::windows::fs::MetadataExt;
    let data = cash_win32::reparse::data(path).ok()?;
    let target = cash_win32::reparse::target(&data)?;
    let folder = metadata.file_attributes() & 0x10 != 0;
    let kind = if target.junction { 3 } else { 2 };
    Some((
        rar::rar50::FileRedirection::new(
            kind,
            u64::from(folder),
            target.substitute.replace('\\', "/").into_bytes(),
        ),
        folder,
    ))
}

/// The folders `-dr` sends to the Recycle Bin whole, as rar does: those at the top of
/// what was archived whose every file and folder was archived.
fn whole_folders(archived: &[&Source]) -> Vec<PathBuf> {
    let paths: std::collections::HashSet<&Path> = archived
        .iter()
        .map(|source| source.path.as_path())
        .collect();
    let folders: std::collections::HashSet<&Path> = archived
        .iter()
        .filter(|source| source.is_dir)
        .map(|source| source.path.as_path())
        .collect();
    folders
        .iter()
        .filter(|folder| {
            !folder
                .parent()
                .is_some_and(|parent| folders.contains(parent))
        })
        .filter(|folder| all_archived(folder, &paths))
        .map(|folder| folder.to_path_buf())
        .collect()
}

/// Whether everything below a folder was archived.
fn all_archived(folder: &Path, paths: &std::collections::HashSet<&Path>) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().all(|entry| {
        let path = entry.path();
        paths.contains(path.as_path())
            && (!entry.file_type().is_ok_and(|kind| kind.is_dir()) || all_archived(&path, paths))
    })
}

/// A folder to the Recycle Bin once its files have gone: as `rd`, an empty one only.
fn recycle_folder(path: &Path) -> io::Result<()> {
    if std::fs::read_dir(path)?.next().is_some() {
        // What `rd` says of it, in Windows' words.
        return Err(io::Error::from_raw_os_error(145));
    }
    cash_win32::fs::recycle(path)
}

/// `-dw`: a file's data overwritten with zero bytes, the file cut to nothing and
/// renamed, then deleted, so that its data is not found again on the disk.
fn wipe(path: &Path) -> io::Result<()> {
    use std::io::{Seek as _, Write as _};
    {
        let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
        let mut left = file.metadata()?.len();
        let zeros = [0u8; 16 * 1024];
        file.seek(io::SeekFrom::Start(0))?;
        while left > 0 {
            let step = usize::try_from(left.min(zeros.len() as u64)).unwrap_or(zeros.len());
            file.write_all(&zeros[..step])?;
            left -= step as u64;
        }
        file.sync_all()?;
        file.set_len(0)?;
    }
    let renamed = path.with_file_name(format!(
        "{:08x}.tmp",
        std::process::id() ^ u32::try_from(path.as_os_str().len()).unwrap_or(0)
    ));
    let gone = if std::fs::rename(path, &renamed).is_ok() {
        &renamed
    } else {
        path
    };
    std::fs::remove_file(gone)
}

/// Windows' error for a file another handle's sharing does not allow opening.
const ERROR_SHARING_VIOLATION: i32 = 32;

/// Opens a file to read while letting no one else write it, as rar opens what it adds:
/// a file another program has open for writing does not open.
fn open_unshared(path: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}

/// A folder's files and folders, in the order the file system gives them.
fn listing(path: &Path) -> Vec<(String, std::fs::Metadata)> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            Some((entry.file_name().to_string_lossy().into_owned(), metadata))
        })
        .collect()
}

/// Whether `-x` or `-n`'s mask takes a source: one with a wildcard and no folder matches
/// its name anywhere, else the path; a plain one the path or a folder of it.
fn mask_match(mask: &str, shown: &str, last: &str) -> bool {
    let mask = mask.trim_end_matches('/');
    if open::has_wildcard(mask) {
        if mask.contains('/') {
            open::mask_matches(mask, shown)
        } else {
            open::mask_matches(mask, last)
        }
    } else {
        let mask = mask.to_lowercase();
        let shown = shown.to_lowercase();
        shown == mask || shown.starts_with(&format!("{mask}/"))
    }
}

/// Whether the time filters let a time in: every one, or any one of those with `o`.
fn time_filters<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    modified: Option<SystemTime>,
) -> bool {
    let Some(modified) = modified else {
        return true;
    };
    let mut any_or = false;
    let mut or_hit = false;
    for filter in &rar.switches.time_filters {
        let limit = match filter.kind {
            'a' | 'b' => parse_date(rar, &filter.value),
            _ => parse_period(&filter.value).map(|period| {
                SystemTime::now()
                    .checked_sub(period)
                    .unwrap_or(SystemTime::UNIX_EPOCH)
            }),
        };
        let Some(limit) = limit else {
            continue;
        };
        let pass = match filter.kind {
            'a' | 'n' => modified >= limit,
            _ => modified < limit,
        };
        if filter.or {
            any_or = true;
            or_hit |= pass;
        } else if !pass {
            return false;
        }
    }
    !any_or || or_hit
}

/// `YYYYMMDDHHMMSS`, separators allowed and the fields at its end left out, on the
/// zone's wall clock.
fn parse_date<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, text: &str) -> Option<SystemTime> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let field = |from: usize, len: usize, default: u32| -> u32 {
        digits
            .get(from..from + len)
            .and_then(|d| d.parse().ok())
            .unwrap_or(default)
    };
    let date = chrono::NaiveDate::from_ymd_opt(
        i32::try_from(field(0, 4, 1970)).ok()?,
        field(4, 2, 1),
        field(6, 2, 1),
    )?;
    let wall = date.and_hms_opt(field(8, 2, 0), field(10, 2, 0), field(12, 2, 0))?;
    let seconds = rar.zone.local_to_unix(wall)?;
    if seconds >= 0 {
        SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds.unsigned_abs()))
    } else {
        SystemTime::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(seconds.unsigned_abs()))
    }
}

/// `[<n>d][<n>h][<n>m][<n>s]`.
fn parse_period(text: &str) -> Option<std::time::Duration> {
    let mut seconds = 0u64;
    let mut number = 0u64;
    for c in text.chars() {
        if let Some(digit) = c.to_digit(10) {
            number = number.checked_mul(10)?.checked_add(u64::from(digit))?;
            continue;
        }
        let unit = match c.to_ascii_lowercase() {
            'd' => 86_400,
            'h' => 3_600,
            'm' => 60,
            's' => 1,
            _ => return None,
        };
        seconds = seconds.checked_add(number.checked_mul(unit)?)?;
        number = 0;
    }
    Some(std::time::Duration::from_secs(seconds.checked_add(number)?))
}

/// A source's name in the archive by the path switches: `-ep` its name alone, `-ep1`
/// without the folder named, `-ep2` its full path, `-ep3` with the drive too, `-ep4` a
/// prefix taken off, `-ap` a folder put before; `-cl` and `-cu` its case.
fn archive_name<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    shown: &str,
    base: &str,
    path: &Path,
) -> String {
    let switches = &rar.switches;
    let mut name = match switches.exclude_paths {
        Some(0) => shown.rsplit('/').next().unwrap_or(shown).to_owned(),
        Some(1) => shown.strip_prefix(base).unwrap_or(shown).to_owned(),
        Some(full @ (2 | 3)) => {
            let text = path.to_string_lossy().replace('\\', "/");
            let text = text.strip_prefix("//?/").unwrap_or(&text);
            match text.find(":/") {
                Some(1) if full == 3 => text.replacen(":/", "_/", 1),
                Some(1) => text.get(3..).unwrap_or_default().to_owned(),
                _ => text.trim_start_matches('/').to_owned(),
            }
        }
        _ => {
            // A path given in full keeps it, but for its drive and its root.
            let mut name = match shown.find(":/") {
                Some(1) => shown.get(3..).unwrap_or_default(),
                _ => shown,
            };
            name = name.trim_start_matches('/');
            while let Some(rest) = name.strip_prefix("./").or_else(|| name.strip_prefix("../")) {
                name = rest;
            }
            name.to_owned()
        }
    };
    if let Some(prefix) = &switches.exclude_prefix {
        let prefix = prefix.replace('\\', "/");
        let start = format!("{}/", prefix.trim_matches('/'));
        if start.len() > 1
            && name
                .get(..start.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(&start))
        {
            name.drain(..start.len());
        }
    }
    if let Some(prefix) = &switches.archive_path {
        let prefix = prefix.replace('\\', "/");
        let prefix = prefix.trim_matches('/');
        if !prefix.is_empty() {
            name = format!("{prefix}/{name}");
        }
    }
    match switches.case {
        Some('l') => name.to_lowercase(),
        Some('u') => name.to_uppercase(),
        _ => name,
    }
}

/// `m`, `mf`, `-df`: what was archived, deleted, a line each, the walk backwards: what a
/// folder holds before the folder. `mf` leaves folders, which count all the same.
fn delete_sources<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    archived: &[&Source],
    delete: Delete,
) {
    let total = archived.len().max(1);
    let mut order: Vec<&Source> = archived.to_vec();
    order.sort_by_key(|source| std::cmp::Reverse(source.walk));
    let whole = if rar.switches.recycle && delete == Delete::All {
        whole_folders(archived)
    } else {
        Vec::new()
    };
    let mut done = 0;
    for source in order {
        if source.is_dir && delete == Delete::Files {
            continue;
        }
        // In a folder that goes to the Recycle Bin whole: gone with it.
        if whole
            .iter()
            .any(|folder| source.path != *folder && source.path.starts_with(folder))
        {
            continue;
        }
        done += 1;
        let percent = done * 100 / total;
        let recycle = rar.switches.recycle;
        let removed = if source.is_dir {
            if whole.contains(&source.path) {
                cash_win32::fs::recycle(&source.path)
            } else if recycle {
                recycle_folder(&source.path)
            } else {
                std::fs::remove_dir(&source.path)
            }
        } else {
            if source.attributes & 0x1 != 0 {
                let _ = cash_win32::unix::set_attributes(&source.path, source.attributes & !0x1);
            }
            if recycle {
                cash_win32::fs::recycle(&source.path)
            } else if rar.switches.wipe {
                wipe(&source.path)
            } else {
                std::fs::remove_file(&source.path)
            }
        };
        let head = if source.is_dir {
            "Deleting directory "
        } else {
            "Deleting "
        };
        let word = if removed.is_ok() {
            "deleted"
        } else {
            "NOT DELETED"
        };
        // The Recycle Bin's deletions show no share of the work.
        let share = if recycle {
            String::new()
        } else {
            format!("{percent:>4}%")
        };
        rar.console
            .msg(&format!("\n{head}{:<34}{word}{share}", source.shown));
        if let Err(error) = removed {
            rar.console.err(&format!(
                "\nCannot delete {}\n{}",
                source.shown,
                open::system_message(&error)
            ));
        }
    }
}
