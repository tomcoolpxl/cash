//! `c`, `cw`, `rn`, `k`, `rr` and `ch`: an archive's comment written or read, its files
//! renamed, the archive locked, given a recovery record or changed by switches. Each
//! but `cw` rewrites the archive, its files carried as they are.

use std::path::Path;

use cash_archive::rar::{ArchiveFamily, Builder, WriterResources};

use super::add;
use super::cmdline::{Command, Name, Overwrite, Parsed};
use super::list::{self, Comment};
use super::open::{self, Failure, Found};
use super::{Rar, Stop, code};

/// What a rewrite changes.
#[derive(Default)]
struct Change {
    comment: Option<Vec<u8>>,
    lock: bool,
    recovery_percent: Option<u64>,
    /// Old name to new, as the archive holds them.
    renames: Vec<(Vec<u8>, Vec<u8>)>,
    /// `-tl`: the archive's time is its newest file's, whatever the command.
    latest_time: bool,
    /// `rn`: the files' times kept as they are.
    keep_times: bool,
}

pub(super) fn run<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    command: &Command,
    parsed: &Parsed,
) -> Result<(), Stop> {
    let Some(archive) = parsed.archive.as_deref() else {
        return Ok(());
    };
    // `rn` takes its names in pairs; with an odd count rar does nothing but end a line.
    let pairs: Vec<(String, String)> = plain_names(parsed)
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[from, to]| (from.clone(), to.clone()))
        .collect();
    if *command == Command::Rename && (pairs.is_empty() || parsed.names.len() % 2 == 1) {
        rar.console.msg("\n");
        return Ok(());
    }
    add::adjusted_recovery(rar, rar.switches.recovery_record.as_deref());
    let display = super::cmdline::with_default_extension(archive).replace('\\', "/");
    let found = Found {
        display: display.clone(),
        path: rar.path(&display),
    };
    let Some(opened) = open_for_change(rar, command, &found)? else {
        return Ok(());
    };
    if *command == Command::CommentWrite {
        comment_write(rar, &found, &opened, parsed);
        return Ok(());
    }
    if *command == Command::Change && rar.switches.archive_metadata == Some('r') {
        restore_metadata(rar, &found, &opened)?;
        done(rar);
        return Ok(());
    }
    if refused(rar, &opened) {
        return Ok(());
    }
    // `c` reads its comment before it refuses.
    if *command != Command::Comment && open::header_mode_refused(rar, &opened) {
        return Err(Stop::Refused(code::FATAL));
    }
    let switches = &rar.switches;
    let mut change = Change {
        recovery_percent: add::asked_recovery_percent(rar),
        keep_times: *command == Command::Rename,
        latest_time: switches.latest_time,
        ..Change::default()
    };
    // `ch -tl` alone leaves the archive as it is, setting only its time.
    let mut time_only = false;
    match command {
        Command::Comment => {
            let file = switches.comment_file.as_ref().and_then(|arg| arg.text());
            change.comment = Some(add::comment_from(rar, &display, file)?);
            if open::header_mode_refused(rar, &opened) {
                return Err(Stop::Aborted(code::FATAL));
            }
        }
        Command::Lock => {
            // The recovery record `-rr` asks for is announced before the lock.
            if change.recovery_percent.is_some() {
                rar.console.msg("\nAdding the data recovery record     ");
            }
            rar.console.msg("\nLocking archive");
            change.lock = true;
        }
        Command::RecoveryRecord(size) => {
            add::adjusted_recovery(rar, size.as_deref());
            change.recovery_percent = add::recovery_percent(size.as_deref());
        }
        Command::Rename => change.renames = renames(rar, &opened, &pairs),
        _ => {
            // `ch`: the switches that change an archive without packing it again.
            if let Some(arg) = &switches.comment_file {
                change.comment = Some(add::comment_from(rar, &display, arg.text())?);
            }
            change.lock = switches.lock;
            if change.lock {
                rar.console.msg("\nLocking archive");
            }
            if let Some(case) = switches.case {
                change.renames = case_renames(&opened, case);
            }
            time_only = change.latest_time
                && change.comment.is_none()
                && !change.lock
                && switches.case.is_none()
                && switches.archive_metadata != Some('s')
                && change.recovery_percent.is_none();
        }
    }
    if change.recovery_percent.is_some() && *command != Command::Lock {
        rar.console.msg("\nAdding the data recovery record     ");
    }
    if time_only {
        add::stamp_time(&found.path, newest(rar, &opened));
    } else if *command != Command::Rename || !change.renames.is_empty() {
        rewrite(rar, &found.path, &opened, &change)?;
    }
    done(rar);
    Ok(())
}

