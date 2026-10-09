//! `d`: members taken out of an archive, a line each in the archive's order; the archive
//! erased when nothing is left in it.

use std::path::Path;
use std::time::SystemTime;

use cash_archive::rar::{Builder, WriterResources};

use super::add;
use super::cmdline::Parsed;
use super::list::Masks;
use super::open::{self, Found};
use super::repack::Repack;
use super::{Rar, Stop, code};

/// The archive's builder for `d`: its comment and layout kept, its main header's
/// records written anew, `-ams`'s with them.
fn rewriter<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    opened: &open::Opened,
    path: &Path,
    newest: Option<SystemTime>,
    password: Option<&[u8]>,
) -> Result<Builder, Stop> {
    let fatal = |error: cash_archive::rar::Error| {
        rar.console.err(&format!("\n{error}"));
        Stop::Aborted(code::FATAL)
    };
    let mut builder = add::rewriting_builder(opened, password)
        .map_err(fatal)?
        .comment(opened.archive.comment(password).ok().flatten())
        .comment_kept_plain(add::comment_stored_plain(opened))
        .layout(add::layout(rar, Some(add::old_bound(&opened.archive))));
    builder = add::encrypting_headers(rar, builder, password);
    if opened.archive.as_rar50().is_some() {
        let quick_open = add::quick_open_on(rar);
        let metadata = add::saved_metadata(rar, path, newest);
        builder = builder
            .archive_metadata(metadata, false, quick_open)
            .map_err(fatal)?;
    }
    Ok(builder)
}

/// "Deleting NAME" for each member going, in the archive's order, logged; a solid
/// archive's repacking said between them as rar goes through its members, every one
/// repacked when none goes.
fn say_deletions<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    names: &[String],
    gone: &[bool],
    solid: bool,
) {
    let mut repack = (solid && !rar.switches.no_percent).then(|| Repack::new(rar, None));
    for (name, &gone) in names.iter().zip(gone) {
        if !gone {
            if let Some(repack) = &mut repack {
                repack.kept(0);
            }
            continue;
        }
        if let Some(repack) = &mut repack {
            repack.dropped(0);
        }
        rar.console.msg(&format!("\nDeleting {name}"));
        rar.log_file(name);
    }
}

pub(super) fn run<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    parsed: &Parsed,
) -> Result<(), Stop> {
    let Some(archive) = parsed.archive.as_deref() else {
        return Ok(());
    };
    let masks = Masks::new(rar, parsed)?;
    add::adjusted_recovery(rar, rar.switches.recovery_record.as_deref());
    let display = super::cmdline::with_default_extension(archive).replace('\\', "/");
    let found = Found {
        display: display.clone(),
        path: rar.path(&display),
    };
    let opened = match open::open(rar, &found)? {
        Ok(opened) => opened,
        Err(failure) => return open::refuse(rar, &found, &failure).map_or(Ok(()), Err),
    };
    if opened.facts.locked {
        rar.console.err("\n\nERROR: Locked archive");
        return Err(Stop::Refused(code::LOCKED));
    }
    if opened.facts.volume {
        rar.console.err("\n\nERROR: Cannot modify volume");
        return Err(Stop::Refused(code::LOCKED));
    }
    if open::header_mode_refused(rar, &opened) {
        return Err(Stop::Refused(code::FATAL));
    }
    rar.console.msg(&format!("\nDeleting from {display}"));

    let members: Vec<cash_archive::rar::ArchiveMember> = opened.archive.members().collect();
    let names: Vec<String> = add::member_names(&opened.archive, &members);
    let gone: Vec<bool> = names.iter().map(|name| masks.wants(name)).collect();
    if gone.contains(&true) {
        rar.log_archive(&found.display);
    }
    say_deletions(rar, &names, &gone, opened.facts.solid);
    if !gone.contains(&true) {
        rar.console.msg("\nNo files to delete\n");
        rar.fail(code::NO_FILES);
        return Ok(());
    }
    if !gone.contains(&false) {
        rar.console
            .msg(&format!("\nErasing empty archive {display}"));
        if let Err(error) = std::fs::remove_file(&found.path) {
            rar.console.err(&format!(
                "\nCannot delete {display}\n{}",
                open::system_message(&error)
            ));
            rar.fail(code::WRITE);
            return Ok(());
        }
        if !rar.switches.no_done {
            rar.console.msg("\nDone\n");
        }
        return Ok(());
    }

    let password = add::data_password(rar)?;
    let kept: Vec<usize> = (0..members.len()).filter(|&index| !gone[index]).collect();
    // The newest file kept: `-tl` and `-ams` take its time.
    let newest = kept
        .iter()
        .filter(|&&index| !members[index].meta.is_directory)
        .filter_map(|&index| add::member_time(&members[index], &rar.zone))
        .max();
    let mut builder = rewriter(rar, &opened, &found.path, newest, password.as_deref())?;
    let mut keeper = add::Keeper::new(rar, &opened, &found.path, &kept, password.as_deref())?;
    for &index in &kept {
        if let Err(error) = keeper.keep(&mut builder, index, password.is_some()) {
            rar.console.err(&format!("\n{}\n{error}", names[index]));
            return Err(Stop::Aborted(code::FATAL));
        }
    }
    if let Some(percent) = add::asked_recovery_percent(rar) {
        builder = builder.recovery_percent(Some(percent));
        rar.console.msg("\nAdding the data recovery record     ");
    }
    let resources = WriterResources::default().with_temp_dir(add::work_dir(rar, &found.path));
    if let Err(error) = builder.write_to_path_with_resources(&found.path, &resources, None) {
        rar.console
            .err(&format!("\nCannot create {display}\n{error}"));
        rar.fail(code::CREATE);
        return Ok(());
    }
    if rar.switches.latest_time {
        add::stamp_time(&found.path, newest);
    }
    if !rar.switches.no_done {
        rar.console.msg("\nDone\n");
    }
    Ok(())
}
