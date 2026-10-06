//! The matchers behind `grep`: ripgrep's regex matcher for every pattern the regex
//! crate takes, and `fancy-regex` behind the same trait for the rest, which is a
//! pattern with a backreference or with the lookarounds sed makes of GNU's word
//! boundaries.
//!
//! Where `-o` and `--color` need the extent of a match, GNU grep reports the longest
//! match at the leftmost position and the regex crate the first alternative's. The
//! alternatives are ordered longest first, as the bundled sed orders them
//! (`cash_sed::sed::fast_regex::sort_alternations_in_pattern`), which settles an
//! alternation of fixed lengths; where an alternative can match nothing (`x*\|b`),
//! a second regex with "all" match semantics, run anchored at the match's start,
//! gives the end GNU would report. The backtracking engine gets the ordering only.

use cash_sed::sed::fast_regex::sort_alternations_in_pattern;
use grep_matcher::{Captures, Match, Matcher, NoError};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{Searcher, Sink};
use regex_automata::meta;
use regex_automata::{Anchored, Input, MatchKind};

/// What the matcher is built from.
pub(super) struct Spec<'a> {
    /// The pattern, in the regex crate's syntax; every `-e` pattern already joined.
    pub pattern: &'a str,
    /// `-i`.
    pub case_insensitive: bool,
    /// `-w`.
    pub word: bool,
    /// `-x`.
    pub whole_line: bool,
    /// Whether the pattern needs the backtracking engine.
    pub fancy: bool,
    /// `-z`: a record ends at NUL and may hold newlines; anchors are the record's.
    pub null_data: bool,
    /// Whether `-o` or `--color` will ask for the extent of matches.
    pub spans: bool,
}

/// A matcher on `fancy-regex`, for a pattern with a backreference or a lookaround.
pub(super) struct FancyMatcher {
    regex: fancy_regex::Regex,
}

/// The captures of a `FancyMatcher`.
pub(super) struct FancyCaptures(Vec<Option<Match>>);

impl Captures for FancyCaptures {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn get(&self, i: usize) -> Option<Match> {
        self.0.get(i).copied().flatten()
    }
}

impl Matcher for FancyMatcher {
    type Captures = FancyCaptures;
    type Error = NoError;

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, NoError> {
        // A backtracking limit reached, or an offset inside a character, counts as no
        // match rather than ending the search.
        Ok(self
            .regex
            .find_from_pos(haystack, at)
            .ok()
            .flatten()
            .map(|m| Match::new(m.start(), m.end())))
    }

    fn new_captures(&self) -> Result<FancyCaptures, NoError> {
        Ok(FancyCaptures(vec![None; self.regex.captures_len()]))
    }

    fn capture_count(&self) -> usize {
        self.regex.captures_len()
    }

    fn captures_at(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut FancyCaptures,
    ) -> Result<bool, NoError> {
        let Some(found) = self.regex.captures_from_pos(haystack, at).ok().flatten() else {
            return Ok(false);
        };
        for (i, slot) in caps.0.iter_mut().enumerate() {
            *slot = found.get(i).map(|m| Match::new(m.start(), m.end()));
        }
        Ok(true)
    }
}

/// The matcher built for a run.
enum Kind {
    Regex(RegexMatcher),
    Fancy(FancyMatcher),
}

/// A matcher, with the regex that extends a match to GNU's length.
pub(super) struct Engine {
    kind: Kind,
    longest: Option<meta::Regex>,
}

/// GNU's message for a pattern the engine refuses although the translation accepted
/// it: a repetition the compiled program cannot hold, or something rarer.
fn gnu_message(error: &str) -> String {
    let lowered = error.to_ascii_lowercase();
    if lowered.contains("size limit")
        || lowered.contains("too big")
        || lowered.contains("too large")
        || lowered.contains("exceed")
    {
        "Regular expression too big".to_owned()
    } else {
        "Invalid regular expression".to_owned()
    }
}

