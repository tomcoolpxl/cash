//
// Copyright (c) 2026 Cash project contributors.
//
// This file is part of the posixutils-rs project covered under
// the MIT License. For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

//! The errors and warnings found reading a program, worded and placed as gawk words and
//! places them. cash reported them in pest's form, ` --> 1:9` and the rules it expected.

/// How gawk reports an error, which also decides whether it goes on reading after it.
#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `awk: cmd. line:3: error: text`; gawk goes on reading.
    Error,
    /// An `Error` that gawk writes twice: `break` and `continue` outside a loop.
    Twice,
    /// `awk: cmd. line:3: warning: text`, and the program runs.
    Warning,
    /// The source line, and below it a caret under the place and the text: a syntax error
    /// and the like, after which gawk stops reading.
    Caret,
    /// A newline (or the end of the program) where something else belonged: the line it
    /// ends, numbered as the next one, with the caret at its end, as gawk's lexer has
    /// counted the newline by then. gawk stops reading.
    Newline,
    /// `awk: cmd. line:1: text`, with nothing before the text: `BEGIN blocks must have an
    /// action part`. gawk stops reading.
    Bare,
    /// `awk: cmd. line:1: fatal: text`, found once the whole program is read: a function
    /// called and not defined. The status is 2.
    Fatal,
}

impl Kind {
    /// Whether gawk stops reading the program after an error of this kind.
    fn stops(self) -> bool {
        matches!(self, Kind::Caret | Kind::Newline | Kind::Bare)
    }
}

/// An error or warning at byte `offset` of a program's source.
#[cfg_attr(test, derive(Debug))]
#[derive(Clone)]
pub(crate) struct Diagnostic {
    pub(crate) kind: Kind,
    pub(crate) offset: usize,
    /// The text, in bytes: gawk names an invalid character by its first byte alone.
    pub(crate) text: Vec<u8>,
}

impl Diagnostic {
    pub(crate) fn new(kind: Kind, offset: usize, text: impl Into<Vec<u8>>) -> Self {
        Self {
            kind,
            offset,
            text: text.into(),
        }
    }

    /// The diagnostic as gawk writes it, for `source`, whose file is `file` (empty for a
    /// program given as an argument).
    pub(crate) fn render(&self, source: &str, file: &str) -> Vec<u8> {
        let place = if file.is_empty() { "cmd. line" } else { file };
        let offset = self.offset.min(source.len());
        let (number, start) = line_of(source, offset);
        let line_end = source
            .get(offset..)
            .and_then(|rest| rest.find('\n'))
            .map_or(source.len(), |end| offset + end);
        let mut out = Vec::new();
        let mut write_line = |prefix: &str, number: usize, text: &[u8]| {
            out.extend_from_slice(format!("awk: {place}:{number}: {prefix}").as_bytes());
            out.extend_from_slice(text);
            out.push(b'\n');
        };
        match self.kind {
            Kind::Error => write_line("error: ", number, &self.text),
            Kind::Twice => {
                write_line("error: ", number, &self.text);
                write_line("error: ", number, &self.text);
            }
            Kind::Warning => write_line("warning: ", number, &self.text),
            Kind::Fatal => write_line("fatal: ", number, &self.text),
            Kind::Bare => write_line("", number, &self.text),
            Kind::Caret | Kind::Newline => {
                let (number, end) = if self.kind == Kind::Newline {
                    (number + 1, offset)
                } else {
                    (number, line_end)
                };
                let line = source.get(start..end).unwrap_or_default();
                write_line("", number, line.as_bytes());
                // A tab stays a tab, so that the caret lines up under it.
                let mut caret: Vec<u8> = line
                    .bytes()
                    .take(offset - start)
                    .map(|byte| if byte == b'\t' { b'\t' } else { b' ' })
                    .collect();
                caret.extend_from_slice(b"^ ");
                caret.extend_from_slice(&self.text);
                write_line("", number, &caret);
            }
        }
        out
    }
}

