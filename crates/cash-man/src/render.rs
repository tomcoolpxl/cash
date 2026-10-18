//! A page drawn for a terminal: lines of styled spans, wrapped to a width.
//!
//! The browser draws the lines in its pane; `help`, and `man` writing to a pipe, print
//! them. Colour is chosen when a line is written ([`Line::ansi`]), so a search can find
//! its matches in the plain text first. The colours are the ones cash uses elsewhere: a
//! heading in the prompt's blue accent, what you type green as the highlighter paints a
//! command, a topic cyan.
//!
//! Only the markdown the pages use is understood: `###` headings, paragraphs, `- ` and
//! `1. ` items, code indented by four spaces or fenced, tables, `` `code` `` and
//! `**bold**`. Anything else is a paragraph.

use unicode_width::UnicodeWidthStr;

use crate::Manual;
use crate::page::{Page, Subject};

/// Prose is indented by this much under a heading, as a man page indents it.
pub const INDENT: usize = 4;

/// Code is indented by this much, further than prose, so that an example stands out
/// without colour too.
pub const CODE_INDENT: usize = 8;

/// How a span is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// As it is.
    Plain,
    /// Bold: a subheading, a table's header, `**bold**` in prose.
    Bold,
    /// A section heading: bold blue, the prompt's accent.
    Heading,
    /// What to type: green, as the highlighter paints a command.
    Code,
    /// A command's name: bold green.
    Command,
    /// A comment after an example: dark grey, as the highlighter paints one.
    Comment,
    /// A topic's name: bold cyan.
    Topic,
    /// A note: yellow.
    Mark,
}

impl Style {
    /// The escape that starts the style; none for plain text.
    #[must_use]
    pub const fn escape(self) -> &'static str {
        match self {
            Self::Plain => "",
            Self::Bold => "\x1b[1m",
            Self::Heading => "\x1b[1;34m",
            Self::Code => "\x1b[32m",
            Self::Command => "\x1b[1;32m",
            Self::Comment => "\x1b[90m",
            Self::Topic => "\x1b[1;36m",
            Self::Mark => "\x1b[33m",
        }
    }
}

/// A run of text in one style, and the page it leads to when it is a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// The text.
    pub text: String,
    /// How it is drawn.
    pub style: Style,
    /// The page or topic it leads to, for a See-also link.
    pub link: Option<String>,
}

/// A line of a drawn page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    /// Its runs of text.
    pub spans: Vec<Span>,
    /// The section it is in: an index into [`Drawn::sections`].
    pub section: usize,
}

impl Line {
    /// The text without its styles.
    #[must_use]
    pub fn plain(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    /// The columns the line takes.
    #[must_use]
    pub fn width(&self) -> usize {
        self.spans.iter().map(|span| span.text.width()).sum()
    }

    /// The line with its styles as escapes when `colour`, else its plain text.
    #[must_use]
    pub fn ansi(&self, colour: bool) -> String {
        let mut out = String::new();
        for span in &self.spans {
            let escape = span.style.escape();
            if colour && !escape.is_empty() {
                out.push_str(escape);
                out.push_str(&span.text);
                out.push_str("\x1b[0m");
            } else {
                out.push_str(&span.text);
            }
        }
        out
    }

    /// Adds `text`, joined to the last span when it has the same style and link.
    fn push(&mut self, text: &str, style: Style, link: Option<&str>) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.style == style
            && last.link.as_deref() == link
        {
            last.text.push_str(text);
            return;
        }
        self.spans.push(Span {
            text: text.to_owned(),
            style,
            link: link.map(str::to_owned),
        });
    }
}

/// Where a section starts in a drawn page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionStart {
    /// Its title as drawn: in capitals for a section, the page's name for the top.
    pub title: String,
    /// Its first line.
    pub line: usize,
}

/// A page drawn: its lines, and where each section starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    /// The lines, in order.
    pub lines: Vec<Line>,
    /// The sections, in order; the first is the top of the page, before any heading.
    pub sections: Vec<SectionStart>,
}

impl Drawn {
    /// The whole page as text, each line ended by a newline.
    #[must_use]
    pub fn text(&self, colour: bool) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(line.ansi(colour).trim_end());
            out.push('\n');
        }
        out
    }
}

