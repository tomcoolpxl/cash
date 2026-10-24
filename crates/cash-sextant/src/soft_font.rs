//! The 63 non-blank cell patterns as a DEC soft font (DECDLD), for terminals whose fonts
//! have no sextants: the terminal is sent a bitmap for each, and draws it in place of an
//! ASCII character while the soft font is the selected character set. Still text: each
//! cell keeps its two colours.
//!
//! The patterns go to characters `!` (0x21) to `_` (0x5F), the pattern's mask plus 0x20,
//! in the 94-character set named ` @`. Each bitmap is a full cell (no gap between cells)
//! of 10x16 pixels, the cell of an 80-column, 24-line terminal, which the terminal scales
//! to its own cell; the three rows of the pattern are 5, 6 and 5 pixels high.

/// Select the soft font as G0.
pub(crate) const SELECT: &str = "\x1b( @";

/// Select ASCII as G0 again.
pub(crate) const DESELECT: &str = "\x1b(B";

const WIDTH: usize = 10;
const HEIGHT: usize = 16;

/// The character that shows `mask` while the soft font is selected.
pub(crate) fn character(mask: u8) -> char {
    char::from(0x20 + (mask & 0x3f))
}

/// The DECDLD sequence that loads the patterns.
pub(crate) fn load() -> String {
    // Font 0, from character 1 (0x21), erase the font first, 10 pixels wide, an 80x24
    // cell, full-cell glyphs, 16 high, a 94-character set; named ` @`.
    let mut sequence = format!("\x1bP0;1;0;{WIDTH};0;2;{HEIGHT};0{{ @");
    let bitmaps: Vec<String> = (1..=0x3f).map(bitmap).collect();
    sequence.push_str(&bitmaps.join(";"));
    sequence.push_str("\x1b\\");
    sequence
}

/// Whether the pixel at `x`, `y` of the cell's bitmap is lit for `mask`.
fn lit(mask: u8, x: usize, y: usize) -> bool {
    let row = match y {
        0..5 => 0,
        5..11 => 1,
        _ => 2,
    };
    let column = usize::from(x >= WIDTH / 2);
    mask & (1 << (row * 2 + column)) != 0
}

/// The sixel data of one pattern: bands of six pixel rows, separated by `/`.
fn bitmap(mask: u8) -> String {
    (0..HEIGHT.div_ceil(6))
        .map(|band| {
            (0..WIDTH)
                .map(|x| {
                    let bits = (0..6)
                        .filter(|bit| {
                            let y = band * 6 + bit;
                            y < HEIGHT && lit(mask, x, y)
                        })
                        .fold(0u8, |sum, bit| sum | 1 << bit);
                    char::from(0x3f + bits)
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_left_sixth_is_five_rows_of_the_left_half() {
        // Rows 0-4 of columns 0-4: bits 0-4 of the first band (0x3f + 0x1f = '^').
        assert_eq!(bitmap(0b00_0001), "^^^^^?????/??????????/??????????");
    }

    #[test]
    fn the_full_block_fills_every_band() {
        // Rows 12-15 exist in the last band: four bits (0x3f + 0x0f = 'N').
        assert_eq!(bitmap(0x3f), "~~~~~~~~~~/~~~~~~~~~~/NNNNNNNNNN");
    }

    #[test]
    fn sixty_three_glyphs_from_bang_to_underscore() {
        let sequence = load();
        assert!(sequence.starts_with("\x1bP0;1;0;10;0;2;16;0{ @"));
        assert!(sequence.ends_with("\x1b\\"));
        assert_eq!(sequence.matches(';').count(), 7 + 62);
        assert_eq!((character(1), character(0x3f)), ('!', '_'));
    }
}
