//! The directory walks the builtins share (decisions.md, "Builtins share their option parsing,
//! input opening and directory walks"; `research/central-components-design.md`).
//!
//! Built on walkdir, which iterates rather than recurses, sorts, limits the depth, keeps
//! to one file system and prunes. cash adds what every walk here needs beside it:
//! - the name each entry is shown by: the root as the user gave it, then `/name`s, with no
//!   doubled `/`;
//! - one answer to what a link is: a symlink or a junction (a name surrogate), as std and
//!   walkdir say, never another reparse point (a cloud placeholder, a deduplicated file),
//!   which is the file or directory it holds;
//! - the links below a root followed here, each into a walk of its own, and a loop found
//!   by file identity: walkdir's own following opens every directory it goes into to read,
//!   for its loop check, so a directory that cannot be read was lost, and its error said
//!   without a name;
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
    /// The root's, when the root is one (`find -H`, `grep -r` on a linked directory).
    Roots,
    /// Every one (`find -L`, `grep -R`), a loop reported.
    All,
}

/// The order of a directory's entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Order {
    /// As Windows lists them.
    Listing,
    /// By name, unit for unit: GNU's tools in the C locale (`find`, `grep`, `tree`).
    Ordinal,
    /// A directory's files before its directories, each by name as Windows folds names:
    /// `where`, as `where.exe` lists them.
    FilesFirst,
}

/// What a walk does.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WalkOptions {
    pub(crate) links: Links,
    pub(crate) order: Order,
    /// A directory after what is in it (`find -depth`, `rm -r`).
    pub(crate) contents_first: bool,
    /// The shallowest entry handed out: 1 leaves the root out.
    pub(crate) min_depth: usize,
    /// The deepest entry handed out.
    pub(crate) max_depth: usize,
    /// No directory on another volume is gone into.
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
    Directory,
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
    /// How deep: 0 for the root.
    pub(crate) depth: usize,
    pub(crate) kind: Kind,
    /// walkdir's entry; none for a root link whose target is not there.
    listed: Option<walkdir::DirEntry>,
    /// A link the walk went through: what it is, is what it names.
    followed: bool,
    /// What gives the facts of an entry of cash's own (D83), which is no file on disk:
    /// cash's own directory for a directory, cash for a command, as `ls -l` shows them.
    stand_in: Option<PathBuf>,
}

impl Entry {
    /// Its last name.
    pub(crate) fn name(&self) -> &OsStr {
        self.path
            .file_name()
            .unwrap_or_else(|| self.path.as_os_str())
    }

    /// What it is itself, a link not followed: free from the listing where Windows gave
    /// it, a link's read again.
    pub(crate) fn own_metadata(&self) -> std::io::Result<std::fs::Metadata> {
        if let Some(stand_in) = &self.stand_in {
            return std::fs::metadata(stand_in);
        }
        match &self.listed {
            Some(entry) if !entry.path_is_symlink() => entry.metadata().map_err(into_io),
            _ => std::fs::symlink_metadata(&self.path),
        }
    }

    /// What it is, through a link the walk follows; free from the listing where Windows
    /// gave it. A dangling link's is its own.
    pub(crate) fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        if let Some(stand_in) = &self.stand_in {
            return std::fs::metadata(stand_in);
        }
        if self.followed {
            return std::fs::metadata(&self.path);
        }
        match &self.listed {
            Some(entry) => entry.metadata().map_err(into_io),
            None => std::fs::symlink_metadata(&self.path),
        }
    }
}

/// What went wrong in a walk, for the tool to word.
#[derive(Debug)]
pub(crate) enum WalkError {
    /// A followed link back to a directory above it: its name, and the directory's.
    Loop { shown: String, ancestor: String },
    /// A link the walk follows whose target is not there: GNU's `grep -R` says so, its
    /// `find -L` lists the link as it is.
    Dangling(Entry),
    /// A directory that could not be listed: handed out first, then this.
    Unlisted {
        shown: String,
        path: PathBuf,
        error: std::io::Error,
    },
    /// An entry that could not be looked at: a root that is not there.
    Io {
        shown: String,
        error: std::io::Error,
    },
    /// The stop token asked: the walk ends here.
    Stopped,
}

