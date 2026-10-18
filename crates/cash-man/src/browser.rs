//! The browser `man` shows: cash's topics and every command on the left, the page of
//! the one chosen on the right (`research/man-browser-design.md`, section 3).
//!
//! Up and Down choose a topic, and its page shows at once: nothing is opened, and no
//! pane takes the keys from another. Page Up, Page Down, Home and End move in the
//! page, and Ctrl or Shift with Up or Down moves it a line. Typing narrows the list,
//! by name, then summary, then text. Ctrl-F finds in the page as you type. Under 100
//! columns the page fills the screen, and Up and Down still choose.
//!
//! It is a state that keys change and frames are drawn from, with no terminal of its
//! own: the `man` builtin reads the keys and writes the frames, and the tests drive it
//! as text.

use cash_picker::filter::Fuzzy;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::Manual;
use crate::page::Subject;
use crate::render::{self, Drawn, Line, SectionStart, Style};

/// Below this many columns, the page alone: a list beside it would leave the page
/// fewer than about 65 columns.
pub const NARROW: usize = 100;

/// A key the browser acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// The topic before.
    Up,
    /// The topic after.
    Down,
    /// Ctrl or Shift with Up: the page a line up.
    LineUp,
    /// Ctrl or Shift with Down: the page a line down.
    LineDown,
    /// The page a screen up.
    PageUp,
    /// The page a screen down.
    PageDown,
    /// The page's start.
    Home,
    /// The page's end.
    End,
    /// Ctrl+PgUp: the section before.
    SectionUp,
    /// Ctrl+PgDn: the section after.
    SectionDown,
    /// Ctrl-F: find in the page.
    Find,
    /// F3: the next match.
    FindNext,
    /// Shift-F3: the match before.
    FindPrevious,
    /// F1: the keys, or the page again.
    Keys,
    /// Enter: the next match, a word being found kept.
    Enter,
    /// Leave a search, then the filter, then the browser.
    Esc,
    /// Take a letter off what is typed.
    Backspace,
    /// A letter: the filter's, or the search's while one is typed.
    Char(char),
}

/// What a key led to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Draw the next frame.
    Continue,
    /// Leave the browser.
    Quit,
}

/// A page of the manual: a command's or a topic's, by its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageId {
    Command(usize),
    Topic(usize),
}

/// An entry of the list: a topic, or one of a command's names.
#[derive(Debug)]
struct Entry {
    name: String,
    summary: String,
    page: PageId,
}

/// A word being found in the page.
#[derive(Debug)]
struct Find {
    word: String,
    /// Whether letters go to the word, not the filter.
    typing: bool,
    /// Each match in the page: its line and its columns, start and end.
    matches: Vec<(usize, usize, usize)>,
    current: usize,
}

/// The page shown, drawn for a width.
#[derive(Debug)]
struct Open {
    page: PageId,
    name: String,
    drawn: Drawn,
    width: usize,
    scroll: usize,
}

/// The keys, as F1 shows them.
const KEYS: [(&str, &str); 11] = [
    (
        "Up Down",
        "the topic before or after: its page shows at once",
    ),
    ("PgUp PgDn", "a screen up or down in the page"),
    ("Home End", "the start or the end of the page"),
    (
        "Ctrl+Up Ctrl+Down",
        "a line up or down in the page; Shift too",
    ),
    ("Ctrl+PgUp Ctrl+PgDn", "the section before or after"),
    ("letters", "narrow the list: by name, summary, then text"),
    ("Backspace", "take a letter off what is typed"),
    ("Ctrl-F", "find in the page, as you type"),
    ("F3 Shift-F3", "the next or the previous match; Enter too"),
    ("F1", "these keys, or the page again"),
    ("Esc", "leave the search, then the filter, then man"),
];

/// A command's own `--help`, by its name.
type OwnHelp<'m> = Box<dyn Fn(&str) -> Option<String> + 'm>;

/// The browser.
pub struct Browser<'m> {
    manual: &'m Manual,
    entries: Vec<Entry>,
    /// The first command in `entries`, after the topics.
    commands_from: usize,
    filter: String,
    fuzzy: Fuzzy,
    /// The entries the list shows, by index.
    shown: Vec<usize>,
    /// The entry chosen, as a place in `shown`.
    selected: usize,
    list_scroll: usize,
    open: Option<Open>,
    find: Option<Find>,
    keys_shown: bool,
    colour: bool,
    /// The rows the page had at the last frame, for a screen's worth of scrolling.
    rows: usize,
    /// A command's own `--help`, for a draft page.
    own_help: OwnHelp<'m>,
}

