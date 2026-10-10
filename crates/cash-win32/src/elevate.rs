//! `sudo` in this terminal, elevated or as another account, by cash itself
//! (research/sudo-in-terminal-design.md).
//!
//! Two processes. The caller, `cash --invoke-bundled --sudo-elevate COMMAND...` or
//! `--sudo-as WHO USER COMMAND...`, is a program like any other the shell starts for
//! `sudo`: its standard handles are the command's, redirections and pipes included, and it
//! is in the shell's job. It starts a cash, `cash --invoke-bundled --sudo-attach ...
//! COMMAND...`, elevated through UAC with the window Windows gives it hidden, or as USER
//! with USER's password and no window, waits for it, and ends with its status.
//!
//! That cash takes the caller's console (`AttachConsole`), so the command has the terminal
//! as any program the shell starts has it, and the caller's handles that are not the
//! console (`DuplicateHandle`: an administrator's process may open the user's own, and the
//! caller lets another account open it), so nothing is relayed. It ends when the caller
//! does, which the shell's job, unable to hold it, cannot see to.

use std::ffi::OsString;
use std::io::{self, Write as _};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_CANCELLED, GENERIC_READ,
    GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree, TRUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetSecurityInfo, SE_KERNEL_OBJECT, SetEntriesInAclW,
    SetSecurityInfo, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, CTRL_BREAK_EVENT, CTRL_C_EVENT, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
    ENABLE_PROCESSED_INPUT, FreeConsole, GetConsoleMode, GetConsoleWindow, GetStdHandle,
    ReadConsoleW, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    SetConsoleCtrlHandler, SetConsoleMode, SetStdHandle,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessWithLogonW, GetCurrentProcess,
    GetExitCodeProcess, INFINITE, LOGON_WITH_PROFILE, OpenProcess, PROCESS_DUP_HANDLE,
    PROCESS_INFORMATION, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, STARTUPINFOW, WaitForSingleObject,
};
use windows_sys::core::BOOL;

/// The started cash's status when it could not take the caller's terminal: this bit, the
/// step in the next byte and the Windows error in the low 16 bits. Application statuses
/// with bit 29 set are Windows' "customer" codes, which no program ends with by chance.
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

/// The longest command line `CreateProcessWithLogonW` takes, in UTF-16 units.
const LOGON_COMMAND_LINE: usize = 1024;

/// Swallows every console event: the command answers its own.
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

/// The folder to start in: this process's, a mapped drive's by its network path, as the
/// started cash has none of the user's drive letters.
fn start_folder() -> PathBuf {
    let dir = std::env::current_dir().unwrap_or_default();
    if crate::path::is_on_network(&dir) {
        crate::path::universal_name(&dir).map_or(dir, PathBuf::from)
    } else {
        dir
    }
}

