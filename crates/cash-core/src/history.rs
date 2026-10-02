//! Facilities for tracking and persisting the shell's command history.

use chrono::Utc;
use std::fmt::Write as _;
use std::{
    collections::HashMap,
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};

pub mod expansion;
pub use expansion::{HistoryExpansionError, HistoryExpansionResult, expand_history};

use crate::error;

/// Represents a unique identifier for a history item.
type ItemId = i64;

/// Interface for querying and manipulating the shell's recorded history of commands.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct History {
    items: rpds::VectorSync<ItemId>,
    id_map: rpds::HashTrieMapSync<ItemId, Item>,
    next_id: ItemId,
    /// Last byte read from each history file. Bash uses this bookkeeping for
    /// `history -n`, which imports only lines appended since the file was last read.
    #[cfg_attr(feature = "serde", serde(default))]
    file_offsets: HashMap<PathBuf, u64>,
    /// How many items are dirty, so that a flush after every command (D44) finds them
    /// without walking the whole history.
    #[cfg_attr(feature = "serde", serde(default))]
    unsaved: usize,
}

impl History {
    /// Constructs a new `History` instance, with its contents initialized from the given readable
    /// stream. If errors are encountered reading lines from the stream, unreadable lines will
    /// be skipped but the call will still return successfully, with a warning logged. An error
    /// result will be returned only if an internal error occurs updating the history.
    ///
    /// # Arguments
    ///
    /// * `reader` - The readable stream to import history from.
    pub fn import(reader: impl Read) -> Result<Self, error::Error> {
        let mut history = Self::default();

        let buf_reader = std::io::BufReader::new(reader);

        let mut next_timestamp = None;
        for line_result in buf_reader.lines() {
            let line = match line_result {
                Ok(line) => line,
                // If we couldn't decode the line due to invalid data (perhaps it wasn't
                // valid UTF8?), skip it and make a best-effort attempt to proceed on.
                // We'll later warn the user.
                Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
                    tracing::warn!("unreadable history line; {err}");
                    continue;
                }
                // In the event of other kinds of errors, return an error result. We don't
                // want to get stuck in a failing I/O loop.
                Err(err) => {
                    return Err(err.into());
                }
            };

            // Look for timestamp comments; ignore other comment lines.
            if let Some(comment) = line.strip_prefix("#") {
                if let Ok(seconds_since_epoch) = comment.trim().parse() {
                    next_timestamp = ItemTimestamp::from_timestamp(seconds_since_epoch, 0);
                } else {
                    next_timestamp = None;
                }

                continue;
            }

            let item = Item {
                id: history.next_id,
                command_line: line,
                timestamp: next_timestamp.take(),
                dirty: false,
            };

            history.add(item)?;
        }

