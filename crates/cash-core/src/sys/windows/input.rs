//! Terminal input utilities.
//!
//! cash: Unix reads the sequences each key sends from terminfo; Windows has no terminfo,
//! and needs none. Every Windows console that sends VT input (ConPTY, and so Windows
//! Terminal, VS Code and conhost in VT mode) sends xterm's sequences, so they are
//! listed here. With them, a binding spelled as the key sends it, `"\e[H"` or `"\e[A"`,
//! binds the Home or Up key, as an `.inputrc` written for Bash expects.

use crate::interfaces::Key;

/// The key an unmodified xterm key sequence names, in both its CSI (`\e[`) and SS3
/// (`\eO`) forms, which terminals send in normal and application cursor mode.
const KEY_SEQUENCES: &[(&[u8], Key)] = &[
    (b"\x1b[A", Key::Up),
    (b"\x1bOA", Key::Up),
    (b"\x1b[B", Key::Down),
    (b"\x1bOB", Key::Down),
    (b"\x1b[C", Key::Right),
    (b"\x1bOC", Key::Right),
    (b"\x1b[D", Key::Left),
    (b"\x1bOD", Key::Left),
    (b"\x1b[H", Key::Home),
    (b"\x1bOH", Key::Home),
    (b"\x1b[1~", Key::Home),
    (b"\x1b[F", Key::End),
    (b"\x1bOF", Key::End),
    (b"\x1b[4~", Key::End),
    (b"\x1b[2~", Key::Insert),
    (b"\x1b[3~", Key::Delete),
    (b"\x1b[5~", Key::PageUp),
    (b"\x1b[6~", Key::PageDown),
    (b"\x1b[Z", Key::BackTab),
    (b"\x1bOP", Key::F(1)),
    (b"\x1bOQ", Key::F(2)),
    (b"\x1bOR", Key::F(3)),
    (b"\x1bOS", Key::F(4)),
    (b"\x1b[15~", Key::F(5)),
    (b"\x1b[17~", Key::F(6)),
    (b"\x1b[18~", Key::F(7)),
    (b"\x1b[19~", Key::F(8)),
    (b"\x1b[20~", Key::F(9)),
    (b"\x1b[21~", Key::F(10)),
    (b"\x1b[23~", Key::F(11)),
    (b"\x1b[24~", Key::F(12)),
];

/// Translates a key code (byte sequence) into a `Key` enum value. Returns `None`
/// if the key code is not recognized.
///
/// # Arguments
///
/// * `key_code`: The byte sequence representing the key code.
pub fn try_get_key_from_key_code(key_code: &[u8]) -> Option<Key> {
    if let Some((_, key)) = KEY_SEQUENCES.iter().find(|(seq, _)| *seq == key_code) {
        Some(key.clone())
    } else if key_code.len() == 1 && !key_code[0].is_ascii_control() {
        Some(Key::Character(key_code[0] as char))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xterm_sequences_name_their_keys() {
        assert_eq!(try_get_key_from_key_code(b"\x1b[H"), Some(Key::Home));
        assert_eq!(try_get_key_from_key_code(b"\x1bOH"), Some(Key::Home));
        assert_eq!(try_get_key_from_key_code(b"\x1b[A"), Some(Key::Up));
        assert_eq!(try_get_key_from_key_code(b"\x1b[3~"), Some(Key::Delete));
        assert_eq!(try_get_key_from_key_code(b"a"), Some(Key::Character('a')));
        assert_eq!(try_get_key_from_key_code(b"\x1b[99~"), None);
    }
}
