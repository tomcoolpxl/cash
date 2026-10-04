//! Process creation outside the shell's own spawn — **D6**, D45.
//!
//! The shell starts a program through `cash_core::sys::tokio_process::spawn`: created
//! suspended, contained in a job of its own (`crate::jobreg::contain`), then resumed, so
//! it cannot run an instruction, let alone start a child, outside its job (spec §6, D6).
//! What is here is the rest: whether cash itself runs in a job, for `cash doctor`, and
//! `detach`'s start of a program that leaves cash's job on purpose (D45).
//!
//! It held a `CreateProcessW` spawn of its own, written when tokio was thought unable to
//! start a program suspended; nothing ran it but its tests (W32-11).

use std::io;
use std::os::windows::io::{FromRawHandle as _, OwnedHandle};
use std::path::Path;

use windows_sys::Win32::Foundation::FALSE;
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
};

use crate::wide::to_wide_nul;

/// Build the double-null-terminated UTF-16 environment block `CreateProcessW` expects.
///
/// Windows conventionally sorts these case-insensitively; some programs rely on it.
fn build_environment_block(vars: &[(String, String)]) -> Vec<u16> {
    let mut sorted: Vec<&(String, String)> = vars.iter().collect();
    sorted.sort_by_key(|entry| entry.0.to_ascii_uppercase());

    let mut block = Vec::new();
    for (name, value) in sorted {
        block.extend(format!("{name}={value}").encode_utf16());
        block.push(0);
    }
    // An empty environment still needs the terminating pair.
    block.push(0);
    block
}

/// Whether the current process is inside a job object.
///
/// Relevant to D6: cash running inside Windows Terminal's or VS Code's own job is fine,
/// because nested jobs have worked since Windows 8 — but it is worth being able to say
/// so in `cash doctor` (D35).
#[must_use]
pub fn in_any_job() -> bool {
    use windows_sys::Win32::System::JobObjects::IsProcessInJob;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut result: i32 = FALSE;
    // SAFETY: returns a pseudo-handle for the current process; it reads no memory.
    let this = unsafe { GetCurrentProcess() };
    // SAFETY: a null job handle asks "is this process in *any* job", and `result` is a
    // live, correctly-typed BOOL out-parameter.
    let ok = unsafe { IsProcessInJob(this, std::ptr::null_mut(), &raw mut result) };
    ok != 0 && result != 0
}

/// Start a process that deliberately leaves cash's job object (D45's `detach`), in the
/// folder `cwd` and with the environment `env` (the shell's, which are not the process's:
/// D5, D10).
///
/// `CREATE_BREAKAWAY_FROM_JOB` only succeeds if the enclosing job permits it, which is
/// why the session job is created with `JOB_OBJECT_LIMIT_BREAKAWAY_OK`
/// ([`crate::job::JobConfig::session`]). D45 records the cost of that: once breakaway is
/// permitted, *any* child can request it, so this escape hatch weakens D6's guarantee
/// slightly for everything.
///
/// `DETACHED_PROCESS` additionally gives the child no console, so it does not die with
/// the terminal and does not scribble on cash's output. And it inherits no handle at all,
/// which the standard library cannot promise: a child it starts inherits every
/// inheritable handle of cash's, among them a pipe cash's own output goes to, which then
/// stays open for as long as the detached program runs.
pub fn spawn_detached(command_line: &str, cwd: &Path, env: &[(String, String)]) -> io::Result<u32> {
    use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, DETACHED_PROCESS};

    let mut command = to_wide_nul(command_line);
    let cwd = to_wide_nul(crate::path::process_directory(cwd)?);
    let environment = build_environment_block(env);

    // SAFETY: `STARTUPINFOW` is plain old data — integers, pointers and a handle triple —
    // for which all-zero is the documented "use the defaults" value. `cb` is set below,
    // which is the only field the API requires.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    // The structure is a fixed ~104 bytes and `cb` is a `u32` by ABI, so the saturating
    // fallback is unreachable; it keeps this a function that returns rather than panics.
    startup.cb = u32::try_from(size_of::<STARTUPINFOW>()).unwrap_or(u32::MAX);

    // SAFETY: `PROCESS_INFORMATION` is four integers-or-handles that CreateProcessW fills
    // in; all-zero is a valid starting state.
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // SAFETY: command is a live, writable UTF-16 buffer; the folder and the environment
    // block are NUL-terminated (the block doubly) and outlive the call; every other
    // pointer is null or points at a correctly-sized local that outlives the call.
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            FALSE, // do not inherit handles: a detached process should hold none of ours
            CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast(),
            cwd.as_ptr(),
            &raw const startup,
            &raw mut info,
        )
    };

    if created == 0 {
        return Err(io::Error::last_os_error());
    }

    // cash neither waits for the program nor ends it, so it keeps no handle to it: both
    // are closed here.
    // SAFETY: the process was made, so both are open handles that are ours.
    drop(unsafe { OwnedHandle::from_raw_handle(info.hThread) });
    // SAFETY: as above.
    drop(unsafe { OwnedHandle::from_raw_handle(info.hProcess) });
    Ok(info.dwProcessId)
}
