// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! The unified format (`-u`), as GNU diff's `context.c` prints it.

use crate::engine::{self, Change};
use crate::normal_diff::{ADD, DELETE, HEADER, LINE_NUMBER, put_line, put_marked};
use crate::params::Params;

/// `first..last` as a `@@` header numbers it: `3,5`, `4` for one line, and `3,0` for
/// no line (the line before the place).
fn range(first: i64, last: i64) -> String {
    let (a, b) = (first + 1, last + 1);
    match b.cmp(&a) {
        std::cmp::Ordering::Less => format!("{b},0"),
        std::cmp::Ordering::Equal => b.to_string(),
        std::cmp::Ordering::Greater => format!("{a},{}", b - a + 1),
    }
}

/// The unified format for `changes`; `header0` and `header1` are the two files' names
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
    put_marked(out, &format!("--- {header0}"), HEADER, p);
    put_marked(out, &format!("+++ {header1}"), HEADER, p);
    let context = i64::try_from(p.context_count).unwrap_or(i64::MAX);
    let n0 = i64::try_from(lines0.len()).unwrap_or(i64::MAX);
    let n1 = i64::try_from(lines1.len()).unwrap_or(i64::MAX);
    let (common, old, new): (&[u8], &[u8], &[u8]) = if p.initial_tab {
        (b"\t", b"-\t", b"+\t")
    } else {
        (b" ", b"-", b"+")
    };
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
        put_marked(
            out,
            &format!("@@ -{} +{} @@", range(first0, last0), range(first1, last1)),
            LINE_NUMBER,
            p,
        );
        let mut next = hunk.iter().peekable();
        let (mut i, mut j) = (first0, first1);
        let at = |n: i64| usize::try_from(n).unwrap_or(0);
        while i <= last0 || j <= last1 {
            match next.peek() {
                Some(change) if i64::try_from(change.line0).unwrap_or(i64::MAX) <= i => {
                    for line in &lines0[change.line0..change.line0 + change.deleted] {
                        put_line(out, old, line, Some(DELETE), p);
                    }
                    for line in &lines1[change.line1..change.line1 + change.inserted] {
                        put_line(out, new, line, Some(ADD), p);
                    }
                    i += i64::try_from(change.deleted).unwrap_or(0);
                    j += i64::try_from(change.inserted).unwrap_or(0);
                    next.next();
                }
                _ => {
                    let Some(line) = lines0.get(at(i)) else {
                        break;
                    };
                    put_line(out, common, line, None, p);
                    i += 1;
                    j += 1;
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
    fn hunks_with_context() {
        let mut p = Params::default();
        assert_eq!(
            diff("a\nb\nc\n", "a\nB\nc\nd\n", &p),
            "--- a\n+++ b\n@@ -1,3 +1,4 @@\n a\n-b\n+B\n c\n+d\n"
        );
        p.context_count = 0;
        assert_eq!(
            diff("a\nb\nc\n", "a\nB\nc\nd\n", &p),
            "--- a\n+++ b\n@@ -2 +2 @@\n-b\n+B\n@@ -3,0 +4 @@\n+d\n"
        );
        assert_eq!(
            diff("a\nb\nc\n", "", &p),
            "--- a\n+++ b\n@@ -1,3 +0,0 @@\n-a\n-b\n-c\n"
        );
    }

    #[test]
    fn far_changes_are_separate_hunks() {
        let p = Params::default();
        let a: String = (1..=20).map(|n| format!("{n}\n")).collect();
        let b = a.replace("10\n", "ten\n").replace("15\n", "fifteen\n");
        assert_eq!(
            diff(&a, &b, &p),
            "--- a\n+++ b\n@@ -7,12 +7,12 @@\n 7\n 8\n 9\n-10\n+ten\n 11\n 12\n 13\n 14\n-15\n+fifteen\n 16\n 17\n 18\n"
        );
        let mut p1 = Params::default();
        p1.context_count = 1;
        assert_eq!(
            diff(&a, &b, &p1),
            "--- a\n+++ b\n@@ -9,3 +9,3 @@\n 9\n-10\n+ten\n 11\n@@ -14,3 +14,3 @@\n 14\n-15\n+fifteen\n 16\n"
        );
    }

    #[test]
    fn an_ignored_change_near_a_real_one_is_shown() {
        let mut p = Params::default();
        p.ignore_blank_lines = true;
        assert_eq!(
            diff("x\n\ny\nz\n", "x\ny\nz\nw\n", &p),
            "--- a\n+++ b\n@@ -1,4 +1,4 @@\n x\n-\n y\n z\n+w\n"
        );
        assert_eq!(diff("x\n\ny\n", "x\ny\n", &p), "--- a\n+++ b\n");
    }
}
