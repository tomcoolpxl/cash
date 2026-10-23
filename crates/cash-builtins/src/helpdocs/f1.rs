//! The F1 help: one screen for Windows users who know Bash, drawn on the alternate
//! screen as `less` draws, and closed by F1, Esc or `q`.
//!
//! The text is `f1.md`, beside the help topics, so that it reviews as text and the test
//! that keeps developer notes out of help covers it. A `## NAME` line starts a section; a
//! line with two columns separated by two or more spaces is an example, or a key, and what
//! it does; a line without them is prose, whose code spans are commands. One item to a
//! line, and the second columns lined up across the sections. `{tools}` stands for the
//! number of commands the running cash answers itself.
//!
//! The title line stays; the rest scrolls when the window is shorter than the text.

use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{cursor, queue, terminal};
use nu_ansi_term::Style;
use unicode_width::UnicodeWidthChar;

use super::render::visible_width;
use crate::top::{FullScreen, terminal_size};

/// The screen's text.
const TEXT: &str = include_str!("f1.md");

/// What stands for the tool count in the text.
const TOOLS: &str = "{tools}";

/// What the title's right end says.
const CLOSES: &str = "F1 or Esc closes";

/// The screen is laid out for this many columns, and narrower when the window is.
pub const WIDTH: usize = 80;

/// Below this many columns a line is cut rather than wrapped: too narrow to read anyway.
const NARROWEST: usize = 20;

/// How far a note wrapped under its example or key is indented beyond it.
const NOTE_INDENT: usize = 4;

/// The styles the screen is drawn in; [`Palette::none`] draws plain text.
#[derive(Clone, Copy, Debug, Default)]
pub struct Palette {
    /// The title and the section names: the prompt's accent.
    pub accent: Style,
    /// A path example, and a command in prose.
    pub example: Style,
    /// A key's name.
    pub key: Style,
    /// What an example or a key does.
    pub note: Style,
}

impl Palette {
    /// No styles at all: plain text, for `NO_COLOR` and `--disable-color`.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }
}

/// The screen's lines for a window `width` columns wide, the title first: `version` is
/// the running cash's, `tools` the number of commands it answers itself.
#[must_use]
pub fn render(version: &str, tools: usize, width: usize, palette: &Palette) -> Vec<String> {
    let width = width.clamp(8, WIDTH);
    let tools = tools_phrase(tools);
    let mut lines = vec![title(version, width, palette), rule(width, palette)];
    let mut section = String::new();
    // One column is the margin, and a line ends before the last column, as the title does.
    let room = width - 2;
    let text: Vec<String> = TEXT
        .lines()
        .map(|line| line.replace(TOOLS, &tools).trim_end().to_owned())
        .collect();
    // When one example or key does not fit beside its note, every one goes above its
    // note, so that the screen keeps one shape.
    let stacked = text.iter().any(|line| {
        let indent_end = line.find(|c| c != ' ').unwrap_or(line.len());
        let two_columns = columns(line.get(indent_end..).unwrap_or_default()).len() == 2;
        two_columns && drawn_width(line) > room
    });
    for line in &text {
        let line = line.as_str();
        let two_columns = {
            let indent_end = line.find(|c| c != ' ').unwrap_or(line.len());
            columns(line.get(indent_end..).unwrap_or_default()).len() == 2
        };
        if let Some(heading) = line.strip_prefix("## ") {
            heading.trim().clone_into(&mut section);
            lines.push(format!(" {}", palette.accent.paint(section.as_str())));
        } else if line.is_empty() {
            lines.push(String::new());
        } else if drawn_width(line) <= room && !(stacked && two_columns && width >= NARROWEST) {
            lines.push(body_line(line, &section, palette));
        } else if width < NARROWEST {
            lines.push(body_line(fit(line, room).trim_end(), &section, palette));
        } else {
            lines.extend(wrapped(line, &section, room, palette));
        }
    }
    lines
}

/// The columns `text` takes once drawn: its backticks take none.
fn drawn_width(text: &str) -> usize {
    text.chars()
        .filter(|&c| c != '`')
        .map(|c| c.width().unwrap_or(0))
        .sum()
}

