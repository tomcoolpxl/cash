//! The keyboard's Ctrl-Z, for the foreground job — **D19**.
//!
//! Windows has no Ctrl-Z event. A console control handler hears Ctrl-C, Ctrl-Break, close,
//! logoff and shutdown, and nothing else: Ctrl-Z is a key like any other, a record carrying
//! the character 0x1A in the console's input queue, and whichever program reads the queue
//! gets it. Programs that read the keyboard use it: to `sort`, `copy con` and Python's
//! prompt, Ctrl-Z then Enter ends the input, as Ctrl-D does on Unix.
//!
//! So while a foreground job runs, cash looks at the queue without reading it, and takes a
//! Ctrl-Z only when no program has: one that has waited unread for [`GRACE`]. A program
//! that reads the keyboard takes a key within milliseconds of its arrival, and keeps it;
//! one that does not (`ping`, a build, `terraform apply`) leaves it there, and cash
//! suspends that job as Ctrl-Z does on Unix. Only the Ctrl-Z is taken: the keys typed
//! around it stay in the queue, in order, for whoever reads next.
//!
//! Accepted costs, recorded in D19: a program busy for longer than [`GRACE`] between two
//! reads can be suspended where it would have read the Ctrl-Z itself, and a key that
//! arrives while the queue is being rewritten comes before the ones put back.

use std::io::Write as _;
use std::os::windows::io::AsRawHandle as _;
use std::time::{Duration, Instant};

use windows_sys::Win32::System::Console::{
    GetNumberOfConsoleInputEvents, INPUT_RECORD, KEY_EVENT, PeekConsoleInputW, ReadConsoleInputW,
    WriteConsoleInputW,
};

/// How often the queue is looked at while a foreground job runs.
pub const POLL: Duration = Duration::from_millis(50);

/// How long a Ctrl-Z waits unread before cash takes it.
///
/// Long for a program reading the keyboard, which takes a key within milliseconds; short
/// for a person, who sees the prompt come back as the key is released.
pub const GRACE: Duration = Duration::from_millis(200);

/// The character a Ctrl-Z key carries.
const CTRL_Z: u16 = 0x1A;

/// The most records looked at in one go: far more than anyone types in [`GRACE`].
const PEEK: u32 = 256;

/// Watches this process's console input for a Ctrl-Z that no program reads.
pub struct Watch {
    input: std::fs::File,
    waiting: Option<Waiting>,
}

/// A Ctrl-Z seen waiting in the queue: since when, and how many records the queue held at
/// the last look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Waiting {
    since: Instant,
    queued: u32,
}

impl Watch {
    /// Watches the console this process is attached to; `None` when it has none.
    #[must_use]
    pub fn new() -> Option<Self> {
        let input = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("CONIN$")
            .ok()?;
        Some(Self {
            input,
            waiting: None,
        })
    }

    /// Looks at the queue once, every [`POLL`]. `true` when a Ctrl-Z has waited unread for
    /// [`GRACE`]: it has then been taken out of the queue, and nothing else has.
    pub fn check(&mut self) -> bool {
        let (queued, ctrl_z) = self.peek().unwrap_or((0, false));
        let (waiting, take) = step(self.waiting, Instant::now(), queued, ctrl_z);
        self.waiting = waiting;
        if take {
            self.waiting = None;
            return self.take();
        }
        false
    }

    /// How many records the queue holds, and whether a Ctrl-Z press is among them.
    fn peek(&self) -> Option<(u32, bool)> {
        let handle = self.input.as_raw_handle();
        let mut queued = 0u32;
        // SAFETY: an open console handle owned by `self.input`, and a valid out-parameter.
        if unsafe { GetNumberOfConsoleInputEvents(handle, &raw mut queued) } == 0 {
            return None;
        }
        if queued == 0 {
            return Some((0, false));
        }
        let records = read_records(handle, PEEK.min(queued), false)?;
        Some((queued, records.iter().any(is_ctrl_z_press)))
    }

    /// Takes the Ctrl-Z records out of the queue and puts every other record back, in
    /// order. `false` when there was no Ctrl-Z left to take: a program read it after all.
    fn take(&self) -> bool {
        let handle = self.input.as_raw_handle();
        let mut queued = 0u32;
        // SAFETY: as in `peek`.
        if unsafe { GetNumberOfConsoleInputEvents(handle, &raw mut queued) } == 0 || queued == 0 {
            return false;
        }
        // A read waits while the queue is empty. It holds `queued` records, and no program
        // has read from it for `GRACE`, so this returns at once.
        let Some(records) = read_records(handle, queued, true) else {
            return false;
        };
        let kept: Vec<INPUT_RECORD> = records.iter().copied().filter(|r| !is_ctrl_z(r)).collect();
        let taken = kept.len() < records.len();
        if !kept.is_empty() {
            let mut written = 0u32;
            // `kept` holds at most `queued` records, a u32.
            let len = u32::try_from(kept.len()).unwrap_or(u32::MAX);
            // SAFETY: the handle is open, `kept` holds `len` initialised records, and
            // `written` is a valid out-parameter.
            unsafe { WriteConsoleInputW(handle, kept.as_ptr(), len, &raw mut written) };
        }
        taken
    }
}

