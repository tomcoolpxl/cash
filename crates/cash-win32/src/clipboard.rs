//! The clipboard's text, for `pbcopy` and `pbpaste`.
//!
//! A process needs no window to use the clipboard: `OpenClipboard` with a null window
//! owns it for the moment, and that is enough to read or replace its text. Another
//! program may hold the clipboard open for an instant (a password manager, a clipboard
//! history), so opening is retried a few times before it is reported as in use.
//!
//! Text goes in as `CF_UNICODETEXT`, which every program reads, and comes out as that
//! or, when only an old program's `CF_TEXT` is there, as the ANSI code page's bytes
//! decoded through `MultiByteToWideChar`.

use std::io;
use std::time::Duration;

use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, GlobalFree, HANDLE};
use windows_sys::Win32::Globalization::{CP_ACP, MultiByteToWideChar};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};

/// `CF_TEXT`: text in the ANSI code page, NUL-terminated.
const CF_TEXT: u32 = 1;
/// `CF_UNICODETEXT`: UTF-16, NUL-terminated.
const CF_UNICODETEXT: u32 = 13;
/// How many times opening the clipboard is tried while another program holds it.
const OPEN_ATTEMPTS: u32 = 10;
/// The wait between two attempts.
const OPEN_RETRY_WAIT: Duration = Duration::from_millis(20);
/// What a clipboard another program keeps open is reported as.
const IN_USE: &str = "the clipboard is in use by another program";

/// The clipboard, open to this process until this is dropped.
struct Open;

impl Open {
    /// Opens the clipboard, retrying while another program holds it.
    fn new() -> io::Result<Self> {
        let busy = i32::try_from(ERROR_ACCESS_DENIED).ok();
        for attempt in 1..=OPEN_ATTEMPTS {
            // SAFETY: a null window is allowed; the clipboard is then owned by no window.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Ok(Self);
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != busy {
                return Err(error);
            }
            if attempt < OPEN_ATTEMPTS {
                std::thread::sleep(OPEN_RETRY_WAIT);
            }
        }
        Err(io::Error::new(io::ErrorKind::WouldBlock, IN_USE))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: pairs with the successful `OpenClipboard` in `new`.
        unsafe { CloseClipboard() };
    }
}

/// Puts `text` on the clipboard as `CF_UNICODETEXT`, replacing whatever was there; an
/// empty `text` leaves the clipboard empty.
///
/// Line endings go as given: a caller that wants Windows programs to see lines gives
/// CRLF.
pub fn set_text(text: &str) -> io::Result<()> {
    let _open = Open::new()?;
    // SAFETY: the clipboard is open to this process.
    if unsafe { EmptyClipboard() } == 0 {
        return Err(io::Error::last_os_error());
    }
    if text.is_empty() {
        return Ok(());
    }
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = units.len() * size_of::<u16>();
    // SAFETY: a plain allocation request; a movable block is what the clipboard takes.
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
    if memory.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `memory` is the block just allocated.
    let pointer = unsafe { GlobalLock(memory) }.cast::<u16>();
    if pointer.is_null() {
        let error = io::Error::last_os_error();
        free(memory);
        return Err(error);
    }
    // SAFETY: the block holds `units.len()` units, is locked so it does not move, and
    // global memory is aligned to at least eight bytes.
    unsafe { std::ptr::copy_nonoverlapping(units.as_ptr(), pointer, units.len()) };
    // SAFETY: pairs with the lock above.
    unsafe { GlobalUnlock(memory) };
    // SAFETY: the clipboard is open, and on success it owns the block from here on.
    if unsafe { SetClipboardData(CF_UNICODETEXT, memory) }.is_null() {
        let error = io::Error::last_os_error();
        free(memory);
        return Err(error);
    }
    Ok(())
}

/// Frees a block the clipboard did not take.
fn free(memory: HANDLE) {
    // SAFETY: the block is this function's own, allocated with `GlobalAlloc` and not
    // handed to the clipboard.
    unsafe { GlobalFree(memory) };
}

/// The clipboard's text: `CF_UNICODETEXT`, else `CF_TEXT` decoded from the ANSI code
/// page, else `None` when it holds no text (an image, files, or nothing).
///
/// Line endings come as they are on the clipboard, CRLF from most Windows programs.
pub fn get_text() -> io::Result<Option<String>> {
    let _open = Open::new()?;
    // SAFETY: the clipboard is open to this process.
    let unicode = unsafe { GetClipboardData(CF_UNICODETEXT) };
    if !unicode.is_null() {
        let bytes = block_bytes(unicode)?;
        let (pairs, _odd) = bytes.as_chunks::<2>();
        let units: Vec<u16> = pairs
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .take_while(|&unit| unit != 0)
            .collect();
        return Ok(Some(String::from_utf16_lossy(&units)));
    }
    // SAFETY: as above.
    let ansi = unsafe { GetClipboardData(CF_TEXT) };
    if ansi.is_null() {
        return Ok(None);
    }
    let bytes = block_bytes(ansi)?;
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    ansi_text(bytes.get(..end).unwrap_or_default()).map(Some)
}

/// A copy of the global memory block `memory`, which the clipboard owns.
fn block_bytes(memory: HANDLE) -> io::Result<Vec<u8>> {
    // SAFETY: the handle is a global memory block the clipboard owns while it is open.
    let size = unsafe { GlobalSize(memory) };
    // SAFETY: as above.
    let pointer = unsafe { GlobalLock(memory) }.cast::<u8>();
    if pointer.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the block is locked, so it holds still, and `GlobalSize` is its length.
    let bytes = unsafe { std::slice::from_raw_parts(pointer, size) }.to_vec();
    // SAFETY: pairs with the lock above.
    unsafe { GlobalUnlock(memory) };
    Ok(bytes)
}

/// `bytes` in the ANSI code page as text.
fn ansi_text(bytes: &[u8]) -> io::Result<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    let length = i32::try_from(bytes.len()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "the clipboard text is too long")
    })?;
    // SAFETY: the input pointer is valid for `length` bytes; a null buffer with no
    // capacity asks for the size.
    let needed =
        unsafe { MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), length, std::ptr::null_mut(), 0) };
    let capacity = usize::try_from(needed).unwrap_or(0);
    if capacity == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut units = vec![0u16; capacity];
    // SAFETY: the buffer holds `needed` units, which the sizing call asked for.
    let written = unsafe {
        MultiByteToWideChar(
            CP_ACP,
            0,
            bytes.as_ptr(),
            length,
            units.as_mut_ptr(),
            needed,
        )
    };
    let Ok(written) = usize::try_from(written) else {
        return Err(io::Error::last_os_error());
    };
    units.truncate(written);
    Ok(String::from_utf16_lossy(&units))
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, PoisonError};

    use super::*;

    /// The clipboard is one per desktop, so the tests take turns.
    static CLIPBOARD: Mutex<()> = Mutex::new(());

    #[test]
    fn text_round_trips_and_an_empty_text_empties_the_clipboard() {
        let _turn = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let before = get_text().unwrap();

        let text = format!("cash clipboard test {}\r\nhéllo €\r\n", std::process::id());
        set_text(&text).unwrap();
        assert_eq!(get_text().unwrap(), Some(text));

        set_text("").unwrap();
        assert_eq!(get_text().unwrap(), None);

        set_text(before.as_deref().unwrap_or_default()).unwrap();
    }

    #[test]
    fn ansi_bytes_decode_and_empty_bytes_are_empty_text() {
        assert_eq!(ansi_text(b"").unwrap(), "");
        assert_eq!(ansi_text(b"abc").unwrap(), "abc");
    }
}
