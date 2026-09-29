use std::collections::VecDeque;
use std::time::Duration;

use crossterm_winapi::{Console, Handle, InputRecord};
use winapi::um::{
    consoleapi::ReadConsoleInputW,
    wincon::{
        PeekConsoleInputW, INPUT_RECORD, KEY_EVENT, LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED,
        RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED,
    },
    winuser::{
        VK_CAPITAL, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU,
        VK_RSHIFT, VK_SHIFT,
    },
};

use crate::event::{
    sys::windows::{parse::MouseButtonsPressed, poll::WinApiPoll},
    Event,
};

#[cfg(feature = "event-stream")]
use crate::event::sys::Waker;
use crate::event::{
    source::EventSource,
    sys::windows::parse::{handle_key_event, handle_mouse_event},
    timeout::PollTimeout,
    InternalEvent,
};

mod vt_keys;

/// cash: the most records one read takes from the console.
const MAX_BATCH: u32 = 1024;

pub(crate) struct WindowsEventSource {
    console: Console,
    /// cash: the console input handle, for the peek and read `Console` does not offer.
    handle: Handle,
    poll: WinApiPoll,
    surrogate_buffer: Option<u16>,
    mouse_buttons_pressed: MouseButtonsPressed,
    /// cash: records read in one batch and not yet turned into events.
    pending: VecDeque<InputRecord>,
    /// cash: keys that arrived as VT text, decoded as they come (`vt_keys`).
    vt: vt_keys::VtKeys,
}

impl WindowsEventSource {
    pub(crate) fn new() -> std::io::Result<WindowsEventSource> {
        let handle = Handle::current_in_handle()?;
        let console = Console::from(handle.clone());
        Ok(WindowsEventSource {
            console,
            handle,

            #[cfg(not(feature = "event-stream"))]
            poll: WinApiPoll::new(),
            #[cfg(feature = "event-stream")]
            poll: WinApiPoll::new()?,

            surrogate_buffer: None,
            mouse_buttons_pressed: MouseButtonsPressed::default(),
            pending: VecDeque::new(),
            vt: vt_keys::VtKeys::default(),
        })
    }

    /// cash: the event `record` makes, decoding keys that arrived as VT text (patch 2).
    ///
    /// A key-down of VT text goes to the decoder. Anything else first ends a sequence the
    /// decoder has open, and is then handled as before, except a key being let go: the
    /// keys of VT text are let go as they arrive, and that must not cut a sequence short.
    fn next_event(&mut self, record: InputRecord) -> Option<Event> {
        if let Some(ch) = vt_text(&record, self.vt.is_open()) {
            return self.vt.feed(ch).map(Event::Key);
        }
        let let_go = matches!(&record, InputRecord::KeyEvent(key) if !key.key_down);
        if self.vt.is_open() && !let_go {
            let key = self.vt.flush();
            self.pending.push_front(record);
            return key.map(Event::Key);
        }
        self.to_event(record)
    }

    fn to_event(&mut self, record: InputRecord) -> Option<Event> {
        match record {
            InputRecord::KeyEvent(record) => handle_key_event(record, &mut self.surrogate_buffer),
            InputRecord::MouseEvent(record) => {
                let mouse_event = handle_mouse_event(record, &self.mouse_buttons_pressed);
                self.mouse_buttons_pressed = MouseButtonsPressed {
                    left: record.button_state.left_button(),
                    right: record.button_state.right_button(),
                    middle: record.button_state.middle_button(),
                };

                mouse_event
            }
            InputRecord::WindowBufferSizeEvent(record) => {
                // windows starts counting at 0, unix at 1, add one to replicate unix behaviour.
                Some(Event::Resize(
                    (record.size.x as i32 + 1) as u16,
                    (record.size.y as i32 + 1) as u16,
                ))
            }
            InputRecord::FocusEvent(record) => {
                let event = if record.set_focus {
                    Event::FocusGained
                } else {
                    Event::FocusLost
                };
                Some(event)
            }
            _ => None,
        }
    }

