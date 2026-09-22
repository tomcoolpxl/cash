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

use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent, SetConsoleCP, SetConsoleOutputCP,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Threading::{
    OpenThread, ResumeThread, SuspendThread, THREAD_SUSPEND_RESUME,
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
    // SAFETY: a plain Win32 call with a scalar argument.
    let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group_id) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
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
            // SAFETY: opening a thread by id; null is returned on failure.
            let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, FALSE, entry.th32ThreadID) };
            if !handle.is_null() {
                action(handle);
                affected += 1;
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
