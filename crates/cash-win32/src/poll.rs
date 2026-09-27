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
use windows_sys::Win32::System::Console::{
    INPUT_RECORD, KEY_EVENT, PeekConsoleInputW, ReadConsoleInputW,
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

/// A console input handle *is* waitable, but it signals on any input record: a key
/// released, focus gained or lost, the mouse, a resize. Only a key pressed with a
/// character is input a read can take, so the others at the head of the queue are
/// dropped and the wait goes on for what is left of the timeout. Without this, the
/// release of the Enter that ran `read -t 0.3` woke the wait at once, and the read
/// then blocked for a whole line, never timing out.
///
/// # Safety
///
/// `handle` must be a valid, open Win32 handle.
unsafe fn wait_on_console(handle: HANDLE, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        // SAFETY: the caller guarantees the handle.
        match unsafe { discard_until_character(handle) } {
            // A character is waiting, or this is not a console input handle whose
            // records can be read: let the read decide.
            Some(true) | None => return true,
            Some(false) => {}
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let millis = u32::try_from(remaining.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the caller guarantees the handle; the call only waits on it.
        if unsafe { WaitForSingleObject(handle, millis) } != WAIT_OBJECT_0 {
            return false;
        }
    }
}

/// Drops the input records at the head of the console's queue that carry no character,
/// stopping at one that does. `Some(true)` if a character is waiting, `Some(false)` if
/// the queue is empty, `None` if the handle's records cannot be read.
///
/// # Safety
///
/// `handle` must be a valid, open Win32 handle.
unsafe fn discard_until_character(handle: HANDLE) -> Option<bool> {
    loop {
        // SAFETY: an all-zero INPUT_RECORD is a valid value for the out-parameter.
        let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
        let mut count = 0u32;
        // SAFETY: the handle is valid and `record` has room for the one record asked for.
        if unsafe { PeekConsoleInputW(handle, &raw mut record, 1, &raw mut count) } == 0 {
            return None;
        }
        if count == 0 {
            return Some(false);
        }
        if u32::from(record.EventType) == KEY_EVENT {
            // SAFETY: a KEY_EVENT record holds a KeyEvent in its union.
            let key = unsafe { record.Event.KeyEvent };
            // SAFETY: the character union is read as UTF-16, the variant the W API fills.
            if key.bKeyDown != 0 && unsafe { key.uChar.UnicodeChar } != 0 {
                return Some(true);
            }
        }
        // SAFETY: as for the peek; this removes the record just looked at.
        if unsafe { ReadConsoleInputW(handle, &raw mut record, 1, &raw mut count) } == 0 {
            return None;
        }
    }
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
