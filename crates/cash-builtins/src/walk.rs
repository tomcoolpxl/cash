//! The folder walks the builtins share (decisions.md, "Builtins share their option parsing,
//! input opening and folder walks"; `research/central-components-design.md`).
//!
//! Built on walkdir, which iterates rather than recurses, follows links or does not, finds
//! a loop by file identity and names the folder it goes back to, sorts, limits the depth,
//! keeps to one file system and prunes. cash adds what every walk here needs beside it:
//! - the name each entry is shown by: the root as the user gave it, then `/name`s, with no
//!   doubled `/`;
//! - one answer to what a link is: a symlink or a junction (a name surrogate), as std and
//!   walkdir say, never another reparse point (a cloud placeholder, a deduplicated file),
//!   which is the file or folder it holds;
//! - the stop token, asked before each entry;
//! - every error handed to the tool, which words it, sets its status and goes on.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use cash_core::StopToken;

/// Which links a walk goes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Links {
    /// None: a link is an entry of its own, [`Kind::Link`].
    Never,
    /// The root's, when the root is one (`find -H`, `grep -r` on a linked folder).
    Roots,
    /// Every one (`find -L`, `grep -R`), a loop reported.
    All,
}

/// The order of a folder's entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Order {
    /// As Windows lists them.
    #[expect(dead_code, reason = "chmod -R walks in this order, next")]
    Listing,
    /// By name, unit for unit: GNU's tools in the C locale (`find`, `grep`, `tree`).
    Ordinal,
    /// By name, without regard to case.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "where -r walks in this order, next")
    )]
    Folded,
}

/// What a walk does.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WalkOptions {
    pub(crate) links: Links,
    pub(crate) order: Order,
    /// A folder after what is in it (`find -depth`, `rm -r`).
    pub(crate) contents_first: bool,
    /// The shallowest entry handed out: 1 leaves the root out.
    pub(crate) min_depth: usize,
    /// The deepest entry handed out.
    pub(crate) max_depth: usize,
    /// No folder on another volume is gone into.
    pub(crate) one_file_system: bool,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            links: Links::Never,
            order: Order::Ordinal,
            contents_first: false,
            min_depth: 0,
            max_depth: usize::MAX,
            one_file_system: false,
        }
    }
}

/// What an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    File,
    Folder,
    /// A symlink or a junction the walk does not go through.
    Link,
    /// Anything else: a device, a socket.
    Other,
}

/// One entry of a walk.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The name it is shown by: the root as given, then `/`-joined names.
    pub(crate) shown: String,
    /// Where it is, losslessly.
    pub(crate) path: PathBuf,
    pub(crate) kind: Kind,
    /// walkdir's entry; none for a dangling link ([`WalkError::Dangling`]).
    listed: Option<walkdir::DirEntry>,
}

impl Entry {
    /// Its last name.
    pub(crate) fn name(&self) -> &OsStr {
        self.path
            .file_name()
            .unwrap_or_else(|| self.path.as_os_str())
    }

    /// What it is, through a link the walk follows; free from the listing where Windows
    /// gave it. A dangling link's is its own.
    pub(crate) fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        match &self.listed {
            Some(entry) => entry.metadata().map_err(into_io),
            None => std::fs::symlink_metadata(&self.path),
        }
    }
}

/// What went wrong in a walk, for the tool to word.
#[derive(Debug)]
pub(crate) enum WalkError {
    /// A followed link back to a folder above it: its name, and the folder's.
    Loop { shown: String, ancestor: String },
    /// A link the walk follows whose target is not there: GNU's `grep -R` says so, its
    /// `find -L` lists the link as it is.
    Dangling(Entry),
    /// A folder that could not be listed, or an entry that could not be looked at.
    Io {
        shown: String,
        error: std::io::Error,
    },
    /// The stop token asked: the walk ends here.
    Stopped,
}

/// A walk of the tree under one root.
pub(crate) struct Walk {
    inner: walkdir::IntoIter,
    links: Links,
    root: PathBuf,
    shown: String,
    stop: StopToken,
    stopped: bool,
}

impl Walk {
    /// A walk of `actual`, shown as `shown`: what the user typed, empty for the working
    /// directory searched with no name.
    pub(crate) fn new(shown: &str, actual: &Path, options: &WalkOptions, stop: StopToken) -> Self {
        let mut walk = walkdir::WalkDir::new(actual)
            .follow_links(options.links == Links::All)
            .follow_root_links(options.links != Links::Never)
            .contents_first(options.contents_first)
            .min_depth(options.min_depth)
            .max_depth(options.max_depth)
            .same_file_system(options.one_file_system);
        walk = match options.order {
            Order::Listing => walk,
            Order::Ordinal => walk.sort_by_file_name(),
            Order::Folded => walk.sort_by(|a, b| folded(a.file_name()).cmp(&folded(b.file_name()))),
        };
        Self {
            inner: walk.into_iter(),
            links: options.links,
            root: actual.to_path_buf(),
            shown: shown.to_owned(),
            stop,
            stopped: false,
        }
    }

    /// Goes no further into the folder just handed out (`find -prune`, an excluded
    /// folder).
    pub(crate) fn skip_folder(&mut self) {
        self.inner.skip_current_dir();
    }