/// `page` drawn for a pane `width` columns wide. `manual` tells a See-also link to a
/// topic from one to a command.
#[must_use]
pub fn draw(page: &Page, manual: &Manual, width: usize) -> Drawn {
    let mut drawer = Drawer {
        drawn: Drawn::default(),
        width,
    };
    let (name, style) = match page.subject {
        Subject::Command => (page.names.join(", "), Style::Command),
        Subject::Topic => (page.title.clone(), Style::Topic),
    };
    drawer.start_section(&name);
    // The name and the summary, a dash between them, wrapped as one: a topic's title
    // can be longer than a line.
    let mut top: Vec<Word> = name
        .split_whitespace()
        .map(|word| Word {
            pieces: vec![(word.to_owned(), style)],
            space_before: true,
        })
        .collect();
    if !page.summary.is_empty() {
        top.push(Word {
            pieces: vec![("\u{2014}".to_owned(), Style::Plain)],
            space_before: true,
        });
        let mut summary = words(&page.summary);
        if let Some(first) = summary.first_mut() {
            first.space_before = true;
        }
        top.extend(summary);
    }
    drawer.wrap_words(Line::default(), top, 0);
    if page.draft {
        drawer.blank();
        let start = drawer.drawn.lines.len();
        drawer.wrap(
            Line::default(),
            "A draft, made from the command's --help: this page is not written yet.",
            0,
        );
        for line in drawer.drawn.lines.get_mut(start..).unwrap_or_default() {
            for span in &mut line.spans {
                span.style = Style::Mark;
            }
        }
    }
    if !page.intro.trim().is_empty() {
        drawer.blank();
        drawer.markdown(&page.intro);
    }
    for section in &page.sections {
        drawer.blank();
        drawer.heading(&section.title);
        drawer.markdown(&section.body);
    }
    if !page.see.is_empty() {
        drawer.blank();
        drawer.heading("See also");
        drawer.links(&page.see, manual);
    }
    drawer.drawn
}

/// A word of prose: its pieces, each in its style, written with no space between them
/// (`` `cd -`, `` is a code span and a comma), and whether a space comes before it.
struct Word {
    pieces: Vec<(String, Style)>,
    space_before: bool,
}

impl Word {
    fn width(&self) -> usize {
        self.pieces.iter().map(|(text, _)| text.width()).sum()
    }
}

/// Adds `piece` to `words`: to the last word when no space comes before it.
fn add(words: &mut Vec<Word>, piece: &str, style: Style, space: bool) {
    match words.last_mut() {
        Some(word) if !space => word.pieces.push((piece.to_owned(), style)),
        _ => words.push(Word {
            pieces: vec![(piece.to_owned(), style)],
            space_before: space,
        }),
    }
}

/// The words of `text`: code spans whole, `**bold**` bold, the rest split at spaces.
fn words(text: &str) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut space = false;
    for (index, part) in text.split('`').enumerate() {
        if index % 2 == 1 {
            add(&mut words, part, Style::Code, space);
            space = false;
            continue;
        }
        for (index, piece) in part.split("**").enumerate() {
            let style = if index % 2 == 1 {
                Style::Bold
            } else {
                Style::Plain
            };
            if piece.trim().is_empty() {
                space = space || !piece.is_empty();
                continue;
            }
            let mut first = true;
            for word in piece.split_whitespace() {
                let before = if first {
                    space || piece.starts_with(char::is_whitespace)
                } else {
                    true
                };
                add(&mut words, word, style, before);
                first = false;
            }
            space = piece.ends_with(char::is_whitespace);
        }
    }
    words
}

/// `word` broken into words at the spaces inside its pieces, each keeping its style: a
/// code span too long for a line.
fn broken(word: Word) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    let mut space = word.space_before;
    for (piece, style) in word.pieces {
        for (index, part) in piece.split(' ').enumerate() {
            if index > 0 {
                space = true;
            }
            if part.is_empty() {
                continue;
            }
            add(&mut out, part, style, space);
            space = false;
        }
    }
    out
}