        Ok(history)
    }

    /// Tries to retrieve a history item by its unique identifier. Returns `None` if no item is
    /// found.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the history item to retrieve.
    pub fn get_by_id(&self, id: ItemId) -> Result<Option<&Item>, error::Error> {
        Ok(self.id_map.get(&id))
    }

    /// Replaces the history item with the given ID with a new item. Returns an error if the item
    /// cannot be updated.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the history item to update.
    /// * `item` - The new history item to replace the old one.
    pub fn update_by_id(&mut self, id: ItemId, item: Item) -> Result<(), error::Error> {
        let existing_item = self
            .id_map
            .get_mut(&id)
            .ok_or(error::ErrorKind::HistoryItemNotFound)?;
        match (existing_item.dirty, item.dirty) {
            (false, true) => self.unsaved += 1,
            (true, false) => self.unsaved = self.unsaved.saturating_sub(1),
            _ => (),
        }
        *existing_item = item;
        Ok(())
    }

    /// Forgets that the item `id` exists, in the dirty count; the caller removes it.
    fn forget_dirty(&mut self, id: ItemId) {
        if self.id_map.get(&id).is_some_and(|item| item.dirty) {
            self.unsaved = self.unsaved.saturating_sub(1);
        }
    }

    /// Removes every item whose command line is `command_line` (`HISTCONTROL=erasedups`).
    pub fn remove_matching(&mut self, command_line: &str) {
        let matching: Vec<ItemId> = self
            .iter()
            .filter(|item| item.command_line == command_line)
            .map(|item| item.id)
            .collect();
        if matching.is_empty() {
            return;
        }
        for id in &matching {
            self.forget_dirty(*id);
            self.id_map.remove_mut(id);
        }
        self.items = self
            .items
            .into_iter()
            .filter(|id| !matching.contains(id))
            .copied()
            .collect();
    }

    /// Keeps only the newest `max_items` items (`HISTSIZE`).
    pub fn keep_newest(&mut self, max_items: usize) {
        let Some(excess) = self.items.len().checked_sub(max_items).filter(|n| *n > 0) else {
            return;
        };
        let dropped: Vec<ItemId> = self.items.iter().take(excess).copied().collect();
        for id in &dropped {
            self.forget_dirty(*id);
            self.id_map.remove_mut(id);
        }
        self.items = self.items.iter().skip(excess).copied().collect();
    }

    /// Removes the nth item from the history. Returns the removed item, or `None` if no such item
    /// exists (i.e., because it was out of range).
    pub fn remove_nth_item(&mut self, n: usize) -> bool {
        if let Some(id) = self.items.get(n).copied() {
            self.forget_dirty(id);
            self.items = self
                .items
                .into_iter()
                .enumerate()
                .filter_map(|(i, id)| if i != n { Some(id) } else { None })
                .copied()
                .collect();

            self.id_map.remove_mut(&id);

            true
        } else {
            false
        }
    }

    /// Adds a new history item. Returns the unique identifier of the newly added item.
    ///
    /// # Arguments
    ///
    /// * `item` - The history item to add.
    pub fn add(&mut self, mut item: Item) -> Result<ItemId, error::Error> {
        let id = self.next_id;

        item.id = id;
        self.next_id += 1;
        if item.dirty {
            self.unsaved += 1;
        }

        self.items.push_back_mut(item.id);
        self.id_map.insert_mut(item.id, item);

        Ok(id)
    }

    /// Deletes a history item by its unique identifier. Returns an error if the item cannot be
    /// deleted.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the history item to delete.
    pub fn delete_item_by_id(&mut self, id: ItemId) -> Result<(), error::Error> {
        self.forget_dirty(id);
        self.id_map.remove_mut(&id);
        self.items = self
            .items
            .into_iter()
            .filter(|&item_id| *item_id != id)
            .copied()
            .collect();

        Ok(())
    }

    /// Clears all history items.
    pub fn clear(&mut self) -> Result<(), error::Error> {
        self.id_map = rpds::HashTrieMapSync::new_sync();
        self.items = rpds::VectorSync::new_sync();
        self.unsaved = 0;
        Ok(())
    }

    /// Flushes the history to backing storage (if relevant).
    ///
    /// # Arguments
    ///
    /// * `history_file_path` - The path to the history file.
    /// * `append` - Whether to append to the file or overwrite it.
    /// * `unsaved_items_only` - Whether to only write unsaved items; if true, any items will be
    ///   marked as "saved" once saved.
    /// * `write_timestamps` - Whether to write timestamps for each command line.
    pub fn flush(
        &mut self,
        history_file_path: impl AsRef<Path>,
        append: bool,
        unsaved_items_only: bool,
        write_timestamps: bool,
    ) -> Result<(), error::Error> {
        // With nothing new, the file is not even opened: this runs after every command.
        if unsaved_items_only && self.unsaved == 0 {
            return Ok(());
        }

        // The dirty items are the newest ones in practice, so they are found from the end.
        let ids: Vec<ItemId> = if unsaved_items_only {
            let mut ids: Vec<ItemId> = self
                .items
                .iter()
                .rev()
                .filter(|id| self.id_map.get(id).is_some_and(|item| item.dirty))
                .take(self.unsaved)
                .copied()
                .collect();
            ids.reverse();
            ids
        } else {
            self.items.iter().copied().collect()
        };

        // One write for all of it: an append of one buffer is a single `WriteFile`, which
        // another tab's append cannot land in the middle of (D44).
        let mut text = String::new();
        for id in &ids {
            if let Some(item) = self.id_map.get(id) {
                if write_timestamps && let Some(timestamp) = item.timestamp {
                    let _ = writeln!(text, "#{}", timestamp.timestamp());
                }
                text.push_str(&item.command_line);
                text.push('\n');
            }
        }

        let mut file_options = std::fs::File::options();
        if append {
            file_options.append(true);
        } else {
            file_options.write(true).truncate(true);
        }
        let mut file = file_options.create(true).open(history_file_path.as_ref())?;
        file.write_all(text.as_bytes())?;
        file.flush()?;

        for id in &ids {
            if let Some(item) = self.id_map.get_mut(id) {
                item.dirty = false;
            }
        }
        if unsaved_items_only {
            self.unsaved = 0;
        } else {
            self.unsaved = self.iter().filter(|item| item.dirty).count();
        }

        Ok(())
    }

    /// Cuts the history file at `path` down to its newest `max_entries` entries
    /// (`HISTFILESIZE`), as Bash does when it starts. A timestamp line stays with the entry
    /// it belongs to. The file is replaced whole, through a temporary file beside it, so
    /// it is never left half written.
    pub fn truncate_file(path: impl AsRef<Path>, max_entries: usize) -> Result<(), error::Error> {
        let path = path.as_ref();
        let contents = match std::fs::read(path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err.into()),
        };

        // Where each entry begins, its timestamp line included.
        let mut entry_starts = Vec::new();
        let mut pending_timestamp = None;
        let mut offset = 0;
        for line in contents.split_inclusive(|b| *b == b'\n') {
            if is_timestamp_line(line) {
                pending_timestamp.get_or_insert(offset);
            } else {
                entry_starts.push(pending_timestamp.take().unwrap_or(offset));
            }
            offset += line.len();
        }

        let Some(excess) = entry_starts
            .len()
            .checked_sub(max_entries)
            .filter(|n| *n > 0)
        else {
            return Ok(());
        };
        let keep_from = entry_starts.get(excess).copied().unwrap_or(contents.len());
        let kept = contents.get(keep_from..).unwrap_or_default();

        let mut temp_name = path.as_os_str().to_owned();
        temp_name.push(format!(".cash-truncate-{}", std::process::id()));
        let temp = PathBuf::from(temp_name);
        std::fs::write(&temp, kept)?;
        if let Err(err) = std::fs::rename(&temp, path) {
            let _ = std::fs::remove_file(&temp);
            return Err(err.into());
        }
        Ok(())
    }

    /// Imports a history file, either in full (`history -r`) or only from the byte after
    /// the last successful import (`history -n`). Imported entries are clean, so a later
    /// `history -a` does not append them back to the same file.
    pub fn read_file(
        &mut self,
        history_file_path: impl AsRef<Path>,
        new_lines_only: bool,
    ) -> Result<(), error::Error> {
        use std::io::{Seek as _, SeekFrom};

        let path = history_file_path.as_ref();
        let key = path.to_path_buf();
        let mut file = std::fs::File::open(path)?;
        let file_len = file.metadata()?.len();
        let previous_end = if new_lines_only {
            self.file_offsets.get(&key).copied().unwrap_or(0)
        } else {
            0
        };
        // A replaced or truncated history file is a new file for `-n` purposes.
        let start = if previous_end > file_len {
            0
        } else {
            previous_end
        };
        file.seek(SeekFrom::Start(start))?;

        let imported = Self::import(file)?;
        for item in imported.iter() {
            self.add(item.clone())?;
        }
        self.file_offsets.insert(key, file_len);
        Ok(())
    }

    /// Records that an initial shell startup import consumed the complete file.
    pub fn mark_file_read_to(&mut self, history_file_path: impl AsRef<Path>, offset: u64) {
        self.file_offsets
            .insert(history_file_path.as_ref().to_path_buf(), offset);
    }

    /// Searches through history using the given query.
    ///
    /// # Arguments
    ///
    /// * `query` - The query to use.
    pub fn search(&self, query: Query) -> Result<impl Iterator<Item = &self::Item>, error::Error> {
        Ok(Search::new(self, query))
    }

    /// Returns whether the history contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns an iterator over the history items.
    pub fn iter(&self) -> impl Iterator<Item = &self::Item> {
        Search::all(self)
    }

    /// Retrieves the nth history item, if it exists. Returns `None` if no such item exists.
    /// Indexing is zero-based, with an index of 0 referencing the oldest item in the history.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the history item to retrieve.
    pub fn get(&self, index: usize) -> Option<&Item> {
        if let Some(id) = self.items.get(index) {
            self.id_map.get(id)
        } else {
            None
        }
    }

    /// Returns the number of items in the history.
    pub fn count(&self) -> usize {
        self.items.len()
    }
}