/// A walk of the tree under one root.
pub(crate) struct Walk {
    /// walkdir's walks under way: the root's, then one inside each link to a directory the
    /// walk goes through, the innermost last.
    walks: Vec<Inner>,
    /// The walk inside the link to a directory just handed out, gone into on the next step
    /// unless [`Self::skip_directory`] says not to.
    pending: Option<Inner>,
    options: WalkOptions,
    root: PathBuf,
    shown: String,
    stop: StopToken,
    stopped: bool,
    /// Under `unixpaths`, what the paths under `/` name (D83): a root of cash's own, `/` or
    /// `/usr/bin`, is walked through what it holds ([`Self::own`]).
    own: Option<cash_win32::root::View>,
    /// The root, when it is one of cash's own directories, until it is handed out.
    own_root: Option<cash_win32::root::Virtual>,
    /// The directories of cash's own under way, the innermost last; walkdir's walks go into
    /// the directories on disk they hold.
    frames: Vec<OwnFrame>,
    /// The directory of cash's own just handed out, gone into on the next step unless
    /// [`Self::skip_directory`] says not to.
    pending_frame: Option<OwnFrame>,
}

/// One of walkdir's walks: the root's, or the one inside a link the walk goes through.
struct Inner {
    iter: walkdir::IntoIter,
    /// How deep its root is in the whole walk.
    depth: usize,
    /// With `contents_first`, the link it is inside, handed out once it is done.
    deferred: Option<Entry>,
    /// Where it starts, and the name that is shown by: the walk's root, or under a directory
    /// of cash's own a directory on disk it holds (`/c`).
    root: PathBuf,
    shown: String,
    /// Whether its root is handed out from it: one under a directory of cash's own is, a
    /// link's is handed out from the walk around it.
    emits_root: bool,
}

/// A directory of cash's own under way (D83): the names it has yet to hand out.
struct OwnFrame {
    shown: String,
    depth: usize,
    entries: std::collections::VecDeque<cash_win32::root::Entry>,
    /// With `contents_first`, the directory itself, handed out once it is done.
    deferred: Option<Entry>,
}

impl Walk {
    /// A walk of `actual`, shown as `shown`: what the user typed, empty for the working
    /// directory searched with no name.
    pub(crate) fn new(shown: &str, actual: &Path, options: &WalkOptions, stop: StopToken) -> Self {
        let root = Inner {
            iter: walkdir_of(
                actual,
                options,
                options.links != Links::Never,
                options.max_depth,
            ),
            depth: 0,
            deferred: None,
            root: actual.to_path_buf(),
            shown: shown.to_owned(),
            emits_root: true,
        };
        Self {
            walks: vec![root],
            pending: None,
            options: *options,
            root: actual.to_path_buf(),
            shown: shown.to_owned(),
            stop,
            stopped: false,
            own: None,
            own_root: None,
            frames: Vec::new(),
            pending_frame: None,
        }
    }

    /// The walk under `unixpaths` (D83, `Shell::root_view`): a root that is one of cash's
    /// own directories, as the shell resolved it (`/`, `/usr/bin`), is walked through what
    /// it holds, the drives' directories on disk among them.
    pub(crate) fn own(mut self, view: Option<cash_win32::root::View>) -> Self {
        if let Some(view) = &view
            && let cash_win32::root::Place::Virtual(directory) =
                view.place(&self.root.to_string_lossy())
        {
            self.walks.clear();
            self.own_root = Some(directory);
        }
        self.own = view;
        self
    }

    /// Goes no further into the directory just handed out (`find -prune`, an excluded
    /// directory).
    pub(crate) fn skip_directory(&mut self) {
        if self.pending_frame.take().is_some() {
            return;
        }
        if self.pending.take().is_none()
            && let Some(walk) = self.walks.last_mut()
        {
            walk.iter.skip_current_dir();
        }
    }

