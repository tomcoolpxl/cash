// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! The script of changes between two files, in the shape GNU diff's `analyze.c` makes
//! it, and the hunks the output formats print from it (`context.c`'s `find_hunk` and
//! `analyze_hunk`).
//!
//! A line is its bytes up to and including its newline; the last line of a file that
//! does not end in one lacks it, and so differs from the same text with one, as in GNU
//! diff. The comparison options (`-i`, `-E`, `-Z`, `-b`, `-w`) act on a key computed
//! from each line, never on the line itself, so the output shows the lines as they
//! are; `--strip-trailing-cr` is the exception, applied to the input by the caller.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::params::Params;

/// A run of lines that differ.
///
/// `deleted` lines of the first file from `line0`, and `inserted` lines of the second
/// from `line1` (both 0-based). `ignore` says every one of its lines is one `-B` or
/// `-I` disregards, so the change counts only when it shares a hunk with one that is
/// not.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Change {
    pub line0: usize,
    pub line1: usize,
    pub deleted: usize,
    pub inserted: usize,
    pub ignore: bool,
}

/// The lines of `data`, each with its newline when it has one.
#[must_use]
pub fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    data.split_inclusive(|&b| b == b'\n').collect()
}

/// `data` with one carriage return before each newline removed (`--strip-trailing-cr`);
/// a carriage return elsewhere, at the end of an unterminated last line included, stays.
#[must_use]
pub fn strip_trailing_cr(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut rest = data;
    while let Some(at) = rest.iter().position(|&b| b == b'\n') {
        let (line, tail) = rest.split_at(at + 1);
        match line.strip_suffix(b"\r\n") {
            Some(body) => {
                out.extend_from_slice(body);
                out.push(b'\n');
            }
            None => out.extend_from_slice(line),
        }
        rest = tail;
    }
    out.extend_from_slice(rest);
    out
}

const fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Whether the options make the key of a line differ from the line.
const fn needs_key(p: &Params) -> bool {
    p.ignore_case
        || p.ignore_tab_expansion
        || p.ignore_trailing_space
        || p.ignore_space_change
        || p.ignore_all_space
}

/// The line as the comparison options see it. Without any, the line itself.
fn key<'a>(line: &'a [u8], p: &Params) -> Cow<'a, [u8]> {
    if !needs_key(p) {
        return Cow::Borrowed(line);
    }
    let (body, newline) = match line.strip_suffix(b"\n") {
        Some(body) => (body, true),
        None => (line, false),
    };
    let mut key: Vec<u8> = if p.ignore_tab_expansion {
        crate::utils::do_expand_tabs(body, p.tabsize)
    } else {
        body.to_vec()
    };
    if p.ignore_case {
        key = match std::str::from_utf8(&key) {
            Ok(text) => text.to_lowercase().into_bytes(),
            Err(_) => key.to_ascii_lowercase(),
        };
    }
    if p.ignore_all_space {
        key.retain(|&b| !is_space(b));
    } else if p.ignore_space_change {
        // Runs of white space are one space; white space at the end is nothing.
        let mut collapsed = Vec::with_capacity(key.len());
        let mut in_space = false;
        for &b in &key {
            if is_space(b) {
                if !in_space {
                    collapsed.push(b' ');
                    in_space = true;
                }
            } else {
                collapsed.push(b);
                in_space = false;
            }
        }
        if collapsed.last() == Some(&b' ') {
            collapsed.pop();
        }
        key = collapsed;
    } else if p.ignore_trailing_space {
        while key.last().is_some_and(|&b| is_space(b)) {
            key.pop();
        }
    }
    // `-b` and `-w` make nothing of a missing final newline; the other options keep it
    // a difference.
    if newline && !p.ignore_all_space && !p.ignore_space_change {
        key.push(b'\n');
    }
    Cow::Owned(key)
}

/// Whether `-B` or `-I` disregards a change made of this line alone.
fn is_ignorable(line: &[u8], p: &Params) -> bool {
    if p.ignore_blank_lines {
        let k = key(line, p);
        let body = k.strip_suffix(b"\n").unwrap_or(&k);
        if body.is_empty() {
            return true;
        }
    }
    let body = line.strip_suffix(b"\n").unwrap_or(line);
    p.ignore_regexps.iter().any(|re| re.is_match(body))
}