/// Whether a line of a history file is the timestamp of the entry after it: `#` and
/// digits, as `import` reads them.
fn is_timestamp_line(line: &[u8]) -> bool {
    let line = line.trim_ascii_end();
    line.strip_prefix(b"#")
        .is_some_and(|digits| !digits.is_empty() && digits.iter().all(u8::is_ascii_digit))
}

/// Represents a timestamp for a history item.
pub type ItemTimestamp = chrono::DateTime<Utc>;

/// Represents an item in the history.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Item {
    /// The unique identifier of the history item.
    pub id: ItemId,
    /// The actual command line.
    pub command_line: String,
    /// The timestamp when the command was started.
    pub timestamp: Option<ItemTimestamp>,
    /// Whether or not the item is dirty, i.e., has not yet been written to backing storage.
    pub dirty: bool,
}

impl Item {
    /// Constructs a new `Item` with the given command line.
    ///
    /// # Arguments
    ///
    /// * `command_line` - The command line of the item.
    pub fn new(command_line: impl Into<String>) -> Self {
        Self {
            id: 0, // NOTE: ID will be assigned when added to the history.
            command_line: command_line.into(),
            timestamp: Some(chrono::Utc::now()),
            dirty: true,
        }
    }
}

/// Encapsulates query parameters for searching through history.
#[derive(Default)]
pub struct Query {
    /// Whether to search forward or backward
    pub direction: Direction,
    /// Optionally, clamp results to items with a timestamp strictly after this.
    pub not_at_or_before_time: Option<ItemTimestamp>,
    /// Optionally, clamp results to items with a timestamp strictly before this.
    pub not_at_or_after_time: Option<ItemTimestamp>,
    /// Optionally, clamp results to items with an ID equal strictly after this.
    pub not_at_or_before_id: Option<ItemId>,
    /// Optionally, clamp results to items with an ID equal strictly before this.
    pub not_at_or_after_id: Option<ItemId>,
    /// Optionally, maximum number of items to retrieve
    pub max_items: Option<i64>,
    /// Optionally, a string-based filter on command line.
    pub command_line_filter: Option<CommandLineFilter>,
}