/// The started cash's arguments: `--invoke-bundled --sudo-attach PID ATTACH IN OUT ERR
/// DIR`, then `command`, quoted as the C runtime parses them.
fn attach_parameters(dir: &Path, command: &[OsString]) -> String {
    // SAFETY: a plain query of this process's own state.
    let has_console = !unsafe { GetConsoleWindow() }.is_null();
    // SAFETY: as above; a standard handle is this process's, or null.
    let handles = STD.map(|which| handle_word(unsafe { GetStdHandle(which) }));
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
    words
        .iter()
        .map(|word| crate::cmd::quote_argument(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The status to end with, once the started cash has: its own, or 1 with a message when it
/// could not take this terminal.
fn report(who: &str, code: u32) -> i32 {
    if code & 0xF000_0000 == SETUP_FAILED {
        let step = STEPS
            .get(((code >> 16) & 0xFF) as usize)
            .copied()
            .unwrap_or("");
        let error = io::Error::from_raw_os_error((code & 0xFFFF).cast_signed());
        eprintln!("{who}: the started cash could not {step}: {error}");
        return 1;
    }
    code.cast_signed()
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

    let dir = start_folder();
    let parameters = attach_parameters(&dir, command);
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
    report("sudo", wait_for(&process))
}

/// The caller's side as another account: asks USER's password at the console, runs
/// `command` as USER, at that account's usual level, taking this terminal, and gives its
/// status.
///
/// `who` is the builtin, `sudo` or `su`, for messages; `command` and `own` are as for
/// [`run_elevated_here`]. The password is wiped once Windows has it.
#[must_use]
pub fn run_as_user_here(own: &Path, who: &str, user: &str, command: &[OsString]) -> i32 {
    let Some(sid) = crate::account::lookup_user(user) else {
        eprintln!("{who}: unknown user {user}");
        return 1;
    };
    let password = match read_password(&format!("Password for {user}: ")) {
        Ok(Some(password)) => password,
        Ok(None) => return 130,
        Err(_) => {
            eprintln!("{who}: a terminal is required to read the password");
            return 1;
        }
    };
    // SAFETY: `ignore` is a valid handler for the life of the process and touches nothing.
    unsafe { SetConsoleCtrlHandler(Some(ignore), TRUE) };

    // The started cash runs as USER, who may not open this process to take its console and
    // handles unless it is let; the grant ends with the process.
    if let Err(err) = let_open_this_process(&sid) {
        eprintln!("{who}: {err}");
        return 1;
    }

    let dir = start_folder();
    let own_text = own.to_string_lossy();
    let command_line = format!(
        "{} {}",
        crate::cmd::quote_argument(&own_text),
        attach_parameters(&dir, command)
    );
    if command_line.encode_utf16().count() > LOGON_COMMAND_LINE {
        eprintln!(
            "{who}: the command line is too long to run as another account (over \
             {LOGON_COMMAND_LINE} characters)"
        );
        return 1;
    }
    let process = match start_as(&own_text, &command_line, &dir, user, &password) {
        Ok(process) => process,
        Err(err) => {
            eprintln!("{who}: {err}");
            return 1;
        }
    };
    drop(password);
    report(who, wait_for(&process))
}

/// `user` as `CreateProcessWithLogonW` takes it: a name and a domain, the local machine
/// (`.`) when none is given; a UPN (`name@domain`) is the name, with no domain.
fn logon_name(user: &str) -> (&str, Option<&str>) {
    if let Some((domain, name)) = user.split_once('\\') {
        (name, Some(domain))
    } else if user.contains('@') {
        (user, None)
    } else {
        (user, Some("."))
    }
}

/// Starts `command_line` as `user`, with `password`, in `dir`, with no window and the
/// account's own environment and profile.
fn start_as(
    program: &str,
    command_line: &str,
    dir: &Path,
    user: &str,
    password: &Secret,
) -> io::Result<OwnedHandle> {
    let (name, domain) = logon_name(user);
    let name = crate::wide::to_wide_nul(name);
    let domain = domain.map(crate::wide::to_wide_nul);
    let program = crate::wide::to_wide_nul(program);
    let mut line = crate::wide::to_wide_nul(command_line);
    let dir = crate::wide::to_wide_nul(dir);
    let startup = STARTUPINFOW {
        cb: u32::try_from(size_of::<STARTUPINFOW>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: every string is NUL-terminated and outlives the call; the command line is a
    // buffer of this process's, which the call may write to; the structures are sized.
    let ok = unsafe {
        CreateProcessWithLogonW(
            name.as_ptr(),
            domain
                .as_ref()
                .map_or(std::ptr::null(), |domain| domain.as_ptr()),
            password.0.as_ptr(),
            LOGON_WITH_PROFILE,
            program.as_ptr(),
            line.as_mut_ptr(),
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            std::ptr::null(),
            dir.as_ptr(),
            &raw const startup,
            &raw mut info,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the thread handle is this process's and not used.
    unsafe { CloseHandle(info.hThread) };
    // SAFETY: the process handle is this process's to close, and nothing else owns it.
    Ok(unsafe { OwnedHandle::from_raw_handle(info.hProcess) })
}

/// Lets `sid` open this process to take its console and handles: an entry added to the
/// process's access list, which lasts as long as the process.
fn let_open_this_process(sid: &crate::account::Sid) -> io::Result<()> {
    // SAFETY: a pseudo handle, which needs no closing.
    let this = unsafe { GetCurrentProcess() };
    let mut old: *mut ACL = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: the out-params are valid; the descriptor is freed below.
    let status = unsafe {
        GetSecurityInfo(
            this,
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut old,
            std::ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status.cast_signed()));
    }
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: PROCESS_DUP_HANDLE
            | PROCESS_QUERY_INFORMATION
            | PROCESS_QUERY_LIMITED_INFORMATION
            | PROCESS_SYNCHRONIZE,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: sid.as_psid().cast(),
            ..Default::default()
        },
    };
    let mut new: *mut ACL = std::ptr::null_mut();
    // SAFETY: one entry, whose SID outlives the call; `old` is the process's own list.
    let status = unsafe { SetEntriesInAclW(1, &raw const entry, old, &raw mut new) };
    let result = if status == 0 {
        // SAFETY: `new` was made by the call above.
        let status = unsafe {
            SetSecurityInfo(
                this,
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                new,
                std::ptr::null(),
            )
        };
        // SAFETY: allocated by `SetEntriesInAclW`, not used after this.
        unsafe { LocalFree(new.cast()) };
        if status == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status.cast_signed()))
        }
    } else {
        Err(io::Error::from_raw_os_error(status.cast_signed()))
    };
    // SAFETY: allocated by `GetSecurityInfo`, and `old` points into it; neither is used
    // after this.
    unsafe { LocalFree(descriptor) };
    result
}

/// A password, NUL-terminated UTF-16, wiped when dropped.
struct Secret(Vec<u16>);

impl Drop for Secret {
    fn drop(&mut self) {
        for unit in &mut self.0 {
            // SAFETY: a valid, aligned `u16` of this vector; volatile, so the wipe is kept.
            unsafe { std::ptr::write_volatile(unit, 0) };
        }
    }
}

/// A Ctrl-C or Ctrl-Break while the password is read.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Notes a Ctrl-C or Ctrl-Break, which ends the read.
unsafe extern "system" fn on_interrupt(event: u32) -> BOOL {
    if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
        INTERRUPTED.store(true, Ordering::SeqCst);
        TRUE
    } else {
        0
    }
}