/// The 1-based number of the line holding byte `offset`, and where that line starts. A
/// newline belongs to the line it ends.
fn line_of(source: &str, offset: usize) -> (usize, usize) {
    let before = source.get(..offset).unwrap_or(source);
    let number = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |newline| newline + 1);
    (number, start)
}

/// The errors of a program, in its order, as gawk reports them: up to and including the
/// first after which gawk stops reading. A function not defined is reported only when
/// there is nothing else, as gawk looks for those once it has read the whole program.
#[cfg_attr(test, derive(Debug))]
#[derive(Clone)]
pub struct CompilerErrors {
    /// Each diagnostic with the index of the source it is in.
    pub(crate) diagnostics: Vec<(usize, Diagnostic)>,
    text: Vec<u8>,
    status: i32,
}

impl CompilerErrors {
    /// The errors in `diagnostics`, each in the source of its index in `sources` (file
    /// name and text).
    pub(crate) fn new(mut diagnostics: Vec<(usize, Diagnostic)>, sources: &[(&str, &str)]) -> Self {
        diagnostics.sort_by_key(|(source, diagnostic)| (*source, diagnostic.offset));
        let errors_of_reading = diagnostics
            .iter()
            .any(|(_, d)| !matches!(d.kind, Kind::Fatal | Kind::Warning));
        let mut text = Vec::new();
        for (source, diagnostic) in &diagnostics {
            let keep = match diagnostic.kind {
                Kind::Warning => false,
                Kind::Fatal => !errors_of_reading,
                _ => true,
            };
            if !keep {
                continue;
            }
            let (file, source_text) = sources.get(*source).copied().unwrap_or_default();
            text.extend(diagnostic.render(source_text, file));
            if diagnostic.kind.stops() || diagnostic.kind == Kind::Fatal {
                break;
            }
        }
        Self {
            diagnostics,
            text,
            status: if errors_of_reading { 1 } else { 2 },
        }
    }

    /// What awk writes to standard error, as gawk writes it.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// The exit status: 1 for an error reading the program, 2 for a function not defined,
    /// as in gawk.
    pub fn status(&self) -> i32 {
        self.status
    }
}

impl std::fmt::Display for CompilerErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.text))
    }
}

/// The words gawk reserves, which are no operand: a `/` after one starts a regex.
const KEYWORDS: [&str; 22] = [
    "BEGIN", "END", "function", "func", "if", "else", "while", "for", "do", "break", "continue",
    "next", "nextfile", "exit", "return", "delete", "getline", "print", "printf", "in", "case",
    "default",
];