impl<'m> Browser<'m> {
    /// A browser on `manual`, on `start` (a command or a topic) or on the topic
    /// "Getting started"; `own_help` gives a command's own `--help`, which a draft page
    /// ends with.
    pub fn new(
        manual: &'m Manual,
        start: Option<&str>,
        colour: bool,
        own_help: impl Fn(&str) -> Option<String> + 'm,
    ) -> Self {
        let mut entries: Vec<Entry> = manual
            .topics
            .iter()
            .enumerate()
            .map(|(index, topic)| Entry {
                name: topic.title.clone(),
                summary: topic.summary.clone(),
                page: PageId::Topic(index),
            })
            .collect();
        let commands_from = entries.len();
        let mut commands: Vec<Entry> = manual
            .pages
            .iter()
            .enumerate()
            .flat_map(|(index, page)| {
                page.names.iter().map(move |name| Entry {
                    name: name.clone(),
                    summary: page.summary_of(name).to_owned(),
                    page: PageId::Command(index),
                })
            })
            .collect();
        commands.sort_by_key(|entry| entry.name.to_lowercase());
        entries.extend(commands);
        let mut browser = Self {
            manual,
            shown: (0..entries.len()).collect(),
            entries,
            commands_from,
            filter: String::new(),
            fuzzy: Fuzzy::plain(),
            selected: 0,
            list_scroll: 0,
            open: None,
            find: None,
            keys_shown: false,
            colour,
            rows: 20,
            own_help: Box::new(own_help),
        };
        let start = browser
            .entry_named(start.unwrap_or("getting-started"))
            .unwrap_or(0);
        browser.select(start);
        browser
    }

    /// The entry for `name`: a command's name as written, else in any case, else a
    /// topic's file name or title.
    fn entry_named(&self, name: &str) -> Option<usize> {
        let page = self.manual.page(name).map(|page| {
            let index = self
                .manual
                .pages
                .iter()
                .position(|other| std::ptr::eq(other, page))
                .unwrap_or_default();
            PageId::Command(index)
        });
        let topic = self
            .manual
            .topics
            .iter()
            .position(|topic| {
                topic.file.eq_ignore_ascii_case(name) || topic.title.eq_ignore_ascii_case(name)
            })
            .map(PageId::Topic);
        let wanted = page.or(topic)?;
        self.entries
            .iter()
            .position(|entry| entry.page == wanted && entry.name.eq_ignore_ascii_case(name))
            .or_else(|| self.entries.iter().position(|entry| entry.page == wanted))
    }

    /// Chooses `place` in the list, and shows its page at once, from its top; a word
    /// being found is found in it.
    fn select(&mut self, place: usize) {
        let Some(&entry) = self.shown.get(place) else {
            return;
        };
        self.selected = place;
        let chosen = &self.entries[entry];
        let (page, name) = (chosen.page, chosen.name.clone());
        let same = self
            .open
            .as_ref()
            .is_some_and(|open| open.page == page && open.name == name);
        if !same {
            let width = self.open.as_ref().map_or(60, |open| open.width);
            self.open = Some(Open {
                drawn: self.draw_page(page, width),
                page,
                name,
                width,
                scroll: 0,
            });
            self.keys_shown = false;
            self.find_matches();
        }
    }

    /// A page drawn for `width`; a draft command's ends with its own `--help`, before
    /// its See also.
    fn draw_page(&self, id: PageId, width: usize) -> Drawn {
        let page = match id {
            PageId::Command(index) => &self.manual.pages[index],
            PageId::Topic(index) => &self.manual.topics[index],
        };
        let mut drawn = render::draw(page, self.manual, width);
        if page.draft && page.subject == Subject::Command {
            let own = page.names.first().and_then(|name| (self.own_help)(name));
            if let Some(own) = own.filter(|own| !own.trim().is_empty()) {
                render::add_own_help(&mut drawn, &own);
            }
        }
        drawn
    }

