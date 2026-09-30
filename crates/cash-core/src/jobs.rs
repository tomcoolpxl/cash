//! Job management

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Display;

use futures::FutureExt;

use crate::ExecutionResult;
use crate::error;
use crate::processes;
use crate::sys;
use crate::trace_categories;
use crate::traps;

pub(crate) type JobJoinHandle = tokio::task::JoinHandle<Result<ExecutionResult, error::Error>>;
pub(crate) type JobResult = (Job, Result<ExecutionResult, error::Error>);

/// Manages the jobs that are currently managed by the shell.
#[derive(Default)]
pub struct JobManager {
    /// The jobs that are currently managed by the shell.
    pub jobs: Vec<Job>,

    /// Read-only views of the parent's jobs, for a subshell.
    ///
    /// cash: a subshell must not *manage* the parent's jobs — it cannot wait on or reap
    /// a process it does not own — but bash lets it *see* them, and `$(jobs -p)` is a
    /// documented way to collect background pids. `JobTask` holds join handles and child
    /// processes, so the jobs themselves cannot be cloned; this carries the metadata
    /// `jobs` needs to render, and nothing that would let a subshell act on them.
    inherited: Vec<JobSnapshot>,

    /// Children reaped since the last `CHLD` trap ran, one for each foreground process
    /// and each background job; see `Shell::run_pending_chld_traps`.
    reaped_children: usize,

    /// Statuses of background jobs that have finished and left the table, oldest first:
    /// Bash's saved-status list (`bgpids`). `wait PID` reads a status here after the job
    /// is gone, as POSIX requires, and `wait -n` takes the ones it has not returned yet.
    saved: VecDeque<SavedStatus>,
}

/// How many finished jobs' statuses are kept; the oldest go first. Bash keeps as many
/// as the child-process limit.
const MAX_SAVED_STATUSES: usize = 1024;

/// A finished background job's status, kept for `wait`.
#[derive(Clone, Debug)]
struct SavedStatus {
    /// The job's shell-internal id, for `wait %N`.
    id: usize,
    /// Every process id the job was known by, `$!` among them.
    pids: Vec<sys::process::ProcessId>,
    /// The job's exit status.
    status: u8,
    /// Not yet returned by `wait -n`, nor collected by `wait PID`.
    unreported: bool,
    /// Returned by `wait -n` or shown by `jobs`, either of which takes the job out of
    /// Bash's job table and leaves its status with its process id. A plain `wait`
    /// forgets these, and `wait -n` returns one only by that process id or in POSIX mode.
    reported: bool,
}

/// Which finished job `wait -n` may return.
pub enum WaitTarget<'a> {
    /// Any job.
    Any,
    /// Only these process ids and job ids.
    Only {
        /// Process ids.
        pids: &'a [sys::process::ProcessId],
        /// Job ids.
        ids: &'a [usize],
    },
}

impl WaitTarget<'_> {
    fn matches(&self, id: usize, pids: &[sys::process::ProcessId]) -> bool {
        match self {
            Self::Any => true,
            Self::Only { pids: wanted, ids } => {
                ids.contains(&id) || pids.iter().any(|pid| wanted.contains(pid))
            }
        }
    }

    /// Whether one of these process ids is asked for by number.
    fn names(&self, pids: &[sys::process::ProcessId]) -> bool {
        match self {
            Self::Any => false,
            Self::Only { pids: wanted, .. } => pids.iter().any(|pid| wanted.contains(pid)),
        }
    }
}

/// A read-only view of a job, as a subshell sees it.
#[derive(Clone)]
pub struct JobSnapshot {
    /// The shell-internal job id.
    pub id: usize,
    /// The job's representative process id, if it has one.
    pub pid: Option<sys::process::ProcessId>,
    /// The command line that started the job.
    pub command_line: String,
    /// The job's state when the snapshot was taken.
    pub state: JobState,
    /// Whether the job was current, previous, or neither.
    pub annotation: JobAnnotation,
    /// The job's exit status, once it has finished.
    pub exit_status: Option<u8>,
    /// Whether this state has not yet been reported by `jobs` or prompt notification.
    notification_pending: bool,
}

impl Display for JobSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_job_line(
            f,
            self.id,
            &self.annotation,
            &self.state,
            self.exit_status,
            &self.command_line,
        )
    }
}

/// A job as Bash's `jobs` and job notices show it: `[1]+  Running` and the command,
/// the mark a space when the job is neither current nor previous, the status padded to
/// 26 columns, a running job's command ending in ` &`, and a job that failed shown as
/// `Exit 3` rather than `Done`.
fn write_job_line(
    f: &mut std::fmt::Formatter<'_>,
    id: usize,
    annotation: &JobAnnotation,
    state: &JobState,
    exit_status: Option<u8>,
    command_line: &str,
) -> std::fmt::Result {
    // A finished job keeps the current job's `+` until it is reported, but not the
    // previous job's `-`, as Bash shows them.
    let mark = match annotation {
        JobAnnotation::Current => '+',
        JobAnnotation::Previous if !matches!(state, JobState::Done) => '-',
        JobAnnotation::Previous | JobAnnotation::None => ' ',
    };
    let status = match (state, exit_status) {
        (JobState::Done, Some(code)) if code != 0 => format!("Exit {code}"),
        (state, _) => state.to_string(),
    };
    let ampersand = if matches!(state, JobState::Running) {
        " &"
    } else {
        ""
    };
    write!(f, "[{id}]{mark}  {status:<26} {command_line}{ampersand}")
}