/// A line too wide for `room` columns, over several: an example or key on its own line
/// and its note under it, indented further; prose broken between words.
fn wrapped(line: &str, section: &str, room: usize, palette: &Palette) -> Vec<String> {
    let indent_end = line.find(|c| c != ' ').unwrap_or(line.len());
    let (indent, text) = line.split_at(indent_end);
    let pieces = columns(text);
    let mut out = Vec::new();
    if let [(item, _), (note, _)] = pieces.as_slice() {
        let item_line = format!("{indent}{item}");
        out.push(body_line(
            fit(&item_line, room).trim_end(),
            section,
            palette,
        ));
        let note_indent = " ".repeat(indent.len() + NOTE_INDENT);
        for part in words_within(note, room.saturating_sub(note_indent.len())) {
            // Two spaces before the note would make it a column of its own.
            out.push(format!(
                " {note_indent}{}",
                spans(&part, palette.note, palette.example)
            ));
        }
    } else {
        for part in words_within(text, room.saturating_sub(indent.len())) {
            out.push(body_line(&format!("{indent}{part}"), section, palette));
        }
    }
    out
}

/// `text` broken between words into lines of at most `room` columns; a word wider than
/// that is cut.
fn words_within(text: &str, room: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        let wanted = if current.is_empty() {
            drawn_width(word)
        } else {
            drawn_width(&current) + 1 + drawn_width(word)
        };
        if wanted > room && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
        if drawn_width(&current) > room {
            lines.push(fit(&current, room).to_owned());
            current.clear();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// `count` commands, as the text says it: rounded down to tens, or `more` when there
/// are too few to be worth a number.
fn tools_phrase(count: usize) -> String {
    let tens = count / 10 * 10;
    if tens >= 100 {
        format!("{tens} tools")
    } else {
        "more".to_owned()
    }
}

/// `cash VERSION  ·  help`, with [`CLOSES`] ending one column before the right edge when
/// there is room for it.
fn title(version: &str, width: usize, palette: &Palette) -> String {
    let whole = format!("cash {version}  ·  help");
    // In a window too narrow even for the title, its start.
    let left = fit(&whole, width.saturating_sub(2)).trim_end().to_owned();
    let mut line = format!(" {}", palette.accent.paint(left.as_str()));
    let room = width
        .saturating_sub(1)
        .saturating_sub(visible_width(&left) + 1);
    if room >= CLOSES.len() + 2 {
        line.push_str(&" ".repeat(room - CLOSES.len()));
        line.push_str(&palette.note.paint(CLOSES).to_string());
    }
    line
}

fn rule(width: usize, palette: &Palette) -> String {
    format!(" {}", palette.note.paint("─".repeat(width - 2)))
}

/// A line of `section`: its columns in turn as the example or key and the note, with the
/// spacing as written; a line without columns as prose.
fn body_line(line: &str, section: &str, palette: &Palette) -> String {
    let indent_end = line.find(|c| c != ' ').unwrap_or(line.len());
    let (indent, text) = line.split_at(indent_end);
    let pieces = columns(text);
    let prose = pieces.len() == 1;
    let code = if section == "KEYS" {
        palette.key
    } else {
        palette.example
    };
    let mut out = format!(" {indent}");
    for (i, (piece, gap)) in pieces.iter().enumerate() {
        let base = if prose || i % 2 == 1 {
            palette.note
        } else {
            code
        };
        out.push_str(&spans(piece, base, palette.example));
        out.push_str(gap);
    }
    out
}

/// `text` at each run of two or more spaces: each piece with the run after it.
fn columns(text: &str) -> Vec<(String, String)> {
    let mut pieces: Vec<(String, String)> = Vec::new();
    let mut piece = String::new();
    let mut gap = String::new();
    for c in text.chars() {
        if c == ' ' {
            gap.push(c);
            continue;
        }
        if gap.len() >= 2 && !piece.is_empty() {
            pieces.push((std::mem::take(&mut piece), std::mem::take(&mut gap)));
        } else {
            piece.push_str(&gap);
            gap.clear();
        }
        piece.push(c);
    }
    pieces.push((piece, gap));
    pieces
}

/// `text` in `base`, its code spans in `code`.
fn spans(text: &str, base: Style, code: Style) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, part) in text.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        let style = if i % 2 == 1 { code } else { base };
        out.push_str(&style.paint(part).to_string());
    }
    out
}

