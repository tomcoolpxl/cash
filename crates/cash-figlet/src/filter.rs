//! Filters, applied to each block as it is printed.
//!
//! toilet's: `crop`, `rainbow`, `metal`, `flip`, `flop`, `180` (`rotate`), `left`, `right`
//! and `border`, written from toilet 0.3's `filter.c` and libcaca's `transform.c` (both
//! WTFPL): the colours are toilet's palettes, and the mirrored and rotated characters
//! come from libcaca's tables, which choose a character that looks like the turned one
//! (`/` for `\`, `b` for `d`, `∀` for `A`).
//!
//! cash's own (decisions.md D84): true-colour gradients, lolcat's rainbow, a shadow, an
//! outline, frames in Unicode's box lines with padding, and a background.
//!
//! A filter works on the block made rectangular first, as libcaca's canvas is.

use crate::canvas::{Block, Cell, Row};
use crate::palette;
use crate::style::Color;

/// Which way a gradient runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Left to right.
    Horizontal,
    /// Top to bottom.
    Vertical,
    /// From the top left corner to the bottom right.
    Diagonal,
}

/// The lines a frame is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStyle {
    /// `┌─┐`
    Single,
    /// `╔═╗`
    Double,
    /// `╭─╮`
    Rounded,
    /// `┏━┓`
    Heavy,
    /// `+-+`
    Ascii,
}

impl FrameStyle {
    /// Every style, by the name `--border` takes.
    pub const NAMES: &[(&str, Self)] = &[
        ("single", Self::Single),
        ("double", Self::Double),
        ("rounded", Self::Rounded),
        ("heavy", Self::Heavy),
        ("ascii", Self::Ascii),
    ];

    /// Its corners and lines: top left, top right, bottom left, bottom right, across,
    /// down.
    const fn parts(self) -> [char; 6] {
        match self {
            Self::Single => ['┌', '┐', '└', '┘', '─', '│'],
            Self::Double => ['╔', '╗', '╚', '╝', '═', '║'],
            Self::Rounded => ['╭', '╮', '╰', '╯', '─', '│'],
            Self::Heavy => ['┏', '┓', '┗', '┛', '━', '┃'],
            Self::Ascii => ['+', '+', '+', '+', '-', '|'],
        }
    }
}

/// One filter.
#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    /// Takes off the blank rows and columns around the text.
    Crop,
    /// Colours the text in diagonal rainbow stripes.
    Rainbow,
    /// Colours the text in metallic blues and greys.
    Metal,
    /// Mirrors left to right.
    Flip,
    /// Mirrors top to bottom.
    Flop,
    /// Turns upside down.
    Rotate180,
    /// Turns a quarter left; two columns become one row.
    Left,
    /// Turns a quarter right.
    Right,
    /// Draws a box around the text.
    Border,
    /// Colours the text along `stops`, in true colour.
    Gradient {
        /// The colours, in order.
        stops: Vec<(u8, u8, u8)>,
        /// Which way they run.
        direction: Direction,
    },
    /// Colours the text as lolcat does: sine waves of red, green and blue, in true
    /// colour, continuing from block to block.
    Lolcat {
        /// Where the waves start.
        seed: f64,
        /// Columns to one step of the waves (lolcat's 3).
        spread: f64,
        /// How fast they change (lolcat's 0.1).
        frequency: f64,
    },
    /// cfonts' and oh-my-logo's "ANSI Shadow": box lines along each stroke's right and
    /// bottom edges.
    Shadow,
    /// Only the edges of the text: a cell with text on all four sides becomes a blank.
    Outline,
    /// A frame in `style`, `padding` columns from the text (half as many rows).
    Frame {
        /// The lines.
        style: FrameStyle,
        /// Blank columns between the text and the frame.
        padding: usize,
    },
    /// Every cell on this background.
    Background(Color),
}

/// Every filter, by the name `-F` takes and what `-F list` says of it. `rotate` is
/// `180`'s old name, and `gay` `rainbow`'s, as toilet keeps them.
pub const FILTERS: &[(&str, Filter, Option<&str>)] = &[
    ("crop", Filter::Crop, Some("crop unused blanks")),
    (
        "rainbow",
        Filter::Rainbow,
        Some("add a rainbow colour effect"),
    ),
    ("gay", Filter::Rainbow, None),
    ("metal", Filter::Metal, Some("add a metallic colour effect")),
    ("flip", Filter::Flip, Some("flip horizontally")),
    ("flop", Filter::Flop, Some("flip vertically")),
    ("rotate", Filter::Rotate180, None),
    ("180", Filter::Rotate180, Some("rotate 180 degrees")),
    (
        "left",
        Filter::Left,
        Some("rotate 90 degrees counterclockwise"),
    ),
    ("right", Filter::Right, Some("rotate 90 degrees clockwise")),
    (
        "border",
        Filter::Border,
        Some("surround text with a border"),
    ),
    (
        "shadow",
        Filter::Shadow,
        Some("box-line shadows on solid letters, as cfonts' block font"),
    ),
    (
        "outline",
        Filter::Outline,
        Some("keep only the edges of solid letters"),
    ),
];

