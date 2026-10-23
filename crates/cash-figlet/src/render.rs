//! Turning text into a figure, as figlet 2.2.5 does it.
//!
//! Its `main` loop, `addchar`, `smushamt`, `smushem`, `splitline` and `putstring` are
//! ported from `figlet.c` (BSD-3-Clause, licenses/figlet-BSD-3-Clause.txt), over cells
//! instead of bytes.
//!
//! Characters are added to the line one at a time, each moved left as far as the layout
//! allows ("smushing", "kerning", or not at all for full width). When the next one does
//! not fit in the width, the line is broken at its last run of blanks, or before that
//! character when there is none, and printed: cut to one column less than the width and
//! padded on the left for centring or right justification, hardblanks shown as blanks.
//! rs-figlet (Apache-2.0) read the same code before; its smusher was the starting point.

use crate::canvas::{Block, Cell, Figure, Row};
use crate::control::Controls;
use crate::font::{DEUTSCH, Font, Glyph};

/// Smushing rule 1: two equal characters become one.
pub const SM_EQUAL: i32 = 1;
/// Smushing rule 2: an underscore gives way to `|/\[]{}()<>`.
pub const SM_LOWLINE: i32 = 2;
/// Smushing rule 4: of `|`, `/\`, `[]`, `{}`, `()`, `<>`, the later class stays.
pub const SM_HIERARCHY: i32 = 4;
/// Smushing rule 8: opposite brackets of a pair become `|`.
pub const SM_PAIR: i32 = 8;
/// Smushing rule 16: `/\` becomes `|`, `\/` becomes `Y`, `><` becomes `X`.
pub const SM_BIGX: i32 = 16;
/// Smushing rule 32: two hardblanks become one.
pub const SM_HARDBLANK: i32 = 32;
/// Kerning: characters move together until they touch.
pub const SM_KERN: i32 = 64;
/// Smushing: characters move one column further and overlap by the rules.
pub const SM_SMUSH: i32 = 128;

/// How characters are put together, as figlet's `-s`, `-k`, `-W`, `-o`, `-S` and `-m` say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// The font's own layout (`-s`).
    #[default]
    Font,
    /// Kerning (`-k`).
    Kerning,
    /// Full width (`-W`).
    FullWidth,
    /// Smushing by overlapping whatever meets (`-o`).
    Overlap,
    /// Smushing by the font's rules, or overlapping when it has none (`-S`).
    Smush,
    /// A layout given as a number (`-m`): -1 full width, 0 kerning, a sum of rules to
    /// smush with them; below -1, the font's.
    Mode(i32),
}

/// Where a printed line goes within the width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    /// Against the left edge (`-l`).
    Left,
    /// Centred (`-c`).
    Center,
    /// Against the right edge (`-r`).
    Right,
}

/// Which way text runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Left to right (`-L`).
    LeftToRight,
    /// Right to left (`-R`).
    RightToLeft,
}

/// Everything that decides how text is rendered, besides the font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// How characters are put together.
    pub layout: Layout,
    /// The output width in columns; a printed line takes at most one less. 1 means no
    /// limit on what is printed, one character to a line.
    pub width: usize,
    /// Where lines go; `None` is right for a font that runs right to left, else left.
    pub justify: Option<Justify>,
    /// Which way text runs; `None` is the font's way.
    pub direction: Option<Direction>,
    /// Paragraph mode (`-p`): a single newline is a blank, only a blank line ends one.
    pub paragraph: bool,
    /// `[\]{|}~` as `ÄÖÜäöüß` (`-D`).
    pub deutsch: bool,
    /// Blank columns after every character, which smushing and kerning leave alone.
    pub letter_spacing: usize,
}

/// What letter spacing puts after a character: kept apart from blanks, so that kerning
/// stops at it, and printed as one.
const SPACER: char = '\u{ffff}';

impl Default for Settings {
    fn default() -> Self {
        Self {
            layout: Layout::Font,
            width: 80,
            justify: None,
            direction: None,
            paragraph: false,
            deutsch: false,
            letter_spacing: 0,
        }
    }
}

impl Settings {
    /// The smushing mode these settings give with `font`: figlet's `smushmode` after
    /// `getparams` and `readfont`.
    pub const fn smush_mode(&self, font: &Font) -> i32 {
        let own = font.header.layout();
        match self.layout {
            Layout::Font => own,
            Layout::Kerning => SM_KERN,
            Layout::FullWidth => 0,
            Layout::Overlap => SM_SMUSH,
            Layout::Smush => SM_SMUSH | own,
            Layout::Mode(mode) if mode < -1 => own,
            Layout::Mode(0) => SM_KERN,
            Layout::Mode(-1) => 0,
            Layout::Mode(mode) => (mode & 63) | SM_SMUSH,
        }
    }
}

