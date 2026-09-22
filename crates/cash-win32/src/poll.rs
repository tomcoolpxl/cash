//! Waiting for input with a deadline — what `read -t` needs.
//!
//! `read -t 5` is ordinary in scripts: a prompt that gives up, a drain loop that stops
//! when a pipe goes quiet, a CI step that will not hang forever. cash reported
//! `poll-based timeout is not supported on this platform` and failed, because the
//! implementation was `poll(2)` and there is no `poll(2)` here.
//!
//! Windows has no single call that answers "is this readable yet" for every handle kind,
//! so this dispatches on what the handle actually is:
//!
//! | Kind | How |
//! |---|---|
//! | Disk file | always ready, as on Unix — bash's `-t` has no effect on a regular file |
//! | Console | `WaitForSingleObject`, which a console input handle signals on |
//! | Pipe | `PeekNamedPipe` on a short cycle, because a pipe is not waitable for *data* |
//!
//! The pipe case is the one Windows makes awkward. A pipe handle is signalled by I/O
//! completion, not by bytes arriving, so waiting on it would return immediately and
//! forever. Peeking on a cycle is what every Windows program that needs this ends up
//! doing; the interval is short enough to feel instant and long enough not to spin a core.

use std::io;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_TYPE_CHAR, FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType,
};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::WaitForSingleObject;

/// How often to look again at a pipe that had nothing in it.
///
/// One millisecond is below the threshold at which a person notices a prompt responding,
/// and far above the cost of a `PeekNamedPipe`.
const PIPE_POLL_INTERVAL: Duration = Duration::from_millis(1);

/// Whether `handle` has input available, waiting up to `timeout` for it.
///
/// `Ok(true)` means there is something to read, `Ok(false)` that the deadline passed
/// first. A zero timeout asks the question without waiting.
///
/// # Errors
///
/// Returns an error if the handle's type cannot be determined, or if the wait fails for
/// a reason other than the deadline.
///
/// # Safety
///
/// `handle` must be a valid, open Win32 handle that outlives the call. Passing a closed
/// or fabricated handle is undefined behaviour in the underlying Win32 calls, which is
/// why this is `unsafe` rather than merely fallible.
pub unsafe fn wait_for_input(handle: HANDLE, timeout: Duration) -> io::Result<bool> {
    // SAFETY: `GetFileType` reads the handle's type from the object manager and touches
    // no memory of ours. An invalid handle yields FILE_TYPE_UNKNOWN rather than faulting.
    let kind = unsafe { GetFileType(handle) };

    match kind {
        // A regular file is always ready. bash does the same: `-t` has no effect on one,
        // because there is nothing to wait for.
        FILE_TYPE_DISK => Ok(true),
        // SAFETY: the caller's contract guarantees the handle; this only waits on it.
        FILE_TYPE_CHAR => Ok(unsafe { wait_on_console(handle, timeout) }),
        // SAFETY: as above.
        FILE_TYPE_PIPE => Ok(unsafe { poll_pipe(handle, timeout) }),
        // Sockets and anything unrecognised: treat as ready rather than claiming a
        // timeout that never happened. A read that then blocks is the pre-existing
        // behaviour, and no worse than refusing outright.
        _ => Ok(true),
    }
}

/// A console input handle *is* waitable: it signals when an input event arrives.
///
/// # Safety
///
/// `handle` must be a valid, open Win32 handle.
unsafe fn wait_on_console(handle: HANDLE, timeout: Duration) -> bool {
    let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);

    // SAFETY: the caller guarantees the handle; the call only waits on it.
    let result = unsafe { WaitForSingleObject(handle, millis) };
    result == WAIT_OBJECT_0
}

/// A pipe is not waitable for data, so ask it how much it holds, on a cycle.
///
/// # Safety
///
/// `handle` must be a valid, open Win32 handle.
unsafe fn poll_pipe(handle: HANDLE, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;

    loop {
        let mut available: u32 = 0;
        // SAFETY: the caller guarantees the handle; every out-parameter is either null
        // or a valid, correctly-typed local, and `PeekNamedPipe` does not consume data.
        let ok = unsafe {
            PeekNamedPipe(
                handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &raw mut available,
                std::ptr::null_mut(),
            )
        };

        if ok == 0 {
            // A broken pipe is not a timeout: the writer is gone, so the read will
            // return end-of-input immediately and the caller should proceed to it.
            return true;
        }

        if available > 0 {
            return true;
        }

        if Instant::now() >= deadline {
            return false;
        }

        std::thread::sleep(
            PIPE_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}
