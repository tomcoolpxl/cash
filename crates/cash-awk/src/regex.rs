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
use regex_automata::{Anchored, Input};

/// A regex wrapper for AWK. Text is a `&str`, so a NUL is an ordinary character: the
/// wrapper took C strings, from posixutils' libc regex, and a NUL in a record was
/// fatal (`REVIEW_REPORT.md` TXT-11).
/// Uses regex_automata configured for MatchKind::All and selects leftmost-longest
/// for POSIX ERE support.
pub struct Regex {
    inner: MetaRegex,
    /// The regex in the engine's syntax, for `folded`.
    translated: String,
    /// The regex that ignores case, made when IGNORECASE first asks for it.
    folded: std::cell::OnceCell<MetaRegex>,
    pattern_string: String,
}

thread_local! {
    /// gawk's IGNORECASE, which every regex follows when it matches.
    static IGNORE_CASE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes every regex ignore case, or heed it, as IGNORECASE says.
pub fn set_ignore_case(ignore: bool) {
    IGNORE_CASE.set(ignore);
}

/// The engine of `translated`, ignoring case when `fold`.
fn engine(translated: &str, fold: bool) -> Result<MetaRegex, String> {
    match regex_syntax::ParserBuilder::new()
        .case_insensitive(fold)
        .build()
        .parse(translated)
    {
        Ok(hir) => {
            let hir = sort_alternations(hir);
            MetaRegex::builder()
                .build_from_hir(&hir)
                .or_else(|_| MetaRegex::new(translated))
                .map_err(|e| e.to_string())
        }
        Err(regex_syntax::Error::Parse(error)) => Err(error.kind().to_string()),
        Err(regex_syntax::Error::Translate(error)) => Err(error.kind().to_string()),
        Err(error) => Err(error.to_string()),
    }
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
    /// Where the match before ended.
    last_end: Option<usize>,
}

impl MatchIter<'_, '_> {
    /// The first character boundary after `at`.
    fn after(&self, at: usize) -> usize {
        let mut next = at + 1;
        while next < self.string.len() && !self.string.is_char_boundary(next) {
            next += 1;
        }
        next
    }
}

impl Iterator for MatchIter<'_, '_> {
    type Item = RegexMatch;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.next_start > self.string.len() {
                return None;
            }
            let input = Input::new(self.string).range(self.next_start..);
            let m = self.regex.engine().find(input)?;
            let result = RegexMatch {
                start: m.start(),
                end: m.end(),
            };
            // An empty match where the one before ended is none, as in gawk: `gsub(/x*/,
            // "-")` of `xab` is `-a-b-`. A match at an empty match's end was found twice,
            // so `gsub(/\</, "|")` doubled each mark after the first.
            if result.start == result.end && self.last_end == Some(result.start) {
                self.next_start = self.after(result.start);
                continue;
            }
            self.last_end = Some(result.end);
            self.next_start = if result.end > result.start {
                result.end
            } else {
                self.after(result.end)
            };
            return Some(result);
        }
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

thread_local! {
    /// The escapes gawk has no meaning for in a regex that it has warned about: once each.
    static WARNED_ESCAPES: std::cell::RefCell<std::collections::HashSet<char>> =
        std::cell::RefCell::default();
    /// gawk's warnings about regexes made since they were last taken (`take_warnings`).
    static WARNINGS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Notes gawk's warning for the regex escape `\c` it has no meaning for, once.
fn unknown_escape(c: char) {
    if WARNED_ESCAPES.with_borrow_mut(|warned| warned.insert(c)) {
        WARNINGS.with_borrow_mut(|warnings| {
            warnings.push(format!(
                "regexp escape sequence `\\{c}' is not a known regexp operator"
            ));
        });
    }
}

/// gawk's warnings about the regexes made since the last call, for the caller to write
/// where the regex is.
pub fn take_warnings() -> Vec<String> {
    WARNINGS.with_borrow_mut(std::mem::take)
}

/// Whether there are warnings to take.
pub fn has_warnings() -> bool {
    WARNINGS.with_borrow(|warnings| !warnings.is_empty())
}

/// gawk's error for `*`, `+`, `?` or an interval with nothing before it to repeat.
const NOTHING_TO_REPEAT: &str = "? * + or {interval} not preceded by valid subpattern";
/// gawk's error for an interval it cannot read or that is out of its range.
const BAD_INTERVAL: &str = "invalid contents of {}";
/// The most an interval may repeat in gawk.
const MOST_REPEATS: u32 = 255;
/// The character classes gawk knows, `[:alpha:]` and the rest.
const CLASS_NAMES: [&str; 12] = [
    "alpha", "digit", "alnum", "upper", "lower", "space", "blank", "punct", "print", "graph",
    "cntrl", "xdigit",
];

/// `pattern`, a POSIX extended regular expression as gawk reads one, in the regex engine's
/// syntax, or gawk's words for what is wrong with it. A `)` with no `(` and a `{` that
/// starts no interval are literal characters, as in gawk; a bracket expression's
/// characters that the engine reads as operators (`[`, `&&`, `--`, `~~`) are made
/// literal. The engine's own error, in its own words, stood for every one of gawk's.
fn translate_ere(pattern: &str) -> Result<String, &'static str> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len());
    let mut at = 0;
    let mut depth = 0_usize;
    // Whether what came last can be repeated: not at the start, nor after `(` or `|`.
    let mut can_repeat = false;
    while let Some(&c) = chars.get(at) {
        at += 1;
        match c {
            '\\' => {
                let next = *chars.get(at).ok_or("invalid trailing backslash")?;
                at += 1;
                match next {
                    // gawk's operators: a word's edge and none, its start and end, and the
                    // start and end of the text.
                    'y' => out.push_str("\\b"),
                    'B' => out.push_str("\\B"),
                    '<' => out.push_str(concat!("\\b", "{start}")),
                    '>' => out.push_str(concat!("\\b", "{end}")),
                    '`' => out.push_str("\\A"),
                    '\'' => out.push_str("\\z"),
                    's' | 'S' | 'w' | 'W' => {
                        out.push('\\');
                        out.push(next);
                    }
                    _ => {
                        let (character, after) = escaped_character(&chars, at - 2);
                        at = after;
                        // An escape gawk has no meaning for is the character, with its
                        // warning: the engine's `\d` was a digit.
                        if character == next
                            && !next.is_ascii_digit()
                            && !".[]()*+?{}|^$\\/-".contains(next)
                        {
                            unknown_escape(next);
                        }
                        push_literal(character, &mut out);
                    }
                }
                can_repeat = true;
            }
            '(' => {
                depth += 1;
                out.push(c);
                can_repeat = false;
            }
            ')' if depth == 0 => {
                out.push_str("\\)");
                can_repeat = true;
            }
            ')' => {
                depth -= 1;
                out.push(c);
                can_repeat = true;
            }
            '|' => {
                out.push(c);
                can_repeat = false;
            }
            '*' | '+' | '?' if !can_repeat => return Err(NOTHING_TO_REPEAT),
            '{' if chars.get(at).is_some_and(char::is_ascii_digit) => {
                let end = interval_end(&chars, at)?;
                if !can_repeat {
                    return Err(NOTHING_TO_REPEAT);
                }
                out.push('{');
                out.extend(chars.get(at..end).unwrap_or_default());
                at = end;
            }
            '{' => {
                out.push_str("\\{");
                can_repeat = true;
            }
            '[' => {
                at = bracket_expression(&chars, at, &mut out)?;
                can_repeat = true;
            }
            _ => {
                out.push(c);
                can_repeat = true;
            }
        }
    }
    if depth > 0 {
        return Err("unbalanced (");
    }
    Ok(out)
}

/// Where the interval whose digits start at `at` ends, past its `}`, checked as gawk
/// checks it: `{n}`, `{n,}` or `{n,m}`, with `n` no more than `m` and both at most 255.
fn interval_end(chars: &[char], mut at: usize) -> Result<usize, &'static str> {
    let number = |at: &mut usize| {
        let start = *at;
        while chars.get(*at).is_some_and(char::is_ascii_digit) {
            *at += 1;
        }
        let digits: String = chars.get(start..*at).unwrap_or_default().iter().collect();
        (!digits.is_empty()).then(|| digits.parse::<u32>().unwrap_or(u32::MAX))
    };
    let least = number(&mut at).unwrap_or_default();
    let mut most = Some(least);
    match chars.get(at) {
        None => return Err("unbalanced {"),
        Some(',') => {
            at += 1;
            if at == chars.len() {
                return Err(BAD_INTERVAL);
            }
            most = number(&mut at);
            if at == chars.len() {
                return Err("unbalanced {");
            }
        }
        Some(_) => {}
    }
    if chars.get(at) != Some(&'}')
        || least > MOST_REPEATS
        || most.is_some_and(|most| most > MOST_REPEATS || most < least)
    {
        return Err(BAD_INTERVAL);
    }
    Ok(at + 1)
}

