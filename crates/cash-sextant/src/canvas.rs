//! A pixel canvas shown with Unicode sextants: each terminal cell is 2 pixels wide and 3
//! high, and holds two colours, the glyph's and the cell's background.
//!
//! The 64 patterns of a 2x3 cell are the 60 sextants of "Symbols for Legacy Computing"
//! (U+1FB00..U+1FB3B), a space, a full block and the two half blocks the sextant range
//! leaves out because Block Elements already has them. A cell whose six pixels hold more
//! than two colours keeps its two commonest; the others take the nearer of the two.

/// A colour as `0xRRGGBB`, or [`EMPTY`].
pub(crate) type Rgb = u32;

/// No pixel: the terminal's own background shows.
pub(crate) const EMPTY: Rgb = u32::MAX;

/// One terminal cell: which of its six pixels are lit (bit 0 the top left, in reading
/// order), their colour and the background's. The screen picks the glyph.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Cell {
    pub(crate) mask: u8,
    pub(crate) fg: Rgb,
    pub(crate) bg: Rgb,
}

impl Cell {
    pub(crate) const BLANK: Self = Self {
        mask: 0,
        fg: EMPTY,
        bg: EMPTY,
    };
}

pub(crate) struct Canvas {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pixels: Vec<Rgb>,
}

impl Canvas {
    /// A canvas for `cols` x `rows` cells.
    pub(crate) fn new(cols: usize, rows: usize) -> Self {
        let (width, height) = (cols * 2, rows * 3);
        Self {
            width,
            height,
            pixels: vec![EMPTY; width * height],
        }
    }

    pub(crate) fn clear(&mut self) {
        self.pixels.fill(EMPTY);
    }

    /// Set the pixel at `x`, `y`; outside the canvas, nothing.
    pub(crate) fn set(&mut self, x: i32, y: i32, colour: Rgb) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            self.pixels[y as usize * self.width + x as usize] = colour;
        }
    }

    fn get(&self, x: usize, y: usize) -> Rgb {
        self.pixels[y * self.width + x]
    }

    /// The cell at column `col`, row `row`.
    pub(crate) fn cell(&self, col: usize, row: usize) -> Cell {
        let mut six = [EMPTY; 6];
        for (i, pixel) in six.iter_mut().enumerate() {
            *pixel = self.get(col * 2 + i % 2, row * 3 + i / 2);
        }
        resolve(six)
    }
}

/// The cell that shows six pixels, in reading order: top left, top right, middle left,
/// middle right, bottom left, bottom right.
pub(crate) fn resolve(six: [Rgb; 6]) -> Cell {
    // The colours, commonest first; an empty pixel counts as a colour, so that a sprite's
    // edge keeps the terminal's background beside it.
    let mut colours: Vec<(Rgb, u8)> = Vec::with_capacity(6);
    for pixel in six {
        match colours.iter_mut().find(|(c, _)| *c == pixel) {
            Some((_, n)) => *n += 1,
            None => colours.push((pixel, 1)),
        }
    }
    colours.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    let first = colours[0].0;
    let Some(&(second, _)) = colours.get(1) else {
        return if first == EMPTY {
            Cell::BLANK
        } else {
            Cell {
                mask: 0x3f,
                fg: first,
                bg: EMPTY,
            }
        };
    };
    // The background is the empty one when there is one, else the commonest.
    let (fg, bg) = if first == EMPTY {
        (second, first)
    } else if second == EMPTY {
        (first, second)
    } else {
        (second, first)
    };
    let mut mask = 0u8;
    for (i, &pixel) in six.iter().enumerate() {
        let lit = pixel == fg || (pixel != bg && distance(pixel, fg) < distance(pixel, bg));
        if lit {
            mask |= 1 << i;
        }
    }
    Cell { mask, fg, bg }
}

/// How far apart two colours look, roughly: squared RGB distance, an empty pixel as black.
fn distance(a: Rgb, b: Rgb) -> u32 {
    let parts = |c: Rgb| {
        let c = if c == EMPTY { 0 } else { c };
        [(c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff]
    };
    let (a, b) = (parts(a), parts(b));
    (0..3).map(|i| a[i].abs_diff(b[i]).pow(2)).sum()
}

/// The glyph whose lit sixths are `mask`'s bits, bit 0 the top left, in reading order.
pub(crate) fn glyph(mask: u8) -> char {
    match mask & 0x3f {
        0 => ' ',
        0x3f => '█',
        0b01_0101 => '▌',
        0b10_1010 => '▐',
        m => {
            // The sextants run in the order of their masks, without the two half blocks.
            let m = u32::from(m);
            let skipped = u32::from(m > 0b01_0101) + u32::from(m > 0b10_1010);
            char::from_u32(0x1FB00 + m - 1 - skipped).unwrap_or('?')
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sextant_range_is_covered_in_order() {
        assert_eq!(glyph(0b00_0001), '\u{1FB00}');
        assert_eq!(glyph(0b00_0010), '\u{1FB01}');
        assert_eq!(glyph(0b01_0100), '\u{1FB13}');
        assert_eq!(glyph(0b01_0110), '\u{1FB14}');
        assert_eq!(glyph(0b11_1110), '\u{1FB3B}');
        let mut seen: Vec<char> = (0..64).map(glyph).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 64);
    }

    #[test]
    fn a_sprite_edge_keeps_the_background_empty() {
        let r = 0x00ff_0000;
        let cell = resolve([r, EMPTY, r, EMPTY, r, r]);
        assert_eq!((cell.fg, cell.bg), (r, EMPTY));
        assert_eq!(cell.mask, 0b11_0101);
    }

    #[test]
    fn a_third_colour_joins_the_nearer_one() {
        let (red, dark_red, blue) = (0x00ff_0000, 0x00cc_0000, 0x0000_00ff);
        let cell = resolve([red, red, blue, blue, blue, dark_red]);
        assert_eq!((cell.fg, cell.bg), (red, blue));
        assert_eq!(cell.mask, 0b10_0011);
    }
}