    /// The name `path`, under the innermost walk's root, is shown by.
    fn shown(&self, path: &Path) -> String {
        let (root, shown) = self
            .walks
            .last()
            .map_or((&self.root, &self.shown), |walk| (&walk.root, &walk.shown));
        let below: Vec<String> = path
            .strip_prefix(root)
            .map(|rest| {
                rest.components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        join_shown(shown, &below.join("/"))
    }

    /// A directory of cash's own, a command or a device (D83), as an entry of the walk: none
    /// is a file on disk, so it is not listed from one, and its path is its name under `/`.
    fn own_entry(shown: String, path: PathBuf, depth: usize, kind: Kind) -> Entry {
        let exe = std::env::current_exe().ok();
        let stand_in = match kind {
            Kind::Directory => exe.and_then(|exe| exe.parent().map(Path::to_path_buf)),
            Kind::File => exe,
            Kind::Link | Kind::Other => None,
        };
        Entry {
            shown,
            path,
            depth,
            kind,
            listed: None,
            followed: false,
            stand_in,
        }
    }

    /// What the directory of cash's own `directory`, `shown` and `depth` deep, has to hand
    /// out: nothing at the deepest the walk goes.
    fn own_frame(
        &self,
        directory: cash_win32::root::Virtual,
        shown: &str,
        depth: usize,
    ) -> Option<OwnFrame> {
        let view = self.own.as_ref()?;
        (depth < self.options.max_depth).then(|| OwnFrame {
            shown: shown.to_owned(),
            depth,
            entries: view.entries(directory).into(),
            deferred: None,
        })
    }

    /// The directory of cash's own `entry` is, handed out before what it holds, or after it
    /// with `contents_first`; `None` when nothing is handed out now.
    fn own_directory(&mut self, entry: Entry, frame: Option<OwnFrame>) -> Option<Entry> {
        let shown_now = entry.depth >= self.options.min_depth;
        if self.options.contents_first {
            match frame {
                Some(mut frame) => {
                    frame.deferred = shown_now.then_some(entry);
                    self.frames.push(frame);
                    None
                }
                None => shown_now.then_some(entry),
            }
        } else if shown_now {
            self.pending_frame = frame;
            Some(entry)
        } else {
            self.frames.extend(frame);
            None
        }
    }

    /// The next step through the directories of cash's own under way, once walkdir's walks
    /// are done: a directory of cash's own, a walk into one on disk, a command, a device;
    /// `None` for a step that hands nothing out.
    fn next_own(&mut self) -> Option<Entry> {
        let frame = self.frames.last_mut()?;
        let Some(next) = frame.entries.pop_front() else {
            return self.frames.pop()?.deferred;
        };
        let depth = frame.depth + 1;
        let shown = join_shown(&frame.shown, &next.name);
        let shown_now = depth >= self.options.min_depth;
        match next.place {
            cash_win32::root::Place::Virtual(directory) => {
                let frame = self.own_frame(directory, &shown, depth);
                let entry = Self::own_entry(
                    shown,
                    PathBuf::from(directory.spelled()),
                    depth,
                    Kind::Directory,
                );
                self.own_directory(entry, frame)
            }
            cash_win32::root::Place::Real(path) => {
                self.walks.push(Inner {
                    iter: walkdir_of(
                        &path,
                        &self.options,
                        self.options.links != Links::Never,
                        self.options.max_depth - depth,
                    ),
                    depth,
                    deferred: None,
                    root: path,
                    shown,
                    emits_root: true,
                });
                None
            }
            cash_win32::root::Place::Command(name) => shown_now.then(|| {
                Self::own_entry(
                    shown,
                    PathBuf::from(cash_win32::path::system_tool_path(&name)),
                    depth,
                    Kind::File,
                )
            }),
            cash_win32::root::Place::Device(spelled) => shown_now
                .then(|| Self::own_entry(shown, PathBuf::from(spelled), depth, Kind::Other)),
        }
    }

    fn entry(&self, entry: walkdir::DirEntry, depth: usize) -> Entry {
        Entry {
            shown: self.shown(entry.path()),
            path: entry.path().to_path_buf(),
            depth: depth + entry.depth(),
            kind: kind(entry.file_type()),
            listed: Some(entry),
            followed: false,
            stand_in: None,
        }
    }

    fn error(&self, error: walkdir::Error, depth: usize) -> WalkError {
        let shown = error
            .path()
            .map_or_else(|| self.shown.clone(), |path| self.shown(path));
        let dangling = error.path().filter(|path| {
            self.options.links != Links::Never
                && error
                    .io_error()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
                && std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
        });
        if let Some(path) = dangling {
            return WalkError::Dangling(Entry {
                shown,
                path: path.to_path_buf(),
                depth: depth + error.depth(),
                kind: Kind::Link,
                listed: None,
                followed: false,
                stand_in: None,
            });
        }
        // walkdir words both alike; what failed on a directory is its listing.
        let listed = error
            .path()
            .filter(|path| std::fs::metadata(path).is_ok_and(|m| m.is_dir()))
            .map(Path::to_path_buf);
        let error = into_io(error);
        match listed {
            Some(path) => WalkError::Unlisted { shown, path, error },
            None => WalkError::Io { shown, error },
        }
    }

    /// A link below the root the walk goes through: what it names, its walk set to go
    /// into it when a directory (after it, or before it with `contents_first`, when nothing
    /// is handed out now); or why it is not gone through.
    fn follow(&mut self, mut entry: Entry) -> Option<Result<Entry, WalkError>> {
        let target = match std::fs::metadata(&entry.path) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Some(Err(WalkError::Dangling(entry)));
            }
            Err(error) => {
                return Some(Err(WalkError::Io {
                    shown: entry.shown,
                    error,
                }));
            }
        };
        entry.followed = true;
        entry.kind = kind_of(&target);
        if entry.kind != Kind::Directory || entry.depth >= self.options.max_depth {
            return Some(Ok(entry));
        }
        if let Some(ancestor) = self.ancestor(&entry.path) {
            return Some(Err(WalkError::Loop {
                shown: entry.shown,
                ancestor,
            }));
        }
        let (root, shown) = self.walks.last().map_or_else(
            || (self.root.clone(), self.shown.clone()),
            |walk| (walk.root.clone(), walk.shown.clone()),
        );
        let mut inner = Inner {
            iter: walkdir_of(
                &entry.path,
                &self.options,
                true,
                self.options.max_depth - entry.depth,
            ),
            depth: entry.depth,
            deferred: None,
            root,
            shown,
            emits_root: false,
        };
        if self.options.contents_first {
            inner.deferred = Some(entry);
            self.walks.push(inner);
            return None;
        }
        self.pending = Some(inner);
        Some(Ok(entry))
    }

