//! Process creation — **D6**, and the reason cash cannot use [`std::process::Command`].
//!
//! # The race this closes
//!
//! Assigning a child to a job object *after* `spawn()` leaves a window in which the
//! child can fork a grandchild that never joins the job. The clean fix is:
//!
//! ```text
//! CreateProcessW(CREATE_SUSPENDED)  ->  AssignProcessToJobObject  ->  ResumeThread
//! ```
//!
//! The child cannot execute a single instruction before it is contained. But
//! `std::process::Child` does not expose the initial thread handle, so `ResumeThread`
//! is impossible through it — which is exactly why §6 concludes cash needs raw
//! `CreateProcessW`, and why upstream's proposed `ExternalCommandSpawner` seam, typed as
//! `std::process::Command`, would not be sufficient on its own.
//!
//! The session-level guarantee holds regardless, because cash itself is inside the
//! session job and children inherit it. Only *per-job* nesting needs this.

use std::io;
use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, TRUE};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_PROCESS_GROUP, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
    GetExitCodeProcess, INFINITE, PROCESS_INFORMATION, ResumeThread, STARTUPINFOW,
    WaitForSingleObject,
};

use crate::job::JobObject;

/// A spawned process, contained in its job before it ever ran.
#[derive(Debug)]
pub struct Child {
    process: HANDLE,
    thread: HANDLE,
    pid: u32,
    group_id: Option<u32>,
}

// SAFETY: the fields are kernel handles and plain integers. A handle is an index into a
// process-wide table with no thread affinity, the Win32 calls made through these
// (`WaitForSingleObject`, `GetExitCodeProcess`, `CloseHandle`) are all thread-safe, and
// `Drop` closes each handle exactly once because `Child` is not `Clone`.
unsafe impl Send for Child {}
// SAFETY: as above — shared references reach only thread-safe Win32 calls.
unsafe impl Sync for Child {}

impl Child {
    /// The process id.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.pid
    }

    /// The process group id, if the child was created as a new group.
    ///
    /// Required for D13: `GenerateConsoleCtrlEvent` needs a group id, and a group only
    /// exists if the child was created with `CREATE_NEW_PROCESS_GROUP`.
    #[must_use]
    pub const fn group_id(&self) -> Option<u32> {
        self.group_id
    }

    /// The raw process handle.
    #[must_use]
    pub const fn as_raw_handle(&self) -> HANDLE {
        self.process
    }

    /// Block until the process exits, returning its raw Windows exit code.
    ///
    /// The raw `DWORD`, not `$?`: pass it through [`crate::exit::from_windows`] to get
    /// the value bash would report (D15), which maps NTSTATUS crashes to `128 + n` so a
    /// crash can never be mistaken for success.
    pub fn wait(&self) -> io::Result<u32> {
        // SAFETY: process handle is valid for the lifetime of self.
        unsafe { WaitForSingleObject(self.process, INFINITE) };
        self.exit_code()?
            .ok_or_else(|| io::Error::other("process exited without an exit code"))
    }

    /// Check whether the process has exited, without blocking.
    pub fn try_wait(&self) -> io::Result<Option<u32>> {
        self.exit_code()
    }

    /// Block until exit and return the status `$?` should report (D15).
    pub fn wait_status(&self) -> io::Result<u8> {
        Ok(crate::exit::from_windows(self.wait()?))
    }

    /// The exit code, or `None` if the process is still running.
    fn exit_code(&self) -> io::Result<Option<u32>> {
        /// `GetExitCodeProcess` reports this while a process is still running.
        const STILL_ACTIVE: u32 = 259;

        let mut code: u32 = 0;
        // SAFETY: handle is valid and `code` is a valid out-param.
        let ok = unsafe { GetExitCodeProcess(self.process, &raw mut code) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((code != STILL_ACTIVE).then_some(code))
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        // SAFETY: the thread handle came from CreateProcessW and is closed exactly once,
        // since `Child` is not `Clone` and `Drop` runs at most once.
        unsafe { CloseHandle(self.thread) };
        // SAFETY: likewise for the process handle.
        unsafe { CloseHandle(self.process) };
    }
}

