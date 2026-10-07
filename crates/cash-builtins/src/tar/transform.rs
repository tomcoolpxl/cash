//! `--transform`: GNU tar's sed-like `s/REGEXP/REPLACE/FLAGS` expressions, the regular
//! expressions read as the bundled sed reads them (cash-sed's translation of GNU's
//! basic and extended syntaxes).

use std::fmt::Write as _;

/// What kind of name a transform applies to: tar's `r`, `s` and `h` flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Target {
    /// A member's own name.
    Regular,
    /// A symbolic link's target.
    Symlink,
    /// A hard link's target.
    Hardlink,
}

#[derive(Debug)]
enum Piece {
    Text(String),
    /// `&`, or `\N`.
    Group(usize),
    /// `\U`, `\L`, `\u`, `\l`, `\E`.
    Case(char),
}

#[derive(Debug)]
struct Transform {
    regex: fancy_regex::Regex,
    replacement: Vec<Piece>,
    global: bool,
    occurrence: usize,
    regular: bool,
    symlink: bool,
    hardlink: bool,
}

/// The transforms of every `--transform`, in order.
#[derive(Debug, Default)]
pub(super) struct Transforms {
    list: Vec<Transform>,
}

/// GNU's words for an expression it cannot read.
fn invalid(detail: &str) -> String {
    if detail.is_empty() {
        "Invalid transform expression".to_owned()
    } else {
        format!("Invalid transform expression: {detail}")
    }
}

/// Splits `text` at unescaped `delimiter`s: the pattern, the replacement and the flags.
fn split(text: &str, delimiter: char) -> Option<(String, String, &str)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some((_, next)) if next == delimiter => current.push(next),
                Some((_, next)) => {
                    current.push('\\');
                    current.push(next);
                }
                None => current.push('\\'),
            }
        } else if c == delimiter {
            parts.push(std::mem::take(&mut current));
            if parts.len() == 2 {
                return Some((
                    parts.remove(0),
                    parts.remove(0),
                    text.get(at + c.len_utf8()..).unwrap_or_default(),
                ));
            }
        } else {
            current.push(c);
        }
    }
    None
}

