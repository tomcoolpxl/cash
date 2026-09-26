//! Console control events and suspend — **D13**, **D19**, **D41**.
//!
//! # The trap that catches everyone
//!
//! `CTRL_C_EVENT` **cannot be delivered to a specific process group.**
//! `GenerateConsoleCtrlEvent` with a nonzero group id *succeeds* and the signal is never
//! received — a silent failure, which is the worst possible shape for the feature cash
//! is built around. Only `CTRL_BREAK_EVENT` is group-deliverable.
//!
//! So cash sends `CTRL_BREAK_EVENT` for targeted delivery. Go's runtime maps both events
//! to `os.Interrupt`, so Terraform, `gh` and `kubectl` — the toolchain in §1 — handle it
//! correctly. Tools that handle `CTRL_C_EVENT` but ignore `CTRL_BREAK_EVENT` remain an
//! open question that needs a survey against the real corpus, not reasoning.
//!
//! # Escalation (D13)
//!
//! The grace period is the user, not a timer:
//!
//! ```text
//! 1st Ctrl-C  -> console control event to the foreground job's process group
//! 2nd Ctrl-C  -> escalate
//! 3rd Ctrl-C  -> TerminateJobObject
//! ```
//!
//! Terraform's own interrupt handling is already two-stage — the first interrupt
//! finishes the *current* operation, which can legitimately take minutes. A fixed
//! timeout would either guillotine a valid apply or be long enough to feel broken.

use std::io;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, STILL_ACTIVE};
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent, SetConsoleCP, SetConsoleCtrlHandler,
    SetConsoleOutputCP,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeThread, OpenThread, ResumeThread, SuspendThread, THREAD_QUERY_LIMITED_INFORMATION,
    THREAD_SUSPEND_RESUME,
};

/// UTF-8. Set for the console per D41.
pub const CODE_PAGE_UTF8: u32 = 65001;

/// What the next interrupt should do (D13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escalation {
    /// Ask politely: deliver a console control event and let the child clean up.
    Interrupt,
    /// Ask again, more firmly.
    Escalate,
    /// Stop asking: terminate the job object.
    Terminate,
}

/// Tracks how many interrupts the user has sent for the current foreground job.
///
/// Reset when a new foreground job starts, so that one Ctrl-C per command does not
/// accumulate into a termination three commands later.
#[derive(Debug, Default)]
pub struct InterruptState {
    count: AtomicU32,
}

impl InterruptState {
    /// A fresh state, as for a newly started foreground job.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
        }
    }

    /// Record an interrupt and report what should happen (D13).
    pub fn record(&self) -> Escalation {
        match self.count.fetch_add(1, Ordering::SeqCst) {
            0 => Escalation::Interrupt,
            1 => Escalation::Escalate,
            _ => Escalation::Terminate,
        }
    }

    /// Reset for a new foreground job.
    pub fn reset(&self) {
        self.count.store(0, Ordering::SeqCst);
    }

    /// How many interrupts have been recorded.
    pub fn count(&self) -> u32 {
        self.count.load(Ordering::SeqCst)
    }
}

/// Deliver an interrupt to a process group (D13 step 1).
///
/// Uses `CTRL_BREAK_EVENT` because `CTRL_C_EVENT` cannot be targeted at a group — see
/// this module's documentation. The target must have been created with
/// `CREATE_NEW_PROCESS_GROUP` for the group id to exist.
pub fn interrupt_process_group(group_id: u32) -> io::Result<()> {
    // Group 0 is not "no group" — it is *every process attached to this console*,
    // including cash itself, the terminal, and any unrelated program sharing it. A shell
    // never means that: `kill 0` means "my own process group", which on Windows is the
    // tree cash spawned, not the console. Refusing here makes the broadcast unreachable
    // from any caller rather than trusting each one to remember.
    if group_id == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to send a console control event to every process in the console",
        ));
    }

    // SAFETY: a plain Win32 call with a scalar argument.
    let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group_id) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Takes Ctrl-C handling back from a parent that turned it off (D13).
///
/// Windows passes "ignore Ctrl-C" down to children: a process started in a new process
/// group, or by one that called `SetConsoleCtrlHandler(NULL, TRUE)` (build tools and task
/// runners do), ignores Ctrl-C, and so does everything it starts. An interactive shell owns
/// its terminal, so it clears the flag at startup; without that, Ctrl-C would stop
/// nothing the user runs. Scripts keep what they inherited, as POSIX has a
/// non-interactive shell keep signals that were ignored on entry.
pub fn enable_ctrl_c() {
    // SAFETY: a plain call; a null handler with FALSE restores default Ctrl-C handling.
    unsafe { SetConsoleCtrlHandler(None, FALSE) };
}

/// Input modes of a console waiting for a line: processed, line-buffered and echoed
/// input, with insert and quick-edit, as a new console window starts.
const COOKED_INPUT: u32 = 0x0001 | 0x0002 | 0x0004 | 0x0010 | 0x0020 | 0x0040 | 0x0080 | 0x0100;
/// Output modes cash relies on: processed output, wrapping, and VT sequences.
const VT_OUTPUT: u32 = 0x0001 | 0x0002 | 0x0004;