/// The first error gawk's lexer finds in `source`, and where the token it is in starts: a
/// string or a regex that the line ends first, a character awk has no use for, or a
/// backslash with more on its line. Whether a `/` divides or starts a regex is decided as
/// gawk's parser decides it, by what comes before it.
pub(crate) fn lexical_error(source: &str) -> Option<(usize, Diagnostic)> {
    let bytes = source.as_bytes();
    let mut at = 0;
    // Whether the token before is an operand, after which `/` divides.
    let mut operand = false;
    while let Some(&byte) = bytes.get(at) {
        let start = at;
        at += 1;
        match byte {
            b' ' | b'\t' | b'\r' | 0x0b | 0x0c => {}
            b'\n' => operand = false,
            b'#' => {
                while bytes.get(at).is_some_and(|&b| b != b'\n') {
                    at += 1;
                }
            }
            b'\\' => match bytes.get(at..at + 2) {
                Some([b'\n', ..]) => at += 1,
                Some([b'\r', b'\n']) => at += 2,
                None if bytes.get(at) == Some(&b'\n') => at += 1,
                _ => {
                    let text = "backslash not last character on line";
                    return Some((start, Diagnostic::new(Kind::Caret, start, text)));
                }
            },
            b'"' => {
                loop {
                    match bytes.get(at) {
                        None | Some(b'\n') => {
                            let text = "unterminated string";
                            return Some((start, Diagnostic::new(Kind::Caret, start, text)));
                        }
                        Some(b'\\') => at += 2,
                        Some(b'"') => break,
                        Some(_) => at += 1,
                    }
                }
                at += 1;
                operand = true;
            }
            b'/' if !operand => {
                match regex_end(bytes, at) {
                    Some(end) => at = end,
                    None => {
                        let text = "unterminated regexp";
                        return Some((start, Diagnostic::new(Kind::Caret, start + 1, text)));
                    }
                }
                operand = true;
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                while bytes
                    .get(at)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    at += 1;
                }
                let word = source.get(start..at).unwrap_or_default();
                operand = !KEYWORDS.contains(&word);
            }
            b'0'..=b'9' | b'.' => {
                while bytes
                    .get(at)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'.')
                {
                    // An exponent's sign belongs to the number.
                    if matches!(bytes.get(at), Some(b'e' | b'E'))
                        && matches!(bytes.get(at + 1), Some(b'+' | b'-'))
                    {
                        at += 1;
                    }
                    at += 1;
                }
                operand = true;
            }
            b')' | b']' => operand = true,
            b'+' | b'-' if bytes.get(at) == Some(&byte) => at += 1,
            b'`' => {
                let text = "invalid char '`' in expression";
                return Some((start, Diagnostic::new(Kind::Caret, start, text)));
            }
            0x80.. => {
                let mut text = b"invalid char '".to_vec();
                text.push(byte);
                text.extend_from_slice(b"' in expression");
                return Some((start, Diagnostic::new(Kind::Caret, start, text)));
            }
            _ => operand = false,
        }
    }
    None
}

/// Where the regex whose text starts at `at`, after its `/`, ends, past its closing `/`;
/// `None` when the line ends first. A `/` in a bracket expression is part of it, as gawk's
/// lexer reads one.
fn regex_end(bytes: &[u8], mut at: usize) -> Option<usize> {
    let mut in_bracket = false;
    loop {
        match *bytes.get(at)? {
            b'\n' => return None,
            b'\\' => at += 2,
            b'[' if !in_bracket => {
                in_bracket = true;
                at += 1;
                if bytes.get(at) == Some(&b'^') {
                    at += 1;
                }
                if bytes.get(at) == Some(&b']') {
                    at += 1;
                }
            }
            b'[' if matches!(bytes.get(at + 1), Some(b':' | b'.' | b'=')) => {
                let kind = bytes.get(at + 1).copied().unwrap_or_default();
                let close = (at + 2..bytes.len()).find(|&i| {
                    bytes.get(i) == Some(&kind) && bytes.get(i + 1) == Some(&b']')
                        || bytes.get(i) == Some(&b'\n')
                });
                at = match close {
                    Some(close) if bytes.get(close) != Some(&b'\n') => close + 2,
                    _ => at + 1,
                };
            }
            b']' if in_bracket => {
                in_bracket = false;
                at += 1;
            }
            b'/' if !in_bracket => return Some(at + 1),
            _ => at += 1,
        }
    }
}

/// The token gawk's parser stops at, for the place pest's stopped at: pest places an error
/// at the start of the rule that failed, so a keyword that must be followed by `(` (`if
/// x`) and gawk's `@` of an indirect call have the error, where gawk places it at what
/// follows them.
fn gawks_token(source: &str, error_at: usize) -> usize {
    let rest = source.get(error_at..).unwrap_or_default();
    let word: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    // The `while` of `do x while` comes after a statement, and is the error itself.
    let starts_statement = source
        .get(..error_at)
        .unwrap_or_default()
        .trim_end_matches([' ', '\t'])
        .chars()
        .last()
        .is_none_or(|c| matches!(c, '{' | ';' | '\n' | ')'));
    let skip = match word.as_str() {
        "if" | "for" | "switch" => word.len(),
        "while" if starts_statement => word.len(),
        _ if rest.starts_with('@') => 1,
        _ => return error_at,
    };
    let after = rest.get(skip..).unwrap_or_default();
    if word.is_empty() || !after.trim_start_matches([' ', '\t']).starts_with('(') {
        let blanks = after.len() - after.trim_start_matches([' ', '\t']).len();
        error_at + skip + blanks
    } else {
        error_at
    }
}