    /// Acts on `key`.
    pub fn key(&mut self, key: Key) -> Outcome {
        if self.find.as_ref().is_some_and(|find| find.typing) && self.find_key(key) {
            return Outcome::Continue;
        }
        let rows = isize::try_from(self.rows.max(1)).unwrap_or(1);
        match key {
            Key::Up => self.select(self.selected.saturating_sub(1)),
            Key::Down => self.select((self.selected + 1).min(self.shown.len().saturating_sub(1))),
            Key::LineUp => self.scroll_by(-1),
            Key::LineDown => self.scroll_by(1),
            Key::PageUp => self.scroll_by(-rows),
            Key::PageDown => self.scroll_by(rows),
            Key::Home => self.scroll_to(0),
            Key::End => self.scroll_to(usize::MAX),
            Key::SectionUp => self.section_step(false),
            Key::SectionDown => self.section_step(true),
            Key::Find => {
                self.find = Some(Find {
                    word: String::new(),
                    typing: true,
                    matches: Vec::new(),
                    current: 0,
                });
            }
            Key::FindNext | Key::Enter => self.next_match(true),
            Key::FindPrevious => self.next_match(false),
            Key::Keys => self.keys_shown = !self.keys_shown,
            Key::Esc => return self.escape(),
            Key::Backspace => {
                if self.filter.pop().is_some() {
                    self.refilter();
                }
            }
            Key::Char(c) if !c.is_control() => {
                self.filter.push(c);
                self.refilter();
            }
            Key::Char(_) => {}
        }
        Outcome::Continue
    }

    /// Esc: the search goes first, then the filter, then the browser.
    fn escape(&mut self) -> Outcome {
        if self.find.take().is_some() {
            return Outcome::Continue;
        }
        if !self.filter.is_empty() {
            // Back to the whole list, the page shown still chosen in it.
            let entry = self.shown.get(self.selected).copied();
            self.filter.clear();
            self.shown = (0..self.entries.len()).collect();
            self.selected = entry.unwrap_or(0);
            return Outcome::Continue;
        }
        Outcome::Quit
    }

    /// The list after the filter changed, its first entry chosen and shown: the name
    /// typed whole first, then the names that match, best first, then the entries a
    /// summary matches, then those whose page holds the text. `sed` is the command
    /// before the topic whose summary mentions it.
    fn refilter(&mut self) {
        let typed = self.filter.trim().to_owned();
        if typed.is_empty() {
            self.shown = (0..self.entries.len()).collect();
        } else {
            self.fuzzy.set(&typed);
            let exact = self
                .entries
                .iter()
                .position(|entry| entry.name.eq_ignore_ascii_case(&typed));
            let by_name: Vec<usize> = self
                .fuzzy
                .rank(self.entries.iter().map(|entry| entry.name.as_str()))
                .into_iter()
                .map(|found| found.index)
                .collect();
            let summaries: Vec<String> = self
                .entries
                .iter()
                .map(|entry| entry.summary.replace('`', ""))
                .collect();
            let by_summary: Vec<usize> = self
                .fuzzy
                .rank(summaries.iter().map(String::as_str))
                .into_iter()
                .map(|found| found.index)
                .collect();
            let by_text: Vec<usize> = self
                .manual
                .search(&typed)
                .into_iter()
                .filter_map(|hit| self.entry_named(hit.name))
                .collect();
            let mut shown: Vec<usize> = exact.into_iter().collect();
            for index in by_name.into_iter().chain(by_summary).chain(by_text) {
                if !shown.contains(&index) {
                    shown.push(index);
                }
            }
            self.shown = shown;
        }
        self.list_scroll = 0;
        self.select(0);
    }

    fn scroll_by(&mut self, by: isize) {
        if let Some(open) = &self.open {
            let to = open.scroll.saturating_add_signed(by);
            self.scroll_to(to);
        }
    }

    fn scroll_to(&mut self, line: usize) {
        let rows = self.rows.max(1);
        if let Some(open) = &mut self.open {
            let last = open.drawn.lines.len().saturating_sub(rows);
            open.scroll = line.min(last);
        }
    }

    /// The section after or before the one the top line is in.
    fn section_step(&mut self, forward: bool) {
        let Some(open) = &self.open else {
            return;
        };
        let starts: Vec<usize> = open
            .drawn
            .sections
            .iter()
            .map(|section| section.line)
            .collect();
        let now = open.scroll;
        let to = if forward {
            starts.into_iter().find(|start| *start > now)
        } else {
            starts.into_iter().rev().find(|start| *start < now)
        };
        if let Some(to) = to {
            self.scroll_to(to);
        }
    }

    /// A key while a word is typed for the search; whether it took the key. The keys
    /// that move go on to the page and the list.
    fn find_key(&mut self, key: Key) -> bool {
        let Some(find) = &mut self.find else {
            return false;
        };
        match key {
            Key::Char(c) if !c.is_control() => find.word.push(c),
            Key::Backspace => {
                find.word.pop();
            }
            Key::Enter => {
                find.typing = false;
                return true;
            }
            Key::Esc => {
                self.find = None;
                return true;
            }
            _ => return false,
        }
        self.find_matches();
        true
    }

