//! Session setup — **D6**'s outermost guarantee, plus **D41**'s console encoding.
//!
//! Installing the session job is the single most important thing cash does at startup.
//! Once the current process is inside a job with `KILL_ON_JOB_CLOSE`, *every* descendant
//! joins automatically and nothing survives cash — not a crash, not Task Manager, not a
//! process that tries to daemonise. That is the property §2 claims is stronger than
//! bash's on Linux, where descendants reparent to init and survive.
//!
//! The job handle is deliberately kept for the lifetime of the process. Dropping it
//! would close the last handle and terminate the job — which now contains cash itself.
//!
//! ## GUI applications
//!
//! `code .` should leave VS Code open when the shell that started it exits, as it does
//! from PowerShell. So when cash exits in an orderly way — `exit`, end of input, or its
//! console window being closed — [`release_at_exit`] ends the console programs it
//! started and lets go of the rest: GUI applications and everything they started
//! (VS Code's terminals and language servers). `cashctl gui-apps close` turns that off
//! for the session. A crash or a kill from Task Manager still reaps everything.

use std::io;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// The session job, held for the process lifetime. A static is never dropped, so this
/// keeps the handle open exactly as leaking it would, and lets exit find it.
static SESSION_JOB: OnceLock<JobObject> = OnceLock::new();

/// The user and kernel CPU time of every process the session job has held, the ended
/// ones and cash itself included, in 100-nanosecond units; `None` when cash runs
/// without one.
#[must_use]
pub fn cpu_time() -> Option<(u64, u64)> {
    SESSION_JOB.get()?.cpu_time().ok()
}

/// Whether cash started inside another job: 0 not asked yet, 1 no, 2 yes.
static NESTED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Whether cash was inside someone else's job object when it started.
///
/// As [`install`] found it before making cash's own; `None` before that. Asked after, the
/// answer is always yes, cash's own job being one: `cash doctor` said "running inside
/// another job object" everywhere (BIN-06).
#[must_use]
pub fn started_nested() -> Option<bool> {
    match NESTED.load(Ordering::Relaxed) {
        0 => None,
        answer => Some(answer == 2),
    }
}

/// Whether GUI applications cash started outlive it. On unless the session says not.
static GUI_APPS_OUTLIVE: AtomicBool = AtomicBool::new(true);

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
    NESTED.store(u8::from(nested) + 1, Ordering::Relaxed);

    // Breakaway is permitted on the *session* job because `detach` (D45) needs it.
    // Per-pipeline jobs do not permit it, per D45's recorded cost.
    let job = JobObject::new(JobConfig::session()).ok();

    let job_installed = match &job {
        Some(job) => assign_current_process(job).is_ok(),
        None => false,
    };

    // Before any program can change them (D68).
    console::remember_starting_modes();
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

/// Install the session and hold the job handle for the process lifetime.
///
/// The handle must outlive every descendant, and the process owns it until exit. When
/// the process does exit — however it exits — the kernel closes the handle and reaps
/// the job, except for what [`release_at_exit`] let go of first.
pub fn install_and_leak() -> SessionState {
    let (job, state) = install();
    if let Some(job) = job {
        if SESSION_JOB.set(job).is_ok() {
            install_close_handler();
        }
    }
    state
}

/// Whether GUI applications cash started outlive it (`cashctl gui-apps`).
#[must_use]
pub fn gui_apps_outlive() -> bool {
    GUI_APPS_OUTLIVE.load(Ordering::Relaxed)
}

/// Choose whether GUI applications cash started outlive it, for this session.
pub fn set_gui_apps_outlive(outlive: bool) {
    GUI_APPS_OUTLIVE.store(outlive, Ordering::Relaxed);
}

/// Prepare the session job for cash's exit: end the console programs cash started, and
/// let GUI applications and their descendants keep running.
///
/// Does nothing when GUI applications are set to close with cash, or when there is no
/// session job; the kernel then reaps everything as the process ends. Safe to call more
/// than once — the exit path and the console close handler may both reach it.
pub fn release_at_exit() {
    static RELEASED: AtomicBool = AtomicBool::new(false);

    if !gui_apps_outlive() {
        return;
    }
    let Some(job) = SESSION_JOB.get() else {
        return;
    };
    if RELEASED.swap(true, Ordering::SeqCst) {
        return;
    }

    let members = job.process_ids().unwrap_or_default();
    let own = std::process::id();
    let parents: std::collections::HashMap<u32, u32> = crate::process::list()
        .into_iter()
        .map(|p| (p.pid, p.parent_pid))
        .collect();
    let is_gui = |pid: u32| {
        crate::process::image_path(pid)
            .and_then(|path| crate::process::is_gui_image(&path))
            .unwrap_or(false)
    };
    let gui: std::collections::HashSet<u32> = members
        .iter()
        .copied()
        .filter(|&pid| pid != own && is_gui(pid))
        .collect();

    // A member is kept if it is a GUI application or was started by one, following
    // parents through the job — never through cash itself, whose children are exactly
    // what is being sorted.
    let kept = |pid: u32| {
        let mut current = pid;
        for _ in 0..64 {
            if gui.contains(&current) {
                return true;
            }
            match parents.get(&current) {
                Some(&parent) if parent != own && members.contains(&parent) => {
                    current = parent;
                }
                _ => return false,
            }
        }
        false
    };

    for &pid in &members {
        if pid != own && !kept(pid) {
            let _ = crate::process::terminate(pid, 1);
        }
    }

    // A command still running sits in its own job too (D6), whose handle closes as cash
    // exits; release those as well, or a GUI application started with `&` goes with it.
    crate::jobreg::release_all();
    job.release_on_close();
}

/// Run [`release_at_exit`] when the console window is closed, or the user logs off or
/// the machine shuts down — the ways an interactive cash usually ends.
fn install_close_handler() {
    use windows_sys::Win32::Foundation::{FALSE, TRUE};
    use windows_sys::Win32::System::Console::{
        CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT, SetConsoleCtrlHandler,
    };

    unsafe extern "system" fn on_close(event: u32) -> i32 {
        if matches!(
            event,
            CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT
        ) {
            release_at_exit();
        }
        // Not handled: the next handler, and then the default one, still run.
        FALSE
    }

    // SAFETY: `on_close` is a valid handler for the life of the process.
    unsafe { SetConsoleCtrlHandler(Some(on_close), TRUE) };
}

/// Put the current process into a job object.
fn assign_current_process(job: &JobObject) -> io::Result<()> {
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
    let current = unsafe { GetCurrentProcess() };
    job.assign_process(current.cast())
}
