//! A listing of the directories on `PATH`, for questions that must not wait on the disk.
//!
//! Resolving a command name probes the file system: on Windows every `PATH` directory is
//! tried with every `PATHEXT` extension. With 86 directories and 13 extensions, a name
//! that exists nowhere costs over 1,100 probes -- 127 ms on the machine this was written
//! on. That is tolerable once per command run. It is not tolerable once per keystroke,
//! which is how often syntax highlighting asks whether the command word exists.
//!
//! The index lists each directory once, on a thread of its own, and answers from memory.
//! It never blocks the caller: until a listing is ready it answers `None`, which callers
//! treat as "don't know". A directory is listed again when its modification time changes,
//! which NTFS and ext4 bump when an entry is added, removed or renamed; the times are
//! checked in the background at most every [`REVALIDATE_EVERY`]. A newly installed
//! program can therefore take a moment to be recognised. That is acceptable for choosing a
//! colour, and is why nothing that *runs* a command consults this index.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime},
};

/// How often the directories' modification times are checked again.
pub const REVALIDATE_EVERY: Duration = Duration::from_secs(2);

/// A shared, lazily built index of the executables on `PATH`.
///
/// Clones share one index, so a subshell or a second highlighter costs nothing.
#[derive(Clone)]
pub struct PathIndex {
    inner: Arc<Mutex<Inner>>,
    revalidate_every: Duration,
}

impl Default for PathIndex {
    fn default() -> Self {
        Self::with_revalidate_interval(REVALIDATE_EVERY)
    }
}

impl std::fmt::Debug for PathIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathIndex").finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Inner {
    /// What the listings in `dirs` were made for; `None` until the first refresh lands.
    key: Option<Key>,
    dirs: HashMap<PathBuf, Listing>,
    refreshing: bool,
    last_refresh_started: Option<Instant>,
}

/// The inputs an answer depends on: the search directories, in order, and the
/// extensions that make a file runnable by name.
#[derive(Clone, PartialEq, Eq)]
struct Key {
    dirs: Vec<PathBuf>,
    extensions: Vec<String>,
}

struct Listing {
    /// The directory's modification time when it was listed; `None` if it could not be
    /// read, so it is listed again on every refresh.
    modified: Option<SystemTime>,
    /// The directory's non-directory entries, folded for comparison.
    names: HashSet<String>,
    /// Entries matched by their bare name, whose contents or mode had to be checked.
    checked: HashMap<String, bool>,
}

impl PathIndex {
    /// An index that checks directory modification times at most every `interval`.
    #[must_use]
    pub fn with_revalidate_interval(interval: Duration) -> Self {
        Self {
            inner: Arc::default(),
            revalidate_every: interval,
        }
    }

    /// Whether running `name` from `dirs` would find an executable: `Some(true)` or
    /// `Some(false)`, or `None` while the listing for these directories is not ready yet.
    ///
    /// `name` must not contain a path separator. Never waits on the file system, except
    /// once per matching file named without an extension (`egrep`), whose contents decide
    /// whether it runs; that answer is remembered until the directory is listed again.
    pub fn contains_executable(&self, dirs: &[PathBuf], name: &str) -> Option<bool> {
        let key = Key {
            dirs: dirs.to_vec(),
            extensions: executable_extensions(),
        };

        let mut inner = self.lock();
        self.start_refresh_if_due(&mut inner, &key);
        if inner.key.as_ref() != Some(&key) {
            return None;
        }

        let wanted = fold(name);
        let named_by_extension = has_listed_extension(&wanted, &key.extensions);
        let mut needs_check = None;

        for dir in &key.dirs {
            let Some(listing) = inner.dirs.get(dir) else {
                continue;
            };
            if named_by_extension && listing.names.contains(&wanted) {
                return Some(true);
            }
            if key
                .extensions
                .iter()
                .any(|ext| listing.names.contains(&format!("{wanted}{ext}")))
            {
                return Some(true);
            }
            if !named_by_extension && listing.names.contains(&wanted) {
                match listing.checked.get(&wanted) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => {
                        needs_check = Some(dir.clone());
                        break;
                    }
                }
            }
        }

        let Some(dir) = needs_check else {
            return Some(false);
        };

        // Check the file without holding the lock, then remember the answer and look
        // again: the directories after this one may still hold a match.
        drop(inner);
        let runnable = runs_by_bare_name(&dir.join(name));
        if let Some(listing) = self.lock().dirs.get_mut(&dir) {
            listing.checked.insert(wanted, runnable);
        }
        self.contains_executable(dirs, name)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves only a stale listing behind.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn start_refresh_if_due(&self, inner: &mut Inner, key: &Key) {
        let due = inner.key.as_ref() != Some(key)
            || inner
                .last_refresh_started
                .is_none_or(|started| started.elapsed() >= self.revalidate_every);
        if !due || inner.refreshing {
            return;
        }

        let known: HashMap<PathBuf, SystemTime> = inner
            .dirs
            .iter()
            .filter_map(|(dir, listing)| Some((dir.clone(), listing.modified?)))
            .collect();

        let index = self.clone();
        let key = key.clone();
        let spawned = std::thread::Builder::new()
            .name("cash-path-index".into())
            .spawn(move || index.refresh(key, &known));

        // Where threads are unavailable the index simply never becomes ready.
        if spawned.is_ok() {
            inner.refreshing = true;
            inner.last_refresh_started = Some(Instant::now());
        }
    }

