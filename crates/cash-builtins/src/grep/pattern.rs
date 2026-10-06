//! `grep`'s patterns are `sed`'s: GNU's basic and extended syntaxes as the bundled sed
//! reads them (`cash_sed::sed::gnu_regex` for the checking and GNU's error wording,
//! `cash_sed::sed::compiler::translate_posix` for the translation to the regex crate's
//! syntax), so the two tools take one dialect with the same quirks.
//!
//! What grep adds on top, before sed's reading: GNU grep's warnings (a stray backslash,
//! a repetition at the start of an extended expression), and the places where GNU
//! grep's reading is looser than GNU sed's, which are all in the extended syntax: a
//! repetition at the start applies to nothing, a `)` with no `(` is a character, and a
//! `{` that does not start an interval is one too. After it: `` \` `` and `\'` become
//! the line's anchors rather than the buffer's, a repetition of a repetition (`a+?`,
//! `a{2}?`), which GNU reads as `(a+)?`, is grouped so the regex crate does not read
//! it as its lazy form, the backreferences of a second pattern are renumbered after
//! the first's groups, and a byte that is not UTF-8 matches as that byte. A fixed
//! string (`-F`) is escaped whole and never reaches sed's reading.

use std::fmt::Write as _;

use cash_sed::sed::compiler::translate_posix;
use cash_sed::sed::gnu_regex;

/// Which of GNU grep's three pattern languages a pattern is written in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Syntax {
    /// `-G`: POSIX basic regular expressions with GNU's extensions.
    Basic,
    /// `-E`: POSIX extended regular expressions with GNU's extensions.
    Extended,
    /// `-F`: fixed strings.
    Fixed,
}

/// A pattern in the regex crate's syntax, with what the translation found out.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Translated {
    /// The pattern.
    pub regex: String,
    /// Whether it needs the backtracking engine: a backreference, or the lookarounds
    /// sed makes of GNU's word boundaries.
    pub fancy: bool,
    /// How many capture groups it opens, so the next pattern's backreferences can be
    /// numbered after them.
    pub groups: usize,
    /// GNU's warnings, without their `grep: warning: ` prefix.
    pub warnings: Vec<String>,
}

/// Writes `c` so that the regex crate reads it as itself.
fn push_literal(out: &mut String, c: char) {
    if regex_syntax_meta(c) {
        out.push('\\');
    }
    out.push(c);
}

/// The regex crate's metacharacters.
const fn regex_syntax_meta(c: char) -> bool {
    matches!(
        c,
        '\\' | '.'
            | '+'
            | '*'
            | '?'
            | '('
            | ')'
            | '|'
            | '['
            | ']'
            | '{'
            | '}'
            | '^'
            | '$'
            | '#'
            | '&'
            | '-'
            | '~'
    )
}

/// Writes a byte that is not part of a UTF-8 sequence, matched as that byte.
fn push_byte(out: &mut String, byte: u8) {
    let _ = write!(out, r"(?-u:\x{byte:02X})");
}

/// A fixed string (`-F`), as a regex that matches it and nothing else.
fn escape_fixed(pattern: &[u8]) -> String {
    let mut out = String::with_capacity(pattern.len() * 2);
    for chunk in pattern.utf8_chunks() {
        for c in chunk.valid().chars() {
            push_literal(&mut out, c);
        }
        for &byte in chunk.invalid() {
            push_byte(&mut out, byte);
        }
    }
    out
}