    /// The directory at or above the root that the link `link` names, shown: a loop.
    fn ancestor(&self, link: &Path) -> Option<String> {
        let root = self.walks.last().map_or(&self.root, |walk| &walk.root);
        let above = link
            .ancestors()
            .skip(1)
            .take_while(|directory| directory.starts_with(root));
        loop_ancestor(link, above).map(|directory| self.shown(directory))
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
        if let Some(inner) = self.pending.take() {
            self.walks.push(inner);
        }
        if let Some(frame) = self.pending_frame.take() {
            self.frames.push(frame);
        }
        // A root of cash's own (D83), handed out first.
        if let Some(directory) = self.own_root.take() {
            let shown = self.shown.clone();
            let frame = self.own_frame(directory, &shown, 0);
            let entry = Self::own_entry(
                shown,
                PathBuf::from(directory.spelled()),
                0,
                Kind::Directory,
            );
            if let Some(entry) = self.own_directory(entry, frame) {
                return Some(Ok(entry));
            }
        }
        loop {
            if self.walks.is_empty() {
                if self.frames.is_empty() {
                    return None;
                }
                if let Some(entry) = self.next_own() {
                    return Some(Ok(entry));
                }
                if let Some(frame) = self.pending_frame.take() {
                    self.frames.push(frame);
                }
                continue;
            }
            let nested = self.walks.len() > 1;
            let (next, depth, emits_root) = {
                let walk = self.walks.last_mut()?;
                (walk.iter.next(), walk.depth, walk.emits_root)
            };
            let Some(next) = next else {
                let done = self.walks.pop()?;
                if let Some(link) = done.deferred
                    && link.depth >= self.options.min_depth
                {
                    return Some(Ok(link));
                }
                continue;
            };
            let found = match next {
                Ok(found) => found,
                Err(error) => return Some(Err(self.error(error, depth))),
            };
            // A link's own entry was handed out from the walk around the walk inside it.
            if nested && !emits_root && found.depth() == 0 {
                continue;
            }
            let mut entry = self.entry(found, depth);
            if self.options.links == Links::All && entry.kind == Kind::Link {
                if entry.depth == 0 {
                    // The root's link, which walkdir went through itself.
                    if let Ok(target) = std::fs::metadata(&entry.path) {
                        entry.followed = true;
                        entry.kind = kind_of(&target);
                    }
                } else {
                    match self.follow(entry) {
                        Some(Ok(followed)) => entry = followed,
                        Some(Err(error)) => return Some(Err(error)),
                        None => continue,
                    }
                }
            }
            if entry.depth < self.options.min_depth {
                // Not handed out, and so not to be skipped: what is in it is walked.
                if let Some(inner) = self.pending.take() {
                    self.walks.push(inner);
                }
                continue;
            }
            return Some(Ok(entry));
        }
    }
}

