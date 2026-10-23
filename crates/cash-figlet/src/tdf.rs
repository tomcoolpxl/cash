//! TheDraw fonts (`.tdf`): the colour fonts of DOS's ANSI art, read into a [`Font`]
//! (decisions.md D84).
//!
//! A file starts `\x13TheDraw FONTS file\x1a`; each font in it starts with the bytes
//! `55 AA 00 FF`, a name (a length and twelve bytes), four reserved bytes, its type (0
//! outline, 1 block, 2 colour), the blank columns after each letter, the size of its data,
//! and where in the data each of the 94 characters from `!` to `~` starts (`FFFF` for
//! none). A character is its width and height, then its cells row by row, a carriage
//! return ending a row and a NUL the character: a byte of code page 437 each, and for a
//! colour font an attribute after it, the PC's colour (low four bits the foreground, high
//! four the background, in the PC's order).
//!
//! Colour and block fonts are read; outline fonts, whose letters name the parts of a
//! frame drawn in a style chosen when printing, are not. A blank, which the fonts do not
//! have, is three columns wide. The first font in a file is the one read.

use std::collections::HashMap;
use std::fmt;

use crate::canvas::{Cell, Row};
use crate::export::cp437_char;
use crate::font::{Font, Glyph, Header, Signature};
use crate::style::{Color, Style};

/// Why a file is not a TheDraw font cash reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TdfError {
    /// Not a TheDraw font file, or one cut short.
    NotAFont,
    /// An outline font.
    Outline,
}

impl fmt::Display for TdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAFont => f.write_str("Not a TheDraw font file"),
            Self::Outline => {
                f.write_str("TheDraw outline fonts are not read, only colour and block fonts")
            }
        }
    }
}

impl std::error::Error for TdfError {}

/// The bytes a TheDraw font file starts with.
const MAGIC: &[u8] = b"\x13TheDraw FONTS file\x1a";

