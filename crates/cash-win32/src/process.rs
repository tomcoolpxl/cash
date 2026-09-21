//! Process queries that the job-object layer and D42's elevated-child tracking need.

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, FILETIME};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
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

/// Total CPU time a process has consumed, in 100-nanosecond units.
///
/// The honest way to verify D19's suspend actually works: a suspended process stops
/// accruing CPU time immediately and deterministically, whereas observing side effects
/// like file writes is slow and racy.
///
/// Returns `None` if the process cannot be opened, which usually means it has exited.
#[must_use]
pub fn cpu_time(pid: u32) -> Option<u64> {
    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if handle.is_null() {
        return None;
    }

    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut kernel = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut user = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };

    // SAFETY: handle is valid and all four out-params are valid FILETIMEs.
    let ok = unsafe {
        GetProcessTimes(
            handle,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };
    // SAFETY: closing a handle we just opened, exactly once.
    unsafe {
        CloseHandle(handle);
    }

    if ok == 0 {
        return None;
    }

    Some(as_u64(kernel) + as_u64(user))
}

/// Combine a `FILETIME`'s halves into a single count of 100ns intervals.
const fn as_u64(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | (time.dwLowDateTime as u64)
}

/// Terminate a process immediately (`kill -9`, D21).
///
/// Uncatchable, as `SIGKILL` is on POSIX. `TerminateProcess` runs no cleanup in the
/// target, which is the point: this is what you reach for when asking politely has
/// already failed.
pub fn terminate(pid: u32) -> std::io::Result<()> {
    use windows_sys::Win32::System::Threading::{PROCESS_TERMINATE, TerminateProcess};

    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, FALSE, pid) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }

    // SAFETY: handle is valid and carries PROCESS_TERMINATE.
    let ok = unsafe { TerminateProcess(handle, 1) };
    // SAFETY: closing a handle we just opened, exactly once.
    unsafe {
        CloseHandle(handle);
    }

    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