/// Represents a task that is part of a job.
pub enum JobTask {
    /// An external process.
    External(processes::ChildProcess),
    /// An internal asynchronous task.
    Internal(JobJoinHandle),
    /// An internal task that has already completed.
    Completed(Option<Result<ExecutionResult, error::Error>>),
}

/// Represents the result of waiting on a job task.
pub enum JobTaskWaitResult {
    /// The task has completed.
    Completed(ExecutionResult),
    /// The task was stopped.
    Stopped,
}

impl JobTask {
    /// Returns whether the task is an external process.
    pub const fn is_external(&self) -> bool {
        matches!(self, Self::External(_))
    }

    /// Waits for the task to complete. Returns the result of the wait.
    pub async fn wait(&mut self) -> Result<JobTaskWaitResult, error::Error> {
        match self {
            Self::External(process) => {
                let wait_result = process.wait().await?;
                match wait_result {
                    processes::ProcessWaitResult::Completed(output) => {
                        Ok(JobTaskWaitResult::Completed(output.into()))
                    }
                    processes::ProcessWaitResult::Stopped => Ok(JobTaskWaitResult::Stopped),
                }
            }
            Self::Internal(handle) => Ok(JobTaskWaitResult::Completed(status_only(handle.await??))),
            Self::Completed(opt) => match opt.take() {
                Some(res) => Ok(JobTaskWaitResult::Completed(status_only(res?))),
                None => Ok(JobTaskWaitResult::Completed(ExecutionResult::success())),
            },
        }
    }

    /// Polls the task for completion. Returns `Some(result)` if the task has completed,
    /// or `None` if it is still running. The result is the execution result of the task.
    /// Behaves in a best-effort manner; if an internal error occurs during polling,
    /// it will return `None`.
    fn poll(&mut self) -> Option<Result<ExecutionResult, error::Error>> {
        match self {
            Self::External(process) => {
                let check_result = process.poll();
                check_result.map(|polled_result| polled_result.map(|output| output.into()))
            }
            Self::Internal(handle) => {
                let checkable_handle = handle;
                checkable_handle
                    .now_or_never()
                    .and_then(|r| r.ok())
                    .map(|r| r.map(status_only))
            }
            Self::Completed(opt) => opt.take().map(|r| r.map(status_only)),
        }
    }
}

/// What a finished background task hands its waiter: its status. An `exit` in the job
/// (`{ exit 3; } &`) ended the job, not the shell that waits for it, which `wait` or
/// `fg` used to exit with it.
const fn status_only(mut result: ExecutionResult) -> ExecutionResult {
    result.next_control_flow = crate::results::ExecutionControlFlow::Normal;
    result
}

