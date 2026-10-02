//! Opening a file, folder or URL with its default handler: what a double-click in
//! Explorer does, and what the `start` builtin does without a command processor between.
//!
//! `start` used to run `cmd.exe /d /s /c start "" <target>`. The standard library quotes
//! an argument only when it holds a space, so a URL such as `https://x/?a=1&b=2` reached
//! cmd unquoted, and cmd ran `b=2` as a second command (`REVIEW_REPORT.md` BI-06).
//! `ShellExecuteExW` takes the target as one string that nothing parses.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Opens `target` with its default handler, the default verb of `ShellExecuteExW`, with
/// `directory` as the working directory of whatever it starts.
///
/// A failure is returned rather than shown in a dialog, as a command-line tool should.
/// The call runs on a thread of its own: the handlers it may load are COM objects that
/// want a single-threaded apartment, and a thread of the shell's runtime must not be
/// turned into one.
pub fn open(target: &str, directory: &Path) -> io::Result<()> {
    let target: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let directory: Vec<u16> = directory.as_os_str().encode_wide().chain(Some(0)).collect();
    std::thread::Builder::new()
        .name("cash-shell-open".into())
        .spawn(move || open_on_this_thread(&target, &directory))?
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("opening it failed unexpectedly")))
}

/// [`open`], on a thread that has no COM apartment yet.
fn open_on_this_thread(target: &[u16], directory: &[u16]) -> io::Result<()> {
    let apartment = (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).cast_unsigned();
    // SAFETY: the reserved pointer is null, as required, and this new thread has no
    // apartment of its own yet.
    let initialized = unsafe { CoInitializeEx(std::ptr::null(), apartment) } >= 0;

    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or(u32::MAX),
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpFile: target.as_ptr(),
        lpDirectory: directory.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: `info` carries its own size, and its two strings are NUL-terminated and
    // outlive the call. No process handle is asked for, so none needs closing.
    let opened = unsafe { ShellExecuteExW(&raw mut info) } != 0;
    // Read before anything else can change the thread's last error.
    let result = if opened {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    };

    if initialized {
        // SAFETY: pairs with the successful `CoInitializeEx` above, on the same thread.
        unsafe { CoUninitialize() };
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_that_does_not_exist_is_an_error_not_a_command() {
        // With cmd in between, the `&` would have run `echo` and opened nothing.
        let error = open(
            "cash-no-such-file-7f3a.txt&echo injected",
            &std::env::temp_dir(),
        );
        assert!(error.is_err());
    }
}