/// walkdir's walk of `path`, `max_depth` deep, through the root's link when
/// `follow_root`, and through no other: [`Walk`] goes through those itself.
fn walkdir_of(
    path: &Path,
    options: &WalkOptions,
    follow_root: bool,
    max_depth: usize,
) -> walkdir::IntoIter {
    let walk = walkdir::WalkDir::new(path)
        .follow_links(false)
        .follow_root_links(follow_root)
        .contents_first(options.contents_first)
        .max_depth(max_depth)
        .same_file_system(options.one_file_system);
    match options.order {
        Order::Listing => walk,
        Order::Ordinal => walk.sort_by_file_name(),
        Order::FilesFirst => walk.sort_by_key(files_first),
    }
    .into_iter()
}

/// The directory of `directories`, a link's own above it, that the link `link` names: a loop.
/// Each is read with no more than its attributes, which the listing right of the directory
/// it is in grants, so one that cannot be read is still told.
pub(crate) fn loop_ancestor<'p>(
    link: &Path,
    mut directories: impl Iterator<Item = &'p Path>,
) -> Option<&'p Path> {
    let identity = |path: &Path| {
        cash_win32::fs::file_info(path)
            .ok()
            .map(|info| (info.volume, info.index))
    };
    let target = identity(link)?;
    directories.find(|directory| identity(directory) == Some(target))
}

