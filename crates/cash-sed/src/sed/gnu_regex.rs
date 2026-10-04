// Check a regular expression's syntax as GNU sed's regcomp does
//
// SPDX-License-Identifier: MIT
//
// This file is part of the uutils sed package.
// It is licensed under the MIT License.
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! GNU sed compiles its regular expressions with glibc's (or gnulib's) `regcomp`, and
//! reports a bad one in that library's words: "Unmatched ( or \(", "Invalid preceding
//! regular expression". cash's sed compiles them with another engine, whose errors are
//! worded its own way, and which takes some expressions `regcomp` refuses (`a\{1\}*`,
//! `[[:foo:]]`) and refuses some it takes (`*a`, a leading `*` being a literal one in a
//! BRE). So an expression is first read here the way `regcomp` reads it, following the
//! structure of glibc's `regcomp.c` (`parse_reg_exp`, `parse_branch`,
//! `parse_expression`, `parse_dup_op`, `parse_bracket_exp`), and its first error is
//! GNU's.

use crate::sed::command::RE_DUP_MAX;

const EBRACK: &str = "Unmatched [, [^, [:, [., or [=";
const EPAREN: &str = "Unmatched ( or \\(";
const ERPAREN: &str = "Unmatched ) or \\)";
const EBRACE: &str = "Unmatched \\{";
const BADBR: &str = "Invalid content of \\{\\}";
const ESIZE: &str = "Regular expression too big";
const BADRPT: &str = "Invalid preceding regular expression";
const ESUBREG: &str = "Invalid back reference";
const EESCAPE: &str = "Trailing backslash";
const ERANGE: &str = "Invalid range end";
const ECTYPE: &str = "Invalid character class name";
const ECOLLATE: &str = "Invalid collation character";
const BADPAT: &str = "Invalid regular expression";

/// The names `[:name:]` may take in a bracket expression.
const CLASS_NAMES: [&[u8]; 12] = [
    b"alpha", b"upper", b"lower", b"digit", b"xdigit", b"space", b"print", b"punct", b"graph",
    b"cntrl", b"blank", b"alnum",
];

/// How GNU sed has `regcomp` read an expression.
#[derive(Clone, Copy, Debug, Default)]
pub struct Syntax {
    /// `-E`: an extended regular expression.
    pub extended: bool,
    /// `--posix`: `\|`, `\+` and `\?` are ordinary in a basic expression, GNU's
    /// operators `\w`, `\<`, ... are ordinary everywhere, and an unmatched `)` is an
    /// ordinary character.
    pub posix: bool,
    /// Whether multibyte characters are read as UTF-8, for the ends of a range.
    pub utf8: bool,
}

/// The first error GNU sed reports for `pattern`, or `None` when it takes it.
///
/// The pattern is the one sed's own parser made of the script: escapes such as `\n`
/// decoded, and `\{,n\}` written `\{0,n\}`. `\A` and `\z`, which that parser makes of
/// GNU's `` \` `` and `\'`, are read as ordinary characters: a user's `\A` is one in
/// GNU sed, and taking it for an anchor could refuse what GNU sed takes.
pub fn syntax_error(pattern: &[u8], syntax: Syntax) -> Option<&'static str> {
    let mut parser = Parser {
        pattern,
        syntax,
        pos: 0,
        token: Token::default(),
        groups: 0,
        completed: 0,
    };
    parser.token = parser.peek(0, true);
    parser.reg_exp(0).err()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kind {
    #[default]
    End,
    Char,
    Anchor,
    Open,
    Close,
    Alt,
    Star,
    Plus,
    Question,
    OpenInterval,
    CloseInterval,
    BackRef,
    Bracket,
    TrailingBackslash,
}

/// A token of the expression: its kind, the character it is (the one after the
/// backslash for an escape), and its length in bytes.
#[derive(Clone, Copy, Debug, Default)]
struct Token {
    kind: Kind,
    c: u8,
    len: usize,
}

/// A number in an interval: none given, one that is not a number, or its value
/// (capped one above `RE_DUP_MAX`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Number {
    Missing,
    Bad,
    Value(usize),
}