/// The characters figlet reads from its command line: the words, a blank between two, a
/// newline for an empty word, and nothing after the last (figlet's `Agetchar`).
pub fn words_input<S: AsRef<str>>(words: &[S]) -> Vec<char> {
    let mut input = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let word = word.as_ref();
        input.extend(word.chars());
        if i + 1 < words.len() {
            input.push(if word.is_empty() { '\n' } else { ' ' });
        }
    }
    input
}

/// The characters of text read from a file or a pipe, a CRLF line end read as one
/// newline, as a Windows file needs.
pub fn text_input(text: &str) -> Vec<char> {
    let mut input: Vec<char> = Vec::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.peek() == Some(&'\n') {
            continue;
        }
        input.push(c);
    }
    input
}

/// Renders `input` in `font`, with the control files' translations and `settings`.
pub fn render(font: &Font, controls: &Controls, settings: &Settings, input: &[char]) -> Figure {
    let mut renderer = Renderer::new(font, controls, settings);
    renderer.push(input);
    Figure {
        blocks: renderer.finish(),
    }
}

/// A glyph with no rows, for a character the font lacks and has no character 0 for.
static MISSING: Glyph = Glyph { rows: Vec::new() };

/// figlet's state while it renders.
///
/// Text goes in a piece at a time ([`Renderer::push`]), and each line comes out as a
/// block as soon as it is broken, as figlet prints a line of standard input once it has
/// read it.
pub struct Renderer<'f> {
    font: &'f Font,
    controls: &'f Controls,
    paragraph: bool,
    deutsch: bool,
    letter_spacing: usize,
    /// -1: a break was forced by a blank or by a character too wide for a line, and
    /// blanks are to be swallowed; 0: the line is blanks; 1: no word break yet; 2: blanks
    /// after a word; 3: a word after a word break.
    word_break: i8,
    last_was_eol: bool,
    /// A newline at the end of a piece in paragraph mode, which the next character
    /// decides.
    pending_newline: bool,
    smush_mode: i32,
    right_to_left: bool,
    justification: usize,
    width: usize,
    line_limit: usize,
    hardblank: char,
    height: usize,
    /// The line being built, a row per row of the font (`outputline`).
    line: Vec<Row>,
    /// Its width: the first row's (`outlinelen`).
    line_len: usize,
    /// The characters it was built from, for breaking it (`inchrline`).
    line_chars: Vec<u32>,
    line_chars_limit: usize,
    current: &'f Glyph,
    current_width: usize,
    previous_width: usize,
    output: Vec<Block>,
}

impl<'f> Renderer<'f> {
    /// A renderer for `font`, with the control files' translations and `settings`.
    pub fn new(font: &'f Font, controls: &'f Controls, settings: &Settings) -> Self {
        let right_to_left = match settings.direction {
            Some(Direction::RightToLeft) => true,
            Some(Direction::LeftToRight) => false,
            None => font.header.right_to_left(),
        };
        let justification = match settings.justify {
            Some(Justify::Left) => 0,
            Some(Justify::Center) => 1,
            Some(Justify::Right) => 2,
            None => 2 * usize::from(right_to_left),
        };
        let width = settings.width.max(1);
        let height = font.header.height;
        Self {
            font,
            controls,
            paragraph: settings.paragraph,
            deutsch: settings.deutsch,
            letter_spacing: settings.letter_spacing,
            word_break: 0,
            last_was_eol: false,
            pending_newline: false,
            smush_mode: settings.smush_mode(font),
            right_to_left,
            justification,
            width,
            line_limit: width - 1,
            hardblank: font.header.hardblank,
            height,
            line: vec![Row::new(); height],
            line_len: 0,
            line_chars: Vec::new(),
            line_chars_limit: width * 4 + 100,
            current: &MISSING,
            current_width: 0,
            previous_width: 0,
            output: Vec::new(),
        }
    }

    /// Renders the next piece of text; the lines it completes are taken with
    /// [`Renderer::take_blocks`].
    pub fn push(&mut self, input: &[char]) {
        if std::mem::take(&mut self.pending_newline) {
            let mut joined = Vec::with_capacity(input.len() + 1);
            joined.push('\n');
            joined.extend_from_slice(input);
            self.process(&joined, false);
        } else {
            self.process(input, false);
        }
    }