/// `text` in lines of at most `width` columns, broken at spaces; a word longer than a
/// line is broken after its hyphens, and failing that where the line ends.
fn fit_words(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let parts: Vec<String> = if word.width() > width {
            word.split_inclusive('-')
                .flat_map(|part| hard_break(part, width))
                .collect()
        } else {
            vec![word.to_owned()]
        };
        for (index, part) in parts.into_iter().enumerate() {
            let space = usize::from(index == 0 && !line.is_empty());
            if !line.is_empty() && line.width() + space + part.width() > width {
                lines.push(std::mem::take(&mut line));
            } else if space == 1 {
                line.push(' ');
            }
            line.push_str(&part);
        }
    }
    lines.push(line);
    lines
}

/// `part` in pieces of at most `width` columns.
fn hard_break(part: &str, width: usize) -> Vec<String> {
    let mut pieces = vec![String::new()];
    for c in part.chars() {
        let full = pieces.last().is_some_and(|piece| {
            piece.width() + unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) > width
        });
        if full {
            pieces.push(String::new());
        }
        if let Some(piece) = pieces.last_mut() {
            piece.push(c);
        }
    }
    pieces
}

/// A block of a section's markdown.
enum Block {
    Subheading(String),
    Paragraph(String),
    /// A list item: its marker (`- `, `1. `) and its text.
    Item(String, String),
    Code(Vec<String>),
    /// The rows of a table, the header first; the `|---|` line is not one.
    Table(Vec<Vec<String>>),
}

/// The cells of a markdown table row, `| a | b |`, or none when `line` is not one or is
/// the `|---|---|` line under the header.
fn table_row(line: &str) -> Option<Vec<String>> {
    let inner = line.trim().strip_prefix('|')?;
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let cells: Vec<String> = inner
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect();
    let separator = cells
        .iter()
        .all(|cell| !cell.is_empty() && cell.trim_matches([':', '-']).is_empty());
    (!separator).then_some(cells)
}

/// The marker of an ordered item, `1. `, and its text.
fn ordered_item(line: &str) -> Option<(String, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let rest = line.get(digits..)?.strip_prefix(". ")?;
    (digits > 0).then(|| (line.get(..digits + 2).unwrap_or_default().to_owned(), rest))
}

/// The blocks of a section's markdown, in order.
fn blocks(body: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut open: Option<Block> = None;
    let mut lines = body.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        let continues = matches!(open, Some(Block::Paragraph(_) | Block::Item(..)));
        if trimmed.starts_with("```") {
            blocks.extend(open.take());
            let code = lines
                .by_ref()
                .take_while(|line| !line.trim().starts_with("```"))
                .map(str::to_owned)
                .collect();
            blocks.push(Block::Code(code));
        } else if trimmed.is_empty() {
            blocks.extend(open.take());
        } else if line.starts_with("    ") && !continues {
            blocks.extend(open.take());
            let mut code = vec![line.get(4..).unwrap_or_default().to_owned()];
            while let Some(next) = lines.peek() {
                if next.starts_with("    ") || next.trim().is_empty() {
                    code.push(next.get(4..).unwrap_or_default().to_owned());
                    lines.next();
                } else {
                    break;
                }
            }
            while code.last().is_some_and(|line| line.trim().is_empty()) {
                code.pop();
            }
            blocks.push(Block::Code(code));
        } else if let Some(text) = trimmed
            .strip_prefix("### ")
            .or_else(|| trimmed.strip_prefix("## "))
        {
            blocks.extend(open.take());
            blocks.push(Block::Subheading(text.to_owned()));
        } else if let Some(text) = trimmed.strip_prefix("- ") {
            blocks.extend(open.take());
            open = Some(Block::Item("- ".to_owned(), text.to_owned()));
        } else if let Some((marker, text)) = ordered_item(trimmed) {
            blocks.extend(open.take());
            open = Some(Block::Item(marker, text.to_owned()));
        } else if trimmed.starts_with('|') && !continues {
            let row = table_row(trimmed);
            match (&mut open, row) {
                (Some(Block::Table(rows)), Some(row)) => rows.push(row),
                (Some(Block::Table(_)), None) => {}
                (_, row) => {
                    blocks.extend(open.take());
                    open = Some(Block::Table(row.into_iter().collect()));
                }
            }
        } else if let Some(Block::Paragraph(text) | Block::Item(_, text)) = &mut open {
            text.push(' ');
            text.push_str(trimmed);
        } else {
            blocks.extend(open.take());
            open = Some(Block::Paragraph(trimmed.to_owned()));
        }
    }
    blocks.extend(open);
    blocks
}

