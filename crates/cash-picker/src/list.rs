//! A list to pick one line from, below the command line as Alt-E's picker is drawn:
//! Ctrl-R's history (spec D79) and `z -i`'s folders (D80).
//!
//! The lines come newest or best first, each with its age. Typing filters them, the
//! best matches first and, among equals, the order they came in. Where there is more
//! than one list (Ctrl-R's whole history and this folder's), the switch key shows the
//! next. As [`crate::ui`], there is no terminal here.

use std::fmt::Write as _;
use std::time::SystemTime;

use unicode_width::UnicodeWidthChar as _;

use crate::colours::{Colours, paint};
use crate::filter::Fuzzy;
use crate::ui::{age, fit};

/// A line to pick.
#[derive(Clone, Debug)]
pub struct Line {
    /// What it says; line ends and tabs are shown as `⏎` and a space.
    pub text: String,
    /// When it was made, shown as an age.
    pub when: Option<SystemTime>,
}

/// One of the lists.
#[derive(Clone, Debug)]
pub struct Scope {
    /// The header's name for it.
    pub title: String,
    /// Its lines, in the order to show them.
    pub lines: Vec<Line>,
}

/// How a list starts.
pub struct Setup {
    /// The lists, the first shown first.
    pub scopes: Vec<Scope>,
    /// The filter it opens with.
    pub typed: String,
    /// The keys the bottom line names.
    pub hints: String,
    /// The colours to draw in.
    pub colours: Colours,
    /// Whether the lines are paths, a match after a `/` weighing more.
    pub paths: bool,
}

/// A key the list acts on.
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
    /// Pick.
    Enter,
    /// Pick the other way (Ctrl-R's: run it at once).
    Tab,
    /// Clear the filter, or close.
    Esc,
    /// Take a letter off the filter.
    Backspace,
    /// A letter of the filter.
    Char(char),
    /// Show the next list.
    Switch,
}

/// What a key led to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing for the caller to do; draw the next frame.
    Continue,
    /// A line was picked: its list, its index there, and whether by Tab.
    Picked {
        /// The list it is in.
        scope: usize,
        /// Its index in that list's lines.
        index: usize,
        /// Whether Tab picked it rather than Enter.
        tab: bool,
    },
    /// Closed without a pick.
    Closed,
}

/// A line shown: its index in the list, and the letters the filter matched.
struct Shown {
    index: usize,
    positions: Vec<usize>,
}

/// The list.
pub struct List {
    scopes: Vec<Scope>,
    /// Each list's lines as shown, one character for each of the text's.
    texts: Vec<Vec<String>>,
    scope: usize,
    typed: String,
    fuzzy: Fuzzy,
    hints: String,
    colours: Colours,
    shown: Vec<Shown>,
    selected: usize,
    scroll: usize,
    built: bool,
}

/// `text` as a line shows it: a line end as `⏎`, a tab or carriage return as a space,
/// each one character for one, so the filter's positions stay the text's.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' => '\u{23ce}',
            '\t' | '\r' => ' ',
            c => c,
        })
        .collect()
}

impl List {
    /// A list as `setup` says.
    #[must_use]
    pub fn new(setup: Setup) -> Self {
        let texts = setup
            .scopes
            .iter()
            .map(|scope| {
                scope
                    .lines
                    .iter()
                    .map(|line| one_line(&line.text))
                    .collect()
            })
            .collect();
        Self {
            scopes: setup.scopes,
            texts,
            scope: 0,
            typed: setup.typed,
            fuzzy: if setup.paths {
                Fuzzy::default()
            } else {
                Fuzzy::plain()
            },
            hints: setup.hints,
            colours: setup.colours,
            shown: Vec::new(),
            selected: 0,
            scroll: 0,
            built: false,
        }
    }

    /// The list shown, by its index in the setup's.
    #[must_use]
    pub const fn scope(&self) -> usize {
        self.scope
    }

    /// A line, by its list and its index there, as a pick gives them.
    #[must_use]
    pub fn line(&self, scope: usize, index: usize) -> Option<&Line> {
        self.scopes.get(scope)?.lines.get(index)
    }

