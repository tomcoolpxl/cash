//! What a rendering produces: cells in rows, rows in blocks.
//!
//! A cell is one terminal column. A wide character (a katakana in `smblock`, an emoji)
//! takes two: its own cell, then a [`Cell::TAIL`] that holds its place, so widths, wrapping
//! and smushing count columns, as libcaca's canvas does.

use crate::style::Style;

/// One terminal column: a character and how it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The character; [`Cell::TAIL_CHAR`] in the second column of a wide one.
    pub ch: char,
    /// Its colours and attributes.
    pub style: Style,
}

impl Cell {
    /// What the second column of a wide character holds.
    pub const TAIL_CHAR: char = '\0';

    /// The second column of a wide character.
    pub const TAIL: Self = Self {
        ch: Self::TAIL_CHAR,
        style: Style::PLAIN,
    };

    /// A blank, with no colour.
    pub const BLANK: Self = Self::plain(' ');

    /// `ch` with no colour.
    pub const fn plain(ch: char) -> Self {
        Self {
            ch,
            style: Style::PLAIN,
        }
    }

    /// Whether this is the second column of a wide character.
    pub const fn is_tail(&self) -> bool {
        self.ch == Self::TAIL_CHAR
    }

    /// Whether the character takes two columns.
    pub fn is_wide(&self) -> bool {
        !self.is_tail() && unicode_width::UnicodeWidthChar::width(self.ch) == Some(2)
    }
}

/// One line of output.
pub type Row = Vec<Cell>;

/// What one line of input became: as many rows as the font is high, or the rows a filter
/// left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    /// The rows, top first.
    pub rows: Vec<Row>,
}

impl Block {
    /// The widest row's width in columns.
    pub fn width(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// Every row padded with blanks to the widest row's width, so the block is a
    /// rectangle, as libcaca's canvas is.
    #[must_use]
    pub fn squared(&self) -> Self {
        let width = self.width();
        Self {
            rows: self
                .rows
                .iter()
                .map(|row| {
                    let mut row = row.clone();
                    row.resize(width, Cell::BLANK);
                    row
                })
                .collect(),
        }
    }
}

/// A whole rendering: a block per line of input, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Figure {
    /// The blocks, first line of input first.
    pub blocks: Vec<Block>,
}

impl Figure {
    /// Every row of every block, in order.
    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.blocks.iter().flat_map(|block| block.rows.iter())
    }
}

/// The cells of `text`, each wide character followed by its tail, all in `style`.
pub fn cells(text: &str, style: Style) -> Row {
    let mut row = Row::with_capacity(text.len());
    for ch in text.chars() {
        let cell = Cell { ch, style };
        row.push(cell);
        if cell.is_wide() {
            row.push(Cell::TAIL);
        }
    }
    row
}
