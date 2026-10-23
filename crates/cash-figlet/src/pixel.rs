//! Pixel fonts: glyphs as bitmaps, drawn with block characters that put several pixels
//! in one cell (decisions.md D84).
//!
//! A bitmap glyph comes from font8x8 (8 by 8, built in) or from an installed Windows font
//! drawn by the builtin; [`font`] turns a set of them into a [`Font`] whose glyphs are
//! block characters, so the renderer lays them out, wraps and justifies them like any
//! other, and every filter applies. How many pixels a cell holds is the [`Mode`]:
//!
//! - `full`: one, `█`, in every font;
//! - `half`: one by two, `▀ ▄ █`, in every font;
//! - `quadrant`: two by two, `▘ ▚ ▟`, in every font Windows ships with block elements;
//! - `sextant`: two by three, Unicode 13's legacy computing block, which Windows Terminal
//!   draws itself and Cascadia Code has;
//! - `octant`: two by four, Unicode 16's, in Cascadia Code from 2404;
//! - `braille`: two by four, as Braille dots, which most fonts have.

use std::collections::HashMap;

use crate::canvas::{Cell, Row};
use crate::font::{Font, Glyph, Header, Signature};
use crate::pixel_data::{BASIC, LATIN, OCTANTS};

/// How many pixels a cell holds, and with which characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// One pixel to a cell.
    Full,
    /// One by two.
    #[default]
    Half,
    /// Two by two.
    Quadrant,
    /// Two by three.
    Sextant,
    /// Two by four.
    Octant,
    /// Two by four, as Braille.
    Braille,
}

impl Mode {
    /// Every mode, by the name `--pixel` takes.
    pub const NAMES: &[(&str, Self)] = &[
        ("full", Self::Full),
        ("half", Self::Half),
        ("quadrant", Self::Quadrant),
        ("sextant", Self::Sextant),
        ("octant", Self::Octant),
        ("braille", Self::Braille),
    ];

    /// The pixels across and down one cell holds.
    pub const fn cell(self) -> (usize, usize) {
        match self {
            Self::Full => (1, 1),
            Self::Half => (1, 2),
            Self::Quadrant => (2, 2),
            Self::Sextant => (2, 3),
            Self::Octant | Self::Braille => (2, 4),
        }
    }

    /// The character for a cell whose pixels are the bits of `pattern`: bit 0 the top
    /// left, then left to right and top to bottom.
    pub fn char_for(self, pattern: u8) -> char {
        match self {
            Self::Full => {
                if pattern & 1 == 0 {
                    ' '
                } else {
                    '█'
                }
            }
            Self::Half => [' ', '▀', '▄', '█']
                .get(usize::from(pattern & 3))
                .copied()
                .unwrap_or(' '),
            Self::Quadrant => [
                ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
            ]
            .get(usize::from(pattern & 15))
            .copied()
            .unwrap_or(' '),
            Self::Sextant => sextant(pattern & 63),
            Self::Octant => OCTANTS.get(usize::from(pattern)).copied().unwrap_or(' '),
            Self::Braille => braille(pattern),
        }
    }
}

/// Unicode's sextant for a two-by-three pattern: U+1FB00 onward, less the four patterns
/// other characters draw (blank, left half, right half, full).
fn sextant(pattern: u8) -> char {
    match pattern {
        0 => ' ',
        21 => '▌',
        42 => '▐',
        63 => '█',
        _ => {
            let skipped = u32::from(pattern > 21) + u32::from(pattern > 42);
            char::from_u32(0x1FB00 + u32::from(pattern) - 1 - skipped).unwrap_or('?')
        }
    }
}

/// The Braille cell for a two-by-four pattern, whose dots are numbered down the left
/// column (1, 2, 3, 7) and then the right (4, 5, 6, 8).
fn braille(pattern: u8) -> char {
    // Pattern bit (left to right, top to bottom) to Braille dot bit.
    const DOTS: [u8; 8] = [0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80];
    let mut dots = 0_u32;
    for (bit, dot) in DOTS.iter().enumerate() {
        if pattern & (1 << bit) != 0 {
            dots |= u32::from(*dot);
        }
    }
    char::from_u32(0x2800 + dots).unwrap_or(' ')
}