/// The index past the bracket expression whose `[` is at `start`, or `None` when it
/// is not closed. `[]`, `[^]` and the `[:…:]`, `[.…]` and `[=…=]` elements are read as
/// POSIX has them.
fn bracket_end(pattern: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    if pattern.get(i) == Some(&b'^') {
        i += 1;
    }
    if pattern.get(i) == Some(&b']') {
        i += 1;
    }
    while let Some(&c) = pattern.get(i) {
        match c {
            b']' => return Some(i + 1),
            b'[' if matches!(pattern.get(i + 1), Some(b':' | b'.' | b'=')) => {
                let marker = pattern.get(i + 1).copied().unwrap_or(b':');
                let mut j = i + 2;
                loop {
                    match pattern.get(j) {
                        None => return None,
                        Some(&m) if m == marker && pattern.get(j + 1) == Some(&b']') => {
                            i = j + 2;
                            break;
                        }
                        Some(_) => j += 1,
                    }
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// What an extended expression's `{` at `i` (just past the brace) starts, by GNU grep's
/// reading: an interval, or an ordinary character when what follows is not one.
/// Returns the index past the `}` of an interval.
fn ere_interval_end(pattern: &[u8], i: usize) -> Option<usize> {
    // A number ends at `,` or `}`; any other character spoils it, and the end of the
    // pattern makes the brace a character.
    let number = |mut j: usize| -> Option<(bool, usize)> {
        let mut spoiled = false;
        loop {
            match pattern.get(j) {
                None => return None,
                Some(b',' | b'}') => return Some((spoiled, j)),
                Some(c) if c.is_ascii_digit() => {}
                Some(_) => spoiled = true,
            }
            j += 1;
        }
    };
    let (spoiled, j) = number(i)?;
    if spoiled {
        return None;
    }
    if pattern.get(j) == Some(&b'}') {
        return Some(j + 1);
    }
    let (spoiled, k) = number(j + 1)?;
    if spoiled {
        return None;
    }
    // `{1,2,3}` is an error, which sed's reading reports; `{1,2}` an interval.
    Some(k + 1)
}

/// GNU grep's warning for a backslash before a character that is not special.
fn stray(c: u8) -> String {
    let shown = match c {
        b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C => "white space".to_owned(),
        c if c.is_ascii_control() || c >= 0x80 => "unprintable character".to_owned(),
        c => char::from(c).to_string(),
    };
    std::format!("stray \\ before {shown}")
}

/// The state of `prepare` that an escape may change.
struct Escapes<'a> {
    extended: bool,
    out: &'a mut Vec<u8>,
    warnings: &'a mut Vec<String>,
    /// A basic expression's `\{` interval is open, so its `\}` closes it rather than
    /// being a character.
    interval_open: bool,
}

/// Writes the escape `\next` of a pattern (`comma` says whether a `,` follows it);
/// returns whether an expression starts after it. `at_start` says whether one starts
/// at it, where a basic expression's `\{` is a stray backslash and a character.
fn prepare_escape(state: &mut Escapes<'_>, next: u8, comma: bool, at_start: bool) -> bool {
    let extended = state.extended;
    match next {
        b'(' | b'|' if !extended => {}
        b'{' if !extended => {
            if at_start {
                state.warnings.push("stray \\ before {".to_owned());
                state.out.push(b'{');
                return false;
            }
            // `\{,n\}` as sed's reading wants it written.
            state.interval_open = true;
            state.out.extend_from_slice(b"\\{");
            if comma {
                state.out.push(b'0');
            }
            return false;
        }
        b'}' if !extended => {
            if !state.interval_open {
                // A `\}` that closes nothing is a `}`, which sed's reading escapes.
                state.out.push(b'}');
                return false;
            }
            state.interval_open = false;
        }
        b'<' | b'>' | b'b' | b'B' | b'`' | b'\'' => {
            state.out.extend_from_slice(&[b'\\', next]);
            return true;
        }
        b'1'..=b'9' | b'w' | b'W' | b's' | b'S' => {}
        b'.' | b'*' | b'[' | b']' | b'\\' | b'^' | b'$' => {}
        b'(' | b')' | b'|' | b'{' | b'}' | b'+' | b'?' => {}
        other => state.warnings.push(stray(other)),
    }
    state.out.extend_from_slice(&[b'\\', next]);
    !extended && matches!(next, b'(' | b'|')
}

/// Prepares a pattern for sed's reading: GNU grep's warnings, and the few readings
/// GNU grep makes that GNU sed does not. The error is GNU's message.
fn prepare(pattern: &[u8], extended: bool) -> Result<(Vec<u8>, Vec<String>), String> {
    let mut out: Vec<u8> = Vec::with_capacity(pattern.len() + 4);
    let mut warnings = Vec::new();
    // Whether an expression starts here: at the start, after a group opens, after an
    // alternative, after an anchor. A repetition here has nothing to apply to.
    let mut at_start = true;
    // Open groups of an extended expression, for a `)` that closes none.
    let mut depth = 0usize;
    let mut interval_open = false;
    let mut i = 0;
    while let Some(&c) = pattern.get(i) {
        i += 1;
        let mut starts = false;
        match c {
            b'\\' => {
                let Some(&next) = pattern.get(i) else {
                    out.push(b'\\');
                    break;
                };
                i += 1;
                let mut state = Escapes {
                    extended,
                    out: &mut out,
                    warnings: &mut warnings,
                    interval_open,
                };
                let comma = pattern.get(i) == Some(&b',');
                starts = prepare_escape(&mut state, next, comma, at_start);
                interval_open = state.interval_open;
            }
            b'[' => {
                let Some(end) = bracket_end(pattern, i - 1) else {
                    // `[` or `[^` with nothing after: GNU grep's generic message; with
                    // something after, sed's reading says what is unmatched.
                    let content = pattern.get(i..).unwrap_or_default();
                    if content.is_empty() || content == b"^" {
                        return Err("Invalid regular expression".to_owned());
                    }
                    out.extend_from_slice(pattern.get(i - 1..).unwrap_or_default());
                    break;
                };
                out.extend_from_slice(pattern.get(i - 1..end).unwrap_or_default());
                i = end;
            }
            b'(' if extended => {
                depth += 1;
                starts = true;
                out.push(c);
            }
            b')' if extended => {
                if depth == 0 {
                    out.extend_from_slice(b"\\)");
                } else {
                    depth -= 1;
                    out.push(c);
                }
            }
            b'|' if extended => {
                starts = true;
                out.push(c);
            }
            b'^' if extended || at_start => {
                starts = true;
                out.push(c);
            }
            b'*' | b'+' | b'?' if extended && at_start => {
                warnings.push(std::format!("{} at start of expression", char::from(c)));
                starts = true;
            }
            b'{' if extended => match ere_interval_end(pattern, i) {
                None => out.extend_from_slice(b"\\{"),
                Some(end) if at_start => {
                    warnings.push("{...} at start of expression".to_owned());
                    i = end;
                    starts = true;
                }
                Some(end) => {
                    out.push(b'{');
                    if pattern.get(i) == Some(&b',') {
                        out.push(b'0');
                    }
                    out.extend_from_slice(pattern.get(i..end).unwrap_or_default());
                    i = end;
                }
            },
            b'*' if !extended => {
                // `a**`, `a\+*` and `a\?*` are `a*`; GNU sed's reading refuses the
                // second repetition, GNU grep's takes it.
                match out.as_slice() {
                    [.., b'*'] if !at_start => {}
                    [.., b'\\', b'+' | b'?'] if !at_start => {
                        out.truncate(out.len() - 2);
                        out.push(b'*');
                    }
                    _ => out.push(b'*'),
                }
            }
            _ => out.push(c),
        }
        at_start = starts;
    }
    Ok((out, warnings))
}

/// Builds the regex crate's syntax from sed's translation of a pattern.
struct Finisher {
    regex: String,
    fancy: bool,
    groups: usize,
    group_base: usize,
    /// Where the last thing a repetition may apply to begins in `regex`.
    atom: Option<usize>,
    /// Whether that thing is already repeated: a second repetition groups it first.
    repeated: bool,
    /// Open groups: where each began in `regex`.
    open: Vec<usize>,
}

impl Finisher {
    fn atom(&mut self, text: &str) {
        self.atom = Some(self.regex.len());
        self.repeated = false;
        self.regex.push_str(text);
    }

    fn anchor(&mut self, text: &str) {
        self.atom = None;
        self.regex.push_str(text);
    }

    /// A repetition `text`: applied to the last atom, grouped first when the atom is
    /// repeated already, as GNU reads `a+?` as `(a+)?`.
    fn repeat(&mut self, text: &str) {
        if let Some(start) = self.atom
            && self.repeated
        {
            self.regex.insert_str(start, "(?:");
            self.regex.push(')');
        }
        self.repeated = self.atom.is_some();
        self.regex.push_str(text);
    }

    /// A backslash has been read; `next` follows it.
    fn escape(&mut self, next: Option<u8>) {
        match next {
            Some(b'A') => self.anchor("^"),
            Some(b'z') => self.anchor("$"),
            Some(b'b' | b'B' | b'<' | b'>') => {
                let text = std::format!("\\{}", char::from(next.unwrap_or(b'b')));
                self.anchor(&text);
            }
            Some(d @ b'1'..=b'9') => {
                self.fancy = true;
                let number = usize::from(d - b'0') + self.group_base;
                let text = std::format!("\\{number}");
                self.atom(&text);
            }
            Some(next) => {
                let text = std::format!("\\{}", char::from(next));
                self.atom(&text);
            }
            None => self.atom("\\"),
        }
    }

    /// A class starts at `i - 1` in `translated`; copies it whole. Returns the index
    /// past it.
    fn class(&mut self, translated: &[u8], i: usize) -> usize {
        let mut depth = 1;
        let mut j = i;
        while let Some(&c) = translated.get(j) {
            j += 1;
            match c {
                b'\\' => j += 1,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
        let text = String::from_utf8_lossy(translated.get(i - 1..j).unwrap_or_default());
        self.atom(&text);
        j
    }

    /// A UTF-8 character at `i - 1` in `translated`, or a byte that is not UTF-8 as
    /// that byte. Returns the index past it.
    fn character(&mut self, translated: &[u8], i: usize) -> usize {
        let rest = translated.get(i - 1..).unwrap_or_default();
        let (valid, invalid) = rest
            .utf8_chunks()
            .next()
            .map_or(("", &[][..]), |ch| (ch.valid(), ch.invalid()));
        if let Some(first) = valid.chars().next() {
            let mut text = String::new();
            text.push(first);
            self.atom(&text);
            i - 1 + first.len_utf8()
        } else if let Some(&byte) = invalid.first() {
            let mut text = String::new();
            push_byte(&mut text, byte);
            self.atom(&text);
            i
        } else {
            i
        }
    }

    fn run(&mut self, translated: &[u8]) {
        let mut i = 0;
        while let Some(&c) = translated.get(i) {
            i += 1;
            match c {
                b'\\' => {
                    let next = translated.get(i).copied();
                    i += 1;
                    self.escape(next);
                }
                b'[' => i = self.class(translated, i),
                b'(' => {
                    self.open.push(self.regex.len());
                    if translated.get(i) == Some(&b'?') {
                        if matches!(translated.get(i + 1), Some(b'=' | b'!' | b'<')) {
                            self.fancy = true;
                        }
                    } else {
                        self.groups += 1;
                    }
                    self.atom = None;
                    self.regex.push('(');
                }
                b')' => {
                    let start = self.open.pop();
                    self.regex.push(')');
                    self.atom = start;
                    self.repeated = false;
                }
                b'|' => self.anchor("|"),
                b'^' | b'$' => self.anchor(&char::from(c).to_string()),
                b'*' | b'+' | b'?' => self.repeat(&char::from(c).to_string()),
                b'{' => {
                    let end = translated
                        .get(i..)
                        .and_then(|rest| rest.iter().position(|&b| b == b'}'))
                        .map_or(translated.len(), |p| i + p + 1);
                    let text =
                        String::from_utf8_lossy(translated.get(i - 1..end).unwrap_or_default())
                            .into_owned();
                    self.repeat(&text);
                    i = end;
                }
                c if c.is_ascii() => self.atom(&char::from(c).to_string()),
                _ => i = self.character(translated, i),
            }
        }
    }
}

/// The regex crate's syntax from sed's translation, with grep's reading of the anchors
/// of the whole, of a repeated repetition, of the group numbering of a later pattern,
/// and of bytes that are not UTF-8.
fn finish(translated: &[u8], group_base: usize) -> Translated {
    let mut finisher = Finisher {
        regex: String::with_capacity(translated.len() + 8),
        fancy: false,
        groups: 0,
        group_base,
        atom: None,
        repeated: false,
        open: Vec::new(),
    };
    finisher.run(translated);
    Translated {
        regex: finisher.regex,
        fancy: finisher.fancy,
        groups: finisher.groups,
        warnings: Vec::new(),
    }
}

/// Translates one of GNU grep's patterns. `group_base` is how many capture groups the
/// patterns before it opened, so its backreferences name the right ones once all are
/// joined into one alternation. The error is GNU's message, without its `grep: `.
pub(super) fn translate(
    pattern: &[u8],
    syntax: Syntax,
    group_base: usize,
) -> Result<Translated, String> {
    if syntax == Syntax::Fixed {
        return Ok(Translated {
            regex: escape_fixed(pattern),
            ..Translated::default()
        });
    }
    let extended = syntax == Syntax::Extended;
    let (prepared, warnings) = prepare(pattern, extended)?;
    let sed_syntax = gnu_regex::Syntax {
        extended,
        posix: false,
        utf8: true,
    };
    if let Some(error) = gnu_regex::syntax_error(&prepared, sed_syntax) {
        return Err(error.to_owned());
    }
    let mut translated = finish(&translate_posix(&prepared, sed_syntax), group_base);
    translated.warnings = warnings;
    Ok(translated)
}

#[cfg(test)]
mod tests {
    use super::{Syntax, Translated, translate};

    fn bre(pattern: &str) -> String {
        translate(pattern.as_bytes(), Syntax::Basic, 0)
            .map_or_else(|e| std::format!("ERR {e}"), |t| t.regex)
    }

    fn ere(pattern: &str) -> String {
        translate(pattern.as_bytes(), Syntax::Extended, 0)
            .map_or_else(|e| std::format!("ERR {e}"), |t| t.regex)
    }

    fn full(pattern: &str, syntax: Syntax) -> Translated {
        translate(pattern.as_bytes(), syntax, 0).unwrap_or_default()
    }

    #[test]
    fn the_same_pattern_translates_as_it_does_for_sed() {
        let sed_syntax = |extended| cash_sed::sed::gnu_regex::Syntax {
            extended,
            posix: false,
            utf8: true,
        };
        for (pattern, extended) in [
            (r"a\(b\)c", false),
            (r"^a.*b$", false),
            (r"a$c$", false),
            (r"^a^[^x]c", false),
            (r"[[:alpha:]][a-z][^0-9][]a-]", false),
            (r"\w\+\s*\<x\>", false),
            (r"*a\|b\{1,2\}", false),
            (r"\(.\)\1", false),
            (r"a+b?c{1}|(d)", false),
            (r"(a|b)+[[:digit:]]{2,}", true),
            (r"a\{1,2\}\.\*", true),
        ] {
            let through_sed =
                cash_sed::sed::compiler::translate_posix(pattern.as_bytes(), sed_syntax(extended));
            let syntax = if extended {
                Syntax::Extended
            } else {
                Syntax::Basic
            };
            assert_eq!(
                translate(pattern.as_bytes(), syntax, 0)
                    .unwrap_or_default()
                    .regex,
                String::from_utf8_lossy(&through_sed),
                "{pattern}"
            );
        }
    }

    #[test]
    fn bre_operators_become_the_plain_ones() {
        assert_eq!(bre(r"\(ab\)\{2\}"), "(ab){2}");
        assert_eq!(bre(r"ab\{1,2\}"), "ab{1,2}");
        assert_eq!(bre(r"ab\{,2\}"), "ab{0,2}");
        assert_eq!(bre(r"ab\?\+"), "a(?:b?)+");
        assert_eq!(bre(r"a\|b"), "a|b");
        assert_eq!(bre("(a){1}|?+"), r"\(a\)\{1\}\|\?\+");
        assert_eq!(bre("*a"), r"\*a");
        assert_eq!(bre("^*a"), r"^\*a");
        assert_eq!(bre(r"\(*a\)"), r"(\*a)");
        assert_eq!(bre(r"a\|*"), r"a|\*");
        assert_eq!(bre("a**"), "a*");
        assert_eq!(bre(r"a\+*"), "a*");
        assert_eq!(bre("a^b"), r"a\^b");
        assert_eq!(bre("a$b"), r"a\$b");
        assert_eq!(bre(r"\(^a\)b$"), "(^a)b$");
        assert_eq!(bre(r"\`foo\'"), "^foo$");
        assert_eq!(bre(r"a\}"), r"a\}");
    }

    #[test]
    fn ere_readings_that_sed_lacks() {
        let t = full("*a", Syntax::Extended);
        assert_eq!(t.regex, "a");
        assert_eq!(t.warnings, vec!["* at start of expression"]);
        let t = full("{1}", Syntax::Extended);
        assert_eq!(t.regex, "");
        assert_eq!(t.warnings, vec!["{...} at start of expression"]);
        assert_eq!(full("^*", Syntax::Extended).regex, "^");
        assert_eq!(full("(*a)", Syntax::Extended).regex, "(a)");
        assert_eq!(full("a|+b", Syntax::Extended).regex, "a|b");
        assert_eq!(ere(")"), r"\)");
        assert_eq!(ere("a)b"), r"a\)b");
        assert_eq!(ere("a{x}"), r"a\{x}");
        assert_eq!(ere("a{1x}"), r"a\{1x}");
        assert_eq!(ere("a{1,2x}"), r"a\{1,2x}");
        assert_eq!(ere("a{1"), r"a\{1");
        assert_eq!(ere("a{1,"), r"a\{1,");
        assert_eq!(ere("a{ 1}"), r"a\{ 1}");
        assert_eq!(ere("a{,}"), "a{0,}");
        assert_eq!(ere("a{,2}"), "a{0,2}");
        assert_eq!(ere("a{}"), "ERR Invalid content of \\{\\}");
        assert_eq!(ere("a{1,2,3}"), "ERR Invalid content of \\{\\}");
        assert_eq!(ere("a{2,1}"), "ERR Invalid content of \\{\\}");
        assert_eq!(ere("("), "ERR Unmatched ( or \\(");
        assert_eq!(ere(r"\{\}\(\)\|\+\?"), r"\{\}\(\)\|\+\?");
    }

    #[test]
    fn a_repeated_repetition_is_grouped_rather_than_lazy() {
        assert_eq!(ere("a+?"), "(?:a+)?");
        assert_eq!(ere("a*?b"), "(?:a*)?b");
        assert_eq!(ere("a{2}?"), "(?:a{2})?");
        assert_eq!(ere("a**"), "(?:a*)*");
        assert_eq!(ere("(ab)+?"), "(?:(ab)+)?");
        assert_eq!(ere("[ab]{1,2}*"), "(?:[ab]{1,2})*");
        assert_eq!(ere("x{1}{2}"), "(?:x{1}){2}");
        assert_eq!(ere("a+b?"), "a+b?");
    }

    #[test]
    fn errors_as_gnu_words_them() {
        assert_eq!(bre(r"\("), "ERR Unmatched ( or \\(");
        assert_eq!(bre(r"\)"), "ERR Unmatched ) or \\)");
        assert_eq!(bre(r"a\{1"), "ERR Unmatched \\{");
        assert_eq!(bre(r"a\{2,1\}"), "ERR Invalid content of \\{\\}");
        assert_eq!(bre(r"a\{99999\}"), "ERR Regular expression too big");
        assert_eq!(bre(r"\1"), "ERR Invalid back reference");
        assert_eq!(bre(r"\(a\)\2"), "ERR Invalid back reference");
        assert_eq!(bre("a\\"), "ERR Trailing backslash");
        assert_eq!(bre("[a"), "ERR Unmatched [, [^, [:, [., or [=");
        assert_eq!(bre("["), "ERR Invalid regular expression");
        assert_eq!(bre("a[^"), "ERR Invalid regular expression");
        assert_eq!(bre("[[:foo:]]"), "ERR Invalid character class name");
        assert_eq!(bre("[b-a]"), "ERR Invalid range end");
        // Sed's reading refuses a repetition of an interval; GNU grep takes `a\{1\}*`.
        assert_eq!(bre(r"a\{1\}*"), "ERR Invalid preceding regular expression");
        let t = full(r"\{1\}", Syntax::Basic);
        assert_eq!(t.regex, r"\{1\}");
        assert_eq!(t.warnings, vec!["stray \\ before {"]);
    }

    #[test]
    fn backreferences_and_word_boundaries_need_the_backtracker() {
        let t = full(r"\(a\)\1", Syntax::Basic);
        assert_eq!(t.regex, r"(a)(?:\1)");
        assert!(t.fancy);
        assert_eq!(t.groups, 1);
        let t = translate(b"(b)\\1", Syntax::Extended, 3).unwrap_or_default();
        assert_eq!(t.regex, r"(b)(?:\4)");
        assert_eq!(t.groups, 1);
        let t = full(r"\<a", Syntax::Basic);
        assert!(t.fancy);
        assert!(t.regex.starts_with("(?<!"));
        assert!(!full("a(b)", Syntax::Extended).fancy);
        assert_eq!(full("(a)(b)", Syntax::Extended).groups, 2);
        // `(?:c)` is no non-capturing group to GNU grep: its `?` applies to nothing.
        let t = full("(a)(?:c)", Syntax::Extended);
        assert_eq!((t.groups, t.regex.as_str()), (2, "(a)(:c)"));
    }

    #[test]
    fn stray_backslashes_warn_and_mean_the_character() {
        let t = full(r"a\/b", Syntax::Basic);
        assert_eq!(t.regex, "a/b");
        assert_eq!(t.warnings, vec!["stray \\ before /"]);
        let t = full(r"\ ", Syntax::Basic);
        assert_eq!(t.regex, " ");
        assert_eq!(t.warnings, vec!["stray \\ before white space"]);
        let none: Vec<String> = Vec::new();
        assert_eq!(full(r"\w\W\s\S\.\*\[\]", Syntax::Basic).warnings, none);
        assert_eq!(full(r"\<", Syntax::Extended).warnings, none);
    }

    #[test]
    fn fixed_strings_and_bytes() {
        assert_eq!(full("a.b[c]\\d*", Syntax::Fixed).regex, r"a\.b\[c\]\\d\*");
        assert_eq!(full("", Syntax::Fixed).regex, "");
        assert_eq!(
            translate(b"a\xffb", Syntax::Fixed, 0)
                .unwrap_or_default()
                .regex,
            r"a(?-u:\xFF)b"
        );
        assert_eq!(
            translate(b"a\xffb", Syntax::Basic, 0)
                .unwrap_or_default()
                .regex,
            r"a(?-u:\xFF)b"
        );
        assert_eq!(bre("é[é]"), "é[é]");
        assert_eq!(bre("[[:alpha:]]*"), r"[\p{Alphabetic}]*");
    }
}
