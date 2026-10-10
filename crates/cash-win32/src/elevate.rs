//! `sudo` in this terminal, elevated by cash itself (research/sudo-in-terminal-design.md).
//!
//! Two processes. The caller, `cash --invoke-bundled --sudo-elevate COMMAND...`, is a
//! program like any other the shell starts for `sudo`: its standard handles are the
//! command's, redirections and pipes included, and it is in the shell's job. It asks UAC
//! for an elevated cash, `cash --invoke-bundled --sudo-attach ... COMMAND...`, with the
//! window Windows gives it hidden, waits for it, and ends with its status.
//!
//! The elevated cash takes the caller's console (`AttachConsole`), so the command has the
//! terminal as any program the shell starts has it, and the caller's handles that are not
//! the console (`DuplicateHandle`: an administrator's process may open the user's own),
//! so nothing is relayed. It ends when the caller does, which the shell's job, unable to
//! hold an elevated process, cannot see to.

use std::ffi::OsString;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_CANCELLED, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE, TRUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleMode, GetConsoleWindow, GetStdHandle, STD_ERROR_HANDLE,
    STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleCtrlHandler, SetStdHandle,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcess, PROCESS_DUP_HANDLE,
    PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows_sys::core::BOOL;

/// The elevated cash's status when it could not take the caller's terminal: this bit,
/// the step in the next byte and the Windows error in the low 16 bits. Application
/// statuses with bit 29 set are Windows' "customer" codes, which no program ends with by
/// chance.
const SETUP_FAILED: u32 = 0x2000_0000;

/// The steps of [`take_callers_terminal`], for the caller's message.
const STEPS: [&str; 5] = [
    "",
    "open the shell's process",
    "attach to its console",
    "take its handle",
    "open the console",
];

/// The standard handles, in the order they are passed.
const STD: [STD_HANDLE; 3] = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];

/// Swallows every console event: the elevated command answers its own.
const unsafe extern "system" fn ignore(_event: u32) -> BOOL {
    TRUE
}

/// How a standard handle of the caller's is passed: `c` for the console, `-` for none,
/// else its value in the caller.
fn handle_word(handle: HANDLE) -> String {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return "-".to_owned();
    }
    let mut mode = 0;
    // SAFETY: a plain query on a handle this process holds.
    if unsafe { GetConsoleMode(handle, &raw mut mode) } != 0 {
        "c".to_owned()
    } else {
        (handle as usize).to_string()
    }
}

/// The caller's side: runs `command` elevated, taking this terminal, and gives its status.
///
/// `command` is a bundled command line, `--sudo-owner SID PROGRAM ...`, which an elevated
/// cash runs after taking this process's terminal and handles; `own` is cash's own
/// executable.
#[must_use]
pub fn run_elevated_here(own: &Path, command: &[OsString]) -> i32 {
    // SAFETY: `ignore` is a valid handler for the life of the process and touches nothing.
    unsafe { SetConsoleCtrlHandler(Some(ignore), TRUE) };

    // SAFETY: a plain query of this process's own state.
    let has_console = !unsafe { GetConsoleWindow() }.is_null();
    // SAFETY: as above; a standard handle is this process's, or null.
    let handles = STD.map(|which| handle_word(unsafe { GetStdHandle(which) }));
    let dir = std::env::current_dir().unwrap_or_default();
    // An elevated process has none of the user's drive letters.
    let dir = if crate::path::is_on_network(&dir) {
        crate::path::universal_name(&dir).map_or(dir, PathBuf::from)
    } else {
        dir
    };

    let mut words = vec![
        "--invoke-bundled".to_owned(),
        "--sudo-attach".to_owned(),
        std::process::id().to_string(),
        if has_console { "1" } else { "0" }.to_owned(),
    ];
    words.extend(handles);
    words.push(dir.to_string_lossy().into_owned());
    words.extend(
        command
            .iter()
            .map(|word| word.to_string_lossy().into_owned()),
    );
    let parameters = words
        .iter()
        .map(|word| crate::cmd::quote_argument(word))
        .collect::<Vec<_>>()
        .join(" ");

    let process =
        match crate::shellopen::start_elevated_hidden(&own.to_string_lossy(), &parameters, &dir) {
            Ok(process) => process,
            Err(err) if err.raw_os_error() == Some(ERROR_CANCELLED.cast_signed()) => {
                eprintln!("sudo: not run: the request to run it as an administrator was declined");
                return 1;
            }
            Err(err) => {
                eprintln!("sudo: {err}");
                return 1;
            }
        };
    let code = wait_for(&process);
    if code & 0xF000_0000 == SETUP_FAILED {
        let step = STEPS
            .get(((code >> 16) & 0xFF) as usize)
            .copied()
            .unwrap_or("");
        let error = io::Error::from_raw_os_error((code & 0xFFFF).cast_signed());
        eprintln!("sudo: the elevated cash could not {step}: {error}");
        return 1;
    }
    code.cast_signed()
}

