//! Path searching utilities.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use crate::sys;
use crate::sys::fs::PathExt;

/// Encapsulates the result of a path search.
pub struct ExecutablePathSearch<PI, N> {
    paths: VecDeque<PI>,
    filename: N,
    /// The shell's `PATHEXT`.
    extensions: Vec<String>,
}

impl<PI, N> Iterator for ExecutablePathSearch<PI, N>
where
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(path) = self.paths.pop_front() {
            let path = PathBuf::from(path.as_ref()).join(self.filename.as_ref());

            // Ask the platform to resolve the path to an actual executable file, which on
            // Windows may involve appending a PATHEXT extension. The helper takes ownership
            // so Unix — where no resolution is needed — can return the path unchanged
            // without allocating.
            //
            // A directory carries the execute bit on Unix but is never a command. Filter
            // the *resolved* path rather than the input: on Windows, resolution appends a
            // PATHEXT extension, so a `prog` directory must not stop `prog.exe` in the
            // same PATH entry from being found.
            if let Some(resolved) = sys::fs::resolve_executable(path, &self.extensions)
                && !resolved.is_dir()
            {
                return Some(resolved);
            }
        }

        None
    }
}

pub(crate) struct ExecutablePathPrefixSearch<PI> {
    paths: VecDeque<PI>,
    queued_items: VecDeque<PathBuf>,
    filename_prefix: String,
    /// The prefix as typed, which each folder is asked for.
    typed_prefix: String,
    case_insensitive: bool,
    /// The shell's `PATHEXT`.
    extensions: Vec<String>,
}

/// The entries of `dir` that may begin with `prefix`, each with whether it is a folder.
///
/// The folder is asked for them (`FindFirstFileExW` with `prefix*`), where it was listed
/// whole: a completion menu asked this of every `PATH` folder on each keystroke, 25 ms
/// with System32 among them. Windows matches an 8.3 alias too, so the caller still
/// compares the long names; a prefix with a character the pattern would read as a
/// wildcard lists the folder whole, as before.
fn entries_beginning(dir: &Path, prefix: &str) -> Vec<(std::ffi::OsString, bool)> {
    if !prefix.contains(['*', '?', '<', '>', '"']) {
        return cash_win32::fs::names_beginning(dir, prefix.as_ref()).unwrap_or_default();
    }
    dir.read_dir()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| {
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            (entry.file_name(), is_dir)
        })
        .collect()
}

impl<PI> Iterator for ExecutablePathPrefixSearch<PI>
where
    PI: AsRef<Path>,
{
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        // If we already found some items and queued them, then yield one now.
        if let Some(item) = self.queued_items.pop_front() {
            return Some(item);
        }

        while let Some(path) = self.paths.pop_front() {
            let path = PathBuf::from(path.as_ref());

            for (name, is_dir) in entries_beginning(&path, &self.typed_prefix) {
                if let Some(filename) = name.to_str() {
                    let compared = if self.case_insensitive {
                        cash_win32::fold::name_key(filename)
                    } else {
                        filename.to_owned()
                    };
                    if !compared.starts_with(&self.filename_prefix) {
                        continue;
                    }
                }

                // A folder is never a command; a file, or a link to one, is when it runs.
                let entry_path = path.join(&name);
                if !is_dir && entry_path.executable(&self.extensions) {
                    self.queued_items.push_back(entry_path);
                }
            }
            if let Some(item) = self.queued_items.pop_front() {
                return Some(item);
            }
        }

        None
    }
}

/// Search for the given executable name in the provided paths.
///
/// # Arguments
///
/// * `paths` - An iterator over the paths to search.
/// * `filename` - The name of the executable file to search for.
/// * `extensions` - The shell's `PATHEXT` (`Shell::pathext`).
pub fn search_for_executable<P, PI, N>(
    paths: P,
    filename: N,
    extensions: Vec<String>,
) -> ExecutablePathSearch<PI, N>
where
    P: Iterator<Item = PI>,
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    ExecutablePathSearch {
        paths: paths.collect(),
        filename,
        extensions,
    }
}