impl JobManager {
    /// Removes a job a wait has collected, updating the current/previous marks. Its
    /// status is saved for a later `wait PID` when `keep_status` (see [`SavedStatus`]);
    /// `by_wait_n` marks it as `wait -n`'s, which a plain `wait` forgets.
    pub fn remove_waited_job(&mut self, id: usize, status: u8, keep_status: bool, by_wait_n: bool) {
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return;
        };
        let job = self.jobs.remove(index);
        self.note_children_reaped(1);
        if keep_status {
            self.save_status(&job, status, false, by_wait_n);
        }
        self.reannotate();
    }

    /// Counts children reaped, for the `CHLD` trap.
    pub const fn note_children_reaped(&mut self, count: usize) {
        self.reaped_children = self.reaped_children.saturating_add(count);
    }

    /// The children reaped since this was last called.
    pub fn take_children_reaped(&mut self) -> usize {
        std::mem::take(&mut self.reaped_children)
    }

    fn save_status(&mut self, job: &Job, status: u8, unreported: bool, reported: bool) {
        let mut pids = job.spawned_pids();
        if let Some(pid) = job.representative_pid()
            && !pids.contains(&pid)
        {
            pids.push(pid);
        }
        if self.saved.len() == MAX_SAVED_STATUSES {
            self.saved.pop_front();
        }
        self.saved.push_back(SavedStatus {
            id: job.id,
            pids,
            status,
            unreported,
            reported,
        });
    }

    /// The saved status of a finished job with this process id, for `wait PID`. It stays
    /// saved, and `wait -n` will not return it; in POSIX mode (`forget`) it is removed,
    /// and a second `wait PID` finds nothing (Bash 5.3).
    pub fn collect_saved_pid(&mut self, pid: sys::process::ProcessId, forget: bool) -> Option<u8> {
        let index = self.saved.iter().rposition(|s| s.pids.contains(&pid))?;
        self.collect_saved(index, forget)
    }

    /// Like [`Self::collect_saved_pid`], for `wait %N`.
    pub fn collect_saved_job(&mut self, id: usize, forget: bool) -> Option<u8> {
        let index = self.saved.iter().rposition(|s| s.id == id)?;
        self.collect_saved(index, forget)
    }

    fn collect_saved(&mut self, index: usize, forget: bool) -> Option<u8> {
        if forget {
            return self.saved.remove(index).map(|entry| entry.status);
        }
        let entry = self.saved.get_mut(index)?;
        entry.unreported = false;
        Some(entry.status)
    }

    /// The oldest finished job `wait -n` has not returned, with its status and process
    /// id. Failing that, one already reported, by `wait -n` or by `jobs`, that `target`
    /// asks for by process id: Bash 5.3 returns that one each time it is asked.
    ///
    /// Bash 5.3 keeps the status for a later `wait PID`, except in POSIX mode
    /// (`posix_mode`), where `wait -n` removes the status it returns, and where a bare
    /// `wait -n` returns a reported job too.
    pub fn take_unreported(
        &mut self,
        target: &WaitTarget<'_>,
        posix_mode: bool,
    ) -> Option<(u8, Option<sys::process::ProcessId>, usize)> {
        let index = self
            .saved
            .iter()
            .position(|s| s.unreported && target.matches(s.id, &s.pids))
            .or_else(|| {
                let any = posix_mode && matches!(target, WaitTarget::Any);
                self.saved
                    .iter()
                    .position(|s| s.reported && (any || target.names(&s.pids)))
            })?;
        let entry = if posix_mode {
            self.saved.remove(index)?
        } else {
            let entry = self.saved.get_mut(index)?;
            entry.unreported = false;
            entry.reported = true;
            entry.clone()
        };
        Some((entry.status, entry.pids.first().copied(), entry.id))
    }

    /// A plain `wait` forgets the statuses `wait -n` returned and those of the jobs
    /// `jobs` showed as finished, as Bash does.
    pub fn forget_reported(&mut self) {
        self.saved.retain(|s| !s.reported);
    }

    /// Returns a new job manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// A manager for a subshell: no jobs of its own, but able to see the parent's.
    #[must_use]
    pub const fn with_inherited(inherited: Vec<JobSnapshot>) -> Self {
        Self {
            jobs: Vec::new(),
            inherited,
            reaped_children: 0,
            saved: VecDeque::new(),
        }
    }

    /// Snapshot the jobs this manager owns, for handing to a subshell.
    #[must_use]
    pub fn snapshot(&self) -> Vec<JobSnapshot> {
        self.jobs
            .iter()
            .map(|job| JobSnapshot {
                id: job.id,
                pid: job.representative_pid(),
                command_line: job.command_line.clone(),
                state: job.state.clone(),
                annotation: job.annotation.clone(),
                exit_status: job.exit_status,
                notification_pending: job.notification_pending,
            })
            .chain(self.inherited.iter().cloned())
            .collect()
    }

    /// The parent's jobs, as seen from a subshell.
    #[must_use]
    pub fn inherited(&self) -> &[JobSnapshot] {
        &self.inherited
    }

    /// Poll jobs without removing completed entries. This lets `jobs -n` report a
    /// transition to Done before the entry is cleaned from the table. A job seen to
    /// finish here keeps its status in `exit_status`, for whatever takes it out of the
    /// table later: `jobs` once it has shown it, a `wait`, or the next poll.
    pub fn refresh_statuses(&mut self) -> Result<(), error::Error> {
        for job in &mut self.jobs {
            if !matches!(job.state, JobState::Done) {
                let _ = job.poll_done()?;
            }
        }
        Ok(())
    }

    /// Consume pending status notifications, optionally restricted to job IDs.
    pub fn take_status_notifications(&mut self, ids: Option<&[usize]>) -> Vec<JobSnapshot> {
        let wanted = |id| ids.is_none_or(|ids| ids.contains(&id));
        let mut notifications = Vec::new();

        for snapshot in &mut self.inherited {
            if snapshot.notification_pending && wanted(snapshot.id) {
                notifications.push(snapshot.clone());
                snapshot.notification_pending = false;
            }
        }
        for job in &mut self.jobs {
            if job.notification_pending && wanted(job.id) {
                notifications.push(JobSnapshot {
                    id: job.id,
                    pid: job.representative_pid(),
                    command_line: job.command_line.clone(),
                    state: job.state.clone(),
                    annotation: job.annotation.clone(),
                    exit_status: job.exit_status,
                    notification_pending: true,
                });
                job.notification_pending = false;
            }
        }

        self.remove_notified_done_jobs();
        notifications
    }

    /// Mark the listed jobs as reported and remove completed entries whose final state
    /// has now been displayed. `None` selects every job.
    pub fn mark_notifications(&mut self, ids: Option<&[usize]>) {
        let wanted = |id| ids.is_none_or(|ids| ids.contains(&id));
        for snapshot in &mut self.inherited {
            if wanted(snapshot.id) {
                snapshot.notification_pending = false;
            }
        }
        for job in &mut self.jobs {
            if wanted(job.id) {
                job.notification_pending = false;
            }
        }
        self.remove_notified_done_jobs();
    }

    /// Removes the finished jobs that have been shown. Each keeps its status for a later
    /// `wait PID`, as in Bash, where `jobs` takes a finished job it has shown out of the
    /// job table and `wait -n` no longer returns it.
    fn remove_notified_done_jobs(&mut self) {
        self.inherited.retain(|snapshot| {
            snapshot.notification_pending || !matches!(snapshot.state, JobState::Done)
        });
        let (kept, shown): (Vec<_>, Vec<_>) = std::mem::take(&mut self.jobs)
            .into_iter()
            .partition(|job| job.notification_pending || !matches!(job.state, JobState::Done));
        self.jobs = kept;
        self.note_children_reaped(shown.len());
        for job in &shown {
            if let Some(status) = job.exit_status {
                self.save_status(job, status, false, true);
            }
        }
        self.reannotate();
    }

    /// Adds a job to the job manager and marks it as the current job;
    /// returns an immutable reference to the job.
    ///
    /// # Arguments
    ///
    /// * `job` - The job to add.
    #[allow(
        clippy::missing_panics_doc,
        reason = "push() guarantees the vector length is >= 1"
    )]
    pub fn add_as_current(&mut self, mut job: Job) -> &Job {
        // cash: the id used to be `len() + 1`, which collides the moment a job leaves the
        // table — `disown %1` on jobs 1 and 2 would hand the next job the id 2, a
        // duplicate that `%2` then resolves to whichever came first. bash numbers past the
        // highest id in use, so a disowned or reaped slot is not handed out again.
        let id = self.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
        job.id = id;
        job.annotation = JobAnnotation::None;
        self.jobs.push(job);
        self.reannotate();

        #[allow(clippy::unwrap_used, reason = "we just pushed an element")]
        self.jobs.last().unwrap()
    }

    /// Removes a job from the table and hands it back, leaving its processes alone.
    ///
    /// cash: this is what `disown` does, and it is deliberately *not* a kill. Dropping the
    /// job drops its tasks, which detaches the background task and releases the child
    /// handle; external children are not spawned with `kill_on_drop`, so nothing dies.
    /// What ends is the shell's bookkeeping: `jobs` stops listing it and `wait` stops
    /// waiting for it.
    pub fn remove_job(&mut self, id: usize) -> Option<Job> {
        let index = self.jobs.iter().position(|j| j.id == id)?;
        let job = self.jobs.remove(index);
        self.reannotate();
        Some(job)
    }

    /// Drops one of the parent's jobs from a subshell's read-only view of them.
    ///
    /// cash: a subshell cannot disown the parent's job — bash's subshell is a fork, so it
    /// only ever edits its own copy of the table. This is that copy.
    pub fn forget_inherited(&mut self, id: usize) -> bool {
        let before = self.inherited.len();
        self.inherited.retain(|snapshot| snapshot.id != id);
        self.inherited.len() != before
    }

    /// Recomputes the `%+` and `%-` marks: newest job current, the one before it previous.
    ///
    /// cash: demoting the current job used to leave the old previous job marked as well,
    /// so three background jobs rendered `[1]- [2]- [3]+` where bash renders
    /// `[1] [2]- [3]+` — two jobs claiming to be `%-`, of which `%-` resolved to the
    /// older. Recomputing from the table keeps exactly one of each, and is also how the
    /// marks find their way back to the right jobs when `disown` removes one.
    fn reannotate(&mut self) {
        for (rank, job) in self.jobs.iter_mut().rev().enumerate() {
            job.annotation = match rank {
                0 => JobAnnotation::Current,
                1 => JobAnnotation::Previous,
                _ => JobAnnotation::None,
            };
        }
    }

    /// Returns the current job, if there is one.
    pub fn current_job(&self) -> Option<&Job> {
        self.jobs
            .iter()
            .find(|j| matches!(j.annotation, JobAnnotation::Current))
    }

    /// Returns a mutable reference to the current job, if there is one.
    pub fn current_job_mut(&mut self) -> Option<&mut Job> {
        self.jobs
            .iter_mut()
            .find(|j| matches!(j.annotation, JobAnnotation::Current))
    }

    /// Returns the previous job, if there is one.
    pub fn prev_job(&self) -> Option<&Job> {
        self.jobs
            .iter()
            .find(|j| matches!(j.annotation, JobAnnotation::Previous))
    }

    /// Returns a mutable reference to the previous job, if there is one.
    pub fn prev_job_mut(&mut self) -> Option<&mut Job> {
        self.jobs
            .iter_mut()
            .find(|j| matches!(j.annotation, JobAnnotation::Previous))
    }

    /// Tries to resolve the given job specification to a job.
    ///
    /// # Arguments
    ///
    /// * `job_spec` - The job specification to resolve.
    pub fn resolve_job_spec(&mut self, job_spec: &str) -> Option<&mut Job> {
        let remainder = job_spec.strip_prefix('%')?;

        match remainder {
            "%" | "+" => self.current_job_mut(),
            "-" => self.prev_job_mut(),
            s if s.chars().all(char::is_numeric) => {
                let id = s.parse::<usize>().ok()?;
                self.jobs.iter_mut().find(|j| j.id == id)
            }
            _ => {
                tracing::warn!(target: trace_categories::UNIMPLEMENTED, "unimplemented: job spec naming command: '{job_spec}'");
                None
            }
        }
    }

    /// Tries to find the job that owns the given process ID.
    ///
    /// cash: `pid=$!; wait "$pid"` is the companion to `$!`, and `wait` had no way to
    /// turn a pid back into the job that owns it. Matching on every pid the job has
    /// spawned rather than only its representative means `wait` finds a job by any
    /// process in its tree, which is what `kill` already does for job specs (D22).
    pub fn resolve_pid(&mut self, pid: sys::process::ProcessId) -> Option<&mut Job> {
        self.jobs
            .iter_mut()
            .find(|job| job.spawned_pids().contains(&pid) || job.representative_pid() == Some(pid))
    }

    /// Waits for all managed jobs to complete.
    pub async fn wait_all(&mut self) -> Result<Vec<Job>, error::Error> {
        // A job that finished before the wait keeps its status for `wait PID`; one the wait
        // itself collects does not, as in Bash.
        let mut done: Vec<Job> = self.poll()?.into_iter().map(|(job, _)| job).collect();
        self.forget_reported();
        for job in &mut self.jobs {
            job.wait().await?;
        }

        done.extend(self.sweep_completed_jobs());
        Ok(done)
    }

    /// Polls all managed jobs for completion.
    pub fn poll(&mut self) -> Result<Vec<JobResult>, error::Error> {
        let mut results = Vec::with_capacity(self.jobs.len());

        let mut i = 0;
        while i != self.jobs.len() {
            if let Some(result) = self.jobs[i].poll_done()? {
                let job = self.jobs.remove(i);
                self.note_children_reaped(1);
                let status = result.as_ref().map_or(1, |r| u8::from(r.exit_code));
                self.save_status(&job, status, true, false);
                results.push((job, result));
            } else if matches!(self.jobs[i].state, JobState::Done) {
                // Seen to finish by an earlier poll that left it in the table: `jobs`
                // polls every job, and removes only those it shows. Its status is the
                // one recorded then.
                // TODO(jobs): A job that is done with no status recorded is removed as
                // a success, and leaves no status behind.
                let job = self.jobs.remove(i);
                self.note_children_reaped(1);
                if let Some(status) = job.exit_status {
                    self.save_status(&job, status, true, false);
                }
                let result = ExecutionResult::new(job.exit_status.unwrap_or(0));
                results.push((job, Ok(result)));
            } else {
                i += 1;
            }
        }

        // cash: a reaped job took its `%+` with it, so once the newest job finished the
        // shell had no current job at all and `fg`, `bg` and a bare `disown` reported
        // there was none while jobs were still listed. bash hands the mark to the newest
        // survivor.
        if !results.is_empty() {
            self.reannotate();
        }

        Ok(results)
    }

    fn sweep_completed_jobs(&mut self) -> Vec<Job> {
        let mut completed_jobs = vec![];

        let mut i = 0;
        while i != self.jobs.len() {
            if self.jobs[i].tasks.is_empty() {
                completed_jobs.push(self.jobs.remove(i));
                self.note_children_reaped(1);
            } else {
                i += 1;
            }
        }

        if !completed_jobs.is_empty() {
            self.reannotate();
        }

        completed_jobs
    }
}

