//! The catalogue's markdown, rendered as a man page renders: plain text wrapped to the
//! terminal, headings in capitals, and bold in place of code spans when colour is on.
//!
//! Only the markdown the pages use is understood: `##` and `###` headings, paragraphs,
//! `- ` list items, fenced code blocks and code spans. Anything else is a paragraph.

use std::fmt::Write as _;

use unicode_width::UnicodeWidthStr;

/// How a page is drawn: the width to wrap at, and whether to use colour.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// The column to wrap prose at.
    pub width: usize,
    /// Bold headings and code, with ANSI escapes.
    pub colour: bool,
}

/// Prose is indented by this much under a heading, as a man page indents it.
pub const INDENT: usize = 4;

const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// A section heading: its text in capitals, bold when colour is on.
pub fn heading(text: &str, style: Style) -> String {
    let text = plain(text).to_uppercase();
    if style.colour {
        format!("{BOLD}{text}{RESET}\n")
    } else {
        format!("{text}\n")
    }
}

/// `text` with its code spans drawn: bold when colour is on, else just their contents.
pub fn inline(text: &str, colour: bool) -> String {
    if !colour {
        return plain(text);
    }
    let mut out = String::with_capacity(text.len());
    for (index, part) in text.split('`').enumerate() {
        if index % 2 == 1 {
            let _ = write!(out, "{BOLD}{part}{RESET}");
        } else {
            out.push_str(part);
        }
    }
    out
}

/// `text` with the backticks of its code spans removed.
pub fn plain(text: &str) -> String {
    text.replace('`', "")
}

/// The columns `text` takes on a terminal, its escape sequences not counted.
pub fn visible_width(text: &str) -> usize {
    let mut width = 0;
    let mut rest = text;
    while let Some(at) = rest.find('\x1b') {
        width += rest.get(..at).map_or(0, UnicodeWidthStr::width);
        let after = rest.get(at..).unwrap_or_default();
        // An SGR sequence, `ESC [ ... m`, is all cash writes.
        rest = after
            .find('m')
            .map_or("", |end| after.get(end + 1..).unwrap_or_default());
    }
    width + rest.width()
}

/// `text` wrapped to `width`, its first line starting with `first` and the others
/// indented by `indent` spaces. Every line ends with a newline.
pub fn wrap(text: &str, width: usize, first: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::new();
    let mut line = first.to_owned();
    let mut line_width = visible_width(first);
    let mut empty = true;
    for word in text.split_whitespace() {
        let word_width = visible_width(word);
        if !empty && line_width + 1 + word_width > width {
            out.push_str(line.trim_end());
            out.push('\n');
            line.clone_from(&pad);
            line_width = indent;
            empty = true;
        }
        if !empty {
            line.push(' ');
            line_width += 1;
        }
        line.push_str(word);
        line_width += word_width;
        empty = false;
    }
    out.push_str(line.trim_end());
    out.push('\n');
    out
}

/// Text indented by `indent` spaces, line by line, as it is: for code and for help text
/// a builtin wrote itself.
pub fn indented(text: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut out = String::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            out.push('\n');
        } else {
            let _ = writeln!(out, "{pad}{}", line.trim_end());
        }
    }
    out
}

/// A block of a page: what [`markdown`] draws, one at a time.
enum Block {
    Heading(String),
    Subheading(String),
    Paragraph(String),
    Item(String),
    Code(Vec<String>),
}

/// The blocks of `body`, in order.
fn blocks(body: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut lines = body.lines();
    let mut open: Option<Block> = None;
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            blocks.extend(open.take());
            let mut code = Vec::new();
            for line in lines.by_ref() {
                if line.trim().starts_with("```") {
                    break;
                }
                code.push(line.to_owned());
            }
            blocks.push(Block::Code(code));
        } else if trimmed.is_empty() {
            blocks.extend(open.take());
        } else if let Some(text) = trimmed.strip_prefix("### ") {
            blocks.extend(open.take());
            blocks.push(Block::Subheading(text.to_owned()));
        } else if let Some(text) = trimmed.strip_prefix("## ") {
            blocks.extend(open.take());
            blocks.push(Block::Heading(text.to_owned()));
        } else if let Some(text) = trimmed.strip_prefix("- ") {
            blocks.extend(open.take());
            open = Some(Block::Item(text.to_owned()));
        } else {
            match &mut open {
                Some(Block::Paragraph(text) | Block::Item(text)) => {
                    text.push(' ');
                    text.push_str(trimmed);
                }
                _ => open = Some(Block::Paragraph(trimmed.to_owned())),
            }
        }
    }
    blocks.extend(open);
    blocks
}

