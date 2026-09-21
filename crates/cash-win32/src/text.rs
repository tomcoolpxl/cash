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
    let mut end = text.len();
    loop {
        let rest = &text[..end];
        if let Some(stripped) = rest.strip_suffix('\n') {
            end = stripped.len();
            // A \r immediately before the \n is part of the terminator, not the data.
            if let Some(stripped) = stripped.strip_suffix('\r') {
                end = stripped.len();
            }
        } else {
            break;
        }
    }
    &text[..end]
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
        let trimmed = text.strip_suffix('\n').map_or(text, |t| t.strip_suffix('\r').unwrap_or(t));
        Box::new(trimmed.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)))
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
