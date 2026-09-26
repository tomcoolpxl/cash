//! Path cache

use crate::{error, variables};
use std::path::{Path, PathBuf};

/// Buckets in a fresh table, as in bash's `FILENAME_HASH_BUCKETS`.
const INITIAL_BUCKETS: usize = 256;

/// A cache of paths associated with names.
///
/// `hash` lists the table by walking it, so its order is observable. It is laid out the way
/// bash lays out its own table — FNV-1 buckets, newest entry first in each, and a fourfold
/// grow once there are twice as many entries as buckets — so a listing comes out in the
/// order bash would print it.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PathCache {
    /// Each bucket's chain, head first. Empty until the first insertion.
    buckets: Vec<Vec<Entry>>,
    /// The number of entries across all buckets.
    len: usize,
}

/// One remembered location, and how often the shell has run it from there.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Entry {
    name: String,
    path: PathBuf,
    hits: u32,
}

/// Bash's `hash_string`: 32-bit FNV-1.
fn hash_string(s: &str) -> u32 {
    s.bytes().fold(2_166_136_261_u32, |h, byte| {
        h.wrapping_mul(16_777_619) ^ u32::from(byte)
    })
}

impl PathCache {
    /// Clears all elements from the cache. Like bash's `hash_flush`, the table keeps its size.
    pub fn reset(&mut self) {
        self.buckets.iter_mut().for_each(Vec::clear);
        self.len = 0;
    }

    fn bucket_of(&self, name: &str) -> usize {
        // The bucket count is always a power of two.
        hash_string(name) as usize & (self.buckets.len() - 1)
    }

    fn entry(&self, name: &str) -> Option<&Entry> {
        if self.buckets.is_empty() {
            return None;
        }
        self.buckets[self.bucket_of(name)]
            .iter()
            .find(|entry| entry.name == name)
    }

    fn entry_mut(&mut self, name: &str) -> Option<&mut Entry> {
        if self.buckets.is_empty() {
            return None;
        }
        let bucket = self.bucket_of(name);
        self.buckets[bucket]
            .iter_mut()
            .find(|entry| entry.name == name)
    }

    /// Returns the path associated with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to lookup.
    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<PathBuf> {
        self.entry(name.as_ref()).map(|entry| entry.path.clone())
    }

    /// Sets the path associated with the given name, starting its hit count over. A name
    /// already in the table keeps its place in a listing.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to set.
    /// * `path` - The path to associate with the name.
    pub fn set<T: Into<String>>(&mut self, name: T, path: PathBuf) {
        let name = name.into();

        if let Some(entry) = self.entry_mut(&name) {
            entry.path = path;
            entry.hits = 0;
            return;
        }

        if self.buckets.is_empty() {
            self.buckets = vec![Vec::new(); INITIAL_BUCKETS];
        } else if self.len >= self.buckets.len() * 2 {
            self.grow();
        }

        let bucket = self.bucket_of(&name);
        self.buckets[bucket].insert(
            0,
            Entry {
                name,
                path,
                hits: 0,
            },
        );
        self.len += 1;
    }

    /// Bash's `hash_rehash` into four times as many buckets: old buckets in order, each
    /// chain from its head, every entry pushed onto the head of its new chain.
    fn grow(&mut self) {
        let size = self.buckets.len() * 4;
        let old = std::mem::replace(&mut self.buckets, vec![Vec::new(); size]);

        for entry in old.into_iter().flatten() {
            let bucket = self.bucket_of(&entry.name);
            self.buckets[bucket].insert(0, entry);
        }
    }

    /// Counts one execution of the command cached under the given name, as `hash` reports
    /// in its listing. Does nothing if the name is not cached.
    ///
    /// # Arguments
    ///
    /// * `name` - The name that was run.
    pub fn record_hit<S: AsRef<str>>(&mut self, name: S) {
        if let Some(entry) = self.entry_mut(name.as_ref()) {
            entry.hits = entry.hits.saturating_add(1);
        }
    }

    /// Returns every entry as `(name, path, hits)`, in the order bash would list them.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &Path, u32)> {
        self.buckets
            .iter()
            .flatten()
            .map(|entry| (entry.name.as_str(), entry.path.as_path(), entry.hits))
    }

    /// Projects the cache into a shell value.
    pub fn to_value(&self) -> Result<variables::ShellValue, error::Error> {
        let pairs = self
            .entries()
            .map(|(name, path, _)| (Some(name.to_owned()), path.to_string_lossy().to_string()))
            .collect::<Vec<_>>();

        variables::ShellValue::associative_array_from_literals(variables::ArrayLiteral(pairs))
    }

    /// Removes the path associated with the given name, if there is one.
    /// Returns whether or not an entry was removed.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to remove.
    pub fn unset<S: AsRef<str>>(&mut self, name: S) -> bool {
        let name = name.as_ref();
        if self.buckets.is_empty() {
            return false;
        }

        let bucket = self.bucket_of(name);
        let chain = &mut self.buckets[bucket];
        let Some(index) = chain.iter().position(|entry| entry.name == name) else {
            return false;
        };

        chain.remove(index);
        self.len -= 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(cache: &PathCache) -> Vec<&str> {
        cache.entries().map(|(name, _, _)| name).collect()
    }

    #[test]
    fn hash_string_is_fnv1() {
        // FNV-1 of the empty string is the offset basis; of "a" it is the published vector.
        assert_eq!(hash_string(""), 0x811c_9dc5);
        assert_eq!(hash_string("a"), 0x050c_5d7e);
    }

    #[test]
    fn lists_in_the_order_bash_does() {
        // Observed from bash 5.3: `hash -p /p/$n $n` for each of these, then `hash -l`.
        let mut cache = PathCache::default();
        for name in ["a", "b", "c", "d", "e", "f", "g", "h", "zz", "foo", "bar"] {
            cache.set(name, PathBuf::from("/p"));
        }
        assert_eq!(
            names(&cache),
            ["foo", "bar", "h", "g", "f", "e", "d", "c", "b", "a", "zz"]
        );
    }

    #[test]
    fn re_setting_a_name_keeps_its_place_and_restarts_its_count() {
        let mut cache = PathCache::default();
        cache.set("a", PathBuf::from("/1"));
        cache.set("b", PathBuf::from("/1"));
        cache.record_hit("a");
        let before = names(&cache).join(" ");

        cache.set("a", PathBuf::from("/2"));
        assert_eq!(names(&cache).join(" "), before);
        assert_eq!(cache.get("a"), Some(PathBuf::from("/2")));
        assert!(cache.entries().all(|(_, _, hits)| hits == 0));
    }

    #[test]
    fn growth_keeps_every_entry() {
        let mut cache = PathCache::default();
        for i in 0..700 {
            cache.set(format!("cmd{i}"), PathBuf::from("/p"));
        }
        assert_eq!(cache.buckets.len(), INITIAL_BUCKETS * 4);
        assert_eq!(cache.entries().count(), 700);
        assert!(cache.unset("cmd350"));
        assert!(!cache.unset("cmd350"));
        assert_eq!(cache.get("cmd699"), Some(PathBuf::from("/p")));

        cache.reset();
        assert_eq!(cache.entries().count(), 0);
        assert_eq!(cache.get("cmd1"), None);
    }
}
