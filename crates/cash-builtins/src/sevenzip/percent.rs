//! 7-Zip's progress line (`CPercentPrinter`): the share done, the items done, what is
//! being done and to what, written over itself at most every 200 ms and wiped before
//! anything else is written.

use std::time::{Duration, Instant};

/// How often the line may change.
const TICK: Duration = Duration::from_millis(200);
/// The longest line, a column short of 80.
const MAX_LEN: usize = 79;
/// A total not known.
pub(super) const UNKNOWN: u64 = u64::MAX;

/// What the line shows (`CPercentPrinterState`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct State {
    pub(super) completed: u64,
    pub(super) total: u64,
    pub(super) files: u64,
    pub(super) command: String,
    pub(super) file_name: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            completed: 0,
            total: UNKNOWN,
            files: 0,
            command: String::new(),
            file_name: String::new(),
        }
    }
}

impl State {
    /// `ClearCurState`.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// The line and what was last shown of it.
#[derive(Debug, Default)]
pub(super) struct Percent {
    pub(super) state: State,
    printed: String,
    printed_state: State,
    printed_percents: String,
    prev_tick: Option<Instant>,
}

impl Percent {
    /// `ClosePrint`: what wipes the line shown, if one is.
    pub(super) fn close(&mut self) -> Option<String> {
        if self.printed.is_empty() {
            return None;
        }
        let width = self.printed.chars().count();
        self.printed.clear();
        Some(format!("\r{}\r", " ".repeat(width)))
    }

    /// `GetPercents`: the share done, four columns wide; megabytes done when the total
    /// is not known.
    fn percents(&self) -> String {
        let s = &self.state;
        let text = if s.total == UNKNOWN || (s.total == 0 && s.completed != 0) {
            format!("{}M", s.completed >> 20)
        } else if s.total != 0 {
            let share = u128::from(s.completed) * 100 / u128::from(s.total);
            format!("{share}%")
        } else {
            "0%".to_owned()
        };
        format!("{text:>4}")
    }

    /// `Print`: what to write for the line to show the state now, if it changes and
    /// 200 ms passed since it last did.
    pub(super) fn print(&mut self) -> Option<String> {
        let now = Instant::now();
        let mut only_percents = false;
        if !self.printed.is_empty() {
            if self.prev_tick.is_some_and(|t| now.duration_since(t) < TICK) {
                return None;
            }
            let (shown, st) = (&self.printed_state, &self.state);
            if shown.command == st.command
                && shown.file_name == st.file_name
                && shown.files == st.files
            {
                if shown.total == st.total && shown.completed == st.completed {
                    return None;
                }
                only_percents = true;
            }
        }
        let mut line = self.percents();
        if only_percents && line == self.printed_percents {
            return None;
        }
        self.printed_percents.clone_from(&line);
        let st = &self.state;
        if st.files != 0 {
            line.push(' ');
            line.push_str(&st.files.to_string());
        }
        if !st.command.is_empty() {
            line.push(' ');
            line.push_str(&st.command);
        }
        let used = line.chars().count();
        if !st.file_name.is_empty() && used < MAX_LEN {
            line.push(' ');
            line.push_str(&fitted(&st.file_name, MAX_LEN - used - 1));
        }
        let text = if self.printed == line {
            None
        } else {
            let mut out = self.close().unwrap_or_default();
            out.push_str(&line);
            self.printed = line;
            Some(out)
        };
        self.printed_state = self.state.clone();
        self.prev_tick = Some(now);
        text
    }
}

/// A name in `room` columns: whole, else its middle given up for ` . ` an eighth at a
/// time, else nothing.
fn fitted(name: &str, room: usize) -> String {
    let chars: Vec<char> = name.replace('\\', "/").chars().collect();
    if chars.len() <= room {
        return chars.into_iter().collect();
    }
    let mut len = chars.len();
    while len != 0 {
        len -= (len / 8).max(1);
        let half = len / 2;
        let mut short: String = chars[..half].iter().collect();
        short.push_str(" . ");
        short.extend(&chars[chars.len() - (len - half)..]);
        if short.chars().count() <= room {
            return short;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_shows_the_share_the_items_and_the_name() {
        let mut p = Percent::default();
        p.state.total = 200;
        p.state.completed = 50;
        p.state.files = 3;
        p.state.command = "+".to_owned();
        p.state.file_name = "d/a.txt".to_owned();
        assert_eq!(p.print().as_deref(), Some(" 25% 3 + d/a.txt"));
        // Within the tick nothing changes.
        p.state.completed = 100;
        assert_eq!(p.print(), None);
        assert_eq!(p.close().as_deref(), Some("\r                \r"));
        assert_eq!(p.close(), None);
    }

    #[test]
    fn an_unknown_total_counts_megabytes() {
        let mut p = Percent::default();
        p.state.completed = 5 << 20;
        p.state.command = "Scan".to_owned();
        assert_eq!(p.print().as_deref(), Some("  5M Scan"));
    }

    #[test]
    fn long_names_lose_their_middle() {
        let name = "a".repeat(50) + &"b".repeat(50);
        let short = fitted(&name, 60);
        assert!(short.chars().count() <= 60, "{short}");
        assert!(short.starts_with("aaa") && short.ends_with("bbb") && short.contains(" . "));
        assert_eq!(fitted("abc", 0), "");
    }
}
