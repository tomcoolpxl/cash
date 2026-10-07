//! What the listings show: mode strings, and names quoted as gnulib's `quotearg` quotes
//! them for GNU tar.

use crate::member::Kind;

/// gnulib's quoting styles, as `--quoting-style` names them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Style {
    /// Names as they are.
    Literal,
    /// Quoted for the shell when they need it.
    Shell,
    /// Always quoted for the shell.
    ShellAlways,
    /// Quoted for the shell, with `$'...'` for unprintable characters.
    ShellEscape,
    /// Always quoted for the shell, with `$'...'`.
    ShellEscapeAlways,
    /// In double quotes, with C's escapes.
    C,
    /// C's escapes, in double quotes when there is one.
    CMaybe,
    /// C's escapes without quotes: GNU tar's default.
    Escape,
    /// In quotes, with C's escapes.
    Locale,
    /// In double quotes, with C's escapes.
    Clocale,
}

impl Style {
    /// The style `--quoting-style` names.
    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "literal" => Self::Literal,
            "shell" => Self::Shell,
            "shell-always" => Self::ShellAlways,
            "shell-escape" => Self::ShellEscape,
            "shell-escape-always" => Self::ShellEscapeAlways,
            "c" => Self::C,
            "c-maybe" => Self::CMaybe,
            "escape" => Self::Escape,
            "locale" => Self::Locale,
            "clocale" => Self::Clocale,
            _ => return None,
        })
    }

    /// The names, in gnulib's order, for an error's list of valid arguments.
    pub const NAMES: [&'static str; 10] = [
        "literal",
        "shell",
        "shell-always",
        "shell-escape",
        "shell-escape-always",
        "c",
        "c-maybe",
        "escape",
        "locale",
        "clocale",
    ];
}

/// The escape of a byte as C writes it: `\n`, `\t`, … or three octal digits.
fn c_escape(byte: u8, out: &mut String) {
    match byte {
        0x07 => out.push_str("\\a"),
        0x08 => out.push_str("\\b"),
        0x0c => out.push_str("\\f"),
        b'\n' => out.push_str("\\n"),
        b'\r' => out.push_str("\\r"),
        b'\t' => out.push_str("\\t"),
        0x0b => out.push_str("\\v"),
        _ => {
            use std::fmt::Write as _;
            let _ = write!(out, "\\{byte:03o}");
        }
    }
}

/// Splits `name` into characters: valid UTF-8 ones when `utf8` (the locale's charset),
/// and single bytes otherwise.
fn pieces(name: &[u8], utf8: bool) -> Vec<Result<char, u8>> {
    let mut out = Vec::new();
    let mut rest = name;
    while !rest.is_empty() {
        if utf8 {
            let len = match rest.first() {
                Some(b) if *b < 0x80 => 1,
                Some(b) if *b >= 0xf0 => 4,
                Some(b) if *b >= 0xe0 => 3,
                Some(b) if *b >= 0xc0 => 2,
                _ => 1,
            };
            if let Some(c) = rest
                .get(..len)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .and_then(|s| s.chars().next())
            {
                out.push(Ok(c));
                rest = rest.get(len..).unwrap_or_default();
                continue;
            }
        }
        let byte = rest.first().copied().unwrap_or(0);
        if byte.is_ascii() {
            out.push(Ok(char::from(byte)));
        } else {
            out.push(Err(byte));
        }
        rest = rest.get(1..).unwrap_or_default();
    }
    out
}

/// Whether a character is printable as it is.
const fn printable(c: char) -> bool {
    !c.is_control()
}

/// The characters the shell styles quote for.
const fn shell_special(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '!'
            | '"'
            | '#'
            | '$'
            | '&'
            | '\''
            | '('
            | ')'
            | '*'
            | ';'
            | '<'
            | '='
            | '>'
            | '?'
            | '['
            | '\\'
            | ']'
            | '^'
            | '`'
            | '{'
            | '|'
            | '}'
            | '~'
    ) || c.is_control()
}

