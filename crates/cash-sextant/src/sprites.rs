//! The pictures, as rows of letters, one letter a pixel; `.` is see-through. A pixel is
//! about half as wide as it is high in most terminal fonts (8x13 in a 16x39 cell), so
//! the pictures are drawn wider than they look.

use crate::canvas::{Canvas, Rgb};

pub(crate) struct Sprite {
    pub(crate) rows: &'static [&'static str],
}

impl Sprite {
    pub(crate) const fn width(&self) -> i32 {
        self.rows[0].len() as i32
    }

    pub(crate) const fn height(&self) -> i32 {
        self.rows.len() as i32
    }

    /// Draw with its centre at `x`, `y`; `tint`, when given, in place of every colour.
    pub(crate) fn draw(&self, canvas: &mut Canvas, x: f32, y: f32, tint: Option<Rgb>) {
        let left = x.round() as i32 - self.width() / 2;
        let top = y.round() as i32 - self.height() / 2;
        for (dy, row) in self.rows.iter().enumerate() {
            for (dx, letter) in row.bytes().enumerate() {
                if let Some(colour) = palette(letter) {
                    canvas.set(left + dx as i32, top + dy as i32, tint.unwrap_or(colour));
                }
            }
        }
    }
}

pub(crate) const fn palette(letter: u8) -> Option<Rgb> {
    Some(match letter {
        b'W' => 0x00e8_ecf4,
        b'w' => 0x0088_90a8,
        b'C' => 0x0040_d8ff,
        b'B' => 0x0030_60ff,
        b'R' => 0x00ff_3848,
        b'O' => 0x00ff_9020,
        b'Y' => 0x00ff_e040,
        b'G' => 0x0040_e070,
        b'g' => 0x0020_8840,
        b'M' => 0x00e0_40e0,
        b'm' => 0x0080_2090,
        _ => return None,
    })
}

pub(crate) const PLAYER: Sprite = Sprite {
    rows: &[
        ".....W.....",
        "....WCW....",
        "....WCW....",
        "..w.WWW.w..",
        ".wWWWBWWWw.",
        "RWWwWWWwWWR",
        "R...O.O...R",
    ],
};

/// The flame under the player, every other frame.
pub(crate) const FLAME: Sprite = Sprite {
    rows: &["....Y.Y....", "....O.O...."],
};

/// The common enemy, two frames.
pub(crate) const DRONE: [Sprite; 2] = [
    Sprite {
        rows: &[
            "..g.....g..",
            "...G...G...",
            "..GGGGGGG..",
            ".GGYGGGYGG.",
            "GG.GGGGG.GG",
            "G..g...g..G",
        ],
    },
    Sprite {
        rows: &[
            "..g.....g..",
            "G..G...G..G",
            "G.GGGGGGG.G",
            "GGGYGGGYGGG",
            "..GGGGGGG..",
            "...g...g...",
        ],
    },
];

/// The stronger enemy at the top of the formation, two frames.
pub(crate) const WARDEN: [Sprite; 2] = [
    Sprite {
        rows: &[
            "....MMM....",
            "..MMMMMMM..",
            ".MMCMMMCMM.",
            "MMMMMMMMMMM",
            "m.M.m.m.M.m",
            ".m.......m.",
        ],
    },
    Sprite {
        rows: &[
            "....MMM....",
            "..MMMMMMM..",
            ".MMCMMMCMM.",
            "MMMMMMMMMMM",
            ".mM.m.m.Mm.",
            "m.........m",
        ],
    },
];
