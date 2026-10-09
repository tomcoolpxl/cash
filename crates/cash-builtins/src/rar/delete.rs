//! `d`: members taken out of an archive, a line each in the archive's order; the archive
//! erased when nothing is left in it.

use cash_archive::rar::WriterResources;

use super::add;
use super::cmdline::Parsed;
use super::list::Masks;
use super::open::{self, Found};
use super::{Rar, Stop, code};

pub(super) fn run<SE: cash_core::ShellExtensions>(
    rar: &Rar<'_, SE>,
    parsed: &Parsed,
) -> Result<(), Stop> {
    let Some(archive) = parsed.archive.as_deref() else {
        return Ok(());
    };
    let masks = Masks::new(rar, parsed)?;
    let display = super::cmdline::with_default_extension(archive).replace('\\', "/");
    let found = Found {
        display: display.clone(),
        path: rar.path(&display),
    };
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
    rar.console.msg(&format!("\nDeleting from {display}"));

    let members: Vec<cash_archive::rar::ArchiveMember> = opened.archive.members().collect();
    let names: Vec<String> = members.iter().map(add::member_name).collect();
    let gone: Vec<bool> = names.iter().map(|name| masks.wants(name)).collect();
    if !gone.contains(&true) {
        rar.console.msg("\nNo files to delete\n");
        rar.fail(code::NO_FILES);
        return Ok(());
    }
    for (name, _) in names.iter().zip(&gone).filter(|(_, gone)| **gone) {
        rar.console.msg(&format!("\nDeleting {name}"));
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
    let mut builder = match opened.archive.preserving_builder(password.as_deref()) {
        Ok(builder) => builder,
        Err(error) => {
            rar.console.err(&format!("\n{error}"));
            return Err(Stop::Aborted(code::FATAL));
        }
    };
    builder = builder.comment(opened.archive.comment(password.as_deref()).ok().flatten());
    builder = builder.layout(add::layout(rar));
    let kept: Vec<usize> = (0..members.len()).filter(|&index| !gone[index]).collect();
    let mut keeper = add::Keeper::new(rar, &opened, &found.path, &kept, password.as_deref())?;
    for &index in &kept {
        if let Err(error) = keeper.keep(&mut builder, index, password.is_some()) {
            rar.console.err(&format!("\n{}\n{error}", names[index]));
            return Err(Stop::Aborted(code::FATAL));
        }
    }
    let resources = WriterResources::default().with_temp_dir(add::work_dir(rar, &found.path));
    if let Err(error) = builder.write_to_path_with_resources(&found.path, &resources, None) {
        rar.console
            .err(&format!("\nCannot create {display}\n{error}"));
        rar.fail(code::CREATE);
        return Ok(());
    }
    if !rar.switches.no_done {
        rar.console.msg("\nDone\n");
    }
    Ok(())
}
