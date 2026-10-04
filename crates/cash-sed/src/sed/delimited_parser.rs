// Parse delimited character sequences
//
// SPDX-License-Identifier: MIT
// Copyright (c) 2025 Diomidis Spinellis
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use crate::sed::command::{CharacterMode, ParsedTransliteration, RE_DUP_MAX, RegexMode};
use crate::sed::error_handling::{compilation_err, compilation_error};
use crate::sed::script_char_provider::ScriptCharProvider;
use crate::sed::script_line_provider::ScriptLineProvider;

use std::char;
use std::ffi::OsString;
use std::string::FromUtf8Error;
use uucore::error::UResult;

/// Construct an OS string from UTF-8 bytes, as Windows has no byte-native paths.
pub fn os_string_from_bytes(bytes: Vec<u8>) -> Result<OsString, FromUtf8Error> {
    String::from_utf8(bytes).map(OsString::from)
}

/// Return true if c is a valid octal digit
fn is_ascii_octal_digit(c: char) -> bool {
    matches!(c, '0'..='7')
}

/// Parse a numeric character escape and return the corresponding char.
/// Advance line to the first character not part of the escape.
/// ndigits is the number of allowed digits and radix is the value's
/// radix (e.g. 8, 10, 16 for octal, decimal, and hex escapes).
/// For values up to 3 ndigits is the maximum number of allowed digits,
/// for values above 3 ndigits is the exact number of allowed digits.
/// Return `None` if no valid character has been specified.
fn parse_numeric_escape(
    line: &mut ScriptCharProvider,
    is_allowed_char: fn(char) -> bool,
    ndigits: usize,
    radix: u32,
) -> Option<char> {
    let mut valid_chars = Vec::new();

    for _ in 0..ndigits {
        if !line.eol() && is_allowed_char(line.current()) {
            valid_chars.push(line.current());
            line.advance();
        } else {
            break;
        }
    }

    if valid_chars.is_empty() {
        return None;
    }

    if ndigits > 3 && valid_chars.len() != ndigits {
        line.retreat(valid_chars.len());
        return None;
    }

    let char_string: String = valid_chars.into_iter().collect();
    let decoded = u32::from_str_radix(&char_string, radix)
        .ok()
        .and_then(char::from_u32);
    if decoded.is_none() {
        // A value that is no character, a surrogate such as `\uD800` or one past
        // U+10FFFF, is not an escape: its digits are read again as text, as for an
        // escape with too few digits.
        line.retreat(char_string.len());
    }
    decoded
}

/// Transforms the specified character into the corresponding ASCII
/// control character as follows.
/// - Convert lowercase letters to uppercase
/// - XOR the ASCII value with 0x40 (inverts bit 6)
///
/// Return `None` if the result is not a valid Unicode scalar.
fn create_control_char(x: char) -> Option<char> {
    if !x.is_ascii() {
        return None;
    }

    let c = x.to_ascii_uppercase();

    let transformed = (c as u8) ^ 0x40;
    char::from_u32(u32::from(transformed))
}

