//! Writing a figure out.
//!
//! `text` is figlet's own output: the characters, each row ending in a newline, the
//! blanks a glyph ends with kept, no colour.

use crate::canvas::Figure;

/// The figure as plain text.
pub fn text(figure: &Figure) -> String {
    let mut out = String::new();
    for row in figure.rows() {
        out.extend(
            row.iter()
                .filter(|cell| !cell.is_tail())
                .map(|cell| cell.ch),
        );
        out.push('\n');
    }
    out
}