/// The pieces of each line of a block of code. In a table of forms or options, the
/// text after the gap is a description, prose whose code spans are drawn as code, as
/// is a line that continues it at its column; everything else is [`code_pieces`].
fn code_lines(lines: &[String]) -> Vec<Vec<(String, Style)>> {
    let mut column: Option<usize> = None;
    let mut drawn = Vec::new();
    for line in lines {
        let line = line.trim_end();
        let indent = line.len() - line.trim_start().len();
        if indent > 0 && column.is_some_and(|column| indent >= column) {
            let (spaces, text) = line.split_at(indent);
            let mut pieces = vec![(spaces.to_owned(), Style::Plain)];
            pieces.extend(prose_pieces(text));
            drawn.push(pieces);
            continue;
        }
        let pieces = code_pieces(line);
        if let [
            (code, Style::Code),
            (gap, Style::Plain),
            (text, Style::Plain),
        ] = pieces.as_slice()
        {
            column = Some(code.width() + gap.width());
            let mut owned = vec![
                ((*code).to_owned(), Style::Code),
                ((*gap).to_owned(), Style::Plain),
            ];
            owned.extend(prose_pieces(text));
            drawn.push(owned);
        } else {
            if !pieces.is_empty() {
                column = None;
            }
            drawn.push(
                pieces
                    .into_iter()
                    .map(|(text, style)| (text.to_owned(), style))
                    .collect(),
            );
        }
    }
    drawn
}

/// Prose in pieces: its code spans in [`Style::Code`], without their backticks.
fn prose_pieces(text: &str) -> Vec<(String, Style)> {
    text.split('`')
        .enumerate()
        .filter(|(_, part)| !part.is_empty())
        .map(|(index, part)| {
            let style = if index % 2 == 1 {
                Style::Code
            } else {
                Style::Plain
            };
            (part.to_owned(), style)
        })
        .collect()
}

/// The pieces of a line of code: what to type in [`Style::Code`]; after two spaces, a
/// description in plain text or a comment (`#`) in [`Style::Comment`]. Spaces inside
/// quotes are the code's.
fn code_pieces(line: &str) -> Vec<(&str, Style)> {
    if line.trim().is_empty() {
        return Vec::new();
    }
    if line.trim_start().starts_with('#') {
        return vec![(line, Style::Comment)];
    }
    if line.starts_with(char::is_whitespace) {
        let text = line.trim_start();
        return vec![
            (
                line.get(..line.len() - text.len()).unwrap_or_default(),
                Style::Plain,
            ),
            (text, Style::Code),
        ];
    }
    let mut quote: Option<char> = None;
    let mut previous = '\0';
    let mut gap = None;
    for (at, c) in line.char_indices() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, ' ') if previous == ' ' => {
                gap = Some(at - 1);
                break;
            }
            _ => {}
        }
        previous = c;
    }
    let Some(at) = gap else {
        return vec![(line, Style::Code)];
    };
    let (code, rest) = line.split_at(at);
    let after = rest.trim_start();
    let (spaces, after) = rest.split_at(rest.len() - after.len());
    let style = if after.starts_with('#') {
        Style::Comment
    } else {
        Style::Plain
    };
    vec![(code, Style::Code), (spaces, Style::Plain), (after, style)]
}

/// Draws a page's parts into lines.
struct Drawer {
    drawn: Drawn,
    width: usize,
}

impl Drawer {
    const fn section(&self) -> usize {
        self.drawn.sections.len().saturating_sub(1)
    }

    fn start_section(&mut self, title: &str) {
        self.drawn.sections.push(SectionStart {
            title: title.to_owned(),
            line: self.drawn.lines.len(),
        });
    }

    fn push(&mut self, mut line: Line) {
        line.section = self.section();
        self.drawn.lines.push(line);
    }