    /// The name `path`, under the root, is shown by.
    fn shown(&self, path: &Path) -> String {
        let below: Vec<String> = path
            .strip_prefix(&self.root)
            .map(|rest| {
                rest.components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        join_shown(&self.shown, &below.join("/"))
    }

    fn entry(&self, entry: walkdir::DirEntry) -> Entry {
        let kind = kind(entry.file_type());
        Entry {
            shown: self.shown(entry.path()),
            path: entry.path().to_path_buf(),
            kind,
            listed: Some(entry),
        }
    }

    fn error(&self, error: walkdir::Error) -> WalkError {
        let shown = error
            .path()
            .map_or_else(|| self.shown.clone(), |path| self.shown(path));
        if let Some(ancestor) = error.loop_ancestor() {
            return WalkError::Loop {
                shown,
                ancestor: self.shown(ancestor),
            };
        }
        let dangling = error.path().filter(|path| {
            self.links != Links::Never
                && error
                    .io_error()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
                && std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
        });
        if let Some(path) = dangling {
            return WalkError::Dangling(Entry {
                shown,
                path: path.to_path_buf(),
                kind: Kind::Link,
                listed: None,
            });
        }
        WalkError::Io {
            shown,
            error: into_io(error),
        }
    }
}

impl Iterator for Walk {
    type Item = Result<Entry, WalkError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.stopped {
            return None;
        }
        if self.stop.requested() {
            self.stopped = true;
            return Some(Err(WalkError::Stopped));
        }
        Some(match self.inner.next()? {
            Ok(entry) => Ok(self.entry(entry)),
            Err(error) => Err(self.error(error)),
        })
    }
}

/// What a listing's entry is, by the one answer the builtins give: a link is a symlink or
/// a junction (a name surrogate), as std says; another reparse point (a cloud
/// placeholder, a deduplicated file) is the file or folder it holds. For a walk of its
/// own that needs a folder whole before it shows any of it, as `tree` draws its branches.
pub(crate) fn kind(file_type: std::fs::FileType) -> Kind {
    if file_type.is_symlink() {
        Kind::Link
    } else if file_type.is_dir() {
        Kind::Folder
    } else if file_type.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

/// `below` under `root` as a name is shown: `root/below`, `root` alone, or `below` alone
/// for the empty root; a root that ends with a separator gets no second one.
fn join_shown(root: &str, below: &str) -> String {
    if below.is_empty() {
        root.to_owned()
    } else if root.is_empty() {
        below.to_owned()
    } else if root.ends_with(['/', '\\']) {
        format!("{root}{below}")
    } else {
        format!("{root}/{below}")
    }
}

/// A name without regard to case, for [`Order::Folded`].
fn folded(name: &OsStr) -> String {
    name.to_string_lossy().to_lowercase()
}

/// walkdir's error as the I/O error under it, or one that says it.
fn into_io(error: walkdir::Error) -> std::io::Error {
    let text = error.to_string();
    error
        .into_io_error()
        .unwrap_or_else(|| std::io::Error::other(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walked(root: &Path, shown: &str, options: &WalkOptions) -> Vec<String> {
        Walk::new(shown, root, options, StopToken::never())
            .map(|next| match next {
                Ok(entry) => format!("{} {:?}", entry.shown, entry.kind),
                Err(WalkError::Loop { shown, ancestor }) => format!("{shown} loops to {ancestor}"),
                Err(WalkError::Dangling(entry)) => format!("{} dangling", entry.shown),
                Err(WalkError::Io { shown, error }) => format!("{shown} {error}"),
                Err(WalkError::Stopped) => "stopped".to_owned(),
            })
            .collect()
    }

    #[test]
    fn names_are_shown_as_given_and_joined_once() {
        assert_eq!(join_shown("dir", "a/b"), "dir/a/b");
        assert_eq!(join_shown("dir/", "a"), "dir/a");
        assert_eq!(join_shown("C:\\x\\", "a"), "C:\\x\\a");
        assert_eq!(join_shown("", "a"), "a");
        assert_eq!(join_shown("dir", ""), "dir");
    }

    #[test]
    fn a_tree_is_walked_in_order_with_links_as_asked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("b/deep")).unwrap();
        std::fs::write(root.join("B.txt"), "x").unwrap();
        std::fs::write(root.join("a.txt"), "x").unwrap();
        std::fs::write(root.join("b/deep/f"), "x").unwrap();
        let ordinal = WalkOptions {
            min_depth: 1,
            ..WalkOptions::default()
        };
        assert_eq!(
            walked(root, "top", &ordinal),
            [
                "top/B.txt File",
                "top/a.txt File",
                "top/b Folder",
                "top/b/deep Folder",
                "top/b/deep/f File"
            ]
        );
        let folded = WalkOptions {
            order: Order::Folded,
            max_depth: 1,
            min_depth: 1,
            ..WalkOptions::default()
        };
        assert_eq!(
            walked(root, "top/", &folded),
            ["top/a.txt File", "top/b Folder", "top/B.txt File"]
        );
        // A junction back to the root: a link not followed, a loop when followed.
        let link = root.join("b").join("up");
        cash_win32::junction::point(&link, root).unwrap();
        let never = WalkOptions {
            min_depth: 1,
            max_depth: 2,
            ..WalkOptions::default()
        };
        assert!(walked(root, "top", &never).contains(&"top/b/up Link".to_owned()));
        let all = WalkOptions {
            links: Links::All,
            min_depth: 1,
            ..WalkOptions::default()
        };
        assert!(
            walked(root, "top", &all).contains(&"top/b/up loops to top".to_owned()),
            "{:?}",
            walked(root, "top", &all)
        );
        std::fs::remove_dir(&link).unwrap();
    }
}