    /// Lists the directories of `key` whose modification time differs from `known`, and
    /// installs the result.
    fn refresh(&self, key: Key, known: &HashMap<PathBuf, SystemTime>) {
        let mut fresh = HashMap::new();
        for dir in &key.dirs {
            if fresh.contains_key(dir) {
                continue;
            }
            // Take the time before listing: a change made in between then shows up as a
            // newer time on the next refresh, instead of being lost.
            let modified = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
            if modified.is_some() && known.get(dir) == modified.as_ref() {
                continue;
            }
            fresh.insert(dir.clone(), Listing::read(dir, modified));
        }

        let mut inner = self.lock();
        inner.dirs.retain(|dir, _| key.dirs.contains(dir));
        inner.dirs.extend(fresh);
        inner.key = Some(key);
        inner.refreshing = false;
    }
}

impl Listing {
    fn read(dir: &Path, modified: Option<SystemTime>) -> Self {
        let names = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    // A directory is never a command, even one named like a program.
                    .filter(|entry| entry.file_type().is_ok_and(|t| !t.is_dir()))
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .map(|name| fold(&name))
                    .collect()
            })
            .unwrap_or_default();

        Self {
            modified,
            names,
            checked: HashMap::new(),
        }
    }
}

/// The extensions that make a file runnable by its bare name, each with its leading dot
/// and folded: `PATHEXT` on Windows, none elsewhere.
fn executable_extensions() -> Vec<String> {
    #[cfg(windows)]
    {
        crate::sys::fs::pathext_extensions()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Folds a file name for comparison: Windows file names are case-insensitive.
fn fold(name: &str) -> String {
    if cfg!(windows) {
        name.to_lowercase()
    } else {
        name.to_owned()
    }
}

fn has_listed_extension(folded_name: &str, extensions: &[String]) -> bool {
    extensions
        .iter()
        .any(|ext| folded_name.len() > ext.len() && folded_name.ends_with(ext.as_str()))
}

/// Whether a file found under the exact name typed would run: its mode on Unix; its
/// contents (a `#!` line or a PE image) on Windows, as execution decides (D46).
fn runs_by_bare_name(path: &Path) -> bool {
    use crate::sys::fs::PathExt;
    path.is_file() && path.executable()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asks until the background listing answers, for at most five seconds.
    fn settled(index: &PathIndex, dirs: &[PathBuf], name: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(answer) = index.contains_executable(dirs, name) {
                return answer;
            }
            assert!(Instant::now() < deadline, "the index never became ready");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn write_program(path: &Path) {
        #[cfg(windows)]
        std::fs::write(path, b"MZ").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(path, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    /// A name the platform runs by its bare name: `tool.exe` on Windows, `tool` elsewhere.
    fn program_file(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        }
    }

    #[test]
    fn answers_none_until_listed_then_from_memory() {
        let dir = tempfile::tempdir().unwrap();
        write_program(&dir.path().join(program_file("tool")));
        let dirs = vec![dir.path().to_path_buf()];
        let index = PathIndex::default();

        assert_eq!(index.contains_executable(&dirs, "tool"), None);
        assert!(settled(&index, &dirs, "tool"));
        assert!(!settled(&index, &dirs, "no-such-tool"));
    }

    #[test]
    fn a_directory_is_not_a_command() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("prog")).unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let index = PathIndex::default();

        assert!(!settled(&index, &dirs, "prog"));
    }

    #[test]
    fn a_new_program_is_seen_once_the_directory_is_listed_again() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let index = PathIndex::with_revalidate_interval(Duration::ZERO);
        assert!(!settled(&index, &dirs, "late"));

        write_program(&dir.path().join(program_file("late")));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !settled(&index, &dirs, "late") {
            assert!(
                Instant::now() < deadline,
                "the new program was never listed"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_changed_path_is_not_answered_from_the_old_listing() {
        let with_tool = tempfile::tempdir().unwrap();
        let without = tempfile::tempdir().unwrap();
        write_program(&with_tool.path().join(program_file("tool")));
        let index = PathIndex::default();

        assert!(settled(&index, &[with_tool.path().to_path_buf()], "tool"));
        assert!(!settled(&index, &[without.path().to_path_buf()], "tool"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_names_are_case_insensitive_and_pathext_is_honoured() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Build.CMD"), b"@echo off\r\n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"not a program").unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let index = PathIndex::default();

        assert!(settled(&index, &dirs, "build"));
        assert!(settled(&index, &dirs, "BUILD.cmd"));
        assert!(!settled(&index, &dirs, "notes"));
        assert!(!settled(&index, &dirs, "notes.txt"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_bare_names_run_only_by_content() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("egrep"),
            b"#!/bin/sh\nexec grep -E \"$@\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("LICENSE"), b"MIT License\n").unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let index = PathIndex::default();

        assert!(settled(&index, &dirs, "egrep"));
        assert!(!settled(&index, &dirs, "license"));
    }

    #[test]
    fn a_later_directory_is_searched_after_a_bare_name_that_does_not_run() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::write(first.path().join("tool"), b"plain text\n").unwrap();
        write_program(&second.path().join(program_file("tool")));
        let dirs = vec![first.path().to_path_buf(), second.path().to_path_buf()];
        let index = PathIndex::default();

        assert!(settled(&index, &dirs, "tool"));
    }
}