fn replacement(text: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut chars = text.chars();
    let flush = |literal: &mut String, pieces: &mut Vec<Piece>| {
        if !literal.is_empty() {
            pieces.push(Piece::Text(std::mem::take(literal)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '&' => {
                flush(&mut literal, &mut pieces);
                pieces.push(Piece::Group(0));
            }
            '\\' => match chars.next() {
                Some(d @ '0'..='9') => {
                    flush(&mut literal, &mut pieces);
                    pieces.push(Piece::Group(d.to_digit(10).map_or(0, |d| d as usize)));
                }
                Some(case @ ('U' | 'L' | 'u' | 'l' | 'E')) => {
                    flush(&mut literal, &mut pieces);
                    pieces.push(Piece::Case(case));
                }
                Some('n') => literal.push('\n'),
                Some('t') => literal.push('\t'),
                Some(other) => literal.push(other),
                None => literal.push('\\'),
            },
            other => literal.push(other),
        }
    }
    flush(&mut literal, &mut pieces);
    pieces
}

impl Transforms {
    /// Adds the expressions of one `--transform`.
    pub(super) fn add(&mut self, expression: &str) -> Result<(), String> {
        let mut rest = expression;
        while !rest.is_empty() {
            let Some(body) = rest.strip_prefix('s') else {
                return Err(invalid(""));
            };
            let delimiter = body.chars().next().ok_or_else(|| invalid(""))?;
            let (pattern, replace, after) = split(
                body.get(delimiter.len_utf8()..).unwrap_or_default(),
                delimiter,
            )
            .ok_or_else(|| invalid(""))?;
            let flags_end = after.find(';').unwrap_or(after.len());
            let flags = after.get(..flags_end).unwrap_or_default();
            rest = after.get(flags_end..).unwrap_or_default();
            rest = rest.strip_prefix(';').unwrap_or(rest);
            let mut transform = Transform {
                regex: fancy_regex::Regex::new("x").map_err(|_| invalid(""))?,
                replacement: replacement(&replace),
                global: false,
                occurrence: 0,
                regular: true,
                symlink: true,
                hardlink: true,
            };
            let mut extended = false;
            let mut ignore_case = false;
            let mut number = String::new();
            for flag in flags.chars() {
                match flag {
                    'g' => transform.global = true,
                    'i' => ignore_case = true,
                    'x' => extended = true,
                    'r' => transform.regular = true,
                    'R' => transform.regular = false,
                    's' => transform.symlink = true,
                    'S' => transform.symlink = false,
                    'h' => transform.hardlink = true,
                    'H' => transform.hardlink = false,
                    d if d.is_ascii_digit() => number.push(d),
                    _ => return Err(format!("Unknown flag in transform expression: {flag}")),
                }
            }
            if !number.is_empty() {
                transform.occurrence = number.parse().map_err(|_| invalid(""))?;
            }
            let syntax = cash_sed::sed::gnu_regex::Syntax {
                extended,
                posix: false,
                utf8: true,
            };
            if let Some(problem) =
                cash_sed::sed::gnu_regex::syntax_error(pattern.as_bytes(), syntax)
            {
                return Err(invalid(problem));
            }
            let translated = cash_sed::sed::compiler::translate_posix(pattern.as_bytes(), syntax);
            let mut source = String::from_utf8_lossy(&translated).into_owned();
            if ignore_case {
                source.insert_str(0, "(?i)");
            }
            transform.regex =
                fancy_regex::Regex::new(&source).map_err(|e| invalid(&e.to_string()))?;
            self.list.push(transform);
        }
        Ok(())
    }

    /// Whether there is any.
    pub(super) const fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// `name` through every transform that applies to `target`.
    pub(super) fn apply(&self, name: &str, target: Target) -> String {
        let mut name = name.to_owned();
        for transform in &self.list {
            let applies = match target {
                Target::Regular => transform.regular,
                Target::Symlink => transform.symlink,
                Target::Hardlink => transform.hardlink,
            };
            if applies {
                name = transform.substitute(&name);
            }
        }
        name
    }
}

impl Transform {
    fn substitute(&self, name: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        let mut count = 0;
        let mut at = 0;
        while at <= name.len() {
            let Ok(Some(found)) = self.regex.captures_from_pos(name, at) else {
                break;
            };
            let Some(whole) = found.get(0) else {
                break;
            };
            count += 1;
            let wanted = if self.occurrence > 0 {
                count == self.occurrence || (self.global && count > self.occurrence)
            } else {
                true
            };
            if wanted {
                out.push_str(name.get(last..whole.start()).unwrap_or_default());
                self.expand(&found, &mut out);
                last = whole.end();
                if !self.global {
                    break;
                }
            }
            at = if whole.end() == whole.start() {
                whole.end()
                    + name
                        .get(whole.end()..)
                        .and_then(|r| r.chars().next())
                        .map_or(1, char::len_utf8)
            } else {
                whole.end()
            };
            if self.occurrence > 0 && !self.global && count >= self.occurrence {
                break;
            }
        }
        out.push_str(name.get(last..).unwrap_or_default());
        out
    }

    fn expand(&self, found: &fancy_regex::Captures<'_, str>, out: &mut String) {
        let mut mode: Option<char> = None;
        let mut once: Option<char> = None;
        for piece in &self.replacement {
            let text = match piece {
                Piece::Text(text) => text.clone(),
                Piece::Group(n) => found
                    .get(*n)
                    .map(|m| m.as_str().to_owned())
                    .unwrap_or_default(),
                Piece::Case('E') => {
                    mode = None;
                    once = None;
                    continue;
                }
                Piece::Case(c @ ('U' | 'L')) => {
                    mode = Some(*c);
                    continue;
                }
                Piece::Case(c) => {
                    once = Some(*c);
                    continue;
                }
            };
            let mut text = match mode {
                Some('U') => text.to_uppercase(),
                Some('L') => text.to_lowercase(),
                _ => text,
            };
            if let Some(case) = once.take() {
                let mut chars = text.chars();
                if let Some(first) = chars.next() {
                    let head: String = if case == 'u' {
                        first.to_uppercase().collect()
                    } else {
                        first.to_lowercase().collect()
                    };
                    text = head + chars.as_str();
                }
            }
            let _ = write!(out, "{text}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(expression: &str, name: &str) -> String {
        let mut transforms = Transforms::default();
        transforms.add(expression).unwrap();
        transforms.apply(name, Target::Regular)
    }

    #[test]
    fn expressions_rename_as_gnu_tar_does() {
        assert_eq!(run("s/src/dst/", "src/a.txt"), "dst/a.txt");
        assert_eq!(run("s,^src/sub,moved,", "src/sub/b.txt"), "moved/b.txt");
        assert_eq!(run("s/a/A/g", "src/a.txt"), "src/A.txt");
        assert_eq!(run("s/a/A/", "a/a"), "A/a");
        assert_eq!(run("s/a/A/2", "a/a/a"), "a/A/a");
        assert_eq!(run("s/\\(.*\\)\\.txt/\\1.md/", "x.txt"), "x.md");
        assert_eq!(run("s/.*/\\U&/", "abc"), "ABC");
        assert_eq!(run("s/(/x/", "h"), "h");
        assert_eq!(run("s/a/b/;s/b/c/", "a"), "c");
    }
}