/// Up to `count` records from the front of the queue: removed when `remove`, only looked
/// at otherwise.
fn read_records(
    handle: std::os::windows::io::RawHandle,
    count: u32,
    remove: bool,
) -> Option<Vec<INPUT_RECORD>> {
    // SAFETY: an all-zero INPUT_RECORD is a valid value for an out-parameter.
    let blank: INPUT_RECORD = unsafe { std::mem::zeroed() };
    let mut records = vec![blank; count as usize];
    let mut read = 0u32;
    let ok = if remove {
        // SAFETY: the handle is an open console input handle, `records` has room for
        // `count` records, and `read` is a valid out-parameter.
        unsafe { ReadConsoleInputW(handle, records.as_mut_ptr(), count, &raw mut read) }
    } else {
        // SAFETY: as above.
        unsafe { PeekConsoleInputW(handle, records.as_mut_ptr(), count, &raw mut read) }
    };
    if ok == 0 {
        return None;
    }
    records.truncate(read as usize);
    Some(records)
}

/// Whether a record is the Ctrl-Z key, pressed or released. A console with VT input on
/// hands it over as the character alone, with no key code, so the character decides.
fn is_ctrl_z(record: &INPUT_RECORD) -> bool {
    if u32::from(record.EventType) != KEY_EVENT {
        return false;
    }
    // SAFETY: a KEY_EVENT record holds a KeyEvent in its union.
    let key = unsafe { record.Event.KeyEvent };
    // SAFETY: the character is read as UTF-16, the variant the W API fills.
    let character = unsafe { key.uChar.UnicodeChar };
    character == CTRL_Z
}

/// Whether a record is the Ctrl-Z key being pressed.
fn is_ctrl_z_press(record: &INPUT_RECORD) -> bool {
    // SAFETY: `is_ctrl_z` found a KEY_EVENT record, which holds a KeyEvent.
    is_ctrl_z(record) && unsafe { record.Event.KeyEvent.bKeyDown } != 0
}

/// One look at the queue, which holds `queued` records and a Ctrl-Z press when `ctrl_z`:
/// the Ctrl-Z waiting from then on, and whether to take it now.
fn step(
    waiting: Option<Waiting>,
    now: Instant,
    queued: u32,
    ctrl_z: bool,
) -> (Option<Waiting>, bool) {
    if !ctrl_z {
        return (None, false);
    }
    match waiting {
        // Fewer records than at the last look: a program is reading, and may yet read the
        // Ctrl-Z, so its wait starts again.
        Some(seen) if queued >= seen.queued => {
            let waited = now.saturating_duration_since(seen.since) >= GRACE;
            let still = Waiting {
                since: seen.since,
                queued,
            };
            (Some(still), waited)
        }
        _ => (Some(Waiting { since: now, queued }), false),
    }
}

/// Shows `^Z` on the console where the job's output stopped, as a terminal echoes the key
/// that stopped a job, and ends the line.
pub fn echo() {
    if let Ok(mut output) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
        let _ = output.write_all(b"^Z\r\n");
        let _ = output.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(character: u16, down: bool) -> INPUT_RECORD {
        // SAFETY: an all-zero INPUT_RECORD is valid; the fields a key record needs follow.
        let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
        record.EventType = u16::try_from(KEY_EVENT).unwrap_or_default();
        record.Event.KeyEvent.bKeyDown = i32::from(down);
        record.Event.KeyEvent.uChar.UnicodeChar = character;
        record
    }

    #[test]
    fn only_the_ctrl_z_character_is_ctrl_z() {
        assert!(is_ctrl_z_press(&key(CTRL_Z, true)));
        assert!(is_ctrl_z(&key(CTRL_Z, false)));
        assert!(!is_ctrl_z_press(&key(CTRL_Z, false)));
        assert!(!is_ctrl_z(&key(u16::from(b'z'), true)));
        // SAFETY: an all-zero INPUT_RECORD is valid: an event of type 0.
        let focus: INPUT_RECORD = unsafe { std::mem::zeroed() };
        assert!(!is_ctrl_z(&focus));
    }

    #[test]
    fn a_ctrl_z_is_taken_once_it_has_waited_unread() {
        let start = Instant::now();
        let (waiting, take) = step(None, start, 4, true);
        assert!(!take);
        let (waiting, take) = step(waiting, start + GRACE / 2, 4, true);
        assert!(!take, "taken before its grace was up");
        let (_, take) = step(waiting, start + GRACE, 6, true);
        assert!(take, "more keys typed since do not keep it waiting");
    }

    #[test]
    fn a_program_reading_the_queue_gets_more_time() {
        let start = Instant::now();
        let (waiting, _) = step(None, start, 4, true);
        let (waiting, take) = step(waiting, start + GRACE, 3, true);
        assert!(!take, "taken from a program that was reading");
        assert_eq!(waiting.map(|w| w.since), Some(start + GRACE));
    }

    #[test]
    fn a_ctrl_z_a_program_read_is_forgotten() {
        let start = Instant::now();
        let (waiting, _) = step(None, start, 2, true);
        let (waiting, take) = step(waiting, start + GRACE / 2, 0, false);
        assert_eq!((waiting, take), (None, false));
        // A later Ctrl-Z waits its own grace.
        let (waiting, take) = step(waiting, start + GRACE, 2, true);
        assert!(!take);
        assert_eq!(waiting.map(|w| w.since), Some(start + GRACE));
    }
}
