//! Opening a file, folder or URL with its default handler: what a double-click in
//! Explorer does, and what the `start` builtin does without a command processor between.
//!
//! `start` used to run `cmd.exe /d /s /c start "" <target>`. The standard library quotes
//! an argument only when it holds a space, so a URL such as `https://x/?a=1&b=2` reached
//! cmd unquoted, and cmd ran `b=2` as a second command (`REVIEW_REPORT.md` BI-06).
//! `ShellExecuteExW` takes the target as one string that nothing parses.
//!
//! It is also the one way to start a program elevated: UAC's consent runs for the
//! `runas` verb, which `CreateProcessW` has no equivalent of.

use std::io;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::Path;

use windows_sys::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};

use crate::wide::to_wide_nul;

/// Opens `target` with its default handler, the default verb of `ShellExecuteExW`, with
/// `directory` as the working directory of whatever it starts.
///
/// A failure is returned rather than shown in a dialog, as a command-line tool should.
/// The call runs on a thread of its own: the handlers it may load are COM objects that
/// want a single-threaded apartment, and a thread of the shell's runtime must not be
/// turned into one.
pub fn open(target: &str, directory: &Path) -> io::Result<()> {
    execute(None, target, None, directory, false).map(drop)
}

/// Starts `program` elevated, through UAC's consent, with `parameters` as the rest of its
/// command line and `directory` as its working directory.
///
/// The elevated process starts from the user's own environment: UAC takes no
/// environment block, so the shell's exported variables cannot follow it. Refusing the
/// consent is an error (`ERROR_CANCELLED`).
///
/// # Arguments
///
/// * `program` - The program, best given by full path.
/// * `parameters` - Its arguments, already quoted as the program parses them.
/// * `directory` - The folder it starts in.
pub fn run_elevated(program: &str, parameters: &str, directory: &Path) -> io::Result<()> {
    execute(Some("runas"), program, Some(parameters), directory, false).map(drop)
}

/// [`run_elevated`], with the window Windows gives a console program hidden, and the
/// elevated process's handle given back to wait on and read its status from.
///
/// # Errors
///
/// The consent was refused (`ERROR_CANCELLED`), or the program could not be started.
pub fn start_elevated_hidden(
    program: &str,
    parameters: &str,
    directory: &Path,
) -> io::Result<OwnedHandle> {
    execute(Some("runas"), program, Some(parameters), directory, true)?
        .ok_or_else(|| io::Error::other("Windows gave no handle to the elevated process"))
}

/// `ShellExecuteExW` with `verb` (its default when `None`), on a thread of its own.
/// `hidden` hides the window of what it starts and asks for its process handle.
fn execute(
    verb: Option<&str>,
    target: &str,
    parameters: Option<&str>,
    directory: &Path,
    hidden: bool,
) -> io::Result<Option<OwnedHandle>> {
    let verb = verb.map(to_wide_nul);
    let target = to_wide_nul(target);
    let parameters = parameters.map(to_wide_nul);
    let directory = to_wide_nul(crate::path::process_directory(directory)?);
    std::thread::Builder::new()
        .name("cash-shell-open".into())
        .spawn(move || {
            open_on_this_thread(
                verb.as_deref(),
                &target,
                parameters.as_deref(),
                &directory,
                hidden,
            )
        })?
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("opening it failed unexpectedly")))
}

/// [`execute`], on a thread that has no COM apartment yet.
fn open_on_this_thread(
    verb: Option<&[u16]>,
    target: &[u16],
    parameters: Option<&[u16]>,
    directory: &[u16],
    hidden: bool,
) -> io::Result<Option<OwnedHandle>> {
    let apartment = (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).cast_unsigned();
    // SAFETY: the reserved pointer is null, as required, and this new thread has no
    // apartment of its own yet.
    let initialized = unsafe { CoInitializeEx(std::ptr::null(), apartment) } >= 0;

    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or(u32::MAX),
        fMask: SEE_MASK_NOASYNC
            | SEE_MASK_FLAG_NO_UI
            | if hidden { SEE_MASK_NOCLOSEPROCESS } else { 0 },
        lpVerb: verb.map_or(std::ptr::null(), <[u16]>::as_ptr),
        lpFile: target.as_ptr(),
        lpParameters: parameters.map_or(std::ptr::null(), <[u16]>::as_ptr),
        lpDirectory: directory.as_ptr(),
        nShow: if hidden { SW_HIDE } else { SW_SHOWNORMAL },
        ..Default::default()
    };
    // SAFETY: `info` carries its own size, and its strings are NUL-terminated or null and
    // outlive the call.
    let opened = unsafe { ShellExecuteExW(&raw mut info) } != 0;
    // Read before anything else can change the thread's last error.
    let result = if !opened {
        Err(io::Error::last_os_error())
    } else if info.hProcess.is_null() {
        Ok(None)
    } else {
        // SAFETY: asked for with `SEE_MASK_NOCLOSEPROCESS`, the handle is this process's
        // to close, and nothing else owns it.
        Ok(Some(unsafe { OwnedHandle::from_raw_handle(info.hProcess) }))
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