/// Writes the bracket expression whose `[` is just before `at` to `out`, each character
/// literal for the engine, and returns where it ends, past its `]`.
fn bracket_expression(
    chars: &[char],
    mut at: usize,
    out: &mut String,
) -> Result<usize, &'static str> {
    // A character the engine reads as an operator in a class, escaped.
    let literal = |c: char, out: &mut String| {
        if matches!(c, '\\' | '[' | ']' | '^' | '-' | '&' | '~') {
            out.push('\\');
        }
        out.push(c);
    };
    out.push('[');
    if chars.get(at) == Some(&'^') {
        out.push('^');
        at += 1;
    }
    // The last single character, which a `-` makes the start of a range.
    let mut last = None;
    if chars.get(at) == Some(&']') {
        literal(']', out);
        last = Some(']');
        at += 1;
    }
    loop {
        let c = *chars.get(at).ok_or("unbalanced [")?;
        match c {
            ']' => {
                out.push(']');
                return Ok(at + 1);
            }
            '[' if matches!(chars.get(at + 1), Some(':' | '.' | '=')) => {
                let (inner, end) = bracketed(chars, at)?;
                if chars.get(at + 1) == Some(&':') {
                    let name: String = inner.iter().collect();
                    if !CLASS_NAMES.contains(&name.as_str()) {
                        return Err("invalid character class name");
                    }
                    out.push_str(&format!("[:{name}:]"));
                    last = None;
                } else {
                    let element = single(inner)?;
                    literal(element, out);
                    last = Some(element);
                }
                at = end;
            }
            '-' if last.is_some() && chars.get(at + 1).is_some_and(|&next| next != ']') => {
                let start = last.unwrap_or_default();
                let (end, after) = match chars.get(at + 1..at + 3) {
                    Some(['[', ':']) => return Err("invalid range endpoint"),
                    Some(['[', '.' | '=']) => {
                        let (inner, after) = bracketed(chars, at + 1)?;
                        (single(inner)?, after)
                    }
                    Some(['\\', _]) => escape_in_bracket(chars, at + 1),
                    _ => (chars.get(at + 1).copied().unwrap_or_default(), at + 2),
                };
                if end < start {
                    return Err("invalid range endpoint");
                }
                out.push('-');
                literal(end, out);
                last = None;
                at = after;
            }
            '\\' => {
                if chars.get(at + 1).is_none() {
                    return Err("unbalanced [");
                }
                let (escaped, after) = escape_in_bracket(chars, at);
                literal(escaped, out);
                last = Some(escaped);
                at = after;
            }
            _ => {
                literal(c, out);
                last = Some(c);
                at += 1;
            }
        }
    }
}

