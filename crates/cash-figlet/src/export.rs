//! Writing a figure out, in one of three formats (decisions.md D84).
//!
//! - `text` is figlet's own output: the characters, each row ending in a newline, the
//!   blanks a glyph ends with kept, no colour.
//! - `utf8` is toilet's default: UTF-8 with ANSI colour escapes, as libcaca's
//!   `_export_utf8` writes them (a reset and the attributes whenever they change, a reset
//!   before each newline); bright colours are bold as well, and true colour is written
//!   as itself.
//! - `ansi` is libcaca's `_export_ansi`: code page 437, the IBM PC's, with its sixteen
//!   colours (light grey on black where a cell has none), each row ending in a reset and
//!   CRLF, for old consoles and BBS-style art.
//!
//! Colour is written only when asked (`colour`); without it `utf8` and `text` are the
//! same, and `ansi` is plain code page 437.

use std::fmt::Write as _;

use crate::canvas::{Block, Figure, Row};
use crate::style::{Color, SIXTEEN, Style};

/// An output format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// UTF-8, with ANSI colour escapes when colour is on.
    Utf8,
    /// Code page 437, with ANSI colour escapes when colour is on.
    Ansi,
    /// Plain UTF-8 text.
    Text,
}

/// Every format, by the name `-E` takes and what `-E list` says of it.
pub const FORMATS: &[(&str, Format, &str)] = &[
    ("utf8", Format::Utf8, "UTF-8 with ANSI escape codes"),
    ("ansi", Format::Ansi, "ANSI, in the IBM PC's code page 437"),
    ("text", Format::Text, "plain text, without colour"),
];

/// The format called `name`.
pub fn format_named(name: &str) -> Option<Format> {
    FORMATS
        .iter()
        .find(|(known, _, _)| *known == name)
        .map(|(_, format, _)| *format)
}

/// The figure as plain text.
pub fn text(figure: &Figure) -> String {
    text_rows(figure.rows())
}

/// Blocks as plain text, for output written as each line of input is rendered.
pub fn text_blocks(blocks: &[Block]) -> String {
    text_rows(blocks.iter().flat_map(|block| block.rows.iter()))
}

/// How many colours the terminal shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Depth {
    /// 24-bit colour, as Windows Terminal and conhost show it.
    #[default]
    True,
    /// The xterm palette's 256.
    Palette256,
    /// The sixteen ANSI colours.
    Ansi16,
}

impl Depth {
    /// Every depth, by the name `--colors` takes.
    pub const NAMES: &[(&str, Self)] = &[
        ("true", Self::True),
        ("24bit", Self::True),
        ("256", Self::Palette256),
        ("16", Self::Ansi16),
    ];

    /// `color` as this depth shows it.
    pub fn fit(self, color: Color) -> Color {
        match (self, color) {
            (Self::True, _) | (_, Color::Ansi(_)) => color,
            (Self::Palette256, Color::Indexed(_)) => color,
            (Self::Palette256, Color::Rgb(r, g, b)) => Color::Indexed(nearest_256(r, g, b)),
            (Self::Ansi16, _) => Color::Ansi(sixteen(color)),
        }
    }
}

/// Blocks in `format`, colour written when `colour` is true, in `depth`.
pub fn write(blocks: &[Block], format: Format, colour: bool, depth: Depth) -> Vec<u8> {
    let fitted: Vec<Block>;
    let blocks = if colour && depth != Depth::True {
        fitted = blocks
            .iter()
            .map(|block| {
                let mut block = block.clone();
                for cell in block.rows.iter_mut().flatten() {
                    cell.style.fg = cell.style.fg.map(|c| depth.fit(c));
                    cell.style.bg = cell.style.bg.map(|c| depth.fit(c));
                }
                block
            })
            .collect();
        &fitted
    } else {
        blocks
    };
    let rows = blocks.iter().flat_map(|block| block.rows.iter());
    match format {
        Format::Text => text_rows(rows).into_bytes(),
        Format::Utf8 if !colour => text_rows(rows).into_bytes(),
        Format::Utf8 => utf8_rows(rows).into_bytes(),
        Format::Ansi => ansi_rows(rows, colour),
    }
}