/// The filters of a colon-separated list, as `-F crop:metal` gives them; the name that
/// is not a filter when one is not.
pub fn parse_list(list: &str) -> Result<Vec<Filter>, String> {
    list.split(':')
        .filter(|name| !name.is_empty())
        .map(|name| {
            FILTERS
                .iter()
                .find(|(known, _, _)| *known == name)
                .map(|(_, filter, _)| filter.clone())
                .ok_or_else(|| name.to_owned())
        })
        .collect()
}

/// `block` after `filters`, in order; `row` is how many rows were printed before it, so
/// that lolcat's waves go on from one block to the next.
pub fn apply(filters: &[Filter], block: &Block, row: usize) -> Block {
    let mut block = if filters.is_empty() {
        return block.clone();
    } else {
        block.squared()
    };
    for filter in filters {
        block = match filter {
            Filter::Crop => crop(&block),
            Filter::Rainbow => colour(&block, |x, y| {
                RAINBOW.get((x / 2 + y) % RAINBOW.len()).copied()
            }),
            Filter::Metal => colour(&block, |x, y| {
                METAL.get(usize::midpoint(y, x / 8) % METAL.len()).copied()
            }),
            Filter::Flip => flip(&block),
            Filter::Flop => flop(&block),
            Filter::Rotate180 => {
                let mut turned = flip_with(&block, rotate_char);
                turned.rows.reverse();
                turned
            }
            Filter::Left => quarter(&block, true),
            Filter::Right => quarter(&block, false),
            Filter::Border => frame(&block, FrameStyle::Single, 0),
            Filter::Gradient { stops, direction } => gradient(&block, stops, *direction),
            Filter::Lolcat {
                seed,
                spread,
                frequency,
            } => colour(&block, |x, y| {
                Some(lolcat(
                    *seed + count(row + y) + count(x) / spread.max(0.1),
                    *frequency,
                ))
            }),
            Filter::Shadow => shadow(&block),
            Filter::Outline => outline(&block),
            Filter::Frame { style, padding } => frame(&block, *style, *padding),
            Filter::Background(colour) => {
                let mut block = block.squared();
                for cell in block.rows.iter_mut().flatten() {
                    cell.style.bg = Some(*colour);
                }
                block
            }
        };
    }
    block
}

/// A count as a float.
fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// lolcat's colour at step `at`: three sine waves a third of a turn apart.
fn lolcat(at: f64, frequency: f64) -> Color {
    let wave =
        |phase: f64| palette::channel(frequency.mul_add(at, phase).sin().mul_add(127.0, 128.0));
    let third = 2.0 * std::f64::consts::PI / 3.0;
    Color::Rgb(wave(0.0), wave(third), wave(2.0 * third))
}

/// The text coloured along `stops`, by where each cell is in the block.
fn gradient(block: &Block, stops: &[(u8, u8, u8)], direction: Direction) -> Block {
    let width = count(block.width().saturating_sub(1)).max(1.0);
    let height = count(block.rows.len().saturating_sub(1)).max(1.0);
    colour(block, |x, y| {
        let t = match direction {
            Direction::Horizontal => count(x) / width,
            Direction::Vertical => count(y) / height,
            Direction::Diagonal => f64::midpoint(count(x) / width, count(y) / height),
        };
        let (r, g, b) = palette::at(stops, t);
        Some(Color::Rgb(r, g, b))
    })
}

