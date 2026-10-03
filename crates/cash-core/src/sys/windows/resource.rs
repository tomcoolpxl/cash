//! CPU time for `time` and `times` (EXE-11), which reported zeros.
//!
//! Windows has no `getrusage`. The shell's own time is its process's; its children's is
//! the session job's accounting (D6), which counts every process the job has held, the
//! ended ones included, and cash itself, whose share is taken out. Unlike
//! `RUSAGE_CHILDREN`, it counts a child that is still running, and so the background
//! jobs running through a `time`d command.

use std::time::Duration;

use crate::error;

/// Returns the user and system CPU time used by the current process.
pub fn get_self_user_and_system_time() -> Result<(Duration, Duration), error::Error> {
    let (user, kernel) = cash_win32::process::own_cpu_time();
    Ok((ticks(user), ticks(kernel)))
}

/// Returns the user and system CPU time used by child processes: none when cash runs
/// without its session job.
pub fn get_children_user_and_system_time() -> Result<(Duration, Duration), error::Error> {
    let (own_user, own_kernel) = cash_win32::process::own_cpu_time();
    let Some((user, kernel)) = cash_win32::session::cpu_time() else {
        return Ok((Duration::ZERO, Duration::ZERO));
    };
    Ok((
        ticks(user.saturating_sub(own_user)),
        ticks(kernel.saturating_sub(own_kernel)),
    ))
}

/// A count of 100-nanosecond units as a duration.
const fn ticks(count: u64) -> Duration {
    Duration::from_nanos(count.saturating_mul(100))
}
