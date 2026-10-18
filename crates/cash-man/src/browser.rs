//! The browser `man` shows: cash's topics and every command on the left, a page on the
//! right, keys to move, find and follow links (`research/man-browser-design.md`,
//! section 3).
//!
//! It is a state that keys change and frames are drawn from, with no terminal of its
//! own: the `man` builtin reads the keys and writes the frames, and the tests drive it
//! as text. Under 80 columns one pane fills the screen and Tab switches between them.

use cash_picker::filter::Fuzzy;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::Manual;
use crate::page::Subject;
use crate::render::{self, Drawn, Line, SectionStart, Style};

/// Below this many columns, one pane at a time.
pub const NARROW: usize = 80;

/// A key the browser acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A line up.
    Up,
    /// A line down.
    Down,
    /// A screen up.
    PageUp,
    /// A screen down.
    PageDown,
    /// The start.
    Home,
    /// The end.
    End,
    /// To the list.
    Left,
    /// To the page.
    Right,
    /// To the other pane.
    Tab,
    /// Open, follow, or confirm what is typed.
    Enter,
    /// Leave what is typed or found; else quit.
    Esc,
    /// Take a letter off what is typed; else back.
    Backspace,
    /// Alt-Left: the page before.
    Back,
    /// Alt-Right: the page after, having gone back.
    Forward,
    /// Ctrl-F: find a word in every page.
    FindEverywhere,
    /// A letter: typed, or a key of the page.
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

/// The pane keys go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    List,
    Page,
}

/// What the list holds.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Listing {
    /// Every entry, or those the filter keeps.
    All,
    /// The See-also pages of the page open.
    SeeAlso,
    /// The pages a search of every page found, and the word.
    Found(String),
}

/// A word being typed or found.
#[derive(Debug)]
struct Find {
    word: String,
    typing: bool,
    everywhere: bool,
    /// Each match in the open page: its line and its columns, start and end.
    matches: Vec<(usize, usize, usize)>,
    current: usize,
}

/// The page open, drawn for a width.
#[derive(Debug)]
struct Open {
    page: PageId,
    name: String,
    drawn: Drawn,
    width: usize,
    scroll: usize,
}

/// A page opened before, to go back to.
#[derive(Clone, Debug)]
struct Visit {
    page: PageId,
    name: String,
    scroll: usize,
}

/// The keys, as `?` shows them.
const KEYS: [(&str, &str); 15] = [
    ("Up Down PgUp PgDn", "move in the pane"),
    ("Home End", "to the start or the end"),
    ("Tab, Left Right", "to the other pane"),
    ("letters (list)", "narrow the list"),
    ("Enter (list)", "open the page"),
    ("Enter (page)", "list its See-also pages"),
    ("/ (page)", "find a word in the page"),
    ("n N", "the next or previous match"),
    ("Ctrl-F", "find a word in every page"),
    ("1 to 8, [ ]", "a section, or the one before or after"),
    ("Backspace, Alt-Left", "the page before"),
    ("Alt-Right", "the page after"),
    ("?", "these keys, or the page again"),
    ("Esc", "leave a search; pressed again, quit"),
    ("q (page)", "quit"),
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
    listing: Listing,
    /// The entries the list shows, by index.
    shown: Vec<usize>,
    selected: usize,
    list_scroll: usize,
    focus: Focus,
    open: Option<Open>,
    back: Vec<Visit>,
    forward: Vec<Visit>,
    find: Option<Find>,
    keys_shown: bool,
    colour: bool,
    /// The rows a page pane had at the last frame, for a screen's worth of scrolling.
    rows: usize,
    /// A command's own `--help`, for a draft page.
    own_help: OwnHelp<'m>,
}

