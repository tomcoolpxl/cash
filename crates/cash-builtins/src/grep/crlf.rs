//! The CRLF rule for `grep` (D20): a line ends at `\n`, and a `\r` just before it is
//! not part of the line.
//!
//! The reader here takes the `\r` out of every `\r\n` before the searcher sees the
//! bytes, so `x$`, `-x`, `-w`, `.` and `\S` all see the line without it, and `-o`
//! never prints it. It remembers which lines had one, so that an output line is
//! written as it was read and `-b` counts the file's own bytes.

use std::cell::RefCell;
use std::io::Read;
use std::rc::Rc;

/// Which lines ended in CRLF, as runs of consecutive line numbers (from 1), which a
/// CRLF file holds in one entry and an LF file in none.
#[derive(Debug, Default)]
pub(super) struct CrlfLines {
    runs: Vec<(u64, u64)>,
}

impl CrlfLines {
    fn add(&mut self, line: u64) {
        match self.runs.last_mut() {
            Some((_, last)) if *last + 1 == line => *last = line,
            _ => self.runs.push((line, line)),
        }
    }

    /// Whether `line` ended in CRLF.
    pub(super) fn has_cr(&self, line: u64) -> bool {
        self.runs
            .binary_search_by(|&(first, last)| {
                if line < first {
                    std::cmp::Ordering::Greater
                } else if line > last {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }

    /// How many of the lines before `line` ended in CRLF: the bytes taken out before
    /// that line began.
    pub(super) fn removed_before(&self, line: u64) -> u64 {
        self.runs
            .iter()
            .take_while(|&&(first, _)| first < line)
            .map(|&(first, last)| last.min(line - 1) - first + 1)
            .sum()
    }
}

/// A reader that drops the `\r` of every `\r\n`, noting the line it ended. The table
/// is shared with the printer, which reads it between the reader's calls.
pub(super) struct CrlfStripper<R> {
    inner: R,
    lines: Rc<RefCell<CrlfLines>>,
    raw: Vec<u8>,
    /// Stripped bytes not yet handed out.
    ready: Vec<u8>,
    given: usize,
    /// A `\r` that ended the last chunk: whether it was CRLF is not known yet.
    pending_cr: bool,
    /// The number of the line being read, from 1.
    line: u64,
    at_end: bool,
}

impl<R: Read> CrlfStripper<R> {
    pub(super) fn new(inner: R, lines: Rc<RefCell<CrlfLines>>) -> Self {
        Self {
            inner,
            lines,
            raw: vec![0; 64 * 1024],
            ready: Vec::with_capacity(64 * 1024),
            given: 0,
            pending_cr: false,
            line: 1,
            at_end: false,
        }
    }

    /// Strips `chunk` onto `ready`.
    fn strip(&mut self, chunk: &[u8]) {
        let mut lines = self.lines.borrow_mut();
        for &byte in chunk {
            if self.pending_cr {
                self.pending_cr = false;
                if byte == b'\n' {
                    lines.add(self.line);
                    self.line += 1;
                    self.ready.push(b'\n');
                    continue;
                }
                self.ready.push(b'\r');
            }
            match byte {
                b'\r' => self.pending_cr = true,
                b'\n' => {
                    self.line += 1;
                    self.ready.push(b'\n');
                }
                _ => self.ready.push(byte),
            }
        }
    }
}

impl<R: Read> Read for CrlfStripper<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.given == self.ready.len() {
            self.ready.clear();
            self.given = 0;
            while self.ready.is_empty() && !self.at_end {
                let n = self.inner.read(&mut self.raw)?;
                if n == 0 {
                    self.at_end = true;
                    if self.pending_cr {
                        self.pending_cr = false;
                        self.ready.push(b'\r');
                    }
                    break;
                }
                let chunk = std::mem::take(&mut self.raw);
                self.strip(&chunk[..n]);
                self.raw = chunk;
            }
        }
        let n = out.len().min(self.ready.len() - self.given);
        out[..n].copy_from_slice(&self.ready[self.given..self.given + n]);
        self.given += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Read as _;
    use std::rc::Rc;

    use super::{CrlfLines, CrlfStripper};

    fn strip_with<R: std::io::Read>(input: R) -> (Vec<u8>, CrlfLines) {
        let lines = Rc::new(RefCell::new(CrlfLines::default()));
        let mut out = Vec::new();
        CrlfStripper::new(input, Rc::clone(&lines))
            .read_to_end(&mut out)
            .unwrap_or_default();
        let lines = Rc::try_unwrap(lines)
            .map(RefCell::into_inner)
            .unwrap_or_default();
        (out, lines)
    }

    fn strip(input: &[u8]) -> (Vec<u8>, CrlfLines) {
        strip_with(input)
    }

    #[test]
    fn only_a_cr_before_lf_is_taken_out() {
        let (out, lines) = strip(b"a\r\nb\nc\rd\r\ne\r");
        assert_eq!(out, b"a\nb\nc\rd\ne\r");
        assert!(lines.has_cr(1));
        assert!(!lines.has_cr(2));
        assert!(lines.has_cr(3));
        assert!(!lines.has_cr(4));
        assert_eq!(lines.runs, vec![(1, 1), (3, 3)]);
    }

    #[test]
    fn runs_merge_and_count_the_bytes_removed_before_a_line() {
        let (out, lines) = strip(b"a\r\nb\r\nc\r\nd\n");
        assert_eq!(out, b"a\nb\nc\nd\n");
        assert_eq!(lines.runs, vec![(1, 3)]);
        assert_eq!(lines.removed_before(1), 0);
        assert_eq!(lines.removed_before(2), 1);
        assert_eq!(lines.removed_before(4), 3);
        assert_eq!(lines.removed_before(9), 3);
        assert_eq!(CrlfLines::default().removed_before(5), 0);
    }

    #[test]
    fn a_cr_split_across_reads_is_still_seen() {
        struct OneByte<'a>(&'a [u8]);
        impl std::io::Read for OneByte<'_> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let Some((&first, rest)) = self.0.split_first() else {
                    return Ok(0);
                };
                self.0 = rest;
                out[0] = first;
                Ok(1)
            }
        }
        let (out, lines) = strip_with(OneByte(b"ab\r\ncd\r"));
        assert_eq!(out, b"ab\ncd\r");
        assert_eq!(lines.runs, vec![(1, 1)]);
    }
}