/// cash's own executable, when `name` asks for a POSIX shell or for cash.
///
/// Matches `sh` and `bash` by basename, with or without an extension, so `bash`,
/// `/bin/sh` and `bash.exe` all land here. Nothing else: `zsh`, `dash` and `pwsh` are
/// other shells and cash does not pretend to be them.
///
/// `cash` matches too, so `cash doctor` and `cash -c` work inside cash whether or not its
/// folder is on `PATH`: a terminal profile usually starts it by full path. Another cash on
/// `PATH` is then reached by its path, as a real bash is.
fn cash_as_shell(name: &Path) -> Option<PathBuf> {
    let stem = name.file_stem()?.to_str()?.to_ascii_lowercase();
    if matches!(stem.as_str(), "sh" | "bash" | "cash") {
        return std::env::current_exe().ok();
    }
    None
}

/// Whether running `name` starts cash itself rather than searching `PATH` (D7).
pub(crate) fn runs_cash_itself(name: &str) -> bool {
    cash_as_shell(Path::new(name)).is_some()
}

/// Resolves a command name the way the shell does when it is about to run it.
///
/// Returns the first executable in search order, or -- if there is none -- the first entry
/// that exists and is not a directory, which the shell reports as the command and then
/// fails to run.
///
/// # Arguments
///
/// * `paths` - An iterator over the paths to search.
/// * `filename` - The name of the command to resolve.
/// * `extensions` - The shell's `PATHEXT` (`Shell::pathext`).
pub fn resolve_command<P, PI, N>(paths: P, filename: N, extensions: &[String]) -> Option<PathBuf>
where
    P: IntoIterator<Item = PI>,
    PI: AsRef<Path>,
    N: AsRef<Path>,
{
    // cash (D7): `sh` and `bash` resolve to cash itself, before `PATH` is consulted.
    //
    // D7 already gives `/usr/bin/env` a virtual resolution so `#!/usr/bin/env bash`
    // works without a fake filesystem. The same argument applies here and the stakes are
    // higher: on Windows `bash` resolves to `C:\WINDOWS\system32\bash.exe`, which is the
    // **WSL launcher**. A script running `bash helper.sh` would silently continue under
    // Linux, with a different filesystem view and none of cash's path guarantees — and
    // `sh` resolves to nothing at all, so `#!/bin/sh` had no interpreter.
    //
    // A real bash is still reachable by full path, which is what a script that genuinely
    // wants one should be using anyway.
    if let Some(own) = cash_as_shell(filename.as_ref()) {
        return Some(own);
    }

    let mut first_non_executable = None;

    for dir in paths {
        let dir = dir.as_ref();
        // The command's names that are in the folder, from one look at it: a look for each
        // name it could have, two for the name and one for every PATHEXT extension, cost
        // about fifteen calls a folder, `type -P cmd` 3 ms over a long PATH. What is found
        // is still checked as before, so a folder that tells case apart answers as it did.
        let listed = listed_names(dir, filename.as_ref());
        if listed.as_ref().is_some_and(Vec::is_empty) {
            continue;
        }
        let present = |name: &std::ffi::OsStr| {
            listed.as_ref().is_none_or(|listed| {
                listed.contains(&cash_win32::fold::name_key(&name.to_string_lossy()))
            })
        };
        let path = dir.join(filename.as_ref());

        // Remember the first non-directory entry seen, in case no executable turns up.
        if first_non_executable.is_none()
            && path.file_name().is_some_and(present)
            && path.metadata().is_ok_and(|m| !m.is_dir())
        {
            first_non_executable = Some(path.clone());
        }

        // Resolve, then reject directories; see `ExecutablePathSearch::next`.
        if let Some(resolved) = sys::fs::resolve_executable_among(path, extensions, present)
            && !resolved.is_dir()
        {
            return Some(resolved);
        }
    }

    first_non_executable
}

/// The names in the folder `dir` that begin with `name`, folded, from one look at it; none
/// where that look cannot stand for a look for each name `name` could have. Windows reads
/// some names otherwise: a wildcard, a folder part, a stream, an 8.3 alias's `~`, a
/// trailing dot or space; an empty folder is the working one; a folder that cannot be
/// read is looked in as before.
fn listed_names(dir: &Path, name: &Path) -> Option<Vec<String>> {
    let text = name.to_str()?;
    if dir.as_os_str().is_empty()
        || text.is_empty()
        || text.contains(['*', '?', '/', '\\', ':', '~', '<', '>', '"'])
        || text.ends_with(['.', ' '])
    {
        return None;
    }
    let names = cash_win32::fs::names_beginning(dir, name.as_os_str()).ok()?;
    Some(
        names
            .iter()
            .map(|(found, _)| cash_win32::fold::name_key(&found.to_string_lossy()))
            .collect(),
    )
}