/// The changes that turn `lines0` into `lines1`.
///
/// As GNU's `analyze.c` finds them: the lines are told apart by equivalence class,
/// the lines whose class has no member in the other file are set aside as changed
/// (GNU's `discard_confusing_lines`), and Myers' algorithm in its linear-space form
/// (the `similar` crate's, as GNU's `diffseq.h`) runs on what is left. Two long files
/// that differ throughout then take moments, where the shortest edit script over
/// every line, as upstream's `diff` crate found it, took minutes and gigabytes.
#[must_use]
pub fn script<'a>(lines0: &[&'a [u8]], lines1: &[&'a [u8]], p: &Params) -> Vec<Change> {
    let mut classes: HashMap<Cow<'a, [u8]>, usize> = HashMap::new();
    let mut ids_of = |lines: &[&'a [u8]]| -> Vec<usize> {
        lines
            .iter()
            .map(|line| {
                let next = classes.len();
                *classes.entry(key(line, p)).or_insert(next)
            })
            .collect()
    };
    let ids0 = ids_of(lines0);
    let ids1 = ids_of(lines1);
    let mut count0 = vec![0usize; classes.len()];
    let mut count1 = vec![0usize; classes.len()];
    for &id in &ids0 {
        count0[id] += 1;
    }
    for &id in &ids1 {
        count1[id] += 1;
    }
    // A line with no equal in the other file is changed whatever the rest is.
    let kept0: Vec<usize> = (0..lines0.len()).filter(|&i| count1[ids0[i]] > 0).collect();
    let kept1: Vec<usize> = (0..lines1.len()).filter(|&j| count0[ids1[j]] > 0).collect();
    let mut changed0 = vec![true; lines0.len()];
    let mut changed1 = vec![true; lines1.len()];
    for &i in &kept0 {
        changed0[i] = false;
    }
    for &j in &kept1 {
        changed1[j] = false;
    }
    let seq0: Vec<usize> = kept0.iter().map(|&i| ids0[i]).collect();
    let seq1: Vec<usize> = kept1.iter().map(|&j| ids1[j]).collect();
    for op in similar::capture_diff_slices(similar::Algorithm::Myers, &seq0, &seq1) {
        let (old_index, old_len, new_index, new_len) = match op {
            similar::DiffOp::Equal { .. } => continue,
            similar::DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index, old_len, new_index, 0),
            similar::DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index, 0, new_index, new_len),
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (old_index, old_len, new_index, new_len),
        };
        for &i in &kept0[old_index..old_index + old_len] {
            changed0[i] = true;
        }
        for &j in &kept1[new_index..new_index + new_len] {
            changed1[j] = true;
        }
    }

    // A run of changed lines in either file is one change (GNU's `build_script`).
    let mut changes = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < lines0.len() || j < lines1.len() {
        let changed =
            changed0.get(i).copied().unwrap_or(false) || changed1.get(j).copied().unwrap_or(false);
        if changed {
            let (line0, line1) = (i, j);
            while changed0.get(i).copied().unwrap_or(false) {
                i += 1;
            }
            while changed1.get(j).copied().unwrap_or(false) {
                j += 1;
            }
            changes.push(Change {
                line0,
                line1,
                deleted: i - line0,
                inserted: j - line1,
                ignore: false,
            });
        } else {
            i += 1;
            j += 1;
        }
    }
    let can_ignore = p.ignore_blank_lines || !p.ignore_regexps.is_empty();
    for change in &mut changes {
        change.ignore = can_ignore
            && lines0[change.line0..change.line0 + change.deleted]
                .iter()
                .chain(&lines1[change.line1..change.line1 + change.inserted])
                .all(|line| is_ignorable(line, p));
    }
    changes
}

/// What a hunk covers and whether anything in it is shown: GNU's `analyze_hunk`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Analysis {
    /// The first line of each file in the hunk (0-based).
    pub first0: i64,
    pub first1: i64,
    /// The last line of each file in the hunk; one less than `first` when the hunk
    /// touches no line of that file.
    pub last0: i64,
    pub last1: i64,
    /// Whether a change in the hunk deletes lines, or inserts lines.
    pub show_old: bool,
    pub show_new: bool,
    /// Whether every change in the hunk is one `-B` or `-I` disregards.
    pub trivial: bool,
}

impl Analysis {
    /// Whether the hunk is printed at all.
    #[must_use]
    pub const fn shown(&self) -> bool {
        !self.trivial && (self.show_old || self.show_new)
    }
}

/// Analyses `hunk`, a non-empty run of consecutive changes.
#[must_use]
pub fn analyze(hunk: &[Change]) -> Analysis {
    let first = hunk.first().copied().unwrap_or_default();
    let last = hunk.last().copied().unwrap_or_default();
    let as_i64 = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    Analysis {
        first0: as_i64(first.line0),
        first1: as_i64(first.line1),
        last0: as_i64(last.line0 + last.deleted) - 1,
        last1: as_i64(last.line1 + last.inserted) - 1,
        show_old: hunk.iter().any(|c| c.deleted > 0),
        show_new: hunk.iter().any(|c| c.inserted > 0),
        trivial: hunk.iter().all(|c| c.ignore),
    }
}

