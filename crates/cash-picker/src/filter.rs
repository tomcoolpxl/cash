//! Typing in the picker: a fuzzy filter on names (spec D73).
//!
//! `crt` matches `crates`, best matches first, with the matched letters for
//! highlighting. Matching is `nucleo-matcher`'s, the Helix editor's matcher.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// A candidate that matched, with its score and the matched characters' positions.
#[derive(Debug, PartialEq, Eq)]
pub struct Match {
    /// The candidate's index in what was searched.
    pub index: usize,
    /// How well it matched: higher is better.
    pub score: u32,
    /// Character positions in the candidate, ascending.
    pub positions: Vec<usize>,
}

/// Matches typed text against paths, keeping its buffers between keystrokes.
pub struct Fuzzy {
    matcher: Matcher,
    pattern: Pattern,
    typed: String,
}

impl Default for Fuzzy {
    fn default() -> Self {
        Self {
            // Path matching weighs a match in the last component, and after a `/`, more.
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
            pattern: Pattern::default(),
            typed: String::new(),
        }
    }
}

impl Fuzzy {
    /// Sets what was typed; case is ignored, as Windows ignores it in names.
    pub fn set(&mut self, typed: &str) {
        if typed != self.typed {
            self.pattern = Pattern::parse(typed, CaseMatching::Ignore, Normalization::Smart);
            typed.clone_into(&mut self.typed);
        }
    }

    /// Whether anything was typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.typed.trim().is_empty()
    }

    /// The candidates that match, best first; equal scores keep the candidates' order,
    /// so nearer entries (searched first) come first.
    pub fn rank<'a>(&mut self, candidates: impl Iterator<Item = &'a str>) -> Vec<Match> {
        let mut buffer = Vec::new();
        let mut positions = Vec::new();
        let mut found: Vec<Match> = candidates
            .enumerate()
            .filter_map(|(index, text)| {
                positions.clear();
                let haystack = Utf32Str::new(text, &mut buffer);
                let score = self
                    .pattern
                    .indices(haystack, &mut self.matcher, &mut positions)?;
                let mut positions: Vec<usize> = positions
                    .iter()
                    .filter_map(|&p| usize::try_from(p).ok())
                    .collect();
                positions.sort_unstable();
                positions.dedup();
                Some(Match {
                    index,
                    score,
                    positions,
                })
            })
            .collect();
        found.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_in_order_match_and_the_best_comes_first() {
        let mut fuzzy = Fuzzy::default();
        fuzzy.set("crt");
        let candidates = [
            "cash/crates",
            "docs",
            "cool8-cpu/crate-tools",
            "vendor/croot",
        ];
        let found = fuzzy.rank(candidates.iter().copied());
        let order: Vec<&str> = found.iter().map(|m| candidates[m.index]).collect();
        assert!(!order.contains(&"docs"), "{order:?}");
        // `crates` in a name beats letters spread over `croot`.
        assert_eq!(order.last(), Some(&"vendor/croot"), "{order:?}");
        assert_eq!(order.len(), 3, "{order:?}");
    }

    #[test]
    fn case_is_ignored_and_positions_point_at_the_letters() {
        let mut fuzzy = Fuzzy::default();
        fuzzy.set("CR");
        let found = fuzzy.rank(std::iter::once("src/crates"));
        assert_eq!(found.len(), 1);
        let text: Vec<char> = "src/crates".chars().collect();
        let letters: String = found[0].positions.iter().map(|&p| text[p]).collect();
        assert_eq!(letters.to_lowercase(), "cr");
    }
}
