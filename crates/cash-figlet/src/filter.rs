//! toilet's filters, applied to each block as it is printed: `crop`, `rainbow`, `metal`,
//! `flip`, `flop`, `180` (`rotate`), `left`, `right` and `border`.
//!
//! Written from toilet 0.3's `filter.c` and libcaca's `transform.c` (both WTFPL): the
//! colours are toilet's palettes, and the mirrored and rotated characters come from
//! libcaca's tables, which choose a character that looks like the turned one (`/` for
//! `\`, `b` for `d`, `∀` for `A`). A filter works on the block made rectangular first, as
//! libcaca's canvas is.

use crate::canvas::{Block, Cell, Row};
use crate::style::Color;

/// One filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
                .map(|(_, filter, _)| *filter)
                .ok_or_else(|| name.to_owned())
        })
        .collect()
}

/// `block` after `filters`, in order.
pub fn apply(filters: &[Filter], block: &Block) -> Block {
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
            Filter::Border => border(&block),
        };
    }
    block
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

fn border(block: &Block) -> Block {
    let width = block.width();
    let mut rows = Vec::with_capacity(block.rows.len() + 2);
    let edge = |l: char, r: char| {
        let mut row = vec![Cell::plain(l)];
        row.extend(std::iter::repeat_n(Cell::plain('─'), width));
        row.push(Cell::plain(r));
        row
    };
    rows.push(edge('┌', '┐'));
    for row in &block.rows {
        let mut new = vec![Cell::plain('│')];
        new.extend_from_slice(row);
        new.resize(width + 1, Cell::BLANK);
        new.push(Cell::plain('│'));
        rows.push(new);
    }
    rows.push(edge('└', '┘'));
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
        assert_eq!(text(&apply(&[Filter::Crop], &b)), ["/\\"]);
        assert_eq!(
            text(&apply(&[Filter::Crop, Filter::Border], &b)),
            ["┌──┐", "│/\\│", "└──┘"]
        );
    }

    #[test]
    fn mirrors_and_turns() {
        let b = block(&["(b/", "_A "]);
        assert_eq!(text(&apply(&[Filter::Flip], &b)), ["\\d)", " A_"]);
        assert_eq!(text(&apply(&[Filter::Flop], &b)), ["‾∀ ", "(p\\"]);
        assert_eq!(text(&apply(&[Filter::Rotate180], &b)), [" ∀‾", "/q)"]);
        // Flipping twice gives the block back.
        assert_eq!(apply(&[Filter::Flip, Filter::Flip], &b), b.squared());
    }

    #[test]
    fn quarter_turns() {
        let b = block(&["--", "||"]);
        assert_eq!(text(&apply(&[Filter::Left], &b)), ["丨⼆"]);
        let b = block(&["ab", "cd"]);
        assert_eq!(text(&apply(&[Filter::Left], &b)), ["abcd"]);
        assert_eq!(text(&apply(&[Filter::Right], &b)), ["cdab"]);
    }

    #[test]
    fn colours_leave_blanks_alone() {
        let b = block(&["a b"]);
        let coloured = apply(&[Filter::Rainbow], &b);
        assert_eq!(coloured.rows[0][0].style.fg, Some(Color::Ansi(13)));
        assert!(coloured.rows[0][1].style.is_plain());
        let metal = apply(&[Filter::Metal], &b);
        assert_eq!(metal.rows[0][2].style.fg, Some(Color::Ansi(12)));
    }
}