/// The nearest of the xterm palette's colour cube and greys.
fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
    let distance = |(pr, pg, pb): (u8, u8, u8)| {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        d(r, pr) + d(g, pg) + d(b, pb)
    };
    (16..=255_u8)
        .min_by_key(|&n| distance(Color::Indexed(n).rgb()))
        .unwrap_or(15)
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

fn utf8_rows<'a>(rows: impl Iterator<Item = &'a Row>) -> String {
    let mut out = String::new();
    for row in rows {
        let mut previous = Style::PLAIN;
        for cell in row.iter().filter(|cell| !cell.is_tail()) {
            if cell.style != previous {
                out.push_str(&sgr(&cell.style));
                previous = cell.style;
            }
            out.push(cell.ch);
        }
        if !previous.is_plain() {
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
    out
}

/// The escape that sets `style` from nothing.
pub fn sgr(style: &Style) -> String {
    if style.is_plain() {
        return "\x1b[0m".to_owned();
    }
    let mut out = String::from("\x1b[0");
    let bright_fg = matches!(style.fg, Some(Color::Ansi(8..)));
    if style.bold || bright_fg {
        out.push_str(";1");
    }
    if style.italic {
        out.push_str(";3");
    }
    if style.underline {
        out.push_str(";4");
    }
    if style.reverse {
        out.push_str(";7");
    }
    match style.fg {
        Some(Color::Ansi(n @ 0..8)) => {
            let _ = write!(out, ";3{n}");
        }
        Some(Color::Ansi(n)) => {
            let n = n.saturating_sub(8) % 8;
            let _ = write!(out, ";3{n};9{n}");
        }
        Some(Color::Indexed(n)) => {
            let _ = write!(out, ";38;5;{n}");
        }
        Some(Color::Rgb(r, g, b)) => {
            let _ = write!(out, ";38;2;{r};{g};{b}");
        }
        None => {}
    }
    match style.bg {
        Some(Color::Ansi(n @ 0..8)) => {
            let _ = write!(out, ";4{n}");
        }
        Some(Color::Ansi(n)) => {
            let _ = write!(out, ";10{}", n.saturating_sub(8) % 8);
        }
        Some(Color::Indexed(n)) => {
            let _ = write!(out, ";48;5;{n}");
        }
        Some(Color::Rgb(r, g, b)) => {
            let _ = write!(out, ";48;2;{r};{g};{b}");
        }
        None => {}
    }
    out.push('m');
    out
}

fn ansi_rows<'a>(rows: impl Iterator<Item = &'a Row>, colour: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        let mut previous = None;
        for cell in row {
            if colour {
                let fg = cell.style.fg.map_or(7, sixteen);
                let bg = cell.style.bg.map_or(0, sixteen);
                if previous != Some((fg, bg)) {
                    let code = match (fg < 8, bg < 8) {
                        (true, true) => format!("3{fg};4{bg}"),
                        (true, false) => format!("5;3{fg};4{}", bg - 8),
                        (false, true) => format!("1;3{};4{bg}", fg - 8),
                        (false, false) => format!("5;1;3{};4{}", fg - 8, bg - 8),
                    };
                    out.extend_from_slice(format!("\x1b[0;{code}m").as_bytes());
                    previous = Some((fg, bg));
                }
            }
            out.push(if cell.is_tail() { b'?' } else { cp437(cell.ch) });
        }
        if colour {
            out.extend_from_slice(b"\x1b[0m");
        }
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// The nearest of the sixteen ANSI colours, in ANSI's order (0 black, 1 red ... 8 to 15
/// bright).
pub fn sixteen(color: Color) -> u8 {
    match color {
        Color::Ansi(n) => n % 16,
        Color::Indexed(n @ 0..16) => n,
        Color::Indexed(_) | Color::Rgb(..) => {
            let (r, g, b) = color.rgb();
            nearest_16(r, g, b)
        }
    }
}

/// The nearest of the sixteen, as Windows Terminal's default scheme shows them.
fn nearest_16(r: u8, g: u8, b: u8) -> u8 {
    let distance = |&(pr, pg, pb): &(u8, u8, u8)| {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        d(r, pr) + d(g, pg) + d(b, pb)
    };
    SIXTEEN
        .iter()
        .enumerate()
        .min_by_key(|(_, colour)| distance(colour))
        .and_then(|(i, _)| u8::try_from(i).ok())
        .unwrap_or(7)
}

/// Code page 437's characters from 0x80 to 0xFF.
const CP437_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»\
░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀\
αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

/// `ch` in code page 437, `?` where it has none.
pub fn cp437(ch: char) -> u8 {
    if (' '..='~').contains(&ch) {
        return u8::try_from(u32::from(ch)).unwrap_or(b'?');
    }
    CP437_HIGH
        .chars()
        .position(|c| c == ch)
        .and_then(|at| u8::try_from(0x80 + at).ok())
        .unwrap_or(b'?')
}

#[cfg(test)]
mod tests {
    use super::{Depth, Format, cp437, sgr, sixteen, write};
    use crate::canvas::{Block, cells};
    use crate::style::{Color, Style};

    fn block(text: &str, style: Style) -> Block {
        Block {
            rows: vec![cells(text, style)],
        }
    }

    #[test]
    fn utf8_escapes_like_libcaca() {
        let red = Style {
            fg: Some(Color::Ansi(1)),
            ..Style::PLAIN
        };
        let bright = Style {
            fg: Some(Color::Ansi(12)),
            ..Style::PLAIN
        };
        assert_eq!(sgr(&red), "\x1b[0;31m");
        assert_eq!(sgr(&bright), "\x1b[0;1;34;94m");
        let blocks = [block("ab", red), block("c", Style::PLAIN)];
        assert_eq!(
            String::from_utf8(write(&blocks, Format::Utf8, true, Depth::True)).unwrap(),
            "\x1b[0;31mab\x1b[0m\nc\n"
        );
        assert_eq!(write(&blocks, Format::Utf8, false, Depth::True), b"ab\nc\n");
        assert_eq!(write(&blocks, Format::Text, true, Depth::True), b"ab\nc\n");
    }

    #[test]
    fn ansi_is_code_page_437() {
        assert_eq!(cp437('A'), b'A');
        assert_eq!(cp437('░'), 0xb0);
        assert_eq!(cp437('█'), 0xdb);
        assert_eq!(cp437('é'), 0x82);
        assert_eq!(cp437('\u{a0}'), 0xff);
        assert_eq!(cp437('ア'), b'?');
        let blocks = [block("░A", Style::PLAIN)];
        assert_eq!(
            write(&blocks, Format::Ansi, false, Depth::True),
            b"\xb0A\r\n"
        );
        assert_eq!(
            write(&blocks, Format::Ansi, true, Depth::True),
            b"\x1b[0;37;40m\xb0A\x1b[0m\r\n"
        );
    }

    #[test]
    fn colours_come_down_to_sixteen() {
        assert_eq!(sixteen(Color::Rgb(250, 10, 20)), 1);
        // xterm's pure red is nearest Windows Terminal's red, not its pinkish bright red.
        assert_eq!(sixteen(Color::Indexed(196)), 1);
        assert_eq!(sixteen(Color::Indexed(231)), 15);
        assert_eq!(sixteen(Color::Indexed(232)), 0);
        assert_eq!(sixteen(Color::Ansi(13)), 13);
    }
}