/// Puts back the console state cash relies on between prompts, for `reset`.
///
/// That is cooked input, VT-processing output and the UTF-8 code page (D41). A program
/// that died in raw mode, with echo off, VT processing off or after `chcp` leaves the
/// console unusable, and escape sequences cannot undo that because they are not
/// interpreted then.
///
/// Returns `false` when the process has no console to restore.
pub fn restore_modes() -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::System::Console::SetConsoleMode;

    let open = |name: &str| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
    };
    let (Ok(input), Ok(output)) = (open("CONIN$"), open("CONOUT$")) else {
        return Ok(false);
    };
    // SAFETY: the handles are open console handles owned by `input` and `output`, alive
    // for the call.
    let input_ok = unsafe { SetConsoleMode(input.as_raw_handle(), COOKED_INPUT) };
    // SAFETY: as above.
    let output_ok = unsafe { SetConsoleMode(output.as_raw_handle(), VT_OUTPUT) };
    if input_ok == 0 || output_ok == 0 {
        return Err(io::Error::last_os_error());
    }
    set_utf8_code_page()?;
    Ok(true)
}

/// Set the console to UTF-8 (D41).
///
/// Fixes console *display* and console-attached children. It deliberately does not claim
/// to fix pipeline bytes: when cash pipes a child's output it reads raw bytes and the
/// console code page is never consulted.
pub fn set_utf8_code_page() -> io::Result<()> {
    // SAFETY: a plain Win32 call taking a scalar code page; it touches no memory of ours.
    let out = unsafe { SetConsoleOutputCP(CODE_PAGE_UTF8) };
    // SAFETY: as above, for the input code page.
    let inp = unsafe { SetConsoleCP(CODE_PAGE_UTF8) };
    if out == 0 || inp == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Suspend every thread of a process (D19).
///
/// Windows has no `SIGSTOP` for arbitrary executables, so `Ctrl-Z` and `kill -STOP`
/// enumerate threads and suspend each — what Process Explorer's Suspend does, using only
/// documented APIs rather than the undocumented `NtSuspendProcess`.
///
/// Accepted costs, recorded in D19: racy against thread creation during the sweep, and a
/// process could in principle resume itself.
pub fn suspend_process(pid: u32) -> io::Result<usize> {
    for_each_thread(pid, |handle| {
        // SAFETY: handle is a valid thread handle with THREAD_SUSPEND_RESUME.
        unsafe { SuspendThread(handle) };
    })
}

/// Resume every thread of a process (D19).
pub fn resume_process(pid: u32) -> io::Result<usize> {
    for_each_thread(pid, |handle| {
        // SAFETY: handle is a valid thread handle with THREAD_SUSPEND_RESUME.
        unsafe { ResumeThread(handle) };
    })
}

/// Apply an operation to every thread of a process, returning how many were affected.
fn for_each_thread<F>(pid: u32, mut action: F) -> io::Result<usize>
where
    F: FnMut(windows_sys::Win32::Foundation::HANDLE),
{
    // A process that has exited has nothing to suspend or resume, whatever the thread
    // snapshot still lists for it: on GitHub's runner an exited process kept reporting a
    // thread that the per-thread exit-code check below did not rule out.
    if !crate::process::is_pid_alive(pid) {
        return Ok(0);
    }

    // SAFETY: TH32CS_SNAPTHREAD ignores the pid argument and snapshots all threads.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot.is_null() {
        return Err(io::Error::last_os_error());
    }

    // SAFETY: `THREADENTRY32` is a plain-old-data Win32 struct of integers, for which an
    // all-zero bit pattern is valid; `dwSize` is filled in immediately below, which is the
    // only field the API requires before the first call.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    // The struct is 28 bytes and `dwSize` is a `u32` by ABI, so the fallback is
    // unreachable — but saturating beats panicking in a function that returns a Result.
    entry.dwSize = u32::try_from(size_of::<THREADENTRY32>()).unwrap_or(u32::MAX);

    let mut affected = 0usize;

    // SAFETY: entry is correctly sized and the snapshot handle is valid.
    let mut ok = unsafe { Thread32First(snapshot, &raw mut entry) };
    while ok != 0 {
        if entry.th32OwnerProcessID == pid {
            let access = THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION;
            // SAFETY: opening a thread by id; null is returned on failure.
            let handle = unsafe { OpenThread(access, FALSE, entry.th32ThreadID) };
            if !handle.is_null() {
                // A thread that has finished stays in the snapshot for as long as anyone
                // holds a handle to it (an antivirus scanner, say), and suspending it
                // "succeeds". It is not running, so it is not counted or touched.
                let mut code = 0u32;
                // SAFETY: handle is a valid thread handle with query access, and `code`
                // outlives the call.
                let queried = unsafe { GetExitCodeThread(handle, &raw mut code) };
                #[allow(clippy::cast_sign_loss, reason = "STILL_ACTIVE is 259")]
                let running = queried == 0 || code == STILL_ACTIVE as u32;
                if running {
                    action(handle);
                    affected += 1;
                }
                // SAFETY: closing a handle we just opened, exactly once.
                unsafe { CloseHandle(handle) };
            }
        }
        // SAFETY: as above.
        ok = unsafe { Thread32Next(snapshot, &raw mut entry) };
    }

    // SAFETY: closing the snapshot handle, exactly once.
    unsafe { CloseHandle(snapshot) };

    Ok(affected)
}