/// The "ANSI Shadow" of cfonts' `block` font and oh-my-logo's filled logos: double box
/// lines along the right and bottom edges of every stroke, one cell out.
///
/// Each blank cell takes its line from which of three neighbours hold text: the one to
/// its left (a stroke's right edge, `║`, topped by `╗`), the one above (a stroke's bottom
/// edge, `═`, begun by `╚`), and the one up and to the left (the corner below a stroke's
/// right edge, `╝`); with all three, the line turns down, `╔`. Worked out from cfonts'
/// alphabet and digits (2026-10-23), where it gives 1,028 of the 1,036 shadow cells; the
/// eight others are drawn by hand in single letters. Best on solid letters: `banner`,
/// `block`, `mono9`, `mono12`, `--pixel`, TheDraw fonts.
fn shadow(block: &Block) -> Block {
    let solid = |x: Option<usize>, y: Option<usize>| -> bool {
        let (Some(x), Some(y)) = (x, y) else {
            return false;
        };
        block
            .rows
            .get(y)
            .and_then(|row| row.get(x))
            .is_some_and(|cell| cell.ch != ' ')
    };
    let width = block.width() + 1;
    let height = block.rows.len() + 1;
    let mut rows: Vec<Row> = Vec::with_capacity(height);
    for y in 0..height {
        let mut row = block.rows.get(y).cloned().unwrap_or_default();
        row.resize(width, Cell::BLANK);
        for (x, cell) in row.iter_mut().enumerate() {
            if cell.ch != ' ' {
                continue;
            }
            let left = solid(x.checked_sub(1), Some(y));
            let above = solid(Some(x), y.checked_sub(1));
            let corner = solid(x.checked_sub(1), y.checked_sub(1));
            let line = match (left, above, corner) {
                (false, false, false) => continue,
                (false, false, true) => '╝',
                (false, true, false) => '╚',
                (false, true, true) => '═',
                (true, false, false) => '╗',
                (true, false, true) => '║',
                (true, true, _) => '╔',
            };
            *cell = Cell::plain(line);
        }
        rows.push(row);
    }
    // The row and column added for the shadow, where none of it landed.
    if rows
        .last()
        .is_some_and(|row| row.iter().all(|c| c.ch == ' '))
    {
        rows.pop();
    }
    if rows
        .iter()
        .all(|row| row.last().is_none_or(|c| c.ch == ' '))
    {
        for row in &mut rows {
            row.pop();
        }
    }
    Block { rows }
}

/// Only the cells of the text that touch a blank, or the edge, on one of four sides.
fn outline(block: &Block) -> Block {
    let filled = |x: Option<usize>, y: Option<usize>| -> bool {
        let (Some(x), Some(y)) = (x, y) else {
            return false;
        };
        block
            .rows
            .get(y)
            .and_then(|row| row.get(x))
            .is_some_and(|cell| cell.ch != ' ')
    };
    let mut out = block.clone();
    for (y, row) in out.rows.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            let inside = filled(x.checked_sub(1), Some(y))
                && filled(Some(x + 1), Some(y))
                && filled(Some(x), y.checked_sub(1))
                && filled(Some(x), Some(y + 1));
            if cell.ch != ' ' && !cell.is_tail() && inside && !cell.is_wide() {
                *cell = Cell::BLANK;
            }
        }
    }
    out
}

/// toilet's rainbow, light magenta to light blue, as ANSI colours.
const RAINBOW: [Color; 6] = [
    Color::Ansi(13),
    Color::Ansi(9),
    Color::Ansi(11),
    Color::Ansi(10),
    Color::Ansi(14),
    Color::Ansi(12),
];

/// toilet's metal: light blue, blue, light grey, dark grey.
const METAL: [Color; 4] = [
    Color::Ansi(12),
    Color::Ansi(4),
    Color::Ansi(7),
    Color::Ansi(8),
];

/// Every cell that is not a blank given the foreground `pick` chooses for its place, on
/// no background.
fn colour(block: &Block, pick: impl Fn(usize, usize) -> Option<Color>) -> Block {
    let mut block = block.clone();
    for (y, row) in block.rows.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            if cell.ch != ' ' && !cell.is_tail() {
                cell.style.fg = pick(x, y);
                cell.style.bg = None;
            }
        }
    }
    block
}

fn crop(block: &Block) -> Block {
    let mut rows_used = Vec::new();
    let (mut left, mut right) = (usize::MAX, 0);
    for (y, row) in block.rows.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            if cell.ch != ' ' {
                left = left.min(x);
                right = right.max(x);
                rows_used.push(y);
            }
        }
    }
    let (Some(&top), Some(&bottom)) = (rows_used.iter().min(), rows_used.iter().max()) else {
        return block.clone();
    };
    Block {
        rows: block
            .rows
            .get(top..=bottom)
            .unwrap_or(&[])
            .iter()
            .map(|row| row.get(left..=right).unwrap_or(&[]).to_vec())
            .collect(),
    }
}

fn flip(block: &Block) -> Block {
    flip_with(block, flip_char)
}

/// Each row mirrored, its characters turned by `turn`, a wide character kept whole.
fn flip_with(block: &Block, turn: fn(char) -> char) -> Block {
    Block {
        rows: block
            .rows
            .iter()
            .map(|row| {
                let mut row: Row = row
                    .iter()
                    .rev()
                    .map(|cell| Cell {
                        ch: if cell.is_tail() {
                            cell.ch
                        } else {
                            turn(cell.ch)
                        },
                        style: cell.style,
                    })
                    .collect();
                // A tail now comes before its character: put it back after.
                let mut x = 0;
                while x + 1 < row.len() {
                    if row.get(x).is_some_and(Cell::is_tail) {
                        row.swap(x, x + 1);
                        x += 1;
                    }
                    x += 1;
                }
                row
            })
            .collect(),
    }
}

