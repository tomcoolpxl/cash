// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! The context format (`-c`), as GNU diff's `context.c` prints it.

use crate::engine::{self, Change};
use crate::normal_diff::{ADD, DELETE, HEADER, LINE_NUMBER, put_line, put_marked};
use crate::params::Params;

/// `first..last` as a `*** 3,5 ****` header numbers it; an empty range is the line
/// before it.
fn range(first: i64, last: i64) -> String {
    let (a, b) = (first + 1, last + 1);
    if b <= a {
        b.to_string()
    } else {
        format!("{a},{b}")
    }
}

/// The context format for `changes`; `header0` and `header1` are the two files' names
/// and times, or their labels.
pub fn print(
    out: &mut Vec<u8>,
    lines0: &[&[u8]],
    lines1: &[&[u8]],
    changes: &[Change],
    header0: &str,
    header1: &str,
    p: &Params,
) {
    put_marked(out, &format!("*** {header0}"), HEADER, p);
    put_marked(out, &format!("--- {header1}"), HEADER, p);
    let context = i64::try_from(p.context_count).unwrap_or(i64::MAX);
    let n0 = i64::try_from(lines0.len()).unwrap_or(i64::MAX);
    let n1 = i64::try_from(lines1.len()).unwrap_or(i64::MAX);
    let sep: &[u8] = if p.initial_tab { b"\t" } else { b" " };
    let prefix = |flag: &[u8]| -> Vec<u8> { [flag, sep].concat() };
    for hunk in engine::hunks(changes, p.context_count) {
        let hunk = &changes[hunk];
        let a = engine::analyze(hunk);
        if !a.shown() {
            continue;
        }
        let first0 = (a.first0 - context).max(0);
        let last0 = (a.last0 + context).min(n0 - 1);
        let first1 = (a.first1 - context).max(0);
        let last1 = (a.last1 + context).min(n1 - 1);
        out.extend_from_slice(b"***************\n");
        put_marked(
            out,
            &format!("*** {} ****", range(first0, last0)),
            LINE_NUMBER,
            p,
        );
        if a.show_old {
            let mut next = hunk.iter().peekable();
            for i in first0..=last0 {
                let i = usize::try_from(i).unwrap_or(0);
                while next.peek().is_some_and(|c| c.line0 + c.deleted <= i) {
                    next.next();
                }
                let flag: &[u8] = match next.peek() {
                    Some(c) if c.line0 <= i => {
                        if c.inserted > 0 {
                            b"!"
                        } else {
                            b"-"
                        }
                    }
                    _ => b" ",
                };
                if let Some(line) = lines0.get(i) {
                    put_line(out, &prefix(flag), line, Some(DELETE), p);
                }
            }
        }
        put_marked(
            out,
            &format!("--- {} ----", range(first1, last1)),
            LINE_NUMBER,
            p,
        );
        if a.show_new {
            let mut next = hunk.iter().peekable();
            for j in first1..=last1 {
                let j = usize::try_from(j).unwrap_or(0);
                while next.peek().is_some_and(|c| c.line1 + c.inserted <= j) {
                    next.next();
                }
                let flag: &[u8] = match next.peek() {
                    Some(c) if c.line1 <= j => {
                        if c.deleted > 0 {
                            b"!"
                        } else {
                            b"+"
                        }
                    }
                    _ => b" ",
                };
                if let Some(line) = lines1.get(j) {
                    put_line(out, &prefix(flag), line, Some(ADD), p);
                }
            }
        }
    }
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
    use crate::engine::{script, split_lines};

    fn diff(a: &str, b: &str, p: &Params) -> String {
        let (l0, l1) = (split_lines(a.as_bytes()), split_lines(b.as_bytes()));
        let changes = script(&l0, &l1, p);
        let mut out = Vec::new();
        print(&mut out, &l0, &l1, &changes, "a", "b", p);
        String::from_utf8(out).expect("text")
    }

    #[test]
    fn marks_and_ranges() {
        let mut p = Params::default();
        assert_eq!(
            diff("a\nb\nc\n", "a\nB\nc\nd\n", &p),
            "*** a\n--- b\n***************\n*** 1,3 ****\n  a\n! b\n  c\n--- 1,4 ----\n  a\n! B\n  c\n+ d\n"
        );
        p.context_count = 0;
        assert_eq!(
            diff("a\nb\nc\n", "a\nB\nc\nd\n", &p),
            "*** a\n--- b\n***************\n*** 2 ****\n! b\n--- 2 ----\n! B\n***************\n*** 3 ****\n--- 4 ----\n+ d\n"
        );
    }

    #[test]
    fn deletions_and_missing_newline() {
        let p = Params::default();
        assert_eq!(
            diff("a\nb\nc\nd\ne\n", "a\nc\nx\nd\ne\nf\n", &p),
            "*** a\n--- b\n***************\n*** 1,5 ****\n  a\n- b\n  c\n  d\n  e\n--- 1,6 ----\n  a\n  c\n+ x\n  d\n  e\n+ f\n"
        );
        assert_eq!(
            diff("a\nb", "a\nb\n", &p),
            "*** a\n--- b\n***************\n*** 1,2 ****\n  a\n! b\n\\ No newline at end of file\n--- 1,2 ----\n  a\n! b\n"
        );
    }
}