/// Represents the current execution state of a job.
#[derive(Clone)]
pub enum JobState {
    /// Unknown state.
    Unknown,
    /// The job is running.
    Running,
    /// The job is stopped.
    Stopped,
    /// The job has completed.
    Done,
}

impl Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            Self::Running => write!(f, "Running"),
            Self::Stopped => write!(f, "Stopped"),
            Self::Done => write!(f, "Done"),
        }
    }
}

/// Represents an annotation for a job.
#[derive(Clone)]
pub enum JobAnnotation {
    /// No annotation.
    None,
    /// The job is the current job.
    Current,
    /// The job is the previous job.
    Previous,
}

impl Display for JobAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, ""),
            Self::Current => write!(f, "+"),
            Self::Previous => write!(f, "-"),
        }
    }
}

/// Encapsulates a set of processes managed by the shell as a single unit.
pub struct Job {
    /// The tasks that make up the job.
    tasks: VecDeque<JobTask>,

    /// If available, the process group ID of the job's processes.
    pgid: Option<sys::process::ProcessId>,

    /// The annotation of the job (e.g., current, previous).
    annotation: JobAnnotation,

    /// The shell-internal ID of the job.
    pub id: usize,

    /// The command line of the job.
    pub command_line: String,

    /// The current operational state of the job.
    pub state: JobState,