    /// The blocks finished so far, taken.
    pub fn take_blocks(&mut self) -> Vec<Block> {
        std::mem::take(&mut self.output)
    }

    /// Ends the text: the last line is printed, and every block not yet taken returned.
    pub fn finish(mut self) -> Vec<Block> {
        if std::mem::take(&mut self.pending_newline) {
            self.process(&['\n'], true);
        }
        if self.line_len != 0 {
            self.print_line();
        }
        self.output
    }

    /// figlet's `main` after the options: the characters one by one, and the line breaks.
    fn process(&mut self, input: &[char], at_end: bool) {
        let mut word_break = self.word_break;
        let mut i = 0;
        while let Some(&ch) = input.get(i) {
            i += 1;
            // In paragraph mode a newline is a blank, unless a blank of any kind follows.
            let paragraph_break = ch == '\n' && self.paragraph && !self.last_was_eol;
            if paragraph_break && i == input.len() && !at_end {
                self.pending_newline = true;
                break;
            }
            let mut c = if paragraph_break
                && !input
                    .get(i)
                    .is_some_and(|n| n.is_ascii_whitespace() || *n == '\x0b')
            {
                u32::from(' ')
            } else {
                u32::from(ch)
            };
            self.last_was_eol = is_c_space(c) && c != u32::from('\t') && c != u32::from(' ');

            if self.deutsch {
                let at = match char::from_u32(c) {
                    Some('['..=']') => usize::try_from(c - u32::from('[')).ok(),
                    Some('{'..='~') => usize::try_from(c - u32::from('{') + 3).ok(),
                    _ => None,
                };
                if let Some(german) = at.and_then(|at| DEUTSCH.get(at)) {
                    c = *german;
                }
            }

            c = self.controls.map(c);

            if is_c_space(c) {
                c = if c == u32::from('\t') || c == u32::from(' ') {
                    u32::from(' ')
                } else {
                    u32::from('\n')
                };
            }
            if (c > 0 && c < u32::from(' ') && c != u32::from('\n')) || c == 127 {
                continue;
            }

            loop {
                let mut not_added = false;
                if word_break == -1 {
                    if c == u32::from(' ') {
                        break;
                    } else if c == u32::from('\n') {
                        word_break = 0;
                        break;
                    }
                    word_break = 0;
                }

                if c == u32::from('\n') {
                    self.print_line();
                    word_break = 0;
                } else if self.add_char(c) {
                    word_break = if c != u32::from(' ') {
                        if word_break >= 2 { 3 } else { 1 }
                    } else if word_break > 0 {
                        2
                    } else {
                        0
                    };
                } else if self.line_len == 0 {
                    // A character wider than a line: printed on its own, cut.
                    let rows = (0..self.height)
                        .map(|row| {
                            let cells = self.current.rows.get(row).map_or(&[][..], Vec::as_slice);
                            if self.right_to_left && self.width > 1 {
                                let from = cells.len().saturating_sub(self.line_limit);
                                self.put_string(cells.get(from..).unwrap_or(&[]))
                            } else {
                                self.put_string(cells)
                            }
                        })
                        .collect();
                    self.output.push(Block { rows });
                    word_break = -1;
                } else if c == u32::from(' ') {
                    if word_break == 2 {
                        self.split_line();
                    } else {
                        self.print_line();
                    }
                    word_break = -1;
                } else {
                    if word_break >= 2 {
                        self.split_line();
                    } else {
                        self.print_line();
                    }
                    word_break = if word_break == 3 { 1 } else { 0 };
                    not_added = true;
                }
                if !not_added {
                    break;
                }
            }
        }
        self.word_break = word_break;
    }

    /// Makes `c`'s glyph the current one (`getletter`): the font's, else its character 0,
    /// else nothing.
    fn get_letter(&mut self, c: u32) {
        self.current = self
            .font
            .glyph(c)
            .or_else(|| self.font.glyph(0))
            .unwrap_or(&MISSING);
        self.previous_width = self.current_width;
        self.current_width = self.current.width();
    }