/// Writes `c` to `out` as a literal character of the engine's syntax.
fn push_literal(c: char, out: &mut String) {
    if c.is_ascii_alphanumeric() || c == ' ' || !c.is_ascii() {
        out.push(c);
    } else {
        out.push_str(&format!("\\x{{{:x}}}", c as u32));
    }
}

/// The character the escape whose backslash is at `at` stands for, and where the escape
/// ends: `\t` and the like, octal and hexadecimal ones (and `\x{8}` that octal escapes were
/// turned into by `translate_ere_escapes`), else the character after the backslash.
fn escaped_character(chars: &[char], at: usize) -> (char, usize) {
    let escaped = chars.get(at + 1).copied().unwrap_or_default();
    match escaped {
        'b' => return ('\x08', at + 2),
        '0'..='7' => {
            let digits: String = chars
                .get(at + 1..)
                .unwrap_or_default()
                .iter()
                .take(3)
                .take_while(|c| c.is_digit(8))
                .collect();
            let value = u32::from_str_radix(&digits, 8).unwrap_or_default();
            return (
                char::from_u32(value).unwrap_or_default(),
                at + 1 + digits.len(),
            );
        }
        'x' if chars.get(at + 2) != Some(&'{') => {
            let digits: String = chars
                .get(at + 2..)
                .unwrap_or_default()
                .iter()
                .take(2)
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            return match u32::from_str_radix(&digits, 16)
                .ok()
                .and_then(char::from_u32)
            {
                Some(c) => (c, at + 2 + digits.len()),
                None => ('x', at + 2),
            };
        }
        _ => {}
    }
    escape_in_bracket(chars, at)
}

/// The character the escape whose backslash is at `at` in a bracket expression stands
/// for, and where the escape ends: `\t` and the like, `\x{8}` that octal escapes were
/// turned into (`translate_ere_escapes`), else the character after the backslash, as gawk
/// reads one. `[ \t]` was a space or a `t`.
fn escape_in_bracket(chars: &[char], at: usize) -> (char, usize) {
    let escaped = chars.get(at + 1).copied().unwrap_or_default();
    let simple = match escaped {
        'n' => Some('\n'),
        't' => Some('\t'),
        'r' => Some('\r'),
        'f' => Some('\x0c'),
        'v' => Some('\x0b'),
        'a' => Some('\x07'),
        _ => None,
    };
    if let Some(c) = simple {
        return (c, at + 2);
    }
    if escaped.is_digit(8) || escaped == 'b' || (escaped == 'x' && chars.get(at + 2) != Some(&'{'))
    {
        return escaped_character(chars, at);
    }
    if escaped == 'x' && chars.get(at + 2) == Some(&'{') {
        let digits_start = at + 3;
        if let Some(close) = (digits_start..chars.len()).find(|&i| chars.get(i) == Some(&'}')) {
            let digits: String = chars
                .get(digits_start..close)
                .unwrap_or_default()
                .iter()
                .collect();
            if let Some(c) = u32::from_str_radix(&digits, 16)
                .ok()
                .and_then(char::from_u32)
            {
                return (c, close + 1);
            }
        }
    }
    (escaped, at + 2)
}

