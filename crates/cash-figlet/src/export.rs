//! Writing a figure out.
//!
//! `text` is figlet's own output: the characters, each row ending in a newline, the
//! blanks a glyph ends with kept, no colour.

use crate::canvas::{Block, Figure, Row};

/// The figure as plain text.
pub fn text(figure: &Figure) -> String {
    text_rows(figure.rows())
}

/// Blocks as plain text, for output written as each line of input is rendered.
pub fn text_blocks(blocks: &[Block]) -> String {
    text_rows(blocks.iter().flat_map(|block| block.rows.iter()))
}

fn text_rows<'a>(rows: impl Iterator<Item = &'a Row>) -> String {
    let mut out = String::new();
    for row in rows {
        out.extend(
            row.iter()
                .filter(|cell| !cell.is_tail())
                .map(|cell| cell.ch),
        );
        out.push('\n');
    }
    out
}