/// Append a parsed script character according to the active character mode.
pub fn push_script_char(bytes: &mut Vec<u8>, ch: char, character_mode: CharacterMode) {
    match character_mode {
        CharacterMode::Byte if (ch as u32) <= 0xFF => bytes.push(ch as u8),
        _ => {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }
}

/// Decode parsed script bytes as UTF-8 for character-mode parsing.
fn parsed_bytes_to_utf8(
    lines: &ScriptLineProvider,
    line: &ScriptCharProvider,
    bytes: Vec<u8>,
    description: &str,
) -> UResult<String> {
    String::from_utf8(bytes)
        .map_err(|e| compilation_err(lines, line, format!("invalid UTF-8 in {description}: {e}")))
}

/// Parse a character escape valid in all contexts (RE pattern, substitution,
/// transliterarion) and return the corresponding char.
/// At entry line.current() must have advanced after the `\\`.
/// Advance line to the first character not part of the escape.
/// Return `None` if an invalid escape has been specified.
///
/// GNU sed has no backspace escape: `\b` is a word boundary in a regular expression and
/// the letter `b` everywhere else. It was a backspace in a replacement, in `y` and in
/// `a`, `i` and `c` text.
pub fn parse_char_escape(line: &mut ScriptCharProvider) -> Option<char> {
    match line.current() {
        'a' => {
            line.advance();
            Some('\x07')
        }
        'f' => {
            line.advance();
            Some('\x0c')
        }
        'n' => {
            line.advance();
            Some('\n')
        }
        'r' => {
            line.advance();
            Some('\r')
        }
        't' => {
            line.advance();
            Some('\t')
        }
        'v' => {
            line.advance();
            Some('\x0b')
        }

        'c' => {
            // Control character escape: \cC. One that ends the line is the letter; reading
            // past the line's end panicked.
            line.advance(); // move past 'c'
            if line.eol() {
                return Some('c');
            }
            match create_control_char(line.current()) {
                Some(decoded) => {
                    line.advance();
                    Some(decoded)
                }
                None => Some('c'),
            }
        }

        'd' => {
            // Decimal escape: \dnnn
            line.advance(); // move past 'd'
            match parse_numeric_escape(line, |c| c.is_ascii_digit(), 3, 10) {
                Some(decoded) => Some(decoded),
                None => Some('d'),
            }
        }

        'o' => {
            // Octal escape: \onnn
            line.advance(); // move past 'o'
            match parse_numeric_escape(line, is_ascii_octal_digit, 3, 8) {
                Some(decoded) => Some(decoded),
                None => Some('o'),
            }
        }

        // GNU sed has no Unicode escapes: `\u` and `\U` are the letters themselves in a
        // regex, in `y` and in `a`, `i` and `c` text, while a replacement reads them as
        // case conversions before it gets here.
        c @ ('u' | 'U') => {
            line.advance();
            Some(c)
        }

        'x' => {
            // Hexadecimal escape: \xnn
            line.advance(); // move past 'x'
            match parse_numeric_escape(line, |c| c.is_ascii_hexdigit(), 2, 16) {
                Some(decoded) => Some(decoded),
                None => Some('x'),
            }
        }
        _ => None,
    }
}

/// What GNU sed makes of a `\c` escape and what follows it.
#[derive(Debug, PartialEq, Eq)]
pub enum ControlEscape {
    /// The control character `\cX` stands for; `\c\\` is the one of a backslash.
    Char(char),
    /// A `\c` before the string's delimiter or the line's end, which has nothing to
    /// stand for.
    Bare,
    /// A `\c` before a backslash that does not start `\\`, GNU sed's error
    /// [`ERR_RECURSIVE_ESCAPE_C`].
    Recursive,
}

/// GNU sed's error for `\c` followed by another escape (`\c\d`).
pub const ERR_RECURSIVE_ESCAPE_C: &str = "recursive escaping after \\c not allowed";

/// Read a `\c` escape, the line at its `c`, as GNU sed reads one in a regular
/// expression, a replacement and the strings of `y`: `\cX` is X's control character,
/// `\c\\` that of a backslash and `\c\/` that of the delimiter, and `\c` before another
/// escape is an error. Before the delimiter or the line's end only the `c` is read.
/// `\c\\` was read as the control character of one backslash, which left the other to
/// escape what came next, and `\c` before the delimiter took the delimiter.
pub fn parse_control_escape(
    line: &mut ScriptCharProvider,
    delimiter: Option<char>,
) -> ControlEscape {
    line.advance(); // Skip the `c`.
    if line.eol() || Some(line.current()) == delimiter {
        return ControlEscape::Bare;
    }
    if line.current() == '\\' {
        let backslash = line.get_pos();
        line.advance();
        if !line.eol() && (line.current() == '\\' || Some(line.current()) == delimiter) {
            let escaped = line.current();
            line.advance();
            return ControlEscape::Char(create_control_char(escaped).unwrap_or('c'));
        }
        // The backslash is left to be read as the escape it starts.
        line.set_position(backslash);
        return ControlEscape::Recursive;
    }
    match create_control_char(line.current()) {
        Some(decoded) => {
            line.advance();
            ControlEscape::Char(decoded)
        }
        None => ControlEscape::Char('c'),
    }
}

/// The escapes GNU sed decodes in a bracket expression (outside POSIX mode): `[\n]` is
/// a newline and `[\t]` a tab, while `[\w]` and `[\]]` are a backslash and what follows
/// it.
fn is_bracket_escape(c: char) -> bool {
    matches!(c, 'a' | 'f' | 'n' | 'r' | 't' | 'v' | 'd' | 'o' | 'x')
}

/// Parse a POSIX RE character class returning it as bytes.
/// This functionality is needed to avoid terminating delimited
/// sequences when a delimiter appears within a character class.
/// A class the line does not end is the error `unterminated`, as for the whole
/// expression in GNU sed: `s/[a/b/` is an unterminated `s` command.
///
/// The class is returned as GNU sed's regex library reads it, where a backslash is an
/// ordinary character: `[\]]` is a backslash followed by `]`, and `[a\]` ends at its
/// `]`. Only the escapes GNU sed decodes before it compiles the expression are
/// decoded, `\n`, `\t`, `\cX`, `\x41` and the like, and none in POSIX mode. The
/// backslash was taken as an escape, so `[\]]` was a `]` alone, `[a\]` was unterminated
/// and `[\.]` left the backslash out. An `\c` before another escape sets `error`.
fn parse_character_class(
    lines: &ScriptLineProvider,
    line: &mut ScriptCharProvider,
    character_mode: CharacterMode,
    posix: bool,
    unterminated: &str,
    error: &mut Option<&'static str>,
) -> UResult<Vec<u8>> {
    let mut result = Vec::new();

    // The caller comes here at a `[`; anything else is not a class.
    if line.eol() || line.current() != '[' {
        return compilation_error(lines, line, "expected `[' to start a character class");
    }

    line.advance();
    result.push(b'[');

    // Optional negation
    if !line.eol() && line.current() == '^' {
        result.push(b'^');
        line.advance();
    }

    // Optional leading ']' inside the class
    if !line.eol() && line.current() == ']' {
        result.push(b']');
        line.advance();
    }

    while !line.eol() {
        let ch = line.current();

        if ch == ']' {
            result.push(b']');
            line.advance();
            return Ok(result);
        }

        if ch == '[' {
            line.advance();
            result.push(b'[');
            if line.eol() {
                continue;
            }
            let marker = line.current();
            // POSIX character class, collating symbol, or equivalence
            if marker == ':' || marker == '.' || marker == '=' {
                line.advance();
                result.push(marker as u8);

                let mut inner = Vec::new();
                let mut terminated = false;

                while !line.eol() {
                    let c = line.current();
                    if c == marker {
                        line.advance();
                        if !line.eol() && line.current() == ']' {
                            line.advance();
                            result.extend_from_slice(&inner);
                            result.push(marker as u8);
                            result.push(b']');
                            terminated = true;
                            break;
                        }
                        // False alarm, just part of the inner name
                        inner.push(marker as u8);
                    } else {
                        inner.push(line.current_byte());
                        line.advance();
                    }
                }

                if !terminated {
                    return compilation_error(lines, line, unterminated);
                }
            }
            // A `[` that starts none of them is itself, and what follows it is read as
            // usual: `[[]` is a `[`. The character after it was taken with it, so that
            // class never ended.
            continue;
        }

        if ch == '\\' {
            line.advance();
            if line.eol() {
                break;
            }
            let escaped = line.current();
            if escaped == '\\' {
                // Two backslashes, each one itself.
                result.extend_from_slice(b"\\\\");
                line.advance();
                continue;
            }
            if !posix && escaped == 'c' {
                match parse_control_escape(line, None) {
                    ControlEscape::Char(decoded) => {
                        push_script_char(&mut result, decoded, character_mode);
                    }
                    ControlEscape::Bare => result.extend_from_slice(b"\\c"),
                    ControlEscape::Recursive => {
                        error.get_or_insert(ERR_RECURSIVE_ESCAPE_C);
                    }
                }
                continue;
            }
            if !posix
                && is_bracket_escape(escaped)
                && let Some(decoded) = parse_char_escape(line)
            {
                push_script_char(&mut result, decoded, character_mode);
                continue;
            }
            // The backslash is itself; the character after it is read as usual.
            result.push(b'\\');
            continue;
        }

        result.push(line.current_byte());
        line.advance();
    }

    compilation_error(lines, line, unterminated)
}

/// Scan and return the opening delimiter of a delimited string
/// Advances the line past the opening delimiter
fn scan_delimiter(lines: &ScriptLineProvider, line: &mut ScriptCharProvider) -> UResult<char> {
    // Sanity check
    if line.eol() {
        return compilation_error(lines, line, "unexpected end of line".to_string());
    }

    let delimiter = line.current();
    if delimiter == '\\' {
        return compilation_error(lines, line, "\\ cannot be used as a string delimiter");
    }
    line.advance(); // skip the opening delimiter
    Ok(delimiter)
}

/// Move `line` to the next line of the script when a backslash ends the current one and
/// a newline follows it there, and return whether it did. GNU sed reads the backslash
/// and newline as a newline in a regular expression, a replacement and the strings of
/// `y`; at the end of a `-e` expression or of the script there is no newline, and the
/// string is unterminated. It was unterminated at every line's end, and a replacement
/// went on into the next `-e` expression.
pub fn continue_on_next_line(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
) -> UResult<bool> {
    if !lines.line_has_newline() {
        return Ok(false);
    }
    match lines.next_line_in_source()? {
        Some(next) => {
            *line = ScriptCharProvider::new(next);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// GNU sed's error for a regular expression an address does not end.
pub const ERR_UNTERMINATED_ADDRESS_REGEX: &str = "unterminated address regex";

/// A regular expression as read from the script: its text, for GNU sed's regex library
/// to read, and an error GNU sed reports once the expression is read.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RegexText {
    pub pattern: Vec<u8>,
    pub error: Option<&'static str>,
}

/// Parse the regular expression delimited by the current line
/// character and return it as a string.
/// On return, the line is on the closing delimiter.
/// In Basic mode, quantifiers like {m,n} must be escaped (\{m,n\}).
/// In Extended mode, quantifiers like {m,n} don't require escaping.
pub fn parse_regex(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    regex_mode: RegexMode,
) -> UResult<Vec<u8>> {
    parse_regex_for_mode(
        lines,
        line,
        regex_mode,
        CharacterMode::Utf8,
        false,
        ERR_UNTERMINATED_ADDRESS_REGEX,
    )
    .map(|text| text.pattern)
}

/// Parse a regular expression according to the current character mode. One the line
/// does not end is the error `unterminated`, which GNU sed words by where the
/// expression is: an address's, or an `s` command's.
///
/// The text returned is the expression GNU sed's regex library reads: the escapes GNU
/// sed decodes first (`\n`, `\t`, `\x41`, ...) decoded, an escaped delimiter made the
/// delimiter, and every other escape, `\w`, `` \` ``, `\A`, left as it is written for
/// the translation into the engine's syntax (`compiler::regex_to_engine`).
pub fn parse_regex_for_mode(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    regex_mode: RegexMode,
    character_mode: CharacterMode,
    posix: bool,
    unterminated: &str,
) -> UResult<RegexText> {
    let delimiter = scan_delimiter(lines, line)?;
    let mut result = Vec::new();
    let mut error = None;
    while !line.eol() {
        match line.current() {
            '[' if delimiter != '[' => {
                let cc = parse_character_class(
                    lines,
                    line,
                    character_mode,
                    posix,
                    unterminated,
                    &mut error,
                )?;
                result.extend_from_slice(&cc);
                continue;
            }
            '\\' => {
                line.advance();
                if line.eol() {
                    if continue_on_next_line(lines, line)? {
                        result.push(b'\n');
                        continue;
                    }
                    return compilation_error(lines, line, unterminated);
                }
                if line.current() == delimiter {
                    // Push escaped delimiter
                    result.push(line.current_byte());
                    line.advance();
                    continue;
                }
                if line.current() == '{' && matches!(regex_mode, RegexMode::Basic) {
                    result.push(b'\\');
                    result.push(b'{');
                    match read_interval(line, delimiter, RegexMode::Basic) {
                        Some(interval) => result.extend_from_slice(interval.as_bytes()),
                        None => line.advance(),
                    }
                    continue;
                }
                if line.current() == '}' {
                    result.push(b'\\');
                    result.push(b'}');
                    line.advance();
                    continue;
                }
                if line.current() == 'c' {
                    match parse_control_escape(line, Some(delimiter)) {
                        ControlEscape::Char(decoded) => {
                            push_script_char(&mut result, decoded, character_mode);
                        }
                        // Before the delimiter a `\c` leaves a trailing backslash, which
                        // GNU sed's regex library refuses.
                        ControlEscape::Bare => result.push(b'\\'),
                        ControlEscape::Recursive => {
                            error.get_or_insert(ERR_RECURSIVE_ESCAPE_C);
                        }
                    }
                    continue;
                }
                // In a regex \b is a word boundary, which the engine spells the same, and
                // GNU's other operators (\w, \<, \`, ...) are left for the translation.
                if line.current() != 'b'
                    && let Some(decoded) = parse_char_escape(line)
                {
                    push_script_char(&mut result, decoded, character_mode);
                } else {
                    result.push(b'\\');
                    result.push(line.current_byte());
                    line.advance();
                }
                continue;
            }
            '{' if delimiter != '{' && matches!(regex_mode, RegexMode::Extended) => {
                result.push(b'{');
                match read_interval(line, delimiter, RegexMode::Extended) {
                    Some(interval) => result.extend_from_slice(interval.as_bytes()),
                    None => line.advance(),
                }
                continue;
            }
            '}' if delimiter != '}' => {
                result.push(b'}');
                line.advance();
                continue;
            }

            c if c == delimiter => {
                return Ok(RegexText {
                    pattern: result,
                    error,
                });
            }
            _ => result.push(line.current_byte()),
        }
        line.advance();
    }
    compilation_error(lines, line, unterminated)
}

/// Read the interval at the line's `{` (after its `\` in a basic expression) and return
/// its content, an absent minimum written 0 (`{,n}` is `{0,n}`), with the line left on
/// the closing `}` (its `\` in a basic expression). One that is not an interval sed
/// takes, `None` with the line still at the `{`, is left as it is written: the check of
/// the whole expression (`gnu_regex`) reports it in GNU sed's words, once the command is
/// read, as GNU sed does. It was refused here, at the brace.
fn read_interval(
    line: &mut ScriptCharProvider,
    delimiter: char,
    regex_mode: RegexMode,
) -> Option<String> {
    let start = line.get_pos();
    let interval = read_interval_content(line, delimiter, regex_mode);
    if interval.is_none() {
        line.set_position(start);
    }
    interval
}

fn read_interval_content(
    line: &mut ScriptCharProvider,
    delimiter: char,
    regex_mode: RegexMode,
) -> Option<String> {
    line.advance(); // Skip the opening brace.
    let mut content = String::new();
    loop {
        if line.eol() || line.current() == delimiter {
            return None;
        }
        match (line.current(), regex_mode) {
            ('}', RegexMode::Extended) => break,
            ('\\', RegexMode::Basic) => {
                let backslash = line.get_pos();
                line.advance();
                if line.eol() || line.current() != '}' {
                    return None;
                }
                line.set_position(backslash);
                break;
            }
            (c @ ('0'..='9' | ','), _) => content.push(c),
            _ => return None,
        }
        line.advance();
    }

    let (min, max) = match content.split_once(',') {
        Some((min, max)) => (min, Some(max)),
        None => (content.as_str(), None),
    };
    let bound = |digits: &str| match digits.parse::<usize>() {
        Ok(value) if value <= RE_DUP_MAX => Some(value),
        _ => None,
    };
    let low = if min.is_empty() { 0 } else { bound(min)? };
    match max {
        // `{}` is not an interval, nor one whose maximum is below its minimum. A second
        // comma makes the maximum no number.
        None if min.is_empty() => return None,
        Some(max) if !max.is_empty() && bound(max)? < low => return None,
        _ => {}
    }

    let mut result = if min.is_empty() {
        "0".to_string()
    } else {
        min.to_string()
    };
    if let Some(max) = max {
        result.push(',');
        result.push_str(max);
    }
    Some(result)
}

// Parse the transliteration string delimited by the current line
/// character and return it as a string.
/// On return the line is on the closing delimiter.
pub fn parse_transliteration(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
) -> UResult<Vec<u8>> {
    parse_transliteration_bytes(lines, line, CharacterMode::Utf8, &mut None)
}

/// Parse transliteration bytes according to the current character mode.
///
/// As in GNU sed, a backslash before a character with no escape of its own stands for
/// the character (`y/\q/x/` maps `q`), and a backslash ending a line that has a newline
/// is a newline. Both were kept, the first as two characters and the second as an
/// unterminated `y`. An `\c` before another escape sets `error`.
fn parse_transliteration_bytes(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    character_mode: CharacterMode,
    error: &mut Option<&'static str>,
) -> UResult<Vec<u8>> {
    let delimiter = scan_delimiter(lines, line)?;
    let mut result = Vec::new();

    while !line.eol() {
        match line.current() {
            '\\' => {
                line.advance();
                if line.eol() {
                    if continue_on_next_line(lines, line)? {
                        result.push(b'\n');
                        continue;
                    }
                    return compilation_error(lines, line, "unterminated `y' command");
                }
                if line.current() == delimiter || line.current() == '\\' {
                    // Push only the escaped character
                    result.push(line.current_byte());
                    line.advance();
                    continue;
                }
                if line.current() == 'c' {
                    match parse_control_escape(line, Some(delimiter)) {
                        ControlEscape::Char(decoded) => {
                            push_script_char(&mut result, decoded, character_mode);
                        }
                        ControlEscape::Bare => {}
                        ControlEscape::Recursive => {
                            error.get_or_insert(ERR_RECURSIVE_ESCAPE_C);
                        }
                    }
                    continue;
                }
                if let Some(decoded) = parse_char_escape(line) {
                    push_script_char(&mut result, decoded, character_mode);
                } else {
                    result.push(line.current_byte());
                    line.advance();
                }
                continue;
            }
            c if c == delimiter => return Ok(result),
            _ => result.push(line.current_byte()),
        }
        line.advance();
    }
    compilation_error(lines, line, "unterminated `y' command")
}

/// Parse a transliteration string according to the current character mode. The line is
/// left on its closing delimiter, where GNU sed reports an `\c` before another escape:
/// that error is returned at it.
pub fn parse_transliteration_for_mode(
    lines: &mut ScriptLineProvider,
    line: &mut ScriptCharProvider,
    character_mode: CharacterMode,
) -> UResult<ParsedTransliteration> {
    let mut error = None;
    let bytes = parse_transliteration_bytes(lines, line, character_mode, &mut error)?;
    if let Some(error) = error {
        return compilation_error(lines, line, error);
    }
    match character_mode {
        CharacterMode::Byte => Ok(ParsedTransliteration::Bytes(bytes)),
        CharacterMode::Utf8 => parsed_bytes_to_utf8(lines, line, bytes, "transliteration string")
            .map(ParsedTransliteration::Text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_os_string_from_bytes_accepts_utf8() {
        let os = os_string_from_bytes("aé".as_bytes().to_vec()).unwrap();
        assert_eq!(os, OsString::from("aé"));
    }

    #[test]
    fn test_parsed_bytes_to_utf8_accepts_valid_utf8() {
        let lines = ScriptLineProvider::with_active_state("test.sed", 1);
        let line = ScriptCharProvider::new("");
        let text = parsed_bytes_to_utf8(&lines, &line, "é".as_bytes().to_vec(), "test").unwrap();
        assert_eq!(text, "é");
    }

    #[test]
    fn test_parsed_bytes_to_utf8_rejects_invalid_utf8() {
        let lines = ScriptLineProvider::with_active_state("test.sed", 1);
        let line = ScriptCharProvider::new("");
        let err = parsed_bytes_to_utf8(&lines, &line, b"\xE9".to_vec(), "test").unwrap_err();
        assert!(err.to_string().contains("invalid UTF-8 in test"));
    }

    fn make_providers(input: &str) -> (ScriptLineProvider, ScriptCharProvider) {
        let lines = ScriptLineProvider::new(vec![]); // Empty for tests
        let line = ScriptCharProvider::new(input);
        (lines, line)
    }

    // parse_numeric_escape
    #[test]
    fn test_compile_octal_escape() {
        let mut provider = ScriptCharProvider::new("141rest");
        let c = parse_numeric_escape(&mut provider, is_ascii_octal_digit, 3, 8);
        assert_eq!(c, Some('a'));
        assert_eq!(provider.current(), 'r'); // "141" was consumed
    }

    #[test]
    fn test_compile_octal_escape_eol() {
        let mut provider = ScriptCharProvider::new("141");
        let c = parse_numeric_escape(&mut provider, is_ascii_octal_digit, 3, 8);
        assert_eq!(c, Some('a'));
        assert!(provider.eol()); // "141" was consumed
    }

    #[test]
    fn test_compile_decimal_escape() {
        let mut provider = ScriptCharProvider::new("0659");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_digit(), 3, 10);
        assert_eq!(c, Some('A'));
        assert_eq!(provider.current(), '9'); // "65" was consumed
    }

    #[test]
    fn test_compile_decimal_invalid() {
        let mut provider = ScriptCharProvider::new("QR");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_digit(), 3, 10);
        assert_eq!(c, None);
        assert_eq!(provider.current(), 'Q');
    }

    #[test]
    fn test_compile_hex_escape() {
        let mut provider = ScriptCharProvider::new("3cZ");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_hexdigit(), 2, 16);
        assert_eq!(c, Some('<'));
        assert_eq!(provider.current(), 'Z'); // "41" was consumed
    }

    #[test]
    fn test_compile_hex_escape_truncated() {
        let mut provider = ScriptCharProvider::new("4G");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_hexdigit(), 2, 16);
        assert_eq!(c, Some('\u{4}')); // Only '4' is valid hex
        assert_eq!(provider.current(), 'G'); // "41" was consumed
    }

    #[test]
    fn test_compile_unicode_escape_short() {
        // U+2665 = '♥'
        let mut provider = ScriptCharProvider::new("26650");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_hexdigit(), 4, 16);
        assert_eq!(c, Some('♥'));
        assert_eq!(provider.current(), '0'); // "2665" was consumed
    }

    #[test]
    fn test_compile_unicode_escape_short_invalid() {
        let mut provider = ScriptCharProvider::new("123Q");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_hexdigit(), 4, 16);
        assert_eq!(c, None);
        assert_eq!(provider.current(), '1');
    }

    #[test]
    fn test_compile_unicode_escape_long_invalid() {
        // U+2665 = '♥'
        let mut provider = ScriptCharProvider::new("1234567Q");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_hexdigit(), 8, 16);
        assert_eq!(c, None);
        assert_eq!(provider.current(), '1');
    }

    #[test]
    fn test_compile_unicode_escape_long() {
        // U+1F600 = 😀
        let mut provider = ScriptCharProvider::new("0001F6009");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_hexdigit(), 8, 16);
        assert_eq!(c, Some('😀'));
        assert_eq!(provider.current(), '9'); // "0001F600" was consumed
    }

    #[test]
    fn test_no_valid_digits() {
        let mut provider = ScriptCharProvider::new("xyz");
        let c = parse_numeric_escape(&mut provider, |c| c.is_ascii_digit(), 3, 10);
        assert_eq!(c, None);
        assert_eq!(provider.current(), 'x'); // No advancement
    }

    // create_control_char
    #[test]
    fn test_lowercase_letter() {
        assert_eq!(create_control_char('z'), Some('\u{1a}')); // 0x5A ^ 0x40 = 0x1A
        assert_eq!(create_control_char('a'), Some('\u{01}')); // 0x41 ^ 0x40 = 0x01
    }

    #[test]
    fn test_uppercase_letter() {
        assert_eq!(create_control_char('Z'), Some('\u{1a}'));
        assert_eq!(create_control_char('A'), Some('\u{01}'));
    }

    #[test]
    fn test_symbol_characters() {
        assert_eq!(create_control_char('{'), Some(';')); // 0x7B ^ 0x40 = 0x3B
        assert_eq!(create_control_char(';'), Some('{')); // 0x3B ^ 0x40 = 0x7B
    }

    #[test]
    fn test_non_ascii_char() {
        // This will not match any transformation and may panic if it overflows
        // But the current function only handles ASCII-safe chars
        assert_eq!(create_control_char('é'), None); // outside ASCII
    }

    #[test]
    fn test_edge_ascii_values() {
        assert_eq!(create_control_char('@'), Some('\0')); // 0x40 ^ 0x40 = 0x00
        assert_eq!(create_control_char('\x7F'), Some('\x3F')); // 0x7F ^ 0x40 = 0x3F
    }

    // parse_char_escape
    fn escape_result_with_current(input: &str) -> (Option<char>, Option<char>) {
        let mut provider = ScriptCharProvider::new(input);
        let result = parse_char_escape(&mut provider);
        let current = if provider.eol() {
            None
        } else {
            Some(provider.current())
        };
        (result, current)
    }

    #[test]
    fn test_standard_escapes_eol() {
        assert_eq!(escape_result_with_current("a"), (Some('\x07'), None));
        assert_eq!(escape_result_with_current("f"), (Some('\x0c'), None));
        assert_eq!(escape_result_with_current("n"), (Some('\n'), None));
        assert_eq!(escape_result_with_current("r"), (Some('\r'), None));
        assert_eq!(escape_result_with_current("t"), (Some('\t'), None));
        assert_eq!(escape_result_with_current("v"), (Some('\x0b'), None));
    }

    #[test]
    fn test_standard_escapes_more() {
        assert_eq!(escape_result_with_current("a."), (Some('\x07'), Some('.')));
        assert_eq!(escape_result_with_current("f."), (Some('\x0c'), Some('.')));
        assert_eq!(escape_result_with_current("n."), (Some('\n'), Some('.')));
        assert_eq!(escape_result_with_current("r."), (Some('\r'), Some('.')));
        assert_eq!(escape_result_with_current("t."), (Some('\t'), Some('.')));
        assert_eq!(escape_result_with_current("v."), (Some('\x0b'), Some('.')));
    }

    #[test]
    fn test_escape_invalid() {
        assert_eq!(escape_result_with_current("zx"), (None, Some('z')));
    }

    #[test]
    fn test_control_escape_valid() {
        assert_eq!(escape_result_with_current("cZ"), (Some('\x1A'), None));
    }

    #[test]
    fn test_control_escape_invalid() {
        assert_eq!(escape_result_with_current("cé"), (Some('c'), Some('Ã')));
    }

    #[test]
    fn test_decimal_escape_valid() {
        assert_eq!(escape_result_with_current("d065r"), (Some('A'), Some('r')));
    }

    #[test]
    fn test_octal_escape_valid() {
        assert_eq!(escape_result_with_current("o141x"), (Some('a'), Some('x')));
    }

    #[test]
    fn test_hex_escape_valid() {
        assert_eq!(escape_result_with_current("x41;"), (Some('A'), Some(';')));
    }

    #[test]
    fn test_push_script_char_byte_mode_single_byte() {
        let mut bytes = Vec::new();
        push_script_char(&mut bytes, 'é', CharacterMode::Byte);
        assert_eq!(bytes, b"\xE9");
    }

    #[test]
    fn test_push_script_char_utf8_mode_encodes_utf8() {
        let mut bytes = Vec::new();
        push_script_char(&mut bytes, 'é', CharacterMode::Utf8);
        assert_eq!(bytes, "é".as_bytes());
    }

    // GNU sed has no Unicode escapes: an escaped `u` or `U` is the letter, and the hex
    // digits after it stay where they are.
    #[test]
    fn test_short_unicode_escape_is_the_letter() {
        assert_eq!(escape_result_with_current("u2665;"), (Some('u'), Some('2')));
    }

    #[test]
    fn test_long_unicode_escape_is_the_letter() {
        assert_eq!(
            escape_result_with_current("U0001F600;"),
            (Some('U'), Some('0'))
        );
    }

    #[test]
    fn test_decimal_escape_fallback() {
        assert_eq!(escape_result_with_current("d;."), (Some('d'), Some(';')));
    }

    #[test]
    fn test_octal_escape_fallback() {
        assert_eq!(escape_result_with_current("o9x"), (Some('o'), Some('9')));
    }

    #[test]
    fn test_hex_escape_fallback() {
        assert_eq!(escape_result_with_current("xyz"), (Some('x'), Some('y')));
    }

    #[test]
    fn test_unknown_escape() {
        assert_eq!(escape_result_with_current("q"), (None, Some('q')));
    }

    // parse_character_class
    fn char_provider_from(input: &str) -> ScriptCharProvider {
        ScriptCharProvider::new(input)
    }

    fn test_lines() -> ScriptLineProvider {
        ScriptLineProvider::with_active_state("test.sed", 3)
    }

    #[test]
    fn test_basic_character_class() {
        let mut line = char_provider_from("[qr]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[qr]");
    }

    #[test]
    fn test_negated_class() {
        let mut line = char_provider_from("[^abc]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[^abc]");
    }

    #[test]
    fn test_leading_close_bracket() {
        let mut line = char_provider_from("[]abc]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[]abc]");
    }

    #[test]
    fn test_leading_negated_close_bracket() {
        let mut line = char_provider_from("[^]abc]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[^]abc]");
    }

    #[test]
    fn test_escaped_character_begin() {
        let mut line = char_provider_from("[\\nabc]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[\nabc]");
    }

    #[test]
    fn test_escaped_character_middle() {
        let mut line = char_provider_from("[a\\nbc]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[a\nbc]");
    }

    #[test]
    fn test_escaped_character_end() {
        let mut line = char_provider_from("[abc\\n]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[abc\n]");
    }

    // A backslash is an ordinary character in a bracket expression, as in GNU sed, so
    // the `]` after it ends the class.
    #[test]
    fn test_backslash_before_close_bracket() {
        let mut line = char_provider_from("[a\\]bc]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, br"[a\]");
        assert_eq!(line.current(), 'b');
    }

    // GNU sed decodes `\n`, `\t` and the like in a bracket expression, but in POSIX mode;
    // any other backslash is itself, two of them two.
    #[test]
    fn test_escapes_in_character_class() {
        for (input, posix, expected) in [
            ("[\\n]", false, &b"[\n]"[..]),
            ("[\\n]", true, br"[\n]"),
            ("[\\t\\x41]", false, b"[\tA]"),
            ("[\\w]", false, br"[\w]"),
            ("[\\b]", false, br"[\b]"),
            ("[\\\\n]", false, br"[\\n]"),
            ("[\\c]]", false, b"[\x1d]"),
        ] {
            let mut line = char_provider_from(input);
            let lines = test_lines();
            let result = parse_character_class(
                &lines,
                &mut line,
                CharacterMode::Utf8,
                posix,
                "unterminated",
                &mut None,
            )
            .unwrap();
            assert_eq!(result, expected, "{input} posix={posix}");
        }
    }

    #[test]
    fn test_posix_class() {
        let mut line = char_provider_from("[[:digit:]]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[[:digit:]]");
    }

    #[test]
    fn test_colon_literal_character_class() {
        let mut line = char_provider_from("[:]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[:]");
    }

    #[test]
    fn test_equivalence_class() {
        let mut line = char_provider_from("[[=a=]]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[[=a=]]");
    }

    #[test]
    fn test_collating_symbol() {
        let mut line = char_provider_from("[[.ch.]]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[[.ch.]]");
    }

    #[test]
    fn test_unterminated_class_error() {
        let mut line = char_provider_from("[abc"); // missing closing ]
        let lines = test_lines();
        let err = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        );
        assert!(err.is_err());
    }

    #[test]
    fn test_open_bracket_at_eol_errors() {
        let mut line = char_provider_from("[");
        let lines = test_lines();
        let err = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unterminated"));
    }

    #[test]
    fn test_unterminated_posix_class_error() {
        let mut line = char_provider_from("[[:digit:]");
        let lines = test_lines();
        let err = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        );
        assert!(err.is_err());
    }

    #[test]
    fn test_unterminated_escape_error() {
        let mut line = char_provider_from("[abc\\"); // missing closing ]
        let lines = test_lines();
        let err = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        );
        assert!(err.is_err());
    }

    #[test]
    fn test_malformed_posix_like_pattern_treated_as_literal() {
        let mut line = char_provider_from("[[x]yz]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[[x]");
    }

    #[test]
    fn test_literal_open_bracket_in_character_class() {
        let mut line = char_provider_from("[a[b]");
        let lines = test_lines();
        let result = parse_character_class(
            &lines,
            &mut line,
            CharacterMode::Utf8,
            false,
            "unterminated",
            &mut None,
        )
        .unwrap();
        assert_eq!(result, b"[a[b]");
    }

    // parse_regex
    #[test]
    fn test_simple_regex() {
        let (mut lines, mut line) = make_providers("/abc/");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, b"abc");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_regex_with_escaped_delimiter() {
        let (mut lines, mut line) = make_providers("/ab\\/c/");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, b"ab/c");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_regex_with_capture() {
        let (mut lines, mut line) = make_providers(r"/\(.\)/c/");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, br"\(.\)");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_regex_with_escape_sequence() {
        let (mut lines, mut line) = make_providers("/ab\\n/");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, b"ab\n");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_basic_regex_quantifier() {
        let (mut lines, mut line) = make_providers("/a\\{2,3\\}/p");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, br"a\{2,3\}");
        assert_eq!(line.current(), '/');
    }

    // A brace that does not start an interval is left as it is written, for the check
    // of the whole expression to report once the command is read, as GNU sed does.
    #[test]
    fn test_regex_with_braces_that_are_not_intervals() {
        for (input, mode, expected) in [
            ("/a\\{2,3/p", RegexMode::Basic, &br"a\{2,3"[..]),
            ("/a\\{2d,3\\}/p", RegexMode::Basic, br"a\{2d,3\}"),
            ("/a{2,3/p", RegexMode::Extended, b"a{2,3"),
            ("/a{}/p", RegexMode::Extended, b"a{}"),
            ("/a{2d,3}/p", RegexMode::Extended, b"a{2d,3}"),
            ("/a{2,-3}/p", RegexMode::Extended, b"a{2,-3}"),
            ("/a{3,2}/p", RegexMode::Extended, b"a{3,2}"),
        ] {
            let (mut lines, mut line) = make_providers(input);
            let parsed = parse_regex(&mut lines, &mut line, mode).unwrap();
            assert_eq!(parsed, expected, "{input}");
            assert_eq!(line.current(), '/');
        }
    }

    #[test]
    fn test_regex_interval_without_minimum() {
        let (mut lines, mut line) = make_providers("/a{,3}/p");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Extended).unwrap();
        assert_eq!(parsed, b"a{0,3}");
    }

    #[test]
    fn test_extended_regex_quantifier() {
        let (mut lines, mut line) = make_providers("/a{2,3}/p");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Extended).unwrap();
        assert_eq!(parsed, b"a{2,3}");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn errors_on_unterminated_regex() {
        let (mut lines, mut line) = make_providers("/unterminated");
        let err = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap_err();
        assert!(err.to_string().contains(ERR_UNTERMINATED_ADDRESS_REGEX));
    }

    #[test]
    fn errors_on_esc_at_re_eol() {
        let (mut lines, mut line) = make_providers("/foo\\");
        let err = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap_err();
        assert!(err.to_string().contains(ERR_UNTERMINATED_ADDRESS_REGEX));
    }

    #[test]
    fn errors_on_backslash_delimiter() {
        let (mut lines, mut line) = make_providers("\\bad");
        let err = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap_err();
        assert!(
            err.to_string()
                .contains("\\ cannot be used as a string delimiter")
        );
    }

    #[test]
    fn test_regex_with_character_class() {
        let (mut lines, mut line) = make_providers("/[a-z]/");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, b"[a-z]");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_regex_with_bracket_delimiter() {
        let (mut lines, mut line) = make_providers("[abc[");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, b"abc");
        assert_eq!(line.current(), '[');
    }

    #[test]
    fn test_bracket_regex_with_bracket_delimiter() {
        let (mut lines, mut line) = make_providers("[a\\[0-9]bc[");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, b"a[0-9]bc");
        assert_eq!(line.current(), '[');
    }

    #[test]
    fn test_regex_with_escaped_bracket_in_character_class() {
        let (mut lines, mut line) = make_providers("/[a\\]z]/");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, br"[a\]z]");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_regex_with_delimiter_inside_character_class() {
        let (mut lines, mut line) = make_providers("/[a/c]/");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, b"[a/c]");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_regex_with_escaped_paren_and_backslash() {
        let (mut lines, mut line) = make_providers("/\\(\\\\/");
        let parsed = parse_regex(&mut lines, &mut line, RegexMode::Basic).unwrap();
        assert_eq!(parsed, br"\(\\");
        assert_eq!(line.current(), '/');
    }

    // read_interval
    #[test]
    fn test_read_interval_bre() {
        let (_, mut line) = make_providers("{2,3\\}");
        assert_eq!(
            read_interval(&mut line, '/', RegexMode::Basic).as_deref(),
            Some("2,3")
        );
        assert_eq!(line.current(), '\\'); // On the closing `\}`
    }

    #[test]
    fn test_read_interval_ere() {
        let (_, mut line) = make_providers("{2,3}");
        assert_eq!(
            read_interval(&mut line, '/', RegexMode::Extended).as_deref(),
            Some("2,3")
        );
        assert_eq!(line.current(), '}');
    }

    #[test]
    fn test_read_interval_writes_an_absent_minimum() {
        for (input, expected) in [("{2}", "2"), ("{,}", "0,"), ("{,3}", "0,3"), ("{2,}", "2,")] {
            let (_, mut line) = make_providers(input);
            assert_eq!(
                read_interval(&mut line, '/', RegexMode::Extended).as_deref(),
                Some(expected),
                "{input}"
            );
        }
    }

    // What is not an interval is left where it is, for the check of the whole
    // expression to report in GNU sed's words.
    #[test]
    fn test_read_interval_leaves_what_is_not_one() {
        for (input, mode) in [
            ("{2,3", RegexMode::Basic),
            ("{\\}", RegexMode::Basic),
            ("{2d,3\\}", RegexMode::Basic),
            ("{2,3,\\}", RegexMode::Basic),
            ("{2,3/x\\}", RegexMode::Basic),
            ("{2,3", RegexMode::Extended),
            ("{}", RegexMode::Extended),
            ("{2d,3}", RegexMode::Extended),
            ("{2,3,}", RegexMode::Extended),
            ("{3,2}", RegexMode::Extended),
            ("{32768}", RegexMode::Extended),
            ("{2,32768}", RegexMode::Extended),
            ("{,32768}", RegexMode::Extended),
            ("{99999999999999999999999}", RegexMode::Extended),
        ] {
            let (_, mut line) = make_providers(input);
            assert_eq!(read_interval(&mut line, '/', mode), None, "{input}");
            assert_eq!(line.get_pos(), 0, "{input}");
        }
    }

    // parse_transliteration
    #[test]
    fn test_simple_transliteration() {
        let (mut lines, mut line) = make_providers("/abc/");
        let parsed = parse_transliteration(&mut lines, &mut line).unwrap();
        assert_eq!(parsed, b"abc");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_transliteration_with_escaped_delimiter() {
        let (mut lines, mut line) = make_providers("/ab\\/c/");
        let parsed = parse_transliteration(&mut lines, &mut line).unwrap();
        assert_eq!(parsed, b"ab/c");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_transliteration_with_escaped_backslash() {
        let (mut lines, mut line) = make_providers("/ab\\\\c/");
        let parsed = parse_transliteration(&mut lines, &mut line).unwrap();
        assert_eq!(parsed, br"ab\c");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_transliteration_backslash_character() {
        let (mut lines, mut line) = make_providers("/\\\\/");
        let parsed =
            parse_transliteration_bytes(&mut lines, &mut line, CharacterMode::Utf8, &mut None)
                .unwrap();
        assert_eq!(parsed, br"\");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_transliteration_with_escape_sequence() {
        let (mut lines, mut line) = make_providers("/ab\\n/");
        let parsed = parse_transliteration(&mut lines, &mut line).unwrap();
        assert_eq!(parsed, b"ab\n");
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_parse_transliteration_for_mode_bytes() {
        let (mut lines, mut line) = make_providers("/a\\xE9/");
        let parsed =
            parse_transliteration_for_mode(&mut lines, &mut line, CharacterMode::Byte).unwrap();
        assert_eq!(parsed, ParsedTransliteration::Bytes(b"a\xE9".to_vec()));
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn test_parse_transliteration_for_mode_utf8() {
        let (mut lines, mut line) = make_providers("/a\\xE9/");
        let parsed =
            parse_transliteration_for_mode(&mut lines, &mut line, CharacterMode::Utf8).unwrap();
        assert_eq!(parsed, ParsedTransliteration::Text("aé".to_string()));
        assert_eq!(line.current(), '/');
    }

    #[test]
    fn errors_on_unterminated_transliteration() {
        let (mut lines, mut line) = make_providers("/unterminated");
        let err = parse_transliteration(&mut lines, &mut line).unwrap_err();
        assert!(err.to_string().contains("unterminated `y' command"));
    }

    #[test]
    fn errors_on_esc_at_tr_eol() {
        let (mut lines, mut line) = make_providers("/foo\\");
        let err = parse_transliteration(&mut lines, &mut line).unwrap_err();
        assert!(err.to_string().contains("unterminated `y' command"));
    }
}
