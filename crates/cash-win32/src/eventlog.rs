//! Writing to the Application event log, Windows' system log, for `logger`.
//!
//! The source is `cash`, which is not registered: registering one writes to
//! `HKLM\SYSTEM\CurrentControlSet\Services\EventLog`, which needs an administrator. Windows
//! takes the entries anyway, and Event Viewer shows each with a note that the description
//! for its event id cannot be found, followed by the message itself.

use std::io;

use windows_sys::Win32::System::EventLog::{
    DeregisterEventSource, EVENTLOG_ERROR_TYPE, EVENTLOG_INFORMATION_TYPE, EVENTLOG_WARNING_TYPE,
    REPORT_EVENT_TYPE, RegisterEventSourceW, ReportEventW,
};

use crate::wide::to_wide_nul;

/// The source the entries are logged under.
pub const SOURCE: &str = "cash";

/// The event id every entry gets: with no message file registered, it names nothing.
const EVENT_ID: u32 = 1;

/// How Event Viewer marks an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An error: syslog's `emerg` to `err`.
    Error,
    /// A warning: syslog's `warning`.
    Warning,
    /// Information: syslog's `notice`, `info` and `debug`.
    Information,
}

impl Kind {
    const fn event_type(self) -> REPORT_EVENT_TYPE {
        match self {
            Self::Error => EVENTLOG_ERROR_TYPE,
            Self::Warning => EVENTLOG_WARNING_TYPE,
            Self::Information => EVENTLOG_INFORMATION_TYPE,
        }
    }
}

/// Writes `message` to the Application event log under [`SOURCE`], marked as `kind`.
///
/// # Errors
///
/// The log could not be opened for the source, or Windows refused the entry.
pub fn report(kind: Kind, message: &str) -> io::Result<()> {
    let source = to_wide_nul(SOURCE);
    // SAFETY: a null server is this machine, and `source` is NUL-terminated.
    let log = unsafe { RegisterEventSourceW(std::ptr::null(), source.as_ptr()) };
    if log.is_null() {
        return Err(io::Error::last_os_error());
    }
    let text = to_wide_nul(message);
    let strings = [text.as_ptr()];
    // SAFETY: `log` was just opened; `strings` holds one NUL-terminated string, which
    // outlives the call; no user, no raw data.
    let written = unsafe {
        ReportEventW(
            log,
            kind.event_type(),
            0,
            EVENT_ID,
            std::ptr::null_mut(),
            1,
            0,
            strings.as_ptr(),
            std::ptr::null(),
        )
    } != 0;
    let error = io::Error::last_os_error();
    // SAFETY: closes the handle opened above, once.
    unsafe { DeregisterEventSource(log) };
    if written { Ok(()) } else { Err(error) }
}