    fn blank(&mut self) {
        if self
            .drawn
            .lines
            .last()
            .is_some_and(|line| !line.spans.is_empty())
        {
            self.push(Line::default());
        }
    }

    fn heading(&mut self, title: &str) {
        let title = title.replace('`', "").to_uppercase();
        self.start_section(&title);
        let mut line = Line::default();
        line.push(&title, Style::Heading, None);
        self.push(line);
    }

    /// `text`'s words after what `first` holds, wrapped to the width; the lines after
    /// the first indented by `indent`.
    fn wrap(&mut self, first: Line, text: &str, indent: usize) {
        self.wrap_words(first, words(text), indent);
    }

    /// `words` after what `first` holds, wrapped to the width; the lines after the first
    /// indented by `indent`. A word wider than a line, a long code span, is broken at
    /// its spaces.
    fn wrap_words(&mut self, first: Line, words: Vec<Word>, indent: usize) {
        let room = self.width.saturating_sub(indent);
        let words = words.into_iter().flat_map(|word| {
            if word.width() > room {
                broken(word)
            } else {
                vec![word]
            }
        });
        let pad = " ".repeat(indent);
        let mut line = first;
        let mut used = line.width();
        let mut empty = true;
        for word in words {
            let space = usize::from(word.space_before && !empty);
            if !empty && used + space + word.width() > self.width {
                self.push(std::mem::take(&mut line));
                line.push(&pad, Style::Plain, None);
                used = indent;
            } else if space == 1 {
                line.push(" ", Style::Plain, None);
                used += 1;
            }
            used += word.width();
            for (piece, style) in &word.pieces {
                line.push(piece, *style, None);
            }
            empty = false;
        }
        self.push(line);
    }

    fn markdown(&mut self, body: &str) {
        // Items of one list sit together: bullets with bullets, numbers with numbers.
        let mut previous_list: Option<bool> = None;
        let mut first = true;
        for block in blocks(body) {
            let list = match &block {
                Block::Item(marker, _) => Some(marker == "- "),
                _ => None,
            };
            if !first && (list.is_none() || list != previous_list) {
                self.push(Line::default());
            }
            first = false;
            previous_list = list;
            match block {
                Block::Subheading(text) => {
                    let mut line = Line::default();
                    line.push(&" ".repeat(INDENT), Style::Plain, None);
                    line.push(&text.replace('`', ""), Style::Bold, None);
                    self.push(line);
                }
                Block::Paragraph(text) => {
                    let mut line = Line::default();
                    line.push(&" ".repeat(INDENT), Style::Plain, None);
                    self.wrap(line, &text, INDENT);
                }
                Block::Item(marker, text) => {
                    let mut line = Line::default();
                    line.push(&" ".repeat(INDENT), Style::Plain, None);
                    line.push(&marker, Style::Plain, None);
                    self.wrap(line, &text, INDENT + marker.width());
                }
                Block::Code(lines) => {
                    for pieces in code_lines(&lines) {
                        let mut line = Line::default();
                        if !pieces.is_empty() {
                            line.push(&" ".repeat(CODE_INDENT), Style::Plain, None);
                        }
                        for (piece, style) in pieces {
                            line.push(&piece, style, None);
                        }
                        self.push(line);
                    }
                }
                Block::Table(rows) => self.table(&rows),
            }
        }
    }