fn flop(block: &Block) -> Block {
    Block {
        rows: block
            .rows
            .iter()
            .rev()
            .map(|row| {
                row.iter()
                    .map(|cell| Cell {
                        ch: if cell.is_tail() {
                            cell.ch
                        } else {
                            flop_char(cell.ch)
                        },
                        style: cell.style,
                    })
                    .collect()
            })
            .collect(),
    }
}

/// A quarter turn, left or right: each pair of columns becomes one pair of cells in a
/// row, as libcaca's `caca_rotate_left` and `caca_rotate_right` turn a canvas.
fn quarter(block: &Block, left: bool) -> Block {
    let width = block.width();
    let height = block.rows.len();
    let new_height = width.div_ceil(2);
    let mut rows = vec![vec![Cell::BLANK; height * 2]; new_height];
    for (y, row) in block.rows.iter().enumerate() {
        for x in 0..new_height {
            let first = row.get(x * 2).copied().unwrap_or(Cell::BLANK);
            let second = row.get(x * 2 + 1).copied().unwrap_or(Cell {
                ch: ' ',
                style: first.style,
            });
            let (mut a, mut b) = (first, second);
            // A blank's colours are not carried: they would land on the wrong cell.
            if a.ch == ' ' {
                a.style = b.style;
            } else if b.ch == ' ' {
                b.style = a.style;
            }
            let (ca, cb) = turn_pair(a.ch, b.ch, left);
            let (row_at, col_at) = if left {
                (new_height - 1 - x, y * 2)
            } else {
                (x, (height - 1 - y) * 2)
            };
            if let Some(target) = rows.get_mut(row_at) {
                if let Some(cell) = target.get_mut(col_at) {
                    *cell = Cell {
                        ch: ca,
                        style: a.style,
                    };
                }
                if let Some(cell) = target.get_mut(col_at + 1) {
                    *cell = Cell {
                        ch: cb,
                        style: b.style,
                    };
                }
            }
        }
    }
    Block { rows }
}

/// A frame around the block, `padding` blank columns inside it on each side and half as
/// many rows (rounded up); toilet's `border` is the single frame with none.
fn frame(block: &Block, style: FrameStyle, padding: usize) -> Block {
    let [top_left, top_right, bottom_left, bottom_right, across, down] = style.parts();
    let inner = block.width() + 2 * padding;
    let pad_rows = padding.div_ceil(2);
    let mut rows = Vec::with_capacity(block.rows.len() + 2 + 2 * pad_rows);
    let edge = |l: char, r: char| {
        let mut row = vec![Cell::plain(l)];
        row.extend(std::iter::repeat_n(Cell::plain(across), inner));
        row.push(Cell::plain(r));
        row
    };
    let blank = || {
        let mut row = vec![Cell::plain(down)];
        row.extend(std::iter::repeat_n(Cell::BLANK, inner));
        row.push(Cell::plain(down));
        row
    };
    rows.push(edge(top_left, top_right));
    rows.extend(std::iter::repeat_with(blank).take(pad_rows));
    for row in &block.rows {
        let mut new = vec![Cell::plain(down)];
        new.extend(std::iter::repeat_n(Cell::BLANK, padding));
        new.extend_from_slice(row);
        new.resize(inner + 1, Cell::BLANK);
        new.push(Cell::plain(down));
        rows.push(new);
    }
    rows.extend(std::iter::repeat_with(blank).take(pad_rows));
    rows.push(edge(bottom_left, bottom_right));
    Block { rows }
}

/// The partner of `ch` in a table of pairs, if it has one.
fn partner(pairs: &[(char, char)], ch: char) -> Option<char> {
    pairs.iter().find_map(|&(a, b)| {
        if a == ch {
            Some(b)
        } else if b == ch {
            Some(a)
        } else {
            None
        }
    })
}

