//! Colours and attributes of one character cell, and the SGR escapes that set them.
//!
//! toilet's colour fonts (`biggray9.tlf` and its kind) carry ANSI SGR escapes inside
//! their glyphs, as libcaca's UTF-8 importer reads them; filters and exports later set and
//! say the same attributes.

/// A colour as a terminal knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    /// One of the sixteen ANSI colours: 0 to 7 normal, 8 to 15 bright.
    Ansi(u8),
    /// One of the 256 of the xterm palette.
    Indexed(u8),
    /// 24-bit colour.
    Rgb(u8, u8, u8),
}

impl Color {
    /// A colour as written on a command line: a name (`red`, `brightblue`, `orange`),
    /// `#rrggbb`, `#rgb`, or a number from 0 to 255 for the xterm palette.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase();
        if let Some(hex) = text.strip_prefix('#') {
            let digit = |c: char| c.to_digit(16).and_then(|d| u8::try_from(d).ok());
            let digits: Vec<u8> = hex.chars().map(digit).collect::<Option<_>>()?;
            return match digits.as_slice() {
                [r, g, b] => Some(Self::Rgb(r * 17, g * 17, b * 17)),
                [r1, r2, g1, g2, b1, b2] => {
                    Some(Self::Rgb(r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2))
                }
                _ => None,
            };
        }
        if let Ok(n) = text.parse::<u8>() {
            return Some(Self::Indexed(n));
        }
        let name = text.replace(['-', '_', ' '], "");
        NAMED
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, color)| *color)
    }

    /// The colour as red, green and blue, as Windows Terminal's default scheme shows the
    /// sixteen and xterm the 256.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Ansi(n) => SIXTEEN
                .get(usize::from(n % 16))
                .copied()
                .unwrap_or((204, 204, 204)),
            Self::Indexed(n @ 0..16) => Self::Ansi(n).rgb(),
            Self::Indexed(n @ 16..232) => {
                let n = n - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
                (level(n / 36), level(n / 6 % 6), level(n % 6))
            }
            Self::Indexed(n) => {
                let grey = 8 + 10 * (n - 232);
                (grey, grey, grey)
            }
            Self::Rgb(r, g, b) => (r, g, b),
        }
    }
}

/// The sixteen colours as Windows Terminal's default scheme ("Campbell") shows them.
pub const SIXTEEN: [(u8, u8, u8); 16] = [
    (12, 12, 12),
    (197, 15, 31),
    (19, 161, 14),
    (193, 156, 0),
    (0, 55, 218),
    (136, 23, 152),
    (58, 150, 221),
    (204, 204, 204),
    (118, 118, 118),
    (231, 72, 86),
    (22, 198, 12),
    (249, 241, 165),
    (59, 120, 255),
    (180, 0, 158),
    (97, 214, 214),
    (242, 242, 242),
];

/// Colour names: the sixteen by their ANSI names, and a few more in true colour.
const NAMED: &[(&str, Color)] = &[
    ("black", Color::Ansi(0)),
    ("red", Color::Ansi(1)),
    ("green", Color::Ansi(2)),
    ("yellow", Color::Ansi(3)),
    ("blue", Color::Ansi(4)),
    ("magenta", Color::Ansi(5)),
    ("cyan", Color::Ansi(6)),
    ("white", Color::Ansi(7)),
    ("gray", Color::Ansi(8)),
    ("grey", Color::Ansi(8)),
    ("brightblack", Color::Ansi(8)),
    ("brightred", Color::Ansi(9)),
    ("brightgreen", Color::Ansi(10)),
    ("brightyellow", Color::Ansi(11)),
    ("brightblue", Color::Ansi(12)),
    ("brightmagenta", Color::Ansi(13)),
    ("brightcyan", Color::Ansi(14)),
    ("brightwhite", Color::Ansi(15)),
    ("orange", Color::Rgb(255, 140, 0)),
    ("pink", Color::Rgb(255, 105, 180)),
    ("purple", Color::Rgb(128, 0, 200)),
    ("violet", Color::Rgb(148, 0, 211)),
    ("indigo", Color::Rgb(75, 0, 130)),
    ("gold", Color::Rgb(255, 200, 0)),
    ("teal", Color::Rgb(0, 128, 128)),
    ("navy", Color::Rgb(0, 0, 128)),
    ("lime", Color::Rgb(50, 205, 50)),
    ("coral", Color::Rgb(255, 127, 80)),
    ("crimson", Color::Rgb(220, 20, 60)),
    ("silver", Color::Rgb(192, 192, 192)),
    ("brown", Color::Rgb(139, 69, 19)),
];