    /// Two characters made one, or `None` when the rules do not allow it (`smushem`).
    fn smush(&self, left: Option<Cell>, right: Option<Cell>) -> Option<Cell> {
        let (l, r) = match (left, right) {
            (Some(l), Some(r)) => (l, r),
            // Past the end of a row: figlet smushes with its `'\0'`, which a blank gives
            // back, so the blank columns of a glyph moved into an empty line are gone.
            (Some(one), None) | (None, Some(one)) => {
                return (one.ch != ' ').then_some(one);
            }
            (None, None) => return None,
        };
        if l.ch == ' ' {
            return Some(r);
        }
        if r.ch == ' ' {
            return Some(l);
        }
        if self.previous_width < 2 || self.current_width < 2 {
            return None;
        }
        // Half of a wide character never overlaps anything but a blank.
        if l.is_tail()
            || r.is_tail()
            || l.is_wide()
            || r.is_wide()
            || l.ch == SPACER
            || r.ch == SPACER
        {
            return None;
        }
        if self.smush_mode & SM_SMUSH == 0 {
            return None;
        }
        let hb = self.hardblank;
        // No rule among the low six bits: overlapping. The visible character wins, else
        // the later one in the text.
        if self.smush_mode.trailing_zeros() >= 6 {
            if l.ch == hb {
                return Some(r);
            }
            if r.ch == hb {
                return Some(l);
            }
            return Some(if self.right_to_left { l } else { r });
        }
        if self.smush_mode & SM_HARDBLANK != 0 && l.ch == hb && r.ch == hb {
            return Some(l);
        }
        if l.ch == hb || r.ch == hb {
            return None;
        }
        if self.smush_mode & SM_EQUAL != 0 && l.ch == r.ch {
            return Some(l);
        }
        if self.smush_mode & SM_LOWLINE != 0 {
            if l.ch == '_' && "|/\\[]{}()<>".contains(r.ch) {
                return Some(r);
            }
            if r.ch == '_' && "|/\\[]{}()<>".contains(l.ch) {
                return Some(l);
            }
        }
        if self.smush_mode & SM_HIERARCHY != 0 {
            for (class, later) in [
                ("|", "/\\[]{}()<>"),
                ("/\\", "[]{}()<>"),
                ("[]", "{}()<>"),
                ("{}", "()<>"),
                ("()", "<>"),
            ] {
                if class.contains(l.ch) && later.contains(r.ch) {
                    return Some(r);
                }
                if class.contains(r.ch) && later.contains(l.ch) {
                    return Some(l);
                }
            }
        }
        let with = |ch| Some(Cell { ch, style: r.style });
        if self.smush_mode & SM_PAIR != 0 {
            match (l.ch, r.ch) {
                ('[', ']') | (']', '[') | ('{', '}') | ('}', '{') | ('(', ')') | (')', '(') => {
                    return with('|');
                }
                _ => {}
            }
        }
        if self.smush_mode & SM_BIGX != 0 {
            match (l.ch, r.ch) {
                ('/', '\\') => return with('|'),
                ('\\', '/') => return with('Y'),
                ('>', '<') => return with('X'),
                _ => {}
            }
        }
        None
    }

    /// How far the current glyph can move into the line (`smushamt`).
    fn smush_amount(&self) -> usize {
        if self.smush_mode & (SM_SMUSH | SM_KERN) == 0 {
            return 0;
        }
        let blank = |cell: Option<Cell>| cell.is_none_or(|c| c.ch == ' ');
        let mut max_smush = self.current_width;
        for row in 0..self.height {
            let line = self.line.get(row).map_or(&[][..], Vec::as_slice);
            let glyph = self.current.rows.get(row).map_or(&[][..], Vec::as_slice);
            let (amount, ch1, ch2) = if self.right_to_left {
                max_smush = max_smush.min(line.len());
                let mut glyph_bd = glyph.len();
                let mut ch1 = glyph.get(glyph_bd).copied();
                while glyph_bd > 0 && blank(ch1) {
                    glyph_bd -= 1;
                    ch1 = glyph.get(glyph_bd).copied();
                }
                let line_bd = line.iter().take_while(|c| c.ch == ' ').count();
                let ch2 = line.get(line_bd).copied();
                let amount =
                    to_isize(line_bd) + to_isize(self.current_width) - 1 - to_isize(glyph_bd);
                (amount, ch1, ch2)
            } else {
                let mut line_bd = line.len();
                let mut ch1 = line.get(line_bd).copied();
                while line_bd > 0 && blank(ch1) {
                    line_bd -= 1;
                    ch1 = line.get(line_bd).copied();
                }
                let glyph_bd = glyph.iter().take_while(|c| c.ch == ' ').count();
                let ch2 = glyph.get(glyph_bd).copied();
                let amount = to_isize(glyph_bd) + to_isize(self.line_len) - 1 - to_isize(line_bd);
                (amount, ch1, ch2)
            };
            let mut amount = amount;
            if blank(ch1) {
                amount += 1;
            } else if ch2.is_some() && self.smush(ch1, ch2).is_some() {
                amount += 1;
            }
            max_smush = max_smush.min(usize::try_from(amount).unwrap_or(0));
        }
        max_smush
    }

