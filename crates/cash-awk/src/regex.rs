//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
// Copyright (c) 2026 Cash project contributors.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

use regex_automata::meta::Regex as MetaRegex;
use regex_automata::{Input, MatchKind};
use std::ffi::CString;

/// A regex wrapper that provides CString-compatible API for AWK.
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
/// Owns the input CString to preserve lifetimes.
pub struct MatchIter<'re> {
    string: String,
    next_start: usize,
    regex: &'re Regex,
}

impl Iterator for MatchIter<'_> {
    type Item = RegexMatch;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_start > self.string.len() {
            return None;
        }

        let input = Input::new(&self.string).range(self.next_start..);
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
    pub fn new(regex: CString) -> Result<Self, String> {
        let pattern = regex.to_str().map_err(|e| e.to_string())?;
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

    /// Returns the first match location in the string, or `None`.
    pub fn find_first(&self, string: &str) -> Option<RegexMatch> {
        self.inner.find(string).map(|m| RegexMatch {
            start: m.start(),
            end: m.end(),
        })
    }

    /// Returns an iterator over all match locations in the string.
    /// Takes ownership of the CString.
    pub fn match_locations(&self, string: CString) -> MatchIter<'_> {
        let s = string.into_string().unwrap_or_default();
        MatchIter {
            next_start: 0,
            regex: self,
            string: s,
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern_string
    }

    pub fn matches(&self, string: &CString) -> bool {
        let s = string.to_str().unwrap_or("");
        self.inner.is_match(s)
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
#[cfg(test)]
pub fn regex_from_str(re: &str) -> Regex {
    Regex::new(CString::new(re).unwrap()).expect("error compiling ere")
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
        assert!(ere.matches(&CString::new("abbbbc").unwrap()));
    }

    #[test]
    fn test_leftmost_longest_ere() {
        let ere = regex_from_str("a|aa");
        let m = ere.find_first("aa").unwrap();
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
        let mut iter = ere.match_locations(CString::new(text).unwrap());
        assert_eq!(iter.next(), Some(RegexMatch { start: 0, end: 5 }));
        assert_eq!(iter.next(), Some(RegexMatch { start: 12, end: 17 }));
        assert_eq!(iter.next(), Some(RegexMatch { start: 19, end: 24 }));
        assert_eq!(iter.next(), Some(RegexMatch { start: 24, end: 29 }));
        assert_eq!(iter.next(), None);
    }
}
