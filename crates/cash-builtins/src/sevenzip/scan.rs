//! 7-Zip's scan of the disk for `a` and `u` (`EnumDirItems.cpp`): each prefix's tree of
//! masks walked from its folder, what the masks take, and a warning for each name that
//! is not there.
//!
//! A file's facts come from its folder's listing, as `FindFirstFile` gives them to
//! 7-Zip: the name's case as it is on disk, and the times the folder's index holds,
//! which can lag behind the file's own for a folder just written to.

use std::fs;
use std::io;
use std::os::windows::fs::MetadataExt;
use std::path::PathBuf;

use super::censor::{Censor, Node, check_to_root};

/// `FILE_ATTRIBUTE_DIRECTORY` and `FILE_ATTRIBUTE_REPARSE_POINT`.
const ATTRIBUTE_DIRECTORY: u32 = 0x10;
const ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// Windows' `ERROR_INVALID_FUNCTION`, 7-Zip's error for a name that is a file where a
/// folder was meant, or the other way round.
const ERROR_INVALID_FUNCTION: i32 = 1;

/// Windows' `ERROR_FILE_NOT_FOUND`: "The system cannot find the file specified."
const ERROR_FILE_NOT_FOUND: i32 = 2;

/// A file or folder the scan took.
#[derive(Debug, Clone)]
pub(super) struct DirItem {
    /// Its name in the archive, `/` between the parts.
    pub(super) name: String,
    /// Its path as the command line reached it, for messages.
    pub(super) shown: String,
    /// Its path for opening.
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
    pub(super) size: u64,
    pub(super) attrib: u32,
    /// Times as FILETIME ticks.
    pub(super) modified: u64,
    pub(super) created: u64,
    pub(super) accessed: u64,
    /// With `-snl`, a link's reparse data, which is not followed.
    pub(super) reparse: Option<Vec<u8>>,
}

/// The size of an item whose size is not known: standard input from a pipe.
pub(super) const UNKNOWN_SIZE: u64 = u64::MAX;

/// `-si`'s item (`SetAs_StdInFile`): standard input under `name`, with the size, times
/// and attributes of the file it is, else no size and the time now.
pub(super) fn stdin_item(name: &str, meta: Option<&fs::Metadata>) -> DirItem {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| {
            d.as_secs() * 10_000_000 + u64::from(d.subsec_nanos() / 100)
        })
        + 116_444_736_000_000_000;
    let mut item = DirItem {
        name: name.to_owned(),
        shown: name.to_owned(),
        path: PathBuf::new(),
        is_dir: false,
        size: UNKNOWN_SIZE,
        attrib: 0,
        modified: now,
        created: now,
        accessed: now,
        reparse: None,
    };
    if let Some(m) = meta {
        item.size = m.file_size();
        item.attrib = m.file_attributes();
        item.modified = m.last_write_time();
        item.created = m.creation_time();
        item.accessed = m.last_access_time();
    }
    item
}

/// What the scan counted (`CDirItemsStat`).
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Stat {
    pub(super) dirs: u64,
    pub(super) files: u64,
    pub(super) size: u64,
}

impl Stat {
    pub(super) fn of(items: &[DirItem]) -> Self {
        let mut stat = Self::default();
        for item in items {
            stat.add(item);
        }
        stat
    }

    const fn add(&mut self, item: &DirItem) {
        if item.is_dir {
            self.dirs += 1;
        } else {
            self.files += 1;
            // A link kept as a link counts as its own size, nothing (`SetLinkInfo`).
            if item.reparse.is_none() {
                self.size = self.size.wrapping_add(item.size);
            }
        }
    }
}

/// A name found on disk: its name there, and its facts.
struct Found {
    name: String,
    attrib: u32,
    size: u64,
    modified: u64,
    created: u64,
    accessed: u64,
}

impl Found {
    const fn is_dir(&self) -> bool {
        self.attrib & ATTRIBUTE_DIRECTORY != 0
    }

    fn of(name: String, meta: &fs::Metadata) -> Self {
        Self {
            name,
            attrib: meta.file_attributes(),
            size: if meta.is_dir() { 0 } else { meta.file_size() },
            modified: meta.last_write_time(),
            created: meta.creation_time(),
            accessed: meta.last_access_time(),
        }
    }
}