/// The inside of the `[:name:]`, `[.c.]` or `[=c=]` that starts at `at`, and where it
/// ends; gawk's error when it does not.
fn bracketed(chars: &[char], at: usize) -> Result<(&[char], usize), &'static str> {
    let kind = chars.get(at + 1).copied().unwrap_or_default();
    let error = if kind == ':' {
        "invalid character class name"
    } else {
        "invalid collating element"
    };
    let start = at + 2;
    let close = (start..chars.len())
        .find(|&i| chars.get(i) == Some(&kind) && chars.get(i + 1) == Some(&']'))
        .ok_or(error)?;
    Ok((chars.get(start..close).unwrap_or_default(), close + 2))
}

/// The one character of a collating element, `[.c.]` or `[=c=]`.
fn single(inner: &[char]) -> Result<char, &'static str> {
    match inner {
        [c] => Ok(*c),
        _ => Err("invalid collating element"),
    }
}

impl Regex {
    /// The regex for `pattern`, or gawk's words for what is wrong with it.
    pub fn new(pattern: &str) -> Result<Self, String> {
        let translated = translate_ere(pattern)?;
        let inner = engine(&translated, false)?;
        Ok(Self {
            inner,
            translated,
            folded: std::cell::OnceCell::new(),
            pattern_string: pattern.to_string(),
        })
    }

    /// The engine to match with: the one that ignores case under IGNORECASE.
    fn engine(&self) -> &MetaRegex {
        if IGNORE_CASE.get() {
            self.folded.get_or_init(|| {
                engine(&self.translated, true).unwrap_or_else(|_| self.inner.clone())
            })
        } else {
            &self.inner
        }
    }

    /// The regex for `pattern`, made while the program runs: a dynamic regular expression,
    /// FS or RS. Something wrong with it is gawk's fatal error, `invalid regexp: unbalanced
    /// (: /(/`.
    pub fn dynamic(pattern: &str) -> Result<Self, String> {
        Self::new(pattern).map_err(|error| format!("invalid regexp: {error}: /{pattern}/"))
    }

    /// The first match that is not empty, in bytes that need not be UTF-8 nor end on a
    /// character: where a record ends when this is RS. An empty match ends no record, as
    /// it would end one at every position and never move on.
    pub fn find_separator(&self, bytes: &[u8]) -> Option<RegexMatch> {
        self.engine()
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
            last_end: None,
        }
    }

    /// The first match in `string` and the places of its groups, the whole match first,
    /// `None` for a group that took no part in it: what gawk's `match(s, r, arr)` puts in
    /// `arr`.
    pub fn captures(&self, string: &str) -> Option<Vec<Option<RegexMatch>>> {
        let mut captures = self.engine().create_captures();
        self.engine()
            .search_captures(&Input::new(string), &mut captures);
        if !captures.is_match() {
            return None;
        }
        Some(
            (0..captures.group_len())
                .map(|group| {
                    captures.get_group(group).map(|span| RegexMatch {
                        start: span.start,
                        end: span.end,
                    })
                })
                .collect(),
        )
    }

    /// The places of the groups of the match `whole` that `match_locations` found in
    /// `string`, `None` for a group that took no part in it: what gawk's `gensub` puts
    /// for `\1` to `\9`. The search starts at the match, the text before it still there
    /// for the assertions that look back.
    pub fn groups_of(&self, string: &str, whole: &RegexMatch) -> Vec<Option<RegexMatch>> {
        let mut captures = self.engine().create_captures();
        let input = Input::new(string)
            .range(whole.start..)
            .anchored(Anchored::Yes);
        self.engine().search_captures(&input, &mut captures);
        (0..captures.group_len())
            .map(|group| {
                captures.get_group(group).map(|span| RegexMatch {
                    start: span.start,
                    end: span.end,
                })
            })
            .collect()
    }

    pub fn pattern(&self) -> &str {
        &self.pattern_string
    }

    pub fn matches(&self, string: &str) -> bool {
        self.engine().is_match(string)
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