/// The archive opened to be changed, "Processing archive" said before a question for
/// its headers' password, else once it is open (not by `cw`); `None` when it does not
/// open, said as rar says it.
fn open_for_change<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    command: &Command,
    found: &Found,
) -> Result<Option<open::Opened>, Stop> {
    let display = &found.display;
    let announced = std::cell::Cell::new(false);
    let announce = || {
        if *command != Command::CommentWrite && !announced.replace(true) {
            rar.console.msg(&format!("\nProcessing archive {display}"));
        }
    };
    let failure = match open::open_announcing(rar, found, &announce)? {
        Ok(opened) => {
            announce();
            return Ok(Some(opened));
        }
        Err(failure) => failure,
    };
    match failure {
        Failure::NotRar | Failure::WrongPassword(_) if *command == Command::CommentWrite => {
            return open::refuse(rar, found, &failure).map_or(Ok(None), Err);
        }
        // The others say what they process, and that it is a bad archive.
        Failure::NotRar => {
            announce();
            rar.console
                .err(&format!("\nERROR: Bad archive {display}\n"));
            done(rar);
        }
        Failure::WrongPassword(_) => {
            announce();
            rar.console.err(&format!(
                "\nIncorrect password for {display}\nERROR: Bad archive {display}\n"
            ));
            rar.fail(code::PASSWORD);
            done(rar);
        }
        _ => {
            open::report(rar, found, &failure, false);
            // The report ended its line.
            if *command != Command::CommentWrite
                && matches!(failure, Failure::Missing(_))
                && !rar.switches.no_done
            {
                rar.console.msg("Done\n");
            }
        }
    }
    Ok(None)
}

/// Whether rar refuses to change the archive, saying so: a locked one, or a RAR 1.5 to
/// 4 volume. A RAR 5 volume is changed by itself, the others of its set left as they
/// are, as rar changes one.
fn refused<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>, opened: &open::Opened) -> bool {
    let legacy_volume = opened.facts.volume && opened.archive.as_rar50().is_none();
    if !opened.facts.locked && !legacy_volume {
        return false;
    }
    let words = if opened.facts.locked {
        "Locked archive"
    } else {
        "Cannot modify volume"
    };
    rar.console.err(&format!("\n\nERROR: {words}"));
    rar.fail(code::LOCKED);
    done(rar);
    true
}

