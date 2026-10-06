// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! The normal format (`3c3`, `< old`, `---`, `> new`), as GNU diff's `normal.c` prints
//! it, and the line printer the other formats share.

use crate::engine::{self, Change};
use crate::params::Params;

/// GNU's default palette: `rs=0:hd=1:ad=32:de=31:ln=36`.
pub const RESET: &str = "\x1b[0m";
pub const HEADER: &str = "\x1b[1m";
pub const ADD: &str = "\x1b[32m";
pub const DELETE: &str = "\x1b[31m";
pub const LINE_NUMBER: &str = "\x1b[36m";

/// Writes `line` after `prefix`, coloured by `color` when `--color` is on, and then
/// GNU's `\ No newline at end of file` when the line has none.
pub fn put_line(out: &mut Vec<u8>, prefix: &[u8], line: &[u8], color: Option<&str>, p: &Params) {
    let colored = p.color && color.is_some();
    if let (true, Some(color)) = (colored, color) {
        out.extend_from_slice(color.as_bytes());
    }
    out.extend_from_slice(prefix);
    let (body, newline) = match line.strip_suffix(b"\n") {
        Some(body) => (body, true),
        None => (line, false),
    };
    if p.expand_tabs {
        out.extend_from_slice(&crate::utils::do_expand_tabs(body, p.tabsize));
    } else {
        out.extend_from_slice(body);
    }
    if colored {
        out.extend_from_slice(RESET.as_bytes());
    }
    out.push(b'\n');
    if !newline {
        out.extend_from_slice(b"\\ No newline at end of file\n");
    }
}

/// Writes `text` as a line of its own, coloured by `color` when `--color` is on.
pub fn put_marked(out: &mut Vec<u8>, text: &str, color: &str, p: &Params) {
    if p.color {
        out.extend_from_slice(color.as_bytes());
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(RESET.as_bytes());
    } else {
        out.extend_from_slice(text.as_bytes());
    }
    out.push(b'\n');
}

/// `first..last` as the normal and ed formats number it: `3`, or `3,5`; an empty range
/// is the line before it.
#[must_use]
pub fn number_range(first: i64, last: i64) -> String {
    let (a, b) = (first + 1, last + 1);
    if b > a {
        format!("{a},{b}")
    } else {
        b.to_string()
    }
}

/// The normal format for `changes`.
pub fn print(
    out: &mut Vec<u8>,
    lines0: &[&[u8]],
    lines1: &[&[u8]],
    changes: &[Change],
    p: &Params,
) {
    let (old_prefix, new_prefix): (&[u8], &[u8]) = if p.initial_tab {
        (b"<\t", b">\t")
    } else {
        (b"< ", b"> ")
    };
    for change in changes {
        let a = engine::analyze(std::slice::from_ref(change));
        if !a.shown() {
            continue;
        }
        let letter = match (a.show_old, a.show_new) {
            (true, true) => 'c',
            (true, false) => 'd',
            _ => 'a',
        };
        let header = format!(
            "{}{letter}{}",
            number_range(a.first0, a.last0),
            number_range(a.first1, a.last1)
        );
        put_marked(out, &header, LINE_NUMBER, p);
        if a.show_old {
            for line in &lines0[change.line0..change.line0 + change.deleted] {
                put_line(out, old_prefix, line, Some(DELETE), p);
            }
        }
        if a.show_old && a.show_new {
            out.extend_from_slice(b"---\n");
        }
        if a.show_new {
            for line in &lines1[change.line1..change.line1 + change.inserted] {
                put_line(out, new_prefix, line, Some(ADD), p);
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
        print(&mut out, &l0, &l1, &changes, p);
        String::from_utf8(out).expect("text")
    }

    #[test]
    fn change_add_and_delete() {
        let p = Params::default();
        assert_eq!(
            diff("a\nb\nc\n", "a\nB\nc\nd\n", &p),
            "2c2\n< b\n---\n> B\n3a4\n> d\n"
        );
        assert_eq!(diff("a\nb\nc\n", "a\nc\n", &p), "2d1\n< b\n");
        assert_eq!(diff("", "a\nb\n", &p), "0a1,2\n> a\n> b\n");
    }

    #[test]
    fn missing_newline_and_colour() {
        let mut p = Params::default();
        assert_eq!(
            diff("a\nb", "a\nb\n", &p),
            "2c2\n< b\n\\ No newline at end of file\n---\n> b\n"
        );
        p.color = true;
        assert_eq!(
            diff("a\nb\n", "a\nB\n", &p),
            "\x1b[36m2c2\x1b[0m\n\x1b[31m< b\x1b[0m\n---\n\x1b[32m> B\x1b[0m\n"
        );
    }

    #[test]
    fn ignored_changes_are_left_out() {
        let mut p = Params::default();
        p.ignore_blank_lines = true;
        assert_eq!(diff("x\n\ny\nz\n", "x\ny\nz\nw\n", &p), "4a4\n> w\n");
    }
}