impl Engine {
    /// Builds the matcher; the error is GNU's message, without its `grep: `.
    pub(super) fn build(spec: &Spec<'_>) -> Result<Self, String> {
        let sorted = sort_alternations_in_pattern(spec.pattern);
        // `-w` as GNU defines it: a match preceded and followed by a non-word
        // character or the line's edge. `-x`: the whole line.
        let wrapped = if spec.whole_line {
            std::format!("^(?:{sorted})$")
        } else if spec.word {
            if spec.fancy || spec.null_data {
                std::format!(r"(?<!\w)(?:{sorted})(?!\w)")
            } else {
                std::format!(r"\b{{start-half}}(?:{sorted})\b{{end-half}}")
            }
        } else {
            sorted
        };
        // With NUL records the backtracker is used too: the searcher runs it on each
        // record alone, so `^` and `$` are the record's edges, where the regex
        // matcher would run over the whole buffer with `\n` as its line.
        if spec.fancy || spec.null_data {
            // In a NUL-terminated record a newline is data, which `.` matches.
            let wrapped = if spec.null_data {
                std::format!("(?s){wrapped}")
            } else {
                wrapped
            };
            let regex = fancy_regex::RegexBuilder::new(&wrapped)
                .case_insensitive(spec.case_insensitive)
                .build()
                .map_err(|e| gnu_message(&e.to_string()))?;
            return Ok(Self {
                kind: Kind::Fancy(FancyMatcher { regex }),
                longest: None,
            });
        }
        // With NUL records the searcher strips each record's terminator and runs the
        // matcher on the record alone, so `^` and `$` are the record's edges.
        let mut builder = RegexMatcherBuilder::new();
        builder
            .case_insensitive(spec.case_insensitive)
            .unicode(true)
            .multi_line(!spec.null_data)
            .line_terminator(Some(if spec.null_data { 0 } else { b'\n' }));
        let matcher = builder
            .build(&wrapped)
            .map_err(|e| gnu_message(&e.to_string()))?;
        let longest = spec.spans.then(|| {
            let syntax = regex_automata::util::syntax::Config::new()
                .case_insensitive(spec.case_insensitive)
                .multi_line(!spec.null_data)
                .unicode(true)
                .utf8(false);
            meta::Builder::new()
                .configure(
                    meta::Config::new()
                        .match_kind(MatchKind::All)
                        .utf8_empty(false),
                )
                .syntax(syntax)
                .build(&wrapped)
                .ok()
        });
        Ok(Self {
            kind: Kind::Regex(matcher),
            longest: longest.flatten(),
        })
    }

    /// The leftmost match in `haystack` at or after `at`, as long as GNU would make it.
    pub(super) fn find_at(&self, haystack: &[u8], at: usize) -> Option<(usize, usize)> {
        let found = match &self.kind {
            Kind::Regex(m) => m.find_at(haystack, at).ok().flatten(),
            Kind::Fancy(m) => m.find_at(haystack, at).ok().flatten(),
        }?;
        let (start, mut end) = (found.start(), found.end());
        if let Some(longest) = &self.longest {
            let input = Input::new(haystack).anchored(Anchored::Yes).range(start..);
            if let Some(m) = longest.find(input) {
                end = end.max(m.end());
            }
        }
        Some((start, end))
    }

    /// Runs `searcher` over `reader` into `sink` with this matcher.
    pub(super) fn search<R: std::io::Read, S: Sink>(
        &self,
        searcher: &mut Searcher,
        reader: R,
        sink: S,
    ) -> Result<(), S::Error> {
        match &self.kind {
            Kind::Regex(m) => searcher.search_reader(m, reader, sink),
            Kind::Fancy(m) => searcher.search_reader(m, reader, sink),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Engine, Spec};

    /// The first match of `pattern` in `haystack` from `at`, or `None` for none or for
    /// a pattern that does not build.
    fn find(pattern: &str, word: bool, haystack: &[u8], at: usize) -> Option<(usize, usize)> {
        Engine::build(&Spec {
            pattern,
            case_insensitive: false,
            word,
            whole_line: false,
            fancy: pattern.contains('\\'),
            null_data: false,
            spans: true,
        })
        .ok()
        .and_then(|e| e.find_at(haystack, at))
    }

    #[test]
    fn the_longest_match_at_the_leftmost_position_as_in_gnu_grep() {
        assert_eq!(find("a|ab", false, b"ab", 0), Some((0, 2)));
        assert_eq!(find("a|ab", false, b"xyz", 0), None);
        assert_eq!(
            find("[0-9]+|[0-9]+\\.[0-9]+", false, b"3.14", 0),
            Some((0, 4))
        );
        assert_eq!(find("(a|ab)(c|bc)", false, b"abc", 0), Some((0, 3)));
        // An alternative that matches nothing does not hide one that matches.
        assert_eq!(find("x*|b", false, b"abc", 1), Some((1, 2)));
        assert_eq!(find("x*|b", false, b"abc", 0), Some((0, 0)));
    }

    #[test]
    fn word_matches_need_a_non_word_character_or_an_edge_on_both_sides() {
        assert_eq!(find("foo", true, b"foobar foo", 0), Some((7, 10)));
        assert_eq!(find("foo", true, b"foo-bar", 0), Some((0, 3)));
        assert_eq!(find("foo", true, b"_foo", 0), None);
        assert_eq!(find("foo", true, "\u{e9}foo".as_bytes(), 0), None);
        assert_eq!(find("abc|abc d", true, b"abc d", 0), Some((0, 5)));
        assert_eq!(find("-", true, b"a - b", 0), Some((2, 3)));
        assert_eq!(find("-", true, b"a-b", 0), None);
    }

    #[test]
    fn a_backreference_runs_on_the_backtracker() {
        assert_eq!(find(r"(a)\1", false, b"xaab", 0), Some((1, 3)));
        assert_eq!(find(r"(a)\1", false, b"ab", 0), None);
        assert_eq!(find(r"(ab)\1", true, b"abab ababx", 0), Some((0, 4)));
    }
}
