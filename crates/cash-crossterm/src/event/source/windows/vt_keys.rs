//! cash: keys that reached the console as VT text (patch 2 in CASH-PATCHES.md).
//!
//! A program that turns on `ENABLE_VIRTUAL_TERMINAL_INPUT` gets keys as the text a
//! terminal sends: Enter as `\r`, Backspace as `\x7f`, an arrow as `ESC [ A`. The console
//! turns each key into that text as it arrives, so keys typed ahead while such a program
//! ran are still text once it has exited and the shell has turned VT input off again:
//! key-down records with no virtual key and no scan code, one per character. Upstream
//! finds no key in them, so Enter, Tab and Escape are dropped, and DEL, `[` and `A` are
//! typed. This decodes them the way crossterm decodes a terminal's bytes on Unix.

use crate::event::{KeyCode, KeyEvent, KeyModifiers};

const ESC: u16 = 0x1b;

/// What has been read of a key sequence that is not yet complete.
#[derive(Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Idle,
    /// An escape: the start of a sequence, Alt with the next key, or the Escape key.
    Escape,
    /// `ESC O`: one more character names the key.
    Ss3,
    /// `ESC [` and the parameters and intermediates since.
    Csi(String),
}

/// Decodes VT text one character at a time.
#[derive(Debug, Default)]
pub(super) struct VtKeys {
    state: State,
}

impl VtKeys {
    /// Whether a sequence has begun and is waiting for more.
    pub(super) fn is_open(&self) -> bool {
        self.state != State::Idle
    }