/// How to create a process.
#[derive(Debug, Default)]
pub struct SpawnOptions<'a> {
    /// The job object to contain the child in, before it runs (D6).
    pub job: Option<&'a JobObject>,

    /// Create the child as a new process group.
    ///
    /// Required for targeted interrupts (D13), because `GenerateConsoleCtrlEvent` needs
    /// a group id. **Carries a documented Windows trap**: a child created this way has
    /// Ctrl-C handling *disabled by default*, which is a well-known source of the
    /// "Ctrl-C does nothing" bug. cash sends `CTRL_BREAK_EVENT`, which is unaffected.
    pub new_process_group: bool,

    /// Working directory for the child.
    pub cwd: Option<&'a Path>,

    /// The child's environment. `None` inherits cash's.
    ///
    /// Build this with [`crate::env::Environment::to_child_block`] so that `PATH` is
    /// converted to the semicolon-separated Windows form (D5).
    pub env: Option<&'a [(String, String)]>,
}

/// Encode a string as a null-terminated UTF-16 buffer.
fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

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

/// Create a process, contained in its job before it executes anything (D6).
///
/// `command_line` should already be encoded with [`crate::cmd::build_command_line`] or
/// [`crate::cmd::build_cmd_command_line`].
pub fn spawn(command_line: &str, options: &SpawnOptions<'_>) -> io::Result<Child> {
    // CreateProcessW may modify the command line in place, so it must be a mutable
    // buffer rather than a borrowed literal.
    let mut command = to_wide(command_line);

    let cwd = options.cwd.map(|p| to_wide(&p.to_string_lossy()));
    let environment = options.env.map(build_environment_block);

    let mut flags = CREATE_UNICODE_ENVIRONMENT;
    // The whole point: nothing runs until the child is inside its job.
    flags |= CREATE_SUSPENDED;
    if options.new_process_group {
        flags |= CREATE_NEW_PROCESS_GROUP;
    }

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

    // SAFETY: every pointer is either null or points at a correctly-sized, live buffer
    // that outlives this call.
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            TRUE, // inherit handles, so redirected stdio reaches the child
            flags,
            environment
                .as_ref()
                .map_or(std::ptr::null(), |e| e.as_ptr().cast()),
            cwd.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            &raw const startup,
            &raw mut info,
        )
    };

    if created == 0 {
        return Err(io::Error::last_os_error());
    }

    let child = Child {
        process: info.hProcess,
        thread: info.hThread,
        pid: info.dwProcessId,
        group_id: options.new_process_group.then_some(info.dwProcessId),
    };

    // Contain it before it runs. If this fails the child is still suspended, so dropping
    // it here cannot leak a running process.
    if let Some(job) = options.job {
        job.assign_process(child.process.cast())?;
    }

    // SAFETY: thread handle is valid and was created suspended exactly once.
    let previous = unsafe { ResumeThread(child.thread) };
    if previous == u32::MAX {
        return Err(io::Error::last_os_error());
    }

    Ok(child)
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

/// Start a process that deliberately leaves cash's job object (D45's `detach`).
///
/// `CREATE_BREAKAWAY_FROM_JOB` only succeeds if the enclosing job permits it, which is
/// why the session job is created with `JOB_OBJECT_LIMIT_BREAKAWAY_OK`
/// ([`crate::job::JobConfig::session`]). D45 records the cost of that: once breakaway is
/// permitted, *any* child can request it, so this escape hatch weakens D6's guarantee
/// slightly for everything.
///
/// `DETACHED_PROCESS` additionally gives the child no console, so it does not die with
/// the terminal and does not scribble on cash's output.
pub fn spawn_detached(command_line: &str) -> io::Result<Child> {
    use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, DETACHED_PROCESS};

    let mut command = to_wide(command_line);

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

    // SAFETY: command is a live, writable UTF-16 buffer; every other pointer is null or
    // points at a correctly-sized local that outlives the call.
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            FALSE, // do not inherit handles: a detached process should hold none of ours
            CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS | CREATE_UNICODE_ENVIRONMENT,
            std::ptr::null(),
            std::ptr::null(),
            &raw const startup,
            &raw mut info,
        )
    };

    if created == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(Child {
        process: info.hProcess,
        thread: info.hThread,
        pid: info.dwProcessId,
        group_id: None,
    })
}