    /// Acts on a key.
    pub fn key(&mut self, key: Key) -> Outcome {
        self.build();
        match key {
            Key::Up => self.step(-1),
            Key::Down => self.step(1),
            Key::PageUp => self.step(-10),
            Key::PageDown => self.step(10),
            Key::Home => self.step(isize::MIN / 2),
            Key::End => self.step(isize::MAX / 2),
            Key::Enter | Key::Tab => {
                if let Some(shown) = self.shown.get(self.selected) {
                    return Outcome::Picked {
                        scope: self.scope,
                        index: shown.index,
                        tab: key == Key::Tab,
                    };
                }
            }
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
            Key::Switch => {
                if self.scopes.len() > 1 {
                    self.scope = (self.scope + 1) % self.scopes.len();
                    self.built = false;
                    self.selected = 0;
                    self.scroll = 0;
                }
            }
        }
        Outcome::Continue
    }

    fn set_typed(&mut self, typed: String) {
        self.typed = typed;
        self.built = false;
        self.selected = 0;
        self.scroll = 0;
    }

    fn step(&mut self, by: isize) {
        let last = self.shown.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(by).min(last);
    }

    /// The lines shown: every one in order with nothing typed, else the matches, best
    /// first.
    fn build(&mut self) {
        if self.built {
            return;
        }
        self.built = true;
        let texts = &self.texts[self.scope];
        if self.typed.trim().is_empty() {
            self.shown = (0..texts.len())
                .map(|index| Shown {
                    index,
                    positions: Vec::new(),
                })
                .collect();
            return;
        }
        self.fuzzy.set(&self.typed);
        self.shown = self
            .fuzzy
            .rank(texts.iter().map(String::as_str))
            .into_iter()
            .map(|found| Shown {
                index: found.index,
                positions: found.positions,
            })
            .collect();
    }

    /// The lines to draw in a `width` by `height` area: a header, the list, and a line
    /// with the filter and the keys.
    pub fn frame(&mut self, width: usize, height: usize) -> Vec<String> {
        self.build();
        let rows = height.saturating_sub(2).max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + rows {
            self.scroll = self.selected + 1 - rows;
        }
        let scope = &self.scopes[self.scope];
        let mut lines = Vec::with_capacity(height);
        let mut header = format!("{}   {}", scope.title, self.shown.len());
        if self.scopes.len() > 1 {
            let next = &self.scopes[(self.scope + 1) % self.scopes.len()].title;
            let _ = write!(header, "   Ctrl-R: {next}");
        }
        lines.push(paint(&self.colours.frame, &fit(&header, width)));
        for i in self.scroll..self.scroll + rows {
            lines.push(self.shown.get(i).map_or_else(String::new, |shown| {
                self.draw(shown, i == self.selected, width)
            }));
        }
        let footer = format!("> {}\u{2588}   {}", self.typed, self.hints);
        lines.push(paint(&self.colours.status, &fit(&footer, width)));
        lines
    }

    fn draw(&self, shown: &Shown, selected: bool, width: usize) -> String {
        let line = &self.scopes[self.scope].lines[shown.index];
        let text = &self.texts[self.scope][shown.index];
        let when = line.when.map(age).unwrap_or_default();
        let prefix = format!("{when:>4}  ");
        if selected {
            let mut row = fit(&format!("{prefix}{text}"), width);
            let used: usize = row.chars().filter_map(|c| c.width()).sum();
            row.push_str(&" ".repeat(width.saturating_sub(used)));
            return paint(&self.colours.selected, &row);
        }
        let mut out = paint(&self.colours.dim, &prefix);
        let room = width.saturating_sub(prefix.chars().count());
        let text = fit(text, room);
        // Runs of matched and unmatched letters, each in its colour.
        let mut run = String::new();
        let mut run_matched = false;
        for (i, c) in text.chars().enumerate() {
            let matched = shown.positions.binary_search(&i).is_ok();
            if matched != run_matched && !run.is_empty() {
                out.push_str(&self.paint_run(&run, run_matched));
                run.clear();
            }
            run_matched = matched;
            run.push(c);
        }
        if !run.is_empty() {
            out.push_str(&self.paint_run(&run, run_matched));
        }
        out
    }