    /// cash: reads the records waiting in the console, up to the first that is not plain
    /// typing, into `pending`.
    ///
    /// Upstream read one record per call, and every call is a round trip to the console
    /// host: a paste through ConPTY arrives as a key-down and a key-up per character, so a
    /// 2000-character paste cost thousands of them. The batch still stops short of
    /// anything else, which is then read on its own as before: that keeps Reedline's
    /// promise to stop reading at Enter, so keys typed after it stay in the console for
    /// the command about to run.
    fn read_batch(&mut self, available: u32) -> std::io::Result<()> {
        // SAFETY: an all-zero INPUT_RECORD is a valid value of this plain C struct.
        let blank: INPUT_RECORD = unsafe { std::mem::zeroed() };
        let mut records = vec![blank; available.clamp(1, MAX_BATCH) as usize];

        let mut peeked = 0;
        // SAFETY: the buffer holds `records.len()` records and `peeked` is a valid out-param.
        let ok = unsafe {
            PeekConsoleInputW(
                *self.handle,
                records.as_mut_ptr(),
                records.len() as u32,
                &mut peeked,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }

        let take = batch_len(&records[..peeked as usize]);
        let mut read = 0;
        // SAFETY: `take` is at most `records.len()`; `read` is a valid out-param.
        let ok = unsafe { ReadConsoleInputW(*self.handle, records.as_mut_ptr(), take, &mut read) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }

        self.pending.extend(
            records[..read as usize]
                .iter()
                .copied()
                .map(InputRecord::from),
        );
        Ok(())
    }
}

/// cash: the character of `record` when it is a key typed as VT text that needs decoding
/// (patch 2): a key-down with neither a virtual key nor a scan code, carrying a control
/// character, or any character while a sequence is `open`. Printable text needs no
/// decoding and is left to upstream, which types it.
fn vt_text(record: &InputRecord, open: bool) -> Option<u16> {
    match record {
        InputRecord::KeyEvent(key)
            if key.key_down && key.virtual_key_code == 0 && key.virtual_scan_code == 0 =>
        {
            let control = (0x01..0x20).contains(&key.u_char) || key.u_char == 0x7f;
            (open || control).then_some(key.u_char)
        }
        _ => None,
    }
}

/// cash: how many of `records` to read together: the run of plain typing they start
/// with, and at least one record, so that anything else is still read by itself.
fn batch_len(records: &[INPUT_RECORD]) -> u32 {
    records.iter().take_while(|r| is_typing(r)).count().max(1) as u32
}

/// cash: whether `record` only types text, so reading it early changes nothing but
/// when it is seen: a printable character without Ctrl or Alt, a modifier key on its
/// own, or any key being let go. Enter, Tab, editing keys, control characters, Alt
/// and AltGr combinations, and events other than keys all end a batch.
fn is_typing(record: &INPUT_RECORD) -> bool {
    if record.EventType != KEY_EVENT {
        return false;
    }
    // SAFETY: `EventType` says the union holds a key event.
    let key = unsafe { record.Event.KeyEvent() };
    if key.bKeyDown == 0 {
        return true;
    }
    let modifier = [
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
        VK_CAPITAL,
    ];
    if modifier.contains(&i32::from(key.wVirtualKeyCode)) {
        return true;
    }
    // SAFETY: always read as the wide character; crossterm uses the wide API throughout.
    let ch = unsafe { *key.uChar.UnicodeChar() };
    let chord = LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED | LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED;
    ch >= 0x20 && ch != 0x7f && key.dwControlKeyState & chord == 0
}

impl EventSource for WindowsEventSource {
    fn try_read(&mut self, timeout: Option<Duration>) -> std::io::Result<Option<InternalEvent>> {
        let poll_timeout = PollTimeout::new(timeout);

        loop {
            // cash: hand out what the last batch read before touching the console again.
            while let Some(record) = self.pending.pop_front() {
                if let Some(event) = self.next_event(record) {
                    return Ok(Some(InternalEvent::Event(event)));
                }
            }
            // cash: a sequence of VT text that nothing more follows is complete as it is:
            // a lone escape is the Escape key.
            if self.vt.is_open() && self.console.number_of_console_input_events()? == 0 {
                if let Some(key) = self.vt.flush() {
                    return Ok(Some(InternalEvent::Event(Event::Key(key))));
                }
            }

            if let Some(event_ready) = self.poll.poll(poll_timeout.leftover())? {
                let number = self.console.number_of_console_input_events()?;
                if event_ready && number != 0 {
                    self.read_batch(number)?;
                    continue;
                }
            }

            if poll_timeout.elapsed() {
                return Ok(None);
            }
        }
    }