/// The start of `text` that fits in `width` columns once drawn: the backticks of its
/// code spans take none.
fn fit(text: &str, width: usize) -> &str {
    let mut used = 0;
    for (at, c) in text.char_indices() {
        if c != '`' {
            used += c.width().unwrap_or(0);
        }
        if used > width {
            return text.get(..at).unwrap_or_default();
        }
    }
    text
}

/// What is on the screen: the lines, and how far the body has scrolled.
struct View {
    lines: Vec<String>,
    offset: usize,
}

/// What a key does to the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Close,
    Scroll(Scroll),
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scroll {
    Lines(isize),
    Pages(isize),
    Home,
    End,
}

impl View {
    const fn new(lines: Vec<String>) -> Self {
        Self { lines, offset: 0 }
    }

    /// How far the body can scroll in a window of `rows`: the title keeps a row.
    const fn max_offset(&self, rows: usize) -> usize {
        self.lines
            .len()
            .saturating_sub(1)
            .saturating_sub(rows.saturating_sub(1))
    }

    fn scroll(&mut self, by: Scroll, rows: usize) {
        let page = isize::try_from(rows.saturating_sub(1)).unwrap_or(isize::MAX);
        self.offset = match by {
            Scroll::Lines(n) => self.offset.saturating_add_signed(n),
            Scroll::Pages(n) => self.offset.saturating_add_signed(n.saturating_mul(page)),
            Scroll::Home => 0,
            Scroll::End => usize::MAX,
        }
        .min(self.max_offset(rows));
    }

    /// The title, and the body from the offset, as many rows as the window has.
    fn visible(&self, rows: usize) -> Vec<&str> {
        self.lines
            .iter()
            .take(1)
            .chain(
                self.lines
                    .iter()
                    .skip(1 + self.offset)
                    .take(rows.saturating_sub(1)),
            )
            .map(String::as_str)
            .collect()
    }

    /// Draws every row, as one synchronized update: a row written over the old one and
    /// erased after its text, never cleared first, so that nothing flickers.
    fn draw<W: Write>(&self, out: &mut W, rows: usize) -> io::Result<()> {
        write!(out, "\x1b[?2026h")?;
        let visible = self.visible(rows);
        for (i, line) in visible.iter().enumerate() {
            queue!(out, cursor::MoveTo(0, row(i)))?;
            write!(out, "{line}")?;
            queue!(out, terminal::Clear(terminal::ClearType::UntilNewLine))?;
        }
        if visible.len() < rows {
            queue!(
                out,
                cursor::MoveTo(0, row(visible.len())),
                terminal::Clear(terminal::ClearType::FromCursorDown)
            )?;
        }
        write!(out, "\x1b[?2026l")?;
        out.flush()
    }
}

fn row(index: usize) -> u16 {
    u16::try_from(index).unwrap_or(u16::MAX)
}

/// What `key` does: F1, Esc, `q` and Ctrl-C close; the arrows, Page Up and Down, Home
/// and End scroll.
const fn action(key: KeyEvent) -> Action {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::F(1) | KeyCode::Esc | KeyCode::Char('q' | 'Q') => Action::Close,
        KeyCode::Char('c' | 'g') if control => Action::Close,
        KeyCode::Up => Action::Scroll(Scroll::Lines(-1)),
        KeyCode::Down => Action::Scroll(Scroll::Lines(1)),
        KeyCode::PageUp => Action::Scroll(Scroll::Pages(-1)),
        KeyCode::PageDown | KeyCode::Char(' ') => Action::Scroll(Scroll::Pages(1)),
        KeyCode::Home => Action::Scroll(Scroll::Home),
        KeyCode::End => Action::Scroll(Scroll::End),
        _ => Action::None,
    }
}

