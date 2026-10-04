//! Owned Win32 handles: one type, std's [`OwnedHandle`], which closes its handle when it
//! is dropped.
//!
//! The crate had a type of its own for each kind of handle (a job, a held process, a
//! process opened for a query, a pseudo console's child) and closed the rest by hand, and
//! a `?` or an early return between the open and the close leaked the handle
//! (`REVIEW_REPORT.md` W32-13, W32-14). A handle is made an [`OwnedHandle`] as soon as the
//! call that opened it returns, and these take it from the two ways Win32 reports a
//! failed open.

use std::io;
use std::os::windows::io::{FromRawHandle as _, OwnedHandle};

use windows_sys::Win32::Foundation::{FALSE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Threading::OpenProcess;

/// Takes `handle` from a call that returns null on failure (`OpenProcess`,
/// `CreateJobObjectW`, …): the call's error for null.
///
/// # Safety
///
/// `handle` is the value the call just returned, null or an open handle the caller owns
/// and closes nowhere else; nothing has run in between that may set the last error.
pub(crate) unsafe fn from_null(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: not null, so an open handle that the caller hands over.
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

/// Takes `handle` from a call that returns `INVALID_HANDLE_VALUE` on failure
/// (`CreateFileW`, `CreateNamedPipeW`, `CreateToolhelp32Snapshot`, …): the call's error
/// for that.
///
/// # Safety
///
/// As for [`from_null`], with `INVALID_HANDLE_VALUE` for null.
pub(crate) unsafe fn from_invalid(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: a valid handle, which the caller hands over.
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

/// Opens the process `pid` with `access`.
///
/// # Errors
///
/// Returns the error of `OpenProcess`: there is no such process, or it may not be opened
/// so.
pub(crate) fn open_process(pid: u32, access: u32) -> io::Result<OwnedHandle> {
    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(access, FALSE, pid) };
    // SAFETY: just returned, and nothing has run since.
    unsafe { from_null(handle) }
}

#[cfg(test)]
mod tests {
    use std::os::windows::io::AsRawHandle as _;

    use windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;

    use super::*;

    #[test]
    fn a_failed_open_is_its_error_and_a_good_one_closes_on_drop() {
        // Process ids are multiples of four, so this one never exists.
        let missing = open_process(3, PROCESS_QUERY_LIMITED_INFORMATION).unwrap_err();
        assert_eq!(missing.raw_os_error(), Some(87), "{missing}");

        let process = open_process(std::process::id(), PROCESS_QUERY_LIMITED_INFORMATION).unwrap();
        assert!(!process.as_raw_handle().is_null());
        // SAFETY: null is what a failed open returns.
        assert!(unsafe { from_null(std::ptr::null_mut()) }.is_err());
        // SAFETY: as is `INVALID_HANDLE_VALUE`, for the other kind.
        assert!(unsafe { from_invalid(INVALID_HANDLE_VALUE) }.is_err());
        drop(process);
    }
}