/// Groups the changes into the hunks a context of `context` lines joins.
///
/// Two changes belong together when fewer than `2 * context + 1` lines separate them,
/// so that their contexts touch or overlap, or fewer than `context` lines when the
/// second is one `-B` or `-I` disregards (GNU's `find_hunk`, as GNU diff 3.12 joins
/// them). Each hunk is a range of indices into `changes`.
#[must_use]
pub fn hunks(changes: &[Change], context: usize) -> Vec<std::ops::Range<usize>> {
    let mut hunks = Vec::new();
    let mut start = 0;
    while start < changes.len() {
        let mut end = start;
        loop {
            let this = changes[end];
            let top0 = this.line0 + this.deleted;
            let top1 = this.line1 + this.inserted;
            let Some(next) = changes.get(end + 1) else {
                break;
            };
            let thresh = if next.ignore {
                context
            } else {
                2 * context + 1
            };
            if next.line0 < top0 + thresh && next.line1 < top1 + thresh {
                end += 1;
            } else {
                break;
            }
        }
        hunks.push(start..end + 1);
        start = end + 1;
    }
    hunks
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::field_reassign_with_default,
    clippy::assert_is_empty,
    clippy::panic_in_result_fn,
    clippy::format_collect,
    reason = "a test stops loudly, and sets the options it is about"
)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<&[u8]> {
        split_lines(text.as_bytes())
    }

    fn change(line0: usize, line1: usize, deleted: usize, inserted: usize) -> Change {
        Change {
            line0,
            line1,
            deleted,
            inserted,
            ignore: false,
        }
    }

    #[test]
    fn a_missing_final_newline_is_a_difference() {
        let p = Params::default();
        let a = lines("a\nb");
        let b = lines("a\nb\n");
        assert_eq!(script(&a, &b, &p), vec![change(1, 1, 1, 1)]);
        let mut w = Params::default();
        w.ignore_all_space = true;
        assert!(script(&a, &b, &w).is_empty());
        let mut b_ = Params::default();
        b_.ignore_space_change = true;
        assert!(script(&a, &b, &b_).is_empty());
    }

    #[test]
    fn space_change_keys() {
        let mut p = Params::default();
        p.ignore_space_change = true;
        let same = |x: &str, y: &str| script(&lines(x), &lines(y), &p).is_empty();
        assert!(same("b \n", "b\n"));
        assert!(same("  b\n", " b\n"));
        assert!(!same(" b\n", "b\n"));
        assert!(!same("a b\n", "ab\n"));
        assert!(same("a b\n", "a  b\n"));
        assert!(same("a\r\n", "a\n"));
    }

    #[test]
    fn tab_expansion_and_case_keys() {
        let mut p = Params::default();
        p.ignore_tab_expansion = true;
        assert!(script(&lines("a\tb\n"), &lines("a       b\n"), &p).is_empty());
        assert!(!script(&lines("\ta\n"), &lines("    a\n"), &p).is_empty());
        let mut p = Params::default();
        p.ignore_case = true;
        assert!(script(&lines("Hello\n"), &lines("hello\n"), &p).is_empty());
        assert!(script(&lines("\u{c9}\n"), &lines("\u{e9}\n"), &p).is_empty());
    }

    #[test]
    fn blank_lines_are_ignorable_only_when_blank_to_the_options() {
        let mut p = Params::default();
        p.ignore_blank_lines = true;
        let s = script(&lines("x\n  \ny\n"), &lines("x\ny\n"), &p);
        assert_eq!(s.len(), 1);
        assert!(!s[0].ignore);
        p.ignore_space_change = true;
        let s = script(&lines("x\n  \ny\n"), &lines("x\ny\n"), &p);
        assert!(s[0].ignore);
        let s = script(&lines("x\n\ny\nz\n"), &lines("x\ny\nz\nw\n"), &p);
        assert_eq!(s.len(), 2);
        assert!(s[0].ignore && !s[1].ignore);
    }

    #[test]
    fn hunks_join_by_context_distance() {
        let changes = vec![change(1, 1, 1, 1), change(5, 5, 1, 1), change(20, 20, 0, 1)];
        assert_eq!(hunks(&changes, 3), vec![0..2, 2..3]);
        assert_eq!(hunks(&changes, 1), vec![0..1, 1..2, 2..3]);
        let a = analyze(&changes[0..2]);
        assert_eq!((a.first0, a.last0, a.first1, a.last1), (1, 5, 1, 5));
        let a = analyze(&changes[2..3]);
        assert_eq!((a.first0, a.last0), (20, 19));
        assert!(!a.show_old && a.show_new);
    }

    #[test]
    fn strips_one_carriage_return_before_each_newline() {
        assert_eq!(strip_trailing_cr(b"a\r\nb\r\r\nc\r"), b"a\nb\r\nc\r");
    }
}
