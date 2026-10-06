// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! The ed script format (`-e`), as GNU diff's `ed.c` prints it: the changes last to
//! first, so that the line numbers of the first file hold as the script runs.

use crate::engine::{self, Change};
use crate::normal_diff::number_range;
use crate::params::Params;

/// The ed script for `changes`.
pub fn print(out: &mut Vec<u8>, lines1: &[&[u8]], changes: &[Change], p: &Params) {
    for change in changes.iter().rev() {
        let a = engine::analyze(std::slice::from_ref(change));
        if !a.shown() {
            continue;
        }
        let letter = match (a.show_old, a.show_new) {
            (true, true) => 'c',
            (true, false) => 'd',
            _ => 'a',
        };
        out.extend_from_slice(format!("{}{letter}\n", number_range(a.first0, a.last0)).as_bytes());
        if a.show_new {
            for line in &lines1[change.line1..change.line1 + change.inserted] {
                let body = line.strip_suffix(b"\n").unwrap_or(line);
                if body == b"." {
                    // A line of one dot would end the insertion: ed's own way around it.
                    out.extend_from_slice(b"..\n.\ns/.//\na\n");
                } else {
                    crate::utils::do_write_line(out, body, p.expand_tabs, p.tabsize)
                        .unwrap_or_default();
                    out.push(b'\n');
                }
            }
            out.extend_from_slice(b".\n");
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

    fn diff(a: &str, b: &str) -> String {
        let p = Params::default();
        let (l0, l1) = (split_lines(a.as_bytes()), split_lines(b.as_bytes()));
        let changes = script(&l0, &l1, &p);
        let mut out = Vec::new();
        print(&mut out, &l1, &changes, &p);
        String::from_utf8(out).expect("text")
    }

    #[test]
    fn last_change_first() {
        assert_eq!(diff("a\nb\nc\n", "a\nB\nc\nd\n"), "3a\nd\n.\n2c\nB\n.\n");
        assert_eq!(
            diff("a\nb\nc\nd\ne\n", "a\nc\nx\nd\ne\nf\n"),
            "5a\nf\n.\n3a\nx\n.\n2d\n"
        );
    }
}
