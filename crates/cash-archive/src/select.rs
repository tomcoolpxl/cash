//! Which members a name or a pattern selects: glibc's `fnmatch` with the flags GNU tar
//! and Info-ZIP's unzip differ on, and GNU tar's way of trying a pattern after each `/` when it is
//! not anchored.

/// How a pattern matches.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Flags {
    /// `*`, `?` and `[...]` are wildcards; otherwise the pattern is a literal name.
    pub wildcards: bool,
    /// The wildcards do not match `/` (`FNM_PATHNAME`).
    pub pathname: bool,
    /// A backslash is an ordinary character (`FNM_NOESCAPE`).
    pub noescape: bool,
    /// The pattern also matches what is under the folder it names
    /// (`FNM_LEADING_DIR`).
    pub leading_dir: bool,
    /// Upper and lower case are the same (`FNM_CASEFOLD`).
    pub casefold: bool,
    /// The pattern must match from the start of the name; otherwise it is also tried
    /// after each `/`.
    pub anchored: bool,
}

const fn fold(byte: u8, flags: Flags) -> u8 {
    if flags.casefold {
        byte.to_ascii_lowercase()
    } else {
        byte
    }
}

/// A bracket expression at the start of `pattern` (after its `[`) against `byte`:
/// whether it matches, and the pattern after its `]`; `None` when it has no `]`.
fn bracket(pattern: &[u8], byte: u8, flags: Flags) -> Option<(bool, &[u8])> {
    let mut rest = pattern;
    let negate = matches!(rest.first(), Some(b'!' | b'^'));
    if negate {
        rest = rest.get(1..)?;
    }
    let mut matched = false;
    let mut first = true;
    loop {
        let &c = rest.first()?;
        if c == b']' && !first {
            rest = rest.get(1..)?;
            break;
        }
        first = false;
        if c == b'[' && rest.get(1) == Some(&b':') {
            if let Some(end) = rest.windows(2).position(|w| w == b":]") {
                let class = rest.get(2..end).unwrap_or_default();
                let hit = match class {
                    b"alpha" => byte.is_ascii_alphabetic(),
                    b"digit" => byte.is_ascii_digit(),
                    b"alnum" => byte.is_ascii_alphanumeric(),
                    b"upper" => {
                        byte.is_ascii_uppercase() || (flags.casefold && byte.is_ascii_alphabetic())
                    }
                    b"lower" => {
                        byte.is_ascii_lowercase() || (flags.casefold && byte.is_ascii_alphabetic())
                    }
                    b"space" => byte.is_ascii_whitespace(),
                    b"blank" => byte == b' ' || byte == b'\t',
                    b"punct" => byte.is_ascii_punctuation(),
                    b"print" => (0x20..0x7f).contains(&byte),
                    b"graph" => byte.is_ascii_graphic(),
                    b"cntrl" => byte.is_ascii_control(),
                    b"xdigit" => byte.is_ascii_hexdigit(),
                    _ => false,
                };
                matched |= hit;
                rest = rest.get(end + 2..)?;
                continue;
            }
        }
        let mut low = c;
        rest = rest.get(1..)?;
        if low == b'\\' && !flags.noescape {
            low = *rest.first()?;
            rest = rest.get(1..)?;
        }
        if rest.first() == Some(&b'-') && rest.get(1).is_some_and(|b| *b != b']') {
            let mut high = *rest.get(1)?;
            rest = rest.get(2..)?;
            if high == b'\\' && !flags.noescape {
                high = *rest.first()?;
                rest = rest.get(1..)?;
            }
            let b = fold(byte, flags);
            if (fold(low, flags)..=fold(high, flags)).contains(&b) || (low..=high).contains(&byte) {
                matched = true;
            }
        } else if fold(low, flags) == fold(byte, flags) {
            matched = true;
        }
    }
    if flags.pathname && byte == b'/' {
        return Some((false, rest));
    }
    Some((matched != negate, rest))
}