/// `ch -amr`: the archive given back the name and time `-ams` saved in it, its times
/// set first, and nothing else changed; without them rar does nothing. A file of that
/// name is asked about, or left by `-o-`, replaced by `-o+` and `-y`; `-or` fails to
/// find the new name it makes, as rar 7.23's does.
fn restore_metadata<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    opened: &open::Opened,
) -> Result<(), Stop> {
    let Some(metadata) = opened
        .archive
        .as_rar50()
        .and_then(|archive| archive.main.archive_metadata())
    else {
        return Ok(());
    };
    if let Some(time) = metadata.creation_filetime().and_then(filetime_system) {
        let times = cash_win32::unix::Times {
            modified: time,
            accessed: None,
            created: Some(time),
        };
        let _ = cash_win32::unix::set_times(&found.path, &times);
    }
    // The name up to its first NUL, and only its last part: never a path elsewhere.
    let Some(saved) = metadata.name.as_ref().and_then(|name| {
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        let text = String::from_utf8_lossy(name.get(..end).unwrap_or_default()).into_owned();
        Path::new(&text.replace('\\', "/"))
            .file_name()
            .map(|part| part.to_string_lossy().into_owned())
    }) else {
        return Ok(());
    };
    if found.path.file_name() == Some(std::ffi::OsStr::new(&saved)) {
        return Ok(());
    }
    let target = found.path.with_file_name(&saved);
    // The folder the archive was named in, as it was named.
    let folder = found
        .display
        .rfind('/')
        .and_then(|slash| found.display.get(..=slash))
        .unwrap_or_default();
    let shown = format!("{folder}{saved}");
    if std::fs::symlink_metadata(&target).is_ok() {
        let switches = &rar.switches;
        let replace = if switches.yes {
            true
        } else {
            match switches.overwrite {
                Some(Overwrite::All) => true,
                Some(Overwrite::Skip) => false,
                Some(Overwrite::Rename) => {
                    let (stem, extension) = saved
                        .rsplit_once('.')
                        .map_or((saved.as_str(), String::new()), |(stem, extension)| {
                            (stem, format!(".{extension}"))
                        });
                    rar.console.err(&format!(
                        "\nCannot rename {} to {}{stem}(1){extension}\nThe system cannot find the file specified.",
                        found.display,
                        folder
                    ));
                    false
                }
                Some(Overwrite::Ask) | None => ask_overwrite(rar, &shown)?,
            }
        };
        if !replace {
            return Ok(());
        }
        // rar deletes the file it replaces, so the archive renamed takes its creation
        // time, as Windows gives a name's new file. A name differing only in case is
        // the archive itself, which rar 7.23 deletes, losing it: cash renames it.
        let current = found
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase());
        if current.as_deref() != Some(saved.to_lowercase().as_str()) {
            let _ = std::fs::remove_file(&target);
        }
    }
    // A rename that fails is said, the exit code left as it is, as rar leaves it.
    if let Err(error) = std::fs::rename(&found.path, &target) {
        rar.console.err(&format!(
            "\nCannot rename {} to {shown}\n{}",
            found.display,
            open::system_message(&error)
        ));
        return Ok(());
    }
    rar.console
        .msg(&format!("\n{} is renamed to {shown}", found.display));
    Ok(())
}

/// rar's question before `ch -amr` replaces a file: yes or no, `Quit` stopping it.
fn ask_overwrite<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    shown: &str,
) -> Result<bool, Stop> {
    const CHOICES: &str = "[Y]es, [N]o, [A]ll, n[E]ver, [Q]uit ";
    rar.console
        .err(&format!("\n\nOverwrite {shown}?\n{CHOICES}"));
    loop {
        let Some(answer) = rar.read_answer(true)? else {
            rar.console.err("Read error in the file stdin");
            if let Some(extra) = rar.stdin_end_words(false) {
                rar.console.err(&format!("\n{extra}"));
            }
            return Err(Stop::Aborted(code::READ));
        };
        match answer.trim().chars().next().map(|c| c.to_ascii_lowercase()) {
            Some('y' | 'a') => return Ok(true),
            Some('n' | 'e') => return Ok(false),
            Some('q') => return Err(Stop::Quit),
            _ => rar.console.err(&format!("\n{CHOICES}")),
        }
    }
}

/// A FILETIME as the system's time.
fn filetime_system(ticks: u64) -> Option<std::time::SystemTime> {
    let nanos = (u128::from(ticks) * 100).checked_sub(11_644_473_600 * 1_000_000_000)?;
    std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_nanos(u64::try_from(nanos).ok()?))
}

fn done<SE: cash_core::ShellExtensions>(rar: &Rar<'_, SE>) {
    if !rar.switches.no_done {
        rar.console.msg("\nDone\n");
    }
}

/// The names after the archive's, list files read.
fn plain_names(parsed: &Parsed) -> Vec<String> {
    parsed
        .names
        .iter()
        .map(|name| match name {
            Name::Plain(text) | Name::List(text) => text.replace('\\', "/"),
        })
        .collect()
}

/// `cw`: the comment to the file named, or to standard output.
fn comment_write<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    found: &Found,
    opened: &open::Opened,
    parsed: &Parsed,
) {
    let target = plain_names(parsed).into_iter().next();
    let Some(Comment::Text(text)) = list::comment(rar, found, opened) else {
        rar.console.msg("\nComment is not present\n");
        return;
    };
    let Some(file) = target else {
        rar.console.notice(&text.replace("\r\n", "\n"));
        rar.console.msg("\n");
        return;
    };
    rar.console.msg(&format!("\nWrite comment to {file}\n"));
    if let Err(error) = std::fs::write(rar.path(&file), text.as_bytes()) {
        rar.console.err(&format!(
            "\nCannot create {file}\n{}",
            open::system_message(&error)
        ));
        rar.fail(code::CREATE);
    }
}

