//! Windows terminal and process-parent utilities.

use crate::sys;

pub use crate::sys::stubs::terminal::{
    Config, get_foreground_pid, get_process_group_id, move_self_to_foreground, move_to_foreground,
    try_get_terminal_device_path,
};

/// Get the native Windows process ID of this process's parent.
///
/// Windows' record of the process that created cash (`cash_win32::facts::parent_pid`),
/// the parent cash's `ps`, `top`, `pgrep` and `pstree` show, so `PPID` names a process
/// those tools can display and `kill` can address.
pub fn get_parent_process_id() -> Option<sys::process::ProcessId> {
    cash_win32::facts::parent_pid().and_then(|pid| i32::try_from(pid).ok())
}