    fn paint_run(&self, run: &str, matched: bool) -> String {
        if matched {
            paint(&self.colours.matched, run)
        } else {
            run.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(texts: &[&str]) -> Vec<Line> {
        texts
            .iter()
            .map(|text| Line {
                text: (*text).to_owned(),
                when: None,
            })
            .collect()
    }

    fn list(scopes: Vec<Scope>, typed: &str) -> List {
        List::new(Setup {
            scopes,
            typed: typed.to_owned(),
            hints: "Enter edit".to_owned(),
            colours: Colours::none(),
            paths: false,
        })
    }

    /// The rows of the list, without the reverse video the selected one keeps when
    /// nothing else is coloured.
    fn shown(list: &mut List) -> Vec<String> {
        list.frame(80, 12)[1..11]
            .iter()
            .map(|line| {
                line.replace("\x1b[7m", "")
                    .replace("\x1b[0m", "")
                    .trim()
                    .to_owned()
            })
            .filter(|line| !line.is_empty())
            .collect()
    }

    #[test]
    fn nothing_typed_shows_every_line_in_order() {
        let mut list = list(
            vec![Scope {
                title: "History".to_owned(),
                lines: lines(&["git status", "cargo build", "ls"]),
            }],
            "",
        );
        assert_eq!(shown(&mut list), ["git status", "cargo build", "ls"]);
        assert_eq!(list.frame(80, 12)[0], "History   3");
    }

    #[test]
    fn typing_filters_and_enter_or_tab_picks() {
        let mut list = list(
            vec![Scope {
                title: "History".to_owned(),
                lines: lines(&["git status", "cargo build", "git commit"]),
            }],
            "git",
        );
        assert_eq!(shown(&mut list), ["git status", "git commit"]);
        list.key(Key::Down);
        assert_eq!(
            list.key(Key::Enter),
            Outcome::Picked {
                scope: 0,
                index: 2,
                tab: false
            }
        );
        assert_eq!(
            list.key(Key::Tab),
            Outcome::Picked {
                scope: 0,
                index: 2,
                tab: true
            }
        );
        // Esc clears the filter, then closes.
        assert_eq!(list.key(Key::Esc), Outcome::Continue);
        assert_eq!(shown(&mut list).len(), 3);
        assert_eq!(list.key(Key::Esc), Outcome::Closed);
    }

    #[test]
    fn the_switch_shows_the_next_list() {
        let mut list = list(
            vec![
                Scope {
                    title: "History".to_owned(),
                    lines: lines(&["a", "b"]),
                },
                Scope {
                    title: "Here".to_owned(),
                    lines: lines(&["b"]),
                },
            ],
            "",
        );
        assert!(list.frame(80, 12)[0].ends_with("Ctrl-R: Here"));
        list.key(Key::Switch);
        assert_eq!(list.scope(), 1);
        assert_eq!(shown(&mut list), ["b"]);
        assert_eq!(
            list.key(Key::Enter),
            Outcome::Picked {
                scope: 1,
                index: 0,
                tab: false
            }
        );
    }

    #[test]
    fn a_command_of_several_lines_shows_on_one() {
        let mut list = list(
            vec![Scope {
                title: "History".to_owned(),
                lines: lines(&["for x in 1\ndo echo\tx\ndone"]),
            }],
            "",
        );
        assert_eq!(
            shown(&mut list),
            ["for x in 1\u{23ce}do echo x\u{23ce}done"]
        );
    }

    #[test]
    fn nothing_to_pick_is_no_pick() {
        let mut list = list(
            vec![Scope {
                title: "History".to_owned(),
                lines: Vec::new(),
            }],
            "",
        );
        assert_eq!(list.key(Key::Enter), Outcome::Continue);
    }
}
