//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

use std::{
    collections::{HashMap, hash_map::Entry},
    rc::Rc,
};

use super::AwkValue;

/// A `for (key in array)` loop's position: the keys the array had when the loop began.
///
/// The loop owns its keys, so the array stays free to change under it, as in gawk: an
/// element added during the loop is not visited, and one deleted before its turn is
/// skipped. A loop left early by `break` or `return` holds nothing of the array's. The
/// array used to count its live loops and refuse insertions while any was live, and a
/// loop left early never gave its count back, so the array stayed locked for the rest of
/// the run (`REVIEW_REPORT.md` TXT-03).
#[cfg_attr(test, derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct KeyIterator {
    keys: Vec<Key>,
    index: usize,
}

pub type Key = Rc<str>;

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Copy, PartialEq)]
pub struct ValueIndex {
    index: usize,
}

pub type KeyValuePair = (Key, AwkValue);

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, PartialEq, Default)]
pub struct Array {
    key_map: HashMap<Key, usize>,
    pairs: Vec<Option<KeyValuePair>>,
}

impl Array {
    /// Remove the element with the given key.
    pub fn delete(&mut self, key: &str) {
        if let Some(pair_index) = self.key_map.remove(key) {
            self.pairs.swap_remove(pair_index);
            if let Some(Some((moved, _))) = self.pairs.get(pair_index)
                && let Some(index) = self.key_map.get_mut(moved)
            {
                *index = pair_index;
            }
        }
    }

    pub fn key_iter(&mut self) -> KeyIterator {
        KeyIterator {
            keys: self
                .pairs
                .iter()
                .flatten()
                .map(|(key, _)| key.clone())
                .collect(),
            index: 0,
        }
    }

    pub fn key_iter_next(&mut self, iter: &mut KeyIterator) -> Option<Key> {
        while let Some(key) = iter.keys.get(iter.index) {
            iter.index += 1;
            if self.key_map.contains_key(key) {
                return Some(key.clone());
            }
        }
        None
    }

    /// Get the `ValueIndex` of the key in the array. If the key does not exist, it will be
    /// inserted, with no type yet: as in gawk, an element that is only read can still
    /// become a subarray (`x = a[1]; a[1][2] = 3`).
    pub fn get_value_index(&mut self, key: Key) -> Result<ValueIndex, String> {
        match self.key_map.entry(key.clone()) {
            Entry::Occupied(e) => Ok(ValueIndex { index: *e.get() }),
            Entry::Vacant(e) => {
                let pair_index = self.pairs.len();
                self.pairs.push(Some((key, AwkValue::uninitialized())));
                e.insert(pair_index);
                Ok(ValueIndex { index: pair_index })
            }
        }
    }

    pub fn index_to_value(&mut self, index: ValueIndex) -> Option<&mut AwkValue> {
        self.pairs
            .get_mut(index.index)
            .and_then(Option::as_mut)
            .map(|(_, val)| val)
    }

    pub fn get_value(&mut self, key: Key) -> Result<&mut AwkValue, String> {
        let index = self.get_value_index(key)?;
        self.index_to_value(index)
            .ok_or_else(|| "array element vanished".to_string())
    }

    /// Set the array element at the given key to the given value
    pub fn set<V: Into<AwkValue>>(&mut self, key: String, value: V) -> Result<ValueIndex, String> {
        let index = self.get_value_index(Rc::<str>::from(key))?;
        if let Some(slot) = self.index_to_value(index) {
            *slot = value.into();
        }
        Ok(index)
    }

    /// The element with the given key, if there is one; none is made.
    pub fn existing_value(&mut self, key: &str) -> Option<&mut AwkValue> {
        let index = *self.key_map.get(key)?;
        self.pairs
            .get_mut(index)
            .and_then(Option::as_mut)
            .map(|(_, value)| value)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.key_map.contains_key(key)
    }

    pub fn clear(&mut self) {
        self.key_map.clear();
        self.pairs.clear();
    }

    pub fn len(&self) -> usize {
        self.key_map.len()
    }
}