/// A glyph as pixels, on or off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    /// Pixels across.
    pub width: usize,
    /// Pixels down.
    pub height: usize,
    /// The pixels, row by row.
    pub pixels: Vec<bool>,
}

impl Bitmap {
    /// Whether the pixel at `x`, `y` is on; outside the bitmap, off.
    pub fn get(&self, x: usize, y: usize) -> bool {
        x < self.width
            && self
                .pixels
                .get(y * self.width + x)
                .copied()
                .unwrap_or(false)
    }

    /// The rows of cells that draw it in `mode`.
    pub fn rows(&self, mode: Mode) -> Vec<Row> {
        let (across, down) = mode.cell();
        let columns = self.width.div_ceil(across);
        let rows = self.height.div_ceil(down);
        (0..rows)
            .map(|row| {
                (0..columns)
                    .map(|column| {
                        let mut pattern = 0_u8;
                        for dy in 0..down {
                            for dx in 0..across {
                                if self.get(column * across + dx, row * down + dy) {
                                    pattern |= 1 << (dy * across + dx);
                                }
                            }
                        }
                        Cell::plain(mode.char_for(pattern))
                    })
                    .collect()
            })
            .collect()
    }
}

impl Bitmap {
    /// A bitmap from how much each pixel is covered (0 to 255), on where it is at least
    /// half.
    pub fn from_coverage(width: usize, height: usize, coverage: &[u8]) -> Self {
        Self {
            width,
            height,
            pixels: (0..width * height)
                .map(|i| coverage.get(i).is_some_and(|&c| c >= 128))
                .collect(),
        }
    }
}

/// The rows blank in every glyph taken off the top and bottom of all of them, as a
/// font's line keeps room for accents and descenders a word may not have.
pub fn trim(glyphs: &mut [(char, Bitmap)]) {
    let used = |bitmap: &Bitmap, y: usize| (0..bitmap.width).any(|x| bitmap.get(x, y));
    let height = glyphs.iter().map(|(_, b)| b.height).max().unwrap_or(0);
    let top = (0..height).find(|&y| glyphs.iter().any(|(_, b)| used(b, y)));
    let bottom = (0..height)
        .rev()
        .find(|&y| glyphs.iter().any(|(_, b)| used(b, y)));
    let (Some(top), Some(bottom)) = (top, bottom) else {
        return;
    };
    for (_, bitmap) in glyphs.iter_mut() {
        let rows = bottom + 1 - top;
        let pixels = (top..=bottom)
            .flat_map(|y| (0..bitmap.width).map(move |x| (x, y)))
            .map(|(x, y)| bitmap.get(x, y))
            .collect();
        *bitmap = Bitmap {
            width: bitmap.width,
            height: rows,
            pixels,
        };
    }
}

/// font8x8's glyph for `ch`: printable ASCII and Latin-1's upper half.
pub fn font8x8(ch: char) -> Option<Bitmap> {
    let code = u32::from(ch);
    let rows = match code {
        0x20..=0x7e => BASIC.get(usize::try_from(code - 0x20).ok()?)?,
        0xa0..=0xff => LATIN.get(usize::try_from(code - 0xa0).ok()?)?,
        _ => return None,
    };
    Some(Bitmap {
        width: 8,
        height: 8,
        pixels: rows
            .iter()
            .flat_map(|row| (0..8).map(move |x| row & (1 << x) != 0))
            .collect(),
    })
}

/// A font of block characters from bitmap glyphs, drawn in `mode`: every glyph as high as
/// the tallest, laid out at full width (the bitmaps keep their own spacing).
pub fn font(glyphs: impl IntoIterator<Item = (char, Bitmap)>, mode: Mode) -> Font {
    let mut drawn: HashMap<u32, Glyph> = HashMap::new();
    let mut height = 1;
    for (ch, bitmap) in glyphs {
        let rows = bitmap.rows(mode);
        height = height.max(rows.len());
        drawn.insert(u32::from(ch), Glyph { rows });
    }
    for glyph in drawn.values_mut() {
        let width = glyph.width();
        glyph.rows.resize(height, vec![Cell::BLANK; width]);
    }
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
    Font::from_glyphs(header, drawn)
}