    /// Every place the word is found in the page, case aside; the first at or after
    /// the top line is the current one, and is brought into view.
    fn find_matches(&mut self) {
        let (Some(find), Some(open)) = (&mut self.find, &self.open) else {
            return;
        };
        let needle: Vec<char> = find.word.to_lowercase().chars().collect();
        find.matches.clear();
        find.current = 0;
        if needle.is_empty() {
            return;
        }
        for (number, line) in open.drawn.lines.iter().enumerate() {
            let text: Vec<char> = line.plain().to_lowercase().chars().collect();
            let mut at = 0;
            while at + needle.len() <= text.len() {
                if text[at..at + needle.len()] == needle[..] {
                    let start: usize = text[..at].iter().filter_map(|c| c.width()).sum();
                    let width: usize = needle.iter().filter_map(|c| c.width()).sum();
                    find.matches.push((number, start, start + width));
                    at += needle.len();
                } else {
                    at += 1;
                }
            }
        }
        let top = open.scroll;
        find.current = find
            .matches
            .iter()
            .position(|(line, _, _)| *line >= top)
            .unwrap_or(0);
        self.show_current_match();
    }

    fn next_match(&mut self, forward: bool) {
        let Some(find) = &mut self.find else {
            return;
        };
        let count = find.matches.len();
        if count == 0 {
            return;
        }
        find.current = if forward {
            (find.current + 1) % count
        } else {
            (find.current + count - 1) % count
        };
        self.show_current_match();
    }

    /// Scrolls the current match into view, a few lines below the top.
    fn show_current_match(&mut self) {
        let line = self
            .find
            .as_ref()
            .and_then(|find| find.matches.get(find.current))
            .map(|(line, _, _)| *line);
        let rows = self.rows.max(1);
        if let (Some(line), Some(open)) = (line, &self.open)
            && (line < open.scroll || line >= open.scroll + rows)
        {
            self.scroll_to(line.saturating_sub(3));
        }
    }

    /// The screen, `width` columns by `height` rows, each line exactly `width` wide.
    pub fn frame(&mut self, width: usize, height: usize) -> Vec<String> {
        let width = width.max(20);
        let height = height.max(4);
        if width < NARROW {
            self.narrow_frame(width, height)
        } else {
            self.wide_frame(width, height)
        }
    }

