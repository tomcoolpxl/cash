//! The picker's state, keys and frames: what is shown, what a key does, and the lines to
//! draw (spec D73).
//!
//! There is no terminal here: [`crate::term`] reads keys and draws the frames, so the
//! picker can be driven and checked without one.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use unicode_width::UnicodeWidthChar as _;

use crate::colours::{Colours, paint};
use crate::context::{Shows, Then};
use crate::filter::Fuzzy;
use crate::search::{Search, Status};
use crate::tree::{self, Entry, Filter, Ignores, Line};

/// A key the picker acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// The line above.
    Up,
    /// The line below.
    Down,
    /// Ten lines up.
    PageUp,
    /// Ten lines down.
    PageDown,
    /// The first line.
    Home,
    /// The last line.
    End,
    /// The root's parent, or the drives at a drive's root.
    Left,
    /// The selected folder as the root.
    Right,
    /// Pick.
    Enter,
    /// Pick and close, whatever the command.
    CtrlEnter,
    /// Clear the filter, or close.
    Esc,
    /// Take a letter off the filter.
    Backspace,
    /// A letter of the filter.
    Char(char),
    /// Alt-F: folders only, or files too.
    Files,
    /// Alt-H: the folder history.
    History,
    /// Alt-.: hidden entries.
    Hidden,
    /// Alt-I: `.gitignore`d entries.
    Ignored,
}

/// What a key led to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing for the caller to do; draw the next frame.
    Continue,
    /// Something was picked; `close` when the picker is done.
    Picked {
        /// What was picked.
        path: PathBuf,
        /// Whether it is a folder.
        folder: bool,
        /// Whether the picker is done.
        close: bool,
    },
    /// Closed without a pick.
    Closed,
}

/// How a picker starts.
pub struct Setup {
    /// The folder it opens in.
    pub root: PathBuf,
    /// The filter it opens with.
    pub typed: String,
    /// Folders only, or files too.
    pub shows: Shows,
    /// What a pick does.
    pub then: Then,
    /// The folder history, most recent first, for Alt-H.
    pub history: Vec<PathBuf>,
    /// The home folder, shown as `~`.
    pub home: Option<PathBuf>,
    /// The colours to draw in.
    pub colours: Colours,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Tree,
    Drives,
    History,
}

/// A line of the list.
#[derive(Clone, Debug)]
struct Item {
    /// What a pick or Right takes; `None` for a line that cannot be selected.
    path: Option<PathBuf>,
    folder: bool,
    /// Tree branches drawn before the text.
    prefix: String,
    text: String,
    /// The characters of `text` a filter matched.
    positions: Vec<usize>,
}

/// The picker.
pub struct Picker {
    root: PathBuf,
    view: View,
    filter: Filter,
    then: Then,
    typed: String,
    fuzzy: Fuzzy,
    cache: HashMap<PathBuf, Vec<Entry>>,
    ignores: Ignores,
    search: Option<(PathBuf, Filter, Search)>,
    history: Vec<PathBuf>,
    home: Option<PathBuf>,
    colours: Colours,
    items: Vec<Item>,
    selected: usize,
    /// The first item shown, when the list is longer than the rows.
    scroll: usize,
    /// The path to select at the next rebuild.
    want: Option<PathBuf>,
    /// Picks made while the picker stayed open.
    picks: usize,
    /// The rows and search size the items were built for; `None` forces a rebuild.
    built: Option<(usize, usize)>,
}

impl Picker {
    /// A picker as `setup` says.
    #[must_use]
    pub fn new(setup: Setup) -> Self {
        Self {
            root: setup.root,
            view: View::Tree,
            filter: Filter {
                shows: setup.shows,
                hidden: false,
                ignored: false,
            },
            then: setup.then,
            typed: setup.typed,
            fuzzy: Fuzzy::default(),
            cache: HashMap::new(),
            ignores: Ignores::default(),
            search: None,
            history: setup.history,
            home: setup.home,
            colours: setup.colours,
            items: Vec::new(),
            selected: 0,
            scroll: 0,
            want: None,
            picks: 0,
            built: None,
        }
    }

    /// The folder shown.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether a search is still running, so that later frames may show more.
    #[must_use]
    pub fn searching(&self) -> bool {
        !self.typed.is_empty()
            && self
                .search
                .as_ref()
                .is_some_and(|(_, _, search)| search.status() == Status::Running)
    }

