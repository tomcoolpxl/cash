//! `help`'s own lines, its lists of builtins and topics and its search results, drawn
//! as the manual draws a page: plain text wrapped to the terminal, headings in capitals.
//!
//! When colour is on, in the colours of the F1 screen and of the prompt's highlighting:
//! headings in the prompt's blue accent, what you type (code spans, command names) green
//! as the highlighter paints a command. The pages themselves are drawn by `cash-man`.

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

const RESET: &str = "\x1b[0m";
/// A heading: the prompt's accent, bold blue, as the F1 screen's section names.
pub const HEADING: &str = "\x1b[1;34m";
/// A command's name where it is listed or named: bold green, as the highlighter paints
/// a command at the prompt.
pub const COMMAND: &str = "\x1b[1;32m";
/// What to type, in prose and in examples: green.
pub const CODE: &str = "\x1b[32m";
/// A topic's name: bold cyan, so that it is told from a command's.
pub const TOPIC: &str = "\x1b[1;36m";
/// A mark beside a name, and the note that explains it: yellow.
pub const MARK: &str = "\x1b[33m";
/// What a search found, in the lines that show it: bold yellow.
pub const HIT: &str = "\x1b[1;33m";

/// `text` in `colour` when colour is on, else as it is.
pub fn paint(text: &str, colour: &str, on: bool) -> String {
    if on {
        format!("{colour}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

/// A section heading: its text in capitals, in [`HEADING`] when colour is on.
pub fn heading(text: &str, style: Style) -> String {
    let text = plain(text).to_uppercase();
    format!("{}\n", paint(&text, HEADING, style.colour))
}

/// `text` with its code spans drawn: in [`CODE`] when colour is on, else just their
/// contents.
pub fn inline(text: &str, colour: bool) -> String {
    if !colour {
        return plain(text);
    }
    let mut out = String::with_capacity(text.len());
    for (index, part) in text.split('`').enumerate() {
        if index % 2 == 1 {
            let _ = write!(out, "{CODE}{part}{RESET}");
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
    fn colour_follows_the_prompt_headings_blue_what_to_type_green() {
        let on = Style {
            width: 60,
            colour: true,
        };
        assert_eq!(heading("Name", on), "\x1b[1;34mNAME\x1b[0m\n");
        assert_eq!(
            inline("run `ls -l` here", true),
            "run \x1b[32mls -l\x1b[0m here"
        );
        assert_eq!(heading("Name", PLAIN), "NAME\n");
    }
}