/// Walks the disk as the censor says, the paths it gives made absolute by `resolve`;
/// `warn` hears each path that could not be read, with why.
///
/// With `symlinks` (`-snl`) a junction or a symbolic link is kept as one, its reparse
/// data read, a folder not entered; else it is followed. `progress` hears the count so
/// far as each folder is entered (`ScanProgress`).
pub(super) fn scan(
    censor: &Censor,
    symlinks: bool,
    resolve: &dyn Fn(&str) -> PathBuf,
    warn: &mut dyn FnMut(&str, &io::Error),
    progress: &mut dyn FnMut(&Stat, &str),
) -> Vec<DirItem> {
    let mut walker = Walker {
        symlinks,
        case: censor.case_sensitive,
        exclude_dirs: censor.exclude_dirs,
        exclude_files: censor.exclude_files,
        resolve,
        warn,
        progress,
        stat: Stat::default(),
        items: Vec::new(),
    };
    for (prefix, node) in censor.pairs() {
        walker.enumerate(&[node], prefix, "", &[], false);
    }
    walker.items
}

struct Walker<'a> {
    symlinks: bool,
    case: bool,
    exclude_dirs: bool,
    exclude_files: bool,
    resolve: &'a dyn Fn(&str) -> PathBuf,
    warn: &'a mut dyn FnMut(&str, &io::Error),
    progress: &'a mut dyn FnMut(&Stat, &str),
    stat: Stat,
    items: Vec<DirItem>,
}

