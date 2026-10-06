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
//! [`Keys`] shows nothing: a console shows keys only while it collects a line. [`Terminal`]
//! is the reader on top of it that the shell's own readers use. It shows what is typed,
//! unless asked not to, and collects a line the way a terminal driver does.

use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle as _, OwnedHandle};
use std::time::Instant;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, GetConsoleMode, GetConsoleScreenBufferInfo,
    GetNumberOfConsoleInputEvents, INPUT_RECORD, KEY_EVENT, ReadConsoleInputW, SetConsoleMode,
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
    /// Characters read and not yet handed over, each with how many times: a record says a
    /// key held down was typed more than once, and one record can complete two characters
    /// (a lone half of a pair, then a character of its own).
    held: std::collections::VecDeque<(char, u16)>,
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
            held: std::collections::VecDeque::new(),
        })
    }

    /// Whether the console was showing what is typed when it was taken over: `stty -echo`
    /// turns that off, and a reader that shows the keys itself should then not.
    #[must_use]
    pub const fn was_echoing(&self) -> bool {
        self.saved & ENABLE_ECHO_INPUT != 0
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
            if let Some((character, left)) = self.held.front_mut() {
                let character = *character;
                if *left > 1 {
                    *left -= 1;
                } else {
                    self.held.pop_front();
                }
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
            // The record's count is the key's: a lone half before it was typed once.
            match complete(&mut self.high, unit) {
                (Some(lone), Some(character)) => {
                    self.held.push_back((lone, 1));
                    self.held.push_back((character, times));
                }
                (Some(character), None) => self.held.push_back((character, times)),
                _ => {}
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

/// Ctrl-C, which interrupts.
pub const CTRL_C: char = '\x03';
/// Ctrl-D, which ends input where a line starts, and hands over a line begun.
pub const CTRL_D: char = '\x04';
/// Ctrl-U, which takes back the line typed so far: a terminal's kill character.
const CTRL_U: char = '\x15';
/// Ctrl-W, which takes back the last word typed: a terminal's word-erase character.
const CTRL_W: char = '\x17';
/// Ctrl-Z, which ends input at a Windows console.
const CTRL_Z: char = '\x1a';
/// What the Backspace key types at a console.
const BACKSPACE: char = '\x08';
/// What Ctrl+Backspace types at a console, and Backspace at one sending a terminal's
/// sequences.
const DELETE: char = '\x7f';
/// Tab, shown as blanks up to the next tab stop.
const TAB: char = '\t';
/// What Enter is handed over as.
const NEWLINE: char = '\n';

/// A console read a key at a time, doing for the shell's own readers (`read`, `select`,
/// `mapfile`) what a terminal driver does for them on Unix.
///
/// Bash leaves the terminal collecting lines unless `read -n`, `-N` or a delimiter other
/// than newline asks for characters as they are typed, turns the terminal's echo off for
/// `read -s`, and has a signal end the read when `-t` runs out or Ctrl-C is typed. A
/// Windows console collecting a line can do none of that: it hands over nothing before
/// Enter, a read of it cannot be ended once a key is typed, and it shows keys only while
/// it collects a line. Of Ctrl-C it makes a console control event, and the read still
/// waits for Enter: the event ends a process that has no handler for it where it stands,
/// the interactive shell included, and one that has a handler, or was started to ignore
/// Ctrl-C, reads on. So at a console the shell's readers take the keys ([`Keys`]), where
/// Ctrl-C is a key, and this shows them and collects the line.
///
/// A line is edited as a terminal driver lets it be: Backspace takes a character back,
/// Ctrl-W a word, Ctrl-U all of it, and Ctrl-D ends input. The console's own editing of
/// a line (the arrow keys, Escape, the function keys) is given up for it; `read -e` has
/// an editor.
pub struct Terminal {
    keys: Keys,
    /// The screen, to show what is typed on; `None` when it is not to be shown.
    screen: Option<std::fs::File>,
    /// Whether a line is collected and handed over at Enter, with Backspace taking a
    /// character back, as a terminal does unless told otherwise. If not, each character
    /// is handed over as it is typed, Backspace among them.
    collects_lines: bool,
    /// Characters handed over and not yet taken.
    ready: std::collections::VecDeque<char>,
}

/// A line typed at a console, or what ended the typing instead.
#[derive(Debug, Eq, PartialEq)]
pub enum Line {
    /// What was typed up to Enter, with the newline Enter stands for; or without one,
    /// when Ctrl-D handed over a line that had been begun.
    Typed(String),
    /// Ctrl-D, or Ctrl-Z, where a line starts.
    EndOfInput,
    /// Ctrl-C. What was typed of the line is dropped.
    Interrupted,
}

impl Terminal {
    /// Takes over the console `input` is a handle to; `None` when it is not a console's
    /// input.
    ///
    /// With `collects_lines`, a line is handed over when it is ended; without, each
    /// character as it is typed. `shows_keys` says whether what is typed is shown.
    #[must_use]
    pub fn open(input: &impl AsHandle, collects_lines: bool, shows_keys: bool) -> Option<Self> {
        // As they are typed, the keys without a character are wanted too, as the
        // sequences a terminal sends: a script reads an arrow as `\e[A`, and the answer
        // to a terminal query starts with `\e`. In a line they are left out, as the
        // console leaves them out of the lines it collects.
        let keys = Keys::open(input, !collects_lines)?;
        // A script that turned echo off (`stty -echo; read -r password; stty echo`) has
        // its keys hidden here as a terminal driver would hide them, since the console
        // shows nothing while the keys are read this way.
        let shows_keys = shows_keys && keys.was_echoing();
        // Opened to be read as well: the console tells the cursor's column only through
        // a handle that may read the screen, and a tab is shown up to the next tab stop.
        let screen = shows_keys
            .then(|| {
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open("CONOUT$")
                    .ok()
            })
            .flatten();
        Some(Self {
            keys,
            screen,
            collects_lines,
            ready: std::collections::VecDeque::new(),
        })
    }

    /// The next character, or `None` when `deadline` passed first. Collecting a line,
    /// that is its first character, once it has been ended; what was typed of a line
    /// that was not ended in time is dropped, as a terminal hands a reader none of it.
    ///
    /// Ctrl-C is handed over as [`CTRL_C`], at once and without the line it was typed
    /// in; Ctrl-D as [`CTRL_D`], after what there was of the line.
    ///
    /// # Errors
    ///
    /// Returns an error if the console's queue can no longer be read or waited on.
    pub fn next(&mut self, deadline: Option<Instant>) -> io::Result<Option<char>> {
        if let Some(ch) = self.ready.pop_front() {
            return Ok(Some(ch));
        }
        if !self.collects_lines {
            let ch = self.key(deadline)?;
            if let Some(ch) = ch {
                self.show(ch);
            }
            return Ok(ch);
        }

        // Each character typed, with the columns it was shown in.
        let mut line: Vec<(char, usize)> = Vec::new();
        loop {
            let Some(ch) = self.key(deadline)? else {
                return Ok(None);
            };
            match ch {
                BACKSPACE | DELETE => {
                    if let Some((_, columns)) = line.pop() {
                        self.erase(columns);
                    }
                }
                CTRL_U => {
                    while let Some((_, columns)) = line.pop() {
                        self.erase(columns);
                    }
                }
                // The blanks after the last word go with it.
                CTRL_W => {
                    for of_word in [false, true] {
                        while let Some((_, columns)) =
                            line.pop_if(|last| last.0.is_whitespace() != of_word)
                        {
                            self.erase(columns);
                        }
                    }
                }
                // Ctrl-C drops the line; Ctrl-D hands over what there is of it.
                CTRL_C => {
                    self.show(ch);
                    return Ok(Some(ch));
                }
                // As the console's own line collection has it, and Windows users know it:
                // Ctrl-Z where a line starts ends input.
                CTRL_Z if line.is_empty() => {
                    line.push((CTRL_D, 0));
                    break;
                }
                NEWLINE | CTRL_D => {
                    let columns = self.show(ch);
                    line.push((ch, columns));
                    break;
                }
                _ => {
                    let columns = self.show(ch);
                    line.push((ch, columns));
                }
            }
        }
        self.ready.extend(line.into_iter().map(|(ch, _)| ch));
        Ok(self.ready.pop_front())
    }

    /// The next line typed, waited for as long as it takes. For a reader of lines; it
    /// asks for a line whether or not the console was opened to collect them.
    ///
    /// # Errors
    ///
    /// Returns an error if the console's queue can no longer be read or waited on.
    pub fn line(&mut self) -> io::Result<Line> {
        let mut line = String::new();
        loop {
            match self.next(None)? {
                None => {}
                Some(CTRL_C) => return Ok(Line::Interrupted),
                Some(CTRL_D) if line.is_empty() => return Ok(Line::EndOfInput),
                Some(CTRL_D) => return Ok(Line::Typed(line)),
                Some(NEWLINE) => {
                    line.push(NEWLINE);
                    return Ok(Line::Typed(line));
                }
                Some(ch) => line.push(ch),
            }
        }
    }

    /// The next key's character. Enter is a newline, as a terminal makes it: the
    /// console's own is a carriage return.
    fn key(&mut self, deadline: Option<Instant>) -> io::Result<Option<char>> {
        Ok(self
            .keys
            .next(deadline)?
            .map(|ch| if ch == '\r' { NEWLINE } else { ch }))
    }

    /// Shows a character as a terminal echoes it, and says how many columns that took: a
    /// control character as `^C`, a tab as blanks up to the next tab stop, and Ctrl-D,
    /// which ends input, not at all.
    fn show(&mut self, ch: char) -> usize {
        use io::Write as _;

        let Some(screen) = &mut self.screen else {
            return 0;
        };
        let shown = match ch {
            NEWLINE => "\r\n".to_owned(),
            CTRL_C => "^C\r\n".to_owned(),
            CTRL_D => return 0,
            TAB => " ".repeat(columns_to_tab_stop(cursor_column(screen))),
            _ => shown_as(ch),
        };
        // A screen that cannot be written to is no reason to fail the read.
        let _ = screen.write_all(shown.as_bytes());
        unicode_width::UnicodeWidthStr::width(shown.as_str())
    }

    /// Takes off the screen the `columns` a character was shown in.
    fn erase(&mut self, columns: usize) {
        use io::Write as _;

        if let Some(screen) = &mut self.screen {
            let _ = screen.write_all("\x08 \x08".repeat(columns).as_bytes());
        }
    }
}

/// What the screen shows for a character typed: itself, or for a control character the
/// key that types it with Ctrl, as `^[` for Escape.
fn shown_as(ch: char) -> String {
    match u8::try_from(ch) {
        Ok(control @ 0..=0x1f) => format!("^{}", char::from(control + b'@')),
        Ok(0x7f) => "^?".to_owned(),
        _ => ch.to_string(),
    }
}

/// How far apart a console's tab stops are.
const TAB_STOP: usize = 8;

/// How many blanks a tab typed at `column` takes to reach the next tab stop.
const fn columns_to_tab_stop(column: usize) -> usize {
    TAB_STOP - column % TAB_STOP
}

/// The column the cursor of the console `screen` is in, from zero; zero as well when the
/// console does not say.
fn cursor_column(screen: &std::fs::File) -> usize {
    // SAFETY: an all-zero CONSOLE_SCREEN_BUFFER_INFO is a valid value for the
    // out-parameter.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: an open handle owned by `screen`, and a valid out-parameter. Anything but
    // a console's screen makes the call fail.
    if unsafe { GetConsoleScreenBufferInfo(screen.as_raw_handle(), &raw mut info) } == 0 {
        return 0;
    }
    usize::try_from(info.dwCursorPosition.X).unwrap_or(0)
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

/// The characters `unit` completes, none, one or two. A character outside the basic
/// plane arrives as two records, one UTF-16 unit each: `high` holds the first until the
/// second comes. Half a pair on its own is U+FFFD, and a unit after a lone high half is
/// still read: it was taken as the pair's second and lost (W32-20).
fn complete(high: &mut Option<u16>, unit: u16) -> (Option<char>, Option<char>) {
    if let Some(first) = high.take() {
        if (0xDC00..0xE000).contains(&unit) {
            let pair = char::decode_utf16([first, unit])
                .next()
                .and_then(Result::ok)
                .unwrap_or(char::REPLACEMENT_CHARACTER);
            return (Some(pair), None);
        }
        return (Some(char::REPLACEMENT_CHARACTER), complete(high, unit).0);
    }
    if (0xD800..0xDC00).contains(&unit) {
        *high = Some(unit);
        return (None, None);
    }
    let character = char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER);
    (Some(character), None)
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
    fn a_control_character_is_shown_as_the_key_that_types_it() {
        assert_eq!(shown_as('a'), "a");
        assert_eq!(shown_as('é'), "é");
        assert_eq!(shown_as('\x1b'), "^[");
        assert_eq!(shown_as(CTRL_C), "^C");
        assert_eq!(shown_as(DELETE), "^?");
    }

    #[test]
    fn a_tab_reaches_the_next_tab_stop() {
        assert_eq!(columns_to_tab_stop(0), 8);
        assert_eq!(columns_to_tab_stop(3), 5);
        assert_eq!(columns_to_tab_stop(7), 1);
        assert_eq!(columns_to_tab_stop(8), 8);
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
        assert_eq!(complete(&mut high, u16::from(b'a')), (Some('a'), None));
        assert_eq!(complete(&mut high, 0x20AC), (Some('€'), None));
        // U+1F600 is D83D DE00 in UTF-16.
        assert_eq!(complete(&mut high, 0xD83D), (None, None));
        assert_eq!(complete(&mut high, 0xDE00), (Some('\u{1F600}'), None));
        assert_eq!(high, None);
    }

    /// Half a pair is U+FFFD, and what follows a lone high half is read too: the `a` was
    /// lost (W32-20). A high half after a high half waits for its own second.
    #[test]
    fn half_a_pair_is_the_replacement_character() {
        const FFFD: char = char::REPLACEMENT_CHARACTER;
        let mut high = None;
        assert_eq!(complete(&mut high, 0xDE00), (Some(FFFD), None));
        assert_eq!(complete(&mut high, 0xD83D), (None, None));
        assert_eq!(
            complete(&mut high, u16::from(b'a')),
            (Some(FFFD), Some('a'))
        );
        assert_eq!(complete(&mut high, 0xD83D), (None, None));
        assert_eq!(complete(&mut high, 0xD83D), (Some(FFFD), None));
        assert_eq!(complete(&mut high, 0xDE00), (Some('\u{1F600}'), None));
    }
}