/// `body`, a page's markdown, drawn as a man page's sections are.
pub fn markdown(body: &str, style: Style) -> String {
    let mut out = String::new();
    let mut previous_was_item = false;
    let mut previous_was_heading = false;
    for block in blocks(body) {
        let is_item = matches!(block, Block::Item(_));
        // A blank line between blocks, but list items sit together, and a section starts
        // right under its heading.
        let together = previous_was_heading || (is_item && previous_was_item);
        if !out.is_empty() && !together {
            out.push('\n');
        }
        previous_was_heading = matches!(block, Block::Heading(_));
        match block {
            Block::Heading(text) => out.push_str(&heading(&text, style)),
            Block::Subheading(text) => {
                let text = inline(&text, style.colour);
                let first = " ".repeat(INDENT);
                if style.colour {
                    let _ = writeln!(out, "{first}{BOLD}{}{RESET}", plain(&text));
                } else {
                    let _ = writeln!(out, "{first}{text}");
                }
            }
            Block::Paragraph(text) => {
                let first = " ".repeat(INDENT);
                out.push_str(&wrap(
                    &inline(&text, style.colour),
                    style.width,
                    &first,
                    INDENT,
                ));
            }
            Block::Item(text) => {
                let first = format!("{}- ", " ".repeat(INDENT));
                out.push_str(&wrap(
                    &inline(&text, style.colour),
                    style.width,
                    &first,
                    INDENT + 2,
                ));
            }
            Block::Code(lines) => out.push_str(&indented(&lines.join("\n"), INDENT + 4)),
        }
        previous_was_item = is_item;
    }
    out
}

/// The pieces of `body` a search matches and shows, as plain text.
///
/// Each heading, each sentence of the prose, and each line of code: a sentence rather
/// than a source line, because the source is wrapped wherever its writer's editor
/// wrapped it.
pub fn search_texts(body: &str) -> Vec<String> {
    let mut texts = Vec::new();
    for block in blocks(body) {
        match block {
            Block::Heading(text) | Block::Subheading(text) => texts.push(plain(&text)),
            Block::Paragraph(text) | Block::Item(text) => {
                let text = plain(&text);
                let mut rest = text.as_str();
                while let Some(at) = rest.find(". ") {
                    texts.push(rest.get(..=at).unwrap_or_default().to_owned());
                    rest = rest.get(at + 2..).unwrap_or_default();
                }
                if !rest.trim().is_empty() {
                    texts.push(rest.to_owned());
                }
            }
            Block::Code(lines) => texts.extend(
                lines
                    .into_iter()
                    .filter(|line| !line.trim().is_empty())
                    .map(|line| line.trim().to_owned()),
            ),
        }
    }
    texts
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Style = Style {
        width: 30,
        colour: false,
    };

    #[test]
    fn wrapping_keeps_to_the_width_and_indents_the_rest() {
        let out = wrap("one two three four five six seven", 16, "  - ", 4);
        assert_eq!(
            out,
            "  - one two\n    three four\n    five six\n    seven\n"
        );
    }

    #[test]
    fn escapes_take_no_columns() {
        assert_eq!(visible_width("\x1b[1mbold\x1b[0m"), 4);
    }

    #[test]
    fn a_page_has_capital_headings_items_and_code() {
        let out = markdown(
            "## Windows notes\n\nSome `code` here.\n\n- one\n- two\n  more\n\n```\nx  y\n```\n",
            PLAIN,
        );
        assert_eq!(
            out,
            "WINDOWS NOTES\n    Some code here.\n\n    - one\n    - two more\n\n        x  y\n"
        );
    }

    #[test]
    fn a_search_reads_sentences_not_source_lines() {
        let texts = search_texts("## Notes\n\nOne `a`\nsentence. And\nanother.\n");
        assert_eq!(texts, ["Notes", "One a sentence.", "And another."]);
    }
}