impl<S: Into<String>, A: Into<AwkValue>> FromIterator<(S, A)> for Array {
    #[expect(
        clippy::expect_used,
        reason = "`set` cannot fail: `get_value_index` inserts a key it does not find"
    )]
    fn from_iter<T: IntoIterator<Item = (S, A)>>(iter: T) -> Self {
        let mut result = Self::default();
        for (key, val) in iter {
            result
                .set(key.into(), val)
                .expect("failed to insert into array");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterate_through_empty_array() {
        let mut array = Array::default();
        let mut iter = array.key_iter();
        assert_eq!(array.key_iter_next(&mut iter), None);
    }

    #[test]
    fn iterate_through_array() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        array.set("b".to_string(), 2.0).unwrap();
        array.set("c".to_string(), 3.0).unwrap();
        let mut iter = array.key_iter();
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("a")));
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("b")));
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("c")));
        assert_eq!(array.key_iter_next(&mut iter), None);
    }

    #[test]
    fn delete_from_array() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        array.delete("a");
        assert_eq!(array.len(), 0);
        assert_eq!(
            array.get_value("a".into()).cloned(),
            Ok(AwkValue::uninitialized())
        );
    }

    #[test]
    fn insert_element() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        assert_eq!(array.len(), 1);
        assert_eq!(
            array.get_value("a".into()).cloned(),
            Ok(AwkValue::from(1.0))
        );
    }

    #[test]
    fn insert_element_twice() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        array.set("a".to_string(), 2.0).unwrap();
        assert_eq!(array.len(), 1);
        assert_eq!(
            array.get_value("a".into()).cloned(),
            Ok(AwkValue::from(2.0))
        );
    }

    #[test]
    fn delete_element_with_active_iterator() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        array.set("b".to_string(), 1.0).unwrap();
        array.set("c".to_string(), 1.0).unwrap();
        array.set("d".to_string(), 1.0).unwrap();
        let mut iter = array.key_iter();
        array.delete("b");
        array.delete("d");
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("a")));
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("c")));
        assert_eq!(array.key_iter_next(&mut iter), None);
    }

    #[test]
    fn insert_with_active_iterator_is_allowed_and_not_visited() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        let mut iter = array.key_iter();
        assert!(array.set("b".to_string(), 2.0).is_ok());
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("a")));
        assert_eq!(array.key_iter_next(&mut iter), None);
        assert_eq!(array.len(), 2);
    }

    #[test]
    fn an_iterator_left_early_holds_nothing() {
        // `for (k in a) { break }`, then an insertion: it was refused for the rest of the
        // run, as the loop never gave its count back.
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        array.set("b".to_string(), 1.0).unwrap();
        let mut iter = array.key_iter();
        assert!(array.key_iter_next(&mut iter).is_some());
        drop(iter);
        assert!(array.set("c".to_string(), 1.0).is_ok());
    }

    #[test]
    fn clearing_during_iteration_ends_it() {
        // `for (k in a) delete a` sliced past the end of the emptied storage.
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        array.set("b".to_string(), 1.0).unwrap();
        let mut iter = array.key_iter();
        assert!(array.key_iter_next(&mut iter).is_some());
        array.clear();
        assert_eq!(array.key_iter_next(&mut iter), None);
    }

    #[test]
    fn insert_element_after_iterator_has_completed_is_ok() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        let mut iter = array.key_iter();
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("a")));
        assert_eq!(array.key_iter_next(&mut iter), None);
        assert!(array.set("e".to_string(), 2.0).is_ok());
        assert_eq!(array.len(), 2);
        assert_eq!(
            array.get_value("e".into()).cloned(),
            Ok(AwkValue::from(2.0))
        );
    }

    #[test]
    fn insert_after_delete_during_iteration_no_corruption() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        array.set("b".to_string(), 2.0).unwrap();
        array.set("c".to_string(), 3.0).unwrap();

        // Start iterator, delete "b" during iteration
        let mut iter = array.key_iter();
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("a")));
        array.delete("b");
        assert_eq!(array.key_iter_next(&mut iter), Some(Rc::from("c")));
        assert_eq!(array.key_iter_next(&mut iter), None);

        // Now insert a new element
        array.set("d".to_string(), 4.0).unwrap();

        // Verify all values are correct and no corruption occurred
        assert_eq!(array.len(), 3);
        assert_eq!(
            array.get_value("a".into()).cloned(),
            Ok(AwkValue::from(1.0))
        );
        assert!(!array.contains("b"));
        assert_eq!(
            array.get_value("c".into()).cloned(),
            Ok(AwkValue::from(3.0))
        );
        assert_eq!(
            array.get_value("d".into()).cloned(),
            Ok(AwkValue::from(4.0))
        );
    }

    #[test]
    fn interleave_iteration_and_deletion_with_multiple_iterators() {
        let mut array = Array::default();
        array.set("a".to_string(), 1.0).unwrap();
        array.set("b".to_string(), 2.0).unwrap();
        array.set("c".to_string(), 3.0).unwrap();
        let mut iter1 = array.key_iter();
        let mut iter2 = array.key_iter();
        assert_eq!(array.key_iter_next(&mut iter1), Some(Rc::from("a")));
        array.delete("a");
        assert_eq!(array.key_iter_next(&mut iter2), Some(Rc::from("b")));
        array.delete("b");
        assert_eq!(array.key_iter_next(&mut iter1), Some(Rc::from("c")));
        assert_eq!(array.key_iter_next(&mut iter2), Some(Rc::from("c")));
        assert_eq!(array.key_iter_next(&mut iter1), None);
        assert_eq!(array.key_iter_next(&mut iter2), None);
    }
}
