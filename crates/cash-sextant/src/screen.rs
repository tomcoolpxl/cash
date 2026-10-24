//! The terminal side: what is on screen, and the bytes that bring it up to a new frame.
//!
//! Only cells that changed are written, a cursor move only where the next changed cell is
//! not the one after the last, a colour only where it differs from the one in force. The
//! frame goes out in one write, inside a synchronized update (DEC mode 2026), so the
//! terminal never shows half of it; a terminal without the mode ignores it.
//!
//! A cell's pattern is drawn as a Unicode sextant, or, with the soft font, as the ASCII
//! character the soft font gives that pattern, the soft font selected for the canvas only.
//! The escapes are written here, not through crossterm's styles, whose colour switch is
//! the whole process's: the colours are the picture, whatever `NO_COLOR` says.

use std::fmt::Write as _;
use std::io::{self, Write};

use crate::canvas::{self, Canvas, Cell, EMPTY, Rgb};
use crate::soft_font;

pub(crate) struct Screen {
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    /// Whether patterns are drawn with the soft font, and whether it has been sent.
    soft_font: bool,
    soft_font_sent: bool,
    /// What the terminal shows, `None` where it is not known.
    shown: Vec<Option<Cell>>,
    /// The text row above the canvas, and what of it is shown.
    text: String,
    text_shown: Option<String>,
    buffer: String,
}

/// The number of text rows above the canvas.
pub(crate) const TEXT_ROWS: usize = 1;

/// The text row's colour.
const TEXT_COLOUR: Rgb = 0x00c8_d0e0;

impl Screen {
    pub(crate) fn new(cols: usize, rows: usize, soft_font: bool) -> Self {
        let canvas_rows = rows.saturating_sub(TEXT_ROWS);
        Self {
            cols,
            rows,
            soft_font,
            soft_font_sent: false,
            shown: vec![None; cols * canvas_rows],
            text: String::new(),
            text_shown: None,
            buffer: String::with_capacity(64 * 1024),
        }
    }

    /// A canvas that fills the screen below the text row.
    pub(crate) fn canvas(&self) -> Canvas {
        Canvas::new(self.cols, self.rows.saturating_sub(TEXT_ROWS))
    }

    pub(crate) fn set_text(&mut self, text: String) {
        self.text = text;
    }

    /// Bring the terminal up to `canvas` and the text row; the bytes written.
    pub(crate) fn present(&mut self, canvas: &Canvas, out: &mut impl Write) -> io::Result<usize> {
        self.buffer.clear();
        let b = &mut self.buffer;
        b.push_str("\x1b[?2026h");
        if self.soft_font && !self.soft_font_sent {
            b.push_str(&soft_font::load());
            self.soft_font_sent = true;
        }

        if self.text_shown.as_ref() != Some(&self.text) {
            let mut line: String = self.text.chars().take(self.cols).collect();
            let width = line.chars().count();
            line.extend(std::iter::repeat_n(' ', self.cols - width));
            b.push_str("\x1b[1;1H\x1b[0m");
            foreground(b, TEXT_COLOUR);
            b.push_str(&line);
            self.text_shown = Some(self.text.clone());
        }

        let canvas_rows = canvas.height / 3;
        let (mut fg, mut bg): (Option<Rgb>, Option<Rgb>) = (None, None);
        let mut cursor: Option<(usize, usize)> = None;
        if self.soft_font {
            b.push_str(soft_font::SELECT);
        }
        for row in 0..canvas_rows {
            for col in 0..self.cols {
                let cell = canvas.cell(col, row);
                let index = row * self.cols + col;
                if self.shown[index] == Some(cell) {
                    continue;
                }
                if cursor != Some((col, row)) {
                    let _ = write!(b, "\x1b[{};{}H", row + TEXT_ROWS + 1, col + 1);
                }
                if bg != Some(cell.bg) {
                    background(b, cell.bg);
                    bg = Some(cell.bg);
                }
                if cell.mask != 0 && fg != Some(cell.fg) {
                    foreground(b, cell.fg);
                    fg = Some(cell.fg);
                }
                b.push(if self.soft_font {
                    soft_font::character(cell.mask)
                } else {
                    canvas::glyph(cell.mask)
                });
                self.shown[index] = Some(cell);
                // The last column leaves the cursor where the terminal pleases.
                cursor = (col + 1 < self.cols).then_some((col + 1, row));
            }
        }
        if self.soft_font {
            b.push_str(soft_font::DESELECT);
        }

        b.push_str("\x1b[?2026l");
        out.write_all(b.as_bytes())?;
        out.flush()?;
        Ok(b.len())
    }
}

fn foreground(b: &mut String, rgb: Rgb) {
    if rgb == EMPTY {
        b.push_str("\x1b[39m");
    } else {
        let _ = write!(
            b,
            "\x1b[38;2;{};{};{}m",
            rgb >> 16 & 0xff,
            rgb >> 8 & 0xff,
            rgb & 0xff
        );
    }
}

fn background(b: &mut String, rgb: Rgb) {
    if rgb == EMPTY {
        b.push_str("\x1b[49m");
    } else {
        let _ = write!(
            b,
            "\x1b[48;2;{};{};{}m",
            rgb >> 16 & 0xff,
            rgb >> 8 & 0xff,
            rgb & 0xff
        );
    }
}