impl Walker<'_> {
    const fn can_include(&self, is_dir: bool) -> bool {
        !(if is_dir {
            self.exclude_dirs
        } else {
            self.exclude_files
        })
    }

    fn path(&self, shown: &str) -> PathBuf {
        (self.resolve)(if shown.is_empty() { "." } else { shown })
    }

    /// `FindFile_KeepDots`: the name as its folder lists it, links followed.
    fn find(&self, shown: &str) -> io::Result<Found> {
        let path = self.path(shown);
        let name = shown
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default();
        if name.is_empty() || name == "." || name == ".." || name.contains(':') {
            let meta = fs::metadata(&path)?;
            return Ok(Found::of(name.to_owned(), &meta));
        }
        let parent = path.parent().map(PathBuf::from).unwrap_or_default();
        for entry in fs::read_dir(&parent)? {
            let entry = entry?;
            let entry_name = entry.file_name().to_string_lossy().into_owned();
            if !same_name(&entry_name, name) {
                continue;
            }
            let meta = entry.metadata()?;
            if meta.file_attributes() & ATTRIBUTE_REPARSE_POINT != 0 && !self.symlinks {
                return Ok(Found::of(entry_name, &fs::metadata(&path)?));
            }
            return Ok(Found::of(entry_name, &meta));
        }
        Err(io::Error::from_raw_os_error(ERROR_FILE_NOT_FOUND))
    }

    /// `ListDir`: a folder's entries in the order NTFS keeps them.
    fn list(&self, phy: &str) -> io::Result<Vec<Found>> {
        let dir = self.path(phy);
        let mut found = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let mut meta = entry.metadata()?;
            if meta.file_attributes() & ATTRIBUTE_REPARSE_POINT != 0
                && !self.symlinks
                && let Ok(target) = fs::metadata(entry.path())
            {
                meta = target;
            }
            found.push(Found::of(name, &meta));
        }
        found.sort_by_cached_key(|f| f.name.to_uppercase());
        Ok(found)
    }

    fn error(&mut self, shown: &str, error: &io::Error) {
        (self.warn)(shown, error);
    }

    /// Whether the scan keeps `found` as a link: `-snl`, and a reparse point.
    const fn kept_link(&self, found: &Found) -> bool {
        self.symlinks && found.attrib & ATTRIBUTE_REPARSE_POINT != 0
    }

    fn add(&mut self, phy: &str, log: &str, found: &Found) {
        let shown = format!("{phy}{}", found.name);
        let path = self.path(&shown);
        let mut size = found.size;
        let mut reparse = None;
        if self.kept_link(found) {
            // SetLinkInfo: the link's reparse data, its size the item's.
            match cash_win32::reparse::data(&path) {
                Ok(data) => {
                    size = data.len() as u64;
                    reparse = Some(data);
                }
                Err(error) => self.error(&shown, &error),
            }
        }
        self.items.push(DirItem {
            name: format!("{log}{}", found.name),
            path,
            shown,
            is_dir: found.is_dir(),
            size,
            attrib: found.attrib,
            modified: found.modified,
            created: found.created,
            accessed: found.accessed,
            reparse,
        });
        if let Some(item) = self.items.last() {
            self.stat.add(item);
        }
    }

    /// `EnumerateDirItems_Spec`: the folder `name` under `phy`, with `stack`'s masks.
    fn enter(
        &mut self,
        stack: &[&Node],
        name: &str,
        phy: &str,
        log: &str,
        add_parts: &[String],
        enter: bool,
    ) {
        self.enumerate(
            stack,
            &format!("{phy}{name}/"),
            &format!("{log}{name}/"),
            add_parts,
            enter,
        );
    }

    /// `EnumerateDirItems`: what the last node of `stack` takes in the folder `phy`,
    /// `add_parts` being the folders between that node and this one.
    fn enumerate(
        &mut self,
        stack: &[&Node],
        phy: &str,
        log: &str,
        add_parts: &[String],
        mut enter: bool,
    ) {
        let Some(&node) = stack.last() else {
            return;
        };
        (self.progress)(&self.stat, phy);
        if !enter && node.need_check_sub_dirs() {
            enter = true;
        }
        if add_parts.is_empty()
            && !enter
            && let Some(names) = node.direct_names()
        {
            self.enumerate_direct(stack, node, &names, phy, log);
            return;
        }
        let entries = match self.list(phy) {
            Ok(entries) => entries,
            Err(error) => {
                self.error(phy, &error);
                return;
            }
        };
        for found in entries {
            self.enumerate_item(stack, &found, phy, log, add_parts, enter);
        }
    }

    /// The names a node gives plainly, looked up without listing their folder, then
    /// the node's subfolders that no name reached.
    fn enumerate_direct(
        &mut self,
        stack: &[&Node],
        node: &Node,
        names: &[(String, bool, bool)],
        phy: &str,
        log: &str,
    ) {
        let mut need_enter = vec![true; node.children().len()];
        for (name, for_file, for_dir) in names {
            let shown = format!("{phy}{name}");
            let found = match self.find(&shown) {
                Ok(found) => found,
                Err(error) => {
                    self.error(&shown, &error);
                    continue;
                }
            };
            let is_dir = found.is_dir();
            if if is_dir { !for_dir } else { !for_file } {
                self.error(
                    &shown,
                    &io::Error::from_raw_os_error(ERROR_INVALID_FUNCTION),
                );
                continue;
            }
            if check_to_root(
                stack,
                false,
                std::slice::from_ref(&found.name),
                !is_dir,
                self.case,
            ) {
                continue;
            }
            if self.can_include(is_dir) {
                self.add(phy, log, &found);
            }
            if !is_dir || self.kept_link(&found) {
                continue;
            }
            if let Some(index) = node.find_sub_node(name, self.case) {
                need_enter[index] = false;
                let mut deeper = stack.to_vec();
                deeper.push(&node.children()[index]);
                self.enter(&deeper, &found.name, phy, log, &[], true);
            } else {
                self.enter(
                    stack,
                    &found.name,
                    phy,
                    log,
                    std::slice::from_ref(name),
                    true,
                );
            }
        }
        for (index, child) in node.children().iter().enumerate() {
            if !need_enter[index] {
                continue;
            }
            let shown = format!("{phy}{}", child.name());
            match self.find(&shown) {
                Err(error) => {
                    if child.are_there_include_items() {
                        self.error(&shown, &error);
                    }
                }
                Ok(found) if !found.is_dir() => {
                    self.error(
                        &shown,
                        &io::Error::from_raw_os_error(ERROR_INVALID_FUNCTION),
                    );
                }
                Ok(found) => {
                    let mut deeper = stack.to_vec();
                    deeper.push(child);
                    self.enter(&deeper, &found.name, phy, log, &[], false);
                }
            }
        }
    }

    /// `EnumerateForItem`: one entry of a listed folder.
    fn enumerate_item(
        &mut self,
        stack: &[&Node],
        found: &Found,
        phy: &str,
        log: &str,
        add_parts: &[String],
        mut enter: bool,
    ) {
        let mut parts = add_parts.to_vec();
        parts.push(found.name.clone());
        let is_dir = found.is_dir();
        if check_to_root(stack, false, &parts, !is_dir, self.case) {
            return;
        }
        if check_to_root(stack, true, &parts, !is_dir, self.case) {
            if self.can_include(is_dir) {
                self.add(phy, log, found);
            }
            if is_dir {
                enter = true;
            }
        }
        if !is_dir || self.kept_link(found) {
            return;
        }
        let Some(&node) = stack.last() else {
            return;
        };
        if add_parts.is_empty()
            && let Some(index) = node.find_sub_node(&found.name, self.case)
        {
            let mut deeper = stack.to_vec();
            deeper.push(&node.children()[index]);
            self.enter(&deeper, &found.name, phy, log, &[], enter);
        } else if enter {
            self.enter(stack, &found.name, phy, log, &parts, enter);
        }
    }
}

/// Names as Windows compares them: without case.
fn same_name(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}
