//! A thread's own memo of what it has parsed, for the parses run again and again: a word
//! in a loop, a pattern, a string of tokens, an `eval`'s program.
//!
//! It took the place of `cached`'s 64-entry LRUs behind a lock (2026-10-17): each lookup
//! copied the text into a key and took the lock, and a loop over more than 64 distinct
//! words missed on every one of them, as an LRU cycled through does. Here the text is
//! looked up as it is, no lock is taken, and up to [`KEPT`] parses are kept for each set
//! of options (fewer where a parse can be large: [`Memo::keeping`]); past that they are
//! forgotten and parsed again as they come, as [`crate::arithmetic::parse_shared`]'s
//! are.

use std::cell::RefCell;
use std::hash::Hash;
use std::thread::LocalKey;

use rustc_hash::FxHashMap;

/// How many parses are kept for one set of options before they are forgotten.
pub const KEPT: usize = 4096;

/// The parses of one kind, by their options and their text.
pub struct Memo<O, V> {
    by_options: FxHashMap<O, FxHashMap<Box<str>, V>>,
    /// How many are kept for one set of options.
    limit: usize,
}

impl<O, V> Memo<O, V> {
    /// A memo that keeps up to `limit` parses for each set of options: fewer than
    /// [`KEPT`] for parses that can be large, as programs are.
    #[must_use]
    pub fn keeping(limit: usize) -> Self {
        Self {
            by_options: FxHashMap::default(),
            limit,
        }
    }
}

impl<O, V> Default for Memo<O, V> {
    fn default() -> Self {
        Self::keeping(KEPT)
    }
}

/// The parse of `text` under `options`, from `memo` when this thread parsed it before.
///
/// Else `parse` parses it, and the parse is kept when it succeeds, as `cached` kept it; a
/// text that does not parse is parsed again, for its error. `memo` is not borrowed while
/// `parse` runs, as a parse may parse again.
pub fn remembered<O, V, E>(
    memo: &'static LocalKey<RefCell<Memo<O, V>>>,
    text: &str,
    options: &O,
    parse: impl FnOnce() -> Result<V, E>,
) -> Result<V, E>
where
    O: Eq + Hash + Clone + 'static,
    V: Clone + 'static,
{
    let found = memo.with_borrow(|memo| {
        memo.by_options
            .get(options)
            .and_then(|parsed| parsed.get(text))
            .cloned()
    });
    if let Some(found) = found {
        return Ok(found);
    }
    let value = parse()?;
    memo.with_borrow_mut(|memo| {
        let limit = memo.limit;
        let parsed = memo.by_options.entry(options.clone()).or_default();
        if parsed.len() >= limit {
            parsed.clear();
        }
        parsed.insert(text.into(), value.clone());
    });
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::{KEPT, Memo, remembered};

    thread_local! {
        static SEEN: RefCell<Memo<bool, String>> = RefCell::default();
    }

    #[test]
    fn a_text_is_parsed_once_for_each_set_of_options_and_forgotten_past_the_limit() {
        let parses = Cell::new(0);
        let parse = |text: &str, upper: bool| {
            remembered(&SEEN, text, &upper, || {
                parses.set(parses.get() + 1);
                Ok::<_, ()>(if upper {
                    text.to_uppercase()
                } else {
                    text.to_owned()
                })
            })
            .unwrap()
        };
        assert_eq!(parse("ab", false), "ab");
        assert_eq!(parse("ab", false), "ab");
        assert_eq!(parse("ab", true), "AB");
        assert_eq!(parses.get(), 2);
        // More texts than are kept, cycled: each kept until the memo is full.
        for round in 0..2 {
            for n in 0..=KEPT {
                parse(&format!("w{n}"), false);
            }
            assert!(parses.get() > 2 + round * KEPT, "{}", parses.get());
        }
    }
}
