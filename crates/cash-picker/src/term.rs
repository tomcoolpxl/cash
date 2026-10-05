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

/// Runs `picker` until it closes, drawing on `out`. `pick` gets each pick (its path,
/// whether it is a folder, whether the picker closes after it).
///
/// # Errors
///
/// When the terminal cannot be drawn on or read from.
pub fn run<W: Write>(
    picker: &mut Picker,
    out: &mut W,
    height: Option<&str>,
    mut pick: impl FnMut(&Path, bool, bool),
) -> io::Result<()> {
    let was_raw = terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw {
        terminal::enable_raw_mode()?;
    }
    let result = run_raw(picker, out, height, &mut pick);
    if !was_raw {
        let _ = terminal::disable_raw_mode();
    }
    result
}

fn run_raw<W: Write>(
    picker: &mut Picker,
    out: &mut W,
    height: Option<&str>,
    pick: &mut dyn FnMut(&Path, bool, bool),
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
                            pick(&path, folder, close);
                            if close {
                                return Ok(());
                            }
                            picker.picked();
                        }
                    }
                }
                Event::Resize(new_columns, _) => area.columns = usize::from(new_columns),
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
        })
    }

    fn draw<W: Write>(&self, out: &mut W, picker: &mut Picker) -> io::Result<()> {
        let lines = picker.frame(self.columns.saturating_sub(1).max(1), self.rows);
        queue!(out, cursor::Hide)?;
        for (i, line) in lines.iter().enumerate() {
            let row = self.top + u16::try_from(i).unwrap_or(u16::MAX);
            queue!(
                out,
                cursor::MoveTo(0, row),
                terminal::Clear(terminal::ClearType::CurrentLine)
            )?;
            write!(out, "{line}")?;
        }
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
