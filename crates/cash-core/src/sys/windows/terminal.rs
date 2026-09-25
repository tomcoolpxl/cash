//! Windows terminal and process-parent utilities.

use crate::sys;

pub use crate::sys::stubs::terminal::{
    Config, get_foreground_pid, get_process_group_id, move_self_to_foreground, move_to_foreground,
    try_get_terminal_device_path,
};

/// Get the native Windows process ID of this process's parent.
///
/// Tool Help is the same snapshot cash's `ps`, `top`, `pgrep`, and `pstree` use, so
/// `PPID` names a process those tools can display and `kill` can address.
pub fn get_parent_process_id() -> Option<sys::process::ProcessId> {
    let own_pid = std::process::id();
    cash_win32::process::list()
        .into_iter()
        .find(|process| process.pid == own_pid)
        .and_then(|process| i32::try_from(process.parent_pid).ok())
}