/// Whether `bytes` are a TheDraw font file.
pub fn is_tdf(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// The PC's sixteen colours, in its order (blue before red), as ANSI's.
const PC_TO_ANSI: [u8; 16] = [0, 4, 2, 6, 1, 5, 3, 7, 8, 12, 10, 14, 9, 13, 11, 15];

/// The style an attribute byte gives: its foreground, and its background unless black,
/// which is left to the terminal.
fn attribute(byte: u8) -> Style {
    let fg = PC_TO_ANSI
        .get(usize::from(byte & 0x0f))
        .copied()
        .unwrap_or(7);
    let bg = PC_TO_ANSI
        .get(usize::from((byte >> 4) & 0x07))
        .copied()
        .unwrap_or(0);
    Style {
        fg: Some(Color::Ansi(fg)),
        bg: (bg != 0).then_some(Color::Ansi(bg)),
        ..Style::PLAIN
    }
}

/// Reads the first font of a TheDraw font file.
pub fn parse(bytes: &[u8]) -> Result<Font, TdfError> {
    parse_for(bytes, None)
}

/// Reads the first font of a TheDraw font file, with only the characters `chars`.
///
/// The font is as high as the tallest of those: a TheDraw font's letters differ in
/// height, and a line is as high as the letters on it, as TheDraw and tdfiglet draw it,
/// not as the tallest letter in the font. With no `chars`, every character is read.
pub fn parse_for(bytes: &[u8], chars: Option<&[char]>) -> Result<Font, TdfError> {
    if !is_tdf(bytes) {
        return Err(TdfError::NotAFont);
    }
    let byte = |at: usize| bytes.get(at).copied().ok_or(TdfError::NotAFont);
    let word =
        |at: usize| -> Result<u16, TdfError> { Ok(u16::from_le_bytes([byte(at)?, byte(at + 1)?])) };
    if bytes.get(20..24) != Some(&[0x55, 0xaa, 0x00, 0xff][..]) {
        return Err(TdfError::NotAFont);
    }
    let kind = byte(41)?;
    let spacing = usize::from(byte(42)?);
    let colour = match kind {
        1 => false,
        2 => true,
        0 => return Err(TdfError::Outline),
        _ => return Err(TdfError::NotAFont),
    };
    let data = 233;
    let mut glyphs: HashMap<u32, Glyph> = HashMap::new();
    for (index, ch) in ('!'..='~').enumerate() {
        let offset = word(45 + 2 * index)?;
        if offset == 0xffff || chars.is_some_and(|chars| !chars.contains(&ch)) {
            continue;
        }
        let at = data + usize::from(offset);
        let width = usize::from(byte(at)?);
        let mut rows: Vec<Row> = vec![Vec::new()];
        let mut p = at + 2;
        loop {
            let cell = byte(p)?;
            p += 1;
            match cell {
                0 => break,
                b'\r' => rows.push(Vec::new()),
                _ => {
                    let style = if colour {
                        let attr = byte(p)?;
                        p += 1;
                        attribute(attr)
                    } else {
                        Style::PLAIN
                    };
                    let ch = if cell < 0x20 { ' ' } else { cp437_char(cell) };
                    if let Some(row) = rows.last_mut() {
                        row.push(Cell { ch, style });
                    }
                }
            }
        }
        for row in &mut rows {
            row.resize(width + spacing, Cell::BLANK);
        }
        glyphs.insert(u32::from(ch), Glyph { rows });
    }
    let height = glyphs.values().map(|g| g.rows.len()).max().unwrap_or(1);
    for glyph in glyphs.values_mut() {
        let width = glyph.width();
        glyph.rows.resize(height, vec![Cell::BLANK; width]);
    }
    glyphs.insert(
        u32::from(' '),
        Glyph {
            rows: vec![vec![Cell::BLANK; 3 + spacing]; height],
        },
    );
    let header = Header {
        signature: Signature::Tlf,
        hardblank: '\u{fffe}',
        height,
        baseline: i32::try_from(height).unwrap_or(1),
        max_length: 0,
        old_layout: -1,
        comment_lines: 0,
        print_direction: Some(0),
        full_layout: Some(0),
    };
    Ok(Font::from_glyphs(header, glyphs))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{TdfError, parse};
    use crate::style::Color;

    /// A colour font with two characters: `A`, two cells red on blue over a white
    /// half block, and `B`, one green cell.
    pub(crate) fn tiny_tdf(kind: u8) -> Vec<u8> {
        let mut file = b"\x13TheDraw FONTS file\x1a".to_vec();
        file.extend_from_slice(&[0x55, 0xaa, 0x00, 0xff]);
        file.push(4);
        file.extend_from_slice(b"TINY\0\0\0\0\0\0\0\0");
        file.extend_from_slice(&[0, 0, 0, 0]);
        file.push(kind);
        file.push(1); // spacing
        let a: Vec<u8> = vec![2, 2, 0xdb, 0x14, 0xdb, 0x14, b'\r', 0xdf, 0x0f, 0];
        let b: Vec<u8> = vec![1, 1, b'#', 0x02, 0];
        let size = u16::try_from(a.len() + b.len()).unwrap();
        file.extend_from_slice(&size.to_le_bytes());
        for ch in '!'..='~' {
            let offset: u16 = match ch {
                'A' => 0,
                'B' => u16::try_from(a.len()).unwrap(),
                _ => 0xffff,
            };
            file.extend_from_slice(&offset.to_le_bytes());
        }
        file.extend(a);
        file.extend(b);
        file
    }

    #[test]
    fn a_colour_font() {
        let font = parse(&tiny_tdf(2)).unwrap();
        assert_eq!(font.header.height, 2);
        let a = font.glyph(u32::from('A')).unwrap();
        // Two columns and the font's one of spacing.
        assert_eq!(a.width(), 3);
        assert_eq!(a.rows[0][0].ch, '█');
        assert_eq!(a.rows[0][0].style.fg, Some(Color::Ansi(1)));
        assert_eq!(a.rows[0][0].style.bg, Some(Color::Ansi(4)));
        assert_eq!(a.rows[1][0].ch, '▀');
        assert_eq!(a.rows[1][0].style.bg, None);
        let b = font.glyph(u32::from('B')).unwrap();
        assert_eq!(b.rows.len(), 2, "every glyph as high as the font");
        assert!(font.glyph(u32::from(' ')).is_some(), "a blank is made");
    }

    #[test]
    fn a_line_is_as_high_as_its_letters() {
        // B is one row high; A two.
        let font = super::parse_for(&tiny_tdf(2), Some(&['B'])).unwrap();
        assert_eq!(font.header.height, 1);
        assert!(font.glyph(u32::from('A')).is_none());
        let font = super::parse_for(&tiny_tdf(2), Some(&['A', 'B'])).unwrap();
        assert_eq!(font.header.height, 2);
    }

    #[test]
    fn outline_fonts_and_other_files_are_refused() {
        assert_eq!(parse(&tiny_tdf(0)).err(), Some(TdfError::Outline));
        assert_eq!(parse(b"flf2a$ 1 1").err(), Some(TdfError::NotAFont));
        let mut cut = tiny_tdf(2);
        cut.truncate(100);
        assert_eq!(parse(&cut).err(), Some(TdfError::NotAFont));
    }
}