impl Query {
    /// Checks if the query includes the given item.
    ///
    /// # Arguments
    ///
    /// * `item` - The item to check.
    pub fn includes(&self, item: &Item) -> bool {
        // Filter based on not_at_or_before_time.
        if let Some(not_at_or_before_time) = &self.not_at_or_before_time {
            if item
                .timestamp
                .is_some_and(|ts| ts <= *not_at_or_before_time)
            {
                return false;
            }
        }

        // Filter based on not_at_or_after_time
        if let Some(not_at_or_after_time) = &self.not_at_or_after_time {
            if item.timestamp.is_some_and(|ts| ts >= *not_at_or_after_time) {
                return false;
            }
        }

        // Filter based on not_at_or_before_id
        if self
            .not_at_or_before_id
            .is_some_and(|query_id| item.id <= query_id)
        {
            return false;
        }

        // Filter based on not_at_or_after_id
        if self
            .not_at_or_after_id
            .is_some_and(|query_id| item.id >= query_id)
        {
            return false;
        }

        // Filter based on command_line_filter
        if let Some(command_line_filter) = &self.command_line_filter {
            match command_line_filter {
                CommandLineFilter::Prefix(prefix) => {
                    if !item.command_line.starts_with(prefix) {
                        return false;
                    }
                }
                CommandLineFilter::Suffix(suffix) => {
                    if !item.command_line.ends_with(suffix) {
                        return false;
                    }
                }
                CommandLineFilter::Contains(contains) => {
                    if !item.command_line.contains(contains) {
                        return false;
                    }
                }
                CommandLineFilter::Exact(exact) => {
                    if item.command_line != *exact {
                        return false;
                    }
                }
            }
        }

        true
    }
}

/// Represents the direction of a search operation.
#[derive(Default)]
pub enum Direction {
    /// Search forward from the oldest part of history.
    #[default]
    Forward,
    /// Search backward from the youngest part of history.
    Backward,
}

