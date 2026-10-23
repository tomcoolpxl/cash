//! FIGfont 2.0 files: figlet's `.flf` and toilet's `.tlf`, read as figlet 2.2.5 reads them
//! (`readfont` and `readfontchar` in `figlet.c`).
//!
//! The header gives the hardblank, the height, the old and full layouts, the number of
//! comment lines and the print direction. The 95 printable ASCII characters and seven
//! German letters follow in order, then any number of characters tagged with their code in
//! decimal, octal (`0` first) or hex (`0x`). Each line of a glyph ends with an end mark,
//! repeated on the glyph's last line; figlet strips trailing blanks and then every
//! repeat of the line's last character, and so does this.
//!
//! A file that is valid UTF-8 is read as UTF-8, as figlet built with toilet's fonts reads
//! every font; anything else is read as Latin-1, which is what fonts from before Unicode
//! hold. A glyph may carry ANSI SGR escapes, which is how toilet's colour fonts colour
//! their cells. A zipped file (one member) is unzipped first, as figlet's `Zopen` does.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;

use crate::canvas::{Cell, Row};
use crate::render::{SM_KERN, SM_SMUSH};
use crate::style::Style;

/// The seven characters after the printable ASCII ones, in the order fonts give them.
pub const DEUTSCH: [u32; 7] = [196, 214, 220, 228, 246, 252, 223];

/// Which kind of font a file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signature {
    /// figlet's `flf2a`.
    Flf,
    /// toilet's `tlf2a`.
    Tlf,
}

/// A font's first line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// `flf2` or `tlf2`.
    pub signature: Signature,
    /// The character that is a blank smushing must not touch, printed as a blank.
    pub hardblank: char,
    /// The number of rows in every glyph.
    pub height: usize,
    /// Rows from the top to the baseline.
    pub baseline: i32,
    /// The widest line in the file, end marks included.
    pub max_length: i32,
    /// The old layout: -1 full width, 0 kerning, 1 to 63 smushing rules.
    pub old_layout: i32,
    /// How many comment lines follow the header.
    pub comment_lines: usize,
    /// 0 left to right, 1 right to left; `None` when the header does not say.
    pub print_direction: Option<i32>,
    /// The full layout, when the header has it.
    pub full_layout: Option<i32>,
}

impl Header {
    /// The layout to render with when nothing overrides it: the full layout, or the old
    /// one decoded into its terms (figlet's `smush2`).
    pub const fn layout(&self) -> i32 {
        match self.full_layout {
            Some(full) => full,
            None if self.old_layout == 0 => SM_KERN,
            None if self.old_layout < 0 => 0,
            None => (self.old_layout & 31) | SM_SMUSH,
        }
    }

    /// Whether the font prints right to left.
    pub fn right_to_left(&self) -> bool {
        self.print_direction == Some(1)
    }
}

/// One character of a font: its rows of cells, top first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    /// The rows; figlet takes the first row's length as the glyph's width.
    pub rows: Vec<Row>,
}

impl Glyph {
    /// The width figlet gives it: its first row's, in columns.
    pub fn width(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }
}

/// Why a file is not a font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontError {
    /// The signature or the header is not a FIGfont's.
    NotAFont,
    /// The file is a zip that could not be read.
    Zip(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAFont => f.write_str("Not a FIGlet 2 font file"),
            Self::Zip(reason) => write!(f, "cannot unzip the file: {reason}"),
        }
    }
}

impl std::error::Error for FontError {}

/// A font, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    /// Its first line.
    pub header: Header,
    /// The comment lines after the header.
    pub comments: Vec<String>,
    glyphs: HashMap<u32, Glyph>,
}

impl Font {
    /// Reads a font from a file's bytes, unzipping it first when it is a zip.
    pub fn parse(bytes: &[u8]) -> Result<Self, FontError> {
        let bytes = unzipped(bytes).map_err(FontError::Zip)?;
        let text = decode(&bytes);
        let mut lines = text.split('\n');
        let first = lines.next().ok_or(FontError::NotAFont)?;
        let header = parse_header(first).ok_or(FontError::NotAFont)?;
        let comments = lines
            .by_ref()
            .take(header.comment_lines)
            .map(|line| line.trim_end_matches('\r').to_owned())
            .collect();

        let mut glyphs = HashMap::new();
        let height = header.height;
        let read_glyph = |lines: &mut std::str::Split<'_, char>| Glyph {
            rows: (0..height)
                .map(|_| glyph_row(lines.next().unwrap_or("")))
                .collect(),
        };
        for code in u32::from(b' ')..=u32::from(b'~') {
            glyphs.insert(code, read_glyph(&mut lines));
        }
        for code in DEUTSCH {
            glyphs.insert(code, read_glyph(&mut lines));
        }
        // Code-tagged characters, until a line that does not start with a number.
        while let Some(line) = lines.next() {
            let Some(code) = parse_c_long(line) else {
                break;
            };
            let glyph = read_glyph(&mut lines);
            if let Ok(code) = u32::try_from(code) {
                glyphs.entry(code).or_insert(glyph);
            }
        }
        Ok(Self {
            header,
            comments,
            glyphs,
        })
    }

    /// A font made rather than read: `header` and the glyphs by their codes.
    pub const fn from_glyphs(header: Header, glyphs: HashMap<u32, Glyph>) -> Self {
        Self {
            header,
            comments: Vec::new(),
            glyphs,
        }
    }

    /// The glyph for `code`, if the font has one.
    pub fn glyph(&self, code: u32) -> Option<&Glyph> {
        self.glyphs.get(&code)
    }

    /// Every code the font has a glyph for, in order.
    pub fn codes(&self) -> Vec<u32> {
        let mut codes: Vec<u32> = self.glyphs.keys().copied().collect();
        codes.sort_unstable();
        codes
    }
}

