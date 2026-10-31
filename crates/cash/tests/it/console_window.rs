//! A cash of its own in a new, minimized console window, which a test attaches to and
//! types into: for what shows only at a console with a window, the font it draws with
//! and the keys its queue holds. The test runner starts each test in a process of its
//! own, so letting go of the runner's console harms no other test.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test helper fails loudly"
)]

use std::os::windows::io::AsRawHandle as _;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Console::{
    AttachConsole, CONSOLE_SCREEN_BUFFER_INFO, COORD, FreeConsole, GetConsoleScreenBufferInfo,
    INPUT_RECORD, KEY_EVENT, ReadConsoleOutputCharacterW, WriteConsoleInputW,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, PROCESS_INFORMATION,
    STARTF_USESHOWWINDOW, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, VkKeyScanW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_MINIMIZE;

use crate::common::{CASH, with_isolated_environment};

/// The console output of the console this process is attached to.
pub fn conout() -> std::fs::File {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONOUT$")
        .expect("open the console output")
}

/// Types `text` into the attached console as key events; a `\r` in it is the Enter key.
pub fn type_keys(text: &str) {
    let input = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")
        .expect("open the console input");
    let mut records = Vec::new();
    for ch in text.chars() {
        let unit = u16::try_from(u32::from(ch)).expect("a BMP character");
        let key = if ch == '\r' {
            0x0D
        } else {
            // SAFETY: a plain call with a scalar argument. The low byte is the key.
            u16::try_from(unsafe { VkKeyScanW(unit) } & 0xFF).unwrap()
        };
        for down in [1, 0] {
            // SAFETY: a plain record of integers, filled in below.
            let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
            record.EventType = u16::try_from(KEY_EVENT).unwrap();
            record.Event.KeyEvent.bKeyDown = down;
            record.Event.KeyEvent.wRepeatCount = 1;
            record.Event.KeyEvent.wVirtualKeyCode = key;
            // SAFETY: a plain call with scalar arguments.
            record.Event.KeyEvent.wVirtualScanCode =
                u16::try_from(unsafe { MapVirtualKeyW(u32::from(key), 0) }).unwrap_or(0);
            record.Event.KeyEvent.uChar.UnicodeChar = unit;
            records.push(record);
        }
    }
    let mut written = 0u32;
    // SAFETY: an open console input handle, a valid array of records and its length.
    let ok = unsafe {
        WriteConsoleInputW(
            input.as_raw_handle(),
            records.as_ptr(),
            u32::try_from(records.len()).unwrap(),
            &raw mut written,
        )
    };
    assert_ne!(
        ok,
        0,
        "WriteConsoleInput failed: {}",
        std::io::Error::last_os_error()
    );
}

/// Types `text` and Enter into the attached console.
pub fn type_line(text: &str) {
    type_keys(&format!("{text}\r"));
}

/// Starts cash with `arguments` in a new, minimized console window, gives it `settle` to
/// start, and attaches this process to that console. Returns the process.
pub fn start_cash_in_its_own_console(arguments: &str, settle: Duration) -> PROCESS_INFORMATION {
    // The test's environment as `cash_command()` gives it, and no first-start offer: that
    // one changes the user's PATH, and must never run from a test.
    let mut environment: Vec<u16> = with_isolated_environment(|vars| {
        let mut block = Vec::new();
        for (name, value) in vars.iter().chain(&[("CASH_NO_OFFER", "1")]) {
            block.extend(format!("{name}={value}").encode_utf16());
            block.push(0);
        }
        block
    });
    environment.push(0);
    let mut command_line: Vec<u16> = format!("\"{CASH}\" {arguments}").encode_utf16().collect();
    command_line.push(0);
    // SAFETY: plain records of integers, filled in below.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = u32::try_from(std::mem::size_of::<STARTUPINFOW>()).unwrap();
    startup.dwFlags = STARTF_USESHOWWINDOW;
    startup.wShowWindow = u16::try_from(SW_MINIMIZE).unwrap();
    // SAFETY: as above.
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: a NUL-terminated, writable command line, a doubly NUL-terminated UTF-16
    // environment block, and valid startup and process records.
    let ok = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            FALSE,
            CREATE_NEW_CONSOLE | CREATE_UNICODE_ENVIRONMENT,
            environment.as_mut_ptr().cast(),
            std::ptr::null(),
            &raw const startup,
            &raw mut process,
        )
    };
    assert_ne!(
        ok,
        0,
        "CreateProcess failed: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: a handle CreateProcess returned, not used again.
    unsafe { CloseHandle(process.hThread) };
    std::thread::sleep(settle);
    // SAFETY: plain calls; letting go of the runner's console, then taking cash's.
    unsafe { FreeConsole() };
    // SAFETY: as above.
    let attached = unsafe { AttachConsole(process.dwProcessId) };
    assert_ne!(
        attached,
        0,
        "AttachConsole failed: {}",
        std::io::Error::last_os_error()
    );
    process
}

/// Waits up to `timeout` for the process to end by itself; `false` when it did not.
pub fn wait_for_end(process: &PROCESS_INFORMATION, timeout: Duration) -> bool {
    let millis = u32::try_from(timeout.as_millis()).unwrap();
    // SAFETY: a process handle the test owns.
    unsafe { WaitForSingleObject(process.hProcess, millis) == WAIT_OBJECT_0 }
}

/// Lets the process and its console go; one still running is terminated.
pub fn let_go(process: &PROCESS_INFORMATION) {
    if !wait_for_end(process, Duration::ZERO) {
        // SAFETY: a process handle the test owns.
        unsafe { TerminateProcess(process.hProcess, 1) };
    }
    // SAFETY: as above, closed once.
    unsafe { CloseHandle(process.hProcess) };
    // SAFETY: a plain call.
    unsafe { FreeConsole() };
}

/// Ends an interactive cash with `exit`, then lets it and its console go.
pub fn end(process: &PROCESS_INFORMATION) {
    type_line("exit");
    wait_for_end(process, Duration::from_secs(10));
    let_go(process);
}

/// The lines the attached console's screen shows, from its top row to the cursor's,
/// trailing blanks trimmed and empty rows left out.
pub fn screen_lines() -> Vec<String> {
    let output = conout();
    // SAFETY: a plain record of integers, which the call fills in.
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: an open console output handle, and a valid out-parameter.
    let ok = unsafe { GetConsoleScreenBufferInfo(output.as_raw_handle(), &raw mut info) };
    assert_ne!(
        ok,
        0,
        "GetConsoleScreenBufferInfo failed: {}",
        std::io::Error::last_os_error()
    );
    let width = usize::try_from(info.dwSize.X).unwrap();
    let mut lines = Vec::new();
    for row in 0..=info.dwCursorPosition.Y {
        let mut units = vec![0u16; width];
        let mut read = 0u32;
        // SAFETY: an open console output handle, room for `width` units, and a valid
        // out-parameter.
        let ok = unsafe {
            ReadConsoleOutputCharacterW(
                output.as_raw_handle(),
                units.as_mut_ptr(),
                u32::try_from(width).unwrap(),
                COORD { X: 0, Y: row },
                &raw mut read,
            )
        };
        assert_ne!(
            ok,
            0,
            "ReadConsoleOutputCharacter failed: {}",
            std::io::Error::last_os_error()
        );
        let text = String::from_utf16_lossy(&units[..usize::try_from(read).unwrap()])
            .trim_end()
            .to_owned();
        if !text.is_empty() {
            lines.push(text);
        }
    }
    lines
}
