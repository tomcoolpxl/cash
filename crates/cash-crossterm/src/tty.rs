//! Making it a little more convenient and safe to query whether
//! something is a terminal teletype or not.
//! This module defines the IsTty trait and the is_tty method to
//! return true if the item represents a terminal.

use std::os::windows::io::AsRawHandle;

use winapi::um::consoleapi::GetConsoleMode;

/// Adds the `is_tty` method to types that might represent a terminal
///
/// ```rust
/// use std::io::stdout;
/// use crossterm::tty::IsTty;
///
/// let is_tty: bool = stdout().is_tty();
/// ```
pub trait IsTty {
    /// Returns true when an instance is a terminal teletype, otherwise false.
    fn is_tty(&self) -> bool;
}

/// On windows, `GetConsoleMode` will return true if we are in a terminal.
/// Otherwise false.
impl<S: AsRawHandle> IsTty for S {
    fn is_tty(&self) -> bool {
        let mut mode = 0;
        // SAFETY: any handle may be asked; the mode is written to a local that outlives it.
        let ok = unsafe { GetConsoleMode(self.as_raw_handle() as *mut _, &mut mode) };
        ok == 1
    }
}