    #[cfg(feature = "event-stream")]
    fn waker(&self) -> Waker {
        self.poll.waker()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winapi::um::wincon::{MOUSE_EVENT, SHIFT_PRESSED};
    use winapi::um::winuser::{VK_BACK, VK_RETURN, VK_TAB};

    fn key(down: bool, vk: i32, ch: u16, state: u32) -> INPUT_RECORD {
        // SAFETY: an all-zero INPUT_RECORD is valid; the key event is then filled in.
        let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
        record.EventType = KEY_EVENT;
        // SAFETY: `EventType` was just set to say the union holds a key event.
        let event = unsafe { record.Event.KeyEvent_mut() };
        event.bKeyDown = i32::from(down);
        event.wRepeatCount = 1;
        event.wVirtualKeyCode = vk as u16;
        // SAFETY: writing the wide character of the union.
        unsafe { *event.uChar.UnicodeChar_mut() = ch };
        event.dwControlKeyState = state;
        record
    }

    fn typed(c: char) -> [INPUT_RECORD; 2] {
        let vk = c.to_ascii_uppercase() as i32;
        [key(true, vk, c as u16, 0), key(false, vk, c as u16, 0)]
    }

    #[test]
    fn plain_typing_is_batched() {
        assert!(is_typing(&key(true, 'A' as i32, 'a' as u16, 0)));
        assert!(is_typing(&key(true, 'A' as i32, 'A' as u16, SHIFT_PRESSED)));
        assert!(is_typing(&key(true, VK_SHIFT, 0, SHIFT_PRESSED)));
        assert!(is_typing(&key(true, 0, ' ' as u16, 0)));
        // Each half of a surrogate pair, which the parser joins in order.
        assert!(is_typing(&key(true, 0, 0xD83E, 0)));
        assert!(is_typing(&key(true, 0, 0xDD80, 0)));
    }

    #[test]
    fn letting_go_of_any_key_is_batched() {
        assert!(is_typing(&key(false, VK_RETURN, '\r' as u16, 0)));
        assert!(is_typing(&key(false, 'C' as i32, 3, LEFT_CTRL_PRESSED)));
    }

    #[test]
    fn keys_that_do_more_than_type_end_a_batch() {
        assert!(!is_typing(&key(true, VK_RETURN, '\r' as u16, 0)));
        assert!(!is_typing(&key(true, VK_TAB, '\t' as u16, 0)));
        assert!(!is_typing(&key(true, VK_BACK, 8, 0)));
        assert!(!is_typing(&key(true, 0x1B, 0x1B, 0)));
        assert!(!is_typing(&key(true, 0x25, 0, 0))); // Left arrow: no character
        assert!(!is_typing(&key(true, 'C' as i32, 3, LEFT_CTRL_PRESSED)));
        assert!(!is_typing(&key(
            true,
            'F' as i32,
            'f' as u16,
            LEFT_ALT_PRESSED
        )));
        // AltGr is Ctrl+Alt: a character, but possibly a binding, so not batched.
        assert!(!is_typing(&key(
            true,
            'Q' as i32,
            '@' as u16,
            LEFT_CTRL_PRESSED | RIGHT_ALT_PRESSED
        )));
        assert!(!is_typing(&key(true, 0, 0x7F, 0)));
    }

    #[test]
    fn other_events_end_a_batch() {
        // SAFETY: an all-zero INPUT_RECORD is valid.
        let mut mouse: INPUT_RECORD = unsafe { std::mem::zeroed() };
        mouse.EventType = MOUSE_EVENT;
        assert!(!is_typing(&mouse));
    }

    #[test]
    fn a_batch_stops_before_enter_so_what_follows_stays_in_the_console() {
        let mut records = Vec::new();
        records.extend(typed('l'));
        records.extend(typed('s'));
        records.push(key(true, VK_RETURN, '\r' as u16, 0));
        records.push(key(false, VK_RETURN, '\r' as u16, 0));
        records.extend(typed('x'));
        assert_eq!(batch_len(&records), 4);
    }

    #[test]
    fn a_batch_that_starts_with_enter_takes_it_alone() {
        let mut records = vec![key(true, VK_RETURN, '\r' as u16, 0)];
        records.extend(typed('x'));
        assert_eq!(batch_len(&records), 1);
    }

    #[test]
    fn a_batch_is_never_empty() {
        assert_eq!(batch_len(&[]), 1);
    }
}