    /// The job's exit status, once it has finished: a failed job is reported as
    /// `Exit N`, as Bash reports it.
    pub exit_status: Option<u8>,

    /// Whether the current state still needs to be shown by `jobs -n`.
    notification_pending: bool,

    /// Process IDs reported by a background task running under this job.
    ///
    /// cash (D11/D22): a background job's tasks are `Internal` — a tokio task executing
    /// the whole and-or list — so the externals it spawns are not job tasks and cannot
    /// be found by walking them. The task reports them here instead.
    spawned_pids: Option<std::sync::Arc<std::sync::Mutex<Vec<sys::process::ProcessId>>>>,

    /// The console as the job left it when Ctrl-Z stopped it under `fg`, for the next `fg`
    /// to put back (D19). One stopped as it started keeps it in its processes instead.
    console_at_stop: Option<cash_win32::console::ConsoleState>,
}

impl Display for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_job_line(
            f,
            self.id,
            &self.annotation,
            &self.state,
            self.exit_status,
            &self.command_line,
        )
    }
}

impl Job {
    /// Whether a wait can still consume a task result for this job.
    pub fn has_unwaited_tasks(&self) -> bool {
        !self.tasks.is_empty()
    }

    /// Returns a new job object.
    ///
    /// # Arguments
    ///
    /// * `children` - The job's known child processes.
    /// * `command_line` - The command line of the job.
    /// * `state` - The current operational state of the job.
    pub(crate) fn new<I>(tasks: I, command_line: String, state: JobState) -> Self
    where
        I: IntoIterator<Item = JobTask>,
    {
        Self {
            id: 0,
            tasks: tasks.into_iter().collect(),
            pgid: None,
            annotation: JobAnnotation::None,
            command_line,
            state,
            exit_status: None,
            notification_pending: true,
            spawned_pids: None,
            console_at_stop: None,
        }
    }