    /// Acts on a key.
    pub fn key(&mut self, key: Key) -> Outcome {
        match key {
            Key::Up => self.step(-1),
            Key::Down => self.step(1),
            Key::PageUp => self.step(-10),
            Key::PageDown => self.step(10),
            Key::Home => self.step(isize::MIN / 2),
            Key::End => self.step(isize::MAX / 2),
            Key::Right => {
                if let Some(item) = self.items.get(self.selected).cloned()
                    && let Some(path) = item.path
                    && item.folder
                {
                    self.enter(path);
                }
            }
            Key::Left => self.left(),
            Key::Enter | Key::CtrlEnter => return self.pick(key == Key::CtrlEnter),
            Key::Esc => {
                if self.typed.is_empty() {
                    return Outcome::Closed;
                }
                self.set_typed(String::new());
            }
            Key::Backspace => {
                let mut typed = self.typed.clone();
                typed.pop();
                self.set_typed(typed);
            }
            Key::Char(c) => {
                let mut typed = self.typed.clone();
                typed.push(c);
                self.set_typed(typed);
            }
            Key::Files => {
                self.filter.shows = match self.filter.shows {
                    Shows::Folders => Shows::Everything,
                    Shows::Everything => Shows::Folders,
                };
                self.refilter();
            }
            Key::Hidden => {
                self.filter.hidden = !self.filter.hidden;
                self.refilter();
            }
            Key::Ignored => {
                self.filter.ignored = !self.filter.ignored;
                self.refilter();
            }
            Key::History => {
                self.view = if self.view == View::History {
                    View::Tree
                } else {
                    View::History
                };
                self.built = None;
                self.selected = 0;
            }
        }
        Outcome::Continue
    }

    /// Counts a pick that left the picker open, for the header.
    pub const fn picked(&mut self) {
        self.picks += 1;
    }

    fn pick(&mut self, close_anyway: bool) -> Outcome {
        let Some(item) = self.items.get(self.selected).cloned() else {
            return Outcome::Continue;
        };
        let Some(path) = item.path else {
            return Outcome::Continue;
        };
        if self.view == View::Drives {
            self.enter(path);
            return Outcome::Continue;
        }
        Outcome::Picked {
            path,
            folder: item.folder,
            close: close_anyway || self.then != Then::StayOpen,
        }
    }

    fn enter(&mut self, folder: PathBuf) {
        self.root = folder;
        self.view = View::Tree;
        self.typed.clear();
        self.want = None;
        self.selected = 0;
        self.built = None;
    }

    fn left(&mut self) {
        match self.view {
            View::Tree => {
                self.want = Some(self.root.clone());
                if let Some(parent) = self.root.parent() {
                    self.root = parent.to_path_buf();
                } else {
                    self.view = View::Drives;
                }
                self.typed.clear();
                self.built = None;
            }
            View::Drives | View::History => {
                self.view = View::Tree;
                self.built = None;
            }
        }
    }

    fn set_typed(&mut self, typed: String) {
        self.typed = typed;
        self.built = None;
        self.selected = 0;
        self.scroll = 0;
    }

    fn refilter(&mut self) {
        self.cache.clear();
        self.search = None;
        if let Some(item) = self.items.get(self.selected) {
            self.want.clone_from(&item.path);
        }
        self.built = None;
    }

    fn step(&mut self, by: isize) {
        let selectable: Vec<usize> = (0..self.items.len())
            .filter(|&i| self.items[i].path.is_some())
            .collect();
        let Some(now) = selectable.iter().position(|&i| i >= self.selected) else {
            return;
        };
        let last = selectable.len().saturating_sub(1);
        let to = now.saturating_add_signed(by).min(last);
        let to = if by < 0 && now.checked_add_signed(by).is_none() {
            0
        } else {
            to
        };
        self.selected = selectable[to];
    }