    /// Adds `c` to the line if it fits (`addchar`).
    fn add_char(&mut self, c: u32) -> bool {
        self.get_letter(c);
        let amount = self.smush_amount();
        if self.line_len + self.current_width + self.letter_spacing > self.line_limit + amount
            || self.line_chars.len() + 1 > self.line_chars_limit
        {
            return false;
        }
        let glyph = self.current;
        for row in 0..self.height {
            let glyph_row = glyph.rows.get(row).map_or(&[][..], Vec::as_slice);
            let Some(line) = self.line.get(row) else {
                continue;
            };
            let new_row = if self.right_to_left {
                let mut row = glyph_row.to_vec();
                for k in 0..amount {
                    let at = (self.current_width + k).saturating_sub(amount);
                    let merged = self.smush(row.get(at).copied(), line.get(k).copied());
                    put(&mut row, at, merged);
                }
                row.extend_from_slice(line.get(amount..).unwrap_or(&[]));
                row
            } else {
                let mut row = line.clone();
                for k in 0..amount {
                    let at = (self.line_len + k).saturating_sub(amount);
                    let merged = self.smush(row.get(at).copied(), glyph_row.get(k).copied());
                    put(&mut row, at, merged);
                }
                row.extend_from_slice(glyph_row.get(amount..).unwrap_or(&[]));
                row
            };
            if let Some(line) = self.line.get_mut(row) {
                *line = new_row;
                // Letter spacing: on the side the next character comes from.
                let spacers = std::iter::repeat_n(Cell::plain(SPACER), self.letter_spacing);
                if self.right_to_left {
                    line.splice(0..0, spacers);
                } else {
                    line.extend(spacers);
                }
            }
        }
        self.line_len = self.line.first().map_or(0, Vec::len);
        self.line_chars.push(c);
        true
    }

    /// One printed row (`putstring`): cut to the width less one, padded for the
    /// justification, hardblanks as blanks.
    fn put_string(&self, cells: &[Cell]) -> Row {
        let mut len = cells.len();
        let mut row = Row::new();
        if self.width > 1 {
            len = len.min(self.width - 1);
            if self.justification > 0 {
                // figlet's `for (i=1;(3-j)*i+len+j-2<outputwidth;i++)`.
                let j = self.justification;
                let mut i = 1;
                while (3 - j) * i + len + j < self.width + 2 {
                    row.push(Cell::BLANK);
                    i += 1;
                }
            }
        }
        for (k, cell) in cells.iter().take(len).enumerate() {
            let cut_head = cell.is_wide() && k + 1 == len;
            let lone_tail = cell.is_tail() && k == 0;
            if cell.ch == self.hardblank || cell.ch == SPACER || cut_head || lone_tail {
                row.push(Cell::BLANK);
            } else {
                row.push(*cell);
            }
        }
        row
    }

    /// Prints the line as a block and starts a new one (`printline`).
    fn print_line(&mut self) {
        let rows = self.line.iter().map(|row| self.put_string(row)).collect();
        self.output.push(Block { rows });
        self.clear_line();
    }

    fn clear_line(&mut self) {
        for row in &mut self.line {
            row.clear();
        }
        self.line_len = 0;
        self.line_chars.clear();
    }

    /// Breaks the line at its last run of blanks: what is before it is printed, what is
    /// after it starts the next line (`splitline`).
    fn split_line(&mut self) {
        let chars = std::mem::take(&mut self.line_chars);
        let mut got_space = false;
        let mut last_space = chars.len().saturating_sub(1);
        let mut end = 0;
        for i in (0..chars.len()).rev() {
            let is_space = chars.get(i) == Some(&u32::from(' '));
            if !got_space && is_space {
                got_space = true;
                last_space = i;
            }
            if got_space && !is_space {
                end = i + 1;
                break;
            }
        }
        let first: Vec<u32> = chars.get(..end).unwrap_or(&[]).to_vec();
        let second: Vec<u32> = chars.get(last_space + 1..).unwrap_or(&[]).to_vec();
        self.clear_line();
        for c in first {
            self.add_char(c);
        }
        self.print_line();
        for c in second {
            self.add_char(c);
        }
    }
}