/// `name` quoted in `style`; `utf8` says whether the locale's charset is UTF-8, which
/// leaves printable multibyte characters as they are; `extra` are more characters to
/// quote (`--quote-chars`).
pub fn quote(name: &[u8], style: Style, utf8: bool, extra: &[u8]) -> String {
    let chars = pieces(name, utf8);
    let needs = |c: char| extra.contains(&u8::try_from(u32::from(c)).unwrap_or(0)) && c.is_ascii();
    match style {
        Style::Literal => String::from_utf8_lossy(name).into_owned(),
        Style::Escape | Style::C | Style::CMaybe | Style::Locale | Style::Clocale => {
            let quoted = !matches!(style, Style::Escape);
            let mut out = String::new();
            let mut needed = false;
            for piece in &chars {
                match piece {
                    Ok('\\') => {
                        out.push_str("\\\\");
                        needed = true;
                    }
                    Ok('"') if quoted && !matches!(style, Style::Locale) => {
                        out.push_str("\\\"");
                        needed = true;
                    }
                    Ok(c) if printable(*c) && !needs(*c) => out.push(*c),
                    Ok(c) if !printable(*c) => {
                        let mut buffer = [0_u8; 4];
                        for byte in c.encode_utf8(&mut buffer).bytes() {
                            c_escape(byte, &mut out);
                        }
                        needed = true;
                    }
                    Ok(c) => {
                        out.push('\\');
                        out.push(*c);
                        needed = true;
                    }
                    Err(byte) => {
                        c_escape(*byte, &mut out);
                        needed = true;
                    }
                }
            }
            match style {
                Style::C | Style::Clocale => format!("\"{out}\""),
                Style::Locale => format!("'{out}'"),
                Style::CMaybe if needed => format!("\"{out}\""),
                _ => out,
            }
        }
        Style::Shell | Style::ShellAlways | Style::ShellEscape | Style::ShellEscapeAlways => {
            let always = matches!(style, Style::ShellAlways | Style::ShellEscapeAlways);
            let escape = matches!(style, Style::ShellEscape | Style::ShellEscapeAlways);
            let needed = always
                || name.is_empty()
                || chars
                    .iter()
                    .any(|p| matches!(p, Ok(c) if shell_special(*c) || needs(*c)) || p.is_err());
            if !needed {
                return String::from_utf8_lossy(name).into_owned();
            }
            let mut out = String::from("'");
            for piece in &chars {
                match piece {
                    Ok('\'') => out.push_str("'\\''"),
                    Ok(c) if escape && !printable(*c) => {
                        out.push_str("'$'");
                        let mut buffer = [0_u8; 4];
                        for byte in c.encode_utf8(&mut buffer).bytes() {
                            c_escape(byte, &mut out);
                        }
                        out.push_str("''");
                    }
                    Ok(c) => out.push(*c),
                    Err(byte) if escape => {
                        out.push_str("'$'");
                        c_escape(*byte, &mut out);
                        out.push_str("''");
                    }
                    Err(byte) => out.push(char::from(*byte)),
                }
            }
            out.push('\'');
            out
        }
    }
}

/// The mode string of a listing: the kind's letter and nine permissions, with set-id
/// and sticky as `s`, `S`, `t`, `T`; tar's own letters for its kinds.
pub fn mode_string(kind: Kind, mode: u32) -> String {
    let letter = match kind {
        Kind::File => '-',
        Kind::Dir => 'd',
        Kind::Symlink => 'l',
        Kind::HardLink => 'h',
        Kind::Char => 'c',
        Kind::Block => 'b',
        Kind::Fifo => 'p',
        Kind::Other(b'7') => 'C',
        Kind::Other(b'V') => 'V',
        Kind::Other(b'M') => 'M',
        Kind::Other(b'N') => 'N',
        Kind::Other(b'S') => 'S',
        Kind::Other(b'D') => 'd',
        Kind::Other(_) => '?',
    };
    let mut out = String::with_capacity(10);
    out.push(letter);
    let bit = |m: u32, c: char| if mode & m != 0 { c } else { '-' };
    let special = |exec: bool, set: bool, on: char, off: char| match (exec, set) {
        (true, true) => on,
        (false, true) => off,
        (true, false) => 'x',
        (false, false) => '-',
    };
    out.push(bit(0o400, 'r'));
    out.push(bit(0o200, 'w'));
    out.push(special(mode & 0o100 != 0, mode & 0o4000 != 0, 's', 'S'));
    out.push(bit(0o040, 'r'));
    out.push(bit(0o020, 'w'));
    out.push(special(mode & 0o010 != 0, mode & 0o2000 != 0, 's', 'S'));
    out.push(bit(0o004, 'r'));
    out.push(bit(0o002, 'w'));
    out.push(special(mode & 0o001 != 0, mode & 0o1000 != 0, 't', 'T'));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_quoted_as_gnu_tar_quotes_them() {
        assert_eq!(
            quote(b"q/back\\slash", Style::Escape, false, b""),
            "q/back\\\\slash"
        );
        assert_eq!(
            quote(b"q/new\nline", Style::Escape, false, b""),
            "q/new\\nline"
        );
        assert_eq!(
            quote("q/é".as_bytes(), Style::Escape, false, b""),
            "q/\\303\\251"
        );
        assert_eq!(quote("q/é".as_bytes(), Style::Escape, true, b""), "q/é");
        assert_eq!(
            quote(b"q/back\\slash", Style::Shell, false, b""),
            "'q/back\\slash'"
        );
        assert_eq!(quote(b"q/", Style::Shell, false, b""), "q/");
        assert_eq!(
            quote(b"q/tab\ttab", Style::C, false, b""),
            "\"q/tab\\ttab\""
        );
        assert_eq!(quote(b"it's", Style::ShellAlways, false, b""), "'it'\\''s'");
    }

    #[test]
    fn modes_read_as_tar_lists_them() {
        assert_eq!(mode_string(Kind::File, 0o644), "-rw-r--r--");
        assert_eq!(mode_string(Kind::HardLink, 0o644), "hrw-r--r--");
        assert_eq!(mode_string(Kind::Dir, 0o1777), "drwxrwxrwt");
        assert_eq!(mode_string(Kind::File, 0o4644), "-rwSr--r--");
    }
}
