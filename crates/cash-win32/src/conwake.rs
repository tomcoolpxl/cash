//! Ending a thread's wait for a line typed at the console.
//!
//! For a builtin that reads the console on a thread of its own while it does something
//! else (`nc`). A wait on the console's input queue ([`crate::conin::Keys`]) ends only when a record
//! arrives, and nothing else can end it: the handle is no file to cancel an I/O on. So the
//! thread that waits is woken the way a user would wake it, with a key — Ctrl-D, which a
//! line reader takes as the end of its input and shows nowhere. The reader hands over
//! what there was of the line, or says the input ended, and the thread sees it was asked
//! to stop and does.

use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle as _};

use windows_sys::Win32::System::Console::{INPUT_RECORD, KEY_EVENT, WriteConsoleInputW};

/// The character the record types: Ctrl-D, which [`crate::conin::Terminal`] takes as the
/// end of input.
const CTRL_D: u16 = 0x04;
/// The virtual key of `D`, which Ctrl-D is typed on.
const VK_D: u16 = 0x44;
/// The console's `LEFT_CTRL_PRESSED` control-key state.
const LEFT_CTRL_PRESSED: u32 = 0x0008;

/// Types Ctrl-D into the console `input` is a handle to, so that a reader waiting for a
/// line on it is handed what it has and returns.
///
/// Only to be used while such a reader is known to be waiting: typed into an idle
/// console, the key would reach the next reader.
///
/// # Errors
///
/// Returns an error if `input` is not a console's input, or the record could not be
/// written.
pub fn end_line_read(input: &impl AsHandle) -> io::Result<()> {
    // SAFETY: an all-zero INPUT_RECORD is valid; the key's fields follow.
    let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
    record.EventType = u16::try_from(KEY_EVENT).unwrap_or_default();
    record.Event.KeyEvent.bKeyDown = 1;
    record.Event.KeyEvent.wRepeatCount = 1;
    record.Event.KeyEvent.wVirtualKeyCode = VK_D;
    record.Event.KeyEvent.dwControlKeyState = LEFT_CTRL_PRESSED;
    record.Event.KeyEvent.uChar.UnicodeChar = CTRL_D;
    let mut written = 0u32;
    // SAFETY: an open handle owned by `input`, one initialised record, and a valid
    // out-parameter. Anything but a console's input makes the call fail.
    let ok = unsafe {
        WriteConsoleInputW(
            input.as_handle().as_raw_handle(),
            &raw const record,
            1,
            &raw mut written,
        )
    };
    if ok == 0 || written != 1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