/// `rn`'s pairs as the archive's renames, each shown as rar shows it: the name typed,
/// then the new one. A folder renamed takes what it holds with it.
fn renames<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    opened: &open::Opened,
    pairs: &[(String, String)],
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let legacy = opened.archive.as_rar50().is_none();
    let mut renames = Vec::new();
    for member in opened.archive.members() {
        let name = add::member_name(&member);
        for (from, to) in pairs {
            if let Some(new) = renamed(from, to, &name) {
                // rar names what the mask matched: the file a wildcard does, the folder
                // named for what is in it.
                let old = if open::has_wildcard(from) {
                    &name
                } else {
                    from
                };
                rar.console.msg(&format!("\nRenaming {old} to {new}"));
                renames.push((member.meta.name.clone(), stored(&new, legacy)));
                break;
            }
        }
    }
    renames
}

/// `-cl` and `-cu` on every name.
fn case_renames(opened: &open::Opened, case: char) -> Vec<(Vec<u8>, Vec<u8>)> {
    let legacy = opened.archive.as_rar50().is_none();
    opened
        .archive
        .members()
        .filter_map(|member| {
            let name = add::member_name(&member);
            let new = if case == 'l' {
                name.to_lowercase()
            } else {
                name.to_uppercase()
            };
            (new != name).then(|| (member.meta.name.clone(), stored(&new, legacy)))
        })
        .collect()
}

/// A name as an archive holds it: RAR 1.5 to 4's with Windows' separator.
fn stored(name: &str, legacy: bool) -> Vec<u8> {
    if legacy {
        name.replace('/', "\\").into_bytes()
    } else {
        name.as_bytes().to_vec()
    }
}

/// The name `name` takes from `rn FROM TO`: a plain FROM the name itself or a folder of
/// it, a wildcard FROM matched a part at a time, each `*` and `?` of TO filled with
/// what FROM's matched.
fn renamed(from: &str, to: &str, name: &str) -> Option<String> {
    if !open::has_wildcard(from) {
        if name.eq_ignore_ascii_case(from) {
            return Some(to.to_owned());
        }
        let rest = name
            .get(..from.len())
            .filter(|head| head.eq_ignore_ascii_case(from))
            .and_then(|_| name.get(from.len()..))
            .filter(|rest| rest.starts_with('/'))?;
        return Some(format!("{to}{rest}"));
    }
    let masks: Vec<&str> = from.split('/').collect();
    let parts: Vec<&str> = name.split('/').collect();
    if masks.len() != parts.len() {
        return None;
    }
    let mut captures = Vec::new();
    for (mask, part) in masks.iter().zip(&parts) {
        captures.extend(capture(mask, part)?);
    }
    let mut captures = captures.into_iter();
    let mut out = String::new();
    for c in to.chars() {
        match c {
            '*' | '?' => out.push_str(&captures.next().unwrap_or_default()),
            _ => out.push(c),
        }
    }
    Some(out)
}

/// What each `*` and `?` of `mask` matches in `text`, case aside; `None` for no match.
fn capture(mask: &str, text: &str) -> Option<Vec<String>> {
    let mask: Vec<char> = mask.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    walk(&mask, &text, &mut out).then_some(out)
}

/// [`capture`]'s match of what is left of the mask against what is left of the text, the
/// longest run first for each `*`.
fn walk(mask: &[char], text: &[char], out: &mut Vec<String>) -> bool {
    match mask.first() {
        None => text.is_empty(),
        Some('*') => (0..=text.len()).rev().any(|take| {
            out.push(text[..take].iter().collect());
            if walk(&mask[1..], &text[take..], out) {
                return true;
            }
            out.pop();
            false
        }),
        Some('?') => {
            let Some(&c) = text.first() else {
                return false;
            };
            out.push(c.to_string());
            if walk(&mask[1..], &text[1..], out) {
                return true;
            }
            out.pop();
            false
        }
        Some(&m) => text.first().is_some_and(|&c| {
            c.to_lowercase().eq(m.to_lowercase()) && walk(&mask[1..], &text[1..], out)
        }),
    }
}

