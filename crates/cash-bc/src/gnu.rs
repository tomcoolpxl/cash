//
// Copyright (c) 2026 Cash project contributors.
// SPDX-License-Identifier: MIT
//

//! Naming the GNU bc extension a failed program used.
//!
//! This bc is POSIX bc (research/busybox-gap-analysis.md, Q4). Scripts written for GNU
//! bc fail here with the parser's own error — "expected a newline, found 'r'" for
//! `print` — which says nothing about why. When input fails to parse, it is scanned for
//! the first construct only GNU bc has, and a note names it.

/// Words only GNU bc gives a meaning; POSIX bc would read each as single letters.
const GNU_WORDS: &[&str] = &[
    "print", "read", "else", "halt", "last", "limits", "continue", "warranty",
];

/// Words POSIX bc itself defines.
const POSIX_WORDS: &[&str] = &[
    "auto", "break", "define", "for", "ibase", "if", "length", "obase", "quit", "return", "scale",
    "sqrt", "while",
];

/// A note naming the first GNU-only construct in `text`, outside strings and comments.
pub fn extension_note(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        let next = chars.get(i + 1).copied();
        match c {
            '"' => {
                i += 1;
                while chars.get(i).is_some_and(|&c| c != '"') {
                    i += 1;
                }
            }
            '/' if next == Some('*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 1;
            }
            '#' => {
                return Some(
                    "note: `#` comments are a GNU bc extension; this bc is POSIX bc, whose \
                     comments are /* ... */"
                        .to_owned(),
                );
            }
            '&' if next == Some('&') => return Some(note("`&&`")),
            '|' if next == Some('|') => return Some(note("`||`")),
            '!' if next != Some('=') => return Some(note("`!` (logical not)")),
            'a'..='z' | '_' => {
                let start = i;
                while chars
                    .get(i)
                    .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if GNU_WORDS.contains(&word.as_str()) {
                    return Some(note(&format!("`{word}`")));
                }
                if word.chars().count() > 1 && !POSIX_WORDS.contains(&word.as_str()) {
                    return Some(format!(
                        "note: multi-letter names such as `{word}` are a GNU bc extension; \
                         in POSIX bc, which this is, a name is one lowercase letter"
                    ));
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn note(what: &str) -> String {
    format!("note: {what} is a GNU bc extension; this bc is POSIX bc")
}

#[cfg(test)]
mod tests {
    use super::extension_note;

    fn named(text: &str) -> String {
        extension_note(text).unwrap_or_default()
    }

    #[test]
    fn gnu_keywords_operators_and_comments_are_named() {
        assert!(named("print \"x\"\n").contains("`print`"));
        assert!(named("x=read()\n").contains("`read`"));
        assert!(named("if (1) 1 else 2\n").contains("`else`"));
        assert!(named("1 && 1\n").contains("`&&`"));
        assert!(named("0 || 1\n").contains("`||`"));
        assert!(named("!0\n").contains("`!`"));
        assert!(named("1 # c\n").contains("`#` comments"));
        assert!(named("total=5\n").contains("`total`"));
    }

    #[test]
    fn posix_programs_and_quoted_text_get_no_note() {
        for text in [
            "define f(x) { auto y; y = x; return (y) }\n",
            "scale=5; sqrt(2); length(3.14)\n",
            "\"print this && that # here\"\n",
            "/* print total || # */ 1\n",
            "if (a != b) 1\n",
            "ibase=16; FF\n",
        ] {
            assert_eq!(extension_note(text), None, "{text:?}");
        }
    }
}