    /// A table as aligned columns, two spaces apart, the header bold. When it is wider
    /// than the pane, its last column is wrapped under itself.
    fn table(&mut self, rows: &[Vec<String>]) {
        let columns = rows.iter().map(Vec::len).max().unwrap_or_default();
        let cell = |row: &[String], index: usize| {
            row.get(index)
                .map_or_else(String::new, |cell| cell.replace('`', ""))
        };
        let widths: Vec<usize> = (0..columns)
            .map(|index| {
                rows.iter()
                    .map(|row| cell(row, index).width())
                    .max()
                    .unwrap_or_default()
            })
            .collect();
        let before_last = columns.saturating_sub(1);
        let last_at = INDENT
            + widths
                .iter()
                .take(before_last)
                .map(|width| width + 2)
                .sum::<usize>();
        let room = self.width.saturating_sub(last_at);
        let wrap_last = columns > 1
            && last_at + widths.last().copied().unwrap_or_default() > self.width
            && room >= 12;
        for (number, row) in rows.iter().enumerate() {
            let mut text = String::new();
            for (index, width) in widths.iter().take(before_last).enumerate() {
                let cell = cell(row, index);
                text.push_str(&cell);
                text.push_str(&" ".repeat(width.saturating_sub(cell.width()) + 2));
            }
            let last = cell(row, before_last);
            let pieces = if wrap_last {
                fit_words(&last, room)
            } else {
                vec![last]
            };
            let style = if number == 0 {
                Style::Bold
            } else {
                Style::Plain
            };
            for (index, piece) in pieces.iter().enumerate() {
                let mut line = Line::default();
                if index == 0 {
                    line.push(&" ".repeat(INDENT), Style::Plain, None);
                    line.push(format!("{text}{piece}").trim_end(), style, None);
                } else {
                    line.push(&" ".repeat(last_at), Style::Plain, None);
                    line.push(piece, style, None);
                }
                self.push(line);
            }
        }
    }