/// What mirroring left to right leaves as it is, and the pairs it swaps (libcaca's).
const FLIP_SAME: &str = " \"#'-.*+:=08AHIMOTUVWXY^_iovwx|░▒▓█▄▀─━┃┼╋┬┴┳┻╦╩═║╬╵╷╹╻";
const FLIP_PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('/', '\\'),
    ('<', '>'),
    ('[', ']'),
    ('b', 'd'),
    ('p', 'q'),
    ('{', '}'),
    (';', '⁏'),
    ('`', '´'),
    (',', 'ˎ'),
    ('1', '߁'),
    ('B', '𐐒'),
    ('C', 'Ͻ'),
    ('D', 'ᗡ'),
    ('E', 'Ǝ'),
    ('J', 'ᒐ'),
    ('L', '⅃'),
    ('N', 'И'),
    ('P', '𐐋'),
    ('R', 'Я'),
    ('S', 'Ƨ'),
    ('c', 'ɔ'),
    ('e', 'ɘ'),
    ('▌', '▐'),
    ('▖', '▗'),
    ('▘', '▝'),
    ('▙', '▟'),
    ('▚', '▞'),
    ('▛', '▜'),
    ('►', '◄'),
    ('→', '←'),
    ('⌐', '¬'),
    ('┌', '┐'),
    ('└', '┘'),
    ('├', '┤'),
    ('┏', '┓'),
    ('┗', '┛'),
    ('┣', '┫'),
    ('╒', '╕'),
    ('╘', '╛'),
    ('╓', '╖'),
    ('╙', '╜'),
    ('╔', '╗'),
    ('╚', '╝'),
    ('╞', '╡'),
    ('╟', '╢'),
    ('╠', '╣'),
    ('╴', '╶'),
    ('╸', '╺'),
    ('⋲', '⋺'),
    ('⋳', '⋻'),
    ('⌈', '⌉'),
    ('⌊', '⌋'),
    ('⌌', '⌍'),
    ('⌎', '⌏'),
    ('⌜', '⌝'),
    ('⌞', '⌟'),
    ('⌦', '⌫'),
    ('〈', '〉'),
    ('⍁', '⍂'),
    ('⍃', '⍄'),
    ('⍅', '⍆'),
    ('⍇', '⍈'),
    ('⌿', '⍀'),
    ('⎛', '⎞'),
    ('⎜', '⎟'),
    ('⎝', '⎠'),
    ('⎡', '⎤'),
    ('⎢', '⎥'),
    ('⎣', '⎦'),
    ('⎧', '⎫'),
    ('⎨', '⎬'),
    ('⎩', '⎭'),
    ('⎰', '⎱'),
    ('⎾', '⏋'),
    ('⎿', '⏌'),
];

/// libcaca's `flipchar`: what a character looks like mirrored left to right.
fn flip_char(ch: char) -> char {
    if FLIP_SAME.contains(ch) {
        return ch;
    }
    partner(FLIP_PAIRS, ch).unwrap_or(ch)
}

/// What mirroring top to bottom leaves as it is, and the pairs it swaps (libcaca's).
const FLOP_SAME: &str =
    " ()*+-038:<=>BCDEHIKOX[]co{|}░▒▓█▌▐─━┃┼╋├┤┣┫╠╣═║╬╴╶╸╺⋲⋺⋳⋻⌦⌫〈〉⍃⍄⍅⍆⍇⍈⎜⎟⎢⎥⎨⎬";
const FLOP_PAIRS: &[(char, char)] = &[
    ('/', '\\'),
    ('M', 'W'),
    (',', '`'),
    ('b', 'p'),
    ('d', 'q'),
    ('p', 'q'),
    ('f', 't'),
    ('.', '\''),
    ('_', '‾'),
    ('!', '¡'),
    ('A', '∀'),
    ('J', 'ᒉ'),
    ('L', 'Г'),
    ('N', 'И'),
    ('P', 'Ь'),
    ('R', 'ʁ'),
    ('S', 'Ƨ'),
    ('U', 'Ո'),
    ('V', 'Λ'),
    ('Y', '⅄'),
    ('h', 'μ'),
    ('i', 'ᴉ'),
    ('j', 'ḷ'),
    ('l', 'ȷ'),
    ('v', 'ʌ'),
    ('w', 'ʍ'),
    ('y', 'λ'),
    ('"', '„'),
    ('m', 'ɯ'),
    ('n', 'u'),
    ('▄', '▀'),
    ('▖', '▘'),
    ('▗', '▝'),
    ('▙', '▛'),
    ('▟', '▜'),
    ('▚', '▞'),
    ('┌', '└'),
    ('┐', '┘'),
    ('┬', '┴'),
    ('┏', '┗'),
    ('┓', '┛'),
    ('┳', '┻'),
    ('╔', '╚'),
    ('╗', '╝'),
    ('╦', '╩'),
    ('╒', '╘'),
    ('╕', '╛'),
    ('╤', '╧'),
    ('╓', '╙'),
    ('╖', '╜'),
    ('╥', '╨'),
    ('╵', '╷'),
    ('╹', '╻'),
    ('⌈', '⌊'),
    ('⌉', '⌋'),
    ('⌌', '⌎'),
    ('⌍', '⌏'),
    ('⌜', '⌞'),
    ('⌝', '⌟'),
    ('⍁', '⍂'),
    ('⌿', '⍀'),
    ('⎛', '⎝'),
    ('⎞', '⎠'),
    ('⎡', '⎣'),
    ('⎤', '⎦'),
    ('⎧', '⎩'),
    ('⎫', '⎭'),
    ('⎰', '⎱'),
    ('⎾', '⎿'),
    ('⏋', '⏌'),
];

