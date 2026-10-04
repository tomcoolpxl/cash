//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
// Copyright (c) 2026 Cash project contributors.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

use regex_automata::Input;
use regex_automata::meta::Regex as MetaRegex;

/// A regex wrapper for AWK. Text is a `&str`, so a NUL is an ordinary character: the
/// wrapper took C strings, from posixutils' libc regex, and a NUL in a record was
/// fatal (`REVIEW_REPORT.md` TXT-11).
/// Uses regex_automata configured for MatchKind::All and selects leftmost-longest
/// for POSIX ERE support.
pub struct Regex {
    inner: MetaRegex,
    pattern_string: String,
}

#[cfg_attr(test, derive(Debug))]
#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub struct RegexMatch {
    pub start: usize,
    pub end: usize,
}

/// Iterator over regex matches in a string.
pub struct MatchIter<'re, 's> {
    string: &'s str,
    next_start: usize,
    regex: &'re Regex,
}

impl Iterator for MatchIter<'_, '_> {
    type Item = RegexMatch;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_start > self.string.len() {
            return None;
        }

        let input = Input::new(self.string).range(self.next_start..);
        let m = self.regex.inner.find(input)?;

        let result = RegexMatch {
            start: m.start(),
            end: m.end(),
        };

        // Move past this match for next iteration
        // Ensure we make progress even on zero-width matches
        self.next_start = if result.end > self.next_start {
            result.end
        } else {
            let mut next = self.next_start + 1;
            while next < self.string.len() && !self.string.is_char_boundary(next) {
                next += 1;
            }
            next
        };

        Some(result)
    }
}

fn hir_max_len(hir: &regex_syntax::hir::Hir) -> usize {
    use regex_syntax::hir::HirKind::*;
    match hir.kind() {
        Empty => 0,
        Literal(l) => l.0.len(),
        Class(_) => 1,
        Look(_) => 0,
        Repetition(rep) => {
            let inner_max = hir_max_len(&rep.sub);
            match rep.max {
                Some(max) => inner_max.saturating_mul(max as usize),
                None => usize::MAX / 2,
            }
        }
        Capture(cap) => hir_max_len(&cap.sub),
        Concat(subs) => {
            let mut total: usize = 0;
            for sub in subs {
                total = total.saturating_add(hir_max_len(sub));
            }
            total
        }
        Alternation(subs) => subs.iter().map(hir_max_len).max().unwrap_or(0),
    }
}

fn sort_alternations(hir: regex_syntax::hir::Hir) -> regex_syntax::hir::Hir {
    use regex_syntax::hir::{Hir, HirKind::*};
    match hir.into_kind() {
        Repetition(mut rep) => {
            rep.sub = Box::new(sort_alternations(*rep.sub));
            Hir::repetition(rep)
        }
        Capture(mut cap) => {
            cap.sub = Box::new(sort_alternations(*cap.sub));
            Hir::capture(cap)
        }
        Concat(subs) => Hir::concat(subs.into_iter().map(sort_alternations).collect()),
        Alternation(subs) => {
            let mut sorted: Vec<Hir> = subs.into_iter().map(sort_alternations).collect();
            sorted.sort_by(|a, b| hir_max_len(b).cmp(&hir_max_len(a)));
            Hir::alternation(sorted)
        }
        Look(look) => Hir::look(look),
        Class(class) => Hir::class(class),
        Literal(lit) => Hir::literal(lit.0),
        Empty => Hir::empty(),
    }
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Self, String> {
        let inner = if let Ok(hir) = regex_syntax::ParserBuilder::new().build().parse(pattern) {
            let hir = sort_alternations(hir);
            MetaRegex::builder()
                .build_from_hir(&hir)
                .or_else(|_| MetaRegex::new(pattern))
        } else {
            MetaRegex::new(pattern)
        }
        .map_err(|e| e.to_string())?;

        Ok(Self {
            inner,
            pattern_string: pattern.to_string(),
        })
    }

    /// The first match that is not empty, in bytes that need not be UTF-8 nor end on a
    /// character: where a record ends when this is RS. An empty match ends no record, as
    /// it would end one at every position and never move on.
    pub fn find_separator(&self, bytes: &[u8]) -> Option<RegexMatch> {
        self.inner
            .find_iter(bytes)
            .find(|m| !m.is_empty())
            .map(|m| RegexMatch {
                start: m.start(),
                end: m.end(),
            })
    }

    /// Returns an iterator over all match locations in the string.
    pub fn match_locations<'s>(&self, string: &'s str) -> MatchIter<'_, 's> {
        MatchIter {
            next_start: 0,
            regex: self,
            string,
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern_string
    }

    pub fn matches(&self, string: &str) -> bool {
        self.inner.is_match(string)
    }
}

#[cfg(test)]
impl core::fmt::Debug for Regex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "/{}/", self.pattern_string)
    }
}

impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.pattern_string == other.pattern_string
    }
}

/// utility function for writing tests
///
/// # Panics
///
/// If `re` is not a valid regex: the test written with it is wrong.
#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a test helper: a bad pattern should fail the test loudly"
)]
pub fn regex_from_str(re: &str) -> Regex {
    Regex::new(re).expect("error compiling ere")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_regex() {
        regex_from_str("test");
    }

    #[test]
    fn test_regex_matches() {
        let ere = regex_from_str("ab*c");
        assert!(ere.matches("abbbbc"));
    }

    #[test]
    fn test_leftmost_longest_ere() {
        let ere = regex_from_str("a|aa");
        let m = ere.find_separator(b"aa").unwrap();
        assert_eq!(m.start, 0);
        assert_eq!(m.end, 2);
    }

    #[test]
    fn test_regex_match_locations() {
        let ere = regex_from_str("match");
        let text = "match 12345 match2 matchmatch";
        for m in ere.inner.find_iter(text) {
            println!("find_iter match: {m:?}");
        }
        let mut iter = ere.match_locations(text);
        assert_eq!(iter.next(), Some(RegexMatch { start: 0, end: 5 }));
        assert_eq!(iter.next(), Some(RegexMatch { start: 12, end: 17 }));
        assert_eq!(iter.next(), Some(RegexMatch { start: 19, end: 24 }));
        assert_eq!(iter.next(), Some(RegexMatch { start: 24, end: 29 }));
        assert_eq!(iter.next(), None);
    }
}
