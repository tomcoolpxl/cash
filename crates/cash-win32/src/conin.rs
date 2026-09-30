//! A console's keys, one character at a time, with a deadline — what `read` needs at a
//! console.
//!
//! A console as a program finds it collects a line. It shows each key, lets the line be
//! edited, and hands the program nothing until Enter; the read that waits for it cannot
//! be given a deadline once a key has been typed, cannot end at a character other than
//! Enter, and shows what is typed whether or not the program wants it shown. So `read -t`,
//! `-d`, `-n` and `-s` all failed at a console: a terminal's answer to a query, which has
//! no Enter in it, was shown on the screen and then waited for until someone pressed the
//! key (2026-09-30).
//!
//! Nor does the read end at Ctrl-C. The console makes a control event of the key, which
//! ends a process that has no handler for it, and the read of one that has goes on
//! waiting for Enter. So a plain `read` is served here as well: Ctrl-C ended the
//! interactive shell, or did nothing to a script.
//!
//! A Unix shell turns the terminal driver's line collection off for such a read and takes
//! the bytes as they come. The console's equivalent is its queue of input records: each
//! key is a record, there whether or not a line is being collected, and a handle to the
//! queue can be waited on. [`Keys`] reads that queue, one record at a time, so nothing is
//! taken that the caller does not ask for: what is typed after the last character `read`
//! wants stays in the queue, for the next command or the prompt.
//!
//! Nothing here shows a key. A console shows keys only while it collects a line, so the
//! caller shows what it wants shown.

use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle as _, OwnedHandle};
use std::time::Instant;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT,
    GetConsoleMode, GetNumberOfConsoleInputEvents, INPUT_RECORD, KEY_EVENT, ReadConsoleInputW,
    SetConsoleMode,
};
use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

/// The Alt key. A character entered as Alt and digits on the number pad arrives when Alt
/// is released, the one key release that carries a character to read.
const VK_MENU: u16 = 0x12;

/// The keys of a console, read as they are typed.
///
/// While it lives, the console collects no line and shows no key, and Ctrl-C is a
/// character (0x03) rather than a console control event, which nothing waiting on the
/// queue would notice. Dropping it puts the console's input mode back as it was.
pub struct Keys {
    input: OwnedHandle,
    /// The console's input mode as it was found.
    saved: u32,
    /// The first half of a character that takes two UTF-16 units, until the second comes.
    high: Option<u16>,
    /// A character whose record says it was typed more than once, as a key held down is,
    /// and how many of them are still to hand over.
    held: Option<(char, u16)>,
}

impl Keys {
    /// Takes over the console `input` is a handle to; `None` when it is not a console's
    /// input.
    ///
    /// With `sequences`, the console hands over the keys that have no character (the
    /// arrows, Home, F1) and what a terminal answers a query with as the escape sequences
    /// a terminal sends, which is what a script reading keys one at a time looks for.
    /// Without it such keys are dropped, and a console host may take an answer it
    /// recognises for itself.
    #[must_use]
    pub fn open(input: &impl AsHandle, sequences: bool) -> Option<Self> {
        let input = input.as_handle().try_clone_to_owned().ok()?;
        let handle: HANDLE = input.as_raw_handle();
        let mut queued = 0u32;
        // SAFETY: an open handle owned by `input`, and a valid out-parameter. Only a
        // console's input handle answers this.
        if unsafe { GetNumberOfConsoleInputEvents(handle, &raw mut queued) } == 0 {
            return None;
        }
        let mut saved = 0u32;
        // SAFETY: as above.
        if unsafe { GetConsoleMode(handle, &raw mut saved) } == 0 {
            return None;
        }

        let keys = saved & !(ENABLE_PROCESSED_INPUT | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT);
        // SAFETY: an open console input handle owned by `input`.
        let sending_sequences = sequences
            && unsafe { SetConsoleMode(handle, keys | ENABLE_VIRTUAL_TERMINAL_INPUT) } != 0;
        // A console too old for VT input refuses the mode whole, so it is asked again
        // without.
        if !sending_sequences {
            // SAFETY: as above.
            unsafe { SetConsoleMode(handle, keys) };
        }

        Some(Self {
            input,
            saved,
            high: None,
            held: None,
        })
    }

    /// The next character typed, waiting for it until `deadline`, or for as long as it
    /// takes when there is none. `None` when the deadline passed first.
    ///
    /// A character already waiting is returned even after the deadline: `read -t 0.001`,
    /// as scripts write it to collect the rest of a key's escape sequence, asks for what
    /// is there.
    ///
    /// # Errors
    ///
    /// Returns an error if the console's queue can no longer be read or waited on.
    pub fn next(&mut self, deadline: Option<Instant>) -> io::Result<Option<char>> {
        loop {
            if let Some((character, left)) = self.held {
                self.held = (left > 1).then(|| (character, left - 1));
                return Ok(Some(character));
            }
            if !self.wait(deadline)? {
                return Ok(None);
            }

            // SAFETY: an all-zero INPUT_RECORD is a valid value for the out-parameter.
            let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
            let mut count = 0u32;
            // SAFETY: an open console input handle owned by `self.input`, room for the one
            // record asked for, and a valid out-parameter. The queue holds a record, so
            // this returns at once.
            let read = unsafe {
                ReadConsoleInputW(
                    self.input.as_raw_handle(),
                    &raw mut record,
                    1,
                    &raw mut count,
                )
            };
            if read == 0 {
                return Err(io::Error::last_os_error());
            }
            if count == 0 {
                continue;
            }
            let Some((unit, times)) = typed(&record) else {
                continue;
            };
            if let Some(character) = complete(&mut self.high, unit) {
                self.held = Some((character, times));
            }
        }
    }