/// The font8x8 font for `chars`, drawn in `mode`.
pub fn font8x8_font(mode: Mode, chars: &[char]) -> Font {
    let mut glyphs: Vec<(char, Bitmap)> = chars
        .iter()
        .filter_map(|&ch| font8x8(ch).map(|bitmap| (ch, bitmap)))
        .collect();
    trim(&mut glyphs);
    font(glyphs, mode)
}

/// The characters a font made from drawn glyphs needs for `text`.
///
/// Its own and a blank, so that rows none of them uses are trimmed away; with no text
/// (standard input, read as it comes), printable ASCII and Latin-1's upper half.
pub fn wanted_chars(text: &str) -> Vec<char> {
    let mut chars: Vec<char> = if text.is_empty() {
        (' '..='~').chain('\u{a0}'..='\u{ff}').collect()
    } else {
        text.chars()
            .filter(|c| !c.is_control())
            .chain(std::iter::once(' '))
            .collect()
    };
    chars.sort_unstable();
    chars.dedup();
    chars
}

#[cfg(test)]
mod tests {
    use super::{Bitmap, Mode, font8x8, font8x8_font, sextant};

    fn bitmap(rows: &[&str]) -> Bitmap {
        Bitmap {
            width: rows.first().map_or(0, |r| r.len()),
            height: rows.len(),
            pixels: rows
                .iter()
                .flat_map(|r| r.chars().map(|c| c == '#'))
                .collect(),
        }
    }

    fn text(bitmap: &Bitmap, mode: Mode) -> Vec<String> {
        bitmap
            .rows(mode)
            .iter()
            .map(|row| row.iter().map(|c| c.ch).collect())
            .collect()
    }

    #[test]
    fn modes() {
        let b = bitmap(&["#.", "##", ".#", "##"]);
        assert_eq!(text(&b, Mode::Full), ["█ ", "██", " █", "██"]);
        assert_eq!(text(&b, Mode::Half), ["█▄", "▄█"]);
        assert_eq!(text(&b, Mode::Quadrant), ["▙", "▟"]);
        // BRAILLE PATTERN DOTS-125678, and BLOCK OCTANT-134678.
        assert_eq!(text(&b, Mode::Braille), ["\u{28f3}"]);
        assert_eq!(text(&b, Mode::Octant), ["\u{1cdd8}"]);
        let six = bitmap(&["#.", ".#", "#."]);
        // BLOCK SEXTANT-145.
        assert_eq!(text(&six, Mode::Sextant), ["\u{1fb17}"]);
    }

    #[test]
    fn sextants_skip_the_halves() {
        assert_eq!(sextant(1), '\u{1fb00}');
        assert_eq!(sextant(20), '\u{1fb13}');
        assert_eq!(sextant(21), '▌');
        assert_eq!(sextant(22), '\u{1fb14}');
        assert_eq!(sextant(62), '\u{1fb3b}');
    }

    #[test]
    fn trimming_takes_off_rows_blank_everywhere() {
        let mut glyphs = vec![
            ('a', bitmap(&["..", "#.", "..", ".."])),
            ('b', bitmap(&["..", "..", ".#", ".."])),
        ];
        super::trim(&mut glyphs);
        assert_eq!(glyphs[0].1, bitmap(&["#.", ".."]));
        assert_eq!(glyphs[1].1, bitmap(&["..", ".#"]));
        let covered = Bitmap::from_coverage(2, 1, &[200, 10]);
        assert_eq!(covered, bitmap(&["#."]));
    }

    #[test]
    fn font8x8_letters() {
        let a = font8x8('A').unwrap();
        // The top row of an A: two pixels near the left.
        assert!(a.get(2, 0) && a.get(3, 0) && !a.get(0, 0));
        assert!(font8x8('\u{263a}').is_none());
        let font = font8x8_font(Mode::Half, &super::wanted_chars(""));
        assert_eq!(font.header.height, 4);
        assert_eq!(font.glyph(u32::from('A')).unwrap().width(), 8);
        let quad = font8x8_font(Mode::Octant, &super::wanted_chars(""));
        assert_eq!(quad.header.height, 2);
        assert_eq!(quad.glyph(u32::from('é')).unwrap().width(), 4);
    }
}