/// Writes `cell` at `at` in `row`, padding with blanks when the row is shorter; with no
/// cell (the rules allowed no overlap there), what was there stays.
fn put(row: &mut Row, at: usize, cell: Option<Cell>) {
    let Some(cell) = cell else {
        return;
    };
    if at >= row.len() {
        row.resize(at, Cell::BLANK);
        row.push(cell);
    } else if let Some(slot) = row.get_mut(at) {
        *slot = cell;
    }
}

/// C's `isascii(c) && isspace(c)`.
const fn is_c_space(c: u32) -> bool {
    matches!(c, 9..=13 | 32)
}

fn to_isize(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Justify, Layout, Settings, render, text_input, words_input};
    use crate::control::Controls;
    use crate::export;
    use crate::font::Font;

    fn tiny() -> Font {
        Font::parse(&crate::font::tests::tiny_font()).unwrap()
    }

    fn show(font: &Font, settings: &Settings, input: &[char]) -> String {
        export::text(&render(font, &Controls::new(), settings, input))
    }

    #[test]
    fn words_become_one_line() {
        assert_eq!(words_input(&["ab", "cd"]), ['a', 'b', ' ', 'c', 'd']);
        assert_eq!(
            words_input(&["ab", "", "cd"]),
            ['a', 'b', ' ', '\n', 'c', 'd']
        );
        assert_eq!(text_input("a\r\nb\n"), ['a', '\n', 'b', '\n']);
    }

    #[test]
    fn full_width_wrapping_and_justification() {
        let font = tiny();
        let settings = Settings {
            width: 6,
            ..Settings::default()
        };
        // At most five columns to a line, broken at the blank.
        assert_eq!(
            show(&font, &settings, &words_input(&["abc", "def"])),
            "abc\ndef\n"
        );
        assert_eq!(
            show(&font, &settings, &words_input(&["abcdefg"])),
            "abcde\nfg\n"
        );
        let settings = Settings {
            width: 7,
            justify: Some(Justify::Right),
            ..Settings::default()
        };
        assert_eq!(show(&font, &settings, &words_input(&["ab"])), "    ab\n");
        let settings = Settings {
            width: 7,
            justify: Some(Justify::Center),
            ..Settings::default()
        };
        assert_eq!(show(&font, &settings, &words_input(&["ab"])), "  ab\n");
    }

    #[test]
    fn hardblanks_print_as_blanks_and_newlines_end_lines() {
        let font = tiny();
        let settings = Settings::default();
        assert_eq!(
            show(&font, &settings, &text_input("a b\n\nc")),
            "a b\n\nc\n"
        );
    }

    #[test]
    fn pieces_render_as_the_whole() {
        let font = tiny();
        let controls = Controls::new();
        for paragraph in [false, true] {
            let settings = Settings {
                width: 8,
                paragraph,
                ..Settings::default()
            };
            let text = "one two\nthree\n\nfour five six\n";
            let whole = render(&font, &controls, &settings, &text_input(text));
            let mut renderer = super::Renderer::new(&font, &controls, &settings);
            let mut blocks = Vec::new();
            for line in text.split_inclusive('\n') {
                renderer.push(&text_input(line));
                blocks.extend(renderer.take_blocks());
            }
            blocks.extend(renderer.finish());
            assert_eq!(blocks, whole.blocks, "paragraph {paragraph}");
        }
    }

    #[test]
    fn letter_spacing_keeps_letters_apart() {
        let font = tiny();
        let settings = Settings {
            letter_spacing: 2,
            ..Settings::default()
        };
        assert_eq!(show(&font, &settings, &words_input(&["ab"])), "a  b  \n");
        let kerned = Settings {
            layout: Layout::Kerning,
            ..settings
        };
        assert_eq!(show(&font, &kerned, &words_input(&["ab"])), "a  b  \n");
    }

    #[test]
    fn layouts() {
        let font = tiny();
        let mode = |layout| {
            Settings {
                layout,
                ..Settings::default()
            }
            .smush_mode(&font)
        };
        assert_eq!(mode(Layout::Font), 0);
        assert_eq!(mode(Layout::Kerning), 64);
        assert_eq!(mode(Layout::Smush), 128);
        assert_eq!(mode(Layout::Mode(15)), 143);
        assert_eq!(mode(Layout::Mode(-2)), 0);
    }
}