/// The names of gawk's builtin functions, which a function cannot be named.
pub(crate) const BUILTIN_NAMES: [&str; 23] = [
    "atan2", "cos", "sin", "exp", "log", "sqrt", "int", "rand", "srand", "gsub", "index", "length",
    "match", "split", "sprintf", "sub", "substr", "tolower", "toupper", "close", "fflush",
    "system", "isarray",
];

/// gawk's error for the program `source`, which does not parse: pest's attempt with a
/// newline added at its end (as gawk's lexer adds one) failed at byte `error_at`. It is a
/// lexical error there or before, a `BEGIN` or `END` with no action, a builtin's name for
/// a function, or a syntax error, `unexpected newline or end of string` at the end of a
/// line.
pub(crate) fn syntax_error(source: &str, error_at: usize) -> Diagnostic {
    let error_at = gawks_token(source, error_at);
    if let Some((token, diagnostic)) = lexical_error(source)
        && token <= error_at
    {
        return diagnostic;
    }
    let before = source.get(..error_at.min(source.len())).unwrap_or(source);
    let last_word = before
        .trim_end()
        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .unwrap_or_default();
    let ends_word = before.trim_end().ends_with(last_word) && !last_word.is_empty();
    let word_here: String = source
        .get(error_at..)
        .unwrap_or_default()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let at_newline = error_at >= source.len() || source.as_bytes().get(error_at) == Some(&b'\n');
    if ends_word && at_newline && matches!(last_word, "BEGIN" | "END") {
        // On the line of the newline after it, unless that is the one gawk adds.
        let offset = if error_at < source.len() && at_newline {
            error_at + 1
        } else {
            error_at.min(source.len())
        };
        let text = format!("{last_word} blocks must have an action part");
        return Diagnostic::new(Kind::Bare, offset, text);
    }
    if ends_word
        && matches!(last_word, "function" | "func")
        && BUILTIN_NAMES.contains(&word_here.as_str())
    {
        let text = format!("`{word_here}' is a built-in function, it cannot be redefined");
        return Diagnostic::new(Kind::Caret, error_at, text);
    }
    let unexpected = "unexpected newline or end of string";
    if error_at > source.len() {
        // The end of the program, after the newline gawk adds: the caret is at the end of
        // its last line.
        let end = source.strip_suffix('\n').unwrap_or(source).len();
        return Diagnostic::new(Kind::Caret, end, unexpected);
    }
    if at_newline {
        return Diagnostic::new(Kind::Newline, error_at, unexpected);
    }
    Diagnostic::new(Kind::Caret, error_at, "syntax error")
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failed assumption in a test should abort it loudly"
)]
mod tests {
    use super::*;

    fn rendered(diagnostic: &Diagnostic, source: &str) -> String {
        String::from_utf8_lossy(&diagnostic.render(source, "")).into_owned()
    }

    #[test]
    fn a_newline_is_placed_at_the_end_of_the_line_it_ends() {
        let source = "BEGIN { x = 1 +\n }";
        let diagnostic = syntax_error(source, 15);
        assert_eq!(
            rendered(&diagnostic, source),
            "awk: cmd. line:2: BEGIN { x = 1 +\nawk: cmd. line:2:                ^ unexpected \
             newline or end of string\n"
        );
    }

    #[test]
    fn a_division_is_no_regex() {
        assert!(lexical_error("BEGIN { x = a / 2 / 3 }").is_none());
        assert!(lexical_error("BEGIN { x = (1) /2/ 3; y = a[1] / 2 }").is_none());
        let (_, diagnostic) = lexical_error("BEGIN { x = /abc }").expect("an error");
        assert_eq!(diagnostic.offset, 13);
    }
}
