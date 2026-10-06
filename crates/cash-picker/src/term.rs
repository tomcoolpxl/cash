//! The picker on the terminal: rows set aside below the command line, a frame drawn in
//! them for each change, keys read until it closes, and the rows erased again (spec D73).
//!
//! The rows are made by writing line feeds, which scroll the screen up when the command
//! line is near the bottom, as fzf's `--height` does. A window too small for
//! [`MIN_ROWS`] gets the picker on the alternate screen, as broot is.

use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{cursor, queue, terminal};

use crate::ui::{Key, Outcome, Picker};

/// The fewest rows the picker takes inline.
pub const MIN_ROWS: usize = 8;

/// The rows the picker takes in a window of `total` rows: `CASH_PICKER_HEIGHT` as a
/// percentage (`50%`) or a row count (`15`), 40% without one; at least [`MIN_ROWS`],
/// and never the command line's own row.
#[must_use]
pub fn rows_for(setting: Option<&str>, total: usize) -> usize {
    let setting = setting.map(str::trim).filter(|s| !s.is_empty());
    let wanted = match setting {
        Some(percent) if percent.ends_with('%') => percent
            .trim_end_matches('%')
            .parse::<usize>()
            .map_or(total * 2 / 5, |p| total * p.min(100) / 100),
        Some(rows) => rows.parse::<usize>().unwrap_or(total * 2 / 5),
        None => total * 2 / 5,
    };
    wanted.max(MIN_ROWS).min(total.saturating_sub(1))
}

/// Runs `picker` until it closes, drawing on `out`.
///
/// `pick` gets each pick (its path, whether it is a folder, whether the picker closes
/// after it), and may return the command line as it now reads from the word being
/// replaced on, which is drawn on the line while the picker stays open; `echo_from` is
/// how many columns before the cursor that word starts, `None` where the line is not to
/// be drawn.
///
/// # Errors
///
/// When the terminal cannot be drawn on or read from.
pub fn run<W: Write>(
    picker: &mut Picker,
    out: &mut W,
    height: Option<&str>,
    echo_from: Option<usize>,
    mut pick: impl FnMut(&Path, bool, bool) -> Option<String>,
) -> io::Result<()> {
    let was_raw = terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw {
        terminal::enable_raw_mode()?;
    }
    let result = run_raw(picker, out, height, echo_from, &mut pick);
    if !was_raw {
        let _ = terminal::disable_raw_mode();
    }
    result
}

fn run_raw<W: Write>(
    picker: &mut Picker,
    out: &mut W,
    height: Option<&str>,
    echo_from: Option<usize>,
    pick: &mut dyn FnMut(&Path, bool, bool) -> Option<String>,
) -> io::Result<()> {
    let (columns, total) = terminal::size()?;
    let (columns, total) = (usize::from(columns), usize::from(total));
    let full_screen = total < MIN_ROWS + 1;
    let mut area = if full_screen {
        queue!(out, terminal::EnterAlternateScreen)?;
        Area {
            top: 0,
            rows: total,
            columns,
            back: None,
            shown: Vec::new(),
        }
    } else {
        Area::below(out, rows_for(height, total), columns, total)?
    };

    let result = (|| -> io::Result<()> {
        loop {
            area.draw(out, picker)?;
            // While a search runs, wake to show what it found.
            let wait = if picker.searching() {
                Duration::from_millis(120)
            } else {
                Duration::from_secs(3600)
            };
            if !event::poll(wait)? {
                continue;
            }
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let Some(key) = translate(key) else {
                        continue;
                    };
                    match picker.key(key) {
                        Outcome::Continue => {}
                        Outcome::Closed => return Ok(()),
                        Outcome::Picked {
                            path,
                            folder,
                            close,
                        } => {
                            let line = pick(&path, folder, close);
                            if close {
                                return Ok(());
                            }
                            if let (Some(line), Some(from)) = (line, echo_from) {
                                area.echo(out, from, &line)?;
                            }
                            picker.picked();
                        }
                    }
                }
                Event::Resize(new_columns, new_rows) => {
                    area.resize(out, usize::from(new_columns), usize::from(new_rows), height)?;
                }
                _ => {}
            }
        }
    })();

    area.erase(out)?;
    if full_screen {
        queue!(out, terminal::LeaveAlternateScreen)?;
    }
    queue!(out, cursor::Show)?;
    out.flush()?;
    result
}

/// The rows the picker draws in.
struct Area {
    top: u16,
    rows: usize,
    columns: usize,
    /// Where the cursor goes back to on closing, for an inline picker.
    back: Option<(u16, u16)>,
    /// The lines on screen, so that a frame redraws only those that changed.
    shown: Vec<String>,
}