/// Waits for `process` and gives its exit code.
fn wait_for(process: &OwnedHandle) -> u32 {
    // SAFETY: a process handle this process owns, with SYNCHRONIZE.
    unsafe { WaitForSingleObject(process.as_raw_handle(), INFINITE) };
    let mut code = 1;
    // SAFETY: the same handle; `code` is a valid out-param.
    unsafe { GetExitCodeProcess(process.as_raw_handle(), &raw mut code) };
    code
}

/// What [`take_callers_terminal`] reads from the elevated cash's command line:
/// `PID ATTACH IN OUT ERR DIR`, then the command.
pub struct Caller<'a> {
    /// The caller's process id.
    pub pid: &'a OsString,
    /// `1` when the caller has a console to attach to.
    pub attach: &'a OsString,
    /// Its standard handles: `c` for the console, `-` for none, else a value in the caller.
    pub handles: [&'a OsString; 3],
    /// Its folder.
    pub dir: &'a OsString,
}

/// The elevated side: takes the caller's console and handles as this process's own.
///
/// It also changes to the caller's folder, and ends this process when the caller ends.
/// On failure, the status to end with, which the caller turns into a message.
///
/// # Errors
///
/// The step that failed, as a status for [`run_elevated_here`] to read.
pub fn take_callers_terminal(caller: &Caller<'_>) -> Result<(), i32> {
    let fail = |step: u32| {
        let error = io::Error::last_os_error().raw_os_error().unwrap_or(0);
        Err((SETUP_FAILED | (step << 16) | (error.cast_unsigned() & 0xFFFF)).cast_signed())
    };
    let pid: u32 = caller.pid.to_string_lossy().parse().unwrap_or(0);
    // SAFETY: a plain call; the handle is checked and owned below.
    let raw = unsafe { OpenProcess(PROCESS_DUP_HANDLE | PROCESS_SYNCHRONIZE, 0, pid) };
    if raw.is_null() {
        return fail(1);
    }
    // SAFETY: just opened, and nothing else owns it.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };

    let words = caller
        .handles
        .map(|word| word.to_string_lossy().into_owned());
    let wants_console = words.iter().any(|word| word == "c");
    if caller.attach == "1" {
        // SAFETY: a plain call: leave the hidden console Windows made.
        unsafe { FreeConsole() };
        // SAFETY: a plain call: take the caller's.
        if unsafe { AttachConsole(pid) } == 0 && wants_console {
            return fail(2);
        }
    }

    let inheritable = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(u32::MAX),
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: TRUE,
    };
    for (n, (word, which)) in words.iter().zip(STD).enumerate() {
        let handle: HANDLE = match word.as_str() {
            "-" => std::ptr::null_mut(),
            "c" => {
                let name = crate::wide::to_wide_nul(if n == 0 { "CONIN$" } else { "CONOUT$" });
                // SAFETY: the name is NUL-terminated; the attributes outlive the call.
                let handle = unsafe {
                    CreateFileW(
                        name.as_ptr(),
                        GENERIC_READ | GENERIC_WRITE,
                        FILE_SHARE_READ | FILE_SHARE_WRITE,
                        &raw const inheritable,
                        OPEN_EXISTING,
                        0,
                        std::ptr::null_mut(),
                    )
                };
                if handle == INVALID_HANDLE_VALUE {
                    return fail(4);
                }
                handle
            }
            value => {
                let source = value.parse::<usize>().unwrap_or(0) as HANDLE;
                let mut copy: HANDLE = std::ptr::null_mut();
                // SAFETY: a pseudo handle, which needs no closing.
                let this = unsafe { GetCurrentProcess() };
                // SAFETY: `process` was opened with PROCESS_DUP_HANDLE; `copy` is a valid
                // out-param, and the copy is this process's.
                let ok = unsafe {
                    DuplicateHandle(
                        process.as_raw_handle(),
                        source,
                        this,
                        &raw mut copy,
                        0,
                        TRUE,
                        DUPLICATE_SAME_ACCESS,
                    )
                };
                if ok == 0 {
                    return fail(3);
                }
                copy
            }
        };
        // SAFETY: a handle this process holds, or null for none; it stays open for the
        // life of the process, as a standard handle does.
        unsafe { SetStdHandle(which, handle) };
    }

    let _ = std::env::set_current_dir(Path::new(caller.dir));

    // The caller ending (the shell killed, the window closed) ends this process, and with
    // it the command's job.
    let watched = process.as_raw_handle() as usize;
    let _ = std::thread::Builder::new()
        .name("cash-sudo-caller".into())
        .spawn(move || {
            // SAFETY: `process` is kept below for the life of the process.
            if unsafe { WaitForSingleObject(watched as HANDLE, INFINITE) } == WAIT_OBJECT_0 {
                std::process::exit(1);
            }
        });
    std::mem::forget(process);
    Ok(())
}