/// An element of a bracket expression.
enum Element {
    Char(u32),
    Collating(Vec<u8>),
    Equivalence(Vec<u8>),
    Class(Vec<u8>),
}

/// A token inside a bracket expression.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BracketToken {
    Char(u32),
    OpenCollating,
    OpenEquivalence,
    OpenClass,
    Range,
    Close,
    End,
}

type Parsed = Result<(), &'static str>;

struct Parser<'a> {
    pattern: &'a [u8],
    syntax: Syntax,
    /// Where the current token starts.
    pos: usize,
    token: Token,
    /// Groups opened so far.
    groups: usize,
    /// The groups (`\1` to `\9`) already closed, which a back reference may name.
    completed: u32,
}

impl Parser<'_> {
    fn byte(&self, i: usize) -> Option<u8> {
        self.pattern.get(i).copied()
    }

    /// The token at `i`. `caret_here` says a `^` there is an anchor in a basic
    /// expression, as at its start and after `\(` and `\|`.
    fn peek(&self, i: usize, caret_here: bool) -> Token {
        let Some(c) = self.byte(i) else {
            return Token::default();
        };
        let Syntax {
            extended, posix, ..
        } = self.syntax;
        let token = |kind, len| Token { kind, c, len };
        if c == b'\\' {
            let Some(c2) = self.byte(i + 1) else {
                return token(Kind::TrailingBackslash, 1);
            };
            let kind = match c2 {
                b'1'..=b'9' => Kind::BackRef,
                b'<' | b'>' | b'b' | b'B' | b'`' | b'\'' if !posix => Kind::Anchor,
                b'(' if !extended => Kind::Open,
                b')' if !extended => Kind::Close,
                b'{' if !extended => Kind::OpenInterval,
                b'}' if !extended => Kind::CloseInterval,
                b'|' if !extended && !posix => Kind::Alt,
                b'+' if !extended && !posix => Kind::Plus,
                b'?' if !extended && !posix => Kind::Question,
                _ => Kind::Char,
            };
            return Token {
                kind,
                c: c2,
                len: 2,
            };
        }
        let kind = match c {
            b'[' => Kind::Bracket,
            b'*' => Kind::Star,
            b'(' if extended => Kind::Open,
            b')' if extended => Kind::Close,
            b'|' if extended => Kind::Alt,
            b'+' if extended => Kind::Plus,
            b'?' if extended => Kind::Question,
            b'{' if extended => Kind::OpenInterval,
            b'}' if extended => Kind::CloseInterval,
            b'^' if extended || i == 0 || caret_here => Kind::Anchor,
            b'$' if extended || i + 1 == self.pattern.len() => Kind::Anchor,
            // In a basic expression `$` is an anchor before `\)` and `\|` too.
            b'$' if matches!(self.peek(i + 1, false).kind, Kind::Alt | Kind::Close) => Kind::Anchor,
            _ => Kind::Char,
        };
        token(kind, 1)
    }

    fn fetch(&mut self, caret_here: bool) {
        self.pos += self.token.len;
        self.token = self.peek(self.pos, caret_here);
    }

    /// Alternatives: `parse_reg_exp`.
    fn reg_exp(&mut self, nest: usize) -> Parsed {
        self.branch(nest)?;
        while self.token.kind == Kind::Alt {
            self.fetch(true);
            if !self.ends_branch(nest) {
                self.branch(nest)?;
            }
        }
        Ok(())
    }

    fn ends_branch(&self, nest: usize) -> bool {
        matches!(self.token.kind, Kind::Alt | Kind::End)
            || (nest > 0 && self.token.kind == Kind::Close)
    }

    /// A sequence of expressions: `parse_branch`.
    fn branch(&mut self, nest: usize) -> Parsed {
        self.expression(nest)?;
        while !self.ends_branch(nest) {
            self.expression(nest)?;
        }
        Ok(())
    }

    /// One atom and the repetitions after it: `parse_expression`.
    fn expression(&mut self, nest: usize) -> Parsed {
        let extended = self.syntax.extended;
        match self.token.kind {
            Kind::Alt | Kind::End => return Ok(()),
            Kind::Char | Kind::CloseInterval => self.fetch(false),
            Kind::Open => self.sub_exp(nest + 1)?,
            Kind::Bracket => self.bracket()?,
            Kind::BackRef => {
                let group = u32::from(self.token.c - b'1');
                if self.completed & (1 << group) == 0 {
                    return Err(ESUBREG);
                }
                self.fetch(false);
            }
            // A basic expression cannot start with an interval.
            Kind::OpenInterval if !extended => return Err(BADRPT),
            // An extended one cannot start with any repetition; in a basic one a `*`,
            // `\+` or `\?` there is an ordinary character.
            Kind::OpenInterval | Kind::Star | Kind::Plus | Kind::Question => {
                if extended {
                    return Err(BADRPT);
                }
                self.fetch(false);
            }
            // An unmatched `)` is an error unless --posix makes it ordinary.
            Kind::Close => {
                if !self.syntax.posix {
                    return Err(ERPAREN);
                }
                self.fetch(false);
            }
            // Nothing repeats an anchor: what follows starts an expression of its own.
            Kind::Anchor => {
                self.fetch(false);
                return Ok(());
            }
            Kind::TrailingBackslash => return Err(EESCAPE),
        }
        while matches!(
            self.token.kind,
            Kind::Star | Kind::Plus | Kind::Question | Kind::OpenInterval
        ) {
            self.dup_op()?;
            // A basic expression takes no `*` or interval after a repetition.
            if !extended && matches!(self.token.kind, Kind::Star | Kind::OpenInterval) {
                return Err(BADRPT);
            }
        }
        Ok(())
    }

    /// A group, the current token its `(`: `parse_sub_exp`.
    fn sub_exp(&mut self, nest: usize) -> Parsed {
        let group = self.groups;
        self.groups += 1;
        self.fetch(true);
        if self.token.kind != Kind::Close {
            self.reg_exp(nest)?;
            if self.token.kind != Kind::Close {
                return Err(EPAREN);
            }
        }
        if group < 9 {
            self.completed |= 1 << group;
        }
        self.fetch(false);
        Ok(())
    }

    /// A repetition, the current token its operator: `parse_dup_op`.
    fn dup_op(&mut self) -> Parsed {
        if self.token.kind == Kind::OpenInterval {
            let mut start = self.fetch_number();
            if start == Number::Missing {
                // `{,n}` is `{0,n}`, and `{}` an error.
                if self.token.kind == Kind::Char && self.token.c == b',' {
                    start = Number::Value(0);
                } else {
                    return Err(BADBR);
                }
            }
            let end = match start {
                Number::Bad => Number::Bad,
                _ if self.token.kind == Kind::CloseInterval => start,
                _ if self.token.kind == Kind::Char && self.token.c == b',' => self.fetch_number(),
                _ => Number::Bad,
            };
            if start == Number::Bad || end == Number::Bad {
                return Err(if self.token.kind == Kind::End {
                    EBRACE
                } else {
                    BADBR
                });
            }
            let (Number::Value(low), high) = (start, end) else {
                return Err(BADBR);
            };
            let reversed = matches!(high, Number::Value(high) if low > high);
            if reversed || self.token.kind != Kind::CloseInterval {
                return Err(BADBR);
            }
            let bound = match high {
                Number::Value(high) => high,
                _ => low,
            };
            if bound > RE_DUP_MAX {
                return Err(ESIZE);
            }
        }
        self.fetch(false);
        Ok(())
    }

    /// The number in an interval, up to the `,` or the closing brace: `fetch_number`.
    fn fetch_number(&mut self) -> Number {
        let mut number = Number::Missing;
        loop {
            self.fetch(false);
            let Token { kind, c, .. } = self.token;
            if kind == Kind::End {
                return Number::Bad;
            }
            if kind == Kind::CloseInterval || c == b',' {
                return number;
            }
            number = match number {
                _ if kind != Kind::Char || !c.is_ascii_digit() => Number::Bad,
                Number::Bad => Number::Bad,
                Number::Missing => Number::Value(usize::from(c - b'0')),
                Number::Value(n) => {
                    Number::Value((n * 10 + usize::from(c - b'0')).min(RE_DUP_MAX + 1))
                }
            };
        }
    }

    /// The bracket token at `i` and its length: `peek_token_bracket`.
    fn peek_bracket(&self, i: usize) -> (BracketToken, usize) {
        let Some(c) = self.byte(i) else {
            return (BracketToken::End, 0);
        };
        if c == b'[' {
            match self.byte(i + 1) {
                Some(b'.') => return (BracketToken::OpenCollating, 2),
                Some(b'=') => return (BracketToken::OpenEquivalence, 2),
                Some(b':') => return (BracketToken::OpenClass, 2),
                _ => {}
            }
        }
        match c {
            b'-' => (BracketToken::Range, 1),
            b']' => (BracketToken::Close, 1),
            _ => {
                let (ch, len) = self.char_at(i);
                (BracketToken::Char(ch), len)
            }
        }
    }

    /// The character at `i` and its length: a UTF-8 one when the expression is read as
    /// UTF-8 and the bytes are one, a byte otherwise.
    fn char_at(&self, i: usize) -> (u32, usize) {
        let rest = self.pattern.get(i..).unwrap_or_default();
        let first = rest.first().copied().unwrap_or_default();
        if self.syntax.utf8 && first >= 0x80 {
            let valid = match std::str::from_utf8(rest) {
                Ok(text) => text,
                // The valid prefix, which the error says ends where.
                Err(e) => std::str::from_utf8(rest.get(..e.valid_up_to()).unwrap_or_default())
                    .unwrap_or_default(),
            };
            if let Some(ch) = valid.chars().next() {
                return (u32::from(ch), ch.len_utf8());
            }
        }
        (u32::from(first), 1)
    }

    /// A bracket expression, the current token its `[`: `parse_bracket_exp`. Leaves the
    /// token after its `]` current.
    fn bracket(&mut self) -> Parsed {
        let mut i = self.pos + 1;
        let (mut token, mut len) = self.peek_bracket(i);
        if token == BracketToken::End {
            return Err(BADPAT);
        }
        if self.byte(i) == Some(b'^') {
            i += 1;
            (token, len) = self.peek_bracket(i);
            if token == BracketToken::End {
                return Err(BADPAT);
            }
        }
        // A `]` first is an ordinary character.
        if token == BracketToken::Close {
            token = BracketToken::Char(u32::from(b']'));
        }
        let mut first = true;
        loop {
            let start = self.bracket_element(&mut i, token, len, first)?;
            first = false;
            (token, len) = self.peek_bracket(i);
            let mut range_end = None;
            if !matches!(start, Element::Class(_) | Element::Equivalence(_)) {
                if token == BracketToken::End {
                    return Err(EBRACK);
                }
                if token == BracketToken::Range {
                    let (token2, len2) = self.peek_bracket(i + len);
                    match token2 {
                        BracketToken::End => return Err(EBRACK),
                        // A `-` last is an ordinary character.
                        BracketToken::Close => token = BracketToken::Char(u32::from(b'-')),
                        _ => {
                            i += len;
                            range_end = Some((token2, len2));
                        }
                    }
                }
            }
            if let Some((token2, len2)) = range_end {
                let end = self.bracket_element(&mut i, token2, len2, true)?;
                (token, len) = self.peek_bracket(i);
                check_range(&start, &end)?;
            } else {
                match &start {
                    Element::Collating(name) | Element::Equivalence(name) if name.len() != 1 => {
                        return Err(ECOLLATE);
                    }
                    Element::Class(name) if !CLASS_NAMES.contains(&name.as_slice()) => {
                        return Err(ECTYPE);
                    }
                    _ => {}
                }
            }
            match token {
                BracketToken::End => return Err(EBRACK),
                BracketToken::Close => {
                    self.pos = i + len;
                    self.token = self.peek(self.pos, false);
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    /// One element of a bracket expression, from `i`, which it moves past it:
    /// `parse_bracket_element`.
    fn bracket_element(
        &self,
        i: &mut usize,
        token: BracketToken,
        len: usize,
        accept_hyphen: bool,
    ) -> Result<Element, &'static str> {
        let at = *i;
        *i += len;
        let delimiter = match token {
            BracketToken::OpenCollating => b'.',
            BracketToken::OpenEquivalence => b'=',
            BracketToken::OpenClass => b':',
            BracketToken::Range => {
                // A `-` is a range's but before the closing `]`.
                if !accept_hyphen && self.peek_bracket(*i).0 != BracketToken::Close {
                    return Err(ERANGE);
                }
                return Ok(Element::Char(u32::from(b'-')));
            }
            BracketToken::Char(ch) => return Ok(Element::Char(ch)),
            BracketToken::Close | BracketToken::End => {
                return Ok(Element::Char(u32::from(self.byte(at).unwrap_or_default())));
            }
        };
        // The name, up to the delimiter and `]`: `parse_bracket_symbol`.
        let mut name = Vec::new();
        loop {
            // glibc's buffer for the name holds 32 bytes.
            if name.len() >= 32 {
                return Err(EBRACK);
            }
            let Some(ch) = self.byte(*i) else {
                return Err(EBRACK);
            };
            *i += 1;
            let Some(next) = self.byte(*i) else {
                return Err(EBRACK);
            };
            if ch == delimiter && next == b']' {
                *i += 1;
                break;
            }
            name.push(ch);
        }
        Ok(match token {
            BracketToken::OpenCollating => Element::Collating(name),
            BracketToken::OpenEquivalence => Element::Equivalence(name),
            _ => Element::Class(name),
        })
    }
}

/// Whether `start-end` is a range `regcomp` takes: `build_range_exp`.
fn check_range(start: &Element, end: &Element) -> Parsed {
    let point = |element: &Element| match element {
        Element::Char(ch) => Ok(*ch),
        Element::Collating(name) if name.len() == 1 => {
            Ok(u32::from(name.first().copied().unwrap_or_default()))
        }
        Element::Collating(_) => Err(ECOLLATE),
        Element::Equivalence(_) | Element::Class(_) => Err(ERANGE),
    };
    if matches!(start, Element::Equivalence(_) | Element::Class(_))
        || matches!(end, Element::Equivalence(_) | Element::Class(_))
    {
        return Err(ERANGE);
    }
    if point(start)? > point(end)? {
        return Err(ERANGE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bre(pattern: &str) -> Option<&'static str> {
        syntax_error(
            pattern.as_bytes(),
            Syntax {
                utf8: true,
                ..Syntax::default()
            },
        )
    }

    fn ere(pattern: &str) -> Option<&'static str> {
        syntax_error(
            pattern.as_bytes(),
            Syntax {
                extended: true,
                utf8: true,
                ..Syntax::default()
            },
        )
    }

    fn posix_bre(pattern: &str) -> Option<&'static str> {
        syntax_error(
            pattern.as_bytes(),
            Syntax {
                posix: true,
                utf8: true,
                ..Syntax::default()
            },
        )
    }

    // Each case was checked against GNU sed 4.9.
    #[test]
    fn test_basic_expressions() {
        for (pattern, error) in [
            ("a", None),
            ("\\(a", Some(EPAREN)),
            ("\\(", Some(EPAREN)),
            ("a\\)", Some(ERPAREN)),
            ("\\)", Some(ERPAREN)),
            ("\\(a\\)\\)", Some(ERPAREN)),
            ("*a", None),
            ("\\(*a\\)", None),
            ("^*a", None),
            ("a\\|*b", None),
            ("\\+a", None),
            ("\\?a", None),
            ("\\{1\\}", Some(BADRPT)),
            ("a**", Some(BADRPT)),
            ("a*\\{2\\}", Some(BADRPT)),
            ("a\\{1\\}*", Some(BADRPT)),
            ("a\\{1\\}\\{2\\}", Some(BADRPT)),
            ("a\\+*", Some(BADRPT)),
            ("a*\\+", None),
            ("a\\{1\\}\\+", None),
            ("a\\{1", Some(EBRACE)),
            ("a\\{1,x", Some(EBRACE)),
            ("a\\{1,2", Some(EBRACE)),
            ("a\\{1}", Some(EBRACE)),
            ("a\\{x\\}", Some(BADBR)),
            ("a\\{\\}", Some(BADBR)),
            ("a\\{2,1\\}", Some(BADBR)),
            ("a\\{1,2,3\\}", Some(BADBR)),
            ("a\\{99999,1\\}", Some(BADBR)),
            ("a\\{1,99999\\}", Some(ESIZE)),
            ("a\\{0,2\\}", None),
            (".\\{0,\\}", None),
            ("\\(a\\)\\2", Some(ESUBREG)),
            ("\\(a\\1\\)", Some(ESUBREG)),
            ("\\(a\\)\\(b\\)\\2\\1", None),
            ("\\(a\\|b\\)\\1", None),
            ("\\(\\)", None),
            ("a\\", Some(EESCAPE)),
            ("\\b*", None),
            ("\\(a$\\)", None),
        ] {
            assert_eq!(bre(pattern), error, "{pattern}");
        }
    }

    #[test]
    fn test_extended_expressions() {
        for (pattern, error) in [
            ("(a", Some(EPAREN)),
            ("a)", Some(ERPAREN)),
            ("(a))", Some(ERPAREN)),
            ("*a", Some(BADRPT)),
            ("a|*b", Some(BADRPT)),
            ("a|+", Some(BADRPT)),
            ("a|?", Some(BADRPT)),
            ("(*a)", Some(BADRPT)),
            ("^*a", Some(BADRPT)),
            ("a$*", Some(BADRPT)),
            ("\\<*a", Some(BADRPT)),
            ("{", Some(BADRPT)),
            ("a{", Some(EBRACE)),
            ("a{1", Some(EBRACE)),
            ("a{x}", Some(BADBR)),
            ("a{}", Some(BADBR)),
            ("a{2,1}", Some(BADBR)),
            ("a{1,2,3}", Some(BADBR)),
            ("a{32768}", Some(ESIZE)),
            ("a{32767}", None),
            ("a**", None),
            ("a+*", None),
            ("a{1}{2}", None),
            ("a{1}*", None),
            ("a{0,2}", None),
            ("()", None),
            ("()*", None),
            ("(|a)", None),
            ("a||b", None),
            ("a|", None),
            ("(a)\\1", None),
            ("(a)\\2", Some(ESUBREG)),
            ("\\(", None),
            ("a\\{1", None),
        ] {
            assert_eq!(ere(pattern), error, "{pattern}");
        }
    }

    #[test]
    fn test_bracket_expressions() {
        for (pattern, error) in [
            ("[a]", None),
            ("[]a]", None),
            ("[^]a]", None),
            ("[a-]", None),
            ("[a-a]", None),
            ("[b-a]", Some(ERANGE)),
            ("[b-a]\\(", Some(ERANGE)),
            ("\\(a\\)[b-a]", Some(ERANGE)),
            ("[[:alpha:]]", None),
            ("[[:foo:]]", Some(ECTYPE)),
            ("[[:alpha:][:foo:]]", Some(ECTYPE)),
            ("[[:alpha:]-z]", Some(ERANGE)),
            ("[[.a.]]", None),
            ("[[.-.]]", None),
            ("[[.space.]]", Some(ECOLLATE)),
            ("[[.foo.]]", Some(ECOLLATE)),
            ("[a-[.z.]]", None),
            ("[[.z.]-a]", Some(ERANGE)),
            ("[[=a=]b]", None),
            ("[[=ab=]]", Some(ECOLLATE)),
            ("[α-ω]", None),
            ("[ω-α]", Some(ERANGE)),
            ("[a]\\{\\}", Some(BADBR)),
        ] {
            assert_eq!(bre(pattern), error, "{pattern}");
        }
    }

    // --posix: `\|`, `\+` and `\?` are ordinary in a basic expression, and so is an
    // unmatched `)`.
    #[test]
    fn test_posix_expressions() {
        assert_eq!(posix_bre("a\\{1\\}*"), Some(BADRPT));
        assert_eq!(posix_bre("a\\+*"), None);
        assert_eq!(posix_bre("a\\)"), None);
        assert_eq!(posix_bre("\\(a"), Some(EPAREN));
    }
}
