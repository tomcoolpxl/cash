//! A backtracking matcher for patterns with `!(…)`.
//!
//! `!(p)` matches any text `p` does not, and a pattern holding it matches when *some*
//! way of splitting the text gives the `!(…)` part a text `p` does not match and the
//! rest of the pattern the rest. A regular expression cannot say that: the translation
//! to one (a lookahead and an atomic group) let `!(*.tar|*.tar.gz)` match `x.tar.gz` and
//! `[[ ab == !(a|ab) ]]` be true (LANG-01). This matcher tries the splits, as Bash's
//! `extmatch` does.
//!
//! It works out, for a part of the pattern and a place in the text, the set of places a
//! match of that part can end, so that `!(p)` is every place `p` cannot end. The text
//! between extended patterns is matched by a lazy DFA stepped from where it starts,
//! which gives all its ends in one pass. What is worked out is kept for as long as the
//! same text is matched: a search for `${x//!(…)/…}` asks about many places of one text.
//! Asking about each span of the text in turn took minutes for 400 characters.

use std::collections::HashMap;
use std::sync::Mutex;

use regex_automata::{Anchored, Input, hybrid::dfa};

use crate::{error, regex};

/// One part of a pattern.
#[derive(Debug)]
enum Node {
    /// Pattern text with no extended pattern in it.
    Plain(Box<Plain>),
    /// An extended pattern: its kind and its alternatives.
    Extended(char, Vec<Sequence>),
}

/// Pattern text with no extended pattern in it.
#[derive(Debug)]
struct Plain {
    /// The text as an anchored regular expression, for when the DFA cannot be built or
    /// gives up.
    regex: fancy_regex::Regex,
    dfa: Option<dfa::DFA>,
    cache: Mutex<Option<dfa::Cache>>,
}

impl Plain {
    /// The places (character indices) at which a match of this part from the place
    /// `start` can end.
    fn ends(&self, text: &str, boundaries: &[usize], start: usize) -> Result<Places, error::Error> {
        let mut places = Places::default();
        let start_byte = boundaries.get(start).copied().unwrap_or(text.len());
        if let Some(ends) = self.dfa_ends(text, start_byte, text.len()) {
            for end in ends {
                if let Ok(place) = boundaries.binary_search(&end) {
                    places.insert(place);
                }
            }
            return Ok(places);
        }
        for (place, &end) in boundaries.iter().enumerate().skip(start) {
            if self
                .regex
                .is_match(text.get(start_byte..end).unwrap_or_default())?
            {
                places.insert(place);
            }
        }
        Ok(places)
    }

    /// [`Self::ends`] by stepping the DFA; `None` if it cannot.
    fn dfa_ends(&self, text: &str, start: usize, end: usize) -> Option<Vec<usize>> {
        let dfa = self.dfa.as_ref()?;
        let mut guard = self.cache.lock().ok()?;
        let cache = guard.get_or_insert_with(|| dfa.create_cache());
        let input = Input::new(text).range(start..end).anchored(Anchored::Yes);
        let mut state = dfa.start_state_forward(cache, &input).ok()?;
        let bytes = text.as_bytes();
        let mut ends = Vec::new();
        let mut at = start;
        loop {
            if text.is_char_boundary(at) && dfa.next_eoi_state(cache, state).ok()?.is_match() {
                ends.push(at);
            }
            if at == end {
                break;
            }
            state = dfa.next_state(cache, state, *bytes.get(at)?).ok()?;
            if state.is_quit() {
                return None;
            }
            if state.is_dead() {
                break;
            }
            at += 1;
        }
        drop(guard);
        Some(ends)
    }
}

/// A pattern, or one alternative of an extended pattern: parts matched one after another.
#[derive(Debug)]
struct Sequence {
    id: usize,
    nodes: Vec<Node>,
}

/// Where a match of the nodes from (sequence, node) that starts at a place can end, by
/// place (character index); the last field tells the repetitions of `+(…)` and `*(…)`
/// from the node as a whole.
type Memo = HashMap<(usize, usize, usize, bool), Places>;

/// A set of places in a text, by character index.
#[derive(Clone, Debug, Default)]
struct Places(Vec<u64>);

impl Places {
    fn insert(&mut self, place: usize) {
        let word = place / 64;
        if self.0.len() <= word {
            self.0.resize(word + 1, 0);
        }
        if let Some(bits) = self.0.get_mut(word) {
            *bits |= 1 << (place % 64);
        }
    }

    fn contains(&self, place: usize) -> bool {
        self.0
            .get(place / 64)
            .is_some_and(|bits| bits & (1 << (place % 64)) != 0)
    }

    fn union_with(&mut self, other: &Self) {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        for (mine, theirs) in self.0.iter_mut().zip(&other.0) {
            *mine |= theirs;
        }
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(word, &bits)| {
            (0..64)
                .filter(move |bit| bits & (1 << bit) != 0)
                .map(move |bit| word * 64 + bit)
        })
    }

    fn last(&self) -> Option<usize> {
        self.iter().last()
    }
}