/// glibc's `fnmatch` of `pattern` against all of `name`.
pub fn fnmatch(pattern: &[u8], name: &[u8], flags: Flags) -> bool {
    let at_end =
        |rest: &[u8]| rest.is_empty() || (flags.leading_dir && rest.first() == Some(&b'/'));
    let Some(&p) = pattern.first() else {
        return at_end(name);
    };
    let rest = pattern.get(1..).unwrap_or_default();
    match p {
        b'?' => match name.first() {
            Some(&b'/') if flags.pathname => false,
            Some(_) => fnmatch(rest, name.get(1..).unwrap_or_default(), flags),
            None => false,
        },
        b'*' => {
            let mut rest = rest;
            while rest.first() == Some(&b'*') {
                rest = rest.get(1..).unwrap_or_default();
            }
            if rest.is_empty() {
                return !flags.pathname || flags.leading_dir || !name.contains(&b'/');
            }
            for skip in 0..=name.len() {
                if flags.pathname && skip > 0 && name.get(skip - 1) == Some(&b'/') {
                    break;
                }
                if fnmatch(rest, name.get(skip..).unwrap_or_default(), flags) {
                    return true;
                }
            }
            false
        }
        b'[' => {
            let Some(&byte) = name.first() else {
                return false;
            };
            match bracket(rest, byte, flags) {
                Some((true, after)) => fnmatch(after, name.get(1..).unwrap_or_default(), flags),
                Some((false, _)) => false,
                None => byte == b'[' && fnmatch(rest, name.get(1..).unwrap_or_default(), flags),
            }
        }
        b'\\' if !flags.noescape => {
            let (literal, after) = match rest.first() {
                Some(&c) => (c, rest.get(1..).unwrap_or_default()),
                None => (b'\\', rest),
            };
            name.first()
                .is_some_and(|b| fold(*b, flags) == fold(literal, flags))
                && fnmatch(after, name.get(1..).unwrap_or_default(), flags)
        }
        _ => {
            name.first()
                .is_some_and(|b| fold(*b, flags) == fold(p, flags))
                && fnmatch(rest, name.get(1..).unwrap_or_default(), flags)
        }
    }
}

/// A literal name against a member's: the same, or a folder above it when
/// `leading_dir`.
fn literal(pattern: &[u8], name: &[u8], flags: Flags) -> bool {
    let pattern = pattern.strip_suffix(b"/").unwrap_or(pattern);
    if name.len() < pattern.len() {
        return false;
    }
    let (head, tail) = name.split_at(pattern.len());
    let same = if flags.casefold {
        head.eq_ignore_ascii_case(pattern)
    } else {
        head == pattern
    };
    same && (tail.is_empty() || (flags.leading_dir && tail.first() == Some(&b'/')) || tail == b"/")
}

/// Whether `pattern` selects `name`, as GNU tar's `exclude_fnmatch` decides: with or
/// without wildcards, and when not anchored, tried again after each `/`.
pub fn matches(pattern: &[u8], name: &[u8], flags: Flags) -> bool {
    let one = |at: &[u8]| {
        if flags.wildcards {
            fnmatch(pattern, at, flags)
        } else {
            literal(pattern, at, flags)
        }
    };
    if one(name) {
        return true;
    }
    if !flags.anchored {
        for (i, byte) in name.iter().enumerate() {
            if *byte == b'/'
                && name.get(i + 1) != Some(&b'/')
                && one(name.get(i + 1..).unwrap_or_default())
            {
                return true;
            }
        }
    }
    false
}

/// Whether a name has characters that would be wildcards: GNU tar warns of them when
/// a name is not found and wildcards are off.
pub fn has_wildcards(pattern: &[u8]) -> bool {
    let mut escaped = false;
    for byte in pattern {
        if escaped {
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if matches!(byte, b'*' | b'?' | b'[') {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const WILD: Flags = Flags {
        wildcards: true,
        pathname: false,
        noescape: false,
        leading_dir: true,
        casefold: false,
        anchored: true,
    };

    #[test]
    fn wildcards_match_as_fnmatch_does() {
        assert!(fnmatch(b"*.txt", b"src/a.txt", WILD));
        assert!(!fnmatch(
            b"*.txt",
            b"src/a.txt",
            Flags {
                pathname: true,
                ..WILD
            }
        ));
        assert!(fnmatch(b"src/[ab].txt", b"src/a.txt", WILD));
        assert!(!fnmatch(b"src/[!ab].txt", b"src/a.txt", WILD));
        assert!(fnmatch(b"sub", b"sub/b.txt", WILD));
        assert!(fnmatch(
            b"S?C",
            b"src",
            Flags {
                casefold: true,
                ..WILD
            }
        ));
        assert!(fnmatch(b"a\\*", b"a*", WILD));
        assert!(!fnmatch(b"a\\*", b"ab", WILD));
    }

    #[test]
    fn names_select_as_gnu_tar_selects_them() {
        let names = Flags {
            wildcards: false,
            leading_dir: true,
            anchored: true,
            ..Flags::default()
        };
        assert!(matches(b"src/sub", b"src/sub/b.txt", names));
        assert!(matches(b"src/sub", b"src/sub/", names));
        assert!(!matches(b"src/su", b"src/sub/", names));
        assert!(!matches(b"b.txt", b"src/sub/b.txt", names));
        assert!(matches(
            b"b.txt",
            b"src/sub/b.txt",
            Flags {
                anchored: false,
                ..names
            }
        ));
        let exclude = Flags {
            wildcards: true,
            leading_dir: true,
            anchored: false,
            ..Flags::default()
        };
        assert!(matches(b"sub", b"src/sub", exclude));
        assert!(matches(b"*.txt", b"src/a.txt", exclude));
        assert!(has_wildcards(b"src/*.txt"));
        assert!(!has_wildcards(b"src/\\*.txt"));
    }
}