/// What a cell looks like besides its character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    /// The foreground colour; the terminal's own when `None`.
    pub fg: Option<Color>,
    /// The background colour; the terminal's own when `None`.
    pub bg: Option<Color>,
    /// Bold, or bright on terminals that show it so.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Blinking; libcaca also uses it to mean a bright background.
    pub blink: bool,
    /// Foreground and background swapped.
    pub reverse: bool,
}

impl Style {
    /// No colour and no attribute.
    pub const PLAIN: Self = Self {
        fg: None,
        bg: None,
        bold: false,
        italic: false,
        underline: false,
        blink: false,
        reverse: false,
    };

    /// Whether the style says nothing, so the cell prints as plain text.
    pub fn is_plain(&self) -> bool {
        *self == Self::PLAIN
    }

    /// Applies the parameters of one SGR escape (`ESC [ params m`), as `0;1;37`; an
    /// empty list is a reset. Codes it does not know are passed over.
    pub fn apply_sgr(&mut self, params: &str) {
        let codes: Vec<u16> = params
            .split([';', ':'])
            .map(|code| code.parse().unwrap_or(0))
            .collect();
        let mut i = 0;
        while i < codes.len() {
            let code = codes.get(i).copied().unwrap_or(0);
            match code {
                0 => *self = Self::PLAIN,
                1 => self.bold = true,
                3 => self.italic = true,
                4 => self.underline = true,
                5 | 6 => self.blink = true,
                7 => self.reverse = true,
                22 => self.bold = false,
                23 => self.italic = false,
                24 => self.underline = false,
                25 => self.blink = false,
                27 => self.reverse = false,
                30..=37 => self.fg = Some(Color::Ansi(low_byte(code - 30))),
                39 => self.fg = None,
                40..=47 => self.bg = Some(Color::Ansi(low_byte(code - 40))),
                49 => self.bg = None,
                90..=97 => self.fg = Some(Color::Ansi(low_byte(code - 90 + 8))),
                100..=107 => self.bg = Some(Color::Ansi(low_byte(code - 100 + 8))),
                38 | 48 => {
                    let (color, used) = extended_color(codes.get(i + 1..).unwrap_or(&[]));
                    if code == 38 {
                        self.fg = color.or(self.fg);
                    } else {
                        self.bg = color.or(self.bg);
                    }
                    i += used;
                }
                _ => {}
            }
            i += 1;
        }
    }
}

/// The colour of `38;5;n` or `38;2;r;g;b` after the 38 (or 48), and how many codes it
/// took.
fn extended_color(rest: &[u16]) -> (Option<Color>, usize) {
    match rest {
        [5, n, ..] => (Some(Color::Indexed(low_byte(*n))), 2),
        [2, r, g, b, ..] => (
            Some(Color::Rgb(low_byte(*r), low_byte(*g), low_byte(*b))),
            4,
        ),
        [5 | 2, ..] => (None, rest.len()),
        _ => (None, 0),
    }
}

/// The low eight bits of a parameter, as terminals read an out-of-range one.
fn low_byte(value: u16) -> u8 {
    u8::try_from(value & 0xff).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{Color, Style};

    #[test]
    fn libcaca_glyph_escapes() {
        let mut style = Style::PLAIN;
        style.apply_sgr("0;1;37;97;5;47;107");
        assert!(style.bold && style.blink);
        assert_eq!(style.fg, Some(Color::Ansi(15)));
        assert_eq!(style.bg, Some(Color::Ansi(15)));
        style.apply_sgr("0");
        assert!(style.is_plain());
        style.apply_sgr("");
        assert!(style.is_plain());
    }

    #[test]
    fn extended_colours() {
        let mut style = Style::PLAIN;
        style.apply_sgr("38;5;202;48;2;1;2;3;4");
        assert_eq!(style.fg, Some(Color::Indexed(202)));
        assert_eq!(style.bg, Some(Color::Rgb(1, 2, 3)));
        assert!(style.underline);
    }
}