/// libcaca's `flopchar`: what a character looks like mirrored top to bottom.
fn flop_char(ch: char) -> char {
    if FLOP_SAME.contains(ch) {
        return ch;
    }
    partner(FLOP_PAIRS, ch).unwrap_or(ch)
}

/// What turning upside down leaves as it is, and the pairs it swaps (libcaca's).
const ROTATE_SAME: &str = " *+-/08:=HINOSXZ\\osxz|░▒▓█▚▞─━┃┼╋═║╬⌿⍀⎰⎱";
const ROTATE_PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('<', '>'),
    ('[', ']'),
    ('{', '}'),
    ('.', '\''),
    ('6', '9'),
    ('M', 'W'),
    ('b', 'q'),
    ('d', 'p'),
    ('n', 'u'),
    ('_', '‾'),
    (',', '´'),
    (';', '؛'),
    ('`', 'ˎ'),
    ('&', '⅋'),
    ('!', '¡'),
    ('?', '¿'),
    ('3', 'Ɛ'),
    ('4', 'ᔭ'),
    ('A', '∀'),
    ('B', '𐐒'),
    ('C', 'Ͻ'),
    ('D', 'ᗡ'),
    ('E', 'Ǝ'),
    ('F', 'Ⅎ'),
    ('G', '⅁'),
    ('J', 'ᒋ'),
    ('L', '⅂'),
    ('P', 'Ԁ'),
    ('Q', 'Ό'),
    ('R', 'ᴚ'),
    ('T', '⊥'),
    ('U', 'Ո'),
    ('V', 'Λ'),
    ('Y', '⅄'),
    ('a', 'ɐ'),
    ('c', 'ɔ'),
    ('e', 'ǝ'),
    ('f', 'ɟ'),
    ('g', 'ᵷ'),
    ('h', 'ɥ'),
    ('i', 'ᴉ'),
    ('j', 'ḷ'),
    ('k', 'ʞ'),
    ('l', 'ȷ'),
    ('m', 'ɯ'),
    ('r', 'ɹ'),
    ('t', 'ʇ'),
    ('v', 'ʌ'),
    ('w', 'ʍ'),
    ('y', 'ʎ'),
    ('ƃ', 'g'),
    ('ə', 'e'),
    ('ɾ', 'j'),
    ('˙', '.'),
    ('ן', 'l'),
    ('"', '„'),
    ('æ', 'ᴂ'),
    ('œ', 'ᴔ'),
    ('ε', 'ɜ'),
    ('ɛ', 'ɜ'),
    ('▌', '▐'),
    ('▄', '▀'),
    ('▖', '▝'),
    ('▗', '▘'),
    ('▙', '▜'),
    ('▟', '▛'),
    ('┌', '┘'),
    ('┐', '└'),
    ('├', '┤'),
    ('┬', '┴'),
    ('┏', '┛'),
    ('┓', '┗'),
    ('┣', '┫'),
    ('┳', '┻'),
    ('╔', '╝'),
    ('╗', '╚'),
    ('╠', '╣'),
    ('╦', '╩'),
    ('╒', '╛'),
    ('╕', '╘'),
    ('╞', '╡'),
    ('╤', '╧'),
    ('╓', '╜'),
    ('╖', '╙'),
    ('╟', '╢'),
    ('╥', '╨'),
    ('╴', '╶'),
    ('╵', '╷'),
    ('╸', '╺'),
    ('╹', '╻'),
    ('⋲', '⋺'),
    ('⋳', '⋻'),
    ('⌈', '⌋'),
    ('⌉', '⌊'),
    ('⌌', '⌏'),
    ('⌍', '⌎'),
    ('⌜', '⌟'),
    ('⌝', '⌞'),
    ('⌦', '⌫'),
    ('〈', '〉'),
    ('⍃', '⍄'),
    ('⍅', '⍆'),
    ('⍇', '⍈'),
    ('⎛', '⎠'),
    ('⎜', '⎟'),
    ('⎞', '⎝'),
    ('⎡', '⎦'),
    ('⎢', '⎥'),
    ('⎤', '⎣'),
    ('⎧', '⎭'),
    ('⎨', '⎬'),
    ('⎫', '⎩'),
    ('⎾', '⏌'),
    ('⏋', '⎿'),
];

/// libcaca's `rotatechar`: what a character looks like upside down.
fn rotate_char(ch: char) -> char {
    if ROTATE_SAME.contains(ch) {
        return ch;
    }
    partner(ROTATE_PAIRS, ch).unwrap_or(ch)
}