/// Shows the screen on standard output until F1, Esc, `q` or Ctrl-C closes it, and puts
/// the main screen back as it was. `version` and `tools` are as [`render`] takes them.
///
/// # Errors
///
/// When the terminal cannot be drawn on or read from.
pub fn show(version: &str, tools: usize, palette: &Palette) -> io::Result<()> {
    // The line editor hands the terminal over cooked; the picker is called the same way.
    let was_raw = terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw {
        terminal::enable_raw_mode()?;
    }
    let result = run(version, tools, palette);
    if !was_raw {
        let _ = terminal::disable_raw_mode();
    }
    result
}

fn run(version: &str, tools: usize, palette: &Palette) -> io::Result<()> {
    let stdout = io::stdout();
    let mut screen = FullScreen::enter(stdout.lock())?;
    let (columns, mut rows) = terminal_size();
    let mut view = View::new(render(version, tools, columns, palette));
    loop {
        view.draw(screen.out(), rows)?;
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match action(key) {
                Action::Close => return Ok(()),
                Action::Scroll(by) => view.scroll(by, rows),
                Action::None => {}
            },
            Event::Resize(new_columns, new_rows) => {
                rows = usize::from(new_rows).max(1);
                view.lines = render(version, tools, usize::from(new_columns).max(1), palette);
                view.offset = view.offset.min(view.max_offset(rows));
                queue!(screen.out(), terminal::Clear(terminal::ClearType::All))?;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use nu_ansi_term::Color;

    use super::*;

    fn painted() -> Palette {
        Palette {
            accent: Style::new().fg(Color::Red),
            example: Style::new().fg(Color::Green),
            key: Style::new().fg(Color::Yellow),
            note: Style::new().fg(Color::Blue),
        }
    }

    fn plain() -> Vec<String> {
        render("1.2.3", 208, WIDTH, &Palette::none())
    }

    #[test]
    fn every_line_is_under_eighty_columns_and_the_title_has_the_version() {
        let lines = plain();
        assert!(lines[0].starts_with(" cash 1.2.3  ·  help"), "{}", lines[0]);
        assert!(lines[0].ends_with(CLOSES), "{}", lines[0]);
        assert_eq!(visible_width(&lines[0]), WIDTH - 1);
        assert_eq!(visible_width(&lines[1]), WIDTH - 1);
        for line in &lines {
            assert!(visible_width(line) < WIDTH, "{line}");
            assert!(!line.contains('\x1b'), "{line:?}");
        }
        let text = lines.join("\n");
        for wanted in [
            "\n PATHS\n   cash prints paths as C:/Users/me, and every program understands them.",
            "\n   cd C:/Users/me/src             forward slashes need no quotes",
            "\n   cd /c/Users/me                 the /c/ form works too\n",
            "\n\n KEYS\n   Alt-E                          pick a file or directory",
            "\n   Ctrl-X Ctrl-E                  edit the line in $EDITOR\n",
            "\n\n MORE\n   Bash scripts run as they are; ls, awk, sed, find and 200 tools are built in.",
            "\n   help NAME                      help for one command\n",
            "\n   sudo CMD                       run a command elevated",
        ] {
            assert!(text.contains(wanted), "no {wanted:?} in:\n{text}");
        }
    }

    /// One item to a line, and every note in one column: no line carries two examples,
    /// two keys or a list of commands.
    #[test]
    fn each_line_holds_one_item_and_the_notes_line_up() {
        let lines = plain();
        let mut columns = Vec::new();
        for line in &lines[2..] {
            let pieces: Vec<&str> = line
                .trim()
                .split("  ")
                .map(str::trim)
                .filter(|piece| !piece.is_empty())
                .collect();
            assert!(pieces.len() <= 2, "more than one item: {line}");
            assert!(!line.contains('·'), "{line}");
            if let [_, note] = pieces[..] {
                let at = line.rfind(note).unwrap();
                columns.push(visible_width(line.get(..at).unwrap()));
            }
        }
        assert!(columns.len() > 15, "{lines:#?}");
        assert!(
            columns.iter().all(|&at| at == columns[0]),
            "{columns:?}\n{lines:#?}"
        );
    }

    #[test]
    fn sections_examples_keys_and_notes_take_their_styles() {
        let palette = painted();
        let paint = |style: Style, text: &str| style.paint(text).to_string();
        let lines = render("1.2.3", 208, WIDTH, &palette);
        let text = lines.join("\n");
        assert!(lines[0].starts_with(&format!(
            " {}",
            paint(palette.accent, "cash 1.2.3  ·  help")
        )));
        assert!(lines[0].ends_with(&paint(palette.note, CLOSES)));
        for wanted in [
            format!(" {}", paint(palette.accent, "PATHS")),
            paint(
                palette.note,
                "cash prints paths as C:/Users/me, and every program understands them.",
            ),
            paint(palette.example, "cd C:/Users/me/src"),
            paint(palette.note, "forward slashes need no quotes"),
            paint(palette.example, "cd /c/Users/me"),
            paint(palette.example, r"winpath -w ~/src"),
            paint(palette.accent, "KEYS"),
            paint(palette.key, "Alt-E"),
            paint(palette.key, "Ctrl-X Ctrl-E"),
            paint(palette.note, "edit the line in $EDITOR"),
            paint(palette.accent, "MORE"),
            paint(palette.note, "Bash scripts run as they are; "),
            paint(palette.example, "ls"),
            paint(palette.example, "help NAME"),
            paint(palette.note, "run a command elevated"),
        ] {
            assert!(text.contains(&wanted), "no {wanted:?} in:\n{text}");
        }
        // A key is never in the example's style, nor an example in a key's.
        assert!(!text.contains(&paint(palette.example, "Alt-E")));
        assert!(!text.contains(&paint(palette.key, "cd C:/Users/me/src")));
    }

    #[test]
    fn the_tool_count_rounds_down_to_tens_or_says_more() {
        assert_eq!(tools_phrase(208), "200 tools");
        assert_eq!(tools_phrase(199), "190 tools");
        assert_eq!(tools_phrase(100), "100 tools");
        assert_eq!(tools_phrase(42), "more");
        let lines = render("1.2.3", 42, WIDTH, &Palette::none());
        assert!(lines.join("\n").contains("find and more are built in."));
    }

    /// Every word of the text, in order, whatever the width.
    fn words(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .skip(2)
            .flat_map(|line| {
                line.split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn a_narrow_window_wraps_and_loses_no_word() {
        let full = words(&plain());
        for width in [60, 44, 32, 24] {
            let lines = render("1.2.3", 208, width, &Palette::none());
            for line in &lines {
                assert!(visible_width(line) < width, "{width}: {line}");
            }
            // The one exception at 24 columns: an example wider than the window is cut.
            if width >= 32 {
                assert_eq!(words(&lines), full, "{width} columns lose a word");
            }
        }
        // At forty columns the closing hint still fits; at thirty it goes.
        let lines = render("1.2.3", 208, 40, &Palette::none());
        assert!(lines[0].ends_with(CLOSES), "{}", lines[0]);
        assert_eq!(visible_width(&lines[0]), 39);
        let lines = render("1.2.3", 208, 30, &Palette::none());
        assert!(!lines[0].contains(CLOSES), "{}", lines[0]);
    }

    #[test]
    fn when_one_note_does_not_fit_every_note_goes_under_its_example() {
        let lines = render("1.2.3", 208, 60, &Palette::none());
        let text = lines.join("\n");
        assert!(
            text.contains("\n   cd /c/Users/me\n       the /c/ form works too\n"),
            "{text}"
        );
        assert!(
            text.contains("\n   man\n       the manual browser: every command and topic\n"),
            "{text}"
        );
        // Prose wraps between words, at the example's indent.
        assert!(
            text.contains(
                "\n   cash prints paths as C:/Users/me, and every program\n   understands them.\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn below_twenty_columns_lines_are_cut() {
        let lines = render("1.2.3", 208, 16, &Palette::none());
        for line in &lines {
            assert!(visible_width(line) < 16, "{line}");
        }
        assert!(lines.iter().any(|line| line.starts_with("   cd C:/Users/")));
    }

    #[test]
    fn man_comes_before_help() {
        let text = plain().join("\n");
        let man = text.find("   man         ").unwrap();
        let man_name = text.find("   man NAME").unwrap();
        let help = text.find("   help        ").unwrap();
        assert!(man < man_name && man_name < help, "{text}");
        assert!(text.contains("the manual browser: every command and topic"));
    }

    #[test]
    fn columns_split_at_two_spaces() {
        assert_eq!(
            columns("cd a  what it does"),
            [
                ("cd a".to_owned(), "  ".to_owned()),
                ("what it does".to_owned(), String::new())
            ]
        );
        assert_eq!(
            columns("Ctrl-X Ctrl-E    edit"),
            [
                ("Ctrl-X Ctrl-E".to_owned(), "    ".to_owned()),
                ("edit".to_owned(), String::new())
            ]
        );
        assert_eq!(
            columns("one line"),
            [("one line".to_owned(), String::new())]
        );
    }

    #[test]
    fn the_body_scrolls_within_the_text_and_the_title_stays() {
        let mut view = View::new((0..21).map(|i| format!("line {i}")).collect());
        assert_eq!(view.max_offset(15), 6);
        assert_eq!(view.max_offset(40), 0);
        let visible = view.visible(15);
        assert_eq!(visible.len(), 15);
        assert_eq!(
            (visible[0], visible[1], visible[14]),
            ("line 0", "line 1", "line 14")
        );

        view.scroll(Scroll::Lines(1), 15);
        assert_eq!(view.offset, 1);
        assert_eq!(view.visible(15)[1], "line 2");
        view.scroll(Scroll::End, 15);
        assert_eq!(view.offset, 6);
        view.scroll(Scroll::Lines(1), 15);
        assert_eq!(view.offset, 6);
        assert_eq!(view.visible(15)[0], "line 0", "the title stays");
        view.scroll(Scroll::Home, 15);
        assert_eq!(view.offset, 0);
        view.scroll(Scroll::Lines(-1), 15);
        assert_eq!(view.offset, 0);
        view.scroll(Scroll::Pages(1), 15);
        assert_eq!(view.offset, 6);
        view.scroll(Scroll::Pages(-1), 15);
        assert_eq!(view.offset, 0);
    }

    #[test]
    fn a_frame_is_one_synchronized_update_that_never_clears_a_row_first() {
        let view = View::new(vec!["title".to_owned(), "body".to_owned()]);
        let mut out = Vec::new();
        view.draw(&mut out, 5).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(
            out.starts_with("\x1b[?2026h") && out.ends_with("\x1b[?2026l"),
            "{out:?}"
        );
        assert!(out.contains("title") && out.contains("body"), "{out:?}");
        assert!(!out.contains("\x1b[2K"), "{out:?}");
        // The rows below the text are erased.
        assert!(out.contains("\x1b[3;1H\x1b[J"), "{out:?}");
    }

    #[test]
    fn keys_close_or_scroll() {
        let key = |code, modifiers| action(KeyEvent::new(code, modifiers));
        assert_eq!(key(KeyCode::F(1), KeyModifiers::NONE), Action::Close);
        assert_eq!(key(KeyCode::Esc, KeyModifiers::NONE), Action::Close);
        assert_eq!(key(KeyCode::Char('q'), KeyModifiers::NONE), Action::Close);
        assert_eq!(
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Action::Close
        );
        assert_eq!(
            key(KeyCode::Down, KeyModifiers::NONE),
            Action::Scroll(Scroll::Lines(1))
        );
        assert_eq!(
            key(KeyCode::PageUp, KeyModifiers::NONE),
            Action::Scroll(Scroll::Pages(-1))
        );
        assert_eq!(
            key(KeyCode::End, KeyModifiers::NONE),
            Action::Scroll(Scroll::End)
        );
        assert_eq!(key(KeyCode::Char('x'), KeyModifiers::NONE), Action::None);
        assert_eq!(key(KeyCode::F(2), KeyModifiers::NONE), Action::None);
    }
}
