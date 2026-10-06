//! Waiting for input with a deadline, for `read -t`.
//!
//! The Unix module uses `poll(2)`; Windows has no equivalent that covers every handle
//! kind, so the dispatch lives in `cash_win32::poll` and this adapts an [`OpenFile`] to
//! it. Before this, `read -t 5` failed outright with "poll-based timeout is not supported
//! on this platform" — a common enough idiom that a prompt with a fallback, or a drain
//! loop that stops when a pipe goes quiet, simply could not be written.

use std::time::Duration;

use crate::openfiles::OpenFile;

/// Polls an open file for input readability with a timeout.
///
/// Returns `Ok(true)` if data is available, `Ok(false)` if the timeout elapsed first.
///
/// # Errors
///
/// Returns an error if the underlying handle cannot be obtained or the wait fails.
pub fn poll_for_input(file: &OpenFile, timeout: Duration) -> std::io::Result<bool> {
    if let OpenFile::Socket(socket) = file {
        return socket_has_input(socket, timeout);
    }
    let Some(handle) = raw_handle(file) else {
        // Nothing to wait on — a stream with no OS handle behind it. Report ready and
        // let the read decide, rather than reporting a timeout that did not happen.
        return Ok(true);
    };

    // SAFETY: the handle came from an `OpenFile` this call borrows, so it is open and
    // outlives the wait.
    unsafe { cash_win32::poll::wait_for_input(handle, timeout) }
}

/// Whether a `/dev/tcp` or `/dev/udp` socket has bytes to read, waiting up to `timeout`
/// for them (`select`, in `cash_win32::sockets`). A connection the peer has closed is
/// ready too, so the read finds its end.
fn socket_has_input(socket: &crate::net::Socket, timeout: Duration) -> std::io::Result<bool> {
    cash_win32::sockets::wait_readable(socket.as_raw_socket(), timeout)
}

/// The Win32 handle behind an open file, where there is one.
fn raw_handle(file: &OpenFile) -> Option<*mut std::ffi::c_void> {
    use std::os::windows::io::AsRawHandle as _;

    let raw = match file {
        OpenFile::Stdin(f) => f.as_raw_handle(),
        OpenFile::Stdout(f) => f.as_raw_handle(),
        OpenFile::Stderr(f) => f.as_raw_handle(),
        OpenFile::File(f) => f.as_raw_handle(),
        OpenFile::PipeReader(r) => r.as_raw_handle(),
        OpenFile::PipeWriter(w) => w.as_raw_handle(),
        // A socket is waited on as a socket, not as a handle: `read -t` on a `/dev/tcp`
        // descriptor goes straight to the read, which blocks until bytes arrive.
        OpenFile::Socket(_) => return None,
        // A generic stream is not guaranteed to be handle-backed.
        OpenFile::Stream(_) => return None,
    };

    Some(raw.cast())
}
