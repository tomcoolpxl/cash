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

use std::borrow::Cow;

/// How gawk reports an error, which also decides whether it goes on reading after it.
#[cfg_attr(test, derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `awk: cmd. line:3: error: text`; gawk goes on reading.
    Error,
    /// An `Error` that gawk writes twice: `break` and `continue` outside a loop.
    Twice,
    /// An `Error` after which gawk reports nothing more: a function defined twice.
    ErrorThenStop,
    /// `awk: cmd. line:3: warning: text`, met reading the program: written in its place
    /// among the errors.
    Warning,
    /// A `Warning` gawk gives once the whole program is read, written only when there is no
    /// error: a function called with more arguments than it has.
    LateWarning,
    /// The source line, and below it a caret under the place and the text: a syntax error
    /// and the like, after which gawk stops reading.
    Caret,
    /// A newline (or the end of the program) where something else belonged: the line it
    /// ends, numbered as the next one, with the caret at its end, as gawk's lexer has
    /// counted the newline by then. gawk stops reading.
    Newline,
    /// `(END OF FILE)` in place of the source line, and the caret under the place in its
    /// line: a program file whose rule or function is not complete when it ends, or one
    /// that ends where gawk's lexer has read past a newline (`&&` at the end). gawk stops
    /// reading.
    EndOfFile,
    /// `awk: cmd. line:1: text`, with nothing before the text: `BEGIN blocks must have an
    /// action part`. gawk stops reading.
    Bare,
    /// `awk: cmd. line:1: fatal: text`, found once the whole program is read: a function
    /// called and not defined. The status is 2.
    Fatal,
    /// `awk: cmd. line:1: fatal: text`, met reading: a control character in the source.
    /// gawk stops there, with status 2.
    ReadingFatal,
}

impl Kind {
    /// Whether this is a warning, with which the program runs.
    pub(crate) fn is_warning(self) -> bool {
        matches!(self, Kind::Warning | Kind::LateWarning)
    }

    /// Whether gawk stops reading the program after an error of this kind.
    fn stops(self) -> bool {
        matches!(
            self,
            Kind::Caret
                | Kind::Newline
                | Kind::EndOfFile
                | Kind::Bare
                | Kind::ReadingFatal
                | Kind::ErrorThenStop
        )
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
    /// The line gawk names, when its lexer has counted past the one `offset` is in: a
    /// string that a backslash and a newline continue.
    pub(crate) line: Option<usize>,
}

impl Diagnostic {
    pub(crate) fn new(kind: Kind, offset: usize, text: impl Into<Vec<u8>>) -> Self {
        Self {
            kind,
            offset,
            text: text.into(),
            line: None,
        }
    }

