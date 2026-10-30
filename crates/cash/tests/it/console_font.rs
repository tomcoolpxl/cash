//! The font of a Windows Console window after a program (D68): Windows PowerShell 5.1
//! switches a console whose code page is UTF-8, as cash's is, to the raster `Terminal`
//! font, and the whole window is redrawn in it (seen 2026-10-24 in Windows Sandbox). cash
//! puts the font it started with back at the next prompt.
//!
//! A console with no window draws with no font and ignores a font set on it, so cash is
//! started in a new console, minimized, which this test then attaches to; the raster
//! font is set from here, as PowerShell does, and a program is run through cash by
//! typing into the console, since the font is only put back after a program ran. The
//! test runner starts each test in a process of its own, so letting go of the runner's
//! console harms no other test.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::os::windows::io::AsRawHandle as _;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Console::{
    AttachConsole, CONSOLE_FONT_INFOEX, FreeConsole, GetCurrentConsoleFontEx, INPUT_RECORD,
    KEY_EVENT, SetCurrentConsoleFontEx, WriteConsoleInputW,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, PROCESS_INFORMATION,
    STARTF_USESHOWWINDOW, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, VkKeyScanW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_MINIMIZE;

use crate::common::{CASH, with_isolated_environment};

/// `TMPF_TRUETYPE` in a font's family.
const TRUETYPE: u32 = 0x04;

/// The console output of the console this process is attached to.
fn conout() -> std::fs::File {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONOUT$")
        .expect("open the console output")
}

fn font() -> CONSOLE_FONT_INFOEX {
    // SAFETY: a plain record of integers and a name, which the call fills in.
    let mut font: CONSOLE_FONT_INFOEX = unsafe { std::mem::zeroed() };
    font.cbSize = u32::try_from(std::mem::size_of::<CONSOLE_FONT_INFOEX>()).unwrap();
    let output = conout();
    // SAFETY: an open console handle, and a valid out-parameter whose size is set.
    let ok = unsafe { GetCurrentConsoleFontEx(output.as_raw_handle(), FALSE, &raw mut font) };
    assert_ne!(
        ok,
        0,
        "GetCurrentConsoleFontEx failed: {}",
        std::io::Error::last_os_error()
    );
    font
}

fn face(font: &CONSOLE_FONT_INFOEX) -> String {
    let name = font
        .FaceName
        .iter()
        .take_while(|&&unit| unit != 0)
        .copied()
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&name)
}

fn describe(font: &CONSOLE_FONT_INFOEX) -> String {
    format!(
        "{} {}x{} family 0x{:x}",
        face(font),
        font.dwFontSize.X,
        font.dwFontSize.Y,
        font.FontFamily
    )
}

/// Sets the raster `Terminal` font, 8 by 12, as Windows PowerShell 5.1 leaves it.
fn set_raster_font() {
    // SAFETY: as in `font`.
    let mut wanted: CONSOLE_FONT_INFOEX = unsafe { std::mem::zeroed() };
    wanted.cbSize = u32::try_from(std::mem::size_of::<CONSOLE_FONT_INFOEX>()).unwrap();
    wanted.dwFontSize.X = 8;
    wanted.dwFontSize.Y = 12;
    wanted.FontFamily = 0x30;
    wanted.FontWeight = 400;
    for (slot, unit) in wanted.FaceName.iter_mut().zip("Terminal".encode_utf16()) {
        *slot = unit;
    }
    let output = conout();
    // SAFETY: an open console handle, and a filled-in font record.
    let ok = unsafe { SetCurrentConsoleFontEx(output.as_raw_handle(), FALSE, &raw const wanted) };
    assert_ne!(
        ok,
        0,
        "SetCurrentConsoleFontEx failed: {}",
        std::io::Error::last_os_error()
    );
}

/// Types `text` and Enter into the attached console, as key events.
fn type_line(text: &str) {
    let input = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")
        .expect("open the console input");
    let mut records = Vec::new();
    for ch in text.chars().chain(std::iter::once('\r')) {
        let unit = u16::try_from(u32::from(ch)).expect("a BMP character");
        // SAFETY: plain calls with scalar arguments.
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

/// Starts an interactive cash in a new, minimized console window and attaches this
/// process to that console. Returns the process handle.
fn start_cash_in_its_own_console() -> PROCESS_INFORMATION {
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
    let mut command_line: Vec<u16> = format!("\"{CASH}\" --no-config --norc")
        .encode_utf16()
        .collect();
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
    std::thread::sleep(Duration::from_millis(2500));
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

fn end(process: &PROCESS_INFORMATION) {
    type_line("exit");
    // SAFETY: a process handle this test owns.
    if unsafe { WaitForSingleObject(process.hProcess, 10_000) } != WAIT_OBJECT_0 {
        // SAFETY: as above.
        unsafe { TerminateProcess(process.hProcess, 1) };
    }
    // SAFETY: as above, closed once.
    unsafe { CloseHandle(process.hProcess) };
    // SAFETY: a plain call.
    unsafe { FreeConsole() };
}

#[test]
fn a_raster_font_a_program_left_is_put_back_at_the_prompt() {
    let process = start_cash_in_its_own_console();
    let starting = font();
    if starting.FontFamily & TRUETYPE == 0 {
        // A console that starts with a raster font keeps it: nothing to check here.
        eprintln!(
            "the console's font is a raster one ({}): skipped",
            describe(&starting)
        );
        end(&process);
        return;
    }

    set_raster_font();
    let switched = font();
    assert_eq!(
        switched.FontFamily & TRUETYPE,
        0,
        "the raster font did not take: {}",
        describe(&switched)
    );

    // Only a program run through cash has the font put back; the next prompt does it.
    type_line("cmd /c exit");
    std::thread::sleep(Duration::from_millis(3000));
    let after = font();
    end(&process);

    assert_ne!(
        after.FontFamily & TRUETYPE,
        0,
        "the font was not put back: {} (started with {})",
        describe(&after),
        describe(&starting)
    );
    assert_eq!(face(&after), face(&starting));
}