/// The archive written again with `change`, its files carried as they are.
fn rewrite<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    path: &Path,
    opened: &open::Opened,
    change: &Change,
) -> Result<(), Stop> {
    let password = add::data_password(rar)?;
    let mut builder = match add::rewriting_builder(opened, password.as_deref()) {
        Ok(builder) => builder,
        Err(error) => {
            rar.console.err(&format!("\n{error}"));
            return Err(Stop::Aborted(code::FATAL));
        }
    };
    let comment = match &change.comment {
        Some(comment) => Some(comment.clone()),
        None => opened.archive.comment(password.as_deref()).ok().flatten(),
    };
    let kept = change.comment.is_none() && add::comment_stored_plain(opened);
    builder = builder
        .comment(comment)
        .comment_kept_plain(kept)
        .layout(add::layout(rar, Some(add::old_bound(&opened.archive))));
    builder = add::encrypting_headers(rar, builder, password.as_deref());
    if let Some(percent) = change.recovery_percent {
        builder = builder.recovery_percent(Some(percent));
    }
    // WinRAR's rewrite has its quick-open locator as a new archive has; the lock is the
    // archive's own, which a rewrite of a locked one never sees.
    if builder.format().family() == ArchiveFamily::Rar50Plus {
        let quick_open = add::quick_open_on(rar);
        let metadata = add::saved_metadata(rar, path, newest(rar, opened));
        builder = builder
            .archive_metadata(metadata, change.lock, quick_open)
            .map_err(|error| {
                rar.console.err(&format!("\n{error}"));
                Stop::Aborted(code::FATAL)
            })?;
    }
    let all: Vec<usize> = (0..opened.archive.members().count()).collect();
    let keeper = add::Keeper::new(rar, opened, path, &all, password.as_deref())?;
    // `rn` keeps the files' times as they are; the others store them again by `-ts`.
    let mut keeper = if change.keep_times {
        keeper.keeping_times()
    } else {
        keeper
    };
    for (index, member) in opened.archive.members().enumerate() {
        if let Err(error) = keeper.keep(&mut builder, index, password.is_some()) {
            rar.console
                .err(&format!("\n{}\n{error}", add::member_name(&member)));
            return Err(Stop::Aborted(code::FATAL));
        }
    }
    // rar renames two files to one name when asked, keeping both.
    if !change.renames.is_empty() {
        builder = builder.allow_duplicate_names(true);
    }
    for (old, new) in &change.renames {
        if let Err(error) = rename(&mut builder, old, new) {
            rar.console.err(&format!("\n{error}"));
            rar.fail(code::WARNING);
        }
    }
    let resources = WriterResources::default().with_temp_dir(add::work_dir(rar, path));
    if let Err(error) = builder.write_to_path_with_resources(path, &resources, None) {
        rar.console
            .err(&format!("\nCannot create {}\n{error}", path.display()));
        rar.fail(code::CREATE);
        return Ok(());
    }
    if change.latest_time {
        add::stamp_time(path, newest(rar, opened));
    }
    Ok(())
}

/// The archive's newest file's time, which `-tl` and `-ams` take.
fn newest<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    opened: &open::Opened,
) -> Option<std::time::SystemTime> {
    opened
        .archive
        .members()
        .filter(|member| !member.meta.is_directory)
        .filter_map(|member| add::member_time(&member, &rar.zone))
        .max()
}

fn rename(builder: &mut Builder, old: &[u8], new: &[u8]) -> cash_archive::rar::Result<()> {
    builder.rename(old, new.to_vec())
}

#[cfg(test)]
mod tests {
    use super::renamed;

    #[test]
    fn rn_renames_by_name_folder_and_wildcard() {
        assert_eq!(
            renamed("src/a.txt", "src/z.txt", "src/a.txt").as_deref(),
            Some("src/z.txt")
        );
        assert_eq!(
            renamed("src/sub", "src/dir", "src/sub/b.txt").as_deref(),
            Some("src/dir/b.txt")
        );
        assert_eq!(renamed("src/sub", "src/dir", "src/subway"), None);
        assert_eq!(renamed("*.txt", "*.bak", "a.txt").as_deref(), Some("a.bak"));
        // A wildcard takes one part of a name, as rar's does.
        assert_eq!(renamed("*.txt", "*.bak", "src/a.txt"), None);
        assert_eq!(
            renamed("src/*.TXT", "src/*.bak", "src/a.txt").as_deref(),
            Some("src/a.bak")
        );
        assert_eq!(renamed("f?le", "g?le", "file").as_deref(), Some("gile"));
    }
}