    /// Waits until the queue holds a record; `false` when `deadline` passed first.
    fn wait(&self, deadline: Option<Instant>) -> io::Result<bool> {
        let handle: HANDLE = self.input.as_raw_handle();
        loop {
            let mut queued = 0u32;
            // SAFETY: an open console input handle owned by `self.input`, and a valid
            // out-parameter.
            if unsafe { GetNumberOfConsoleInputEvents(handle, &raw mut queued) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if queued > 0 {
                return Ok(true);
            }

            let millis = match deadline {
                None => INFINITE,
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Ok(false);
                    }
                    // Rounded up, so that what is left of a millisecond is waited for
                    // rather than spun through; INFINITE is the one value not to reach.
                    u32::try_from(left.as_micros().div_ceil(1000))
                        .unwrap_or(INFINITE - 1)
                        .min(INFINITE - 1)
                }
            };
            // SAFETY: as above; the call only waits on the handle.
            match unsafe { WaitForSingleObject(handle, millis) } {
                WAIT_OBJECT_0 | WAIT_TIMEOUT => {}
                _ => return Err(io::Error::last_os_error()),
            }
        }
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        // SAFETY: an open console input handle owned by `self.input`.
        unsafe { SetConsoleMode(self.input.as_raw_handle(), self.saved) };
    }
}

/// The UTF-16 unit a record types and how many times, if it types one: a key pressed
/// that has a character. Key releases, keys without a character (Shift, the arrows when
/// the console does not send sequences), the mouse, focus and resizes type nothing.
fn typed(record: &INPUT_RECORD) -> Option<(u16, u16)> {
    if u32::from(record.EventType) != KEY_EVENT {
        return None;
    }
    // SAFETY: a KEY_EVENT record holds a KeyEvent in its union.
    let key = unsafe { record.Event.KeyEvent };
    // SAFETY: the character is read as UTF-16, the variant the W API fills.
    let unit = unsafe { key.uChar.UnicodeChar };
    if unit == 0 || (key.bKeyDown == 0 && key.wVirtualKeyCode != VK_MENU) {
        return None;
    }
    Some((unit, key.wRepeatCount.max(1)))
}

/// The character `unit` completes, if it completes one. A character outside the basic
/// plane arrives as two records, one UTF-16 unit each: `high` holds the first until the
/// second comes. Half a pair on its own is U+FFFD.
fn complete(high: &mut Option<u16>, unit: u16) -> Option<char> {
    if let Some(first) = high.take() {
        return Some(
            char::decode_utf16([first, unit])
                .next()
                .and_then(Result::ok)
                .unwrap_or(char::REPLACEMENT_CHARACTER),
        );
    }
    if (0xD800..0xDC00).contains(&unit) {
        *high = Some(unit);
        return None;
    }
    Some(char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(unit: u16, down: bool, virtual_key: u16, repeat: u16) -> INPUT_RECORD {
        // SAFETY: an all-zero INPUT_RECORD is valid; the fields a key record needs follow.
        let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
        record.EventType = u16::try_from(KEY_EVENT).unwrap_or_default();
        record.Event.KeyEvent.bKeyDown = i32::from(down);
        record.Event.KeyEvent.wVirtualKeyCode = virtual_key;
        record.Event.KeyEvent.wRepeatCount = repeat;
        record.Event.KeyEvent.uChar.UnicodeChar = unit;
        record
    }

    #[test]
    fn only_a_key_pressed_with_a_character_types() {
        assert_eq!(typed(&key(u16::from(b'a'), true, 0x41, 1)), Some((97, 1)));
        assert_eq!(typed(&key(u16::from(b'a'), false, 0x41, 1)), None);
        // Shift on its own, and an arrow on a console that sends no sequences.
        assert_eq!(typed(&key(0, true, 0x10, 1)), None);
        assert_eq!(typed(&key(0, true, 0x26, 1)), None);
        // SAFETY: an all-zero INPUT_RECORD is valid: an event of type 0.
        let focus: INPUT_RECORD = unsafe { std::mem::zeroed() };
        assert_eq!(typed(&focus), None);
    }

    #[test]
    fn a_held_key_types_its_character_as_often_as_its_record_says() {
        assert_eq!(typed(&key(u16::from(b'x'), true, 0x58, 3)), Some((120, 3)));
        // Records written by a program may leave the count at zero.
        assert_eq!(typed(&key(u16::from(b'x'), true, 0x58, 0)), Some((120, 1)));
    }

    #[test]
    fn alt_released_types_the_character_entered_on_the_number_pad() {
        assert_eq!(typed(&key(0xE9, false, VK_MENU, 1)), Some((0xE9, 1)));
    }

    #[test]
    fn two_units_make_one_character_outside_the_basic_plane() {
        let mut high = None;
        assert_eq!(complete(&mut high, u16::from(b'a')), Some('a'));
        assert_eq!(complete(&mut high, 0x20AC), Some('€'));
        // U+1F600 is D83D DE00 in UTF-16.
        assert_eq!(complete(&mut high, 0xD83D), None);
        assert_eq!(complete(&mut high, 0xDE00), Some('\u{1F600}'));
        assert_eq!(high, None);
    }

    #[test]
    fn half_a_pair_is_the_replacement_character() {
        let mut high = None;
        assert_eq!(
            complete(&mut high, 0xDE00),
            Some(char::REPLACEMENT_CHARACTER)
        );
        assert_eq!(complete(&mut high, 0xD83D), None);
        assert_eq!(
            complete(&mut high, u16::from(b'a')),
            Some(char::REPLACEMENT_CHARACTER)
        );
    }
}
