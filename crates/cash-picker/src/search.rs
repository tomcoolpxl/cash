//! The deeper search behind typing: every entry below the root, nearest first, read on a
//! thread of its own so that drawing and keys never wait for the disk (spec D73).
//!
//! It walks level by level with the tree's own reading ([`crate::tree::read`]), so it
//! leaves out what the tree leaves out, and stops at [`MAX_ENTRIES`] or [`MAX_TIME`].

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::tree::{self, Filter, Ignores};

/// Entries a search collects at most.
pub const MAX_ENTRIES: usize = 100_000;

/// How long a search runs at most.
pub const MAX_TIME: Duration = Duration::from_secs(4);

/// An entry the search found.
#[derive(Clone, Debug)]
pub struct Found {
    /// Where it is.
    pub path: PathBuf,
    /// Its path below the root, with `/` between names.
    pub relative: String,
    /// A folder rather than a file.
    pub folder: bool,
}

/// How far a search got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Still reading.
    Running,
    /// Read everything below the root.
    Done,
    /// Stopped at the entry or time limit.
    Cut,
}

/// A search running in the background; dropping it stops the thread.
pub struct Search {
    found: Arc<Mutex<Vec<Found>>>,
    status: Arc<AtomicU8>,
    stop: Arc<AtomicBool>,
}

impl Search {
    /// Starts searching below `root` for what `filter` lists.
    #[must_use]
    pub fn start(root: &Path, filter: Filter) -> Self {
        let search = Self {
            found: Arc::default(),
            status: Arc::new(AtomicU8::new(0)),
            stop: Arc::default(),
        };
        let (found, status, stop) = (
            Arc::clone(&search.found),
            Arc::clone(&search.status),
            Arc::clone(&search.stop),
        );
        let root = root.to_path_buf();
        let started = std::thread::Builder::new()
            .name("croot search".to_owned())
            .spawn(move || {
                let end = walk(&root, filter, &found, &stop);
                status.store(end as u8, Ordering::Release);
            });
        if started.is_err() {
            search.status.store(Status::Cut as u8, Ordering::Release);
        }
        search
    }

    /// How far it got.
    #[must_use]
    pub fn status(&self) -> Status {
        match self.status.load(Ordering::Acquire) {
            0 => Status::Running,
            1 => Status::Done,
            _ => Status::Cut,
        }
    }

    /// How many entries it has found so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.found.lock().map_or(0, |found| found.len())
    }

    /// Whether it has found nothing yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Calls `f` with what it has found so far.
    pub fn with_found<R>(&self, f: impl FnOnce(&[Found]) -> R) -> R {
        match self.found.lock() {
            Ok(found) => f(&found),
            Err(poisoned) => f(&poisoned.into_inner()),
        }
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// Reads level by level below `root`; the status it ended with, as stored.
fn walk(root: &Path, filter: Filter, found: &Mutex<Vec<Found>>, stop: &AtomicBool) -> Status {
    let started = Instant::now();
    let mut ignores = Ignores::default();
    let mut pending: VecDeque<(PathBuf, String)> =
        VecDeque::from([(root.to_path_buf(), String::new())]);
    let mut count = 0usize;
    while let Some((folder, prefix)) = pending.pop_front() {
        if stop.load(Ordering::Acquire) {
            return Status::Cut;
        }
        if count >= MAX_ENTRIES || started.elapsed() >= MAX_TIME {
            return Status::Cut;
        }
        // Folders are walked whatever the picker lists, to find the files below them.
        let entries = tree::read(
            &folder,
            Filter {
                shows: crate::context::Shows::Everything,
                ..filter
            },
            &mut ignores,
        );
        let mut batch = Vec::with_capacity(entries.len());
        for entry in entries {
            let relative = if prefix.is_empty() {
                entry.name.clone()
            } else {
                format!("{prefix}/{}", entry.name)
            };
            if entry.folder {
                pending.push_back((entry.path.clone(), relative.clone()));
            }
            if entry.folder || filter.shows == crate::context::Shows::Everything {
                batch.push(Found {
                    path: entry.path,
                    relative,
                    folder: entry.folder,
                });
            }
        }
        count += batch.len();
        if let Ok(mut found) = found.lock() {
            found.extend(batch);
        }
    }
    Status::Done
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Shows;

    #[test]
    fn it_finds_entries_level_by_level() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a").join("deep")).unwrap();
        std::fs::write(dir.path().join("a").join("deep").join("x.txt"), "").unwrap();
        std::fs::write(dir.path().join("top.txt"), "").unwrap();
        let filter = Filter {
            shows: Shows::Everything,
            hidden: false,
            ignored: false,
            by_date: false,
        };
        let search = Search::start(dir.path(), filter);
        let deadline = Instant::now() + Duration::from_secs(10);
        while search.status() == Status::Running && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(search.status(), Status::Done);
        let found: Vec<String> =
            search.with_found(|f| f.iter().map(|f| f.relative.clone()).collect());
        assert_eq!(found, ["a", "top.txt", "a/deep", "a/deep/x.txt"]);

        let folders = Search::start(
            dir.path(),
            Filter {
                shows: Shows::Folders,
                ..filter
            },
        );
        while folders.status() == Status::Running && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let found: Vec<String> =
            folders.with_found(|f| f.iter().map(|f| f.relative.clone()).collect());
        assert_eq!(found, ["a", "a/deep"]);
    }
}
