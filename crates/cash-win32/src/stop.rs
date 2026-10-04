//! Asking one process to stop, then making it (D21's `TERM`).
//!
//! A console control event cannot do this. `GenerateConsoleCtrlEvent` targets a process
//! *group*, and cash does not start its children as group leaders; aimed at a pid that
//! leads no group, the event goes to every process on the console instead, the shell
//! and the terminal's other programs included. `kill -TERM $pid` therefore used to kill
//! the shell that ran it.
//!
//! What Windows does offer per process is the window: `WM_CLOSE` to a program's
//! top-level windows is how `taskkill` without `/F` asks, and it is what a GUI program's
//! close button sends, so an editor gets to offer to save. A program with no window to
//! ask — a console program — has no per-process way to be asked at all and is
//! terminated at once. One that was asked and is still running after the grace period
//! is terminated then, from a background thread holding a handle to it, so a pid Windows
//! has since reused cannot be hit.

use std::collections::HashMap;
use std::io;
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE, WAIT_TIMEOUT, WPARAM};
use windows_sys::Win32::System::Threading::{
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    WM_CLOSE,
};

use crate::handle::open_process;

/// How long an asked program has to exit before it is terminated.
pub const GRACE: Duration = Duration::from_secs(5);

struct Search {
    pid: u32,
    asked: usize,
}

/// Asks process `pid`'s main windows to close, and returns how many were asked.
///
/// `WM_CLOSE` goes to each visible, unowned top-level window. Owned windows (dialogs)
/// and hidden helper windows are the program's business; its main windows are what a
/// user would close.
pub fn close_windows(pid: u32) -> usize {
    let mut search = Search { pid, asked: 0 };
    // SAFETY: `ask` only runs during this call, and `search` outlives it.
    unsafe {
        EnumWindows(Some(ask), (&raw mut search) as LPARAM);
    }
    search.asked
}

/// `EnumWindows` callback: asks `hwnd` to close when it is one of the searched process's
/// main windows.
unsafe extern "system" fn ask(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` is the `Search` that `close_windows` passed, alive for the call.
    let search = unsafe { &mut *(lparam as *mut Search) };
    let mut owner_pid = 0u32;
    // SAFETY: `hwnd` came from EnumWindows and `owner_pid` is a valid out-parameter.
    unsafe { GetWindowThreadProcessId(hwnd, &raw mut owner_pid) };
    if owner_pid != search.pid {
        return TRUE;
    }
    // SAFETY: querying a window EnumWindows just handed us.
    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
    // SAFETY: as above.
    let owned = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
    if visible && !owned {
        // SAFETY: posting a message to a window handle; it fails harmlessly if the
        // window has gone.
        if unsafe { PostMessageW(hwnd, WM_CLOSE, 0 as WPARAM, 0) } != 0 {
            search.asked += 1;
        }
    }
    TRUE
}

/// Asks process `pid` to stop and returns at once (D21): a program with a window is
/// asked to close and terminated if it is still running after `grace`; one without is
/// terminated now.
///
/// A process terminated exits with `status`, which the caller sets to POSIX's 128 + the
/// signal's number, so `wait` reports what Bash reports.
pub fn request_stop(pid: u32, grace: Duration, status: u32) -> io::Result<()> {
    let handle = open_process(pid, PROCESS_SYNCHRONIZE | PROCESS_TERMINATE)?;

    // A process cash started in a group of its own (a background job at the prompt) can
    // be sent a Ctrl-Break, which console programs handle as an interrupt. It is asked
    // that way as well as through any windows, and terminated if it outlasts the grace.
    let broke = interrupt_group(pid);
    if close_windows(pid) == 0 && !broke {
        return terminate_handle(&handle, status);
    }

    // The thread owns the handle from here on, and closes it when it ends.
    let millis = u32::try_from(grace.as_millis()).unwrap_or(u32::MAX);
    std::thread::spawn(move || {
        // SAFETY: `handle` was opened with SYNCHRONIZE and is owned by this thread.
        if unsafe { WaitForSingleObject(handle.as_raw_handle(), millis) } == WAIT_TIMEOUT {
            let _ = terminate_handle(&handle, status);
        }
    });
    Ok(())
}

/// Processes cash started in a process group of their own, each with a handle held open.
///
/// The handle is what makes a Ctrl-Break aimed at one safe: Windows does not reuse a pid
/// while a handle to its process is open, so the pid still names the group leader cash
/// started, and not some later process that leads no group, at which a console control
/// event would reach every process on the console (D21).
static LEADERS: Mutex<Option<HashMap<u32, OwnedHandle>>> = Mutex::new(None);

/// Records that cash started `pid` with `CREATE_NEW_PROCESS_GROUP` (D13).
pub fn register_group_leader(pid: u32) {
    let Ok(handle) = open_process(pid, PROCESS_SYNCHRONIZE) else {
        return;
    };
    let mut guard = LEADERS.lock().unwrap_or_else(PoisonError::into_inner);
    let leaders = guard.get_or_insert_with(HashMap::new);
    // Closing the handles of leaders that have exited keeps the table small.
    leaders.retain(|_, held| still_running(held));
    let replaced = leaders.insert(pid, handle);
    drop(guard);
    drop(replaced);
}

/// Whether `pid` is a running process cash started as the leader of its own group.
pub fn leads_group(pid: u32) -> bool {
    let leaders = LEADERS.lock().unwrap_or_else(PoisonError::into_inner);
    leaders
        .as_ref()
        .and_then(|l| l.get(&pid))
        .is_some_and(still_running)
}

/// Sends a Ctrl-Break to `pid`'s group if cash started it as a group leader.
///
/// Returns whether it did. Anything else is left alone: aimed at a pid that leads no
/// group, the event would reach the whole console.
pub fn interrupt_group(pid: u32) -> bool {
    leads_group(pid) && crate::console::interrupt_process_group(pid).is_ok()
}

fn still_running(handle: &OwnedHandle) -> bool {
    // SAFETY: `handle` is a process handle held open by the registry.
    let waited = unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) };
    waited == WAIT_TIMEOUT
}

fn terminate_handle(handle: &OwnedHandle, status: u32) -> io::Result<()> {
    // SAFETY: `handle` is valid and carries PROCESS_TERMINATE.
    if unsafe { TerminateProcess(handle.as_raw_handle(), status) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