/// What a followed link names is, by its metadata.
fn kind_of(target: &std::fs::Metadata) -> Kind {
    if target.is_dir() {
        Kind::Directory
    } else if target.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

/// What a listing's entry is, by the one answer the builtins give: a link is a symlink or
/// a junction (a name surrogate), as std says; another reparse point (a cloud
/// placeholder, a deduplicated file) is the file or directory it holds. For a walk of its
/// own that needs a directory whole before it shows any of it, as `tree` draws its branches.
pub(crate) fn kind(file_type: std::fs::FileType) -> Kind {
    if file_type.is_symlink() {
        Kind::Link
    } else if file_type.is_dir() {
        Kind::Directory
    } else if file_type.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

/// `below` under `root` as a name is shown: `root/below`, `root` alone, or `below` alone
/// for the empty root; a root that ends with a separator gets no second one.
pub(crate) fn join_shown(root: &str, below: &str) -> String {
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

/// Where an entry goes in [`Order::FilesFirst`]: after the files when it is a directory or a
/// link to one (a junction, a directory symlink, which Windows marks as directories), then by its
/// name as Windows folds names.
fn files_first(entry: &walkdir::DirEntry) -> (bool, String) {
    use std::os::windows::fs::FileTypeExt;
    let kind = entry.file_type();
    (
        kind.is_dir() || kind.is_symlink_dir(),
        cash_win32::fold::name_key(&entry.file_name().to_string_lossy()),
    )
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
                Err(WalkError::Unlisted { shown, error, .. }) => {
                    format!("{shown} unlisted {error}")
                }
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
                "top/b Directory",
                "top/b/deep Directory",
                "top/b/deep/f File"
            ]
        );
        let files_first = WalkOptions {
            order: Order::FilesFirst,
            max_depth: 1,
            min_depth: 1,
            ..WalkOptions::default()
        };
        assert_eq!(
            walked(root, "top/", &files_first),
            ["top/a.txt File", "top/B.txt File", "top/b Directory"]
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

    #[test]
    fn a_followed_link_is_gone_into_in_its_place() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("top");
        let other = dir.path().join("other");
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(root.join("a").join("f"), "x").unwrap();
        std::fs::write(other.join("o"), "x").unwrap();
        cash_win32::junction::point(&root.join("a").join("j"), &other).unwrap();
        let all = WalkOptions {
            links: Links::All,
            min_depth: 1,
            ..WalkOptions::default()
        };
        assert_eq!(
            walked(&root, "t", &all),
            [
                "t/a Directory",
                "t/a/f File",
                "t/a/j Directory",
                "t/a/j/o File"
            ]
        );
        let last = WalkOptions {
            contents_first: true,
            ..all
        };
        assert_eq!(
            walked(&root, "t", &last),
            [
                "t/a/f File",
                "t/a/j/o File",
                "t/a/j Directory",
                "t/a Directory"
            ]
        );
        // A link above the shallowest entry shown is still gone into.
        let deep = WalkOptions {
            min_depth: 3,
            ..all
        };
        assert_eq!(walked(&root, "t", &deep), ["t/a/j/o File"]);
        // Its depth counts from the root, and the walk inside it stops at the deepest.
        let shallow = WalkOptions {
            max_depth: 2,
            ..all
        };
        assert_eq!(
            walked(&root, "t", &shallow),
            ["t/a Directory", "t/a/f File", "t/a/j Directory"]
        );
        std::fs::remove_dir(root.join("a").join("j")).unwrap();
    }

    #[test]
    fn a_dangling_link_and_a_name_no_string_holds_are_walked() {
        use std::os::windows::ffi::OsStringExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("top");
        let gone = dir.path().join("gone");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&gone).unwrap();
        cash_win32::junction::point(&root.join("j"), &gone).unwrap();
        std::fs::remove_dir(&gone).unwrap();
        // A lone surrogate: Windows holds it, a Rust string cannot, the walk's path does.
        let lone = std::ffi::OsString::from_wide(&[0xD800, u16::from(b'x')]);
        std::fs::write(root.join(&lone), "x").unwrap();
        let never = WalkOptions {
            min_depth: 1,
            ..WalkOptions::default()
        };
        let all = WalkOptions {
            links: Links::All,
            ..never
        };
        assert!(walked(&root, "t", &never).contains(&"t/j Link".to_owned()));
        assert!(walked(&root, "t", &all).contains(&"t/j dangling".to_owned()));
        let reached = Walk::new("t", &root, &never, StopToken::never())
            .filter_map(Result::ok)
            .any(|entry| entry.path.file_name() == Some(lone.as_os_str()));
        assert!(reached);
        std::fs::remove_dir(root.join("j")).unwrap();
    }

    #[test]
    fn each_order_sorts_as_it_says() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in ["b", "_c", "A"] {
            std::fs::write(root.join(name), "x").unwrap();
        }
        std::fs::create_dir(root.join("D")).unwrap();
        let order = |order| {
            let options = WalkOptions {
                order,
                min_depth: 1,
                ..WalkOptions::default()
            };
            walked(root, "", &options)
        };
        // As NTFS keeps them: by name folded to capitals, `_` after the letters.
        assert_eq!(
            order(Order::Listing),
            ["A File", "b File", "D Directory", "_c File"]
        );
        // Unit by unit: capitals, then `_`, then small letters.
        assert_eq!(
            order(Order::Ordinal),
            ["A File", "D Directory", "_c File", "b File"]
        );
        assert_eq!(
            order(Order::FilesFirst),
            ["A File", "b File", "_c File", "D Directory"]
        );
    }
}