    /// The diagnostic as gawk writes it, for `source`, whose file is `file` (empty for a
    /// program given as an argument).
    pub(crate) fn render(&self, source: &str, file: &str) -> Vec<u8> {
        let place = if file.is_empty() { "cmd. line" } else { file };
        let offset = self.offset.min(source.len());
        let (offset_line, start) = line_of(source, offset);
        let number = self.line.unwrap_or(offset_line);
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
            Kind::Error | Kind::ErrorThenStop => write_line("error: ", number, &self.text),
            Kind::Twice => {
                write_line("error: ", number, &self.text);
                write_line("error: ", number, &self.text);
            }
            Kind::Warning | Kind::LateWarning => write_line("warning: ", number, &self.text),
            Kind::Fatal | Kind::ReadingFatal => write_line("fatal: ", number, &self.text),
            Kind::Bare => write_line("", number, &self.text),
            Kind::Caret | Kind::Newline | Kind::EndOfFile => {
                let (number, end) = if self.kind == Kind::Newline {
                    (number + 1, offset)
                } else {
                    (number, line_end)
                };
                let line = source.get(start..end).unwrap_or_default();
                if self.kind == Kind::EndOfFile {
                    write_line("", number, b"(END OF FILE)");
                } else {
                    write_line("", number, line.as_bytes());
                }
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
        // An `(END OF FILE)` error comes after all, its caret where it may be.
        diagnostics.sort_by_key(|(source, diagnostic)| {
            let offset = if diagnostic.kind == Kind::EndOfFile {
                usize::MAX
            } else {
                diagnostic.offset
            };
            (*source, offset)
        });
        let errors_of_reading = diagnostics
            .iter()
            .any(|(_, d)| !d.kind.is_warning() && d.kind != Kind::Fatal);
        let mut text = Vec::new();
        let mut status = if errors_of_reading { 1 } else { 2 };
        for (source, diagnostic) in &diagnostics {
            let keep = match diagnostic.kind {
                Kind::LateWarning => false,
                Kind::Fatal => !errors_of_reading,
                _ => true,
            };
            if !keep {
                continue;
            }
            let (file, source_text) = sources.get(*source).copied().unwrap_or_default();
            text.extend(diagnostic.render(source_text, file));
            if diagnostic.kind == Kind::ReadingFatal {
                status = 2;
            }
            if diagnostic.kind.stops() || diagnostic.kind == Kind::Fatal {
                break;
            }
        }
        Self {
            diagnostics,
            text,
            status,
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

/// The operators of two or three characters, longest first, read as one token.
const OPERATORS: [&str; 18] = [
    "**=", "&&", "||", "++", "--", "**", "<=", ">=", "==", "!=", "!~", "+=", "-=", "*=", "%=",
    "^=", ">>", "/=",
];

/// A token of gawk's lexer, where the errors at the end of a file place their caret.
#[derive(Clone, Copy)]
struct Token {
    /// Where gawk's lexer has the token start: a regex's is its text, after the `/`.
    start: usize,
    end: usize,
    kind: TokenKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Newline,
    /// A comment, to the end of its line.
    Comment,
    Other,
}

/// What gawk's lexer finds in a program: its first error, and the tokens before it.
struct Scan {
    error: Option<(usize, Diagnostic)>,
    tokens: Vec<Token>,
}

/// gawk's lexer over `source`: its tokens, to the first error it finds, and that error with
/// where the token it is in starts: a string or a regex that the line or the program ends
/// first, a character awk has no use for, or a backslash with more on its line. A `/`
/// after an operand divides, and starts a regex elsewhere; `/=` is an assignment only
/// after a variable that starts an expression (`x /= 2`, `a < x /= 2`), and starts a regex
/// after any other operand (`(x) /=2/`, `1 + x /=2/`), as gawk's grammar has it.
fn scan(source: &str) -> Scan {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    // Whether the token before is an operand, after which `/` divides.
    let mut operand = false;
    // Whether that operand is a variable that starts an expression, which `/=` assigns to.
    let mut assignable = false;
    // Whether an operand that starts here starts an expression.
    let mut starts_expression = true;
    // After `$`: whether the field's operand starts an expression.
    let mut field: Option<bool> = None;
    // For each `(` and `[` open: whether what it closes is a variable that starts an
    // expression, `$(1)` or `a[1]`.
    let mut groups: Vec<bool> = Vec::new();
    let error = loop {
        let Some(&byte) = bytes.get(at) else {
            break None;
        };
        let start = at;
        at += 1;
        let mut token = TokenKind::Other;
        match byte {
            b' ' | b'\t' | b'\r' => continue,
            b'\n' => {
                token = TokenKind::Newline;
                operand = false;
                assignable = false;
                starts_expression = true;
            }
            b'#' => {
                while bytes.get(at).is_some_and(|&b| b != b'\n') {
                    at += 1;
                }
                token = TokenKind::Comment;
            }
            b'\\' => match bytes.get(at..at + 2) {
                Some([b'\n', ..]) => {
                    at += 1;
                    continue;
                }
                Some([b'\r', b'\n']) => {
                    at += 2;
                    continue;
                }
                None if bytes.get(at) == Some(&b'\n') => {
                    at += 1;
                    continue;
                }
                _ => {
                    let text = "backslash not last character on line";
                    break Some((start, Diagnostic::new(Kind::Caret, start, text)));
                }
            },
            b'"' => {
                // gawk counts the lines a backslash and a newline join.
                let mut joined = 0;
                let closed = loop {
                    match bytes.get(at) {
                        None | Some(b'\n') => break false,
                        Some(b'\\') => {
                            if bytes.get(at + 1) == Some(&b'\n') {
                                joined += 1;
                            }
                            at += 2;
                        }
                        Some(b'"') => break true,
                        Some(_) => at += 1,
                    }
                };
                if !closed {
                    let mut diagnostic = Diagnostic::new(Kind::Caret, start, "unterminated string");
                    diagnostic.line = (joined > 0).then(|| line_of(source, start).0 + joined);
                    break Some((start, diagnostic));
                }
                at += 1;
                (operand, assignable, starts_expression) = (true, false, false);
                field = None;
            }
            b'/' if !operand || bytes.get(at) == Some(&b'=') && !assignable => {
                match regex_end(bytes, at) {
                    Ok(end) => at = end,
                    Err(open) => {
                        let text = match (open.at_end, open.backslash) {
                            (true, true) => "unterminated regexp ends with `\\' at end of file",
                            (true, false) => "unterminated regexp at end of file",
                            _ => "unterminated regexp",
                        };
                        let mut diagnostic = Diagnostic::new(Kind::Caret, start + 1, text);
                        diagnostic.line =
                            (open.joined > 0).then(|| line_of(source, start).0 + open.joined);
                        break Some((start, diagnostic));
                    }
                }
                tokens.push(Token {
                    start: start + 1,
                    end: at,
                    kind: TokenKind::Other,
                });
                (operand, assignable, starts_expression) = (true, false, false);
                field = None;
                continue;
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                while bytes
                    .get(at)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    at += 1;
                }
                let word = source.get(start..at).unwrap_or_default();
                let starts = field.take().unwrap_or(starts_expression);
                if KEYWORDS.contains(&word) {
                    (operand, assignable, starts_expression) = (false, false, true);
                } else {
                    operand = true;
                    // A builtin's value is no variable: `length /=1/` is a regex.
                    assignable = starts && !BUILTIN_NAMES.contains(&word);
                    starts_expression = false;
                }
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
                // `$1` is a variable; a number is not.
                assignable = field.take().unwrap_or(false);
                operand = true;
                starts_expression = false;
            }
            b'$' => {
                field = Some(field.unwrap_or(starts_expression));
                (operand, assignable, starts_expression) = (false, false, false);
            }
            b'(' | b'[' => {
                // `a[` and `$(` open a variable; any other `(` a group or a call.
                groups.push(if byte == b'[' {
                    assignable
                } else {
                    field.take().unwrap_or(false)
                });
                (operand, assignable, starts_expression) = (false, false, true);
            }
            b')' | b']' => {
                assignable = groups.pop().unwrap_or(false);
                operand = true;
                starts_expression = false;
            }
            b'`' => {
                let text = "invalid char '`' in expression";
                break Some((start, Diagnostic::new(Kind::Caret, start, text)));
            }
            // gawk names a vertical tab or a form feed as it names any character it has
            // no use for, and stops at any other control character, fatally.
            0x0b | 0x0c => {
                let mut text = b"invalid char '".to_vec();
                text.push(byte);
                text.extend_from_slice(b"' in expression");
                break Some((start, Diagnostic::new(Kind::Caret, start, text)));
            }
            0x00..=0x1f | 0x7f => {
                let text = format!("error: invalid character '\\{byte:03o}' in source code");
                break Some((start, Diagnostic::new(Kind::ReadingFatal, start, text)));
            }
            0x80.. => {
                let mut text = b"invalid char '".to_vec();
                text.push(byte);
                text.extend_from_slice(b"' in expression");
                break Some((start, Diagnostic::new(Kind::Caret, start, text)));
            }
            _ => {
                let rest = source.get(start..).unwrap_or_default();
                let operator = OPERATORS
                    .iter()
                    .find(|operator| rest.starts_with(**operator))
                    .map_or(1, |operator| operator.len());
                at = start + operator;
                match source.get(start..at).unwrap_or_default() {
                    // After an operand, `x++` is still one; before one, `++x` is no
                    // variable.
                    "++" | "--" if operand => assignable = false,
                    // The operands of arithmetic start no expression: `1 + x /=2/`.
                    "++" | "--" | "+" | "-" | "*" | "/" | "%" | "^" | "**" | "!" => {
                        (operand, assignable, starts_expression) = (false, false, false);
                    }
                    _ => {
                        (operand, assignable, starts_expression) = (false, false, true);
                        field = None;
                    }
                }
            }
        }
        tokens.push(Token {
            start,
            end: at,
            kind: token,
        });
    };
    Scan { error, tokens }
}

/// How a regex that is not closed ends: at a newline, or at the end of the program, there
/// after a backslash (`backslash`), having joined `joined` lines with a backslash and a
/// newline.
struct OpenRegex {
    at_end: bool,
    backslash: bool,
    joined: usize,
}

/// Where the regex whose text starts at `at`, after its `/`, ends, past its closing `/`;
/// how it ends when the line or the program ends first. A `/` in a bracket expression is
/// part of it, as gawk's lexer reads one, and a backslash and a newline join two lines.
fn regex_end(bytes: &[u8], mut at: usize) -> Result<usize, OpenRegex> {
    let mut in_bracket = false;
    let mut joined = 0;
    loop {
        let Some(&byte) = bytes.get(at) else {
            return Err(OpenRegex {
                at_end: true,
                backslash: false,
                joined,
            });
        };
        match byte {
            b'\n' => {
                return Err(OpenRegex {
                    at_end: false,
                    backslash: false,
                    joined,
                });
            }
            b'\\' => {
                match bytes.get(at + 1) {
                    None => {
                        return Err(OpenRegex {
                            at_end: true,
                            backslash: true,
                            joined,
                        });
                    }
                    Some(b'\n') => joined += 1,
                    Some(_) => {}
                }
                at += 2;
            }
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
            b'/' if !in_bracket => return Ok(at + 1),
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
    // pest takes the `+` of `(x) += 2` for an addition and fails at the `=`, where gawk's
    // lexer has one assignment token, its error at the token's start.
    if rest.starts_with('=') {
        let before = source.get(..error_at).unwrap_or_default();
        let operator = if before.ends_with("**") && !before.ends_with("***") {
            2
        } else if before.ends_with(['+', '-', '*', '/', '%', '^'])
            && !before.ends_with("++")
            && !before.ends_with("--")
        {
            1
        } else {
            0
        };
        return error_at - operator;
    }
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
pub(crate) const BUILTIN_NAMES: [&str; 26] = [
    "atan2", "cos", "sin", "exp", "log", "sqrt", "int", "rand", "srand", "gsub", "index", "length",
    "match", "split", "sprintf", "sub", "substr", "tolower", "toupper", "close", "fflush",
    "system", "isarray", "asort", "asorti", "gensub",
];

/// gawk's error for the program `source`, which does not parse: pest's attempt with a
/// newline added at its end failed at byte `error_at`. It is a lexical error there or
/// before, a `BEGIN` or `END` with no action, a builtin's name for a function, or a syntax
/// error, `unexpected newline or end of string` at the end of a line. gawk's lexer adds
/// the newline to a program given as an argument; a program file (`from_file`) ends where
/// it ends, and a rule it leaves incomplete is gawk's `(END OF FILE)` error.
pub(crate) fn syntax_error(source: &str, error_at: usize, from_file: bool) -> Diagnostic {
    let error_at = gawks_token(source, error_at);
    // The text gawk's lexer reads.
    let read = if from_file || source.ends_with('\n') {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(format!("{source}\n"))
    };
    let scanned = scan(&read);
    if let Some((token, diagnostic)) = scanned.error
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
        // On the line of the newline after it, unless that is the one gawk adds, or the
        // last of a file.
        let last = from_file && error_at + 1 == source.len();
        let offset = if error_at < source.len() && at_newline && !last {
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
    if let Some(diagnostic) = read_past_end(&read, &scanned.tokens, error_at) {
        return diagnostic;
    }
    if error_at >= source.len()
        && let Some(diagnostic) = end_of_file(source, &read, &scanned.tokens, error_at, from_file)
    {
        return diagnostic;
    }
    let unexpected = "unexpected newline or end of string";
    if error_at > source.len() {
        // The end of the program, after the newline gawk adds: the caret is at the end of
        // its last line, one gawk numbers as the next when a backslash continues it.
        let end = source.strip_suffix('\n').unwrap_or(source).len();
        let mut diagnostic = Diagnostic::new(Kind::Caret, end, unexpected);
        if read.ends_with("\\\n") {
            diagnostic.line = Some(line_of(source, end).0 + 1);
        }
        return diagnostic;
    }
    if at_newline {
        // A comment's newline is the comment to gawk's lexer: the caret is at its `#`.
        if let Some(comment) = scanned
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::Comment && token.end == error_at)
        {
            let mut diagnostic = Diagnostic::new(Kind::Caret, comment.start, "syntax error");
            diagnostic.line = Some(line_of(source, comment.start).0 + 1);
            return diagnostic;
        }
        return Diagnostic::new(Kind::Newline, error_at, unexpected);
    }
    Diagnostic::new(Kind::Caret, error_at, "syntax error")
}

/// gawk's error that a program file is not complete.
const INCOMPLETE: &str =
    "source files / command-line arguments must contain complete functions or rules";

/// gawk's error for a program, read as `read` (with the newline gawk adds to an argument)
/// into `tokens`, that ends in `&&`, `||`, `?` or `:` and fails after it (at `error_at`):
/// gawk's lexer reads past the newlines after them, so the program ends there, in a file
/// or not, and gawk names the line two past its newlines. A backslash and a newline are
/// a newline where none belongs, on that line.
fn read_past_end(read: &str, tokens: &[Token], error_at: usize) -> Option<Diagnostic> {
    let last = tokens
        .iter()
        .rev()
        .find(|token| token.kind == TokenKind::Other)?;
    if error_at < last.end
        || !matches!(
            read.get(last.start..last.end),
            Some("&&" | "||" | "?" | ":")
        )
    {
        return None;
    }
    let line = read.matches('\n').count() + 2;
    let after = read.get(last.end..).unwrap_or_default();
    let mut diagnostic = match after.find("\\\n") {
        Some(backslash) => Diagnostic::new(
            Kind::Caret,
            last.end + backslash + 1,
            "unexpected newline or end of string",
        ),
        None => Diagnostic::new(Kind::EndOfFile, last.start, INCOMPLETE),
    };
    diagnostic.line = Some(line);
    Some(diagnostic)
}

/// gawk's `(END OF FILE)` error, for `source` failing at its end (`error_at`), read as
/// `read` into `tokens`; `None` when gawk's error there is another.
///
/// Only a file, or a program whose last line is a comment, ends in gawk's lexer: with a
/// newline last, the line that newline ends is named, with the caret at its comment or
/// before it all; without, the caret is at the last token (at the last blank, after one),
/// on its line or, when a newline was due there, the next.
fn end_of_file(
    source: &str,
    read: &str,
    tokens: &[Token],
    error_at: usize,
    from_file: bool,
) -> Option<Diagnostic> {
    let message = INCOMPLETE;
    let newlines = read.matches('\n').count();
    let last_line_start = |text: &str| text.rfind('\n').map_or(0, |newline| newline + 1);
    // A newline last that no backslash joins to a next line.
    if read.ends_with('\n') && !read.ends_with("\\\n") {
        let body = read.strip_suffix('\n').unwrap_or(read);
        let line_start = last_line_start(body);
        let comment = tokens
            .iter()
            .find(|token| token.kind == TokenKind::Comment && token.start >= line_start);
        // On the command line, a newline due where gawk's is is the comment's own.
        if !from_file && (comment.is_none() || error_at == source.len()) {
            return None;
        }
        let offset = comment.map_or(line_start, |comment| comment.start);
        let mut diagnostic = Diagnostic::new(Kind::EndOfFile, offset, message);
        diagnostic.line = Some(newlines);
        return Some(diagnostic);
    }
    if !from_file || read.ends_with('\n') {
        return None;
    }
    let line_start = last_line_start(source);
    let last_line = line_of(source, line_start).0;
    // A newline was due where the file ends, so gawk has counted one more line.
    let line = if error_at == source.len() {
        last_line + 1
    } else {
        last_line
    };
    let comment = tokens
        .iter()
        .find(|token| token.kind == TokenKind::Comment && token.start >= line_start);
    let (offset, line) = if let Some(comment) = comment {
        // A comment alone on the last line is numbered as the line before it.
        let alone = source
            .get(line_start..comment.start)
            .is_some_and(|before| before.trim().is_empty());
        let line = if alone && error_at > source.len() {
            newlines.max(1)
        } else {
            line
        };
        (comment.start, line)
    } else if source.ends_with([' ', '\t']) {
        (source.len() - 1, line)
    } else {
        let last = tokens.last().map_or(0, |token| token.start);
        (last, line)
    };
    let mut diagnostic = Diagnostic::new(Kind::EndOfFile, offset, message);
    diagnostic.line = Some(line);
    Some(diagnostic)
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
        let diagnostic = syntax_error(source, 15, false);
        assert_eq!(
            rendered(&diagnostic, source),
            "awk: cmd. line:2: BEGIN { x = 1 +\nawk: cmd. line:2:                ^ unexpected \
             newline or end of string\n"
        );
    }

    #[test]
    fn a_division_is_no_regex() {
        assert!(scan("BEGIN { x = a / 2 / 3 }").error.is_none());
        assert!(
            scan("BEGIN { x = (1) /2/ 3; y = a[1] / 2 }")
                .error
                .is_none()
        );
        let (_, diagnostic) = scan("BEGIN { x = /abc }").error.expect("an error");
        assert_eq!(diagnostic.offset, 13);
    }
}