/// A file's bytes, unzipped when they are a zip: its first member, as figlet reads a
/// zipped font or control file.
pub fn unzipped(bytes: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    use cash_archive::zip::read;
    use std::io::{Cursor, Read, Seek, SeekFrom};

    if !bytes.starts_with(b"PK\x03\x04") {
        return Ok(Cow::Borrowed(bytes));
    }
    let mut file = Cursor::new(bytes);
    let archive = read::open(&mut file).map_err(|e| format!("{e:?}"))?;
    let entry = archive
        .entries
        .first()
        .ok_or_else(|| "the zip is empty".to_owned())?;
    let local = read::local(&mut file, &archive, entry).map_err(|e| format!("{e:?}"))?;
    file.seek(SeekFrom::Start(local.data_offset))
        .map_err(|e| e.to_string())?;
    let mut data = read::data((&mut file).take(entry.compressed_size), entry, None)
        .map_err(|e| format!("{e:?}"))?;
    let mut out = Vec::new();
    data.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(Cow::Owned(out))
}

/// A file's text: UTF-8 when it is valid UTF-8, Latin-1 otherwise.
pub fn decode(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => Cow::Owned(bytes.iter().map(|&b| char::from(b)).collect()),
    }
}

/// The header, read as figlet's `sscanf(line, "%*c%c %d %*d %d %d %d %d %d")` reads what
/// follows the four-character signature: at least the hardblank and five numbers.
fn parse_header(line: &str) -> Option<Header> {
    let mut chars = line.chars();
    let magic: String = chars.by_ref().take(4).collect();
    let signature = match magic.as_str() {
        "flf2" => Signature::Flf,
        "tlf2" => Signature::Tlf,
        _ => return None,
    };
    chars.next()?; // the `a`
    let hardblank = chars.next()?;
    let mut rest = Numbers {
        text: chars.as_str(),
    };
    let height = rest.next()?;
    let baseline = rest.next()?;
    let max_length = rest.next()?;
    let old_layout = rest.next()?;
    let comment_lines = rest.next()?;
    let print_direction = rest.next();
    let full_layout = print_direction.and_then(|_| rest.next());
    Some(Header {
        signature,
        hardblank,
        height: usize::try_from(height).unwrap_or(0).max(1),
        baseline,
        max_length,
        old_layout,
        comment_lines: usize::try_from(comment_lines).unwrap_or(0),
        print_direction,
        full_layout,
    })
}

/// Decimal numbers read one after another, as `sscanf`'s `%d` reads them: blanks
/// skipped, an optional sign, digits up to the first character that is not one.
struct Numbers<'a> {
    text: &'a str,
}

impl Iterator for Numbers<'_> {
    type Item = i32;

    fn next(&mut self) -> Option<i32> {
        let text = self.text.trim_start();
        let digits_from = usize::from(text.starts_with(['-', '+']));
        let digits = text
            .get(digits_from..)?
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if digits == 0 {
            return None;
        }
        let (number, rest) = text.split_at(digits_from + digits);
        self.text = rest;
        let value: i64 = number.parse().ok()?;
        Some(i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX }))
    }
}

/// A number as C's `%li` reads it at the start of `line`: blanks skipped, an optional
/// sign, then hex after `0x`, octal after `0`, or decimal.
pub fn parse_c_long(line: &str) -> Option<i64> {
    let text = line.trim_start();
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (radix, digits) = if let Some(hex) = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .filter(|hex| hex.starts_with(|c: char| c.is_ascii_hexdigit()))
    {
        (16, hex)
    } else if text.starts_with('0') {
        (8, text)
    } else {
        (10, text)
    };
    let len = digits
        .chars()
        .take_while(|c| c.is_digit(radix))
        .map(char::len_utf8)
        .sum::<usize>();
    if len == 0 {
        return None;
    }
    let value = i64::from_str_radix(digits.get(..len)?, radix).ok()?;
    Some(if negative { -value } else { value })
}

