//! The font of a Windows Console window after a program (D68): Windows PowerShell 5.1
//! switches a console whose code page is UTF-8, as cash's is, to the raster `Terminal`
//! font, and the whole window is redrawn in it (seen 2026-10-24 in Windows Sandbox). cash
//! puts the font it started with back at the next prompt.
//!
//! A console with no window draws with no font and ignores a font set on it, so cash is
//! started in a new console, minimized, which this test then attaches to
//! ([`crate::console_window`]); the raster font is set from here, as PowerShell does, and
//! a program is run through cash by typing into the console, since the font is only put
//! back after a program ran.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::os::windows::io::AsRawHandle as _;
use std::time::Duration;

use windows_sys::Win32::Foundation::FALSE;
use windows_sys::Win32::System::Console::{
    CONSOLE_FONT_INFOEX, GetCurrentConsoleFontEx, SetCurrentConsoleFontEx,
};

use crate::console_window::{conout, end, start_cash_in_its_own_console, type_line};

/// `TMPF_TRUETYPE` in a font's family.
const TRUETYPE: u32 = 0x04;

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

#[test]
fn a_raster_font_a_program_left_is_put_back_at_the_prompt() {
    let process = start_cash_in_its_own_console("--no-config --norc", Duration::from_millis(2500));
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