    fn list_width(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| entry.name.width())
            .max()
            .unwrap_or(10)
            .clamp(14, 26)
            + 3
    }

    /// Draws the page again when it is shown at a new width, keeping its place.
    fn fit_page(&mut self, width: usize) {
        let Some(open) = self.open.as_ref().filter(|open| open.width != width) else {
            return;
        };
        let (page, scroll, total) = (open.page, open.scroll, open.drawn.lines.len().max(1));
        let drawn = self.draw_page(page, width);
        let scroll = scroll * drawn.lines.len() / total;
        if let Some(open) = &mut self.open {
            open.drawn = drawn;
            open.width = width;
            open.scroll = scroll;
        }
        self.find_matches();
    }

    fn wide_frame(&mut self, width: usize, height: usize) -> Vec<String> {
        let list_outer = self.list_width() + 2;
        let page_outer = width - list_outer;
        let rows = height - 3;
        self.rows = rows;
        self.fit_page(page_outer - 4);
        self.keep_selection_in_view(rows);
        let list = self.list_lines(list_outer - 2, rows);
        let page = self.page_lines(page_outer - 4, rows);
        let dim = |text: &str| self.paint_plain(text, "\x1b[2m");
        let list_title = if self.filter.is_empty() {
            "Topics & commands".to_owned()
        } else {
            format!("Filter: {}", self.filter)
        };
        let mut out = Vec::with_capacity(height);
        out.push(format!(
            "{}{}",
            dim(&border('┌', '┐', &list_title, list_outer)),
            dim(&border('┌', '┐', &self.page_title(), page_outer))
        ));
        for (left, right) in list.iter().zip(&page) {
            out.push(format!(
                "{}{left}{}{} {right} {}",
                dim("│"),
                dim("│"),
                dim("│"),
                dim("│")
            ));
        }
        out.push(format!(
            "{}{}",
            dim(&border('└', '┘', "", list_outer)),
            dim(&border('└', '┘', "", page_outer))
        ));
        out.push(self.bottom_line(width, false));
        out
    }

    fn narrow_frame(&mut self, width: usize, height: usize) -> Vec<String> {
        let rows = height - 2;
        self.rows = rows;
        self.fit_page(width - 2);
        let place = format!(
            "{} / {}",
            (self.selected + 1).min(self.shown.len()),
            self.shown.len()
        );
        let title = self.page_title();
        let gap = width.saturating_sub(title.width() + place.width() + 3);
        let top = pad(&format!(" {title}{}  {place}", " ".repeat(gap)), width);
        let mut out = vec![self.paint_plain(&top, "\x1b[7m")];
        out.extend(
            self.page_lines(width - 2, rows)
                .into_iter()
                .map(|line| format!(" {line} ")),
        );
        out.push(self.bottom_line(width, true));
        out
    }

    const fn keep_selection_in_view(&mut self, rows: usize) {
        if self.selected < self.list_scroll {
            self.list_scroll = self.selected;
        } else if self.selected >= self.list_scroll + rows {
            self.list_scroll = self.selected + 1 - rows;
        }
    }

    /// The page's title: its name, and the section its top line is in.
    fn page_title(&self) -> String {
        let Some(open) = &self.open else {
            return String::new();
        };
        if self.keys_shown {
            return "Keys".to_owned();
        }
        let mut title = open.name.clone();
        if let Some(section) = current_section(&open.drawn.sections, open.scroll)
            && section.line < open.scroll
        {
            title = format!("{title} \u{203a} {}", section.title);
        }
        title
    }

    fn list_lines(&self, width: usize, rows: usize) -> Vec<String> {
        let mut lines = Vec::with_capacity(rows);
        let rule_at = self.filter.is_empty().then_some(self.commands_from);
        let rule = self.paint_plain(
            &pad(
                &format!(" {}", "\u{2500}".repeat(width.saturating_sub(2))),
                width,
            ),
            "\x1b[2m",
        );
        let mut index = self.list_scroll;
        while lines.len() < rows {
            let Some(&entry_index) = self.shown.get(index) else {
                lines.push(" ".repeat(width));
                continue;
            };
            // A rule between the topics and the commands, where it is in view.
            if rule_at == Some(entry_index) && entry_index > 0 && !lines.is_empty() {
                lines.push(rule.clone());
                if lines.len() == rows {
                    break;
                }
            }
            let entry = &self.entries[entry_index];
            let text = pad(&format!("  {}", entry.name), width);
            let line = if index == self.selected {
                self.paint_plain(&text, "\x1b[7m")
            } else if self.colour && matches!(entry.page, PageId::Topic(_)) {
                format!("{}{text}\x1b[0m", Style::Topic.escape())
            } else {
                text
            };
            lines.push(line);
            index += 1;
        }
        lines
    }

    fn page_lines(&self, width: usize, rows: usize) -> Vec<String> {
        if self.keys_shown || self.open.is_none() {
            return Self::key_lines(width, rows);
        }
        let Some(open) = &self.open else {
            return Vec::new();
        };
        (open.scroll..open.scroll + rows)
            .map(|number| match open.drawn.lines.get(number) {
                Some(line) => {
                    let marks: Vec<(usize, usize, bool)> = self
                        .find
                        .iter()
                        .flat_map(|find| {
                            find.matches
                                .iter()
                                .enumerate()
                                .filter_map(move |(index, mark)| {
                                    (mark.0 == number).then_some((
                                        mark.1,
                                        mark.2,
                                        index == find.current,
                                    ))
                                })
                        })
                        .collect();
                    paint_line(line, width, self.colour, &marks)
                }
                None => " ".repeat(width),
            })
            .collect()
    }

    fn key_lines(width: usize, rows: usize) -> Vec<String> {
        let mut lines: Vec<String> = KEYS
            .iter()
            .map(|(keys, what)| pad(&format!("  {keys:<22}{what}"), width))
            .collect();
        lines.resize(rows.max(lines.len()), " ".repeat(width));
        lines.truncate(rows);
        lines
    }

    /// The bottom line: the search being typed, or the keys and where the page is.
    /// `narrow` names the filter too, as no list shows it.
    fn bottom_line(&self, width: usize, narrow: bool) -> String {
        if let Some(find) = &self.find {
            let found = if find.word.is_empty() {
                String::new()
            } else if find.matches.is_empty() {
                format!("no {}", find.word)
            } else {
                format!("{} / {}", find.current + 1, find.matches.len())
            };
            let cursor = if find.typing { "\u{2588}" } else { "" };
            let text = format!(
                " Find: {}{cursor}  F3 next  Shift-F3 back  Esc done",
                find.word
            );
            let gap = width.saturating_sub(text.width() + found.width() + 1);
            return self.paint_plain(
                &pad(&format!("{text}{}{found} ", " ".repeat(gap)), width),
                "\x1b[2m",
            );
        }
        let keys = if narrow && !self.filter.is_empty() {
            format!(" Filter: {}  Esc clears", self.filter)
        } else if narrow {
            " \u{2191}\u{2193} topic  PgUp PgDn page  Ctrl-F find  F1 keys  Esc quit".to_owned()
        } else {
            " \u{2191}\u{2193} topic  PgUp PgDn page  Ctrl+\u{2191}\u{2193} line  Ctrl-F find  F1 keys  \
             Esc quit"
                .to_owned()
        };
        let place = self.open.as_ref().map_or_else(String::new, |open| {
            format!(
                "line {} of {}",
                (open.scroll + 1).min(open.drawn.lines.len()),
                open.drawn.lines.len()
            )
        });
        let gap = width.saturating_sub(keys.width() + place.width() + 1);
        let line = if gap == 0 {
            pad(&keys, width)
        } else {
            format!("{keys}{}{place} ", " ".repeat(gap))
        };
        self.paint_plain(&pad(&line, width), "\x1b[2m")
    }

    /// `text` in an attribute: a colour only when colour is on, reverse video always.
    fn paint_plain(&self, text: &str, attribute: &str) -> String {
        if self.colour || attribute.contains('7') {
            let attribute = if self.colour { attribute } else { "\x1b[7m" };
            format!("{attribute}{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }
}

/// The section whose heading is at or above `line`.
fn current_section(sections: &[SectionStart], line: usize) -> Option<&SectionStart> {
    sections
        .iter()
        .rev()
        .find(|section| section.line <= line && section.line > 0)
}

/// `text` cut or padded to `width` columns.
fn pad(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push_str(&" ".repeat(width - used));
    out
}

/// A border line of a box `width` columns wide, its title after the corner.
fn border(left: char, right: char, title: &str, width: usize) -> String {
    let inner = width.saturating_sub(2);
    let title = if title.is_empty() {
        String::new()
    } else {
        pad(
            &format!(" {title} "),
            title.width().saturating_add(2).min(inner),
        )
    };
    let rest = inner.saturating_sub(title.width());
    format!("{left}{title}{}{right}", "\u{2500}".repeat(rest))
}

/// A line of a page, cut or padded to `width` columns, with the matches of a search
/// in reverse video (the current one bold too) and its styles when `colour`.
fn paint_line(line: &Line, width: usize, colour: bool, marks: &[(usize, usize, bool)]) -> String {
    let mut out = String::new();
    let mut used = 0;
    let mut current: Option<(Style, Option<bool>)> = None;
    for span in &line.spans {
        for c in span.text.chars() {
            let w = c.width().unwrap_or(0);
            if used + w > width {
                break;
            }
            let mark = marks
                .iter()
                .find(|(start, end, _)| used >= *start && used < *end)
                .map(|(_, _, now)| *now);
            let look = (span.style, mark);
            if current != Some(look) {
                if current.is_some() {
                    out.push_str("\x1b[0m");
                }
                if colour {
                    out.push_str(span.style.escape());
                }
                match mark {
                    Some(true) => out.push_str("\x1b[1;7m"),
                    Some(false) => out.push_str("\x1b[7m"),
                    None => {}
                }
                current = Some(look);
            }
            out.push(c);
            used += w;
        }
    }
    if current.is_some() {
        out.push_str("\x1b[0m");
    }
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    if !colour && marks.is_empty() {
        // Nothing to mark and no colour: no escapes at all.
        return strip(&out);
    }
    out
}

/// `text` without its escape sequences.
fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::Page;

    fn manual() -> Manual {
        let page = |file: &str, text: &str| Page::parse(file, Subject::Command, text);
        let body = (1..=40)
            .map(|n| format!("Line {n} of the text."))
            .collect::<Vec<_>>()
            .join("\n\n");
        Manual {
            pages: vec![
                page(
                    "cd",
                    &format!(
                        "---\nsummary: Change the directory.\nkind: Bash builtins\nsee: pwd paths\n---\n\
                         ## Syntax\n\n    cd DIRECTORY\n\n## How it works\n\n{body}\n\n## Options\n\n    -L    logical\n"
                    ),
                ),
                page(
                    "pwd",
                    "---\nsummary: Print the directory.\nkind: Bash builtins\n---\n## Syntax\n\n    pwd\n",
                ),
                page(
                    "grep",
                    "---\nnames: grep egrep\nsummary: Match lines.\nsummary egrep: grep -E.\nkind: Text and file tools\ndraft: yes\n---\n",
                ),
            ],
            topics: vec![
                Page::parse(
                    "getting-started",
                    Subject::Topic,
                    "---\ntitle: Getting started\nsummary: Where to begin, as pwd shows.\n---\nRead cd's page.\n",
                ),
                Page::parse(
                    "paths",
                    Subject::Topic,
                    "---\ntitle: Paths\nsummary: C:/ and /c/.\n---\nText.\n",
                ),
            ],
        }
    }

    fn plain(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .map(|line| strip(line).trim_end().to_owned())
            .collect()
    }

    fn bottom(browser: &mut Browser<'_>) -> String {
        plain(&browser.frame(100, 12))[11].clone()
    }

    #[test]
    fn it_starts_on_getting_started_with_topics_then_commands() {
        let manual = manual();
        let mut browser = Browser::new(&manual, None, false, |_| None);
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[0].starts_with("┌ Topics & commands"), "{frame:#?}");
        assert!(frame[0].contains("┐┌ Getting started ─"), "{frame:#?}");
        assert!(frame[1].contains("│  Getting started"), "{frame:#?}");
        assert!(
            frame[1].contains("Getting started \u{2014} Where to begin, as pwd shows."),
            "{frame:#?}"
        );
        assert!(frame[2].contains("  Paths"), "{frame:#?}");
        assert!(frame[3].contains("─────"), "{frame:#?}");
        assert!(frame[4].contains("  cd"), "{frame:#?}");
        assert!(frame.iter().all(|line| line.width() <= 100), "{frame:#?}");
        assert_eq!(browser.frame(100, 12).len(), 12);
    }

    #[test]
    fn up_and_down_choose_a_topic_and_show_it_at_once() {
        let manual = manual();
        let mut browser = Browser::new(&manual, None, false, |_| None);
        browser.key(Key::Down);
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[0].contains("┐┌ Paths ─"), "{frame:#?}");
        assert!(
            frame[1].contains("Paths \u{2014} C:/ and /c/."),
            "{frame:#?}"
        );
        browser.key(Key::Down);
        assert!(plain(&browser.frame(100, 12))[1].contains("cd \u{2014} Change the directory."));
        browser.key(Key::Up);
        browser.key(Key::Up);
        browser.key(Key::Up);
        assert!(plain(&browser.frame(100, 12))[0].contains("┐┌ Getting started ─"));
        // No key opens anything: Enter only goes to a next match.
        assert_eq!(browser.key(Key::Enter), Outcome::Continue);
    }

    #[test]
    fn typing_narrows_the_list_and_shows_the_first_at_once() {
        let manual = manual();
        let mut browser = Browser::new(&manual, None, false, |_| None);
        for c in "pwd".chars() {
            browser.key(Key::Char(c));
        }
        let frame = plain(&browser.frame(100, 10));
        assert!(frame[0].starts_with("┌ Filter: pwd"), "{frame:#?}");
        // The command named first, the topic whose summary mentions it after.
        assert!(
            frame[1].contains("pwd \u{2014} Print the directory."),
            "{frame:#?}"
        );
        assert!(frame[2].contains("Getting started"), "{frame:#?}");
        // Esc clears the filter, the page kept and chosen in the whole list; again, quit.
        assert_eq!(browser.key(Key::Esc), Outcome::Continue);
        let frame = plain(&browser.frame(100, 10));
        assert!(frame[0].starts_with("┌ Topics & commands"), "{frame:#?}");
        assert!(frame[0].contains("┐┌ pwd ─"), "{frame:#?}");
        assert_eq!(browser.key(Key::Esc), Outcome::Quit);
    }

    #[test]
    fn the_filter_finds_a_page_by_its_text_too() {
        let manual = manual();
        let mut browser = Browser::new(&manual, None, false, |_| None);
        for c in "logical".chars() {
            browser.key(Key::Char(c));
        }
        let frame = plain(&browser.frame(100, 10));
        assert!(frame[0].contains("┐┌ cd ─"), "{frame:#?}");
        browser.key(Key::Backspace);
        assert_eq!(browser.filter, "logica");
    }

    #[test]
    fn page_keys_move_in_the_page_and_its_section_stays_on_top() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("cd"), false, |_| None);
        browser.frame(100, 12);
        browser.key(Key::SectionDown);
        browser.key(Key::SectionDown);
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[1].contains("HOW IT WORKS"), "{frame:#?}");
        browser.key(Key::LineDown);
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[0].contains("cd \u{203a} HOW IT WORKS"), "{frame:#?}");
        browser.key(Key::PageDown);
        assert!(
            bottom(&mut browser).contains("line 16 of "),
            "{}",
            bottom(&mut browser)
        );
        browser.key(Key::End);
        let frame = plain(&browser.frame(100, 12));
        assert!(
            frame.iter().any(|line| line.contains("OPTIONS")),
            "{frame:#?}"
        );
        browser.key(Key::Home);
        assert!(plain(&browser.frame(100, 12))[1].contains("cd \u{2014} Change the directory."));
        browser.key(Key::LineUp);
        assert!(bottom(&mut browser).contains("line 1 of "));
    }

    #[test]
    fn ctrl_f_finds_as_it_is_typed_and_f3_goes_on() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("cd"), false, |_| None);
        browser.frame(100, 12);
        browser.key(Key::Find);
        for c in "line 3".chars() {
            browser.key(Key::Char(c));
        }
        // Found as typed: "Line 3" and "Line 30" to "Line 39".
        assert!(
            bottom(&mut browser).contains("Find: line 3█"),
            "{}",
            bottom(&mut browser)
        );
        assert!(
            bottom(&mut browser).ends_with("1 / 11"),
            "{}",
            bottom(&mut browser)
        );
        assert!(
            browser
                .frame(100, 12)
                .iter()
                .any(|line| line.contains("\x1b[1;7mL"))
        );
        browser.key(Key::FindNext);
        assert!(bottom(&mut browser).ends_with("2 / 11"));
        browser.key(Key::FindPrevious);
        browser.key(Key::FindPrevious);
        assert!(bottom(&mut browser).ends_with("11 / 11"));
        // Enter ends the typing; letters go to the filter again, the matches kept.
        browser.key(Key::Enter);
        browser.key(Key::Enter);
        assert!(
            bottom(&mut browser).ends_with("1 / 11"),
            "{}",
            bottom(&mut browser)
        );
        // Esc leaves the search, then the browser.
        assert_eq!(browser.key(Key::Esc), Outcome::Continue);
        assert!(browser.find.is_none());
        assert_eq!(browser.key(Key::Esc), Outcome::Quit);
    }

    #[test]
    fn a_draft_ends_with_its_own_help_and_a_narrow_screen_shows_the_page_alone() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("egrep"), false, |name| {
            Some(format!(
                "Usage: {name} [OPTION]... PATTERN\n  -E  extended\n"
            ))
        });
        let frame = plain(&browser.frame(60, 20));
        assert!(frame[0].starts_with(" egrep"), "{frame:#?}");
        assert!(frame[0].ends_with("4 / 6"), "{frame:#?}");
        let text = frame.join("\n");
        assert!(text.contains("grep, egrep \u{2014} Match lines."), "{text}");
        assert!(text.contains("USAGE AND OPTIONS"), "{text}");
        assert!(text.contains("Usage: grep [OPTION]... PATTERN"), "{text}");
        assert!(!text.contains('│'), "the page alone: {text}");
        // Up still chooses, and the page follows.
        browser.key(Key::Up);
        let frame = plain(&browser.frame(60, 20));
        assert!(frame[0].starts_with(" cd"), "{frame:#?}");
        assert!(frame.iter().all(|line| line.width() <= 60), "{frame:#?}");
        browser.key(Key::Char('p'));
        assert!(plain(&browser.frame(60, 20))[19].starts_with(" Filter: p"));
    }

    #[test]
    fn f1_shows_the_keys() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("cd"), false, |_| None);
        browser.key(Key::Keys);
        let frame = plain(&browser.frame(100, 24));
        assert!(frame[0].contains("┐┌ Keys ─"), "{frame:#?}");
        assert!(
            frame
                .iter()
                .any(|line| line.contains("its page shows at once")),
            "{frame:#?}"
        );
        browser.key(Key::Keys);
        assert!(plain(&browser.frame(100, 24))[0].contains("┐┌ cd ─"));
    }

    #[test]
    fn colour_paints_styles_and_no_colour_keeps_only_reverse_video() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("cd"), true, |_| None);
        let frame = browser.frame(100, 12);
        assert!(
            frame.iter().any(|line| line.contains("\x1b[1;34mSYNTAX")),
            "{frame:#?}"
        );
        let mut plain_browser = Browser::new(&manual, Some("cd"), false, |_| None);
        let frame = plain_browser.frame(100, 12);
        assert!(
            frame.iter().all(|line| !line.contains("\x1b[1;34m")),
            "{frame:#?}"
        );
        assert!(
            frame.iter().any(|line| line.contains("\x1b[7m")),
            "the selection: {frame:#?}"
        );
    }
}