impl Area {
    /// `rows` rows below the cursor's line, scrolling the screen up as far as needed.
    fn below<W: Write>(out: &mut W, rows: usize, columns: usize, total: usize) -> io::Result<Self> {
        let (column, row) = cursor::position()?;
        let bottom = total.saturating_sub(1);
        let scroll = (usize::from(row) + rows).saturating_sub(bottom);
        // Line feeds from the bottom row scroll; the cursor's line moves up with them.
        write!(out, "{}", "\n".repeat(rows))?;
        let line = u16::try_from(usize::from(row).saturating_sub(scroll)).unwrap_or(0);
        Ok(Self {
            top: line + 1,
            rows,
            columns,
            back: Some((column, line)),
            shown: Vec::new(),
        })
    }

    /// Fits the area to a window now `columns` by `total`: its height worked out again from
    /// `height`, and, inline, the screen scrolled up when it no longer fits below the
    /// command line. What was drawn is erased, and the next frame draws everything.
    fn resize<W: Write>(
        &mut self,
        out: &mut W,
        columns: usize,
        total: usize,
        height: Option<&str>,
    ) -> io::Result<()> {
        self.columns = columns;
        self.shown.clear();
        let Some((column, line)) = self.back else {
            // Full screen: all of it.
            self.rows = total;
            return queue!(out, terminal::Clear(terminal::ClearType::All));
        };
        queue!(
            out,
            cursor::MoveTo(0, self.top),
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )?;
        let rows = rows_for(height, total);
        let bottom = total.saturating_sub(1);
        let last = usize::from(self.top) + rows.saturating_sub(1);
        if last > bottom {
            // Line feeds on the bottom row scroll; the command line moves up with them.
            let scroll = last - bottom;
            queue!(
                out,
                cursor::MoveTo(0, u16::try_from(bottom).unwrap_or(u16::MAX))
            )?;
            write!(out, "{}", "\n".repeat(scroll))?;
            let scroll = u16::try_from(scroll).unwrap_or(u16::MAX);
            self.top = self.top.saturating_sub(scroll);
            self.back = Some((column, line.saturating_sub(scroll)));
        }
        self.rows = rows;
        Ok(())
    }

    /// Draws a frame: only the lines that differ from those on screen, each written
    /// over the old one and the rest of its row erased after it, never cleared first,
    /// and all of it as one synchronized update (mode 2026), which the terminal shows
    /// at once. Clearing every row and writing it again, several times a second while
    /// a search ran, made the picker flicker (the user, 2026-10-06).
    fn draw<W: Write>(&mut self, out: &mut W, picker: &mut Picker) -> io::Result<()> {
        let lines = picker.frame(self.columns.saturating_sub(1).max(1), self.rows);
        if lines == self.shown {
            return Ok(());
        }
        write!(out, "\x1b[?2026h")?;
        queue!(out, cursor::Hide)?;
        for (i, line) in lines.iter().enumerate() {
            if self.shown.get(i) == Some(line) {
                continue;
            }
            let row = self.top + u16::try_from(i).unwrap_or(u16::MAX);
            queue!(out, cursor::MoveTo(0, row))?;
            write!(out, "{line}")?;
            queue!(out, terminal::Clear(terminal::ClearType::UntilNewLine))?;
        }
        write!(out, "\x1b[?2026l")?;
        self.shown = lines;
        out.flush()
    }

    /// Draws `line` on the command line's row, from `from` columns before the cursor,
    /// so that a pick shows there while the picker stays open. The line editor draws
    /// the line again once the picker closes.
    fn echo<W: Write>(&self, out: &mut W, from: usize, line: &str) -> io::Result<()> {
        let Some((column, row)) = self.back else {
            return Ok(());
        };
        let Some(start) = usize::from(column).checked_sub(from) else {
            return Ok(());
        };
        // What fits on the row; a longer line would run into the picker's rows.
        let room = self.columns.saturating_sub(start + 1);
        let mut shown = String::new();
        let mut used = 0;
        for c in line.chars() {
            let width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + width > room {
                break;
            }
            shown.push(c);
            used += width;
        }
        queue!(
            out,
            cursor::MoveTo(u16::try_from(start).unwrap_or(0), row),
            terminal::Clear(terminal::ClearType::UntilNewLine)
        )?;
        write!(out, "{shown}")?;
        out.flush()
    }