/// Reads a line at the console without showing it, after `prompt`; `None` when Ctrl-C
/// ended it, as Unix's `su` ends.
///
/// The console's modes are put back whatever happens, and a new line follows, as the Enter
/// typed is not shown.
fn read_password(prompt: &str) -> io::Result<Option<Secret>> {
    let input = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")?;
    let mut output = std::fs::OpenOptions::new().write(true).open("CONOUT$")?;
    let mut mode = 0;
    // SAFETY: a plain query on a handle this process holds.
    if unsafe { GetConsoleMode(input.as_raw_handle(), &raw mut mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    write!(output, "{prompt}")?;
    let quiet = (mode | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT) & !ENABLE_ECHO_INPUT;
    // SAFETY: `on_interrupt` touches an atomic only; it is removed below.
    unsafe { SetConsoleCtrlHandler(Some(on_interrupt), TRUE) };
    // SAFETY: a plain call on a console handle this process holds.
    unsafe { SetConsoleMode(input.as_raw_handle(), quiet) };

    let mut line = Secret(Vec::with_capacity(256));
    let mut chunk = [0u16; 128];
    let ended = loop {
        let mut read = 0u32;
        // SAFETY: `chunk` holds as many units as asked for; `read` is a valid out-param.
        let ok = unsafe {
            ReadConsoleW(
                input.as_raw_handle(),
                chunk.as_mut_ptr().cast(),
                128,
                &raw mut read,
                std::ptr::null(),
            )
        };
        if ok == 0 || read == 0 || INTERRUPTED.load(Ordering::SeqCst) {
            break false;
        }
        let got = chunk.get(..read as usize).unwrap_or_default();
        line.0.extend_from_slice(got);
        if got.contains(&u16::from(b'\n')) || got.contains(&u16::from(b'\r')) {
            break true;
        }
    };
    chunk.fill(0);

    // SAFETY: as above: the modes put back, the handler removed.
    unsafe { SetConsoleMode(input.as_raw_handle(), mode) };
    // SAFETY: as above.
    unsafe { SetConsoleCtrlHandler(Some(on_interrupt), 0) };
    let _ = writeln!(output);
    if !ended {
        return Ok(None);
    }
    while line
        .0
        .last()
        .is_some_and(|unit| *unit == u16::from(b'\n') || *unit == u16::from(b'\r'))
    {
        line.0.pop();
    }
    line.0.push(0);
    Ok(Some(line))
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
/// The step that failed, as a status for the caller ([`run_elevated_here`],
/// [`run_as_user_here`]) to read.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_name_is_split_as_windows_takes_it() {
        assert_eq!(logon_name("alice"), ("alice", Some(".")));
        assert_eq!(logon_name(r"CORP\alice"), ("alice", Some("CORP")));
        assert_eq!(
            logon_name("alice@corp.example"),
            ("alice@corp.example", None)
        );
    }

    #[test]
    fn a_status_from_a_failed_setup_is_a_message_and_1() {
        assert_eq!(report("sudo", 7), 7);
        assert_eq!(report("sudo", SETUP_FAILED | (2 << 16) | 5), 1);
    }
}