/// libcaca's `leftpair` and `rightpair`: two cells side by side, turned a quarter. The
/// tables come in fours (two in a pair table, four in a pair of pairs), each entry the
/// one before it turned a quarter left.
fn turn_pair(a: char, b: char, left: bool) -> (char, char) {
    const T: char = Cell::TAIL_CHAR;
    const TWO_BY_TWO: &[[(char, char); 2]] = &[
        [('-', '-'), ('丨', T)],
        [('|', '|'), ('⼆', T)],
        [('▄', '▀'), ('▀', '▄')],
    ];
    const TWO_BY_FOUR: &[[(char, char); 4]] = &[
        [(':', ' '), ('.', '.'), (' ', ':'), ('\'', '\'')],
        [(' ', '`'), ('´', ' '), ('ˎ', ' '), (' ', ',')],
        [(' ', '`'), ('\'', ' '), ('.', ' '), (' ', ',')],
        [('`', ' '), (',', ' '), (' ', '´'), (' ', 'ˎ')],
        [('`', ' '), (',', ' '), (' ', '.'), (' ', '\'')],
        [('/', ' '), ('-', 'ˎ'), (' ', '/'), ('`', '-')],
        [('/', ' '), ('-', '.'), (' ', '/'), ('\'', '-')],
        [('\\', ' '), (',', '-'), (' ', '\\'), ('-', '´')],
        [('\\', ' '), ('.', '-'), (' ', '\\'), ('-', '\'')],
        [('\\', ' '), ('_', ','), (' ', '\\'), ('´', '‾')],
        [('\\', '_'), ('_', '/'), ('‾', '\\'), ('/', '‾')],
        [('_', '\\'), ('‾', '/'), ('\\', '‾'), ('/', '_')],
        [('|', ' '), ('_', '_'), (' ', '|'), ('‾', '‾')],
        [('_', '|'), ('‾', '|'), ('|', '‾'), ('|', '_')],
        [('|', '_'), ('_', '|'), ('‾', '|'), ('|', '‾')],
        [('_', ' '), (' ', '╷'), (' ', '‾'), ('╵', ' ')],
        [(' ', '_'), (' ', '╵'), ('‾', ' '), ('╷', ' ')],
        [('.', '_'), ('.', '╵'), ('‾', '\''), ('╷', '\'')],
        [('(', '_'), ('‿', '|'), ('‾', ')'), ('|', '⁀')],
        [('(', '‾'), ('|', '‿'), ('_', ')'), ('⁀', '|')],
        [('\\', '/'), ('＞', T), ('/', '\\'), ('＜', T)],
        [(')', ' '), ('︵', T), (' ', '('), ('︶', T)],
        [('}', ' '), ('︷', T), (' ', '{'), ('︸', T)],
        [('(', ' '), ('ˎ', ','), (' ', ')'), ('´', '`')],
        [(' ', 'v'), ('>', ' '), ('ʌ', ' '), (' ', '<')],
        [(' ', 'V'), ('>', ' '), ('Λ', ' '), (' ', '<')],
        [('v', ' '), ('>', ' '), (' ', 'ʌ'), (' ', '<')],
        [('V', ' '), ('>', ' '), (' ', 'Λ'), (' ', '<')],
        [('\\', '|'), ('＞', T), ('|', '\\'), ('＜', T)],
        [('|', '/'), ('＞', T), ('/', '|'), ('＜', T)],
        [('▄', ' '), (' ', '▄'), (' ', '▀'), ('▀', ' ')],
        [('█', ' '), ('▄', '▄'), (' ', '█'), ('▀', '▀')],
        [('█', '▄'), ('▄', '█'), ('▀', '█'), ('█', '▀')],
        [('░', ' '), ('⣤', '⣤'), (' ', '░'), ('⠛', '⠛')],
        [('▒', ' '), ('⣶', '⣶'), (' ', '▒'), ('⠿', '⠿')],
    ];
    for group in TWO_BY_TWO {
        if let Some(at) = group.iter().position(|&pair| pair == (a, b)) {
            return group.get(1 - at).copied().unwrap_or((a, b));
        }
    }
    for group in TWO_BY_FOUR {
        if let Some(at) = group.iter().position(|&pair| pair == (a, b)) {
            let next = if left { (at + 1) % 4 } else { (at + 3) % 4 };
            return group.get(next).copied().unwrap_or((a, b));
        }
    }
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::{Filter, apply, parse_list};
    use crate::canvas::{Block, cells};
    use crate::style::{Color, Style};

    fn block(rows: &[&str]) -> Block {
        Block {
            rows: rows.iter().map(|row| cells(row, Style::PLAIN)).collect(),
        }
    }

    fn text(block: &Block) -> Vec<String> {
        block
            .rows
            .iter()
            .map(|row| row.iter().filter(|c| !c.is_tail()).map(|c| c.ch).collect())
            .collect()
    }

    #[test]
    fn lists() {
        assert_eq!(
            parse_list("crop:metal"),
            Ok(vec![Filter::Crop, Filter::Metal])
        );
        assert_eq!(
            parse_list("gay::rotate"),
            Ok(vec![Filter::Rainbow, Filter::Rotate180])
        );
        assert_eq!(parse_list("crop:sparkle"), Err("sparkle".to_owned()));
    }

    #[test]
    fn crop_and_border() {
        let b = block(&["      ", "  /\\  ", "      "]);
        assert_eq!(text(&apply(&[Filter::Crop], &b, 0)), ["/\\"]);
        assert_eq!(
            text(&apply(&[Filter::Crop, Filter::Border], &b, 0)),
            ["┌──┐", "│/\\│", "└──┘"]
        );
    }

    #[test]
    fn mirrors_and_turns() {
        let b = block(&["(b/", "_A "]);
        assert_eq!(text(&apply(&[Filter::Flip], &b, 0)), ["\\d)", " A_"]);
        assert_eq!(text(&apply(&[Filter::Flop], &b, 0)), ["‾∀ ", "(p\\"]);
        assert_eq!(text(&apply(&[Filter::Rotate180], &b, 0)), [" ∀‾", "/q)"]);
        // Flipping twice gives the block back.
        assert_eq!(apply(&[Filter::Flip, Filter::Flip], &b, 0), b.squared());
    }

    #[test]
    fn quarter_turns() {
        let b = block(&["--", "||"]);
        assert_eq!(text(&apply(&[Filter::Left], &b, 0)), ["丨⼆"]);
        let b = block(&["ab", "cd"]);
        assert_eq!(text(&apply(&[Filter::Left], &b, 0)), ["abcd"]);
        assert_eq!(text(&apply(&[Filter::Right], &b, 0)), ["cdab"]);
    }

    #[test]
    fn colours_leave_blanks_alone() {
        let b = block(&["a b"]);
        let coloured = apply(&[Filter::Rainbow], &b, 0);
        assert_eq!(coloured.rows[0][0].style.fg, Some(Color::Ansi(13)));
        assert!(coloured.rows[0][1].style.is_plain());
        let metal = apply(&[Filter::Metal], &b, 0);
        assert_eq!(metal.rows[0][2].style.fg, Some(Color::Ansi(12)));
    }

    #[test]
    fn gradients_run_across() {
        let b = block(&["aaa"]);
        let stops = vec![(0, 0, 0), (200, 0, 0)];
        let out = apply(
            &[Filter::Gradient {
                stops,
                direction: super::Direction::Horizontal,
            }],
            &b,
            0,
        );
        let fgs: Vec<_> = out.rows[0].iter().map(|c| c.style.fg).collect();
        assert_eq!(
            fgs,
            [
                Some(Color::Rgb(0, 0, 0)),
                Some(Color::Rgb(100, 0, 0)),
                Some(Color::Rgb(200, 0, 0))
            ]
        );
    }

    #[test]
    fn lolcat_goes_on_from_block_to_block() {
        let lolcat = Filter::Lolcat {
            seed: 0.0,
            spread: 3.0,
            frequency: 0.1,
        };
        let two_rows = apply(std::slice::from_ref(&lolcat), &block(&["x", "x"]), 0);
        let second = apply(std::slice::from_ref(&lolcat), &block(&["x"]), 1);
        assert_eq!(two_rows.rows[1][0].style.fg, second.rows[0][0].style.fg);
        // lolcat's colour at step 0: red at 128, green and blue a third of a turn on.
        assert_eq!(two_rows.rows[0][0].style.fg, Some(Color::Rgb(128, 238, 18)));
    }

    #[test]
    fn shadows_are_box_lines_as_cfonts_draws_them() {
        let h = block(&["██  ██", "██  ██", "██████", "██  ██", "██  ██"]);
        assert_eq!(
            text(&apply(&[Filter::Shadow], &h, 0)),
            [
                "██╗ ██╗",
                "██║ ██║",
                "██████║",
                "██╔═██║",
                "██║ ██║",
                "╚═╝ ╚═╝",
            ]
        );
    }

    #[test]
    fn shadow_outline_frame_background() {
        assert_eq!(
            text(&apply(
                &[Filter::Outline],
                &block(&["###", "###", "###"]),
                0
            )),
            ["###", "# #", "###"]
        );
        let framed = apply(
            &[Filter::Frame {
                style: super::FrameStyle::Rounded,
                padding: 1,
            }],
            &block(&["hi"]),
            0,
        );
        assert_eq!(
            text(&framed),
            ["╭────╮", "│    │", "│ hi │", "│    │", "╰────╯"]
        );
        let on_blue = apply(&[Filter::Background(Color::Ansi(4))], &block(&["a "]), 0);
        assert!(
            on_blue.rows[0]
                .iter()
                .all(|c| c.style.bg == Some(Color::Ansi(4)))
        );
    }
}