    fn erase<W: Write>(&self, out: &mut W) -> io::Result<()> {
        queue!(
            out,
            cursor::MoveTo(0, self.top),
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )?;
        if let Some((column, row)) = self.back {
            queue!(out, cursor::MoveTo(column, row))?;
        }
        Ok(())
    }
}

/// The picker's key for a key event; `None` for one it ignores.
const fn translate(key: KeyEvent) -> Option<Key> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    Some(match key.code {
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Enter if control => Key::CtrlEnter,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Char('c' | 'g') if control => Key::Esc,
        KeyCode::Char('f') if alt => Key::Files,
        KeyCode::Char('h') if alt => Key::History,
        KeyCode::Char('.') if alt => Key::Hidden,
        KeyCode::Char('i') if alt => Key::Ignored,
        KeyCode::Char('s') if alt => Key::Sort,
        KeyCode::Char(c) if !control && !alt => Key::Char(c),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_height_is_40_percent_or_as_set() {
        assert_eq!(rows_for(None, 50), 20);
        assert_eq!(rows_for(Some("50%"), 50), 25);
        assert_eq!(rows_for(Some("15"), 50), 15);
        // At least eight rows, and never the whole window.
        assert_eq!(rows_for(Some("2"), 50), MIN_ROWS);
        assert_eq!(rows_for(Some("100%"), 50), 49);
        assert_eq!(rows_for(Some("junk"), 50), 20);
    }

    #[test]
    fn a_frame_draws_only_what_changed_and_never_clears_a_row_first() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("one")).unwrap();
        let mut picker = Picker::new(crate::ui::Setup {
            root: dir.path().to_path_buf(),
            typed: String::new(),
            shows: crate::context::Shows::Folders,
            then: crate::context::Then::Run,
            history: Vec::new(),
            home: None,
            colours: crate::colours::Colours::none(),
        });
        let mut area = Area {
            top: 1,
            rows: 8,
            columns: 60,
            back: None,
            shown: Vec::new(),
        };
        let mut first = Vec::new();
        area.draw(&mut first, &mut picker).unwrap();
        let first = String::from_utf8(first).unwrap();
        assert!(first.contains("one/"), "{first:?}");
        // A whole-line clear (ESC [ 2 K) blanks a row before it is written again.
        assert!(!first.contains("\x1b[2K"), "{first:?}");
        assert!(first.starts_with("\x1b[?2026h") && first.ends_with("\x1b[?2026l"));

        let mut again = Vec::new();
        area.draw(&mut again, &mut picker).unwrap();
        assert!(again.is_empty(), "{:?}", String::from_utf8_lossy(&again));

        picker.key(Key::Char('o'));
        let mut changed = Vec::new();
        area.draw(&mut changed, &mut picker).unwrap();
        let changed = String::from_utf8(changed).unwrap();
        assert!(changed.contains("> o"), "{changed:?}");
        assert!(changed.len() < first.len(), "{changed:?}");
    }

    #[test]
    fn a_resize_works_out_the_height_again_and_makes_room() {
        let mut area = Area {
            top: 21,
            rows: 16,
            columns: 80,
            back: Some((4, 20)),
            shown: vec!["old".to_owned()],
        };
        // From 40 rows to 30: 40% is 12 rows, from row 21 down to 32, past row 29.
        let mut out = Vec::new();
        area.resize(&mut out, 100, 30, None).unwrap();
        assert_eq!((area.rows, area.columns), (12, 100));
        assert_eq!((area.top, area.back), (18, Some((4, 17))));
        assert!(area.shown.is_empty(), "the next frame draws every line");
        let out = String::from_utf8(out).unwrap();
        assert!(out.ends_with("\n\n\n"), "{out:?}");

        // Taller again: room below, nothing scrolls.
        let mut out = Vec::new();
        area.resize(&mut out, 100, 50, Some("15")).unwrap();
        assert_eq!((area.rows, area.top), (15, 18));
        assert!(!String::from_utf8(out).unwrap().contains('\n'));
    }

    #[test]
    fn keys_translate_and_releases_are_left_to_the_caller() {
        let key = |code, modifiers| translate(KeyEvent::new(code, modifiers));
        assert_eq!(key(KeyCode::Char('f'), KeyModifiers::ALT), Some(Key::Files));
        assert_eq!(
            key(KeyCode::Char('f'), KeyModifiers::NONE),
            Some(Key::Char('f'))
        );
        assert_eq!(
            key(KeyCode::Enter, KeyModifiers::CONTROL),
            Some(Key::CtrlEnter)
        );
        assert_eq!(
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some(Key::Esc)
        );
        assert_eq!(key(KeyCode::F(5), KeyModifiers::NONE), None);
    }
}