/// Whether figlet's `iswspace` calls `c` a blank: what Rust calls whitespace, less the
/// no-break spaces glibc leaves out.
const fn is_blank(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\u{a0}' | '\u{2007}' | '\u{202f}')
}

/// One line of a glyph: trailing blanks and the end marks taken off, the rest made cells,
/// with any SGR escapes in it colouring what follows them.
fn glyph_row(line: &str) -> Row {
    let line = line.trim_end_matches(is_blank);
    let line = match line.chars().next_back() {
        Some(end_mark) => line.trim_end_matches(end_mark),
        None => line,
    };
    let mut row = Row::with_capacity(line.len());
    let mut style = Style::PLAIN;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                let mut params = String::new();
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        if c == 'm' {
                            style.apply_sgr(&params);
                        }
                        break;
                    }
                    params.push(c);
                }
            } else {
                chars.next();
            }
            continue;
        }
        let cell = Cell { ch, style };
        row.push(cell);
        if cell.is_wide() {
            row.push(Cell::TAIL);
        }
    }
    row
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Font, Signature, glyph_row, parse_c_long, parse_header};
    use crate::style::{Color, Style};

    #[test]
    fn headers() {
        let header = parse_header("flf2a$ 6 5 16 15 11 0 24463").unwrap();
        assert_eq!(header.signature, Signature::Flf);
        assert_eq!(header.hardblank, '$');
        assert_eq!(header.height, 6);
        assert_eq!(header.comment_lines, 11);
        assert_eq!(header.full_layout, Some(24463));
        assert_eq!(header.layout(), 24463);

        // toilet's fonts often have a blank for a hardblank.
        let header = parse_header("tlf2a 1 1 3 -1 15 0 0 0").unwrap();
        assert_eq!(header.signature, Signature::Tlf);
        assert_eq!(header.hardblank, ' ');
        assert_eq!(header.old_layout, -1);
        assert_eq!(header.layout(), 0);

        let header = parse_header("flf2a$ 6 5 20 15 3").unwrap();
        assert_eq!(header.layout(), 15 | 128);
        assert_eq!(parse_header("flf2a$ 6 5 20 0 3").unwrap().layout(), 64);
        assert!(parse_header("flf2a$ 6 5 20 15").is_none());
        assert!(parse_header("flf3a$ 6 5 20 15 3").is_none());
    }

    #[test]
    fn c_numbers() {
        assert_eq!(parse_c_long("196  LATIN CAPITAL LETTER A"), Some(196));
        assert_eq!(parse_c_long("0x0100"), Some(256));
        assert_eq!(parse_c_long("0177"), Some(127));
        assert_eq!(parse_c_long("-0x2"), Some(-2));
        assert_eq!(parse_c_long("0"), Some(0));
        assert_eq!(parse_c_long("  12x"), Some(12));
        assert_eq!(parse_c_long("abc"), None);
        assert_eq!(parse_c_long(""), None);
    }

    #[test]
    fn end_marks_go_and_colours_come_in() {
        let text: String = glyph_row(" __ @@  ").iter().map(|c| c.ch).collect();
        assert_eq!(text, " __ ");
        // figlet strips every repeat of the last character, so a glyph cannot end in it.
        let text: String = glyph_row("a@@@").iter().map(|c| c.ch).collect();
        assert_eq!(text, "a");
        let row = glyph_row("   \x1b[0;1;37;97;5;47;107m_\x1b[0m   @ ");
        let text: String = row.iter().map(|c| c.ch).collect();
        assert_eq!(text, "   _   ");
        assert!(row[0].style.is_plain());
        assert_eq!(row[3].style.fg, Some(Color::Ansi(15)));
        assert_eq!(row[4].style, Style::PLAIN);
    }

    #[test]
    fn wide_characters_take_two_columns() {
        let row = glyph_row("ア@");
        assert_eq!(row.len(), 2);
        assert!(row[1].is_tail());
    }

    #[test]
    fn a_small_font() {
        let font = Font::parse(&tiny_font()).unwrap();
        assert_eq!(font.glyph(u32::from('A')).unwrap().width(), 2);
        assert_eq!(font.glyph(0x100).unwrap().width(), 3);
        assert!(font.glyph(0x101).is_none());
        assert_eq!(font.comments, ["a comment"]);
    }

    /// A font one row high: printable ASCII as themselves, the German letters as `?`,
    /// and U+0100 tagged.
    pub(crate) fn tiny_font() -> Vec<u8> {
        use std::fmt::Write;
        let mut text = String::from("flf2a$ 1 1 4 -1 1\na comment\n");
        for c in ' '..='~' {
            let glyph = if c == ' ' { '$' } else { c };
            if c == 'A' {
                text.push_str("AA@@\n");
            } else {
                writeln!(text, "{glyph}@@").unwrap();
            }
        }
        for _ in 0..7 {
            text.push_str("?@@\n");
        }
        text.push_str("0x0100 LATIN CAPITAL LETTER A WITH MACRON\n-A-@@\n");
        text.into_bytes()
    }
}
