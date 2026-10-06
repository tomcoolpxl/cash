// This file is part of cash's copy of the uutils diffutils package (CASH-PATCHES.md).
//
// For the full copyright and license information, please view the LICENSE-*
// files that was distributed with this source code.

//! The side-by-side format (`-y`), as GNU diff's `side.c` prints it: two columns of
//! half lines, a gutter with ` `, `|`, `<` or `>` between them, tabs for the padding
//! unless `-t`.

use unicode_width::UnicodeWidthChar as _;

use crate::engine::{self, Change};
use crate::normal_diff::{ADD, DELETE, RESET};
use crate::params::Params;

const GUTTER_WIDTH_MINIMUM: usize = 3;

/// The columns, from the width and the tab size, as GNU computes them.
struct Layout {
    half_width: usize,
    column_two_offset: usize,
    tabsize: usize,
    expand_tabs: bool,
}

impl Layout {
    fn new(p: &Params) -> Self {
        let t = if p.expand_tabs { 1 } else { p.tabsize };
        let w = p.width;
        let t_plus_g = t + GUTTER_WIDTH_MINIMUM;
        let unaligned_off = (w >> 1) + (t_plus_g >> 1) + (w & t_plus_g & 1);
        let off = unaligned_off - unaligned_off % t;
        let half_width = off
            .saturating_sub(GUTTER_WIDTH_MINIMUM)
            .min(w.saturating_sub(off));
        Self {
            half_width,
            column_two_offset: if half_width == 0 { w } else { off },
            tabsize: p.tabsize,
            expand_tabs: p.expand_tabs,
        }
    }

    /// Moves from column `from` to column `to` with tabs where they fit, then spaces.
    fn tab_from_to(&self, out: &mut Vec<u8>, from: usize, to: usize) -> usize {
        let mut from = from;
        if !self.expand_tabs {
            let mut tab = from + self.tabsize - from % self.tabsize;
            while tab <= to {
                out.push(b'\t');
                from = tab;
                tab += self.tabsize;
            }
        }
        while from < to {
            out.push(b' ');
            from += 1;
        }
        to
    }

    /// Writes `line` from column `indent`, cut at `out_bound` columns; returns the
    /// column reached.
    fn print_half_line(
        &self,
        out: &mut Vec<u8>,
        line: &[u8],
        indent: usize,
        out_bound: usize,
    ) -> usize {
        let mut in_position = 0usize;
        let mut out_position = 0usize;
        let mut rest = line;
        while let Some((bytes, width)) = next_char(rest) {
            rest = rest.get(bytes.len()..).unwrap_or_default();
            match bytes {
                b"\t" => {
                    let spaces = self.tabsize - in_position % self.tabsize;
                    if in_position == out_position {
                        let mut tabstop = out_position + spaces;
                        if self.expand_tabs {
                            tabstop = tabstop.min(out_bound);
                            while out_position < tabstop {
                                out.push(b' ');
                                out_position += 1;
                            }
                        } else if tabstop < out_bound {
                            out_position = tabstop;
                            out.push(b'\t');
                        }
                    }
                    in_position += spaces;
                }
                b"\r" => {
                    out.push(b'\r');
                    self.tab_from_to(out, 0, indent);
                    in_position = 0;
                    out_position = 0;
                }
                b"\x08" => {
                    if in_position != 0 {
                        in_position -= 1;
                        if in_position < out_bound {
                            if out_position <= in_position {
                                while out_position < in_position {
                                    out.push(b' ');
                                    out_position += 1;
                                }
                            } else {
                                out_position = in_position;
                                out.push(b'\x08');
                            }
                        }
                    }
                }
                b"\n" => {}
                _ => match width {
                    // A control character, or a byte that is no character.
                    None => {
                        if in_position < out_bound {
                            out.extend_from_slice(bytes);
                        }
                    }
                    Some(width) => {
                        if in_position + width <= out_bound {
                            in_position += width;
                            out_position = in_position;
                            out.extend_from_slice(bytes);
                        } else {
                            in_position += width;
                        }
                    }
                },
            }
        }
        out_position
    }

    /// One line of the two columns: GNU's `print_1sdiff_line`.
    fn put_line(
        &self,
        out: &mut Vec<u8>,
        left: Option<&[u8]>,
        sep: u8,
        right: Option<&[u8]>,
        p: &Params,
    ) {
        let ends_in_newline = |line: &[u8]| line.last() == Some(&b'\n');
        let mut col = 0;
        let mut put_newline = false;
        let color = match sep {
            b'<' => Some(DELETE),
            b'>' => Some(ADD),
            _ => None,
        }
        .filter(|_| p.color);
        if let Some(color) = color {
            out.extend_from_slice(color.as_bytes());
        }
        if let Some(left) = left {
            put_newline |= ends_in_newline(left);
            col = self.print_half_line(out, left, 0, self.half_width);
        }
        let mut sep = sep;
        if sep != b' ' {
            col =
                self.tab_from_to(out, col, (self.half_width + self.column_two_offset - 1) / 2) + 1;
            if let (b'|', Some(right)) = (sep, right) {
                if put_newline != ends_in_newline(right) {
                    sep = if put_newline { b'/' } else { b'\\' };
                }
            }
            out.push(sep);
        }
        if let Some(right) = right {
            put_newline |= ends_in_newline(right);
            if right.first() != Some(&b'\n') {
                col = self.tab_from_to(out, col, self.column_two_offset);
                self.print_half_line(out, right, col, self.half_width);
            }
        }
        if put_newline {
            out.push(b'\n');
        }
        if color.is_some() {
            out.extend_from_slice(RESET.as_bytes());
        }
    }
}

