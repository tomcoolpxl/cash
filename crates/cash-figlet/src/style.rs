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
