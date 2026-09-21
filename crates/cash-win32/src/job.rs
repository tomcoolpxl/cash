//! Job objects — the kernel-enforced process containment behind **D6**.
//!
//! This is the mechanism that makes cash's process lifetime guarantee stronger than
//! bash's on Linux. A process cannot leave its job, children join automatically, and
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` reaps every member when the last handle closes —
//! including when cash is killed from Task Manager, because the kernel closes handles
//! during process teardown.
//!
//! Two job shapes exist, per the spec:
//!
//! - [`JobConfig::session`] — one per cash session, permitting breakaway so that
//!   `detach` (D45) can start something meant to outlive the shell.
//! - [`JobConfig::job`] — one per pipeline or background job (D6), no breakaway.
//!
//! Note the cost recorded in D45: permitting breakaway on the session job means *any*
//! child can request it via `CREATE_BREAKAWAY_FROM_JOB`, which slightly weakens the
//! guarantee for everyone. That is why it is opt-in per job rather than blanket.

use std::io;
use std::os::windows::io::{AsRawHandle, RawHandle};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_PROCESS_ID_LIST,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicProcessIdList,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject,
};

/// How a job object should behave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobConfig {
    /// Terminate every member when the last handle to the job closes.
    ///
    /// This is the whole point of D6. Leaving it off produces a job that groups
    /// processes for accounting but guarantees nothing about their lifetime.
    pub kill_on_close: bool,

    /// Permit members to leave the job via `CREATE_BREAKAWAY_FROM_JOB`.
    ///
    /// Required for `detach` (D45), and carries that decision's documented cost: once
    /// permitted, any child may request breakaway, not just the ones cash intends.
    pub allow_breakaway: bool,
}

impl JobConfig {
    /// Configuration for the per-session job: kills on close, permits breakaway.
    #[must_use]
    pub const fn session() -> Self {
        Self { kill_on_close: true, allow_breakaway: true }
    }

    /// Configuration for a per-pipeline job: kills on close, no breakaway.
    #[must_use]
    pub const fn job() -> Self {
        Self { kill_on_close: true, allow_breakaway: false }
    }
}

/// An owned Windows job object.
///
/// Dropping this closes the handle. If the job was created with
/// [`JobConfig::kill_on_close`] and this was the last handle, every process still in the
/// job is terminated by the kernel.
#[derive(Debug)]
pub struct JobObject {
    handle: HANDLE,
}

// A job handle is just a kernel handle; it carries no thread affinity, and the Win32
// calls made through it are all thread-safe.
unsafe impl Send for JobObject {}
unsafe impl Sync for JobObject {}

impl JobObject {
    /// Create a job object with the given configuration.
    ///
    /// The handle is non-inheritable, which matters: an inheritable handle leaked into a
    /// child would keep the job alive past cash's exit and defeat `kill_on_close`.
    pub fn new(config: JobConfig) -> io::Result<Self> {
        // SAFETY: both arguments are optional and we pass null for each, which
        // CreateJobObjectW documents as "default security, unnamed".
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        let job = Self { handle };
        job.apply(config)?;
        Ok(job)
    }

    /// Convenience: a per-session job (see [`JobConfig::session`]).
    pub fn session() -> io::Result<Self> {
        Self::new(JobConfig::session())
    }

    /// Convenience: a per-pipeline job (see [`JobConfig::job`]).
    pub fn for_pipeline() -> io::Result<Self> {
        Self::new(JobConfig::job())
    }

    fn apply(&self, config: JobConfig) -> io::Result<()> {
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };

        if config.kill_on_close {
            limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        }
        if config.allow_breakaway {
            limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        }

        // SAFETY: `limits` is a correctly-sized, fully-initialised structure of the type
        // that JobObjectExtendedLimitInformation expects.
        let ok = unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                    .expect("struct size fits in u32"),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Assign an already-running process to this job.
    ///
    /// Note the race documented in §6 of the spec: between `CreateProcessW` and this
    /// call, a fast-forking child can spawn a grandchild that never joins the job. The
    /// clean fix is to create the process suspended, assign, then resume — which is why
    /// cash needs raw `CreateProcessW` rather than [`std::process::Command`].
    ///
    /// The session-level guarantee holds regardless, because cash itself is inside the
    /// session job and children inherit it automatically.
    pub fn assign_process(&self, process: RawHandle) -> io::Result<()> {
        // SAFETY: caller supplies a valid process handle with PROCESS_SET_QUOTA and
        // PROCESS_TERMINATE rights, which std's Child handles carry.
        let ok = unsafe { AssignProcessToJobObject(self.handle, process as HANDLE) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Assign a [`std::process::Child`] to this job.
    ///
    /// This is the convenience path. Per D42, it will fail for an elevated child: a
    /// medium-integrity process cannot acquire the rights needed over a high-integrity
    /// one, and that hole is documented rather than worked around.
    pub fn assign_child(&self, child: &std::process::Child) -> io::Result<()> {
        self.assign_process(child.as_raw_handle())
    }

    /// Whether a process is a member of this job.
    pub fn contains(&self, process: RawHandle) -> io::Result<bool> {
        let mut result: i32 = 0;
        // SAFETY: `result` is a valid BOOL out-param.
        let ok = unsafe { IsProcessInJob(process as HANDLE, self.handle, &raw mut result) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result != 0)
    }

    /// The process IDs currently in this job.
    ///
    /// Used by D22 for tree-scoped `kill`, by D19 for tree-scoped suspend, and as the
    /// direct proof that children join automatically.
    pub fn process_ids(&self) -> io::Result<Vec<u32>> {
        // The trailing ProcessIdList is variable-length, so grow until it fits.
        let mut capacity = 64usize;
        loop {
            let header = size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
            let bytes = header + capacity * size_of::<usize>();
            let mut buffer = vec![0u8; bytes];

            // SAFETY: buffer is at least as large as the header, and we tell the API its
            // true length.
            let ok = unsafe {
                QueryInformationJobObject(
                    self.handle,
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    u32::try_from(bytes).expect("buffer size fits in u32"),
                    std::ptr::null_mut(),
                )
            };

            // SAFETY: on success the buffer starts with a valid header.
            let list = unsafe { &*buffer.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };

            if ok == 0 {
                // ERROR_MORE_DATA means the list did not fit; everything else is real.
                const ERROR_MORE_DATA: i32 = 234;
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(ERROR_MORE_DATA) && capacity < 65_536 {
                    capacity *= 4;
                    continue;
                }
                return Err(err);
            }

            let count = list.NumberOfProcessIdsInList as usize;
            // SAFETY: the API reports how many entries it wrote into the trailing array.
            let ids = unsafe {
                std::slice::from_raw_parts(list.ProcessIdList.as_ptr(), count)
                    .iter()
                    .map(|&id| id as u32)
                    .collect()
            };
            return Ok(ids);
        }
    }

    /// Terminate every process in the job immediately.
    ///
    /// This is `kill -9` (D21) and D13's third Ctrl-C. It is deliberately *not* the
    /// first response to an interrupt: see D13's rationale about Terraform state locks.
    pub fn terminate(&self, exit_code: u32) -> io::Result<()> {
        // SAFETY: self.handle is a valid job handle for the lifetime of self.
        let ok = unsafe { TerminateJobObject(self.handle, exit_code) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// The raw job handle, for callers that need it during process creation.
    #[must_use]
    pub fn as_raw(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        // SAFETY: handle came from CreateJobObjectW and is closed exactly once.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}
