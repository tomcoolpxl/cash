//! Line endings and byte-order marks — **D20**, **D41**.
//!
//! D20's rule, in one sentence:
//!
//! > Wherever cash itself decides where a line ends, `\r\n` terminates a line exactly as
//! > `\n` does.
//!
//! That is deliberately narrow. Bytes flowing through a pipe between two external
//! programs are never touched — `a.exe | b.exe` stays byte-transparent. Only the places
//! where cash parses output into shell values are affected: `$(...)`, `read`, `mapfile`,
//! here-strings and here-documents.
//!
//! §9 measured the status quo: `v=$(cat crlf.txt)` yields four bytes, `val\r`, and
//! `[ "$v" = "val" ]` then fails while printing identically, because the `\r` merely
//! returns the cursor. Python, .NET, classic Win32 tools and `cmd.exe` all emit CRLF and
//! cannot be fixed at source.

/// The UTF-8 byte-order mark that Windows editors like to prepend.
pub const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Strip a leading UTF-8 BOM, if present (D41).
///
/// Without this, a script saved by Notepad fails immediately: the shebang is not
/// recognised and the first token carries three invisible bytes. §9 measured exactly
/// that — `command not found: ﻿echo`.
#[must_use]
pub fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(BOM).unwrap_or(bytes)
}

/// Strip a leading UTF-8 BOM from a string slice.
#[must_use]
pub fn strip_bom_str(text: &str) -> &str {
    text.strip_prefix('\u{FEFF}').unwrap_or(text)
}

/// Trim trailing newlines for command substitution (D20).
///
/// Bash strips trailing `\n` from `$(...)`; cash strips the `\r` that would otherwise be
/// left behind. Applies repeatedly, as bash does — `$(printf 'x\n\n\n')` is `x`.
///
/// This is safe precisely because `$(...)` is *already* not byte-transparent in bash: it
/// strips trailing newlines and drops NUL bytes, so nothing legitimate passes binary
/// through it.
#[must_use]
pub fn trim_substitution_output(text: &str) -> &str {
    let mut rest = text;
    while let Some(stripped) = rest.strip_suffix('\n') {
        // A \r immediately before the \n is part of the terminator, not the data.
        rest = stripped.strip_suffix('\r').unwrap_or(stripped);
    }
    rest
}

/// Strip one line terminator from the end of a line read by `read` or `mapfile` (D20).
///
/// Handles `\r\n` and bare `\n`. A lone trailing `\r` with no `\n` is left alone: that is
/// data, not a terminator, and stripping it would break tools that emit progress output.
#[must_use]
pub fn trim_line_terminator(line: &str) -> &str {
    line.strip_suffix('\n')
        .map_or(line, |l| l.strip_suffix('\r').unwrap_or(l))
}

/// Split input into lines, treating `\r\n` and `\n` alike (D20).
///
/// Used by `mapfile`/`readarray` and by `while read` loops. A trailing terminator does
/// not produce a final empty line, matching bash.
pub fn split_lines(text: &str) -> impl Iterator<Item = &str> {
    // Emptiness is judged on the *original* input, not the trimmed one. Input of "\n"
    // is a single empty line, exactly as `mapfile < <(printf '\n')` gives in bash;
    // only genuinely empty input has no lines at all.
    let iter: Box<dyn Iterator<Item = &str>> = if text.is_empty() {
        Box::new(std::iter::empty())
    } else {
        let trimmed = text
            .strip_suffix('\n')
            .map_or(text, |t| t.strip_suffix('\r').unwrap_or(t));
        Box::new(
            trimmed
                .split('\n')
                .map(|line| line.strip_suffix('\r').unwrap_or(line)),
        )
    };
    iter
}

/// Whether a script's bytes look like they were saved with CRLF endings.
///
/// Useful for diagnostics — `cash doctor` (D35) can say so rather than leaving the user
/// to wonder — but not for behaviour: D7 tolerates CRLF unconditionally, with no mode.
#[must_use]
pub fn looks_crlf(bytes: &[u8]) -> bool {
    bytes.windows(2).any(|w| w == b"\r\n")
}

/// Rewrite `\r\n` to `\n` in *script source*, borrowing when there is nothing to do.
///
/// This is D7's "CRLF tolerated in script parsing", which is a different rule from D20's
/// "CRLF terminates a line in data" even though both come down to the same two bytes.
/// The reason it is not optional: `core.autocrlf` is `true` by default in Git for
/// Windows, so a repository checked out here has CRLF script files, and a shell that
/// cannot run them cannot run anything the user already has.
///
/// A lone `\r` is left alone — it is data inside a string literal, not a terminator.
#[must_use]
pub fn normalize_crlf(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains("\r\n") {
        std::borrow::Cow::Owned(text.replace("\r\n", "\n"))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

/// A `Read` adapter that applies [`normalize_crlf`] to a stream.
///
/// The parser reads script source incrementally, so the normalisation has to be
/// streaming too — a script may arrive on stdin, and holding all of it in memory to run
/// a replace over it would be the wrong shape. The only state needed is a single
/// carriage return held back across a chunk boundary, because whether to emit it depends
/// on the byte after it, which may not have arrived yet.
pub struct NormalizeCrlf<R> {
    inner: R,
    buffer: Box<[u8]>,
    filled: usize,
    position: usize,
    /// A `\r` was consumed and its fate is undecided: it is dropped if the next byte is
    /// `\n` and emitted otherwise, including at end of input.
    held_return: bool,
    at_end: bool,
}

impl<R: std::io::Read> NormalizeCrlf<R> {
    /// Wrap a reader, normalising `\r\n` to `\n` as it is read.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: vec![0u8; 8192].into_boxed_slice(),
            filled: 0,
            position: 0,
            held_return: false,
            at_end: false,
        }
    }
}

impl<R: std::io::Read> std::io::Read for NormalizeCrlf<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }

        let mut written = 0usize;
        loop {
            // Refill only while nothing has been produced yet. Once there are bytes to
            // hand back, returning a short read is correct and blocking a pipe for more
            // is not.
            if self.position >= self.filled && !self.at_end && written == 0 {
                self.filled = self.inner.read(&mut self.buffer)?;
                self.position = 0;
                if self.filled == 0 {
                    self.at_end = true;
                }
            }

            if self.held_return {
                if self.position < self.filled {
                    self.held_return = false;
                    if self.buffer[self.position] != b'\n' {
                        // Data, not a terminator: give the `\r` back.
                        out[written] = b'\r';
                        written += 1;
                        if written == out.len() {
                            return Ok(written);
                        }
                        continue;
                    }
                    // Fall through and emit the `\n` that follows; the `\r` is dropped.
                } else if self.at_end {
                    self.held_return = false;
                    out[written] = b'\r';
                    return Ok(written + 1);
                } else {
                    // Undecidable without more input, and something is already written.
                    return Ok(written);
                }
            }

            if self.position >= self.filled {
                return Ok(written);
            }

            let byte = self.buffer[self.position];
            self.position += 1;
            if byte == b'\r' {
                self.held_return = true;
                continue;
            }

            out[written] = byte;
            written += 1;
            if written == out.len() {
                return Ok(written);
            }
        }
    }
}