/// A compiled pattern that holds `!(…)`.
#[derive(Debug)]
pub(crate) struct BacktrackingMatcher {
    sequence: Sequence,
    /// The text last matched, and what was worked out about it.
    last: Mutex<Option<(String, Memo)>>,
}

impl BacktrackingMatcher {
    /// The matcher for `pattern`, glob text as `Pattern::to_regex_str` builds it, if it
    /// holds a `!(…)`; `None` otherwise, or if it is malformed, which leaves it to the
    /// regular expression.
    pub(crate) fn for_pattern(
        pattern: &str,
        case_insensitive: bool,
        multiline: bool,
    ) -> Result<Option<Self>, error::Error> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut parser = Parser {
            chars: &chars,
            pos: 0,
            next_id: 0,
            has_negation: false,
            case_insensitive,
            multiline,
        };
        let Some(sequence) = parser.sequence(false)? else {
            return Ok(None);
        };
        if parser.pos != chars.len() || !parser.has_negation {
            return Ok(None);
        }
        Ok(Some(Self {
            sequence,
            last: Mutex::new(None),
        }))
    }

    /// Whether the whole of `text` matches.
    pub(crate) fn is_match(&self, text: &str) -> Result<bool, error::Error> {
        self.with_run(text, |run, sequence| {
            let last = run.boundaries.len() - 1;
            Ok(run.ends(sequence, 0, 0)?.contains(last))
        })
    }

    /// Where the pattern matches in `text` at or after `from`: the leftmost start, and the
    /// longest match there; only at the start or only to the end when asked.
    pub(crate) fn find(
        &self,
        text: &str,
        from: usize,
        at_start: bool,
        at_end: bool,
    ) -> Result<Option<(usize, usize)>, error::Error> {
        self.with_run(text, |run, sequence| {
            let last = run.boundaries.len() - 1;
            let first = run.boundaries.partition_point(|&at| at < from);
            for start in first..=last {
                if at_start && start != 0 {
                    break;
                }
                let ends = run.ends(sequence, 0, start)?;
                let end = if at_end {
                    ends.contains(last).then_some(last)
                } else {
                    ends.last()
                };
                if let Some(end) = end {
                    let byte =
                        |place: usize| run.boundaries.get(place).copied().unwrap_or_default();
                    return Ok(Some((byte(start), byte(end))));
                }
            }
            Ok(None)
        })
    }

    /// Runs `f` with what is known about `text`, kept for the next call on the same text.
    fn with_run<R>(
        &self,
        text: &str,
        f: impl FnOnce(&mut Run<'_>, &Sequence) -> Result<R, error::Error>,
    ) -> Result<R, error::Error> {
        let memo = self
            .last
            .lock()
            .ok()
            .and_then(|mut last| last.take())
            .filter(|(last_text, _)| last_text == text)
            .map(|(_, memo)| memo)
            .unwrap_or_default();
        let mut run = Run {
            text,
            boundaries: text
                .char_indices()
                .map(|(i, _)| i)
                .chain(std::iter::once(text.len()))
                .collect(),
            memo,
        };
        let result = f(&mut run, &self.sequence);
        if let Ok(mut last) = self.last.lock() {
            *last = Some((text.to_owned(), run.memo));
        }
        result
    }
}

struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
    next_id: usize,
    has_negation: bool,
    case_insensitive: bool,
    multiline: bool,
}

impl Parser<'_> {
    /// Parts up to the end, or inside an extended pattern up to its next `|` or `)`.
    /// `None` for an extended pattern left open.
    fn sequence(&mut self, inside: bool) -> Result<Option<Sequence>, error::Error> {
        let id = self.next_id;
        self.next_id += 1;
        let mut nodes = Vec::new();
        let mut plain = String::new();

        while let Some(&c) = self.chars.get(self.pos) {
            if inside && matches!(c, '|' | ')') {
                break;
            }
            if c == '\\' {
                plain.push(c);
                self.pos += 1;
                if let Some(&escaped) = self.chars.get(self.pos) {
                    plain.push(escaped);
                    self.pos += 1;
                }
            } else if c == '[' {
                let end = self.bracket_end();
                plain.extend(self.chars.get(self.pos..end).unwrap_or_default());
                self.pos = end;
            } else if matches!(c, '?' | '*' | '+' | '@' | '!')
                && self.chars.get(self.pos + 1) == Some(&'(')
            {
                if !plain.is_empty() {
                    nodes.push(self.plain(&std::mem::take(&mut plain))?);
                }
                self.pos += 2;
                let mut alternatives = Vec::new();
                loop {
                    let Some(alternative) = self.sequence(true)? else {
                        return Ok(None);
                    };
                    alternatives.push(alternative);
                    match self.chars.get(self.pos) {
                        Some('|') => self.pos += 1,
                        Some(')') => {
                            self.pos += 1;
                            break;
                        }
                        _ => return Ok(None),
                    }
                }
                if c == '!' {
                    self.has_negation = true;
                }
                nodes.push(Node::Extended(c, alternatives));
            } else {
                plain.push(c);
                self.pos += 1;
            }
        }
        if !plain.is_empty() {
            nodes.push(self.plain(&plain)?);
        }
        Ok(Some(Sequence { id, nodes }))
    }

    /// Where the bracket expression at `pos` ends, past its `]`; just past the `[` when it
    /// has none, which leaves the `[` itself to the regex translation.
    fn bracket_end(&self) -> usize {
        let mut at = self.pos + 1;
        if matches!(self.chars.get(at), Some('!' | '^')) {
            at += 1;
        }
        if self.chars.get(at) == Some(&']') {
            at += 1;
        }
        while let Some(&c) = self.chars.get(at) {
            match c {
                ']' => return at + 1,
                '\\' => at += 2,
                '[' if self.chars.get(at + 1) == Some(&':') => {
                    let close = (at + 2..self.chars.len().saturating_sub(1)).find(|&i| {
                        self.chars.get(i) == Some(&':') && self.chars.get(i + 1) == Some(&']')
                    });
                    at = close.map_or(at + 1, |close| close + 2);
                }
                _ => at += 1,
            }
        }
        self.pos + 1
    }

    fn plain(&self, pattern: &str) -> Result<Node, error::Error> {
        let translated = cash_parser::pattern::pattern_to_regex_str(pattern, true)?;
        let regex = regex::compile_regex(
            std::format!("^(?:{translated})$"),
            self.case_insensitive,
            self.multiline,
        )?;
        let dfa = regex::every_match_dfa(&translated, self.case_insensitive, self.multiline);
        Ok(Node::Plain(Box::new(Plain {
            regex,
            dfa,
            cache: Mutex::new(None),
        })))
    }
}