/// Filter criteria for command lines.
pub enum CommandLineFilter {
    /// The command line must start with this string.
    Prefix(String),
    /// The command line must end with this string.
    Suffix(String),
    /// The command line must contain this string.
    Contains(String),
    /// The command line must match this string exactly.
    Exact(String),
}

/// Represents a search operation.
pub struct Search<'a> {
    /// The history to search through.
    history: &'a History,
    /// The query to apply.
    query: Query,
    /// The next index in `items`.
    next_index: Option<usize>,
    /// Count of items returned so far.
    count: usize,
}

impl<'a> Search<'a> {
    /// Constructs a new search against the provided history, querying *all* items.
    ///
    /// # Arguments
    ///
    /// * `history` - The history to search through.
    pub fn all(history: &'a History) -> Self {
        Self::new(history, Query::default())
    }

    /// Constructs a new search against the provided history, using the given query.
    ///
    /// # Arguments
    ///
    /// * `history` - The history to search through.
    /// * `query` - The query to use.
    pub fn new(history: &'a History, query: Query) -> Self {
        let next_index = match query.direction {
            Direction::Forward => Some(0),
            Direction::Backward => {
                if history.items.is_empty() {
                    None
                } else {
                    Some(history.items.len() - 1)
                }
            }
        };

        Self {
            history,
            query,
            next_index,
            count: 0,
        }
    }

    const fn increment_next_index(&mut self) {
        if let Some(index) = self.next_index {
            self.next_index = match self.query.direction {
                Direction::Forward => Some(index + 1),
                Direction::Backward => {
                    if index == 0 {
                        None
                    } else {
                        Some(index - 1)
                    }
                }
            }
        }
    }
}

impl<'a> Iterator for Search<'a> {
    type Item = &'a Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            {
                let index = self.next_index?;
                // Make sure we haven't hit the end of the history.
                if index >= self.history.items.len() {
                    return None;
                }

                let id = self.history.items[index];
                self.increment_next_index();

                if let Some(item) = self.history.id_map.get(&id) {
                    // Filter based on max_items. Once we hit the limit,
                    // we stop searching.
                    #[expect(clippy::cast_possible_truncation)]
                    #[expect(clippy::cast_sign_loss)]
                    if self
                        .query
                        .max_items
                        .is_some_and(|max_items| self.count >= max_items as usize)
                    {
                        return None;
                    }

                    // Check other filters. If they don't match, then we
                    // skip but keep searching.
                    if self.query.includes(item) {
                        self.count += 1;
                        return Some(item);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn a_flush_appends_only_what_is_new() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        let mut history = History::default();
        history.add(Item::new("one")).unwrap();
        history.add(Item::new("two")).unwrap();
        history.flush(&path, true, true, false).unwrap();
        history.add(Item::new("three")).unwrap();
        history.flush(&path, true, true, false).unwrap();
        // Nothing new: the file is not touched.
        history.flush(&path, true, true, false).unwrap();
        assert_eq!(read(&path), "one\ntwo\nthree\n");
    }

    #[test]
    fn the_newest_items_are_kept() {
        let mut history = History::default();
        for line in ["a", "b", "c", "d"] {
            history.add(Item::new(line)).unwrap();
        }
        history.keep_newest(2);
        let lines: Vec<_> = history
            .iter()
            .map(|item| item.command_line.as_str())
            .collect();
        assert_eq!(lines, ["c", "d"]);
        history.keep_newest(0);
        assert!(history.is_empty());
    }

    #[test]
    fn dropped_items_are_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        let mut history = History::default();
        for line in ["a", "b", "a", "c"] {
            history.add(Item::new(line)).unwrap();
        }
        history.remove_matching("a");
        history.flush(&path, true, true, false).unwrap();
        assert_eq!(read(&path), "b\nc\n");
    }

    #[test]
    fn the_file_is_cut_to_its_newest_entries_with_their_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        std::fs::write(&path, "#1\none\n#2\ntwo\nthree\n#4\nfour\n").unwrap();
        History::truncate_file(&path, 2).unwrap();
        assert_eq!(read(&path), "three\n#4\nfour\n");
        History::truncate_file(&path, 5).unwrap();
        assert_eq!(read(&path), "three\n#4\nfour\n");
        History::truncate_file(&path, 0).unwrap();
        assert_eq!(read(&path), "");
        // A file that does not exist is left alone.
        History::truncate_file(dir.path().join("none"), 1).unwrap();
    }
}
