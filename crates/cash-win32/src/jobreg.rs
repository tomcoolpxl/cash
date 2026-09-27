//! Per-job process containment — **D6**'s tree kill, and **D22**'s job-spec scope.
//!
//! The session job (see [`crate::session`]) guarantees nothing survives cash. That is
//! the outermost promise and it holds unconditionally. But it says nothing about killing
//! *one job* while the shell keeps running, and without this a `kill %1` reaped only the
//! process cash spawned directly:
//!
//! ```text
//! before kill : ping=1 cmd=1
//! after kill  : ping=1 cmd=0     <- the grandchild was orphaned
//! ```
//!
//! Each externally spawned process therefore also gets its own **nested** job object.
//! Children join it automatically, so terminating the job reaps the whole tree. Nesting
//! has worked since Windows 8, so a process being in both this job and the session job
//! is fine.
//!
//! ## The race, stated plainly
//!
//! §6 records that assigning a child to a job *after* `spawn()` leaves a window in which
//! it can fork a grandchild that never joins. The clean fix is `CREATE_SUSPENDED` →
//! assign → resume, which [`crate::spawn`] implements — but the shell spawns through
//! tokio, which owns process creation and cannot start one suspended.
//!
//! So this is the post-spawn assignment, with that window open. It is a real but narrow
//! gap, and the session job still catches anything that slips through it: such a process
//! cannot outlive cash, only a `kill` of its own job.

use std::collections::HashMap;
use std::io;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::FALSE;
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

use crate::job::{JobConfig, JobObject};

/// Jobs holding spawned process trees, keyed by the pid cash knows them as.
fn registry() -> &'static Mutex<HashMap<u32, JobObject>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u32, JobObject>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Put a freshly spawned process into its own nested job, so its descendants can be
/// reaped as a unit.
///
/// Failure is not fatal and is not reported upward: the process is already running and
/// already inside the session job, so the worst case is that `kill` on this job reaps
/// only the top process — the behaviour before this existed.
pub fn contain(pid: u32) {
    let Ok(job) = JobObject::new(JobConfig::job()) else {
        return;
    };

    // SAFETY: OpenProcess returns null rather than a bad handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, FALSE, pid) };
    if handle.is_null() {
        return;
    }

    let assigned = job.assign_process(handle.cast()).is_ok();

    // SAFETY: closing a handle we just opened, exactly once. The job holds its own
    // reference to the process, so closing this does not undo the assignment.
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(handle);
    }

    if assigned && let Ok(mut registry) = registry().lock() {
        registry.insert(pid, job);
    }
}

/// Terminate the whole tree rooted at `pid`, if it was contained.
///
/// Returns `false` when the pid has no job — the caller should then fall back to
/// terminating the single process.
pub fn terminate_tree(pid: u32) -> io::Result<bool> {
    let Ok(mut registry) = registry().lock() else {
        return Ok(false);
    };

    let Some(job) = registry.remove(&pid) else {
        return Ok(false);
    };

    // Dropping the job would also reap it, since it is created with
    // KILL_ON_JOB_CLOSE — but terminate first so the outcome does not depend on when
    // the handle happens to be dropped.
    job.terminate(1)?;
    Ok(true)
}

/// Every process still alive in `pid`'s job, including `pid` itself.
#[must_use]
pub fn tree_pids(pid: u32) -> Vec<u32> {
    registry()
        .lock()
        .ok()
        .and_then(|registry| registry.get(&pid).and_then(|job| job.process_ids().ok()))
        .unwrap_or_default()
}

/// The root pid of every process tree cash has spawned and still holds.
///
/// This is what POSIX's "my process group" means here. `kill 0` on Linux signals every
/// process in the shell's own group — its children — and on Windows the nearest true
/// equivalent is the set of trees cash created, *not* the console, which also contains
/// the terminal and whatever else happens to be attached to it.
///
/// Only live roots are returned. The registry keeps a job until it is swept, so a
/// command that has already exited would otherwise be handed to `kill` as a target and
/// come back as "No such process" — for a process the user never named.
#[must_use]
pub fn roots() -> Vec<u32> {
    registry()
        .lock()
        .map(|registry| {
            registry
                .keys()
                .copied()
                .filter(|&pid| crate::process::is_pid_alive(pid))
                .collect()
        })
        .unwrap_or_default()
}

/// Drop the job for a process that has exited, releasing its handle.
///
/// Whatever the process left running keeps running (see [`JobObject::release`]).
pub fn forget(pid: u32) {
    let job = registry().lock().ok().and_then(|mut r| r.remove(&pid));
    if let Some(job) = job {
        job.release();
    }
}

/// Release every job still held, so its members outlive cash's exit — for
/// [`crate::session::release_at_exit`], once it has ended what should not.
pub fn release_all() {
    let jobs: Vec<JobObject> = registry()
        .lock()
        .map(|mut r| r.drain().map(|(_, job)| job).collect())
        .unwrap_or_default();
    for job in jobs {
        job.release();
    }
}

/// Discard jobs whose root process is gone.
///
/// Called opportunistically so a long-lived interactive session does not accumulate a
/// handle per command ever run. A finished command's descendants — an editor window a
/// launcher started — are left running, not reaped with the handle.
pub fn sweep() {
    let Ok(mut registry) = registry().lock() else {
        return;
    };
    let dead: Vec<u32> = registry
        .keys()
        .copied()
        .filter(|&pid| !crate::process::is_pid_alive(pid))
        .collect();
    for pid in dead {
        if let Some(job) = registry.remove(&pid) {
            job.release();
        }
    }
}