    /// The See-also links, a comma between them, wrapped.
    fn links(&mut self, names: &[String], manual: &Manual) {
        let mut line = Line::default();
        line.push(&" ".repeat(INDENT), Style::Plain, None);
        let mut used = INDENT;
        for (index, name) in names.iter().enumerate() {
            let last = index + 1 == names.len();
            let width = name.width() + usize::from(!last);
            if index > 0 && used + 1 + width > self.width {
                self.push(std::mem::take(&mut line));
                line.push(&" ".repeat(INDENT), Style::Plain, None);
                used = INDENT;
            } else if index > 0 {
                line.push(" ", Style::Plain, None);
                used += 1;
            }
            let style = if manual.topic(name).is_some() && manual.page(name).is_none() {
                Style::Topic
            } else {
                Style::Command
            };
            line.push(name, style, Some(name));
            if !last {
                line.push(",", Style::Plain, None);
            }
            used += width;
        }
        self.push(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual() -> Manual {
        let page = |file: &str, text: &str| Page::parse(file, Subject::Command, text);
        Manual {
            pages: vec![page("pwd", "---\nsummary: s\nkind: k\n---\n")],
            topics: vec![Page::parse(
                "paths",
                Subject::Topic,
                "---\ntitle: Paths\n---\n",
            )],
        }
    }

    fn drawn(text: &str, width: usize) -> Drawn {
        draw(&Page::parse("cd", Subject::Command, text), &manual(), width)
    }

    #[test]
    fn a_page_is_its_name_its_sections_and_its_links() {
        let page = drawn(
            "---\nsummary: Change the directory.\nkind: k\nsee: pwd paths nowhere\n---\n\
             ## Syntax\n\n    cd DIRECTORY    go to DIRECTORY\n\n## Examples\n\nGo `home`:\n\n    cd\n",
            40,
        );
        assert_eq!(
            page.text(false),
            "cd \u{2014} Change the directory.\n\nSYNTAX\n        cd DIRECTORY    go to DIRECTORY\n\n\
             EXAMPLES\n    Go home:\n\n        cd\n\nSEE ALSO\n    pwd, paths, nowhere\n"
        );
        let titles: Vec<&str> = page.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["cd", "SYNTAX", "EXAMPLES", "SEE ALSO"]);
        assert_eq!(page.sections[1].line, 2);
        assert_eq!(page.lines[3].section, 1);
        let links: Vec<(&str, Style)> = page
            .lines
            .last()
            .unwrap()
            .spans
            .iter()
            .filter_map(|span| span.link.as_deref().map(|link| (link, span.style)))
            .collect();
        assert_eq!(
            links,
            [
                ("pwd", Style::Command),
                ("paths", Style::Topic),
                ("nowhere", Style::Command)
            ]
        );
    }

    #[test]
    fn prose_wraps_to_the_width_and_items_hang() {
        let page = drawn(
            "---\nsummary: s\nkind: k\n---\n## How it works\n\n\
             one two three four five six seven eight\n\n- alpha beta gamma delta epsilon\n  zeta\n\
             - next\n\n10. ordered item that wraps here\n",
            20,
        );
        assert_eq!(
            page.text(false),
            "cd \u{2014} s\n\nHOW IT WORKS\n    one two three\n    four five six\n    seven eight\n\n\
             \x20   - alpha beta\n      gamma delta\n      epsilon zeta\n    - next\n\n\
             \x20   10. ordered item\n        that wraps\n        here\n"
        );
    }

    #[test]
    fn code_is_what_to_type_its_description_plain_and_its_comment_grey() {
        assert_eq!(
            code_pieces("cd -P DIR    go there"),
            [
                ("cd -P DIR", Style::Code),
                ("    ", Style::Plain),
                ("go there", Style::Plain)
            ]
        );
        assert_eq!(
            code_pieces("cd -      # back"),
            [
                ("cd -", Style::Code),
                ("      ", Style::Plain),
                ("# back", Style::Comment)
            ]
        );
        assert_eq!(
            code_pieces("printf 'a  b' x"),
            [("printf 'a  b' x", Style::Code)]
        );
        assert_eq!(
            code_pieces("    echo hi"),
            [("    ", Style::Plain), ("echo hi", Style::Code)]
        );
        assert_eq!(code_pieces("# tidy.sed"), [("# tidy.sed", Style::Comment)]);
        assert_eq!(code_pieces("   "), []);
    }

    #[test]
    fn a_table_of_options_has_prose_descriptions_and_examples_stay_code() {
        let lines: Vec<String> = [
            "-L     Logical, `$PWD` as written,",
            "       and `..` drops a part.",
            "if true; then",
            "    echo `date`",
            "fi",
        ]
        .map(str::to_owned)
        .to_vec();
        let owned = |pieces: &[(&str, Style)]| -> Vec<(String, Style)> {
            pieces
                .iter()
                .map(|(text, style)| ((*text).to_owned(), *style))
                .collect()
        };
        assert_eq!(
            code_lines(&lines),
            [
                owned(&[
                    ("-L", Style::Code),
                    ("     ", Style::Plain),
                    ("Logical, ", Style::Plain),
                    ("$PWD", Style::Code),
                    (" as written,", Style::Plain),
                ]),
                owned(&[
                    ("       ", Style::Plain),
                    ("and ", Style::Plain),
                    ("..", Style::Code),
                    (" drops a part.", Style::Plain),
                ]),
                owned(&[("if true; then", Style::Code)]),
                owned(&[("    ", Style::Plain), ("echo `date`", Style::Code)]),
                owned(&[("fi", Style::Code)]),
            ]
        );
    }

    #[test]
    fn inline_code_and_bold_are_styled_and_a_comma_stays_with_its_word() {
        let page = drawn(
            "---\nsummary: s\nkind: k\n---\n## Advanced\n\nRun `cd -`, **then** go.\n",
            12,
        );
        let line = &page.lines[3];
        assert_eq!(line.plain(), "    Run");
        let next = &page.lines[4];
        assert_eq!(next.plain(), "    cd -,");
        assert_eq!(next.spans[1].style, Style::Code);
        assert_eq!(page.lines[5].ansi(true), "    \x1b[1mthen\x1b[0m go.");
    }

    #[test]
    fn a_table_is_aligned_and_a_draft_says_so() {
        let page = drawn(
            "---\nsummary: s\nkind: k\ndraft: yes\n---\n## Options\n\n| a | bb |\n|---|---|\n| `ccc` | d |\n",
            40,
        );
        let text = page.text(false);
        assert!(
            text.contains("A draft, made from the command's --help"),
            "{text}"
        );
        assert!(
            text.ends_with("OPTIONS\n    a    bb\n    ccc  d\n"),
            "{text}"
        );
        assert_eq!(page.lines[2].spans[0].style, Style::Mark);
    }

    #[test]
    fn every_page_reads_in_eighty_columns() {
        let manual = crate::manual();
        let wide: Vec<String> = manual
            .pages
            .iter()
            .chain(&manual.topics)
            .flat_map(|page| {
                draw(page, manual, 80)
                    .lines
                    .into_iter()
                    .filter(|line| line.width() > 80)
                    .map(|line| format!("{}: {}", page.file, line.plain()))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(wide, Vec::<String>::new(), "{wide:#?}");
    }
}