/// One text being matched, with what has been worked out about it.
struct Run<'t> {
    text: &'t str,
    /// The byte offset of each place: each character's, and the end.
    boundaries: Vec<usize>,
    memo: Memo,
}

impl Run<'_> {
    /// Where a match of the nodes of `sequence` from `node` on, starting at `start`, can
    /// end.
    fn ends(
        &mut self,
        sequence: &Sequence,
        node: usize,
        start: usize,
    ) -> Result<Places, error::Error> {
        let key = (sequence.id, node, start, false);
        if let Some(known) = self.memo.get(&key) {
            return Ok(known.clone());
        }
        let mut ends = Places::default();
        match sequence.nodes.get(node) {
            None => ends.insert(start),
            Some(Node::Plain(plain)) => {
                for split in plain.ends(self.text, &self.boundaries, start)?.iter() {
                    ends.union_with(&self.ends(sequence, node + 1, split)?);
                }
            }
            Some(Node::Extended(kind, alternatives)) => {
                let matched = self.alternative_ends(alternatives, start)?;
                match kind {
                    '@' | '?' => {
                        if *kind == '?' {
                            ends.union_with(&self.ends(sequence, node + 1, start)?);
                        }
                        for split in matched.iter() {
                            ends.union_with(&self.ends(sequence, node + 1, split)?);
                        }
                    }
                    '*' => ends = self.repeated(sequence, node, alternatives, start)?,
                    '+' => {
                        for split in matched.iter().filter(|&split| split > start) {
                            ends.union_with(&self.repeated(sequence, node, alternatives, split)?);
                        }
                    }
                    _ => {
                        let last = self.boundaries.len() - 1;
                        for split in (start..=last).filter(|&split| !matched.contains(split)) {
                            ends.union_with(&self.ends(sequence, node + 1, split)?);
                        }
                    }
                }
            }
        }
        self.memo.insert(key, ends.clone());
        Ok(ends)
    }

    /// Where a match of one of the alternatives starting at `start` can end.
    fn alternative_ends(
        &mut self,
        alternatives: &[Sequence],
        start: usize,
    ) -> Result<Places, error::Error> {
        let mut ends = Places::default();
        for alternative in alternatives {
            ends.union_with(&self.ends(alternative, 0, start)?);
        }
        Ok(ends)
    }

    /// Where `*(…)` at `node` and the rest of the sequence can end from `start`: none of
    /// it and the rest, or one alternative over some text and again from after it. Worked
    /// out from the end of the text back, so that the depth of the calls does not grow
    /// with the text.
    fn repeated(
        &mut self,
        sequence: &Sequence,
        node: usize,
        alternatives: &[Sequence],
        start: usize,
    ) -> Result<Places, error::Error> {
        let key = |place| (sequence.id, node, place, true);
        if let Some(known) = self.memo.get(&key(start)) {
            return Ok(known.clone());
        }
        let last = self.boundaries.len() - 1;
        for place in (start..=last).rev() {
            if self.memo.contains_key(&key(place)) {
                continue;
            }
            let mut ends = self.ends(sequence, node + 1, place)?;
            let matched = self.alternative_ends(alternatives, place)?;
            for split in matched.iter().filter(|&split| split > place) {
                if let Some(again) = self.memo.get(&key(split)) {
                    ends.union_with(again);
                }
            }
            self.memo.insert(key(place), ends);
        }
        Ok(self.memo.get(&key(start)).cloned().unwrap_or_default())
    }
}
