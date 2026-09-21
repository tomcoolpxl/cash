//! Process queries that the job-object layer and D42's elevated-child tracking need.

use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// `GetExitCodeProcess` reports this while a process is still running.
const STILL_ACTIVE: u32 = 259;

/// Whether a process ID currently refers to a running process.
///
/// Used to verify job teardown, and by D42 to check whether a tracked elevated child —
/// which cannot be assigned to cash's job object — is still alive at exit.
///
/// PIDs are reused by Windows, so a `true` result means "some process with this id is
/// running", not necessarily the one you started. Callers that need certainty should
/// hold a handle instead.
#[must_use]
pub fn is_pid_alive(pid: u32) -> bool {
    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if handle.is_null() {
        return false;
    }

    let mut code: u32 = 0;
    // SAFETY: handle is valid here, and `code` is a valid out-param.
    let ok = unsafe { GetExitCodeProcess(handle, &raw mut code) };
    // SAFETY: closing a handle we just opened, exactly once.
    unsafe {
        CloseHandle(handle);
    }

    ok != 0 && code == STILL_ACTIVE
}