    /// The lines to draw in a `width` by `height` area: a header, the list, and a line
    /// with the filter and the keys.
    pub fn frame(&mut self, width: usize, height: usize) -> Vec<String> {
        let rows = height.saturating_sub(2).max(1);
        let found = self.search.as_ref().map_or(0, |(_, _, s)| s.len());
        if self.built != Some((rows, found)) {
            self.rebuild(rows);
            self.built = Some((rows, found));
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + rows {
            self.scroll = self.selected + 1 - rows;
        }

        let mut lines = Vec::with_capacity(height);
        lines.push(paint(&self.colours.frame, &fit(&self.header(), width)));
        for i in self.scroll..self.scroll + rows {
            lines.push(self.items.get(i).map_or_else(String::new, |item| {
                self.draw_item(item, i == self.selected, width)
            }));
        }
        let footer = format!(
            "> {}\u{2588}   Enter pick  \u{2192} open  \u{2190} up  Alt-F files  Alt-H history  Esc",
            self.typed
        );
        lines.push(paint(&self.colours.status, &fit(&footer, width)));
        lines
    }

    fn header(&self) -> String {
        let place = match self.view {
            View::Tree => self.shown(&self.root),
            View::Drives => "Drives".to_owned(),
            View::History => "Folder history".to_owned(),
        };
        let mut header = format!(
            "{place}   [{}]",
            match self.filter.shows {
                Shows::Folders => "folders",
                Shows::Everything => "all",
            }
        );
        if self.filter.hidden {
            header.push_str(" +hidden");
        }
        if self.filter.ignored {
            header.push_str(" +ignored");
        }
        if self.picks > 0 {
            let _ = write!(header, "   {} picked", self.picks);
        }
        if !self.typed.is_empty()
            && let Some((_, _, search)) = &self.search
        {
            match search.status() {
                Status::Running => header.push_str("   searching\u{2026}"),
                Status::Cut => {
                    let _ = write!(header, "   search stopped at {} entries", search.len());
                }
                Status::Done => {}
            }
        }
        header
    }

    /// A folder as the header shows it: under the home folder as `~/…`.
    fn shown(&self, path: &Path) -> String {
        let text = cash_win32::path::render(path);
        if let Some(home) = &self.home {
            let home = cash_win32::path::render(home);
            if let Some(rest) = text
                .get(..home.len())
                .filter(|head| head.eq_ignore_ascii_case(&home))
                .and_then(|_| text.get(home.len()..))
            {
                if rest.is_empty() || rest.starts_with('/') {
                    return format!("~{rest}");
                }
            }
        }
        text
    }

    fn draw_item(&self, item: &Item, selected: bool, width: usize) -> String {
        let plain = format!("{}{}", item.prefix, item.text);
        if selected {
            let mut line = fit(&plain, width);
            let used: usize = line.chars().filter_map(char::width).sum();
            line.push_str(&" ".repeat(width.saturating_sub(used)));
            return paint(&self.colours.selected, &line);
        }
        let mut out = paint(&self.colours.dim, &item.prefix);
        let entry_colour = if item.path.is_some() {
            self.colours.entry(&item.text, item.folder)
        } else {
            self.colours.dim.clone()
        };
        let room = width.saturating_sub(item.prefix.chars().filter_map(char::width).sum());
        let text = fit(&item.text, room);
        // Runs of matched and unmatched letters, each in its colour.
        let mut run = String::new();
        let mut run_matched = false;
        for (i, c) in text.chars().enumerate() {
            let matched = item.positions.contains(&i);
            if matched != run_matched && !run.is_empty() {
                let sgr = if run_matched {
                    &self.colours.matched
                } else {
                    &entry_colour
                };
                out.push_str(&paint(sgr, &run));
                run.clear();
            }
            run_matched = matched;
            run.push(c);
        }
        if !run.is_empty() {
            let sgr = if run_matched {
                &self.colours.matched
            } else {
                &entry_colour
            };
            out.push_str(&paint(sgr, &run));
        }
        out
    }

    fn rebuild(&mut self, rows: usize) {
        let previous = self.items.get(self.selected).and_then(|i| i.path.clone());
        self.fuzzy.set(&self.typed);
        self.items = match self.view {
            View::Tree if self.typed.is_empty() => self.tree_items(rows),
            View::Tree => self.match_items(),
            View::Drives => self.drive_items(),
            View::History => self.history_items(),
        };
        let want = self.want.take().or(previous);
        self.selected = want
            .and_then(|want| {
                self.items
                    .iter()
                    .position(|i| i.path.as_ref() == Some(&want))
            })
            .or_else(|| self.items.iter().position(|i| i.path.is_some()))
            .unwrap_or(0);
    }

    fn tree_items(&mut self, rows: usize) -> Vec<Item> {
        let filter = self.filter;
        let (cache, ignores) = (&mut self.cache, &mut self.ignores);
        let mut read = |folder: &Path| -> Vec<Entry> {
            cache
                .entry(folder.to_path_buf())
                .or_insert_with(|| tree::read(folder, filter, ignores))
                .clone()
        };
        let top = read(&self.root);
        let lines = tree::layout(top, rows, &mut read);
        lines
            .into_iter()
            .map(|line| match line {
                Line::Entry {
                    entry,
                    depth,
                    last,
                    rails,
                } => Item {
                    prefix: branches(&rails, depth, Some(last)),
                    text: if entry.folder {
                        format!("{}/", entry.name)
                    } else {
                        entry.name
                    },
                    path: Some(entry.path),
                    folder: entry.folder,
                    positions: Vec::new(),
                },
                Line::More {
                    count,
                    depth,
                    rails,
                } => Item {
                    prefix: branches(&rails, depth, None),
                    text: format!("\u{2026} {count} more"),
                    path: None,
                    folder: false,
                    positions: Vec::new(),
                },
            })
            .collect()
    }

    fn match_items(&mut self) -> Vec<Item> {
        let wanted = (self.root.clone(), self.filter);
        if self
            .search
            .as_ref()
            .is_none_or(|(root, filter, _)| (root, filter) != (&wanted.0, &wanted.1))
        {
            self.search = Some((
                wanted.0.clone(),
                wanted.1,
                Search::start(&wanted.0, wanted.1),
            ));
        }
        let Some((_, _, search)) = &self.search else {
            return Vec::new();
        };
        let fuzzy = &mut self.fuzzy;
        search.with_found(|found| {
            let ranked = fuzzy.rank(found.iter().map(|f| f.relative.as_str()));
            ranked
                .into_iter()
                .take(1000)
                .filter_map(|m| {
                    let entry = found.get(m.index)?;
                    Some(Item {
                        path: Some(entry.path.clone()),
                        folder: entry.folder,
                        prefix: String::new(),
                        text: if entry.folder {
                            format!("{}/", entry.relative)
                        } else {
                            entry.relative.clone()
                        },
                        positions: m.positions,
                    })
                })
                .collect()
        })
    }

    fn drive_items(&mut self) -> Vec<Item> {
        let mut places: Vec<(String, PathBuf)> = cash_win32::sysinfo::logical_drives()
            .into_iter()
            .map(|drive| (format!("{drive}/"), PathBuf::from(format!("{drive}\\"))))
            .collect();
        if let Some(home) = &self.home {
            places.insert(0, ("~/".to_owned(), home.clone()));
        }
        self.listed(places)
    }

    fn history_items(&mut self) -> Vec<Item> {
        let places: Vec<(String, PathBuf)> = self
            .history
            .iter()
            .filter(|folder| folder.is_dir())
            .map(|folder| {
                (
                    format!("{}/", self.shown(folder).trim_end_matches('/')),
                    folder.clone(),
                )
            })
            .collect();
        self.listed(places)
    }

    /// Folders as a list, filtered by what was typed.
    fn listed(&mut self, places: Vec<(String, PathBuf)>) -> Vec<Item> {
        if self.typed.is_empty() {
            return places
                .into_iter()
                .map(|(text, path)| Item {
                    path: Some(path),
                    folder: true,
                    prefix: String::new(),
                    text,
                    positions: Vec::new(),
                })
                .collect();
        }
        let ranked = self
            .fuzzy
            .rank(places.iter().map(|(text, _)| text.as_str()));
        ranked
            .into_iter()
            .filter_map(|m| {
                let (text, path) = places.get(m.index)?;
                Some(Item {
                    path: Some(path.clone()),
                    folder: true,
                    prefix: String::new(),
                    text: text.clone(),
                    positions: m.positions,
                })
            })
            .collect()
    }
}

/// The tree branches before a line: for each level above, `│` where that level goes on,
/// then `├─` or `└─` for the line's own branch (`last`), or `└─` for a `… N more` line.
/// The root's own entries have none.
fn branches(rails: &[bool], depth: usize, last: Option<bool>) -> String {
    if depth <= 1 {
        return String::new();
    }
    let mut prefix: String = rails
        .iter()
        .skip(1)
        .map(|&on| if on { "\u{2502}  " } else { "   " })
        .collect();
    prefix.push_str(match last {
        Some(false) => "\u{251c}\u{2500} ",
        Some(true) | None => "\u{2514}\u{2500} ",
    });
    prefix
}

/// `text` cut to `width` columns, with `…` where it was cut.
fn fit(text: &str, width: usize) -> String {
    let total: usize = text.chars().filter_map(char::width).sum();
    if total <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker(root: &Path, shows: Shows, then: Then) -> Picker {
        Picker::new(Setup {
            root: root.to_path_buf(),
            typed: String::new(),
            shows,
            then,
            history: Vec::new(),
            home: None,
            colours: Colours::none(),
        })
    }

    /// The frame without escapes, trailing spaces trimmed.
    fn plain(picker: &mut Picker, height: usize) -> Vec<String> {
        picker
            .frame(60, height)
            .into_iter()
            .map(|line| strip(&line).trim_end().to_owned())
            .collect()
    }

    fn strip(text: &str) -> String {
        let mut out = String::new();
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

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for folder in ["alpha/one", "alpha/two", "beta"] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        dir
    }

    #[test]
    fn the_tree_shows_folders_with_their_levels() {
        let dir = sample();
        let mut picker = picker(dir.path(), Shows::Folders, Then::Run);
        let frame = plain(&mut picker, 8);
        assert_eq!(
            &frame[1..5],
            [
                "alpha/",
                "\u{251c}\u{2500} one/",
                "\u{2514}\u{2500} two/",
                "beta/"
            ],
            "{frame:#?}"
        );
        assert!(frame[0].contains("[folders]"), "{frame:#?}");
        assert!(frame.iter().all(|l| !l.contains("notes.txt")), "{frame:#?}");
    }

    #[test]
    fn enter_picks_the_selection_and_cd_closes() {
        let dir = sample();
        let mut picker = picker(dir.path(), Shows::Folders, Then::Run);
        picker.frame(60, 8);
        picker.key(Key::Down);
        assert_eq!(
            picker.key(Key::Enter),
            Outcome::Picked {
                path: dir.path().join("alpha").join("one"),
                folder: true,
                close: true,
            }
        );
    }

    #[test]
    fn right_and_left_move_the_root() {
        let dir = sample();
        let mut picker = picker(dir.path(), Shows::Folders, Then::Run);
        picker.frame(60, 8);
        picker.key(Key::Right);
        assert_eq!(picker.root(), dir.path().join("alpha"));
        let frame = plain(&mut picker, 8);
        assert_eq!(&frame[1..3], ["one/", "two/"], "{frame:#?}");
        picker.key(Key::Left);
        assert_eq!(picker.root(), dir.path());
        picker.frame(60, 8);
        // The folder left is selected again.
        assert_eq!(
            picker.key(Key::Enter),
            Outcome::Picked {
                path: dir.path().join("alpha"),
                folder: true,
                close: true,
            }
        );
    }

    #[test]
    fn files_show_with_alt_f_and_other_commands_stay_open() {
        let dir = sample();
        let mut picker = picker(dir.path(), Shows::Folders, Then::StayOpen);
        picker.key(Key::Files);
        let frame = plain(&mut picker, 10);
        assert!(frame.iter().any(|l| l == "notes.txt"), "{frame:#?}");
        picker.key(Key::End);
        assert_eq!(
            picker.key(Key::Enter),
            Outcome::Picked {
                path: dir.path().join("notes.txt"),
                folder: false,
                close: false,
            }
        );
        assert!(matches!(
            picker.key(Key::CtrlEnter),
            Outcome::Picked { close: true, .. }
        ));
    }

    #[test]
    fn typing_filters_below_the_root_and_esc_clears_then_closes() {
        let dir = sample();
        let mut picker = picker(dir.path(), Shows::Folders, Then::Run);
        picker.frame(60, 8);
        for c in "two".chars() {
            picker.key(Key::Char(c));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut frame = plain(&mut picker, 8);
        while picker.searching() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
            frame = plain(&mut picker, 8);
        }
        let frame_now = plain(&mut picker, 8);
        assert_eq!(frame_now[1], "alpha/two/", "{frame:#?} {frame_now:#?}");
        assert!(frame_now.last().unwrap().starts_with("> two"));
        assert_eq!(picker.key(Key::Esc), Outcome::Continue);
        assert_eq!(picker.key(Key::Esc), Outcome::Closed);
    }

    #[test]
    fn a_long_folder_scrolls_down_to_its_last_entry() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..30 {
            std::fs::create_dir(dir.path().join(format!("f{i:02}"))).unwrap();
        }
        let mut picker = picker(dir.path(), Shows::Folders, Then::Run);
        // Six rows of list: Down past the bottom scrolls rather than stopping.
        picker.frame(60, 8);
        for _ in 0..7 {
            picker.key(Key::Down);
        }
        let frame = plain(&mut picker, 8);
        assert!(frame.iter().any(|l| l == "f07/"), "{frame:#?}");
        assert!(frame.iter().all(|l| !l.contains("more")), "{frame:#?}");
        picker.key(Key::PageDown);
        picker.key(Key::End);
        let frame = plain(&mut picker, 8);
        assert_eq!(frame[6], "f29/", "{frame:#?}");
        assert_eq!(
            picker.key(Key::Enter),
            Outcome::Picked {
                path: dir.path().join("f29"),
                folder: true,
                close: true,
            }
        );
    }

    #[test]
    fn long_lines_are_cut_with_an_ellipsis() {
        assert_eq!(fit("abcdef", 4), "abc\u{2026}");
        assert_eq!(fit("abc", 4), "abc");
    }
}