impl<'m> Browser<'m> {
    /// A browser on `manual`, open on `start` (a command or a topic) or on the topic
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
            listing: Listing::All,
            selected: 0,
            list_scroll: 0,
            focus: Focus::Page,
            open: None,
            back: Vec::new(),
            forward: Vec::new(),
            find: None,
            keys_shown: false,
            colour,
            rows: 20,
            own_help: Box::new(own_help),
        };
        let start = start.map_or("getting-started", |start| start);
        if let Some(entry) = browser.entry_named(start) {
            browser.selected = entry;
            browser.open_entry(entry);
        } else {
            browser.focus = Focus::List;
        }
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

    /// Opens the page of `entry`, remembering the one open to go back to.
    fn open_entry(&mut self, entry: usize) {
        let Some(chosen) = self.entries.get(entry) else {
            return;
        };
        let (page, name) = (chosen.page, chosen.name.clone());
        if let Some(open) = &self.open {
            if open.page == page {
                self.focus = Focus::Page;
                return;
            }
            self.back.push(Visit {
                page: open.page,
                name: open.name.clone(),
                scroll: open.scroll,
            });
            self.forward.clear();
        }
        self.show(Visit {
            page,
            name,
            scroll: 0,
        });
    }

    /// Shows a page, scrolled as `visit` says.
    fn show(&mut self, visit: Visit) {
        let width = self.open.as_ref().map_or(60, |open| open.width);
        self.open = Some(Open {
            drawn: self.draw_page(visit.page, width),
            page: visit.page,
            name: visit.name,
            width,
            scroll: visit.scroll,
        });
        self.find = None;
        self.keys_shown = false;
        self.focus = Focus::Page;
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
        if self.find.as_ref().is_some_and(|find| find.typing) {
            self.find_key(key);
            return Outcome::Continue;
        }
        match key {
            Key::Back => self.go_back(),
            Key::Forward => self.go_forward(),
            Key::FindEverywhere => self.start_find(true),
            Key::Tab => self.toggle_focus(),
            Key::Esc => return self.escape(),
            _ => {
                return match self.focus {
                    Focus::List => self.list_key(key),
                    Focus::Page => self.page_key(key),
                };
            }
        }
        Outcome::Continue
    }

    const fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::List if self.open.is_some() => Focus::Page,
            _ => Focus::List,
        };
    }

    /// Esc: a filter, a list of links or found pages, or a search goes first; then the
    /// browser.
    fn escape(&mut self) -> Outcome {
        if self.find.take().is_some() {
            return Outcome::Continue;
        }
        if !self.filter.is_empty() || self.listing != Listing::All {
            self.filter.clear();
            self.set_listing(Listing::All);
            return Outcome::Continue;
        }
        Outcome::Quit
    }

    fn list_key(&mut self, key: Key) -> Outcome {
        let rows = self.rows.max(1);
        match key {
            Key::Up => self.selected = self.selected.saturating_sub(1),
            Key::Down => {
                self.selected = (self.selected + 1).min(self.shown.len().saturating_sub(1));
            }
            Key::PageUp => self.selected = self.selected.saturating_sub(rows),
            Key::PageDown => {
                self.selected = (self.selected + rows).min(self.shown.len().saturating_sub(1));
            }
            Key::Home => self.selected = 0,
            Key::End => self.selected = self.shown.len().saturating_sub(1),
            Key::Right if self.open.is_some() => self.focus = Focus::Page,
            Key::Enter => self.open_selected(),
            Key::Backspace if self.filter.pop().is_some() => self.refilter(),
            Key::Backspace => self.go_back(),
            Key::Char(c) if !c.is_control() => {
                self.filter.push(c);
                self.listing = Listing::All;
                self.refilter();
            }
            _ => {}
        }
        Outcome::Continue
    }

    fn open_selected(&mut self) {
        let Some(&entry) = self.shown.get(self.selected) else {
            return;
        };
        let word = match &self.listing {
            Listing::Found(word) => Some(word.clone()),
            _ => None,
        };
        self.open_entry(entry);
        if let Some(word) = word {
            self.find = Some(Find {
                word,
                typing: false,
                everywhere: false,
                matches: Vec::new(),
                current: 0,
            });
            self.find_matches();
        }
    }

    fn page_key(&mut self, key: Key) -> Outcome {
        let rows = self.rows.max(1);
        match key {
            Key::Up => self.scroll_by(-1),
            Key::Down => self.scroll_by(1),
            Key::PageUp => self.scroll_by(-isize::try_from(rows).unwrap_or(1)),
            Key::PageDown => self.scroll_by(isize::try_from(rows).unwrap_or(1)),
            Key::Home => self.scroll_to(0),
            Key::End => self.scroll_to(usize::MAX),
            Key::Left => self.focus = Focus::List,
            Key::Enter => {
                if self.see_also().is_empty() {
                    return Outcome::Continue;
                }
                self.filter.clear();
                self.set_listing(Listing::SeeAlso);
                self.focus = Focus::List;
            }
            Key::Backspace => self.go_back(),
            Key::Char('q') => return Outcome::Quit,
            Key::Char('/') => self.start_find(false),
            Key::Char('n') => self.next_match(true),
            Key::Char('N') => self.next_match(false),
            Key::Char('?') => self.keys_shown = !self.keys_shown,
            Key::Char('[') => self.section_step(false),
            Key::Char(']') => self.section_step(true),
            Key::Char(c @ '1'..='8') => {
                let n = c.to_digit(10).and_then(|n| usize::try_from(n).ok());
                self.section_number(n.unwrap_or(1));
            }
            _ => {}
        }
        Outcome::Continue
    }

    /// The entries the open page's See also names, in its order.
    fn see_also(&self) -> Vec<usize> {
        let Some(open) = &self.open else {
            return Vec::new();
        };
        let page = match open.page {
            PageId::Command(index) => &self.manual.pages[index],
            PageId::Topic(index) => &self.manual.topics[index],
        };
        page.see
            .iter()
            .filter_map(|name| self.entry_named(name))
            .collect()
    }

    fn set_listing(&mut self, listing: Listing) {
        self.shown = match &listing {
            Listing::All => (0..self.entries.len()).collect(),
            Listing::SeeAlso => self.see_also(),
            Listing::Found(word) => {
                let mut found = Vec::new();
                for hit in self.manual.search(word) {
                    if let Some(entry) = self.entry_named(hit.name)
                        && !found.contains(&entry)
                    {
                        found.push(entry);
                    }
                }
                found
            }
        };
        self.listing = listing;
        self.selected = 0;
        self.list_scroll = 0;
        if self.listing == Listing::All && !self.filter.is_empty() {
            self.refilter();
        }
    }

    /// The list after the filter changed: the entries whose name or summary it
    /// matches, best first.
    fn refilter(&mut self) {
        if self.filter.trim().is_empty() {
            self.shown = (0..self.entries.len()).collect();
        } else {
            self.fuzzy.set(&self.filter);
            let candidates: Vec<String> = self
                .entries
                .iter()
                .map(|entry| format!("{} {}", entry.name, entry.summary.replace('`', "")))
                .collect();
            self.shown = self
                .fuzzy
                .rank(candidates.iter().map(String::as_str))
                .into_iter()
                .map(|found| found.index)
                .collect();
        }
        self.selected = 0;
        self.list_scroll = 0;
    }

    fn go_back(&mut self) {
        let Some(visit) = self.back.pop() else {
            return;
        };
        if let Some(open) = &self.open {
            self.forward.push(Visit {
                page: open.page,
                name: open.name.clone(),
                scroll: open.scroll,
            });
        }
        self.show(visit);
    }

    fn go_forward(&mut self) {
        let Some(visit) = self.forward.pop() else {
            return;
        };
        if let Some(open) = &self.open {
            self.back.push(Visit {
                page: open.page,
                name: open.name.clone(),
                scroll: open.scroll,
            });
        }
        self.show(visit);
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

    /// The `n`th section under a heading.
    fn section_number(&mut self, n: usize) {
        let to = self
            .open
            .as_ref()
            .and_then(|open| open.drawn.sections.get(n))
            .map(|section| section.line);
        if let Some(to) = to {
            self.scroll_to(to);
        }
    }

    fn start_find(&mut self, everywhere: bool) {
        if !everywhere && self.open.is_none() {
            return;
        }
        self.find = Some(Find {
            word: String::new(),
            typing: true,
            everywhere,
            matches: Vec::new(),
            current: 0,
        });
    }

    /// A key while a word is typed for a search.
    fn find_key(&mut self, key: Key) {
        let Some(find) = &mut self.find else {
            return;
        };
        match key {
            Key::Char(c) if !c.is_control() => find.word.push(c),
            Key::Backspace => {
                find.word.pop();
            }
            Key::Esc => self.find = None,
            Key::Enter if find.word.trim().is_empty() => self.find = None,
            Key::Enter if find.everywhere => {
                let word = find.word.clone();
                self.find = None;
                self.filter.clear();
                self.set_listing(Listing::Found(word));
                self.focus = Focus::List;
            }
            Key::Enter => {
                find.typing = false;
                self.find_matches();
            }
            _ => {}
        }
    }

    /// Every place the word is found in the open page, case aside; the first at or
    /// after the top line is the current one.
    fn find_matches(&mut self) {
        let (Some(find), Some(open)) = (&mut self.find, &self.open) else {
            return;
        };
        let needle: Vec<char> = find.word.to_lowercase().chars().collect();
        find.matches.clear();
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

    /// Draws the open page again when its pane is a new width, keeping its place.
    fn fit_page(&mut self, width: usize) {
        let needs = self.open.as_ref().is_some_and(|open| open.width != width);
        if !needs {
            return;
        }
        let Some(open) = &self.open else {
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
        if self.find.as_ref().is_some_and(|find| !find.typing) {
            self.find_matches();
        }
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
        let list_title = self.list_title();
        let page_title = self.page_title();
        let mut out = Vec::with_capacity(height);
        out.push(format!(
            "{}{}",
            dim(&border('┌', '┐', &list_title, list_outer)),
            dim(&border('┌', '┐', &page_title, page_outer))
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
        out.push(self.bottom_line(width));
        out
    }

    fn narrow_frame(&mut self, width: usize, height: usize) -> Vec<String> {
        let rows = height - 2;
        self.rows = rows;
        self.fit_page(width - 2);
        self.keep_selection_in_view(rows);
        let title = match self.focus {
            Focus::List => self.list_title(),
            Focus::Page => self.page_title(),
        };
        let mut out = vec![self.paint_plain(&pad(&format!(" {title}"), width), "\x1b[7m")];
        match self.focus {
            Focus::List => out.extend(self.list_lines(width, rows)),
            Focus::Page => out.extend(
                self.page_lines(width - 2, rows)
                    .into_iter()
                    .map(|line| format!(" {line} ")),
            ),
        }
        out.push(self.bottom_line(width));
        out
    }

    const fn keep_selection_in_view(&mut self, rows: usize) {
        if self.selected < self.list_scroll {
            self.list_scroll = self.selected;
        } else if self.selected >= self.list_scroll + rows {
            self.list_scroll = self.selected + 1 - rows;
        }
    }

    fn list_title(&self) -> String {
        match &self.listing {
            Listing::All if self.filter.is_empty() => "Topics & commands".to_owned(),
            Listing::All => format!("Filter: {}", self.filter),
            Listing::SeeAlso => format!(
                "See also: {}",
                self.open.as_ref().map_or("", |open| open.name.as_str())
            ),
            Listing::Found(word) => format!("Found: {word}"),
        }
    }

    /// The page pane's title: the page, and the section its top line is in.
    fn page_title(&self) -> String {
        let Some(open) = &self.open else {
            return String::new();
        };
        if self.keys_shown {
            return "Keys".to_owned();
        }
        let summary = self
            .entries
            .iter()
            .find(|entry| entry.page == open.page && entry.name == open.name)
            .map_or("", |entry| entry.summary.as_str())
            .replace('`', "");
        let mut title = if summary.is_empty() {
            open.name.clone()
        } else {
            format!("{} \u{2014} {summary}", open.name)
        };
        if let Some(section) = current_section(&open.drawn.sections, open.scroll)
            && section.line < open.scroll
        {
            title = format!("{title} \u{203a} {}", section.title);
        }
        title
    }

    fn list_lines(&self, width: usize, rows: usize) -> Vec<String> {
        let open = self
            .open
            .as_ref()
            .map(|open| (open.page, open.name.as_str()));
        let mut lines = Vec::with_capacity(rows);
        let rule_at =
            (self.listing == Listing::All && self.filter.is_empty()).then_some(self.commands_from);
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
            let marker = if open == Some((entry.page, entry.name.as_str())) {
                "\u{25b8} "
            } else {
                "  "
            };
            let text = pad(&format!("{marker}{}", entry.name), width);
            let style = if matches!(entry.page, PageId::Topic(_)) {
                Style::Topic
            } else {
                Style::Plain
            };
            let line = if index == self.selected {
                let attribute = if self.focus == Focus::List {
                    "\x1b[7m"
                } else {
                    "\x1b[2;7m"
                };
                self.paint_plain(&text, attribute)
            } else if self.colour && style == Style::Topic {
                format!("{}{text}\x1b[0m", style.escape())
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

    fn bottom_line(&self, width: usize) -> String {
        if let Some(find) = self.find.as_ref().filter(|find| find.typing) {
            let prompt = if find.everywhere {
                "Find in every page: "
            } else {
                "/"
            };
            return pad(&format!("{prompt}{}\u{2588}", find.word), width);
        }
        let keys = match self.focus {
            Focus::List => " type to filter  Enter open  Tab page  Ctrl-F every page  Esc quit",
            Focus::Page => " / find  n N  Enter see also  [ ] sections  Tab list  ? keys  Esc quit",
        };
        let place = match (&self.find, &self.open, self.focus) {
            (Some(find), _, _) if !find.matches.is_empty() => {
                format!("{} / {}", find.current + 1, find.matches.len())
            }
            (Some(find), _, _) => format!("no {}", find.word),
            (None, Some(open), Focus::Page) => format!(
                "line {} of {}",
                (open.scroll + 1).min(open.drawn.lines.len()),
                open.drawn.lines.len()
            ),
            _ => format!(
                "{} / {}",
                (self.selected + 1).min(self.shown.len()),
                self.shown.len()
            ),
        };
        let gap = width.saturating_sub(keys.width() + place.width() + 1);
        let line = if gap == 0 {
            pad(&format!(" {place}"), width)
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
            .join(
                "

",
            );
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
                    "---\ntitle: Getting started\nsummary: Where to begin.\n---\nRead cd's page.\n",
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

    #[test]
    fn it_opens_on_getting_started_with_topics_then_commands() {
        let manual = manual();
        let mut browser = Browser::new(&manual, None, false, |_| None);
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[0].starts_with("┌ Topics & commands"), "{frame:#?}");
        assert!(
            frame[0].contains("Getting started \u{2014} Where to begin."),
            "{frame:#?}"
        );
        assert!(frame[1].contains("▸ Getting started"), "{frame:#?}");
        assert!(frame[2].contains("  Paths"), "{frame:#?}");
        assert!(frame[3].contains("─────"), "{frame:#?}");
        assert!(frame[4].contains("  cd"), "{frame:#?}");
        assert!(
            frame[1].contains("Getting started \u{2014} Where to begin."),
            "{frame:#?}"
        );
        assert!(frame.iter().all(|line| line.width() <= 100), "{frame:#?}");
        assert_eq!(browser.frame(100, 12).len(), 12);
    }

    #[test]
    fn typing_filters_the_list_and_enter_opens_the_page() {
        let manual = manual();
        let mut browser = Browser::new(&manual, None, false, |_| None);
        browser.key(Key::Tab);
        for c in "pwd".chars() {
            browser.key(Key::Char(c));
        }
        let frame = plain(&browser.frame(100, 10));
        assert!(frame[0].starts_with("┌ Filter: pwd"), "{frame:#?}");
        assert!(frame[1].contains("pwd"), "{frame:#?}");
        browser.key(Key::Enter);
        let frame = plain(&browser.frame(100, 10));
        assert!(
            frame[0].contains("pwd \u{2014} Print the directory."),
            "{frame:#?}"
        );
        // Esc clears the filter first, and then quits.
        assert_eq!(browser.key(Key::Esc), Outcome::Continue);
        assert_eq!(browser.key(Key::Esc), Outcome::Quit);
    }

    #[test]
    fn scrolling_keeps_the_section_on_the_top_line_and_sections_jump() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("cd"), false, |_| None);
        browser.frame(100, 12);
        browser.key(Key::Char('2'));
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[1].contains("HOW IT WORKS"), "{frame:#?}");
        browser.key(Key::Down);
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[0].contains("\u{203a} HOW IT WORKS"), "{frame:#?}");
        // The next section, as near the top as the end of the page lets it come.
        browser.key(Key::Char(']'));
        let frame = plain(&browser.frame(100, 12));
        assert!(
            frame.iter().any(|line| line.contains("OPTIONS")),
            "{frame:#?}"
        );
        browser.key(Key::Home);
        assert!(plain(&browser.frame(100, 12))[1].contains("cd \u{2014} Change the directory."));
    }

    #[test]
    fn find_marks_the_matches_and_n_goes_to_the_next() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("cd"), false, |_| None);
        browser.frame(100, 12);
        browser.key(Key::Char('/'));
        for c in "line 3".chars() {
            browser.key(Key::Char(c));
        }
        assert!(plain(&browser.frame(100, 12))[11].starts_with("/line 3"));
        browser.key(Key::Enter);
        let frame = browser.frame(100, 12);
        let bottom = strip(&frame[11]);
        // "Line 3" and "Line 30" to "Line 39".
        assert!(bottom.trim_end().ends_with("1 / 11"), "{bottom}");
        assert!(
            frame.iter().any(|line| line.contains("\x1b[1;7mL")),
            "{frame:#?}"
        );
        browser.key(Key::Char('n'));
        assert!(
            strip(&browser.frame(100, 12)[11])
                .trim_end()
                .ends_with("2 / 11")
        );
        browser.key(Key::Char('N'));
        browser.key(Key::Char('N'));
        assert!(
            strip(&browser.frame(100, 12)[11])
                .trim_end()
                .ends_with("11 / 11")
        );
        // Esc leaves the search, then the browser.
        assert_eq!(browser.key(Key::Esc), Outcome::Continue);
        assert!(browser.find.is_none());
        assert_eq!(browser.key(Key::Esc), Outcome::Quit);
    }

    #[test]
    fn enter_lists_the_see_also_and_backspace_goes_back() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("cd"), false, |_| None);
        browser.frame(100, 12);
        browser.key(Key::Enter);
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[0].starts_with("┌ See also: cd"), "{frame:#?}");
        assert!(
            frame[1].contains("pwd") && frame[2].contains("Paths"),
            "{frame:#?}"
        );
        browser.key(Key::Down);
        browser.key(Key::Enter);
        assert!(plain(&browser.frame(100, 12))[0].contains("Paths \u{2014} C:/ and /c/."));
        browser.key(Key::Backspace);
        assert!(plain(&browser.frame(100, 12))[0].contains("cd \u{2014} Change the directory."));
        browser.key(Key::Forward);
        assert!(plain(&browser.frame(100, 12))[0].contains("Paths"));
    }

    #[test]
    fn find_everywhere_lists_the_pages_and_opens_on_the_match() {
        let manual = manual();
        let mut browser = Browser::new(&manual, None, false, |_| None);
        browser.frame(100, 12);
        browser.key(Key::FindEverywhere);
        for c in "logical".chars() {
            browser.key(Key::Char(c));
        }
        browser.key(Key::Enter);
        let frame = plain(&browser.frame(100, 12));
        assert!(frame[0].starts_with("┌ Found: logical"), "{frame:#?}");
        assert!(frame[1].contains("cd"), "{frame:#?}");
        browser.key(Key::Enter);
        let frame = browser.frame(100, 12);
        assert!(
            strip(&frame[11]).trim_end().ends_with("1 / 1"),
            "{frame:#?}"
        );
        assert!(
            frame.iter().any(|line| line.contains("\x1b[1;7mlogical")),
            "{frame:#?}"
        );
    }

    #[test]
    fn a_draft_ends_with_its_own_help_and_a_narrow_screen_has_one_pane() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("egrep"), false, |name| {
            Some(format!(
                "Usage: {name} [OPTION]... PATTERN\n  -E  extended\n"
            ))
        });
        let frame = plain(&browser.frame(60, 20));
        assert!(frame[0].contains("egrep \u{2014} grep -E."), "{frame:#?}");
        let text = frame.join("\n");
        assert!(text.contains("USAGE AND OPTIONS"), "{text}");
        assert!(text.contains("Usage: grep [OPTION]... PATTERN"), "{text}");
        assert!(!text.contains('│'), "one pane, no frames: {text}");
        browser.key(Key::Tab);
        let frame = plain(&browser.frame(60, 20));
        assert!(frame[0].contains("Topics & commands"), "{frame:#?}");
        assert!(frame.iter().all(|line| line.width() <= 60), "{frame:#?}");
    }

    #[test]
    fn keys_are_shown_on_a_question_mark_and_q_quits_the_page() {
        let manual = manual();
        let mut browser = Browser::new(&manual, Some("cd"), false, |_| None);
        browser.key(Key::Char('?'));
        let frame = plain(&browser.frame(100, 24));
        assert!(frame[0].contains("Keys"), "{frame:#?}");
        assert!(
            frame
                .iter()
                .any(|line| line.contains("find a word in every page")),
            "{frame:#?}"
        );
        assert_eq!(browser.key(Key::Char('q')), Outcome::Quit);
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
