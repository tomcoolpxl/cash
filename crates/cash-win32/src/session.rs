//! Session setup — **D6**'s outermost guarantee, plus **D41**'s console encoding.
//!
//! Installing the session job is the single most important thing cash does at startup.
//! Once the current process is inside a job with `KILL_ON_JOB_CLOSE`, *every* descendant
//! joins automatically and nothing survives cash — not a crash, not Task Manager, not a
//! process that tries to daemonise. That is the property §2 claims is stronger than
//! bash's on Linux, where descendants reparent to init and survive.
//!
//! The job handle is deliberately leaked for the lifetime of the process. Dropping it
//! would close the last handle and terminate the job — which now contains cash itself.

use std::io;

use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::console;
use crate::job::{JobConfig, JobObject};

/// What startup managed to set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionState {
    /// Whether the session job was installed and the current process joined it.
    ///
    /// False means D6's guarantee is not in force and cash should say so rather than
    /// implying containment it does not have.
    pub job_installed: bool,

    /// Whether the console was switched to UTF-8 (D41).
    ///
    /// False is normal when there is no console — a piped or redirected invocation.
    pub utf8_console: bool,

    /// Whether cash was already inside someone else's job, such as Windows Terminal's
    /// or VS Code's. Nested jobs have worked since Windows 8, so this is informational
    /// rather than a problem.
    pub nested: bool,
}

/// Install cash's session job and console settings.
///
/// The returned [`JobObject`] **must be kept alive for the process lifetime**; dropping
/// it terminates the job, and the job contains cash. [`install_and_leak`] is the usual
/// entry point for a `main`.
///
/// Failure to install the job is reported rather than fatal: a shell that refuses to
/// start is worse than one whose containment guarantee is weaker, as long as it is
/// honest about which it is.
pub fn install() -> (Option<JobObject>, SessionState) {
    let nested = crate::spawn::in_any_job();

    // Breakaway is permitted on the *session* job because `detach` (D45) needs it.
    // Per-pipeline jobs do not permit it, per D45's recorded cost.
    let job = JobObject::new(JobConfig::session()).ok();

    let job_installed = match &job {
        Some(job) => assign_current_process(job).is_ok(),
        None => false,
    };

    let utf8_console = console::set_utf8_code_page().is_ok();

    (
        job,
        SessionState {
            job_installed,
            utf8_console,
            nested,
        },
    )
}

/// Install the session and leak the job handle for the process lifetime.
///
/// Leaking is correct here, not sloppy: the handle must outlive every descendant, and
/// the process is about to own it until exit. When the process does exit — however it
/// exits — the kernel closes the handle and reaps the job.
pub fn install_and_leak() -> SessionState {
    let (job, state) = install();
    if let Some(job) = job {
        std::mem::forget(job);
    }
    state
}

/// Put the current process into a job object.
fn assign_current_process(job: &JobObject) -> io::Result<()> {
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
    let current = unsafe { GetCurrentProcess() };
    job.assign_process(current.cast())
}
