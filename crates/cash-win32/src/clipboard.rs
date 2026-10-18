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
/// How long opening the clipboard is tried while another program holds it: a clipboard
/// manager reading what was copied, or a program copying, can keep it open past the
/// 200 ms tried before, on a busy desktop.
const OPEN_PATIENCE: Duration = Duration::from_secs(2);
/// The first wait between two attempts, doubled after each up to `OPEN_LONGEST_WAIT`.
const OPEN_FIRST_WAIT: Duration = Duration::from_millis(20);
/// The longest wait between two attempts.
const OPEN_LONGEST_WAIT: Duration = Duration::from_millis(250);
/// What a clipboard another program keeps open is reported as.
const IN_USE: &str = "the clipboard is in use by another program";
/// What a clipboard refused while the session is locked is reported as.
const LOCKED: &str = "the clipboard cannot be used while the session is locked";

/// The clipboard, open to this process until this is dropped.
struct Open;

impl Open {
    /// Opens the clipboard, retrying while another program holds it, for `OPEN_PATIENCE`.
    ///
    /// Windows refuses it with the same answer while the session is locked, when no
    /// program has it and waiting does not help: that is said at once.
    fn new() -> io::Result<Self> {
        let busy = i32::try_from(ERROR_ACCESS_DENIED).ok();
        let started = std::time::Instant::now();
        let mut wait = OPEN_FIRST_WAIT;
        loop {
            // SAFETY: a null window is allowed; the clipboard is then owned by no window.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Ok(Self);
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != busy {
                return Err(error);
            }
            if wait == OPEN_FIRST_WAIT && session_locked() {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, LOCKED));
            }
            if started.elapsed() + wait > OPEN_PATIENCE {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, IN_USE));
            }
            std::thread::sleep(wait);
            wait = (wait * 2).min(OPEN_LONGEST_WAIT);
        }
    }
}

/// Whether this session is locked, when Windows lends no program the clipboard.
///
/// From the session's flags (`WTSQuerySessionInformation`, `WTSSessionInfoEx`);
/// wtsapi32 is loaded only when this is asked, not at every start. False when it cannot
/// be told, as in a service's session, which is never locked.
#[must_use]
pub fn session_locked() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    use windows_sys::Win32::System::RemoteDesktop::{
        WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK, WTSINFOEXW,
        WTSSessionInfoEx,
    };
    type Query = unsafe extern "system" fn(HANDLE, u32, i32, *mut *mut u16, *mut u32) -> i32;
    type Free = unsafe extern "system" fn(*mut core::ffi::c_void);

    // SAFETY: loads a system DLL from System32 alone; it stays loaded.
    let module = unsafe {
        LoadLibraryExW(
            windows_sys::core::w!("wtsapi32.dll"),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if module.is_null() {
        return false;
    }
    // SAFETY: the module is loaded and the name is NUL terminated.
    let address = |name: &core::ffi::CStr| unsafe { GetProcAddress(module, name.as_ptr().cast()) };
    let (Some(query), Some(free)) = (
        address(c"WTSQuerySessionInformationW"),
        address(c"WTSFreeMemory"),
    ) else {
        return false;
    };
    // SAFETY: the documented signature of WTSQuerySessionInformationW.
    let query =
        unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, Query>(query) };
    // SAFETY: the documented signature of WTSFreeMemory.
    let free = unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, Free>(free) };
    let mut buffer: *mut u16 = std::ptr::null_mut();
    let mut bytes = 0u32;
    // SAFETY: the out-pointers are valid; the buffer is freed below.
    let ok = unsafe {
        query(
            WTS_CURRENT_SERVER_HANDLE,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &raw mut buffer,
            &raw mut bytes,
        )
    };
    if ok == 0 || buffer.is_null() {
        return false;
    }
    let locked = bytes as usize >= size_of::<WTSINFOEXW>() && {
        // SAFETY: Windows wrote a WTSINFOEXW of the size it said, level 1 in its union;
        // read as it lies, the buffer's type promising only a `u16`'s alignment.
        let info = unsafe { buffer.cast::<WTSINFOEXW>().read_unaligned() };
        // SAFETY: level 1 is the union's only member.
        info.Level == 1
            && unsafe { info.Data.WTSInfoExLevel1.SessionFlags }.cast_unsigned()
                == WTS_SESSIONSTATE_LOCK
    };
    // SAFETY: the buffer came from the query.
    unsafe { free(buffer.cast()) };
    locked
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

/// Whether the clipboard holds only text, or nothing.
///
/// Text is its three encodings and the locale Windows adds to them: all that
/// [`get_text`] and [`set_text`] can save and put back. A test that borrows the clipboard
/// of whoever runs it asks first, so an image, a file list or formatted text there is
/// not lost.
#[must_use]
pub fn holds_text_only() -> bool {
    use windows_sys::Win32::System::DataExchange::EnumClipboardFormats;
    /// `CF_OEMTEXT` and `CF_LOCALE`, which Windows makes from the others.
    const CF_OEMTEXT: u32 = 7;
    const CF_LOCALE: u32 = 16;
    let Ok(_open) = Open::new() else {
        return false;
    };
    let mut format = 0;
    loop {
        // SAFETY: the clipboard is open to this process; 0 starts the listing.
        format = unsafe { EnumClipboardFormats(format) };
        match format {
            0 => return true,
            CF_TEXT | CF_UNICODETEXT | CF_OEMTEXT | CF_LOCALE => {}
            _ => return false,
        }
    }
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
    fn an_open_waits_out_a_program_that_holds_the_clipboard_half_a_second() {
        let _turn = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        // Another thread stands for another program: the clipboard opened on one thread
        // is closed to the others until it is closed again. 200 ms was all an open tried.
        let (held, holding) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let open = Open::new();
            let _ = held.send(open.is_ok());
            std::thread::sleep(Duration::from_millis(500));
            drop(open);
        });
        if holding.recv() == Ok(true) {
            assert!(Open::new().is_ok());
        }
        let _ = holder.join();
    }

    #[test]
    fn text_round_trips_and_an_empty_text_empties_the_clipboard() {
        let _turn = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        // The clipboard is one per desktop and shared with every program; when another
        // holds it past cash's retries (a clipboard manager, a remote session), or the
        // session is locked, this test cannot run, so it skips rather than fails — the
        // behaviour is covered when the clipboard is free.
        let before = match get_text() {
            Ok(before) => before,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::PermissionDenied
                ) =>
            {
                eprintln!("skipped: {error}");
                return;
            }
            Err(error) => panic!("read the clipboard: {error}"),
        };
        // The test puts back the text it found; an image, a file list or formatted text
        // there would be lost, on the clipboard of whoever runs the tests.
        if !holds_text_only() {
            eprintln!("skipped: the clipboard holds more than text, which this would lose");
            return;
        }

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