/// The next character of `rest` and its width in columns: `None` for a control
/// character or a byte that starts no UTF-8 character, which prints but takes no
/// column.
fn next_char(rest: &[u8]) -> Option<(&[u8], Option<usize>)> {
    let first = *rest.first()?;
    if first < 0x80 {
        let bytes = rest.get(..1)?;
        let width = (0x20..0x7f).contains(&first).then_some(1);
        return Some((bytes, width));
    }
    let len = match first {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    };
    match rest.get(..len).and_then(|b| std::str::from_utf8(b).ok()) {
        Some(text) => {
            let width = text.chars().next().and_then(|c| c.width());
            Some((text.as_bytes(), width))
        }
        None => Some((rest.get(..1)?, None)),
    }
}

/// The side-by-side format for `changes`.
pub fn print(
    out: &mut Vec<u8>,
    lines0: &[&[u8]],
    lines1: &[&[u8]],
    changes: &[Change],
    p: &Params,
) {
    let layout = Layout::new(p);
    let (mut next0, mut next1) = (0usize, 0usize);
    let common =
        |out: &mut Vec<u8>, limit0: usize, limit1: usize, next0: &mut usize, next1: &mut usize| {
            if !p.suppress_common_lines {
                for (i0, i1) in (*next0..limit0).zip(*next1..) {
                    if p.left_column {
                        layout.put_line(out, lines0.get(i0).copied(), b'(', None, p);
                    } else {
                        layout.put_line(
                            out,
                            lines0.get(i0).copied(),
                            b' ',
                            lines1.get(i1).copied(),
                            p,
                        );
                    }
                }
            }
            *next0 = limit0;
            *next1 = limit1;
        };
    for change in changes {
        let a = engine::analyze(std::slice::from_ref(change));
        if !a.shown() {
            continue;
        }
        common(out, change.line0, change.line1, &mut next0, &mut next1);
        let (mut i, mut j) = (change.line0, change.line1);
        let (end0, end1) = (
            change.line0 + change.deleted,
            change.line1 + change.inserted,
        );
        while i < end0 && j < end1 {
            layout.put_line(out, lines0.get(i).copied(), b'|', lines1.get(j).copied(), p);
            i += 1;
            j += 1;
        }
        while i < end0 {
            layout.put_line(out, lines0.get(i).copied(), b'<', None, p);
            i += 1;
        }
        while j < end1 {
            layout.put_line(out, None, b'>', lines1.get(j).copied(), p);
            j += 1;
        }
        next0 = end0;
        next1 = end1;
    }
    common(out, lines0.len(), lines1.len(), &mut next0, &mut next1);
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
    fn gnus_columns_at_the_default_width() {
        let p = Params::default();
        assert_eq!(
            diff("a\nb\nc\n", "a\nB\nc\nd\n", &p),
            "a\t\t\t\t\t\t\t\ta\nb\t\t\t\t\t\t\t      |\tB\nc\t\t\t\t\t\t\t\tc\n\t\t\t\t\t\t\t      >\td\n"
        );
    }

    #[test]
    fn narrow_widths_and_options() {
        let mut p = Params::default();
        p.width = 20;
        assert_eq!(
            diff("a\nb\nc\n", "a\nB\nc\nd\n", &p),
            "a\ta\nb     |\tB\nc\tc\n      >\td\n"
        );
        p.width = 30;
        p.suppress_common_lines = true;
        assert_eq!(diff("a\nb\n", "a\nB\n", &p), "b\t      |\tB\n");
        p.suppress_common_lines = false;
        p.left_column = true;
        assert_eq!(diff("a\nb\n", "a\nB\n", &p), "a\t      (\nb\t      |\tB\n");
    }

    #[test]
    fn long_lines_are_cut_and_a_missing_newline_bends_the_bar() {
        let mut p = Params::default();
        p.width = 30;
        assert_eq!(
            diff(
                "abcdefghijklmnopqrstuvwxyz\n",
                "abcdefghijklmnopqrstuvwxyZ\n",
                &p
            ),
            "abcdefghijklm |\tabcdefghijklm\n"
        );
        assert_eq!(diff("a\nb", "a\nb\n", &p), "a\t\ta\nb\t      \\\tb\n");
        assert_eq!(diff("a\nb\n", "x\nb", &p), "a\t      |\tx\nb\t      /\tb\n");
    }

    #[test]
    fn tabs_in_the_text_and_expanded_tabs() {
        let mut p = Params::default();
        p.width = 30;
        assert_eq!(diff("a\tb\n", "a\tc\n", &p), "a\tb     |\ta\tc\n");
        p.expand_tabs = true;
        assert_eq!(
            diff("a\nb\n", "a\nB\n", &p),
            "a                a\nb             |  B\n"
        );
    }
}