pub(crate) fn search_for_executable_with_prefix<P, PI>(
    paths: P,
    filename_prefix: &str,
    case_insensitive: bool,
    extensions: Vec<String>,
) -> ExecutablePathPrefixSearch<PI>
where
    P: Iterator<Item = PI>,
    PI: AsRef<Path>,
{
    let stored_prefix = if case_insensitive {
        cash_win32::fold::name_key(filename_prefix)
    } else {
        filename_prefix.into()
    };

    ExecutablePathPrefixSearch {
        paths: paths.collect(),
        queued_items: VecDeque::new(),
        filename_prefix: stored_prefix,
        typed_prefix: filename_prefix.to_owned(),
        case_insensitive,
        extensions,
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use anyhow::Result;

    use super::*;

    /// Each folder is asked for the names beginning with the prefix: programs that begin
    /// with it are found, in either case when asked to ignore case, and a folder or a
    /// file that is no program is not; a prefix with a wildcard in it lists the folder whole
    /// and finds nothing, as no name holds one.
    #[test]
    fn a_prefix_finds_the_programs_that_begin_with_it() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        for name in ["abc.exe", "ABD.cmd", "abe.txt", "xyz.exe"] {
            std::fs::write(scratch.path().join(name), "")?;
        }
        std::fs::create_dir(scratch.path().join("abf.exe"))?;
        let pathext = cash_win32::resolve::pathext_of(None);
        let names = |prefix: &str, case_insensitive: bool| {
            let mut names: Vec<String> = search_for_executable_with_prefix(
                std::iter::once(scratch.path()),
                prefix,
                case_insensitive,
                pathext.clone(),
            )
            .filter_map(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
            .collect();
            names.sort();
            names
        };
        assert_eq!(names("ab", true), ["ABD.cmd", "abc.exe"]);
        assert_eq!(names("ab", false), ["abc.exe"]);
        // No file can have one: the folder is listed whole, and nothing begins with it.
        assert_eq!(names("a*", true), Vec::<String>::new());
        assert_eq!(names("", true), ["ABD.cmd", "abc.exe", "xyz.exe"]);
        Ok(())
    }

    /// A directory whose name matches must not be mistaken for a command; an executable
    /// later in the search order takes its place.
    #[test]
    fn directory_is_not_a_command() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        let first = scratch.path().join("first");
        let second = scratch.path().join("second");
        std::fs::create_dir_all(first.join("prog"))?;
        std::fs::create_dir_all(&second)?;
        std::fs::write(second.join("prog"), "")?;

        let paths = [first.as_path(), second.as_path()];

        // The plain (non-executable) file wins over the directory, rather than the
        // directory being reported as the command.
        let pathext = cash_win32::resolve::pathext_of(None);
        assert_eq!(
            resolve_command(paths, "prog", &pathext),
            Some(second.join("prog"))
        );

        // ...and a directory is never yielded as an executable at all.
        assert_eq!(
            search_for_executable(paths.iter(), "prog", pathext).next(),
            None
        );

        Ok(())
    }

    /// On Windows, `PATHEXT` resolution has to run before directories are rejected: a
    /// `prog` directory must not keep `prog.bat` in the same PATH entry from being found.
    #[test]
    fn same_named_directory_does_not_hide_a_pathext_match() -> Result<()> {
        let scratch = tempfile::tempdir()?;
        std::fs::create_dir_all(scratch.path().join("prog"))?;
        std::fs::write(scratch.path().join("prog.bat"), "@echo off\r\n")?;

        let paths = [scratch.path()];
        let pathext = cash_win32::resolve::pathext_of(None);
        for found in [
            search_for_executable(paths.iter(), "prog", pathext.clone()).next(),
            resolve_command(paths, "prog", &pathext),
        ] {
            assert!(
                found.as_ref().is_some_and(|path| path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bat"))),
                "unexpected resolution: {found:?}"
            );
        }

        Ok(())
    }
}