    /// Takes the next character; returns the key it completes, if any.
    pub(super) fn feed(&mut self, ch: u16) -> Option<KeyEvent> {
        match std::mem::take(&mut self.state) {
            State::Idle if ch == ESC => {
                self.state = State::Escape;
                None
            }
            State::Idle => single(ch),
            State::Escape => match ch {
                0x5b => {
                    self.state = State::Csi(String::new());
                    None
                }
                0x4f => {
                    self.state = State::Ss3;
                    None
                }
                // Two escapes are one Escape key, as on Unix.
                ESC => Some(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                _ => single(ch).map(|mut key| {
                    key.modifiers |= KeyModifiers::ALT;
                    key
                }),
            },
            State::Ss3 => ss3(ch),
            State::Csi(mut body) => match ch {
                0x20..=0x3f => {
                    body.push(char::from(ch as u8));
                    self.state = State::Csi(body);
                    None
                }
                0x40..=0x7e => csi(&body, ch as u8),
                // Not part of a sequence: what came so far is dropped.
                _ => self.feed(ch),
            },
        }
    }

    /// Ends a sequence that nothing more follows: a lone escape is the Escape key, and
    /// `ESC [` or `ESC O` is Alt with that character. The rest of an unfinished sequence
    /// is dropped.
    pub(super) fn flush(&mut self) -> Option<KeyEvent> {
        match std::mem::take(&mut self.state) {
            State::Idle => None,
            State::Escape => Some(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            State::Ss3 => Some(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::ALT)),
            State::Csi(body) if body.is_empty() => {
                Some(KeyEvent::new(KeyCode::Char('['), KeyModifiers::ALT))
            }
            State::Csi(_) => None,
        }
    }
}

/// One character on its own: Enter, Tab and Backspace by the text a terminal sends for
/// them, and Ctrl with a letter for the other control characters.
fn single(ch: u16) -> Option<KeyEvent> {
    let (code, modifiers) = match ch {
        0x0d => (KeyCode::Enter, KeyModifiers::NONE),
        0x09 => (KeyCode::Tab, KeyModifiers::NONE),
        0x7f => (KeyCode::Backspace, KeyModifiers::NONE),
        0x01..=0x1a => (
            KeyCode::Char(char::from(b'a' + (ch as u8 - 0x01))),
            KeyModifiers::CONTROL,
        ),
        0x1c..=0x1f => (
            KeyCode::Char(char::from(b'4' + (ch as u8 - 0x1c))),
            KeyModifiers::CONTROL,
        ),
        0x00 | ESC => return None,
        _ => (
            KeyCode::Char(char::from_u32(u32::from(ch))?),
            KeyModifiers::NONE,
        ),
    };
    Some(KeyEvent::new(code, modifiers))
}

/// `ESC O` and a character: the cursor keys in application mode, and F1 to F4.
fn ss3(ch: u16) -> Option<KeyEvent> {
    let code = match ch {
        0x41 => KeyCode::Up,
        0x42 => KeyCode::Down,
        0x43 => KeyCode::Right,
        0x44 => KeyCode::Left,
        0x48 => KeyCode::Home,
        0x46 => KeyCode::End,
        0x50..=0x53 => KeyCode::F(1 + (ch - 0x50) as u8),
        _ => return None,
    };
    Some(KeyEvent::new(code, KeyModifiers::NONE))
}

/// `ESC [`, the parameters `body`, and the final character: cursor and editing keys,
/// F1 to F12 and Shift-Tab, with xterm's modifier parameter. Anything else, such as a
/// mouse or focus report, is no key.
fn csi(body: &str, last: u8) -> Option<KeyEvent> {
    if body.starts_with(|c: char| !c.is_ascii_digit() && c != ';') {
        return None;
    }
    let mut params = body.split(';').map(|p| p.parse::<u16>().ok());
    let first = params.next().flatten();
    let modifiers = modifiers(params.next().flatten());
    let code = match (last, first) {
        (b'A', _) => KeyCode::Up,
        (b'B', _) => KeyCode::Down,
        (b'C', _) => KeyCode::Right,
        (b'D', _) => KeyCode::Left,
        (b'H', _) => KeyCode::Home,
        (b'F', _) => KeyCode::End,
        (b'P'..=b'S', _) => KeyCode::F(1 + last - b'P'),
        (b'Z', _) => return Some(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        (b'~', Some(1 | 7)) => KeyCode::Home,
        (b'~', Some(2)) => KeyCode::Insert,
        (b'~', Some(3)) => KeyCode::Delete,
        (b'~', Some(4 | 8)) => KeyCode::End,
        (b'~', Some(5)) => KeyCode::PageUp,
        (b'~', Some(6)) => KeyCode::PageDown,
        (b'~', Some(n @ 11..=15)) => KeyCode::F((n - 10) as u8),
        (b'~', Some(n @ 17..=21)) => KeyCode::F((n - 11) as u8),
        (b'~', Some(n @ 23..=24)) => KeyCode::F((n - 12) as u8),
        _ => return None,
    };
    Some(KeyEvent::new(code, modifiers))
}

/// xterm's modifier parameter: one more than the sum of Shift 1, Alt 2 and Ctrl 4.
fn modifiers(param: Option<u16>) -> KeyModifiers {
    let mask = param.unwrap_or(1).saturating_sub(1);
    let mut modifiers = KeyModifiers::NONE;
    if mask & 1 != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if mask & 2 != 0 {
        modifiers |= KeyModifiers::ALT;
    }
    if mask & 4 != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    modifiers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str) -> Vec<KeyEvent> {
        let mut vt = VtKeys::default();
        let mut keys: Vec<KeyEvent> = text.encode_utf16().filter_map(|c| vt.feed(c)).collect();
        keys.extend(vt.flush());
        keys
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn enter_tab_and_backspace() {
        assert_eq!(
            keys("\r\t\x7f"),
            [
                key(KeyCode::Enter, KeyModifiers::NONE),
                key(KeyCode::Tab, KeyModifiers::NONE),
                key(KeyCode::Backspace, KeyModifiers::NONE),
            ]
        );
    }

    #[test]
    fn control_characters_are_ctrl_with_a_letter() {
        assert_eq!(
            keys("\x01\x08\x1a"),
            [
                key(KeyCode::Char('a'), KeyModifiers::CONTROL),
                key(KeyCode::Char('h'), KeyModifiers::CONTROL),
                key(KeyCode::Char('z'), KeyModifiers::CONTROL),
            ]
        );
    }

    #[test]
    fn cursor_keys_in_both_modes() {
        assert_eq!(
            keys("\x1b[A\x1b[D\x1bOB\x1bOC"),
            [
                key(KeyCode::Up, KeyModifiers::NONE),
                key(KeyCode::Left, KeyModifiers::NONE),
                key(KeyCode::Down, KeyModifiers::NONE),
                key(KeyCode::Right, KeyModifiers::NONE),
            ]
        );
    }

    #[test]
    fn modifiers_on_cursor_keys() {
        assert_eq!(
            keys("\x1b[1;5C\x1b[1;2H\x1b[1;3D"),
            [
                key(KeyCode::Right, KeyModifiers::CONTROL),
                key(KeyCode::Home, KeyModifiers::SHIFT),
                key(KeyCode::Left, KeyModifiers::ALT),
            ]
        );
    }

    #[test]
    fn editing_and_function_keys() {
        assert_eq!(
            keys("\x1b[3~\x1b[5~\x1b[2~\x1b[15~\x1b[24~\x1bOP\x1b[Z\x1b[3;5~"),
            [
                key(KeyCode::Delete, KeyModifiers::NONE),
                key(KeyCode::PageUp, KeyModifiers::NONE),
                key(KeyCode::Insert, KeyModifiers::NONE),
                key(KeyCode::F(5), KeyModifiers::NONE),
                key(KeyCode::F(12), KeyModifiers::NONE),
                key(KeyCode::F(1), KeyModifiers::NONE),
                key(KeyCode::BackTab, KeyModifiers::SHIFT),
                key(KeyCode::Delete, KeyModifiers::CONTROL),
            ]
        );
    }

    #[test]
    fn escape_then_a_key_is_alt_with_it() {
        assert_eq!(
            keys("\x1bf\x1b\r\x1b\x7f"),
            [
                key(KeyCode::Char('f'), KeyModifiers::ALT),
                key(KeyCode::Enter, KeyModifiers::ALT),
                key(KeyCode::Backspace, KeyModifiers::ALT),
            ]
        );
    }

    #[test]
    fn a_lone_escape_is_the_escape_key() {
        assert_eq!(keys("\x1b"), [key(KeyCode::Esc, KeyModifiers::NONE)]);
        assert_eq!(keys("\x1b\x1b"), [key(KeyCode::Esc, KeyModifiers::NONE)]);
    }

    #[test]
    fn an_unfinished_bracket_is_alt_with_it() {
        assert_eq!(keys("\x1b["), [key(KeyCode::Char('['), KeyModifiers::ALT)]);
    }

    #[test]
    fn reports_that_are_not_keys_are_dropped() {
        // An SGR mouse report, a focus report, and a bracketed paste's markers.
        assert_eq!(
            keys("\x1b[<0;10;5M\x1b[I\x1b[200~x\x1b[201~"),
            [key(KeyCode::Char('x'), KeyModifiers::NONE)]
        );
    }

    #[test]
    fn text_after_a_broken_sequence_is_kept() {
        assert_eq!(keys("\x1b[1\r"), [key(KeyCode::Enter, KeyModifiers::NONE)]);
    }
}