    /// Attach the sink that a background task reports spawned process IDs into.
    #[must_use]
    pub(crate) fn with_spawned_pids(
        mut self,
        pids: std::sync::Arc<std::sync::Mutex<Vec<sys::process::ProcessId>>>,
    ) -> Self {
        self.spawned_pids = Some(pids);
        self
    }

    /// Returns a pid-style string for the job.
    pub fn to_pid_style_string(&self) -> String {
        let display_pid = self
            .representative_pid()
            .map_or(Cow::Borrowed("<pid unknown>"), |pid| {
                Cow::Owned(pid.to_string())
            });
        // As Bash prints a job started at the prompt: `[1] 1234`, with no mark.
        std::format!("[{}] {}", self.id, display_pid)
    }

    /// Returns the annotation of the job.
    pub fn annotation(&self) -> JobAnnotation {
        self.annotation.clone()
    }

    /// Returns the command name of the job.
    pub fn command_name(&self) -> &str {
        self.command_line
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default()
    }

    /// Returns whether the job is the current job.
    pub const fn is_current(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Current)
    }

    /// Returns whether the job is the previous job.
    pub const fn is_prev(&self) -> bool {
        matches!(self.annotation, JobAnnotation::Previous)
    }

    /// Polls whether the job has completed.
    pub fn poll_done(
        &mut self,
    ) -> Result<Option<Result<ExecutionResult, error::Error>>, error::Error> {
        let mut result: Option<Result<ExecutionResult, error::Error>> = None;

        // An earlier poll saw it finish and handed out its result; there is nothing new,
        // and the status recorded then stays.
        if self.tasks.is_empty() && matches!(self.state, JobState::Done) {
            return Ok(None);
        }

        tracing::debug!(target: trace_categories::JOBS, "Polling job {} for completion...", self.id);

        while !self.tasks.is_empty() {
            let task = &mut self.tasks[0];
            match task.poll() {
                Some(r) => {
                    self.tasks.remove(0);
                    result = Some(r);
                }
                None => {
                    return Ok(None);
                }
            }
        }

        tracing::debug!(target: trace_categories::JOBS, "Job {} has completed.", self.id);

        self.state = JobState::Done;
        self.exit_status = result
            .as_ref()
            .map(|r| r.as_ref().map_or(1, |r| u8::from(r.exit_code)));
        self.notification_pending = true;

        Ok(result)
    }

    /// Waits for the job to complete.
    pub async fn wait(&mut self) -> Result<ExecutionResult, error::Error> {
        // A job a poll has already seen finish has no task left to wait for: its status
        // is the one recorded then, not success.
        let mut result = match (&self.state, self.exit_status) {
            (JobState::Done, Some(status)) => ExecutionResult::new(status),
            _ => ExecutionResult::success(),
        };

        while let Some(task) = self.tasks.back_mut() {
            match task.wait().await? {
                JobTaskWaitResult::Completed(execution_result) => {
                    result = execution_result;
                    self.tasks.pop_back();
                }
                JobTaskWaitResult::Stopped => {
                    self.state = JobState::Stopped;
                    self.notification_pending = true;
                    return Ok(ExecutionResult::stopped());
                }
            }
        }

        self.state = JobState::Done;
        self.exit_status = Some(u8::from(result.exit_code));
        self.notification_pending = true;

        Ok(result)
    }

    /// Waits for the job with it in the foreground, as `fg` does.
    ///
    /// cash (D13): on Windows a job started in the background at the prompt leads a
    /// process group of its own, so the keyboard's Ctrl-C no longer reaches it. While it
    /// is in the foreground, cash relays a Ctrl-C to its processes as a Ctrl-Break, the
    /// one console event a group can be sent, and a second Ctrl-C terminates its tree.
    /// A job whose processes share the console's group receives Ctrl-C directly and is
    /// left to handle it, however many times it is pressed.
    ///
    /// cash (D19): when `ctrl_z`, the keyboard's Ctrl-Z stops the job again, suspending
    /// every process it has with its tree.
    pub async fn wait_in_foreground(
        &mut self,
        ctrl_z: bool,
    ) -> Result<ExecutionResult, error::Error> {
        let pids = self.spawned_pids.clone();
        // Those still running: one that has ended is not to be terminated or sent a
        // Ctrl-Break by its number.
        let current = move || -> Vec<u32> {
            pids.as_ref()
                .and_then(|p| p.lock().ok().map(|p| p.clone()))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|pid| u32::try_from(pid).ok())
                .filter(|&pid| cash_win32::children::is_running(pid))
                .collect()
        };
        let mut sigtstp = sys::signal::tstp_signal_listener(ctrl_z)?;
        let mut relayed = false;
        {
            let wait = self.wait();
            tokio::pin!(wait);
            loop {
                tokio::select! {
                    result = &mut wait => return result,
                    () = sigtstp.recv() => break,
                    _ = sys::signal::await_ctrl_c() => {
                        let pids = current();
                        if relayed {
                            for pid in pids {
                                // The second Ctrl-C ends it as SIGINT would: 130.
                                let reaped = cash_win32::jobreg::terminate_tree(pid, 130)
                                    .unwrap_or(false);
                                if !reaped {
                                    let _ = cash_win32::process::terminate(pid, 130);
                                }
                            }
                        } else {
                            // `|` rather than `||`: every leader gets the Ctrl-Break.
                            relayed = pids.into_iter().fold(false, |any, pid| {
                                any | cash_win32::stop::interrupt_group(pid)
                            });
                        }
                    }
                }
            }
        }

        // Ctrl-Z.
        self.console_at_stop = sys::signal::stop_for_ctrl_z(&self.pids());
        self.state = JobState::Stopped;
        self.notification_pending = true;
        Ok(ExecutionResult::stopped())
    }

    /// Wait past stop notifications until the job has actually terminated.
    pub async fn wait_for_termination(&mut self) -> Result<ExecutionResult, error::Error> {
        loop {
            let result = self.wait().await?;
            if !matches!(self.state, JobState::Stopped) {
                return Ok(result);
            }
            // A stopped native process cannot make progress until resumed. Avoid a
            // tight poll while retaining Bash's `wait -f` behavior.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    /// Moves the job to execute in the background.
    pub fn move_to_background(&mut self) -> Result<(), error::Error> {
        if matches!(self.state, JobState::Stopped) {
            // Its console state was for the foreground; running in the background, the
            // job gets the console as the prompt leaves it.
            self.take_console_at_stop();
            self.resume()
        } else {
            error::unimp("move job to background")
        }
    }

    /// Moves the job to execute in the foreground.
    ///
    /// cash (D19): a job Ctrl-Z stopped gets back the console as it left it, and one that
    /// read keys one at a time, as a full-screen program does, is asked to redraw the
    /// screen the prompt has drawn over.
    pub fn move_to_foreground(&mut self) -> Result<(), error::Error> {
        if matches!(self.state, JobState::Stopped) {
            let console = self.take_console_at_stop();
            if let Some(console) = &console {
                console.restore();
            }
            self.resume()?;
            if console.is_some_and(|console| console.raw_input()) {
                cash_win32::console::request_redraw();
            }
        }

        if let Some(pgid) = self.process_group_id() {
            sys::terminal::move_to_foreground(pgid)?;
        }

        Ok(())
    }

    /// Checks whether the job can be signaled: `kill -0 %1`.
    ///
    /// cash: a job whose processes have all ended, and which is still listed, can: there
    /// is nothing left to refuse the signal. Bash 5.3 answers 0 and says nothing.
    pub fn check_signalable(&self) -> Result<(), error::Error> {
        if self.has_processes() {
            Ok(())
        } else {
            Err(error::ErrorKind::FailedToSendSignal.into())
        }
    }

    /// Kills the job.
    ///
    /// cash (D22): the signal goes to every process of the job that is still running,
    /// as Bash's goes to the job's process group: both ends of `a | b &`, and `b` in
    /// `{ a; b; } &` once `a` has ended. One that has ended is passed over, as Bash
    /// passes it over ("avoid pid recycling problem", its `jobs.c`): its pid is not
    /// signalled, because on Windows that number is soon another process's. So a job
    /// with nothing left running is signalled without effect and without error.
    ///
    /// # Arguments
    ///
    /// * `signal` - The signal to send to the job.
    pub fn kill(&mut self, signal: traps::TrapSignal) -> Result<(), error::Error> {
        use sys::signal::Signal;

        if !self.has_processes() {
            return Err(error::ErrorKind::FailedToSendSignal.into());
        }

        match signal {
            // cash (D19, D22): a job spec stops and continues the job's whole tree, and
            // the job's state follows, as `jobs`, `fg` and `bg` read it. Stopping a job
            // twice suspends it once, so one `CONT` resumes it.
            traps::TrapSignal::Signal(Signal::Stop | Signal::Tstp) => {
                if matches!(self.state, JobState::Stopped) {
                    return Ok(());
                }
                let pids = self.pids();
                if pids.is_empty() {
                    // Nothing is running, so nothing is stopped.
                    return Ok(());
                }
                sys::signal::suspend_trees(&pids)?;
                self.state = JobState::Stopped;
                self.notification_pending = true;
                Ok(())
            }
            traps::TrapSignal::Signal(Signal::Cont) => self.resume(),
            _ => {
                let mut first_error = None;
                for pid in self.pids() {
                    if let Err(e) = sys::signal::kill_process(pid, signal) {
                        // One that ended between the look and the signal is like one
                        // that had ended before.
                        let ended = e
                            .as_io_error()
                            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound);
                        if !ended {
                            first_error.get_or_insert(e);
                        }
                    }
                }
                first_error.map_or(Ok(()), Err)
            }
        }
    }

    /// Resumes the job's processes with their trees, and marks it running (D19).
    fn resume(&mut self) -> Result<(), error::Error> {
        if !self.has_processes() {
            return Err(error::ErrorKind::FailedToSendSignal.into());
        }
        sys::signal::resume_trees(&self.pids())?;
        self.state = JobState::Running;
        self.notification_pending = true;
        Ok(())
    }

    /// Takes the console state saved when Ctrl-Z stopped the job (D19): its own when it
    /// was stopped under `fg`, otherwise its processes'.
    fn take_console_at_stop(&mut self) -> Option<cash_win32::console::ConsoleState> {
        self.console_at_stop.take().or_else(|| {
            self.tasks.iter_mut().find_map(|task| match task {
                JobTask::External(process) => process.take_console_at_stop(),
                JobTask::Internal(_) | JobTask::Completed(_) => None,
            })
        })
    }

    /// Every process of the job that is still running: its pipeline's and those its
    /// background task spawned. Each roots a tree of its own (D22).
    ///
    /// cash: only those still running, because these are acted on by pid (a signal, a
    /// suspend, a resume), and the pid of one that has ended is soon another process's on
    /// Windows. Running is asked of a process cash holds open, not of the number.
    fn pids(&self) -> Vec<sys::process::ProcessId> {
        // A pipeline's process is held by its task, which waits for it: its pid is its
        // own, so the pid can be asked.
        let mut pids: Vec<_> = self
            .tasks
            .iter()
            .filter_map(|task| match task {
                JobTask::External(process) => process.pid(),
                JobTask::Internal(_) | JobTask::Completed(_) => None,
            })
            .filter(|&pid| u32::try_from(pid).is_ok_and(cash_win32::process::is_pid_alive))
            .collect();
        // One a background task spawned is held from its start (`cash_win32::children`)
        // and let go only long after it has ended. Asked of the pid, one that had been
        // let go would be whichever process has the number now.
        for pid in self.spawned_pids() {
            if !pids.contains(&pid)
                && u32::try_from(pid).is_ok_and(cash_win32::children::is_running)
            {
                pids.push(pid);
            }
        }
        pids
    }

    /// Whether the job has, or has had, a process to signal. One that runs inside the
    /// shell alone (`{ read x; } &`) has none.
    fn has_processes(&self) -> bool {
        self.representative_pid().is_some()
    }

    /// Tries to retrieve a "representative" pid for the job.
    pub fn representative_pid(&self) -> Option<sys::process::ProcessId> {
        for task in &self.tasks {
            match task {
                JobTask::External(p) => {
                    if let Some(pid) = p.pid() {
                        return Some(pid);
                    }
                }
                JobTask::Internal(_) | JobTask::Completed(_) => (),
            }
        }

        // cash (D11/D22): an `Internal` task is a tokio task running the and-or list, so
        // the process it spawned is not among `tasks`. It reports the pid here instead,
        // which is what makes `$!` and `kill %1` work for a background job.
        self.spawned_pids
            .as_ref()
            .and_then(|pids| pids.lock().ok()?.first().copied())
    }

    /// Every process ID this job has spawned, for tree-scoped operations (D22).
    pub fn spawned_pids(&self) -> Vec<sys::process::ProcessId> {
        self.spawned_pids
            .as_ref()
            .and_then(|pids| pids.lock().ok().map(|p| p.clone()))
            .unwrap_or_default()
    }

    /// Tries to retrieve the process group ID (PGID) of the job.
    pub fn process_group_id(&self) -> Option<sys::process::ProcessId> {
        // TODO(jobs): Don't assume that the first PID is the PGID.
        self.pgid.or_else(|| self.representative_pid())
    }
}

/// Global counter of active subshells and background tasks across the process.
static ACTIVE_SUBSHELL_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Default limit on concurrent background tasks / subshell branches (matching `ulimit -u`).
pub const DEFAULT_MAX_CONCURRENT_SUBSHELLS: usize = 128;

/// RAII slot guard for concurrent subshell/async task execution.
#[derive(Debug)]
pub struct SubshellSlotGuard;

impl SubshellSlotGuard {
    /// Attempts to acquire a slot for a concurrent subshell or async background task.
    /// Returns `Err(ErrorKind::ForkResourceUnavailable)` if the concurrency limit is reached.
    pub fn try_acquire() -> Result<Self, error::Error> {
        let max = match std::env::var("CASH_MAX_SUBSHELLS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            Some(n) if n > 0 => n,
            _ => DEFAULT_MAX_CONCURRENT_SUBSHELLS,
        };

        let current = ACTIVE_SUBSHELL_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if current >= max {
            ACTIVE_SUBSHELL_COUNT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            return Err(error::ErrorKind::ForkResourceUnavailable.into());
        }
        Ok(Self)
    }
}

impl Drop for SubshellSlotGuard {
    fn drop(&mut self) {
        ACTIVE_SUBSHELL_COUNT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}
